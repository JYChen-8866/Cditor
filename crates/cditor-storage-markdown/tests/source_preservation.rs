use cditor_core::layout::PAGE_POLICY_VERSION;
use cditor_core::rich_text::{BlockPayload, InlineMark};
use cditor_storage::{DocumentStorage, LoadDocumentRequest, LoadedDocument, StorageSaveBatch, DOCUMENT_INDEX_VISIBLE_VERSION};
use cditor_storage::layout_cache::LayoutCacheKey;
use cditor_storage_markdown::MarkdownStorage;

fn request() -> LoadDocumentRequest {
    LoadDocumentRequest {
        document_id: 1, initial_payload_window_blocks: 32, visible_index_version: DOCUMENT_INDEX_VISIBLE_VERSION,
        layout_key: LayoutCacheKey {
            width_bucket: 10, exact_width_px: 800, content_version: 1, attrs_version: 0,
            style_version: 0, font_version: 0, theme_version: 0, scale_factor_milli: 1000,
        }, page_policy_version: PAGE_POLICY_VERSION,
    }
}
fn batch(loaded: LoadedDocument) -> StorageSaveBatch {
    StorageSaveBatch {
        document_id: 1, icon_json: None, cover_json: None, layout_key: None,
        payloads: loaded.initial_payloads, index_records: loaded.records, structure_version: 1,
        transactions: vec![], block_attrs: vec![], page_layout_snapshot: None,
    }
}

#[tokio::test]
async fn unchanged_preserves_all_source_bytes() {
    for original in ["\r\n__bold *中文* bold__  \r\nnext\r\n\r\n[a][r]\r\n\r\n[r]: <https://example.com> 'title'\r\n", "", "\n\n", "raw <span>HTML</span>\n\n![alt](image.png)"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("1.md");
        std::fs::write(&path, original).unwrap();
        let storage = MarkdownStorage::new(dir.path().into());
        let loaded = storage.load_document(request()).await.unwrap();
        storage.commit(batch(loaded)).await.unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), original);
    }
}

#[tokio::test]
async fn partial_payload_save_preserves_nested_delimiters_and_other_blocks() {
    let original = "__bold *italic* bold__\r\n\r\nUntouched [ref][id]\r\n\r\n[id]: <https://example.com> 'Title'\r\n";
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("1.md");
    std::fs::write(&path, original).unwrap();
    let storage = MarkdownStorage::new(dir.path().into());
    let mut save = batch(storage.load_document(request()).await.unwrap());
    save.index_records.clear();
    save.payloads.truncate(1);
    save.payloads[0].content_version += 1;
    let BlockPayload::RichText { spans } = &mut save.payloads[0].payload else { panic!() };
    spans[1].text = "edited".into();
    storage.commit(save).await.unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original.replace("italic", "edited"));
    let reloaded = MarkdownStorage::new(dir.path().into()).load_document(request()).await.unwrap();
    assert_eq!(reloaded.initial_payloads.len(), 2);
}

#[tokio::test]
async fn format_change_only_rewrites_local_inline() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("1.md");
    std::fs::write(&path, "text\n\n__untouched__\n").unwrap();
    let storage = MarkdownStorage::new(dir.path().into());
    let mut save = batch(storage.load_document(request()).await.unwrap());
    let BlockPayload::RichText { spans } = &mut save.payloads[0].payload else { panic!() };
    spans[0].marks.push(InlineMark::Bold);
    storage.commit(save).await.unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "**text**\n\n__untouched__\n");
}

#[tokio::test]
async fn rejected_edits_and_external_changes_never_overwrite_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("1.md");
    let original = "**bold**\n";
    std::fs::write(&path, original).unwrap();
    let storage = MarkdownStorage::new(dir.path().into());
    let mut save = batch(storage.load_document(request()).await.unwrap());
    let valid = save.clone();
    let BlockPayload::RichText { spans } = &mut save.payloads[0].payload else { panic!() };
    spans[0].marks.push(InlineMark::Color("red".into()));
    assert!(storage.commit(save).await.is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    std::fs::write(&path, "external change").unwrap();
    assert!(storage.commit(valid).await.is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "external change");
}
