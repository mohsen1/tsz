---
name: Async generator class method parser extras
status: claim
timestamp: 2026-05-06 12:06:28
branch: fix/conformance-next-20260506-120628
---

# Claim

Workstream 1 (Diagnostic Conformance) for
`TypeScript/tests/cases/conformance/parser/ecmascript2018/asyncGenerators/parser.asyncGenerators.classMethods.es2018.ts`.

## Scope

Remove the extra TS1212/TS1213 parser diagnostics while preserving the expected
TS1005, TS1109, and TS5024 output for async generator class method fixtures.

## Verification Plan

- Focused parser/checker unit coverage in the owning area.
- `cargo nextest run` for affected tests.
- `./scripts/conformance/conformance.sh run --filter "parser.asyncGenerators.classMethods.es2018" --verbose`
