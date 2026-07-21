use gpui::Entity;

#[derive(Clone)]
pub struct CditorComponent {
    pub view: Entity<()>,
    pub handle: crate::CditorHandle,
}

impl CditorComponent {
    pub fn from_erased_view(view: Entity<()>) -> Self {
        let handle = crate::CditorHandle::new(view.downgrade());
        Self { view, handle }
    }
}
