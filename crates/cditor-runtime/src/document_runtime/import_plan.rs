use cditor_core::import_plan::{
    ClipboardImportContent, ImportContent, ImportPlan, ImportReport, ImportTarget,
};

use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportApplicationReport {
    pub changed: bool,
    pub revision: u64,
    pub plan_report: ImportReport,
}

impl DocumentRuntime {
    pub fn import_target(&self) -> (ImportTarget, BlockId) {
        (
            ImportTarget {
                document_id: self.document_id,
                expected_revision: self.revision(),
                focused_block_id: self.focused_block_id(),
            },
            self.next_available_block_id(),
        )
    }

    pub fn apply_import_plan(
        &mut self,
        plan: &ImportPlan,
    ) -> Result<ImportApplicationReport, String> {
        let target = plan.target();
        if plan.report().rejected() {
            return Err("import plan was rejected during planning".to_owned());
        }
        if target.document_id != self.document_id {
            return Err("import plan targets a different document".to_owned());
        }
        if target.expected_revision != self.revision() {
            return Err("import plan target revision is stale".to_owned());
        }
        if target.focused_block_id != self.focused_block_id() {
            return Err("import plan focus target is stale".to_owned());
        }
        let changed = match plan.content() {
            ImportContent::Clipboard(content) => self.apply_planned_clipboard(content)?,
            ImportContent::Markdown(document) => self
                .insert_imported_markdown_content_transaction(
                    document.clone(),
                    EditTransactionKind::Paste,
                    cditor_core::edit::ChangeOrigin::Import,
                )?,
            ImportContent::PlainText(text) => self.replace_text_from_paste(None, text)?,
        };
        Ok(ImportApplicationReport {
            changed,
            revision: self.revision(),
            plan_report: plan.report().clone(),
        })
    }

    fn apply_planned_clipboard(
        &mut self,
        content: &ClipboardImportContent,
    ) -> Result<bool, String> {
        // Valid Cditor metadata is authoritative. In particular, whole-document
        // and block copies must not be reinterpreted from their plain-text
        // clipboard representation as external Markdown.
        if let Some(selection) = content.internal_selection.as_ref() {
            if self.paste_clipboard_selection(selection)? {
                return Ok(true);
            }
            return self.replace_text_from_paste(None, &content.plain_text);
        }
        if let Some(table) = &content.delimited_table {
            let snapshot = TableClipboardSnapshot {
                range: TableRange::normalized(
                    0,
                    0,
                    table.row_count().saturating_sub(1),
                    table.column_count().saturating_sub(1),
                ),
                table: table.clone(),
                plain_text: table.plain_text(),
                markdown: String::new(),
            };
            if self.paste_table_clipboard_at_focused_cell(&snapshot)? {
                return Ok(true);
            }
        }
        if self.clipboard_paste_target_is_literal() {
            // SiYuan semantics: only code contexts keep external text literal
            // (raw insertion into the code block). Everywhere else the text is
            // parsed as Markdown so pasted headings/lists/quotes become blocks.
            return self.replace_text_from_paste(None, &content.plain_text);
        }
        if let Some(markdown) = &content.markdown
            && self.insert_imported_markdown_content_transaction(
                markdown.clone(),
                EditTransactionKind::Paste,
                cditor_core::edit::ChangeOrigin::Import,
            )?
        {
            return Ok(true);
        }
        self.replace_text_from_paste(None, &content.plain_text)
    }

    /// Mirrors SiYuan's paste rule: a paste target is literal only when it is a
    /// code block (or an editing surface that must not be restructured, such as
    /// a table cell, image caption or collection title).
    fn clipboard_paste_target_is_literal(&self) -> bool {
        let Some(target) = self.input_session_target() else {
            return false;
        };
        match target {
            InputTarget::BlockText { block_id } => match self.block_payload_record(block_id) {
                Some(record) => matches!(record.kind, RichBlockKind::Code { .. }),
                None => true,
            },
            InputTarget::TableCell { .. }
            | InputTarget::ImageCaption { .. }
            | InputTarget::CollectionTitle { .. }
            | InputTarget::ComplexBlock { .. }
            | InputTarget::BlockChrome { .. } => true,
        }
    }
}
