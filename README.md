# Cditor

English | [简体中文](README.zh-CN.md)
<img width="1521" height="1073" alt="image" src="https://github.com/user-attachments/assets/3c38c5e0-2c44-4e4e-ac49-b6841dad2c9f" />

<img width="1510" height="1066" alt="image" src="https://github.com/user-attachments/assets/57b68491-9215-45fa-b159-967d4a2eec3c" />

Cditor is an open-source, block-based rich-text editor built in Rust with [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui). It is designed for large documents, native desktop performance, stable virtual scrolling, structured editing, and embedding in other GPUI applications.

> [!IMPORTANT]
> Cditor is under active development. APIs, persistence formats, and user-facing behavior may change before a stable release.

## Highlights

- Windowed rendering and virtual scrolling designed for documents with up to 100,000 blocks
- Paragraphs, headings, quotes, callouts, lists, todos, toggles, code blocks, tables, images, Mermaid diagrams, and whiteboards
- Rich-text marks, Markdown import/export, structural editing, and cross-block selections
- Native keyboard, clipboard, mouse, and IME integration, including CJK and emoji input
- Undo/redo backed by persistent document transactions
- Table editing with cell selection, copy/paste, merge/split, resize, reorder, and horizontal scrolling
- SQLite and PostgreSQL persistence adapters
- Inline AI integration through OpenAI-compatible providers
- Reusable SDK and GPUI component APIs
- macOS and Windows desktop targets

## Project Status

Cditor is currently suitable for development, experimentation, and integration testing. It is not yet presented as a production-stable editor.

The editor architecture intentionally separates document truth from the rendered viewport:

> The UI is a projection of the current viewport. Documents, selections, layout heights, transactions, and scroll state live in the editor kernel rather than in GPUI entity lifecycles.

This allows Cditor to load and render bounded payload windows while preserving document-wide editing operations.

For detailed design and implementation notes, see:

- [Large-document architecture](doc/large-document-rich-text-architecture.md)
- [Implementation status](doc/large-document-rich-text-implementation-status.md)
- [Project structure](doc/architecture/project-structure.md)
- [Component API and integration guide](doc/guides/cditor-component-integration.md)

## Quick Start

### Prerequisites

- A stable Rust toolchain with Rust 2024 edition support
- Git
- Platform-native build tools required by GPUI

On Windows, use the 64-bit MSVC Rust toolchain and install Visual Studio Build Tools with **Desktop development with C++** and a current Windows SDK.

### Run the desktop editor

```bash
cargo run -p cditor-desktop
```

Without a configured database, Cditor starts with an in-memory demo document.

Run a small demo:

```bash
CDITOR_SMALL_DEMO=1 cargo run -p cditor-desktop
```

Run the 100,000-block performance demo:

```bash
CDITOR_LARGE_DEMO=1 cargo run -p cditor-desktop
```

PowerShell equivalent:

```powershell
$env:CDITOR_LARGE_DEMO = "1"
cargo run -p cditor-desktop
```

### Optional database backends

Run with SQLite:

```bash
CDITOR_SQLITE_PATH=./cditor.db cargo run -p cditor-desktop
```

Start the development PostgreSQL container and run the editor:

```bash
docker compose up -d postgres
./scripts/dev/run_editor_postgres.sh
```

Or configure PostgreSQL directly:

```bash
export CDITOR_DATABASE_URL='postgres://user:password@localhost:5432/cditor'
export CDITOR_DOCUMENT_ID=1
cargo run -p cditor-desktop
```

`CDITOR_SQLITE_PATH` and `CDITOR_DATABASE_URL` are mutually exclusive.

## Build and Test

Build the default desktop application:

```bash
cargo build
```

Check the full workspace:

```bash
cargo check --workspace
```

Run all workspace tests:

```bash
cargo test --workspace
```

Run formatting and repository quality gates:

```bash
cargo fmt --all -- --check
./scripts/dev/check_workspace.sh
```

For performance-oriented development builds, use:

```bash
./scripts/dev/run_editor_sqlite.sh
./scripts/dev/run_editor_postgres.sh
```

