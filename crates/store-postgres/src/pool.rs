use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};

use super::error::{PostgresStorageError, PostgresStorageResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostgresPoolConfig {
    pub database_url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout: Duration,
    pub idle_timeout: Option<Duration>,
    pub max_lifetime: Option<Duration>,
    pub require_ssl: bool,
}

impl PostgresPoolConfig {
    pub fn new(database_url: impl Into<String>) -> Self {
        Self {
            database_url: database_url.into(),
            max_connections: 8,
            min_connections: 1,
            acquire_timeout: Duration::from_secs(15),
            idle_timeout: Some(Duration::from_secs(10 * 60)),
            max_lifetime: Some(Duration::from_secs(30 * 60)),
            require_ssl: false,
        }
    }

    pub fn for_tests(database_url: impl Into<String>) -> Self {
        Self {
            database_url: database_url.into(),
            max_connections: 2,
            min_connections: 0,
            acquire_timeout: Duration::from_secs(2),
            idle_timeout: Some(Duration::from_secs(60)),
            max_lifetime: Some(Duration::from_secs(5 * 60)),
            require_ssl: false,
        }
    }
}

pub async fn create_pg_pool(config: &PostgresPoolConfig) -> PostgresStorageResult<PgPool> {
    let mut connect_options = config.database_url.parse::<PgConnectOptions>()?;
    if config.require_ssl {
        connect_options = connect_options.ssl_mode(PgSslMode::Require);
    }

    let connect = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(config.acquire_timeout)
        .idle_timeout(config.idle_timeout)
        .max_lifetime(config.max_lifetime)
        .connect_with(connect_options);
    let pool = tokio::time::timeout(config.acquire_timeout, connect)
        .await
        .map_err(|_| PostgresStorageError::Timeout {
            operation: "PostgreSQL connection",
            timeout: config.acquire_timeout,
        })??;

    Ok(pool)
}

pub async fn health_check(pool: &PgPool) -> PostgresStorageResult<()> {
    sqlx::query("SELECT 1").execute(pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_pool_config_defaults_are_safe_for_app_startup() {
        let config = PostgresPoolConfig::new("postgres://user:pass@localhost/cditor");

        assert_eq!(config.max_connections, 8);
        assert_eq!(config.min_connections, 1);
        assert_eq!(config.acquire_timeout, Duration::from_secs(15));
        assert!(!config.require_ssl);
    }

    #[test]
    fn postgres_test_pool_config_uses_small_pool() {
        let config = PostgresPoolConfig::for_tests("postgres://user:pass@localhost/cditor_test");

        assert_eq!(config.max_connections, 2);
        assert_eq!(config.min_connections, 0);
        assert_eq!(config.acquire_timeout, Duration::from_secs(2));
    }
}
