# Editor Component Boundary

Cditor is a rich-text editor component, not an application database layer.

## Ownership

The host application owns:

- workspace, knowledge base, document tree, permissions, favorites, tags and trash;
- globally stable product document identifiers;
- database paths, URLs, pools, migrations and credentials;
- local/cloud sync topology and cross-document search;
- asset upload lifecycle and product-level metadata.

Cditor owns:

- block content, document ordering and editor-specific attributes;
- selection, undo/redo, layout cache and virtual scrolling;
- edit transaction and recovery invariants;
- the storage-neutral `DocumentStorage` port required for windowed loading and durable commits.

## Composition

```rust
let storage: Arc<dyn DocumentStorage> = app.open_content_store().await?;
let editor = Cditor::new()
    .with_document_id(document_id)
    .with_storage(storage);
```

`Cditor::new()` defaults to an empty in-memory document. The component never opens a path,
parses a database URL, creates a pool, runs a migration or chooses a workspace.

Concrete SQLite, PostgreSQL or service adapters belong in the host application's infrastructure
layer. They implement `DocumentStorage` and may translate the host's globally stable document ID
to their own schema without exposing that schema to Cditor.

## Dependency Rule

```text
host application -> database adapter -> DocumentStorage <- Cditor session/runtime/UI
```

The dependency points toward the storage contract. Cditor must not depend on a concrete adapter.
