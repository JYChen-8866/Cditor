use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqliteDurability {
    Full,
    Balanced,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteStorageOptions {
    pub path: PathBuf,
    pub create_if_missing: bool,
    pub durability: SqliteDurability,
    pub busy_timeout: Duration,
    pub max_connections: u32,
}

impl SqliteStorageOptions {
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            create_if_missing: true,
            durability: SqliteDurability::Full,
            busy_timeout: Duration::from_secs(5),
            max_connections: 4,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn create_if_missing(mut self, create: bool) -> Self {
        self.create_if_missing = create;
        self
    }

    pub fn durability(mut self, durability: SqliteDurability) -> Self {
        self.durability = durability;
        self
    }

    pub fn busy_timeout(mut self, timeout: Duration) -> Self {
        self.busy_timeout = timeout;
        self
    }

    pub fn max_connections(mut self, max_connections: u32) -> Self {
        self.max_connections = max_connections.clamp(1, 8);
        self
    }
}
