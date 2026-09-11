//! 插入符位移补间。
//!
//! 光标本身是无状态的：每帧从当前 offset 现算矩形。offset 一变，矩形下一帧就
//! 出现在新位置，看起来是闪现。这里记住"上一帧画在哪"，把原点补间过去。
//!
//! 只补间原点，尺寸始终用目标值——行高变化时不该跟着拉伸。

use std::cell::Cell;
use std::time::{Duration, Instant};

use gpui::{Bounds, Pixels, Point, Window, px};

/// 同行小步移动的补间时长。
const DURATION: Duration = Duration::from_millis(70);
/// 跨行或跨 block 的 transition 总时长。
const LONG_JUMP_DURATION: Duration = Duration::from_millis(200);
const LONG_JUMP_FADE_OUT_DURATION: Duration = Duration::from_millis(60);
const LONG_JUMP_FADE_IN_START: Duration = Duration::from_millis(140);
const LONG_JUMP_FADE_IN_OFFSET_PX: f32 = 6.0;
const LONG_JUMP_SCROLL_START: Duration = Duration::from_millis(40);
const LONG_JUMP_SCROLL_END: Duration = Duration::from_millis(180);
const VERTICAL_MOVE_DURATION: Duration = Duration::from_millis(90);

/// 超过这个横向距离视为长距离移动，而不是纯同行打字。
const SNAP_DISTANCE_PX: f32 = 120.0;

/// 纵向只要动了就当换行处理，直接跳。
const SAME_LINE_TOLERANCE_PX: f32 = 1.0;

/// 相邻行和相邻 block 的纵向移动仍做位置补间；只有超大跨度才淡出淡入。
const MAX_VERTICAL_SLIDE_DISTANCE_PX: f32 = 96.0;

fn ease_out_quint(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(5)
}

