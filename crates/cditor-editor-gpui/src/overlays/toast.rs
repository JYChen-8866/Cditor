use std::time::{Duration, Instant};

use gpui::{AnyElement, AppContext, Context, IntoElement, ParentElement, Styled, div, px, rgb};

use crate::editor_view::CditorV2View;
use crate::theme::GuiTheme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiToast {
    pub message: String,
    pub created_at: Instant,
    pub duration: Duration,
}

impl GuiToast {
    pub fn new(message: impl Into<String>, duration: Duration) -> Self {
        Self {
            message: message.into(),
            created_at: Instant::now(),
            duration,
        }
    }

    pub fn is_alive(&self, now: Instant) -> bool {
        now.duration_since(self.created_at) < self.duration
    }
}

pub(crate) fn show_toast(
    view: &mut CditorV2View,
    message: impl Into<String>,
    duration: Duration,
    cx: &mut Context<CditorV2View>,
) {
    view.overlay.toast = Some(GuiToast::new(message, duration));
    let dismiss_after = cx.background_spawn(async move {
        std::thread::sleep(duration);
    });
    cx.spawn(async move |view, cx| {
        let _ = dismiss_after.await;
        let _ = view.update(cx, |view, cx| {
            let should_clear = view
                .overlay
                .toast
                .as_ref()
                .is_some_and(|toast| !toast.is_alive(Instant::now()));
            if should_clear {
                view.overlay.toast = None;
                cx.notify();
            }
        });
    })
    .detach();
    cx.notify();
}

pub fn render_toast(toast: &GuiToast, theme: GuiTheme) -> AnyElement {
    let (background, foreground) = toast_palette(theme);
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom(px(24.0))
        .flex()
        .justify_center()
        .child(
            div()
                .min_h(px(32.0))
                .max_w(px(420.0))
                .rounded(px(4.0))
                .bg(rgb(background))
                .text_color(rgb(foreground))
                .text_size(px(13.0))
                .px(px(12.0))
                .py(px(8.0))
                .shadow_sm()
                .child(toast.message.clone()),
        )
        .into_any_element()
}

fn toast_palette(theme: GuiTheme) -> (u32, u32) {
    (theme.text, theme.panel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toast_expires_after_duration() {
        let created_at = Instant::now();
        let toast = GuiToast {
            message: "ok".to_owned(),
            created_at,
            duration: Duration::from_secs(3),
        };

        assert!(toast.is_alive(created_at + Duration::from_secs(2)));
        assert!(!toast.is_alive(created_at + Duration::from_secs(3)));
    }

    #[test]
    fn toast_uses_notion_theme_contrast() {
        let theme = GuiTheme::light();

        assert_eq!(toast_palette(theme), (theme.text, theme.panel));
    }
}
