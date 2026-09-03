use super::*;

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }

    fn usize(&mut self, upper: usize) -> usize {
        (self.next() as usize) % upper
    }
}

#[test]
fn randomized_measured_height_stale_result_and_anchor_property() {
    const BLOCK_COUNT: usize = 512;
    const STEPS: usize = 2_000;
    let payloads = (1..=BLOCK_COUNT as BlockId)
        .map(|block_id| {
            BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, "payload")
        })
        .collect();
    let mut runtime = DocumentRuntime::from_payloads(1, payloads, 720.0);
    runtime
        .layout
        .scroll
        .scroll_to_global_offset(8_000.25, ScrollOrigin::UserWheel)
        .unwrap();
    let mut expected_heights = runtime.layout.height_index.heights.clone();
    let mut rng = Lcg(0x5eed_cafe_f00d_beef);

    for step in 0..STEPS {
        let block_index = rng.usize(BLOCK_COUNT);
        let block_id = block_index as BlockId + 1;
        let current_version = runtime
            .document
            .payload_window
            .get(block_id)
            .expect("all property payloads stay resident")
            .content_version;
        let before_anchor = runtime
            .target_for_global_offset(runtime.layout.scroll.global_scroll_top)
            .expect("non-empty document has a viewport anchor");
        let next_height = 24.0 + rng.usize(121) as f64;
        let stale = rng.next().is_multiple_of(4);
        let queued = runtime
            .queue_measured_height(block_id, current_version, next_height)
            .unwrap();

        if stale && queued {
            runtime
                .document
                .payload_window
                .get_mut(block_id)
                .unwrap()
                .content_version = current_version.saturating_add(1);
        } else if queued {
            expected_heights[block_index] = next_height;
        }

        let applied = runtime.flush_pending_height_corrections().unwrap();
        assert_eq!(applied, queued && !stale, "step={step}");
        let after_anchor = runtime
            .target_for_global_offset(runtime.layout.scroll.global_scroll_top)
            .expect("anchor remains resolvable after measurement");
        assert_eq!(after_anchor.block_id, before_anchor.block_id, "step={step}");
        assert!(
            (after_anchor.offset_in_block - before_anchor.offset_in_block).abs() < 1e-6,
            "step={step} before={before_anchor:?} after={after_anchor:?}"
        );

        let expected_total: f64 = expected_heights.iter().sum();
        assert!(
            (runtime.layout.height_index.total_height() - expected_total).abs() < 1e-6,
            "step={step}"
        );
        assert_eq!(
            runtime.layout.height_index.heights, expected_heights,
            "step={step}"
        );
    }
}

#[test]
fn stable_video_height_report_does_not_restore_an_old_scroll_position() {
    let video_payload = BlockPayload::Video(VideoPayload {
        source: "assets/demo.mp4".into(),
        title: "demo.mp4".into(),
        intrinsic_width: Some(1920),
        intrinsic_height: Some(1080),
        ..Default::default()
    });
    let video_height = cditor_core::layout::video_block_height_px(
        &video_payload,
        cditor_core::layout::BODY_BLOCK_CONTENT_WIDTH_PX,
    );
    let payloads = vec![
        BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "before"),
        BlockPayloadRecord {
            block_id: 2,
            content_version: 1,
            kind: RichBlockKind::Video,
            payload: video_payload,
        },
        BlockPayloadRecord::rich_text(3, RichBlockKind::Paragraph, "after"),
    ];
    let mut runtime = DocumentRuntime::from_payloads(1, payloads, 200.0);
    runtime
        .layout
        .scroll
        .scroll_to_global_offset(320.0, ScrollOrigin::UserWheel)
        .unwrap();
    let scrolled_position = runtime.layout.scroll.global_scroll_top;

    assert!(!runtime.queue_measured_height(2, 1, video_height).unwrap());
    assert!(!runtime.flush_pending_height_corrections().unwrap());
    assert_eq!(runtime.layout.scroll.global_scroll_top, scrolled_position);
}
