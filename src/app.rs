use std::{
    error::Error,
    fmt, iter,
    net::Ipv4Addr,
    sync::mpsc::{self, RecvTimeoutError, Sender},
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::{
    ip_source::{FetchError, IpSource},
    monitor::NotificationDecision,
    notification::{ActionSink, MacNotifier, Notifier},
    vpn_detection::{VpnStatus, detect_vpn_status},
};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NotificationCoordinator {
    active: Option<NotificationDecision>,
    delivered: bool,
}

impl NotificationCoordinator {
    pub fn observe(
        &mut self,
        decision: Option<NotificationDecision>,
        muted: bool,
        is_show_status_icon: bool,
    ) {
        if muted || !is_show_status_icon {
            self.active = None;
            self.delivered = false;
            return;
        }

        let is_repeatable_mismatch =
            matches!(decision, Some(NotificationDecision::Mismatch { .. }));
        if is_repeatable_mismatch || self.active != decision {
            self.active = decision;
            self.delivered = false;
        }
    }

    pub fn pending(&self) -> Option<NotificationDecision> {
        (!self.delivered).then(|| self.active.clone()).flatten()
    }

    pub fn mark_delivered(&mut self, decision: &NotificationDecision) {
        if self.active.as_ref() == Some(decision) {
            self.delivered = true;
        }
    }
}

pub trait NotifierReadySink: Send + 'static {
    fn send(&self, result: Result<MacNotifier, String>);
}

#[derive(Default)]
pub struct NotificationService {
    state: NotificationCoordinator,
    notifier: Option<MacNotifier>,
}

impl NotificationService {
    pub fn observe(
        &mut self,
        decision: Option<NotificationDecision>,
        muted: bool,
        is_show_status_icon: bool,
    ) {
        self.state.observe(decision, muted, is_show_status_icon);
    }

    pub fn clear_on_status_icon_hidden(&mut self) {
        self.state.observe(None, false, false);
    }

    pub fn bootstrap<S: NotifierReadySink>(&self, sink: S) {
        if thread::Builder::new()
            .name("ipchecker-notifier-bootstrap".to_owned())
            .spawn(move || {
                let mut notifier = MacNotifier::new();
                let result = notifier
                    .authorize()
                    .map(|()| notifier)
                    .map_err(|error| error.to_string());
                sink.send(result);
            })
            .is_err()
        {
            log::warn!("failed to start notification authorization");
        }
    }

    pub fn finish_bootstrap(
        &mut self,
        result: Result<MacNotifier, String>,
        is_show_status_icon: bool,
    ) {
        match result {
            Ok(notifier) => self.notifier = Some(notifier),
            Err(error) => {
                self.state.observe(None, false, is_show_status_icon);
                log::warn!("notification authorization unavailable: {error}");
            }
        }
    }

