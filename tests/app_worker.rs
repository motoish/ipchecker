use std::{
    net::Ipv4Addr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::Duration,
};

use ipchecker::{
    app::{EventSink, EventSinkClosed, WorkerCommand, WorkerEvent, WorkerHandle},
    ip_source::{FetchError, IpSource},
};

#[derive(Clone)]
struct ChannelSink {
    sender: Sender<WorkerEvent>,
}

impl ChannelSink {
    fn new() -> (Self, Receiver<WorkerEvent>) {
        let (sender, events) = mpsc::channel();
        (Self { sender }, events)
    }
}

impl EventSink for ChannelSink {
    fn send(&self, event: WorkerEvent) -> Result<(), EventSinkClosed> {
        self.sender.send(event).map_err(|_| EventSinkClosed)
    }
}

struct CountingSource {
    calls: Arc<AtomicUsize>,
    result: Result<Ipv4Addr, FetchError>,
}

struct BlockingSource {
    calls: Arc<AtomicUsize>,
    started: Sender<()>,
    permits: Receiver<()>,
}

impl CountingSource {
    fn success(ip: Ipv4Addr, calls: Arc<AtomicUsize>) -> Self {
        Self {
            calls,
            result: Ok(ip),
        }
    }
}

impl IpSource for CountingSource {
    fn fetch(&mut self) -> Result<Ipv4Addr, FetchError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.result.clone()
    }
}

impl IpSource for BlockingSource {
    fn fetch(&mut self) -> Result<Ipv4Addr, FetchError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.started.send(()).unwrap();
        self.permits.recv().unwrap();
        Ok(ip("192.0.2.1"))
    }
}

fn ip(value: &str) -> Ipv4Addr {
    value.parse().expect("test IP address should parse")
}

#[test]
fn startup_performs_one_immediate_check() {
    let (sink, events) = ChannelSink::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let _worker = WorkerHandle::start(
        CountingSource::success(ip("192.0.2.1"), Arc::clone(&calls)),
        Duration::from_secs(60),
        sink,
    );

    assert!(matches!(
        events.recv_timeout(Duration::from_millis(250)).unwrap(),
        WorkerEvent::FetchCompleted(Ok(current)) if current == ip("192.0.2.1")
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn check_now_uses_the_same_fetch_path() {
    let (sink, events) = ChannelSink::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let worker = WorkerHandle::start(
        CountingSource::success(ip("192.0.2.1"), Arc::clone(&calls)),
        Duration::from_secs(60),
        sink,
    );
    events.recv_timeout(Duration::from_millis(250)).unwrap();

    worker.command(WorkerCommand::CheckNow).unwrap();

    assert!(matches!(
        events.recv_timeout(Duration::from_millis(250)).unwrap(),
        WorkerEvent::FetchCompleted(Ok(current)) if current == ip("192.0.2.1")
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn interval_change_triggers_an_immediate_check() {
    let (sink, events) = ChannelSink::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let worker = WorkerHandle::start(
        CountingSource::success(ip("192.0.2.1"), Arc::clone(&calls)),
        Duration::from_secs(60),
        sink,
    );
    events.recv_timeout(Duration::from_millis(250)).unwrap();

    worker
        .command(WorkerCommand::SetInterval(Duration::from_secs(30)))
        .unwrap();

    assert!(matches!(
        events.recv_timeout(Duration::from_millis(250)).unwrap(),
        WorkerEvent::FetchCompleted(Ok(current)) if current == ip("192.0.2.1")
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn drop_interrupts_a_live_worker_waiting_for_its_next_interval() {
    let (sink, events) = ChannelSink::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let worker = WorkerHandle::start(
        CountingSource::success(ip("192.0.2.1"), Arc::clone(&calls)),
        Duration::from_secs(1),
        sink,
    );
    events.recv_timeout(Duration::from_millis(250)).unwrap();
    let (finished, completed) = mpsc::channel();

    thread::spawn(move || {
        drop(worker);
        finished.send(()).unwrap();
    });

    completed.recv_timeout(Duration::from_millis(250)).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn shutdown_takes_priority_over_an_earlier_queued_check() {
    let (sink, _events) = ChannelSink::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let (started, starts) = mpsc::channel();
    let (permits, permit_receiver) = mpsc::channel();
    let worker = WorkerHandle::start(
        BlockingSource {
            calls: Arc::clone(&calls),
            started,
            permits: permit_receiver,
        },
        Duration::from_secs(60),
        sink,
    );
    starts.recv_timeout(Duration::from_secs(1)).unwrap();

    worker.command(WorkerCommand::CheckNow).unwrap();
    worker.command(WorkerCommand::Shutdown).unwrap();
    permits.send(()).unwrap();

    let extra_check_started = starts.recv_timeout(Duration::from_millis(250)).is_ok();
    if extra_check_started {
        permits.send(()).unwrap();
    }
    drop(worker);

    assert!(!extra_check_started);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn queued_manual_checks_are_coalesced_after_an_active_fetch() {
    let (sink, _events) = ChannelSink::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let (started, starts) = mpsc::channel();
    let (permits, permit_receiver) = mpsc::channel();
    let worker = WorkerHandle::start(
        BlockingSource {
            calls: Arc::clone(&calls),
            started,
            permits: permit_receiver,
        },
        Duration::from_secs(60),
        sink,
    );
    starts.recv_timeout(Duration::from_secs(1)).unwrap();

    for _ in 0..3 {
        worker.command(WorkerCommand::CheckNow).unwrap();
    }
    permits.send(()).unwrap();

    starts.recv_timeout(Duration::from_secs(1)).unwrap();
    worker.command(WorkerCommand::Shutdown).unwrap();
    permits.send(()).unwrap();

    while starts.recv_timeout(Duration::from_millis(100)).is_ok() {
        permits.send(()).unwrap();
    }
    drop(worker);

    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
