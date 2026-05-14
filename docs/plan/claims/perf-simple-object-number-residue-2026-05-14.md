# Simple-Object Number Residue

## Scope

Claim the next `compute_type_of_symbol` simple-local-interface shortcut slice
from `docs/plan/PERFORMANCE_PLAN.md`.

The current measured residue says primitive-looking `number` annotations reach
the shortcut as `type_reference` with `identifier_not_found_symbol`. This slice
will prove why that happens, then either admit the primitive case under a
conformance-backed guard or document why the parser/classification boundary
needs a different fix.

## Initial PR State

This file intentionally starts as a coordination marker. Implementation,
measurements, and validation evidence will be added before the PR is marked
ready.

## Result

The `number` residue is parser shape, not missing symbol resolution. The parser
commonly represents bare primitive keyword type names as `TYPE_REFERENCE` nodes,
while normal type-literal lowering already maps those names back to primitive
`TypeId`s.

This slice mirrors that resolver-free mapping in the simple-local-interface
shortcut guard and also admits plain literal type annotations, which are needed
for the generated leaf interfaces' `tag: "leaf-N"` members. It does not admit
arbitrary type references, template literal types, arrays, tuples, unions, or
intersections.

On regenerated monorepo-006:

| Metric | Before | After |
| --- | ---: | ---: |
| diagnostics | 10,198 | 10,198 |
| simple-object fast-path hits | 0 | 24,760 |
| simple-object success outcomes | 0 | 24,760 |
| non-primitive annotation rejects | 24,762 | 2 |
| type-reference reject residue rows | `number=24,761` | none |

Decision record:
[`docs/plan/perf-runs/2026-05-14-simple-object-number-literal-admission.md`](../perf-runs/2026-05-14-simple-object-number-literal-admission.md).

## Validation

- `cargo check -p tsz-checker --lib`
- `cargo test -p tsz-checker --lib simple_local_interface_keyword -- --nocapture`
- `cargo fmt --all --check`
- `git diff --check`
- `CARGO_TARGET_DIR=/private/tmp/tsz-simple-number-target cargo build -p tsz-cli --bin tsz --release --features perf-tools`
- `TSZ_PERF_COUNTERS=1 /private/tmp/tsz-simple-number-target/release/tsz --noEmit -p scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json --extendedDiagnostics --pretty false --diagnostics-json docs/plan/perf-runs/raw/2026-05-14-simple-object-number-residue-diag.json --perf-counters-json docs/plan/perf-runs/raw/2026-05-14-simple-object-number-residue-pc.json`
