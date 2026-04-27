# fix(checker): emit TS2502 for self-indexed property annotations

- **Date**: 2026-04-27
- **Time**: 2026-04-27 01:29:38 UTC
- **Branch**: `codex/conformance-roadmap`
- **PR**: pending
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fix the missing TS2502 diagnostics in
`TypeScript/tests/cases/conformance/types/keyof/circularIndexedAccessErrors.ts`
for property declarations whose annotation indexes the containing type by the
same property name:

```ts
type T = { x: T["x"] };
interface I { x: I["x"] }
class C { x: C["x"] }
```

This is separate from the active type-display, JSX, index-signature, overload,
namespace/import-qualification, and parser-recovery worktrees. The change belongs
in checker circularity detection around member declarations and type literals.

## Verification Plan

CI only per request. Add focused checker regression coverage and rely on the PR
checks/full conformance to validate behavior.
