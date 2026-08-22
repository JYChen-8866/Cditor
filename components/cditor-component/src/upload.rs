//! Stateful Upload component ported from `gpui-component`.

use std::{
    path::{Path, PathBuf},
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

use gpui::{
    App, Context, ElementId, Entity, ExternalPaths, InteractiveElement, IntoElement, MouseButton,
    ParentElement, PathPromptOptions, Pixels, Render, SharedString, Styled, Window, div,
    prelude::FluentBuilder as _, px, relative,
};

use crate::SvgIcon;

const ICON_UPLOAD: &[u8] = include_bytes!("../../../assets/icons/upload.svg");
const ICON_FILE: &[u8] = include_bytes!("../../../assets/icons/file.svg");
const ICON_TRASH: &[u8] = include_bytes!("../../../assets/icons/trash-2.svg");
const ICON_SUCCESS: &[u8] = include_bytes!("../../../assets/icons/circle-check.svg");
const ICON_ERROR: &[u8] = include_bytes!("../../../assets/icons/circle-x.svg");
const ICON_CLOSE: &[u8] = include_bytes!("../../../assets/icons/close.svg");
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadStatus {
    Ready,
    Uploading,
    Success,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UploadListType {
    #[default]
    Text,
    PictureCard,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UploadFile {
    pub id: SharedString,
    pub name: SharedString,
    pub size: Option<u64>,
    pub status: UploadStatus,
    pub progress: u8,
    pub description: Option<SharedString>,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadRejectReason {
    TypeMismatch,
    TooLarge,
    MetadataUnavailable,
}

#[derive(Debug, Clone, Copy)]
pub struct UploadStyle {
    pub background: u32,
    pub border: u32,
    pub hover_border: u32,
    pub text: u32,
    pub muted: u32,
    pub icon: u32,
}

type SelectHandler = Rc<dyn Fn(Vec<PathBuf>, &mut App)>;
type RemoveHandler = Rc<dyn Fn(UploadFile, &mut Window, &mut App)>;
type ClickHandler = Rc<dyn Fn(&mut Window, &mut App)>;

pub struct Upload {
    id: SharedString,
    style: UploadStyle,
    files: Vec<UploadFile>,
    list_type: UploadListType,
    drag: bool,
    disabled: bool,
    multiple: bool,
    directories: bool,
    limit: Option<usize>,
    accept: Option<SharedString>,
    max_size: Option<u64>,
    selecting: bool,
    last_error: Option<SharedString>,
    button_text: SharedString,
    title: SharedString,
    hint: Option<SharedString>,
    tip: Option<SharedString>,
    width: Option<Pixels>,
    on_select: Option<SelectHandler>,
    on_remove: Option<RemoveHandler>,
    on_click: Option<ClickHandler>,
}

impl UploadFile {
    pub fn new(id: impl Into<SharedString>, name: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            size: None,
            status: UploadStatus::Ready,
            progress: 0,
            description: None,
            path: None,
        }
    }

    pub fn size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    pub fn status(mut self, status: UploadStatus) -> Self {
        self.status = status;
        self
    }

    pub fn progress(mut self, progress: u8) -> Self {
        self.progress = progress.min(100);
        self
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }
}

impl Upload {
    pub fn new(id: impl Into<SharedString>, style: UploadStyle) -> Self {
        Self {
            id: id.into(),
            style,
            files: Vec::new(),
            list_type: UploadListType::Text,
            drag: false,
            disabled: false,
            multiple: false,
            directories: false,
            limit: None,
            accept: None,
            max_size: None,
            selecting: false,
            last_error: None,
            button_text: "点击上传".into(),
            title: "拖放文件或点击选择".into(),
            hint: None,
            tip: None,
            width: None,
            on_select: None,
            on_remove: None,
            on_click: None,
        }
    }

    pub fn unique(style: UploadStyle) -> Self {
        Self::new(
            format!("upload-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed)),
            style,
        )
    }

    pub fn id(mut self, id: impl Into<SharedString>) -> Self {
        self.id = id.into();
        self
    }
    pub fn files(mut self, files: impl IntoIterator<Item = UploadFile>) -> Self {
        self.files = files.into_iter().collect();
        self
    }
    pub fn add_file(mut self, file: UploadFile) -> Self {
        self.files.push(file);
        self
    }
    pub fn list_type(mut self, value: UploadListType) -> Self {
        self.list_type = value;
        self
    }
    pub fn picture_card(self) -> Self {
        self.list_type(UploadListType::PictureCard)
    }
    pub fn drag(mut self, value: bool) -> Self {
        self.drag = value;
        self
    }
    pub fn disabled(mut self, value: bool) -> Self {
        self.disabled = value;
        self
    }
    pub fn multiple(mut self, value: bool) -> Self {
        self.multiple = value;
        self
    }
    pub fn directories(mut self, value: bool) -> Self {
        self.directories = value;
        self
    }
    pub fn limit(mut self, value: usize) -> Self {
        self.limit = Some(value);
        self
    }
    pub fn accept(mut self, value: impl Into<SharedString>) -> Self {
        self.accept = Some(value.into());
        self
    }
    pub fn max_size(mut self, value: u64) -> Self {
        self.max_size = Some(value);
        self
    }
    pub fn button_text(mut self, value: impl Into<SharedString>) -> Self {
        self.button_text = value.into();
        self
    }
    pub fn title(mut self, value: impl Into<SharedString>) -> Self {
        self.title = value.into();
        self
    }
    pub fn hint(mut self, value: impl Into<SharedString>) -> Self {
        self.hint = Some(value.into());
        self
    }
    pub fn tip(mut self, value: impl Into<SharedString>) -> Self {
        self.tip = Some(value.into());
        self
    }
    pub fn width(mut self, value: impl Into<Pixels>) -> Self {
        self.width = Some(value.into());
        self
    }
    pub fn width_lg(self) -> Self {
        self.width(px(420.0))
    }

    pub fn on_select(mut self, handler: impl Fn(Vec<PathBuf>, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(handler));
        self
    }

    pub fn on_paths(self, handler: impl Fn(Vec<PathBuf>, &mut App) + 'static) -> Self {
        self.on_select(handler)
    }

    pub fn on_remove(
        mut self,
        handler: impl Fn(UploadFile, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_remove = Some(Rc::new(handler));
        self
    }

    /// Compatibility hook for hosts that need to run an action when the
    /// trigger is pressed. The built-in picker still runs afterwards.
    pub fn on_click(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    pub fn set_files(&mut self, files: Vec<UploadFile>, cx: &mut Context<Self>) {
        self.files = files;
        cx.notify();
    }

    pub fn set_style(&mut self, style: UploadStyle, cx: &mut Context<Self>) {
        self.style = style;
        cx.notify();
    }

    pub fn push_file(&mut self, file: UploadFile, cx: &mut Context<Self>) {
        if Self::can_accept_more_len(self.files.len(), self.limit, self.disabled) {
            self.files.push(file);
            cx.notify();
        }
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }
    pub fn files_ref(&self) -> &[UploadFile] {
        &self.files
    }
    pub fn selected_paths(&self) -> Vec<PathBuf> {
        self.files
            .iter()
            .filter_map(|file| file.path.clone())
            .collect()
    }

    pub fn can_accept_more_len(current: usize, limit: Option<usize>, disabled: bool) -> bool {
        !disabled && !limit.is_some_and(|limit| current >= limit)
    }

    pub fn matches_accept_name(name: &str, accept: Option<&str>) -> bool {
        let Some(accept) = accept.map(str::trim) else {
            return true;
        };
        if accept.is_empty() {
            return true;
        }
        let lower_name = name.to_lowercase();
        let ext = Path::new(name)
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_lowercase);
        accept
            .split(',')
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .any(|token| {
                let token = token.to_lowercase();
                if token == "*" || token == "*/*" {
                    return true;
                }
                if let Some(expected) = token.strip_prefix('.') {
                    return ext.as_deref() == Some(expected);
                }
                if let Some(group) = token.strip_suffix("/*") {
                    return matches_mime_group(ext.as_deref(), group);
                }
                lower_name.ends_with(&token)
            })
    }

    pub fn validate_file_name_size(
        name: &str,
        size: Option<u64>,
        accept: Option<&str>,
        max_size: Option<u64>,
    ) -> Result<(), UploadRejectReason> {
        if !Self::matches_accept_name(name, accept) {
            return Err(UploadRejectReason::TypeMismatch);
        }
        if let Some(max_size) = max_size {
            let Some(size) = size else {
                return Err(UploadRejectReason::MetadataUnavailable);
            };
            if size > max_size {
                return Err(UploadRejectReason::TooLarge);
            }
        }
        Ok(())
    }

    pub fn validate_path(
        path: &Path,
        accept: Option<&str>,
        max_size: Option<u64>,
    ) -> Result<UploadFile, UploadRejectReason> {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file");
        let size = std::fs::metadata(path).ok().map(|metadata| metadata.len());
        Self::validate_file_name_size(name, size, accept, max_size)?;
        let mut file = UploadFile::new(path.to_string_lossy(), name).path(path);
        file.size = size;
        Ok(file)
    }

    fn accept_selected_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let mut accepted = Vec::new();
        let mut rejected = 0;
        for path in paths {
            if !Self::can_accept_more_len(self.files.len(), self.limit, self.disabled) {
                break;
            }
            let file = if self.directories && path.is_dir() {
                Some(
                    UploadFile::new(
                        path.to_string_lossy(),
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("folder"),
                    )
                    .path(&path),
                )
            } else {
                Self::validate_path(&path, self.accept.as_deref(), self.max_size).ok()
            };
            match file {
                Some(file) => {
                    accepted.push(path);
                    self.files.push(file);
                }
                None => rejected += 1,
            }
            if !self.multiple {
                break;
            }
        }
        self.last_error =
            (rejected > 0).then(|| format!("{rejected} 个文件不符合类型或大小限制").into());
        if !accepted.is_empty()
            && let Some(handler) = self.on_select.clone()
        {
            handler(accepted, cx);
        }
        cx.notify();
    }

    fn trigger_select(&mut self, cx: &mut Context<Self>) {
        if !Self::can_accept_more_len(self.files.len(), self.limit, self.disabled) || self.selecting
        {
            return;
        }
        self.selecting = true;
        self.last_error = None;
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: self.directories,
            multiple: self.multiple,
            prompt: Some(self.button_text.clone()),
        });
        cx.notify();
        cx.spawn(async move |entity, cx| {
            let result = receiver.await;
            entity
                .update(cx, |upload, cx| {
                    upload.selecting = false;
                    match result {
                        Ok(Ok(Some(paths))) => upload.accept_selected_paths(paths, cx),
                        Ok(Ok(None)) => {
                            upload.last_error = None;
                            cx.notify();
                        }
                        Ok(Err(error)) => {
                            upload.last_error = Some(format!("打开文件失败：{error}").into());
                            cx.notify();
                        }
                        Err(_) => {
                            upload.last_error = Some("文件选择已取消".into());
                            cx.notify();
                        }
                    }
                })
                .ok();
        })
        .detach();
    }

    pub fn remove_file_by_id(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.files.iter().position(|file| file.id.as_ref() == id) {
            let file = self.files.remove(index);
            if let Some(handler) = self.on_remove.clone() {
                handler(file, window, cx);
            }
            cx.notify();
        }
    }
}

impl Render for Upload {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let enabled = Self::can_accept_more_len(self.files.len(), self.limit, self.disabled)
            && !self.selecting;
        let entity = cx.entity();
        let click = self.on_click.clone();
        let trigger = if self.drag {
            render_drag_trigger(self, enabled, entity.clone(), click).into_any_element()
        } else {
            render_button_trigger(self, enabled, entity.clone(), click).into_any_element()
        };
        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .when_some(self.width, |root, width| root.w(width))
            .child(trigger)
            .when_some(self.tip.clone(), |root, tip| {
                root.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(gpui::rgb(self.style.muted))
                        .child(tip),
                )
            })
            .when_some(self.last_error.clone(), |root, error| {
                root.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(gpui::rgb(0xeb5757))
                        .child(error),
                )
            })
            .child(match self.list_type {
                UploadListType::Text => {
                    render_text_list(&self.id, &self.files, self.style, entity).into_any_element()
                }
                UploadListType::PictureCard => {
                    render_picture_list(&self.id, &self.files, self.style, entity)
                        .into_any_element()
                }
            })
    }
}

