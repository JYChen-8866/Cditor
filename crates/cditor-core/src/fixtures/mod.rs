//! 版本化 benchmark/regression fixture（P0-007，总设计 29.2）。
//!
//! 四类基准语料：100k mixed（复用 [`crate::demo_fixtures`]）、Bidi 压力文档、
//! large code、large table。每个生成器都是确定性的：同版本同参数必须产生
//! 相同的语义 checksum；manifest checksum 在测试中固定，生成器的任何语义
//! 改动都必须显式提升对应 fixture 版本并更新 pinned checksum。

pub mod bidi;
pub mod code;
pub mod table;
pub mod unknown;

use crate::rich_text::RichTextDocument;

/// manifest 自身的结构版本。
pub const FIXTURE_MANIFEST_SCHEMA_VERSION: u32 = 1;

/// 100k mixed fixture 的版本（生成器在 [`crate::demo_fixtures`]）。
pub const MIXED_FIXTURE_VERSION: u32 = 1;

/// 单个 fixture 的可复现描述，benchmark 报告必须记录 name/version/checksum。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixtureManifest {
    pub name: &'static str,
    pub version: u32,
    pub block_count: usize,
    pub semantic_checksum: u64,
}

/// 对文档 id、index 与 payload 的语义序列化做 FNV-1a 64 位哈希。
///
/// 依赖 serde_json 对 struct 字段顺序的确定性输出；不含任何随机源。
pub fn document_semantic_checksum(document: &RichTextDocument) -> u64 {
    let serialized = serde_json::to_string(&(
        document.id,
        document.index_records(),
        document.payload_records(),
    ))
    .expect("fixture documents must serialize");
    fnv1a_64(serialized.as_bytes())
}

/// 为文档生成 manifest。
pub fn fixture_manifest(
    name: &'static str,
    version: u32,
    document: &RichTextDocument,
) -> FixtureManifest {
    FixtureManifest {
        name,
        version,
        block_count: document.blocks.len(),
        semantic_checksum: document_semantic_checksum(document),
    }
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes.iter().fold(OFFSET_BASIS, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo_fixtures::{LARGE_MIXED_DEMO_BLOCKS, large_mixed_rich_text_document};

    #[test]
    fn checksum_is_deterministic_across_builds() {
        let first = large_mixed_rich_text_document(7, 256);
        let second = large_mixed_rich_text_document(7, 256);
        assert_eq!(
            document_semantic_checksum(&first),
            document_semantic_checksum(&second)
        );
    }

    #[test]
    fn checksum_distinguishes_content_changes() {
        let base = large_mixed_rich_text_document(7, 256);
        let longer = large_mixed_rich_text_document(7, 257);
        let other_document = large_mixed_rich_text_document(8, 256);

        assert_ne!(
            document_semantic_checksum(&base),
            document_semantic_checksum(&longer)
        );
        assert_ne!(
            document_semantic_checksum(&base),
            document_semantic_checksum(&other_document)
        );
    }

    #[test]
    fn manifest_records_identity_dimensions() {
        let document = large_mixed_rich_text_document(7, 64);
        let manifest = fixture_manifest("mixed-sample", MIXED_FIXTURE_VERSION, &document);

        assert_eq!(manifest.name, "mixed-sample");
        assert_eq!(manifest.version, 1);
        assert_eq!(manifest.block_count, 64);
        assert_eq!(
            manifest.semantic_checksum,
            document_semantic_checksum(&document)
        );
    }

    #[test]
    #[ignore = "builds the full 100k mixed fixture; run explicitly for benchmark manifests"]
    fn full_mixed_fixture_manifest_is_reproducible() {
        let document = crate::demo_fixtures::large_mixed_demo_document();
        let manifest = fixture_manifest("mixed-100k", MIXED_FIXTURE_VERSION, &document);

        assert_eq!(manifest.block_count, LARGE_MIXED_DEMO_BLOCKS);
        assert_eq!(
            manifest.semantic_checksum,
            document_semantic_checksum(&document)
        );
    }
}
