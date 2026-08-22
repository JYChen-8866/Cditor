use cditor_core::ids::BlockId;
use cditor_core::rich_text::{BlockPayload, InlineMark};
use cditor_editor_protocol::command::{
    CditorCommand, CommandEnvelope, CommandOutcomeStatus, CommandSource,
};

use crate::editor_view::{CditorV2View, GuiPlatformInputTarget};
use crate::input::link_edit::{LinkEditField, LinkEditState};

/// Key handling outcome for the link popup, mirroring
/// `CodeLanguageEditKeyResult`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinkEditKeyResult {
    Commit,
    Cancel,
    Changed,
    Ignored,
}

impl CditorV2View {
    /// Opens the link popup for the captured toolbar selection. Both popup
    /// inputs are registered platform-IME targets: the document composition is
    /// committed first, competing overlays close, and focus transfers through
    /// the render-pass focus request like every other auxiliary input.
    pub(crate) fn open_link_edit_from_toolbar(
        &mut self,
        block_id: BlockId,
        anchor: usize,
        focus: usize,
        x: f32,
        y: f32,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.status.readonly {
            return;
        }
        let range = anchor.min(focus)..anchor.max(focus);
        if range.is_empty() {
            return;
        }
        let Some((selected_text, existing_href)) = self.ready_session().and_then(|session| {
            let context = session.text_block_context(block_id).ok().flatten()?;
            let text = context.text;
            let start = range.start.min(text.len());
            let end = range.end.min(text.len());
            if !text.is_char_boundary(start) || !text.is_char_boundary(end) || start >= end {
                return None;
            }
            let selected = text[start..end].to_owned();
            let existing = session
                .loaded_payload_record(block_id)
                .ok()
                .flatten()
                .and_then(|record| match &record.payload {
                    BlockPayload::RichText { spans } => link_href_in_range(spans, start..end),
                    _ => None,
                });
            Some((selected, existing))
        }) else {
            return;
        };
        if !self.commit_document_composition_before_external_focus(cx) {
            return;
        }
        self.overlay.slash_menu = None;
        self.overlay.code_language_edit = None;
        self.overlay.ai_prompt = None;
        self.overlay.color_menu_open = false;
        self.overlay.link_edit = Some(LinkEditState::new(
            block_id,
            range,
            selected_text,
            existing_href,
            x,
            y,
        ));
        self.input
            .request_focus(GuiPlatformInputTarget::link_url(block_id));
        cx.notify();
    }

    pub(crate) fn focus_link_edit_field(
        &mut self,
        field: LinkEditField,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(edit) = self.overlay.link_edit.as_mut() else {
            return;
        };
        edit.focus_field(field);
        let target = match field {
            LinkEditField::Text => GuiPlatformInputTarget::link_text(edit.block_id),
            LinkEditField::Url => GuiPlatformInputTarget::link_url(edit.block_id),
        };
        self.input.request_focus(target);
        cx.notify();
    }

    /// Commits the popup: re-asserts the captured selection (the popup owns
    /// window focus), then applies href/label through the runtime command.
    pub(crate) fn commit_link_edit(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(edit) = self.overlay.link_edit.take() else {
            return;
        };
        let href = edit.normalized_href();
        let text = edit.label_replacement();
        self.teardown_link_edit_focus(&edit);
        if href.is_none() && text.is_none() {
            // Nothing to apply: an empty URL with an unchanged label is a
            // dismissal, not a "clear link" request (that is the trash icon).
            cx.notify();
            return;
        }
        self.apply_link_to_captured_selection(&edit, href, text, cx);
        cx.notify();
    }

    /// The popup's trash action: removes the link from the captured selection.
    pub(crate) fn clear_link_from_popup(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(edit) = self.overlay.link_edit.take() else {
            return;
        };
        self.teardown_link_edit_focus(&edit);
        self.apply_link_to_captured_selection(&edit, None, None, cx);
        cx.notify();
    }

