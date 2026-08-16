use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use std::{io, mem, ptr};

pub const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);
pub const TRAY_TITLE_WIDTH_TEMPLATE: &str = "↑\t1023.9\tMB/s\n↓\t1023.9\tMB/s";

const UNKNOWN_NUMBER: &str = "—";
const KIB: f64 = 1024.0;
const RATE_AVERAGE_SAMPLES: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InterfaceCounters {
    pub received: u64,
    pub sent: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InterfaceSnapshot(BTreeMap<String, InterfaceCounters>);

impl FromIterator<(String, InterfaceCounters)> for InterfaceSnapshot {
    fn from_iter<T: IntoIterator<Item = (String, InterfaceCounters)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NetworkRates {
    pub download_bps: f64,
    pub upload_bps: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkSpeedLabels {
    pub download: String,
    pub upload: String,
}

impl NetworkSpeedLabels {
    pub fn unknown() -> Self {
        Self {
            download: format_with_unit(UNKNOWN_NUMBER, "KB/s"),
            upload: format_with_unit(UNKNOWN_NUMBER, "KB/s"),
        }
    }

    pub fn from_rates(rates: NetworkRates) -> Self {
        Self {
            download: format_bytes_per_second(rates.download_bps),
            upload: format_bytes_per_second(rates.upload_bps),
        }
    }

    pub fn tray_title(&self) -> String {
        format!("↑\t{}\n↓\t{}", self.upload, self.download)
    }
}

#[derive(Debug, Default)]
pub struct NetworkSpeedSampler {
    previous: Option<(Instant, InterfaceSnapshot)>,
    recent_rates: VecDeque<NetworkRates>,
    labels: NetworkSpeedLabels,
}

impl Default for NetworkSpeedLabels {
    fn default() -> Self {
        Self::unknown()
    }
}

impl NetworkSpeedSampler {
    pub fn observe(&mut self, now: Instant, snapshot: InterfaceSnapshot) -> &NetworkSpeedLabels {
        if let Some((previous_at, previous)) = &self.previous {
            let elapsed = now.saturating_duration_since(*previous_at);
            if let Some(rates) = rates_from_snapshots(previous, &snapshot, elapsed) {
                self.recent_rates.push_back(rates);
                while self.recent_rates.len() > RATE_AVERAGE_SAMPLES {
                    self.recent_rates.pop_front();
                }
                self.labels = NetworkSpeedLabels::from_rates(average_rates(&self.recent_rates));
            } else {
                self.recent_rates.clear();
                self.labels = NetworkSpeedLabels::unknown();
            }
        }
        self.previous = Some((now, snapshot));
        &self.labels
    }

    pub fn observe_failure(&mut self) -> &NetworkSpeedLabels {
        &self.labels
    }
}

pub fn should_count_interface(name: &str) -> bool {
    let Some(primary) = name.split(':').next() else {
        return false;
    };
    let Some(suffix) = primary.strip_prefix("en") else {
        return false;
    };
    !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
}

pub fn rates_from_snapshots(
    previous: &InterfaceSnapshot,
    current: &InterfaceSnapshot,
    elapsed: Duration,
) -> Option<NetworkRates> {
    let seconds = elapsed.as_secs_f64();
    if seconds <= 0.0 {
        return None;
    }

    let mut common_interface_found = false;
    let mut received_delta = 0u64;
    let mut sent_delta = 0u64;
    for (name, current_counters) in &current.0 {
        let Some(previous_counters) = previous.0.get(name) else {
            continue;
        };
        common_interface_found = true;
        received_delta = received_delta.saturating_add(
            current_counters
                .received
                .saturating_sub(previous_counters.received),
        );
        sent_delta =
            sent_delta.saturating_add(current_counters.sent.saturating_sub(previous_counters.sent));
    }

    common_interface_found.then_some(NetworkRates {
        download_bps: received_delta as f64 / seconds,
        upload_bps: sent_delta as f64 / seconds,
    })
}

fn average_rates(rates: &VecDeque<NetworkRates>) -> NetworkRates {
    let count = rates.len() as f64;
    NetworkRates {
        download_bps: rates.iter().map(|rate| rate.download_bps).sum::<f64>() / count,
        upload_bps: rates.iter().map(|rate| rate.upload_bps).sum::<f64>() / count,
    }
}

pub fn format_bytes_per_second(bytes_per_second: f64) -> String {
    let bytes = bytes_per_second.max(0.0);
    let kib = bytes / KIB;
    if kib.round() < KIB {
        return format_with_unit(&(kib.round() as u64).to_string(), "KB/s");
    }

    let mib = bytes / (KIB * KIB);
    let mib_display = (mib * 10.0).round() / 10.0;
    if mib_display < KIB {
        return format_with_unit(&format!("{mib_display:.1}"), "MB/s");
    }

    let gib_display = ((mib / KIB) * 10.0).round() / 10.0;
    format_with_unit(&format!("{gib_display:.1}"), "GB/s")
}

fn format_with_unit(number: &str, unit: &str) -> String {
    format!("{number}\t{unit}")
}

#[cfg(target_os = "macos")]
pub fn read_interface_snapshot() -> io::Result<InterfaceSnapshot> {
    read_interface_snapshot_macos()
}

#[cfg(not(target_os = "macos"))]
pub fn read_interface_snapshot() -> Result<InterfaceSnapshot, String> {
    Err("network counters are only available on macOS".to_owned())
}

#[cfg(target_os = "macos")]
fn read_interface_snapshot_macos() -> io::Result<InterfaceSnapshot> {
    let mut name = [libc::CTL_NET, libc::PF_ROUTE, 0, 0, libc::NET_RT_IFLIST2, 0];
    let mut buffer = Vec::new();
    for _ in 0..3 {
        let mut length = 0usize;
        let size_result = unsafe {
            libc::sysctl(
                name.as_mut_ptr(),
                name.len() as u32,
                ptr::null_mut(),
                &mut length,
                ptr::null_mut(),
                0,
            )
        };
        if size_result != 0 {
            return Err(io::Error::last_os_error());
        }
        if length == 0 {
            return Ok(InterfaceSnapshot::default());
        }

        buffer.resize(length, 0);
        let read_result = unsafe {
            libc::sysctl(
                name.as_mut_ptr(),
                name.len() as u32,
                buffer.as_mut_ptr().cast(),
                &mut length,
                ptr::null_mut(),
                0,
            )
        };
        if read_result == 0 {
            buffer.truncate(length);
            return Ok(parse_interface_snapshot(&buffer));
        }

        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ENOMEM) {
            return Err(error);
        }
    }
    Err(io::Error::last_os_error())
}

#[cfg(target_os = "macos")]
fn parse_interface_snapshot(buffer: &[u8]) -> InterfaceSnapshot {
    let mut counters = BTreeMap::new();
    let mut offset = 0usize;
    while offset + mem::size_of::<libc::if_msghdr2>() <= buffer.len() {
        let header =
            unsafe { ptr::read_unaligned(buffer.as_ptr().add(offset).cast::<libc::if_msghdr2>()) };
        let message_len = header.ifm_msglen as usize;
        if message_len == 0 || offset + message_len > buffer.len() {
            break;
        }
        if i32::from(header.ifm_type) == libc::RTM_IFINFO2 {
            let name_offset = offset + mem::size_of::<libc::if_msghdr2>();
            if name_offset < offset + message_len
                && let Some(name) = sockaddr_dl_name(&buffer[name_offset..offset + message_len])
                && should_count_interface(&name)
                && interface_is_up(&header)
            {
                let received =
                    unsafe { ptr::addr_of!(header.ifm_data.ifi_ibytes).read_unaligned() };
                let sent = unsafe { ptr::addr_of!(header.ifm_data.ifi_obytes).read_unaligned() };
                counters.insert(name, InterfaceCounters { received, sent });
            }
        }
        offset += message_len;
    }
    InterfaceSnapshot(counters)
}

#[cfg(target_os = "macos")]
fn interface_is_up(header: &libc::if_msghdr2) -> bool {
    header.ifm_flags & libc::IFF_UP != 0 && header.ifm_flags & libc::IFF_RUNNING != 0
}

#[cfg(target_os = "macos")]
fn sockaddr_dl_name(bytes: &[u8]) -> Option<String> {
    if bytes.len() < mem::size_of::<libc::sockaddr_dl>() {
        return None;
    }
    let sockaddr = unsafe { ptr::read_unaligned(bytes.as_ptr().cast::<libc::sockaddr_dl>()) };
    if i32::from(sockaddr.sdl_family) != libc::AF_LINK {
        return None;
    }
    let name_len = sockaddr.sdl_nlen as usize;
    let data_offset = mem::offset_of!(libc::sockaddr_dl, sdl_data);
    if name_len == 0 || data_offset.checked_add(name_len)? > bytes.len() {
        return None;
    }
    let name = String::from_utf8_lossy(&bytes[data_offset..data_offset + name_len])
        .trim_end_matches('\0')
        .to_owned();
    if name.is_empty() { None } else { Some(name) }
}
