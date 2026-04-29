# fix(checker): align TS2351 fingerprint for ESM module exports

- **Date**: 2026-04-29
- **Branch**: `fix/checker-esm-module-exports-ts2351-fingerprint`
- **PR**: #1807
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Investigate and fix the quick-pick fingerprint-only mismatch in
`esmModuleExports2.ts`. The current divergence is a missing expected TS2351
fingerprint at `importer-cts.cts:5:5` for constructing a CommonJS import of an
ESM module with a `"module.exports"` export; diagnostic codes already match, so
the expected scope is message, anchor, or diagnostic formatting parity.

## Outcome

TSZ now treats a default import from an ESM target with a `"module.exports"`
export as the CommonJS-facing `module.exports` binding when the importing file is
in Node20/NodeNext require-mode emit. That makes `new Foo2()` report TS2351 when
the `"module.exports"` binding is non-constructable, matching the
`esmModuleExports2.ts` fingerprint.

## Files Touched

- `docs/plan/claims/fix-checker-esm-module-exports-ts2351-fingerprint.md`
- `crates/tsz-checker/src/state/type_analysis/computed/type_alias_variable_alias.rs`
- `crates/tsz-checker/src/state/type_resolution/module/interop.rs`
- `crates/tsz-checker/src/types/computation/complex.rs`
- `crates/tsz-checker/src/types/computation/identifier/core.rs`
- `crates/tsz-checker/src/types/computation/identifier/resolution.rs`
- `crates/tsz-checker/tests/conformance_issues/features/import_aliases.rs`

## Verification

- `./scripts/conformance/conformance.sh run --filter "esmModuleExports2" --verbose` (baseline: fingerprint-only failure)
- `cargo check --package tsz-checker` (pass)
- `cargo check --package tsz-solver` (pass)
- `cargo build --profile dist-fast --bin tsz` (pass)
- `cargo nextest run --package tsz-checker --test conformance_issues test_esm_module_exports_non_default_binding_default_import_is_namespace_object` (pass)
- `cargo nextest run --package tsz-checker --lib` (3005 passed, 10 skipped)
- `cargo nextest run --package tsz-solver --lib` (5550 passed, 9 skipped)
- `./scripts/conformance/conformance.sh run --filter "esmModuleExports2" --verbose` (1/1 passed)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL` (`FINAL RESULTS: 12245/12582 passed (97.3%)`)
