//! Video controls adapted from Frame's preview timeline.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use cditor_component::SvgIcon;
use cditor_core::ids::BlockId;
use gpui::{
    AnyElement, App, AppContext, Bounds, Context, DragMoveEvent, Element, ElementId,
    GlobalElementId, InteractiveElement, IntoElement, LayoutId, MouseButton, MouseDownEvent,
    ParentElement, Pixels, Position, Render, StatefulInteractiveElement, Style, Styled, Window,
    div, px, relative, size,
};

use crate::{editor_view::CditorV2View, theme::GuiTheme};

const PLAY: &[u8] = include_bytes!("../../../../../assets/icons/play.svg");
const PAUSE: &[u8] = include_bytes!("../../../../../assets/icons/pause.svg");
const VOLUME: &[u8] = include_bytes!("../../../../../assets/icons/volume.svg");
const MUTED: &[u8] = include_bytes!("../../../../../assets/icons/volume-muted.svg");
const FULLSCREEN: &[u8] = include_bytes!("../../../../../assets/icons/fullscreen.svg");
const EXIT_FULLSCREEN: &[u8] = include_bytes!("../../../../../assets/icons/minimize.svg");

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum ControlKind {
    Timeline,
    Volume,
}

#[derive(Clone)]
struct ControlDrag {
    block_id: BlockId,
    kind: ControlKind,
}

struct ControlDragPreview;

impl Render for ControlDragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size(px(0.0))
    }
}

#[derive(Clone, Default)]
pub(super) struct ControlBounds(Arc<Mutex<HashMap<(BlockId, ControlKind), Bounds<Pixels>>>>);

impl ControlBounds {
    pub(super) fn clear(&self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
    pub(super) fn retain(&self, ids: &HashSet<BlockId>) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|(id, _), _| ids.contains(id));
    }
    fn set(&self, block_id: BlockId, kind: ControlKind, bounds: Bounds<Pixels>) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert((block_id, kind), bounds);
    }
    fn fraction(&self, block_id: BlockId, kind: ControlKind, x: Pixels) -> Option<f64> {
        let bounds = *self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&(block_id, kind))?;
        let width = bounds.size.width.as_f32();
        (width > 0.0).then(|| f64::from(((x - bounds.origin.x).as_f32() / width).clamp(0.0, 1.0)))
    }
}

struct BoundsProbe {
    block_id: BlockId,
    kind: ControlKind,
    bounds: ControlBounds,
}

impl IntoElement for BoundsProbe {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}
impl Element for BoundsProbe {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (
            window.request_layout(
                Style {
                    position: Position::Absolute,
                    size: size(relative(1.0).into(), relative(1.0).into()),
                    flex_grow: 1.0,
                    flex_shrink: 1.0,
                    ..Style::default()
                },
                [],
                cx,
            ),
            (),
        )
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
        self.bounds.set(self.block_id, self.kind, bounds)
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
}