    /// Copies the URL draft to the clipboard without closing the popup.
    pub(crate) fn copy_link_href_from_popup(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(edit) = self.overlay.link_edit.as_ref() else {
            return;
        };
        let href = edit.href_draft.trim();
        if href.is_empty() {
            return;
        }
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(href.to_owned()));
    }

    pub(crate) fn cancel_link_edit(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(edit) = self.overlay.link_edit.take() else {
            return;
        };
        self.teardown_link_edit_focus(&edit);
        cx.notify();
    }

    fn teardown_link_edit_focus(&mut self, edit: &LinkEditState) {
        self.input
            .clear_focus_request(GuiPlatformInputTarget::link_text(edit.block_id));
        self.input
            .clear_focus_request(GuiPlatformInputTarget::link_url(edit.block_id));
        self.input.request_focus_dismissal();
        if self
            .input
            .target
            .is_some_and(|target| target.is_link_edit_for(edit.block_id))
        {
            self.input.target = None;
        }
    }

    fn apply_link_to_captured_selection(
        &mut self,
        edit: &LinkEditState,
        href: Option<String>,
        text: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let block_id = edit.block_id;
        let range = edit.range.clone();
        let restored = self.ready_session().and_then(|session| {
            session
                .dispatch_with_snapshot(CommandEnvelope::new(
                    CditorCommand::SetDocumentSelection {
                        selection: cditor_core::edit::DocumentSelection {
                            anchor: cditor_core::edit::TextPosition::downstream(
                                block_id,
                                range.start,
                            ),
                            focus: cditor_core::edit::TextPosition::downstream(block_id, range.end),
                        },
                    },
                    CommandSource::Toolbar,
                ))
                .ok()
        });
        if restored.is_none() {
            return false;
        }
        matches!(
            self.dispatch_command(
                CditorCommand::SetInlineLink { href, text },
                CommandSource::Toolbar,
                cx,
            ),
            Ok(outcome) if outcome.status == CommandOutcomeStatus::Applied
        )
    }

    /// Keyboard routing for the popup: Enter commits, Escape cancels, Tab
    /// switches between the label and URL fields.
    pub(crate) fn apply_link_edit_key(
        &mut self,
        action: crate::input::routing::BoundInputAction,
        cx: &mut gpui::Context<Self>,
    ) -> LinkEditKeyResult {
        use crate::input::routing::BoundInputAction;
        let result = {
            let Some(edit) = self.overlay.link_edit.as_mut() else {
                return LinkEditKeyResult::Ignored;
            };
            match action {
                BoundInputAction::Newline
                | BoundInputAction::NewlineBelow
                | BoundInputAction::SoftLineBreak => LinkEditKeyResult::Commit,
                BoundInputAction::Cancel => LinkEditKeyResult::Cancel,
                BoundInputAction::Tab { .. } => {
                    edit.toggle_field();
                    LinkEditKeyResult::Changed
                }
                BoundInputAction::MoveLeft { .. } => {
                    edit.move_caret_left();
                    LinkEditKeyResult::Changed
                }
                BoundInputAction::MoveRight { .. } => {
                    edit.move_caret_right();
                    LinkEditKeyResult::Changed
                }
                BoundInputAction::MoveToLineStart { .. } => {
                    edit.caret_offset = 0;
                    LinkEditKeyResult::Changed
                }
                BoundInputAction::MoveToLineEnd { .. } => {
                    edit.caret_offset = edit.active_draft().len();
                    LinkEditKeyResult::Changed
                }
                BoundInputAction::DeleteBackward => {
                    edit.delete_backward();
                    LinkEditKeyResult::Changed
                }
                _ => LinkEditKeyResult::Ignored,
            }
        };
        match result {
            LinkEditKeyResult::Commit => self.commit_link_edit(cx),
            LinkEditKeyResult::Cancel => self.cancel_link_edit(cx),
            LinkEditKeyResult::Changed => {
                if let LinkEditKeyResult::Changed = result
                    && let Some(edit) = self.overlay.link_edit.as_ref()
                {
                    // Tab may have moved the IME to the other field: retarget
                    // the platform registration through the focus request.
                    let target = match edit.focused_field {
                        LinkEditField::Text => GuiPlatformInputTarget::link_text(edit.block_id),
                        LinkEditField::Url => GuiPlatformInputTarget::link_url(edit.block_id),
                    };
                    self.input.request_focus(target);
                }
                cx.notify();
            }
            LinkEditKeyResult::Ignored => {}
        }
        result
    }
}

/// The first link href covering the range's start, used to prefill the popup
/// when editing an existing link.
fn link_href_in_range(
    spans: &[cditor_core::rich_text::InlineSpan],
    range: std::ops::Range<usize>,
) -> Option<String> {
    let mut offset = 0usize;
    for span in spans {
        let span_start = offset;
        let span_end = span_start + span.text.len();
        offset = span_end;
        if range.start >= span_end {
            continue;
        }
        if range.start < span_start {
            break;
        }
        return span.marks.iter().find_map(|mark| match mark {
            InlineMark::Link { href } | InlineMark::DocumentLink { href } => Some(href.clone()),
            _ => None,
        });
    }
    None
}
