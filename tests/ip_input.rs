use std::net::Ipv4Addr;

use ipchecker::ip_input::parse_expected_ip;
#[cfg(target_os = "macos")]
use ipchecker::ip_input::prompt_expected_ip;

#[cfg(target_os = "macos")]
#[test]
fn exposes_native_prompt_with_the_runtime_contract() {
    let _prompt: fn(Option<Ipv4Addr>) -> Option<Ipv4Addr> = prompt_expected_ip;
}

#[test]
fn parses_ipv4_after_trimming_whitespace() {
    assert_eq!(
        parse_expected_ip("  203.0.113.10\n").unwrap(),
        Ipv4Addr::new(203, 0, 113, 10)
    );
}

#[test]
fn rejects_empty_malformed_and_ipv6_input() {
    for value in ["", "   ", "203.0.113", "999.1.1.1", "2001:db8::1"] {
        assert!(
            parse_expected_ip(value).is_err(),
            "unexpectedly accepted {value:?}"
        );
    }
}
