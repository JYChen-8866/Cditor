use std::time::{Duration, Instant};

use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px, rgb};

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

pub fn render_toast(toast: &GuiToast) -> AnyElement {
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom(px(24.0))
        .flex()
        .justify_center()
        .child(
            div()
                .rounded(px(7.0))
                .bg(rgb(0x202124))
                .text_color(rgb(0xffffff))
                .text_size(px(13.0))
                .px(px(14.0))
                .py(px(10.0))
                .shadow_lg()
                .child(toast.message.clone()),
        )
        .into_any_element()
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
}
