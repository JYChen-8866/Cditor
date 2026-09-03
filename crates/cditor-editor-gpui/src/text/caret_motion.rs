//! 插入符位移补间。
//!
//! 光标本身是无状态的：每帧从当前 offset 现算矩形。offset 一变，矩形下一帧就
//! 出现在新位置，看起来是闪现。这里记住"上一帧画在哪"，把原点补间过去。
//!
//! 只补间原点，尺寸始终用目标值——行高变化时不该跟着拉伸。

use std::cell::Cell;
use std::time::{Duration, Instant};

use gpui::{Bounds, Pixels, Point, Window, px};

/// 补间时长。60~90ms 是手感区间：再长打字时会觉得糊。
const DURATION: Duration = Duration::from_millis(70);

/// 超过这个横向距离就直接跳。点到行尾、翻页、切 block 时横穿整屏
/// 看起来是坏的而不是顺的。
const SNAP_DISTANCE_PX: f32 = 120.0;

/// 纵向只要动了就当换行处理，直接跳。
const SAME_LINE_TOLERANCE_PX: f32 = 1.0;

fn ease_out_quint(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(5)
}

fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

#[derive(Clone, Copy, Debug)]
struct Motion {
    from: Point<Pixels>,
    to: Point<Pixels>,
    started: Instant,
}

/// 是否该直接跳过去而不补间。
fn should_snap(from: Point<Pixels>, to: Point<Pixels>) -> bool {
    let dy = (f32::from(to.y) - f32::from(from.y)).abs();
    if dy > SAME_LINE_TOLERANCE_PX {
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
}

impl CaretMotion {
    /// 当前该显示的原点。补间跑完返回终点。
    fn displayed_origin(&self, now: Instant) -> Option<Point<Pixels>> {
        let motion = self.motion.get()?;
        let elapsed = now.saturating_duration_since(motion.started);
        if elapsed >= DURATION {
            return Some(motion.to);
        }
        let t = ease_out_quint(elapsed.as_secs_f32() / DURATION.as_secs_f32());
        Some(Point {
            x: px(lerp(f32::from(motion.from.x), f32::from(motion.to.x), t)),
            y: px(lerp(f32::from(motion.from.y), f32::from(motion.to.y), t)),
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
        };
        let Some(current) = self.displayed_origin(now) else {
            // 第一帧：没有历史，直接就位。
            self.motion.set(Some(settled));
            return target;
        };

        let target_changed = self.motion.get().map(|m| m.to) != Some(target.origin);
        if target_changed {
            if should_snap(current, target.origin) {
                self.motion.set(Some(settled));
                return target;
            }
            self.motion.set(Some(Motion {
                from: current,
                to: target.origin,
                started: now,
            }));
        }

        Bounds {
            origin: self.displayed_origin(now).unwrap_or(target.origin),
            size: target.size,
        }
    }

    /// 绘制阶段的入口：把目标矩形换成这一帧该画的矩形，并在补间未完成时
    /// 预约下一帧。
    ///
    /// 光标本身不产生重绘——不预约下一帧的话，补间会停在按键那一刻的位置，
    /// 直到下一次输入才继续。
    pub(crate) fn resolve_and_drive(
        &self,
        target: Bounds<Pixels>,
        window: &Window,
    ) -> Bounds<Pixels> {
        let now = Instant::now();
        let displayed = self.resolve(target, now);
        if self.is_animating(now) {
            window.request_animation_frame();
        }
        displayed
    }

    /// 补间还在跑吗。用来决定要不要请求下一帧。
    pub(crate) fn is_animating(&self, now: Instant) -> bool {
        self.motion.get().is_some_and(|motion| {
            motion.from != motion.to && now.saturating_duration_since(motion.started) < DURATION
        })
    }

    /// 丢掉历史位置。光标消失（失焦、IME 组字中）时调用，
    /// 否则下次出现会从一个过期位置滑过来。
    pub(crate) fn reset(&self) {
        self.motion.set(None);
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
    fn line_change_snaps() {
        let motion = CaretMotion::default();
        let start = Instant::now();
        motion.resolve(bounds(80.0, 0.0), start);
        // 换行：纵向变了，直接跳。
        let next = motion.resolve(bounds(4.0, 18.0), start);
        assert_eq!(next, bounds(4.0, 18.0));
        assert!(!motion.is_animating(start));
    }

    #[test]
    fn long_jump_on_same_line_snaps() {
        let motion = CaretMotion::default();
        let start = Instant::now();
        motion.resolve(bounds(10.0, 0.0), start);
        let far = 10.0 + SNAP_DISTANCE_PX + 1.0;
        assert_eq!(motion.resolve(bounds(far, 0.0), start), bounds(far, 0.0));
        assert!(!motion.is_animating(start));
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
