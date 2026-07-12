use gpui::KeyDownEvent;

use super::GuiInputCommand;

pub fn is_empty_line_ai_shortcut(event: &KeyDownEvent) -> bool {
    let modifiers = event.keystroke.modifiers;
    event.keystroke.key == "space"
        && !event.keystroke.is_ime_in_progress()
        && !modifiers.platform
        && !modifiers.control
        && !modifiers.alt
        && !modifiers.shift
}

pub fn command_for_key_down(event: &KeyDownEvent) -> GuiInputCommand {
    if event.keystroke.is_ime_in_progress() {
        return GuiInputCommand::Ignore;
    }

    let modifiers = event.keystroke.modifiers;
    let key = event.keystroke.key.as_str();
    let command = modifiers.platform || modifiers.control;

    if command && !modifiers.alt {
        return match key {
            "a" => GuiInputCommand::SelectAllFocusedText,
            "c" => GuiInputCommand::CopySelection,
            "x" => GuiInputCommand::CutSelection,
            "v" => GuiInputCommand::PasteClipboard,
            "b" => GuiInputCommand::ToggleBold,
            "i" => GuiInputCommand::ToggleItalic,
            "u" => GuiInputCommand::ToggleUnderline,
            "e" => GuiInputCommand::ToggleInlineCode,
            "z" if modifiers.shift => GuiInputCommand::RedoFocusedBlock,
            "z" => GuiInputCommand::UndoFocusedBlock,
            "enter" => GuiInputCommand::InsertParagraphAfterFocused,
            _ => GuiInputCommand::Ignore,
        };
    }

    if modifiers.platform || modifiers.control || modifiers.alt {
        return GuiInputCommand::Ignore;
    }

    match key {
        "tab" if modifiers.shift => GuiInputCommand::OutdentBlock,
        "tab" => GuiInputCommand::IndentBlock,
        "left" => GuiInputCommand::MoveCaretLeft {
            extend_selection: modifiers.shift,
        },
        "right" => GuiInputCommand::MoveCaretRight {
            extend_selection: modifiers.shift,
        },
        "up" => GuiInputCommand::MoveCaretUp {
            extend_selection: modifiers.shift,
        },
        "down" => GuiInputCommand::MoveCaretDown {
            extend_selection: modifiers.shift,
        },
        "enter" if modifiers.shift => GuiInputCommand::InsertSoftLineBreak,
        "enter" => GuiInputCommand::HandleEnter,
        "backspace" => GuiInputCommand::DeleteBackward,
        "delete" => GuiInputCommand::DeleteForward,
        _ => GuiInputCommand::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event_for_key(key: &str, modifiers: gpui::Modifiers) -> KeyDownEvent {
        KeyDownEvent {
            keystroke: gpui::Keystroke {
                modifiers,
                key: key.into(),
                key_char: Some(key.to_owned()),
            },
            is_held: false,
            prefer_character_input: false,
        }
    }

    #[test]
    fn keyboard_maps_plain_text_and_editing_keys() {
        assert_eq!(
            command_for_key_down(&event_for_key("a", gpui::Modifiers::default())),
            GuiInputCommand::Ignore
        );
        assert_eq!(
            command_for_key_down(&event_for_key("enter", gpui::Modifiers::default())),
            GuiInputCommand::HandleEnter
        );
        assert_eq!(
            command_for_key_down(&event_for_key("backspace", gpui::Modifiers::default())),
            GuiInputCommand::DeleteBackward
        );
        assert_eq!(
            command_for_key_down(&event_for_key("tab", gpui::Modifiers::default())),
            GuiInputCommand::IndentBlock
        );
        let mut shift = gpui::Modifiers::default();
        shift.shift = true;
        assert_eq!(
            command_for_key_down(&event_for_key("tab", shift)),
            GuiInputCommand::OutdentBlock
        );
    }

    #[test]
    fn empty_line_ai_shortcut_requires_unmodified_space() {
        assert!(is_empty_line_ai_shortcut(&event_for_key(
            "space",
            gpui::Modifiers::default()
        )));
        let mut shift = gpui::Modifiers::default();
        shift.shift = true;
        assert!(!is_empty_line_ai_shortcut(&event_for_key("space", shift)));
        assert!(!is_empty_line_ai_shortcut(&event_for_key(
            "enter",
            gpui::Modifiers::default()
        )));
    }

    #[test]
    fn keyboard_maps_command_shortcuts() {
        let mut modifiers = gpui::Modifiers::default();
        modifiers.platform = true;
        assert_eq!(
            command_for_key_down(&event_for_key("a", modifiers)),
            GuiInputCommand::SelectAllFocusedText
        );
        assert_eq!(
            command_for_key_down(&event_for_key("c", modifiers)),
            GuiInputCommand::CopySelection
        );
        assert_eq!(
            command_for_key_down(&event_for_key("x", modifiers)),
            GuiInputCommand::CutSelection
        );
        assert_eq!(
            command_for_key_down(&event_for_key("v", modifiers)),
            GuiInputCommand::PasteClipboard
        );
        assert_eq!(
            command_for_key_down(&event_for_key("b", modifiers)),
            GuiInputCommand::ToggleBold
        );
        assert_eq!(
            command_for_key_down(&event_for_key("z", modifiers)),
            GuiInputCommand::UndoFocusedBlock
        );
        modifiers.shift = true;
        assert_eq!(
            command_for_key_down(&event_for_key("z", modifiers)),
            GuiInputCommand::RedoFocusedBlock
        );

        let mut plain = gpui::Modifiers::default();
        assert_eq!(
            command_for_key_down(&event_for_key("left", plain)),
            GuiInputCommand::MoveCaretLeft {
                extend_selection: false
            }
        );
        plain.shift = true;
        assert_eq!(
            command_for_key_down(&event_for_key("right", plain)),
            GuiInputCommand::MoveCaretRight {
                extend_selection: true
            }
        );
        assert_eq!(
            command_for_key_down(&event_for_key("up", plain)),
            GuiInputCommand::MoveCaretUp {
                extend_selection: true
            }
        );
        assert_eq!(
            command_for_key_down(&event_for_key("down", gpui::Modifiers::default())),
            GuiInputCommand::MoveCaretDown {
                extend_selection: false
            }
        );
    }
}
