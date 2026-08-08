use gpui::{
    App, Bounds, ElementInputHandler, Entity, FocusHandle, HitboxId, InteractiveElement,
    MouseButton, MouseDownEvent, Pixels, StatefulInteractiveElement, Window,
};

use crate::editor_view::{CditorV2View, GuiPlatformInputTarget};
use crate::input::trace::trace_input;
use crate::text::TextPlatformLayoutIdentity;

/// Activates an editable mobile text surface as one atomic transition.
///
/// Focus/selection placement remains owned by Cditor. This helper only changes
/// the platform session after a completed press has selected the actual target.
pub(crate) fn activate_mobile_text_input(_window: &Window) {
    #[cfg(feature = "mobile-text-session")]
    _window.show_soft_keyboard();
}

pub(crate) fn mobile_manual_focus<E: InteractiveElement>(element: E) -> E {
    #[cfg(all(
        feature = "mobile-text-session",
        any(target_os = "ios", target_os = "android")
    ))]
    {
        element.manual_focus()
    }
    #[cfg(not(all(
        feature = "mobile-text-session",
        any(target_os = "ios", target_os = "android")
    )))]
    {
        element
    }
}

pub(crate) const fn mobile_text_input_uses_manual_focus() -> bool {
    cfg!(all(
        feature = "mobile-text-session",
        any(target_os = "ios", target_os = "android")
    ))
}

pub(crate) fn finish_auxiliary_text_input(
    _editor_focus: &FocusHandle,
    _window: &mut Window,
    _cx: &mut App,
) {
    #[cfg(all(
        feature = "mobile-text-session",
        any(target_os = "ios", target_os = "android")
    ))]
    _window.dismiss_text_input();

    #[cfg(not(all(
        feature = "mobile-text-session",
        any(target_os = "ios", target_os = "android")
    )))]
    _window.focus(_editor_focus, _cx);
}

pub(crate) const fn retains_pointer_drag_after_text_activation(is_mobile: bool) -> bool {
    !is_mobile
}

const fn text_activation_click_count(tap_count: usize) -> usize {
    if tap_count == 0 { 1 } else { tap_count }
}

/// Binds text activation to the platform's correct gesture boundary.
///
/// Desktop selection starts on mouse-down so drag selection remains immediate.
/// Direct-touch platforms commit on `on_press`, whose GPUI recognizer rejects a
/// moved, cancelled, or long-held pointer. The no-op long-press listener is
/// intentional: it marks the same recognizer as consumed by a long press while
/// UIKit continues owning native text selection.
pub(crate) fn on_text_activation<E>(
    element: E,
    listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> E
where
    E: StatefulInteractiveElement,
{
    #[cfg(all(
        feature = "mobile-text-session",
        any(target_os = "ios", target_os = "android")
    ))]
    {
        element
            .on_press(move |event, window, cx| {
                let event = MouseDownEvent {
                    button: MouseButton::Left,
                    position: event.position(),
                    modifiers: event.modifiers(),
                    click_count: text_activation_click_count(event.tap_count()),
                    first_mouse: false,
                };
                listener(&event, window, cx);
            })
            .on_long_press(|_event, _window, _cx| {})
    }
    #[cfg(not(all(
        feature = "mobile-text-session",
        any(target_os = "ios", target_os = "android")
    )))]
    {
        element.on_mouse_down(MouseButton::Left, listener)
    }
}

pub(crate) fn handle_registered_platform_input(
    view: &Entity<CditorV2View>,
    focus: &FocusHandle,
    target: GuiPlatformInputTarget,
    layout_identity: TextPlatformLayoutIdentity,
    bounds: Bounds<Pixels>,
    hitbox_id: Option<HitboxId>,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let registration = view.update(cx, |view, _cx| {
        view.register_platform_input_target(target, layout_identity, bounds)
    });
    if registration.registered {
        view.update(cx, |view, _| view.input.hitbox_id = hitbox_id);
        trace_input(
            "platform_input.registered",
            format_args!(
                "target={target:?} layout={layout_identity:?} bounds={bounds:?} coordinates_changed={}",
                registration.character_coordinates_changed
            ),
        );
        window.handle_input(focus, ElementInputHandler::new(bounds, view.clone()), cx);
        if registration.character_coordinates_changed {
            window.invalidate_character_coordinates();
        }
    }
    registration.registered
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor_view::platform_input_registration_allows;
    use cditor_core::rich_text::{
        BlockPayload, BlockPayloadRecord, RichBlockKind, TableCellPayload, TablePayload,
        TableRowPayload,
    };
    use cditor_runtime::DocumentRuntime;

    #[test]
    fn text_activation_preserves_double_and_triple_taps() {
        assert_eq!(text_activation_click_count(0), 1);
        assert_eq!(text_activation_click_count(1), 1);
        assert_eq!(text_activation_click_count(2), 2);
        assert_eq!(text_activation_click_count(3), 3);
    }

    #[test]
    fn completed_mobile_activation_does_not_start_pointer_drag_selection() {
        assert!(!retains_pointer_drag_after_text_activation(true));
        assert!(retains_pointer_drag_after_text_activation(false));
    }

    #[test]
    fn adapter_targets_match_runtime_block_and_table_sessions() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![
                BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "body"),
                BlockPayloadRecord {
                    block_id: 2,
                    content_version: 1,
                    kind: RichBlockKind::Table,
                    payload: BlockPayload::Table(TablePayload {
                        rows: vec![TableRowPayload {
                            cells: vec![TableCellPayload::plain("cell")],
                            height: Default::default(),
                        }],
                        columns: Vec::new(),
                        header_rows: 0,
                        header_cols: 0,
                        header_style: Default::default(),
                    }),
                },
            ],
            720.0,
        );

        crate::test_support::focus_block_at_offset(&mut runtime, 1, 1);
        assert!(platform_input_registration_allows(
            None,
            GuiPlatformInputTarget::BlockText { block_id: 1 },
            &runtime,
        ));
        assert!(!platform_input_registration_allows(
            None,
            GuiPlatformInputTarget::TableCell {
                block_id: 2,
                row: 0,
                col: 0,
            },
            &runtime,
        ));

        crate::test_support::focus_table_cell_at_offset(&mut runtime, 2, 0, 0, 1);
        assert!(platform_input_registration_allows(
            None,
            GuiPlatformInputTarget::TableCell {
                block_id: 2,
                row: 0,
                col: 0,
            },
            &runtime,
        ));
        assert!(!platform_input_registration_allows(
            None,
            GuiPlatformInputTarget::BlockText { block_id: 1 },
            &runtime,
        ));

        let menu = GuiPlatformInputTarget::table_menu_query(2);
        assert!(platform_input_registration_allows(
            Some(menu),
            menu,
            &runtime
        ));
        assert!(!platform_input_registration_allows(
            Some(menu),
            GuiPlatformInputTarget::TableCell {
                block_id: 2,
                row: 0,
                col: 0,
            },
            &runtime,
        ));
        assert!(!platform_input_registration_allows(
            Some(GuiPlatformInputTarget::None),
            GuiPlatformInputTarget::TableCell {
                block_id: 2,
                row: 0,
                col: 0,
            },
            &runtime,
        ));
    }
}
