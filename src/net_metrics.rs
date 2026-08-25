use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Sender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::{
    app::EventSinkClosed,
    net_latency::{LatencyDisplay, NetworkLatencySampler, measure_tcp_latency},
    net_speed::{
        NetworkSpeedLabels, NetworkSpeedSampler, SAMPLE_INTERVAL, read_interface_snapshot,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkMetricsSampling {
    pub is_show_network_speed: bool,
    pub is_show_network_latency: bool,
}

#[derive(Debug)]
pub enum NetworkMetricsCommand {
    SetSampling(NetworkMetricsSampling),
    Shutdown,
}

pub trait NetworkMetricsSink: Clone + Send + 'static {
    fn send_labels(&self, labels: NetworkSpeedLabels) -> Result<(), EventSinkClosed>;
}

struct SharedSamplingState {
    is_show_network_speed: AtomicBool,
    is_show_network_latency: AtomicBool,
    is_shutdown: AtomicBool,
    latest_latency: Mutex<LatencyDisplay>,
}

impl SharedSamplingState {
    fn new(sampling: NetworkMetricsSampling) -> Self {
        Self {
            is_show_network_speed: AtomicBool::new(sampling.is_show_network_speed),
            is_show_network_latency: AtomicBool::new(sampling.is_show_network_latency),
            is_shutdown: AtomicBool::new(false),
            latest_latency: Mutex::new(LatencyDisplay::unknown()),
        }
    }

    fn set_sampling(&self, sampling: NetworkMetricsSampling) {
        self.is_show_network_speed
            .store(sampling.is_show_network_speed, Ordering::Relaxed);
        let was_latency_enabled = self.is_show_network_latency.load(Ordering::Relaxed);
        self.is_show_network_latency
            .store(sampling.is_show_network_latency, Ordering::Relaxed);
        if was_latency_enabled
            && !sampling.is_show_network_latency
            && let Ok(mut latest) = self.latest_latency.lock()
        {
            *latest = LatencyDisplay::unknown();
        }
    }

    fn is_show_network_speed(&self) -> bool {
        self.is_show_network_speed.load(Ordering::Relaxed)
    }

    fn is_show_network_latency(&self) -> bool {
        self.is_show_network_latency.load(Ordering::Relaxed)
    }

    fn is_shutdown(&self) -> bool {
        self.is_shutdown.load(Ordering::Relaxed)
    }

    fn request_shutdown(&self) {
        self.is_shutdown.store(true, Ordering::Relaxed);
    }

    fn latest_latency(&self) -> LatencyDisplay {
        self.latest_latency
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_else(|_| LatencyDisplay::unknown())
    }

    fn store_latency(&self, latency: LatencyDisplay) {
        if let Ok(mut latest) = self.latest_latency.lock() {
            *latest = latency;
        }
    }
}

pub struct NetworkMetricsHandle {
    commands: Sender<NetworkMetricsCommand>,
    shared: Arc<SharedSamplingState>,
    speed_thread: Option<JoinHandle<()>>,
    latency_thread: Option<JoinHandle<()>>,
    command_thread: Option<JoinHandle<()>>,
}

impl NetworkMetricsHandle {
    pub fn start<S>(sink: S, sampling: NetworkMetricsSampling) -> Self
    where
        S: NetworkMetricsSink,
    {
        let shared = Arc::new(SharedSamplingState::new(sampling));
        let (commands, receiver) = mpsc::channel();

        let command_shared = Arc::clone(&shared);
        let command_thread = thread::Builder::new()
            .name("ipchecker-net-metrics-cmd".to_owned())
            .spawn(move || {
                while let Ok(command) = receiver.recv() {
                    match command {
                        NetworkMetricsCommand::SetSampling(sampling) => {
                            command_shared.set_sampling(sampling);
                        }
                        NetworkMetricsCommand::Shutdown => {
                            command_shared.request_shutdown();
                            break;
                        }
                    }
                }
            })
            .ok();

        let speed_shared = Arc::clone(&shared);
        let speed_sink = sink.clone();
        let speed_thread = thread::Builder::new()
            .name("ipchecker-net-speed".to_owned())
            .spawn(move || run_speed_sampler(speed_shared, speed_sink))
            .ok();

        let latency_shared = Arc::clone(&shared);
        let latency_sink = sink;
        let latency_thread = thread::Builder::new()
            .name("ipchecker-net-latency".to_owned())
            .spawn(move || run_latency_sampler(latency_shared, latency_sink))
            .ok();

        Self {
            commands,
            shared,
            speed_thread,
            latency_thread,
            command_thread,
        }
    }

    pub fn set_sampling(&self, sampling: NetworkMetricsSampling) {
        let _ = self
            .commands
            .send(NetworkMetricsCommand::SetSampling(sampling));
    }
}

impl Drop for NetworkMetricsHandle {
    fn drop(&mut self) {
        self.shared.request_shutdown();
        let _ = self.commands.send(NetworkMetricsCommand::Shutdown);
        if let Some(thread) = self.command_thread.take() {
            let _ = thread.join();
        }
        if let Some(thread) = self.speed_thread.take() {
            let _ = thread.join();
        }
        if let Some(thread) = self.latency_thread.take() {
            let _ = thread.join();
        }
    }
}

fn run_speed_sampler<S>(shared: Arc<SharedSamplingState>, sink: S)
where
    S: NetworkMetricsSink,
{
    let mut sampler = NetworkSpeedSampler::default();
    while !shared.is_shutdown() {
        if shared.is_show_network_speed() {
            let latency = if shared.is_show_network_latency() {
                shared.latest_latency()
            } else {
                LatencyDisplay::unknown()
            };
            let labels = match read_interface_snapshot() {
                Ok(counters) => sampler
                    .observe(Instant::now(), counters)
                    .clone()
                    .with_latency(latency),
                Err(error) => {
                    log::warn!("failed to read interface counters: {error}");
                    sampler.observe_failure().clone().with_latency(latency)
                }
            };
            if sink.send_labels(labels).is_err() {
                break;
            }
        }
        sleep_interruptible(&shared, SAMPLE_INTERVAL);
    }
}

fn run_latency_sampler<S>(shared: Arc<SharedSamplingState>, sink: S)
where
    S: NetworkMetricsSink,
{
    let mut sampler = NetworkLatencySampler::default();
    while !shared.is_shutdown() {
        if shared.is_show_network_latency() {
            let latency = sampler.observe(measure_tcp_latency()).clone();
            shared.store_latency(latency.clone());
            if !shared.is_show_network_speed() {
                let labels = NetworkSpeedLabels::unknown().with_latency(latency);
                if sink.send_labels(labels).is_err() {
                    break;
                }
            }
        }
        sleep_interruptible(&shared, SAMPLE_INTERVAL);
    }
}

fn sleep_interruptible(shared: &SharedSamplingState, duration: Duration) {
    let deadline = Instant::now() + duration;
    while !shared.is_shutdown() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        thread::sleep(remaining.min(Duration::from_millis(100)));
    }
}
