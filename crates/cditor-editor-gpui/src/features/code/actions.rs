use std::collections::{HashMap, HashSet};
use std::time::Duration;

#[cfg(test)]
use super::collapse_motion::COLLAPSE_DURATION;
use super::{CodeCollapseTween, HeightTween, V1_CODE_COLLAPSED_BLOCK_HEIGHT_PX};
use cditor_core::ids::BlockId;
use cditor_editor_protocol::command::{CditorCommand, CommandOutcomeStatus, CommandSource};
use gpui::Context;
use web_time::Instant;

use crate::editor_view::CditorV2View;

const COPY_FEEDBACK_DURATION: Duration = Duration::from_millis(1_500);

pub(crate) fn toggle_code_block_collapsed_from_gui(
    view: &mut CditorV2View,
    block_id: BlockId,
    cx: &mut Context<CditorV2View>,
) {
    let context = view
        .ready_session()
        .and_then(|session| session.block_layout_context(block_id).ok())
        .flatten();

    // Flip the collapsed flag immediately so the toolbar chevron reflects the intent.
    let collapsing = toggle_collapsed(&mut view.overlay.collapsed_code_blocks, block_id);

    if let Some(context) = context {
        let now = Instant::now();
        let active_tween = view.overlay.code_collapse_tweens.remove(&block_id);
        let target_height = if collapsing {
            // Persist the pre-collapse height so expand can restore the exact size.
            // If an expand tween is still in flight, context.effective_height is only
            // the current interpolated sample. Saving it would make every rapid
            // collapse/expand cycle permanently shorter. The taller tween endpoint is
            // the authoritative expanded height across a mid-animation reversal.
            next_code_block_height(
                &mut view.overlay.collapsed_code_block_heights,
                block_id,
                true,
                expanded_height_for_collapse(context.effective_height, active_tween.as_ref()),
                None,
                context.estimated_height,
            )
        } else {
            // 展开终点必须是块真正的全高，否则落定那一刻真实测量接管、和补间终点不等，
            // 就再跳一下。优先级：折叠前存下的高度 > 文本布局实测 > 估算值兜底。
            //
            // 中间那级来自 `text/element.rs` 的 `normalize_text_inner_measured_height`，
            // 对 Code 已经含 `code_block_v1_chrome_y()`（内含 block shell 留白），
            // 与 `effective_height` 同单位，可直接用。
            next_code_block_height(
                &mut view.overlay.collapsed_code_block_heights,
                block_id,
                false,
                context.effective_height,
                view.cache
                    .text_layouts
                    .get(&block_id)
                    .map(|layout| layout.measured_height),
                context.estimated_height,
            )
        };

        let content_version = context.content_version.unwrap_or(0);

        // Create or retarget the height tween so a mid-animation toggle continues smoothly.
        let tween = match active_tween {
            Some(existing) => CodeCollapseTween::start(
                existing.tween.retarget(target_height, now),
                content_version,
                now,
            ),
            None => CodeCollapseTween::start(
                HeightTween::new(context.effective_height, target_height, now),
                content_version,
                now,
            ),
        };

        // Push the first interpolated height so the very next layout sees motion.
        let first_height = tween.frame_height;
        view.overlay.code_collapse_tweens.insert(block_id, tween);

        // 补间期间高度所有权归动画路径：文本布局仍在按自然全高测量并上报，放它进来
        // 会跟补间高度互相覆盖（pending 表按 block 去重，后写赢），折叠走到一半被拽回
        // 全高。`advance_height_tweens` 落定时交还。中途反向点击时这里是幂等的。
        let _ = view
            .ready_session()
            .and_then(|session| session.begin_block_height_animation(block_id).ok());

        let _ = view.ready_session().and_then(|session| {
            session
                .apply_animated_block_height(block_id, content_version, first_height)
                .ok()
        });
        // Do not mark_dirty during the animation. mark_dirty is issued once on settle.
    }

    cx.notify();
}

