use cditor_core::edit::{InnerSelectionAnchor, SelectionEndpoint, UnifiedDocumentSelection};
use cditor_core::ids::BlockId;

use super::{DocumentRuntime, state::FocusedInnerSelection};

impl DocumentRuntime {
    pub fn set_focused_inner_selection(
        &mut self,
        block_id: BlockId,
        anchor: InnerSelectionAnchor,
        focus: InnerSelectionAnchor,
    ) -> Result<bool, String> {
        let kind = self
            .payload_window
            .get(block_id)
            .map(|payload| payload.kind.clone())
            .ok_or_else(|| format!("missing payload for block {block_id}"))?;
        let accepts = match kind {
            cditor_core::rich_text::RichBlockKind::Code { .. } => {
                matches!(
                    anchor,
                    InnerSelectionAnchor::TextOffset(_) | InnerSelectionAnchor::CodeLine { .. }
                ) && matches!(
                    focus,
                    InnerSelectionAnchor::TextOffset(_) | InnerSelectionAnchor::CodeLine { .. }
                )
            }
            cditor_core::rich_text::RichBlockKind::Whiteboard => {
                matches!(anchor, InnerSelectionAnchor::CanvasPoint { .. })
                    && matches!(focus, InnerSelectionAnchor::CanvasPoint { .. })
            }
            _ => false,
        };
        if !accepts {
            return Err(format!(
                "inner selection is incompatible with block kind {kind:?}"
            ));
        }
        if self.focused_block_id() != Some(block_id) {
            self.try_focus_block(block_id)?;
        }
        let changed = self
            .focused_inner_selection
            .as_ref()
            .is_none_or(|selection| {
                selection.block_id != block_id
                    || selection.anchor != anchor
                    || selection.focus != focus
            });
        self.break_typing_coalescing();
        self.document_selection = None;
        self.selected_block_ids.clear();
        self.focused_text_selection = None;
        self.focused_table_cell = None;
        self.focused_inner_selection = Some(FocusedInnerSelection {
            block_id,
            anchor,
            focus,
        });
        Ok(changed)
    }

    pub fn clear_focused_inner_selection(&mut self) -> bool {
        self.focused_inner_selection.take().is_some()
    }

    pub fn unified_document_selection_snapshot(&self) -> Option<UnifiedDocumentSelection> {
        if let Some(selection) = self.document_selection {
            return Some(selection.unified());
        }
        if !self.selected_block_ids.is_empty() {
            let mut positions = self
                .selected_block_ids
                .iter()
                .filter_map(|block_id| {
                    self.index
                        .index_of(*block_id)
                        .map(|index| (index, *block_id))
                })
                .collect::<Vec<_>>();
            positions.sort_unstable_by_key(|(index, _)| *index);
            return Some(UnifiedDocumentSelection {
                anchor: SelectionEndpoint::Block {
                    block_id: positions.first()?.1,
                },
                focus: SelectionEndpoint::Block {
                    block_id: positions.last()?.1,
                },
            });
        }
        if let Some(selection) = &self.focused_inner_selection {
            return Some(UnifiedDocumentSelection {
                anchor: SelectionEndpoint::Inner {
                    block_id: selection.block_id,
                    anchor: selection.anchor.clone(),
                },
                focus: SelectionEndpoint::Inner {
                    block_id: selection.block_id,
                    anchor: selection.focus.clone(),
                },
            });
        }
        let cell = self.focused_table_cell?;
        let anchor_offset = if cell.selection_reversed {
            cell.selected_range_end
        } else {
            cell.selected_range_start
        };
        Some(UnifiedDocumentSelection {
            anchor: SelectionEndpoint::Inner {
                block_id: cell.block_id,
                anchor: InnerSelectionAnchor::TableCell {
                    row: cell.row,
                    col: cell.col,
                    offset: anchor_offset,
                },
            },
            focus: SelectionEndpoint::Inner {
                block_id: cell.block_id,
                anchor: InnerSelectionAnchor::TableCell {
                    row: cell.row,
                    col: cell.col,
                    offset: cell.offset,
                },
            },
        })
    }
}