pub(super) fn render_video_controls(
    block_id: BlockId,
    width: f32,
    snapshot: Option<cditor_video::VideoPlaybackSnapshot>,
    bounds: ControlBounds,
    theme: GuiTheme,
    view: gpui::Entity<CditorV2View>,
    is_fullscreen: bool,
) -> AnyElement {
    let playing = snapshot.is_some_and(|s| s.playing);
    let muted = snapshot.is_none_or(|s| s.muted);
    let progress = playback_fraction(snapshot);
    let volume = snapshot.map_or(0.0, |s| f64::from(s.volume.clamp(0.0, 1.0)));
    let duration = snapshot.and_then(|s| s.duration_seconds);
    let position = snapshot.map_or(0.0, |s| s.position_seconds);
    let playback_rate = snapshot.map_or(cditor_video::DEFAULT_PLAYBACK_RATE, |s| s.playback_rate);
    let play_view = view.clone();
    let mute_view = view.clone();
    let seek_view = view.clone();
    let volume_view = view.clone();
    let seek_bounds = bounds.clone();
    let volume_bounds = bounds.clone();
    let seek_drag_view = view.clone();
    let volume_drag_view = view.clone();
    let fullscreen_view = view.clone();
    let speed_view = view.clone();
    let play_id = if is_fullscreen {
        "video-fullscreen-playback"
    } else {
        "video-playback"
    };
    let mute_id = if is_fullscreen {
        "video-fullscreen-mute"
    } else {
        "video-mute"
    };
    let fullscreen_id = if is_fullscreen {
        "video-exit-fullscreen"
    } else {
        "video-fullscreen"
    };
    let timeline_id = if is_fullscreen {
        "video-fullscreen-timeline"
    } else {
        "video-timeline"
    };
    let volume_id = if is_fullscreen {
        "video-fullscreen-volume"
    } else {
        "video-volume"
    };
    let speed_id = if is_fullscreen {
        "video-fullscreen-speed"
    } else {
        "video-speed"
    };

    div()
        .w(px(width))
        .h(px(40.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .px(px(8.0))
        .bg(gpui::rgba(0x000000c7))
        .border_t_1()
        .border_color(gpui::rgba(0xffffff24))
        .child(
            icon_button(play_id, if playing { PAUSE } else { PLAY }).on_mouse_down(
                MouseButton::Left,
                move |_, _, cx| {
                    play_view.update(cx, |view, cx| {
                        view.cache.video_playbacks.command(
                            block_id,
                            if playing {
                                cditor_video::VideoCommand::Pause
                            } else {
                                cditor_video::VideoCommand::Play
                            },
                        );
                        if !playing {
                            view.start_video_ticker(block_id, cx)
                        }
                        cx.notify();
                    });
                },
            ),
        )
        .child(
            div()
                .w(px(86.0))
                .text_size(px(11.0))
                .text_color(gpui::rgba(0xffffffe0))
                .child(format!(
                    "{} / {}",
                    format_time(position),
                    duration.map(format_time).unwrap_or_else(|| "--:--".into())
                )),
        )
        .child(
            div()
                .id((timeline_id, block_id))
                .relative()
                .flex_1()
                .h(px(20.0))
                .flex()
                .items_center()
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |event: &MouseDownEvent, _, cx| {
                    let Some(duration) = duration else { return };
                    let Some(fraction) =
                        seek_bounds.fraction(block_id, ControlKind::Timeline, event.position.x)
                    else {
                        return;
                    };
                    seek_view.update(cx, |view, cx| {
                        view.cache.video_playbacks.command(
                            block_id,
                            cditor_video::VideoCommand::Seek(duration * fraction),
                        );
                        if playing {
                            view.start_video_ticker(block_id, cx)
                        }
                        cx.notify();
                    });
                })
                .on_drag(
                    ControlDrag {
                        block_id,
                        kind: ControlKind::Timeline,
                    },
                    |_, _, _, cx| cx.new(|_| ControlDragPreview),
                )
                .on_drag_move(move |event: &DragMoveEvent<ControlDrag>, _, cx| {
                    let (drag_block_id, drag_kind) = {
                        let drag = event.drag(cx);
                        (drag.block_id, drag.kind)
                    };
                    if drag_block_id != block_id || drag_kind != ControlKind::Timeline {
                        return;
                    }
                    let Some(duration) = duration else { return };
                    let fraction = fraction_from_bounds(event.event.position.x, event.bounds);
                    seek_drag_view.update(cx, |view, cx| {
                        view.cache.video_playbacks.command(
                            block_id,
                            cditor_video::VideoCommand::Seek(duration * fraction),
                        );
                        cx.notify();
                    });
                })
                .child(
                    div()
                        .w_full()
                        .h(px(4.0))
                        .rounded(px(2.0))
                        .bg(gpui::rgba(0xffffff52))
                        .child(
                            div()
                                .h_full()
                                .w(relative(progress as f32))
                                .rounded(px(2.0))
                                .bg(gpui::rgb(theme.focused)),
                        ),
                )
                .child(
                    div()
                        .absolute()
                        .left(relative(progress as f32))
                        .ml(px(-5.0))
                        .size(px(10.0))
                        .rounded(px(5.0))
                        .bg(gpui::rgb(theme.focused)),
                )
                .child(BoundsProbe {
                    block_id,
                    kind: ControlKind::Timeline,
                    bounds: bounds.clone(),
                }),
        )
        .child(
            icon_button(mute_id, if muted { MUTED } else { VOLUME }).on_mouse_down(
                MouseButton::Left,
                move |_, _, cx| {
                    mute_view.update(cx, |view, cx| {
                        view.cache
                            .video_playbacks
                            .command(block_id, cditor_video::VideoCommand::SetMuted(!muted));
                        cx.notify();
                    });
                },
            ),
        )
        .child(
            div()
                .id((volume_id, block_id))
                .relative()
                .w(px(54.0))
                .h(px(20.0))
                .flex()
                .items_center()
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |event: &MouseDownEvent, _, cx| {
                    let Some(value) =
                        volume_bounds.fraction(block_id, ControlKind::Volume, event.position.x)
                    else {
                        return;
                    };
                    volume_view.update(cx, |view, cx| {
                        view.cache.video_playbacks.command(
                            block_id,
                            cditor_video::VideoCommand::SetVolume(value as f32),
                        );
                        view.cache
                            .video_playbacks
                            .command(block_id, cditor_video::VideoCommand::SetMuted(false));
                        cx.notify();
                    });
                })
                .on_drag(
                    ControlDrag {
                        block_id,
                        kind: ControlKind::Volume,
                    },
                    |_, _, _, cx| cx.new(|_| ControlDragPreview),
                )
                .on_drag_move(move |event: &DragMoveEvent<ControlDrag>, _, cx| {
                    let (drag_block_id, drag_kind) = {
                        let drag = event.drag(cx);
                        (drag.block_id, drag.kind)
                    };
                    if drag_block_id != block_id || drag_kind != ControlKind::Volume {
                        return;
                    }
                    let value = fraction_from_bounds(event.event.position.x, event.bounds);
                    volume_drag_view.update(cx, |view, cx| {
                        view.cache.video_playbacks.command(
                            block_id,
                            cditor_video::VideoCommand::SetVolume(value as f32),
                        );
                        view.cache
                            .video_playbacks
                            .command(block_id, cditor_video::VideoCommand::SetMuted(false));
                        cx.notify();
                    });
                })
                .child(
                    div()
                        .w_full()
                        .h(px(3.0))
                        .rounded(px(2.0))
                        .bg(gpui::rgba(0xffffff52))
                        .child(
                            div()
                                .h_full()
                                .w(relative(volume as f32))
                                .bg(gpui::rgba(0xffffffff)),
                        ),
                )
                .child(BoundsProbe {
                    block_id,
                    kind: ControlKind::Volume,
                    bounds,
                }),
        )
        .child(
            div()
                .id((speed_id, block_id))
                .w(px(40.0))
                .h(px(28.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .cursor_pointer()
                .text_size(px(11.0))
                .text_color(gpui::rgba(0xffffffff))
                .hover(|element| element.bg(gpui::rgba(0xffffff24)))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    speed_view.update(cx, |view, cx| {
                        view.cache.video_playbacks.command(
                            block_id,
                            cditor_video::VideoCommand::SetPlaybackRate(next_playback_rate(
                                playback_rate,
                            )),
                        );
                        cx.notify();
                    });
                })
                .child(format_playback_rate(playback_rate)),
        )
        .child(
            icon_button(
                fullscreen_id,
                if is_fullscreen {
                    EXIT_FULLSCREEN
                } else {
                    FULLSCREEN
                },
            )
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                fullscreen_view.update(cx, |view, cx| {
                    if is_fullscreen {
                        view.exit_fullscreen_video(window, cx);
                    } else {
                        view.enter_fullscreen_video(block_id, window, cx);
                    }
                });
            }),
        )
        .into_any_element()
}

