use cditor_storage_markdown::MarkdownStorage;
use cditor_storage::{DocumentStorage, LoadDocumentRequest, DOCUMENT_INDEX_VISIBLE_VERSION};
use cditor_core::ids::DocumentId;
use cditor_storage::layout_cache::LayoutCacheKey;
use cditor_core::layout::PAGE_POLICY_VERSION;

#[tokio::test]
async fn test_markdown_storage_roundtrip() {
    // 创建临时目录
    let temp_dir = std::env::temp_dir().join("cditor-markdown-test");
    std::fs::create_dir_all(&temp_dir).unwrap();

    // 创建测试 Markdown 文件
    let test_markdown = r#"# Hello CDitor

This is a test paragraph with some **bold** text.

## Section 1

- Item 1
- Item 2
- Item 3

```rust
fn main() {
    println!("Hello, world!");
}
```

> This is a quote

## Section 2

More content here.
"#;

    let doc_path = temp_dir.join("1.md");
    std::fs::write(&doc_path, test_markdown).unwrap();

    // 创建 MarkdownStorage
    let storage = MarkdownStorage::new(temp_dir.clone());

    // 加载文档
    let request = LoadDocumentRequest {
        document_id: DocumentId::from(1u64),
        initial_payload_window_blocks: 32,
        visible_index_version: DOCUMENT_INDEX_VISIBLE_VERSION,
        layout_key: LayoutCacheKey {
            width_bucket: 10,
            exact_width_px: 800,
            content_version: 1,
            attrs_version: 0,
            style_version: 0,
            font_version: 0,
            theme_version: 0,
            scale_factor_milli: 1_000,
        },
        page_policy_version: PAGE_POLICY_VERSION,
    };

    let loaded = storage.load_document(request).await.unwrap();

    // 验证加载的 blocks
    println!("✅ Loaded {} blocks", loaded.records.len());
    println!("✅ Loaded {} payloads", loaded.initial_payloads.len());

    assert!(loaded.records.len() > 0, "Should have loaded some blocks");
    assert_eq!(loaded.records.len(), loaded.initial_payloads.len(), "Records and payloads should match");

    // 打印前几个 blocks
    for (i, payload) in loaded.initial_payloads.iter().take(5).enumerate() {
        println!("Block {}: {:?} - {:?}", i, payload.kind, payload.plain_text());
    }

    println!("\n✅ Markdown storage test passed!");

    // 清理
    std::fs::remove_dir_all(&temp_dir).ok();
}
