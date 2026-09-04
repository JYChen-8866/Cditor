//! 代码块折叠/展开时的滚动稳定性复现测试。
//!
//! 这里只关心一个不变量：**改高度不许让用户正在看的东西移动**。
//! 折叠块本身的高度变化是预期内的，视口相对文档的位置漂移不是。

use super::*;

/// 折叠后的代码块高度，与 `features/code` 里的 V1_CODE_COLLAPSED_BLOCK_HEIGHT_PX 对齐。
const COLLAPSED: f64 = 46.0;
const EXPANDED: f64 = 500.0;
const VIEWPORT: f64 = 400.0;

/// 一个代码块夹在段落之间：block 1 段落、block 2 代码块、其余段落。
fn runtime_with_code_block(trailing_paragraphs: usize, code_height: f64) -> DocumentRuntime {
    let mut payloads = vec![
        BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "before"),
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
    for offset in 0..trailing_paragraphs {
        payloads.push(BlockPayloadRecord::rich_text(
            3 + offset as BlockId,
            RichBlockKind::Paragraph,
            "after",
        ));
    }
    let mut runtime = DocumentRuntime::from_payloads(1, payloads, VIEWPORT);
    // 把代码块撑到展开高度，作为折叠前的初始状态。
    runtime.apply_measured_height(2, 1, code_height).unwrap();
    runtime
}

/// 代码块整个在视口内、下方内容充足时折叠：视口顶那个块不许动。
///
/// 这是最常见的情形——用户看得见折叠按钮才点得到它。
#[test]
fn collapsing_code_block_below_viewport_top_keeps_scroll_top() {
    let mut runtime = runtime_with_code_block(40, EXPANDED);
    runtime
        .layout
        .scroll
        .scroll_to_global_offset(16.0, ScrollOrigin::UserWheel)
        .unwrap();
    let before_top = runtime.layout.scroll.global_scroll_top;
    let before_anchor = runtime.target_for_global_offset(before_top).unwrap();

    runtime.apply_measured_height(2, 1, COLLAPSED).unwrap();

    let after_top = runtime.layout.scroll.global_scroll_top;
    let after_anchor = runtime.target_for_global_offset(after_top).unwrap();
    assert_eq!(
        before_anchor.block_id, after_anchor.block_id,
        "视口顶换了块：before={before_anchor:?} after={after_anchor:?}"
    );
    assert!(
        (before_anchor.offset_in_block - after_anchor.offset_in_block).abs() < 1e-6,
        "视口顶在块内的偏移漂了：before={before_anchor:?} after={after_anchor:?}"
    );
    assert_eq!(before_top, after_top, "scroll_top 不该动");
}

/// 下方内容不足时折叠：夹取会把 scroll_top 强行拉回，顶部内容挤进视口。
///
/// 这是几何上不可避免的——文档变矮到撑不满视口下沿时，只能往回夹。
/// 但夹取量应当恰好等于溢出量，不该多夹。
#[test]
fn collapsing_near_document_end_clamps_by_exactly_the_overflow() {
    let mut runtime = runtime_with_code_block(2, EXPANDED);
    let max_before = runtime.layout.scroll.max_scroll_top();
    runtime
        .layout
        .scroll
        .scroll_to_global_offset(max_before, ScrollOrigin::UserWheel)
        .unwrap();
    let before_top = runtime.layout.scroll.global_scroll_top;

    runtime.apply_measured_height(2, 1, COLLAPSED).unwrap();

    let after_top = runtime.layout.scroll.global_scroll_top;
    let max_after = runtime.layout.scroll.max_scroll_top();
    assert!(
        after_top <= max_after + 1e-6,
        "scroll_top={after_top} 超出新上限 {max_after}"
    );
    assert!(
        after_top <= before_top,
        "折叠让文档变矮，scroll_top 不该变大：{before_top} -> {after_top}"
    );
    // 夹取只允许夹到新上限，不允许夹过头。
    assert!(
        (after_top - max_after).abs() < 1e-6 || (after_top - before_top).abs() < 1e-6,
        "夹取过头：before={before_top} after={after_top} max_after={max_after}"
    );
}

