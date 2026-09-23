//! Server configuration, read from the environment (`docs/v2/SYNC.md` §2,
//! `docs/v2/DEPLOY.md` §2).

use std::str::FromStr;

/// Knobs that change how the API behaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// `false` makes `POST /auth/register` answer `403 registration_disabled`.
    pub allow_registration: bool,
    /// Lifetime of a token in days; `0` or less issues already expired tokens
    /// (useful in tests).
    pub token_ttl_days: i64,
    /// Take the client address from the rightmost `X-Forwarded-For` entry
    /// instead of the TCP peer. Only behind a proxy that appends it, with the
    /// server unreachable except through that proxy.
    pub trust_forwarded_for: bool,
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
            trust_forwarded_for: false,
            login_max_failures: 5,
            login_window_secs: 900,
            ip_max_failures: 30,
        }
    }
}

impl Config {
    /// Reads `SPARAGNE_ALLOW_REGISTRATION`, `SPARAGNE_TOKEN_TTL_DAYS`,
    /// `SPARAGNE_TRUST_PROXY`, `SPARAGNE_LOGIN_MAX_FAILURES`,
    /// `SPARAGNE_LOGIN_WINDOW_SECS` and `SPARAGNE_IP_MAX_FAILURES`, falling
    /// back to the defaults when unset or unparsable.
    #[must_use]
    pub fn from_env() -> Self {
        let default = Self::default();
        Self {
            allow_registration: env_bool("SPARAGNE_ALLOW_REGISTRATION", default.allow_registration),
            token_ttl_days: env_parse("SPARAGNE_TOKEN_TTL_DAYS", default.token_ttl_days),
            trust_forwarded_for: env_bool("SPARAGNE_TRUST_PROXY", default.trust_forwarded_for),
            login_max_failures: env_parse(
                "SPARAGNE_LOGIN_MAX_FAILURES",
                default.login_max_failures,
            ),
            login_window_secs: env_parse("SPARAGNE_LOGIN_WINDOW_SECS", default.login_window_secs),
            ip_max_failures: env_parse("SPARAGNE_IP_MAX_FAILURES", default.ip_max_failures),
        }
    }

    /// Seconds a freshly issued token stays valid.
    #[must_use]
    pub const fn token_ttl_seconds(&self) -> i64 {
        self.token_ttl_days.saturating_mul(86_400)
    }
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
