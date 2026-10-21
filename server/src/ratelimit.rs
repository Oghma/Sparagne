//! In-memory brakes on guessing passwords and on mass registration
//! (`docs/DEPLOY.md` §2): logins count per username and per client address,
//! registrations per address. A bucket counts hits in a window that opens
//! with its first hit; the hit that reaches the limit blocks the key for a
//! whole window, during which the request is `429` before any password is
//! hashed. A login counts when it starts, so a burst of parallel guesses
//! cannot slip past the check, and one that succeeds gives its hit back.
//! Every method takes `now`, so tests move the clock; a restart forgets it all.

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv6Addr},
    sync::{Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use crate::{
    config::Config,
    error::{ApiError, ApiResult},
};

/// Above this many buckets, the expired ones are dropped.
const PRUNE_ABOVE: usize = 10_000;
const REGISTER_MAX_ATTEMPTS: u32 = 10;
const REGISTER_WINDOW: Duration = Duration::from_secs(3_600);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Key {
    Username(String),
    LoginFrom(IpAddr),
    RegisterFrom(IpAddr),
}

#[derive(Clone, Copy, Debug)]
struct Bucket {
    hits: u32,
    /// When the counting window closes and the bucket is forgotten.
    window_ends: Instant,
    /// Set by the hit that reached the limit.
    blocked_until: Option<Instant>,
}

impl Bucket {
    fn fresh(now: Instant, window: Duration) -> Self {
        Self {
            hits: 0,
            window_ends: now.checked_add(window).unwrap_or(now),
            blocked_until: None,
        }
    }

    fn expired(&self, now: Instant) -> bool {
        match self.blocked_until {
            Some(until) => until <= now,
            None => self.window_ends <= now,
        }
    }

    fn blocked_for(&self, now: Instant) -> Option<Duration> {
        self.blocked_until
            .filter(|until| *until > now)
            .map(|until| until - now)
    }
}

/// The per-process limiter held by [`AppState`](crate::AppState).
#[derive(Debug)]
pub struct RateLimiter {
    config: Config,
    buckets: Mutex<HashMap<Key, Bucket>>,
}

impl RateLimiter {
    #[must_use]
    pub fn new(config: Config) -> Self {
        Self {
            config,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Starts a login (or a password change) for `username` from `from`,
    /// counted against both. `username` is `None` for a name no account can
    /// have, which gets no bucket of its own.
    pub fn begin_login(&self, username: Option<&str>, from: IpAddr, now: Instant) -> ApiResult<()> {
        let config = &self.config;
        let user =
            username.map(|name| (Key::Username(name.to_string()), config.login_max_failures));
        let address = (Key::LoginFrom(network(from)), config.ip_max_failures);
        let window = Duration::from_secs(config.login_window_secs);
        self.begin(user.into_iter().chain([address]).collect(), window, now)
    }

    /// The password was right: the username starts over and the address gets
    /// its hit back.
    pub fn login_succeeded(&self, username: &str, from: IpAddr) {
        let mut buckets = self.lock();
        buckets.remove(&Key::Username(username.to_string()));
        if let Some(bucket) = buckets.get_mut(&Key::LoginFrom(network(from))) {
            bucket.hits = bucket.hits.saturating_sub(1);
            if bucket.hits < self.config.ip_max_failures {
                bucket.blocked_until = None;
            }
        }
    }

    pub fn begin_register(&self, from: IpAddr, now: Instant) -> ApiResult<()> {
        let key = Key::RegisterFrom(network(from));
        self.begin(vec![(key, REGISTER_MAX_ATTEMPTS)], REGISTER_WINDOW, now)
    }

    /// `429` while any of the keys is blocked, for the longest wait;
    /// otherwise one hit on each, against its own limit.
    fn begin(&self, keys: Vec<(Key, u32)>, window: Duration, now: Instant) -> ApiResult<()> {
        let mut buckets = self.lock();
        let waits = keys
            .iter()
            .filter_map(|(key, _)| buckets.get(key)?.blocked_for(now));
        if let Some(wait) = waits.max() {
            return Err(too_many(wait));
        }
        for (key, max) in keys {
            hit(&mut buckets, key, max, window, now);
        }
        prune(&mut buckets, now);
        Ok(())
    }

    /// Nothing is left half written under this lock, so a poisoned one is
    /// recovered.
    fn lock(&self) -> MutexGuard<'_, HashMap<Key, Bucket>> {
        self.buckets.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Counts one hit on `key`; the one that reaches `max` blocks the key for
/// `window`. A `max` of zero is no limit at all.
fn hit(buckets: &mut HashMap<Key, Bucket>, key: Key, max: u32, window: Duration, now: Instant) {
    if max == 0 {
        return;
    }
    let bucket = buckets
        .entry(key)
        .or_insert_with(|| Bucket::fresh(now, window));
    if bucket.expired(now) {
        *bucket = Bucket::fresh(now, window);
    }
    bucket.hits = bucket.hits.saturating_add(1);
    if bucket.hits >= max && bucket.blocked_until.is_none() {
        bucket.blocked_until = Some(now.checked_add(window).unwrap_or(now));
    }
}

fn prune(buckets: &mut HashMap<Key, Bucket>, now: Instant) {
    if buckets.len() > PRUNE_ABOVE {
        buckets.retain(|_, bucket| !bucket.expired(now));
    }
}

/// Whole seconds, rounded up, so `Retry-After` never says "now" too early.
fn too_many(wait: Duration) -> ApiError {
    let seconds = wait.as_secs() + u64::from(wait.subsec_nanos() > 0);
    ApiError::too_many_requests(seconds)
}

/// The key of an address: IPv6 clients usually hold a whole /64, so the
/// limit applies to the /64 rather than to each of its addresses.
fn network(ip: IpAddr) -> IpAddr {
    match ip.to_canonical() {
        IpAddr::V6(v6) => {
            let prefix = u128::from(v6) & !((1_u128 << 64) - 1);
            IpAddr::V6(Ipv6Addr::from(prefix))
        }
        v4 => v4,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::net::Ipv4Addr;

    use axum::http::StatusCode;

    use super::*;

    const WINDOW: Duration = Duration::from_secs(900);
    const HOME: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
    const AWAY: IpAddr = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 7));

    fn limiter(login: u32, ip: u32) -> RateLimiter {
        RateLimiter::new(Config {
            login_max_failures: login,
            ip_max_failures: ip,
            login_window_secs: WINDOW.as_secs(),
            ..Config::default()
        })
    }

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn a_username_locks_on_the_last_allowed_failure_for_a_whole_window() {
        let limiter = limiter(3, 100);
        let t0 = Instant::now();
        for i in 0..3 {
            assert!(
                limiter
                    .begin_login(Some("alice"), HOME, t0 + secs(i))
                    .is_ok()
            );
        }
        let err = limiter
            .begin_login(Some("alice"), AWAY, t0 + secs(3))
            .expect_err("locked");
        assert_eq!(err.status, StatusCode::TOO_MANY_REQUESTS);
        // Locked at t0 + 2s for 900s: 899s left at t0 + 3s.
        assert_eq!(err.retry_after, Some(899));
        // Somebody else is not affected.
        assert!(limiter.begin_login(Some("bob"), HOME, t0 + secs(3)).is_ok());
        // The last moment of the lock still says a whole second.
        let last = t0 + secs(2) + WINDOW - Duration::from_millis(1);
        let err = limiter
            .begin_login(Some("alice"), HOME, last)
            .expect_err("locked");
        assert_eq!(err.retry_after, Some(1));
        // Past the lock the username starts over.
        assert!(
            limiter
                .begin_login(Some("alice"), HOME, t0 + secs(2) + WINDOW)
                .is_ok()
        );
    }

    #[test]
    fn failures_spread_over_more_than_a_window_never_lock() {
        let limiter = limiter(3, 100);
        let t0 = Instant::now();
        for i in 0..10 {
            let now = t0 + secs(i * 500);
            assert!(limiter.begin_login(Some("alice"), HOME, now).is_ok(), "{i}");
        }
    }

    #[test]
    fn a_success_clears_the_username_and_gives_the_hit_back() {
        let limiter = limiter(3, 3);
        let t0 = Instant::now();
        for _ in 0..2 {
            assert!(limiter.begin_login(Some("alice"), HOME, t0).is_ok());
        }
        // The third attempt reaches both limits, and succeeds.
        assert!(limiter.begin_login(Some("alice"), HOME, t0).is_ok());
        limiter.login_succeeded("alice", HOME);
        assert!(limiter.begin_login(Some("alice"), HOME, t0).is_ok());
        // Two failures from HOME remain on the address, plus this one.
        assert!(limiter.begin_login(Some("bob"), HOME, t0).is_err());
    }

    #[test]
    fn an_address_is_limited_across_usernames() {
        let limiter = limiter(100, 3);
        let t0 = Instant::now();
        for name in ["a1", "a2", "a3"] {
            assert!(limiter.begin_login(Some(name), HOME, t0).is_ok());
        }
        assert!(limiter.begin_login(Some("a4"), HOME, t0).is_err());
        assert!(limiter.begin_login(None, HOME, t0).is_err());
        assert!(limiter.begin_login(Some("a4"), AWAY, t0).is_ok());
    }

    #[test]
    fn an_ipv6_client_is_limited_by_its_64() {
        let limiter = limiter(100, 2);
        let t0 = Instant::now();
        let one: IpAddr = "2001:db8:1:2::1".parse().unwrap_or(HOME);
        let two: IpAddr = "2001:db8:1:2:ffff::9".parse().unwrap_or(HOME);
        let other: IpAddr = "2001:db8:1:3::1".parse().unwrap_or(HOME);
        assert!(limiter.begin_login(None, one, t0).is_ok());
        assert!(limiter.begin_login(None, two, t0).is_ok());
        assert!(limiter.begin_login(None, one, t0).is_err());
        assert!(limiter.begin_login(None, other, t0).is_ok());
    }

    #[test]
    fn registrations_are_counted_per_address() {
        let limiter = limiter(5, 30);
        let t0 = Instant::now();
        for _ in 0..REGISTER_MAX_ATTEMPTS {
            assert!(limiter.begin_register(HOME, t0).is_ok());
        }
        let err = limiter.begin_register(HOME, t0).expect_err("limited");
        assert_eq!(err.retry_after, Some(3_600));
        assert!(limiter.begin_register(AWAY, t0).is_ok());
        assert!(limiter.begin_register(HOME, t0 + secs(3_600)).is_ok());
    }

    #[test]
    fn a_zero_limit_is_no_limit() {
        let limiter = limiter(0, 0);
        let t0 = Instant::now();
        for _ in 0..50 {
            assert!(limiter.begin_login(Some("alice"), HOME, t0).is_ok());
        }
        assert!(limiter.lock().is_empty());
    }

    #[test]
    fn expired_buckets_are_pruned_past_the_threshold() {
        let limiter = limiter(5, 0);
        let t0 = Instant::now();
        for i in 0..=PRUNE_ABOVE {
            let name = format!("user{i}");
            assert!(limiter.begin_login(Some(&name), HOME, t0).is_ok());
        }
        // Exactly one over: the next insert prunes, but nothing has expired.
        assert!(limiter.begin_login(Some("late"), HOME, t0).is_ok());
        assert_eq!(limiter.lock().len(), PRUNE_ABOVE + 2);
        // A window later every old bucket is gone.
        assert!(
            limiter
                .begin_login(Some("later"), HOME, t0 + WINDOW)
                .is_ok()
        );
        assert_eq!(limiter.lock().len(), 1);
    }
}
