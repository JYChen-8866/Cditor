pub mod close_guard;
mod payload_loader;
pub mod save_indicator;
pub mod storage_saver;

pub use cditor_session::{
    DEFAULT_STORAGE_SAVE_DEBOUNCE, PersistenceBarrierKind, PersistencePipeline,
    PersistencePipelineError, save_storage_batch,
};
pub(crate) use payload_loader::{
    PayloadWindowLoadSchedule, PayloadWindowLoadScheduler, STORAGE_VIEWPORT_LOAD_TIMEOUT,
};
pub use save_indicator::{
    EditorLoadStateLabel, EditorSaveStatus, render_load_state, render_readonly_notice,
    render_save_indicator,
};
pub use storage_saver::{mark_dirty_and_schedule_save, schedule_storage_autosave};
