use std::{
    fs,
    net::Ipv4Addr,
    path::{Path, PathBuf},
    sync::mpsc::{self, Sender},
};

use chrono::NaiveDate;
use ipchecker::daily_ip_log::{
    DailyIpLogAlertState, DailyIpLogEvent, DailyIpLogEventSink, DailyIpLogHandle,
    DailyIpLogWorkerClosed, RecordOutcome, record_public_ip,
};

#[derive(Clone)]
struct IgnoredEventSink;

impl DailyIpLogEventSink for IgnoredEventSink {
    fn send(&self, _event: DailyIpLogEvent) -> Result<(), DailyIpLogWorkerClosed> {
        Ok(())
    }
}

#[derive(Clone)]
struct ChannelEventSink {
    sender: Sender<DailyIpLogEvent>,
}

impl DailyIpLogEventSink for ChannelEventSink {
    fn send(&self, event: DailyIpLogEvent) -> Result<(), DailyIpLogWorkerClosed> {
        self.sender.send(event).map_err(|_| DailyIpLogWorkerClosed)
    }
}

#[test]
fn successful_observation_creates_the_monthly_csv() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let date = NaiveDate::from_ymd_opt(2026, 9, 1).expect("valid date");

    let outcome = record_public_ip(directory.path(), date, Ipv4Addr::new(203, 0, 113, 10))
        .expect("record public IP");

    assert_eq!(outcome, RecordOutcome::Written);
    assert_eq!(
        fs::read_to_string(
            directory
                .path()
                .join("ipchecker-daily-global-ip-log-2026-09.csv")
        )
        .expect("read monthly CSV"),
        "date,ips\n2026-09-01,\"203.0.113.10\"\n"
    );
}

#[test]
fn same_day_addresses_are_deduplicated_in_first_observed_order() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let date = NaiveDate::from_ymd_opt(2026, 9, 1).expect("valid date");
    let first = Ipv4Addr::new(203, 0, 113, 10);
    let second = Ipv4Addr::new(198, 51, 100, 20);

    assert_eq!(
        record_public_ip(directory.path(), date, first).expect("record first IP"),
        RecordOutcome::Written
    );
    assert_eq!(
        record_public_ip(directory.path(), date, first).expect("record duplicate IP"),
        RecordOutcome::Unchanged
    );
    assert_eq!(
        record_public_ip(directory.path(), date, second).expect("record second IP"),
        RecordOutcome::Written
    );

    assert_eq!(
        fs::read_to_string(
            directory
                .path()
                .join("ipchecker-daily-global-ip-log-2026-09.csv")
        )
        .expect("read monthly CSV"),
        "date,ips\n2026-09-01,\"203.0.113.10;198.51.100.20\"\n"
    );
}

#[test]
fn dates_are_sorted_and_a_new_month_uses_a_new_file() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let september_first = NaiveDate::from_ymd_opt(2026, 9, 1).expect("valid date");
    let september_second = NaiveDate::from_ymd_opt(2026, 9, 2).expect("valid date");
    let october_first = NaiveDate::from_ymd_opt(2026, 10, 1).expect("valid date");

    record_public_ip(
        directory.path(),
        september_second,
        Ipv4Addr::new(198, 51, 100, 20),
    )
    .expect("record September 2 IP");
    record_public_ip(
        directory.path(),
        september_first,
        Ipv4Addr::new(203, 0, 113, 10),
    )
    .expect("record September 1 IP");
    record_public_ip(
        directory.path(),
        october_first,
        Ipv4Addr::new(192, 0, 2, 30),
    )
    .expect("record October IP");

    assert_eq!(
        fs::read_to_string(
            directory
                .path()
                .join("ipchecker-daily-global-ip-log-2026-09.csv")
        )
        .expect("read September CSV"),
        concat!(
            "date,ips\n",
            "2026-09-01,\"203.0.113.10\"\n",
            "2026-09-02,\"198.51.100.20\"\n"
        )
    );
    assert_eq!(
        fs::read_to_string(
            directory
                .path()
                .join("ipchecker-daily-global-ip-log-2026-10.csv")
        )
        .expect("read October CSV"),
        "date,ips\n2026-10-01,\"192.0.2.30\"\n"
    );
}

#[test]
fn malformed_existing_csv_is_never_overwritten() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory
        .path()
        .join("ipchecker-daily-global-ip-log-2026-09.csv");
    let malformed = "wrong,header\nkeep,this file\n";
    fs::write(&path, malformed).expect("write malformed CSV");

    let result = record_public_ip(
        directory.path(),
        NaiveDate::from_ymd_opt(2026, 9, 1).expect("valid date"),
        Ipv4Addr::new(203, 0, 113, 10),
    );

    assert!(result.is_err());
    assert_eq!(
        fs::read_to_string(path).expect("read original malformed CSV"),
        malformed
    );
}

