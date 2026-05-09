# fix(wasm): thread allow_js / declaration / source_map through Program options

- **Date**: 2026-05-09
- **Branch**: `claude/nice-darwin-QHBhb`
- **PR**: TBD
- **Status**: claim
- **Workstream**: WASM API parity

## Intent

`TsCompilerOptions::to_checker_options` in
`crates/tsz-wasm/src/wasm_api/program.rs` hardcodes `allow_js: false` and
`emit_declarations: false`, ignoring the JS-supplied `allowJs` and
`declaration` fields. `TsProgram::emit_json` likewise hardcodes the
per-file `declaration` and `sourceMap` metadata to `false`.

This PR threads the three option fields through:

- `allow_js` -> `CheckerOptions.allow_js` (closes #4734, partially #4748)
- `declaration` -> `CheckerOptions.emit_declarations` (#4748)
- per-emitted-file `declaration` / `sourceMap` metadata in
  `emit_json` (#4738, #4748)

## Files Touched

- `crates/tsz-wasm/src/wasm_api/program.rs`
- `crates/tsz-wasm/src/wasm_tests.rs` (unit tests)
- `docs/plan/claims/claude-nice-darwin-qhbhb.md`

## Verification

- `cargo nextest run -p tsz-wasm`
