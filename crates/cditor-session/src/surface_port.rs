use cditor_core::ids::SurfaceId;
use cditor_editor_protocol::{ProtocolError, ProtocolErrorCode};
use cditor_runtime::DocumentRuntime;

use crate::EditorSessionHandle;

/// Bounded identity used to validate a UI-owned layout cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceVersionSnapshot {
    pub surface_id: SurfaceId,
    pub content_version: u64,
    pub layout_version: u64,
}

pub fn project_surface_version(
    runtime: &DocumentRuntime,
    surface_id: SurfaceId,
) -> Option<SurfaceVersionSnapshot> {
    let block_id = surface_id.block_id()?;
    let content_version = runtime
        .text_surface_snapshot(surface_id)?
        .identity
        .content_version;
    let layout_version = runtime.block_layout_version(block_id)?;
    Some(SurfaceVersionSnapshot {
        surface_id,
        content_version,
        layout_version,
    })
}

impl EditorSessionHandle {
    pub fn surface_version(
        &self,
        surface_id: SurfaceId,
    ) -> Result<Option<SurfaceVersionSnapshot>, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            ProtocolError::new(
                ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        Ok(project_surface_version(&session.runtime, surface_id))
    }
}

#[cfg(test)]
mod tests {
    use cditor_core::rich_text::{
        BlockPayload, BlockPayloadRecord, ImagePayload, RichBlockKind, TableCellPayload,
        TablePayload, TableRowPayload,
    };

    use super::*;
    use crate::EditorSession;

    fn runtime_with_surfaces() -> DocumentRuntime {
        DocumentRuntime::from_payloads(
            1,
            vec![
                BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "body"),
                BlockPayloadRecord {
                    block_id: 2,
                    content_version: 7,
                    kind: RichBlockKind::Table,
                    payload: BlockPayload::Table(TablePayload {
                        rows: vec![TableRowPayload {
                            cells: vec![TableCellPayload::plain("cell")],
                            height: Default::default(),
                        }],
                        columns: Vec::new(),
                        header_rows: 0,
                        header_cols: 0,
                        header_style: Default::default(),
                    }),
                },
                BlockPayloadRecord {
                    block_id: 3,
                    content_version: 11,
                    kind: RichBlockKind::Image,
                    payload: BlockPayload::Image(ImagePayload {
                        caption: "caption".into(),
                        ..Default::default()
                    }),
                },
                BlockPayloadRecord {
                    block_id: 4,
                    content_version: 13,
                    kind: RichBlockKind::Database,
                    payload: BlockPayload::Empty,
                },
            ],
            720.0,
        )
    }

    #[test]
    fn projects_block_and_table_cell_owner_versions() {
        let runtime = runtime_with_surfaces();

        let block = project_surface_version(&runtime, SurfaceId::Block(1)).unwrap();
        assert_eq!(block.content_version, 1);
        assert_eq!(
            block.layout_version,
            runtime.block_layout_version(1).unwrap()
        );

        let cell_id = SurfaceId::TableCell {
            block_id: 2,
            row: 0,
            column: 0,
        };
        let cell = project_surface_version(&runtime, cell_id).unwrap();
        assert_eq!(cell.surface_id, cell_id);
        assert_eq!(cell.content_version, 7);
        assert_eq!(
            cell.layout_version,
            runtime.block_layout_version(2).unwrap()
        );
    }

    #[test]
    fn projects_auxiliary_surface_identity_and_rejects_missing_surfaces() {
        let runtime = runtime_with_surfaces();
        let caption_id = SurfaceId::ImageCaption { block_id: 3 };

        let caption = project_surface_version(&runtime, caption_id).unwrap();
        assert_eq!(caption.surface_id, caption_id);
        assert_eq!(caption.content_version, 11);
        assert_eq!(
            caption.layout_version,
            runtime.block_layout_version(3).unwrap()
        );
        let title_id = SurfaceId::CollectionTitle { block_id: 4 };
        let title = project_surface_version(&runtime, title_id).unwrap();
        assert_eq!(title.surface_id, title_id);
        assert_eq!(title.content_version, 13);
        assert_eq!(
            title.layout_version,
            runtime.block_layout_version(4).unwrap()
        );
        assert_eq!(
            project_surface_version(
                &runtime,
                SurfaceId::TableCell {
                    block_id: 2,
                    row: 8,
                    column: 9,
                }
            ),
            None
        );
        assert_eq!(
            project_surface_version(
                &runtime,
                SurfaceId::Ephemeral {
                    owner_id: 4,
                    local_id: 5,
                }
            ),
            None
        );
    }

    #[test]
    fn handle_returns_owned_version_without_exposing_runtime() {
        let handle = EditorSession::new(runtime_with_surfaces(), false).into_handle();

        let version = handle
            .surface_version(SurfaceId::Block(1))
            .unwrap()
            .unwrap();

        assert_eq!(version.surface_id, SurfaceId::Block(1));
        assert_eq!(version.content_version, 1);
    }
}
