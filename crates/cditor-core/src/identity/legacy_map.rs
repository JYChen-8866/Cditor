//! legacy `u64` -> PersistentId 迁移映射表（P1-003，ADR-006）。
//!
//! 迁移时为每个既有实体生成一次 UUIDv7 并固化映射；表保持双向索引与冲突
//! 拒绝，序列化为按 legacy id 排序的确定性 pair 列表，便于 dry-run 之间
//! 比较 checksum（P1-013 输入）。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::persistent_id::PersistentId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyIdMapError {
    /// 同一 legacy id 已映射到不同持久 ID。
    LegacyConflict {
        legacy: u64,
        existing: PersistentId,
        incoming: PersistentId,
    },
    /// 同一持久 ID 已被其他 legacy id 占用。
    PersistentConflict {
        persistent: PersistentId,
        existing: u64,
        incoming: u64,
    },
}

impl std::fmt::Display for LegacyIdMapError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LegacyConflict {
                legacy,
                existing,
                incoming,
            } => write!(
                formatter,
                "legacy id {legacy} already maps to {existing}, refusing {incoming}"
            ),
            Self::PersistentConflict {
                persistent,
                existing,
                incoming,
            } => write!(
                formatter,
                "persistent id {persistent} already claimed by legacy {existing}, refusing {incoming}"
            ),
        }
    }
}

impl std::error::Error for LegacyIdMapError {}

/// 双向 legacy 映射表。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LegacyIdMap {
    forward: HashMap<u64, PersistentId>,
    reverse: HashMap<PersistentId, u64>,
}

impl LegacyIdMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记一条映射；重复登记同一对是幂等，冲突则拒绝。
    pub fn insert(
        &mut self,
        legacy: u64,
        persistent: PersistentId,
    ) -> Result<(), LegacyIdMapError> {
        if let Some(existing) = self.forward.get(&legacy) {
            if *existing == persistent {
                return Ok(());
            }
            return Err(LegacyIdMapError::LegacyConflict {
                legacy,
                existing: *existing,
                incoming: persistent,
            });
        }
        if let Some(existing) = self.reverse.get(&persistent) {
            return Err(LegacyIdMapError::PersistentConflict {
                persistent,
                existing: *existing,
                incoming: legacy,
            });
        }
        self.forward.insert(legacy, persistent);
        self.reverse.insert(persistent, legacy);
        Ok(())
    }

    pub fn persistent_of(&self, legacy: u64) -> Option<PersistentId> {
        self.forward.get(&legacy).copied()
    }

    pub fn legacy_of(&self, persistent: PersistentId) -> Option<u64> {
        self.reverse.get(&persistent).copied()
    }

    pub fn len(&self) -> usize {
        self.forward.len()
    }

    pub fn is_empty(&self) -> bool {
        self.forward.is_empty()
    }

    /// 按 legacy id 排序的确定性导出，用于持久化与 checksum。
    pub fn to_sorted_pairs(&self) -> Vec<LegacyIdPair> {
        let mut pairs: Vec<_> = self
            .forward
            .iter()
            .map(|(legacy, persistent)| LegacyIdPair {
                legacy: *legacy,
                persistent: *persistent,
            })
            .collect();
        pairs.sort_by_key(|pair| pair.legacy);
        pairs
    }

    /// 从导出的 pair 列表重建；冲突返回错误。
    pub fn from_pairs(
        pairs: impl IntoIterator<Item = LegacyIdPair>,
    ) -> Result<Self, LegacyIdMapError> {
        let mut map = Self::new();
        for pair in pairs {
            map.insert(pair.legacy, pair.persistent)?;
        }
        Ok(map)
    }
}

/// 序列化单元：一条 legacy -> persistent 映射。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyIdPair {
    pub legacy: u64,
    pub persistent: PersistentId,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pid(byte: u8) -> PersistentId {
        PersistentId::from_bytes([byte; 16])
    }

    #[test]
    fn insert_lookup_and_idempotent_reinsert() {
        let mut map = LegacyIdMap::new();
        map.insert(1, pid(0xA)).unwrap();
        map.insert(2, pid(0xB)).unwrap();
        map.insert(1, pid(0xA)).unwrap(); // 幂等

        assert_eq!(map.persistent_of(1), Some(pid(0xA)));
        assert_eq!(map.legacy_of(pid(0xB)), Some(2));
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn conflicting_mappings_are_rejected() {
        let mut map = LegacyIdMap::new();
        map.insert(1, pid(0xA)).unwrap();

        assert_eq!(
            map.insert(1, pid(0xB)),
            Err(LegacyIdMapError::LegacyConflict {
                legacy: 1,
                existing: pid(0xA),
                incoming: pid(0xB),
            })
        );
        assert_eq!(
            map.insert(2, pid(0xA)),
            Err(LegacyIdMapError::PersistentConflict {
                persistent: pid(0xA),
                existing: 1,
                incoming: 2,
            })
        );
        // 拒绝后原映射保持不变。
        assert_eq!(map.persistent_of(1), Some(pid(0xA)));
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn sorted_pairs_round_trip_deterministically() {
        let mut map = LegacyIdMap::new();
        for legacy in [9u64, 3, 7, 1] {
            map.insert(legacy, pid(legacy as u8)).unwrap();
        }

        let pairs = map.to_sorted_pairs();
        assert_eq!(
            pairs.iter().map(|pair| pair.legacy).collect::<Vec<_>>(),
            [1, 3, 7, 9]
        );

        let json = serde_json::to_string(&pairs).expect("serialize");
        let parsed: Vec<LegacyIdPair> = serde_json::from_str(&json).expect("deserialize");
        let rebuilt = LegacyIdMap::from_pairs(parsed).expect("rebuild");
        assert_eq!(rebuilt, map);
    }

    #[test]
    fn from_pairs_rejects_corrupted_export() {
        let pairs = vec![
            LegacyIdPair {
                legacy: 1,
                persistent: pid(0xA),
            },
            LegacyIdPair {
                legacy: 1,
                persistent: pid(0xB),
            },
        ];
        assert!(LegacyIdMap::from_pairs(pairs).is_err());
    }
}
