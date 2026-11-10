//! Server configuration, read from the environment (`docs/DEPLOY.md` §2).

use std::str::FromStr;

use thiserror::Error;

use crate::client_ip::{Cidr, TrustedProxies};

const TRUST_PROXY: &str = "SPARAGNE_TRUST_PROXY";
const TRUSTED_PROXIES: &str = "SPARAGNE_TRUSTED_PROXIES";

/// A setting the server refuses to start with.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ConfigError {
    #[error("{TRUSTED_PROXIES}: {0:?} is not an address or a CIDR")]
    TrustedProxy(String),
}

/// Knobs that change how the API behaves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// `false` makes `POST /auth/register` answer `403 registration_disabled`.
    pub allow_registration: bool,
    /// Lifetime of a token in days; `0` or less issues already expired tokens
    /// (useful in tests).
    pub token_ttl_days: i64,
    /// The peers whose rightmost `X-Forwarded-For` entry is the client
    /// address, instead of the peer itself: the proxies that append it.
    pub trusted_proxies: TrustedProxies,
    /// Failed logins for one username, within [`Config::login_window_secs`],
    /// that lock it for as long again; `0` turns the limit off.
    pub login_max_failures: u32,
    /// The window of both login limits, in seconds.
    pub login_window_secs: u64,
    /// Failed logins from one client address within the window before it is
    /// refused for as long again; `0` turns the limit off.
    pub ip_max_failures: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            allow_registration: true,
            token_ttl_days: 30,
            trusted_proxies: TrustedProxies::None,
            login_max_failures: 5,
            login_window_secs: 900,
            ip_max_failures: 30,
        }
    }
}

impl Config {
    /// Reads `SPARAGNE_ALLOW_REGISTRATION`, `SPARAGNE_TOKEN_TTL_DAYS`,
    /// `SPARAGNE_TRUST_PROXY`, `SPARAGNE_TRUSTED_PROXIES`,
    /// `SPARAGNE_LOGIN_MAX_FAILURES`, `SPARAGNE_LOGIN_WINDOW_SECS` and
    /// `SPARAGNE_IP_MAX_FAILURES`, falling back to the defaults when unset or
    /// unparsable. The list of proxies is the exception: a mistake there
    /// would silently change whose address the limits count.
    ///
    /// # Errors
    ///
    /// An entry of `SPARAGNE_TRUSTED_PROXIES` that is not an address or a
    /// CIDR.
    pub fn from_env() -> Result<Self, ConfigError> {
        let default = Self::default();
        let trust_proxy = std::env::var(TRUST_PROXY).ok();
        let proxies = std::env::var(TRUSTED_PROXIES).ok();
        let trusted_proxies = trusted_proxies(trust_proxy.as_deref(), proxies.as_deref())?;
        if trusted_proxies != TrustedProxies::Any && trusts_any(trust_proxy.as_deref()) {
            tracing::warn!("{TRUSTED_PROXIES} is set: {TRUST_PROXY}=true is ignored");
        }
        Ok(Self {
            allow_registration: env_bool("SPARAGNE_ALLOW_REGISTRATION", default.allow_registration),
            token_ttl_days: env_parse("SPARAGNE_TOKEN_TTL_DAYS", default.token_ttl_days),
            trusted_proxies,
            login_max_failures: env_parse(
                "SPARAGNE_LOGIN_MAX_FAILURES",
                default.login_max_failures,
            ),
            login_window_secs: env_parse("SPARAGNE_LOGIN_WINDOW_SECS", default.login_window_secs),
            ip_max_failures: env_parse("SPARAGNE_IP_MAX_FAILURES", default.ip_max_failures),
        })
    }

    /// Seconds a freshly issued token stays valid.
    #[must_use]
    pub const fn token_ttl_seconds(&self) -> i64 {
        self.token_ttl_days.saturating_mul(86_400)
    }
}

/// The list wins over the switch, being the narrower of the two; an empty
/// list is no list.
fn trusted_proxies(
    trust_proxy: Option<&str>,
    list: Option<&str>,
) -> Result<TrustedProxies, ConfigError> {
    let proxies = list
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            entry
                .parse::<Cidr>()
                .map_err(|_| ConfigError::TrustedProxy(entry.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(if !proxies.is_empty() {
        TrustedProxies::Only(proxies)
    } else if trusts_any(trust_proxy) {
        TrustedProxies::Any
    } else {
        TrustedProxies::None
    })
}

fn trusts_any(trust_proxy: Option<&str>) -> bool {
    trust_proxy.and_then(parse_bool).unwrap_or(false)
}

fn env_bool(name: &str, default: bool) -> bool {
    std::env::var(name)
        .ok()
        .and_then(|value| parse_bool(&value))
        .unwrap_or(default)
}

fn env_parse<T: FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(default)
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only(proxies: &[&str]) -> TrustedProxies {
        TrustedProxies::Only(
            proxies
                .iter()
                .filter_map(|text| text.parse().ok())
                .collect(),
        )
    }

    #[test]
    fn nothing_set_trusts_nobody() {
        assert_eq!(trusted_proxies(None, None), Ok(TrustedProxies::None));
        assert_eq!(
            trusted_proxies(Some("false"), Some("  ")),
            Ok(TrustedProxies::None)
        );
        assert_eq!(
            trusted_proxies(Some("maybe"), Some(" , ")),
            Ok(TrustedProxies::None)
        );
    }

    #[test]
    fn the_switch_alone_trusts_any_peer() {
        assert_eq!(trusted_proxies(Some("true"), None), Ok(TrustedProxies::Any));
        assert_eq!(
            trusted_proxies(Some(" On "), Some("")),
            Ok(TrustedProxies::Any)
        );
    }

    #[test]
    fn a_list_wins_over_the_switch() {
        let expected = only(&["192.168.178.193", "172.16.0.0/12", "::1"]);
        let list = Some(" 192.168.178.193, 172.16.0.0/12 ,::1,");
        assert_eq!(trusted_proxies(None, list), Ok(expected.clone()));
        assert_eq!(trusted_proxies(Some("true"), list), Ok(expected));
    }

    #[test]
    fn a_wrong_entry_is_named() {
        let err = trusted_proxies(Some("true"), Some("192.168.178.193, 192.168.178.300"));
        assert_eq!(
            err,
            Err(ConfigError::TrustedProxy("192.168.178.300".to_string()))
        );
        assert_eq!(
            err.map_err(|err| err.to_string()),
            Err(
                "SPARAGNE_TRUSTED_PROXIES: \"192.168.178.300\" is not an address or a CIDR"
                    .to_string()
            )
        );
    }
}
