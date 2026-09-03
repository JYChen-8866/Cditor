use std::collections::{HashMap, HashSet};
use std::time::Duration;

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
        let target_height = if collapsing {
            // Persist the pre-collapse height so expand can restore the exact size.
            view.overlay
                .collapsed_code_block_heights
                .insert(block_id, context.effective_height);
            V1_CODE_COLLAPSED_BLOCK_HEIGHT_PX
        } else {
            view.overlay
                .collapsed_code_block_heights
                .remove(&block_id)
                .unwrap_or(context.estimated_height)
        };

        let content_version = context.content_version.unwrap_or(0);

        // Create or retarget the height tween so a mid-animation toggle continues smoothly.
        let tween = match view.overlay.code_collapse_tweens.remove(&block_id) {
            Some(mut existing) => {
                existing.tween = existing.tween.retarget(target_height, now);
                existing.content_version = content_version;
                existing
            }
            None => CodeCollapseTween {
                tween: HeightTween::new(context.effective_height, target_height, now),
                content_version,
            },
        };

        // Push the first interpolated height so the very next layout sees motion.
        let first_height = tween.tween.height(now);
        view.overlay.code_collapse_tweens.insert(block_id, tween);

        let _ = view.ready_session().and_then(|session| {
            session
                .apply_measured_block_height(block_id, content_version, first_height)
                .ok()
        });
        // Do not mark_dirty during the animation. mark_dirty is issued once on settle.
    }

    cx.notify();
}

fn toggle_collapsed(collapsed: &mut HashSet<BlockId>, block_id: BlockId) -> bool {
    if collapsed.remove(&block_id) {
        false
    } else {
        collapsed.insert(block_id);
        true
    }
}

fn next_code_block_height(
    collapsed_heights: &mut HashMap<BlockId, f64>,
    block_id: BlockId,
    collapsing: bool,
    effective_height: f64,
    estimated_height: f64,
) -> f64 {
    if collapsing {
        collapsed_heights.insert(block_id, effective_height);
        V1_CODE_COLLAPSED_BLOCK_HEIGHT_PX
    } else {
        collapsed_heights
            .remove(&block_id)
            .unwrap_or(estimated_height)
    }
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
            next_code_block_height(&mut heights, 7, true, 386.0, 300.0),
            V1_CODE_COLLAPSED_BLOCK_HEIGHT_PX
        );
        assert_eq!(heights.get(&7), Some(&386.0));

        assert_eq!(
            next_code_block_height(&mut heights, 7, false, 38.0, 300.0),
            386.0
        );
        assert!(!heights.contains_key(&7));
    }

    #[test]
    fn expand_without_a_saved_height_falls_back_to_the_estimate() {
        let mut heights = HashMap::new();

        assert_eq!(
            next_code_block_height(&mut heights, 7, false, 38.0, 300.0),
            300.0
        );
        assert!(heights.is_empty());
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
                        .apply_measured_block_height(2, t.content_version, t.tween.target())
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
                        .apply_measured_block_height(2, t.content_version, t.tween.target())
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
