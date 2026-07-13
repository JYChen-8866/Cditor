use cditor_core::ids::BlockId;
use cditor_runtime::{DocumentRuntime, InputTarget};

use crate::gui::app::cditor_v2_view::CditorV2View;
use crate::gui::app::input_trace::trace_input;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GuiPlatformInputTarget {
    BlockText {
        block_id: BlockId,
    },
    TableCell {
        block_id: BlockId,
        row: usize,
        col: usize,
    },
    CodeLanguage {
        block_id: BlockId,
    },
    AiPrompt {
        block_id: BlockId,
    },
    TableMenuQuery {
        block_id: BlockId,
    },
    /// Complex block or block chrome focus (no platform text input)
    None,
}

impl GuiPlatformInputTarget {
    pub(crate) fn from_runtime_target(target: InputTarget) -> Self {
        match target {
            InputTarget::BlockText { block_id } => Self::BlockText { block_id },
            InputTarget::TableCell { block_id, row, col } => Self::TableCell { block_id, row, col },
            // Complex blocks and block chrome don't need platform text input
            InputTarget::ComplexBlock { .. } | InputTarget::BlockChrome { .. } => Self::None,
        }
    }

    pub(crate) fn code_language(block_id: BlockId) -> Self {
        Self::CodeLanguage { block_id }
    }

    pub(crate) fn ai_prompt(block_id: BlockId) -> Self {
        Self::AiPrompt { block_id }
    }

    pub(crate) fn table_menu_query(block_id: BlockId) -> Self {
        Self::TableMenuQuery { block_id }
    }

    pub(crate) fn block_id(self) -> BlockId {
        match self {
            Self::BlockText { block_id }
            | Self::TableCell { block_id, .. }
            | Self::CodeLanguage { block_id }
            | Self::AiPrompt { block_id }
            | Self::TableMenuQuery { block_id } => block_id,
            Self::None => BlockId::default(),
        }
    }

    pub(crate) fn is_code_language_for(self, block_id: BlockId) -> bool {
        self == Self::CodeLanguage { block_id }
    }

    pub(crate) fn is_ai_prompt_for(self, block_id: BlockId) -> bool {
        self == Self::AiPrompt { block_id }
    }

    pub(crate) fn is_table_menu_query_for(self, block_id: BlockId) -> bool {
        self == Self::TableMenuQuery { block_id }
    }

    pub(crate) fn matches_runtime_target(self, target: InputTarget) -> bool {
        self == Self::from_runtime_target(target)
    }
}

impl CditorV2View {
    pub(in crate::gui::app) fn begin_platform_input_registration_frame(&mut self) {
        self.platform_input_target = self
            .ai_prompt
            .as_ref()
            .map(|prompt| GuiPlatformInputTarget::ai_prompt(prompt.block_id))
            .or_else(|| {
                self.code_language_edit
                    .as_ref()
                    .map(|edit| GuiPlatformInputTarget::code_language(edit.block_id))
            })
            .or_else(|| {
                self.table_interaction_mode
                    .axis_selection()
                    .map(|selection| GuiPlatformInputTarget::table_menu_query(selection.block_id))
            });
    }

    pub(crate) fn register_platform_input_target(
        &mut self,
        target: GuiPlatformInputTarget,
    ) -> bool {
        let Some(runtime) = self.ready_runtime_ref() else {
            return false;
        };
        if !platform_input_registration_allows(self.platform_input_target, target, runtime) {
            trace_input(
                "register_platform_input_target.rejected",
                format_args!(
                    "current={:?} target={:?} runtime={:?}",
                    self.platform_input_target,
                    target,
                    runtime.input_session_target()
                ),
            );
            return false;
        }
        self.platform_input_target = Some(target);
        true
    }
}

pub(crate) fn platform_input_registration_allows(
    current: Option<GuiPlatformInputTarget>,
    target: GuiPlatformInputTarget,
    runtime: &DocumentRuntime,
) -> bool {
    if matches!(
        target,
        GuiPlatformInputTarget::AiPrompt { .. } | GuiPlatformInputTarget::TableMenuQuery { .. }
    ) {
        return current.is_none_or(|current| current == target);
    }
    if current.is_some_and(|current| current != target) {
        return false;
    }
    runtime
        .input_session_target()
        .is_some_and(|runtime_target| target.matches_runtime_target(runtime_target))
}