fn expanded_height_for_collapse(
    effective_height: f64,
    active_tween: Option<&CodeCollapseTween>,
) -> f64 {
    active_tween.map_or(effective_height, |tween| tween.tween.expanded_end())
}

fn toggle_collapsed(collapsed: &mut HashSet<BlockId>, block_id: BlockId) -> bool {
    if collapsed.remove(&block_id) {
        false
    } else {
        collapsed.insert(block_id);
        true
    }
}

/// 折叠/展开的目标高度。折叠与展开共用这条，两侧的存档、兜底、最小高约束在这里
/// 一次定死，避免生产路径和测试各自实现一份而分叉。
///
/// - 折叠：存下折叠前高度（含零高度块，见下），返回收起高度。
/// - 展开：存档 > 文本布局实测 > 估算兜底，且不低于外框最小高。
/// 展开分支收到 `measured_height` —— 展开态的块渲染会挂 `min_h`，终点低于它的话
/// 落定帧 `min_h` 生效会把块顶起来，看起来像动画结束后又跳了一下。
fn next_code_block_height(
    collapsed_heights: &mut HashMap<BlockId, f64>,
    block_id: BlockId,
    collapsing: bool,
    effective_height: f64,
    measured_height: Option<f64>,
    estimated_height: f64,
) -> f64 {
    if collapsing {
        collapsed_heights.insert(block_id, effective_height);
        V1_CODE_COLLAPSED_BLOCK_HEIGHT_PX
    } else {
        collapsed_heights
            .remove(&block_id)
            .or(measured_height)
            .unwrap_or(estimated_height)
            .max(cditor_core::layout::code_block_v1_outer_min_height())
    }
}

/// 切换 mermaid 代码块的预览态：显示渲染图，还是显示源码。
///
/// 这是显示切换，不是块类型转换——`RichBlockKind::Code { language }` 保持不变，
/// 所以输入框、折叠、语言选择器全都还是代码块那一套。mermaid 只负责把图画出来。
///
/// 返回切换后是否处于预览态，方便调用方立刻反映到按钮上。
pub(crate) fn toggle_mermaid_preview_from_gui(
    view: &mut CditorV2View,
    block_id: BlockId,
    cx: &mut Context<CditorV2View>,
) -> bool {
    let previewing = toggle_collapsed(&mut view.overlay.mermaid_preview_code_blocks, block_id);
    // 源码和图的高度不一样，切换后必须重新上报。高度上报带 (block, version, height)
    // 去重，同一个组合报过一次就会被跳过，所以这里要先把这个块的记录清掉，否则切回来
    // 的高度发不出去，布局会一直停在切换前那个高度上。正牌 mermaid 块的源码/预览
    // 切换也是这么做的（见 features/mermaid/actions.rs）。
    #[cfg(feature = "mermaid")]
    crate::features::media::invalidate_rendered_media_height_report(block_id);
    cx.notify();
    previewing
}

pub(crate) fn copy_code_block_from_gui(
    view: &mut CditorV2View,
    block_id: BlockId,
    cx: &mut Context<CditorV2View>,
) {
    if matches!(
        view.dispatch_command(
            CditorCommand::CopyBlockText { block_id },
            CommandSource::Toolbar,
            cx,
        ),
        Ok(outcome) if outcome.status == CommandOutcomeStatus::Applied
    ) {
        view.overlay.code_copy_feedback_block_id = Some(block_id);
        view.overlay.code_copy_feedback_generation =
            view.overlay.code_copy_feedback_generation.wrapping_add(1);
        let generation = view.overlay.code_copy_feedback_generation;
        let dismiss_after = cx.background_executor().timer(COPY_FEEDBACK_DURATION);
        cx.spawn(async move |view, cx| {
            let _ = dismiss_after.await;
            let _ = view.update(cx, |view, cx| {
                if copy_feedback_matches(
                    view.overlay.code_copy_feedback_block_id,
                    view.overlay.code_copy_feedback_generation,
                    block_id,
                    generation,
                ) {
                    view.overlay.code_copy_feedback_block_id = None;
                    cx.notify();
                }
            });
        })
        .detach();
        crate::overlays::show_toast(view, "已将代码拷贝到剪贴板", Duration::from_secs(3), cx);
    }
}

