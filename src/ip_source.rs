use std::{net::Ipv4Addr, time::Duration};

pub const PUBLIC_IP_URLS: [&str; 3] = [
    "https://api.ipify.org",
    "https://ifconfig.me/ip",
    "https://icanhazip.com",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFailure {
    pub url: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    AllSourcesFailed(Vec<SourceFailure>),
}

pub trait IpSource: Send + 'static {
    fn fetch(&mut self) -> Result<Ipv4Addr, FetchError>;
}

pub trait HttpTextClient {
    fn get_text(&mut self, url: &str) -> Result<String, String>;
}

pub struct FallbackIpSource<C> {
    client: C,
    urls: [&'static str; 3],
}

impl<C> FallbackIpSource<C> {
    pub fn new(client: C, urls: [&'static str; 3]) -> Self {
        Self { client, urls }
    }

    pub fn client(&self) -> &C {
        &self.client
    }
}

impl<C> IpSource for FallbackIpSource<C>
where
    C: HttpTextClient + Send + 'static,
{
    fn fetch(&mut self) -> Result<Ipv4Addr, FetchError> {
        let mut failures = Vec::with_capacity(self.urls.len());

        for url in self.urls {
            let result = self.client.get_text(url).and_then(|body| {
                body.trim()
                    .parse::<Ipv4Addr>()
                    .map_err(|error| error.to_string())
                    .and_then(|ip| {
                        is_usable_public_ipv4(ip)
                            .then_some(ip)
                            .ok_or_else(|| "address is not a public IPv4 address".to_owned())
                    })
            });

            match result {
                Ok(ip) => return Ok(ip),
                Err(reason) => failures.push(SourceFailure {
                    url: url.to_owned(),
                    reason,
                }),
            }
        }

        Err(FetchError::AllSourcesFailed(failures))
    }
}

fn is_usable_public_ipv4(ip: Ipv4Addr) -> bool {
    !ip.is_private()
        && !ip.is_loopback()
        && !ip.is_link_local()
        && !ip.is_unspecified()
        && !ip.is_multicast()
        && !ip.is_broadcast()
        && !is_shared_address(ip)
}

fn is_shared_address(ip: Ipv4Addr) -> bool {
    let [first, second, _, _] = ip.octets();
    first == 100 && (64..=127).contains(&second)
}

pub struct ReqwestTextClient {
    client: reqwest::blocking::Client,
}

impl ReqwestTextClient {
    pub fn new() -> Result<Self, reqwest::Error> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(8))
            .build()?;
        Ok(Self { client })
    }
}

impl HttpTextClient for ReqwestTextClient {
    fn get_text(&mut self, url: &str) -> Result<String, String> {
        self.client
            .get(url)
            .send()
            .and_then(|response| response.error_for_status())
            .and_then(|response| response.text())
            .map_err(|error| error.to_string())
    }
}

pub struct ReqwestIpSource(FallbackIpSource<ReqwestTextClient>);

impl ReqwestIpSource {
    pub fn new() -> Result<Self, reqwest::Error> {
        Ok(Self(FallbackIpSource::new(
            ReqwestTextClient::new()?,
            PUBLIC_IP_URLS,
        )))
    }
}

impl IpSource for ReqwestIpSource {
    fn fetch(&mut self) -> Result<Ipv4Addr, FetchError> {
        self.0.fetch()
    }
}
