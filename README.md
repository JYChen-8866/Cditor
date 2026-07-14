# Cditor

Cditor is a large-document rich text editor built with Rust and GPUI. It is designed to support 100,000-level Blocks, sophisticated rich text, cross-page selections, stable virtual scrolling, and PostgreSQL persistence.

The project is under active ongoing development. Its core architecture, runtime, tables, Markdown support, IME integration, clipboard handling, media assets, Mermaid rendering, whiteboard embedding, inline AI, and PostgreSQL storage have all been implemented and tested. All production readiness and performance acceptance criteria are defined in the [Large-Document Architecture](doc/large-document-rich-text-architecture.md), [Implementation Status](doc/large-document-rich-text-implementation-status.md), and corresponding acceptance documents.

## Core Capabilities
- Lightweight indexing, paginated height modeling, and windowed rendering for 100,000-level Blocks
- Document state, selections, layout heights, and virtual scroll states decoupled from UI Entities
- Diverse Block types: Paragraph, Heading, Quote, Callout, Todo, Lists, Toggle, Code, Table, Image, File, Mermaid, Whiteboard, Embed, Database, and more
- Rich text marks, Markdown import/export, and incremental shortcut input
- Cross-Block clipboard operations, structural editing, undo/redo, and persistent transactions
- Full CJK, Emoji, UTF-8/UTF-16 offset support, plus IME composition and candidate positioning
- Table cell editing, multi-cell selections, copy/paste, merge/split, resizing, reordering, and horizontal scrolling
- PostgreSQL-backed document storage, payload persistence, layout caching, full-text search, asset management, transaction workflows, recovery queues, and sync outboxes
- Streaming preview and in-place replacement for inline AI
- Native Mermaid rendering and integrated standalone ding-board whiteboard
- Regression test suites covering large-document rendering, scroll stability, input latency, and viewport projection logic

## Architectural Principles
The most critical constraint governing this project is:
> The UI is merely a projection of the current viewport; the source of truth for documents, selections, layout heights, and scroll states must live within the editor kernel.

This enforces the following rules:
- GPUI Entities may be created and destroyed alongside virtual viewport windows; document state must never depend on Entity lifecycles.
- Copy, Cut, Paste, Undo, Redo, and cross-page selections read data directly from the kernel, not the live UI tree.
- Precise layout calculations are prioritized for content near the active viewport; distant unmeasured content allows estimated heights that converge once loaded.
- Smooth continuous scrolling and stable viewport rendering take precedence over instant absolute accuracy of global total document height.
- Critical input hot paths must never synchronously block on PostgreSQL calls, full payload loads, global layout recalculations, or background indexing tasks.

See the full design specification in [Architecture for 100,000-Block Large Documents](doc/large-document-rich-text-architecture.md).

## Project Directory Layout
```text
.
├── Cargo.toml                   # Workspace members, unified versioning, edition, and license definitions
├── Cargo.lock                   # Single workspace dependency lockfile
├── README.md                    # Project entry documentation
├── .env.example                 # Template for local environment variables (no real secrets included)
├── docker-compose.yml           # Local PostgreSQL configuration for development & testing
├── assets/                      # Shared static assets used across the Cditor application
├── config/                      # Committed non-sensitive runtime configuration files
├── crates/
│   ├── core/                    # Document kernel, rich text logic, transactions, selections, layout models
│   ├── editor/                  # GPUI-agnostic viewport, scrolling, window planning, and hit-test algorithms
│   ├── runtime/                 # Live document state, edit orchestration, projection logic, task scheduling
│   ├── store/                   # Storage abstractions, caching strategies, persistence state machine
│   ├── store-postgres/          # PostgreSQL implementation, database migrations, integration tests
│   ├── app/                     # GPUI application entrypoint, rendering, platform input handling, overlays
│   ├── ai/                      # Inline AI provider implementations and configuration loaders
│   └── ding-board/              # Standalone embeddable whiteboard crate
├── doc/
│   ├── architecture/            # Current system and subsystem architecture documentation
│   ├── plans/                   # Feature roadmaps and issue analysis documents
│   ├── acceptance/              # Manual acceptance test guides and completion summaries
│   ├── guides/                  # End-user operation & developer usage guides
│   ├── prototypes/              # Interactive UI/editor interaction prototypes
│   ├── refactor/                # Active refactoring plans in progress
│   └── archive/                 # Historical migration materials (does not reflect current structure)
└── scripts/
    ├── dev/                     # Launch scripts, structural validation, workspace health checks
    ├── database/                # Remote PostgreSQL utilities and SSH tunnel tooling
    └── archive/                 # One-off completed migration scripts (retired workflows)
```