    pub fn deliver_pending<S: ActionSink>(&mut self, action_sink: S) {
        let Some(decision) = self.state.pending() else {
            return;
        };
        let Some(notifier) = &mut self.notifier else {
            return;
        };
        match notifier.send(decision.clone(), Box::new(action_sink)) {
            Ok(()) => self.state.mark_delivered(&decision),
            Err(error) => log::warn!("failed to send notification: {error}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventSinkClosed;

impl fmt::Display for EventSinkClosed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("event sink is closed")
    }
}

impl Error for EventSinkClosed {}

#[derive(Debug)]
pub enum WorkerCommand {
    CheckNow,
    SetInterval(Duration),
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerEvent {
    FetchCompleted {
        result: Result<Ipv4Addr, FetchError>,
        vpn_status: Result<VpnStatus, String>,
    },
}

pub trait EventSink: Clone + Send + 'static {
    fn send(&self, event: WorkerEvent) -> Result<(), EventSinkClosed>;
}

pub struct WorkerHandle {
    commands: Sender<WorkerCommand>,
    thread: Option<JoinHandle<()>>,
}

impl WorkerHandle {
    pub fn start<S, E>(source: S, interval: Duration, sink: E) -> Self
    where
        S: IpSource,
        E: EventSink,
    {
        let (commands, receiver) = mpsc::channel();
        let thread = thread::spawn(move || run_worker(source, interval, sink, receiver));

        Self {
            commands,
            thread: Some(thread),
        }
    }

    pub fn command(&self, command: WorkerCommand) -> Result<(), mpsc::SendError<WorkerCommand>> {
        self.commands.send(command)
    }
}

impl Drop for WorkerHandle {
    fn drop(&mut self) {
        let _ = self.commands.send(WorkerCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run_worker<S, E>(
    mut source: S,
    mut interval: Duration,
    sink: E,
    commands: mpsc::Receiver<WorkerCommand>,
) where
    S: IpSource,
    E: EventSink,
{
    if !check(&mut source, &sink) {
        return;
    }

    loop {
        let keep_running = match commands.recv_timeout(interval) {
            Ok(first) => {
                let mut should_check = false;
                let mut should_shutdown = false;
                for command in iter::once(first).chain(commands.try_iter()) {
                    match command {
                        WorkerCommand::CheckNow => should_check = true,
                        WorkerCommand::SetInterval(next) => {
                            interval = next;
                            should_check = true;
                        }
                        WorkerCommand::Shutdown => should_shutdown = true,
                    }
                }

                if should_shutdown {
                    break;
                }
                !should_check || check(&mut source, &sink)
            }
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => check(&mut source, &sink),
        };

        if !keep_running {
            break;
        }
    }
}

fn check<S, E>(source: &mut S, sink: &E) -> bool
where
    S: IpSource,
    E: EventSink,
{
    let event = observe_ip(source, detect_vpn_status);
    if sink.send(event).is_err() {
        eprintln!("ipchecker worker stopped because the event sink is closed");
        return false;
    }

    true
}

fn observe_ip<S, F>(source: &mut S, mut detect: F) -> WorkerEvent
where
    S: IpSource,
    F: FnMut() -> std::io::Result<VpnStatus>,
{
    // Keep the VPN evidence with the request, even if the UI handles it later.
    let before = detect().map_err(|error| error.to_string());
    let result = source.fetch();
    let after = detect().map_err(|error| error.to_string());
    let vpn_status = match (before, after) {
        (Err(error), _) | (_, Err(error)) => Err(error),
        (Ok(VpnStatus::Inactive), Ok(VpnStatus::Inactive)) => Ok(VpnStatus::Inactive),
        _ => Ok(VpnStatus::Active),
    };
    WorkerEvent::FetchCompleted { result, vpn_status }
}

#[cfg(test)]
mod observation_tests {
    use super::*;
    use crate::vpn_detection::{DailyIpRecordDecision, decide_daily_ip_recording};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    struct SwitchingSource {
        active: Arc<AtomicBool>,
        next_active: bool,
        fail: bool,
    }

    impl IpSource for SwitchingSource {
        fn fetch(&mut self) -> Result<Ipv4Addr, FetchError> {
            self.active.store(self.next_active, Ordering::SeqCst);
            if self.fail {
                Err(FetchError::AllSourcesFailed(Vec::new()))
            } else {
                Ok(Ipv4Addr::new(192, 0, 2, 1))
            }
        }
    }

    #[test]
    fn vpn_evidence_brackets_fetch_and_survives_delayed_delivery() {
        for (before, after, expected) in [
            (true, false, DailyIpRecordDecision::SkipVpn),
            (false, true, DailyIpRecordDecision::SkipVpn),
            (true, true, DailyIpRecordDecision::SkipVpn),
            (false, false, DailyIpRecordDecision::Record),
        ] {
            let active = Arc::new(AtomicBool::new(before));
            let mut source = SwitchingSource {
                active: Arc::clone(&active),
                next_active: after,
                fail: false,
            };
            let WorkerEvent::FetchCompleted { result, vpn_status } =
                observe_ip(&mut source, || {
                    Ok(if active.load(Ordering::SeqCst) {
                        VpnStatus::Active
                    } else {
                        VpnStatus::Inactive
                    })
                });
            // The UI may handle the event after another network change.
            active.store(false, Ordering::SeqCst);
            assert_eq!(result, Ok(Ipv4Addr::new(192, 0, 2, 1)));
            assert_eq!(
                decide_daily_ip_recording(false, || vpn_status.clone()),
                expected
            );
            assert_eq!(
                decide_daily_ip_recording(true, || vpn_status),
                DailyIpRecordDecision::Record
            );
        }
    }

    #[test]
    fn either_detection_failure_prevents_filtered_logging_without_losing_ip() {
        for failed_call in [0, 1] {
            let mut source = SwitchingSource {
                active: Arc::new(AtomicBool::new(false)),
                next_active: false,
                fail: false,
            };
            let mut call = 0;
            let WorkerEvent::FetchCompleted { result, vpn_status } =
                observe_ip(&mut source, || {
                    let fails = call == failed_call;
                    call += 1;
                    if fails {
                        Err(std::io::Error::other("detection failed"))
                    } else {
                        Ok(VpnStatus::Inactive)
                    }
                });
            assert!(result.is_ok());
            assert_eq!(call, 2);
            assert_eq!(
                decide_daily_ip_recording(false, || vpn_status),
                DailyIpRecordDecision::SkipDetectionFailed
            );
        }
    }

    #[test]
    fn failed_fetch_remains_a_failure() {
        let mut source = SwitchingSource {
            active: Arc::new(AtomicBool::new(false)),
            next_active: false,
            fail: true,
        };
        let WorkerEvent::FetchCompleted { result, .. } =
            observe_ip(&mut source, || Ok(VpnStatus::Inactive));
        assert_eq!(result, Err(FetchError::AllSourcesFailed(Vec::new())));
    }
}
