//! base-256 fractional 顺序键（P1-005/P1-006，ADR-006）。
//!
//! key 是非空字节串，视为 [0,1) 内的 256 进制小数；字典序即文档序。不变量：
//! 不以 0x00 结尾（保证小数与字节串一一对应）。[`OrderKey::between`] 产生
//! 严格介于两界之间的最短 key；[`OrderKey::between_with_entropy`] 追加熵
//! 尾缀用于并发插入消歧；[`rebalanced_keys`] 只为指定数量生成等距短 key，
//! 不触碰任何 Block 身份（总设计 6.1）。

use std::fmt;

use serde::{Deserialize, Serialize};

use super::generator::IdEntropy;

/// 顺序键错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderKeyError {
    /// key 不能为空。
    Empty,
    /// key 不能以 0x00 结尾。
    TrailingZero,
    /// 生成要求 lower < upper。
    NotStrictlyOrdered { lower: OrderKey, upper: OrderKey },
}

impl fmt::Display for OrderKeyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(formatter, "order key must not be empty"),
            Self::TrailingZero => write!(formatter, "order key must not end with 0x00"),
            Self::NotStrictlyOrdered { lower, upper } => {
                write!(
                    formatter,
                    "order keys not strictly ordered: {lower} >= {upper}"
                )
            }
        }
    }
}

impl std::error::Error for OrderKeyError {}

/// fractional 顺序键。`Ord` 即字节字典序，即文档内 sibling 顺序。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OrderKey(Vec<u8>);

