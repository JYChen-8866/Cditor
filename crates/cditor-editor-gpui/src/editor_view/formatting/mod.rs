//! Selection and gutter formatting controls.

mod actions;
mod color;
mod selection_delay;
mod selection_geometry;
mod toolbar;

pub(crate) use selection_delay::{SelectionToolbarDelay, floating_toolbar_passes_selection_delay};
pub(crate) use toolbar::{formatting_toolbar_context, formatting_toolbar_state};
