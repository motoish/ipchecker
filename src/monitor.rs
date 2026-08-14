use std::net::Ipv4Addr;

use crate::ip_source::FetchError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitorState {
    Matched,
    Mismatched,
    Unknown,
    Unconfigured,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationDecision {
    Mismatch {
        current: Ipv4Addr,
        expected: Ipv4Addr,
    },
    FetchFailure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorOutcome {
    pub state: MonitorState,
    pub current_ip: Option<Ipv4Addr>,
    pub last_success_ip: Option<Ipv4Addr>,
    pub notification: Option<NotificationDecision>,
}

impl MonitorOutcome {
    pub fn recompare_expected(&self, expected: Option<Ipv4Addr>) -> Self {
        let Some(current_ip) = self.current_ip else {
            return self.clone();
        };
        let state = match expected {
            None => MonitorState::Unconfigured,
            Some(expected_ip) if current_ip == expected_ip => MonitorState::Matched,
            Some(_) => MonitorState::Mismatched,
        };

        Self {
            state,
            current_ip: self.current_ip,
            last_success_ip: self.last_success_ip,
            notification: None,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Monitor {
    last_success_ip: Option<Ipv4Addr>,
    failure_notified: bool,
}

impl Monitor {
    pub fn apply(
        &mut self,
        fetch: Result<Ipv4Addr, FetchError>,
        expected: Option<Ipv4Addr>,
        muted: bool,
    ) -> MonitorOutcome {
        match fetch {
            Ok(current_ip) => {
                self.last_success_ip = Some(current_ip);
                self.failure_notified = false;

                let (state, notification) = match expected {
                    None => (MonitorState::Unconfigured, None),
                    Some(expected) if current_ip == expected => (MonitorState::Matched, None),
                    Some(expected) => (
                        MonitorState::Mismatched,
                        (!muted).then_some(NotificationDecision::Mismatch {
                            current: current_ip,
                            expected,
                        }),
                    ),
                };

                MonitorOutcome {
                    state,
                    current_ip: Some(current_ip),
                    last_success_ip: self.last_success_ip,
                    notification,
                }
            }
            Err(FetchError::AllSourcesFailed(_)) => {
                let notification = if !self.failure_notified && !muted {
                    self.failure_notified = true;
                    Some(NotificationDecision::FetchFailure)
                } else {
                    None
                };

                MonitorOutcome {
                    state: MonitorState::Unknown,
                    current_ip: None,
                    last_success_ip: self.last_success_ip,
                    notification,
                }
            }
        }
    }
}
