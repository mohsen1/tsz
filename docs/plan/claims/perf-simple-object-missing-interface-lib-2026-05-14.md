# Claim: Classify simple-object missing-interface lib rows in attribution

Date: 2026-05-14
Branch: `codex/perf-simple-object-missing-interface-lib-20260514`
PR: #6917
Status: ready

## Claim

The simple local-interface object shortcut previously recorded seven
`reject_missing_interface_decl` rows on regenerated monorepo-006 after the
declaration/provenance residue naming slice. With perf counters enabled, this
work classifies the conformance-safe non-iterator subset as actual/cloned
lib-backed in attribution before the shortcut records a missing-interface
reject, leaving iterator-family rows for a separate proof. Normal checker
execution keeps the attribution suppression disabled, and semantic type
computation for those rows stays on the existing full merge path.

## Scope

- Classify only the conformance-safe non-iterator residue family:
  `PropertyDescriptor`, `PropertyDescriptorMap`, and `RegExpIndicesArray`.
- Require actual/cloned lib symbol provenance; do not lower declaration arenas
  manually.
- Keep the early lib-type return path limited to the existing
  out-of-arena/lib-symbol cases.
- Gate the attribution-only suppression on `perf_counters::enabled_fast()` so
  normal emit/conformance runs keep the existing control flow.
- Leave `reject_out_of_arena_decl` rows and all non-allowlisted symbols on the
  current fallback path.
- Record regenerated monorepo-006 attribution counters before making a timing
  claim.

## Validation

- `cargo test -p tsz-checker --test simple_local_interface_fastpath_tests -- --nocapture`
- `cargo check -p tsz-checker --lib`
- `CARGO_TARGET_DIR=/private/tmp/tsz-simple-missing-target CARGO_INCREMENTAL=0 cargo build -p tsz-cli --bin tsz --release --features perf-tools`
- `TSZ_PERF_COUNTERS=1 /private/tmp/tsz-simple-missing-target/release/tsz --noEmit -p scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json --extendedDiagnostics --pretty false --diagnostics-json docs/plan/perf-runs/raw/2026-05-14-simple-object-missing-interface-lib-diag.json --perf-counters-json docs/plan/perf-runs/raw/2026-05-14-simple-object-missing-interface-lib-pc.json` (expected exit `2`)
