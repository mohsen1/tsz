# Claim: Resolve simple-object missing-interface rows through lib metadata

Date: 2026-05-14
Branch: `codex/perf-simple-object-missing-interface-lib-20260514`
PR: #6917
Status: ready

## Claim

The simple local-interface object shortcut still records seven
`reject_missing_interface_decl` rows on regenerated monorepo-006 after the
declaration/provenance residue naming slice. This work proves whether those
exact rows can reuse existing lib metadata before the shortcut records a
missing-interface reject.

## Scope

- Admit only the named missing-interface residue family:
  `Iterable`, `IteratorReturnResult`, `IteratorYieldResult`,
  `PropertyDescriptor`, `PropertyDescriptorMap`, `RegExpIndicesArray`, and
  `RegExpStringIterator`.
- Reuse existing lib metadata resolvers (`resolve_lib_type_by_name`, with the
  existing parameter-aware resolver as a fallback); do not lower declaration
  arenas manually.
- Leave `reject_out_of_arena_decl` rows and all non-allowlisted symbols on the
  current fallback path.
- Record regenerated monorepo-006 attribution counters before making a timing
  claim.

## Validation

- `cargo test -p tsz-checker --test simple_local_interface_fastpath_tests -- --nocapture`
- `cargo check -p tsz-checker --lib`
- `CARGO_TARGET_DIR=/private/tmp/tsz-simple-missing-target CARGO_INCREMENTAL=0 cargo build -p tsz-cli --bin tsz --release --features perf-tools`
- `TSZ_PERF_COUNTERS=1 /private/tmp/tsz-simple-missing-target/release/tsz --noEmit -p scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json --extendedDiagnostics --pretty false --diagnostics-json docs/plan/perf-runs/raw/2026-05-14-simple-object-missing-interface-lib-diag.json --perf-counters-json docs/plan/perf-runs/raw/2026-05-14-simple-object-missing-interface-lib-pc.json` (expected exit `2`)
