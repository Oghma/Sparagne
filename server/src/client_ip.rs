//! The address a request comes from, the key of the per-client limits in
//! [`crate::ratelimit`]. Without a proxy it is the TCP peer, which `main.rs`
//! asks axum to record as [`ConnectInfo`]. Behind a proxy the peer is always
//! the proxy, so when the peer is a trusted one ([`TrustedProxies`]) the
//! rightmost `X-Forwarded-For` entry is used instead: the one the proxy
//! appended itself, which a client cannot forge. From any other peer the
//! header is ignored, since anyone can send it.

use std::{
    convert::Infallible,
    fmt,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    str::FromStr,
};

use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::{HeaderMap, request::Parts},
};
use thiserror::Error;

use crate::state::AppState;

/// What a request without a known peer counts as: the router driven
/// in-process, as the tests do, has no connection to take one from.
pub const UNKNOWN: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);

const X_FORWARDED_FOR: &str = "x-forwarded-for";

/// An address or a network in CIDR notation (`192.168.1.5`, `10.0.0.0/8`,
/// `fd00::/8`). An IPv4-mapped IPv6 address counts as its IPv4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cidr {
    addr: IpAddr,
    prefix: u8,
}

/// Why a text is not a [`Cidr`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error("not an address or a CIDR")]
pub struct InvalidCidr;

impl Cidr {
    #[must_use]
    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.addr, ip.to_canonical()) {
            (IpAddr::V4(net), IpAddr::V4(ip)) => {
                let mask = u32::MAX
                    .checked_shl(u32::from(32 - self.prefix))
                    .unwrap_or(0);
                (u32::from(net) ^ u32::from(ip)) & mask == 0
            }
            (IpAddr::V6(net), IpAddr::V6(ip)) => {
                let mask = u128::MAX
                    .checked_shl(u32::from(128 - self.prefix))
                    .unwrap_or(0);
                (u128::from(net) ^ u128::from(ip)) & mask == 0
            }
            _ => false,
        }
    }
}

impl FromStr for Cidr {
    type Err = InvalidCidr;

    fn from_str(text: &str) -> Result<Self, InvalidCidr> {
        let (addr, prefix) = match text.split_once('/') {
            Some((addr, prefix)) => (addr, Some(prefix)),
            None => (text, None),
        };
        let written: IpAddr = addr.trim().parse().map_err(|_| InvalidCidr)?;
        let width: u8 = if written.is_ipv4() { 32 } else { 128 };
        let prefix = match prefix {
            // Digits only: `u8::from_str` would take a leading `+`.
            Some(digits) if digits.bytes().all(|byte| byte.is_ascii_digit()) => {
                digits.parse().map_err(|_| InvalidCidr)?
            }
            Some(_) => return Err(InvalidCidr),
            None => width,
        };
        if prefix > width {
            return Err(InvalidCidr);
        }
        let addr = written.to_canonical();
        if addr.is_ipv4() && written.is_ipv6() {
            // `::ffff:a.b.c.d/n`: the IPv4 network under the 96 mapped bits.
            let prefix = prefix.checked_sub(96).ok_or(InvalidCidr)?;
            return Ok(Self { addr, prefix });
        }
        Ok(Self { addr, prefix })
    }
}

impl fmt::Display for Cidr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let width = if self.addr.is_ipv4() { 32 } else { 128 };
        if self.prefix == width {
            write!(f, "{}", self.addr)
        } else {
            write!(f, "{}/{}", self.addr, self.prefix)
        }
    }
}

/// Whose `X-Forwarded-For` is believed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum TrustedProxies {
    /// Nobody's: the client is always the TCP peer.
    #[default]
    None,
    /// Any peer's (`SPARAGNE_TRUST_PROXY=true`): only where nothing but the
    /// proxy can reach the server, as on `127.0.0.1` or inside `compose.yml`.
    Any,
    /// Only that of a peer in the list (`SPARAGNE_TRUSTED_PROXIES`).
    Only(Vec<Cidr>),
}

impl TrustedProxies {
    fn trusts(&self, peer: Option<IpAddr>) -> bool {
        match self {
            Self::None => false,
            Self::Any => true,
            Self::Only(proxies) => {
                peer.is_some_and(|ip| proxies.iter().any(|proxy| proxy.contains(ip)))
            }
        }
    }
}

/// How the `listening` line of the log names the setting.
impl fmt::Display for TrustedProxies {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => f.write_str("none"),
            Self::Any => f.write_str("any"),
            Self::Only(proxies) => {
                for (i, proxy) in proxies.iter().enumerate() {
                    if i > 0 {
                        f.write_str(",")?;
                    }
                    write!(f, "{proxy}")?;
                }
                Ok(())
            }
        }
    }
}

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
            &state.config().trusted_proxies,
        )))
    }
}

