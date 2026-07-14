use gpui::Entity;

use crate::gui::CditorV2View;

use super::handle::CditorHandle;

/// A renderable GPUI entity paired with its stable control handle.
pub struct CditorComponent {
    pub view: Entity<CditorV2View>,
    pub handle: CditorHandle,
}

impl CditorComponent {
    pub(crate) fn from_view(view: Entity<CditorV2View>) -> Self {
        let handle = CditorHandle::new(view.downgrade());
        Self { view, handle }
    }
}
