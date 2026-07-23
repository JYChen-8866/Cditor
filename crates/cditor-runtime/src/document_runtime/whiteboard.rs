use super::*;

pub(super) fn default_whiteboard_payload() -> BlockPayload {
    BlockPayload::Whiteboard(cditor_core::rich_text::WhiteboardPayload::default())
}

impl DocumentRuntime {
    pub(crate) fn update_whiteboard_scene_json(
        &mut self,
        block_id: BlockId,
        scene_json: impl Into<String>,
    ) -> Result<bool, String> {
        let scene_json = scene_json.into();
        let record = self
            .document
            .payload_window
            .get(block_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {block_id}"))?;
        let BlockPayload::Whiteboard(mut whiteboard) = record.payload else {
            return Err(format!("block {block_id} is not a whiteboard"));
        };
        if whiteboard.scene_json == scene_json {
            return Ok(false);
        }
        whiteboard.scene_json = scene_json;
        self.apply_local_block_payload_transaction(
            block_id,
            EditTransactionKind::ExplicitCommand,
            record.kind,
            BlockPayload::Whiteboard(whiteboard),
        )
    }
}
