# fix(cli): TS2427 hard-keyword interface name suppresses other predefined-name TS2427s

- **Date**: 2026-04-26 17:41:38
- **Branch**: `fix/ts2427-void-null-suppress-others`
- **PR**: TBD
- **Status**: claim
- **Workstream**: Conformance fingerprint parity

## Intent

When a file contains `interface void {}` or `interface null {}`, tsc only
emits the TS2427 for the hard-keyword name and suppresses TS2427 for any
other reserved-name interfaces (`any`, `number`, etc.) in the same file.
This is because tsc's parser produces a parse error for the hard-keyword
name, which cascade-suppresses the lazy diagnostic queue for the other
declarations.

We don't currently emit a parse error for `void`/`null` as interface names
(intentional; see `state_declarations.rs`), so tsz over-emits 6 TS2427
diagnostics on `interfacesWithPredefinedTypesAsNames.ts` while tsc emits 1.

This PR mirrors tsc's effective behavior in the CLI driver's
`post_process_checker_diagnostics`: when a hard-keyword (`void`/`null`)
TS2427 is present, suppress all OTHER TS2427 in the same file. The
hard-keyword TS2427 is identified via its diagnostic message (since the
checker doesn't expose a richer kind for it).

## Files Touched

- `crates/tsz-cli/src/driver/check.rs` (~30 LOC)
- `crates/tsz-cli/tests/tsc_compat_tests.rs` (~75 LOC, 3 new tests)

## Verification

- `cargo nextest run --package tsz-cli` (1063 tests pass, 15 skipped)
- `cargo nextest run --package tsz-checker --lib` (2889 tests pass)
- `./scripts/conformance/conformance.sh run --filter "interfacesWithPredefinedTypesAsNames"` (1/1 PASS)
- Full conformance: `12183 -> 12187 (+4)` net improvements, no regressions.
  - `interfacesWithPredefinedTypesAsNames.ts`
  - `jsExportMemberMergedWithModuleAugmentation2.ts`
  - `catchClauseWithTypeAnnotation.ts`
  - `templateLiteralTypes5.ts`
