use crate::{
    config::Config,
    monitor::{MonitorOutcome, MonitorState},
    session::Session,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconState {
    Normal,
    Alert,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiModel {
    pub current_title: String,
    pub expected_title: String,
    pub can_use_current_ip: bool,
    pub can_copy_current_ip: bool,
    pub interval_minutes: u64,
    pub muted: bool,
    pub is_show_network_speed: bool,
    pub is_show_network_latency: bool,
    pub is_show_status_icon: bool,
    pub can_toggle_show_network_speed: bool,
    pub can_toggle_show_network_latency: bool,
    pub can_toggle_show_status_icon: bool,
    pub icon_state: IconState,
    pub tooltip: String,
}

impl UiModel {
    pub fn from_state(config: &Config, session: &Session, outcome: &MonitorOutcome) -> Self {
        let display_ip = outcome.current_ip.or(outcome.last_success_ip);
        let expected_value = config
            .expected_ip
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| t!("status.unset").to_string());
        let (current_title, tooltip) = match (outcome.current_ip, outcome.last_success_ip) {
            (Some(current), _) => (
                t!("status.current_ip", ip = current).to_string(),
                t!(
                    "status.tooltip",
                    current = current,
                    expected = expected_value
                )
                .to_string(),
            ),
            (None, Some(last_success)) => (
                t!("status.last_success_ip", ip = last_success).to_string(),
                t!(
                    "status.tooltip_last",
                    current = last_success,
                    expected = expected_value
                )
                .to_string(),
            ),
            (None, None) => (
                t!("status.current_ip", ip = t!("status.unknown")).to_string(),
                t!("status.tooltip", current = "—", expected = expected_value).to_string(),
            ),
        };
        let icon_state = match outcome.state {
            MonitorState::Matched | MonitorState::Unconfigured => IconState::Normal,
            MonitorState::Mismatched => IconState::Alert,
            MonitorState::Unknown => IconState::Unknown,
        };

        Self {
            current_title,
            expected_title: t!("status.expected_ip", ip = expected_value).to_string(),
            can_use_current_ip: outcome.current_ip.is_some(),
            can_copy_current_ip: display_ip.is_some(),
            interval_minutes: config.interval_minutes,
            muted: session.is_muted(),
            is_show_network_speed: config.is_show_network_speed,
            is_show_network_latency: config.is_show_network_latency,
            is_show_status_icon: config.is_show_status_icon,
            can_toggle_show_network_speed: !config.is_show_network_speed
                || config.is_show_network_latency
                || config.is_show_status_icon,
            can_toggle_show_network_latency: !config.is_show_network_latency
                || config.is_show_network_speed
                || config.is_show_status_icon,
            can_toggle_show_status_icon: !config.is_show_status_icon
                || config.is_show_network_speed
                || config.is_show_network_latency,
            icon_state,
            tooltip,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    CopyCurrentIp,
    SetExpectedFromInput,
    UseCurrentIp,
    SetInterval(u64),
    CheckNow,
    ToggleMuted,
    ToggleShowNetworkSpeed,
    ToggleShowNetworkLatency,
    ToggleShowStatusIcon,
    CheckForUpdates,
    About,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiCommand {
    CopyCurrentIp,
    SetExpectedFromInput,
    UseCurrentIp,
    SetInterval(u64),
    CheckNow,
    SetMuted(bool),
    SetShowNetworkSpeed(bool),
    SetShowNetworkLatency(bool),
    SetShowStatusIcon(bool),
    CheckForUpdates,
    About,
    Quit,
}

impl UiCommand {
    pub fn from_menu_action(
        action: MenuAction,
        is_muted: bool,
        is_show_network_speed: bool,
        is_show_network_latency: bool,
        is_show_status_icon: bool,
    ) -> Self {
        match action {
            MenuAction::CopyCurrentIp => Self::CopyCurrentIp,
            MenuAction::SetExpectedFromInput => Self::SetExpectedFromInput,
            MenuAction::UseCurrentIp => Self::UseCurrentIp,
            MenuAction::SetInterval(minutes) => Self::SetInterval(minutes),
            MenuAction::CheckNow => Self::CheckNow,
            MenuAction::ToggleMuted => Self::SetMuted(!is_muted),
            MenuAction::ToggleShowNetworkSpeed => Self::SetShowNetworkSpeed(!is_show_network_speed),
            MenuAction::ToggleShowNetworkLatency => {
                Self::SetShowNetworkLatency(!is_show_network_latency)
            }
            MenuAction::ToggleShowStatusIcon => Self::SetShowStatusIcon(!is_show_status_icon),
            MenuAction::CheckForUpdates => Self::CheckForUpdates,
            MenuAction::About => Self::About,
            MenuAction::Quit => Self::Quit,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FeedbackRestoreGuard {
    generation: u64,
    active: Option<u64>,
}

impl FeedbackRestoreGuard {
    pub fn issue(&mut self) -> u64 {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("feedback restore generation exhausted");
        self.active = Some(self.generation);
        self.generation
    }

    pub fn claim(&mut self, token: u64) -> bool {
        if self.active == Some(token) {
            self.active = None;
            true
        } else {
            false
        }
    }

    pub fn cancel(&mut self) {
        self.active = None;
    }
}
