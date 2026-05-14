# Simple-Object Residual Annotations

## Scope

Claim the next `compute_type_of_symbol` simple-local-interface shortcut slice
from `docs/plan/PERFORMANCE_PLAN.md`.

After the primitive/literal admission, regenerated monorepo-006 has only two
live `reject_non_primitive_annotation` rows left in this shortcut:
`union_or_intersection=1` and `array_or_tuple=1`. This slice will attribute
those remaining annotations and either admit a local resolver-free case or
document why they should stay on fallback.

## Initial PR State

This file intentionally starts as a coordination marker. Implementation,
measurements, and validation evidence will be added before the PR is marked
ready.

## Result

The two remaining annotation rejects are safe to admit under a recursive
resolver-free guard: union/intersection, array, and tuple annotations are
accepted only when every child annotation is already accepted by the simple
local-interface shortcut.

On regenerated monorepo-006:

| Metric | Before | After |
| --- | ---: | ---: |
| diagnostics | 10,198 | 10,198 |
| simple-object fast-path hits | 24,760 | 24,762 |
| simple-object success outcomes | 24,760 | 24,762 |
| non-primitive annotation rejects | 2 | 0 |
| union/intersection rejects | 1 | 0 |
| array/tuple rejects | 1 | 0 |

Decision record:
[`docs/plan/perf-runs/2026-05-14-simple-object-residual-annotations.md`](../perf-runs/2026-05-14-simple-object-residual-annotations.md).

## Validation

- `cargo check -p tsz-checker --lib`
- `cargo fmt --all --check`
- `git diff --check`
- `CARGO_TARGET_DIR=/private/tmp/tsz-simple-number-target cargo build -p tsz-cli --bin tsz --release --features perf-tools`
- `TSZ_PERF_COUNTERS=1 /private/tmp/tsz-simple-number-target/release/tsz --noEmit -p scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json --extendedDiagnostics --pretty false --diagnostics-json docs/plan/perf-runs/raw/2026-05-14-simple-object-residual-annotations-diag.json --perf-counters-json docs/plan/perf-runs/raw/2026-05-14-simple-object-residual-annotations-pc.json`
