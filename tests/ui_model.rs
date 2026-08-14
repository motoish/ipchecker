use std::net::Ipv4Addr;

use ipchecker::{
    app::PendingNotificationDecision,
    config::Config,
    monitor::{Monitor, MonitorOutcome, MonitorState, NotificationDecision},
    session::Session,
    ui::{
        FeedbackRestoreGuard, IconState, MenuAction, UiCommand, UiError, UiModel,
        icon_rgba_for_state, install_app_edit_menu,
    },
};

fn ip(value: &str) -> Ipv4Addr {
    value.parse().expect("test IP address should parse")
}

fn model_for(
    state: MonitorState,
    last_success_ip: Option<Ipv4Addr>,
    expected_ip: Option<Ipv4Addr>,
    muted: bool,
) -> UiModel {
    let config = Config {
        expected_ip,
        interval_minutes: 5,
    };
    let mut session = Session::new();
    session.set_muted(muted);
    let outcome = MonitorOutcome {
        state,
        current_ip: None,
        last_success_ip,
        notification: None,
    };

    UiModel::from_state(&config, &session, &outcome)
}

fn pixel_alpha(rgba: &[u8], x: u32, y: u32) -> u8 {
    rgba[((y * 36 + x) * 4 + 3) as usize]
}

#[test]
fn template_icons_use_filled_disc_with_carved_glyphs() {
    let normal = icon_rgba_for_state(IconState::Normal);
    let alert = icon_rgba_for_state(IconState::Alert);
    let unknown = icon_rgba_for_state(IconState::Unknown);

    assert_eq!(normal.len(), 36 * 36 * 4);
    assert_eq!(alert.len(), 36 * 36 * 4);
    assert_eq!(unknown.len(), 36 * 36 * 4);

    // Filled disc interior away from glyphs stays opaque.
    assert!(pixel_alpha(&normal, 8, 18) > 200);
    assert!(pixel_alpha(&alert, 8, 18) > 200);
    assert!(pixel_alpha(&unknown, 8, 18) > 200);

    // Outside the disc stays transparent.
    assert_eq!(pixel_alpha(&normal, 1, 1), 0);

    // Antialiased edge should produce partial-alpha pixels.
    assert!(normal.chunks(4).any(|pixel| pixel[3] > 0 && pixel[3] < 255));

    assert_ne!(normal, alert);
    assert_ne!(normal, unknown);
    assert_ne!(alert, unknown);
}

#[test]
fn glyph_shapes_are_knocked_out_of_the_filled_disc() {
    let alert = icon_rgba_for_state(IconState::Alert);

    // X diagonals meet at the center; that core should be carved transparent.
    assert!(
        pixel_alpha(&alert, 18, 18) < 40,
        "glyph knockout should clear the core, got alpha {}",
        pixel_alpha(&alert, 18, 18)
    );
}

#[test]
fn unknown_with_last_success_keeps_last_ip_visible() {
    rust_i18n::set_locale("zh-CN");
    let model = model_for(MonitorState::Unknown, Some(ip("192.0.2.1")), None, false);

    assert_eq!(model.current_title, "当前公网 IP：192.0.2.1");
    assert_eq!(model.icon_state, IconState::Unknown);
}

#[test]
fn current_ip_row_is_copyable_only_when_an_address_is_known() {
    rust_i18n::set_locale("zh-CN");

    let without_ip = model_for(MonitorState::Unknown, None, None, false);
    assert!(!without_ip.can_copy_current_ip);
    assert_eq!(without_ip.current_title, "当前公网 IP：未知");

    let with_ip = model_for(MonitorState::Unknown, Some(ip("192.0.2.1")), None, false);
    assert!(with_ip.can_copy_current_ip);
    assert_eq!(with_ip.current_title, "当前公网 IP：192.0.2.1");
}

#[test]
fn unset_expected_disables_use_current_until_success() {
    rust_i18n::set_locale("zh-CN");
    let model = model_for(MonitorState::Unknown, None, None, false);

    assert_eq!(model.expected_title, "期望 IP：未设置");
    assert!(!model.can_use_current_ip);
}

#[test]
fn muted_mismatch_remains_alert_and_checks_menu_toggle() {
    let model = model_for(
        MonitorState::Mismatched,
        Some(ip("192.0.2.1")),
        Some(ip("192.0.2.2")),
        true,
    );

    assert_eq!(model.icon_state, IconState::Alert);
    assert!(model.muted);
}

