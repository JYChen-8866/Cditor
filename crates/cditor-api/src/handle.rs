use gpui::{App, WeakEntity};

#[derive(Clone)]
pub struct CditorHandle {
    entity: WeakEntity<()>,
}

impl CditorHandle {
    pub(crate) fn new(entity: WeakEntity<()>) -> Self {
        Self { entity }
    }

    pub fn is_ready(&self, _cx: &App) -> bool {
        self.entity.upgrade().is_some()
    }

    pub fn document_info(&self, _cx: &App) -> Option<crate::document::DocumentInfo> {
        None
    }
}
