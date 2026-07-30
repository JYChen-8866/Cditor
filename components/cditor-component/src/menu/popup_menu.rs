// Adapted from gpui-component's PopupMenu.
// Copyright 2024-2025 Longbridge. Licensed under Apache-2.0.

use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    Action, Anchor, AnyElement, App, AppContext, Bounds, ClickEvent, Context, DismissEvent, Edges,
    Entity, EventEmitter, FocusHandle, Focusable, Hsla, InteractiveElement, IntoElement,
    KeyBinding, MouseButton, MouseDownEvent, ParentElement, Pixels, Render, Rgba, ScrollHandle,
    SharedString, StatefulInteractiveElement, Styled, WeakEntity, Window, actions, anchored,
    canvas, div, px, rems,
};

use crate::SvgIcon;
use crate::scrollbar::{InteractiveScrollbar, InteractiveScrollbarStyle, ScrollbarAxis};

const CONTEXT: &str = "CditorPopupMenu";
const SUBMENU_ARROW: &[u8] = include_bytes!("../../../../assets/icons/jiantou.svg");
const POPUP_MENU_PADDING_PX: f32 = 4.0;
const POPUP_MENU_ITEM_GAP_PX: f32 = 2.0;
const POPUP_MENU_SCROLLBAR_WIDTH_PX: f32 = 10.0;
pub const POPUP_MENU_ITEM_FONT_SIZE_PX: f32 = 14.0;
pub const POPUP_MENU_LABEL_FONT_SIZE_PX: f32 = 11.0;

