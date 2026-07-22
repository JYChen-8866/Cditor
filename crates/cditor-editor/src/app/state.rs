use cditor_runtime::DocumentRuntime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorReadonlyReason {
    NewerDocumentSchema {
        written_major: u64,
        supported_major: u32,
    },
}

impl EditorReadonlyReason {
    pub fn message(&self) -> String {
        match self {
            Self::NewerDocumentSchema {
                written_major,
                supported_major,
            } => format!(
                "只读：文档格式 v{written_major} 高于当前支持的 v{supported_major}，请升级 Cditor 后编辑。"
            ),
        }
    }
}

pub enum CditorViewState {
    Ready(Box<DocumentRuntime>),
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
        *self = Self::Ready(Box::new(runtime));
    }

    pub fn apply_load_failed(&mut self, message: impl Into<String>) {
        *self = Self::LoadFailed {
            message: message.into(),
        };
    }
}
