---
name: JSDoc typedef export assignment false positives
status: ready
timestamp: 2026-05-06 13:18:00
branch: fix/conformance-next-20260506-131409
---

# Claim

Workstream 1 (Diagnostic Conformance) for
`TypeScript/tests/cases/conformance/jsdoc/declarations/jsDeclarationsTypedefPropertyAndExportAssignment.ts`.

## Scope

Suppress the extra TS2304/TS2552 diagnostics emitted while checking checked-JS
JSDoc typedefs that flow through a CommonJS `module.exports` assignment and an
`import('./module.js').TaskGroup` typedef.

## Verification Plan

- Focused checked-JS/JSDoc regression for the typedef import/export assignment path.
- `cargo nextest run` for the affected checker regression target.
- `./scripts/conformance/conformance.sh run --filter "jsDeclarationsTypedefPropertyAndExportAssignment" --verbose`.

## Verification

- `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=.target/nextest-local cargo nextest run -p tsz-checker --lib -E 'test(jsdoc_mapped_type_tag_scopes_parameter_for_nested_template) | test(jsdoc_import_type_typedef_alias_is_visible_to_later_typedefs)'`
- `./scripts/conformance/conformance.sh run --filter "jsDeclarationsTypedefPropertyAndExportAssignment" --verbose` (1/1 passed, fingerprint-only 0)
