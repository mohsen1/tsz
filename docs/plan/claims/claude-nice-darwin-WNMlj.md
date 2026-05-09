# fix(resolver): honor versioned types@... conditions in package imports

- **Date**: 2026-05-08
- **Branch**: `claude/nice-darwin-WNMlj`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance / module resolution parity)

## Intent

Fix issue #3564: `package.json#imports` condition matching ignores versioned
`types@...` conditions because the imports path uses the static
`resolve_export_targets_to_strings` helper, which does exact-membership key
matching. The exports path uses `condition_key_matches`, which already
understands versioned `types@...` keys via the same logic that handles
`typesVersions`. Route the imports path through `condition_key_matches` so
both surfaces apply the same condition rules.

## Files Touched

- `crates/tsz-core/src/module_resolver/exports_imports.rs` (small refactor:
  static helper becomes `&self` method to share `condition_key_matches`).
- `crates/tsz-core/src/module_resolver/tests.rs` (regression test).

## Verification

- `cargo nextest run -p tsz-core` (module_resolver tests).
- `cargo nextest run -p tsz-checker` (no regressions).
