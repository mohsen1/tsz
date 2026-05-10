# fix(parser): align reachability recovery diagnostics

- **Date**: 2026-05-10
- **Branch**: `fix/reachability-checks-parser-recovery-2026-05-10`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance parser recovery

## Intent

Fix the fingerprint-only parser recovery drift in
`TypeScript/tests/cases/compiler/reachabilityChecksNoCrash1.ts`.
The goal is to match tsc's TS1xxx recovery anchors without broad parser
fallbacks or test-specific special cases, and to add focused parser tests that
cover the recovery shape behind the conformance case.

## Files Touched

- Production and regression-test files TBD after triage.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter reachabilityChecksNoCrash1 --verbose --print-fingerprints --workers 1`
- Planned: focused parser regression test in the owning parser module.
