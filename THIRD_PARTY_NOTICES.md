# Third-party notices

## Zed Mermaid renderer

CDitor uses the `mermaid_render` crate from the Zed repository, pinned to commit
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