impl OrderKey {
    /// 校验并包装既有字节串。
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, OrderKeyError> {
        if bytes.is_empty() {
            return Err(OrderKeyError::Empty);
        }
        if bytes.last() == Some(&0) {
            return Err(OrderKeyError::TrailingZero);
        }
        Ok(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// 生成严格介于两界之间的最短 key。
    ///
    /// `None` 下界表示 0（文档头），`None` 上界表示 1（文档尾）。
    pub fn between(
        lower: Option<&OrderKey>,
        upper: Option<&OrderKey>,
    ) -> Result<OrderKey, OrderKeyError> {
        if let (Some(lower_key), Some(upper_key)) = (lower, upper)
            && lower_key >= upper_key
        {
            return Err(OrderKeyError::NotStrictlyOrdered {
                lower: lower_key.clone(),
                upper: upper_key.clone(),
            });
        }
        let lower_bytes = lower.map(|key| key.0.as_slice()).unwrap_or(&[]);
        let upper_bytes = upper.map(|key| key.0.as_slice()).unwrap_or(&[]);
        let bytes = midpoint(lower_bytes, upper_bytes);
        debug_assert!(!bytes.is_empty() && bytes.last() != Some(&0));
        Ok(Self(bytes))
    }

    /// [`Self::between`] 加两字节熵尾缀：并发插入同一间隙时以熵消歧，
    /// 结果仍严格位于 (lower, upper)。
    pub fn between_with_entropy(
        lower: Option<&OrderKey>,
        upper: Option<&OrderKey>,
        entropy: &mut impl IdEntropy,
    ) -> Result<OrderKey, OrderKeyError> {
        let mut base = Self::between(lower, upper)?.0;
        let mut suffix = [0u8; 2];
        entropy.fill(&mut suffix);
        base.push(suffix[0]);
        // 末字节映射到 1..=255，保持"不以 0x00 结尾"。
        base.push(suffix[1] % 255 + 1);
        debug_assert!(upper.is_none_or(|upper| base.as_slice() < upper.0.as_slice()));
        Ok(Self(base))
    }

    /// 文档头之前插入。
    pub fn before(first: &OrderKey) -> OrderKey {
        Self::between(None, Some(first)).expect("open lower bound is always valid")
    }

    /// 文档尾之后插入。
    pub fn after(last: &OrderKey) -> OrderKey {
        Self::between(Some(last), None).expect("open upper bound is always valid")
    }

    /// 空列表的第一个 key。
    pub fn first() -> OrderKey {
        Self::between(None, None).expect("empty bounds are always valid")
    }
}

impl fmt::Display for OrderKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// 计算严格介于 a 与 b 之间的最短 256 进制小数。
///
/// 空 `a` 表示 0；空 `b` 表示 1。前置条件：a < b（作为小数），由调用方保证。
fn midpoint(a: &[u8], b: &[u8]) -> Vec<u8> {
    // 消去公共前缀（只有 b 非空时才有意义；b 为空表示 1，无前缀可言）。
    if !b.is_empty() {
        let common = a
            .iter()
            .zip(b.iter())
            .take_while(|(left, right)| left == right)
            .count();
        if common > 0 {
            let mut result = b[..common].to_vec();
            result.extend(midpoint(&a[common..], &b[common..]));
            return result;
        }
    }

    let digit_a = u16::from(a.first().copied().unwrap_or(0));
    let digit_b = b.first().map_or(256, |byte| u16::from(*byte));

    if digit_a == digit_b {
        // 仅发生在 a 为空（=0，隐含无穷个 0x00 数位）且 b 以 0x00 开头：
        // 沿用 0x00 并继续在 b 内部下推。
        debug_assert!(
            a.is_empty() && digit_b == 0,
            "midpoint precondition violated"
        );
        let mut result = vec![0u8];
        result.extend(midpoint(&[], &b[1..]));
        return result;
    }
    debug_assert!(digit_a < digit_b, "midpoint precondition violated");

    if digit_b - digit_a > 1 {
        // 间隙 >= 2：单字节中点即严格介于两界。
        let mid = ((digit_a + digit_b) / 2) as u8;
        debug_assert!(mid != 0);
        return vec![mid];
    }

    // 间隙 == 1：沿用下界首字节，剩余部分以 1（空上界）为上界递归。
    let mut result = vec![digit_a as u8];
    result.extend(midpoint(if a.is_empty() { &[] } else { &a[1..] }, &[]));
    result
}

/// 为 `count` 个 sibling 生成等距、尽量短的 key（P1-006 局部 rebalance）。
///
/// 深度取能容纳 `count + 1` 个间隔的最小字节数；返回严格递增序列。调用方
/// 负责把结果按当前顺序赋回 sibling 的 order key，不改变任何 Block 身份。
pub fn rebalanced_keys(count: usize) -> Vec<OrderKey> {
    if count == 0 {
        return Vec::new();
    }
    // depth = ceil(log_256(count + 1))
    let mut depth = 1usize;
    let mut capacity = 256u128;
    while capacity < count as u128 + 1 {
        capacity *= 256;
        depth += 1;
    }
    let step = capacity / (count as u128 + 1);
    (1..=count as u128)
        .map(|index| {
            let value = index * step;
            let mut bytes = Vec::with_capacity(depth);
            for position in (0..depth).rev() {
                bytes.push((value >> (8 * position)) as u8);
            }
            while bytes.last() == Some(&0) {
                bytes.pop();
            }
            debug_assert!(!bytes.is_empty());
            OrderKey(bytes)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 确定性 LCG 熵，仅用于测试。
    struct TestEntropy {
        state: u64,
    }

    impl IdEntropy for TestEntropy {
        fn fill(&mut self, bytes: &mut [u8]) {
            for byte in bytes {
                self.state = self
                    .state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                *byte = (self.state >> 33) as u8;
            }
        }
    }

    fn key(bytes: &[u8]) -> OrderKey {
        OrderKey::from_bytes(bytes.to_vec()).unwrap()
    }

    #[test]
    fn validation_rejects_empty_and_trailing_zero() {
        assert_eq!(OrderKey::from_bytes(vec![]), Err(OrderKeyError::Empty));
        assert_eq!(
            OrderKey::from_bytes(vec![0x80, 0x00]),
            Err(OrderKeyError::TrailingZero)
        );
        assert!(OrderKey::from_bytes(vec![0x00, 0x01]).is_ok());
    }

    #[test]
    fn between_produces_strictly_ordered_shortest_keys() {
        let first = OrderKey::first();
        assert_eq!(first.as_bytes(), [0x80]);

        let mid = OrderKey::between(Some(&key(&[0x40])), Some(&key(&[0x60]))).unwrap();
        assert_eq!(mid.as_bytes(), [0x50]);

        // 相邻单字节：进入下一层深度。
        let deep = OrderKey::between(Some(&key(&[0x40])), Some(&key(&[0x41]))).unwrap();
        assert_eq!(deep.as_bytes(), [0x40, 0x80]);

        // 公共前缀被保留。
        let prefixed =
            OrderKey::between(Some(&key(&[0x40, 0x10])), Some(&key(&[0x40, 0x20]))).unwrap();
        assert_eq!(prefixed.as_bytes(), [0x40, 0x18]);
    }

    #[test]
    fn between_handles_prefix_and_boundary_cases() {
        // 下界是上界的前缀。
        let under_prefix =
            OrderKey::between(Some(&key(&[0x40])), Some(&key(&[0x40, 0x01]))).unwrap();
        assert!(key(&[0x40]) < under_prefix && under_prefix < key(&[0x40, 0x01]));
        assert_eq!(under_prefix.as_bytes(), [0x40, 0x00, 0x80]);

        // 上界只差最后一位 0x01。
        let tiny_gap = OrderKey::between(None, Some(&key(&[0x01]))).unwrap();
        assert_eq!(tiny_gap.as_bytes(), [0x00, 0x80]);

        // 上界 0x00 0x01：最小的合法 key 之一。
        let smaller = OrderKey::between(None, Some(&key(&[0x00, 0x01]))).unwrap();
        assert!(smaller < key(&[0x00, 0x01]));
        assert_eq!(smaller.as_bytes(), [0x00, 0x00, 0x80]);

        // 下界 0xFF。
        let above = OrderKey::after(&key(&[0xFF]));
        assert!(above > key(&[0xFF]));
        assert_eq!(above.as_bytes(), [0xFF, 0x80]);
    }

    #[test]
    fn between_rejects_unordered_bounds() {
        assert!(matches!(
            OrderKey::between(Some(&key(&[0x50])), Some(&key(&[0x40]))),
            Err(OrderKeyError::NotStrictlyOrdered { .. })
        ));
        assert!(matches!(
            OrderKey::between(Some(&key(&[0x50])), Some(&key(&[0x50]))),
            Err(OrderKeyError::NotStrictlyOrdered { .. })
        ));
    }

    #[test]
    fn repeated_head_and_tail_inserts_grow_logarithmically() {
        let mut head = OrderKey::first();
        for _ in 0..64 {
            let next = OrderKey::before(&head);
            assert!(next < head);
            head = next;
        }
        assert!(
            head.as_bytes().len() <= 10,
            "head depth {}",
            head.as_bytes().len()
        );

        let mut tail = OrderKey::first();
        for _ in 0..64 {
            let next = OrderKey::after(&tail);
            assert!(next > tail);
            tail = next;
        }
        assert!(
            tail.as_bytes().len() <= 10,
            "tail depth {}",
            tail.as_bytes().len()
        );
    }

    #[test]
    fn randomized_inserts_preserve_total_order_and_uniqueness() {
        let mut entropy = TestEntropy { state: 0x5eed };
        let mut keys = vec![OrderKey::first()];
        let mut rng_state = 0x1234_5678_u64;
        let mut next_rand = move |bound: usize| {
            rng_state = rng_state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (rng_state >> 33) as usize % bound
        };

        for step in 0..2_000 {
            let position = next_rand(keys.len() + 1);
            let lower = position.checked_sub(1).map(|index| keys[index].clone());
            let upper = keys.get(position).cloned();
            let inserted = if step % 2 == 0 {
                OrderKey::between(lower.as_ref(), upper.as_ref()).unwrap()
            } else {
                OrderKey::between_with_entropy(lower.as_ref(), upper.as_ref(), &mut entropy)
                    .unwrap()
            };
            if let Some(lower) = &lower {
                assert!(*lower < inserted);
            }
            if let Some(upper) = &upper {
                assert!(inserted < *upper);
            }
            keys.insert(position, inserted);
        }

        for pair in keys.windows(2) {
            assert!(
                pair[0] < pair[1],
                "order violated: {} >= {}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn concurrent_inserts_into_same_gap_disambiguate() {
        let lower = key(&[0x40]);
        let upper = key(&[0x41]);
        let mut entropy_a = TestEntropy { state: 1 };
        let mut entropy_b = TestEntropy { state: 2 };

        let from_a =
            OrderKey::between_with_entropy(Some(&lower), Some(&upper), &mut entropy_a).unwrap();
        let from_b =
            OrderKey::between_with_entropy(Some(&lower), Some(&upper), &mut entropy_b).unwrap();

        assert_ne!(from_a, from_b, "entropy suffix must disambiguate");
        for candidate in [&from_a, &from_b] {
            assert!(lower < *candidate && *candidate < upper);
            assert_ne!(candidate.as_bytes().last(), Some(&0));
        }
        // 两个结果之间还能继续插入。
        let (low, high) = if from_a < from_b {
            (&from_a, &from_b)
        } else {
            (&from_b, &from_a)
        };
        let wedged = OrderKey::between(Some(low), Some(high)).unwrap();
        assert!(*low < wedged && wedged < *high);
    }

    #[test]
    fn rebalanced_keys_are_short_even_and_ordered() {
        assert!(rebalanced_keys(0).is_empty());

        let single = rebalanced_keys(1);
        assert_eq!(single[0].as_bytes(), [0x80]);

        let keys = rebalanced_keys(255);
        assert_eq!(keys.len(), 255);
        assert!(keys.iter().all(|key| key.as_bytes().len() == 1));
        for pair in keys.windows(2) {
            assert!(pair[0] < pair[1]);
        }

        let large = rebalanced_keys(4_096);
        assert_eq!(large.len(), 4_096);
        assert!(large.iter().all(|key| key.as_bytes().len() <= 2));
        for pair in large.windows(2) {
            assert!(pair[0] < pair[1]);
        }
        // rebalance 后仍能在任意相邻对之间继续插入。
        let wedged = OrderKey::between(Some(&large[7]), Some(&large[8])).unwrap();
        assert!(large[7] < wedged && wedged < large[8]);
    }

    #[test]
    fn serde_round_trip_preserves_bytes() {
        let original = key(&[0x00, 0x40, 0xFF]);
        let json = serde_json::to_string(&original).expect("serialize");
        let back: OrderKey = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, original);
    }
}
