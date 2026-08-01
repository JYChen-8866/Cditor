// Ported from gpui-component's Combobox component.
// Copyright 2024-2025 Longbridge. Licensed under Apache-2.0.
// Adaptation: controlled state and Cditor-owned input/theme/icon primitives.

use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, Styled,
    Window, deferred, div, px, rgb,
};

use crate::{InteractiveScrollbar, InteractiveScrollbarStyle, ScrollbarAxis, SvgIcon};

const CHEVRON: &[u8] = include_bytes!("../../../assets/icons/chevron-down.svg");
const CHECK: &[u8] = include_bytes!("../../../assets/icons/check.svg");
const SCROLLBAR_WIDTH_PX: f32 = 10.0;
const SCROLLBAR_INSET_PX: f32 = 4.0;

type ToggleHandler = Rc<dyn Fn(f32, &mut Window, &mut App)>;
type AppHandler = Rc<dyn Fn(&mut Window, &mut App)>;
type ScrollHandler = Rc<dyn Fn(isize, &mut Window, &mut App)>;
type ScrollToHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComboboxPlacement {
    Below,
    Above,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComboboxStyle {
    pub background: u32,
    pub hover: u32,
    pub border: u32,
    pub text: u32,
    pub muted: u32,
    pub accent: u32,
    pub focused_border: u32,
    pub trigger_height: Pixels,
    pub trigger_min_width: Pixels,
    pub trigger_max_width: Pixels,
    pub trigger_radius: Pixels,
    pub popup_width: Pixels,
    pub popup_gap: Pixels,
    pub popup_radius: Pixels,
    pub search_height: Pixels,
    pub row_height: Pixels,
}

#[derive(Clone)]
pub struct ComboboxItem {
    pub label: String,
    pub detail: Option<String>,
    pub selected: bool,
    pub checked: bool,
    on_select: AppHandler,
}

impl ComboboxItem {
    pub fn new(
        label: impl Into<String>,
        on_select: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            label: label.into(),
            detail: None,
            selected: false,
            checked: false,
            on_select: Rc::new(on_select),
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
}

/// A controlled, searchable, single-select combobox.
///
/// Query text, keyboard selection and focus remain in the caller. This mirrors
/// the controlled-state portion of `gpui-component::Combobox` while avoiding
/// dependencies on its input, searchable-list, icon and theme subsystems.
pub struct Combobox {
    label: String,
    trigger_icon: Option<AnyElement>,
    open: bool,
    placement: ComboboxPlacement,
    style: ComboboxStyle,
    search: Option<AnyElement>,
    items: Vec<ComboboxItem>,
    total_items: usize,
    scroll_start: usize,
    visible_items: usize,
    empty_label: String,
    popup_right_offset: Pixels,
    deferred_priority: usize,
    on_toggle: ToggleHandler,
    on_dismiss: AppHandler,
    on_scroll: ScrollHandler,
    on_scroll_to: ScrollToHandler,
}

impl Combobox {
    pub fn new(
        label: impl Into<String>,
        open: bool,
        style: ComboboxStyle,
        on_toggle: impl Fn(f32, &mut Window, &mut App) + 'static,
        on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            label: label.into(),
            trigger_icon: None,
            open,
            placement: ComboboxPlacement::Below,
            style,
            search: None,
            items: Vec::new(),
            total_items: 0,
            scroll_start: 0,
            visible_items: 7,
            empty_label: "No matching suggestions".to_owned(),
            popup_right_offset: px(0.0),
            deferred_priority: 100,
            on_toggle: Rc::new(on_toggle),
            on_dismiss: Rc::new(on_dismiss),
            on_scroll: Rc::new(|_, _, _| {}),
            on_scroll_to: Rc::new(|_, _, _| {}),
        }
    }

    pub fn placement(mut self, placement: ComboboxPlacement) -> Self {
        self.placement = placement;
        self
    }

    pub fn search(mut self, search: impl IntoElement) -> Self {
        self.search = Some(search.into_any_element());
        self
    }

    pub fn items(mut self, items: Vec<ComboboxItem>, total_items: usize) -> Self {
        self.items = items;
        self.total_items = total_items;
        self
    }

    pub fn scroll(
        mut self,
        scroll_start: usize,
        visible_items: usize,
        on_scroll: impl Fn(isize, &mut Window, &mut App) + 'static,
        on_scroll_to: impl Fn(usize, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.scroll_start = scroll_start;
        self.visible_items = visible_items.max(1);
        self.on_scroll = Rc::new(on_scroll);
        self.on_scroll_to = Rc::new(on_scroll_to);
        self
    }

    pub fn popup_right_offset(mut self, offset: Pixels) -> Self {
        self.popup_right_offset = offset;
        self
    }

    pub fn trigger_icon(mut self, icon: impl IntoElement) -> Self {
        self.trigger_icon = Some(icon.into_any_element());
        self
    }

    pub fn empty_label(mut self, label: impl Into<String>) -> Self {
        self.empty_label = label.into();
        self
    }

    fn render_trigger(&mut self) -> AnyElement {
        let style = self.style;
        let open = self.open;
        let on_toggle = self.on_toggle.clone();
        let trigger_icon = self.trigger_icon.take();
        div()
            .h(style.trigger_height)
            .min_w(style.trigger_min_width)
            .max_w(style.trigger_max_width)
            .px(px(12.0))
            .flex()
            .items_center()
            .rounded(style.trigger_radius)
            .border_1()
            .border_color(rgb(if open {
                style.focused_border
            } else {
                style.border
            }))
            .text_color(rgb(style.text))
            .bg(rgb(if open { style.hover } else { style.background }))
            .shadow_xs()
            .hover(move |trigger| trigger.border_color(rgb(style.muted)))
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                on_toggle(f32::from(event.position.y), window, cx);
                cx.stop_propagation();
            })
            .child(
                div()
                    .w_full()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(4.0))
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .min_w(px(0.0))
                            .flex_shrink_1()
                            .overflow_hidden()
                            .when_some(trigger_icon, |content, icon| content.child(icon))
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .flex_shrink_1()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .child(self.label.clone()),
                            ),
                    )
                    .child(
                        SvgIcon::new("combobox-chevron-down", CHEVRON)
                            .color(rgb(style.muted))
                            .size(px(12.0)),
                    ),
            )
            .into_any_element()
    }

    fn render_popup(mut self) -> AnyElement {
        let style = self.style;
        let list_rows = self.total_items.clamp(1, self.visible_items);
        let list_height = style.row_height * list_rows as f32;
        let panel_height = style.search_height + list_height;
        let on_scroll = self.on_scroll.clone();
        let mut panel = div()
            .absolute()
            .right(-self.popup_right_offset)
            .w(style.popup_width)
            .h(panel_height)
            .rounded(style.popup_radius)
            .border_1()
            .border_color(rgb(style.border))
            .bg(rgb(style.background))
            .shadow_md()
            .occlude()
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation()
            })
            .on_scroll_wheel(move |event, window, cx| {
                let delta_y = f32::from(event.delta.pixel_delta(style.row_height).y);
                let rows = scroll_delta_rows(delta_y, f32::from(style.row_height));
                if rows != 0 {
                    on_scroll(rows, window, cx);
                }
                cx.stop_propagation();
            })
            .when(self.placement == ComboboxPlacement::Below, |panel| {
                panel.top(style.trigger_height + style.popup_gap)
            })
            .when(self.placement == ComboboxPlacement::Above, |panel| {
                panel.bottom(style.trigger_height + style.popup_gap)
            });

        if let Some(search) = self.search.take() {
            panel = panel.child(search);
        }
        if self.items.is_empty() {
            panel = panel.child(
                div()
                    .h(style.row_height)
                    .flex()
                    .items_center()
                    .px(px(12.0))
                    .text_size(px(12.0))
                    .text_color(rgb(style.muted))
                    .child(self.empty_label),
            );
        } else {
            panel = panel.child(
                div()
                    .w_full()
                    .h(list_height)
                    .bg(rgb(style.background))
                    .p(px(4.0))
                    .children(
                        self.items
                            .clone()
                            .into_iter()
                            .map(|item| render_item(style, item)),
                    ),
            );
            if self.total_items > self.visible_items {
                panel = panel.child(self.render_scrollbar(list_height));
            }
        }
        deferred(panel)
            .with_priority(self.deferred_priority)
            .into_any_element()
    }

    fn render_scrollbar(&self, list_height: Pixels) -> AnyElement {
        let visible = self.visible_items.min(self.total_items);
        let max_start = self.total_items.saturating_sub(visible);
        let row_height = f32::from(self.style.row_height);
        let on_scroll_to = self.on_scroll_to.clone();
        div()
            .absolute()
            .right_0()
            .top(self.style.search_height + px(SCROLLBAR_INSET_PX))
            .w(px(SCROLLBAR_WIDTH_PX))
            .h(list_height - px(SCROLLBAR_INSET_PX * 2.0))
            .child(InteractiveScrollbar::for_callback(
                ScrollbarAxis::Vertical,
                self.scroll_start.min(max_start) as f32 * row_height,
                max_start as f32 * row_height,
                visible as f32 / self.total_items as f32,
                InteractiveScrollbarStyle::notion(self.style.muted, self.style.text),
                move |offset, window, cx| {
                    on_scroll_to((offset / row_height).round() as usize, window, cx);
                },
            ))
            .into_any_element()
    }
}

