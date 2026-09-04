use cditor_core::ids::BlockId;
use gpui::{Context, Window};
use std::time::Duration;

use crate::editor_view::CditorV2View;
use crate::persistence::EditorSaveStatus;
use cditor_editor_protocol::command::{CditorCommand, CommandOutcomeStatus, CommandSource};

pub(crate) fn block_focus_offset_after_missed_hit_test(
    focused_block_id: Option<BlockId>,
    target_block_id: BlockId,
    target_caret_offset: Option<usize>,
) -> usize {
    if focused_block_id == Some(target_block_id) {
        target_caret_offset.unwrap_or(0)
    } else {
        0
    }
}

impl CditorV2View {
    pub(crate) fn insert_paragraph_after_block_from_gui(
        &mut self,
        block_id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.status.readonly {
            return false;
        }
        window.focus(&self.focus.editor, cx);
        match self.dispatch_command(
            CditorCommand::InsertParagraphAfterBlock { block_id },
            CommandSource::Toolbar,
            cx,
        ) {
            Ok(outcome) if outcome.status == CommandOutcomeStatus::Applied => {
                self.overlay.slash_menu = None;
                cx.notify();
                true
            }
            Ok(_) => false,
            Err(error) => {
                self.status.save_status = EditorSaveStatus::Failed(error.to_string());
                cx.notify();
                false
            }
        }
    }

    pub(crate) fn delete_block_from_gui(
        &mut self,
        block_id: BlockId,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.status.readonly {
            return false;
        }
        match self.dispatch_command(
            CditorCommand::DeleteBlock { block_id },
            CommandSource::Toolbar,
            cx,
        ) {
            Ok(outcome) if outcome.status == CommandOutcomeStatus::Applied => {
                if self.overlay.gutter_toolbar_block_id == Some(block_id) {
                    self.overlay.gutter_toolbar_block_id = None;
                    self.overlay.block_transform_menu_open = false;
                    self.overlay.color_menu_open = false;
                }
                if self.interaction.action_block_id == Some(block_id) {
                    self.interaction.action_block_id = None;
                }
                cx.notify();
                true
            }
            Ok(_) => false,
            Err(error) => {
                self.status.save_status = EditorSaveStatus::Failed(error.to_string());
                cx.notify();
                false
            }
        }
    }

    /// 展开 gutter 菜单里「复制」的二级菜单。
    ///
    /// 与颜色那一项同构：展开一个就把另一个收起来，避免两个二级菜单同时挂在
    /// 主菜单右侧互相盖住。
    pub(crate) fn open_copy_menu_from_gui(&mut self, cx: &mut Context<Self>) -> bool {
        if self.overlay.copy_menu_open || self.overlay.gutter_toolbar_block_id.is_none() {
            return false;
        }
        self.overlay.copy_menu_open = true;
        self.overlay.color_menu_open = false;
        self.overlay.block_transform_menu_open = false;
        self.overlay.block_transform_popup_menu = None;
        self.overlay.block_transform_popup_menu_dismiss_subscription = None;
        cx.notify();
        true
    }

    /// 悬停进入展开、离开后延迟收起。延迟是为了让指针能从触发行斜着移到
    /// 二级菜单上而不中途关闭。
    pub(crate) fn set_copy_menu_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        self.overlay.copy_menu_hover_generation =
            self.overlay.copy_menu_hover_generation.wrapping_add(1);
        if hovered {
            self.open_copy_menu_from_gui(cx);
            return;
        }

