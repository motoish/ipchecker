use std::{net::Ipv4Addr, time::Duration};

use ipchecker::{
    app::NotificationCoordinator,
    monitor::NotificationDecision,
    notification::{
        ActionSink, AuthorizationResolution, AuthorizationState, NotificationAction,
        NotificationActionSpec, NotificationResponseKind, action_for_response, content_for,
        state_change_for_response,
    },
};

fn ip(value: &str) -> Ipv4Addr {
    value.parse().expect("test IP address should parse")
}

#[test]
fn repeated_pending_condition_is_not_cleared_before_delivery() {
    let mut coordinator = NotificationCoordinator::default();
    let decision = NotificationDecision::FetchFailure;

    coordinator.observe(Some(decision.clone()), false);
    coordinator.observe(Some(decision.clone()), false);

    assert_eq!(coordinator.pending(), Some(decision));
}

#[test]
fn delivered_condition_is_suppressed_until_the_episode_changes() {
    let mut coordinator = NotificationCoordinator::default();
    let first = NotificationDecision::Mismatch {
        current: ip("192.0.2.1"),
        expected: ip("192.0.2.2"),
    };

    coordinator.observe(Some(first.clone()), false);
    coordinator.mark_delivered(&first);
    coordinator.observe(Some(first), false);
    assert_eq!(coordinator.pending(), None);

    coordinator.observe(None, false);
    let next = NotificationDecision::Mismatch {
        current: ip("192.0.2.3"),
        expected: ip("192.0.2.2"),
    };
    coordinator.observe(Some(next.clone()), false);
    assert_eq!(coordinator.pending(), Some(next));
}

#[test]
fn muting_clears_the_active_notification_episode() {
    let mut coordinator = NotificationCoordinator::default();
    coordinator.observe(Some(NotificationDecision::FetchFailure), false);

    coordinator.observe(Some(NotificationDecision::FetchFailure), true);

    assert_eq!(coordinator.pending(), None);
}

fn assert_send_sync_static<T: Send + Sync + 'static>() {}

#[test]
fn mismatch_copy_contains_current_and_expected_addresses() {
    rust_i18n::set_locale("zh-CN");
    let content = content_for(NotificationDecision::Mismatch {
        current: ip("192.0.2.1"),
        expected: ip("192.0.2.2"),
    });

    assert_eq!(content.title, "公网 IP 与期望不符");
    assert_eq!(content.body, "当前 192.0.2.1，期望 192.0.2.2");
    assert_eq!(
        content.actions,
        vec![
            NotificationActionSpec {
                identifier: "continue",
                label: "继续提醒".to_owned(),
                action: NotificationAction::Continue,
            },
            NotificationActionSpec {
                identifier: "mute-session",
                label: "本会话不再提醒".to_owned(),
                action: NotificationAction::MuteSession,
            },
        ]
    );
    assert_eq!(content.response_timeout, Some(Duration::from_secs(55)));
}

#[test]
fn failure_copy_is_mild_and_has_no_actions() {
    rust_i18n::set_locale("zh-CN");
    let content = content_for(NotificationDecision::FetchFailure);

    assert_eq!(content.title, "无法获取公网 IP");
    assert!(content.actions.is_empty());
    assert_eq!(content.response_timeout, None);
}

#[test]
fn denied_authorization_reports_once_and_disables_delivery() {
    let mut authorization = AuthorizationState::default();

    assert!(authorization.needs_request());
    assert_eq!(
        authorization.resolve(false),
        AuthorizationResolution::Disabled {
            report_denial: true,
        }
    );
    assert_eq!(authorization, AuthorizationState::Disabled);
    assert!(!authorization.needs_request());
    assert!(!authorization.delivery_enabled());
    assert_eq!(
        authorization.resolve(false),
        AuthorizationResolution::Disabled {
            report_denial: false,
        }
    );
}

#[test]
fn granted_authorization_enables_delivery_without_another_request() {
    let mut authorization = AuthorizationState::default();

    assert_eq!(
        authorization.resolve(true),
        AuthorizationResolution::Enabled
    );
    assert_eq!(authorization, AuthorizationState::Enabled);
    assert!(!authorization.needs_request());
    assert!(authorization.delivery_enabled());
}

#[test]
fn response_mapping_classifies_buttons_and_passive_responses() {
    assert_eq!(
        action_for_response(NotificationResponseKind::Action("continue")),
        Some(NotificationAction::Continue)
    );
    assert_eq!(
        action_for_response(NotificationResponseKind::Action("mute-session")),
        Some(NotificationAction::MuteSession)
    );
    assert_eq!(
        action_for_response(NotificationResponseKind::Action("unknown")),
        None
    );
    assert_eq!(action_for_response(NotificationResponseKind::Default), None);
    assert_eq!(
        action_for_response(NotificationResponseKind::Dismissed),
        None
    );
    assert_eq!(
        action_for_response(NotificationResponseKind::TimedOut),
        None
    );
}

#[test]
fn only_mute_is_forwarded_as_a_state_change() {
    assert_eq!(
        state_change_for_response(NotificationResponseKind::Action("mute-session")),
        Some(NotificationAction::MuteSession)
    );
    assert_eq!(
        state_change_for_response(NotificationResponseKind::Action("continue")),
        None
    );
    assert_eq!(
        state_change_for_response(NotificationResponseKind::Default),
        None
    );
}

#[test]
fn action_sink_trait_objects_are_response_thread_safe() {
    assert_send_sync_static::<Box<dyn ActionSink>>();
}
