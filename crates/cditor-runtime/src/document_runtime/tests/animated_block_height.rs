//! 高度动画期间的所有权测试。
//!
//! 补间要求两件正常测量不提供的东西：
//! 1. **尾帧不许被容差门丢掉**。`measured_height_tolerance_px` 对代码块约 1px，
//!    是为了滤掉静态内容的 sub-pixel 抖动；但出缓动的尾段每帧本来就只动零点几个
//!    像素，同一把尺子会把那一整段丢掉——下方文档提前冻住，块内部还在缩，一个
//!    补间于是被看成"先开一半、停一下、再开一半"。
//! 2. **同一时刻只有一个写高度的人**。文本布局始终在按内容的自然全高上报
//!    （`text/element.rs` → `queue_measured_height`）。补间期间内容区被裁剪，但
//!    内层文本仍按全高测量。pending 表按 block 去重、后写覆盖先写，两条通道同时
//!    写就会互相拽。

use super::*;

const VIEWPORT: f64 = 400.0;
const EXPANDED: f64 = 500.0;
/// 与 `features/code` 的 V1_CODE_COLLAPSED_BLOCK_HEIGHT_PX 对齐。
const COLLAPSED: f64 = 46.0;

/// block 1 段落 + block 2 代码块（已撑到展开高度）+ 若干后续段落。
fn runtime_with_code_block() -> DocumentRuntime {
    let mut payloads = vec![
        BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "before"),
        BlockPayloadRecord {
            block_id: 2,
            content_version: 1,
            kind: RichBlockKind::Code {
                language: Some("rust".to_owned()),
            },
            payload: BlockPayload::RichText {
                spans: vec![InlineSpan::plain("fn main() {}")],
            },
        },
    ];
    for offset in 0..10 {
        payloads.push(BlockPayloadRecord::rich_text(
            3 + offset as BlockId,
            RichBlockKind::Paragraph,
            "after",
        ));
    }
    let mut runtime = DocumentRuntime::from_payloads(1, payloads, VIEWPORT);
    runtime.apply_measured_height(2, 1, EXPANDED).unwrap();
    runtime
}

fn effective_height(runtime: &DocumentRuntime, block_id: BlockId) -> f64 {
    runtime
        .block_layout_meta(block_id)
        .expect("layout meta")
        .effective_height()
}

#[test]
fn begin_and_end_toggle_the_animation_ownership_flag() {
    let mut runtime = runtime_with_code_block();
    assert!(!runtime.is_block_height_animating(2));

    runtime.begin_block_height_animation(2);
    assert!(runtime.is_block_height_animating(2));

    // 幂等：补间播到一半反向点击会再走一次 begin。
    runtime.begin_block_height_animation(2);
    assert!(runtime.is_block_height_animating(2));

    runtime.end_block_height_animation(2);
    assert!(!runtime.is_block_height_animating(2));
}

/// 补间尾帧的亚像素增量必须落到布局。
///
/// 0.3px 远小于代码块的容差（约 1px），正常测量会把它当噪声丢掉。这正是原先
/// "下方内容提前停住"的成因。
#[test]
fn animated_height_lands_increments_below_the_measurement_tolerance() {
    let mut runtime = runtime_with_code_block();
    let tolerance = cditor_core::layout::measured_height_tolerance_px(&RichBlockKind::Code {
        language: Some("rust".to_owned()),
    });
    assert!(
        tolerance > 0.3,
        "该用例要求 0.3px 在正常容差之下，当前容差 {tolerance}"
    );

    runtime.begin_block_height_animation(2);

    let start = effective_height(&runtime, 2);
    let nudged = start - 0.3;
    assert!(
        runtime.apply_animated_block_height(2, 1, nudged).unwrap(),
        "亚容差增量在动画通道必须被接受"
    );
    assert_eq!(effective_height(&runtime, 2), nudged);

    runtime.end_block_height_animation(2);
}

/// 同样的 0.3px 走正常通道会被容差挡下——两条通道的尺子确实不同。
#[test]
fn normal_measurement_still_filters_sub_tolerance_noise() {
    let mut runtime = runtime_with_code_block();
    let start = effective_height(&runtime, 2);

    assert!(
        !runtime.apply_measured_height(2, 1, start - 0.3).unwrap(),
        "静态测量应当把亚容差抖动当噪声丢掉"
    );
    assert_eq!(effective_height(&runtime, 2), start);
}

