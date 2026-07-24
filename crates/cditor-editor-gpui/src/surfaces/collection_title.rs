use cditor_core::ids::{BlockId, SurfaceId};
use cditor_session::SurfaceVersionSnapshot;

use crate::editor_view::CditorV2View;
use crate::text::RichTextPlatformLayout;

pub(crate) const fn surface_id(block_id: BlockId) -> SurfaceId {
    SurfaceId::CollectionTitle { block_id }
}

pub(crate) fn current_layout(
    view: &CditorV2View,
    current: SurfaceVersionSnapshot,
) -> Option<&RichTextPlatformLayout> {
    let SurfaceId::CollectionTitle { .. } = current.surface_id else {
        return None;
    };
    let cache = view.cache.text_surface_layouts.get(&current.surface_id)?;
    super::text::layout_cache_is_current(cache, current).then_some(cache)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_title_surface_identity_keeps_owning_block() {
        assert_eq!(surface_id(17), SurfaceId::CollectionTitle { block_id: 17 });
    }
}
