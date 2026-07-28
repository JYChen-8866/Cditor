use cditor_core::document::BlockIndexRecord;
use cditor_core::rich_text::{
    BlockPayload, BlockPayloadRecord, RichBlockKind, TextAlign, kind_tag_for_rich_block_kind,
};
use cditor_editor_protocol::command::{CommandSource, EditorCommand};
use cditor_runtime::document_runtime::{DocumentRuntimeColdStartData, DocumentRuntimeIndexSource};
use cditor_runtime::{InputTarget, TableCellPosition};
use cditor_runtime::{RealtimeInput, RealtimeInputRequest};
use gpui::{AppContext, Bounds, Size, TestAppContext, point, px};

use super::*;
use crate::interaction::geometry::ProjectedBlockRect;

#[test]
fn projected_hit_uses_current_placement_instead_of_stale_paint_bounds() {
    let cache = crate::text::test_platform_layout(
        1,
        1,
        "ab\u{4e2d}cd",
        Bounds::new(
            point(px(20_000_000.0), px(20_000_000.0)),
            gpui::size(px(300.0), px(120.0)),
        ),
        None,
    );
    let caret = cache
        .snapshot
        .caret_rect(TextLayoutPosition::downstream(5), 1.0);
    let placement = ProjectedTextPlacement {
        window_origin_x_px: 132.0,
        window_origin_y_px: 84.0,
        wrap_width_px: 300.0,
        text_align: cditor_core::rich_text::TextAlign::Start,
    };
    let point = point(
        px((placement.window_origin_x_px + f64::from(caret.x)) as f32),
        px((placement.window_origin_y_px + f64::from(caret.y + caret.height / 2.0)) as f32),
    );
    let hit =
        platform_text_position_for_local_point(&cache, projected_text_hit_point(placement, point));

    assert_eq!(hit.offset, 5);
}

#[gpui::test]
fn block_hit_entrypoints_ignore_stale_paint_bounds_after_a_large_window_jump(
    cx: &mut TestAppContext,
) {
    let runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "ab\u{4e2d}cd",
        )],
        720.0,
    );
    let view = cx.new(|cx| CditorV2View::from_runtime(runtime, false, cx));

    view.update(cx, |view, _cx| {
        view.interaction.document_viewport_origin =
            Some(DocumentViewportOrigin { x: 100.0, y: 40.0 });
        view.interaction.presented_scroll_top = 20_000_000.25;
        view.interaction.projected_block_rects = vec![ProjectedBlockRect {
            block_id: 1,
            document_top: 20_000_128.25,
            document_bottom: 20_000_248.25,
            text_origin_x_in_block_px: 32.0,
            text_origin_y_in_block_px: 12.0,
            text_width_px: 300.0,
            text_align: Some(TextAlign::Start),
            ..ProjectedBlockRect::default()
        }];
        let current = view
            .ready_session()
            .unwrap()
            .surface_version(SurfaceId::Block(1))
            .unwrap()
            .unwrap();
        let mut cache = crate::text::test_platform_layout(
            1,
            current.content_version,
            "ab\u{4e2d}cd",
            Bounds::new(
                point(px(20_000_000.0), px(20_000_000.0)),
                gpui::size(px(300.0), px(120.0)),
            ),
            None,
        );
        cache.layout_version = current.layout_version;
        let caret = cache
            .snapshot
            .caret_rect(TextLayoutPosition::downstream(5), 1.0);
        view.cache.text_layouts.insert(1, cache, None);
        let placement = view.projected_text_placement_for_block(1).unwrap();
        let click = point(
            px((placement.window_origin_x_px + f64::from(caret.x)) as f32),
            px((placement.window_origin_y_px + f64::from(caret.y + caret.height / 2.0)) as f32),
        );

        assert_eq!(
            view.text_position_for_block_at_position(1, click)
                .unwrap()
                .offset,
            5,
        );
        assert_eq!(
            view.text_position_for_surface_at_position(SurfaceId::Block(1), click)
                .unwrap()
                .offset,
            5,
        );
    });
}

