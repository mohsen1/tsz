# fix(driver): suppress semantic diagnostics when JS-only-syntactic errors exist

- **Date**: 2026-05-02
- **Branch**: `claude/brave-thompson-uf2ae`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (conformance fixes)

## Intent

Match `tsc`'s `emitFilesAndReportErrors` short-circuit: when any JS-only
syntactic diagnostic (`TS8002`, `TS8003`, `TS8004`–`TS8013`, `TS8016`,
`TS8017`, `TS8037`, `TS8038`) appears anywhere in a program, suppress
checker semantic diagnostics across every file. tsc routes those codes
through `getSyntacticDiagnostics` via `getJSSyntacticDiagnosticsForFile`,
which gates `getSemanticDiagnostics` for the whole program.

This unblocks `compiler/modulePreserve4.ts`, where a single `TS8002`
("`import ... =` can only be used in TypeScript files") in a `.cjs`
file makes tsc emit *only* `TS8002` while tsz was emitting cascading
`TS1192`/`TS1295`/`TS2305`/`TS2339`/`TS2591` checker semantics.

## Files Touched

- `crates/tsz-cli/src/driver/check_utils.rs`
  - new `is_js_only_syntactic_diagnostic` helper
  - new `keep_diagnostic_when_js_only_syntactic_skips_semantic` helper
- `crates/tsz-cli/src/driver/core.rs`
  - apply the program-wide suppression after per-file diagnostic
    collection, before `config_diagnostics` are folded in (config-parse
    diagnostics like `TS5023` escape the gate, matching tsc).
- `crates/tsz-cli/src/driver/tests.rs`
  - regression test pinning the program-wide suppression behaviour.

## Verification

- `cargo test -p tsz-cli --lib js_only_syntactic_error_suppresses_semantic_diagnostics_program_wide` (1 test passes)
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` (clean)
- `./scripts/conformance/conformance.sh run --filter modulePreserve4` (1/1 passed; was failing)
- `./scripts/conformance/conformance.sh run --filter modulePreserve` (7/7 passed)
- `./scripts/conformance/conformance.sh run --filter jsFileCompilation` (86/86 passed)
- targeted runs of `useBeforeDeclaration_classDecorators`, `jsExtendsImplicitAny`,
  `parameterDecoratorsEmitCrash` (all pass — confirmed `TS1206` from TS files
  and JSDoc-checker `TS8xxx` codes don't trigger the new gate).
- `scripts/session/verify-all.sh --quick` (formatting + clippy + nextest +
  conformance — clean before push)
