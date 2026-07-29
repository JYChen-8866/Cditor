# Drafft Ink / GPUI whiteboard integration plan

Status: active technical validation  
Owner: `components/cditor-whiteboard-drafft`  
Last updated: 2026-07-25

## Decision

Reuse Drafft Ink's Rust core without WebView and rebuild its small application
shell and renderer directly in GPUI. Do not replace the current production
whiteboard until the independent GPUI crate reaches measured feature parity.

The integration lives in a new sibling crate,
`components/cditor-whiteboard-drafft`. The existing
`components/cditor-whiteboard` crate is not modified by the spike. CDitor core,
runtime, session, storage, and editor protocol continue treating
`WhiteboardPayload.scene_json` as opaque versioned data. The existing GPUI
renderer remains the default and production fallback throughout the migration.

This is an independent pure-GPUI application-shell and renderer port. Drafft
Ink's winit/egui shell, browser APIs, WebAssembly shell and Vello presentation
path are out of the runtime scope.

Implementation rule: use the pinned upstream crate APIs directly wherever they
exist. If a source-level adaptation is unavoidable, start from the corresponding
upstream module verbatim, retain attribution, and limit changes to the GPUI
boundary. Do not independently rewrite an upstream model, renderer, geometry,
camera, snapping, tool, or event algorithm.

## Why GPUI-native painting was chosen

The current board paints directly into GPUI with `canvas`, `PathBuilder`, and
`Window::paint_path`. On macOS the pinned GPUI revision uses its own Metal
renderer. Drafft Ink's renderer builds Vello scenes and targets wgpu 27.

The pinned GPUI public API does not accept an arbitrary Metal or wgpu texture:

- `SurfaceSource` accepts only `CVPixelBuffer` on macOS.
- `RenderImage` owns CPU-side BGRA image frames.
- custom `Element::paint` can enqueue GPUI primitives, but it cannot submit an
  external Vello scene or texture through a public API.

An IOSurface bridge would require RGBA-to-bi-planar-YUV conversion and explicit
synchronization across independent Metal queues. GPUI already offers native
path tessellation, clipping and z-order, so the chosen path translates Drafft's
kurbo geometry into GPUI `PathBuilder` commands. It has no frame copy, texture
bridge, WebView, or second writable scene model.

## Non-goals

- No WebView, DOM, React, browser event loop, or WASM embedding.
- No immediate replacement of `Scene`, `Element`, `Camera`, or persisted JSON.
- No dependency from `cditor-whiteboard` back into CDitor crates.
- No rewrite of document thumbnail caching or whiteboard block virtualization.
- No simultaneous port of Drafft Ink's egui toolbar and GPUI interaction UI.
- No dual writable scene models in production.
- No frame-by-frame PNG encoding or CPU bitmap upload as a final renderer.

## License boundary

Drafft Ink declares `AGPL-3.0`; `cditor-whiteboard` currently declares
`GPL-3.0-or-later`. Directly linking or adapting Drafft Ink code introduces AGPL
obligations for the resulting covered work. This is a release and distribution
decision, not merely a Cargo detail.

Rules:

1. Keep the complete audited Drafft repository vendored in the separate
   `cditor-whiteboard-drafft` crate and behind explicit features until the
   product's license decision is recorded.
2. Keep Drafft adapters inside the isolated crate and keep source-derived
   algorithms in narrowly named paint modules with attribution so provenance
   and modifications remain auditable.
3. Record the exact upstream revision, retain its license notices and assets,
   and keep the vendor tree free of local modifications.
4. Do not copy source fragments into generic GPUI modules.
5. Do not enable the feature in default desktop builds before license review.

## Invariants

The following must remain true at every migration stage:

```text
CDitor document truth        = WhiteboardPayload.scene_json
Document/runtime knowledge   = opaque JSON + content version only
Board edit truth             = exactly one authoritative scene model
Document preview lifecycle   = virtual-window-scoped read-only entity
Full editor lifecycle        = dedicated overlay/session entity
Document block size          = stable before board resources are ready
Document wheel ownership     = host document in read-only embeds
```

Existing JSON must open without migration. New fields use serde defaults. If a
future Drafft-native format is adopted, the persisted envelope must be versioned
and old scenes must have a deterministic one-way converter plus golden fixtures.

## Target module boundary

```text
components/
  cditor-whiteboard/           # existing component; untouched by the spike
  cditor-whiteboard-drafft/
    UPSTREAM.md                # immutable revision and update procedure
    vendor/drafft-ink/         # verbatim upstream repository (without .git)
    src/
      lib.rs                   # public boundary and upstream re-exports
      model_host.rs            # GPUI events -> upstream Canvas/ToolManager
      paint/
        plan.rs                # culled retained GPUI paint commands
        path.rs                # kurbo -> GPUI PathBuilder conversion
        rough.rs               # attributed Drafft rough-stroke geometry
        pattern.rs             # roughr patterns + path-accurate clipping
        selection.rs           # upstream handles -> GPUI paint commands
      view/
        mod.rs                 # GPUI lifecycle and input
        toolbar.rs             # native GPUI controls using upstream SVGs
    examples/
      gpui_board.rs            # independent native GPUI prototype
```

