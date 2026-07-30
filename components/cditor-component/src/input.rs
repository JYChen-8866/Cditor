// Ported from gpui-component's Input and clear_button implementations.
// Copyright 2024-2025 Longbridge. Licensed under Apache-2.0.

use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, ElementId, FocusHandle, Hsla, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Pixels, RenderOnce, Styled, Window, div, px,
};

use crate::SvgIcon;

const CLEAR_ICON: &[u8] = include_bytes!("../../../assets/icons/circle-x.svg");
const DEFAULT_HEIGHT_PX: f32 = 30.0;
const CLEAR_BUTTON_SIZE_PX: f32 = 24.0;
const CLEAR_ICON_SIZE_PX: f32 = 16.0;

type CleanHandler = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(Debug, Clone, Copy)]
pub struct InputStyle {
    pub background: Hsla,
    pub foreground: Hsla,
    pub muted_foreground: Hsla,
    pub border: Hsla,
    pub focused_border: Hsla,
    pub hover_background: Hsla,
    pub radius: Pixels,
}

/// A controlled text-input shell adapted from gpui-component's `Input`.
///
/// Cditor keeps text editing in its existing `EntityInputHandler`; this component owns the
/// upstream input chrome, focus styling, suffix layout, and cleanable interaction.
#[derive(IntoElement)]
pub struct Input {
    id: ElementId,
    child: AnyElement,
    prefix: Option<AnyElement>,
    suffix: Option<AnyElement>,
    focus: Option<FocusHandle>,
    style: InputStyle,
    height: Pixels,
    appearance: bool,
    cleanable: bool,
    empty: bool,
    disabled: bool,
    bordered: bool,
    focus_bordered: bool,
    on_clean: Option<CleanHandler>,
}

impl Input {
    pub fn new(id: impl Into<ElementId>, child: impl IntoElement, style: InputStyle) -> Self {
        Self {
            id: id.into(),
            child: child.into_any_element(),
            prefix: None,
            suffix: None,
            focus: None,
            style,
            height: px(DEFAULT_HEIGHT_PX),
            appearance: true,
            cleanable: false,
            empty: true,
            disabled: false,
            bordered: true,
            focus_bordered: true,
            on_clean: None,
        }
    }

    pub fn prefix(mut self, prefix: impl IntoElement) -> Self {
        self.prefix = Some(prefix.into_any_element());
        self
    }

    pub fn suffix(mut self, suffix: impl IntoElement) -> Self {
        self.suffix = Some(suffix.into_any_element());
        self
    }

    pub fn focus(mut self, focus: FocusHandle) -> Self {
        self.focus = Some(focus);
        self
    }

    pub fn height(mut self, height: Pixels) -> Self {
        self.height = height;
        self
    }

    pub fn appearance(mut self, appearance: bool) -> Self {
        self.appearance = appearance;
        self
    }

    pub fn bordered(mut self, bordered: bool) -> Self {
        self.bordered = bordered;
        self
    }

    pub fn focus_bordered(mut self, bordered: bool) -> Self {
        self.focus_bordered = bordered;
        self
    }

    pub fn cleanable(mut self, cleanable: bool) -> Self {
        self.cleanable = cleanable;
        self
    }

    pub fn empty(mut self, empty: bool) -> Self {
        self.empty = empty;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_clean(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_clean = Some(Rc::new(handler));
        self
    }

    fn show_clear_button(&self) -> bool {
        self.cleanable && !self.disabled && !self.empty && self.on_clean.is_some()
    }
}

impl RenderOnce for Input {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let focused = self
            .focus
            .as_ref()
            .is_some_and(|focus| focus.is_focused(window));
        let show_clear_button = self.show_clear_button();
        let has_suffix = self.suffix.is_some() || show_clear_button;
        let focus = self.focus.clone();
        let style = self.style;

        div()
            .id(self.id)
            .h(self.height)
            .w_full()
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(8.0))
            .text_color(if self.disabled {
                style.muted_foreground
            } else {
                style.foreground
            })
            .when(self.disabled, |input| input.opacity(0.5))
            .when_some(focus.clone(), |input, focus| input.track_focus(&focus))
            .when(self.appearance, |input| {
                input
                    .bg(style.background)
                    .rounded(style.radius)
                    .when(self.bordered, |input| {
                        input
                            .border_1()
                            .border_color(if focused && self.focus_bordered {
                                style.focused_border
                            } else {
                                style.border
                            })
                    })
            })
            .children(self.prefix)
            .child(div().min_w(px(0.0)).h_full().flex_1().child(self.child))
            .when(has_suffix, |input| {
                input.child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .when(show_clear_button, |suffix| {
                            let handler = self.on_clean.clone().expect("clean handler is present");
                            suffix.child(
                                div()
                                    .id("cleanable-input-clear")
                                    .size(px(CLEAR_BUTTON_SIZE_PX))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(style.radius)
                                    .cursor_pointer()
                                    .hover(move |button| button.bg(style.hover_background))
                                    .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                                        handler(window, cx);
                                        if let Some(focus) = focus.as_ref() {
                                            window.focus(focus, cx);
                                        }
                                        cx.stop_propagation();
                                    })
                                    .child(
                                        SvgIcon::new("cleanable-input-circle-x", CLEAR_ICON)
                                            .color(style.muted_foreground)
                                            .size(px(CLEAR_ICON_SIZE_PX)),
                                    ),
                            )
                        })
                        .children(self.suffix),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_button_matches_upstream_visibility_rules() {
        let style = InputStyle {
            background: gpui::transparent_black(),
            foreground: gpui::black(),
            muted_foreground: gpui::black(),
            border: gpui::black(),
            focused_border: gpui::black(),
            hover_background: gpui::transparent_black(),
            radius: px(4.0),
        };
        let input = || Input::new("test-input", div(), style).on_clean(|_, _| {});

        assert!(input().cleanable(true).empty(false).show_clear_button());
        assert!(!input().cleanable(false).empty(false).show_clear_button());
        assert!(!input().cleanable(true).empty(true).show_clear_button());
        assert!(
            !input()
                .cleanable(true)
                .empty(false)
                .disabled(true)
                .show_clear_button()
        );
    }

    #[test]
    fn clear_icon_is_embedded_from_the_shared_asset_directory() {
        assert!(std::str::from_utf8(CLEAR_ICON).unwrap().starts_with("<svg"));
    }
}
