#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceSnapshot {
    pub index: u32,
    pub name: String,
    pub is_up: bool,
}

impl InterfaceSnapshot {
    pub fn new(index: u32, name: impl Into<String>, is_up: bool) -> Self {
        Self {
            index,
            name: name.into(),
            is_up,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VpnStatus {
    Active,
    Inactive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DailyIpRecordDecision {
    Record,
    SkipVpn,
    SkipDetectionFailed,
}

pub fn decide_daily_ip_recording<E, F>(
    include_vpn_addresses: bool,
    detect_vpn: F,
) -> DailyIpRecordDecision
where
    F: FnOnce() -> Result<VpnStatus, E>,
{
    if include_vpn_addresses {
        DailyIpRecordDecision::Record
    } else {
        match detect_vpn() {
            Ok(VpnStatus::Active) => DailyIpRecordDecision::SkipVpn,
            Ok(VpnStatus::Inactive) => DailyIpRecordDecision::Record,
            Err(_) => DailyIpRecordDecision::SkipDetectionFailed,
        }
    }
}

pub fn classify_vpn(
    interfaces: &[InterfaceSnapshot],
    ipv4_route_interface_indices: &[u32],
) -> VpnStatus {
    if interfaces.iter().any(|interface| {
        interface.is_up
            && is_tunnel_interface_name(&interface.name)
            && ipv4_route_interface_indices.contains(&interface.index)
    }) {
        VpnStatus::Active
    } else {
        VpnStatus::Inactive
    }
}

fn is_tunnel_interface_name(name: &str) -> bool {
    ["utun", "ppp", "ipsec", "tun", "tap"]
        .iter()
        .filter_map(|prefix| name.strip_prefix(prefix))
        .any(|suffix| !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()))
}

#[cfg(target_os = "macos")]
pub fn detect_vpn_status() -> std::io::Result<VpnStatus> {
    let interfaces = active_interfaces()?;
    if !interfaces
        .iter()
        .any(|interface| is_tunnel_interface_name(&interface.name))
    {
        return Ok(VpnStatus::Inactive);
    }
    let route_indices = ipv4_route_interface_indices()?;
    Ok(classify_vpn(&interfaces, &route_indices))
}

#[cfg(not(target_os = "macos"))]
pub fn detect_vpn_status() -> std::io::Result<VpnStatus> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "VPN detection is supported only on macOS",
    ))
}

#[cfg(target_os = "macos")]
fn active_interfaces() -> std::io::Result<Vec<InterfaceSnapshot>> {
    use std::{collections::HashSet, ffi::CStr, io, ptr};

    struct InterfaceAddresses(*mut libc::ifaddrs);

    impl Drop for InterfaceAddresses {
        fn drop(&mut self) {
            unsafe { libc::freeifaddrs(self.0) };
        }
    }

    let mut first = ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut first) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let addresses = InterfaceAddresses(first);
    let mut interfaces = Vec::new();
    let mut seen = HashSet::new();
    let mut current = addresses.0;

    while let Some(address) = unsafe { current.as_ref() } {
        if !address.ifa_name.is_null() {
            let index = unsafe { libc::if_nametoindex(address.ifa_name) };
            if index != 0 && seen.insert(index) {
                let name = unsafe { CStr::from_ptr(address.ifa_name) }
                    .to_string_lossy()
                    .into_owned();
                interfaces.push(InterfaceSnapshot::new(
                    index,
                    name,
                    address.ifa_flags & libc::IFF_UP as u32 != 0,
                ));
            }
        }
        current = address.ifa_next;
    }

    Ok(interfaces)
}