#[gpui::test]
fn synchronous_block_bounds_layout_the_current_composition_preview(cx: &mut TestAppContext) {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "",
        )],
        720.0,
    );
    crate::test_support::focus_block_at_offset(&mut runtime, 1, 0);
    let expected = runtime.input_session_identity().unwrap();
    runtime
        .apply_realtime_input(RealtimeInputRequest {
            expected,
            input: RealtimeInput::UpdateComposition {
                range: 0..0,
                text: "ni",
                selected_range: Some(2..2),
            },
        })
        .unwrap();
    let view = cx.new(|cx| CditorV2View::from_runtime(runtime, false, cx));

    view.update(cx, |view, _cx| {
        view.interaction.document_viewport_origin =
            Some(DocumentViewportOrigin { x: 100.0, y: 40.0 });
        view.interaction.projected_block_rects = vec![ProjectedBlockRect {
            block_id: 1,
            document_top: 120.0,
            document_bottom: 180.0,
            text_origin_x_in_block_px: 32.0,
            text_origin_y_in_block_px: 12.0,
            text_width_px: 300.0,
            text_align: Some(TextAlign::Start),
            ..ProjectedBlockRect::default()
        }];
        let session = view.ready_session().unwrap();
        let current = session
            .surface_version(SurfaceId::Block(1))
            .unwrap()
            .unwrap();
        let first = view
            .synchronous_text_range_bounds_for_block(session, 1, current, 1..1)
            .unwrap();
        let second = view
            .synchronous_text_range_bounds_for_block(session, 1, current, 2..2)
            .unwrap();
        let placement = view.projected_text_placement_for_block(1).unwrap();

        assert!(f32::from(second.left()) > f32::from(first.left()));
        assert!(f32::from(second.left()) > placement.window_origin_x_px as f32);
        assert!(f32::from(second.size.height) > 0.0);
        assert_ne!(second, Bounds::default());
    });
}

#[gpui::test]
fn cold_resident_block_first_body_selection_hits_and_activates_without_gutter(
    cx: &mut TestAppContext,
) {
    let text = "zero target end";
    let target_offset = text.find("target").unwrap() + 2;
    let records = vec![
        BlockIndexRecord::new(
            1,
            None,
            0,
            kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
            0,
        )
        .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(1, 32.0)),
    ];
    let (mut runtime, _) = DocumentRuntime::from_cold_start_data(
        DocumentRuntimeColdStartData {
            document_id: 1,
            document_title: "Cold click".to_owned(),
            structure_version: 1,
            records,
            block_attrs: Vec::new(),
            initial_payloads: Vec::new(),
            initial_payload_window_end: 0,
            index_source: DocumentRuntimeIndexSource::Blocks,
            layout_cache_hits: 0,
        },
        720.0,
    )
    .unwrap();
    let request = runtime
        .plan_payload_window_load_if_needed(0..1)
        .expect("the visible cold payload must need loading");
    assert_eq!(
        runtime.apply_payload_window_result(
            cditor_runtime::content::payload_window::PayloadWindowLoadResult::prepare(
                request,
                vec![BlockPayloadRecord::rich_text(
                    1,
                    RichBlockKind::Paragraph,
                    text,
                )],
                Vec::new(),
            ),
        ),
        cditor_runtime::content::payload_window::PayloadWindowApplyDecision::Applied,
    );
    let view = cx.new(|cx| CditorV2View::from_runtime(runtime, false, cx));

    view.update(cx, |view, cx| {
        view.interaction.document_viewport_origin =
            Some(DocumentViewportOrigin { x: 100.0, y: 40.0 });
        view.interaction.presented_scroll_top = 20_000_000.25;
        view.interaction.projected_block_rects = vec![ProjectedBlockRect {
            block_id: 1,
            document_top: 20_000_128.25,
            document_bottom: 20_000_188.25,
            text_origin_x_in_block_px: 32.0,
            text_origin_y_in_block_px: 12.0,
            text_width_px: 300.0,
            text_align: Some(TextAlign::Start),
            ..ProjectedBlockRect::default()
        }];
        let placement = view.projected_text_placement_for_block(1).unwrap();
        let target = {
            let session = view.ready_session().unwrap();
            assert_eq!(session.document_snapshot().unwrap().focused_block_id, None);
            let current = session
                .surface_version(SurfaceId::Block(1))
                .unwrap()
                .unwrap();
            let element = cold_text_element_for_block(session, 1, current, placement).unwrap();
            let caret = element.local_caret_rect_for_offset(target_offset);
            let click = point(
                px((placement.window_origin_x_px + f64::from(caret.x)) as f32),
                px((placement.window_origin_y_px + f64::from(caret.y + caret.height / 2.0)) as f32),
            );
            view.text_position_for_block_at_position(1, click)
                .expect("Parley cold hit must resolve the body click")
        };
        assert_eq!(target.offset, target_offset);

        view.dispatch_command(
            EditorCommand::SetDocumentSelection {
                selection: cditor_core::edit::DocumentSelection::caret(
                    cditor_core::edit::TextPosition {
                        block_id: 1,
                        offset: target.offset,
                        affinity: target.affinity,
                    },
                ),
            },
            CommandSource::Toolbar,
            cx,
        )
        .expect("the first body selection must activate the resident payload");

        let session = view.ready_session().unwrap();
        assert_eq!(
            session.document_snapshot().unwrap().focused_block_id,
            Some(1)
        );
        assert_eq!(
            session.input_context().unwrap().target,
            Some(InputTarget::BlockText { block_id: 1 }),
        );
        assert_eq!(
            session.text_block_context(1).unwrap().unwrap().caret,
            Some(target_offset),
        );
    });
}

