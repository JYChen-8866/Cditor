# cditor-whiteboard-drafft

An isolated native GPUI integration for the Rust crates from
[Drafft Ink](https://github.com/PatWie/drafft-ink). It does not use WebView and
does not modify or depend on the existing `cditor-whiteboard` component.

The crate is intentionally not connected to the desktop application yet. It is
a pure GPUI port of Drafft's application shell and renderer: Drafft remains the
authoritative canvas/model/tool implementation, while GPUI owns native window
lifecycle, input dispatch, UI, path tessellation and painting.

The crate uses Drafft Ink's own public `Canvas`, `Camera`, shapes, snapping,
tools, selection and document APIs directly. It does not use WebView and does
not copy frames through Vello, wgpu, CoreVideo or CPU bitmaps. The local code is
limited to the host event adapter, a retained GPUI paint plan, path conversion,
and GPUI controls that upstream does not provide.

The running GPUI application includes every Drafft drawing tool, pressure-aware
freehand, deterministic rough strokes, clipped pattern fills, text and IME
editing, math editing, raster images, selection/manipulation handles, pan/zoom,
undo/redo, grouping, snapping and native property controls. Clipboard support
accepts Drafft shapes, Excalidraw JSON, Mermaid source and platform images.
Images preserve rotation and opacity through a retained transformed-raster
cache.

The native shell also includes asynchronous JSON/Excalidraw open and save,
recent files, Excalidraw library tabs, 1x/2x/3x PNG export/copy, keyboard help,
and tab document/camera snapshots. Collaboration remains deliberately outside
this isolated renderer port because enabling Loro/network presence is a product
architecture and AGPL deployment decision, not a presentation detail.

The audited upstream repository is vendored verbatim under `vendor/drafft-ink`
(excluding only its `.git` metadata). See `UPSTREAM.md` for the immutable source
revision and update procedure.

Features:

- `drafft-core`: enables the pinned `drafftink-core` dependency.
- `drafft-vello`: enables Drafft Ink's Vello renderer and implies
  `drafft-core`.

`drafft-core` is enabled by default; Vello remains an explicit GPU integration
feature only for upstream renderer comparison and is not part of the GPUI
runtime. See
`doc/plans/drafft-ink-gpui-integration.md` for gates and work order.

Run the independent native prototype with:

```sh
cargo run --manifest-path components/cditor-whiteboard-drafft/Cargo.toml \
  --example gpui_board
```

## License

This crate is `AGPL-3.0` because it is the explicit boundary for directly linked
Drafft Ink code. It must not be enabled in a release build without completing
the product's license review and source-distribution obligations.
