use std::collections::HashMap;
use std::ops::Range;

use cditor_core::ids::{BlockId, DocumentId};
use cditor_core::layout::{HeightConfidence, HeightEstimate};
use cditor_core::version::StructureVersion;

pub const BLOCK_LAYOUT_TABLE_SQL: &str = r#"CREATE TABLE block_layout (
    block_id TEXT NOT NULL,
    layout_key_hash TEXT NOT NULL,
    width_bucket INTEGER NOT NULL,
    exact_width REAL,
    content_version INTEGER NOT NULL,
    attrs_version INTEGER NOT NULL DEFAULT 0,
    style_version INTEGER NOT NULL DEFAULT 0,
    font_version INTEGER NOT NULL DEFAULT 0,
    theme_version INTEGER NOT NULL DEFAULT 0,
    scale_factor REAL NOT NULL DEFAULT 1.0,
    measured_height REAL,
    estimated_height REAL NOT NULL,
    confidence INTEGER NOT NULL DEFAULT 0,
    max_error_hint REAL NOT NULL DEFAULT 0,
    line_count INTEGER,
    layout_cost INTEGER NOT NULL DEFAULT 0,
    measured_at INTEGER,
    PRIMARY KEY (block_id, layout_key_hash)
);"#;

pub const PAGE_LAYOUT_TABLE_SQL: &str = r#"CREATE TABLE page_layout (
    document_id TEXT NOT NULL,
    visible_index_version INTEGER NOT NULL DEFAULT 0,
    structure_version INTEGER NOT NULL,
    layout_key_hash TEXT NOT NULL,
    page_policy_version INTEGER NOT NULL,
    page_index INTEGER NOT NULL,
    block_start_index INTEGER NOT NULL,
    block_count INTEGER NOT NULL,
    first_block_id TEXT,
    last_block_id TEXT,
    height REAL NOT NULL,
    measured_ratio REAL NOT NULL DEFAULT 0,
    confidence INTEGER NOT NULL DEFAULT 0,
    max_error_hint REAL NOT NULL DEFAULT 0,
    dirty INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (
        document_id,
        visible_index_version,
        structure_version,
        layout_key_hash,
        page_policy_version,
        page_index
    )
);"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayoutCacheKey {
    pub width_bucket: u16,
    pub exact_width_px: u32,
    pub content_version: u64,
    pub attrs_version: u64,
    pub style_version: u64,
    pub font_version: u64,
    pub theme_version: u64,
    pub scale_factor_milli: u32,
}

impl LayoutCacheKey {
    pub fn hash_key(&self) -> String {
        format!(
            "w{}-x{}-c{}-a{}-s{}-f{}-t{}-z{}",
            self.width_bucket,
            self.exact_width_px,
            self.content_version,
            self.attrs_version,
            self.style_version,
            self.font_version,
            self.theme_version,
            self.scale_factor_milli
        )
    }

    pub fn same_exact_layout(&self, other: &Self) -> bool {
        self == other
    }