fn ease_in_out_cubic(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

#[derive(Clone, Copy, Debug)]
struct Motion {
    from: Point<Pixels>,
    to: Point<Pixels>,
    started: Instant,
    duration: Duration,
    long_jump: bool,
}

#[derive(Clone, Copy, Debug)]
struct MotionSample {
    origin: Point<Pixels>,
    opacity: f32,
}

#[derive(Clone, Copy, Debug)]
struct ScrollMotion {
    from: f64,
    to: f64,
    started: Instant,
}

fn is_long_jump(from: Point<Pixels>, to: Point<Pixels>) -> bool {
    let dy = (f32::from(to.y) - f32::from(from.y)).abs();
    if dy > MAX_VERTICAL_SLIDE_DISTANCE_PX {
        return true;
    }
    let dx = (f32::from(to.x) - f32::from(from.x)).abs();
    dx > SNAP_DISTANCE_PX
}

/// 记住光标上一帧的显示位置，把移动补间成滑动。
///
/// 状态放在 `Cell` 里：补间要在 element 的 prepaint 阶段推进，那时只拿得到
/// 视图的只读引用（`Entity::update` 在视图已被借用时会 panic）。
#[derive(Default)]
pub(crate) struct CaretMotion {
    motion: Cell<Option<Motion>>,
    scroll_motion: Cell<Option<ScrollMotion>>,
}

impl CaretMotion {
    /// 当前该显示的原点。补间跑完返回终点。
    fn sample(&self, now: Instant) -> Option<MotionSample> {
        let motion = self.motion.get()?;
        let elapsed = now.saturating_duration_since(motion.started);
        if elapsed >= motion.duration {
            return Some(MotionSample {
                origin: motion.to,
                opacity: 1.0,
            });
        }
        if motion.long_jump {
            if elapsed < LONG_JUMP_FADE_OUT_DURATION {
                let t = elapsed.as_secs_f32() / LONG_JUMP_FADE_OUT_DURATION.as_secs_f32();
                return Some(MotionSample {
                    origin: motion.from,
                    opacity: 1.0 - ease_in_out_cubic(t),
                });
            }
            if elapsed < LONG_JUMP_FADE_IN_START {
                return Some(MotionSample {
                    origin: motion.from,
                    opacity: 0.0,
                });
            }
            let t = (elapsed - LONG_JUMP_FADE_IN_START).as_secs_f32()
                / (motion.duration - LONG_JUMP_FADE_IN_START).as_secs_f32();
            let t = ease_in_out_cubic(t);
            return Some(MotionSample {
                origin: Point {
                    x: motion.to.x,
                    y: motion.to.y - px(LONG_JUMP_FADE_IN_OFFSET_PX * (1.0 - t)),
                },
                opacity: t,
            });
        }
        let t = elapsed.as_secs_f32() / motion.duration.as_secs_f32();
        let t = ease_out_quint(t);
        Some(MotionSample {
            origin: Point {
                x: px(lerp(f32::from(motion.from.x), f32::from(motion.to.x), t)),
                y: px(lerp(f32::from(motion.from.y), f32::from(motion.to.y), t)),
            },
            opacity: 1.0,
        })
    }

    /// 给定这一帧算出的真实光标矩形，返回该画在哪。
    ///
    /// 目标变了就从**当前显示位置**起补间，不是从上一个目标起——这样连打时
    /// 每次按键光标从它此刻所在的地方继续走，不会往回跳。
    pub(crate) fn resolve(&self, target: Bounds<Pixels>, now: Instant) -> Bounds<Pixels> {
        let settled = Motion {
            from: target.origin,
            to: target.origin,
            started: now,
            duration: DURATION,
            long_jump: false,
        };
        let Some(current) = self.sample(now) else {
            // 第一帧：没有历史，直接就位。
            self.motion.set(Some(settled));
            return target;
        };

        let target_changed = self.motion.get().map(|m| m.to) != Some(target.origin);
        if target_changed {
            let dy = (f32::from(target.origin.y) - f32::from(current.origin.y)).abs();
            let long_jump = is_long_jump(current.origin, target.origin);
            crate::diagnostics::stderr::write(format_args!(
                "[cditor][caret][caret_motion.start] from={:?} to={:?} dy={dy:.2} long_jump={long_jump}",
                current.origin, target.origin
            ));
            let duration = if long_jump {
                LONG_JUMP_DURATION
            } else if dy > SAME_LINE_TOLERANCE_PX {
                VERTICAL_MOVE_DURATION
            } else {
                DURATION
            };
            self.motion.set(Some(Motion {
                from: current.origin,
                to: target.origin,
                started: now,
                duration,
                long_jump,
            }));
        }

        let sample = self.sample(now).unwrap_or(MotionSample {
            origin: target.origin,
            opacity: 1.0,
        });
        Bounds {
            origin: sample.origin,
            size: target.size,
        }
    }

    pub(crate) fn resolve_with_opacity(
        &self,
        target: Bounds<Pixels>,
        now: Instant,
    ) -> (Bounds<Pixels>, f32) {
        let bounds = self.resolve(target, now);
        let opacity = self.sample(now).map_or(1.0, |sample| sample.opacity);
        (bounds, opacity)
    }

    pub(crate) fn resolve_with_opacity_and_drive(
        &self,
        target: Bounds<Pixels>,
        window: &Window,
    ) -> (Bounds<Pixels>, f32, bool) {
        let now = Instant::now();
        let sample = self.resolve_with_opacity(target, now);
        let animating = self.is_animating(now);
        if animating {
            window.request_animation_frame();
        }
        (sample.0, sample.1, animating)
    }

    /// 补间还在跑吗。用来决定要不要请求下一帧。
    pub(crate) fn is_animating(&self, now: Instant) -> bool {
        self.scroll_is_animating(now)
            || self.motion.get().is_some_and(|motion| {
                motion.from != motion.to
                    && now.saturating_duration_since(motion.started) < motion.duration
            })
    }

    /// 丢掉历史位置。光标消失（失焦、IME 组字中）时调用，
    /// 否则下次出现会从一个过期位置滑过来。
    pub(crate) fn reset(&self) {
        self.motion.set(None);
        self.scroll_motion.set(None);
    }

    pub(crate) fn begin_scroll_transition(&self, from: f64, to: f64) {
        self.begin_scroll_transition_at(from, to, Instant::now());
    }

    fn begin_scroll_transition_at(&self, from: f64, to: f64, started: Instant) {
        if (from - to).abs() < 0.5 {
            self.scroll_motion.set(None);
            return;
        }
        self.scroll_motion
            .set(Some(ScrollMotion { from, to, started }));
    }

    pub(crate) fn presented_scroll_top(&self, truth_scroll_top: f64, now: Instant) -> Option<f64> {
        let motion = self.scroll_motion.get()?;
        let elapsed = now.saturating_duration_since(motion.started);
        if elapsed <= LONG_JUMP_SCROLL_START {
            return Some(motion.from);
        }
        if elapsed >= LONG_JUMP_SCROLL_END {
            self.scroll_motion.set(None);
            return Some(truth_scroll_top);
        }
        let t = (elapsed - LONG_JUMP_SCROLL_START).as_secs_f32()
            / (LONG_JUMP_SCROLL_END - LONG_JUMP_SCROLL_START).as_secs_f32();
        let t = ease_in_out_cubic(t);
        Some(motion.from + (motion.to - motion.from) * f64::from(t))
    }

    fn scroll_is_animating(&self, now: Instant) -> bool {
        self.scroll_motion.get().is_some_and(|motion| {
            now.saturating_duration_since(motion.started) < LONG_JUMP_SCROLL_END
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::size;

    fn bounds(x: f32, y: f32) -> Bounds<Pixels> {
        Bounds {
            origin: Point { x: px(x), y: px(y) },
            size: size(px(2.0), px(18.0)),
        }
    }

    #[test]
    fn first_frame_lands_on_target_without_animating() {
        let motion = CaretMotion::default();
        let now = Instant::now();
        assert_eq!(motion.resolve(bounds(10.0, 0.0), now), bounds(10.0, 0.0));
        assert!(!motion.is_animating(now));
    }

    #[test]
    fn typing_one_char_tweens_from_previous_position() {
        let motion = CaretMotion::default();
        let start = Instant::now();
        motion.resolve(bounds(10.0, 0.0), start);

        // 目标跳到 18，这一帧应该还在起点附近。
        let first = motion.resolve(bounds(18.0, 0.0), start);
        assert_eq!(f32::from(first.origin.x), 10.0);
        assert!(motion.is_animating(start));

        // 中途在两点之间。
        let mid = motion.resolve(bounds(18.0, 0.0), start + Duration::from_millis(35));
        let mid_x = f32::from(mid.origin.x);
        assert!(mid_x > 10.0 && mid_x < 18.0, "mid_x = {mid_x}");

        // 跑完就位并退休。
        let done = motion.resolve(bounds(18.0, 0.0), start + DURATION);
        assert_eq!(f32::from(done.origin.x), 18.0);
        assert!(!motion.is_animating(start + DURATION));
    }

    #[test]
    fn size_always_comes_from_target() {
        let motion = CaretMotion::default();
        let start = Instant::now();
        motion.resolve(bounds(10.0, 0.0), start);
        let mut taller = bounds(18.0, 0.0);
        taller.size.height = px(30.0);
        assert_eq!(motion.resolve(taller, start).size.height, px(30.0));
    }

    #[test]
    fn short_vertical_move_slides_between_lines() {
        let motion = CaretMotion::default();
        let start = Instant::now();
        motion.resolve(bounds(80.0, 0.0), start);

        let (next, opacity) = motion.resolve_with_opacity(bounds(4.0, 24.0), start);
        assert_eq!(next.origin, bounds(80.0, 0.0).origin);
        assert_eq!(opacity, 1.0);

        let (mid, opacity) =
            motion.resolve_with_opacity(bounds(4.0, 24.0), start + VERTICAL_MOVE_DURATION / 2);
        assert!(f32::from(mid.origin.y) > 0.0 && f32::from(mid.origin.y) < 24.0);
        assert_eq!(opacity, 1.0);
        assert!(motion.is_animating(start + VERTICAL_MOVE_DURATION / 2));

        let done = motion.resolve(bounds(4.0, 24.0), start + VERTICAL_MOVE_DURATION);
        assert_eq!(done, bounds(4.0, 24.0));
        assert!(!motion.is_animating(start + VERTICAL_MOVE_DURATION));
    }

    #[test]
    fn distant_jump_fades_out_then_fades_in_at_the_target() {
        let motion = CaretMotion::default();
        let start = Instant::now();
        motion.resolve(bounds(80.0, 0.0), start);
        let (next, opacity) = motion.resolve_with_opacity(bounds(4.0, 180.0), start);
        assert_eq!(next.origin, bounds(80.0, 0.0).origin);
        assert_eq!(opacity, 1.0);
        assert!(motion.is_animating(start));

        let (fading_out, opacity) = motion
            .resolve_with_opacity(bounds(4.0, 180.0), start + LONG_JUMP_FADE_OUT_DURATION / 2);
        assert_eq!(fading_out.origin, bounds(80.0, 0.0).origin);
        assert!(opacity > 0.0 && opacity < 1.0);

        let (hidden, opacity) =
            motion.resolve_with_opacity(bounds(4.0, 180.0), start + LONG_JUMP_FADE_OUT_DURATION);
        assert_eq!(hidden.origin, bounds(80.0, 0.0).origin);
        assert_eq!(opacity, 0.0);

        let (arriving, opacity) = motion.resolve_with_opacity(
            bounds(4.0, 180.0),
            start + LONG_JUMP_FADE_IN_START + Duration::from_millis(30),
        );
        assert_eq!(arriving.origin.x, bounds(4.0, 180.0).origin.x);
        assert!(f32::from(arriving.origin.y) > 174.0 && f32::from(arriving.origin.y) < 180.0);
        assert!(opacity > 0.0 && opacity < 1.0);

        let done = motion.resolve(bounds(4.0, 180.0), start + LONG_JUMP_DURATION);
        assert_eq!(done, bounds(4.0, 180.0));
        let (_, opacity) =
            motion.resolve_with_opacity(bounds(4.0, 180.0), start + LONG_JUMP_DURATION);
        assert_eq!(opacity, 1.0);
        assert!(!motion.is_animating(start + LONG_JUMP_DURATION));
    }

    #[test]
    fn long_jump_on_same_line_uses_the_same_fade_transition() {
        let motion = CaretMotion::default();
        let start = Instant::now();
        motion.resolve(bounds(10.0, 0.0), start);
        let far = 10.0 + SNAP_DISTANCE_PX + 1.0;
        assert_eq!(motion.resolve(bounds(far, 0.0), start).origin.x, px(10.0));
        assert!(motion.is_animating(start));
        let (_, opacity) =
            motion.resolve_with_opacity(bounds(far, 0.0), start + LONG_JUMP_FADE_OUT_DURATION / 2);
        assert!(opacity > 0.0 && opacity < 1.0);
        let (done, opacity) =
            motion.resolve_with_opacity(bounds(far, 0.0), start + LONG_JUMP_DURATION);
        assert_eq!(done, bounds(far, 0.0));
        assert_eq!(opacity, 1.0);
    }

    #[test]
    fn reversal_starts_from_current_position_not_old_origin() {
        let motion = CaretMotion::default();
        let start = Instant::now();
        motion.resolve(bounds(0.0, 0.0), start);
        motion.resolve(bounds(20.0, 0.0), start);

        // 滑到一半反向。
        let half = start + Duration::from_millis(35);
        let at_half = f32::from(motion.resolve(bounds(20.0, 0.0), half).origin.x);
        let reversed = motion.resolve(bounds(0.0, 0.0), half);
        // 从当前位置起算，不是从 20 跳回去。
        assert_eq!(f32::from(reversed.origin.x), at_half);
    }

    #[test]
    fn reset_drops_history() {
        let motion = CaretMotion::default();
        let start = Instant::now();
        motion.resolve(bounds(100.0, 0.0), start);
        motion.reset();
        // 复位后第一帧直接就位，不从 100 滑过来。
        assert_eq!(motion.resolve(bounds(4.0, 0.0), start), bounds(4.0, 0.0));
        assert!(!motion.is_animating(start));
    }

    #[test]
    fn document_jump_scroll_waits_then_eases_to_truth() {
        let motion = CaretMotion::default();
        let start = Instant::now();
        motion.begin_scroll_transition_at(800.0, 0.0, start);

        assert_eq!(motion.presented_scroll_top(0.0, start), Some(800.0));
        assert_eq!(
            motion.presented_scroll_top(0.0, start + LONG_JUMP_SCROLL_START),
            Some(800.0)
        );
        let mid = motion
            .presented_scroll_top(
                0.0,
                start + (LONG_JUMP_SCROLL_START + LONG_JUMP_SCROLL_END) / 2,
            )
            .unwrap();
        assert!(mid > 0.0 && mid < 800.0, "mid = {mid}");
        assert_eq!(
            motion.presented_scroll_top(0.0, start + LONG_JUMP_SCROLL_END),
            Some(0.0)
        );
    }

    #[test]
    fn easing_is_monotonic_and_bounded() {
        assert_eq!(ease_out_quint(0.0), 0.0);
        assert_eq!(ease_out_quint(1.0), 1.0);
        let mut prev = 0.0;
        for step in 1..=20 {
            let value = ease_out_quint(step as f32 / 20.0);
            assert!(value >= prev, "not monotonic at {step}");
            prev = value;
        }
    }
}
