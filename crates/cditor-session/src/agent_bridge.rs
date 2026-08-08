//! Agent bridge — connects AI document tools to the live editor runtime.

use cditor_core::ids::BlockId;
use cditor_core::rich_text::{BlockPayload, RichBlockKind};
use cditor_editor_protocol::{ProtocolError, ProtocolErrorCode};
use cditor_import_export::markdown::parse_gfm_table;
use cditor_runtime::{AgentBlockOutline, DocumentRuntime};

use crate::EditorSessionHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentOutlineRequest {
    pub max_blocks: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentOutline {
    pub document_id: u64,
    pub revision: u64,
    pub structure_version: u64,
    pub title: Option<String>,
    pub blocks: Vec<AgentBlockOutline>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentEditOperation {
    InsertHeadingAfter {
        after_block_id: BlockId,
        level: u8,
        text: String,
    },
    InsertCodeBlockAfter {
        after_block_id: BlockId,
        language: Option<String>,
        text: String,
    },
    InsertBlockAfter {
        after_block_id: BlockId,
        text: String,
    },
    InsertBlockAsFirstChild {
        parent_id: BlockId,
        text: String,
    },
    InsertBlockAsLastChild {
        parent_id: BlockId,
        text: String,
    },
    MoveBlockBefore {
        block_id: BlockId,
        previous_block_id: Option<BlockId>,
    },
    MoveBlockToParent {
        block_id: BlockId,
        parent_id: BlockId,
    },
    SetBlockText {
        block_id: BlockId,
        text: String,
    },
    DeleteBlocks {
        block_ids: Vec<BlockId>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEditRequest {
    pub expected_structure_version: Option<u64>,
    pub operations: Vec<AgentEditOperation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEditOutcome {
    pub revision: u64,
    pub structure_version: u64,
    pub new_block_ids: Vec<BlockId>,
    pub changed: bool,
}

pub fn project_agent_outline(
    runtime: &DocumentRuntime,
    request: AgentOutlineRequest,
) -> AgentOutline {
    AgentOutline {
        document_id: runtime.document_id(),
        revision: runtime.revision(),
        structure_version: runtime.structure_version(),
        title: runtime.document_title().map(str::to_owned),
        blocks: runtime.agent_document_outline(request.max_blocks),
    }
}

pub fn project_agent_edit(
    runtime: &mut DocumentRuntime,
    request: AgentEditRequest,
    readonly: bool,
) -> Result<AgentEditOutcome, ProtocolError> {
    if readonly && !request.operations.is_empty() {
        return Err(
            ProtocolError::new(ProtocolErrorCode::Readonly, "document is read-only")
                .with_document(runtime.document_id()),
        );
    }
    if let Some(expected) = request.expected_structure_version
        && expected != runtime.structure_version()
    {
        return Err(ProtocolError::new(
            ProtocolErrorCode::StalePrecondition,
            format!(
                "expected structure version {expected}, current is {}",
                runtime.structure_version()
            ),
        )
        .with_document(runtime.document_id()));
    }

    let mut new_block_ids = Vec::new();
    let mut changed = false;
    for operation in request.operations {
        match operation {
            AgentEditOperation::InsertHeadingAfter {
                after_block_id,
                level,
                text,
            } => {
                let new_id = runtime
                    .agent_insert_heading_after(after_block_id, level, &text)
                    .map_err(agent_apply_error)?;
                new_block_ids.push(new_id);
                changed = true;
            }
            AgentEditOperation::InsertBlockAfter {
                after_block_id,
                text,
            } => {
                // A GFM table inserted as plain text must become a real Table
                // block instead of leaving raw `| ... |` markdown visible.
                let new_id = if let Some(table) = parse_gfm_table(&text) {
                    runtime
                        .agent_insert_block_payload_after(
                            after_block_id,
                            RichBlockKind::Table,
                            BlockPayload::Table(table),
                        )
                        .map_err(agent_apply_error)?
                } else {
                    runtime
                        .agent_insert_block_after(after_block_id, &text)
                        .map_err(agent_apply_error)?
                };
                new_block_ids.push(new_id);
                changed = true;
            }
            AgentEditOperation::InsertCodeBlockAfter {
                after_block_id,
                language,
                text,
            } => {
                let new_id = runtime
                    .agent_insert_code_block_after(after_block_id, language.as_deref(), &text)
                    .map_err(agent_apply_error)?;
                new_block_ids.push(new_id);
                changed = true;
            }
            AgentEditOperation::InsertBlockAsFirstChild { parent_id, text } => {
                let new_id = runtime
                    .agent_insert_block_as_first_child(parent_id, &text)
                    .map_err(agent_apply_error)?;
                new_block_ids.push(new_id);
                changed = true;
            }
            AgentEditOperation::InsertBlockAsLastChild { parent_id, text } => {
                let new_id = runtime
                    .agent_insert_block_as_last_child(parent_id, &text)
                    .map_err(agent_apply_error)?;
                new_block_ids.push(new_id);
                changed = true;
            }
            AgentEditOperation::MoveBlockBefore {
                block_id,
                previous_block_id,
            } => {
                runtime
                    .agent_move_block_before(block_id, previous_block_id)
                    .map_err(agent_apply_error)?;
                changed = true;
            }
            AgentEditOperation::MoveBlockToParent {
                block_id,
                parent_id,
            } => {
                runtime
                    .agent_move_block_to_parent(block_id, parent_id)
                    .map_err(agent_apply_error)?;
                changed = true;
            }
            AgentEditOperation::SetBlockText { block_id, text } => {
                runtime
                    .agent_set_block_text(block_id, &text)
                    .map_err(agent_apply_error)?;
                changed = true;
            }
            AgentEditOperation::DeleteBlocks { block_ids } => {
                let deleted = runtime
                    .agent_delete_blocks(&block_ids)
                    .map_err(agent_apply_error)?;
                new_block_ids.extend(deleted);
                changed = true;
            }
        }
    }

    Ok(AgentEditOutcome {
        revision: runtime.revision(),
        structure_version: runtime.structure_version(),
        new_block_ids,
        changed,
    })
}

fn agent_apply_error(message: String) -> ProtocolError {
    ProtocolError::new(ProtocolErrorCode::ApplyFailed, message)
}

impl EditorSessionHandle {
    pub fn agent_outline(
        &self,
        request: AgentOutlineRequest,
    ) -> Result<AgentOutline, ProtocolError> {
        let session = self
            .inner
            .try_borrow()
            .map_err(|_| crate::session::busy_error())?;
        Ok(project_agent_outline(&session.runtime, request))
    }

    pub fn agent_edit(&self, request: AgentEditRequest) -> Result<AgentEditOutcome, ProtocolError> {
        let mut session = self.try_session_mut()?;
        let readonly = session.readonly;
        project_agent_edit(&mut session.runtime, request, readonly)
    }
}

#[cfg(test)]
mod tests {
    use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};
    use cditor_runtime::DocumentRuntime;

    use super::*;
    use crate::EditorSession;

    fn outline_runtime() -> DocumentRuntime {
        DocumentRuntime::from_payloads(
            1,
            vec![
                BlockPayloadRecord::rich_text(1, RichBlockKind::Heading { level: 1 }, "First"),
                BlockPayloadRecord::rich_text(2, RichBlockKind::Heading { level: 1 }, "Second"),
                BlockPayloadRecord::rich_text(3, RichBlockKind::Paragraph, "body"),
            ],
            720.0,
        )
    }

    #[test]
    fn agent_outline_projects_blocks_in_document_order() {
        let handle = EditorSession::new(outline_runtime(), false).into_handle();
        let outline = handle
            .agent_outline(AgentOutlineRequest { max_blocks: 100 })
            .unwrap();
        assert_eq!(outline.document_id, 1);
        assert_eq!(
            outline
                .blocks
                .iter()
                .map(|block| block.block_id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert!(matches!(
            outline.blocks[1].kind,
            RichBlockKind::Heading { level: 1 }
        ));
        assert_eq!(outline.blocks[1].text, "Second");
    }

    #[test]
    fn agent_edit_inserts_heading_after_second_h1_with_ai_origin() {
        let handle = EditorSession::new(outline_runtime(), false).into_handle();
        let before = handle
            .agent_outline(AgentOutlineRequest { max_blocks: 100 })
            .unwrap();
        let outcome = handle
            .agent_edit(AgentEditRequest {
                expected_structure_version: Some(before.structure_version),
                operations: vec![AgentEditOperation::InsertHeadingAfter {
                    after_block_id: 2,
                    level: 2,
                    text: "实现方案".to_owned(),
                }],
            })
            .unwrap();
        assert!(outcome.changed);
        assert_eq!(outcome.new_block_ids, vec![4]);
        assert!(outcome.revision > before.revision);

        let after = handle
            .agent_outline(AgentOutlineRequest { max_blocks: 100 })
            .unwrap();
        let inserted = after
            .blocks
            .iter()
            .find(|block| block.block_id == 4)
            .expect("inserted heading is visible in outline");
        assert!(matches!(inserted.kind, RichBlockKind::Heading { level: 2 }));
        assert_eq!(inserted.text, "实现方案");
    }

    #[test]
    fn agent_edit_inserts_code_block_after_first_h1() {
        let handle = EditorSession::new(outline_runtime(), false).into_handle();
        let before = handle
            .agent_outline(AgentOutlineRequest { max_blocks: 100 })
            .unwrap();
        let outcome = handle
            .agent_edit(AgentEditRequest {
                expected_structure_version: Some(before.structure_version),
                operations: vec![AgentEditOperation::InsertCodeBlockAfter {
                    after_block_id: 1,
                    language: Some("rust".to_owned()),
                    text: "fn main() {}".to_owned(),
                }],
            })
            .unwrap();
        assert_eq!(outcome.new_block_ids, vec![4]);

        let after = handle
            .agent_outline(AgentOutlineRequest { max_blocks: 100 })
            .unwrap();
        let inserted = after
            .blocks
            .iter()
            .find(|block| block.block_id == 4)
            .expect("inserted code block is visible in outline");
        assert!(matches!(
            inserted.kind,
            RichBlockKind::Code {
                language: Some(ref language)
            } if language == "rust"
        ));
        assert_eq!(inserted.text, "fn main() {}");
    }

    #[test]
    fn agent_edit_canonicalizes_mermaid_code_language_to_mermaid_block() {
        let handle = EditorSession::new(outline_runtime(), false).into_handle();
        let before = handle
            .agent_outline(AgentOutlineRequest { max_blocks: 100 })
            .unwrap();
        let source = "flowchart TD\n  A --> B";
        let outcome = handle
            .agent_edit(AgentEditRequest {
                expected_structure_version: Some(before.structure_version),
                operations: vec![AgentEditOperation::InsertCodeBlockAfter {
                    after_block_id: 1,
                    language: Some(" Mermaid ".to_owned()),
                    text: source.to_owned(),
                }],
            })
            .unwrap();
        assert_eq!(outcome.new_block_ids, vec![4]);

        let after = handle
            .agent_outline(AgentOutlineRequest { max_blocks: 100 })
            .unwrap();
        let inserted = after
            .blocks
            .iter()
            .find(|block| block.block_id == 4)
            .expect("inserted Mermaid block is visible in outline");
        assert_eq!(inserted.kind, RichBlockKind::Mermaid);
        assert_eq!(inserted.text, source);
    }

    #[test]
    fn agent_edit_inserts_gfm_table_as_table_block() {
        let handle = EditorSession::new(outline_runtime(), false).into_handle();
        let before = handle
            .agent_outline(AgentOutlineRequest { max_blocks: 100 })
            .unwrap();
        let outcome = handle
            .agent_edit(AgentEditRequest {
                expected_structure_version: Some(before.structure_version),
                operations: vec![AgentEditOperation::InsertBlockAfter {
                    after_block_id: 3,
                    text: "| A | B |\n|---|---|\n| 1 | 2 |".to_owned(),
                }],
            })
            .unwrap();
        assert_eq!(outcome.new_block_ids, vec![4]);

        let after = handle
            .agent_outline(AgentOutlineRequest { max_blocks: 100 })
            .unwrap();
        let inserted = after
            .blocks
            .iter()
            .find(|block| block.block_id == 4)
            .expect("inserted table is visible in outline");
        assert_eq!(inserted.kind, RichBlockKind::Table);
    }

    #[test]
    fn agent_edit_keeps_non_table_text_as_paragraph() {
        let handle = EditorSession::new(outline_runtime(), false).into_handle();
        let before = handle
            .agent_outline(AgentOutlineRequest { max_blocks: 100 })
            .unwrap();
        handle
            .agent_edit(AgentEditRequest {
                expected_structure_version: Some(before.structure_version),
                operations: vec![AgentEditOperation::InsertBlockAfter {
                    after_block_id: 3,
                    text: "plain paragraph".to_owned(),
                }],
            })
            .unwrap();

        let after = handle
            .agent_outline(AgentOutlineRequest { max_blocks: 100 })
            .unwrap();
        let inserted = after
            .blocks
            .iter()
            .find(|block| block.block_id == 4)
            .expect("inserted paragraph is visible in outline");
        assert_eq!(inserted.kind, RichBlockKind::Paragraph);
        assert_eq!(inserted.text, "plain paragraph");
    }

    #[test]
    fn agent_edit_rejects_stale_structure_version() {
        let handle = EditorSession::new(outline_runtime(), false).into_handle();
        let before = handle
            .agent_outline(AgentOutlineRequest { max_blocks: 100 })
            .unwrap();
        handle
            .agent_edit(AgentEditRequest {
                expected_structure_version: Some(before.structure_version),
                operations: vec![AgentEditOperation::InsertHeadingAfter {
                    after_block_id: 2,
                    level: 2,
                    text: "one".to_owned(),
                }],
            })
            .unwrap();
        let error = handle
            .agent_edit(AgentEditRequest {
                expected_structure_version: Some(before.structure_version),
                operations: vec![AgentEditOperation::InsertHeadingAfter {
                    after_block_id: 4,
                    level: 3,
                    text: "stale".to_owned(),
                }],
            })
            .unwrap_err();
        assert_eq!(error.code, ProtocolErrorCode::StalePrecondition);
    }

    #[test]
    fn readonly_agent_edit_is_rejected() {
        let handle = EditorSession::new(outline_runtime(), true).into_handle();
        let error = handle
            .agent_edit(AgentEditRequest {
                expected_structure_version: None,
                operations: vec![AgentEditOperation::SetBlockText {
                    block_id: 1,
                    text: "nope".to_owned(),
                }],
            })
            .unwrap_err();
        assert_eq!(error.code, ProtocolErrorCode::Readonly);
    }

    #[test]
    fn agent_edit_inserts_first_and_last_child() {
        let handle = EditorSession::new(outline_runtime(), false).into_handle();
        let before = handle
            .agent_outline(AgentOutlineRequest { max_blocks: 100 })
            .unwrap();
        handle
            .agent_edit(AgentEditRequest {
                expected_structure_version: Some(before.structure_version),
                operations: vec![
                    AgentEditOperation::InsertBlockAsFirstChild {
                        parent_id: 3,
                        text: "first child".to_owned(),
                    },
                    AgentEditOperation::InsertBlockAsLastChild {
                        parent_id: 3,
                        text: "last child".to_owned(),
                    },
                ],
            })
            .unwrap();

        let after = handle
            .agent_outline(AgentOutlineRequest { max_blocks: 100 })
            .unwrap();
        let first = after
            .blocks
            .iter()
            .find(|block| block.text == "first child")
            .expect("first child exists");
        let last = after
            .blocks
            .iter()
            .find(|block| block.text == "last child")
            .expect("last child exists");
        assert_eq!(first.parent_id, Some(3));
        assert_eq!(last.parent_id, Some(3));
        assert!(
            after
                .blocks
                .iter()
                .position(|block| block.block_id == first.block_id)
                < after
                    .blocks
                    .iter()
                    .position(|block| block.block_id == last.block_id)
        );
    }

    #[test]
    fn agent_edit_moves_block_before_and_into_parent() {
        let runtime = DocumentRuntime::from_payloads(
            1,
            vec![
                BlockPayloadRecord::rich_text(1, RichBlockKind::Quote, "quote"),
                BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "a"),
                BlockPayloadRecord::rich_text(3, RichBlockKind::Paragraph, "b"),
            ],
            720.0,
        );
        let handle = EditorSession::new(runtime, false).into_handle();
        let before = handle
            .agent_outline(AgentOutlineRequest { max_blocks: 100 })
            .unwrap();
        handle
            .agent_edit(AgentEditRequest {
                expected_structure_version: Some(before.structure_version),
                operations: vec![AgentEditOperation::MoveBlockBefore {
                    block_id: 3,
                    previous_block_id: Some(1),
                }],
            })
            .unwrap();
        let after = handle
            .agent_outline(AgentOutlineRequest { max_blocks: 100 })
            .unwrap();
        assert_eq!(after.blocks[0].block_id, 3);
        assert_eq!(after.blocks[1].block_id, 1);

        let before = handle
            .agent_outline(AgentOutlineRequest { max_blocks: 100 })
            .unwrap();
        handle
            .agent_edit(AgentEditRequest {
                expected_structure_version: Some(before.structure_version),
                operations: vec![AgentEditOperation::MoveBlockToParent {
                    block_id: 2,
                    parent_id: 1,
                }],
            })
            .unwrap();
        let after = handle
            .agent_outline(AgentOutlineRequest { max_blocks: 100 })
            .unwrap();
        let moved = after
            .blocks
            .iter()
            .find(|block| block.block_id == 2)
            .expect("moved block exists");
        assert_eq!(moved.parent_id, Some(1));
    }
}