/// The rightmost `X-Forwarded-For` entry when the peer is a trusted proxy and
/// the entry is an address, else the peer, else [`UNKNOWN`].
#[must_use]
pub fn client_ip(
    headers: &HeaderMap,
    peer: Option<SocketAddr>,
    trusted: &TrustedProxies,
) -> IpAddr {
    let peer = peer.map(|addr| addr.ip().to_canonical());
    trusted
        .trusts(peer)
        .then(|| rightmost_forwarded_for(headers))
        .flatten()
        .map(|ip| ip.to_canonical())
        .or(peer)
        .unwrap_or(UNKNOWN)
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
        assert_eq!(
            client_ip(&forged, proxy(), &TrustedProxies::Any),
            ip("203.0.113.9")
        );
        let two_lines = headers(&["1.1.1.1", "203.0.113.9"]);
        assert_eq!(
            client_ip(&two_lines, proxy(), &TrustedProxies::Any),
            ip("203.0.113.9")
        );
        let v6 = headers(&["2001:db8::1"]);
        assert_eq!(
            client_ip(&v6, proxy(), &TrustedProxies::Any),
            ip("2001:db8::1")
        );
        let with_port = headers(&["[2001:db8::1]:443"]);
        assert_eq!(
            client_ip(&with_port, proxy(), &TrustedProxies::Any),
            ip("2001:db8::1")
        );
    }

    #[test]
    fn an_untrusted_header_is_ignored() {
        let forged = headers(&["203.0.113.9"]);
        assert_eq!(
            client_ip(&forged, proxy(), &TrustedProxies::None),
            ip("10.0.0.2")
        );
        assert_eq!(client_ip(&forged, None, &TrustedProxies::None), UNKNOWN);
    }

    #[test]
    fn the_peer_stands_in_for_a_missing_or_garbled_header() {
        assert_eq!(
            client_ip(&HeaderMap::new(), proxy(), &TrustedProxies::Any),
            ip("10.0.0.2")
        );
        let garbled = headers(&["not an address"]);
        assert_eq!(
            client_ip(&garbled, proxy(), &TrustedProxies::Any),
            ip("10.0.0.2")
        );
        assert_eq!(
            client_ip(&HeaderMap::new(), None, &TrustedProxies::Any),
            UNKNOWN
        );
    }

    #[test]
    fn a_mapped_ipv4_is_the_ipv4() {
        let peer = "[::ffff:192.0.2.1]:5000".parse().ok();
        assert_eq!(
            client_ip(&HeaderMap::new(), peer, &TrustedProxies::None),
            ip("192.0.2.1")
        );
    }

    /// A host nothing in these tests uses, for a text that does not parse.
    fn cidr(text: &str) -> Cidr {
        text.parse().unwrap_or(Cidr {
            addr: UNKNOWN,
            prefix: 32,
        })
    }

    fn only(proxies: &[&str]) -> TrustedProxies {
        TrustedProxies::Only(
            proxies
                .iter()
                .filter_map(|text| text.parse().ok())
                .collect(),
        )
    }

    #[test]
    fn a_listed_proxy_gives_the_rightmost_entry() {
        let forwarded = headers(&["1.1.1.1, 203.0.113.9"]);
        for list in [&["10.0.0.2"][..], &["192.0.2.0/24", "10.0.0.0/8"]] {
            assert_eq!(
                client_ip(&forwarded, proxy(), &only(list)),
                ip("203.0.113.9")
            );
        }
    }

    #[test]
    fn a_peer_off_the_list_cannot_choose_its_address() {
        let forged = headers(&["203.0.113.9"]);
        let trusted = only(&["10.0.0.3", "192.0.2.0/24"]);
        assert_eq!(client_ip(&forged, proxy(), &trusted), ip("10.0.0.2"));
        // No peer to check against the list: the header says nothing.
        assert_eq!(client_ip(&forged, None, &trusted), UNKNOWN);
    }

    #[test]
    fn a_mapped_peer_matches_its_ipv4_entry_and_back() {
        let forwarded = headers(&["203.0.113.9"]);
        let mapped = "[::ffff:10.0.0.2]:41000".parse().ok();
        assert_eq!(
            client_ip(&forwarded, mapped, &only(&["10.0.0.2"])),
            ip("203.0.113.9")
        );
        assert_eq!(
            client_ip(&forwarded, proxy(), &only(&["::ffff:10.0.0.2"])),
            ip("203.0.113.9")
        );
        assert_eq!(
            client_ip(&forwarded, proxy(), &only(&["::ffff:10.0.0.0/120"])),
            ip("203.0.113.9")
        );
    }

    #[test]
    fn a_cidr_holds_its_network_and_nothing_else() {
        let net = cidr("192.168.178.64/26");
        assert!(net.contains(ip("192.168.178.64")));
        assert!(net.contains(ip("192.168.178.127")));
        assert!(!net.contains(ip("192.168.178.63")));
        assert!(!net.contains(ip("192.168.178.128")));
        assert!(!net.contains(ip("::1")));

        let v6 = cidr("fd00::/8");
        assert!(v6.contains(ip("fd12:3456::1")));
        assert!(!v6.contains(ip("fe80::1")));
        assert!(!v6.contains(ip("10.0.0.1")));

        let everything = cidr("0.0.0.0/0");
        assert!(everything.contains(ip("203.0.113.9")));
        let one = cidr("::1");
        assert!(one.contains(ip("::1")));
        assert!(!one.contains(ip("::2")));
    }

    #[test]
    fn what_is_not_a_cidr_is_refused() {
        for text in [
            "",
            "proxy",
            "192.168.178.300",
            "10.0.0.0/33",
            "10.0.0.0/",
            "10.0.0.0/+8",
            "10.0.0.0/8/8",
            "::1/129",
            "::ffff:10.0.0.0/8",
        ] {
            assert_eq!(text.parse::<Cidr>(), Err(InvalidCidr), "{text:?}");
        }
    }

    #[test]
    fn the_log_names_what_is_trusted() {
        assert_eq!(TrustedProxies::None.to_string(), "none");
        assert_eq!(TrustedProxies::Any.to_string(), "any");
        let list = only(&["192.168.178.193/32", "::ffff:10.0.0.2", "fd00::/8"]);
        assert_eq!(list.to_string(), "192.168.178.193,10.0.0.2,fd00::/8");
    }
}