#[gpui::test]
fn cold_block_hit_and_multiclick_respect_center_and_end_alignment(cx: &mut TestAppContext) {
    for alignment in [TextAlign::Center, TextAlign::End] {
        let runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "short words",
            )],
            720.0,
        );
        let view = cx.new(|cx| CditorV2View::from_runtime(runtime, false, cx));

        view.update(cx, |view, _cx| {
            view.interaction.document_viewport_origin =
                Some(DocumentViewportOrigin { x: 100.0, y: 40.0 });
            view.interaction.projected_block_rects = vec![ProjectedBlockRect {
                block_id: 1,
                document_top: 120.0,
                document_bottom: 180.0,
                text_origin_x_in_block_px: 32.0,
                text_origin_y_in_block_px: 12.0,
                text_width_px: 300.0,
                text_align: Some(alignment),
                ..ProjectedBlockRect::default()
            }];
            let session = view.ready_session().unwrap();
            let current = session
                .surface_version(SurfaceId::Block(1))
                .unwrap()
                .unwrap();
            let placement = view.projected_text_placement_for_block(1).unwrap();
            let element = cold_text_element_for_block(session, 1, current, placement).unwrap();
            let target = TextLayoutPosition::downstream(2);
            let caret = element.local_caret_rect_for_offset(target.offset);
            assert!(
                caret.x > 0.0,
                "{alignment:?} alignment must shift the local caret"
            );
            let click = point(
                px((placement.window_origin_x_px + f64::from(caret.x)) as f32),
                px((placement.window_origin_y_px + f64::from(caret.y + caret.height / 2.0)) as f32),
            );
            let local_point = projected_text_hit_point(placement, click);

            assert_eq!(
                view.text_position_for_block_at_position(1, click)
                    .unwrap()
                    .offset,
                target.offset,
            );
            assert_eq!(
                view.text_selection_for_block_at_position(1, click, TextLayoutSelectionKind::Word,),
                Some(element.selection_at_point(local_point, TextLayoutSelectionKind::Word)),
            );
            assert_eq!(
                view.text_selection_for_block_at_position(1, click, TextLayoutSelectionKind::Line,),
                Some(element.selection_at_point(local_point, TextLayoutSelectionKind::Line)),
            );
        });
    }
}

#[gpui::test]
fn resize_rejects_old_wrap_width_before_block_hit(cx: &mut TestAppContext) {
    let text = "one two three four five six seven";
    let runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            text,
        )],
        720.0,
    );
    let view = cx.new(|cx| CditorV2View::from_runtime(runtime, false, cx));

    view.update(cx, |view, _cx| {
        view.interaction.document_viewport_origin =
            Some(DocumentViewportOrigin { x: 100.0, y: 40.0 });
        view.interaction.projected_block_rects = vec![ProjectedBlockRect {
            block_id: 1,
            document_top: 120.0,
            document_bottom: 320.0,
            text_origin_x_in_block_px: 24.0,
            text_origin_y_in_block_px: 8.0,
            text_width_px: 96.0,
            text_align: Some(TextAlign::Start),
            ..ProjectedBlockRect::default()
        }];
        let current = view
            .ready_session()
            .unwrap()
            .surface_version(SurfaceId::Block(1))
            .unwrap()
            .unwrap();
        let placement = view.projected_text_placement_for_block(1).unwrap();
        let target_offset = text.find("five").unwrap();
        let click = {
            let session = view.ready_session().unwrap();
            let element = cold_text_element_for_block(session, 1, current, placement).unwrap();
            let caret = element.local_caret_rect_for_offset(target_offset);
            point(
                px((placement.window_origin_x_px + f64::from(caret.x)) as f32),
                px((placement.window_origin_y_px + f64::from(caret.y + caret.height / 2.0)) as f32),
            )
        };
        let mut stale = crate::text::test_platform_layout(
            1,
            current.content_version,
            text,
            Bounds::new(point(px(4.0), px(4.0)), gpui::size(px(360.0), px(32.0))),
            None,
        );
        stale.layout_version = current.layout_version;
        view.cache.text_layouts.insert(1, stale, None);
        assert!(view.current_text_layout_cache(current, 1).is_none());

        assert_eq!(
            view.text_position_for_block_at_position(1, click)
                .unwrap()
                .offset,
            target_offset,
        );
    });
}

