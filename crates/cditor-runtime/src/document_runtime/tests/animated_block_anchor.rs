//! 折叠/展开动画期间，被操作的那个块的顶边不许在视口里移动。
//!
//! 这条不变量来自实测：用户点了展开按钮，按钮自己跑出了视口。探针打出来的是
//!
//! ```text
//! pushed_h=46.33  scroll_top 706.00 -> 706.33   delta=+0.33
//! pushed_h=65.20  scroll_top 706.33 -> 725.20   delta=+18.87
//! ...
//! pushed_h=172.64 scroll_top 831.85 -> 832.64   delta=+0.79
//! ```
//!
//! 每帧 scroll_top 的增量恰好等于块高度的增量，累计 +127 = 173−46。视口顶锚点
//! 为了让"视口顶那一点"不动，把整个高度增长量加到了 scroll_top 上，于是块顶
//! （连同长在它上面的按钮）往上跑了 127px。
//!
//! 块自身变高不会移动它自己的 block_top，所以"块顶不动" == "scroll_top 不动"。

use super::*;

const COLLAPSED: f64 = 46.0;
const EXPANDED: f64 = 173.0;
const VIEWPORT: f64 = 704.0;

/// 代码块夹在段落中间，前面有足够内容能滚动。
fn runtime_with_code_block_at(code_position: usize, code_height: f64) -> DocumentRuntime {
    let code_id = (code_position + 1) as BlockId;
    let mut payloads = Vec::new();
    for block_id in 1..=60u64 {
        if block_id == code_id {
            payloads.push(BlockPayloadRecord {
                block_id,
                content_version: 1,
                kind: RichBlockKind::Code {
                    language: Some("rust".to_owned()),
                },
                payload: BlockPayload::RichText {
                    spans: vec![cditor_core::rich_text::InlineSpan::plain("fn main() {}")],
                },
            });
        } else {
            payloads.push(BlockPayloadRecord::rich_text(
                block_id,
                RichBlockKind::Paragraph,
                "line",
            ));
        }
    }
    let mut runtime = DocumentRuntime::from_payloads(1, payloads, VIEWPORT);
    runtime
        .apply_measured_height(code_id, 1, code_height)
        .unwrap();
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

/// 展开：逐帧走动画通道，块顶在视口里的位置每一帧都不许动。
#[test]
fn expanding_via_animation_channel_keeps_the_block_top_fixed_on_screen() {
    let code_id = 14u64;
    let mut runtime = runtime_with_code_block_at(13, COLLAPSED);
    let block_top = block_top_of(&runtime, code_id);

    // 滚到块顶略微高于视口顶——实测复现时就是这个位置（button_screen_y 为负）。
    runtime
        .layout
        .scroll
        .scroll_to_global_offset(block_top + 75.0, ScrollOrigin::UserWheel)
        .unwrap();
    let before_scroll_top = runtime.layout.scroll.global_scroll_top;
    let before_button_y = block_top - before_scroll_top;

    runtime.begin_block_height_animation(code_id);
    let frames = 16;
    let mut drift = Vec::new();
    for frame in 1..=frames {
        let t = frame as f64 / frames as f64;
        let height = COLLAPSED + (EXPANDED - COLLAPSED) * t;
        runtime
            .apply_animated_block_height(code_id, 1, height)
            .unwrap();
        let button_y = block_top_of(&runtime, code_id) - runtime.layout.scroll.global_scroll_top;
        if (button_y - before_button_y).abs() > 1e-6 {
            drift.push(format!(
                "frame {frame}: h={height:.2} button_y {before_button_y:.2} -> {button_y:.2}"
            ));
        }
    }
    runtime.end_block_height_animation(code_id);

    assert!(
        drift.is_empty(),
        "展开动画期间块顶移位（按钮会跑出视口）：\n{}",
        drift.join("\n")
    );
    assert!(
        (runtime.layout.scroll.global_scroll_top - before_scroll_top).abs() < 1e-6,
        "scroll_top 被推动：{before_scroll_top} -> {}",
        runtime.layout.scroll.global_scroll_top
    );
}

/// 折叠：同样不许动。
#[test]
fn collapsing_via_animation_channel_keeps_the_block_top_fixed_on_screen() {
    let code_id = 14u64;
    let mut runtime = runtime_with_code_block_at(13, EXPANDED);
    let block_top = block_top_of(&runtime, code_id);

    runtime
        .layout
        .scroll
        .scroll_to_global_offset(block_top + 64.0, ScrollOrigin::UserWheel)
        .unwrap();
    let before_scroll_top = runtime.layout.scroll.global_scroll_top;
    let before_button_y = block_top - before_scroll_top;

    runtime.begin_block_height_animation(code_id);
    let frames = 16;
    let mut drift = Vec::new();
    for frame in 1..=frames {
        let t = frame as f64 / frames as f64;
        let height = EXPANDED + (COLLAPSED - EXPANDED) * t;
        runtime
            .apply_animated_block_height(code_id, 1, height)
            .unwrap();
        let button_y = block_top_of(&runtime, code_id) - runtime.layout.scroll.global_scroll_top;
        if (button_y - before_button_y).abs() > 1e-6 {
            drift.push(format!(
                "frame {frame}: h={height:.2} button_y {before_button_y:.2} -> {button_y:.2}"
            ));
        }
    }
    runtime.end_block_height_animation(code_id);

    assert!(
        drift.is_empty(),
        "折叠动画期间块顶移位：\n{}",
        drift.join("\n")
    );
}

/// 扫一遍滚动位置：块在视口里的任何位置，展开都不许移动块顶。
#[test]
fn animated_expand_keeps_block_top_fixed_at_every_scroll_position() {
    let code_id = 14u64;
    let mut failures = Vec::new();

    for step in 0..24 {
        let mut runtime = runtime_with_code_block_at(13, COLLAPSED);
        let block_top = block_top_of(&runtime, code_id);
        let scroll_top = (block_top - 300.0 + step as f64 * 32.0).max(0.0);
        runtime
            .layout
            .scroll
            .scroll_to_global_offset(scroll_top, ScrollOrigin::UserWheel)
            .unwrap();
        let actual = runtime.layout.scroll.global_scroll_top;
        let before_button_y = block_top - actual;

        runtime.begin_block_height_animation(code_id);
        runtime
            .apply_animated_block_height(code_id, 1, EXPANDED)
            .unwrap();
        runtime.end_block_height_animation(code_id);

        let after_button_y =
            block_top_of(&runtime, code_id) - runtime.layout.scroll.global_scroll_top;
        if (after_button_y - before_button_y).abs() > 1e-6 {
            failures.push(format!(
                "scroll_top={actual:.1}: button_y {before_button_y:.1} -> {after_button_y:.1}（位移 {:.1}）",
                after_button_y - before_button_y
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "展开时块顶移位：\n{}",
        failures.join("\n")
    );
}

/// 反例保护：**非**动画通道的高度变化（比如图片异步测量完成）仍要补偿视口顶锚点。
///
/// 这条锚点补偿是对的，不能被上面的修复顺手关掉——否则视口上方的块测量完成时，
/// 用户正在读的内容会整段跳走。
#[test]
fn non_animated_height_change_above_viewport_still_restores_the_anchor() {
    let mut runtime = runtime_with_code_block_at(13, COLLAPSED);
    runtime
        .layout
        .scroll
        .scroll_to_global_offset(600.0, ScrollOrigin::UserWheel)
        .unwrap();
    let before_anchor = runtime
        .target_for_global_offset(runtime.layout.scroll.global_scroll_top)
        .unwrap();
    let before_scroll_top = runtime.layout.scroll.global_scroll_top;

    // block 1 在视口上方，走正常测量通道长高。
    runtime.apply_measured_height(1, 1, 200.0).unwrap();

    let after_anchor = runtime
        .target_for_global_offset(runtime.layout.scroll.global_scroll_top)
        .unwrap();
    assert_eq!(
        before_anchor.block_id, after_anchor.block_id,
        "视口顶换了块：普通测量的锚点补偿被误关了"
    );
    assert!(
        runtime.layout.scroll.global_scroll_top > before_scroll_top,
        "视口上方的块长高，scroll_top 应当跟着增大以守住视口顶"
    );
}
