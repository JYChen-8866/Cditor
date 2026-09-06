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

## 2026-07-25 refactor revalidation

The Phase 9 crate-boundary cleanup was validated twice on the same macOS/aarch64
machine with the full command above. Both runs passed every open, scroll, editing,
structure, and mixed budget. The second run is retained in
`target/benchmark-reports/frame-baseline-full.json`.

| Mixed metric | 2026-07-22 baseline | 2026-07-25 revalidation | Change |
| --- | ---: | ---: | ---: |
| worst frame p95 | 0.107 ms | 0.105959 ms | -0.97% |
| worst frame max | 0.185 ms | 0.166667 ms | -9.91% |
| peak projected blocks | 108 | 108 | unchanged |
| peak resident payloads | 512 | 512 | unchanged |
| peak Runtime payload + text-undo bytes | 195,019 | 195,019 | unchanged |

The first confirmation run measured 0.111542 ms p95 and 0.261625 ms max while
remaining far inside the 16 ms / 50 ms budgets. The immediate repeat returned
below-baseline p95 and max values with identical resource bounds, so the small
timing spread is treated as measurement noise rather than a stable regression.

## 2026-07-26 SQLite visible-payload regression protocol

This protocol covers the production failure that the Runtime-only benchmark
cannot reproduce: dragging the scrollbar through a roughly 100,000-block SQLite
document could paint block chrome immediately while text remained absent until a
later timer or pointer event advanced deferred work.

The corrected pipeline has three separately timed stages:

```text
storage query (background)
  -> payload preparation (background: kind/schema normalization, table repair,
     byte accounting, Arc allocation)
  -> visible residency commit (GPUI update: token/ownership validation,
     HashMap insertion, loading-marker cleanup, notify)
  -> projection
  -> frame-budgeted text layout and paint
```

The visible residency commit is deliberately not a `MainThreadBudget` task. It
is a bounded viewport-liveness state transition. Text shaping, measurement,
syntax highlighting, entity diff, and cache maintenance remain budgeted. Cache
byte refresh and LRU eviction run as bounded idle slices and must yield between
slices.

### Reproduction

```bash
CDITOR_SQLITE_PATH=/Users/jychen/Desktop/CDitor/workspace.cditor.db \
CDITOR_DOCUMENT_ID=1 \
CDITOR_TRACE_PAYLOAD=1 \
scripts/dev/run_editor_sqlite.sh
```

Drag rapidly to neighborhoods around blocks 30,000, 50,000, and 80,000, then
reverse direction and revisit each range. Also verify that closing a diagnostic
consumer does not terminate the editor:

```bash
CDITOR_SQLITE_PATH=/Users/jychen/Desktop/CDitor/workspace.cditor.db \
CDITOR_DOCUMENT_ID=1 \
CDITOR_TRACE_PAYLOAD=1 \
scripts/dev/run_editor_sqlite.sh 2>&1 | head -80
```

### Required trace evidence

For each cold target, retain these correlated records by `generation` and
`range`:

```text
visible-query.start / visible-query.complete
visible-prepare.complete
visible-commit.complete
projection.full-placeholder | projection.stable-preparing | projection.resident
frame telemetry, including text-layout cache and main-thread lane depth
```

The trace must distinguish database latency from background CPU preparation and
the GPUI commit. A large `visible-query.complete -> visible-commit.complete` gap
is a scheduling regression even when SQLite itself is fast.

### Pass criteria

- A cold scrollbar jump may show one height-stable full-window placeholder while
  its visible core is genuinely unavailable.
- A resident-overlap wheel or momentum move must keep resident text visible;
  only missing overscan edges may use per-block placeholders.
- Query completion must lead to text without a click, key press, or unrelated
  event waking the scheduler.
- Revisited resident windows must render without a placeholder transition.
- Visible layout work advances on the next frame deadline, not in 120 ms steps;
  the 120 ms delay remains limited to prefetch/background idle work.
- `visible-commit.complete` must stay below 1 ms in the normal bounded window;
  any sustained overage is investigated rather than hidden by a larger budget.
- `prepare` time is charged to the background stage and must not appear inside
  `visible-commit.complete`.
- No full resident-cache scan or unbounded protected-entry scan occurs in a
  visible commit or interactive frame.
- Closing `head -80` must not panic or abort the desktop process through stderr
  `BrokenPipe`; the application remains alive until explicitly closed.
