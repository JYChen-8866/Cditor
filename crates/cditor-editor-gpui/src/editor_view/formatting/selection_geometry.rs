use cditor_core::ids::SurfaceId;
use cditor_session::ToolbarContextSnapshot;
use gpui::{Bounds, Pixels, point};

use crate::editor_view::CditorV2View;

impl CditorV2View {
    pub(crate) fn projected_toolbar_text_selection_bounds(
        &self,
        context: &ToolbarContextSnapshot,
    ) -> Option<Bounds<Pixels>> {
        if !context.cross_block_fragments.is_empty() {
            let session = self.ready_session()?;
            let mut bounds = context.cross_block_fragments.iter().map(|fragment| {
                let block_id = fragment.fragment.block_id;
                let current = session
                    .surface_version(SurfaceId::Block(block_id))
                    .ok()
                    .flatten()?;
                if current.content_version != fragment.content_version {
                    return None;
                }
                self.text_range_bounds_for_block(block_id, fragment.fragment.range.clone())
            });
            let first = bounds.next()??;
            return bounds.try_fold(first, |combined, bounds| {
                Some(union_bounds(combined, bounds?))
            });
        }

        let block_id = context.focused_block_id?;
        let range = context.selected_range.clone()?;
        if range.is_empty() {
            return None;
        }
        let payload = context.focused_payload.as_ref()?;
        let current = self
            .ready_session()?
            .surface_version(SurfaceId::Block(block_id))
            .ok()
            .flatten()?;
        if payload.content_version != current.content_version {
            return None;
        }
        self.text_range_bounds_for_block(block_id, range)
    }
}

fn union_bounds(left: Bounds<Pixels>, right: Bounds<Pixels>) -> Bounds<Pixels> {
    Bounds::from_corners(
        point(left.left().min(right.left()), left.top().min(right.top())),
        point(
            left.right().max(right.right()),
            left.bottom().max(right.bottom()),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{px, size};

    #[test]
    fn union_covers_both_projected_fragments() {
        let left = Bounds::new(point(px(120.0), px(80.0)), size(px(40.0), px(20.0)));
        let right = Bounds::new(point(px(100.0), px(96.0)), size(px(80.0), px(24.0)));

        assert_eq!(
            union_bounds(left, right),
            Bounds::from_corners(point(px(100.0), px(80.0)), point(px(180.0), px(120.0)))
        );
    }
}
