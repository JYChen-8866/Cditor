# Third-party notices

## Zed Mermaid renderer

Cditor vendors the `mermaid_render` crate from the Zed repository, pinned to
commit `1d217ee39d381ac101b7cf49d3d22451ac1093fe`.

- Project: <https://github.com/zed-industries/zed>
- Component: `crates/mermaid_render`
- Vendored source: `vendor/mermaid_render`
- License: GPL-3.0-or-later

The renderer uses the patched `merman` release selected by that Zed commit:

- Project: <https://github.com/zed-industries/merman>
- Version: `v0.6.2-with-patches`
- License: MIT OR Apache-2.0

The exact resolved revisions and transitive dependencies are recorded in
`Cargo.lock`. Binary and source distributions must retain the license material
required by these components.

## drafft-ink-derived whiteboard

The active GPUI whiteboard contains code derived from `drafft-ink`. The former
standalone experiment has been removed; the maintained integration now lives in
`components/cditor-whiteboard-gpui`.

- Project: <https://github.com/PatWie/drafft-ink>
- Component: `components/cditor-whiteboard-gpui`
- Upstream license: GNU Affero General Public License v3
- License text: `components/cditor-whiteboard-gpui/DRAFFT-INK-LICENSE-AGPL`

## cditor-whiteboard assets

Cditor also contains whiteboard visual assets used by its GPUI components.

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

## programming-languages-logos

The editor bundles programming language logos from
`programming-languages-logos@0.0.3`.

- Project: <https://github.com/abranhe/programming-languages-logos>
- Bundled assets: `assets/icons/*.svg` copied from the package's
  `src/<language>/<language>.svg`
- License: MIT
- License text: `assets/icons/programming-languages-logos-LICENSE`

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
