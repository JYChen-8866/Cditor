//! 持久身份体系（Phase 1，ADR-006）。
//!
//! - [`persistent_id`]：UUIDv7 `PersistentId` 与各实体 typed ID。
//! - [`generator`]：RFC 9562 Method 3 单调生成器，时钟/熵源可注入。
//! - [`arena`]：`RuntimeHandle(u64)` 与 PersistentId 的双向 arena，隔离热路径。
//! - [`legacy_map`]：legacy `u64` -> PersistentId 迁移映射表。
//! - [`order_key`]：base-256 fractional 顺序键与局部 rebalance。
//!
//! 现有 `crate::ids` 中的 `u64` 别名仍是 Runtime 热路径 handle；持久层、
//! 网络协议和多设备语义必须使用本模块的 typed ID（总设计 6.1）。

pub mod arena;
pub mod generator;
pub mod legacy_map;
pub mod order_key;
pub mod persistent_id;

pub use arena::{IdArena, RuntimeHandle};
pub use generator::{IdClock, IdEntropy, OsEntropy, PersistentIdGenerator, SystemClock};
pub use legacy_map::{LegacyIdMap, LegacyIdMapError};
pub use order_key::{OrderKey, OrderKeyError, rebalanced_keys};
pub use persistent_id::{
    ActorUid, AssetUid, BlockUid, CollectionUid, ColumnUid, DeviceUid, DocumentUid, OperationUid,
    PersistentId, PropertyUid, RowUid, SurfaceUid, ViewUid, WorkspaceUid,
};
