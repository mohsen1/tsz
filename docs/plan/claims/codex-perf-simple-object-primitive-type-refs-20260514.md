# Claim: Simple-object primitive and literal shortcut admission

- **Date**: 2026-05-14
- **Branch**: `codex/perf-simple-object-primitive-type-refs-20260514`
- **PR**: TBD
- **Status**: ready
- **Workstream**: Performance plan simple local-interface shortcut attribution

## Claim

The #6747 residue run showed that the guarded simple local-interface shortcut
was blocked by primitive `number` type references in generated interfaces.
After admitting those, the same interfaces then stopped at string-literal
`tag` properties.

This PR admits only no-argument primitive intrinsic type references plus
literal/template literal annotations. It does not add a general resolver to the
shortcut path; actual property lowering still uses
`get_type_from_type_node_in_type_literal`.

## Scope

- Accept primitive intrinsic type-reference annotations such as `number`,
  `string`, and `boolean` when they have no type arguments.
- Accept literal and template-literal annotations.
- Keep arrays, tuples, unions, intersections, aliases, and qualified names on
  the existing fallback path.
- Record raw monorepo-006 attribution output for the counter delta.

## Evidence

- `crates/tsz-checker/src/state/type_analysis/computed/simple_local_interface.rs`
  widens only the shortcut admission predicate.
- `crates/tsz-checker/tests/simple_local_interface_fastpath_tests.rs` locks
  primitive and literal property assignability after shortcut admission.
- `docs/plan/perf-runs/2026-05-14-simple-object-primitive-literal-type-refs.md`
  records the attribution result.

## Validation

- `cargo test -p tsz-checker --test simple_local_interface_fastpath_tests -- --nocapture`
- `cargo check -p tsz-checker --lib`
- `cargo fmt --all --check`
- `git diff --check`
- `CARGO_TARGET_DIR=/Users/mohsen/.cache/tsz-target cargo build -p tsz-cli --bin tsz --release --features perf-tools`
- `TSZ_PERF_COUNTERS=1 /Users/mohsen/.cache/tsz-target/release/tsz --noEmit -p scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json --extendedDiagnostics --pretty false --diagnostics-json docs/plan/perf-runs/raw/2026-05-14-simple-object-primitive-literal-type-refs-monorepo-006-diag.json --perf-counters-json docs/plan/perf-runs/raw/2026-05-14-simple-object-primitive-literal-type-refs-monorepo-006-pc.json`
