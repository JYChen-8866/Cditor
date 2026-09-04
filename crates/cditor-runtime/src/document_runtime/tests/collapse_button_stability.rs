//! 折叠按钮必须留在原地。
//!
//! 这是用户真正关心的不变量，和"视口顶的内容不动"**不是**一回事：
//! 当被展开的块本身就在视口顶部或更上方时，保住视口顶反而必须把块顶边往上推，
//! 按钮就跑出视口了。
//!
//! 按钮长在代码块的 header 上，header 在块顶。所以判据是：
//!
//! ```text
//! 块顶在视口里的位置 = block_top - scroll_top   // 展开前后必须相等
//! ```

use super::*;

const COLLAPSED: f64 = 46.0;
const EXPANDED: f64 = 500.0;
const VIEWPORT: f64 = 400.0;

/// 代码块在文档最前面（block 1），后面跟若干段落。
fn runtime_with_leading_code_block(trailing: usize, code_height: f64) -> DocumentRuntime {
    let mut payloads = vec![BlockPayloadRecord {
        block_id: 1,
        content_version: 1,
        kind: RichBlockKind::Code {
            language: Some("rust".to_owned()),
        },
        payload: BlockPayload::RichText {
            spans: vec![cditor_core::rich_text::InlineSpan::plain("fn main() {}")],
        },
    }];
    for offset in 0..trailing {
        payloads.push(BlockPayloadRecord::rich_text(
            2 + offset as BlockId,
            RichBlockKind::Paragraph,
            "after",
        ));
    }
    let mut runtime = DocumentRuntime::from_payloads(1, payloads, VIEWPORT);
    runtime.apply_measured_height(1, 1, code_height).unwrap();
    runtime
}

fn block_top_of(runtime: &DocumentRuntime, block_id: BlockId) -> f64 {
    let index = runtime
        .document
        .visible_index
        .visible_index_of(block_id)
        .unwrap();
    runtime.layout.height_index.offset_of_block(index).unwrap()
}

/// 展开文档顶部的折叠代码块：按钮（块顶）在视口里的位置不许变。
///
/// 扫一遍 scroll_top，任何一个"按钮此刻可见"的位置都必须守住。
#[test]
fn expanding_a_leading_code_block_keeps_the_toggle_button_in_place() {
    let mut failures = Vec::new();

    for scroll_step in 0..40 {
        let scroll_top = scroll_step as f64 * 8.0;
        let mut runtime = runtime_with_leading_code_block(40, COLLAPSED);
        runtime
            .layout
            .scroll
            .scroll_to_global_offset(scroll_top, ScrollOrigin::UserWheel)
            .unwrap();
        let actual_scroll_top = runtime.layout.scroll.global_scroll_top;
        let before_block_top = block_top_of(&runtime, 1);
        let before_button_y = before_block_top - actual_scroll_top;

        // 按钮不在视口里就没法点，跳过。header 高 COLLAPSED。
        let button_visible = before_button_y > -COLLAPSED && before_button_y < VIEWPORT;
        if !button_visible {
            continue;
        }

        runtime.apply_measured_height(1, 1, EXPANDED).unwrap();

        let after_button_y = block_top_of(&runtime, 1) - runtime.layout.scroll.global_scroll_top;
        if (after_button_y - before_button_y).abs() > 1e-6 {
            failures.push(format!(
                "scroll_top={actual_scroll_top:.1}: 按钮从视口 y={before_button_y:.1} 跑到 y={after_button_y:.1}（位移 {:.1}）",
                after_button_y - before_button_y
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "展开顶部代码块时按钮移位：\n{}",
        failures.join("\n")
    );
}

/// 反向：折叠文档顶部的代码块，按钮同样不许动。
#[test]
fn collapsing_a_leading_code_block_keeps_the_toggle_button_in_place() {
    let mut failures = Vec::new();

    for scroll_step in 0..40 {
        let scroll_top = scroll_step as f64 * 12.0;
        let mut runtime = runtime_with_leading_code_block(40, EXPANDED);
        runtime
            .layout
            .scroll
            .scroll_to_global_offset(scroll_top, ScrollOrigin::UserWheel)
            .unwrap();
        let actual_scroll_top = runtime.layout.scroll.global_scroll_top;
        let before_block_top = block_top_of(&runtime, 1);
        let before_button_y = before_block_top - actual_scroll_top;

        let button_visible = before_button_y > -COLLAPSED && before_button_y < VIEWPORT;
        if !button_visible {
            continue;
        }

        runtime.apply_measured_height(1, 1, COLLAPSED).unwrap();

        let after_button_y = block_top_of(&runtime, 1) - runtime.layout.scroll.global_scroll_top;
        if (after_button_y - before_button_y).abs() > 1e-6 {
            failures.push(format!(
                "scroll_top={actual_scroll_top:.1}: 按钮从视口 y={before_button_y:.1} 跑到 y={after_button_y:.1}（位移 {:.1}）",
                after_button_y - before_button_y
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "折叠顶部代码块时按钮移位：\n{}",
        failures.join("\n")
    );
}

/// 代码块前面还有内容时展开，按钮同样不许动。
#[test]
fn expanding_a_code_block_after_a_paragraph_keeps_the_button_in_place() {
    let mut failures = Vec::new();

    for scroll_step in 0..40 {
        let scroll_top = scroll_step as f64 * 8.0;
        let mut payloads = vec![
            BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "intro"),
            BlockPayloadRecord {
                block_id: 2,
                content_version: 1,
                kind: RichBlockKind::Code {
                    language: Some("rust".to_owned()),
                },
                payload: BlockPayload::RichText {
                    spans: vec![cditor_core::rich_text::InlineSpan::plain("fn main() {}")],
                },
            },
        ];
        for offset in 0..40 {
            payloads.push(BlockPayloadRecord::rich_text(
                3 + offset as BlockId,
                RichBlockKind::Paragraph,
                "after",
            ));
        }
        let mut runtime = DocumentRuntime::from_payloads(1, payloads, VIEWPORT);
        runtime.apply_measured_height(2, 1, COLLAPSED).unwrap();
        runtime
            .layout
            .scroll
            .scroll_to_global_offset(scroll_top, ScrollOrigin::UserWheel)
            .unwrap();
        let actual_scroll_top = runtime.layout.scroll.global_scroll_top;
        let before_button_y = block_top_of(&runtime, 2) - actual_scroll_top;

        let button_visible = before_button_y > -COLLAPSED && before_button_y < VIEWPORT;
        if !button_visible {
            continue;
        }

        runtime.apply_measured_height(2, 1, EXPANDED).unwrap();

        let after_button_y = block_top_of(&runtime, 2) - runtime.layout.scroll.global_scroll_top;
        if (after_button_y - before_button_y).abs() > 1e-6 {
            failures.push(format!(
                "scroll_top={actual_scroll_top:.1}: 按钮从视口 y={before_button_y:.1} 跑到 y={after_button_y:.1}（位移 {:.1}）",
                after_button_y - before_button_y
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "展开段落后的代码块时按钮移位：\n{}",
        failures.join("\n")
    );
}
