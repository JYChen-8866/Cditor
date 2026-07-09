use gpui::{App, Bounds, ElementInputHandler, Entity, FocusHandle, Pixels, Window};

use crate::gui::app::{CditorV2View, GuiPlatformInputTarget};

pub(crate) fn handle_registered_platform_input(
    view: &Entity<CditorV2View>,
    focus: &FocusHandle,
    target: GuiPlatformInputTarget,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let registered = view.update(cx, |view, _cx| view.register_platform_input_target(target));
    if registered {
        window.handle_input(focus, ElementInputHandler::new(bounds, view.clone()), cx);
    }
    registered
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::app::cditor_v2_view::platform_input_registration_allows;
    use cditor_core::rich_text::{
        BlockPayload, BlockPayloadRecord, RichBlockKind, TableCellPayload, TablePayload,
        TableRowPayload,
    };
    use cditor_runtime::DocumentRuntime;

    #[test]
    fn adapter_targets_match_runtime_block_and_table_sessions() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![
                BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "body"),
                BlockPayloadRecord {
                    block_id: 2,
                    content_version: 1,
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
            ],
            720.0,
        );

        runtime.focus_block_at_offset(1, 1).unwrap();
        assert!(platform_input_registration_allows(
            None,
            GuiPlatformInputTarget::BlockText { block_id: 1 },
            &runtime,
        ));
        assert!(!platform_input_registration_allows(
            None,
            GuiPlatformInputTarget::TableCell {
                block_id: 2,
                row: 0,
                col: 0,
            },
            &runtime,
        ));

        runtime.focus_table_cell_at_offset(2, 0, 0, 1).unwrap();
        assert!(platform_input_registration_allows(
            None,
            GuiPlatformInputTarget::TableCell {
                block_id: 2,
                row: 0,
                col: 0,
            },
            &runtime,
        ));
        assert!(!platform_input_registration_allows(
            None,
            GuiPlatformInputTarget::BlockText { block_id: 1 },
            &runtime,
        ));
    }
}
