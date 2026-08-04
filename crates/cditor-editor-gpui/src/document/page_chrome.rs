use cditor_component::{InteractiveScrollbar, InteractiveScrollbarStyle, ScrollbarAxis, SvgIcon};
use gpui::{
    AnyElement, AnyView, App, Entity, InteractiveElement, IntoElement, MouseButton, ObjectFit,
    ParentElement, ScrollHandle, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder, px, rgb,
};

use cditor_core::layout::BODY_BLOCK_CONTENT_WIDTH_PX;
use cditor_core::rich_text::{PageCover, PageIcon};
use cditor_editor_protocol::command::{CommandSource, EditorCommand};
#[cfg(not(target_family = "wasm"))]
use cditor_sdk::providers::{AssetInput, ImportedAsset};

use crate::app::worker_admission::EditorWorkerAdmission;
use crate::document::DocumentLayoutMetrics;
use crate::document::layout_metrics::PAGE_COVER_HEIGHT_PX;
use crate::editor_view::CditorV2View;
use crate::image_loader::{RasterImageElement, load_render_image};
use crate::theme::GuiTheme;

const PAGE_ICON_SIZE_PX: f32 = 72.0;
const PAGE_ICON_TOP_WITHOUT_COVER_PX: f32 = 36.0;
const PAGE_ICON_COVER_OVERLAP_PX: f32 = 36.0;
const PAGE_ACTIONS_TOP_WITHOUT_COVER_PX: f32 = 54.0;
const PAGE_ACTIONS_COVER_BOTTOM_GAP_PX: f32 = 12.0;
const PAGE_ACTION_HEIGHT_PX: f32 = 28.0;
const PAGE_ICON_MENU_WIDTH_PX: f32 = 300.0;
const PAGE_ICON_MENU_DESIRED_HEIGHT_PX: f32 = 400.0;
const PAGE_ICON_MENU_MIN_HEIGHT_PX: f32 = 160.0;
const PAGE_ICON_MENU_GAP_PX: f32 = 8.0;
const PAGE_ICON_MENU_PADDING_PX: f32 = 6.0;
const PAGE_ICON_CELL_SIZE_PX: f32 = 30.0;
const PAGE_ICON_CELL_GAP_PX: f32 = 4.0;
const PAGE_ICON_GRID_ROW_HEIGHT_PX: f32 = PAGE_ICON_CELL_SIZE_PX + PAGE_ICON_CELL_GAP_PX;
const PAGE_ICON_TAB_BAR_HEIGHT_PX: f32 = 34.0;
const PAGE_ICON_MENU_ESTIMATED_CONTENT_HEIGHT_PX: f32 =
    PAGE_ICON_TAB_BAR_HEIGHT_PX + 9.0 * PAGE_ICON_GRID_ROW_HEIGHT_PX;

const SYSTEM_EMOJIS: &[&str] = &[
    "😀", "😄", "😁", "😂", "🤣", "😊", "😍", "🤩", "😎", "🤔", "😴", "🥳", "😢", "😭", "😡", "👍",
    "👎", "👏", "🙏", "💪", "🤝", "✌️", "👀", "🧠", "💡", "📚", "📝", "📅", "📌", "✏️", "💻", "📱",
    "🎯", "🏆", "⭐", "🔥", "✅", "❌", "❗", "❓", "❤️", "💛", "💚", "💙", "💜", "🌈", "☀️", "🌙",
    "🍀", "🌸", "🚀", "🎉",
];

struct CustomIcon {
    path: &'static str,
    key: &'static str,
    bytes: &'static [u8],
}

macro_rules! custom_icon {
    ($name:literal) => {
        CustomIcon {
            path: concat!("icons/", $name, ".svg"),
            key: concat!("custom-icon-", $name),
            bytes: include_bytes!(concat!("../../../../assets/icons/", $name, ".svg")),
        }
    };
}