actions!(
    cditor_popup_menu,
    [
        Confirm,
        Cancel,
        SelectUp,
        SelectDown,
        SelectLeft,
        SelectRight
    ]
);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", Confirm, Some(CONTEXT)),
        KeyBinding::new("escape", Cancel, Some(CONTEXT)),
        KeyBinding::new("up", SelectUp, Some(CONTEXT)),
        KeyBinding::new("down", SelectDown, Some(CONTEXT)),
        KeyBinding::new("left", SelectLeft, Some(CONTEXT)),
        KeyBinding::new("right", SelectRight, Some(CONTEXT)),
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupMenuStyle {
    pub background: Hsla,
    pub foreground: Hsla,
    pub muted_foreground: Hsla,
    pub border: Hsla,
    pub selected_background: Hsla,
    pub selected_foreground: Hsla,
    pub radius: Pixels,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PopupMenuCheckSide {
    #[default]
    Left,
    Right,
}

impl Default for PopupMenuStyle {
    fn default() -> Self {
        Self {
            background: gpui::white(),
            foreground: gpui::black(),
            muted_foreground: gpui::black().opacity(0.55),
            border: gpui::black().opacity(0.12),
            selected_background: gpui::black().opacity(0.06),
            selected_foreground: gpui::black(),
            radius: px(6.0),
        }
    }
}

#[derive(Clone)]
pub struct PopupMenuIcon(Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>);

impl PopupMenuIcon {
    pub fn new<E, F>(render: F) -> Self
    where
        E: IntoElement,
        F: Fn(&mut Window, &mut App) -> E + 'static,
    {
        Self(Rc::new(move |window, cx| {
            render(window, cx).into_any_element()
        }))
    }

    fn render(&self, window: &mut Window, cx: &mut App) -> AnyElement {
        (self.0)(window, cx)
    }
}

pub enum PopupMenuItem {
    Separator,
    Label(SharedString),
    Item {
        icon: Option<PopupMenuIcon>,
        label: SharedString,
        description: Option<SharedString>,
        disabled: bool,
        checked: bool,
        action: Option<Box<dyn Action>>,
        handler: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
    },
    ElementItem {
        icon: Option<PopupMenuIcon>,
        disabled: bool,
        checked: bool,
        render: Box<dyn Fn(&mut Window, &mut App) -> AnyElement>,
        handler: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
    },
    Submenu {
        icon: Option<PopupMenuIcon>,
        label: SharedString,
        description: Option<SharedString>,
        disabled: bool,
        menu: Entity<PopupMenu>,
    },
}

impl FluentBuilder for PopupMenuItem {}

impl PopupMenuItem {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self::Item {
            icon: None,
            label: label.into(),
            description: None,
            disabled: false,
            checked: false,
            action: None,
            handler: None,
        }
    }

    pub fn element<E, F>(render: F) -> Self
    where
        E: IntoElement,
        F: Fn(&mut Window, &mut App) -> E + 'static,
    {
        Self::ElementItem {
            icon: None,
            disabled: false,
            checked: false,
            render: Box::new(move |window, cx| render(window, cx).into_any_element()),
            handler: None,
        }
    }

    pub fn submenu(label: impl Into<SharedString>, menu: Entity<PopupMenu>) -> Self {
        Self::Submenu {
            icon: None,
            label: label.into(),
            description: None,
            disabled: false,
            menu,
        }
    }

    pub fn separator() -> Self {
        Self::Separator
    }

    pub fn label(label: impl Into<SharedString>) -> Self {
        Self::Label(label.into())
    }

    pub fn icon(mut self, icon: PopupMenuIcon) -> Self {
        match &mut self {
            Self::Item { icon: value, .. }
            | Self::ElementItem { icon: value, .. }
            | Self::Submenu { icon: value, .. } => *value = Some(icon),
            _ => {}
        }
        self
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        match &mut self {
            Self::Item {
                description: value, ..
            }
            | Self::Submenu {
                description: value, ..
            } => *value = Some(description.into()),
            _ => {}
        }
        self
    }

    pub fn action(mut self, action: Box<dyn Action>) -> Self {
        if let Self::Item { action: value, .. } = &mut self {
            *value = Some(action);
        }
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        match &mut self {
            Self::Item {
                disabled: value, ..
            }
            | Self::ElementItem {
                disabled: value, ..
            }
            | Self::Submenu {
                disabled: value, ..
            } => *value = disabled,
            _ => {}
        }
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        match &mut self {
            Self::Item { checked: value, .. } | Self::ElementItem { checked: value, .. } => {
                *value = checked
            }
            _ => {}
        }
        self
    }

    pub fn on_click<F>(mut self, handler: F) -> Self
    where
        F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    {
        match &mut self {
            Self::Item { handler: value, .. } | Self::ElementItem { handler: value, .. } => {
                *value = Some(Rc::new(handler));
            }
            _ => {}
        }
        self
    }

    fn is_clickable(&self) -> bool {
        matches!(
            self,
            Self::Item {
                disabled: false,
                ..
            } | Self::ElementItem {
                disabled: false,
                handler: Some(_),
                ..
            } | Self::Submenu {
                disabled: false,
                ..
            }
        )
    }

    fn is_separator(&self) -> bool {
        matches!(self, Self::Separator)
    }

    fn has_left_icon(&self, check_side: PopupMenuCheckSide) -> bool {
        match self {
            Self::Item { icon, checked, .. } | Self::ElementItem { icon, checked, .. } => {
                icon.is_some() || (check_side == PopupMenuCheckSide::Left && *checked)
            }
            Self::Submenu { icon, .. } => icon.is_some(),
            _ => false,
        }
    }
}

pub struct PopupMenu {
    focus_handle: FocusHandle,
    menu_items: Vec<PopupMenuItem>,
    action_context: Option<FocusHandle>,
    selected_index: Option<usize>,
    min_width: Option<Pixels>,
    max_width: Option<Pixels>,
    max_height: Option<Pixels>,
    bounds: Bounds<Pixels>,
    style: PopupMenuStyle,
    check_side: PopupMenuCheckSide,
    parent_menu: Option<WeakEntity<Self>>,
    scrollable: bool,
    scroll_handle: ScrollHandle,
    submenu_anchor: (Anchor, Pixels),
    rich_rows: bool,
}

