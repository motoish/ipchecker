use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
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
    // Low bit is visibility; upper bits identify the sampling session.
    speed_state: AtomicUsize,
    is_show_network_latency: AtomicBool,
    is_shutdown: AtomicBool,
    latest_latency: Mutex<LatencyDisplay>,
}

impl SharedSamplingState {
    fn new(sampling: NetworkMetricsSampling) -> Self {
        Self {
            speed_state: AtomicUsize::new(usize::from(sampling.is_show_network_speed)),
            is_show_network_latency: AtomicBool::new(sampling.is_show_network_latency),
            is_shutdown: AtomicBool::new(false),
            latest_latency: Mutex::new(LatencyDisplay::unknown()),
        }
    }

    fn set_sampling(&self, sampling: NetworkMetricsSampling) {
        self.speed_state
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |state| {
                ((state & 1 != 0) != sampling.is_show_network_speed).then(|| {
                    ((state & !1).wrapping_add(2)) | usize::from(sampling.is_show_network_speed)
                })
            })
            .ok();
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
        self.speed_state.load(Ordering::Acquire) & 1 != 0
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
    let mut observed_state = shared.speed_state.load(Ordering::Acquire);
    while !shared.is_shutdown() {
        let state = reset_speed_if_needed(&shared, &mut sampler, &mut observed_state);
        if state & 1 != 0 {
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
            // A visibility change during the read invalidates this sample.
            if shared.speed_state.load(Ordering::Acquire) != state {
                continue;
            }
            if sink.send_labels(labels).is_err() {
                break;
            }
        }
        sleep_interruptible(&shared, SAMPLE_INTERVAL);
    }
}

fn reset_speed_if_needed(
    shared: &SharedSamplingState,
    sampler: &mut NetworkSpeedSampler,
    observed_state: &mut usize,
) -> usize {
    let state = shared.speed_state.load(Ordering::Acquire);
    if state != *observed_state {
        *sampler = NetworkSpeedSampler::default();
        *observed_state = state;
    }
    state
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net_speed::{InterfaceCounters, InterfaceSnapshot};

    fn snapshot(bytes: u64) -> InterfaceSnapshot {
        [(
            "en0".to_owned(),
            InterfaceCounters {
                received: bytes,
                sent: bytes,
            },
        )]
        .into_iter()
        .collect()
    }

    #[test]
    fn reenabling_speed_discards_old_baseline_and_average_even_between_ticks() {
        for disabled_for in [Duration::from_secs(3600), Duration::from_millis(10)] {
            let enabled = NetworkMetricsSampling {
                is_show_network_speed: true,
                is_show_network_latency: false,
            };
            let shared = SharedSamplingState::new(enabled);
            let mut observed = shared.speed_state.load(Ordering::Acquire);
            let mut sampler = NetworkSpeedSampler::default();
            let start = Instant::now();
            sampler.observe(start, snapshot(0));
            assert_eq!(
                sampler
                    .observe(start + Duration::from_secs(1), snapshot(1_048_576))
                    .download,
                "1.0\tMB/s"
            );

            shared.set_sampling(NetworkMetricsSampling {
                is_show_network_speed: false,
                ..enabled
            });
            shared.set_sampling(enabled);
            // No sampler tick occurred between disable and enable.
            reset_speed_if_needed(&shared, &mut sampler, &mut observed);
            let resumed = start + Duration::from_secs(1) + disabled_for;
            assert_eq!(
                *sampler.observe(resumed, snapshot(1_048_576)),
                NetworkSpeedLabels::unknown()
            );
            let next = sampler.observe(resumed + Duration::from_secs(1), snapshot(1_048_576));
            assert_eq!(next.download, "0\tKB/s");
            assert_eq!(next.upload, "0\tKB/s");
        }
    }

    #[test]
    fn changing_latency_does_not_reset_speed_history() {
        let shared = SharedSamplingState::new(NetworkMetricsSampling {
            is_show_network_speed: true,
            is_show_network_latency: false,
        });
        let mut observed = shared.speed_state.load(Ordering::Acquire);
        let mut sampler = NetworkSpeedSampler::default();
        let start = Instant::now();
        sampler.observe(start, snapshot(0));
        shared.set_sampling(NetworkMetricsSampling {
            is_show_network_speed: true,
            is_show_network_latency: true,
        });
        reset_speed_if_needed(&shared, &mut sampler, &mut observed);
        assert_eq!(
            sampler
                .observe(start + Duration::from_secs(1), snapshot(1_048_576))
                .download,
            "1.0\tMB/s"
        );
    }
}
