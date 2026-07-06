use crate::core::block::BlockChromeSnapshot;
use crate::core::edit::SelectionRange;
use crate::core::ids::{BlockId, DocumentId};
use crate::core::layout::BlockLayoutMeta;
use crate::core::rich_text::{BlockAttrs, BlockPayloadView, RichBlockKind};
use crate::editor::debug_overlay::DebugOverlaySnapshot;
use crate::editor::scroll::VirtualScrollState;
use crate::editor::window::RenderWindow;

#[derive(Debug, Clone, PartialEq)]
pub struct EditorViewProjection {
    pub document_id: DocumentId,
    pub scroll: VirtualScrollState,
    pub render_window: RenderWindow,
    pub blocks: Vec<ViewBlockSnapshot>,
    pub before_window_height: f64,
    pub placeholder_window_height: Option<f64>,
    pub after_window_height: f64,
    pub total_visible_blocks: usize,
    pub debug: DebugOverlaySnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ViewBlockSnapshot {
    pub block_id: BlockId,
    pub visible_index: usize,
    pub depth: u16,
    pub chrome: BlockChromeSnapshot,
    pub kind: RichBlockKind,
    pub attrs: BlockAttrs,
    pub payload: BlockPayloadView,
    pub layout: BlockLayoutMeta,
    pub selected: bool,
    pub selection_range: Option<SelectionRange>,
    pub focused: bool,
    pub caret_offset: Option<usize>,
    pub marked_range: Option<std::ops::Range<usize>>,
    pub pinned: bool,
    pub placeholder: bool,
}