#[gpui::test]
fn code_block_hit_applies_negative_fractional_internal_scroll(cx: &mut TestAppContext) {
    let text = (0..40)
        .map(|line| format!("line-{line:02}"))
        .collect::<Vec<_>>()
        .join("\n");
    let target_offset = text.find("line-30").unwrap();
    let runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Code {
                language: Some("rust".to_owned()),
            },
            payload: BlockPayload::Code {
                language: Some("rust".to_owned()),
                text: text.clone(),
            },
        }],
        720.0,
    );
    let view = cx.new(|cx| CditorV2View::from_runtime(runtime, false, cx));

    view.update(cx, |view, _cx| {
        view.interaction.document_viewport_origin =
            Some(DocumentViewportOrigin { x: 100.0, y: 40.0 });
        view.interaction.projected_block_rects = vec![ProjectedBlockRect {
            block_id: 1,
            document_top: 120.0,
            document_bottom: 1_200.0,
            text_origin_x_in_block_px: 24.0,
            text_origin_y_in_block_px: 8.0,
            text_width_px: 480.0,
            text_align: Some(TextAlign::Start),
            has_internal_text_scroll: true,
            ..ProjectedBlockRect::default()
        }];
        let handle = gpui::ScrollHandle::default();
        handle.set_offset(point(px(0.0), px(-480.5)));
        view.interaction.code_scroll_handles.insert(1, handle);
        let session = view.ready_session().unwrap();
        let current = session
            .surface_version(SurfaceId::Block(1))
            .unwrap()
            .unwrap();
        let placement = view.projected_text_placement_for_block(1).unwrap();
        let element = cold_text_element_for_block(session, 1, current, placement).unwrap();
        let caret = element.local_caret_rect_for_offset(target_offset);
        let click = point(
            px((placement.window_origin_x_px + f64::from(caret.x)) as f32),
            px((placement.window_origin_y_px + f64::from(caret.y + caret.height / 2.0)) as f32),
        );

        assert_eq!(
            view.text_position_for_block_at_position(1, click)
                .unwrap()
                .offset,
            target_offset,
        );
    });
}

#[test]
fn layout_cache_rejects_stale_surface_content_and_layout_identity() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(cditor_core::rich_text::TablePayload {
                rows: vec![cditor_core::rich_text::TableRowPayload {
                    cells: vec![cditor_core::rich_text::TableCellPayload::plain("cell")],
                    height: Default::default(),
                }],
                columns: Vec::new(),
                header_rows: 0,
                header_cols: 0,
                header_style: Default::default(),
            }),
        }],
        720.0,
    );
    crate::test_support::focus_table_cell_at_offset(&mut runtime, 1, 0, 0, 4);
    crate::test_support::replace_realtime_text(&mut runtime, None, "\nmore");
    let current_version = runtime.block_content_version(1).unwrap();
    let stale_cache = crate::text::test_platform_layout(
        1,
        current_version.saturating_sub(1),
        "cell",
        Bounds {
            origin: point(px(10.0), px(20.0)),
            size: Size {
                width: px(120.0),
                height: px(36.0),
            },
        },
        Some(TableCellPosition { row: 0, col: 0 }),
    );
    let surface_id = SurfaceId::TableCell {
        block_id: 1,
        row: 0,
        column: 0,
    };
    let current = cditor_session::project_surface_version(&runtime, surface_id).unwrap();
    assert!(!layout_cache_is_current(&stale_cache, current, None, None));
    let mut current_cache = crate::text::test_platform_layout(
        1,
        current_version,
        "cell\nmore",
        Bounds {
            origin: point(px(10.0), px(20.0)),
            size: Size {
                width: px(120.0),
                height: px(88.0),
            },
        },
        Some(TableCellPosition { row: 0, col: 0 }),
    );
    current_cache.layout_version = current.layout_version;

    assert!(layout_cache_is_current(&current_cache, current, None, None));
    assert!(!layout_cache_is_current(
        &current_cache,
        SurfaceVersionSnapshot {
            layout_version: current.layout_version.saturating_add(1),
            ..current
        },
        None,
        None,
    ));
    assert!(!layout_cache_is_current(
        &current_cache,
        SurfaceVersionSnapshot {
            surface_id: SurfaceId::Block(1),
            ..current
        },
        None,
        None,
    ));
}

