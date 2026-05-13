# Claim: Name the Remaining Declaration-File Delegation Residue

## Claim

`DelegateCrossArenaSymbol` declaration-file misses now expose a bounded
symbol-level attribution table in perf-counter JSON and text dumps. The field
is additive to schema version 1:

```json
"delegate_declaration_file_miss_residues": [
  {
    "name": "Record",
    "kind": "type_alias",
    "source": "symbol_arenas",
    "target_file": "lib.es5.d.ts",
    "count": 2
  }
]
```

The recorder is gated by `TSZ_PERF_COUNTERS` and only runs after the existing
delegate caches, alias shortcut, direct actual-lib path, direct cross-file
interface lowering, and direct source-file variable annotation path decline.

## Evidence

- `cargo fmt --all --check`
- `cargo test -p tsz-common perf_counters::json_tests -- --nocapture`
- `cargo check -p tsz-checker`
- `CARGO_BUILD_JOBS=1 cargo build --release -p tsz-cli --features perf-tools`
- Attribution run:

```bash
TSZ_TYPESCRIPT_LIB_DIR=/Users/mohsen/code/tsz/scripts/node_modules/typescript/lib \
TSZ_PERF_COUNTERS=1 \
.target/release/tsz --extendedDiagnostics --noEmit \
  -p scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json \
  --diagnostics-json docs/plan/perf-runs/raw/2026-05-13-delegate-decl-residue-names-monorepo-006-diag.json \
  --perf-counters-json docs/plan/perf-runs/raw/2026-05-13-delegate-decl-residue-names-monorepo-006-pc.json
```

The attribution run exits 2 due the synthetic fixture diagnostics, but writes
the JSON artifacts.

## Measured Outcome

On `monorepo-006`:

- `delegate.misses = 38`
- `checker.with_parent_cache_constructed = 39`
- `DelegateCrossArenaSymbol` children = 30
- declaration-file targets = 30
- `delegate_declaration_file_miss_residues` rows = 27
- diagnostics = 10,198

The row counts sum to the 30 declaration-file child-checker constructions.
The top repeated rows are `FlatArray`, `IteratorResult`, and `Record`, each
with count 2.

## Scope

This PR does not admit any new direct-lib type surface. It only makes the
remaining declaration-file tail inspectable so the next T2.2 PR can prove and
target a concrete subset.
