# Third-party notices

## Zed Mermaid renderer

Cditor uses the `mermaid_render` crate from the Zed repository, pinned to commit
`1d217ee39d381ac101b7cf49d3d22451ac1093fe`.

- Project: <https://github.com/zed-industries/zed>
- Component: `crates/mermaid_render`
- License: GPL-3.0-or-later

The renderer uses the patched `merman` release selected by that Zed commit:

- Project: <https://github.com/zed-industries/merman>
- Version: `v0.6.2-with-patches`
- License: MIT OR Apache-2.0

The exact resolved revisions and transitive dependencies are recorded in
`Cargo.lock`. Binary and source distributions must retain the license material
required by these components.

## cditor-whiteboard whiteboard

Cditor ships the standalone `cditor-whiteboard` GPUI whiteboard as a bundled workspace
component.

- Component: `components/cditor-whiteboard`
- License: GPL-3.0-or-later
- Component documentation: `components/cditor-whiteboard/README.md`

The whiteboard bundle includes the following third-party visual assets.

### JetBrains Mono

JetBrains Mono is the whiteboard's built-in default text face.

- Project: <https://github.com/JetBrains/JetBrainsMono>
- Bundled asset: `components/cditor-whiteboard/assets/JetBrainsMono-Regular.ttf`
- Copyright: Copyright 2020 The JetBrains Mono Project Authors
- License: SIL Open Font License 1.1
- License text: `components/cditor-whiteboard/assets/JetBrainsMono-OFL.txt`

### Lucide icons

The whiteboard toolbar and shape controls include icons from Lucide.

- Project: <https://github.com/lucide-icons/lucide>
- Bundled assets: `components/cditor-whiteboard/assets/icons/*.svg`
- License: ISC; portions originating from Feather retain their MIT attribution
- License text: `components/cditor-whiteboard/assets/icons/LICENSE`

Source and binary distributions that contain the whiteboard must retain the
corresponding font and icon license files listed above.

## Google Fonts COLRv1 test font

Cditor's exact-raster test suite bundles a generated COLRv1 font for
deterministic color-glyph regression coverage.

- Project: <https://github.com/googlefonts/color-fonts>
- Upstream asset: `fonts/test_glyphs-glyf_colr_1.ttf`
- Bundled asset:
  `crates/text/tests/fixtures/text-layout/v1/fonts/COLRv1StaticTestGlyphs.ttf`
- License: Apache-2.0
- Asset notice:
  `crates/text/tests/fixtures/text-layout/v1/fonts/COLRv1StaticTestGlyphs-NOTICE.md`
- License text: `LICENSE-APACHE`
