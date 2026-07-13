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

## ding-board whiteboard

Cditor ships the standalone `ding-board` GPUI whiteboard as a bundled workspace
component.

- Component: `crates/ding-board`
- License: GPL-3.0-or-later
- Component documentation: `crates/ding-board/README.md`

The whiteboard bundle includes the following third-party visual assets.

### JetBrains Mono

JetBrains Mono is the whiteboard's built-in default text face.

- Project: <https://github.com/JetBrains/JetBrainsMono>
- Bundled asset: `crates/ding-board/assets/JetBrainsMono-Regular.ttf`
- Copyright: Copyright 2020 The JetBrains Mono Project Authors
- License: SIL Open Font License 1.1
- License text: `crates/ding-board/assets/JetBrainsMono-OFL.txt`

### Lucide icons

The whiteboard toolbar and shape controls include icons from Lucide.

- Project: <https://github.com/lucide-icons/lucide>
- Bundled assets: `crates/ding-board/assets/icons/*.svg`
- License: ISC; portions originating from Feather retain their MIT attribution
- License text: `crates/ding-board/assets/icons/LICENSE`

Source and binary distributions that contain the whiteboard must retain the
corresponding font and icon license files listed above.
