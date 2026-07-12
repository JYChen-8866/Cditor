# Cditor / ding-board integration architecture

## Goals

- Keep `ding-board` reusable and independent from cditor document internals.
- Persist whiteboards through the existing `WhiteboardPayload.scene_json` contract.
- Render document blocks through ding-board's local thumbnail API.
- Create GPUI thumbnail entities only for the current virtual render window.
- Keep whiteboard rendering and resources out of `core`, `engine`, and storage crates.

## Workspace ownership

```text
crates/
  core/                         # WhiteboardPayload { scene_json }, block kind, layout rule
  engine/                       # payload/window/undo/persistence orchestration; no GPUI dependency
  store/                        # storage contracts
  store-postgres/               # opaque scene_json persistence
  ding-board/                   # reusable whiteboard product crate
    src/                        # Scene, WhiteboardView, BoardThumbnailView
    assets/                     # board-owned fonts and icons
    examples/                   # standalone board host examples
  app/
    src/gui/block/whiteboard/   # cditor-specific host adapter
      mod.rs
      cache.rs                  # visible-window thumbnail entity lifecycle
      render.rs                 # stable block frame + BoardThumbnailView
      style.rs                  # GuiTheme -> WhiteboardStyle mapping
```

`ding-board` belongs at `crates/ding-board`. It must not live under
`crates/app`, because the board has its own model, serialization, editing UI,
thumbnail renderer, examples, and assets. The app depends on ding-board; the
dependency must never point in the opposite direction.

## Dependency direction

```text
cditor-core <- cditor-runtime <- cditor-app -> ding-board
      ^              ^               |
      |              |               +-> GPUI thumbnail entities
      +--------------+------------------> opaque scene_json only
```

Rules:

1. `cditor-core` owns the block payload contract, not the board scene types.
2. `cditor-runtime` treats scene JSON as opaque document data.
3. `cditor-app` parses scene JSON and maps editor theme tokens.
4. `ding-board` never imports cditor crates.
5. PostgreSQL stores scene JSON; thumbnail state is reconstructible cache data.

## Document embed flow

```text
EditorViewProjection
  -> visible WhiteboardPayload only
  -> Scene::from_json(scene_json)
  -> WhiteboardView::new_read_only(scene, style)
  -> Entity<WhiteboardView>
  -> Whiteboard block stable layout frame
```

The document embed uses a read-only `WhiteboardView`. It allows direct pointer
dragging to inspect the canvas, but read-only views do not register wheel input,
so document scrolling remains owned by cditor. Editing tools, selection changes,
and scene persistence remain disabled in this state.

Read-only rendering uses a dedicated lightweight branch: it does not construct
editor toolbars, input handlers, selection chrome, or editing panels. Both
read-only and full-editor rendering cull elements outside the camera viewport
with a screen-space safety margin. Camera-independent text glyph layouts are
cached by element content/style signature and reused while panning or zooming.

## Cache policy

`WhiteboardThumbnailCache` is owned by `CditorV2View`.

- Key: `BlockId`.
- Version: block `content_version`.
- Cache hit: reuse the existing read-only `Entity<WhiteboardView>`.
- Version change: replace the read-only entity from the latest persisted scene.
- Window change: drop entries that are no longer in the projected window.
- Runtime replacement: clear the cache.

This keeps entity lifetime aligned with virtualization while preserving payload
and stable layout truth in the runtime.

## Stable layout

Whiteboard blocks reserve a 472 px inner thumbnail frame, matching the existing
480 px whiteboard stable-box estimate after block shell padding. Empty or invalid
legacy scenes still produce a renderable empty thumbnail snapshot, so layout does
not jump after asynchronous UI creation.

## Full editor boundary

The document block remains read-only. Double-clicking it creates a dedicated
`Entity<WhiteboardView>` in an app-level overlay:

```text
app/src/gui/overlay/whiteboard_editor.rs
app/src/gui/app/cditor_v2_view/whiteboard.rs
engine/src/document_runtime/whiteboard.rs
```

The session owns board focus, wheel capture, its built-in editing UI and
`set_on_change`. Scene changes update the runtime whiteboard payload and schedule
the existing PostgreSQL dirty path. The board keeps its own fine-grained undo
history, so scene persistence updates do not create one document undo snapshot per
pointer movement.

## ding-board internal follow-up

The imported crate is correctly placed but its current `src/lib.rs` is too large.
Its public API should remain stable while implementation moves toward:

```text
ding-board/src/
  lib.rs
  model/
  camera/
  geometry/
  render/
  input/
  tools/
  thumbnail/
  embed/
  persistence/
```

This internal split is independent of cditor integration and should be performed
as a dedicated ding-board refactor with its existing behavior tests preserved.

## Completed checklist

- [x] Add `crates/ding-board` to the workspace.
- [x] Add ding-board as an app-only dependency.
- [x] Add direct persisted-scene thumbnail snapshot API.
- [x] Render whiteboard blocks with a wheel-transparent read-only `WhiteboardView`.
- [x] Add read-only render specialization, viewport culling, and glyph layout caching.
- [x] Cache thumbnail entities by block and content version.
- [x] Evict thumbnail entities outside the current virtual window.
- [x] Map cditor theme tokens to `WhiteboardStyle`.
- [x] Preserve a stable whiteboard block height for empty scenes.
- [x] Add full-screen whiteboard editing session and runtime scene persistence.
- [ ] Split ding-board's oversized `src/lib.rs` into feature modules.
