use std::net::Ipv4Addr;

use ipchecker::ip_source::FetchError;
use ipchecker::monitor::{Monitor, MonitorOutcome, MonitorState, NotificationDecision};
use ipchecker::session::Session;

fn ip(value: &str) -> Ipv4Addr {
    value.parse().expect("test IP address should parse")
}

#[test]
fn unset_expected_ip_is_unconfigured_and_silent() {
    let outcome = Monitor::default().apply(Ok(ip("203.0.113.1")), None, false);
    assert_eq!(outcome.state, MonitorState::Unconfigured);
    assert_eq!(outcome.notification, None);
}

#[test]
fn mismatch_notifies_on_each_unmuted_poll() {
    let mut monitor = Monitor::default();
    let expected = Some(ip("203.0.113.2"));
    for _ in 0..2 {
        let outcome = monitor.apply(Ok(ip("203.0.113.1")), expected, false);
        assert_eq!(outcome.state, MonitorState::Mismatched);
        assert!(matches!(
            outcome.notification,
            Some(NotificationDecision::Mismatch { .. })
        ));
    }
}

#[test]
fn mute_suppresses_notification_without_hiding_mismatch() {
    let outcome = Monitor::default().apply(Ok(ip("203.0.113.1")), Some(ip("203.0.113.2")), true);
    assert_eq!(outcome.state, MonitorState::Mismatched);
    assert_eq!(outcome.notification, None);
}

#[test]
fn failure_remains_an_active_notification_condition_until_success() {
    let mut monitor = Monitor::default();
    let first = monitor.apply(Err(FetchError::AllSourcesFailed(vec![])), None, false);
    let second = monitor.apply(Err(FetchError::AllSourcesFailed(vec![])), None, false);
    assert_eq!(first.notification, Some(NotificationDecision::FetchFailure));
    assert_eq!(
        second.notification,
        Some(NotificationDecision::FetchFailure)
    );
    monitor.apply(Ok(ip("203.0.113.1")), None, false);
    let later = monitor.apply(Err(FetchError::AllSourcesFailed(vec![])), None, false);
    assert_eq!(later.notification, Some(NotificationDecision::FetchFailure));
}

#[test]
fn a_fresh_session_is_never_muted() {
    let mut session = Session::new();
    session.set_muted(true);
    assert!(session.is_muted());
    assert!(!Session::new().is_muted());
}

#[test]
fn equal_addresses_produce_matched_without_notification() {
    let outcome = Monitor::default().apply(Ok(ip("203.0.113.1")), Some(ip("203.0.113.1")), false);
    assert_eq!(outcome.state, MonitorState::Matched);
    assert_eq!(outcome.notification, None);
}

#[test]
fn failure_retains_last_success_ip() {
    let mut monitor = Monitor::default();
    monitor.apply(Ok(ip("203.0.113.1")), None, false);

    let outcome = monitor.apply(Err(FetchError::AllSourcesFailed(vec![])), None, false);

    assert_eq!(outcome.state, MonitorState::Unknown);
    assert_eq!(outcome.current_ip, None);
    assert_eq!(outcome.last_success_ip, Some(ip("203.0.113.1")));
}

#[test]
fn recomparing_without_a_current_ip_clears_the_old_notification_decision() {
    let outcome = Monitor::default().apply(Err(FetchError::AllSourcesFailed(vec![])), None, false);

    let recomputed = outcome.recompare_expected(Some(ip("203.0.113.2")));

    assert_eq!(recomputed.state, MonitorState::Unknown);
    assert_eq!(recomputed.notification, None);
}

#[test]
fn failure_never_creates_a_mismatch_decision() {
    let outcome = Monitor::default().apply(
        Err(FetchError::AllSourcesFailed(vec![])),
        Some(ip("203.0.113.2")),
        false,
    );

    assert_eq!(outcome.state, MonitorState::Unknown);
    assert!(!matches!(
        outcome.notification,
        Some(NotificationDecision::Mismatch { .. })
    ));
}

#[test]
fn monitor_outcome_exposes_successful_current_and_last_success_ip() {
    let outcome: MonitorOutcome =
        Monitor::default().apply(Ok(ip("203.0.113.1")), Some(ip("203.0.113.1")), false);

    assert_eq!(outcome.current_ip, Some(ip("203.0.113.1")));
    assert_eq!(outcome.last_success_ip, Some(ip("203.0.113.1")));
}