fn render_button_trigger(
    upload: &Upload,
    enabled: bool,
    entity: Entity<Upload>,
    click: Option<ClickHandler>,
) -> impl IntoElement {
    let style = upload.style;
    let text = if upload.selecting {
        "正在选择…".into()
    } else {
        upload.button_text.clone()
    };
    div()
        .id(ElementId::Name(format!("{}-trigger", upload.id).into()))
        .flex()
        .items_center()
        .gap(px(8.0))
        .px(px(14.0))
        .py(px(8.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(gpui::rgb(if enabled {
            style.hover_border
        } else {
            style.border
        }))
        .bg(gpui::rgb(style.background))
        .when(enabled, |element| element.cursor_pointer())
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            if enabled {
                if let Some(handler) = click.clone() {
                    handler(window, cx);
                }
                entity.update(cx, |upload, cx| upload.trigger_select(cx));
            }
        })
        .child(
            SvgIcon::new("upload-button", ICON_UPLOAD)
                .size(px(16.0))
                .color(gpui::rgb(style.icon)),
        )
        .child(
            div()
                .text_size(px(14.0))
                .text_color(gpui::rgb(style.text))
                .child(text),
        )
}

fn render_drag_trigger(
    upload: &Upload,
    enabled: bool,
    entity: Entity<Upload>,
    click: Option<ClickHandler>,
) -> impl IntoElement {
    let style = upload.style;
    let drop_entity = entity.clone();
    let hint = upload.hint.clone().unwrap_or_else(|| upload_hint(upload));
    div()
        .id(ElementId::Name(format!("{}-trigger", upload.id).into()))
        .w_full()
        .h(px(150.0))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(10.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(gpui::rgb(if enabled {
            style.border
        } else {
            style.hover_border
        }))
        .bg(gpui::rgb(style.background))
        .when(enabled, |element| {
            element
                .cursor_pointer()
                .hover(move |element| element.border_color(gpui::rgb(style.hover_border)))
        })
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            if enabled {
                if let Some(handler) = click.clone() {
                    handler(window, cx);
                }
                entity.update(cx, |upload, cx| upload.trigger_select(cx));
            }
        })
        .on_drop(move |paths: &ExternalPaths, _, cx| {
            if enabled {
                drop_entity.update(cx, |upload, cx| {
                    upload.accept_selected_paths(paths.paths().to_vec(), cx)
                });
            }
        })
        .child(
            SvgIcon::new("upload-drag", ICON_UPLOAD)
                .size(px(32.0))
                .color(gpui::rgb(style.icon)),
        )
        .child(
            div()
                .text_size(px(14.0))
                .text_color(gpui::rgb(style.text))
                .child(upload.title.clone()),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(gpui::rgb(style.muted))
                .child(hint),
        )
}

