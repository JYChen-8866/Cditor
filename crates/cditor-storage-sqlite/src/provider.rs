use std::sync::Arc;

use async_trait::async_trait;
use cditor_storage::{DocumentStorage, StorageProvider, StorageResult};

use crate::{SqliteDocumentStorage, SqliteStorageOptions};

#[derive(Debug, Clone)]
pub struct SqliteStorageProvider {
    options: SqliteStorageOptions,
}

impl SqliteStorageProvider {
    pub fn new(options: SqliteStorageOptions) -> Self {
        Self { options }
    }

    pub fn file(path: impl Into<std::path::PathBuf>) -> Self {
        Self::new(SqliteStorageOptions::file(path))
    }

    pub fn options(&self) -> &SqliteStorageOptions {
        &self.options
    }
}

#[async_trait]
impl StorageProvider for SqliteStorageProvider {
    fn label(&self) -> &str {
        "SQLite"
    }

    async fn open(&self) -> StorageResult<Arc<dyn DocumentStorage>> {
        Ok(Arc::new(
            SqliteDocumentStorage::open(self.options.clone()).await?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_provider_preserves_options_and_contract_metadata() {
        let provider = SqliteStorageProvider::file("workspace.cditor.db");

        assert_eq!(provider.label(), "SQLite");
        assert_eq!(
            provider.options().path(),
            std::path::Path::new("workspace.cditor.db")
        );
        assert_eq!(provider.open_timeout(), std::time::Duration::from_secs(90));
    }
}
