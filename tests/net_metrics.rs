use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use ipchecker::app::EventSinkClosed;
use ipchecker::net_metrics::{NetworkMetricsHandle, NetworkMetricsSampling, NetworkMetricsSink};
use ipchecker::net_speed::NetworkSpeedLabels;

#[derive(Clone)]
struct RecordingSink {
    labels: Arc<Mutex<Vec<NetworkSpeedLabels>>>,
    send_count: Arc<AtomicUsize>,
}

impl NetworkMetricsSink for RecordingSink {
    fn send_labels(&self, labels: NetworkSpeedLabels) -> Result<(), EventSinkClosed> {
        self.send_count.fetch_add(1, Ordering::SeqCst);
        self.labels.lock().expect("labels mutex").push(labels);
        Ok(())
    }
}

#[test]
fn disabled_sampling_does_not_emit_labels() {
    let sink = RecordingSink {
        labels: Arc::new(Mutex::new(Vec::new())),
        send_count: Arc::new(AtomicUsize::new(0)),
    };
    let handle = NetworkMetricsHandle::start(
        sink.clone(),
        NetworkMetricsSampling {
            is_show_network_speed: false,
            is_show_network_latency: false,
        },
    );

    thread::sleep(Duration::from_millis(350));
    drop(handle);

    assert_eq!(sink.send_count.load(Ordering::SeqCst), 0);
}

#[test]
fn enabling_speed_only_emits_speed_labels_without_waiting_on_latency() {
    let (ready_tx, ready_rx) = mpsc::channel();
    let sink = RecordingSink {
        labels: Arc::new(Mutex::new(Vec::new())),
        send_count: Arc::new(AtomicUsize::new(0)),
    };
    let handle = NetworkMetricsHandle::start(
        sink.clone(),
        NetworkMetricsSampling {
            is_show_network_speed: true,
            is_show_network_latency: false,
        },
    );

    thread::spawn(move || {
        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_secs(2) {
            if sink.send_count.load(Ordering::SeqCst) > 0 {
                let _ = ready_tx.send(());
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
    });

    ready_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("speed sampler should emit within one second even when latency is disabled");
    drop(handle);
}
