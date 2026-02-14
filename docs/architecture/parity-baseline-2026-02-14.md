# TSZ Parity Baseline (2026-02-14)

Status: Phase 0 baseline snapshot  
Date: 2026-02-14  
Scope: TypeScript conformance slice (`offset=0`, `max=500`)

## Repro Commands

```bash
./scripts/conformance.sh run --offset 0 --max 500 --workers 16
./scripts/conformance.sh analyze --offset 0 --max 500 --workers 16 --top 15
```

## Baseline Result

From `./scripts/conformance.sh run --offset 0 --max 500 --workers 16`:

- Passed: `438/499` (`87.8%`)
- Skipped: `1`
- Crashed: `0`
- Timeout: `0`

## Top Error Code Mismatches

- `TS2322`: missing `10`, extra `8`
- `TS2345`: missing `1`, extra `7`
- `TS2769`: missing `0`, extra `6`
- `TS2304`: missing `5`, extra `1`
- `TS2339`: missing `2`, extra `2`
- `TS2693`: missing `3`, extra `0`
- `TS2504`: missing `1`, extra `1`
- `TS2740`: missing `0`, extra `2`
- `TS2739`: missing `1`, extra `1`
- `TS1005`: missing `0`, extra `2`

## Failure Taxonomy (61 failing tests)

From `./scripts/conformance.sh analyze --offset 0 --max 500 --workers 16 --top 15`:

- False positives: `16`
- All missing: `13`
- Wrong codes: `32`
- Close to passing (diff <= 2): `19`

## Highest-Impact Not-Implemented Codes

Codes reported as not emitted by `tsz` in this slice:

- `TS2693` (3 tests)
- `TS2741` (2 tests)
- `TS2461` (2 tests)
- `TS2488` (2 tests)
- `TS2705` (2 tests)
- `TS2589` (2 tests)
- `TS2737` (2 tests)

## Initial Priority Targets (Phase 0 -> Phase 1 handoff)

1. Reduce false positives centered on `TS2322`, `TS2345`, `TS2769`.
2. Raise coverage of partially implemented `TS2322` and `TS2304`.
3. Implement high-impact missing diagnostics starting with `TS2693`, `TS2741`, `TS2461`, `TS2488`, `TS2705`.

## Post-Refactor Verification (2026-02-14)

After checker boundary migration steps (query-boundary routing + guardrails), reran:

```bash
./scripts/conformance.sh run --offset 0 --max 500 --workers 16
```

Result remained stable:

- Passed: `438/499` (`87.8%`)
- Skipped: `1`
- Crashed: `0`
- Timeout: `0`

## Targeted Parity Win (2026-02-14)

Implemented shorthand-property missing-value diagnostic (`TS18004`) for object literals in checker.

Targeted validation:

```bash
./scripts/conformance.sh run --filter "argumentsReferenceInObjectLiteral_Js.ts" --workers 16
```

Result:

- `1/1` passed (`100.0%`)

Updated slice validation:

```bash
./scripts/conformance.sh run --offset 0 --max 500 --workers 16
```

Result improved:

- Previous: `438/499` (`87.8%`)
- Current: `439/499` (`88.0%`)
