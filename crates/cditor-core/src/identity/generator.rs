//! UUIDv7 单调生成器（P1-004，ADR-006，RFC 9562 Method 3）。
//!
//! rand_a 的 12 位作为同毫秒单调计数；时钟回拨时冻结在已见过的最大毫秒并
//! 继续计数，计数溢出则毫秒 +1；62 位 rand_b 每次取新熵。时间源与熵源可
//! 注入，用于回拨/溢出/碰撞的确定性测试。

use uuid::Uuid;

use super::persistent_id::PersistentId;

/// 毫秒时间源。
pub trait IdClock {
    fn unix_millis(&mut self) -> u64;
}

/// 熵源；`fill` 必须完全填充缓冲区。
pub trait IdEntropy {
    fn fill(&mut self, bytes: &mut [u8]);
}

/// 生产时间源：`SystemTime::now()`。
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl IdClock for SystemClock {
    fn unix_millis(&mut self) -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0)
    }
}

/// 生产熵源：操作系统 CSPRNG。
#[derive(Debug, Default, Clone, Copy)]
pub struct OsEntropy;

impl IdEntropy for OsEntropy {
    fn fill(&mut self, bytes: &mut [u8]) {
        getrandom::fill(bytes).expect("operating system entropy must be available");
    }
}

/// 单设备内单调的 UUIDv7 生成器。
///
/// 不是线程安全类型；每个生成线程（或全局互斥内）各持一个实例。跨设备唯一性
/// 由 62 位新鲜熵保证，不依赖设备间时钟同步。
#[derive(Debug)]
pub struct PersistentIdGenerator<C: IdClock, E: IdEntropy> {
    clock: C,
    entropy: E,
    last_millis: u64,
    counter: u16,
    counter_initialized: bool,
}

impl PersistentIdGenerator<SystemClock, OsEntropy> {
    pub fn system() -> Self {
        Self::with_sources(SystemClock, OsEntropy)
    }
}

impl<C: IdClock, E: IdEntropy> PersistentIdGenerator<C, E> {
    pub fn with_sources(clock: C, entropy: E) -> Self {
        Self {
            clock,
            entropy,
            last_millis: 0,
            counter: 0,
            counter_initialized: false,
        }
    }

    /// 生成下一个严格递增的 UUIDv7。
    pub fn next_id(&mut self) -> PersistentId {
        let now = self.clock.unix_millis();
        if !self.counter_initialized || now > self.last_millis {
            // 新毫秒：计数从熵重新播种（保留高位余量，见 seed_counter）。
            self.last_millis = now;
            self.counter = self.seed_counter();
            self.counter_initialized = true;
        } else {
            // 同毫秒或时钟回拨：冻结在已见过的最大毫秒并递增计数。
            if self.counter >= 0x0FFF {
                self.last_millis += 1;
                self.counter = self.seed_counter();
            } else {
                self.counter += 1;
            }
        }

        let mut rand_b = [0u8; 8];
        self.entropy.fill(&mut rand_b);

        PersistentId::from_uuid(build_v7(self.last_millis, self.counter, rand_b))
    }

    fn seed_counter(&mut self) -> u16 {
        let mut seed = [0u8; 2];
        self.entropy.fill(&mut seed);
        // 12 位计数只播种低 11 位，保证同毫秒至少还有 2048 次递增空间。
        u16::from_be_bytes(seed) & 0x07FF
    }
}

