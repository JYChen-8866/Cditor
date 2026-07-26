use gpui::{Entity, FocusHandle};

use cditor_runtime::TableCellPosition;

use super::{RichTextElement, RichTextInputHandler};
use crate::editor_view::CditorV2View;

impl RichTextElement {
    pub fn with_input_handler(
        mut self,
        view: Entity<CditorV2View>,
        focus: FocusHandle,
        focused: bool,
    ) -> Self {
        self.input_handler = Some(RichTextInputHandler {
            view,
            focus,
            focused,
            table_cell_position: None,
        });
        self
    }

    pub fn with_table_cell_input_handler(
        mut self,
        view: Entity<CditorV2View>,
        focus: FocusHandle,
        focused: bool,
        table_cell_position: TableCellPosition,
    ) -> Self {
        self.input_handler = Some(RichTextInputHandler {
            view,
            focus,
            focused,
            table_cell_position: Some(table_cell_position),
        });
        self
    }
}
