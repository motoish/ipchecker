use std::time::{Duration, Instant};

use ipchecker::net_speed::{
    InterfaceCounters, InterfaceSnapshot, NetworkRates, NetworkSpeedLabels, NetworkSpeedSampler,
    format_bytes_per_second, rates_from_snapshots, should_count_interface,
};

fn rate(number: &str, unit: &str) -> String {
    format!("{number}\t{unit}")
}

fn counters(received: u64, sent: u64) -> InterfaceCounters {
    InterfaceCounters { received, sent }
}

fn snapshot<const N: usize>(entries: [(&str, u64, u64); N]) -> InterfaceSnapshot {
    entries
        .into_iter()
        .map(|(name, received, sent)| (name.to_owned(), counters(received, sent)))
        .collect()
}

#[test]
fn formats_from_kb_per_second_with_fixed_width() {
    assert_eq!(format_bytes_per_second(0.0), rate("0", "KB/s"));
    assert_eq!(format_bytes_per_second(511.0), rate("0", "KB/s"));
    assert_eq!(format_bytes_per_second(512.0), rate("1", "KB/s"));
    assert_eq!(format_bytes_per_second(1024.0), rate("1", "KB/s"));
    assert_eq!(format_bytes_per_second(1536.0), rate("2", "KB/s"));
    assert_eq!(format_bytes_per_second(10_240.0), rate("10", "KB/s"));
    assert_eq!(
        format_bytes_per_second(1023.0 * 1024.0),
        rate("1023", "KB/s")
    );
    assert_eq!(
        format_bytes_per_second(1024.0 * 1024.0),
        rate("1.0", "MB/s")
    );
    assert_eq!(
        format_bytes_per_second(1.5 * 1024.0 * 1024.0),
        rate("1.5", "MB/s")
    );
    assert_eq!(
        format_bytes_per_second(12.34 * 1024.0 * 1024.0),
        rate("12.3", "MB/s")
    );
    assert_eq!(
        format_bytes_per_second(1024.0 * 1024.0 * 1024.0),
        rate("1.0", "GB/s")
    );
    assert_eq!(format_bytes_per_second(-8.0), rate("0", "KB/s"));
}

#[test]
fn tray_titles_pin_units_with_tabs() {
    let labels = NetworkSpeedLabels::from_rates(NetworkRates {
        download_bps: 1024.0,
        upload_bps: 1023.0 * 1024.0,
    });
    let title = labels.tray_title();
    let lines: Vec<&str> = title.lines().collect();

    assert_eq!(lines, ["↑\t1023\tKB/s", "↓\t1\tKB/s"]);
    assert_eq!(
        NetworkSpeedLabels::unknown().tray_title(),
        format!("↑\t{}\n↓\t{}", rate("—", "KB/s"), rate("—", "KB/s"))
    );
}

#[test]
fn counts_only_numbered_en_interfaces() {
    assert!(should_count_interface("en0"));
    assert!(should_count_interface("en1"));
    assert!(should_count_interface("en10"));
    assert!(should_count_interface("en0:1"));
    assert!(!should_count_interface("en"));
    assert!(!should_count_interface("energy"));
    assert!(!should_count_interface("lo0"));
    assert!(!should_count_interface("awdl0"));
    assert!(!should_count_interface("utun0"));
    assert!(!should_count_interface("bridge0"));
    assert!(!should_count_interface("ap1"));
}

#[test]
fn first_sample_stays_unknown_until_a_delta_exists() {
    let mut sampler = NetworkSpeedSampler::default();
    let now = Instant::now();

    let first = sampler
        .observe(now, snapshot([("en0", 1_000, 200)]))
        .clone();

    assert_eq!(first, NetworkSpeedLabels::unknown());
    assert_eq!(
        first.tray_title(),
        format!("↑\t{}\n↓\t{}", rate("—", "KB/s"), rate("—", "KB/s"))
    );
}