#[test]
fn click_count_maps_single_to_caret_double_to_word_and_triple_to_line() {
    assert_eq!(selection_kind_for_click_count(1), None);
    assert_eq!(
        selection_kind_for_click_count(2),
        Some(TextLayoutSelectionKind::Word)
    );
    assert_eq!(
        selection_kind_for_click_count(3),
        Some(TextLayoutSelectionKind::Line)
    );
    assert_eq!(
        selection_kind_for_click_count(5),
        Some(TextLayoutSelectionKind::Line)
    );
}

#[gpui::test]
fn render_state_projects_caption_snapshot_and_focus_session(cx: &mut TestAppContext) {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 10,
            content_version: 3,
            kind: RichBlockKind::Image,
            payload: BlockPayload::Image(cditor_core::rich_text::ImagePayload {
                caption: "caption".into(),
                ..Default::default()
            }),
        }],
        720.0,
    );
    let surface_id = super::super::caption::surface_id(10);
    crate::test_support::focus_text_surface_at_offset(&mut runtime, surface_id, 2);
    let view = cx.new(|cx| CditorV2View::from_runtime(runtime, false, cx));

    view.update(cx, |view, _cx| {
        let state = view.text_surface_render_state(surface_id).unwrap();
        assert!(state.focused);
        assert_eq!(state.caret_offset, Some(2));
        assert_eq!(state.snapshot.plain_text(), "caption");
        assert_eq!(state.snapshot.identity.content_version, 3);
    });
}

#[gpui::test]
fn auxiliary_caption_hit_uses_explicit_live_placement_for_cold_and_warm_layouts(
    cx: &mut TestAppContext,
) {
    let text = "zero target end";
    let word_start = text.find("target").unwrap();
    let target_offset = word_start + 2;
    let runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 10,
            content_version: 3,
            kind: RichBlockKind::Image,
            payload: BlockPayload::Image(cditor_core::rich_text::ImagePayload {
                caption: text.into(),
                ..Default::default()
            }),
        }],
        720.0,
    );
    let surface_id = super::super::caption::surface_id(10);
    let view = cx.new(|cx| CditorV2View::from_runtime(runtime, false, cx));

    view.update(cx, |view, _cx| {
        let geometry = TextSurfaceInteractionGeometry::from_bounds(
            Bounds::new(point(px(420.0), px(260.0)), gpui::size(px(220.0), px(30.0))),
            220.0,
            TextAlign::Center,
            RichTextTypography {
                font_size_px: Some(14.0),
                line_height_px: Some(20.0),
                font_weight: None,
            },
        );
        let session = view.ready_session().unwrap();
        let current = session.surface_version(surface_id).unwrap().unwrap();
        let cold = cold_text_element_for_auxiliary_surface(session, surface_id, current, geometry)
            .unwrap();
        let cold_caret = cold.local_caret_rect_for_offset(target_offset);
        let cold_click = point(
            px((geometry.placement.window_origin_x_px + f64::from(cold_caret.x)) as f32),
            px((geometry.placement.window_origin_y_px
                + f64::from(cold_caret.y + cold_caret.height / 2.0)) as f32),
        );

        assert_eq!(
            view.text_position_for_auxiliary_surface_at_position(surface_id, cold_click, geometry,)
                .unwrap()
                .offset,
            target_offset,
        );
        let selection = view
            .text_selection_for_auxiliary_surface_at_position(
                surface_id,
                cold_click,
                TextLayoutSelectionKind::Word,
                geometry,
            )
            .unwrap();
        let selected = selection.anchor.offset.min(selection.focus.offset)
            ..selection.anchor.offset.max(selection.focus.offset);
        assert_eq!(selected, word_start..word_start + "target".len());

        let mut stale = crate::text::test_platform_layout(
            10,
            current.content_version,
            text,
            Bounds::new(
                point(px(20_000_000.0), px(20_000_000.0)),
                gpui::size(px(220.0), px(30.0)),
            ),
            None,
        );
        stale.surface_id = surface_id;
        stale.layout_version = current.layout_version;
        stale.text_align = TextAlign::Center;
        let warm_caret = stale
            .snapshot
            .caret_rect(TextLayoutPosition::downstream(target_offset), 1.0);
        let warm_click = point(
            px((geometry.placement.window_origin_x_px + f64::from(warm_caret.x)) as f32),
            px((geometry.placement.window_origin_y_px
                + f64::from(warm_caret.y + warm_caret.height / 2.0)) as f32),
        );
        view.cache
            .text_surface_layouts
            .insert(surface_id, stale, None);

        assert_eq!(
            view.text_position_for_auxiliary_surface_at_position(surface_id, warm_click, geometry,)
                .unwrap()
                .offset,
            target_offset,
        );
    });
}
