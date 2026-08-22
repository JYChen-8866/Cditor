use cditor_component::{PopupMenu, PopupMenuItem};
use cditor_editor_protocol::command::{CditorCommand, CommandSource};
use gpui::{
    AnyElement, Context, DismissEvent, Entity, Focusable, IntoElement, ParentElement, Styled,
    Window, deferred, div, px,
};

use crate::editor_view::CditorV2View;
use crate::overlays::gutter_popup_menu_style;
use crate::theme::GuiTheme;

const MENU_WIDTH_PX: f32 = 190.0;
const MENU_HEIGHT_PX: f32 = 90.0;
const VIEWPORT_MARGIN_PX: f32 = 8.0;

pub(crate) fn show_editor_context_menu(
    view: &mut CditorV2View,
    pointer_x: f32,
    pointer_y: f32,
    viewport_width: f32,
    viewport_height: f32,
    theme: GuiTheme,
    window: &mut Window,
    cx: &mut Context<CditorV2View>,
) {
    let copy_enabled = view
        .sdk_command_state(&CditorCommand::CopySelection)
        .enabled;
    let copy_markdown_enabled = view
        .sdk_command_state(&CditorCommand::CopySelectionAsMarkdown)
        .enabled;
    let paste_enabled = !view.status.readonly
        && view
            .sdk_command_state(&CditorCommand::PasteClipboard)
            .enabled;
    let position = context_menu_position(pointer_x, pointer_y, viewport_width, viewport_height);
    let editor_focus = view.focus.editor.clone();
    let editor = cx.entity();
    let copy_editor = editor.clone();
    let markdown_editor = editor.clone();
    let paste_editor = editor.clone();
    let menu =
        PopupMenu::build(window, cx, move |menu, _window, _cx| {
            menu.style(gutter_popup_menu_style(theme))
                .action_context(editor_focus)
                .min_w(px(MENU_WIDTH_PX))
                .max_w(px(MENU_WIDTH_PX))
                .item(PopupMenuItem::new("复制").disabled(!copy_enabled).on_click(
                    move |_, _, cx| {
                        copy_editor.update(cx, |view, cx| {
                            let _ = view.dispatch_command(
                                CditorCommand::CopySelection,
                                CommandSource::ContextMenu,
                                cx,
                            );
                        });
                    },
                ))
                .item(
                    PopupMenuItem::new("粘贴")
                        .disabled(!paste_enabled)
                        .on_click(move |_, _, cx| {
                            paste_editor.update(cx, |view, cx| {
                                let _ = view.dispatch_command(
                                    CditorCommand::PasteClipboard,
                                    CommandSource::ContextMenu,
                                    cx,
                                );
                            });
                        }),
                )
                .item(
                    PopupMenuItem::new("复制为 Markdown")
                        .disabled(!copy_markdown_enabled)
                        .on_click(move |_, _, cx| {
                            markdown_editor.update(cx, |view, cx| {
                                let _ = view.dispatch_command(
                                    CditorCommand::CopySelectionAsMarkdown,
                                    CommandSource::ContextMenu,
                                    cx,
                                );
                            });
                        }),
                )
        });
    let subscription = cx.subscribe(&menu, |view: &mut CditorV2View, _, _: &DismissEvent, cx| {
        view.overlay.editor_context_menu = None;
        view.overlay.editor_context_menu_position = None;
        view.overlay.editor_context_menu_dismiss_subscription = None;
        cx.notify();
    });
    menu.focus_handle(cx).focus(window, cx);
    view.overlay.editor_context_menu = Some(menu);
    view.overlay.editor_context_menu_position = Some(position);
    view.overlay.editor_context_menu_dismiss_subscription = Some(subscription);
    cx.notify();
}

pub(crate) fn render_editor_context_menu(
    position: (f32, f32),
    menu: Entity<PopupMenu>,
) -> AnyElement {
    deferred(
        div()
            .absolute()
            .left(px(position.0))
            .top(px(position.1))
            .child(menu),
    )
    .with_priority(180)
    .into_any_element()
}

fn context_menu_position(
    pointer_x: f32,
    pointer_y: f32,
    viewport_width: f32,
    viewport_height: f32,
) -> (f32, f32) {
    let max_x = (viewport_width - MENU_WIDTH_PX - VIEWPORT_MARGIN_PX).max(VIEWPORT_MARGIN_PX);
    let max_y = (viewport_height - MENU_HEIGHT_PX - VIEWPORT_MARGIN_PX).max(VIEWPORT_MARGIN_PX);
    (
        pointer_x.clamp(VIEWPORT_MARGIN_PX, max_x),
        pointer_y.clamp(VIEWPORT_MARGIN_PX, max_y),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_menu_stays_inside_the_editor_viewport() {
        assert_eq!(
            context_menu_position(790.0, 590.0, 800.0, 600.0),
            (602.0, 502.0)
        );
        assert_eq!(
            context_menu_position(-20.0, -30.0, 800.0, 600.0),
            (8.0, 8.0)
        );
    }

    #[test]
    fn context_menu_handles_a_viewport_smaller_than_the_menu() {
        assert_eq!(context_menu_position(60.0, 40.0, 120.0, 70.0), (8.0, 8.0));
    }
}