The new crate directly uses upstream `Canvas`, `Camera`, shape, snapping, tool,
selection and document APIs. It must not recreate equivalent types or
algorithms. Only GPUI lifecycle, input translation, paint-command emission and
native controls are local because Drafft Ink does not provide them.

The new crate may not depend on `cditor-whiteboard` during the presentation
spike. A future desktop composition layer chooses between the two independent
components; it must not introduce a second writable model inside either one.

## Phase 0: architecture and dependency audit

Status: complete.

Findings:

- The current board is already a native, host-agnostic GPUI product component.
- It supports persisted scenes, thumbnails, editing, input, text, diagrams,
  templates, images, embeds, undo/redo, culling, and layout caching.
- Replacing the model first would regress existing stored boards and CDitor's
  thumbnail/runtime contracts.
- Drafft Ink separates `drafftink-core` and `drafftink-render`, but its renderer
  still expects Drafft's canvas and shape model.
- Drafft Ink currently requires Rust 1.86, edition 2024, Vello 0.6 at a pinned
  upstream revision, wgpu 27, and AGPL-3.0.

Exit criterion: complete for the isolated AGPL crate. The crate is not connected
to release builds.

## Phase 1: native GPUI renderer and interaction slice

Status: in progress; base window, paths and editing loop are running.

Implement a standalone example under `cditor-whiteboard-drafft`. It must not
depend on or modify `WhiteboardView`, persistence, or editor integration.

The spike renders:

- background and dot grid;
- one filled/stroked rectangle;
- one freehand path with at least 1,000 points;
- pan and zoom camera transform;
- resize and Retina scale-factor changes.

Evaluate the presentation paths in this order:

### A. Shared native GPU texture

Determine whether the pinned GPUI Metal renderer and wgpu/Vello can share an
`MTLTexture` with explicit synchronization and lifetime ownership. This requires
a narrow upstream GPUI extension because no suitable public API exists today.

Accept only if:

- no GPU-to-CPU readback occurs per frame;
- texture ownership and frame fences are explicit;
- resize cannot display a freed or stale texture;
- clipping, opacity, z-order, and Retina scaling work inside a GPUI element;
- the extension is small enough to maintain or upstream.

### B. CoreVideo / IOSurface presentation

Test Vello rendering into an IOSurface-backed target that can be wrapped as a
`CVPixelBuffer` and presented by GPUI's existing `surface` element.

The pinned GPUI renderer requires a full-range bi-planar YCbCr buffer. The spike
therefore uses an RGBA Vello target followed by a GPU-only RGBA-to-two-plane
conversion pass. It must import the IOSurface plane textures through wgpu 27's
Metal HAL and own synchronization before handing the buffer to GPUI.

Accept only if it remains GPU resident, has deterministic pixel format/color
space, and does not require copying the full frame on every update.

### C. Drafft algorithms with GPUI rendering

If A and B fail, do not ship an image-upload renderer. Reuse selected
`drafftink-core` algorithms through adapters while continuing to emit GPUI paths.
Candidate reuse includes camera math, shape geometry, hit testing, snapping, and
CRDT operations. Each adopted algorithm needs parity fixtures against current
scene behavior.

Phase 1 exit artifact:

```text
doc/acceptance/drafft-gpui-render-spike.md
```

It records the chosen path, rejected paths with evidence, dependency graph,
frame timings, memory, resize/HiDPI results, and any required GPUI patch.

## Phase 2: isolated backend integration

Only after Phase 1 passes:

1. Stabilize the public host/presentation contract in the new crate.
2. Keep `drafft-core` and `drafft-vello` features disabled by default.
3. Add `DrafftVelloBackend` only for capabilities proven by the spike.
4. Build a separate Drafft-backed GPUI view; do not branch inside the existing
   whiteboard's shape painters.
5. Let the desktop composition root choose the existing component or the new
   component. A failed new-component initialization falls back before an editor
   session is created.
6. Keep the existing whiteboard as the production implementation until the new
   crate reaches full parity.

Fallback must be visible in telemetry and must not rewrite scene JSON.

## Phase 3: model adapter and visual parity

Build a read-only `Scene -> Drafft render model` adapter. Do not maintain a
writable mirror.

Parity corpus:

- every current shape and connector type;
- freehand paths at different zoom levels;
- rotations and non-uniform sizes;
- fills, strokes, transparency, and theme-following colors;
- text, labels, style spans, CJK, emoji, and font fallback;
- mind-map connector styles and flowchart layouts;
- images and embeds through the GPUI overlay path;
- empty, malformed, and previous-version scene JSON.