/// 视口顶落在**正在折叠的那个块内部**：offset_in_block 会超出块的新高度。
///
/// 恢复公式是 `new_block_top + offset_in_block`。块从 500 收到 46 之后，
/// 原先 300 的块内偏移已经指到块尾之外，视口被钉到后面几个块里去了。
#[test]
fn collapsing_the_anchor_block_clamps_offset_into_the_new_height() {
    let mut runtime = runtime_with_code_block(40, EXPANDED);
    // 滚到代码块内部：block 1 高 32，代码块从 32 起算，+300 落在块中间。
    runtime
        .layout
        .scroll
        .scroll_to_global_offset(32.0 + 300.0, ScrollOrigin::UserWheel)
        .unwrap();
    let before_anchor = runtime
        .target_for_global_offset(runtime.layout.scroll.global_scroll_top)
        .unwrap();
    assert_eq!(before_anchor.block_id, 2, "锚点应落在代码块内");

    runtime.apply_measured_height(2, 1, COLLAPSED).unwrap();

    let after_top = runtime.layout.scroll.global_scroll_top;
    let code_index = runtime.document.visible_index.visible_index_of(2).unwrap();
    let code_top = runtime
        .layout
        .height_index
        .offset_of_block(code_index)
        .unwrap();

    // 视口顶最远只能停在折叠块的下边缘。落在边缘上意味着紧跟其后的内容顶到视口
    // 顶——折叠掉一段内容后本就该看到这个。越过边缘就是钉进了后面的块，那是 bug。
    //
    // 注意别改成断言 `block_id == 2`：`block_at_offset` 用半开区间，偏移恰好等于
    // 下边缘时归属下一个块。
    assert!(
        after_top <= code_top + COLLAPSED + 1e-6,
        "视口顶越过折叠块下边缘：scroll_top={after_top} code_top={code_top} collapsed={COLLAPSED}"
    );
    assert!(
        after_top >= code_top - 1e-6,
        "视口顶跑到折叠块上方去了：scroll_top={after_top} code_top={code_top}"
    );
}

/// 展开位于视口上方的代码块：视口顶那个块必须留在原地。
///
/// 展开让文档变高、下方内容整体下移。若不补偿，视口相对上移，
/// 顶部内容挤进视口——按钮就跑了。
#[test]
fn expanding_code_block_above_viewport_keeps_anchor_fixed() {
    let mut runtime = runtime_with_code_block(40, COLLAPSED);
    // 滚到代码块下方：代码块折叠态占 32..78，从 200 起视口顶在后面的段落里。
    runtime
        .layout
        .scroll
        .scroll_to_global_offset(200.0, ScrollOrigin::UserWheel)
        .unwrap();
    let before_anchor = runtime
        .target_for_global_offset(runtime.layout.scroll.global_scroll_top)
        .unwrap();

    runtime.apply_measured_height(2, 1, EXPANDED).unwrap();

    let after_anchor = runtime
        .target_for_global_offset(runtime.layout.scroll.global_scroll_top)
        .unwrap();
    assert_eq!(
        before_anchor.block_id, after_anchor.block_id,
        "展开后视口顶换了块：before={before_anchor:?} after={after_anchor:?}"
    );
    assert!(
        (before_anchor.offset_in_block - after_anchor.offset_in_block).abs() < 1e-6,
        "展开后块内偏移漂了：before={before_anchor:?} after={after_anchor:?}"
    );
}

/// 补间逐帧推高度，累计效果必须和一次到位相同。
///
/// 折叠动画 180ms 约 11 帧，每帧 apply 一次高度。若每帧各自取锚点、各自夹取，
/// 误差会逐帧累积，最终位置和直接折叠不一致。
#[test]
fn frame_by_frame_collapse_matches_single_step_collapse() {
    let mut stepwise = runtime_with_code_block(40, EXPANDED);
    let mut single = runtime_with_code_block(40, EXPANDED);
    for runtime in [&mut stepwise, &mut single] {
        runtime
            .layout
            .scroll
            .scroll_to_global_offset(16.0, ScrollOrigin::UserWheel)
            .unwrap();
    }

    // 模拟补间：从 500 逐帧收到 46。
    let frames = 11;
    for frame in 1..=frames {
        let t = frame as f64 / frames as f64;
        let height = EXPANDED + (COLLAPSED - EXPANDED) * t;
        stepwise.apply_measured_height(2, 1, height).unwrap();
    }
    single.apply_measured_height(2, 1, COLLAPSED).unwrap();

    assert!(
        (stepwise.layout.scroll.global_scroll_top - single.layout.scroll.global_scroll_top).abs()
            < 1e-6,
        "逐帧折叠与一次折叠结果不同：stepwise={} single={}",
        stepwise.layout.scroll.global_scroll_top,
        single.layout.scroll.global_scroll_top,
    );
}
