//! 代码块收起/展开的高度补间。
//!
//! 折叠不是纯绘制：高度要喂进布局引擎，下面每个块的位置都跟着它走。所以补间
//! 期间每帧推一个新高度，下方内容才是跟着一起滑，而不是在末尾跳一下。
//!
//! 关键约束：`mark_dirty` 只在补间结束时发一次。它会 bump revision、排 autosave
//! 和 undo spill，每帧调就是十几次自动保存和十几条 undo。图片拖拽改尺寸
//! （`interaction/image_resize.rs`）走的是同一个模式：过程中只推高度，落定才标脏。

use std::time::{Duration, Instant};

/// 补间时长。比光标位移（70ms）长一些——这里要挪动下方整篇内容，太快会显得跳。
const DURATION: Duration = Duration::from_millis(180);

fn ease_out_quint(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(5)
}

/// 一个代码块正在进行的高度补间。
#[derive(Clone, Copy, Debug)]
pub(crate) struct HeightTween {
    from: f64,
    to: f64,
    started: Instant,
}

impl HeightTween {
    pub(crate) fn new(from: f64, to: f64, now: Instant) -> Self {
        Self {
            from,
            to,
            started: now,
        }
    }

    /// 从**当前高度**起算一个新目标。
    ///
    /// 动画播一半又点一次折叠时，块从它此刻的高度继续走，不会先跳回原处
    /// 再往回滑。
    pub(crate) fn retarget(&self, to: f64, now: Instant) -> Self {
        Self {
            from: self.height(now),
            to,
            started: now,
        }
    }

    /// 这一帧该用的高度。补间跑完返回终点。
    pub(crate) fn height(&self, now: Instant) -> f64 {
        let elapsed = now.saturating_duration_since(self.started);
        if elapsed >= DURATION {
            return self.to;
        }
        let t = ease_out_quint(elapsed.as_secs_f32() / DURATION.as_secs_f32()) as f64;
        self.from + (self.to - self.from) * t
    }

    /// 还在动吗。用来决定要不要再排一帧、以及是否已经可以标脏。
    pub(crate) fn is_animating(&self, now: Instant) -> bool {
        self.from != self.to && now.saturating_duration_since(self.started) < DURATION
    }

    /// 补间的终点高度。落定时用它做权威值，避免用插值算出来的近似数。
    pub(crate) fn target(&self) -> f64 {
        self.to
    }

    /// 两端里"展开"那一端的高度。
    ///
    /// 一端总是折叠高度、另一端总是展开高度，所以取大的那个。折叠时要把展开高度
    /// 存下来做还原值——补间还在飞的时候不能拿当前插值高度去存，否则展开会停在
    /// 一个动画中间值上。
    pub(crate) fn expanded_end(&self) -> f64 {
        self.from.max(self.to)
    }
}

/// 一个代码块进行中的折叠补间，连同回推布局所需的上下文。
#[derive(Clone, Copy, Debug)]
pub(crate) struct CodeCollapseTween {
    pub(crate) tween: HeightTween,
    /// 回推高度时要带的内容版本。布局引擎用它拒绝过期的测量值。
    pub(crate) content_version: u64,
}

/// 补间时长，导出给测试用来一次推到落定。
pub(crate) const COLLAPSE_DURATION: Duration = DURATION;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_at_from_and_ends_at_to() {
        let now = Instant::now();
        let tween = HeightTween::new(300.0, 40.0, now);
        assert_eq!(tween.height(now), 300.0);
        assert_eq!(tween.height(now + DURATION), 40.0);
        assert_eq!(tween.height(now + DURATION * 2), 40.0);
    }

    #[test]
    fn interpolates_between_endpoints() {
        let now = Instant::now();
        let tween = HeightTween::new(300.0, 40.0, now);
        let mid = tween.height(now + Duration::from_millis(90));
        assert!(mid < 300.0 && mid > 40.0, "mid = {mid}");
    }

    #[test]
    fn animating_only_until_duration_elapses() {
        let now = Instant::now();
        let tween = HeightTween::new(300.0, 40.0, now);
        assert!(tween.is_animating(now));
        assert!(tween.is_animating(now + Duration::from_millis(179)));
        assert!(!tween.is_animating(now + DURATION));
    }

    #[test]
    fn zero_length_tween_never_animates() {
        let now = Instant::now();
        let tween = HeightTween::new(120.0, 120.0, now);
        assert!(!tween.is_animating(now));
        assert_eq!(tween.height(now), 120.0);
    }

    #[test]
    fn retarget_continues_from_current_height() {
        let now = Instant::now();
        let collapsing = HeightTween::new(300.0, 40.0, now);
        let half = now + Duration::from_millis(90);
        let at_half = collapsing.height(half);

        // 播到一半反向展开：从此刻的高度接着走，不先跳回 300。
        let expanding = collapsing.retarget(300.0, half);
        assert_eq!(expanding.height(half), at_half);
        assert_eq!(expanding.height(half + DURATION), 300.0);
    }

    #[test]
    fn target_reports_the_authoritative_endpoint() {
        let now = Instant::now();
        assert_eq!(HeightTween::new(300.0, 40.0, now).target(), 40.0);
    }

    #[test]
    fn expanded_end_is_the_taller_side_regardless_of_direction() {
        let now = Instant::now();
        assert_eq!(HeightTween::new(300.0, 40.0, now).expanded_end(), 300.0);
        assert_eq!(HeightTween::new(40.0, 300.0, now).expanded_end(), 300.0);

        // 折叠播一半又反向：展开端仍是原始全高，不是当前插值。
        let half = now + Duration::from_millis(90);
        let reversed = HeightTween::new(300.0, 40.0, now).retarget(300.0, half);
        assert_eq!(reversed.expanded_end(), 300.0);
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

    #[test]
    fn expanding_tween_grows_monotonically() {
        let now = Instant::now();
        let tween = HeightTween::new(40.0, 300.0, now);
        let mut prev = 0.0;
        for step in 0..=18 {
            let height = tween.height(now + Duration::from_millis(step * 10));
            assert!(height >= prev, "shrank at step {step}: {height} < {prev}");
            prev = height;
        }
        assert_eq!(tween.height(now + DURATION), 300.0);
    }
}
