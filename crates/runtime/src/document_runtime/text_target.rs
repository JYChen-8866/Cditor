use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FocusedTextEdit {
    pub(super) target: InputTarget,
    pub(super) text: String,
    pub(super) range: Range<usize>,
}

impl DocumentRuntime {
    pub(super) fn resolve_focused_text_edit(
        &self,
        explicit_range: Option<Range<usize>>,
    ) -> Option<FocusedTextEdit> {
        let target = self.focused_text_target()?;
        let text = self.base_text_for_target(target)?;
        let range = explicit_range
            .or_else(|| {
                self.active_composition()
                    .filter(|composition| composition.block_id == target.block_id())
                    .map(|composition| {
                        composition.range_start as usize..composition.range_end as usize
                    })
            })
            .or_else(|| self.input_session_selected_range())
            .or_else(|| match target {
                InputTarget::BlockText { .. } => self.focused_text_selection_range(),
                InputTarget::TableCell { .. } => None,
                // Complex blocks and block chrome don't have text selection
                InputTarget::ComplexBlock { .. } | InputTarget::BlockChrome { .. } => None,
            })
            .unwrap_or_else(|| {
                let offset = match target {
                    InputTarget::BlockText { block_id } => {
                        self.caret_offset_for_block(block_id).unwrap_or(text.len())
                    }
                    InputTarget::TableCell { block_id, row, col } => self
                        .focused_table_cell
                        .filter(|cell| {
                            cell.block_id == block_id && cell.row == row && cell.col == col
                        })
                        .map(|cell| cell.offset)
                        .unwrap_or(text.len()),
                    // Complex blocks and block chrome have no caret
                    InputTarget::ComplexBlock { .. } | InputTarget::BlockChrome { .. } => {
                        text.len()
                    }
                };
                offset..offset
            });
        let range = normalized_grapheme_range(&text, range);
        Some(FocusedTextEdit {
            target,
            text,
            range,
        })
    }

    pub(super) fn focused_text_target(&self) -> Option<InputTarget> {
        if let Some(cell) = self.focused_table_cell {
            return Some(InputTarget::TableCell {
                block_id: cell.block_id,
                row: cell.row,
                col: cell.col,
            });
        }
        self.input_session_target().or_else(|| {
            self.focused_block_id()
                .map(|block_id| InputTarget::BlockText { block_id })
        })
    }

    pub(super) fn base_text_for_target(&self, target: InputTarget) -> Option<String> {
        match target {
            InputTarget::BlockText { block_id } => self
                .text_models
                .get(&block_id)
                .map(|model| model.text().to_owned()),
            InputTarget::TableCell { block_id, row, col } => {
                self.table_cell_plain_text(block_id, row, col)
            }
            // Complex blocks and block chrome don't have editable text
            InputTarget::ComplexBlock { .. } | InputTarget::BlockChrome { .. } => None,
        }
    }
}

pub(super) fn normalized_grapheme_range(text: &str, range: Range<usize>) -> Range<usize> {
    let offsets = TextOffsetMap::build(text);
    let range = offsets
        .normalize_internal_range(InternalTextOffset(range.start)..InternalTextOffset(range.end));
    range.start.0..range.end.0
}

pub(super) fn normalized_grapheme_offset(text: &str, offset: usize) -> usize {
    normalized_grapheme_range(text, offset..offset).start
}