fn icon_button(id: &'static str, icon: &'static [u8]) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .size(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.0))
        .cursor_pointer()
        .hover(|e| e.bg(gpui::rgba(0xffffff24)))
        .child(
            SvgIcon::new(id, icon)
                .size(px(16.0))
                .color(gpui::rgba(0xffffffff)),
        )
}

fn playback_fraction(snapshot: Option<cditor_video::VideoPlaybackSnapshot>) -> f64 {
    let Some(snapshot) = snapshot else { return 0.0 };
    let Some(duration) = snapshot.duration_seconds else {
        return 0.0;
    };
    (snapshot.position_seconds / duration.max(0.001)).clamp(0.0, 1.0)
}

const PLAYBACK_RATES: [f64; 6] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0];

fn next_playback_rate(current: f64) -> f64 {
    PLAYBACK_RATES
        .iter()
        .position(|rate| (current - rate).abs() < 0.001)
        .map_or(cditor_video::DEFAULT_PLAYBACK_RATE, |index| {
            PLAYBACK_RATES[(index + 1) % PLAYBACK_RATES.len()]
        })
}

fn format_playback_rate(playback_rate: f64) -> String {
    if playback_rate.fract().abs() <= f64::EPSILON {
        format!("{playback_rate:.0}x")
    } else {
        format!("{playback_rate}x")
    }
}

fn fraction_from_bounds(x: Pixels, bounds: Bounds<Pixels>) -> f64 {
    let width = bounds.size.width.as_f32();
    if width <= 0.0 {
        return 0.0;
    }
    f64::from(((x - bounds.origin.x).as_f32() / width).clamp(0.0, 1.0))
}
fn format_time(seconds: f64) -> String {
    if !seconds.is_finite() {
        return "00:00".into();
    }
    let total = seconds.max(0.0).round() as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formats_short_and_long_time() {
        assert_eq!(format_time(65.0), "01:05");
        assert_eq!(format_time(3661.0), "1:01:01");
    }
    #[test]
    fn clamps_playback_fraction() {
        let snapshot = cditor_video::VideoPlaybackSnapshot {
            position_seconds: 20.0,
            duration_seconds: Some(10.0),
            playing: false,
            ended: true,
            volume: 1.0,
            muted: false,
            playback_rate: 1.0,
        };
        assert_eq!(playback_fraction(Some(snapshot)), 1.0);
    }

    #[test]
    fn playback_rate_control_cycles_supported_rates() {
        assert_eq!(next_playback_rate(0.5), 0.75);
        assert_eq!(next_playback_rate(1.0), 1.25);
        assert_eq!(next_playback_rate(2.0), 0.5);
        assert_eq!(format_playback_rate(1.0), "1x");
        assert_eq!(format_playback_rate(1.25), "1.25x");
    }
}
