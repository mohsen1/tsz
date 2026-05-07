# fix(cli): --declarationMap honors config-supplied declaration/composite

- **Date**: 2026-05-07
- **Branch**: `claude/nice-darwin-Ewk8x`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance / CLI parity)

## Intent

Closes #3712. The CLI shim that re-validates compiler options
(`validate_cli_compiler_option_diagnostics` in
`crates/tsz-cli/src/driver/core.rs`) synthesizes a JSON tsconfig from the
user's CLI flags and feeds it back through `parse_tsconfig_with_diagnostics`.
When `--declarationMap` is passed but `declaration` / `composite` come from
the existing tsconfig, the synthesized JSON only contains `declarationMap`,
so the TS5069 prerequisite check fires falsely. tsc accepts this combination.

The fix mirrors the pattern already used for `--emitDeclarationOnly`: when
CLI sets `--declarationMap`, also include the config-supplied `declaration`
or `composite` value in the synthesized validation payload so the TS5069
prerequisite check sees the merged effective state.

## Files Touched

- `crates/tsz-cli/src/driver/core.rs` (small change in
  `validate_cli_compiler_option_diagnostics`)
- `crates/tsz-cli/src/driver/tests.rs` (regression test)

## Verification

- `cargo nextest run -p tsz-cli`
- Manual reproduction matches `tsc -p . --declarationMap --pretty false`
  (exits 0, emits `a.js`, `a.d.ts`, `a.d.ts.map`).
