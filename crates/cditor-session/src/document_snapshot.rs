use cditor_core::{
    edit::DocumentSelection,
    ids::BlockId,
    rich_text::{BlockAttrs, DocumentMetadata, PageCover, PageIcon, RichBlockKind},
};
use cditor_editor_protocol::{ProtocolError, ProtocolErrorCode};
use cditor_runtime::DocumentRuntime;
use std::collections::HashSet;

use crate::EditorSessionHandle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDocumentSnapshot {
    pub document_id: u64,
    pub name: Option<String>,
    pub title: Option<String>,
    pub title_from_heading: bool,
    pub cover: Option<PageCover>,
    pub icon: Option<PageIcon>,
    pub revision: u64,
    pub block_count: usize,
    pub readonly: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub focused_block_id: Option<BlockId>,
    pub has_document_text_selection: bool,
    pub has_entire_document_text_selection: bool,
    pub selection: Option<DocumentSelection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextBlockContextSnapshot {
    pub block_id: BlockId,
    pub kind: RichBlockKind,
    pub folded: bool,
    pub text: String,
    pub caret: Option<usize>,
    pub content_version: u64,
}

pub fn project_document_snapshot(
    runtime: &DocumentRuntime,
    readonly: bool,
) -> SessionDocumentSnapshot {
    SessionDocumentSnapshot {
        document_id: runtime.document_id(),
        name: runtime.document_name().map(ToOwned::to_owned),
        title: runtime.document_title().map(ToOwned::to_owned),
        title_from_heading: false,
        cover: runtime.page_cover().cloned(),
        icon: runtime.page_icon().cloned(),
        revision: runtime.revision(),
        block_count: runtime.document_block_count(),
        readonly,
        can_undo: !readonly && runtime.can_undo(),
        can_redo: !readonly && runtime.can_redo(),
        focused_block_id: runtime.focused_block_id(),
        has_document_text_selection: runtime.has_document_text_selection(),
        has_entire_document_text_selection: runtime.has_entire_document_text_selection(),
        selection: runtime.document_selection_snapshot(),
    }
}

pub fn project_selected_text(runtime: &DocumentRuntime) -> Option<String> {
    runtime.selected_focused_text()
}

pub fn project_block_attrs(runtime: &DocumentRuntime, block_id: BlockId) -> Option<BlockAttrs> {
    runtime
        .block_payload_record(block_id)
        .map(|_| runtime.block_attrs(block_id))
}

pub fn project_whiteboard_scene(runtime: &DocumentRuntime, block_id: BlockId) -> Option<String> {
    let payload = runtime.block_payload_record(block_id)?;
    let cditor_core::rich_text::BlockPayload::Whiteboard(whiteboard) = &payload.payload else {
        return None;
    };
    Some(whiteboard.scene_json.clone())
}

pub fn project_text_block_context(
    runtime: &DocumentRuntime,
    block_id: BlockId,
) -> Option<TextBlockContextSnapshot> {
    let payload = runtime.block_payload_record(block_id)?;
    let text = payload.plain_text();
    Some(TextBlockContextSnapshot {
        block_id,
        kind: payload.kind,
        folded: runtime.is_block_folded(block_id),
        text,
        caret: runtime.caret_offset_for_block(block_id),
        content_version: payload.content_version,
    })
}

pub fn project_visible_block_subset(
    runtime: &DocumentRuntime,
    candidate_block_ids: &[BlockId],
) -> HashSet<BlockId> {
    let visible_block_ids = runtime.visible_block_ids();
    candidate_block_ids
        .iter()
        .copied()
        .filter(|block_id| visible_block_ids.contains(block_id))
        .collect()
}

pub fn project_focused_text_block_context(
    runtime: &DocumentRuntime,
) -> Option<TextBlockContextSnapshot> {
    project_text_block_context(runtime, runtime.focused_block_id()?)
}

pub fn project_focused_block_kind(runtime: &DocumentRuntime) -> Option<(BlockId, RichBlockKind)> {
    let block_id = runtime.focused_block_id()?;
    runtime.block_kind(block_id).map(|kind| (block_id, kind))
}

impl EditorSessionHandle {
    pub fn document_metadata(&self) -> Result<DocumentMetadata, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            ProtocolError::new(
                ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        Ok(session.runtime.document_metadata().clone())
    }

    pub fn set_document_name(&self, name: impl Into<String>) -> Result<Option<u64>, ProtocolError> {
        let mut session = self.try_session_mut()?;
        if session.readonly {
            return Err(ProtocolError::new(
                ProtocolErrorCode::PermissionDenied,
                "document is readonly",
            ));
        }
        Ok(session
            .runtime
            .set_document_name(name)
            .then(|| session.runtime.revision()))
    }

    /// Renders the whole document to GitHub-Flavored Markdown.
    ///
    /// The export reads the runtime's full document model. Heavyweight
    /// payloads evicted from the in-memory cache by cache maintenance appear
    /// as placeholders, so a complete export should run on a fresh read-only
    /// session whose payload window covers the whole document.
    pub fn export_markdown(&self) -> Result<String, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            ProtocolError::new(
                ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        let document = session.runtime.rich_text_document();
        Ok(cditor_import_export::markdown::export_plain_markdown(
            &document,
        ))
    }

    /// Returns only the focused block identity and kind, without cloning its payload.
    pub fn focused_block_kind(&self) -> Result<Option<(BlockId, RichBlockKind)>, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            ProtocolError::new(
                ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        Ok(project_focused_block_kind(&session.runtime))
    }

    pub fn committed_block_plain_text(
        &self,
        block_id: BlockId,
    ) -> Result<Option<String>, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| busy_error())?;
        Ok(session
            .runtime
            .loaded_payload_records_snapshot()
            .into_iter()
            .find(|record| record.block_id == block_id)
            .map(|record| record.plain_text()))
    }

    pub fn document_snapshot(&self) -> Result<SessionDocumentSnapshot, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            ProtocolError::new(
                ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        Ok(project_document_snapshot(
            &session.runtime,
            session.readonly,
        ))
    }

    pub fn document_title_block_id(&self) -> Result<Option<BlockId>, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| busy_error())?;
        Ok(session.runtime.document_title_block_id())
    }

    /// Word and line counts computed over the currently loaded payload window.
    pub fn text_statistics(&self) -> Result<(usize, usize), ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            ProtocolError::new(
                ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        let mut word_count = 0usize;
        let mut line_count = 0usize;
        for record in session.runtime.loaded_payload_records_snapshot() {
            let text = record.plain_text();
            line_count +=
                text.bytes().filter(|byte| *byte == b'\n').count() + usize::from(!text.is_empty());
            word_count += text.split_whitespace().count();
        }
        Ok((word_count, line_count))
    }

    pub fn selected_text(&self) -> Result<Option<String>, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            ProtocolError::new(
                ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        Ok(project_selected_text(&session.runtime))
    }

    pub fn text_block_context(
        &self,
        block_id: BlockId,
    ) -> Result<Option<TextBlockContextSnapshot>, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            ProtocolError::new(
                ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        Ok(project_text_block_context(&session.runtime, block_id))
    }

    pub fn block_attrs(&self, block_id: BlockId) -> Result<Option<BlockAttrs>, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            ProtocolError::new(
                ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        Ok(project_block_attrs(&session.runtime, block_id))
    }

    pub fn whiteboard_scene(&self, block_id: BlockId) -> Result<Option<String>, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            ProtocolError::new(
                ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        Ok(project_whiteboard_scene(&session.runtime, block_id))
    }

    pub fn focused_text_block_context(
        &self,
    ) -> Result<Option<TextBlockContextSnapshot>, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            ProtocolError::new(
                ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        Ok(project_focused_text_block_context(&session.runtime))
    }

    pub fn visible_block_subset(
        &self,
        candidate_block_ids: &[BlockId],
    ) -> Result<HashSet<BlockId>, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            ProtocolError::new(
                ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        Ok(project_visible_block_subset(
            &session.runtime,
            candidate_block_ids,
        ))
    }
}

