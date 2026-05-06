# Claim: Fix in-keyword typeguard diagnostics

## Target

`TypeScript/tests/cases/compiler/inKeywordTypeguard.ts`

Snapshot category: wrong-code.

Expected diagnostics include TS2638.

Actual diagnostics currently include an extra TS7053 instead of TS2638.

## Plan

Investigate `in`-operator narrowing and index-expression diagnostics around unknown/object operands. Align the checker so the conformance case reports TS2638 and avoids the extra TS7053 without weakening valid element-access errors.

## Verification

Planned:

- focused checker regression
- filtered conformance for `inKeywordTypeguard`
