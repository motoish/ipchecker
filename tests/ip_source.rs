use std::{
    collections::VecDeque,
    io::{Read, Write},
    net::{Ipv4Addr, TcpListener},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use ipchecker::ip_source::{
    FallbackIpSource, FetchError, HttpTextClient, IpSource, PUBLIC_IP_URLS, ReqwestTextClient,
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
    let fake = FakeClient::new([ok("8.8.8.8\n"), ok("1.1.1.1"), ok("9.9.9.9")]);
    let mut source = FallbackIpSource::new(fake, PUBLIC_IP_URLS);

    assert_eq!(source.fetch().unwrap(), ip("8.8.8.8"));
    assert_eq!(source.client().calls(), &[PUBLIC_IP_URLS[0]]);
}

#[test]
fn falls_back_after_transport_error_and_invalid_body() {
    let fake = FakeClient::new([err("timeout"), ok("2001:db8::1"), ok("1.1.1.1")]);
    let mut source = FallbackIpSource::new(fake, PUBLIC_IP_URLS);

    assert_eq!(source.fetch().unwrap(), ip("1.1.1.1"));
    assert_eq!(source.client().calls().len(), 3);
}

#[test]
fn falls_back_after_private_ipv4_response() {
    let fake = FakeClient::new([ok("192.168.1.10"), ok("8.8.8.8"), ok("1.1.1.1")]);
    let mut source = FallbackIpSource::new(fake, PUBLIC_IP_URLS);

    assert_eq!(source.fetch().unwrap(), ip("8.8.8.8"));
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
        "0.1.2.3",
        "192.0.2.1",
        "198.51.100.7",
        "203.0.113.8",
        "198.18.0.1",
        "240.0.0.1",
        "224.0.0.1",
        "255.255.255.255",
        "100.64.0.1",
    ] {
        let fake = FakeClient::new([ok(rejected), ok("8.8.8.8"), ok("1.1.1.1")]);
        let mut source = FallbackIpSource::new(fake, PUBLIC_IP_URLS);

        assert_eq!(
            source.fetch().unwrap(),
            ip("8.8.8.8"),
            "expected {rejected} to be rejected as non-public"
        );
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

#[test]
fn reqwest_client_opens_a_fresh_tcp_connection_for_each_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let server_accepted = Arc::clone(&accepted);
    let server = thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut streams = Vec::new();
        while std::time::Instant::now() < deadline {
            match listener.accept() {
                Ok((stream, _)) => {
                    server_accepted.fetch_add(1, Ordering::SeqCst);
                    stream.set_nonblocking(true).unwrap();
                    streams.push(stream);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => panic!("accept failed: {error}"),
            }

            for stream in &mut streams {
                let mut buffer = [0u8; 1024];
                match stream.read(&mut buffer) {
                    Ok(0) | Err(_) => {}
                    Ok(_) => {
                        let body = b"8.8.8.8";
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(response.as_bytes());
                        let _ = stream.write_all(body);
                        let _ = stream.flush();
                    }
                }
            }

            if server_accepted.load(Ordering::SeqCst) >= 2 {
                thread::sleep(Duration::from_millis(20));
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
    });

    let url = format!("http://{address}/");
    {
        let mut client = ReqwestTextClient::new().unwrap();
        assert_eq!(client.get_text(&url).unwrap().trim(), "8.8.8.8");
        assert_eq!(client.get_text(&url).unwrap().trim(), "8.8.8.8");
    }
    server.join().unwrap();
    assert_eq!(
        accepted.load(Ordering::SeqCst),
        2,
        "each public-IP poll must open a new TCP connection"
    );
}
