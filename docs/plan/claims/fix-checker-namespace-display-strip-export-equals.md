# fix(checker): strip synthetic `.export=` suffix from import-type namespace display

- **Date**: 2026-04-29
- **Branch**: `fix/checker-namespace-display-strip-export-equals`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance — fingerprint parity)

## Intent

Fix wrong namespace name in TS2694 diagnostics for `import("module").Bar.Q`-style
type references where `Q` is missing. tsz currently emits
`Namespace '"module".Bar.export=' has no exported member 'Q'.` while tsc
emits `Namespace '"module".Bar' has no exported member 'Q'.` — the trailing
`.export=` is the binder's internal synonym for `export = ...` bindings and
must not appear in user-facing diagnostics.

## Root Cause

`crates/tsz-checker/src/state/type_resolution/import_type.rs` builds the
namespace display string with two helpers
(`import_type_namespace_name`, `import_type_namespace_name_with_segments`)
that always append `.export=` to the path. The synthetic key is a binder
implementation detail and does not appear in tsc's TS2694 messages.

## Fix

Drop the `.export=` suffix from both helpers' format strings. The path now
matches tsc's exact format: `"<module>"` for direct module access,
`"<module>".A.B.C` for nested namespace qualifications.

## Files Touched

- `crates/tsz-checker/src/state/type_resolution/import_type.rs` (~6 LOC)

## Verification

- `cargo nextest run -p tsz-checker -E 'test(/import_type|namespace_no_export|TS2694|ts2694/)'`: 65/65 pass.
- `./scripts/conformance/conformance.sh run --filter "importTypeLocalMissing" --verbose`: TS2694 message format now matches tsc; remaining divergence is column anchor (separate concern).
- Full conformance: net **12235 → 12245 (+10)**.
