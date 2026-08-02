use crate::block::chrome::{
    BLOCK_SHELL_BORDER_WIDTH_PX, BLOCK_SHELL_OUTER_PADDING_X_PX, BlockChromeStyle,
    BlockHorizontalGeometry,
};
use crate::document::DocumentLayoutMetrics;
use crate::features::code::{V1_CODE_TEXT_OFFSET_TOP_PX, V1_CODE_TEXT_OFFSET_X_PX};
use crate::theme::GuiTheme;
use cditor_core::layout::{BlockWidthClass, block_width_class_for_kind};
use cditor_core::rich_text::RichBlockKind;
use cditor_runtime::ViewBlockSnapshot;

pub const DOCUMENT_TRACK_SIDE_INSET_PX: f32 = 48.0;
pub const DOCUMENT_ROOT_CONTENT_SURFACE_LEFT_PX: f32 =
    BlockHorizontalGeometry::for_depth(0).content_surface_left_px;
pub const DOCUMENT_SHELL_RIGHT_INSET_PX: f32 =
    BLOCK_SHELL_BORDER_WIDTH_PX + BLOCK_SHELL_OUTER_PADDING_X_PX;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DocumentBlockGeometry {
    pub width_class: BlockWidthClass,
    pub track_left_px: f32,
    pub track_width_px: f32,
    pub shell_left_px: f32,
    pub shell_width_px: f32,
}

impl DocumentBlockGeometry {
    pub fn for_block(block: &ViewBlockSnapshot, document: DocumentLayoutMetrics) -> Self {
        let width_class = block_width_class_for_kind(&block.kind);
        let content_width_px = block
            .table_view
            .as_ref()
            .filter(|_| matches!(block.kind, RichBlockKind::Table))
            .map(|table| table.width_px.min(width_class.content_width_px() as f32))
            .unwrap_or(width_class.content_width_px() as f32);
        Self::for_content_width(width_class, content_width_px, document)
    }

    #[cfg(test)]
    pub fn for_kind(kind: &RichBlockKind, document: DocumentLayoutMetrics) -> Self {
        Self::for_width_class(block_width_class_for_kind(kind), document)
    }

    #[cfg(test)]
    pub fn for_width_class(width_class: BlockWidthClass, document: DocumentLayoutMetrics) -> Self {
        Self::for_content_width(width_class, width_class.content_width_px() as f32, document)
    }

    fn for_content_width(
        width_class: BlockWidthClass,
        content_width_px: f32,
        document: DocumentLayoutMetrics,
    ) -> Self {
        let available_track_width =
            (document.page_width_px - DOCUMENT_TRACK_SIDE_INSET_PX * 2.0).max(1.0);
        let track_width_px = content_width_px.max(1.0).min(available_track_width);
        let track_left_px = (document.page_width_px - track_width_px) / 2.0;
        let shell_left_px = track_left_px - DOCUMENT_ROOT_CONTENT_SURFACE_LEFT_PX;
        let shell_width_px =
            DOCUMENT_ROOT_CONTENT_SURFACE_LEFT_PX + track_width_px + DOCUMENT_SHELL_RIGHT_INSET_PX;
        Self {
            width_class,
            track_left_px,
            track_width_px,
            shell_left_px,
            shell_width_px,
        }
    }

