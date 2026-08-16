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
            return Self {
                state: MonitorState::Unknown,
                current_ip: None,
                last_success_ip: self.last_success_ip,
                notification: None,
            };
        };

        Self {
            state: state_for(current_ip, expected),
            current_ip: self.current_ip,
            last_success_ip: self.last_success_ip,
            notification: None,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Monitor {
    last_success_ip: Option<Ipv4Addr>,
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
                let state = state_for(current_ip, expected);
                let notification = if muted {
                    None
                } else {
                    expected
                        .filter(|expected_ip| *expected_ip != current_ip)
                        .map(|expected| NotificationDecision::Mismatch {
                            current: current_ip,
                            expected,
                        })
                };

                MonitorOutcome {
                    state,
                    current_ip: Some(current_ip),
                    last_success_ip: self.last_success_ip,
                    notification,
                }
            }
            Err(FetchError::AllSourcesFailed(_)) => {
                let notification = (!muted).then_some(NotificationDecision::FetchFailure);

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

fn state_for(current: Ipv4Addr, expected: Option<Ipv4Addr>) -> MonitorState {
    match expected {
        None => MonitorState::Unconfigured,
        Some(expected) if current == expected => MonitorState::Matched,
        Some(_) => MonitorState::Mismatched,
    }
}
