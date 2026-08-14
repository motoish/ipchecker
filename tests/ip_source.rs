use std::{collections::VecDeque, net::Ipv4Addr};

use ipchecker::ip_source::{
    FallbackIpSource, FetchError, HttpTextClient, IpSource, PUBLIC_IP_URLS,
};

struct FakeClient {
    responses: VecDeque<Result<String, String>>,
    calls: Vec<String>,
}

impl FakeClient {
    fn new<const N: usize>(responses: [Result<String, String>; N]) -> Self {
        Self {
            responses: responses.into(),
            calls: Vec::new(),
        }
    }

    fn calls(&self) -> &[String] {
        &self.calls
    }
}

impl HttpTextClient for FakeClient {
    fn get_text(&mut self, url: &str) -> Result<String, String> {
        self.calls.push(url.to_owned());
        self.responses
            .pop_front()
            .expect("a fake response should exist for every request")
    }
}

fn ip(value: &str) -> Ipv4Addr {
    value.parse().expect("test IP address should parse")
}

fn ok(body: &str) -> Result<String, String> {
    Ok(body.to_owned())
}

fn err(reason: &str) -> Result<String, String> {
    Err(reason.to_owned())
}

#[test]
fn returns_first_valid_ipv4_without_calling_later_sources() {
    let fake = FakeClient::new([ok("198.51.100.7\n"), ok("203.0.113.8"), ok("192.0.2.9")]);
    let mut source = FallbackIpSource::new(fake, PUBLIC_IP_URLS);

    assert_eq!(source.fetch().unwrap(), ip("198.51.100.7"));
    assert_eq!(source.client().calls(), &[PUBLIC_IP_URLS[0]]);
}

#[test]
fn falls_back_after_transport_error_and_invalid_body() {
    let fake = FakeClient::new([err("timeout"), ok("2001:db8::1"), ok("192.0.2.9")]);
    let mut source = FallbackIpSource::new(fake, PUBLIC_IP_URLS);

    assert_eq!(source.fetch().unwrap(), ip("192.0.2.9"));
    assert_eq!(source.client().calls().len(), 3);
}

#[test]
fn falls_back_after_private_ipv4_response() {
    let fake = FakeClient::new([ok("192.168.1.10"), ok("198.51.100.7"), ok("192.0.2.9")]);
    let mut source = FallbackIpSource::new(fake, PUBLIC_IP_URLS);

    assert_eq!(source.fetch().unwrap(), ip("198.51.100.7"));
    assert_eq!(
        source.client().calls(),
        &[PUBLIC_IP_URLS[0], PUBLIC_IP_URLS[1]]
    );
}

#[test]
fn rejects_other_non_public_ipv4_ranges() {
    for rejected in [
        "10.0.0.1",
        "127.0.0.1",
        "169.254.1.1",
        "0.0.0.0",
        "224.0.0.1",
        "255.255.255.255",
        "100.64.0.1",
    ] {
        let fake = FakeClient::new([ok(rejected), ok("198.51.100.7"), ok("192.0.2.9")]);
        let mut source = FallbackIpSource::new(fake, PUBLIC_IP_URLS);

        assert_eq!(source.fetch().unwrap(), ip("198.51.100.7"));
        assert_eq!(source.client().calls().len(), 2);
    }
}

#[test]
fn reports_every_failure_when_all_sources_fail() {
    let fake = FakeClient::new([err("offline"), ok("192.168.1.10"), err("timeout")]);
    let mut source = FallbackIpSource::new(fake, PUBLIC_IP_URLS);

    let FetchError::AllSourcesFailed(failures) = source.fetch().unwrap_err();
    assert_eq!(failures.len(), 3);
}