        let generation = self.overlay.copy_menu_hover_generation;
        let delay = cx.background_executor().timer(Duration::from_millis(140));
        cx.spawn(async move |view, cx| {
            delay.await;
            let _ = view.update(cx, |view, cx| {
                if view.overlay.copy_menu_hover_generation == generation
                    && view.overlay.copy_menu_open
                {
                    view.overlay.copy_menu_open = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(crate) fn copy_block_text_from_gui(
        &mut self,
        block_id: BlockId,
        cx: &mut Context<Self>,
    ) -> bool {
        let copied = self
            .dispatch_command(
                CditorCommand::CopyBlockText { block_id },
                CommandSource::Toolbar,
                cx,
            )
            .is_ok_and(|outcome| outcome.status == CommandOutcomeStatus::Applied);
        if !copied {
            return false;
        }

        self.clear_gutter_action();
        crate::overlays::show_toast(self, "已复制区块", Duration::from_secs(3), cx);
        true
    }

    pub(crate) fn copy_block_markdown_from_gui(
        &mut self,
        block_id: BlockId,
        cx: &mut Context<Self>,
    ) -> bool {
        let copied = self
            .dispatch_command(
                CditorCommand::CopyBlockMarkdown { block_id },
                CommandSource::Toolbar,
                cx,
            )
            .is_ok_and(|outcome| outcome.status == CommandOutcomeStatus::Applied);
        if !copied {
            return false;
        }

        self.clear_gutter_action();
        crate::overlays::show_toast(self, "已复制为 Markdown", Duration::from_secs(3), cx);
        true
    }

    pub(crate) fn copy_block_link_from_gui(
        &mut self,
        block_id: BlockId,
        cx: &mut Context<Self>,
    ) -> bool {
        let copied = self
            .dispatch_command(
                CditorCommand::CopyBlockLink { block_id },
                CommandSource::Toolbar,
                cx,
            )
            .is_ok_and(|outcome| outcome.status == CommandOutcomeStatus::Applied);
        if !copied {
            return false;
        }

        self.clear_gutter_action();
        crate::overlays::show_toast(self, "已复制区块链接", Duration::from_secs(3), cx);
        true
    }
}

#[cfg(test)]
mod tests {
    use cditor_core::clipboard::{CditorClipboardEnvelope, ClipboardSelection};
    use cditor_core::rich_text::{BlockPayload, InlineMark};
    use cditor_runtime::DocumentRuntime;
    use gpui::{AppContext, TestAppContext};

    use super::*;

    #[gpui::test]
    fn copying_block_link_writes_canonical_link_and_closes_gutter(cx: &mut TestAppContext) {
        let view = cx.new(|cx| CditorV2View::from_runtime(DocumentRuntime::demo(), false, cx));
        let (document_id, block_id) = view.update(cx, |view, cx| {
            let snapshot = view.ready_session().unwrap().snapshot().unwrap();
            let block_id = 1;
            view.sdk_configure_block_link_provider(Some(std::sync::Arc::new(|block_id| {
                cditor_core::internal_link::BlockLinkPresentation::new(
                    format!("aurin://doc/node/content/block/{block_id}"),
                    "Document title",
                )
            })));
            view.overlay.gutter_toolbar_block_id = Some(block_id);
            assert!(view.copy_block_link_from_gui(block_id, cx));
            assert_eq!(view.overlay.gutter_toolbar_block_id, None);
            assert_eq!(
                view.overlay
                    .toast
                    .as_ref()
                    .map(|toast| toast.message.as_str()),
                Some("已复制区块链接")
            );
            (snapshot.document_id, block_id)
        });

        assert_eq!(
            cx.read_from_clipboard().and_then(|item| item.text()),
            Some(format!("aurin://doc/node/content/block/{block_id}"))
        );
        assert_ne!(
            cx.read_from_clipboard().and_then(|item| item.text()),
            Some(cditor_core::internal_link::block_link(
                document_id,
                block_id
            ))
        );

        let item = cx.read_from_clipboard().expect("block link clipboard item");
        let text = item.text().expect("block link clipboard text");
        let envelope = CditorClipboardEnvelope::decode_metadata(
            item.metadata().expect("block link clipboard metadata"),
            &text,
        )
        .expect("valid Cditor clipboard metadata");
        let ClipboardSelection::DocumentLink { label, href } = envelope.selection else {
            panic!("block link must use document-link clipboard metadata");
        };
        assert_eq!(label, "Document title");
        assert_eq!(href, text);
    }

    #[gpui::test]
    fn copying_block_link_without_host_provider_uses_cditor_link(cx: &mut TestAppContext) {
        let view = cx.new(|cx| CditorV2View::from_runtime(DocumentRuntime::demo(), false, cx));
        let (document_id, block_id) = view.update(cx, |view, cx| {
            let snapshot = view.ready_session().unwrap().snapshot().unwrap();
            let block_id = 1;
            assert!(view.copy_block_link_from_gui(block_id, cx));
            (snapshot.document_id, block_id)
        });

        assert_eq!(
            cx.read_from_clipboard().and_then(|item| item.text()),
            Some(cditor_core::internal_link::block_link(
                document_id,
                block_id
            ))
        );
    }

    #[gpui::test]
    fn pasting_copied_block_link_preserves_the_colored_link_mark(cx: &mut TestAppContext) {
        let view = cx.new(|cx| CditorV2View::from_runtime(DocumentRuntime::demo(), false, cx));
        view.update(cx, |view, cx| {
            view.sdk_configure_block_link_provider(Some(std::sync::Arc::new(|block_id| {
                cditor_core::internal_link::BlockLinkPresentation::new(
                    format!("aurin://doc/node/content/block/{block_id}"),
                    "Document title",
                )
            })));
            assert!(view.copy_block_link_from_gui(1, cx));
        });
        let item = cx.read_from_clipboard().expect("block link clipboard item");
        let text = item.text().expect("block link clipboard text");
        let metadata = item.metadata().expect("block link clipboard metadata");

        let mut target = DocumentRuntime::empty();
        target
            .dispatch(cditor_editor_protocol::command::CommandEnvelope::new(
                cditor_editor_protocol::command::EditorCommand::FocusBlock { block_id: 1 },
                cditor_editor_protocol::command::CommandSource::Automation,
            ))
            .unwrap();
        let report = cditor_session::project_clipboard_import(&mut target, &text, Some(metadata))
            .expect("paste block link");
        assert!(report.outcome.changed());

        let payload = target.block_payload_record(1).unwrap();
        let BlockPayload::RichText { spans } = payload.payload else {
            panic!("pasted block link must remain rich text");
        };
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "Document title");
        assert_eq!(
            spans[0].marks,
            vec![InlineMark::DocumentLink { href: text }]
        );
    }
}
