//! UUIDv7 持久 ID 与 typed newtype（P1-001，ADR-006）。
//!
//! `PersistentId` 是 128 位 UUIDv7：字节序即时间序，支持离线生成。每类实体
//! 一个 newtype，禁止裸 `Uuid`/`PersistentId` 跨实体边界混用。JSON 序列化为
//! 标准 hyphenated 字符串，二进制存储使用 16 字节 [`PersistentId::into_bytes`]。

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 128 位持久 ID（UUIDv7）。
///
/// `Ord` 按 128 位字节序比较；对 UUIDv7 而言即创建时间序（同毫秒内按
/// 单调计数）。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(transparent)]
pub struct PersistentId(Uuid);

impl PersistentId {
    /// 全零 nil ID，仅用于占位/默认值，禁止作为真实实体身份持久化。
    pub const NIL: Self = Self(Uuid::nil());

    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    pub const fn into_bytes(self) -> [u8; 16] {
        self.0.into_bytes()
    }

    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(Uuid::from_bytes(bytes))
    }

    pub const fn is_nil(&self) -> bool {
        self.0.is_nil()
    }

    /// UUIDv7 内嵌的 Unix 毫秒时间戳；非 v7 布局返回 `None`。
    pub fn unix_millis(&self) -> Option<u64> {
        if self.0.get_version_num() != 7 {
            return None;
        }
        let bytes = self.0.as_bytes();
        let mut millis = 0u64;
        for byte in &bytes[0..6] {
            millis = (millis << 8) | u64::from(*byte);
        }
        Some(millis)
    }
}

impl fmt::Display for PersistentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.hyphenated().fmt(formatter)
    }
}

impl FromStr for PersistentId {
    type Err = uuid::Error;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(text)?))
    }
}

macro_rules! typed_persistent_id {
    ($(#[$doc:meta] $name:ident),+ $(,)?) => {
        $(
            #[$doc]
            #[derive(
                Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
                Serialize, Deserialize, Default,
            )]
            #[serde(transparent)]
            pub struct $name(pub PersistentId);

            impl $name {
                pub const NIL: Self = Self(PersistentId::NIL);

                pub const fn new(id: PersistentId) -> Self {
                    Self(id)
                }

                pub const fn id(&self) -> PersistentId {
                    self.0
                }

                pub const fn is_nil(&self) -> bool {
                    self.0.is_nil()
                }
            }

            impl fmt::Display for $name {
                fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                    self.0.fmt(formatter)
                }
            }

            impl From<PersistentId> for $name {
                fn from(id: PersistentId) -> Self {
                    Self(id)
                }
            }

            impl From<$name> for PersistentId {
                fn from(typed: $name) -> Self {
                    typed.0
                }
            }
        )+
    };
}

typed_persistent_id! {
    /// Workspace 持久 ID。
    WorkspaceUid,
    /// Document 持久 ID（区别于 Runtime 热路径的 `crate::ids::DocumentId`）。
    DocumentUid,
    /// Block 持久 ID（区别于 Runtime 热路径的 `crate::ids::BlockId`）。
    BlockUid,
    /// TextSurface 持久 ID。
    SurfaceUid,
    /// 表格行持久 ID。
    RowUid,
    /// 表格列持久 ID。
    ColumnUid,
    /// Collection 持久 ID。
    CollectionUid,
    /// Collection property 持久 ID。
    PropertyUid,
    /// Collection view 持久 ID。
    ViewUid,
    /// Operation 持久 ID。
    OperationUid,
    /// Actor（用户/自动化主体）持久 ID。
    ActorUid,
    /// Device 持久 ID。
    DeviceUid,
    /// Asset 持久 ID。
    AssetUid,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v7_with_millis(millis: u64, tail: u8) -> PersistentId {
        let random = [tail; 10];
        PersistentId::from_uuid(
            uuid::Builder::from_unix_timestamp_millis(millis, &random).into_uuid(),
        )
    }

    #[test]
    fn ordering_follows_embedded_timestamp() {
        let earlier = v7_with_millis(1_000, 0xFF);
        let later = v7_with_millis(1_001, 0x00);
        assert!(earlier < later);
        assert_eq!(earlier.unix_millis(), Some(1_000));
        assert_eq!(later.unix_millis(), Some(1_001));
    }

    #[test]
    fn json_round_trips_as_hyphenated_string() {
        let id = v7_with_millis(42, 7);
        let json = serde_json::to_string(&id).expect("serialize");
        assert!(json.contains('-'));
        let back: PersistentId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, id);
        assert_eq!(id.to_string().parse::<PersistentId>().unwrap(), id);
    }

    #[test]
    fn byte_round_trip_is_lossless() {
        let id = v7_with_millis(u64::from(u32::MAX) + 17, 0xA5);
        assert_eq!(PersistentId::from_bytes(id.into_bytes()), id);
    }

    #[test]
    fn typed_ids_do_not_compare_across_types() {
        let raw = v7_with_millis(9, 1);
        let block = BlockUid::new(raw);
        let document = DocumentUid::new(raw);
        // 类型系统禁止直接比较；这里只验证互转保持原值。
        assert_eq!(PersistentId::from(block), PersistentId::from(document));
        assert_eq!(block.id(), raw);
    }

    #[test]
    fn nil_is_flagged_and_non_v7_has_no_timestamp() {
        assert!(PersistentId::NIL.is_nil());
        assert!(BlockUid::NIL.is_nil());
        assert_eq!(PersistentId::NIL.unix_millis(), None);
        let v4 = PersistentId::from_uuid(Uuid::from_bytes([0x42; 16]));
        assert_eq!(v4.unix_millis(), None);
    }
}
