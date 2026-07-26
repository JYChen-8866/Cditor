# Drafft Ink / GPUI render spike

Date: 2026-07-25  
Status: in progress  
Crate: `components/cditor-whiteboard-drafft`

## Completed evidence

- Drafft Ink revision `8ce40ab2cf3cde7efa78a7e077fc9267fd4b3761`
  is vendored verbatim under the independent crate. A recursive diff against
  the audited checkout reported no differences (excluding `.git`).
- The independent crate is excluded from CDitor's main Cargo workspace. Its
  AGPL code, lockfile, dependency graph, and build cache do not enter the
  default desktop build.
- Default `drafft-core` build passes from local path dependencies.
- `drafft-vello` build passes with upstream's pinned Vello revision
  `1771112ebc0ae04d996ebf493d3a00d73303f014` and wgpu 27.
- `examples/build_vello_scene.rs` runs successfully. It uses upstream
  `Canvas`, `Rectangle`, `Freehand`, `RenderContext`, and `VelloRenderer` to
  build a non-empty scene containing:
  - one rectangle;
  - one 1,000-point freehand path;
  - a pan and zoom camera transform;
  - a 2x scale factor.

Commands used (tests were not run):

```sh
cargo check --manifest-path components/cditor-whiteboard-drafft/Cargo.toml
cargo check --manifest-path components/cditor-whiteboard-drafft/Cargo.toml --features drafft-vello
cargo run --manifest-path components/cditor-whiteboard-drafft/Cargo.toml \
  --features drafft-vello --example build_vello_scene
```

## Dependency findings

`drafftink-core` is not a minimal model crate. Its current manifest also pulls
in winit input, Loro, native TLS/WebSocket collaboration, storage, and related
platform dependencies. This is acceptable inside the isolated spike but must be
measured before desktop integration.

The upstream Vello patch raises the effective renderer toolchain floor to Rust
1.88 even though Drafft Ink's workspace currently declares Rust 1.86. The
independent integration crate therefore declares Rust 1.88; the local checked
toolchain is Rust 1.95.

The vendored upstream core produces three unused-import warnings. They are left
unchanged to preserve a clean vendor tree.

## Presentation gate

Native GPUI presentation is now proven for the first interaction slice.

The accepted path is direct Drafft core reuse with GPUI-native primitives. The
standalone `examples/gpui_board.rs` window compiles and runs without the
`drafft-vello` feature. It currently proves:

- upstream `Canvas`, `Camera`, `ToolManager`, document order, selection and undo;
- GPUI mouse-to-world input translation;
- viewport-culling and retained kurbo paint commands;
- GPUI `PathBuilder` rendering for rectangle, ellipse, line, arrow and freehand;
- upstream pressure-path behavior ported from the pinned Vello renderer;
- deterministic Drafft-compatible hand-drawn double strokes;
- every upstream roughr fill pattern, geometrically clipped to the actual
  closed path rather than a rectangular content mask;
- upstream shape-specific selection handles rendered at fixed screen size;
- upstream handle hit testing, corner resize, endpoint/intermediate-point
  editing, rotation, Shift constraints and undo transaction semantics;
- path-intersection-aware marquee selection and handle-specific hover cursors;
- native SVG toolbar using the vendored upstream assets;
- pan, zoom, preview, commit, selection drag, delete, undo/redo and shortcuts.

Command used (tests were not run):

```sh
cargo check --manifest-path components/cditor-whiteboard-drafft/Cargo.toml \
  --example gpui_board
cargo run --manifest-path components/cditor-whiteboard-drafft/Cargo.toml \
  --example gpui_board
```

Text, images, math, resize/rotate interaction, guides and the full property UI
remain before Phase 1 can be accepted as feature-complete. Unit coverage for
rough-path determinism, stroke multiplicity, pattern clipping and selection
handle sizing has been added but was not executed per the current instruction.

## Rejected texture bridge

The pinned GPUI revision exposes:

- `SurfaceSource::Surface(CVPixelBuffer)` on macOS;
- CPU-backed `RenderImage` frames;
- GPUI-native primitive insertion through `Window::paint_*`.

It does not expose a public arbitrary Metal/wgpu texture import API. The audit
considered:

1. synchronized shared `MTLTexture` presentation;
2. GPU-resident IOSurface/CVPixelBuffer presentation;
3. direct Drafft algorithm reuse with GPUI-native primitives, which was chosen.

Per-frame texture readback, PNG encoding, or full-frame CPU upload remains a
rejected production path.

### macOS surface audit result

The current GPUI macOS renderer does not accept a generic RGBA
`CVPixelBuffer`. Its `draw_surfaces` path asserts
`kCVPixelFormatType_420YpCbCr8BiPlanarFullRange`, creates separate Metal
textures for plane 0 (Y, `R8Unorm`) and plane 1 (CbCr, `RG8Unorm`), then samples
both in the surface shader.

Vello renders to a regular RGBA storage texture. It cannot render its scene
directly into GPUI's two-plane YUV buffer. Therefore the production candidate
for path 2 is now precisely defined:

```text
Vello RGBA texture
  -> GPU RGBA-to-NV12/full-range conversion pass
  -> IOSurface-backed two-plane CVPixelBuffer
  -> existing GPUI surface element
```

wgpu 27 exposes the required low-level hooks:

- `wgpu::Device::create_texture_from_hal`;
- `wgpu_hal::metal::Device::texture_from_raw`;
- raw Metal texture access through the Metal HAL.

This makes a GPU-resident bridge technically plausible, but it adds YUV color
conversion, two Metal wrapper versions, cross-queue synchronization and surface
lifetime ownership without improving the product model. It is rejected for the
GPUI port. Path 1 also remains blocked by GPUI's lack of arbitrary texture
import.
