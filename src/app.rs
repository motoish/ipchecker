use std::{
    error::Error,
    fmt,
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
pub struct PendingNotificationDecision(Option<NotificationDecision>);

impl PendingNotificationDecision {
    pub fn replace(&mut self, decision: Option<NotificationDecision>) {
        self.0 = decision;
    }

    pub fn take(&mut self) -> Option<NotificationDecision> {
        self.0.take()
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
            Ok(WorkerCommand::CheckNow) => check(&mut source, &sink),
            Ok(WorkerCommand::SetInterval(next)) => {
                interval = next;
                check(&mut source, &sink)
            }
            Ok(WorkerCommand::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
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