const CUSTOM_ICONS: &[CustomIcon] = &[
    custom_icon!("ai-expand"),
    custom_icon!("ai-explain"),
    custom_icon!("ai-improve"),
    custom_icon!("ai-proofread"),
    custom_icon!("ai-shorten"),
    custom_icon!("ai-translate"),
    custom_icon!("bold"),
    custom_icon!("bulb"),
    custom_icon!("bulleted-list"),
    custom_icon!("callout"),
    custom_icon!("check"),
    custom_icon!("chevron-down"),
    custom_icon!("circle-x"),
    custom_icon!("code"),
    custom_icon!("color"),
    custom_icon!("copy"),
    custom_icon!("copy-check"),
    custom_icon!("cuttion"),
    custom_icon!("delete"),
    custom_icon!("divider"),
    custom_icon!("fullscreen"),
    custom_icon!("gutter"),
    custom_icon!("gutter-horizontal"),
    custom_icon!("heading-1"),
    custom_icon!("heading-2"),
    custom_icon!("heading-3"),
    custom_icon!("important"),
    custom_icon!("inlie-code"),
    custom_icon!("inline-ai"),
    custom_icon!("itaic"),
    custom_icon!("jiantou"),
    custom_icon!("math"),
    custom_icon!("mermaid"),
    custom_icon!("minisize"),
    custom_icon!("note"),
    custom_icon!("number-list"),
    custom_icon!("quote"),
    custom_icon!("search"),
    custom_icon!("strikethrough"),
    custom_icon!("table"),
    custom_icon!("text"),
    custom_icon!("theme"),
    custom_icon!("todo"),
    custom_icon!("underline"),
    custom_icon!("warning"),
    custom_icon!("whiteboard"),
];

fn custom_icon_for_source(source: &str) -> Option<(&'static str, &'static [u8])> {
    CUSTOM_ICONS
        .iter()
        .find(|icon| icon.path == source)
        .map(|icon| (icon.key, icon.bytes))
}

