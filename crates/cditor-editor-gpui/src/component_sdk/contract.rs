use std::sync::Arc;

use gpui::{App, Context, Task, Window};

use cditor_sdk::command::{CditorCommand, CommandOutcome, CommandState};
use cditor_sdk::diagnostics::CditorDiagnostics;
use cditor_sdk::document::{
    CloseGuard, DocumentInfo, DocumentSelection, HibernationGuard, RecoveryExport, SaveReport,
    SaveStatus, ScrollAlignment, SearchDecoration, TextStatistics,
};
use cditor_sdk::{Cditor, CditorError};
use cditor_session::{AgentEditOutcome, AgentEditRequest, AgentOutline, AgentOutlineRequest};

use super::CditorComponent;

/// The small UI contract required by the public component handle.
///
/// The concrete GPUI view remains in `cditor-editor-gpui`; the framework-free SDK crate only owns
/// this contract so hosts can depend on stable control semantics without
/// depending on a renderer implementation.
pub trait CditorViewContract: Sized + 'static {
    fn sdk_configure_ai(&mut self, provider: Option<Arc<dyn cditor_ai::AiProvider>>, enabled: bool);
    fn sdk_is_ready(&self) -> bool;
    fn sdk_is_readonly(&self) -> bool;
    fn sdk_set_readonly(&mut self, readonly: bool, cx: &mut Context<Self>);
    fn sdk_focus(&mut self, window: &mut Window, cx: &mut Context<Self>);
    fn sdk_blur(&mut self, window: &mut Window, cx: &mut Context<Self>);
    fn sdk_can_undo(&self) -> bool;
    fn sdk_can_redo(&self) -> bool;
    fn sdk_undo(&mut self, cx: &mut Context<Self>) -> Result<bool, CditorError>;
    fn sdk_redo(&mut self, cx: &mut Context<Self>) -> Result<bool, CditorError>;
    fn sdk_document_info(&self) -> Option<DocumentInfo>;
    fn sdk_set_document_name(
        &mut self,
        name: String,
        cx: &mut Context<Self>,
    ) -> Result<bool, CditorError>;
    fn sdk_text_statistics(&self) -> Option<TextStatistics>;
    fn sdk_is_dirty(&self) -> bool;
    fn sdk_save_status(&self) -> SaveStatus;
    fn sdk_close_guard(&self) -> CloseGuard;
    fn sdk_hibernation_guard(&self) -> HibernationGuard;
    fn sdk_prepare_for_shutdown(&mut self, cx: &mut Context<Self>) -> Result<(), CditorError>;
    fn sdk_export_markdown(&self) -> Result<String, CditorError>;
    fn sdk_content_height(&self) -> Result<f64, CditorError>;
    fn sdk_export_recovery(&self) -> Result<RecoveryExport, CditorError>;
    fn sdk_save(&mut self, cx: &mut Context<Self>) -> Task<Result<SaveReport, CditorError>>;
    fn sdk_flush(&mut self, cx: &mut Context<Self>) -> Task<Result<SaveReport, CditorError>>;
    fn sdk_diagnostics(&self) -> Result<CditorDiagnostics, CditorError>;
    fn sdk_selection(&self) -> Option<DocumentSelection>;
    fn sdk_set_selection(
        &mut self,
        selection: DocumentSelection,
        cx: &mut Context<Self>,
    ) -> Result<(), CditorError>;
    fn sdk_selected_text(&self) -> Option<String>;
    fn sdk_scroll_to_block(
        &mut self,
        block_id: cditor_core::ids::BlockId,
        alignment: ScrollAlignment,
        cx: &mut Context<Self>,
    ) -> Result<(), CditorError>;
    fn sdk_set_search_decorations(
        &mut self,
        decorations: Vec<SearchDecoration>,
        cx: &mut Context<Self>,
    );
    fn sdk_highlight_block(&mut self, block_id: cditor_core::ids::BlockId, cx: &mut Context<Self>);
    fn sdk_execute_command(
        &mut self,
        command: CditorCommand,
        cx: &mut Context<Self>,
    ) -> Result<CommandOutcome, CditorError>;
    fn sdk_command_state(&self, command: &CditorCommand) -> CommandState;
    fn sdk_agent_outline(&self, request: AgentOutlineRequest) -> Result<AgentOutline, CditorError>;
    fn sdk_agent_edit(
        &mut self,
        request: AgentEditRequest,
        cx: &mut Context<Self>,
    ) -> Result<AgentEditOutcome, CditorError>;
}

/// Factory boundary implemented by the application composition crate.
pub trait CditorViewFactory {
    type View: CditorViewContract;

    fn build_component(
        &self,
        builder: Cditor,
        cx: &mut App,
    ) -> Result<CditorComponent<Self::View>, CditorError>;
}
