// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Outbound HTTP for adapters. A request may only go over https to a host on the adapter's list (or the one host an
//! owner entered, such as an OIDC issuer), and every address that host resolves to must be public: private,
//! loopback, link-local, shared, documentation and multicast ranges are refused in the DNS step, so a rebinding DNS
//! answer cannot reach the internal network either. Redirects are not followed. Calls are bounded by timeouts, only
//! idempotent requests are retried, and errors never carry the URL (paths and queries can hold keys).

use super::R;
use crate::providers::ProviderError;
use reqwest::{
    Method, RequestBuilder, Response, StatusCode, Url,
    dns::{Addrs, Name, Resolve, Resolving},
    header::HeaderValue,
    redirect,
};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

const TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Attempts for an idempotent request: the first plus two retries.
const ATTEMPTS: u32 = 3;
const BACKOFF: Duration = Duration::from_millis(200);

/// The hosts a guard lets requests reach.
#[derive(Clone, Debug)]
pub enum Hosts {
    /// An adapter's fixed list (`AdapterSpec::hosts`).
    Fixed(&'static [&'static str]),
    /// One host the owner entered (an OIDC issuer).
    Owner(String),
}

impl Hosts {
    fn allows(&self, host: &str) -> bool {
        match self {
            Self::Fixed(list) => list.iter().any(|h| h.eq_ignore_ascii_case(host)),
            Self::Owner(h) => h.eq_ignore_ascii_case(host),
        }
    }
}

#[derive(Clone)]
pub struct HttpGuard {
    client: reqwest::Client,
    hosts: Hosts,
    /// Tests only: the one local origin (`http://127.0.0.1:port`) a mock serves on.
    local: Option<Url>,
}

/// Whether an address is on the public internet. Everything else is refused, including addresses embedded in IPv6.
pub fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => public_v4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => public_v4(v4),
            None => public_v6(v6),
        },
    }
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    let shared = a == 100 && (64..128).contains(&b); // 100.64.0.0/10, carrier-grade NAT
    let benchmark = a == 198 && (18..20).contains(&b); // 198.18.0.0/15
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_multicast()
        || shared
        || benchmark
        || a == 0
        || a >= 240)
}

fn public_v6(ip: Ipv6Addr) -> bool {
    let first = ip.segments()[0];
    let unique_local = first & 0xfe00 == 0xfc00; // fc00::/7
    let link_local = first & 0xffc0 == 0xfe80; // fe80::/10
    let documentation = first == 0x2001 && ip.segments()[1] == 0x0db8; // 2001:db8::/32
    let nat64 = first == 0x0064 && ip.segments()[1] == 0xff9b; // 64:ff9b::/96 can embed a private v4
    !(ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() || unique_local || link_local || documentation || nat64)
}

/// Resolves with the system resolver and keeps only public addresses. No public address: the lookup fails.
struct PublicOnly;

impl Resolve for PublicOnly {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            let found: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0)).await?.filter(|a| public_ip(a.ip())).collect();
            if found.is_empty() {
                return Err(format!("{host} has no public address").into());
            }
            Ok(Box::new(found.into_iter()) as Addrs)
        })
    }
}

/// An error without its URL.
fn upstream(e: reqwest::Error) -> ProviderError {
    ProviderError::Upstream(e.without_url().to_string())
}

fn refused(why: &str) -> ProviderError {
    ProviderError::Upstream(format!("outbound request refused: {why}"))
}

/// A header value (an API key, a Basic or Bearer credential) that debug output and logs show as "Sensitive".
pub fn secret_header(value: &str) -> R<HeaderValue> {
    let mut v = HeaderValue::from_str(value).map_err(|_| refused("header value is not visible ASCII"))?;
    v.set_sensitive(true);
    Ok(v)
}

impl HttpGuard {
    pub fn new(hosts: Hosts) -> R<Self> {
        let client = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(redirect::Policy::none())
            .https_only(true)
            .dns_resolver(Arc::new(PublicOnly))
            .build()
            .map_err(upstream)?;
        Ok(Self { client, hosts, local: None })
    }

    /// Tests only: also allows plain http to the one local origin of `base` (a mock gateway on 127.0.0.1). Never
    /// call it outside tests; nothing in the configuration reaches it.
    #[doc(hidden)]
    pub fn allow_local_for_tests(hosts: Hosts, base: &str) -> R<Self> {
        let local = Url::parse(base).map_err(|_| refused("bad test base"))?;
        let client = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .redirect(redirect::Policy::none())
            .build()
            .map_err(upstream)?;
        Ok(Self {
            client,
            hosts,
            local: Some(local),
        })
    }