/// Resolves a built-in page icon asset to its embedded SVG bytes.
///
/// Hosts can reuse the same icon family when rendering a document's icon
/// outside the editor (for example in a workspace tree).
pub fn custom_page_icon_asset(source: &str) -> Option<(&'static str, &'static [u8])> {
    custom_icon_for_source(source)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PageDecorationSnapshot {
    pub cover: Option<PageCover>,
    pub icon: Option<PageIcon>,
    pub revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PageChromeGeometry {
    content_left_px: f32,
    icon_top_px: f32,
    actions_top_px: f32,
}

impl PageChromeGeometry {
    fn new(viewport_width_px: f32, layout: DocumentLayoutMetrics, has_cover: bool) -> Self {
        let page_left_px = ((viewport_width_px - layout.page_width_px) / 2.0).max(0.0);
        let available_width = (layout.page_width_px - 96.0).max(1.0);
        let body_width = (BODY_BLOCK_CONTENT_WIDTH_PX as f32).min(available_width);
        let content_left_px = page_left_px + (layout.page_width_px - body_width) / 2.0;
        Self {
            content_left_px,
            icon_top_px: if has_cover {
                PAGE_COVER_HEIGHT_PX - PAGE_ICON_COVER_OVERLAP_PX
            } else {
                PAGE_ICON_TOP_WITHOUT_COVER_PX
            },
            actions_top_px: if has_cover {
                PAGE_COVER_HEIGHT_PX + PAGE_ACTIONS_COVER_BOTTOM_GAP_PX
            } else {
                PAGE_ACTIONS_TOP_WITHOUT_COVER_PX
            },
        }
    }
}

pub(crate) fn render_page_chrome(
    decorations: &PageDecorationSnapshot,
    viewport_width_px: f32,
    viewport_height_px: f32,
    layout: DocumentLayoutMetrics,
    scroll_top: f64,
    readonly: bool,
    page_chrome_extras: Option<AnyView>,
    theme: GuiTheme,
    workers: &EditorWorkerAdmission,
    asset_provider: Option<std::sync::Arc<dyn cditor_sdk::providers::AssetProvider>>,
    view: Entity<CditorV2View>,
    page_icon_menu_open: bool,
    page_icon_menu_custom_tab: bool,
    page_icon_menu_scroll_handle: ScrollHandle,
    cx: &mut App,
) -> (AnyElement, Option<AnyElement>) {
    let has_cover = decorations.cover.is_some();
    let geometry = PageChromeGeometry::new(viewport_width_px, layout, has_cover);
    let chrome_height_px = (geometry.icon_top_px + PAGE_ICON_SIZE_PX)
        .max(geometry.actions_top_px + PAGE_ACTION_HEIGHT_PX)
        .max(if has_cover { PAGE_COVER_HEIGHT_PX } else { 0.0 });
    let mut chrome = div()
        .id("cditor-page-chrome")
        .group("cditor-page-chrome")
        .absolute()
        .left_0()
        .right_0()
        .top(px(-(scroll_top as f32)))
        .h(px(chrome_height_px));
    let mut page_icon_menu = None;

    if let Some(cover) = decorations.cover.as_ref() {
        chrome = chrome.child(render_cover(
            cover,
            decorations.revision,
            theme,
            workers,
            asset_provider.clone(),
            view.clone(),
            cx,
        ));
    }

    if let Some(icon) = decorations.icon.as_ref() {
        chrome = chrome.child(render_icon(
            icon,
            geometry,
            decorations.revision,
            theme,
            workers,
            asset_provider,
            view.clone(),
            cx,
        ));
    }

    if let Some(actions) = render_page_actions(
        decorations,
        geometry,
        viewport_width_px,
        theme,
        view.clone(),
        page_chrome_extras,
        readonly,
    ) {
        chrome = chrome.child(actions);
    }
    if !readonly && page_icon_menu_open {
        let actions_left_px = page_actions_left_px(geometry, decorations.icon.is_some());
        page_icon_menu = Some(render_page_icon_menu(
            actions_left_px,
            geometry.actions_top_px,
            viewport_width_px,
            viewport_height_px,
            theme,
            view,
            page_icon_menu_custom_tab,
            page_icon_menu_scroll_handle,
        ));
    }

    (chrome.into_any_element(), page_icon_menu)
}

fn render_cover(
    cover: &PageCover,
    revision: u64,
    theme: GuiTheme,
    workers: &EditorWorkerAdmission,
    asset_provider: Option<std::sync::Arc<dyn cditor_sdk::providers::AssetProvider>>,
    view: Entity<CditorV2View>,
    cx: &mut App,
) -> AnyElement {
    let (source, position_y) = match cover {
        PageCover::External { url, position_y } => (url.as_str(), position_y.ratio()),
        PageCover::Asset { asset, position_y } => (asset.source.as_str(), position_y.ratio()),
    };
    let image = load_render_image(source, 0, revision, workers, asset_provider, view, cx);
    div()
        .id("cditor-page-cover")
        .absolute()
        .left_0()
        .right_0()
        .top_0()
        .h(px(PAGE_COVER_HEIGHT_PX))
        .overflow_hidden()
        .bg(rgb(theme.skeleton))
        .when_some(image, |this, image| {
            this.child(
                RasterImageElement::new(image, ObjectFit::Cover, px(0.0))
                    .with_cover_position_y(position_y),
            )
        })
        .into_any_element()
}

fn render_icon(
    icon: &PageIcon,
    geometry: PageChromeGeometry,
    revision: u64,
    theme: GuiTheme,
    workers: &EditorWorkerAdmission,
    asset_provider: Option<std::sync::Arc<dyn cditor_sdk::providers::AssetProvider>>,
    view: Entity<CditorV2View>,
    cx: &mut App,
) -> AnyElement {
    let base = div()
        .id("cditor-page-icon")
        .absolute()
        .left(px(geometry.content_left_px))
        .top(px(geometry.icon_top_px))
        .size(px(PAGE_ICON_SIZE_PX))
        .flex()
        .items_center()
        .justify_center();
    match icon {
        PageIcon::Emoji { emoji } => base
            .text_size(px(64.0))
            .line_height(px(PAGE_ICON_SIZE_PX))
            .child(emoji.clone())
            .into_any_element(),
        PageIcon::Asset { asset } => {
            if let Some((key, bytes)) = custom_icon_for_source(&asset.source) {
                base.rounded(px(6.0))
                    .overflow_hidden()
                    .child(
                        SvgIcon::new(key, bytes)
                            .color(rgb(theme.text))
                            .size(px(PAGE_ICON_SIZE_PX)),
                    )
                    .into_any_element()
            } else {
                let image = load_render_image(
                    &asset.source,
                    0,
                    revision,
                    workers,
                    asset_provider,
                    view,
                    cx,
                );
                base.rounded(px(6.0))
                    .overflow_hidden()
                    .bg(rgb(theme.skeleton))
                    .when_some(image, |this, image| {
                        this.child(RasterImageElement::new(image, ObjectFit::Contain, px(6.0)))
                    })
                    .into_any_element()
            }
        }
    }
}

fn page_actions_left_px(geometry: PageChromeGeometry, has_icon: bool) -> f32 {
    geometry.content_left_px
        + if has_icon {
            PAGE_ICON_SIZE_PX + 8.0
        } else {
            0.0
        }
}

fn render_page_actions(
    decorations: &PageDecorationSnapshot,
    geometry: PageChromeGeometry,
    viewport_width_px: f32,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    page_chrome_extras: Option<AnyView>,
    readonly: bool,
) -> Option<AnyElement> {
    let actions_left_px = page_actions_left_px(geometry, decorations.icon.is_some());
    if readonly && page_chrome_extras.is_none() {
        return None;
    }
    let row_width_px = (viewport_width_px - actions_left_px - 24.0).max(0.0);
    let row = div()
        .id("cditor-page-decoration-actions")
        .absolute()
        .left(px(actions_left_px))
        .top(px(geometry.actions_top_px))
        .w(px(row_width_px))
        .h(px(PAGE_ACTION_HEIGHT_PX))
        .flex()
        .items_center()
        .gap_1()
        .opacity(0.0)
        .group_hover("cditor-page-chrome", |style| style.opacity(1.0))
        .hover(|style| style.opacity(1.0))
        .when(!readonly, |row| {
            row.children(action_buttons(decorations, theme, view))
        })
        .when_some(page_chrome_extras, |row, extra| {
            row.child(div().flex_1().min_w(px(0.0)).child(extra))
        });
    Some(row.into_any_element())
}

fn action_buttons(
    decorations: &PageDecorationSnapshot,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> Vec<AnyElement> {
    let mut buttons = Vec::new();
    if decorations.icon.is_none() {
        buttons.push(page_action_button(
            "cditor-add-page-icon",
            "添加图标",
            theme,
            {
                let view = view.clone();
                move |_window, cx| open_page_icon_menu(&view, cx)
            },
        ));
    } else {
        buttons.push(page_action_button(
            "cditor-change-page-icon",
            "更换图标",
            theme,
            {
                let view = view.clone();
                move |_window, cx| open_page_icon_menu(&view, cx)
            },
        ));
        buttons.push(page_action_button(
            "cditor-remove-page-icon",
            "移除图标",
            theme,
            {
                let view = view.clone();
                move |_window, cx| dispatch_page_icon(&view, None, cx)
            },
        ));
    }
    buttons.push(page_action_button(
        "cditor-page-cover-action",
        if decorations.cover.is_some() {
            "更换封面"
        } else {
            "添加封面"
        },
        theme,
        {
            let view = view.clone();
            move |_window, cx| {
                view.update(cx, |view, cx| view.choose_page_cover(cx));
            }
        },
    ));
    if decorations.cover.is_some() {
        buttons.push(page_action_button(
            "cditor-remove-page-cover",
            "移除封面",
            theme,
            move |_window, cx| dispatch_page_cover(&view, None, cx),
        ));
    }
    buttons
}

fn page_action_button(
    id: &'static str,
    label: &'static str,
    theme: GuiTheme,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .h(px(PAGE_ACTION_HEIGHT_PX))
        .px_2()
        .flex()
        .items_center()
        .rounded(px(4.0))
        .text_size(px(12.0))
        .text_color(rgb(theme.text))
        .bg(rgb(theme.hover_surface))
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(theme.action_hover_background)))
        .on_click(move |_event, window, cx| on_click(window, cx))
        .child(label)
        .into_any_element()
}

fn open_page_icon_menu(view: &Entity<CditorV2View>, cx: &mut App) {
    view.update(cx, |view, cx| {
        view.overlay.page_icon_menu_open = !view.overlay.page_icon_menu_open;
        if view.overlay.page_icon_menu_open {
            view.overlay
                .page_icon_menu_scroll_handle
                .set_offset(gpui::point(gpui::px(0.0), gpui::px(0.0)));
        }
        cx.notify();
    });
}

fn set_page_icon_menu_tab(view: &Entity<CditorV2View>, custom_tab: bool, cx: &mut App) {
    view.update(cx, |view, cx| {
        if view.overlay.page_icon_menu_custom_tab == custom_tab {
            return;
        }
        view.overlay.page_icon_menu_custom_tab = custom_tab;
        view.overlay
            .page_icon_menu_scroll_handle
            .set_offset(gpui::point(gpui::px(0.0), gpui::px(0.0)));
        cx.notify();
    });
}

fn dispatch_page_icon(view: &Entity<CditorV2View>, emoji: Option<String>, cx: &mut App) {
    view.update(cx, |view, cx| {
        let _ = view.dispatch_command(
            EditorCommand::SetPageIconEmoji { emoji },
            CommandSource::Toolbar,
            cx,
        );
    });
}

fn pick_page_icon_emoji(view: &Entity<CditorV2View>, emoji: String, cx: &mut App) {
    view.update(cx, |view, cx| {
        view.overlay.page_icon_menu_open = false;
        let _ = view.dispatch_command(
            EditorCommand::SetPageIconEmoji { emoji: Some(emoji) },
            CommandSource::Toolbar,
            cx,
        );
    });
}

fn pick_page_icon_asset(view: &Entity<CditorV2View>, source: &'static str, cx: &mut App) {
    view.update(cx, |view, cx| {
        view.overlay.page_icon_menu_open = false;
        let _ = view.dispatch_command(
            EditorCommand::SetPageIconAsset {
                source: Some(source.to_owned()),
            },
            CommandSource::Toolbar,
            cx,
        );
    });
}

fn dispatch_page_cover(view: &Entity<CditorV2View>, source: Option<String>, cx: &mut App) {
    view.update(cx, |view, cx| {
        let _ = view.dispatch_command(
            EditorCommand::SetPageCover {
                source,
                position_y_milli: 500,
            },
            CommandSource::Toolbar,
            cx,
        );
    });
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PageIconMenuGeometry {
    left_px: f32,
    top_px: f32,
    height_px: f32,
}

fn page_icon_menu_geometry(
    actions_left_px: f32,
    actions_top_px: f32,
    viewport_width_px: f32,
    viewport_height_px: f32,
) -> PageIconMenuGeometry {
    let left = actions_left_px.clamp(
        10.0,
        (viewport_width_px - PAGE_ICON_MENU_WIDTH_PX - 10.0).max(10.0),
    );
    let available_height = (viewport_height_px - 20.0).max(1.0);
    let height = PAGE_ICON_MENU_DESIRED_HEIGHT_PX
        .min(available_height)
        .max(PAGE_ICON_MENU_MIN_HEIGHT_PX.min(available_height));
    let top = (actions_top_px + PAGE_ACTION_HEIGHT_PX + PAGE_ICON_MENU_GAP_PX)
        .min((viewport_height_px - height - 10.0).max(10.0));
    PageIconMenuGeometry {
        left_px: left,
        top_px: top,
        height_px: height,
    }
}

fn render_page_icon_menu(
    actions_left_px: f32,
    actions_top_px: f32,
    viewport_width_px: f32,
    viewport_height_px: f32,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    custom_tab: bool,
    scroll_handle: ScrollHandle,
) -> AnyElement {
    let geometry = page_icon_menu_geometry(
        actions_left_px,
        actions_top_px,
        viewport_width_px,
        viewport_height_px,
    );
    let content_view = view.clone();
    let grid = if custom_tab {
        render_custom_icon_grid(theme, view.clone())
    } else {
        render_system_emoji_grid(theme, view.clone())
    };
    let content = div()
        .w_full()
        .h_full()
        .flex()
        .flex_col()
        .child(render_page_icon_tab_bar(theme, view, custom_tab))
        .child(
            div()
                .id("cditor-page-icon-menu-scroll")
                .flex_1()
                .w_full()
                .pr(px(8.0))
                .overflow_y_scroll()
                .track_scroll(&scroll_handle)
                .on_scroll_wheel(move |_event, _window, cx| {
                    content_view.update(cx, |_view, cx| cx.notify());
                })
                .child(grid),
        );

    div()
        .id("cditor-page-icon-menu")
        .absolute()
        .left(px(geometry.left_px))
        .top(px(geometry.top_px))
        .w(px(PAGE_ICON_MENU_WIDTH_PX))
        .h(px(geometry.height_px))
        .p(px(PAGE_ICON_MENU_PADDING_PX))
        .rounded(px(9.0))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.panel))
        .shadow_lg()
        .occlude()
        .overflow_hidden()
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .child(content)
        .child(render_menu_scrollbar(
            scroll_handle,
            geometry.height_px - PAGE_ICON_MENU_PADDING_PX * 2.0 - PAGE_ICON_TAB_BAR_HEIGHT_PX,
            theme,
        ))
        .into_any_element()
}