impl PopupMenu {
    fn new(cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            menu_items: Vec::new(),
            action_context: None,
            selected_index: None,
            min_width: None,
            max_width: None,
            max_height: None,
            bounds: Bounds::default(),
            style: PopupMenuStyle::default(),
            check_side: PopupMenuCheckSide::default(),
            parent_menu: None,
            scrollable: false,
            scroll_handle: ScrollHandle::default(),
            submenu_anchor: (Anchor::TopLeft, Pixels::ZERO),
            rich_rows: false,
        }
    }

    pub fn build(
        window: &mut Window,
        cx: &mut App,
        build: impl FnOnce(Self, &mut Window, &mut Context<Self>) -> Self,
    ) -> Entity<Self> {
        cx.new(|cx| build(Self::new(cx), window, cx))
    }

    pub fn style(mut self, style: PopupMenuStyle) -> Self {
        self.style = style;
        self
    }

    pub fn action_context(mut self, handle: FocusHandle) -> Self {
        self.action_context = Some(handle);
        self
    }

    pub fn min_w(mut self, width: impl Into<Pixels>) -> Self {
        self.min_width = Some(width.into());
        self
    }

    pub fn max_w(mut self, width: impl Into<Pixels>) -> Self {
        self.max_width = Some(width.into());
        self
    }

    pub fn max_h(mut self, height: impl Into<Pixels>) -> Self {
        self.max_height = Some(height.into());
        self
    }

    pub fn scrollable(mut self, scrollable: bool) -> Self {
        self.scrollable = scrollable;
        self
    }

    pub fn check_side(mut self, side: PopupMenuCheckSide) -> Self {
        self.check_side = side;
        self
    }

    pub fn rich_rows(mut self, rich_rows: bool) -> Self {
        self.rich_rows = rich_rows;
        self
    }

    pub fn item(mut self, item: PopupMenuItem) -> Self {
        self.push_item(item);
        self
    }

    pub fn map_last_item(mut self, map: impl FnOnce(PopupMenuItem) -> PopupMenuItem) -> Self {
        if let Some(item) = self.menu_items.pop() {
            self.menu_items.push(map(item));
        }
        self
    }

    pub fn separator(mut self) -> Self {
        self.push_item(PopupMenuItem::Separator);
        self
    }

    pub fn replace_items(&mut self, items: impl IntoIterator<Item = PopupMenuItem>) {
        self.menu_items.clear();
        for item in items {
            self.push_item(item);
        }
        if self
            .selected_index
            .is_some_and(|ix| ix >= self.menu_items.len())
        {
            self.selected_index = None;
        }
    }

    fn estimated_content_height_px(&self) -> f32 {
        let mut item_count = 0usize;
        let mut height = 0.0;
        for (index, item) in self.menu_items.iter().enumerate() {
            if index + 1 == self.menu_items.len() && item.is_separator() {
                continue;
            }
            item_count += 1;
            height +=
                match item {
                    PopupMenuItem::Separator => 5.0,
                    PopupMenuItem::Label(_) => 26.0,
                    PopupMenuItem::Item { .. } | PopupMenuItem::Submenu { .. } => {
                        if self.rich_rows { 48.0 } else { 26.0 }
                    }
                    PopupMenuItem::ElementItem { .. } => {
                        if self.rich_rows {
                            48.0
                        } else {
                            26.0
                        }
                    }
                };
        }
        height + item_count.saturating_sub(1) as f32 * POPUP_MENU_ITEM_GAP_PX
    }

    pub fn set_style(&mut self, style: PopupMenuStyle) {
        self.style = style;
    }

    pub fn menu_focus_handle(&self) -> FocusHandle {
        self.focus_handle.clone()
    }

    fn push_item(&mut self, item: PopupMenuItem) {
        if item.is_separator()
            && (self.menu_items.is_empty()
                || self
                    .menu_items
                    .last()
                    .is_some_and(PopupMenuItem::is_separator))
        {
            return;
        }
        self.menu_items.push(item);
    }

    pub fn submenu(
        self,
        label: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
        build: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> Self {
        self.submenu_with_icon(None, label, window, cx, build)
    }

    pub fn submenu_with_icon(
        self,
        icon: Option<PopupMenuIcon>,
        label: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
        build: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> Self {
        self.submenu_with_icon_and_disabled(icon, label, false, window, cx, build)
    }

    pub fn submenu_with_icon_and_disabled(
        mut self,
        icon: Option<PopupMenuIcon>,
        label: impl Into<SharedString>,
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
        build: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> Self {
        let menu = PopupMenu::build(window, cx, build);
        let parent = cx.entity().downgrade();
        menu.update(cx, |menu, _| menu.parent_menu = Some(parent));
        self.menu_items.push(
            PopupMenuItem::submenu(label, menu)
                .disabled(disabled)
                .when_some(icon, PopupMenuItem::icon),
        );
        self
    }

    fn clickable_items(&self) -> impl DoubleEndedIterator<Item = usize> + '_ {
        self.menu_items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.is_clickable())
            .map(|(ix, _)| ix)
    }

    fn set_selected(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        if self.selected_index != index {
            self.selected_index = index;
            if let Some(index) = index {
                self.scroll_handle.scroll_to_item(index);
            }
            cx.notify();
        }
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        let next = self
            .clickable_items()
            .find(|index| self.selected_index.is_none_or(|selected| *index > selected))
            .or_else(|| self.clickable_items().next());
        self.set_selected(next, cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        let previous = self
            .clickable_items()
            .rev()
            .find(|index| {
                self.selected_index
                    .is_some_and(|selected| *index < selected)
            })
            .or_else(|| self.clickable_items().next_back());
        self.set_selected(previous, cx);
    }

    fn active_submenu(&self) -> Option<Entity<Self>> {
        self.selected_index
            .and_then(|index| match self.menu_items.get(index) {
                Some(PopupMenuItem::Submenu { menu, .. }) => Some(menu.clone()),
                _ => None,
            })
    }

    fn select_right(&mut self, _: &SelectRight, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(menu) = self.active_submenu() {
            menu.update(cx, |menu, cx| {
                let first = menu.clickable_items().next();
                menu.set_selected(first, cx);
                menu.focus_handle.focus(window, cx);
            });
        } else if self.parent_menu.is_none() {
            cx.propagate();
        }
    }

    fn select_left(&mut self, _: &SelectLeft, window: &mut Window, cx: &mut Context<Self>) {
        let Some(parent) = self.parent_menu.as_ref().and_then(WeakEntity::upgrade) else {
            cx.propagate();
            return;
        };
        self.selected_index = None;
        parent.update(cx, |menu, cx| {
            menu.focus_handle.focus(window, cx);
            cx.notify();
        });
    }

    fn on_click(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        window.prevent_default();
        self.selected_index = Some(index);
        self.confirm(&Confirm, window, cx);
    }

    fn confirm(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.selected_index else {
            return;
        };
        let Some(item) = self.menu_items.get(index) else {
            return;
        };
        match item {
            PopupMenuItem::Item {
                handler, action, ..
            } => {
                if let Some(handler) = handler {
                    handler(&ClickEvent::default(), window, cx);
                } else if let Some(action) = action {
                    if let Some(handle) = &self.action_context {
                        handle.focus(window, cx);
                    }
                    window.dispatch_action(action.boxed_clone(), cx);
                }
                self.dismiss(&Cancel, window, cx);
            }
            PopupMenuItem::ElementItem {
                handler: Some(handler),
                ..
            } => {
                handler(&ClickEvent::default(), window, cx);
                self.dismiss(&Cancel, window, cx);
            }
            _ => {}
        }
    }

    fn dismiss(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_submenu().is_some() {
            return;
        }
        cx.emit(DismissEvent);
        if let Some(handle) = &self.action_context {
            handle.focus(window, cx);
        }
        if let Some(parent) = self.parent_menu.as_ref().and_then(WeakEntity::upgrade) {
            parent.update(cx, |menu, cx| {
                menu.selected_index = None;
                menu.dismiss(&Cancel, window, cx);
            });
        }
    }

    fn on_mouse_down_out(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .parent_menu
            .as_ref()
            .and_then(WeakEntity::upgrade)
            .is_some_and(|parent| parent.read(cx).bounds.contains(&event.position))
        {
            return;
        }
        self.dismiss(&Cancel, window, cx);
    }

    fn update_submenu_anchor(&mut self, window: &Window) {
        let opens_left =
            self.max_width.unwrap_or(px(500.0)) + self.bounds.origin.x > window.bounds().size.width;
        let anchor = if opens_left {
            Anchor::TopRight
        } else {
            Anchor::TopLeft
        };
        let left = if opens_left {
            -px(16.0)
        } else {
            self.bounds.size.width - px(8.0)
        };
        self.submenu_anchor = (anchor, left);
    }

    fn render_icon(
        icon: &Option<PopupMenuIcon>,
        checked: bool,
        style: PopupMenuStyle,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        if let Some(icon) = icon {
            return div()
                .size(px(16.0))
                .child(icon.render(window, cx))
                .into_any_element();
        }
        div()
            .size(px(16.0))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(POPUP_MENU_ITEM_FONT_SIZE_PX))
            .text_color(style.foreground)
            .child(if checked { "✓" } else { "" })
            .into_any_element()
    }

    fn render_item(
        &self,
        index: usize,
        item: &PopupMenuItem,
        has_left_icon: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if item.is_separator() {
            return div()
                .my(px(2.0))
                .mx(px(-4.0))
                .h(px(1.0))
                .bg(self.style.border)
                .into_any_element();
        }

        if let PopupMenuItem::Label(label) = item {
            return div()
                .h(px(26.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .text_size(px(POPUP_MENU_LABEL_FONT_SIZE_PX))
                .text_color(self.style.muted_foreground)
                .child(label.clone())
                .into_any_element();
        }

        if let PopupMenuItem::ElementItem { render, .. } = item {
            return div().w_full().child(render(window, cx)).into_any_element();
        }

        let selected = self.selected_index == Some(index);
        let disabled = match item {
            PopupMenuItem::Item { disabled, .. } | PopupMenuItem::Submenu { disabled, .. } => {
                *disabled
            }
            _ => false,
        };
        let is_submenu = matches!(item, PopupMenuItem::Submenu { .. });
        let (icon, label, description, checked) = match item {
            PopupMenuItem::Item {
                icon,
                label,
                description,
                checked,
                ..
            } => (icon, label, description, *checked),
            PopupMenuItem::Submenu {
                icon,
                label,
                description,
                ..
            } => (icon, label, description, false),
            _ => unreachable!(),
        };
        let rich_rows = self.rich_rows;
        let mut row = div()
            .id(("popup-menu-item", index))
            .h(px(if rich_rows { 48.0 } else { 26.0 }))
            .w_full()
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(if rich_rows { 10.0 } else { 4.0 }))
            .rounded(self.style.radius)
            .text_size(px(POPUP_MENU_ITEM_FONT_SIZE_PX))
            .text_color(if disabled {
                self.style.muted_foreground
            } else if selected {
                self.style.selected_foreground
            } else {
                self.style.foreground
            })
            .when(selected && !disabled, |row| {
                row.bg(self.style.selected_background)
            })
            .when(!disabled, |row| {
                row.cursor_pointer()
                    .hover(|row| row.bg(self.style.selected_background))
                    .on_hover(cx.listener(move |menu, hovered, _, cx| {
                        if *hovered {
                            menu.set_selected(Some(index), cx);
                        } else if !is_submenu && menu.selected_index == Some(index) {
                            menu.set_selected(None, cx);
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |menu, _, window, cx| {
                            menu.on_click(index, window, cx);
                        }),
                    )
            });
        if has_left_icon {
            let icon = Self::render_icon(icon, checked, self.style, window, cx);
            row = row.child(if rich_rows {
                div()
                    .flex_none()
                    .size(px(36.0))
                    .rounded(self.style.radius)
                    .border_1()
                    .border_color(self.style.border)
                    .bg(self.style.background)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon)
                    .into_any_element()
            } else {
                icon
            });
        }
        row = row.child(if rich_rows {
            div()
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(1.0))
                .child(
                    div()
                        .text_size(px(POPUP_MENU_ITEM_FONT_SIZE_PX))
                        .child(label.clone()),
                )
                .when_some(description.clone(), |column, description| {
                    column.child(
                        div()
                            .text_size(px(POPUP_MENU_LABEL_FONT_SIZE_PX))
                            .text_color(self.style.muted_foreground)
                            .child(description),
                    )
                })
                .into_any_element()
        } else {
            div().flex_1().child(label.clone()).into_any_element()
        });
        if checked && self.check_side == PopupMenuCheckSide::Right {
            row = row.child(
                div()
                    .w(px(16.0))
                    .text_size(px(POPUP_MENU_ITEM_FONT_SIZE_PX))
                    .text_color(self.style.foreground)
                    .child("✓"),
            );
        }
        if let PopupMenuItem::Submenu { menu, .. } = item {
            row = row.child(
                SvgIcon::new("popup-menu-submenu-arrow", SUBMENU_ARROW)
                    .color(self.style.muted_foreground)
                    .size(px(16.0)),
            );
            if selected {
                let (anchor, left) = self.submenu_anchor;
                row = row.child(
                    anchored()
                        .anchor(anchor)
                        .child(
                            div()
                                .id("popup-submenu")
                                .top_neg_1()
                                .left(left)
                                .child(menu.clone()),
                        )
                        .snap_to_window_with_margin(Edges::all(px(4.0))),
                );
            }
        }
        row.into_any_element()
    }
}

impl FluentBuilder for PopupMenu {}
impl EventEmitter<DismissEvent> for PopupMenu {}

impl Focusable for PopupMenu {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for PopupMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.update_submenu_anchor(window);
        let item_count = self.menu_items.len();
        let has_left_icon = self
            .menu_items
            .iter()
            .any(|item| item.has_left_icon(self.check_side));
        let max_height = self.max_height.unwrap_or_else(|| {
            (window.window_bounds().get_bounds().size.height * 0.5).min(px(450.0))
        });
        let scroll_viewport_height = (f32::from(max_height) - POPUP_MENU_PADDING_PX * 2.0).max(1.0);
        let estimated_content_height = self.estimated_content_height_px();
        let show_scrollbar =
            self.scrollable && estimated_content_height > scroll_viewport_height + 0.5;
        let entity = cx.entity().clone();
        let content = div()
            .id("popup-menu-scroll-content")
            .w_full()
            .flex()
            .flex_col()
            .gap(px(POPUP_MENU_ITEM_GAP_PX))
            .when(self.scrollable, |content| {
                content
                    .max_h(px(scroll_viewport_height))
                    .when(show_scrollbar, |content| {
                        content.pr(px(POPUP_MENU_SCROLLBAR_WIDTH_PX))
                    })
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll_handle)
            })
            .children(
                self.menu_items
                    .iter()
                    .enumerate()
                    .filter(|(index, item)| !(*index + 1 == item_count && item.is_separator()))
                    .map(|(index, item)| self.render_item(index, item, has_left_icon, window, cx)),
            );
        div()
            .id("popup-menu")
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::dismiss))
            .on_mouse_down_out(cx.listener(Self::on_mouse_down_out))
            .min_w(rems(8.0))
            .when_some(self.min_width, |menu, width| menu.min_w(width))
            .max_w(self.max_width.unwrap_or(px(500.0)))
            .p(px(POPUP_MENU_PADDING_PX))
            .flex()
            .flex_col()
            .rounded(px(8.0))
            .border_1()
            .border_color(self.style.border)
            .bg(self.style.background)
            .text_color(self.style.foreground)
            .shadow_lg()
            .relative()
            .occlude()
            .when(self.scrollable, |menu| menu.max_h(max_height))
            .child(
                canvas(
                    move |bounds, _, cx| {
                        entity.update(cx, |menu, _| menu.bounds = bounds);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(content)
            .when(show_scrollbar, |menu| {
                menu.child(
                    div()
                        .absolute()
                        .top(px(POPUP_MENU_PADDING_PX))
                        .right_0()
                        .w(px(POPUP_MENU_SCROLLBAR_WIDTH_PX))
                        .h(px(scroll_viewport_height))
                        .child(InteractiveScrollbar::for_scroll_handle(
                            ScrollbarAxis::Vertical,
                            self.scroll_handle.clone(),
                            scroll_viewport_height,
                            estimated_content_height,
                            InteractiveScrollbarStyle::notion(
                                hsla_to_rgb(self.style.muted_foreground),
                                hsla_to_rgb(self.style.foreground),
                            ),
                        )),
                )
            })
    }
}

fn hsla_to_rgb(color: Hsla) -> u32 {
    let color = Rgba::from(color);
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
    (channel(color.r) << 16) | (channel(color.g) << 8) | channel(color.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn separators_are_deduplicated_and_trimmed(cx: &mut gpui::TestAppContext) {
        let menu = cx.new(|cx| {
            PopupMenu::new(cx)
                .separator()
                .item(PopupMenuItem::new("First"))
                .separator()
                .separator()
        });
        menu.read_with(cx, |menu, _| {
            assert_eq!(menu.menu_items.len(), 2);
            assert!(
                menu.menu_items
                    .last()
                    .is_some_and(PopupMenuItem::is_separator)
            );
        });
    }

    #[gpui::test]
    fn clickable_items_skip_labels_separators_and_disabled_rows(cx: &mut gpui::TestAppContext) {
        let menu = cx.new(|cx| {
            PopupMenu::new(cx)
                .item(PopupMenuItem::label("Label"))
                .item(PopupMenuItem::new("Disabled").disabled(true))
                .item(PopupMenuItem::new("Enabled"))
        });
        menu.read_with(cx, |menu, _| {
            assert_eq!(menu.clickable_items().collect::<Vec<_>>(), vec![2]);
        });
    }

    #[test]
    fn checked_rows_reserve_the_left_icon_column() {
        assert!(
            PopupMenuItem::new("Checked")
                .checked(true)
                .has_left_icon(PopupMenuCheckSide::Left)
        );
        assert!(
            !PopupMenuItem::new("Checked")
                .checked(true)
                .has_left_icon(PopupMenuCheckSide::Right)
        );
        assert!(!PopupMenuItem::new("Plain").has_left_icon(PopupMenuCheckSide::Left));
    }

    #[test]
    fn default_geometry_matches_upstream_popup_menu() {
        let style = PopupMenuStyle::default();
        assert_eq!(style.radius, px(6.0));
        assert_eq!(rems(8.0), rems(8.0));
        assert_eq!(px(500.0), px(500.0));
    }

    #[test]
    fn submenu_arrow_uses_the_shared_svg_asset() {
        assert!(
            std::str::from_utf8(SUBMENU_ARROW)
                .unwrap()
                .starts_with("<svg")
        );
    }

    #[test]
    fn popup_menu_font_scale_matches_editor_menu_rows() {
        assert_eq!(POPUP_MENU_ITEM_FONT_SIZE_PX, 14.0);
        assert_eq!(POPUP_MENU_LABEL_FONT_SIZE_PX, 11.0);
    }

    #[gpui::test]
    fn rich_secondary_menu_reports_overflow_for_a_primary_sized_viewport(
        cx: &mut gpui::TestAppContext,
    ) {
        let menu = cx.new(|cx| {
            let mut menu = PopupMenu::new(cx).rich_rows(true).scrollable(true);
            for index in 0..12 {
                menu = menu.item(PopupMenuItem::new(format!("Item {index}")));
            }
            menu
        });

        menu.read_with(cx, |menu, _| {
            const PRIMARY_SIZED_SCROLL_VIEWPORT_PX: f32 = 418.0;
            assert!(menu.scrollable);
            assert!(menu.estimated_content_height_px() > PRIMARY_SIZED_SCROLL_VIEWPORT_PX);
        });
    }

    #[test]
    fn rich_rows_are_opt_in_and_keep_descriptions() {
        let item = PopupMenuItem::new("Text").description("Plain text block");
        let PopupMenuItem::Item { description, .. } = item else {
            panic!("standard item")
        };
        assert_eq!(description.as_deref(), Some("Plain text block"));
    }
}
