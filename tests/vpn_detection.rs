use ipchecker::vpn_detection::{
    DailyIpRecordDecision, InterfaceSnapshot, VpnStatus, classify_vpn, decide_daily_ip_recording,
};

#[test]
fn idle_forticlient_utun_without_an_ipv4_route_is_not_active() {
    let interfaces = [InterfaceSnapshot::new(40, "utun40", true)];

    assert_eq!(classify_vpn(&interfaces, &[]), VpnStatus::Inactive);
}

#[test]
fn up_utun_with_any_ipv4_route_is_active() {
    let interfaces = [InterfaceSnapshot::new(40, "utun40", true)];

    assert_eq!(classify_vpn(&interfaces, &[40]), VpnStatus::Active);
}

#[test]
fn ordinary_interface_route_is_not_vpn_activity() {
    let interfaces = [InterfaceSnapshot::new(4, "en0", true)];

    assert_eq!(classify_vpn(&interfaces, &[4]), VpnStatus::Inactive);
}

#[test]
fn down_tunnel_interface_is_not_active_even_if_a_stale_route_remains() {
    let interfaces = [InterfaceSnapshot::new(40, "utun40", false)];

    assert_eq!(classify_vpn(&interfaces, &[40]), VpnStatus::Inactive);
}

#[test]
fn common_macos_tunnel_interface_families_are_recognized() {
    for name in ["utun2", "ppp0", "ipsec0", "tun0", "tap0"] {
        let interfaces = [InterfaceSnapshot::new(7, name, true)];
        assert_eq!(
            classify_vpn(&interfaces, &[7]),
            VpnStatus::Active,
            "{name} should be treated as a tunnel interface"
        );
    }
}

#[test]
fn tunnel_prefix_requires_a_numeric_interface_suffix() {
    for name in ["utun", "tunnel0", "tapbridge"] {
        let interfaces = [InterfaceSnapshot::new(7, name, true)];
        assert_eq!(
            classify_vpn(&interfaces, &[7]),
            VpnStatus::Inactive,
            "{name} should not be treated as a tunnel interface"
        );
    }
}

#[test]
fn including_vpn_addresses_records_without_running_detection() {
    let decision = decide_daily_ip_recording::<(), _>(true, || {
        panic!("VPN detection should be bypassed when VPN addresses are included")
    });

    assert_eq!(decision, DailyIpRecordDecision::Record);
}

#[test]
fn excluding_vpn_addresses_skips_an_active_vpn_observation() {
    let decision = decide_daily_ip_recording::<(), _>(false, || Ok(VpnStatus::Active));

    assert_eq!(decision, DailyIpRecordDecision::SkipVpn);
}

#[test]
fn excluding_vpn_addresses_records_when_no_vpn_is_active() {
    let decision = decide_daily_ip_recording::<(), _>(false, || Ok(VpnStatus::Inactive));

    assert_eq!(decision, DailyIpRecordDecision::Record);
}

#[test]
fn excluding_vpn_addresses_fails_closed_when_detection_fails() {
    let decision =
        decide_daily_ip_recording(false, || Err::<VpnStatus, _>("route table unavailable"));

    assert_eq!(decision, DailyIpRecordDecision::SkipDetectionFailed);
}
