//! Vendored Markdown parsing models from velotype
//! (`https://github.com/manyougz/velotype`, `src/components/markdown/`).
//!
//! These files are copied nearly verbatim so Cditor's Markdown import parses
//! inline syntax, HTML styling, links, footnotes and tables the same way
//! velotype does. The gpui/uuid/http-client dependencies were replaced with
//! std/local stand-ins, and the render-only halves (image loading, code
//! highlighting, table runtime layout) were omitted.

pub(crate) mod footnote;
pub(crate) mod html;
pub(crate) mod inline;
pub(crate) mod link;
pub(crate) mod paste;
pub(crate) mod table;
