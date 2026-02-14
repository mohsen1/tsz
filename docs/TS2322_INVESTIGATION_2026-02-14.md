# TS2322 Investigation (2026-02-14)

## Critical correction (2026-02-14, post-architecture update)

The original investigation run can be skewed if `tsz-conformance` uses `tsz` from `PATH`
instead of the workspace build artifact.

- Verified example: `compiler/arrayFind.ts`
  - With default `tsz` from `PATH`: reported extra `TS2322`
  - With workspace binary (`./.target/dist-fast/tsz`): passes with no diagnostics

Fundamental fix applied in code:

- `crates/conformance/src/runner.rs` now resolves default `tsz` to
  `./.target/dist-fast/tsz` when present.
- This makes parity runs deterministic to the checked-out workspace implementation
  and prevents stale-binary false positives/negatives in TS2322 analysis.

Operational rule going forward:

- If `--tsz-binary` is omitted, conformance prefers workspace fast build.
- If an explicit `--tsz-binary` is provided, that path is used as-is.

## Scope

Investigate TS2322 parity mismatches using the conformance harness and identify high-impact failure patterns plus likely ownership boundaries.

## Command Used

```bash
./.target/dist-fast/tsz-conformance \
  --cache-file ./tsc-cache-full.json \
  --error-code 2322 \
  --workers 8 \
  --timeout 5 \
  --print-test \
  > /tmp/ts2322_detailed.log 2>&1
```

Artifacts used:

- `/tmp/ts2322_detailed.log`
- `/tmp/ts2322_analysis3.txt` (from `python3 scripts/analyze-conformance.py /tmp/ts2322_detailed.log`)

## Result Snapshot

- Run date: 2026-02-14
- Final results from this run: `7730/12546 passed (61.6%)`
- Skipped: `16`
- Crashed: `1`
- Timeout: `0`
- TS2322 top mismatch count in this run: `missing=280, extra=379`

Note: this run was filtered by `--error-code 2322` and is intended for diagnosis, not as canonical global parity status.

## TS2322 Buckets

Derived from `/tmp/ts2322_detailed.log`:

- `missing_2322` (`expected` contains TS2322, `actual` does not): `280`
- `extra_2322` (`actual` contains TS2322, `expected` does not): `379`
- `both` (TS2322 appears in both sides, but other code mismatches still fail): `172`

## High-Signal Patterns

### 1) False-positive TS2322 pressure is high

- `252` failing tests have `expected=[]` but we still emit errors.
- Top extra code in those: `TS2322`.
- Representative tests:
  - `TypeScript/tests/cases/compiler/arrayFind.ts`
  - `TypeScript/tests/cases/compiler/arrayconcat.ts`
  - `TypeScript/tests/cases/compiler/asyncFunctionReturnType.ts`
  - `TypeScript/tests/cases/compiler/contextualReturnTypeOfIIFE.ts`

### 2) Missing TS2322 is also high

- `135` failing tests have `actual=[]` where tsc expects errors, often TS2322.
- Representative tests:
  - `TypeScript/tests/cases/compiler/arrowExpressionBodyJSDoc.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability35.ts`
  - `TypeScript/tests/cases/compiler/contextualTypingOfArrayLiterals1.ts`
  - `TypeScript/tests/cases/compiler/genericCloneReturnTypes2.ts`

### 3) Wrong-code substitutions are common

Frequent replacements when TS2322 is expected:

- TS2322 -> TS2345
- TS2322 -> TS2740/TS2739 (shape-focused diagnostics without expected top-level assignability code)
- TS2322 absent due to over-routing to other diagnostics

Example mismatches:

- `TypeScript/tests/cases/compiler/allowJscheckJsTypeParameterNoCrash.ts`
  - expected: `[TS2322]`
  - actual: `[TS2345]`
- `TypeScript/tests/cases/compiler/assignmentToObject.ts`
  - expected: `[TS2322]`
  - actual: `[TS2740]`

## Option/Mode Correlations

Observed high co-occurrence with TS2322 mismatch buckets:

- `extra_2322`: often with `target:*`, `strict:true`
- `missing_2322`: often with `allowJs:true`, `checkJs:true`, `jsx:*`, plus `target:*`

This indicates mismatch behavior is sensitive to mode/config-specific checker paths.

## Likely Root-Cause Areas (Ownership-First)

### Checker orchestration (WHERE)

- `crates/tsz-checker/src/assignability_checker.rs`
  - Central TS2322 emission and suppression gates.
- `crates/tsz-checker/src/assignment_checker.rs`
  - Assignment path anchoring/suppression and assignment-specific routing.
- `crates/tsz-checker/src/error_handler.rs`
  - Shared error emission entrypoints that should route through central assignability helpers.

### Checker -> Solver boundary

- `crates/tsz-checker/src/query_boundaries/assignability.rs`
  - Relation/explain bridge; correctness depends on consistent boundary usage across checker paths.

### Solver compatibility policy (WHAT)

- `crates/tsz-solver/src/lawyer.rs`
  - Any propagation, variance, weak-type behavior, and other compatibility quirks that influence assignability outcomes.

## Prioritized Follow-ups

1. Enumerate top repeated `(expected, actual)` mismatch signatures for TS2322 and map each to a single checker entrypoint.
2. Audit all TS2322/TS2345 emission callsites to ensure centralized gateway usage only (no ad-hoc branches).
3. Focus first on high-volume false positives in strict/target paths, then JS/JSDoc missing TS2322 paths.
4. Add focused regression tests for:
   - TS2322 vs TS2345 routing
   - TS2322 vs TS2740/TS2739 prioritization
   - JS/JSDoc contextual typing assignability diagnostics
