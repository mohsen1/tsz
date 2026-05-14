# Claim: Simple-object type-reference residue attribution run

- **Date**: 2026-05-14
- **Branch**: `codex/perf-simple-object-type-ref-residue-run-20260514`
- **PR**: #6747
- **Status**: ready
- **Workstream**: Performance plan simple local-interface shortcut attribution

## Claim

After #6734, the next safe step is a docs-only attribution run that consumes
the new residue table before any behavior change.

This PR records the monorepo-006 residue-name result: the entire live
`identifier_not_found_symbol` bucket is the single name `number`.

## Scope

- No code changes.
- No timing claim.
- Preserve raw diagnostics and perf-counter JSON artifacts.
- Update the plan with the next implementation target.

## Evidence

- `docs/plan/perf-runs/2026-05-14-simple-object-type-reference-residues.md`
  records the command, raw artifact paths, counter totals, and decision.
- `docs/plan/PERFORMANCE_PLAN.md` narrows the next step to a primitive
  `number` investigation instead of broad type-symbol resolver work.

## Validation

- `scripts/bench/scale-cliff/generate-fixtures.sh`
- `CARGO_TARGET_DIR=/Users/mohsen/.cache/tsz-target cargo build -p tsz-cli --bin tsz --release --features perf-tools`
- `TSZ_PERF_COUNTERS=1 /Users/mohsen/.cache/tsz-target/release/tsz --noEmit -p scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json --extendedDiagnostics --pretty false --diagnostics-json docs/plan/perf-runs/raw/2026-05-14-simple-object-type-ref-residues-monorepo-006-diag.json --perf-counters-json docs/plan/perf-runs/raw/2026-05-14-simple-object-type-ref-residues-monorepo-006-pc.json`
- `jq -r '.compute_type_of_symbol_interface_simple_object_type_reference_reject_residues[] | [.name,.outcome,.count] | @tsv' docs/plan/perf-runs/raw/2026-05-14-simple-object-type-ref-residues-monorepo-006-pc.json`