pub(super) fn copy_feedback_matches(
    current_block_id: Option<BlockId>,
    current_generation: u64,
    expected_block_id: BlockId,
    expected_generation: u64,
) -> bool {
    current_block_id == Some(expected_block_id) && current_generation == expected_generation
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::rich_text::{RichBlockRecord, RichTextDocument};
    use cditor_runtime::DocumentRuntime;
    use gpui::{AppContext, TestAppContext};

    fn code_block_view(cx: &mut TestAppContext) -> gpui::Entity<CditorV2View> {
        cx.new(|cx| {
            let mut document = RichTextDocument::empty(9);
            document.push_root_block(RichBlockRecord::heading(1, 1, "Title"));
            document.push_root_block(RichBlockRecord::code_block(
                2,
                Some("rust".to_owned()),
                "fn main() {}\nlet x = 1;",
            ));
            CditorV2View::from_runtime_with_options(
                DocumentRuntime::from_rich_text_document(document, 720.0),
                false,
                false,
                cx,
            )
        })
    }

    #[test]
    fn copy_feedback_only_expires_for_the_matching_block_and_generation() {
        assert!(copy_feedback_matches(Some(7), 3, 7, 3));
        assert!(!copy_feedback_matches(Some(8), 3, 7, 3));
        assert!(!copy_feedback_matches(Some(7), 4, 7, 3));
        assert!(!copy_feedback_matches(None, 3, 7, 3));
    }

    #[test]
    fn collapse_toggle_is_reversible_and_block_scoped() {
        let mut collapsed = HashSet::from([7]);

        assert!(toggle_collapsed(&mut collapsed, 9));
        assert_eq!(collapsed, HashSet::from([7, 9]));
        assert!(!toggle_collapsed(&mut collapsed, 9));
        assert_eq!(collapsed, HashSet::from([7]));
    }

    #[test]
    fn collapse_height_saves_effective_height_and_expand_restores_it() {
        let mut heights = HashMap::new();

        assert_eq!(
            next_code_block_height(&mut heights, 7, true, 386.0, None, 300.0),
            V1_CODE_COLLAPSED_BLOCK_HEIGHT_PX
        );
        assert_eq!(heights.get(&7), Some(&386.0));

        assert_eq!(
            next_code_block_height(&mut heights, 7, false, 38.0, None, 300.0),
            386.0
        );
        assert!(!heights.contains_key(&7));
    }

    #[test]
    fn collapse_during_expand_preserves_the_full_expanded_height() {
        let now = Instant::now();
        let expanding = CodeCollapseTween::start(
            HeightTween::new(V1_CODE_COLLAPSED_BLOCK_HEIGHT_PX, 640.0, now),
            3,
            now,
        );
        let intermediate_height = expanding.tween.height(now + COLLAPSE_DURATION / 2);
        assert!(intermediate_height < 640.0);

        assert_eq!(
            expanded_height_for_collapse(intermediate_height, Some(&expanding)),
            640.0
        );
    }

    #[test]
    fn expand_target_prefers_measured_height_over_the_estimate() {
        let mut heights = HashMap::new();

        // 没存档：文本布局实测优先于估算。
        let target = next_code_block_height(&mut heights, 7, false, 38.0, Some(412.0), 300.0);
        assert_eq!(target, 412.0);
        assert!(heights.is_empty());
    }

    #[test]
    fn expand_target_never_drops_below_the_outer_min_height() {
        let mut heights = HashMap::new();

        // 短代码块：估算和实测都压不到外框最小高之下，否则落定帧 min_h 生效会顶一下。
        let outer_min = cditor_core::layout::code_block_v1_outer_min_height();
        let low = next_code_block_height(&mut heights, 7, false, 38.0, Some(80.0), 90.0);
        assert_eq!(low, outer_min);
        assert!(low > 80.0, "被抬到最小高: {low}");

        // 顶到最小高之上就不动它了。
        let ok = next_code_block_height(&mut heights, 7, false, 38.0, Some(500.0), 90.0);
        assert_eq!(ok, 500.0);
    }

    #[gpui::test]
    fn code_block_collapse_and_expand_update_the_runtime_block_height(cx: &mut TestAppContext) {
        let view = code_block_view(cx);

        let original = view.read_with(cx, |view, _| {
            view.ready_session()
                .unwrap()
                .block_layout_context(2)
                .unwrap()
                .unwrap()
                .effective_height
        });
        assert!(original > V1_CODE_COLLAPSED_BLOCK_HEIGHT_PX);

        // Toggle to collapse. With animation the immediate sample is an
        // interpolation; the authoritative collapsed height is applied when
        // the tween settles. We validate the final state by forcing settle.
        view.update(cx, |view, cx| {
            toggle_code_block_collapsed_from_gui(view, 2, cx);
        });

        // Intent is recorded immediately (for icon + expand restore value).
        let (flag, saved) = view.read_with(cx, |view, _| {
            (
                view.overlay.collapsed_code_blocks.contains(&2),
                view.overlay.collapsed_code_block_heights.get(&2).copied(),
            )
        });
        assert!(flag);
        assert_eq!(saved, Some(original));

        // A tween is in flight.
        let tween_present = view.read_with(cx, |view, _| {
            view.overlay.code_collapse_tweens.contains_key(&2)
        });
        assert!(tween_present);

        // Simulate settle: remove the tween and apply the final target height
        // exactly as the animation driver would on completion.
        view.update(cx, |view, _| {
            if let Some(t) = view.overlay.code_collapse_tweens.remove(&2) {
                let _ = view.ready_session().and_then(|session| {
                    session
                        .finish_block_height_animation(2, t.content_version, t.tween.target(), true)
                        .ok()
                });
            }
        });

        let collapsed = view.read_with(cx, |view, _| {
            view.ready_session()
                .unwrap()
                .block_layout_context(2)
                .unwrap()
                .unwrap()
        });
        assert_eq!(
            collapsed.effective_height,
            V1_CODE_COLLAPSED_BLOCK_HEIGHT_PX
        );
        let natural_height_was_applied = view.read_with(cx, |view, _| {
            view.ready_session()
                .unwrap()
                .apply_measured_block_height(2, 1, original)
                .unwrap()
        });
        assert!(
            !natural_height_was_applied,
            "稳定收起时，隐藏内容的自然高度不能重新撑开文档槽位"
        );

        // Expand.
        view.update(cx, |view, cx| {
            toggle_code_block_collapsed_from_gui(view, 2, cx);
        });

        let tween_present2 = view.read_with(cx, |view, _| {
            view.overlay.code_collapse_tweens.contains_key(&2)
        });
        assert!(tween_present2);

        // Force settle to the restored original height.
        view.update(cx, |view, _| {
            if let Some(t) = view.overlay.code_collapse_tweens.remove(&2) {
                let _ = view.ready_session().and_then(|session| {
                    session
                        .finish_block_height_animation(
                            2,
                            t.content_version,
                            t.tween.target(),
                            false,
                        )
                        .ok()
                });
            }
        });

        let expanded = view.read_with(cx, |view, _| {
            view.ready_session()
                .unwrap()
                .block_layout_context(2)
                .unwrap()
                .unwrap()
        });
        assert_eq!(expanded.effective_height, original);

        let (flag2, heights_empty) = view.read_with(cx, |view, _| {
            (
                view.overlay.collapsed_code_blocks.contains(&2),
                view.overlay.collapsed_code_block_heights.is_empty(),
            )
        });
        assert!(!flag2);
        assert!(heights_empty);
    }
}
