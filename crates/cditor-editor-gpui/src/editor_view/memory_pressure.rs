use std::collections::HashSet;

use cditor_core::ids::BlockId;
use gpui::{App, Bounds, Context, Pixels};

use crate::cache::RetiredRenderResources;
use crate::interaction::geometry::{DocumentViewportOrigin, ProjectedBlockRect};
use crate::{CditorMemoryPressure, CditorViewMemoryTrimReport};

use super::CditorV2View;

#[derive(Default)]
struct MemoryProtectionSnapshot {
    visible: HashSet<BlockId>,
    ui: HashSet<BlockId>,
    focused: Option<BlockId>,
    input_target: Option<BlockId>,
    selection_endpoints: Option<(BlockId, BlockId)>,
    fullscreen_video: Option<BlockId>,
}

impl MemoryProtectionSnapshot {
    fn into_blocks(mut self) -> HashSet<BlockId> {
        self.visible.extend(self.ui);
        self.visible.extend(self.focused);
        self.visible.extend(self.input_target);
        self.visible.extend(
            self.selection_endpoints
                .into_iter()
                .flat_map(|(anchor, focus)| [anchor, focus]),
        );
        self.visible.extend(self.fullscreen_video);
        self.visible
    }
}

impl CditorV2View {
    /// Trims only reconstructible resources owned by this editor view.
    ///
    /// This method never mutates the document runtime, dirty state, input/IME
    /// session, selection, or undo history. Process-wide caches are not touched;
    /// hosts must call `trim_process_reconstructible_caches` once per pressure
    /// event, independently of the number of resident editor views.
    pub fn sdk_apply_memory_pressure(
        &mut self,
        pressure: CditorMemoryPressure,
        cx: &mut Context<Self>,
    ) -> CditorViewMemoryTrimReport {
        let visible = physically_visible_presented_blocks(
            &self.interaction.projected_block_rects,
            self.interaction.document_viewport_origin,
            self.interaction.rendered_editor_viewport_bounds(),
            self.interaction.presented_scroll_top,
        );
        let ui = self.ui_protected_block_ids();
        let fullscreen_video = self.overlay.fullscreen_video_block_id;

        let (focused, input_target, selection_endpoints) = match self.ready_session() {
            Some(session) => {
                let (Ok(ui_snapshot), Ok(document_snapshot)) =
                    (session.ui_snapshot(), session.document_snapshot())
                else {
                    return CditorViewMemoryTrimReport {
                        protected_blocks: visible
                            .union(&ui)
                            .copied()
                            .chain(fullscreen_video)
                            .collect::<HashSet<_>>()
                            .len(),
                        skipped_runtime_busy: true,
                        ..Default::default()
                    };
                };
                (
                    ui_snapshot.focused_block_id,
                    ui_snapshot
                        .input_target
                        .map(cditor_runtime::InputTarget::block_id),
                    document_snapshot
                        .selection
                        .map(|selection| (selection.anchor.block_id, selection.focus.block_id)),
                )
            }
            None => (None, None, None),
        };

        let protected = MemoryProtectionSnapshot {
            visible,
            ui,
            focused,
            input_target,
            selection_endpoints,
            fullscreen_video,
        }
        .into_blocks();
        let protected_blocks = protected.len();
        let trim = self.cache.apply_memory_pressure(pressure, &protected);
        let retired_render_resources = retired_resource_count(&trim.retired);
        retire_render_resources_after_effect(trim.retired, cx);

        CditorViewMemoryTrimReport {
            protected_blocks,
            mermaid_entries_evicted: trim.mermaid_evicted_entries,
            video_entries_evicted: trim.video_evicted_entries,
            platform_geometry_entries_evicted: trim.platform_geometry_evicted_entries,
            retired_render_resources,
            skipped_runtime_busy: false,
        }
    }
}

fn physically_visible_presented_blocks(
    rects: &[ProjectedBlockRect],
    document_origin: Option<DocumentViewportOrigin>,
    viewport: Option<Bounds<Pixels>>,
    presented_scroll_top: f64,
) -> HashSet<BlockId> {
    let (Some(document_origin), Some(viewport)) = (document_origin, viewport) else {
        return rects.iter().map(|rect| rect.block_id).collect();
    };
    let viewport_top = f32::from(viewport.top()) as f64;
    let viewport_bottom = f32::from(viewport.bottom()) as f64;
    rects
        .iter()
        .filter(|rect| {
            let top = document_origin.y + rect.document_top - presented_scroll_top;
            let bottom = document_origin.y + rect.document_bottom - presented_scroll_top;
            bottom > viewport_top && top < viewport_bottom
        })
        .map(|rect| rect.block_id)
        .collect()
}

fn retired_resource_count(resources: &RetiredRenderResources) -> usize {
    let count = resources.images.len();
    #[cfg(feature = "gpui-dynamic-image")]
    let count = count.saturating_add(resources.dynamic_images.len());
    count
}

fn retire_render_resources_after_effect(resources: RetiredRenderResources, cx: &mut App) {
    if retired_resource_count(&resources) == 0 {
        return;
    }
    cx.defer(move |cx| {
        #[cfg(feature = "gpui-dynamic-image")]
        for image in resources.dynamic_images {
            cx.drop_dynamic_image(image, None);
        }
        for image in resources.images {
            cx.drop_image(image, None);
        }
    });
}

#[cfg(test)]
mod tests {
    use gpui::{Bounds, point, px, size};

    use super::*;

    fn rect(block_id: BlockId, top: f64, bottom: f64) -> ProjectedBlockRect {
        ProjectedBlockRect {
            block_id,
            document_top: top,
            document_bottom: bottom,
            ..Default::default()
        }
    }

    #[test]
    fn physical_visibility_uses_presented_scroll_and_half_open_intersection() {
        let rects = [
            rect(1, 0.0, 40.0),
            rect(2, 40.0, 100.0),
            rect(3, 100.0, 160.0),
            rect(4, 160.0, 220.0),
        ];
        let viewport = Bounds::new(point(px(0.0), px(20.0)), size(px(500.0), px(100.0)));

        let protected = physically_visible_presented_blocks(
            &rects,
            Some(DocumentViewportOrigin { x: 0.0, y: 20.0 }),
            Some(viewport),
            40.0,
        );

        assert_eq!(protected, HashSet::from([2, 3]));
    }

    #[test]
    fn missing_presented_geometry_fails_closed_to_the_render_window() {
        let rects = [rect(7, 0.0, 30.0), rect(9, 30.0, 60.0)];

        assert_eq!(
            physically_visible_presented_blocks(&rects, None, None, 0.0),
            HashSet::from([7, 9])
        );
    }

    #[test]
    fn protection_snapshot_merges_every_identity_without_duplicates() {
        let protected = MemoryProtectionSnapshot {
            visible: HashSet::from([1, 2]),
            ui: HashSet::from([2, 3]),
            focused: Some(4),
            input_target: Some(4),
            selection_endpoints: Some((5, 6)),
            fullscreen_video: Some(7),
        }
        .into_blocks();

        assert_eq!(protected, HashSet::from([1, 2, 3, 4, 5, 6, 7]));
    }
}
