use std::collections::HashSet;

use cditor_core::ids::BlockId;
use gpui::Context;
use web_time::Instant;

use crate::editor_view::CditorV2View;

pub(crate) fn toggle_source_from_gui(
    view: &mut CditorV2View,
    block_id: BlockId,
    cx: &mut Context<CditorV2View>,
) {
    crate::features::media::invalidate_rendered_media_height_report(block_id);

    let _was_showing_source = view.cache.mermaid_source_blocks.contains(&block_id);
    let now_showing_source = toggle_source_visibility(&mut view.cache.mermaid_source_blocks, block_id);

    // Start/continue a height tween so the switch is a continuous layout change,
    // exactly like code block collapse/expand.
    if let Some(context) = view
        .ready_session()
        .and_then(|session| session.block_layout_context(block_id).ok())
        .flatten()
    {
        let now = Instant::now();
        let content_version = context.content_version.unwrap_or(0);

        // Target height for the *new* side.
        let target_height = if now_showing_source {
            // Going to source: compute from current text (same metric as code blocks).
            let source: String = view
                .ready_session()
                .and_then(|session| session.block_plain_text(block_id).ok())
                .flatten()
                .unwrap_or_default();
            cditor_core::layout::estimate_mermaid_source_block_height_px(
                &source,
                // Use a reasonable content width; the runtime will clamp/adjust.
                // We don't have the exact projected width here, but the estimate is monotonic.
                cditor_core::layout::BODY_BLOCK_CONTENT_WIDTH_PX,
            )
        } else {
            // Going to preview: prefer a known geometry from the render cache if present.
            // Fall back to current effective (so we at least continue smoothly) or the loading estimate.
            view.cache
                .mermaid_renders
                .preview_dimensions(block_id, content_version, crate::theme::active_theme(cx))
                .map(|dims| {
                    // Replicate the geometry math from render.rs at 1.0 scale (no extra padding here).
                    let natural_w = dims.width.max(1) as f32;
                    let natural_h = dims.height.max(1) as f32;
                    let max_w = cditor_core::layout::BODY_BLOCK_CONTENT_WIDTH_PX as f32
                        - 2.0 * crate::block::chrome::BLOCK_CONTENT_BORDER_WIDTH_PX;
                    let max_h = 1200.0_f32;
                    let scale = (max_w / natural_w).min(max_h / natural_h).min(1.0);
                    let img_h = natural_h * scale;
                    let body_h = img_h + 64.0; // 2 * MERMAID_PREVIEW_PADDING_Y
                    let chrome = cditor_core::layout::MERMAID_TOOLBAR_HEIGHT_PX
                        + cditor_core::layout::COMPLEX_BLOCK_SHELL_CHROME_HEIGHT_PX;
                    (chrome + body_h as f64).max(1.0)
                })
                .unwrap_or(context.effective_height)
        };

        let tween = match view.overlay.mermaid_source_tweens.remove(&block_id) {
            Some(mut existing) => {
                existing.tween = existing.tween.retarget(target_height, now);
                existing.content_version = content_version;
                existing
            }
            None => crate::features::code::CodeCollapseTween {
                tween: crate::features::code::HeightTween::new(
                    context.effective_height,
                    target_height,
                    now,
                ),
                content_version,
            },
        };

        let first_h = tween.tween.height(now);
        view.overlay.mermaid_source_tweens.insert(block_id, tween);

        let _ = view.ready_session().and_then(|session| {
            session
                .apply_measured_block_height(block_id, content_version, first_h)
                .ok()
        });
    }

    cx.notify();
}

pub(crate) fn show_focused_source_after_enter(
    view: &mut CditorV2View,
    cx: &mut Context<CditorV2View>,
) -> bool {
    let Some((block_id, _)) = view
        .ready_session()
        .and_then(|session| session.focused_block_kind().ok().flatten())
    else {
        return false;
    };
    show_source_after_creation(view, block_id, cx)
}

pub(crate) fn show_source_after_creation(
    view: &mut CditorV2View,
    block_id: BlockId,
    cx: &mut Context<CditorV2View>,
) -> bool {
    let is_mermaid = view.ready_session().is_some_and(|session| {
        session.text_block_context(block_id).is_ok_and(|context| {
            context.is_some_and(|context| {
                matches!(context.kind, cditor_core::rich_text::RichBlockKind::Mermaid)
            })
        })
    });
    if !is_mermaid || !view.cache.mermaid_source_blocks.insert(block_id) {
        return false;
    }

    crate::features::media::invalidate_rendered_media_height_report(block_id);
    cx.notify();
    true
}

fn toggle_source_visibility(visible_sources: &mut HashSet<BlockId>, block_id: BlockId) -> bool {
    if visible_sources.remove(&block_id) {
        false
    } else {
        visible_sources.insert(block_id);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_visibility_toggle_is_reversible_and_block_scoped() {
        let mut visible_sources = HashSet::from([7]);

        assert!(toggle_source_visibility(&mut visible_sources, 9));
        assert_eq!(visible_sources, HashSet::from([7, 9]));
        assert!(!toggle_source_visibility(&mut visible_sources, 9));
        assert_eq!(visible_sources, HashSet::from([7]));
    }
}