/// 按 RFC 9562 布局拼装 UUIDv7：48 位毫秒 + 12 位计数（rand_a）+ 62 位熵。
fn build_v7(millis: u64, counter: u16, rand_b: [u8; 8]) -> Uuid {
    let mut bytes = [0u8; 16];
    bytes[0] = (millis >> 40) as u8;
    bytes[1] = (millis >> 32) as u8;
    bytes[2] = (millis >> 24) as u8;
    bytes[3] = (millis >> 16) as u8;
    bytes[4] = (millis >> 8) as u8;
    bytes[5] = millis as u8;
    // version(7) + rand_a 高 4 位。
    bytes[6] = 0x70 | ((counter >> 8) as u8 & 0x0F);
    bytes[7] = counter as u8;
    // variant(10) + rand_b。
    bytes[8] = 0x80 | (rand_b[0] & 0x3F);
    bytes[9..16].copy_from_slice(&rand_b[1..8]);
    Uuid::from_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    struct ScriptedClock {
        samples: Vec<u64>,
        cursor: usize,
    }

    impl ScriptedClock {
        fn new(samples: Vec<u64>) -> Self {
            Self { samples, cursor: 0 }
        }
    }

    impl IdClock for ScriptedClock {
        fn unix_millis(&mut self) -> u64 {
            let value = self.samples[self.cursor.min(self.samples.len() - 1)];
            self.cursor += 1;
            value
        }
    }

    /// 确定性 LCG 熵，仅用于测试。
    struct TestEntropy {
        state: u64,
    }

    impl TestEntropy {
        fn new(seed: u64) -> Self {
            Self { state: seed }
        }
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

    fn generator(samples: Vec<u64>) -> PersistentIdGenerator<ScriptedClock, TestEntropy> {
        PersistentIdGenerator::with_sources(ScriptedClock::new(samples), TestEntropy::new(0x5eed))
    }

    #[test]
    fn ids_are_strictly_increasing_within_one_millisecond() {
        let mut generator = generator(vec![1_000; 64]);
        let ids: Vec<_> = (0..64).map(|_| generator.next_id()).collect();
        for pair in ids.windows(2) {
            assert!(pair[0] < pair[1], "{} !< {}", pair[0], pair[1]);
        }
        assert!(ids.iter().all(|id| id.unix_millis() == Some(1_000)));
    }

    #[test]
    fn clock_rollback_does_not_move_ids_backwards() {
        let mut generator = generator(vec![2_000, 2_001, 1_500, 1_400, 2_002]);
        let ids: Vec<_> = (0..5).map(|_| generator.next_id()).collect();
        for pair in ids.windows(2) {
            assert!(pair[0] < pair[1], "rollback produced regression");
        }
        // 回拨期间毫秒冻结在已见过的最大值。
        assert_eq!(ids[2].unix_millis(), Some(2_001));
        assert_eq!(ids[3].unix_millis(), Some(2_001));
        assert_eq!(ids[4].unix_millis(), Some(2_002));
    }

    #[test]
    fn counter_overflow_carries_into_next_millisecond() {
        let mut generator = generator(vec![3_000; 6_000]);
        let ids: Vec<_> = (0..6_000).map(|_| generator.next_id()).collect();
        for pair in ids.windows(2) {
            assert!(pair[0] < pair[1]);
        }
        // 播种最多 0x07FF，同毫秒容量至少 2048；6000 个必然进位到后续毫秒。
        let last_millis = ids.last().unwrap().unix_millis().unwrap();
        assert!(
            last_millis > 3_000,
            "expected carry, stayed at {last_millis}"
        );
        assert!(last_millis <= 3_003);
    }

    #[test]
    fn generated_ids_are_valid_v7_and_unique() {
        let mut generator = generator((0..1_000).map(|i| 5_000 + i / 3).collect());
        let mut seen = HashSet::new();
        for _ in 0..1_000 {
            let id = generator.next_id();
            assert_eq!(id.as_uuid().get_version_num(), 7);
            assert_eq!(id.as_uuid().get_variant(), uuid::Variant::RFC4122);
            assert!(seen.insert(id), "duplicate id {id}");
        }
    }

    #[test]
    fn independent_devices_do_not_collide() {
        // 两台设备时钟相同、熵种子不同：62 位新鲜熵保证不同。
        let mut device_a = generator(vec![9_000; 512]);
        let mut device_b = PersistentIdGenerator::with_sources(
            ScriptedClock::new(vec![9_000; 512]),
            TestEntropy::new(0x0123_4567_89ab_cdef),
        );
        let ids_a: HashSet<_> = (0..512).map(|_| device_a.next_id()).collect();
        let ids_b: HashSet<_> = (0..512).map(|_| device_b.next_id()).collect();
        assert!(ids_a.is_disjoint(&ids_b));
    }

    #[test]
    fn system_generator_produces_current_epoch_v7() {
        let mut generator = PersistentIdGenerator::system();
        let id = generator.next_id();
        assert_eq!(id.as_uuid().get_version_num(), 7);
        let millis = id.unix_millis().expect("v7 timestamp");
        // 2020-01-01 之后、2100 年之前的粗校验，防止时间单位写错。
        assert!(millis > 1_577_836_800_000);
        assert!(millis < 4_102_444_800_000);
    }
}
