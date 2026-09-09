use super::*;

#[test]
fn existing_constructors_default_to_sqlite_mode() {
    let runtime = DocumentRuntime::empty_composer();
    assert_eq!(runtime.mode(), DocumentRuntimeMode::Sqlite);
    assert!(runtime.markdown_source().is_none());
}

#[test]
fn markdown_constructor_retains_source_without_changing_projection() {
    let mut document = cditor_core::rich_text::RichTextDocument::empty(7);
    document.push_root_block(cditor_core::rich_text::RichBlockRecord::rich_text(1, cditor_core::rich_text::RichBlockKind::Heading { level: 1 }, "Hello"));
    let runtime = DocumentRuntime::markdown("# Hello\n", document, 720.0).expect("markdown runtime");

    assert_eq!(runtime.mode(), DocumentRuntimeMode::Markdown);
    assert_eq!(runtime.markdown_source().unwrap().text(), "# Hello\n");
    assert_eq!(runtime.document_id(), 7);
}

#[test]
fn markdown_source_transactions_are_explicit_and_versioned() {
    let mut document = cditor_core::rich_text::RichTextDocument::empty(7);
    document.push_root_block(cditor_core::rich_text::RichBlockRecord::paragraph(1, "hello"));
    let mut runtime = DocumentRuntime::markdown("hello", document, 720.0).expect("markdown runtime");
    let source = runtime.markdown_source().unwrap();
    let transaction = cditor_core::markdown::SourceTransaction {
        revision: source.revision(),
        edits: vec![cditor_core::markdown::SourceEdit {
            range: cditor_core::markdown::ByteOffset(5)..cditor_core::markdown::ByteOffset(5),
            replacement: " world".to_owned(),
        }],
        after_selection: cditor_core::markdown::SourceSelection {
            anchor: cditor_core::markdown::ByteOffset(11),
            focus: cditor_core::markdown::ByteOffset(11),
        },
    };

    runtime
        .apply_markdown_source_transaction(transaction)
        .expect("source transaction");
    assert_eq!(runtime.markdown_source().unwrap().text(), "hello world");
    assert_eq!(runtime.markdown_source().unwrap().revision(), 1);
    assert!(runtime.undo_markdown_source().expect("source undo"));
    assert_eq!(runtime.markdown_source().unwrap().text(), "hello");
    assert!(runtime.redo_markdown_source().expect("source redo"));
    assert_eq!(runtime.markdown_source().unwrap().text(), "hello world");
}

#[test]
fn sqlite_constructor_forces_rich_text_mode() {
    let document = cditor_core::rich_text::RichTextDocument::empty(9);
    let runtime = DocumentRuntime::sqlite(document, 720.0);
    assert_eq!(runtime.mode(), DocumentRuntimeMode::Sqlite);
    assert!(runtime.markdown_source().is_none());
}
