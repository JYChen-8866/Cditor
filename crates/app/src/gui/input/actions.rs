use gpui::{App, KeyBinding, actions};

pub(crate) const CDITOR_KEY_CONTEXT: &str = "CditorEditor";

actions!(
    cditor,
    [
        Newline,
        SoftLineBreak,
        NewlineBelow,
        Tab,
        Backtab,
        Cancel,
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        MoveToLineStart,
        MoveToLineEnd,
        SelectToLineStart,
        SelectToLineEnd,
        Backspace,
        Delete,
        SelectAll,
        Copy,
        Cut,
        Paste,
        Undo,
        Redo,
        ToggleBold,
        ToggleItalic,
        ToggleUnderline,
        ToggleInlineCode,
        Duplicate,
    ]
);

/// Install the editor keymap once during application initialization.
///
/// `secondary` is GPUI's cross-platform primary editing modifier: Command on
/// macOS and Control on Windows/Linux. This mirrors Zed's action/keymap path
/// and deliberately leaves native key translation to GPUI.
pub fn bind_cditor_keys(cx: &mut App) {
    let context = Some(CDITOR_KEY_CONTEXT);
    let bindings = vec![
        KeyBinding::new("enter", Newline, context),
        KeyBinding::new("shift-enter", SoftLineBreak, context),
        KeyBinding::new("secondary-enter", NewlineBelow, context),
        KeyBinding::new("tab", Tab, context),
        KeyBinding::new("shift-tab", Backtab, context),
        KeyBinding::new("escape", Cancel, context),
        KeyBinding::new("left", MoveLeft, context),
        KeyBinding::new("right", MoveRight, context),
        KeyBinding::new("up", MoveUp, context),
        KeyBinding::new("down", MoveDown, context),
        KeyBinding::new("shift-left", SelectLeft, context),
        KeyBinding::new("shift-right", SelectRight, context),
        KeyBinding::new("shift-up", SelectUp, context),
        KeyBinding::new("shift-down", SelectDown, context),
        KeyBinding::new("home", MoveToLineStart, context),
        KeyBinding::new("end", MoveToLineEnd, context),
        KeyBinding::new("shift-home", SelectToLineStart, context),
        KeyBinding::new("shift-end", SelectToLineEnd, context),
        KeyBinding::new("backspace", Backspace, context),
        KeyBinding::new("shift-backspace", Backspace, context),
        KeyBinding::new("delete", Delete, context),
        KeyBinding::new("secondary-a", SelectAll, context),
        KeyBinding::new("secondary-c", Copy, context),
        KeyBinding::new("secondary-x", Cut, context),
        KeyBinding::new("secondary-v", Paste, context),
        KeyBinding::new("secondary-z", Undo, context),
        KeyBinding::new("secondary-shift-z", Redo, context),
        KeyBinding::new("secondary-b", ToggleBold, context),
        KeyBinding::new("secondary-i", ToggleItalic, context),
        KeyBinding::new("secondary-u", ToggleUnderline, context),
        KeyBinding::new("secondary-e", ToggleInlineCode, context),
        KeyBinding::new("secondary-d", Duplicate, context),
    ];
    cx.bind_keys(bindings);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        // Zed's default Windows editing aliases.
        KeyBinding::new("secondary-y", Redo, context),
        KeyBinding::new("shift-delete", Cut, context),
        KeyBinding::new("secondary-insert", Copy, context),
        KeyBinding::new("shift-insert", Paste, context),
    ]);
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        // Native macOS/Emacs navigation aliases used by Zed.
        KeyBinding::new("ctrl-h", Backspace, context),
        KeyBinding::new("ctrl-d", Delete, context),
        KeyBinding::new("ctrl-b", MoveLeft, context),
        KeyBinding::new("ctrl-f", MoveRight, context),
        KeyBinding::new("ctrl-p", MoveUp, context),
        KeyBinding::new("ctrl-n", MoveDown, context),
        KeyBinding::new("ctrl-a", MoveToLineStart, context),
        KeyBinding::new("ctrl-e", MoveToLineEnd, context),
        KeyBinding::new("secondary-left", MoveToLineStart, context),
        KeyBinding::new("secondary-right", MoveToLineEnd, context),
        KeyBinding::new("secondary-shift-left", SelectToLineStart, context),
        KeyBinding::new("secondary-shift-right", SelectToLineEnd, context),
    ]);
}

#[cfg(test)]
mod tests {
    use gpui::Keystroke;

    #[test]
    fn secondary_modifier_uses_the_host_editing_convention() {
        let parsed = Keystroke::parse("secondary-a").unwrap();
        #[cfg(target_os = "macos")]
        assert!(parsed.modifiers.platform && !parsed.modifiers.control);
        #[cfg(not(target_os = "macos"))]
        assert!(parsed.modifiers.control && !parsed.modifiers.platform);
    }
}
