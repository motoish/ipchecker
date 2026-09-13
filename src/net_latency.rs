use std::{
    collections::VecDeque,
    net::{SocketAddr, TcpStream},
    time::{Duration, Instant},
};

pub const LATENCY_HOST: &str = "1.1.1.1";
pub const LATENCY_PORT: u16 = 443;
pub const LATENCY_SAMPLE_INTERVAL: Duration = Duration::from_secs(5);
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
    measure_tcp_latency_to(address)
}

pub fn measure_tcp_latency_to(address: SocketAddr) -> Option<u64> {
    let started = Instant::now();
    TcpStream::connect_timeout(&address, LATENCY_TIMEOUT).ok()?;
    Some(started.elapsed().as_millis() as u64)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LatencyMode {
    #[default]
    Icmp,
    Tcp,
}

impl LatencyMode {
    pub fn menu_label(self) -> String {
        match self {
            Self::Icmp => format!("ICMP · {LATENCY_HOST}"),
            Self::Tcp => format!("TCP · {LATENCY_HOST}:{LATENCY_PORT}"),
        }
    }
}

fn parse_ping_reply(output: &str) -> Option<u64> {
    output.lines().find_map(|line| {
        if !line.contains("bytes from ") {
            return None;
        }
        let value = line.split_once("time=")?.1.split_whitespace().next()?;
        let millis: f64 = value.parse().ok()?;
        (millis.is_finite() && millis >= 0.0).then_some(millis as u64)
    })
}

#[cfg(test)]
mod icmp_tests {
    use super::*;
    #[test]
    fn parses_only_successful_echo_reply_times() {
        assert_eq!(
            parse_ping_reply("64 bytes from 1.1.1.1: icmp_seq=0 ttl=58 time=12.937 ms"),
            Some(12)
        );
        assert_eq!(
            parse_ping_reply("64 bytes from 1.1.1.1: time=0.123 ms"),
            Some(0)
        );
        for output in [
            "Request timeout for icmp_seq 0",
            "100.0% packet loss",
            "64 bytes from 1.1.1.1: time=NaN ms",
        ] {
            assert_eq!(parse_ping_reply(output), None);
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod continuous_ping_tests {
    use super::*;
    use std::process::{Command, Stdio};
    #[test]
    fn silent_child_times_out_and_is_reaped_on_drop() {
        let child = Command::new("/bin/cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        let mut ping = ContinuousPing::from_child(child).unwrap();
        assert_eq!(
            ping.next_sample().unwrap_err().kind(),
            std::io::ErrorKind::TimedOut
        );
        drop(ping);
        // SAFETY: signal 0 checks existence without signalling any process.
        assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    }

    #[test]
    fn cancelling_a_pending_probe_does_not_wait_for_the_next_packet() {
        let child = Command::new("/bin/cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut ping = ContinuousPing::from_child(child).unwrap();
        let started = Instant::now();
        assert_eq!(
            ping.next_sample_while(|| false).unwrap_err().kind(),
            std::io::ErrorKind::Interrupted
        );
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn continuous_probe_tolerates_the_gap_between_five_second_samples() {
        let child = Command::new("/bin/sh")
            .args(["-c", "printf '64 bytes from 1.1.1.1: time=16 ms\n'; sleep 3; printf '64 bytes from 1.1.1.1: time=17 ms\n'"])
            .stdout(Stdio::piped()).spawn().unwrap();
        let mut ping = ContinuousPing::from_child(child).unwrap();
        assert_eq!(ping.next_sample().unwrap(), Some(16));
        assert_eq!(ping.next_sample().unwrap(), Some(17));
    }

    #[test]
    fn continuous_stream_preserves_samples_and_timeouts() {
        let child = Command::new("/usr/bin/printf")
            .arg("PING 1.1.1.1\n64 bytes from 1.1.1.1: time=16.5 ms\nRequest timeout for icmp_seq 1\n64 bytes from 1.1.1.1: time=18.2 ms\n")
            .stdout(Stdio::piped()).spawn().unwrap();
        let mut ping = ContinuousPing::from_child(child).unwrap();
        assert_eq!(ping.next_sample().unwrap(), Some(16));
        assert_eq!(ping.next_sample().unwrap(), None);
        assert_eq!(ping.next_sample().unwrap(), Some(18));
        assert!(ping.next_sample().is_err());
    }
}

/// One continuously paced ping process per ICMP sampling session. This avoids
/// repeatedly restarting the probe and adding the last RTT to every interval.
pub(crate) struct ContinuousPing {
    #[cfg(target_os = "macos")]
    child: std::process::Child,
    #[cfg(target_os = "macos")]
    stdout: std::process::ChildStdout,
    #[cfg(target_os = "macos")]
    buffer: Vec<u8>,
}

impl ContinuousPing {
    pub(crate) fn start() -> std::io::Result<Self> {
        #[cfg(target_os = "macos")]
        {
            use std::process::{Command, Stdio};
            let child = Command::new("/sbin/ping")
                .args([
                    "-n",
                    "-i",
                    &LATENCY_SAMPLE_INTERVAL.as_secs().to_string(),
                    "-W",
                    "1000",
                    LATENCY_HOST,
                ])
                .env("LC_ALL", "C")
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()?;
            Self::from_child(child)
        }
        #[cfg(not(target_os = "macos"))]
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "ICMP requires macOS",
        ))
    }

    #[cfg(target_os = "macos")]
    fn from_child(mut child: std::process::Child) -> std::io::Result<Self> {
        use std::{io, os::fd::AsRawFd};
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::other("ping stdout is unavailable"));
        };
        let probe = Self {
            child,
            stdout,
            buffer: Vec::new(),
        };
        let fd = probe.stdout.as_raw_fd();
        // SAFETY: fd is an owned, live pipe descriptor. Preserve existing flags.
        unsafe {
            let flags = libc::fcntl(fd, libc::F_GETFL);
            if flags < 0 || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(probe)
    }

    #[cfg(test)]
    fn next_sample(&mut self) -> std::io::Result<Option<u64>> {
        self.next_sample_while(|| true)
    }

    pub(crate) fn next_sample_while(
        &mut self,
        keep_waiting: impl Fn() -> bool,
    ) -> std::io::Result<Option<u64>> {
        #[cfg(target_os = "macos")]
        {
            use std::{
                io::{self, Read},
                os::fd::AsRawFd,
            };
            let deadline = Instant::now() + LATENCY_SAMPLE_INTERVAL + LATENCY_TIMEOUT;
            loop {
                if !keep_waiting() {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "latency sampling changed",
                    ));
                }
                while let Some(end) = self.buffer.iter().position(|byte| *byte == b'\n') {
                    let line: Vec<u8> = self.buffer.drain(..=end).collect();
                    let line = String::from_utf8_lossy(&line);
                    if let Some(millis) = parse_ping_reply(&line) {
                        return Ok(Some(millis));
                    }
                    if line.starts_with("Request timeout for icmp_seq") {
                        return Ok(None);
                    }
                }
                if self.buffer.len() > 8192 {
                    return Err(io::Error::other("unexpected ping output"));
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "ping output timed out",
                    ));
                }
                let mut descriptor = libc::pollfd {
                    fd: self.stdout.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                };
                // SAFETY: descriptor points to one initialized pollfd with a live pipe fd.
                let ready = unsafe {
                    libc::poll(
                        &mut descriptor,
                        1,
                        remaining.as_millis().clamp(1, 100) as i32,
                    )
                };
                if ready < 0 {
                    let error = io::Error::last_os_error();
                    if error.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    return Err(error);
                }
                if ready == 0 {
                    continue;
                }
                let mut bytes = [0u8; 1024];
                match self.stdout.read(&mut bytes) {
                    Ok(0) => {
                        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "ping exited"));
                    }
                    Ok(count) => self.buffer.extend_from_slice(&bytes[..count]),
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) =>
                    {
                        continue;
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = keep_waiting;
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "ICMP requires macOS",
            ))
        }
    }
}

impl Drop for ContinuousPing {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
