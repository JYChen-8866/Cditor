use cditor_runtime::DocumentRuntime;

pub enum CditorViewState {
    Ready(DocumentRuntime),
    Loading { message: String },
    LoadFailed { message: String },
}

impl CditorViewState {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready(_))
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    pub fn is_load_failed(&self) -> bool {
        matches!(self, Self::LoadFailed { .. })
    }

    pub fn apply_loaded_runtime(&mut self, runtime: DocumentRuntime) {
        *self = Self::Ready(runtime);
    }

    pub fn apply_load_failed(&mut self, message: impl Into<String>) {
        *self = Self::LoadFailed {
            message: message.into(),
        };
    }
}