    /// A request to `url` once it passes the rules: https, a listed host, and no non-public address literal.
    pub fn request(&self, method: Method, url: &str) -> R<RequestBuilder> {
        let u = Url::parse(url).map_err(|_| refused("not a URL"))?;
        if !self.local.as_ref().is_some_and(|l| l.origin() == u.origin()) {
            self.check(&u)?;
        }
        Ok(self.client.request(method, u))
    }

    fn check(&self, u: &Url) -> R<()> {
        if u.scheme() != "https" {
            return Err(refused("https only"));
        }
        if !u.username().is_empty() || u.password().is_some() {
            return Err(refused("credentials in the URL"));
        }
        let host = u.host_str().ok_or_else(|| refused("no host"))?;
        let literal = host.trim_start_matches('[').trim_end_matches(']').parse::<IpAddr>();
        if literal.is_ok_and(|ip| !public_ip(ip)) {
            return Err(refused("non-public address"));
        }
        if !self.hosts.allows(host) {
            return Err(refused("host not on the adapter's list"));
        }
        Ok(())
    }

    /// Sends a request. An idempotent one is retried on a transport error or a 502/503/504, with a short backoff;
    /// anything else is sent once, because it may have taken effect.
    pub async fn send(&self, rb: RequestBuilder, idempotent: bool) -> R<Response> {
        let attempts = if idempotent { ATTEMPTS } else { 1 };
        let mut last = refused("no attempt made");
        for attempt in 1..=attempts {
            let Some(this) = rb.try_clone() else {
                return rb.send().await.map_err(upstream);
            };
            match this.send().await {
                Ok(r) if attempt < attempts && retryable(r.status()) => last = ProviderError::Upstream(format!("status {}", r.status())),
                Ok(r) => return Ok(r),
                Err(e) if e.is_builder() || e.is_redirect() => return Err(upstream(e)),
                Err(e) => last = upstream(e),
            }
            if attempt < attempts {
                tokio::time::sleep(BACKOFF * attempt).await;
            }
        }
        Err(last)
    }
}

fn retryable(s: StatusCode) -> bool {
    matches!(s, StatusCode::BAD_GATEWAY | StatusCode::SERVICE_UNAVAILABLE | StatusCode::GATEWAY_TIMEOUT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guard() -> HttpGuard {
        HttpGuard::new(Hosts::Fixed(&["api.example.com"])).expect("client")
    }

    #[test]
    fn private_and_special_addresses_are_not_public() {
        for bad in [
            "10.1.2.3",
            "127.0.0.1",
            "169.254.169.254",
            "172.16.0.1",
            "192.168.1.1",
            "100.64.0.1",
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.1",
            "::1",
            "::",
            "fc00::1",
            "fd12::1",
            "fe80::1",
            "::ffff:10.0.0.1",
            "::ffff:127.0.0.1",
            "64:ff9b::a00:1",
            "2001:db8::1",
        ] {
            assert!(!public_ip(bad.parse().expect(bad)), "{bad}");
        }
        for ok in ["1.1.1.1", "203.0.114.1", "2606:4700::1111"] {
            assert!(public_ip(ok.parse().expect(ok)), "{ok}");
        }
    }

    #[test]
    fn requests_need_https_and_a_listed_public_host() {
        let g = guard();
        assert!(g.request(Method::GET, "https://api.example.com/v1/x").is_ok());
        assert!(
            g.request(Method::GET, "https://API.example.com/v1/x").is_ok(),
            "hosts compare case-insensitively"
        );
        for bad in [
            "http://api.example.com/v1/x",
            "https://evil.example.com/",
            "https://api.example.com.evil.test/",
            "https://user:pw@api.example.com/",
            "https://10.0.0.1/",
            "https://127.0.0.1/",
            "https://169.254.169.254/latest/meta-data",
            "https://[::1]/",
            "file:///etc/passwd",
            "not a url",
        ] {
            assert!(g.request(Method::GET, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn an_owner_host_allows_that_host_only() {
        let g = HttpGuard::new(Hosts::Owner("login.example.org".into())).expect("client");
        assert!(g.request(Method::GET, "https://login.example.org/.well-known/openid-configuration").is_ok());
        assert!(g.request(Method::GET, "https://other.example.org/").is_err());
    }

    #[tokio::test]
    async fn names_resolving_to_private_addresses_are_refused() {
        let found = PublicOnly.resolve("localhost".parse().expect("name")).await;
        assert!(found.is_err(), "localhost resolves to loopback only");
    }

    #[test]
    fn secret_headers_are_hidden_from_debug_output() {
        let v = secret_header("Basic c2VjcmV0Og==").expect("value");
        assert!(v.is_sensitive());
        assert!(!format!("{v:?}").contains("c2VjcmV0"));
    }
}
