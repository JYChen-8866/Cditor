//! 独立 schema 版本与兼容模型（P1-007/P1-011，总设计 6.3）。
//!
//! 七个域各自独立演进版本：document format、block payload、operation、
//! clipboard、plugin manifest、SQLite schema、PostgreSQL schema。规则：
//!
//! - writer 一律写当前版本；
//! - reader 接受同 major 的旧 minor（直接读）；
//! - 同 major 的新 minor：best-effort 读，重写时必须保留未知字段；
//! - 新 major：本版本无法安全写，进入只读兼容模式（P1-011）；
//! - 旧 major：需要显式 migrator 升级后才可用。

pub mod envelope;
pub mod registry;

use std::fmt;

use serde::{Deserialize, Serialize};

pub use envelope::{DecodeOutcome, EnvelopeError, VersionedEnvelope};
pub use registry::{BlockCapabilities, BlockDescriptor, BlockRegistry, PayloadMigrator};

/// `major.minor` schema 版本。major 破坏性、minor 向后兼容新增。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SchemaVersion {
    pub major: u32,
    pub minor: u32,
}

impl SchemaVersion {
    pub const fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }
}

impl fmt::Display for SchemaVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}", self.major, self.minor)
    }
}

/// 独立演进的 schema 域。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaDomain {
    DocumentFormat,
    BlockPayload,
    Operation,
    Clipboard,
    PluginManifest,
    SqliteSchema,
    PostgresSchema,
}

impl SchemaDomain {
    /// 该域当前的 writer 版本。
    pub const fn current_version(self) -> SchemaVersion {
        match self {
            Self::DocumentFormat => CURRENT_DOCUMENT_FORMAT,
            Self::BlockPayload => CURRENT_BLOCK_PAYLOAD,
            Self::Operation => CURRENT_OPERATION,
            Self::Clipboard => CURRENT_CLIPBOARD,
            Self::PluginManifest => CURRENT_PLUGIN_MANIFEST,
            Self::SqliteSchema => CURRENT_SQLITE_SCHEMA,
            Self::PostgresSchema => CURRENT_POSTGRES_SCHEMA,
        }
    }
}

/// document format：对应既有 `RichTextFormatVersion = 1`，以 1.0 起步。
pub const CURRENT_DOCUMENT_FORMAT: SchemaVersion = SchemaVersion::new(1, 0);
pub const CURRENT_BLOCK_PAYLOAD: SchemaVersion = SchemaVersion::new(1, 0);
pub const CURRENT_OPERATION: SchemaVersion = SchemaVersion::new(1, 1);
pub const CURRENT_CLIPBOARD: SchemaVersion = SchemaVersion::new(1, 0);
pub const CURRENT_PLUGIN_MANIFEST: SchemaVersion = SchemaVersion::new(1, 0);
pub const CURRENT_SQLITE_SCHEMA: SchemaVersion = SchemaVersion::new(1, 0);
pub const CURRENT_POSTGRES_SCHEMA: SchemaVersion = SchemaVersion::new(1, 0);

/// reader 对某个已写入版本的处置策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadPolicy {
    /// 同版本或旧 minor：正常读写。
    ReadWrite,
    /// 新 minor：可读，重写必须保留未知字段。
    ReadWritePreservingUnknown,
    /// 新 major：只读兼容模式，禁止写（P1-011）。
    ReadOnlyNewerMajor,
    /// 旧 major：必须先经 migrator 升级。
    NeedsMigration,
}

impl ReadPolicy {
    /// 评估以 `current` reader 读 `written` 数据的策略。
    pub fn assess(written: SchemaVersion, current: SchemaVersion) -> Self {
        if written.major > current.major {
            Self::ReadOnlyNewerMajor
        } else if written.major < current.major {
            Self::NeedsMigration
        } else if written.minor > current.minor {
            Self::ReadWritePreservingUnknown
        } else {
            Self::ReadWrite
        }
    }

    pub const fn allows_write(self) -> bool {
        matches!(self, Self::ReadWrite | Self::ReadWritePreservingUnknown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_policy_matrix() {
        let current = SchemaVersion::new(2, 3);
        assert_eq!(
            ReadPolicy::assess(SchemaVersion::new(2, 3), current),
            ReadPolicy::ReadWrite
        );
        assert_eq!(
            ReadPolicy::assess(SchemaVersion::new(2, 0), current),
            ReadPolicy::ReadWrite
        );
        assert_eq!(
            ReadPolicy::assess(SchemaVersion::new(2, 4), current),
            ReadPolicy::ReadWritePreservingUnknown
        );
        assert_eq!(
            ReadPolicy::assess(SchemaVersion::new(3, 0), current),
            ReadPolicy::ReadOnlyNewerMajor
        );
        assert_eq!(
            ReadPolicy::assess(SchemaVersion::new(1, 9), current),
            ReadPolicy::NeedsMigration
        );
    }

    #[test]
    fn write_permission_follows_policy() {
        assert!(ReadPolicy::ReadWrite.allows_write());
        assert!(ReadPolicy::ReadWritePreservingUnknown.allows_write());
        assert!(!ReadPolicy::ReadOnlyNewerMajor.allows_write());
        assert!(!ReadPolicy::NeedsMigration.allows_write());
    }

    #[test]
    fn domains_expose_independent_current_versions() {
        // 七个域独立存在且序列化 tag 稳定。
        let domains = [
            SchemaDomain::DocumentFormat,
            SchemaDomain::BlockPayload,
            SchemaDomain::Operation,
            SchemaDomain::Clipboard,
            SchemaDomain::PluginManifest,
            SchemaDomain::SqliteSchema,
            SchemaDomain::PostgresSchema,
        ];
        let tags: Vec<_> = domains
            .iter()
            .map(|domain| serde_json::to_value(domain).unwrap())
            .collect();
        assert_eq!(
            tags,
            [
                "document_format",
                "block_payload",
                "operation",
                "clipboard",
                "plugin_manifest",
                "sqlite_schema",
                "postgres_schema",
            ]
        );
        for domain in domains {
            assert_eq!(domain.current_version().major, 1);
        }
    }

    #[test]
    fn version_ordering_is_major_then_minor() {
        assert!(SchemaVersion::new(1, 9) < SchemaVersion::new(2, 0));
        assert!(SchemaVersion::new(2, 0) < SchemaVersion::new(2, 1));
    }
}
