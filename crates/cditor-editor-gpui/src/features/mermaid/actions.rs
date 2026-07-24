use std::collections::HashSet;

use cditor_core::ids::BlockId;
use gpui::Context;

use crate::editor_view::CditorV2View;

pub(crate) fn toggle_source_from_gui(
    view: &mut CditorV2View,
    block_id: BlockId,
    cx: &mut Context<CditorV2View>,
) {
    crate::features::media::invalidate_rendered_media_height_report(block_id);
    toggle_source_visibility(&mut view.cache.mermaid_source_blocks, block_id);
    cx.notify();
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
