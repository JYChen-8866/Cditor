# Editor input / IME / table / code / Parley manual acceptance (2026-07-25)

> Related task: R9-005
>
> Branch: `codex/parley-text-layout` with uncommitted refactor changes
>
> Allowed conclusions: Passed / Failed / Not run. Automated coverage does not
> turn a manual case into Passed.

## Environment

| Item | Value |
| --- | --- |
| Device | Mac13,1 / Apple M1 Max / 64 GiB |
| OS | macOS 27.0, build 26A5378j |
| Display | Not recorded; must be recorded with visual evidence |
| Input methods | Not run; record exact Chinese/Japanese/Korean IME versions |
| Profile | dev, `scripts/dev/run_editor_sqlite.sh` |
| Fixture | SQLite document 1 plus targeted documents described below |

Startup smoke passed with an isolated `/tmp/cditor-r9-smoke.db`: the desktop
binary compiled, entered its GPUI run loop and opened the SQLite-backed editor
without a startup error. The temporary process was then stopped; a pre-existing
user-owned editor process was left untouched. The missing AI API key produced
the expected provider-disabled diagnostic and is outside this matrix.

## Automated prerequisite

The following prerequisites passed on 2026-07-25:

- `cargo test --workspace`;
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
- `cargo test -p cditor-editor-gpui --lib` (451 tests);
- `cargo test -p cditor-runtime --lib` (526 tests);
- `cargo test -p cditor-text --lib` (57 tests);
- full 100k mixed benchmark, including the IME-composition scenario.

The six clipboard tests that previously bypassed Session were corrected to use
`cditor_session::project_clipboard_import`; all six pass through the production
ImportPlan boundary.

## Manual matrix

| ID | Area | Steps | Expected result | Conclusion | Evidence |
| --- | --- | --- | --- | --- | --- |
| M-001 | Plain input | Type Latin, CJK, combining marks and emoji in empty, middle and end positions; undo/redo each sequence. | One insertion per key callback, no byte panic, caret and undo boundary remain correct. | Not run | Requires screen recording. |
| M-002 | Cross-block selection | Select forward and backward across wrapped blocks, copy, cut, paste and undo. | Highlight, clipboard content, focus and semantic selection survive scrolling and undo. | Not run | Requires screen recording. |
| M-003 | Chinese IME | Compose and commit multi-stage Pinyin in paragraph, heading, wrapped list and code block. | Preview is not committed early; candidate window follows caret; commit creates one undo step. | Not run | Record IME name/version and candidate-window video. |
| M-004 | Japanese/Korean IME | Repeat conversion, reconversion, cancel and `unmark_text` flows in block text. | No jump-to-end, duplicate input, double caret or stale marked range. | Not run | Requires installed native IMEs. |
| M-005 | Table-cell IME | Compose Chinese/Japanese/Korean text in first, wrapped and horizontally scrolled cells. | Candidate rect tracks the cell caret; composition remains cell-scoped; Enter and cancel follow cell policy. | Not run | Cross-reference `table-manual-acceptance.md` R-003. |
| M-006 | IME invalidation | While composing, scroll, resize the window, change zoom/font if exposed, refocus the same block and switch surfaces. | Stale handlers/layout identities are rejected without losing committed text or moving the candidate rect to another surface. | Not run | Requires telemetry plus video. |
| M-007 | Table resize | Drag row and column boundaries slowly and rapidly, then undo/redo. | Preview is painted every pointer frame; commit matches preview; following blocks and horizontal scrollbar update without jumps. | Not run | Cross-reference R-006/R-007. |
| M-008 | Table navigation | Use arrows, Tab/Shift-Tab, Enter, Home/End, range selection, row/column selection, merge/split and clipboard. | Focus hierarchy and commands stay table-scoped until an explicit escape; no hidden document mutation. | Not run | Cross-reference R-010 through R-016. |
| M-009 | Code highlighting | Insert Rust/JavaScript/JSON code, switch language/theme, edit Unicode and undo. | Highlight spans update incrementally, preserve source bytes and never disappear after focus or scroll. | Not run | Capture before/after screenshots. |
| M-010 | Code rendering | Inspect short, soft-wrapped and long code at 1x/2x display scale. | Monospace text is sharp; every wrapped line has identical font size/weight; caret and selection align with glyphs. | Not run | Pixel-level screenshots required. |
| M-011 | Parley geometry | Exercise CJK, Arabic/Hebrew mixed Bidi, emoji ZWJ, combining marks and soft-wrap boundaries. | Hit-test, visual navigation, selection, caret and line geometry come from the same Parley snapshot. | Not run | Record cases and display scale. |
| M-012 | Empty and wrapped blocks | Focus empty paragraph/heading/list/table cell and compare single-line versus wrapped final line. | Empty caret is visible; placeholder does not alter metrics; final wrapped line matches all other lines. | Not run | Screenshots required. |
| M-013 | Long surface | Open the 10 MiB code fixture and scroll/edit near beginning, middle and end. | Initial view does not shape the full source; internal scroll remains responsive and anchor is stable after local reflow. | Not run | Requires production segmented-layout telemetry. |
| M-014 | Soak | Type, compose, select and scroll continuously for 10 minutes with input tracing enabled. | Normal-input geometry fallback rate remains zero; no long-frame cluster or stale composition remains. | Not run | Attach redacted telemetry JSON and video. |

## Result

- Passed: 0 / 14.
- Failed: 0 / 14.
- Not run: 14 / 14.
- Gate R9-005: not passed. Automated prerequisites are green, but native IME,
  visual and pointer-interaction evidence has not been captured.
- Remaining risks: platform event ordering, candidate-window placement, display
  scale raster quality and production long-surface segmentation cannot be
  established by headless tests alone.