fn render_text_list(
    id: &SharedString,
    files: &[UploadFile],
    style: UploadStyle,
    entity: Entity<Upload>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .children(files.iter().cloned().map(move |file| {
            let remove_id = file.id.clone();
            let remove_entity = entity.clone();
            div()
                .id(ElementId::Name(format!("{}-file-{}", id, file.id).into()))
                .flex()
                .flex_col()
                .gap(px(5.0))
                .rounded(px(6.0))
                .px(px(10.0))
                .py(px(8.0))
                .bg(gpui::rgb(style.background))
                .border_1()
                .border_color(gpui::rgb(style.border))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(status_icon(file.status, style))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .child(
                                    div()
                                        .text_size(px(14.0))
                                        .text_color(gpui::rgb(style.text))
                                        .child(file.name.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(gpui::rgb(style.muted))
                                        .child(file_meta(&file)),
                                ),
                        )
                        .child(
                            div()
                                .id(ElementId::Name(format!("{}-remove", file.id).into()))
                                .p(px(4.0))
                                .cursor_pointer()
                                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                    remove_entity.update(cx, |upload, cx| {
                                        upload.remove_file_by_id(remove_id.as_ref(), window, cx)
                                    });
                                })
                                .child(
                                    SvgIcon::new("upload-trash", ICON_TRASH)
                                        .size(px(14.0))
                                        .color(gpui::rgb(style.muted)),
                                ),
                        ),
                )
                .when(file.status == UploadStatus::Uploading, |item| {
                    item.child(progress_bar(file.progress, style))
                })
        }))
}