These scripts default to the optimized `editor-dev` Cargo profile while retaining development diagnostics.

## Configuration

Common environment variables:

| Variable | Description |
| --- | --- |
| `CDITOR_DATABASE_URL` | PostgreSQL connection URL |
| `CDITOR_SQLITE_PATH` | SQLite database path |
| `CDITOR_DOCUMENT_ID` | Document to open in database mode |
| `CDITOR_SMALL_DEMO` | Load the small built-in demo |
| `CDITOR_LARGE_DEMO` | Load the 100,000-block demo |
| `CDITOR_READONLY` | Open the editor in read-only mode |
| `CDITOR_DEBUG_OVERLAY` | Show layout and viewport diagnostics |
| `CDITOR_AI_API_KEY` | API key for an OpenAI-compatible AI provider |
| `CDITOR_AI_BASE_URL` | OpenAI-compatible API base URL |
| `CDITOR_AI_MODEL` | AI model name |

Boolean variables accept `1`, `true`, `yes`, or `on` and their corresponding false values, case-insensitively.

Never commit API keys or production database credentials. Use process environment variables or a local, ignored `.env` file.

## Embedding Cditor

Cditor can be embedded into another GPUI application. The reusable integration surface is split across the SDK, protocol, session, runtime, and GPUI editor crates.

Applications embedding `CditorV2View` directly must install the editor key bindings during GPUI startup:

```rust
cditor_editor_gpui::input::bind_cditor_keys(cx);
```

See the [Cditor Component API and Integration Guide](doc/guides/cditor-component-integration.md) for initialization, commands, events, persistence providers, and lifecycle details.

## Workspace Overview

```text
apps/
├── cditor-desktop/              Desktop GPUI application
└── cditor-web/                  Web application experiments
components/
├── cditor-component/            Shared GPUI components
├── cditor-whiteboard/           Cditor whiteboard component
└── cditor-whiteboard-gpui/      GPUI whiteboard rendering adapter
crates/
├── cditor-core/                 Document model, blocks, selections, and transactions
├── cditor-viewport/             Framework-independent viewport algorithms
├── cditor-runtime/              Live document state and projection
├── cditor-session/              Application service and task coordination
├── cditor-editor-gpui/          GPUI rendering and platform input
├── cditor-editor-protocol/      Commands, queries, events, and protocol types
├── cditor-sdk/                  Public embedding API
├── cditor-text/                 Text shaping, layout, and geometry
├── cditor-storage*/             Storage contracts and adapters
├── cditor-import-export/        External format support
└── cditor-ai*/                  AI contracts and provider adapters
```

A more detailed responsibility and dependency breakdown is available in [Project Structure](doc/architecture/project-structure.md).

## Third-Party Components

Cditor includes and adapts open-source components from other projects. Their original copyright notices and license terms remain applicable.

### Zed Mermaid renderer

Cditor uses Zed's `mermaid_render` crate for native Mermaid diagram rendering:

- Upstream: [zed-industries/zed](https://github.com/zed-industries/zed)
- Component: `crates/mermaid_render`
- Integration: pinned Git dependency recorded in `Cargo.lock`

Zed and its Mermaid renderer retain their respective upstream copyright and license notices.

See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for resolved revisions, transitive components, bundled fonts, icons, and required attribution material.

## Contributing

Issues and pull requests are welcome. Before submitting a change:

1. Keep domain logic independent from GPUI where possible.
2. Add tests for new behavior and regressions.
3. Run formatting, checks, and relevant tests.
4. Do not commit secrets, local database files, or generated build artifacts.
5. Preserve third-party copyright and license notices.

Recommended local checks:

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
```

## License

Except where a file or bundled third-party component states otherwise, Cditor is licensed under the **GNU General Public License v3.0 or later** (`GPL-3.0-or-later`). See [LICENSE-GPL](LICENSE-GPL).

Third-party components remain governed by their own licenses. In particular, the drafft-ink-derived whiteboard code is provided under the upstream **GNU Affero General Public License v3** terms. Consult [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and the license files distributed with each component before redistribution.
