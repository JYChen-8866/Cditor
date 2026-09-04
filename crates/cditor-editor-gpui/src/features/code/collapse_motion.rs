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
const DURATION: Duration = Duration::from_millis(200);

/// 出缓动，三次方。
///
/// 别换成更高次方。曲线的阶数决定的不是"快慢"而是**帧与帧之间的推进是否均匀**：
/// 按 60Hz 把 200ms 拆成 12 帧，cubic 的首帧推进 ~23%，之后逐帧递减；五次方
/// （quint）首帧就吃掉 ~38%，四帧内跑完 90%，剩下一半时间在磨最后几个百分点。
/// 那些尾帧的位移小到人眼看不出在动，观感就从"一段平滑的动画"塌成"先弹开一大半，
/// 停一下，再补完"——即使代码里只有一个补间。`advance_bound` 那条测试就是钉这个
/// 性质的。
fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
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
        let t = ease_out_cubic(elapsed.as_secs_f32() / DURATION.as_secs_f32()) as f64;
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
    /// **本帧**的高度，由每帧的补间驱动写入一次。
    ///
    /// 一帧之内所有读高度的地方都必须用这个值，不许各自去 `tween.height(now)`
    /// 重采样。布局用 H(t₁) 补偿了锚点，渲染却用 H(t₂) 定位，两者差多少、
    /// 视口就漂多少；而补间单调，折叠与展开的差符号相反，于是同一个按钮会朝
    /// 相反方向跑。一帧一个时间戳、一个高度真相，这个漂移才不存在。
    pub(crate) frame_height: f64,
}

impl CodeCollapseTween {
    /// 起一个新补间，本帧高度取补间起点。
    pub(crate) fn start(tween: HeightTween, content_version: u64, now: Instant) -> Self {
        Self {
            tween,
            content_version,
            frame_height: tween.height(now),
        }
    }

    /// 把本帧的高度采样一次并记下来。返回值就是本帧该用的高度。
    pub(crate) fn sample_frame(&mut self, now: Instant) -> f64 {
        self.frame_height = self.tween.height(now);
        self.frame_height
    }
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
    fn start_records_the_tween_start_as_this_frames_height() {
        let now = Instant::now();
        let tween = CodeCollapseTween::start(HeightTween::new(300.0, 40.0, now), 7, now);

        assert_eq!(tween.frame_height, 300.0);
        assert_eq!(tween.content_version, 7);
    }

    /// 采样一次之后，本帧高度必须冻结。
    ///
    /// 这是防"按钮乱跑"的核心不变量：布局按 H(t₁) 补偿锚点，渲染必须也用
    /// H(t₁) 定位。任何消费方回去自己 `tween.height(now)` 拿到的都是更晚的
    /// H(t₂)，两者之差就是视口漂移量。
    #[test]
    fn frame_height_stays_frozen_between_samples() {
        let now = Instant::now();
        let mut tween = CodeCollapseTween::start(HeightTween::new(300.0, 40.0, now), 1, now);

        let sampled = tween.sample_frame(now + Duration::from_millis(60));

        // 时间在走，但没再采样——本帧高度不许变。
        assert_eq!(tween.frame_height, sampled);
        assert_eq!(tween.frame_height, sampled, "重复读取必须得到同一个值");
        assert!(
            tween.tween.height(now + Duration::from_millis(120)) != sampled,
            "补间本身应当仍在推进，否则这个测试证明不了冻结"
        );
    }

    /// 折叠与展开的采样误差符号相反——这正是按钮"忽上忽下"的由来。
    #[test]
    fn resampling_later_drifts_in_opposite_directions_for_collapse_and_expand() {
        let now = Instant::now();
        let early = now + Duration::from_millis(40);
        let late = now + Duration::from_millis(70);

        let collapsing = HeightTween::new(300.0, 40.0, now);
        let expanding = HeightTween::new(40.0, 300.0, now);

        // 同一帧内若两处用了不同时间戳，折叠得到负差、展开得到正差。
        assert!(collapsing.height(late) - collapsing.height(early) < 0.0);
        assert!(expanding.height(late) - expanding.height(early) > 0.0);
    }

    #[test]
    fn sample_frame_tracks_the_tween_until_settle() {
        let now = Instant::now();
        let mut tween = CodeCollapseTween::start(HeightTween::new(300.0, 40.0, now), 1, now);

        let mut previous = tween.frame_height;
        for step in 1..=6 {
            let sampled = tween.sample_frame(now + Duration::from_millis(step * 30));
            assert!(sampled <= previous, "折叠过程中高度不该回升");
            previous = sampled;
        }
        assert_eq!(tween.sample_frame(now + DURATION), 40.0);
    }

    #[test]
    fn easing_is_monotonic_and_bounded() {
        assert_eq!(ease_out_cubic(0.0), 0.0);
        assert_eq!(ease_out_cubic(1.0), 1.0);
        let mut prev = 0.0;
        for step in 1..=20 {
            let value = ease_out_cubic(step as f32 / 20.0);
            assert!(value >= prev, "not monotonic at {step}");
            prev = value;
        }
    }

    /// 任何单帧的推进都不许超过全程的 25%。
    ///
    /// 这条钉的是"动画看起来是一段还是两段"。曲线越陡，头几帧吃掉的比例越大，尾帧
    /// 就越接近 0 位移——人眼把那段读成停顿，于是一个补间被看成两段。25% 是 cubic
    /// 在 60Hz 下的首帧值（~23%）之上留一点余量；换成五次方会直接顶到 38% 而挂掉。
    #[test]
    fn no_single_frame_advances_more_than_a_quarter() {
        const FRAME: Duration = Duration::from_micros(16_667);
        let now = Instant::now();
        let tween = HeightTween::new(0.0, 1_000.0, now);

        let mut prev = tween.height(now);
        let mut frame = 1;
        loop {
            let at = now + FRAME * frame;
            let height = tween.height(at);
            let advance = height - prev;
            assert!(
                advance <= 250.0,
                "frame {frame} advanced {advance:.1} of 1000 (>25%)"
            );
            if !tween.is_animating(at) {
                break;
            }
            prev = height;
            frame += 1;
        }
    }

    #[test]
    fn expanding_tween_grows_monotonically() {
        let now = Instant::now();
        let tween = HeightTween::new(40.0, 300.0, now);
        let mut prev = 0.0;
        for step in 0..=20 {
            let height = tween.height(now + Duration::from_millis(step * 10));
            assert!(height >= prev, "shrank at step {step}: {height} < {prev}");
            prev = height;
        }
        assert_eq!(tween.height(now + DURATION), 300.0);
    }
}
