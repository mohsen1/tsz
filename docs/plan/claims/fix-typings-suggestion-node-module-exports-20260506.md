---
name: Typings suggestion Node module.exports diagnostic
status: claim
timestamp: 2026-05-06 12:42:57
branch: fix/conformance-next-20260506-124257
---

# Claim

Workstream 1 (Diagnostic Conformance) for
`TypeScript/tests/cases/conformance/typings/typingsSuggestion1.ts`.

## Scope

Align the missing Node global diagnostic for `module.exports` when
`compilerOptions.types` is explicitly empty. tsc reports TS2591, while tsz
currently reports TS2580.

## Verification Plan

- Focused diagnostic/unit coverage for Node global environment capabilities.
- `cargo nextest run` for affected checker tests.
- `./scripts/conformance/conformance.sh run --filter "typingsSuggestion1" --verbose`
