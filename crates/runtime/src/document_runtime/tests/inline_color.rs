use super::*;

fn spans(runtime: &DocumentRuntime) -> Vec<InlineSpan> {
    match runtime.block_payload_record(1).unwrap().payload {
        BlockPayload::RichText { spans } => spans,
        payload => panic!("expected rich text payload, got {payload:?}"),
    }
}

#[test]
fn selection_color_preserves_the_selection_and_is_one_undo_step() {
    let mut runtime = runtime_with_rich_spans(vec![InlineSpan::plain("abcdef")]);
    runtime.set_document_text_selection(1, 5, 1, 1).unwrap();

    assert!(
        runtime
            .set_inline_color_on_selection(InlineColorTarget::Text, Some("#337ea9"))
            .unwrap()
    );
    assert_eq!(runtime.input_session_selected_range(), Some(1..5));
    assert!(runtime.input_session_selection_reversed());
    assert_eq!(
        spans(&runtime),
        vec![
            InlineSpan::plain("a"),
            InlineSpan {
                text: "bcde".to_owned(),
                marks: vec![InlineMark::Color("#337ea9".to_owned())],
            },
            InlineSpan::plain("f"),
        ]
    );

    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(spans(&runtime), vec![InlineSpan::plain("abcdef")]);
}

#[test]
fn range_color_for_gutter_does_not_replace_the_current_text_selection() {
    let mut runtime = runtime_with_rich_spans(vec![InlineSpan::plain("abcdef")]);
    runtime.set_document_text_selection(1, 4, 1, 2).unwrap();

    assert!(
        runtime
            .set_inline_color_for_range(1, 0..6, InlineColorTarget::Background, Some("#fbf3db"),)
            .unwrap()
    );
    assert_eq!(runtime.input_session_selected_range(), Some(2..4));
    assert!(runtime.input_session_selection_reversed());
    assert_eq!(
        spans(&runtime),
        vec![InlineSpan {
            text: "abcdef".to_owned(),
            marks: vec![InlineMark::Background("#fbf3db".to_owned())],
        }]
    );
}

#[test]
fn applying_the_same_color_is_a_noop_and_default_clears_only_that_family() {
    let mut runtime = runtime_with_rich_spans(vec![InlineSpan {
        text: "abcdef".to_owned(),
        marks: vec![
            InlineMark::Bold,
            InlineMark::Color("#337ea9".to_owned()),
            InlineMark::Background("#fbf3db".to_owned()),
        ],
    }]);

    assert!(
        !runtime
            .set_inline_color_for_range(1, 0..6, InlineColorTarget::Text, Some("#337ea9"))
            .unwrap()
    );
    assert!(
        runtime
            .set_inline_color_for_range(1, 0..6, InlineColorTarget::Text, None)
            .unwrap()
    );
    assert_eq!(
        spans(&runtime),
        vec![InlineSpan {
            text: "abcdef".to_owned(),
            marks: vec![
                InlineMark::Bold,
                InlineMark::Background("#fbf3db".to_owned()),
            ],
        }]
    );
}
