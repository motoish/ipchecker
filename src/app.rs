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
    FetchCompleted(Result<Ipv4Addr, FetchError>),
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
    let event = WorkerEvent::FetchCompleted(source.fetch());
    if sink.send(event).is_err() {
        eprintln!("ipchecker worker stopped because the event sink is closed");
        return false;
    }

    true
}