#[test]
fn tooltip_uses_documented_copy() {
    rust_i18n::set_locale("zh-CN");
    let model = model_for(
        MonitorState::Matched,
        Some(ip("192.0.2.1")),
        Some(ip("192.0.2.1")),
        false,
    );

    assert_eq!(model.tooltip, "当前: 192.0.2.1 | 期望: 192.0.2.1");
}

#[cfg(target_os = "macos")]
#[test]
fn exposes_app_edit_menu_installer_for_dialog_shortcuts() {
    let _install: fn() -> Result<tray_icon::menu::Menu, UiError> = install_app_edit_menu;
}

#[test]
fn interval_menu_actions_preserve_the_documented_minute_values() {
    for minutes in [1, 5, 15, 30, 60] {
        assert_eq!(
            UiCommand::from_menu_action(MenuAction::SetInterval(minutes), false),
            UiCommand::SetInterval(minutes)
        );
    }
}

#[test]
fn stateless_menu_actions_map_to_their_matching_commands() {
    for (action, expected) in [
        (
            MenuAction::SetExpectedFromInput,
            UiCommand::SetExpectedFromInput,
        ),
        (MenuAction::UseCurrentIp, UiCommand::UseCurrentIp),
        (MenuAction::CopyCurrentIp, UiCommand::CopyCurrentIp),
        (MenuAction::CheckNow, UiCommand::CheckNow),
        (MenuAction::About, UiCommand::About),
        (MenuAction::Quit, UiCommand::Quit),
    ] {
        assert_eq!(UiCommand::from_menu_action(action, false), expected);
    }
}

#[test]
fn mute_menu_action_inverts_only_the_current_session_choice() {
    assert_eq!(
        UiCommand::from_menu_action(MenuAction::ToggleMuted, false),
        UiCommand::SetMuted(true)
    );
    assert_eq!(
        UiCommand::from_menu_action(MenuAction::ToggleMuted, true),
        UiCommand::SetMuted(false)
    );
}

#[test]
fn changing_expected_from_match_to_different_ip_recompares_immediately() {
    let outcome = Monitor::default().apply(Ok(ip("192.0.2.1")), Some(ip("192.0.2.1")), false);

    let recomputed = outcome.recompare_expected(Some(ip("192.0.2.2")));

    assert_eq!(recomputed.state, MonitorState::Mismatched);
    assert_eq!(recomputed.current_ip, Some(ip("192.0.2.1")));
    assert_eq!(recomputed.notification, None);
}

#[test]
fn changing_expected_from_mismatch_to_current_ip_recompares_immediately() {
    let outcome = Monitor::default().apply(Ok(ip("192.0.2.1")), Some(ip("192.0.2.2")), false);

    let recomputed = outcome.recompare_expected(Some(ip("192.0.2.1")));

    assert_eq!(recomputed.state, MonitorState::Matched);
    assert_eq!(recomputed.current_ip, Some(ip("192.0.2.1")));
    assert_eq!(recomputed.notification, None);
}

#[test]
fn pending_notification_keeps_only_the_latest_decision_and_clears_on_take() {
    let mut pending = PendingNotificationDecision::default();
    pending.replace(Some(NotificationDecision::Mismatch {
        current: ip("192.0.2.1"),
        expected: ip("192.0.2.2"),
    }));
    pending.replace(Some(NotificationDecision::FetchFailure));

    assert_eq!(pending.take(), Some(NotificationDecision::FetchFailure));
    assert_eq!(pending.take(), None);
}

#[test]
fn pending_notification_is_cleared_by_a_new_outcome_without_a_decision() {
    let mut pending = PendingNotificationDecision::default();
    pending.replace(Some(NotificationDecision::FetchFailure));

    pending.replace(None);

    assert_eq!(pending.take(), None);
}

#[test]
fn newer_feedback_supersedes_an_older_restore_token() {
    let mut guard = FeedbackRestoreGuard::default();
    let older = guard.issue();
    let newer = guard.issue();

    assert!(!guard.claim(older));
    assert!(guard.claim(newer));
}

#[test]
fn current_feedback_restore_token_can_be_claimed_only_once() {
    let mut guard = FeedbackRestoreGuard::default();
    let token = guard.issue();

    assert!(guard.claim(token));
    assert!(!guard.claim(token));
}

#[test]
fn normal_ui_update_cancels_pending_feedback_restore() {
    let mut guard = FeedbackRestoreGuard::default();
    let token = guard.issue();

    guard.cancel();

    assert!(!guard.claim(token));
}
