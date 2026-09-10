//! Shared state: the two databases, the configuration and the hasher.
//!
//! SQLite has a single writer, so both handles sit behind a `Mutex` and every
//! piece of work that touches them runs on a blocking task through
//! [`AppState::run`]. When both locks are needed the core one is always taken
//! first.

use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use argon2::{Algorithm, Argon2, Params, Version};
use sparagne_core::Core;

use crate::{
    config::Config,
    db::ServerDb,
    error::{ApiError, ApiResult},
};

const VAULTS_DB: &str = "vaults.sqlite";
const SERVER_DB: &str = "server.sqlite";

struct Inner {
    core: Mutex<Core>,
    db: Mutex<ServerDb>,
    config: Config,
    argon: Argon2<'static>,
}

/// Handle passed to every route.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("config", &self.inner.config)
            .finish_non_exhaustive()
    }
}

impl AppState {
    /// Opens `vaults.sqlite` and `server.sqlite` under `data_dir`, creating
    /// the directory when missing.
    pub fn open(data_dir: impl AsRef<Path>, config: Config) -> ApiResult<Self> {
        let dir = data_dir.as_ref();
        std::fs::create_dir_all(dir).map_err(|err| ApiError::internal(err.to_string()))?;
        let core = Core::open(dir.join(VAULTS_DB))?;
        let db = ServerDb::open(dir.join(SERVER_DB))?;
        Ok(Self::build(core, db, config, Argon2::default()))
    }

    /// Both databases in memory. For tests: the password hashing parameters
    /// are the cheapest the algorithm allows, so they are not usable for
    /// anything durable.
    pub fn in_memory(config: Config) -> ApiResult<Self> {
        let core = Core::open_in_memory()?;
        let db = ServerDb::open_in_memory()?;
        let params =
            Params::new(Params::MIN_M_COST, Params::MIN_T_COST, 1, None).unwrap_or(Params::DEFAULT);
        let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        Ok(Self::build(core, db, config, argon))
    }

    fn build(core: Core, db: ServerDb, config: Config, argon: Argon2<'static>) -> Self {
        Self {
            inner: Arc::new(Inner {
                core: Mutex::new(core),
                db: Mutex::new(db),
                config,
                argon,
            }),
        }
    }

    #[must_use]
    pub fn config(&self) -> &Config {
        &self.inner.config
    }

    #[must_use]
    pub fn argon(&self) -> &Argon2<'static> {
        &self.inner.argon
    }

    /// The vault log and projection. A poisoned lock is recovered: every
    /// core call is transactional, so nothing is left half written.
    pub fn core(&self) -> MutexGuard<'_, Core> {
        self.inner
            .core
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Accounts, tokens and memberships.
    pub fn db(&self) -> MutexGuard<'_, ServerDb> {
        self.inner.db.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs the blocking part of a request off the async runtime.
    pub async fn run<T, F>(&self, work: F) -> ApiResult<T>
    where
        F: FnOnce(&Self) -> ApiResult<T> + Send + 'static,
        T: Send + 'static,
    {
        let state = self.clone();
        match tokio::task::spawn_blocking(move || work(&state)).await {
            Ok(result) => result,
            Err(err) => Err(ApiError::internal(err.to_string())),
        }
    }
}