fn busy_error() -> ProtocolError {
    ProtocolError::new(
        ProtocolErrorCode::Busy,
        "editor session is already processing a synchronous request",
    )
    .retryable()
}

#[cfg(test)]
mod tests {
    use cditor_core::rich_text::{
        BlockPayloadRecord, CoverPositionY, DocumentMetadata, PageCover, RichTextDocument,
    };
    use cditor_editor_protocol::command::{CommandEnvelope, CommandSource, EditorCommand};

    use super::*;
    use crate::EditorSession;

    #[test]
    fn export_markdown_renders_heading_paragraph_and_quote() {
        let handle = EditorSession::new(DocumentRuntime::demo(), true).into_handle();
        let markdown = handle.export_markdown().expect("export markdown");
        assert!(markdown.contains("# Cditor"), "heading missing: {markdown}");
        assert!(
            markdown.contains("最小 GPUI 富文本编辑器"),
            "paragraph missing: {markdown}"
        );
        assert!(
            markdown.contains("runtime 才是真相"),
            "quote missing: {markdown}"
        );
    }

    #[test]
    fn export_markdown_on_blank_document_is_not_an_error() {
        let handle = EditorSession::new(DocumentRuntime::empty(), true).into_handle();
        let markdown = handle.export_markdown().expect("export markdown");
        // `empty()` contains a blank system title; Markdown maps that page
        // metadata surface to a top-level heading marker.
        assert!(
            markdown.contains("#"),
            "expected heading marker: {markdown:?}"
        );
    }

