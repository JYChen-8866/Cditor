use std::path::PathBuf;

use gpui::{
    AnyElement, AppContext, ClipboardItem, Context, Image, ImageFormat, InteractiveElement,
    IntoElement, ParentElement, SharedString, StatefulInteractiveElement, Styled, div, px, rgb,
};

use super::DrafftBoardView;
use crate::paint;
use crate::theme::chrome;
use crate::{parse_document, parse_library};

impl DrafftBoardView {
    pub(super) fn render_file_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = chrome(cx);
        let trigger = div()
            .id("drafft-file-menu-trigger")
            .absolute()
            .left(px(12.0))
            .top(px(12.0))
            .w(px(32.0))
            .h(px(32.0))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(3.0))
            .rounded(px(6.0))
            .bg(if self.file_menu_open {
                rgb(c.accent)
            } else {
                rgb(c.bg)
            })
            .border_1()
            .border_color(rgb(c.border))
            .hover(|style| style.bg(rgb(c.hover)))
            .on_click(cx.listener(|view, _, _, cx| {
                view.file_menu_open = !view.file_menu_open;
                view.shortcuts_open = false;
                cx.notify();
            }))
            .children((0..3).map(|_| {
                div().w(px(14.0)).h(px(1.5)).bg(if self.file_menu_open {
                    rgb(c.on_accent)
                } else {
                    rgb(c.text)
                })
            }));

        let mut root = div().child(trigger);
        if self.file_menu_open {
            root = root.child(self.file_menu_panel(cx));
        }
        if self.shortcuts_open {
            root = root.child(self.shortcuts_panel(cx));
        }
        if let Some(status) = &self.file_status {
            root = root.child(
                div()
                    .absolute()
                    .left(px(12.0))
                    .bottom(px(16.0))
                    .px(px(8.0))
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .rounded(px(4.0))
                    .bg(rgb(c.bg))
                    .border_1()
                    .border_color(rgb(c.border))
                    .text_size(px(11.0))
                    .text_color(rgb(c.text_muted))
                    .child(status.clone()),
            );
        }
        root.into_any_element()
    }

    fn file_menu_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = chrome(cx);
        div()
            .absolute()
            .left(px(12.0))
            .top(px(52.0))
            .w(px(210.0))
            .p(px(6.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(rgb(c.border))
            .bg(rgb(c.bg))
            .shadow_sm()
            .occlude()
            .child(self.menu_item("Save", "Cmd+S", cx, |view, cx| {
                view.save_document(false, cx)
            }))
            .child(self.menu_item("Save As...", "", cx, |view, cx| {
                view.save_document(true, cx)
            }))
            .child(separator(c.border))
            .child(self.menu_item("Open...", "Cmd+O", cx, |view, cx| view.open_document(cx)))
            .child(
                self.menu_item("Open Excalidraw Library...", "", cx, |view, cx| {
                    view.open_library(cx)
                }),
            )
            .children(self.recent_paths.iter().take(5).cloned().map(|path| {
                let label = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string());
                self.menu_item(label, "Recent", cx, move |view, cx| {
                    view.load_path(path.clone(), cx)
                })
            }))
            .child(separator(c.border))
            .child(
                self.menu_item("Import Mermaid from Clipboard", "", cx, |view, cx| {
                    view.import_mermaid_clipboard(cx)
                }),
            )
            .child(self.menu_item("Clear Document", "", cx, |view, cx| {
                view.board.clear_document();
                view.file_menu_open = false;
                cx.notify();
            }))
            .child(separator(c.border))
            .child(self.menu_item("Export PNG...", "Cmd+E", cx, |view, cx| {
                view.export_png(false, cx)
            }))
            .child(
                self.menu_item("Copy as PNG", "Shift+Cmd+C", cx, |view, cx| {
                    view.export_png(true, cx)
                }),
            )
            .child(
                div()
                    .h(px(28.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(11.0))
                    .text_color(rgb(c.text_muted))
                    .child("Export scale")
                    .child(div().flex().gap(px(3.0)).children([1u8, 2, 3].map(|scale| {
                        div()
                            .id(("export-scale", usize::from(scale)))
                            .w(px(26.0))
                            .h(px(22.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.0))
                            .bg(if self.export_scale == scale {
                                rgb(c.accent)
                            } else {
                                rgb(c.hover)
                            })
                            .text_color(if self.export_scale == scale {
                                rgb(c.bg)
                            } else {
                                rgb(c.text)
                            })
                            .child(format!("{scale}x"))
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.export_scale = scale;
                                cx.notify();
                            }))
                    }))),
            )
            .child(separator(c.border))
            .child(self.menu_item("Keyboard Shortcuts", "?", cx, |view, cx| {
                view.shortcuts_open = true;
                view.file_menu_open = false;
                cx.notify();
            }))
            .into_any_element()
    }

    fn menu_item(
        &self,
        label: impl Into<SharedString>,
        shortcut: &'static str,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let c = chrome(cx);
        let label = label.into();
        div()
            .id(label.to_string())
            .h(px(28.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .justify_between()
            .rounded(px(4.0))
            .text_size(px(12.0))
            .text_color(rgb(c.text))
            .hover(|style| style.bg(rgb(c.hover)))
            .child(label)
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(c.text_muted))
                    .child(shortcut),
            )
            .on_click(cx.listener(move |view, _, _, cx| action(view, cx)))
            .into_any_element()
    }

    fn shortcuts_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = chrome(cx);
        const SHORTCUTS: [(&str, &str); 11] = [
            ("Cmd+C / X / V", "Copy, cut, paste"),
            ("Cmd+D", "Duplicate selection"),
            ("Option+Drag", "Duplicate while moving"),
            ("Cmd+G", "Group"),
            ("Cmd+Shift+G", "Ungroup"),
            ("Cmd+Z / Shift+Cmd+Z", "Undo / redo"),
            ("Hold Space", "Draw or click at cursor"),
            ("Delete", "Delete selection"),
            ("Cmd+A", "Select all"),
            ("Cmd+E / Shift+Cmd+E", "Export / copy PNG"),
            ("1-9", "Choose tool"),
        ];
        div()
            .absolute()
            .left_1_2()
            .top_1_2()
            .ml(px(-220.0))
            .mt(px(-170.0))
            .w(px(440.0))
            .p(px(18.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(rgb(c.border))
            .bg(rgb(c.bg))
            .shadow_lg()
            .occlude()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child("Keyboard Shortcuts")
                    .child(
                        div()
                            .id("close-shortcuts")
                            .px(px(6.0))
                            .rounded(px(4.0))
                            .hover(|style| style.bg(rgb(c.hover)))
                            .child("x")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.shortcuts_open = false;
                                cx.notify();
                            })),
                    ),
            )
            .children(SHORTCUTS.map(|(keys, description)| {
                div()
                    .flex()
                    .justify_between()
                    .text_size(px(12.0))
                    .child(keys)
                    .child(div().text_color(rgb(c.text_muted)).child(description))
            }))
            .into_any_element()
    }

    pub(super) fn save_document(&mut self, save_as: bool, cx: &mut Context<Self>) {
        let Ok(json) = self.board.document_json() else {
            self.file_status = Some("Could not serialize document".into());
            return;
        };
        if !save_as && let Some(path) = self.current_path.clone() {
            self.write_document(path, json, cx);
            return;
        }
        self.file_menu_open = false;
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            let task = cx.background_spawn(async move {
                rfd::FileDialog::new()
                    .set_title("Save Drafft Document")
                    .set_file_name("Untitled.json")
                    .add_filter("Drafft Document", &["json"])
                    .save_file()
            });
            cx.spawn(async move |view, cx| {
                let Some(path) = task.await else { return };
                let _ = view.update(cx, |view, cx| view.write_document(path, json, cx));
            })
            .detach();
        }
    }

    fn write_document(&mut self, path: PathBuf, json: String, cx: &mut Context<Self>) {
        let task_path = path.clone();
        let task = cx.background_spawn(async move { std::fs::write(&task_path, json) });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |view, cx| {
                match result {
                    Ok(()) => {
                        view.current_path = Some(path.clone());
                        view.remember_path(path.clone());
                        view.file_status = Some(format!("Saved {}", path.display()));
                    }
                    Err(error) => view.file_status = Some(format!("Save failed: {error}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn open_document(&mut self, cx: &mut Context<Self>) {
        self.file_menu_open = false;
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            let task = cx.background_spawn(async move {
                rfd::FileDialog::new()
                    .set_title("Open Drafft Document")
                    .add_filter("Drafft / Excalidraw", &["json", "excalidraw"])
                    .pick_file()
            });
            cx.spawn(async move |view, cx| {
                let Some(path) = task.await else { return };
                let _ = view.update(cx, |view, cx| view.load_path(path, cx));
            })
            .detach();
        }
    }

    fn load_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let read_path = path.clone();
        let task = cx.background_spawn(async move {
            std::fs::read_to_string(&read_path)
                .map_err(|error| error.to_string())
                .and_then(|content| parse_document(&read_path, &content))
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |view, cx| {
                match result {
                    Ok(document) => {
                        view.board.replace_document(document);
                        view.image_paint_engine.borrow_mut().clear();
                        view.sync_active_tab();
                        view.current_path = Some(path.clone());
                        view.remember_path(path.clone());
                        view.file_status = Some(format!("Opened {}", path.display()));
                    }
                    Err(error) => view.file_status = Some(format!("Open failed: {error}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn remember_path(&mut self, path: PathBuf) {
        self.recent_paths.retain(|candidate| candidate != &path);
        self.recent_paths.insert(0, path);
        self.recent_paths.truncate(5);
    }

    fn open_library(&mut self, cx: &mut Context<Self>) {
        self.file_menu_open = false;
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            let task = cx.background_spawn(async move {
                rfd::FileDialog::new()
                    .set_title("Open Excalidraw Library")
                    .add_filter("Excalidraw Library", &["excalidrawlib"])
                    .pick_file()
            });
            cx.spawn(async move |view, cx| {
                let Some(path) = task.await else { return };
                let fallback = path
                    .file_stem()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Library".into());
                let result = std::fs::read_to_string(&path)
                    .map_err(|error| error.to_string())
                    .and_then(|content| {
                        parse_library(&content, &fallback)
                            .ok_or_else(|| "Not a valid Excalidraw library".to_string())
                    });
                let _ = view.update(cx, |view, cx| {
                    match result {
                        Ok((name, document)) => {
                            view.add_tab(name.clone(), document);
                            view.file_status = Some(format!("Opened library {name}"));
                        }
                        Err(error) => view.file_status = Some(format!("Open failed: {error}")),
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }

    fn import_mermaid_clipboard(&mut self, cx: &mut Context<Self>) {
        let center = self.board.canvas.camera.screen_to_world(self.last_pointer);
        let imported = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .is_some_and(|text| self.board.paste_text_at(&text, center));
        self.file_status = Some(if imported {
            "Imported Mermaid diagram".into()
        } else {
            "Clipboard does not contain Mermaid source".into()
        });
        self.file_menu_open = false;
        cx.notify();
    }

    pub(super) fn export_png(&mut self, copy: bool, cx: &mut Context<Self>) {
        let result = paint::export_png(
            &self.board.canvas,
            self.board.selected(),
            self.export_scale,
            &mut self.text_outline_engine.borrow_mut(),
            &mut self.image_paint_engine.borrow_mut(),
        );
        let bytes = match result {
            Ok(bytes) => bytes,
            Err(error) => {
                self.file_status = Some(format!("Export failed: {error}"));
                cx.notify();
                return;
            }
        };
        self.file_menu_open = false;
        if copy {
            let image = Image::from_bytes(ImageFormat::Png, bytes);
            cx.write_to_clipboard(ClipboardItem::new_image(&image));
            self.file_status = Some("Copied PNG to clipboard".into());
            cx.notify();
            return;
        }
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            let task = cx.background_spawn(async move {
                rfd::FileDialog::new()
                    .set_title("Export PNG")
                    .set_file_name("drawing.png")
                    .add_filter("PNG Image", &["png"])
                    .save_file()
            });
            cx.spawn(async move |view, cx| {
                let Some(path) = task.await else { return };
                let result = std::fs::write(&path, bytes);
                let _ = view.update(cx, |view, cx| {
                    view.file_status = Some(match result {
                        Ok(()) => format!("Exported {}", path.display()),
                        Err(error) => format!("Export failed: {error}"),
                    });
                    cx.notify();
                });
            })
            .detach();
        }
    }
}

fn separator(color: u32) -> AnyElement {
    div()
        .my(px(3.0))
        .h(px(1.0))
        .bg(rgb(color))
        .into_any_element()
}