#[test]
fn second_sample_reports_bytes_per_second() {
    let mut sampler = NetworkSpeedSampler::default();
    let start = Instant::now();
    sampler.observe(start, snapshot([("en0", 1_000, 200)]));

    let labels = sampler
        .observe(
            start + Duration::from_secs(1),
            snapshot([("en0", 2_500, 700)]),
        )
        .clone();

    assert_eq!(labels.download, rate("1", "KB/s"));
    assert_eq!(labels.upload, rate("0", "KB/s"));
    assert_eq!(
        labels.tray_title(),
        format!("↑\t{}\n↓\t{}", rate("0", "KB/s"), rate("1", "KB/s"))
    );
}

#[test]
fn counter_wrap_reports_zero_rate_instead_of_a_spike() {
    let rates = rates_from_snapshots(
        &snapshot([("en0", 8_000, 4_000)]),
        &snapshot([("en0", 100, 50)]),
        Duration::from_secs(1),
    )
    .expect("elapsed time is positive");

    assert_eq!(rates.download_bps, 0.0);
    assert_eq!(rates.upload_bps, 0.0);
}

#[test]
fn failed_read_keeps_the_last_rendered_frame() {
    let mut sampler = NetworkSpeedSampler::default();
    let start = Instant::now();
    sampler.observe(start, snapshot([("en0", 0, 0)]));
    sampler.observe(start + Duration::from_secs(1), snapshot([("en0", 1000, 0)]));

    let kept = sampler.observe_failure().clone();

    assert_eq!(kept.download, rate("1", "KB/s"));
    assert_eq!(kept.upload, rate("0", "KB/s"));
}

#[test]
fn displayed_rate_averages_the_last_three_seconds() {
    let mut sampler = NetworkSpeedSampler::default();
    let start = Instant::now();
    sampler.observe(start, snapshot([("en0", 0, 0)]));
    sampler.observe(
        start + Duration::from_secs(1),
        snapshot([("en0", 3 * 1024, 0)]),
    );
    sampler.observe(
        start + Duration::from_secs(2),
        snapshot([("en0", 3 * 1024, 0)]),
    );
    let labels = sampler
        .observe(
            start + Duration::from_secs(3),
            snapshot([("en0", 3 * 1024, 0)]),
        )
        .clone();

    assert_eq!(labels.download, rate("1", "KB/s"));
    assert_eq!(labels.upload, rate("0", "KB/s"));
}

#[test]
fn newly_active_interfaces_start_with_a_fresh_baseline() {
    let rates = rates_from_snapshots(
        &snapshot([("en0", 1_000, 200)]),
        &snapshot([("en0", 2_024, 712), ("en1", 8_000_000, 4_000_000)]),
        Duration::from_secs(1),
    )
    .expect("en0 exists in both snapshots");

    assert_eq!(rates.download_bps, 1_024.0);
    assert_eq!(rates.upload_bps, 512.0);
}

#[test]
fn inactive_interfaces_are_removed_without_resetting_common_interfaces() {
    let rates = rates_from_snapshots(
        &snapshot([("en0", 1_000, 200), ("en1", 8_000_000, 4_000_000)]),
        &snapshot([("en0", 2_024, 712)]),
        Duration::from_secs(1),
    )
    .expect("en0 exists in both snapshots");

    assert_eq!(rates.download_bps, 1_024.0);
    assert_eq!(rates.upload_bps, 512.0);
}

#[test]
fn a_complete_interface_switch_waits_for_a_new_delta() {
    let mut sampler = NetworkSpeedSampler::default();
    let start = Instant::now();
    sampler.observe(start, snapshot([("en0", 0, 0)]));
    sampler.observe(
        start + Duration::from_secs(1),
        snapshot([("en0", 2 * 1024, 1024)]),
    );

    let labels = sampler
        .observe(
            start + Duration::from_secs(2),
            snapshot([("en1", 8_000_000, 4_000_000)]),
        )
        .clone();

    assert_eq!(labels, NetworkSpeedLabels::unknown());
}

#[cfg(target_os = "macos")]
#[test]
fn macos_interface_counters_can_be_read() {
    ipchecker::net_speed::read_interface_snapshot()
        .expect("NET_RT_IFLIST2 should be readable on macOS");
}