#[cfg(target_os = "macos")]
fn read_sysctl_with_retry<F>(mut call: F) -> std::io::Result<Vec<u8>>
where
    F: FnMut(Option<&mut [u8]>) -> std::io::Result<usize>,
{
    const MAX_ATTEMPTS: usize = 3;

    let mut bytes = Vec::new();
    let mut last_error = None;
    for _ in 0..MAX_ATTEMPTS {
        let length = call(None)?;
        if length == 0 {
            return Ok(Vec::new());
        }
        bytes.resize(length, 0);
        match call(Some(&mut bytes)) {
            Ok(written) => {
                bytes.truncate(written);
                return Ok(bytes);
            }
            Err(error) if error.raw_os_error() == Some(libc::ENOMEM) => {
                last_error = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.expect("retry loop runs at least once"))
}

#[cfg(target_os = "macos")]
fn ipv4_route_interface_indices() -> std::io::Result<Vec<u32>> {
    use std::{io, ptr};

    const NET_RT_DUMP2: libc::c_int = 7;
    let mut mib = [
        libc::CTL_NET,
        libc::PF_ROUTE,
        0,
        libc::AF_INET,
        NET_RT_DUMP2,
        0,
    ];
    let bytes = read_sysctl_with_retry(|buffer| {
        let (data_ptr, mut length) = match buffer {
            Some(slice) => (slice.as_mut_ptr().cast(), slice.len()),
            None => (ptr::null_mut(), 0),
        };
        if unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                mib.len() as libc::c_uint,
                data_ptr,
                &mut length,
                ptr::null_mut(),
                0,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(length)
    })?;
    parse_route_interface_indices(&bytes)
}

#[cfg(target_os = "macos")]
fn parse_route_interface_indices(bytes: &[u8]) -> std::io::Result<Vec<u32>> {
    use std::{io, mem, ptr};

    let header_size = mem::size_of::<libc::rt_msghdr2>();
    let mut indices = Vec::new();
    let mut offset = 0;

    while offset < bytes.len() {
        let remaining = bytes.len() - offset;
        if remaining < header_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "truncated route message header",
            ));
        }
        let header =
            unsafe { ptr::read_unaligned(bytes.as_ptr().add(offset).cast::<libc::rt_msghdr2>()) };
        let message_length = usize::from(header.rtm_msglen);
        if message_length < header_size || message_length > remaining {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid route message length",
            ));
        }
        if header.rtm_version == libc::RTM_VERSION as u8
            && header.rtm_flags & libc::RTF_UP != 0
            && header.rtm_index != 0
        {
            indices.push(u32::from(header.rtm_index));
        }
        offset += message_length;
    }

    Ok(indices)
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use std::{io, mem, slice};

    use super::{parse_route_interface_indices, read_sysctl_with_retry};

    fn route_message(index: u16, flags: i32) -> Vec<u8> {
        let mut header: libc::rt_msghdr2 = unsafe { mem::zeroed() };
        header.rtm_msglen = mem::size_of::<libc::rt_msghdr2>() as u16;
        header.rtm_version = libc::RTM_VERSION as u8;
        header.rtm_index = index;
        header.rtm_flags = flags;
        unsafe {
            slice::from_raw_parts(
                (&raw const header).cast::<u8>(),
                mem::size_of::<libc::rt_msghdr2>(),
            )
            .to_vec()
        }
    }

    #[test]
    fn route_parser_returns_only_up_route_interface_indices() {
        let mut bytes = route_message(40, libc::RTF_UP);
        bytes.extend(route_message(41, 0));

        assert_eq!(parse_route_interface_indices(&bytes).unwrap(), vec![40]);
    }

    #[test]
    fn sysctl_retry_recovers_after_enomem_when_route_table_grows() {
        let payload = route_message(40, libc::RTF_UP);
        let mut read_attempts = 0;

        let bytes = read_sysctl_with_retry(|buffer| match buffer {
            None => Ok(if read_attempts == 0 {
                payload.len() / 2
            } else {
                payload.len() + 16
            }),
            Some(slice) => {
                read_attempts += 1;
                if read_attempts == 1 {
                    Err(io::Error::from_raw_os_error(libc::ENOMEM))
                } else {
                    slice[..payload.len()].copy_from_slice(&payload);
                    Ok(payload.len())
                }
            }
        })
        .unwrap();

        assert_eq!(read_attempts, 2);
        assert_eq!(bytes, payload);
    }

    #[test]
    fn sysctl_retry_fails_immediately_on_non_enomem_error() {
        let mut read_attempts = 0;

        let error = read_sysctl_with_retry(|buffer| match buffer {
            None => Ok(32),
            Some(_) => {
                read_attempts += 1;
                Err(io::Error::from_raw_os_error(libc::EPERM))
            }
        })
        .unwrap_err();

        assert_eq!(read_attempts, 1);
        assert_eq!(error.raw_os_error(), Some(libc::EPERM));
    }

    #[test]
    fn sysctl_retry_stops_after_exhausting_enomem_attempts() {
        let mut read_attempts = 0;

        let error = read_sysctl_with_retry(|buffer| match buffer {
            None => Ok(32),
            Some(_) => {
                read_attempts += 1;
                Err(io::Error::from_raw_os_error(libc::ENOMEM))
            }
        })
        .unwrap_err();

        assert_eq!(read_attempts, 3);
        assert_eq!(error.raw_os_error(), Some(libc::ENOMEM));
    }

    #[test]
    fn sysctl_retry_returns_empty_without_reading_when_length_is_zero() {
        let mut read_called = false;

        let bytes = read_sysctl_with_retry(|buffer| match buffer {
            None => Ok(0),
            Some(_) => {
                read_called = true;
                Ok(0)
            }
        })
        .unwrap();

        assert!(bytes.is_empty());
        assert!(!read_called);
    }
}
