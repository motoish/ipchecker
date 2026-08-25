use ipchecker::net_latency::{
    LatencyDisplay, LatencyLevel, NetworkLatencySampler, format_latency, format_unknown_latency,
    measure_tcp_latency,
};

#[test]
fn formats_latency_for_tray_leading_column() {
    assert_eq!(format_latency(42), "42 ms");
    assert_eq!(format_latency(150), "150 ms");
    assert_eq!(format_latency(300), "?");
    assert_eq!(format_unknown_latency(), "?");
}

#[test]
fn latency_level_thresholds_match_recommended_bands() {
    assert_eq!(LatencyLevel::from_millis(0), LatencyLevel::Low);
    assert_eq!(LatencyLevel::from_millis(99), LatencyLevel::Low);
    assert_eq!(LatencyLevel::from_millis(100), LatencyLevel::Medium);
    assert_eq!(LatencyLevel::from_millis(299), LatencyLevel::Medium);
    assert_eq!(LatencyLevel::from_millis(300), LatencyLevel::High);
}

#[test]
fn four_digit_latency_fits_tray_column_template() {
    use ipchecker::net_speed::TRAY_LATENCY_WIDTH_TEMPLATE;

    assert_eq!(format_latency(999), "?");
    assert_eq!(LatencyDisplay::from_millis(250).text, "250 ms");
    assert_eq!(TRAY_LATENCY_WIDTH_TEMPLATE, "9999 ms");
    assert!(LatencyDisplay::from_millis(250).text.len() <= TRAY_LATENCY_WIDTH_TEMPLATE.len());
}

#[test]
fn averages_recent_latency_samples() {
    let mut sampler = NetworkLatencySampler::default();
    assert_eq!(
        sampler.observe(Some(30)),
        &LatencyDisplay {
            level: LatencyLevel::Low,
            text: "30 ms".into(),
        }
    );
    assert_eq!(
        sampler.observe(Some(60)),
        &LatencyDisplay {
            level: LatencyLevel::Low,
            text: "45 ms".into(),
        }
    );
    assert_eq!(
        sampler.observe(Some(90)),
        &LatencyDisplay {
            level: LatencyLevel::Low,
            text: "60 ms".into(),
        }
    );
    assert_eq!(
        sampler.observe(Some(120)),
        &LatencyDisplay {
            level: LatencyLevel::Low,
            text: "90 ms".into(),
        }
    );
}

#[test]
fn medium_and_high_latency_use_status_display() {
    assert_eq!(
        LatencyDisplay::from_millis(150),
        LatencyDisplay {
            level: LatencyLevel::Medium,
            text: "150 ms".into(),
        }
    );
    assert_eq!(
        LatencyDisplay::from_millis(300),
        LatencyDisplay {
            level: LatencyLevel::High,
            text: "?".into(),
        }
    );

    let mut sampler = NetworkLatencySampler::default();
    sampler.observe(Some(300));
    sampler.observe(Some(300));
    assert_eq!(
        sampler.observe(Some(300)),
        &LatencyDisplay {
            level: LatencyLevel::High,
            text: "?".into(),
        }
    );
}

#[test]
fn latency_failure_clears_to_unknown() {
    let mut sampler = NetworkLatencySampler::default();
    sampler.observe(Some(50));
    assert_eq!(sampler.observe(None), &LatencyDisplay::unknown());
}

#[test]
fn tcp_latency_probe_reaches_a_public_host() {
    let latency = measure_tcp_latency();
    assert!(
        latency.is_some(),
        "expected a TCP latency sample to 1.1.1.1:443"
    );
    assert!(latency.unwrap() <= 5_000);
}
