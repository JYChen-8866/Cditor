//! Selection and gutter formatting controls.

mod actions;
mod color;
mod selection_delay;
mod toolbar;

pub(in crate::app) use selection_delay::{
    SelectionToolbarDelay, floating_toolbar_passes_selection_delay,
};
pub(in crate::app) use toolbar::{formatting_toolbar_context, formatting_toolbar_state};