fn render_page_icon_tab_bar(
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    custom_tab: bool,
) -> AnyElement {
    div()
        .w_full()
        .h(px(PAGE_ICON_TAB_BAR_HEIGHT_PX))
        .mb(px(PAGE_ICON_MENU_GAP_PX))
        .p(px(2.0))
        .flex()
        .gap(px(2.0))
        .rounded(px(7.0))
        .bg(rgb(theme.surface))
        .child(page_icon_tab_button(
            "cditor-page-icon-tab-system",
            "系统表情",
            !custom_tab,
            theme,
            {
                let view = view.clone();
                move |_window, cx| set_page_icon_menu_tab(&view, false, cx)
            },
        ))
        .child(page_icon_tab_button(
            "cditor-page-icon-tab-custom",
            "自定义表情",
            custom_tab,
            theme,
            {
                let view = view.clone();
                move |_window, cx| set_page_icon_menu_tab(&view, true, cx)
            },
        ))
        .into_any_element()
}

fn page_icon_tab_button(
    id: &'static str,
    label: &'static str,
    active: bool,
    theme: GuiTheme,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .flex_1()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.0))
        .text_size(px(12.0))
        .text_color(rgb(if active { theme.text } else { theme.muted }))
        .bg(rgb(if active {
            theme.hover_surface
        } else {
            theme.surface
        }))
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(theme.hover_surface)))
        .on_click(move |_event, window, cx| on_click(window, cx))
        .child(label)
        .into_any_element()
}

