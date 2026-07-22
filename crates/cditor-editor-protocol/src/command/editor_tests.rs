use super::*;

#[test]
fn editor_commands_share_stable_ids_and_typed_arguments() {
    let command = EditorCommand::SetBlockColor {
        block_id: 7,
        target: InlineColorTarget::Background,
        color: Some("#ffffff".to_owned()),
    };
    assert_eq!(command.stable_id(), builtin::BLOCK_SET_COLOR);
    assert!(matches!(
        command.arguments(),
        CommandArgs::BlockColor { block_id: 7, .. }
    ));
}

#[test]
fn editor_command_invocation_keeps_source_and_catalog_contract() {
    let command = EditorCommand::MoveCaret {
        direction: CaretDirection::NextWord,
        extend_selection: true,
    };
    let invocation = command.invocation(CommandSource::Keyboard);
    assert_eq!(invocation.id.as_str(), builtin::TEXT_MOVE_CARET);
    assert_eq!(
        CommandCatalog::builtin().validate_invocation(&invocation),
        Ok(())
    );
}

#[test]
fn envelope_keeps_optimistic_revision_and_request_identity() {
    let envelope = CommandEnvelope::new(EditorCommand::Undo, CommandSource::Sdk)
        .expecting_revision(9)
        .with_request_id(12);
    assert_eq!(envelope.expected_revision, Some(9));
    assert_eq!(envelope.invocation().request_id, Some(12));
}

#[test]
fn drag_commit_commands_keep_typed_catalog_arguments() {
    let commands = [
        EditorCommand::MoveBlockBefore {
            block_id: 2,
            before_block_id: Some(4),
        },
        EditorCommand::MoveBlockToParent {
            block_id: 3,
            parent_id: Some(2),
            sibling_index: 0,
        },
        EditorCommand::SetMediaWidthRatio {
            block_id: 7,
            ratio_milli: 750,
        },
        EditorCommand::TableResizeAxis {
            block_id: 8,
            axis: TableAxis::Column,
            index: 2,
            size_px: 180,
        },
        EditorCommand::TableMoveAxis {
            block_id: 8,
            axis: TableAxis::Row,
            from_index: 1,
            to_index: 3,
        },
    ];
    let catalog = CommandCatalog::builtin();
    for command in commands {
        assert_eq!(
            catalog.validate_invocation(&command.invocation(CommandSource::Toolbar)),
            Ok(())
        );
    }
}

#[test]
fn down_placer_command_has_a_stable_catalog_contract() {
    let invocation = EditorCommand::EnsureTrailingParagraph.invocation(CommandSource::Toolbar);
    assert_eq!(
        invocation.id.as_str(),
        builtin::BLOCK_ENSURE_TRAILING_PARAGRAPH
    );
    assert_eq!(invocation.args, CommandArgs::None);
}

#[test]
fn document_selection_command_preserves_direction_and_affinity() {
    let selection = cditor_core::edit::DocumentSelection {
        anchor: cditor_core::edit::TextPosition {
            block_id: 2,
            offset: 4,
            affinity: cditor_core::edit::TextAffinity::Upstream,
        },
        focus: cditor_core::edit::TextPosition::downstream(1, 1),
    };
    let invocation =
        EditorCommand::SetDocumentSelection { selection }.invocation(CommandSource::Sdk);
    assert_eq!(invocation.id.as_str(), builtin::SELECTION_SET_DOCUMENT);
    assert_eq!(invocation.args, CommandArgs::DocumentSelection(selection));
}

#[test]
fn block_focus_command_uses_a_read_only_typed_target() {
    let invocation = EditorCommand::FocusBlock { block_id: 9 }.invocation(CommandSource::Toolbar);
    assert_eq!(invocation.args, CommandArgs::BlockTarget { block_id: 9 });
    assert_eq!(
        CommandCatalog::builtin()
            .definition(&invocation.id)
            .unwrap()
            .mutability,
        CommandMutability::ReadOnly
    );
}

#[test]
fn table_cell_focus_command_preserves_geometry_adapter_output() {
    let invocation = EditorCommand::FocusTableCell {
        block_id: 8,
        row: 2,
        col: 3,
        offset: Some(5),
        affinity: cditor_core::edit::TextAffinity::Upstream,
    }
    .invocation(CommandSource::Toolbar);
    assert_eq!(invocation.id.as_str(), builtin::SELECTION_FOCUS_TABLE_CELL);
    assert!(matches!(
        invocation.args,
        CommandArgs::TableCellFocus {
            offset: Some(5),
            affinity: cditor_core::edit::TextAffinity::Upstream,
            ..
        }
    ));
}

#[test]
fn auxiliary_surface_selection_keeps_surface_and_affinity() {
    let surface_id = cditor_core::ids::SurfaceId::ImageCaption { block_id: 10 };
    let invocation = EditorCommand::SetTextSurfaceSelection {
        surface_id,
        anchor_offset: 2,
        focus_offset: 6,
        focus_affinity: cditor_core::edit::TextAffinity::Upstream,
    }
    .invocation(CommandSource::Toolbar);

    assert_eq!(invocation.id.as_str(), builtin::SELECTION_SET_TEXT_SURFACE);
    assert_eq!(
        invocation.args,
        CommandArgs::TextSurfaceSelection {
            surface_id,
            anchor_offset: 2,
            focus_offset: 6,
            focus_affinity: cditor_core::edit::TextAffinity::Upstream,
        }
    );
    assert_eq!(
        CommandCatalog::builtin().validate_invocation(&invocation),
        Ok(())
    );
}

#[test]
fn table_cell_selection_keeps_direction_and_geometry_affinity() {
    let invocation = EditorCommand::SetTableCellSelection {
        block_id: 10,
        row: 1,
        col: 2,
        anchor_offset: 7,
        focus_offset: 3,
        focus_affinity: cditor_core::edit::TextAffinity::Upstream,
    }
    .invocation(CommandSource::Keyboard);

    assert_eq!(invocation.id.as_str(), builtin::SELECTION_SET_TABLE_CELL);
    assert!(matches!(
        invocation.args,
        CommandArgs::TableCellSelection {
            anchor_offset: 7,
            focus_offset: 3,
            focus_affinity: cditor_core::edit::TextAffinity::Upstream,
            ..
        }
    ));
    assert_eq!(
        CommandCatalog::builtin().validate_invocation(&invocation),
        Ok(())
    );
}
