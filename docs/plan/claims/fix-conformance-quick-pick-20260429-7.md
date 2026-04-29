# [WIP] fix(conformance): restore JSDoc prefix/postfix parsing diagnostics

- **Date**: 2026-04-29
- **Branch**: `fix/conformance-quick-pick-20260429-7`
- **PR**: #1823
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Picked by `scripts/session/quick-pick.sh` on 2026-04-29 20:40:57 UTC.
Target `TypeScript/tests/cases/conformance/jsdoc/jsdocPrefixPostfixParsing.ts`
currently emits no diagnostics where `tsc` expects `TS1005`, `TS1014`,
`TS7006`, and `TS8024`. This PR will diagnose the parser/checker boundary
root cause, restore the missing diagnostics in the owning layer, and add a
focused Rust regression test.

## Files Touched

- TBD after investigation.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "jsdocPrefixPostfixParsing" --verbose`
- Planned: targeted `cargo nextest run` for touched crates