    pub const fn track_right_px(self) -> f32 {
        self.track_left_px + self.track_width_px
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DocumentTextGeometry {
    pub(crate) origin_x_px: f64,
    pub(crate) origin_y_px: f64,
    pub(crate) width_px: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DocumentTextViewport {
    pub(crate) local_top_px: f32,
    pub(crate) height_px: f32,
}

impl DocumentTextViewport {
    pub(crate) fn for_block(
        block_top_in_window_px: f64,
        text_origin_y_px: f64,
        window_start_global_y: f64,
        scroll_top: f64,
        viewport_height_px: f32,
        top_inset_px: f32,
    ) -> Self {
        let text_top_in_viewport = f64::from(top_inset_px) + window_start_global_y - scroll_top
            + block_top_in_window_px
            + text_origin_y_px;
        Self {
            local_top_px: (-text_top_in_viewport).max(0.0) as f32,
            height_px: viewport_height_px.max(1.0),
        }
    }
}

impl DocumentTextGeometry {
    pub(crate) fn for_block(
        block: &ViewBlockSnapshot,
        theme: GuiTheme,
        document: DocumentLayoutMetrics,
    ) -> Self {
        let chrome = BlockChromeStyle::from_snapshot(block, theme);
        let horizontal = chrome.horizontal_geometry();
        let block_geometry = DocumentBlockGeometry::for_block(block, document);
        let is_code = matches!(block.kind, RichBlockKind::Code { .. });
        let code_x = if is_code {
            f64::from(V1_CODE_TEXT_OFFSET_X_PX)
        } else {
            0.0
        };
        let code_y = if is_code {
            f64::from(V1_CODE_TEXT_OFFSET_TOP_PX)
        } else {
            0.0
        };
        Self {
            origin_x_px: f64::from(block_geometry.shell_left_px + horizontal.text_left_px) + code_x,
            origin_y_px: f64::from(chrome.outer_padding_top_px + BLOCK_SHELL_BORDER_WIDTH_PX)
                + f64::from(chrome.content_padding_y_px)
                + code_y,
            width_px: (f64::from(block_geometry.shell_width_px - horizontal.text_left_px)
                - f64::from(horizontal.content_right_inset_px)
                - code_x * 2.0)
                .max(1.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::layout::BODY_BLOCK_CONTENT_WIDTH_PX;
    use cditor_runtime::DocumentRuntime;

    #[test]
    fn desktop_tracks_share_one_center_and_keep_the_rail_outside_content() {
        let document = DocumentLayoutMetrics::default();
        let body = DocumentBlockGeometry::for_kind(&RichBlockKind::Paragraph, document);
        let wide = DocumentBlockGeometry::for_width_class(BlockWidthClass::Wide, document);
        let full = DocumentBlockGeometry::for_kind(&RichBlockKind::Table, document);

        assert_eq!(DOCUMENT_ROOT_CONTENT_SURFACE_LEFT_PX, 48.0);
        assert_eq!(body.track_width_px, BODY_BLOCK_CONTENT_WIDTH_PX as f32);
        assert_eq!(wide.track_width_px, 960.0);
        assert_eq!(full.track_width_px, 1200.0);
        assert_eq!(body.track_left_px + body.track_width_px / 2.0, 648.0);
        assert_eq!(wide.track_left_px + wide.track_width_px / 2.0, 648.0);
        assert_eq!(full.track_left_px + full.track_width_px / 2.0, 648.0);
        assert_eq!(full.shell_left_px, 0.0);
        assert_eq!(full.track_right_px(), document.page_width_px - 48.0);
    }

    #[test]
    fn narrow_documents_shrink_every_track_without_losing_safe_insets() {
        let document = DocumentLayoutMetrics::for_viewport(700.0);
        for kind in [
            RichBlockKind::Paragraph,
            RichBlockKind::Whiteboard,
            RichBlockKind::Mermaid,
            RichBlockKind::Table,
        ] {
            let geometry = DocumentBlockGeometry::for_kind(&kind, document);
            assert_eq!(geometry.track_left_px, 48.0);
            assert_eq!(geometry.track_width_px, 604.0);
            assert_eq!(geometry.track_right_px(), 652.0);
        }
    }

    #[test]
    fn table_track_starts_at_its_projected_width_and_grows_to_the_full_capability() {
        let table_payload = cditor_core::rich_text::TablePayload {
            rows: (0..3)
                .map(|_| cditor_core::rich_text::TableRowPayload {
                    cells: vec![cditor_core::rich_text::TableCellPayload::default(); 3],
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let runtime = DocumentRuntime::from_payloads(
            1,
            vec![cditor_core::rich_text::BlockPayloadRecord {
                block_id: 1,
                content_version: 1,
                kind: RichBlockKind::Table,
                payload: cditor_core::rich_text::BlockPayload::Table(table_payload),
            }],
            BODY_BLOCK_CONTENT_WIDTH_PX,
        );
        let mut table = runtime.projection_for_window().blocks[0].clone();
        let document = DocumentLayoutMetrics::default();

        let body_sized = DocumentBlockGeometry::for_block(&table, document);
        assert_eq!(body_sized.width_class, BlockWidthClass::Full);
        assert_eq!(
            body_sized.track_width_px,
            BODY_BLOCK_CONTENT_WIDTH_PX as f32
        );
        assert_eq!(body_sized.track_left_px, 248.0);
        assert_eq!(body_sized.shell_left_px, 200.0);

        table.table_view.as_mut().unwrap().width_px = 920.0;
        let expanded = DocumentBlockGeometry::for_block(&table, document);
        assert_eq!(expanded.track_width_px, 920.0);
        assert_eq!(expanded.track_left_px, 188.0);

        table.table_view.as_mut().unwrap().width_px = 1_400.0;
        let full = DocumentBlockGeometry::for_block(&table, document);
        assert_eq!(full.track_width_px, 1200.0);
        assert_eq!(full.track_left_px, 48.0);
        assert_eq!(full.shell_left_px, 0.0);
    }

    #[test]
    fn text_viewport_maps_global_virtual_scroll_to_block_local_coordinates() {
        let viewport = DocumentTextViewport::for_block(400.0, 8.0, 10_000.0, 10_600.0, 800.0, 32.0);
        assert_eq!(viewport.local_top_px, 160.0);
        assert_eq!(viewport.height_px, 800.0);

        let below_viewport =
            DocumentTextViewport::for_block(900.0, 8.0, 10_000.0, 10_600.0, 800.0, 32.0);
        assert_eq!(below_viewport.local_top_px, 0.0);
    }
}
