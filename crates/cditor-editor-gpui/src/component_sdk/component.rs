use super::{CditorHandle, CditorViewContract};
use gpui::Entity;

pub struct CditorComponent<V: CditorViewContract> {
    pub view: Entity<V>,
    pub handle: CditorHandle<V>,
}

impl<V: CditorViewContract> CditorComponent<V> {
    pub fn from_view(view: Entity<V>) -> Self {
        let handle = CditorHandle::new(view.downgrade());
        Self { view, handle }
    }
}

impl<V: CditorViewContract> Clone for CditorComponent<V> {
    fn clone(&self) -> Self {
        Self {
            view: self.view.clone(),
            handle: self.handle.clone(),
        }
    }
}