#[test]
fn duplicate_date_rows_are_rejected_without_overwriting_the_file() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory
        .path()
        .join("ipchecker-daily-global-ip-log-2026-09.csv");
    let duplicate_dates = concat!(
        "date,ips\n",
        "2026-09-01,\"203.0.113.10\"\n",
        "2026-09-01,\"198.51.100.20\"\n"
    );
    fs::write(&path, duplicate_dates).expect("write duplicate dates");

    let result = record_public_ip(
        directory.path(),
        NaiveDate::from_ymd_opt(2026, 9, 1).expect("valid date"),
        Ipv4Addr::new(192, 0, 2, 30),
    );

    assert!(result.is_err());
    assert_eq!(
        fs::read_to_string(path).expect("read original CSV"),
        duplicate_dates
    );
}

#[test]
fn worker_drains_queued_records_before_shutdown() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let date = NaiveDate::from_ymd_opt(2026, 9, 1).expect("valid date");
    let worker = DailyIpLogHandle::start(IgnoredEventSink);

    worker
        .record(
            directory.path().to_owned(),
            date,
            Ipv4Addr::new(203, 0, 113, 10),
        )
        .expect("queue first IP");
    worker
        .record(
            directory.path().to_owned(),
            date,
            Ipv4Addr::new(198, 51, 100, 20),
        )
        .expect("queue second IP");
    drop(worker);

    assert_eq!(
        fs::read_to_string(
            directory
                .path()
                .join("ipchecker-daily-global-ip-log-2026-09.csv")
        )
        .expect("read worker output"),
        "date,ips\n2026-09-01,\"203.0.113.10;198.51.100.20\"\n"
    );
}

#[test]
fn worker_reports_recorded_and_failed_events_with_directory() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let date = NaiveDate::from_ymd_opt(2026, 9, 1).expect("valid date");
    let missing_directory = directory.path().join("missing");
    let (sender, events) = mpsc::channel();
    let worker = DailyIpLogHandle::start(ChannelEventSink { sender });

    worker
        .record(
            directory.path().to_owned(),
            date,
            Ipv4Addr::new(203, 0, 113, 10),
        )
        .expect("queue first IP");
    worker
        .record(
            directory.path().to_owned(),
            date,
            Ipv4Addr::new(203, 0, 113, 10),
        )
        .expect("queue duplicate IP");
    worker
        .record(
            missing_directory.clone(),
            date,
            Ipv4Addr::new(203, 0, 113, 10),
        )
        .expect("queue failing write");
    drop(worker);

    let received: Vec<_> = events.into_iter().collect();
    assert_eq!(received.len(), 3);
    assert_eq!(
        received[0],
        DailyIpLogEvent::Recorded {
            directory: directory.path().to_owned(),
            outcome: RecordOutcome::Written,
        }
    );
    assert_eq!(
        received[1],
        DailyIpLogEvent::Recorded {
            directory: directory.path().to_owned(),
            outcome: RecordOutcome::Unchanged,
        }
    );
    assert!(matches!(
        &received[2],
        DailyIpLogEvent::Failed { directory, .. } if directory == &missing_directory
    ));
}

#[test]
fn alert_state_resets_after_success_or_explicit_reset_and_ignores_stale_events() {
    let first_dir = PathBuf::from("/tmp/ipchecker-first");
    let second_dir = PathBuf::from("/tmp/ipchecker-second");
    let first_failure = DailyIpLogEvent::Failed {
        directory: first_dir.clone(),
        error: "permission denied".to_owned(),
    };
    let first_success = DailyIpLogEvent::Recorded {
        directory: first_dir.clone(),
        outcome: RecordOutcome::Written,
    };
    let second_failure = DailyIpLogEvent::Failed {
        directory: second_dir.clone(),
        error: "permission denied".to_owned(),
    };

    let mut state = DailyIpLogAlertState::default();

    // First failure alerts once; repeated failures stay suppressed.
    assert!(state.observe(&first_failure, true, Some(Path::new(&first_dir))));
    assert!(!state.observe(&first_failure, true, Some(Path::new(&first_dir))));

    // Recovery via a successful write re-arms the alert for a future failure.
    assert!(!state.observe(&first_success, true, Some(Path::new(&first_dir))));
    assert!(state.observe(&first_failure, true, Some(Path::new(&first_dir))));

    // Explicit reset (directory change / re-enable) re-arms the alert while
    // ignoring stale failures from the previous directory or while disabled.
    state.reset();
    assert!(!state.observe(&first_failure, true, Some(Path::new(&second_dir))));
    assert!(!state.observe(&second_failure, false, Some(Path::new(&second_dir))));
    assert!(state.observe(&second_failure, true, Some(Path::new(&second_dir))));
}
