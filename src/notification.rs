use std::time::Duration;

use crate::monitor::NotificationDecision;

const MISMATCH_RESPONSE_TIMEOUT: Duration = Duration::from_secs(55);

const CONTINUE_ACTION_ID: &str = "continue";
const MUTE_SESSION_ACTION_ID: &str = "mute-session";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationAction {
    Continue,
    MuteSession,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationActionSpec {
    pub identifier: &'static str,
    pub label: String,
    pub action: NotificationAction,
}

fn continue_action() -> NotificationActionSpec {
    NotificationActionSpec {
        identifier: CONTINUE_ACTION_ID,
        label: t!("notify.continue").to_string(),
        action: NotificationAction::Continue,
    }
}

fn mute_session_action() -> NotificationActionSpec {
    NotificationActionSpec {
        identifier: MUTE_SESSION_ACTION_ID,
        label: t!("notify.mute_session").to_string(),
        action: NotificationAction::MuteSession,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationContent {
    pub title: String,
    pub body: String,
    pub actions: Vec<NotificationActionSpec>,
    pub response_timeout: Option<Duration>,
}

pub fn content_for(decision: NotificationDecision) -> NotificationContent {
    match decision {
        NotificationDecision::Mismatch { current, expected } => NotificationContent {
            title: t!("notify.mismatch_title").to_string(),
            body: t!(
                "notify.mismatch_body",
                current = current,
                expected = expected
            )
            .to_string(),
            actions: vec![continue_action(), mute_session_action()],
            response_timeout: Some(MISMATCH_RESPONSE_TIMEOUT),
        },
        NotificationDecision::FetchFailure => NotificationContent {
            title: t!("notify.fetch_failure_title").to_string(),
            body: String::new(),
            actions: Vec::new(),
            response_timeout: None,
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationResponseKind<'a> {
    Action(&'a str),
    Default,
    Dismissed,
    TimedOut,
}

pub fn action_for_response(response: NotificationResponseKind<'_>) -> Option<NotificationAction> {
    match response {
        NotificationResponseKind::Action(identifier) => {
            if identifier == CONTINUE_ACTION_ID {
                Some(NotificationAction::Continue)
            } else if identifier == MUTE_SESSION_ACTION_ID {
                Some(NotificationAction::MuteSession)
            } else {
                None
            }
        }
        NotificationResponseKind::Default
        | NotificationResponseKind::Dismissed
        | NotificationResponseKind::TimedOut => None,
    }
}

pub fn state_change_for_response(
    response: NotificationResponseKind<'_>,
) -> Option<NotificationAction> {
    match action_for_response(response) {
        Some(NotificationAction::MuteSession) => Some(NotificationAction::MuteSession),
        Some(NotificationAction::Continue) | None => None,
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationState {
    #[default]
    NotRequested,
    Enabled,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationResolution {
    Enabled,
    Disabled { report_denial: bool },
}

impl AuthorizationState {
    pub fn needs_request(self) -> bool {
        self == Self::NotRequested
    }

    pub fn delivery_enabled(self) -> bool {
        self == Self::Enabled
    }

    pub fn resolve(&mut self, granted: bool) -> AuthorizationResolution {
        match (*self, granted) {
            (Self::NotRequested, true) => {
                *self = Self::Enabled;
                AuthorizationResolution::Enabled
            }
            (Self::NotRequested, false) => {
                *self = Self::Disabled;
                AuthorizationResolution::Disabled {
                    report_denial: true,
                }
            }
            (Self::Enabled, _) => AuthorizationResolution::Enabled,
            (Self::Disabled, _) => AuthorizationResolution::Disabled {
                report_denial: false,
            },
        }
    }
}

pub trait ActionSink: Send + Sync + 'static {
    fn send(&self, action: NotificationAction);
}

pub trait Notifier {
    fn authorize(&mut self) -> Result<(), NotifyError>;

    fn send(
        &mut self,
        decision: NotificationDecision,
        actions: Box<dyn ActionSink>,
    ) -> Result<(), NotifyError>;
}

#[derive(Debug, thiserror::Error)]
pub enum NotifyError {
    #[error("notifications are unsupported on this platform")]
    UnsupportedPlatform,

    #[cfg(target_os = "macos")]
    #[error(transparent)]
    Platform(#[from] mac_usernotifications::Error),

    #[cfg(target_os = "macos")]
    #[error("failed to start notification response thread")]
    ResponseThread(#[source] std::io::Error),
}

#[cfg(target_os = "macos")]
#[derive(Debug, Default)]
pub struct MacNotifier {
    authorization: AuthorizationState,
}

#[cfg(target_os = "macos")]
impl MacNotifier {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(target_os = "macos")]
impl Notifier for MacNotifier {
    fn authorize(&mut self) -> Result<(), NotifyError> {
        if !self.authorization.needs_request() {
            return Ok(());
        }

        mac_usernotifications::check_bundle()?;
        let granted = mac_usernotifications::blocking::request_auth()?;
        if let AuthorizationResolution::Disabled {
            report_denial: true,
        } = self.authorization.resolve(granted)
        {
            eprintln!(
                "ipchecker notification permission denied; allow it in System Settings -> Notifications"
            );
        }

        Ok(())
    }

    fn send(
        &mut self,
        decision: NotificationDecision,
        actions: Box<dyn ActionSink>,
    ) -> Result<(), NotifyError> {
        use std::thread;

        use mac_usernotifications::{Action, Notification};

        if !self.authorization.delivery_enabled() {
            return Ok(());
        }

        let content = content_for(decision);

        if content.actions.is_empty() {
            Notification::new()
                .title(&content.title)
                .message(&content.body)
                .send_blocking()?;
            return Ok(());
        }

        thread::Builder::new()
            .name("ipchecker-notification-response".to_owned())
            .spawn(move || {
                let response_timeout = content
                    .response_timeout
                    .expect("actionable notification must have a response timeout");
                let response = mac_usernotifications::block_on(async move {
                    let notification = content.actions.iter().fold(
                        Notification::new()
                            .title(&content.title)
                            .message(&content.body),
                        |notification, action| {
                            notification.action(Action::button(action.identifier, &action.label))
                        },
                    );

                    notification
                        .timeout(response_timeout)
                        .send()
                        .await?
                        .response()
                        .await
                });

                match response {
                    Ok(response) => {
                        let response = if response.is_default_action() {
                            NotificationResponseKind::Default
                        } else if response.is_dismiss_action() {
                            NotificationResponseKind::Dismissed
                        } else if response.is_timed_out() {
                            NotificationResponseKind::TimedOut
                        } else {
                            NotificationResponseKind::Action(&response.action_identifier)
                        };

                        if let Some(action) = state_change_for_response(response) {
                            actions.send(action);
                        }
                    }
                    Err(error) => {
                        eprintln!("ipchecker notification response failed: {error}");
                    }
                }
            })
            .map_err(NotifyError::ResponseThread)?;

        Ok(())
    }
}

#[cfg(not(target_os = "macos"))]
#[derive(Debug, Default)]
pub struct UnsupportedNotifier;

#[cfg(not(target_os = "macos"))]
impl Notifier for UnsupportedNotifier {
    fn authorize(&mut self) -> Result<(), NotifyError> {
        Err(NotifyError::UnsupportedPlatform)
    }

    fn send(
        &mut self,
        _decision: NotificationDecision,
        _actions: Box<dyn ActionSink>,
    ) -> Result<(), NotifyError> {
        Err(NotifyError::UnsupportedPlatform)
    }
}
