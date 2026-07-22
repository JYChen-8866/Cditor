use super::{BlockHeightIndex, HeightConfidence, HeightEstimate};

#[test]
fn randomized_range_insert_delete_and_move_preserve_existing_measurements() {
    let mut rng = Lcg::new(0x6a11_2026_0722);
    let initial = (1..=64)
        .map(|height| HeightEstimate::new(height as f64, HeightConfidence::Exact, 0.0))
        .collect::<Vec<_>>();
    let mut index = BlockHeightIndex::new(initial).unwrap();
    let mut expected = index
        .heights
        .iter()
        .copied()
        .zip(index.confidence.iter().copied())
        .collect::<Vec<_>>();

    for step in 0..1_000 {
        match rng.next_usize(3) {
            0 => {
                let at = rng.next_usize(index.len() + 1);
                let estimates = (0..=rng.next_usize(3))
                    .map(|offset| {
                        HeightEstimate::new(
                            1_000.0 + step as f64 * 4.0 + offset as f64,
                            HeightConfidence::Predictive,
                            0.0,
                        )
                    })
                    .collect::<Vec<_>>();
                index.insert_range(at, &estimates).unwrap();
                expected.splice(
                    at..at,
                    estimates
                        .iter()
                        .map(|estimate| (estimate.height, estimate.confidence)),
                );
            }
            1 if index.len() > 1 => {
                let start = rng.next_usize(index.len() - 1);
                let length = (rng.next_usize(4) + 1).min(index.len() - start - 1);
                index.delete_range(start..start + length).unwrap();
                expected.drain(start..start + length);
            }
            _ if index.len() > 1 => {
                let start = rng.next_usize(index.len());
                let length = (rng.next_usize(4) + 1).min(index.len() - start);
                let range = start..start + length;
                let target = rng.next_usize(index.len() + 1);
                index.move_range(range.clone(), target).unwrap();
                if !range.contains(&target) && target != range.end {
                    let moved = expected.drain(range.clone()).collect::<Vec<_>>();
                    let adjusted_target = if target > range.end {
                        target - range.len()
                    } else {
                        target
                    };
                    expected.splice(adjusted_target..adjusted_target, moved);
                }
            }
            _ => {}
        }

        assert_eq!(
            index
                .heights
                .iter()
                .copied()
                .zip(index.confidence.iter().copied())
                .collect::<Vec<_>>(),
            expected
        );
        assert_prefix_matches_naive(&index);
    }
}

fn assert_prefix_matches_naive(index: &BlockHeightIndex) {
    let mut sum = 0.0;
    for position in 0..index.len() {
        assert_eq!(index.offset_of_block(position), Some(sum));
        sum += index.heights[position];
    }
    assert_eq!(index.total_height(), sum);
}

struct Lcg(u64);

impl Lcg {
    const fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_usize(&mut self, upper_bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        if upper_bound == 0 {
            0
        } else {
            (self.0 as usize) % upper_bound
        }
    }
}
