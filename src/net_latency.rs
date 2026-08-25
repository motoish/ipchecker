use std::{
    collections::VecDeque,
    net::{SocketAddr, TcpStream},
    time::{Duration, Instant},
};

pub const LATENCY_HOST: &str = "1.1.1.1";
pub const LATENCY_PORT: u16 = 443;
pub const LATENCY_TIMEOUT: Duration = Duration::from_secs(2);
/// Low latency is strictly below this threshold (ms).
pub const LOW_LATENCY_MS: u64 = 100;
/// Medium latency is below this threshold; at or above is high (ms).
pub const MEDIUM_LATENCY_MS: u64 = 300;
const LATENCY_AVERAGE_SAMPLES: usize = 3;
const HIGH_LATENCY_TEXT: &str = "?";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LatencyLevel {
    Low,
    Medium,
    #[default]
    High,
}

impl LatencyLevel {
    pub fn from_millis(millis: u64) -> Self {
        if millis < LOW_LATENCY_MS {
            Self::Low
        } else if millis < MEDIUM_LATENCY_MS {
            Self::Medium
        } else {
            Self::High
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatencyDisplay {
    pub level: LatencyLevel,
    pub text: String,
}

impl LatencyDisplay {
    pub fn unknown() -> Self {
        Self {
            level: LatencyLevel::High,
            text: HIGH_LATENCY_TEXT.to_owned(),
        }
    }

    pub fn from_millis(millis: u64) -> Self {
        let level = LatencyLevel::from_millis(millis);
        let text = match level {
            LatencyLevel::High => HIGH_LATENCY_TEXT.to_owned(),
            LatencyLevel::Low | LatencyLevel::Medium => format!("{millis} ms"),
        };
        Self { level, text }
    }
}

impl Default for LatencyDisplay {
    fn default() -> Self {
        Self::unknown()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NetworkLatencySampler {
    recent: VecDeque<u64>,
    display: LatencyDisplay,
}

impl NetworkLatencySampler {
    pub fn observe(&mut self, sample: Option<u64>) -> &LatencyDisplay {
        if let Some(millis) = sample {
            self.recent.push_back(millis);
            while self.recent.len() > LATENCY_AVERAGE_SAMPLES {
                self.recent.pop_front();
            }
            let average = self.recent.iter().sum::<u64>() / self.recent.len() as u64;
            self.display = LatencyDisplay::from_millis(average);
        } else {
            self.recent.clear();
            self.display = LatencyDisplay::unknown();
        }
        &self.display
    }

    pub fn display(&self) -> &LatencyDisplay {
        &self.display
    }
}

pub fn format_latency(millis: u64) -> String {
    LatencyDisplay::from_millis(millis).text
}

pub fn format_unknown_latency() -> String {
    LatencyDisplay::unknown().text
}

pub fn measure_tcp_latency() -> Option<u64> {
    let address: SocketAddr = format!("{LATENCY_HOST}:{LATENCY_PORT}").parse().ok()?;
    let started = Instant::now();
    TcpStream::connect_timeout(&address, LATENCY_TIMEOUT).ok()?;
    Some(started.elapsed().as_millis() as u64)
}