fn render_picture_list(
    id: &SharedString,
    files: &[UploadFile],
    style: UploadStyle,
    entity: Entity<Upload>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_wrap()
        .gap(px(12.0))
        .children(files.iter().cloned().map(move |file| {
            let remove_id = file.id.clone();
            let remove_entity = entity.clone();
            div()
                .id(ElementId::Name(
                    format!("{}-picture-{}", id, file.id).into(),
                ))
                .relative()
                .w(px(112.0))
                .h(px(112.0))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(gpui::rgb(style.border))
                .bg(gpui::rgb(style.background))
                .child(status_icon(file.status, style).size(px(24.0)))
                .child(
                    div()
                        .px(px(8.0))
                        .text_size(px(12.0))
                        .text_color(gpui::rgb(style.text))
                        .child(file.name.clone()),
                )
                .when(file.status == UploadStatus::Uploading, |item| {
                    item.child(
                        div()
                            .absolute()
                            .bottom(px(8.0))
                            .left(px(8.0))
                            .right(px(8.0))
                            .child(progress_bar(file.progress, style)),
                    )
                })
                .child(
                    div()
                        .id(ElementId::Name(
                            format!("{}-picture-remove", file.id).into(),
                        ))
                        .absolute()
                        .top(px(6.0))
                        .right(px(6.0))
                        .p(px(4.0))
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            remove_entity.update(cx, |upload, cx| {
                                upload.remove_file_by_id(remove_id.as_ref(), window, cx)
                            });
                        })
                        .child(
                            SvgIcon::new("upload-close", ICON_CLOSE)
                                .size(px(14.0))
                                .color(gpui::rgb(style.muted)),
                        ),
                )
        }))
}