/// 动画期间，文本布局上报的自然全高必须被拒。
///
/// 否则它和补间高度轮流写同一张 pending 表，折叠走到一半被拽回全高再拉下去。
#[test]
fn measured_height_is_rejected_while_a_block_is_animating() {
    let mut runtime = runtime_with_code_block();
    runtime.begin_block_height_animation(2);

    // 补间已经把块缩到中途。
    runtime.apply_animated_block_height(2, 1, 300.0).unwrap();
    assert_eq!(effective_height(&runtime, 2), 300.0);

    // 文本布局此刻仍在按自然全高上报——必须无效。
    assert!(
        !runtime.queue_measured_height(2, 1, EXPANDED).unwrap(),
        "动画期间的静态测量必须被拒"
    );
    assert!(runtime.layout.pending_measured_heights.is_empty());
    assert_eq!(
        effective_height(&runtime, 2),
        300.0,
        "补间高度不许被自然全高覆盖"
    );

    runtime.end_block_height_animation(2);
}

/// 所有权只针对被标记的块，不波及邻居。
#[test]
fn animation_ownership_is_scoped_to_the_animating_block() {
    let mut runtime = runtime_with_code_block();
    runtime.begin_block_height_animation(2);

    // 邻居段落的正常测量照常受理。
    assert!(
        runtime.apply_measured_height(3, 1, 96.0).unwrap(),
        "未参与动画的块不受影响"
    );
    assert_eq!(effective_height(&runtime, 3), 96.0);

    runtime.end_block_height_animation(2);
}

/// 交还所有权之后，正常测量立刻恢复受理。
///
/// 少了这一步，块的高度会永久卡在补间终值上，后续打字都不再改变块高。
#[test]
fn measurement_resumes_after_the_animation_ends() {
    let mut runtime = runtime_with_code_block();

    runtime.begin_block_height_animation(2);
    runtime
        .apply_animated_block_height(2, 1, COLLAPSED)
        .unwrap();
    runtime.end_block_height_animation(2);

    assert_eq!(effective_height(&runtime, 2), COLLAPSED);
    assert!(
        runtime.apply_measured_height(2, 1, EXPANDED).unwrap(),
        "交还所有权后静态测量必须重新受理"
    );
    assert_eq!(effective_height(&runtime, 2), EXPANDED);
}

/// 过期的 content_version 在动画通道同样要被拒。
///
/// 绕开容差门不等于绕开版本校验：内容在补间途中被改过，那个补间的高度就是过期数据。
#[test]
fn animated_height_still_honours_the_content_version() {
    let mut runtime = runtime_with_code_block();
    runtime.begin_block_height_animation(2);

    assert!(
        !runtime.apply_animated_block_height(2, 999, 123.0).unwrap(),
        "版本不匹配的动画高度必须被拒"
    );
    assert_eq!(effective_height(&runtime, 2), EXPANDED);

    runtime.end_block_height_animation(2);
}

/// 逐帧推进整个补间：每一帧都要真的落到布局上，不许有平台段。
///
/// 这是"两段式"最直接的回归钉子。按 60Hz 把出缓动的 12 帧全喂进去，如果某一段
/// 被容差门吞掉，`applied` 就会出现空档。
#[test]
fn every_frame_of_a_collapse_sweep_reaches_the_layout() {
    let mut runtime = runtime_with_code_block();
    runtime.begin_block_height_animation(2);

    // ease-out cubic，200ms，60Hz。
    let frames = 12;
    let mut previous = EXPANDED;
    let mut applied = 0;
    for frame in 1..=frames {
        let t = (frame as f64 / frames as f64).min(1.0);
        let eased = 1.0 - (1.0 - t).powi(3);
        let height = EXPANDED + (COLLAPSED - EXPANDED) * eased;

        let changed = runtime.apply_animated_block_height(2, 1, height).unwrap();
        if changed {
            applied += 1;
            assert_eq!(
                effective_height(&runtime, 2),
                height,
                "第 {frame} 帧没有落到布局"
            );
        }
        assert!(height <= previous, "折叠必须单调收缩");
        previous = height;
    }

    assert_eq!(
        applied, frames,
        "每一帧都应落到布局；缺帧就是容差门又在吞尾段"
    );

    runtime.end_block_height_animation(2);
    assert!((effective_height(&runtime, 2) - COLLAPSED).abs() < 0.01);
}