For a detailed breakdown, refer to [Cditor Project Structure](doc/architecture/project-structure.md).

## Workspace Crate Responsibilities
| Directory | Cargo Package | Core Responsibilities | Excluded Dependencies & Logic |
| --- | --- | --- | --- |
| `crates/core` | `cditor-core` | Base models: Blocks, DocumentIndex, RichText, Selections, Transactions, Layout | GPUI, SQLx, concrete database implementations |
| `crates/editor` | `cditor-editor` | VirtualScroll, ScrollAnchor, WindowPlanner, HitTest, Trace Replay | GPUI Views, PostgreSQL logic |
| `crates/runtime` | `cditor-runtime` | DocumentRuntime, editing sessions, projection, payload windows, task scheduling | Application windows, visual UI components |
| `crates/store` | `cditor-storage` | Storage contracts, layout cache, debouncing, optimistic persistence | PostgreSQL SQL implementations, GPUI |
| `crates/store-postgres` | `cditor-storage-postgres` | PostgreSQL connection pools, migrations, storage backends, sync queues, type mapping | Editor interaction logic, UI state |
| `crates/app` | `cditor-app` | GPUI app entrypoint, Block rendering, input handling, overlays, persistence bridge | Source-of-truth document state, global scroll state |
| `crates/ai` | `cditor-ai` | AI provider integrations, config parsing, streaming results, request cancellation | Document structure, UI rendering logic |
| `crates/ding-board` | `ding-board` | Standalone whiteboard models, rendering, input handling, asset management | Direct dependencies on Cditor core |

### Dependency Graph
```text
cditor-app
  ├──> cditor-runtime ───> cditor-editor ───> cditor-core
  │          ├───────────> cditor-core
  │          └───────────> cditor-ai
  ├──> cditor-storage-postgres ───> cditor-storage ───> cditor-core
  ├──> cditor-editor
  ├──> cditor-ai
  └──> ding-board
```

Arrows point from dependent crates to the crates they consume. For example:
`cditor-runtime` relies only on `cditor-core`, `cditor-editor`, and `cditor-ai`.
`cditor-app` serves as the final assembly layer, composing all above crates alongside `ding-board`.

Naming note for `cditor-editor`: This crate is frequently misinterpreted. It contains only viewport calculation logic with no UI framework coupling. All GPUI rendering and user interaction entrypoints live in `cditor-app`.

PostgreSQL cold-start and payload-window I/O live in `cditor-app`, the composition root. The app converts database rows into storage-neutral runtime inputs, so new storage backends do not propagate concrete database types into `cditor-runtime`.

## Environment Prerequisites
### Mandatory
- Stable Rust toolchain supporting the Rust 2024 edition
- Git: GPUI and Mermaid renderer are pinned to specific commits in the Zed repository; Git dependencies are fetched on initial build
- Platform-native compilation tooling required for GPUI

### Windows Toolchain

Cditor uses the native MSVC GPUI backend on Windows. Install:

- 64-bit Windows 10 or Windows 11
- [Rustup](https://rustup.rs/) with the `stable-x86_64-pc-windows-msvc` toolchain
- Visual Studio Build Tools with **Desktop development with C++**, MSVC v143 or newer, and a current Windows SDK
- Git for Windows

Verify the active host toolchain in PowerShell:

```powershell
rustup default stable-x86_64-pc-windows-msvc
rustc -vV
cargo --version
```

The GNU Rust target is not supported by the desktop build. Use the MSVC target shown above.

### Optional
- Docker & Docker Compose: Local PostgreSQL deployment
- OpenAI API-compatible LLM service: For inline AI functionality
- SSH: Remote PostgreSQL initialization and tunnel script support

Verify Rust installation:
```bash
rustc --version
cargo --version
```

## Quick Start
### 1. Run Without a Database
If `CDITOR_DATABASE_URL` is unset, the binary defaults to an in-memory backend with no PostgreSQL required:
```bash
cargo run -p cditor-app
```

Launch a small demo document:
```bash
CDITOR_SMALL_DEMO=1 cargo run -p cditor-app
```

PowerShell uses this equivalent syntax on Windows:

```powershell
$env:CDITOR_SMALL_DEMO = "1"
cargo run -p cditor-app
```

Launch a large demo document with 100,000 Blocks:
```bash
CDITOR_LARGE_DEMO=1 cargo run -p cditor-app
```
The large demo constructs performance-testing large documents, resulting in longer startup times and higher memory usage than standard mode.

### Keyboard and IME Architecture

Cditor follows Zed's GPUI input architecture across macOS and Windows:

1. GPUI's native platform backend translates physical keys to canonical keystrokes such as `enter`, `backspace`, and `home`.
2. The Cditor keymap converts keystrokes into typed actions inside the `CditorEditor` key context.
3. Context-aware action handlers route the action to the document, table cell, slash menu, table menu, code-language input, or AI prompt.
4. Printable text and IME composition never depend on physical key names; they flow through GPUI's `EntityInputHandler` using UTF-16 platform ranges and UTF-8 document offsets.

The editor keeps document line endings as LF internally. Windows TSF and clipboard input are normalized from CRLF at the platform boundary. The main editing bindings are:

| Operation | macOS | Windows |
| --- | --- | --- |
| Create/split Block | `Enter` | `Enter` |
| Soft line break | `Shift+Enter` | `Shift+Enter` |
| Create Block below | `Command+Enter` | `Ctrl+Enter` |
| Select current Block, then full document | `Command+A` once/twice | `Ctrl+A` once/twice |
| Undo / redo | `Command+Z` / `Command+Shift+Z` | `Ctrl+Z` / `Ctrl+Y` or `Ctrl+Shift+Z` |
| Line start / end | `Home`, `End`, or Zed's macOS aliases | `Home`, `End` |
| Clipboard | `Command+C/X/V` | `Ctrl+C/X/V`; Zed-compatible `Ctrl+Insert`, `Shift+Delete`, `Shift+Insert` |

Applications embedding `CditorV2View` directly must install the keymap once during GPUI startup before opening the window:

```rust
cditor_app::gui::input::bind_cditor_keys(cx);
```

### 2. Local PostgreSQL Deployment
Start the development database container:
```bash
docker compose up -d postgres
```

Launch the editor with default development database credentials:
```bash
./scripts/dev/run_editor.sh
```
This script injects the default dev connection string:
```text
CDITOR_DATABASE_URL=postgres://cditor:cditor@localhost:5432/cditor_dev
```
If `CDITOR_DOCUMENT_ID` is not explicitly set, the application loads document ID `1`.

Check database container status:
```bash
docker compose ps
```

Stop the container:
```bash
docker compose down
```
Database data persists in Docker volumes. Run `docker compose down -v` to delete all local database data — use with caution.

### 3. Custom PostgreSQL Instance
```bash
export CDITOR_DATABASE_URL='postgres://user:password@host:5432/database'
export CDITOR_DOCUMENT_ID=42
cargo run -p cditor-app
```

Remote PostgreSQL tooling documentation: [scripts/README.md](scripts/README.md) and [Remote PostgreSQL Guide](doc/architecture/remote-postgres.md)

## Runtime Configuration
### Editor Environment Variables
| Variable | Default | Description |
| --- | --- | --- |
| `CDITOR_DATABASE_URL` | Unset | Enables PostgreSQL when defined; falls back to in-memory/demo backends if empty |
| `CDITOR_DOCUMENT_ID` | `1` (database mode only) | Target document ID to open |
| `CDITOR_WORKSPACE_ID` | Unset | Workspace identifier |
| `CDITOR_SMALL_DEMO` | `false` | Load small demo document when running without a database |
| `CDITOR_LARGE_DEMO` | `false` | Load 100,000-Block demo document when running without a database |
| `CDITOR_READONLY` | `false` | Enable read-only editor mode |
| `CDITOR_DEBUG_OVERLAY` | `false` | Render debug overlays showing layout, viewport, and scroll metrics |
| `CDITOR_PAYLOAD_WINDOW_SIZE` | `128` | Chunk size for payload window loading; minimum value = 1 |
| `CDITOR_SEED_LARGE_DEMO` | `false` | Populate PostgreSQL with a large demo dataset |
| `CDITOR_SEED_LARGE_DEMO_BLOCKS` | `100000` | Number of Blocks generated for PostgreSQL large demo |
| `CDITOR_FORCE_RESEED_LARGE_DEMO` | `false` | Drop and regenerate full PostgreSQL demo data |
| `CDITOR_TRACE_INPUT` | `false` | Print verbose logs for platform input and IME events |
| `CDITOR_TRACE_TABLE` | `false` | Print table interaction debug traces |
| `CDITOR_TRACE_MARKDOWN` | `false` | Print Markdown parsing and clipboard operation traces |
| `CDITOR_TRACE_BLOCK_COLOR` | `false` | Print block color target, persistence, and resolved paint traces |

Boolean variables accept case-insensitive values: `1/true/yes/on` and `0/false/no/off`.

### Inline AI Configuration
Non-sensitive AI settings live in [config/ai.toml](config/ai.toml):
```toml
base_url = "https://api.deepseek.com"
model = "deepseek-v4-flash"
```

API keys must be supplied via environment variables or a local `.env` file — never commit raw tokens to version control:
```bash
export CDITOR_AI_API_KEY='your-api-key'
```

Compatible legacy environment variables:
| Cditor Variable Name | Alias Compatibility Variables |
| --- | --- |
| `CDITOR_AI_API_KEY` | `OPENAI_AUTH_TOKEN`, `OPENAI_API_KEY` |
| `CDITOR_AI_BASE_URL` | `OPENAI_BASE_URL` |
| `CDITOR_AI_MODEL` | `OPENAI_MODEL` |
| `CDITOR_AI_CONFIG` | Custom file path for TOML AI configuration |

AI configuration priority order: process environment variables → local `.env` file → config file → hardcoded built-in defaults. The application runs normally without an API key, falling back to a mock AI provider.

## Building the Project
Default target crate: `cditor-app`
```bash
cargo build
```

Build all workspace crates:
```bash
cargo build --workspace
```

Syntax and type checking for full workspace:
```bash
cargo check --workspace
```

Check individual crates:
```bash
cargo check -p cditor-core
cargo check -p cditor-runtime
cargo check -p cditor-app
```

Launch editor with GPUI runtime shader feature enabled:
```bash
cargo run -p cditor-app --features runtime-shaders
```

### GitHub Desktop Artifacts

The `Desktop builds` GitHub Actions workflow produces three downloadable artifacts:

| Artifact | Output | Target |
| --- | --- | --- |
| `Cditor-Windows-x64` | `Cditor.exe` and SHA-256 checksum | 64-bit Windows |
| `Cditor-macOS-Apple-Silicon` | `Cditor-macOS-arm64.dmg` and SHA-256 checksum | Apple Silicon Macs |
| `Cditor-macOS-Intel` | `Cditor-macOS-x64.dmg` and SHA-256 checksum | Intel Macs |

The workflow runs for pushes to `main`, pull requests targeting `main`, version tags, and manual dispatches. Branch and pull-request builds remain available from the workflow run's **Artifacts** section. A `v*` tag additionally creates a GitHub Release and attaches all three installers plus their SHA-256 checksum files.

Desktop installers belong in GitHub Releases rather than GitHub Packages, which is a package registry for formats such as containers and language packages. Download published EXE and DMG files from the repository's **Releases** page.

Release artifacts use the workspace's performance-first Cargo profile: optimization
level 3, fat whole-program LTO, one codegen unit, abort-on-panic, disabled
incremental compilation, and stripped symbols. Windows additionally links the
static C runtime. The workflow deliberately avoids `target-cpu=native`, so a
binary built on a GitHub runner does not accidentally require that runner's CPU
instruction set. Run `./scripts/dev/check_release_profile.sh` to verify these
release invariants locally.

The macOS application bundles are ad-hoc signed so the DMG layout and bundle integrity can be verified in CI. They are not Apple-notarized because the public repository does not contain Apple Developer signing credentials. macOS may therefore require using **Open** from Finder's context menu the first time the application is launched.

## Testing & Quality Gates
Run all default unit tests:
```bash
cargo test --workspace
```

Test individual crates:
```bash
cargo test -p cditor-core
cargo test -p cditor-editor
cargo test -p cditor-runtime
cargo test -p cditor-app --lib
```

Structural validation script:
```bash
./scripts/dev/check_structure.sh
```

Full local CI quality gate suite:
```bash
./scripts/dev/check_workspace.sh
```
The full gate executes these steps sequentially:
1. Project directory structure validation
2. `cargo fmt --all -- --check` (format compliance)
3. `cargo check --workspace` (static analysis)
4. `cargo test --workspace` (unit test suite)

### PostgreSQL Integration Tests
Spin up an isolated test database instance:
```bash
docker compose up -d postgres_test
export CDITOR_TEST_DATABASE_URL='postgres://cditor:cditor@localhost:5433/cditor_test'
```

PostgreSQL integration tests are marked `ignored` by default to avoid external service dependencies during standard unit test runs. Execute them per crate explicitly:
```bash
cargo test -p cditor-storage-postgres -- --ignored
cargo test -p cditor-app --lib -- --ignored
```

Many ignored integration tests generate or load 100,000-Block datasets, resulting in longer execution times and increased database resource consumption.

## Development Standards
### File & Directory Rules
- All Rust source files (excluding whiteboard modules) must stay under 700 lines of code.
- Files exceeding the line limit must be split by functional domain: models, input handling, rendering, persistence, geometry, or test logic.
- Unit tests belong in sibling `*_tests.rs` files or module-local `tests/` subdirectories.
- One-off migration scripts reside in `scripts/archive/` and are not used for daily development workflows.
- Historical legacy documentation lives in `doc/archive/` and does not reflect current implementation structure.
- The workspace maintains exactly one root `Cargo.lock` file.
- Secrets, database credentials, and absolute local file paths must never be committed to version control.

`./scripts/dev/check_structure.sh` enforces the 700-line limit, validates deprecated paths, scans for unwanted system metadata, and prevents `cditor-runtime` from acquiring PostgreSQL, SQLx, or GPUI dependencies.

### Feature Placement Guidelines
| Feature Domain | Primary Code Location |
| --- | --- |
| New Block types & payload schemas | `crates/core/src/block`, `crates/core/src/rich_text` |
| Document edits & selection logic | `crates/core/src/edit` |
| Height estimation & layout indexing | `crates/core/src/layout` |
| Virtual scrolling, anchors, window planning | `crates/editor/src/scroll`, `crates/editor/src/window` |
| Live document state & projection logic | `crates/runtime/src/document_runtime`, `crates/runtime/src/projection` |
| Task scheduling & performance budgeting | `crates/runtime/src/scheduling` |
| Storage abstractions & caching logic | `crates/store/src` |
| PostgreSQL tables & query implementations | `crates/store-postgres/migrations`, `crates/store-postgres/src/stores` |
| GPUI Block visual rendering | `crates/app/src/gui/block` |
| Keyboard, mouse, and IME input | `crates/app/src/gui/input`, `crates/app/src/gui/app/input` |
| Floating overlays & popup interactions | `crates/app/src/gui/overlay` |
| AI provider implementations | `crates/ai/src` |
| Cditor integration with whiteboard | `crates/app/src/gui/block/whiteboard` |

All new functionality must include accompanying unit tests. Any feature touching database logic, cross-crate transactions, or state recovery workflows additionally requires integration tests.

## Debugging Workflows
Full debug trace bundle (small demo, layout overlay, input logging):
```bash
CDITOR_SMALL_DEMO=1 CDITOR_DEBUG_OVERLAY=1 CDITOR_TRACE_INPUT=1 cargo run -p cditor-app
```

Table interaction debugging:
```bash
CDITOR_SMALL_DEMO=1 CDITOR_TRACE_TABLE=1 cargo run -p cditor-app
```

Markdown parsing & clipboard issue debugging:
```bash
CDITOR_SMALL_DEMO=1 CDITOR_TRACE_MARKDOWN=1 cargo run -p cditor-app
```

## Documentation Index
- [Root Documentation Index](doc/README.md)
- [Architecture for 100,000-Block Large Documents](doc/large-document-rich-text-architecture.md)
- [Current Implementation Status](doc/large-document-rich-text-implementation-status.md)
- [Current Project Structure](doc/architecture/project-structure.md)
- [V2 Rich Text Editor GUI Architecture](doc/architecture/v2-rich-text-editor-gui-architecture.md)
- [Database Implementation Plan](doc/architecture/database-implementation-plan.md)
- [Open Issues & Task Roadmap](doc/plans/current-editor-issues-deep-analysis-and-task-list.md)
- [Notion-Style Table Feature Roadmap](doc/plans/notion-table-feature-plan.md)
- [Table Manual Acceptance Test Guide](doc/acceptance/table-manual-acceptance.md)
- [Scripts Usage Guide](scripts/README.md)

All content under `doc/archive/` exists solely to preserve historical migration context and does not reflect current directory structures, command usage, or implementation logic.

## Structural Review Conclusion
The current project layout is logically organized:
- Workspace crates are cleanly separated by responsibility: core data models, viewport algorithms, runtime state, storage layers, UI rendering, AI services, and standalone whiteboard functionality.
- The `runtime` source directory aligns with its matching `cditor-runtime` crate.
- All GPUI UI code is isolated within the `app` crate; core data models have no reverse UI dependencies.
- PostgreSQL persistence logic is decoupled from generic storage abstractions.
- PostgreSQL cold start and window loading are assembled by `cditor-app`; `cditor-runtime` receives storage-neutral records.
- Documentation, utility scripts, and test suites are grouped by functional purpose.
- A strict 700-line source file limit enforces modular code organization for all non-whiteboard modules.

`cditor-editor` implements pure UI-agnostic viewport algorithms despite its name; all visual editing and platform input remain in `cditor-app`. The structure check protects the more important dependency boundaries automatically.

## License
Project licensing terms and third-party dependency notices:
- [LICENSE-GPL](LICENSE-GPL)
- [LICENSE-APACHE](LICENSE-APACHE)
- [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)
