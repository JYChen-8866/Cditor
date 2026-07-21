//! Bidi 压力 fixture：阿拉伯语、希伯来语、混合双向、组合记号与 emoji。
//!
//! 覆盖总设计 29.2 的 "CJK/emoji/combining/Bidi/font fallback 文档" 中的
//! 双向部分：纯 RTL 段落、RTL 标题、RTL/LTR 混排（数字、货币、拉丁 SKU）、
//! 希伯来语 niqqud 组合记号、阿拉伯-印度数字和 emoji 与 RTL 混排。

use crate::ids::{BlockId, DocumentId};
use crate::rich_text::{RichBlockRecord, RichTextDocument};

/// Bidi fixture 生成器版本；语义改动必须提升并更新 pinned checksum。
pub const BIDI_FIXTURE_VERSION: u32 = 1;

/// 标准 Bidi 压力文档的 block 数。
pub const BIDI_STRESS_BLOCKS: usize = 4_096;

/// 生成确定性的 Bidi 压力文档。
pub fn bidi_stress_document(document_id: DocumentId, count: usize) -> RichTextDocument {
    let mut document = RichTextDocument::empty(document_id);
    document.metadata.title = Some(format!("Bidi stress fixture v{BIDI_FIXTURE_VERSION}"));
    document.metadata.tags = vec!["fixture".to_owned(), "bidi".to_owned()];

    for index in 0..count {
        document.push_root_block(bidi_block(index as BlockId + 1, index));
    }

    document
}

fn bidi_block(block_id: BlockId, index: usize) -> RichBlockRecord {
    let section = index / 8 + 1;
    match index % 8 {
        0 => RichBlockRecord::heading(block_id, 2, format!("القسم {section} — عنوان تجريبي")),
        1 => RichBlockRecord::paragraph(
            block_id,
            format!("فقرة عربية خالصة رقم {section}: النص العربي يمتد من اليمين إلى اليسار."),
        ),
        2 => RichBlockRecord::paragraph(
            block_id,
            format!("פסקה עברית מספר {section}: טקסט בעברית נכתב מימין לשמאל."),
        ),
        3 => RichBlockRecord::paragraph(
            block_id,
            format!(
                "订单 {section}: המחיר 25.99 ₪ للسلعة ABC-{section} مع خصم 10%، 然后继续中文。"
            ),
        ),
        4 => RichBlockRecord::quote(
            block_id,
            format!("בְּרֵאשִׁית בָּרָא — ציטוט עם נִקּוּד (combining marks) {section}"),
        ),
        5 => RichBlockRecord::bulleted_list(
            block_id,
            format!("🚀 اختبار emoji مع النص العربي رقم {section} ثم English tail"),
        ),
        6 => RichBlockRecord::paragraph(
            block_id,
            format!("الأرقام العربية الهندية ٠١٢٣٤٥٦٧٨٩ والرقم اللاتيني {section}"),
        ),
        _ => RichBlockRecord::paragraph(
            block_id,
            format!("The word שלום means peace, and مرحبا means hello ({section})."),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{document_semantic_checksum, fixture_manifest};
    use crate::rich_text::BlockPayload;

    fn payload_text(record: &RichBlockRecord) -> String {
        match record.to_payload_record().payload {
            BlockPayload::RichText { spans } => {
                spans.into_iter().map(|span| span.text).collect::<String>()
            }
            other => panic!("bidi fixture should only emit rich text payloads, got {other:?}"),
        }
    }

    #[test]
    fn generator_is_deterministic() {
        let first = bidi_stress_document(11, 128);
        let second = bidi_stress_document(11, 128);
        assert_eq!(
            document_semantic_checksum(&first),
            document_semantic_checksum(&second)
        );
    }

    #[test]
    fn every_cycle_slot_contains_strong_rtl_characters() {
        let is_strong_rtl = |ch: char| matches!(ch, '\u{0590}'..='\u{05FF}' | '\u{0600}'..='\u{06FF}' | '\u{FB1D}'..='\u{FDFF}');
        for index in 0..8 {
            let block = bidi_block(index as BlockId + 1, index);
            let text = payload_text(&block);
            assert!(
                text.chars().any(is_strong_rtl),
                "cycle slot {index} lost its RTL content: {text}"
            );
        }
    }

    #[test]
    fn mixed_direction_slots_also_contain_ltr_runs() {
        for index in [3, 5, 7] {
            let text = payload_text(&bidi_block(index as BlockId + 1, index));
            assert!(
                text.chars().any(|ch| ch.is_ascii_alphanumeric()),
                "slot {index} should mix LTR runs: {text}"
            );
        }
    }

    #[test]
    fn combining_mark_slot_keeps_niqqud() {
        let text = payload_text(&bidi_block(5, 4));
        assert!(
            text.chars().any(|ch| matches!(ch, '\u{05B0}'..='\u{05C7}')),
            "niqqud combining marks missing: {text}"
        );
    }

    #[test]
    fn standard_fixture_manifest_shape() {
        let document = bidi_stress_document(11, BIDI_STRESS_BLOCKS);
        let manifest = fixture_manifest("bidi-stress", BIDI_FIXTURE_VERSION, &document);
        assert_eq!(manifest.block_count, BIDI_STRESS_BLOCKS);
    }
}
