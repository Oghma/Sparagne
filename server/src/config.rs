//! Server configuration, read from the environment (`docs/v2/SYNC.md` §2).

/// Knobs that change how the API behaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// `false` makes `POST /auth/register` answer `403 registration_disabled`.
    pub allow_registration: bool,
    /// Lifetime of a token in days; `0` or less issues already expired tokens
    /// (useful in tests).
    pub token_ttl_days: i64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            allow_registration: true,
            token_ttl_days: 30,
        }
    }
}

impl Config {
    /// Reads `SPARAGNE_ALLOW_REGISTRATION` and `SPARAGNE_TOKEN_TTL_DAYS`,
    /// falling back to the defaults when unset or unparsable.
    #[must_use]
    pub fn from_env() -> Self {
        let default = Self::default();
        let allow_registration = match std::env::var("SPARAGNE_ALLOW_REGISTRATION") {
            Ok(value) => parse_bool(&value).unwrap_or(default.allow_registration),
            Err(_) => default.allow_registration,
        };
        let token_ttl_days = match std::env::var("SPARAGNE_TOKEN_TTL_DAYS") {
            Ok(value) => value.trim().parse().unwrap_or(default.token_ttl_days),
            Err(_) => default.token_ttl_days,
        };
        Self {
            allow_registration,
            token_ttl_days,
        }
    }

    /// Seconds a freshly issued token stays valid.
    #[must_use]
    pub const fn token_ttl_seconds(&self) -> i64 {
        self.token_ttl_days.saturating_mul(86_400)
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}