impl IntoElement for Combobox {
    type Element = AnyElement;

    fn into_element(mut self) -> Self::Element {
        let open = self.open;
        let on_dismiss = self.on_dismiss.clone();
        let trigger = self.render_trigger();
        div()
            .relative()
            .h(self.style.trigger_height)
            .flex()
            .items_center()
            .when(open, |root| {
                root.on_mouse_down_out(move |_event, window, cx| on_dismiss(window, cx))
            })
            .child(trigger)
            .when(open, |root| root.child(self.render_popup()))
            .into_any_element()
    }
}

fn render_item(style: ComboboxStyle, item: ComboboxItem) -> AnyElement {
    let on_select = item.on_select;
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_between()
        .w_full()
        .h(style.row_height)
        .gap(px(4.0))
        .px(px(12.0))
        .rounded(style.trigger_radius)
        .cursor_pointer()
        .bg(rgb(if item.selected {
            style.accent
        } else {
            style.background
        }))
        .hover(move |row| row.bg(rgb(style.hover)).cursor_pointer())
        .child(
            div()
                .w_full()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(12.0))
                .text_color(rgb(style.text))
                .child(item.label),
        )
        .when_some(item.detail, |row, detail| {
            row.child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(style.muted))
                    .child(detail),
            )
        })
        .child(
            div()
                .w(px(12.0))
                .h(px(12.0))
                .flex_none()
                .when(item.checked, |check| {
                    check.child(
                        SvgIcon::new("combobox-check", CHECK)
                            .color(rgb(style.text))
                            .size(px(12.0)),
                    )
                }),
        )
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            on_select(window, cx);
            cx.stop_propagation();
        })
        .into_any_element()
}

fn scroll_delta_rows(delta_y: f32, row_height: f32) -> isize {
    if delta_y.abs() < 1.0 {
        return 0;
    }
    let rows = (delta_y.abs() / row_height).ceil().max(1.0) as isize;
    if delta_y > 0.0 { -rows } else { rows }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_icons_are_embedded_from_the_workspace_assets() {
        assert!(
            std::str::from_utf8(CHEVRON)
                .unwrap()
                .contains("m6 9 6 6 6-6")
        );
        assert!(
            std::str::from_utf8(CHECK)
                .unwrap()
                .contains("M20 6 9 17l-5-5")
        );
    }

    #[test]
    fn wheel_delta_maps_to_rows_in_both_directions() {
        assert_eq!(scroll_delta_rows(0.2, 34.0), 0);
        assert_eq!(scroll_delta_rows(1.0, 34.0), -1);
        assert_eq!(scroll_delta_rows(35.0, 34.0), -2);
        assert_eq!(scroll_delta_rows(-35.0, 34.0), 2);
    }
}
