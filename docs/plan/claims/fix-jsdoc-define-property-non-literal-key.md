# fix(checker): keep TS2339 on require'd-namespace expando writes

- **Date**: 2026-04-29
- **Branch**: `fix/jsdoc-define-property-non-literal-key`
- **PR**: #1721
- **Status**: ready
- **Workstream**: 1 (Conformance fixes)

## Intent

`Object.defineProperty(exports, X, …)` with a non-literal `X` was already
correctly excluded from the synthesized CJS export surface (literal-only
extraction). The remaining gap was upstream: writes against a `const mod =
require("…")` namespace (`mod.other = 0`) silently bypassed TS2339 because
the JSDoc "assigned-value type" recovery in `property_access_type/resolve.rs`
treated those writes as JS-style expando declarations on `mod`, returning
the RHS type instead of letting the property-not-found diagnostic emit.

This claim adds a guard at that fallback so it skips when the property
access is rooted in an imported namespace (ESM `import * as` / `import =
require()` ALIAS symbols, or `const X = require("…")` JS bindings). With
the guard, both the read (`mod.other`) and the write (`mod.other = 0`)
correctly surface as TS2339 — restoring conformance for
`checkOtherObjectAssignProperty.ts` and any downstream test that hit the
same shape.

## Files Touched

- `crates/tsz-checker/src/state/state.rs` — new helper
  `property_access_root_is_imported_namespace`.
- `crates/tsz-checker/src/types/property_access_type/resolve.rs` — gate the
  JSDoc assigned-value-type fallback on the new helper.
- `crates/tsz-checker/tests/commonjs_require_value_tests.rs` — regression
  test (`check_js_require_namespace_does_not_admit_expando_writes`).

## Verification

- `cargo nextest run -p tsz-checker --lib` — 2960 passed.
- `cargo nextest run -p tsz-checker --test commonjs_require_value_tests` —
  new test passes; existing JSON-require test unchanged.
- `./scripts/conformance/conformance.sh run --filter
  "checkOtherObjectAssignProperty" --verbose` — 1/1 passed (was 0/1).
- Targeted regression checks on local-function expandos and ESM namespace
  imports — both unchanged.
- Full conformance run: net **+2** (12235 → 12237), 0 regressions. Improvements:
  `checkOtherObjectAssignProperty.ts` (the target) plus a bonus
  `importAliasModuleExports.ts` (same root cause).