    #[test]
    fn document_metadata_returns_an_owned_runtime_snapshot() {
        let mut document = RichTextDocument::empty(9);
        document.metadata = DocumentMetadata {
            name: Some("Preview source".to_owned()),
            created_at: Some("2026-09-01".to_owned()),
            updated_at: Some("2026-09-05".to_owned()),
            tags: vec!["design".to_owned(), "notes".to_owned()],
            cover: Some(PageCover::External {
                url: "https://example.com/cover.jpg".to_owned(),
                position_y: CoverPositionY::CENTER,
            }),
            ..DocumentMetadata::default()
        };
        let handle = EditorSession::new(
            DocumentRuntime::from_rich_text_document(document, 720.0),
            false,
        )
        .into_handle();

        let metadata = handle.document_metadata().expect("document metadata");

        assert_eq!(metadata.name.as_deref(), Some("Preview source"));
        assert_eq!(metadata.tags, ["design", "notes"]);
        assert!(metadata.cover.is_some());
        assert_eq!(metadata.updated_at.as_deref(), Some("2026-09-05"));
    }

    #[test]
    fn document_snapshot_owns_metadata_and_applies_readonly_history_policy() {
        let runtime = DocumentRuntime::demo();
        let block_id = runtime.visible_block_ids()[0];
        let handle = EditorSession::new(runtime, true).into_handle();
        handle
            .dispatch(CommandEnvelope::new(
                EditorCommand::FocusBlock { block_id },
                CommandSource::Sdk,
            ))
            .unwrap();

        let snapshot = handle.document_snapshot().unwrap();
        assert_eq!(snapshot.document_id, handle.snapshot().unwrap().document_id);
        assert!(snapshot.readonly);
        assert!(!snapshot.can_undo);
        assert!(!snapshot.can_redo);
        assert_eq!(snapshot.focused_block_id, Some(block_id));
        assert!(!snapshot.has_document_text_selection);
        assert!(!snapshot.has_entire_document_text_selection);
        assert!(snapshot.selection.is_some());
    }

    #[test]
    fn text_block_context_is_owned_and_bounded_to_one_payload() {
        let runtime = DocumentRuntime::demo();
        let block_id = runtime.visible_block_ids()[0];
        let expected = runtime.block_payload_record(block_id).unwrap();
        let handle = EditorSession::new(runtime, false).into_handle();
        handle
            .dispatch(CommandEnvelope::new(
                EditorCommand::FocusBlock { block_id },
                CommandSource::Sdk,
            ))
            .unwrap();

        let context = handle.focused_text_block_context().unwrap().unwrap();
        assert_eq!(context.block_id, block_id);
        assert_eq!(context.kind, expected.kind);
        assert!(!context.folded);
        assert_eq!(context.text, expected.plain_text());
        assert_eq!(context.content_version, expected.content_version);
        assert!(context.caret.is_some());
        assert!(handle.block_attrs(block_id).unwrap().is_some());
        assert!(handle.block_attrs(u64::MAX).unwrap().is_none());
    }

    #[test]
    fn focused_block_kind_projects_identity_without_text_context() {
        let block_id = 41;
        let runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                block_id,
                RichBlockKind::Code {
                    language: Some("rust".to_owned()),
                },
                "fn main() {}",
            )],
            720.0,
        );
        let handle = EditorSession::new(runtime, false).into_handle();
        handle
            .dispatch(CommandEnvelope::new(
                EditorCommand::FocusBlock { block_id },
                CommandSource::Sdk,
            ))
            .unwrap();

        let focused = handle.focused_block_kind().unwrap();

        assert_eq!(
            focused,
            Some((
                block_id,
                RichBlockKind::Code {
                    language: Some("rust".to_owned()),
                }
            ))
        );
    }

    #[test]
    fn visible_subset_is_bounded_by_the_requested_cache_keys() {
        let runtime = DocumentRuntime::demo();
        let visible = runtime.visible_block_ids().to_vec();
        let handle = EditorSession::new(runtime, false).into_handle();

        let subset = handle
            .visible_block_subset(&[visible[0], u64::MAX])
            .unwrap();
        assert_eq!(subset, HashSet::from([visible[0]]));
    }

    #[test]
    fn whiteboard_scene_query_returns_owned_scene_only_for_whiteboards() {
        let whiteboard_id = 1;
        let paragraph_id = 2;
        let runtime = DocumentRuntime::from_payloads(
            9,
            vec![
                BlockPayloadRecord {
                    block_id: whiteboard_id,
                    content_version: 1,
                    kind: RichBlockKind::Whiteboard,
                    payload: cditor_core::rich_text::BlockPayload::Whiteboard(
                        cditor_core::rich_text::WhiteboardPayload {
                            scene_json: "{}".to_owned(),
                        },
                    ),
                },
                BlockPayloadRecord::rich_text(paragraph_id, RichBlockKind::Paragraph, "paragraph"),
            ],
            720.0,
        );
        let handle = EditorSession::new(runtime, false).into_handle();

        let scene = handle.whiteboard_scene(whiteboard_id).unwrap().unwrap();
        assert!(!scene.is_empty());
        assert!(handle.whiteboard_scene(paragraph_id).unwrap().is_none());
    }
}
