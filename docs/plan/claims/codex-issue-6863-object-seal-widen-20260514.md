# fix(checker): widen Object.seal mutable object literals

- **Date**: 2026-05-14
- **Branch**: `codex/issue-6863-object-seal-widen-20260514`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance / false-positive fixes

## Intent

Fix #6863, where `Object.seal({ x: 1 })` preserves the literal property type and causes a false positive assignment error for `sealed.x = 2`. The fix should preserve `Object.freeze`/`as const` literal behavior while widening mutable object literal property values for `Object.seal` compatibility with TypeScript.

## Files Touched

- TBD

## Verification

- TBD
