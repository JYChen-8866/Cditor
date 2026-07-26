pub mod close_guard;
pub mod save_indicator;
pub mod storage_saver;

pub use cditor_session::{
    DEFAULT_STORAGE_SAVE_DEBOUNCE, PersistenceBarrierKind, PersistencePipeline,
    PersistencePipelineError,
};
pub use save_indicator::{
    EditorLoadStateLabel, EditorSaveStatus, render_load_state, render_readonly_notice,
    render_save_failure_notice,
};
pub use storage_saver::schedule_storage_autosave;
