# fix(checker): emit TS2823 for import attributes module option

- **Date**: 2026-05-12
- **Branch**: `fix/import-attributes-module-ts2823-20260512`
- **Issue**: #5785
- **Status**: claim
- **Workstream**: conformance

## Intent

Emit TS2823 when import attributes are used without a compatible `--module` option.

## Scope

- Reproduce the focused issue/conformance case.
- Add the smallest validation in the import/export grammar or semantic statement checking path.
- Preserve existing module-resolution diagnostics such as TS2307.

## Verification Plan

- Focused unit/conformance for TS2823 import attributes.
- `cargo fmt --all`
- Relevant direct checker tests or pre-commit before marking ready.