    pub fn same_historical_bucket(&self, other: &Self) -> bool {
        self.width_bucket == other.width_bucket
            && self.font_version == other.font_version
            && self.theme_version == other.theme_version
            && self.scale_factor_milli == other.scale_factor_milli
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockLayoutRow {
    pub block_id: BlockId,
    pub layout_key_hash: String,
    pub width_bucket: u16,
    pub exact_width_px: u32,
    pub content_version: u64,
    pub attrs_version: u64,
    pub style_version: u64,
    pub font_version: u64,
    pub theme_version: u64,
    pub scale_factor_milli: u32,
    pub measured_height: Option<f64>,
    pub estimated_height: f64,
    pub confidence: HeightConfidence,
    pub max_error_hint: f64,
    pub line_count: Option<u32>,
    pub layout_cost: u32,
    pub measured_at: Option<u64>,
}

impl BlockLayoutRow {
    pub fn new(block_id: BlockId, key: LayoutCacheKey, estimate: HeightEstimate) -> Self {
        Self {
            block_id,
            layout_key_hash: key.hash_key(),
            width_bucket: key.width_bucket,
            exact_width_px: key.exact_width_px,
            content_version: key.content_version,
            attrs_version: key.attrs_version,
            style_version: key.style_version,
            font_version: key.font_version,
            theme_version: key.theme_version,
            scale_factor_milli: key.scale_factor_milli,
            measured_height: (estimate.confidence == HeightConfidence::Exact)
                .then_some(estimate.height),
            estimated_height: estimate.height,
            confidence: estimate.confidence,
            max_error_hint: estimate.max_error_hint,
            line_count: None,
            layout_cost: 0,
            measured_at: None,
        }
    }

    pub fn key(&self) -> LayoutCacheKey {
        LayoutCacheKey {
            width_bucket: self.width_bucket,
            exact_width_px: self.exact_width_px,
            content_version: self.content_version,
            attrs_version: self.attrs_version,
            style_version: self.style_version,
            font_version: self.font_version,
            theme_version: self.theme_version,
            scale_factor_milli: self.scale_factor_milli,
        }
    }

    pub fn load_for_key(&self, requested: LayoutCacheKey) -> CachedHeight {
        let stored = self.key();
        if stored.same_exact_layout(&requested) {
            return CachedHeight {
                height: self.measured_height.unwrap_or(self.estimated_height),
                confidence: self.confidence,
                source: CacheSource::ExactMatch,
                max_error_hint: self.max_error_hint,
            };
        }

        let confidence = if stored.same_historical_bucket(&requested) {
            HeightConfidence::Historical
        } else {
            HeightConfidence::Default
        };
        CachedHeight {
            height: self.estimated_height,
            confidence,
            source: CacheSource::VersionMismatch,
            max_error_hint: self.max_error_hint.max(8.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageLayoutRow {
    pub document_id: DocumentId,
    pub visible_index_version: u64,
    pub structure_version: StructureVersion,
    pub layout_key_hash: String,
    pub page_policy_version: u64,
    pub page_index: usize,
    pub block_start_index: usize,
    pub block_count: usize,
    pub first_block_id: Option<BlockId>,
    pub last_block_id: Option<BlockId>,
    pub height: f64,
    pub measured_ratio: f64,
    pub confidence: HeightConfidence,
    pub max_error_hint: f64,
    pub dirty: bool,
    pub updated_at: u64,
}

impl PageLayoutRow {
    pub fn page_block_range(&self) -> Range<usize> {
        self.block_start_index..self.block_start_index + self.block_count
    }

    pub fn load_for_context(
        &self,
        structure_version: StructureVersion,
        layout_key_hash: &str,
        page_policy_version: u64,
    ) -> CachedPageHeight {
        if self.structure_version == structure_version
            && self.layout_key_hash == layout_key_hash
            && self.page_policy_version == page_policy_version
        {
            return CachedPageHeight {
                height: self.height,
                confidence: self.confidence,
                source: CacheSource::ExactMatch,
                dirty: self.dirty,
                max_error_hint: self.max_error_hint,
            };
        }

        CachedPageHeight {
            height: self.height,
            confidence: HeightConfidence::Historical,
            source: CacheSource::VersionMismatch,
            dirty: true,
            max_error_hint: self.max_error_hint.max(32.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheSource {
    ExactMatch,
    VersionMismatch,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CachedHeight {
    pub height: f64,
    pub confidence: HeightConfidence,
    pub source: CacheSource,
    pub max_error_hint: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CachedPageHeight {
    pub height: f64,
    pub confidence: HeightConfidence,
    pub source: CacheSource,
    pub dirty: bool,
    pub max_error_hint: f64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct InMemoryLayoutCacheStore {
    block_layout: HashMap<(BlockId, String), BlockLayoutRow>,
    page_layout: HashMap<(DocumentId, u64, StructureVersion, String, u64, usize), PageLayoutRow>,
}

impl InMemoryLayoutCacheStore {
    pub fn save_block_layout(&mut self, row: BlockLayoutRow) {
        self.block_layout
            .insert((row.block_id, row.layout_key_hash.clone()), row);
    }

    pub fn load_block_height(&self, block_id: BlockId, key: LayoutCacheKey) -> CachedHeight {
        let exact_hash = key.hash_key();
        if let Some(row) = self.block_layout.get(&(block_id, exact_hash)) {
            return row.load_for_key(key);
        }
        self.block_layout
            .values()
            .filter(|row| row.block_id == block_id)
            .max_by_key(|row| row.measured_at.unwrap_or(0))
            .map(|row| row.load_for_key(key))
            .unwrap_or(CachedHeight {
                height: 0.0,
                confidence: HeightConfidence::Default,
                source: CacheSource::Missing,
                max_error_hint: f64::INFINITY,
            })
    }

    pub fn save_page_layout(&mut self, row: PageLayoutRow) {
        self.page_layout.insert(
            (
                row.document_id,
                row.visible_index_version,
                row.structure_version,
                row.layout_key_hash.clone(),
                row.page_policy_version,
                row.page_index,
            ),
            row,
        );
    }

    pub fn load_page_height(
        &self,
        document_id: DocumentId,
        visible_index_version: u64,
        structure_version: StructureVersion,
        layout_key_hash: &str,
        page_policy_version: u64,
        page_index: usize,
    ) -> CachedPageHeight {
        let exact_key = (
            document_id,
            visible_index_version,
            structure_version,
            layout_key_hash.to_string(),
            page_policy_version,
            page_index,
        );
        if let Some(row) = self.page_layout.get(&exact_key) {
            return row.load_for_context(structure_version, layout_key_hash, page_policy_version);
        }

        self.page_layout
            .values()
            .filter(|row| row.document_id == document_id && row.page_index == page_index)
            .max_by_key(|row| row.updated_at)
            .map(|row| {
                row.load_for_context(structure_version, layout_key_hash, page_policy_version)
            })
            .unwrap_or(CachedPageHeight {
                height: 0.0,
                confidence: HeightConfidence::Default,
                source: CacheSource::Missing,
                dirty: true,
                max_error_hint: f64::INFINITY,
            })
    }
}

pub fn serialize_confidence(confidence: HeightConfidence) -> u8 {
    match confidence {
        HeightConfidence::Default => 0,
        HeightConfidence::Historical => 1,
        HeightConfidence::Predictive => 2,
        HeightConfidence::Exact => 3,
    }
}

pub fn deserialize_confidence(value: u8) -> HeightConfidence {
    match value {
        3 => HeightConfidence::Exact,
        2 => HeightConfidence::Predictive,
        1 => HeightConfidence::Historical,
        _ => HeightConfidence::Default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemas_define_block_and_page_layout_tables() {
        assert!(BLOCK_LAYOUT_TABLE_SQL.contains("CREATE TABLE block_layout"));
        assert!(BLOCK_LAYOUT_TABLE_SQL.contains("measured_height"));
        assert!(BLOCK_LAYOUT_TABLE_SQL.contains("estimated_height"));
        assert!(BLOCK_LAYOUT_TABLE_SQL.contains("width_bucket"));
        assert!(BLOCK_LAYOUT_TABLE_SQL.contains("layout_key_hash"));
        assert!(PAGE_LAYOUT_TABLE_SQL.contains("CREATE TABLE page_layout"));
        assert!(PAGE_LAYOUT_TABLE_SQL.contains("structure_version"));
        assert!(PAGE_LAYOUT_TABLE_SQL.contains("page_policy_version"));
        assert!(PAGE_LAYOUT_TABLE_SQL.contains("measured_ratio"));
    }

    #[test]
    fn cold_start_loads_historical_height_without_full_measure() {
        let mut store = InMemoryLayoutCacheStore::default();
        let key = key(10, 800, 1, 1, 1);
        store.save_block_layout(BlockLayoutRow::new(
            42,
            key,
            HeightEstimate {
                height: 128.0,
                confidence: HeightConfidence::Exact,
                max_error_hint: 0.0,
            },
        ));

        let cached = store.load_block_height(42, key);

        assert_eq!(cached.height, 128.0);
        assert_eq!(cached.confidence, HeightConfidence::Exact);
        assert_eq!(cached.source, CacheSource::ExactMatch);
    }

    #[test]
    fn font_version_change_downgrades_block_layout_to_default_or_historical_not_exact() {
        let mut store = InMemoryLayoutCacheStore::default();
        let old_key = key(10, 800, 1, 1, 1);
        store.save_block_layout(BlockLayoutRow::new(
            42,
            old_key,
            HeightEstimate {
                height: 100.0,
                confidence: HeightConfidence::Exact,
                max_error_hint: 0.0,
            },
        ));

        let cached = store.load_block_height(42, key(10, 800, 1, 2, 1));

        assert_ne!(cached.confidence, HeightConfidence::Exact);
        assert_eq!(cached.source, CacheSource::VersionMismatch);
    }

    #[test]
    fn width_bucket_change_downgrades_cached_block_height() {
        let mut store = InMemoryLayoutCacheStore::default();
        store.save_block_layout(BlockLayoutRow::new(
            42,
            key(10, 800, 1, 1, 1),
            HeightEstimate {
                height: 100.0,
                confidence: HeightConfidence::Exact,
                max_error_hint: 0.0,
            },
        ));

        let cached = store.load_block_height(42, key(11, 880, 1, 1, 1));

        assert_eq!(cached.confidence, HeightConfidence::Default);
        assert_eq!(cached.source, CacheSource::VersionMismatch);
        assert!(cached.max_error_hint >= 8.0);
    }

    #[test]
    fn structure_version_change_downgrades_page_layout_to_hint() {
        let mut store = InMemoryLayoutCacheStore::default();
        store.save_page_layout(PageLayoutRow {
            document_id: 1,
            visible_index_version: 7,
            structure_version: 3,
            layout_key_hash: "layout-a".to_string(),
            page_policy_version: 1,
            page_index: 2,
            block_start_index: 200,
            block_count: 100,
            first_block_id: Some(201),
            last_block_id: Some(300),
            height: 2400.0,
            measured_ratio: 1.0,
            confidence: HeightConfidence::Exact,
            max_error_hint: 0.0,
            dirty: false,
            updated_at: 100,
        });

        let cached = store.load_page_height(1, 7, 4, "layout-a", 1, 2);

        assert_eq!(cached.height, 2400.0);
        assert_eq!(cached.confidence, HeightConfidence::Historical);
        assert_eq!(cached.source, CacheSource::VersionMismatch);
        assert!(cached.dirty);
    }

    #[test]
    fn confidence_round_trip_matches_schema_values() {
        assert_eq!(serialize_confidence(HeightConfidence::Default), 0);
        assert_eq!(serialize_confidence(HeightConfidence::Historical), 1);
        assert_eq!(serialize_confidence(HeightConfidence::Predictive), 2);
        assert_eq!(serialize_confidence(HeightConfidence::Exact), 3);
        assert_eq!(deserialize_confidence(3), HeightConfidence::Exact);
        assert_eq!(deserialize_confidence(99), HeightConfidence::Default);
    }

    fn key(
        width_bucket: u16,
        exact_width_px: u32,
        content_version: u64,
        font_version: u64,
        theme_version: u64,
    ) -> LayoutCacheKey {
        LayoutCacheKey {
            width_bucket,
            exact_width_px,
            content_version,
            attrs_version: 0,
            style_version: 0,
            font_version,
            theme_version,
            scale_factor_milli: 1000,
        }
    }
}
