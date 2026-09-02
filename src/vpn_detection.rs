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
    let mut length = 0;
    if unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as libc::c_uint,
            ptr::null_mut(),
            &mut length,
            ptr::null_mut(),
            0,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }

    let mut bytes = vec![0_u8; length];
    if unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as libc::c_uint,
            bytes.as_mut_ptr().cast(),
            &mut length,
            ptr::null_mut(),
            0,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    bytes.truncate(length);
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
    use std::{mem, slice};

    use super::parse_route_interface_indices;

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
}
