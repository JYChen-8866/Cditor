use super::*;

pub(super) fn default_whiteboard_payload() -> BlockPayload {
    BlockPayload::Whiteboard(cditor_core::rich_text::WhiteboardPayload::default())
}

impl DocumentRuntime {
    pub fn update_whiteboard_scene_json(
        &mut self,
        block_id: BlockId,
        scene_json: impl Into<String>,
    ) -> Result<bool, String> {
        let scene_json = scene_json.into();
        let payload = self
            .payload_window
            .payloads
            .get_mut(&block_id)
            .ok_or_else(|| format!("missing payload for block {block_id}"))?;
        let BlockPayload::Whiteboard(whiteboard) = &mut payload.payload else {
            return Err(format!("block {block_id} is not a whiteboard"));
        };
        if whiteboard.scene_json == scene_json {
            return Ok(false);
        }
        whiteboard.scene_json = scene_json;
        payload.content_version = payload.content_version.saturating_add(1);
        if let Some(editing) = self.editing.as_mut() {
            if editing.block_id == block_id {
                editing.content_version = payload.content_version;
            }
        }
        Ok(true)
    }
}
