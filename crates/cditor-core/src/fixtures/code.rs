//! large code fixture：10MiB/100k 行级别的单块 code surface 与多 code block 文档。
//!
//! 对应总设计 29.2 的 "10MB/100k-line code surface"。生成的源码是确定性的
//! Rust 形态文本（含缩进、注释、长行），用于 layout/虚拟化/高亮基准；
//! P2-018 已证明整块同步 layout 不达预算，该 fixture 同时是 P6-015 内部
//! 虚拟化的验收语料。

use crate::ids::{BlockId, DocumentId};
use crate::rich_text::{RichBlockRecord, RichTextDocument};

/// code fixture 生成器版本；语义改动必须提升并更新 pinned checksum。
pub const CODE_FIXTURE_VERSION: u32 = 1;

/// 完整 large-code fixture 的目标字节数（10 MiB）。
pub const LARGE_CODE_FULL_TARGET_BYTES: usize = 10 * 1024 * 1024;

/// 生成目标字节数的确定性 Rust 形态源码。
///
/// 实际输出 >= `target_bytes`，且总是以完整行结束。
pub fn large_code_source(target_bytes: usize) -> String {
    let mut source = String::with_capacity(target_bytes + 256);
    source.push_str("//! Deterministic large-code fixture; generated line by line.\n");
    let mut line = 0usize;
    while source.len() < target_bytes {
        match line % 40 {
            0 => {
                source.push_str(&format!(
                    "\npub fn segment_{:06}(input: &[u64]) -> u64 {{\n",
                    line / 40
                ));
            }
            38 => source.push_str("    input.iter().copied().fold(acc, u64::wrapping_add)\n"),
            39 => source.push_str("}\n"),
            n if n % 7 == 3 => {
                source.push_str(&format!(
                    "    // step {line}: fold the previous accumulator into the running state\n"
                ));
            }
            n if n % 11 == 5 => {
                source.push_str(&format!(
                    "    let widened_{line:06} = u128::from(acc).wrapping_mul({}) as u64; let acc = widened_{line:06} ^ 0x{:016x}; debug_assert!(acc != {});\n",
                    line % 97 + 2,
                    (line as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15),
                    line % 13
                ));
            }
            _ => {
                source.push_str(&format!(
                    "    let acc = acc.rotate_left({}) ^ input.get({}).copied().unwrap_or({});\n",
                    line % 63 + 1,
                    line % 1024,
                    line % 251
                ));
            }
        }
        line += 1;
    }
    if !line.is_multiple_of(40) {
        source.push_str("    acc\n}\n");
    }
    source
}

/// 单个巨型 code block 的文档。
pub fn large_code_document(document_id: DocumentId, target_bytes: usize) -> RichTextDocument {
    let mut document = RichTextDocument::empty(document_id);
    document.metadata.title = Some(format!("Large code fixture v{CODE_FIXTURE_VERSION}"));
    document.metadata.tags = vec!["fixture".to_owned(), "large-code".to_owned()];
    document.push_root_block(RichBlockRecord::code_block(
        1,
        Some("rust".to_owned()),
        large_code_source(target_bytes),
    ));
    document
}

/// 多个中等 code block 组成的文档（每块 `lines_per_block` 行）。
pub fn many_code_blocks_document(
    document_id: DocumentId,
    block_count: usize,
    lines_per_block: usize,
) -> RichTextDocument {
    let mut document = RichTextDocument::empty(document_id);
    document.metadata.title = Some(format!("Many code blocks fixture v{CODE_FIXTURE_VERSION}"));
    document.metadata.tags = vec!["fixture".to_owned(), "many-code-blocks".to_owned()];
    for index in 0..block_count {
        let block_id = index as BlockId + 1;
        let mut code = format!("// block {block_id}\n");
        for line in 0..lines_per_block {
            code.push_str(&format!(
                "const ITEM_{index:04}_{line:04}: usize = {};\n",
                index * lines_per_block + line
            ));
        }
        let language = if index % 2 == 0 { "rust" } else { "typescript" };
        document.push_root_block(RichBlockRecord::code_block(
            block_id,
            Some(language.to_owned()),
            code,
        ));
    }
    document
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::document_semantic_checksum;

    #[test]
    fn source_reaches_target_and_ends_with_full_line() {
        let source = large_code_source(64 * 1024);
        assert!(source.len() >= 64 * 1024);
        assert!(source.ends_with('\n'));
    }

    #[test]
    fn source_is_deterministic() {
        assert_eq!(large_code_source(32 * 1024), large_code_source(32 * 1024));
    }

    #[test]
    fn source_contains_long_and_indented_lines() {
        let source = large_code_source(64 * 1024);
        let max_line = source.lines().map(str::len).max().unwrap_or(0);
        assert!(max_line > 120, "expected long lines, max was {max_line}");
        assert!(source.lines().any(|line| line.starts_with("    ")));
        assert!(
            source
                .lines()
                .any(|line| line.trim_start().starts_with("//"))
        );
    }

    #[test]
    fn documents_are_deterministic() {
        let first = large_code_document(13, 16 * 1024);
        let second = large_code_document(13, 16 * 1024);
        assert_eq!(
            document_semantic_checksum(&first),
            document_semantic_checksum(&second)
        );

        let many_first = many_code_blocks_document(14, 32, 20);
        let many_second = many_code_blocks_document(14, 32, 20);
        assert_eq!(many_first.blocks.len(), 32);
        assert_eq!(
            document_semantic_checksum(&many_first),
            document_semantic_checksum(&many_second)
        );
    }

    #[test]
    #[ignore = "builds the full 10MiB code fixture; run explicitly for benchmark manifests"]
    fn full_code_fixture_reaches_ten_mebibytes_and_100k_lines() {
        let source = large_code_source(LARGE_CODE_FULL_TARGET_BYTES);
        assert!(source.len() >= LARGE_CODE_FULL_TARGET_BYTES);
        assert!(
            source.lines().count() >= 100_000,
            "expected at least 100k lines, got {}",
            source.lines().count()
        );
    }
}
