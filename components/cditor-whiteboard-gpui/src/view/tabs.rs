use crate::canvas::CanvasDocument;
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};

use crate::theme::chrome;

use super::{BoardTab, DrafftBoardView};

impl DrafftBoardView {
    pub(super) fn render_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = chrome(cx);
        div()
            .absolute()
            .top(px(12.0))
            .left(px(56.0))
            .right(px(56.0))
            .h(px(32.0))
            .flex()
            .justify_center()
            .child(
                div()
                    .h(px(32.0))
                    .p(px(3.0))
                    .flex()
                    .gap(px(2.0))
                    .rounded(px(7.0))
                    .border_1()
                    .border_color(rgb(c.border))
                    .bg(rgb(c.bg))
                    .shadow_sm()
                    .children(self.tabs.iter().enumerate().map(|(index, tab)| {
                        let selected = index == self.active_tab;
                        div()
                            .id(("drafft-tab", index))
                            .h(px(24.0))
                            .max_w(px(180.0))
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .rounded(px(4.0))
                            .bg(if selected { rgb(c.active) } else { rgb(c.bg) })
                            .text_size(px(11.0))
                            .text_color(if selected {
                                rgb(c.accent)
                            } else {
                                rgb(c.text_muted)
                            })
                            .hover(|style| style.bg(rgb(c.hover)))
                            .child(tab.name.clone())
                            .when(self.tabs.len() > 1, |tab_element| {
                                tab_element.child(
                                    div()
                                        .id(("close-drafft-tab", index))
                                        .w(px(16.0))
                                        .h(px(16.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(3.0))
                                        .hover(|style| style.bg(rgb(c.active)))
                                        .child("x")
                                        .on_click(cx.listener(move |view, _, _, cx| {
                                            cx.stop_propagation();
                                            view.close_tab(index);
                                            cx.notify();
                                        })),
                                )
                            })
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.switch_tab(index);
                                cx.notify();
                            }))
                    })),
            )
            .into_any_element()
    }

    fn snapshot_active_tab(&mut self) {
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return;
        };
        tab.name = self.board.canvas.document.name.clone();
        tab.camera = self.board.canvas.camera.clone();
    }

    fn switch_tab(&mut self, index: usize) {
        if index == self.active_tab || index >= self.tabs.len() {
            return;
        }
        let Some(document) = self.tabs[index].document.take() else {
            return;
        };
        let camera = self.tabs[index].camera.clone();
        self.snapshot_active_tab();
        let previous = self.board.swap_document(document);
        self.tabs[self.active_tab].document = Some(previous);
        self.active_tab = index;
        self.board.canvas.camera = camera;
        self.image_paint_engine.borrow_mut().clear();
        self.current_path = None;
    }

    fn close_tab(&mut self, index: usize) {
        if self.tabs.len() <= 1 || index >= self.tabs.len() {
            return;
        }
        if index != self.active_tab {
            self.tabs.remove(index);
            if self.active_tab > index {
                self.active_tab -= 1;
            }
            return;
        }
        self.tabs.remove(index);
        let active = index.min(self.tabs.len() - 1);
        let document = self.tabs[active]
            .document
            .take()
            .expect("inactive tab owns its document");
        let camera = self.tabs[active].camera.clone();
        self.board.replace_document(document);
        self.active_tab = active;
        self.board.canvas.camera = camera;
        self.image_paint_engine.borrow_mut().clear();
        self.current_path = None;
    }

    pub(super) fn add_tab(&mut self, name: String, mut document: CanvasDocument) {
        self.snapshot_active_tab();
        document.name = name.clone();
        let previous = self.board.swap_document(document);
        self.tabs[self.active_tab].document = Some(previous);
        self.tabs.push(BoardTab {
            name,
            document: None,
            camera: crate::Camera::new(),
        });
        self.active_tab = self.tabs.len() - 1;
        self.board.canvas.camera = crate::Camera::new();
        self.image_paint_engine.borrow_mut().clear();
        self.current_path = None;
    }

    pub(super) fn sync_active_tab(&mut self) {
        self.snapshot_active_tab();
    }
}
