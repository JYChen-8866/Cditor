use super::*;

impl DocumentRuntime {
    pub fn focus_block(&mut self, block_id: BlockId) {
        let previous_focus = self.focused_block_id();
        let text_len = self
            .text_models
            .get(&block_id)
            .map(PieceTableTextModel::len)
            .unwrap_or(0);
        trace_input(
            "focus_block",
            format_args!(
                "previous_focus={previous_focus:?} next_block={block_id} caret_to_text_len={text_len}"
            ),
        );
        self.selected_block_ids.clear();
        self.document_selection = None;
        self.focused_text_selection = None;
        self.editing = Some(EditingSession::start(
            block_id,
            self.payload_window
                .get(block_id)
                .map(|payload| payload.content_version)
                .unwrap_or(1),
            CaretAnchor {
                block_id,
                text_offset: text_len as u64,
                caret_rect_y_in_block: 0.0,
                viewport_y: 120.0,
            },
        ));
    }

    pub fn focused_block_id(&self) -> Option<BlockId> {
        self.editing.as_ref().map(|editing| editing.block_id)
    }

    pub fn first_visible_block_id(&self) -> Option<BlockId> {
        self.visible_index.id_at_visible_index(0)
    }

    pub fn focused_text(&self) -> Option<&str> {
        let block_id = self.focused_block_id()?;
        self.text_models.get(&block_id).map(|model| model.text())
    }

    pub fn focused_text_owned(&self) -> Option<(BlockId, String)> {
        let block_id = self.focused_block_id()?;
        let text = self.text_models.get(&block_id)?.text().to_owned();
        Some((block_id, text))
    }

    pub fn caret_offset_for_block(&self, block_id: BlockId) -> Option<usize> {
        self.editing
            .as_ref()
            .filter(|editing| editing.block_id == block_id)
            .map(|editing| editing.caret_anchor.text_offset as usize)
    }

    pub fn focus_block_at_offset(
        &mut self,
        block_id: BlockId,
        offset: usize,
    ) -> Result<(), String> {
        self.set_caret_offset(block_id, offset)
    }

    pub fn set_caret_offset(&mut self, block_id: BlockId, offset: usize) -> Result<(), String> {
        if self.focused_block_id() != Some(block_id) {
            self.focus_block(block_id);
        }
        let model = self
            .text_models
            .get(&block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?;
        let offset = previous_char_boundary(model.text(), offset.min(model.len()));
        let previous_caret = self.caret_offset_for_block(block_id);
        let editing = self.editing.as_mut().expect("editing session exists");
        editing.caret_anchor.text_offset = offset as u64;
        editing.caret_anchor.block_id = block_id;
        self.document_selection = None;
        self.focused_text_selection = None;
        trace_input(
            "set_caret_offset",
            format_args!(
                "block={block_id} requested_offset={} clamped_offset={offset} previous_caret={previous_caret:?} text_len={}",
                offset,
                model.len()
            ),
        );
        Ok(())
    }
}
