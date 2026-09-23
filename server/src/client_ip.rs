//! The address a request comes from: the key of the per-client limits in
//! [`crate::ratelimit`].
//!
//! Without a proxy it is the TCP peer, which `main.rs` asks axum to record as
//! [`ConnectInfo`]. Behind Caddy the peer is always the proxy, so with
//! `SPARAGNE_TRUST_PROXY=true` the rightmost `X-Forwarded-For` entry is used
//! instead: the one the proxy appended itself, which a client cannot forge.
//! Without that setting the header is ignored, since anyone can send it.

use std::{
    convert::Infallible,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::{HeaderMap, request::Parts},
};

use crate::state::AppState;

/// What a request without a known peer counts as: the router driven
/// in-process, as the tests do, has no connection to take one from.
pub const UNKNOWN: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);

const X_FORWARDED_FOR: &str = "x-forwarded-for";

/// The client address of the request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClientIp(pub IpAddr);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Infallible> {
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| *addr);
        Ok(Self(client_ip(
            &parts.headers,
            peer,
            state.config().trust_forwarded_for,
        )))
    }
}

/// The rightmost `X-Forwarded-For` entry when the proxy is trusted and the
/// entry is an address, else the peer, else [`UNKNOWN`].
#[must_use]
pub fn client_ip(
    headers: &HeaderMap,
    peer: Option<SocketAddr>,
    trust_forwarded_for: bool,
) -> IpAddr {
    trust_forwarded_for
        .then(|| rightmost_forwarded_for(headers))
        .flatten()
        .or_else(|| peer.map(|addr| addr.ip()))
        .map_or(UNKNOWN, |ip| ip.to_canonical())
}

fn rightmost_forwarded_for(headers: &HeaderMap) -> Option<IpAddr> {
    let last = headers.get_all(X_FORWARDED_FOR).iter().next_back()?;
    let entry = last.to_str().ok()?.rsplit(',').next()?.trim();
    entry
        .parse::<IpAddr>()
        .ok()
        .or_else(|| entry.parse::<SocketAddr>().ok().map(|addr| addr.ip()))
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    const PROXY: &str = "10.0.0.2:41000";

    fn headers(values: &[&'static str]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for value in values {
            map.append(X_FORWARDED_FOR, HeaderValue::from_static(value));
        }
        map
    }

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap_or(UNKNOWN)
    }

    fn proxy() -> Option<SocketAddr> {
        PROXY.parse().ok()
    }

    #[test]
    fn a_trusted_proxy_gives_the_rightmost_entry() {
        let forged = headers(&["1.1.1.1, 203.0.113.9"]);
        assert_eq!(client_ip(&forged, proxy(), true), ip("203.0.113.9"));
        let two_lines = headers(&["1.1.1.1", "203.0.113.9"]);
        assert_eq!(client_ip(&two_lines, proxy(), true), ip("203.0.113.9"));
        let v6 = headers(&["2001:db8::1"]);
        assert_eq!(client_ip(&v6, proxy(), true), ip("2001:db8::1"));
        let with_port = headers(&["[2001:db8::1]:443"]);
        assert_eq!(client_ip(&with_port, proxy(), true), ip("2001:db8::1"));
    }

    #[test]
    fn an_untrusted_header_is_ignored() {
        let forged = headers(&["203.0.113.9"]);
        assert_eq!(client_ip(&forged, proxy(), false), ip("10.0.0.2"));
        assert_eq!(client_ip(&forged, None, false), UNKNOWN);
    }

    #[test]
    fn the_peer_stands_in_for_a_missing_or_garbled_header() {
        assert_eq!(client_ip(&HeaderMap::new(), proxy(), true), ip("10.0.0.2"));
        let garbled = headers(&["not an address"]);
        assert_eq!(client_ip(&garbled, proxy(), true), ip("10.0.0.2"));
        assert_eq!(client_ip(&HeaderMap::new(), None, true), UNKNOWN);
    }

    #[test]
    fn a_mapped_ipv4_is_the_ipv4() {
        let peer = "[::ffff:192.0.2.1]:5000".parse().ok();
        assert_eq!(client_ip(&HeaderMap::new(), peer, false), ip("192.0.2.1"));
    }
}