fn render_system_emoji_grid(theme: GuiTheme, view: Entity<CditorV2View>) -> AnyElement {
    div()
        .w_full()
        .flex()
        .flex_wrap()
        .gap(px(PAGE_ICON_CELL_GAP_PX))
        .children(SYSTEM_EMOJIS.iter().enumerate().map(|(index, emoji)| {
            let view = view.clone();
            let emoji = *emoji;
            div()
                .id(("page-icon-system-emoji", index))
                .size(px(PAGE_ICON_CELL_SIZE_PX))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .text_size(px(18.0))
                .cursor_pointer()
                .hover(|style| style.bg(rgb(theme.hover_surface)))
                .on_click(move |_event, _window, cx| {
                    pick_page_icon_emoji(&view, emoji.to_owned(), cx);
                    cx.stop_propagation();
                })
                .child(emoji)
                .into_any_element()
        }))
        .into_any_element()
}

fn render_custom_icon_grid(theme: GuiTheme, view: Entity<CditorV2View>) -> AnyElement {
    div()
        .w_full()
        .flex()
        .flex_wrap()
        .gap(px(PAGE_ICON_CELL_GAP_PX))
        .children(CUSTOM_ICONS.iter().enumerate().map(|(index, icon)| {
            let view = view.clone();
            let (key, bytes) = (icon.key, icon.bytes);
            let path = icon.path;
            div()
                .id(("page-icon-custom-icon", index))
                .size(px(PAGE_ICON_CELL_SIZE_PX))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .cursor_pointer()
                .hover(|style| style.bg(rgb(theme.hover_surface)))
                .on_click(move |_event, _window, cx| {
                    pick_page_icon_asset(&view, path, cx);
                    cx.stop_propagation();
                })
                .child(
                    SvgIcon::new(key, bytes)
                        .color(rgb(theme.text))
                        .size(px(20.0)),
                )
                .into_any_element()
        }))
        .into_any_element()
}

