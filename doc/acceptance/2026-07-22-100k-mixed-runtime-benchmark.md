# 100k mixed Runtime benchmark acceptance

Date: 2026-07-22

## Scope

The benchmark drives one real `DocumentRuntime` built from the versioned 100,000-block uneven-height fixture. A single measured loop interleaves:

- wheel-style virtual scrolling;
- deterministic non-local block jumps;
- payload-window planning and completion around the jump target;
- focus plus a real text edit and undo-history recording;
- scrollbar begin/drag/end against the frozen height model;
- bounded projection and payload-cache trimming.

The loop does not substitute the synthetic latency values used by older isolated acceptance scenarios. Runtime construction is outside the per-frame samples and is reported by the existing cold-start benchmark.

## Reproduction

```bash
cargo bench -p cditor-test-support --bench frame_baseline -- --full
```

Machine recorded by the report: macOS/aarch64, 10 logical cores, Cargo `bench` profile. The machine-readable report is generated at `target/benchmark-reports/frame-baseline-full.json`.

## Full-mode result

Three independent runs completed without a budget failure. Each run used 512 mixed iterations over 100,000 blocks:

| Metric | Result | Budget |
| --- | ---: | ---: |
| scroll operations | 512 | must be non-zero |
| jump operations | 64 | must be non-zero |
| text edit operations | 64 | must equal jumps |
| scrollbar drag operations | 32 | must be non-zero |
| worst mixed-frame p95 | 0.107 ms | 16 ms |
| worst mixed-frame max | 0.185 ms | 50 ms |
| peak projected blocks | 108 | 320 |
| peak resident payloads | 512 | 512 |
| peak Runtime payload + text-undo bytes | 195,019 bytes | 48 MiB |

The full benchmark exited successfully and wrote a report with `mixed.passed = true` and an empty failure list.

## Automated regression coverage

`acceptance::mixed::tests::real_100k_mixed_sequence_stays_bounded` runs the same real Runtime path in the normal test profile with 24 iterations. It asserts the 100,000-block fixture, all four operation classes, bounded projection/residency, and successful budget evaluation.

This acceptance covers the Runtime/kernel mixed workload. GPUI frame-deadline and lane attribution remain tracked separately by P6-006, P6-007, and P6-014; this result does not claim those production wiring items are complete.
