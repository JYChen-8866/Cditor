use std::fmt;

use gpui::{App, Task, WeakEntity};

use crate::gui::CditorV2View;

use super::{
    command::{CditorCommand, CommandOutcome, CommandState},
    diagnostics::CditorDiagnostics,
    document::{
        CloseGuard, DocumentInfo, DocumentSelection, SaveReport, SaveStatus, ScrollAlignment,
    },
    error::CditorError,
};

/// Non-retaining control surface for a Cditor component.
#[derive(Clone)]
pub struct CditorHandle {
    entity: WeakEntity<CditorV2View>,
}

impl fmt::Debug for CditorHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CditorHandle")
            .field("entity", &self.entity)
            .finish_non_exhaustive()
    }
}

impl CditorHandle {
    pub(crate) fn new(entity: WeakEntity<CditorV2View>) -> Self {
        Self { entity }
    }

    pub fn focus(&self, cx: &mut App) -> Result<(), CditorError> {
        self.require_component()?;
        self.entity
            .update_in(cx, |view, window, cx| view.sdk_focus(window, cx))
            .map_err(|error| CditorError::Internal(error.to_string()))
    }

    pub fn blur(&self, cx: &mut App) -> Result<(), CditorError> {
        self.require_component()?;
        self.entity
            .update_in(cx, |view, window, cx| view.sdk_blur(window, cx))
            .map_err(|error| CditorError::Internal(error.to_string()))
    }

    pub fn is_ready(&self, cx: &App) -> bool {
        self.entity
            .read_with(cx, |view, _| view.sdk_is_ready())
            .unwrap_or(false)
    }

    pub fn is_readonly(&self, cx: &App) -> bool {
        self.entity
            .read_with(cx, |view, _| view.sdk_is_readonly())
            .unwrap_or(false)
    }

    pub fn set_readonly(&self, readonly: bool, cx: &mut App) -> Result<(), CditorError> {
        self.entity
            .update(cx, |view, cx| view.sdk_set_readonly(readonly, cx))
            .map_err(|_| CditorError::ComponentDropped)
    }

    pub fn undo(&self, cx: &mut App) -> Result<(), CditorError> {
        self.entity
            .update(cx, |view, cx| view.sdk_undo(cx))
            .map_err(|_| CditorError::ComponentDropped)??;
        Ok(())
    }

    pub fn redo(&self, cx: &mut App) -> Result<(), CditorError> {
        self.entity
            .update(cx, |view, cx| view.sdk_redo(cx))
            .map_err(|_| CditorError::ComponentDropped)??;
        Ok(())
    }

    pub fn can_undo(&self, cx: &App) -> bool {
        self.entity
            .read_with(cx, |view, _| view.sdk_can_undo())
            .unwrap_or(false)
    }

    pub fn can_redo(&self, cx: &App) -> bool {
        self.entity
            .read_with(cx, |view, _| view.sdk_can_redo())
            .unwrap_or(false)
    }

    pub fn document_info(&self, cx: &App) -> Option<DocumentInfo> {
        self.entity
            .read_with(cx, |view, _| view.sdk_document_info())
            .ok()
            .flatten()
    }

    pub fn is_dirty(&self, cx: &App) -> bool {
        self.entity
            .read_with(cx, |view, _| view.sdk_is_dirty())
            .unwrap_or(false)
    }

    pub fn save_status(&self, cx: &App) -> SaveStatus {
        self.entity
            .read_with(cx, |view, _| view.sdk_save_status())
            .unwrap_or_else(|_| SaveStatus::Failed(CditorError::ComponentDropped.to_string()))
    }

    pub fn close_guard(&self, cx: &App) -> CloseGuard {
        self.entity
            .read_with(cx, |view, _| view.sdk_close_guard())
            .unwrap_or(CloseGuard {
                dirty: false,
                saving: false,
                failed_operations: 0,
                can_close_safely: true,
            })
    }

    pub fn save(&self, cx: &mut App) -> Task<Result<SaveReport, CditorError>> {
        self.entity
            .update(cx, |view, cx| view.sdk_save(cx))
            .unwrap_or_else(|_| Task::ready(Err(CditorError::ComponentDropped)))
    }

    pub fn flush(&self, cx: &mut App) -> Task<Result<SaveReport, CditorError>> {
        self.entity
            .update(cx, |view, cx| view.sdk_flush(cx))
            .unwrap_or_else(|_| Task::ready(Err(CditorError::ComponentDropped)))
    }

    pub fn diagnostics(&self, cx: &App) -> Result<CditorDiagnostics, CditorError> {
        self.entity
            .read_with(cx, |view, _| view.sdk_diagnostics())
            .map_err(|_| CditorError::ComponentDropped)?
    }

    pub fn selection(&self, cx: &App) -> Option<DocumentSelection> {
        self.entity
            .read_with(cx, |view, _| view.sdk_selection())
            .ok()
            .flatten()
    }

    pub fn set_selection(
        &self,
        selection: DocumentSelection,
        cx: &mut App,
    ) -> Result<(), CditorError> {
        self.entity
            .update(cx, |view, cx| view.sdk_set_selection(selection, cx))
            .map_err(|_| CditorError::ComponentDropped)??;
        Ok(())
    }

    pub fn selected_text(&self, cx: &App) -> Option<String> {
        self.entity
            .read_with(cx, |view, _| view.sdk_selected_text())
            .ok()
            .flatten()
    }

    pub fn scroll_to_block(
        &self,
        block_id: cditor_core::ids::BlockId,
        alignment: ScrollAlignment,
        cx: &mut App,
    ) -> Result<(), CditorError> {
        self.entity
            .update(cx, |view, cx| {
                view.sdk_scroll_to_block(block_id, alignment, cx)
            })
            .map_err(|_| CditorError::ComponentDropped)??;
        Ok(())
    }

    pub fn execute(
        &self,
        command: CditorCommand,
        cx: &mut App,
    ) -> Result<CommandOutcome, CditorError> {
        self.entity
            .update(cx, |view, cx| view.sdk_execute_command(command, cx))
            .map_err(|_| CditorError::ComponentDropped)?
    }

    pub fn command_state(&self, command: &CditorCommand, cx: &App) -> CommandState {
        self.entity
            .read_with(cx, |view, _| view.sdk_command_state(command))
            .unwrap_or(CommandState::DISABLED)
    }

    fn require_component(&self) -> Result<(), CditorError> {
        self.entity
            .upgrade()
            .map(|_| ())
            .ok_or(CditorError::ComponentDropped)
    }
}
