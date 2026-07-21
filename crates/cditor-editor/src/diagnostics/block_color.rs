//! Block-color mutation and paint diagnostics.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use cditor_core::ids::BlockId;
use cditor_core::rich_text::BlockAttrs;

pub(crate) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("CDITOR_TRACE_BLOCK_COLOR")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false)
    })
}

pub(crate) fn trace(event: &str, details: impl std::fmt::Display) {
    if enabled() {
        eprintln!("[cditor][block-color][gui][{event}] {details}");
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderTraceState {
    attrs_color: Option<String>,
    attrs_background: Option<String>,
    resolved_text: u32,
    resolved_background: u32,
    action_active: bool,
}

pub(crate) fn trace_render(
    block_id: BlockId,
    attrs: &BlockAttrs,
    resolved_text: u32,
    resolved_background: u32,
    action_active: bool,
) {
    if !enabled() {
        return;
    }

    let state = RenderTraceState {
        attrs_color: attrs.color.clone(),
        attrs_background: attrs.background_color.clone(),
        resolved_text,
        resolved_background,
        action_active,
    };
    static LAST_RENDERED: OnceLock<Mutex<HashMap<BlockId, RenderTraceState>>> = OnceLock::new();
    let changed = LAST_RENDERED
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map(|mut rendered| rendered.insert(block_id, state.clone()).as_ref() != Some(&state))
        .unwrap_or(true);
    if changed {
        trace(
            "render.resolve",
            format_args!(
                "block_id={block_id} attrs_text={:?} attrs_background={:?} resolved_text=#{resolved_text:06x} resolved_background=#{resolved_background:06x} action_active={action_active}",
                state.attrs_color, state.attrs_background,
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_trace_state_keeps_runtime_attrs_and_resolved_paint_separate() {
        let state = RenderTraceState {
            attrs_color: Some("#d44c47".to_owned()),
            attrs_background: Some("#fdebec".to_owned()),
            resolved_text: 0xd44c47,
            resolved_background: 0xfdebec,
            action_active: true,
        };

        assert_eq!(state.attrs_color.as_deref(), Some("#d44c47"));
        assert_eq!(state.attrs_background.as_deref(), Some("#fdebec"));
        assert_eq!(state.resolved_text, 0xd44c47);
        assert_eq!(state.resolved_background, 0xfdebec);
    }
}