fn status_icon(status: UploadStatus, style: UploadStyle) -> SvgIcon {
    let (key, bytes, color) = match status {
        UploadStatus::Ready => ("upload-ready", ICON_FILE, style.muted),
        UploadStatus::Uploading => ("upload-uploading", ICON_UPLOAD, style.hover_border),
        UploadStatus::Success => ("upload-success", ICON_SUCCESS, 0x22a06b),
        UploadStatus::Error => ("upload-error", ICON_ERROR, 0xeb5757),
    };
    SvgIcon::new(key, bytes)
        .size(px(16.0))
        .color(gpui::rgb(color))
}

fn progress_bar(progress: u8, style: UploadStyle) -> impl IntoElement {
    div()
        .h(px(4.0))
        .rounded(px(999.0))
        .bg(gpui::rgb(style.border))
        .child(
            div()
                .h_full()
                .w(relative(progress as f32 / 100.0))
                .rounded(px(999.0))
                .bg(gpui::rgb(style.hover_border)),
        )
}

fn upload_hint(upload: &Upload) -> SharedString {
    let mut hint = match (upload.multiple, upload.accept.as_deref()) {
        (true, Some(accept)) => format!("可选择多个文件，支持 {accept}"),
        (true, None) => "可选择多个文件".to_owned(),
        (false, Some(accept)) => format!("支持 {accept}"),
        (false, None) => "将文件拖放到这里".to_owned(),
    };
    if let Some(max_size) = upload.max_size {
        hint.push_str(&format!("，最大 {}", format_size(max_size)));
    }
    hint.into()
}

fn file_meta(file: &UploadFile) -> String {
    let status = match file.status {
        UploadStatus::Ready => "待上传",
        UploadStatus::Uploading => "上传中",
        UploadStatus::Success => "已完成",
        UploadStatus::Error => "失败",
    };
    let size = file
        .size
        .map(format_size)
        .unwrap_or_else(|| "未知大小".into());
    match &file.description {
        Some(description) => format!("{status} · {size} · {description}"),
        None => format!("{status} · {size}"),
    }
}

fn format_size(size: u64) -> String {
    if size >= 1024 * 1024 {
        format!("{:.1} MB", size as f64 / 1024.0 / 1024.0)
    } else if size >= 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else {
        format!("{size} B")
    }
}

fn matches_mime_group(ext: Option<&str>, group: &str) -> bool {
    match group {
        "image" => matches!(
            ext,
            Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg")
        ),
        "text" => matches!(
            ext,
            Some("txt" | "md" | "csv" | "json" | "toml" | "yaml" | "yml" | "rs")
        ),
        "audio" => matches!(ext, Some("mp3" | "wav" | "ogg" | "flac" | "m4a")),
        "video" => matches!(ext, Some("mp4" | "mov" | "m4v" | "webm" | "mkv" | "avi")),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style() -> UploadStyle {
        UploadStyle {
            background: 0,
            border: 1,
            hover_border: 2,
            text: 3,
            muted: 4,
            icon: 5,
        }
    }

    #[test]
    fn width_lg_matches_original_component() {
        assert_eq!(Upload::unique(style()).width_lg().width, Some(px(420.0)));
    }
    #[test]
    fn defaults_match_original_component() {
        let upload = Upload::unique(style());
        assert!(!upload.drag);
        assert!(!upload.multiple);
        assert_eq!(upload.file_count(), 0);
    }
    #[test]
    fn accepts_extensions_and_mime_groups() {
        assert!(Upload::matches_accept_name("a.PNG", Some(".png,.jpg")));
        assert!(Upload::matches_accept_name("a.mp4", Some("video/*")));
        assert!(!Upload::matches_accept_name("a.zip", Some("video/*")));
    }
    #[test]
    fn rejects_large_files() {
        assert_eq!(
            Upload::validate_file_name_size("a.mp4", Some(9), Some("video/*"), Some(8)),
            Err(UploadRejectReason::TooLarge)
        );
    }
    #[test]
    fn progress_is_clamped() {
        assert_eq!(UploadFile::new("a", "a").progress(200).progress, 100);
    }
    #[test]
    fn size_is_human_readable() {
        assert_eq!(format_size(1536), "1.5 KB");
    }
}