Every mismatch is either fixed or represented by a declared backend capability
that routes that layer to GPUI. Silent omission is not allowed.

## Phase 4: interaction and algorithm adoption

Adopt Drafft core capabilities one subsystem at a time only when they improve
the current implementation:

1. camera and coordinate transforms;
2. spatial index and hit testing;
3. snapping and smart guides;
4. path smoothing and erasing;
5. shape geometry;
6. optional Loro collaboration.

For each subsystem:

- define the current behavior contract with fixtures;
- add an adapter at the board boundary;
- switch one authoritative implementation;
- remove the replaced implementation in the same milestone;
- retain JSON compatibility and undo semantics.

Collaboration is a separate architecture decision. It must not be introduced as
a side effect of renderer migration.

## Phase 5: format decision

After renderer and interaction parity, choose one of:

1. keep the current `Scene` as the product format and use Drafft internally;
2. introduce a versioned native Drafft payload with a one-way converter;
3. decline the model migration and retain only selected algorithms/rendering.

The decision requires migration fixtures, downgrade behavior, storage-size
measurements, and a recovery story for malformed or partially migrated scenes.

## Performance gates

Measure p50, p95, p99 and worst frame; averages alone are insufficient.

```text
Interactive pan/zoom main-thread work: p95 < 8 ms, p99 < 16 ms
Pointer-to-visible-frame latency:      p95 < 16 ms
Dropped-frame ratio over 10 seconds:   < 1%
Idle board GPU work:                   no continuous redraw
Resize/Retina transition:              no blank or stale frame
Document thumbnail mount:              no synchronous device creation
Visible element traversal:             viewport-culling proportional
GPU-to-CPU full-frame readback:         0 in production
```

Test fixtures must include 100, 1,000, 10,000, and 50,000 elements, a dense
freehand scene, image-heavy scenes, and a document with multiple visible
whiteboard blocks. Document preview must continue obeying the outer virtual
window and stable-box rules.

## Verification matrix

Automated coverage to add with each implementation phase:

- adapter unit tests and property tests for coordinate transforms;
- golden JSON round trips for existing scenes;
- backend capability and fallback tests;
- deterministic render-scene snapshots where possible;
- resize, scale-factor, and zero-size viewport tests;
- read-only wheel ownership and editor focus tests;
- benchmark fixtures for culling and frame construction.

Manual acceptance:

- document preview scrolls without wheel capture;
- double-click opens the full editor with identical camera/content;
- editing persists through the existing `scene_json` path;
- backend failure reopens the same board using GPUI paths;
- light/dark themes, Retina/non-Retina, resize, sleep/wake, and multiple windows.

## Immediate work order

- [x] Audit current CDitor and whiteboard ownership boundaries.
- [x] Audit the pinned GPUI presentation APIs.
- [x] Audit Drafft Ink's crate split, renderer contract, toolchain, and license.
- [x] Create the independent `cditor-whiteboard-drafft` crate boundary.
- [x] Vendor the complete audited Drafft Ink revision into the new crate.
- [x] Build and run the upstream Canvas -> Vello scene path with a rectangle,
  1,000-point freehand path, camera transform, and 2x scale factor.
- [x] Audit shared texture and CoreVideo/IOSurface paths and record the missing
  APIs and synchronization cost.
- [x] Choose direct Drafft core reuse with GPUI-native primitives.
- [x] Build and run a no-CDitor native GPUI window in the independent crate.
- [x] Implement Canvas host, culled paint plan, path conversion, base tools,
  selection drag, pan/zoom, undo/redo and native SVG toolbar.
- [x] Port patterned fill, path-accurate pattern clipping, deterministic rough
  double strokes and visual selection handles from the pinned upstream model.
- [x] Port text, image and math painting from the pinned upstream renderer.
- [x] Port handle hit testing, endpoint/intermediate-point editing, corner
  resize, rotation, Shift constraints, marquee selection and hover cursors.
- [x] Port smart guides, snapping feedback and remaining tools.
- [x] Complete GPUI property controls, IME editing, clipboard, keyboard
  shortcuts, file tabs, native persistence and PNG export.
- [x] Add image clipboard import plus retained rotation/opacity raster caching.
- [x] Add JSON/Excalidraw open, Excalidraw library tabs, Mermaid import, and
  document/selection PNG export at 1x/2x/3x.
- [ ] Complete accessibility metadata and keyboard-only focus traversal.
- [ ] Integrate behind disabled-by-default Cargo features.
- [ ] Complete parity, failure, performance, and document-host acceptance.

## Stop conditions

Stop and return for an explicit product decision if:

- AGPL cannot be accepted for shipped binaries or network use;
- text/image/overlay parity requires two conflicting writable scene models;
- the new backend misses the performance gates or destabilizes document scroll.
