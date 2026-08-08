#[cfg(feature = "mermaid")]
mod actions;
#[cfg(feature = "mermaid")]
mod cache;
#[cfg(not(feature = "mermaid"))]
mod disabled;
#[cfg(feature = "mermaid")]
mod render;
#[cfg(feature = "mermaid")]
mod theme;

#[cfg(feature = "mermaid")]
pub(crate) use actions::{show_focused_source_after_enter, show_source_after_creation};
#[cfg(feature = "mermaid")]
pub(crate) use cache::{MermaidRenderCache, MermaidRenderStatus};
#[cfg(not(feature = "mermaid"))]
pub(crate) use disabled::{MermaidRenderCache, render_mermaid_block};
#[cfg(feature = "mermaid")]
pub(crate) use render::render_mermaid_block;
