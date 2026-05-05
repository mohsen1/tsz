# [WIP] fix(checker): parenthesized contextual implicit-any diagnostic

- **Date**: 2026-05-05
- **Branch**: `fix/checker-parenthesized-contextual-implicit-any`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance / missing diagnostics

## Intent

Random conformance pick selected
`TypeScript/tests/cases/conformance/expressions/contextualTyping/parenthesizedContexualTyping2.ts`.
The test is currently only-missing `TS7006`; tsz matches the other expected
codes (`TS2322`, `TS2345`, `TS2347`, `TS2695`) but fails to report one
implicit-any parameter diagnostic in a parenthesized contextual-typing shape.

## Files Touched

- TBD after root-cause analysis.

## Verification

- Baseline target command:
  `./scripts/conformance/conformance.sh run --filter "parenthesizedContexualTyping2" --verbose`
- Planned: focused checker regression test for the missing implicit-any path.
- Planned: targeted conformance rerun for `parenthesizedContexualTyping2`.
