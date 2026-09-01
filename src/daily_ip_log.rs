use std::{
    collections::BTreeMap,
    fs::File,
    io::Write,
    net::Ipv4Addr,
    path::{Path, PathBuf},
    sync::mpsc::{self, Sender},
    thread::{self, JoinHandle},
};

use chrono::{Datelike, NaiveDate};
use tempfile::NamedTempFile;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordOutcome {
    Written,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DailyIpLogEvent {
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("daily public IP log worker is closed")]
pub struct DailyIpLogWorkerClosed;

pub trait DailyIpLogEventSink: Clone + Send + 'static {
    fn send(&self, event: DailyIpLogEvent) -> Result<(), DailyIpLogWorkerClosed>;
}

enum DailyIpLogCommand {
    Record {
        directory: PathBuf,
        date: NaiveDate,
        ip: Ipv4Addr,
    },
    Shutdown,
}

pub struct DailyIpLogHandle {
    commands: Sender<DailyIpLogCommand>,
    thread: Option<JoinHandle<()>>,
}

impl DailyIpLogHandle {
    pub fn start<S: DailyIpLogEventSink>(sink: S) -> Self {
        let (commands, receiver) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("ipchecker-daily-ip-log".to_owned())
            .spawn(move || {
                while let Ok(command) = receiver.recv() {
                    match command {
                        DailyIpLogCommand::Record {
                            directory,
                            date,
                            ip,
                        } => {
                            if let Err(error) = record_public_ip(&directory, date, ip)
                                && sink
                                    .send(DailyIpLogEvent::Failed(error.to_string()))
                                    .is_err()
                            {
                                log::debug!(
                                    "daily IP log error arrived after the event sink closed"
                                );
                            }
                        }
                        DailyIpLogCommand::Shutdown => break,
                    }
                }
            })
            .expect("failed to start daily public IP log worker");
        Self {
            commands,
            thread: Some(thread),
        }
    }

    pub fn record(
        &self,
        directory: PathBuf,
        date: NaiveDate,
        ip: Ipv4Addr,
    ) -> Result<(), DailyIpLogWorkerClosed> {
        self.commands
            .send(DailyIpLogCommand::Record {
                directory,
                date,
                ip,
            })
            .map_err(|_| DailyIpLogWorkerClosed)
    }
}

impl Drop for DailyIpLogHandle {
    fn drop(&mut self) {
        let _ = self.commands.send(DailyIpLogCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Debug, Error)]
pub enum DailyIpLogError {
    #[error("daily public IP log I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("daily public IP log CSV is invalid: {0}")]
    Csv(#[from] csv::Error),
    #[error("daily public IP log must use the header 'date,ips'")]
    InvalidHeader,
    #[error("daily public IP log contains an invalid date '{0}'")]
    InvalidDate(String),
    #[error("daily public IP log contains an invalid IPv4 address '{0}'")]
    InvalidIp(String),
    #[error("daily public IP log contains more than one row for '{0}'")]
    DuplicateDate(String),
}

pub fn record_public_ip(
    directory: &Path,
    date: NaiveDate,
    ip: Ipv4Addr,
) -> Result<RecordOutcome, DailyIpLogError> {
    let path = monthly_log_path(directory, date);
    let mut entries = if path.exists() {
        read_entries(&path)?
    } else {
        BTreeMap::new()
    };
    let addresses = entries.entry(date).or_default();
    if addresses.contains(&ip) {
        return Ok(RecordOutcome::Unchanged);
    }
    addresses.push(ip);

    let mut temporary = NamedTempFile::new_in(directory)?;
    writeln!(temporary, "date,ips")?;
    for (entry_date, addresses) in entries {
        let addresses = addresses
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(";");
        writeln!(temporary, "{entry_date},\"{addresses}\"")?;
    }
    temporary.flush()?;
    temporary
        .persist(path)
        .map_err(|error| DailyIpLogError::Io(error.error))?;
    Ok(RecordOutcome::Written)
}

fn read_entries(path: &Path) -> Result<BTreeMap<NaiveDate, Vec<Ipv4Addr>>, DailyIpLogError> {
    let mut reader = csv::ReaderBuilder::new().from_reader(File::open(path)?);
    let headers = reader.headers()?;
    if headers.len() != 2 || headers.get(0) != Some("date") || headers.get(1) != Some("ips") {
        return Err(DailyIpLogError::InvalidHeader);
    }

    let mut entries = BTreeMap::new();
    for record in reader.records() {
        let record = record?;
        let date_text = &record[0];
        let date = NaiveDate::parse_from_str(date_text, "%Y-%m-%d")
            .map_err(|_| DailyIpLogError::InvalidDate(date_text.to_owned()))?;
        let mut addresses = Vec::new();
        for address in record[1].split(';') {
            let ip = address
                .parse::<Ipv4Addr>()
                .map_err(|_| DailyIpLogError::InvalidIp(address.to_owned()))?;
            if !addresses.contains(&ip) {
                addresses.push(ip);
            }
        }
        if entries.insert(date, addresses).is_some() {
            return Err(DailyIpLogError::DuplicateDate(date_text.to_owned()));
        }
    }
    Ok(entries)
}

fn monthly_log_path(directory: &Path, date: NaiveDate) -> PathBuf {
    directory.join(format!(
        "ipchecker-daily-global-ip-log-{:04}-{:02}.csv",
        date.year(),
        date.month()
    ))
}