fn render_menu_scrollbar(
    scroll_handle: ScrollHandle,
    track_height: f32,
    theme: GuiTheme,
) -> AnyElement {
    let max_offset = f32::from(scroll_handle.max_offset().y)
        .max((PAGE_ICON_MENU_ESTIMATED_CONTENT_HEIGHT_PX - track_height).max(0.0));
    if max_offset <= 0.5 || track_height <= 0.5 {
        return div().into_any_element();
    }
    div()
        .absolute()
        .top(px(PAGE_ICON_MENU_PADDING_PX))
        .right_0()
        .w(px(10.0))
        .h(px(track_height))
        .child(InteractiveScrollbar::for_scroll_handle(
            ScrollbarAxis::Vertical,
            scroll_handle,
            track_height,
            PAGE_ICON_MENU_ESTIMATED_CONTENT_HEIGHT_PX,
            InteractiveScrollbarStyle::notion(theme.scrollbar, theme.scrollbar_hover),
        ))
        .into_any_element()
}

impl CditorV2View {
    fn choose_page_cover(&mut self, cx: &mut gpui::Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let asset_provider = self.features.asset_provider.clone();
            let background = cx.background_executor().clone();
            cx.spawn(async move |view, cx| {
                let Some(file) = rfd::AsyncFileDialog::new()
                    .set_title("选择页面封面")
                    .add_filter("图片", &["png", "jpg", "jpeg", "webp", "gif"])
                    .pick_file()
                    .await
                else {
                    return;
                };
                let command = if let Some(provider) = asset_provider {
                    let input = page_cover_asset_input(file.file_name(), file.read().await);
                    match background
                        .spawn(
                            async move { crate::provider_io::import_asset(provider, input).await },
                        )
                        .await
                    {
                        Ok(imported) => imported_page_cover_command(imported),
                        Err(error) => {
                            let _ = view.update(cx, |view, cx| {
                                crate::overlays::show_toast(
                                    view,
                                    format!("Failed to import page cover: {error}"),
                                    std::time::Duration::from_secs(5),
                                    cx,
                                );
                            });
                            return;
                        }
                    }
                } else {
                    set_page_cover_command(file.path())
                };
                let _ = view.update(cx, |view, cx| {
                    if let Err(error) = view.dispatch_command(command, CommandSource::Toolbar, cx) {
                        crate::overlays::show_toast(
                            view,
                            format!("Failed to set page cover: {error}"),
                            std::time::Duration::from_secs(5),
                            cx,
                        );
                    }
                });
            })
            .detach();
        }
    }
}

#[cfg(not(target_family = "wasm"))]
fn page_cover_asset_input(file_name: String, bytes: Vec<u8>) -> AssetInput {
    let file_name = if file_name.trim().is_empty() {
        "cover".to_owned()
    } else {
        file_name
    };
    AssetInput {
        media_type: image_media_type_for_name(&file_name).map(ToOwned::to_owned),
        name: file_name,
        bytes,
    }
}

#[cfg(not(target_family = "wasm"))]
fn imported_page_cover_command(imported: ImportedAsset) -> EditorCommand {
    EditorCommand::SetPageCover {
        source: Some(imported.reference.source),
        position_y_milli: 500,
    }
}

#[cfg(not(target_family = "wasm"))]
fn image_media_type_for_name(file_name: &str) -> Option<&'static str> {
    match std::path::Path::new(file_name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => Some("image/png"),
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        Some("webp") => Some("image/webp"),
        Some("gif") => Some("image/gif"),
        _ => None,
    }
}

#[cfg(not(target_family = "wasm"))]
fn set_page_cover_command(path: &std::path::Path) -> EditorCommand {
    EditorCommand::SetPageCover {
        source: Some(path.to_string_lossy().into_owned()),
        position_y_milli: 500,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext, Context, Render, TestAppContext, Window};

    #[derive(Clone)]
    struct TagBarExtra;

    impl Render for TagBarExtra {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            "tag-extra"
        }
    }

    #[test]
    fn page_chrome_geometry_centers_on_the_body_track() {
        let layout = DocumentLayoutMetrics::for_viewport(1_440.0);
        let plain = PageChromeGeometry::new(1_440.0, layout, false);
        let covered = PageChromeGeometry::new(1_440.0, layout, true);

        assert_eq!(plain.content_left_px, 320.0);
        assert_eq!(plain.icon_top_px, 36.0);
        assert_eq!(covered.icon_top_px, 184.0);
        assert_eq!(covered.actions_top_px, 232.0);
    }

    #[cfg(not(target_family = "wasm"))]
    #[test]
    fn selected_cover_path_is_preserved_for_the_page_cover_command() {
        let command = set_page_cover_command(std::path::Path::new(
            r"C:\Users\Aurin\Pictures\cover image.png",
        ));
        let EditorCommand::SetPageCover {
            source,
            position_y_milli,
        } = command
        else {
            panic!("cover picker produced the wrong editor command");
        };
        assert_eq!(
            source.as_deref(),
            Some(r"C:\Users\Aurin\Pictures\cover image.png")
        );
        assert_eq!(position_y_milli, 500);
    }

    #[cfg(not(target_family = "wasm"))]
    #[test]
    fn imported_cover_uses_the_managed_asset_source() {
        use cditor_core::edit::{AssetSnapshot, AssetState};
        use cditor_core::rich_text::AssetRef;

        let command = imported_page_cover_command(ImportedAsset {
            reference: AssetRef {
                source: "assets/0123456789abcdef.png".to_owned(),
                media_type: Some("image/png".to_owned()),
                name: Some("cover.png".to_owned()),
                size_bytes: Some(4),
            },
            snapshot: AssetSnapshot {
                asset_id: 1,
                file_name: "cover.png".to_owned(),
                media_type: "image/png".to_owned(),
                size_bytes: 4,
                source: "assets/0123456789abcdef.png".to_owned(),
                checksum: Some("0".repeat(64)),
                state: AssetState::Ready,
            },
        });
        let EditorCommand::SetPageCover { source, .. } = command else {
            panic!("managed page cover produced the wrong editor command");
        };
        assert_eq!(source.as_deref(), Some("assets/0123456789abcdef.png"));
    }

    #[cfg(not(target_family = "wasm"))]
    #[test]
    fn page_cover_asset_input_keeps_file_metadata() {
        let input = page_cover_asset_input("Cover.JPEG".to_owned(), vec![1, 2, 3]);
        assert_eq!(input.name, "Cover.JPEG");
        assert_eq!(input.media_type.as_deref(), Some("image/jpeg"));
        assert_eq!(input.bytes, vec![1, 2, 3]);
    }

    #[test]
    fn page_icon_menu_geometry_clamps_inside_the_viewport() {
        let wide = page_icon_menu_geometry(320.0, 54.0, 1_440.0, 900.0);
        assert_eq!(wide.left_px, 320.0);
        assert_eq!(wide.top_px, 90.0);
        assert_eq!(wide.height_px, PAGE_ICON_MENU_DESIRED_HEIGHT_PX);

        let narrow = page_icon_menu_geometry(600.0, 100.0, 240.0, 300.0);
        assert_eq!(narrow.left_px, 10.0);
        assert_eq!(narrow.top_px, 10.0);
        assert_eq!(narrow.height_px, 280.0);
    }

    #[test]
    fn custom_icon_for_source_resolves_embedded_svg_icons() {
        assert!(custom_icon_for_source("icons/theme.svg").is_some());
        assert!(custom_icon_for_source("icons/fullscreen.svg").is_some());
        assert!(custom_icon_for_source("icons/missing.svg").is_none());
    }

    #[gpui::test]
    fn page_actions_render_host_extras_alongside_icon_actions(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_window, cx| {
            CditorV2View::from_runtime_with_options(
                cditor_runtime::DocumentRuntime::demo(),
                false,
                false,
                cx,
            )
        });
        let extra = cx.new(|_cx| TagBarExtra);
        let layout = DocumentLayoutMetrics::for_viewport(1_440.0);
        let geometry = PageChromeGeometry::new(1_440.0, layout, false);
        let decorations = PageDecorationSnapshot::default();

        assert!(
            render_page_actions(
                &decorations,
                geometry,
                1_440.0,
                GuiTheme::light(),
                view.clone(),
                None,
                true,
            )
            .is_none()
        );
        assert!(
            render_page_actions(
                &decorations,
                geometry,
                1_440.0,
                GuiTheme::light(),
                view.clone(),
                Some(AnyView::from(extra)),
                true,
            )
            .is_some()
        );
        assert!(
            render_page_actions(
                &decorations,
                geometry,
                1_440.0,
                GuiTheme::light(),
                view,
                None,
                false,
            )
            .is_some()
        );
    }

    #[gpui::test]
    fn page_icon_menu_tabs_render_system_and_custom_sections(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_window, cx| {
            CditorV2View::from_runtime_with_options(
                cditor_runtime::DocumentRuntime::demo(),
                false,
                false,
                cx,
            )
        });
        view.update(cx, |view, cx| {
            view.overlay.page_icon_menu_open = true;
            cx.notify();
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        view.update(cx, |view, cx| {
            view.overlay.page_icon_menu_custom_tab = true;
            cx.notify();
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
    }
}
