# TS2322 Conformance Gap Report (2026-02-15)

## Current observed baseline (after latest `main` sync)

Command:
- `./scripts/conformance.sh analyze --error-code 2322 --top 1000`

Observed (from `./scripts/conformance.sh analyze` + `./scripts/conformance.sh run --error-code 2322 --verbose`):

- `TS2322` partial: missing=755, extra=95
- Conformance top-level failure split:
  - compiler: `394`
  - conformance: `361`
- Full run stats (`run --error-code 2322 --verbose`):
  - missing-only: `755`
  - extra-only: `95`
  - both: `92`
- Quick-win bucket: 325 single-code failures are `TS2322`-only.

## Why this looks like ~700+ errors, not a small regression

The missing mass is not random:
- 394 compiler-facing failures + 361 conformance-facing failures.
- It clusters in a small number of semantic families repeatedly emitting `TS2322` only in expected/actual deltas.
- This points to compatibility/assignability surfaces (not parser/reporter plumbing).

## Top TS2322 missing families (file-level aggregation)

Highest concentration from run outputs (current run, missing only):

1. `assignmentCompatability*` (31)
2. `typeTagPrototypeAssignment` (30)
3. `declarationFunctionDeclarations` (9)
4. `assignmentCompatWithConstructSignatures` (6)
5. `assignmentCompatWithCallSignatures` (6)
6. `interMixingModulesInterfaces` (6)
7. `jsdocTypeCast` (6)
8. `generatorTypeCheck` (4)
9. `declarationFilesWithTypeReferences` (4)
10. `contextualTyping` (4)

## Top suite-path concentration (TS2322 missing)

Top conformance buckets:
- `conformance/types` (166)
- `conformance/es6` (62)
- `conformance/expressions` (47)
- `conformance/jsdoc` (33)
- `conformance/jsx` (29)
- `conformance/classes` (26)
- `conformance/parser` (8)

## Additional signal: unit test in checker currently failing in main

- `crates/tsz-checker/tests/ts2322_tests.rs::test_ts2322_check_js_true_does_not_relabel_with_unrelated_diagnostics`
- This test exercises JS + JSDoc generic inference:
  - `// @ts-check`
  - generic `wrap(value)` returning `value: T`
  - `const n: number = wrap("string");`
- Current failure indicates there may be a second TS2322-regression path in JSDoc/check-js generic assignability, separate from conformance-only failures.

## Failure map hypothesis

1. `TS2322` misses are dominated by object/assignability compatibility behavior at solver boundary (optional/required properties, index signatures, and callability intersections).
2. Secondary misses in conformance suites indicate contextual typing + inference-heavy paths and type-relationship helpers still produce wrong mismatch routing.
3. `TS2322` extras (`95`) are smaller but present in both compiler/conformance; likely from over-eager mismatch/reporting in inference/flow paths.

## Step-by-step remediation plan

### Step 1 – lock evidence for assignment/callability core
1. Run focused capture again and persist outputs:
   - `./scripts/conformance.sh run --error-code 2322 --filter assignmentCompatability --verbose > /tmp/assign_compat_2322.txt`
   - `./scripts/conformance.sh run --error-code 2322 --filter typeTagPrototypeAssignment --verbose > /tmp/typeTagProto_2322.txt`
2. Trace one-pass for each using existing solver checker logs:
   - run conformance on `assignmentCompatability11.ts` + `typeTagPrototypeAssignment1.ts`-adjacent fixture with TS2322-relevant logs on.
3. Confirm whether fault is:
   - wrong source/target `TypeId` preparation, or
   - permissive object/member relation result in solver subtype path.

### Step 2 – fix high-confidence miss cluster
1. Patch only assignability-compatible code paths (solver-first, checker-only orchestration).
2. Target at first:
   - required-property and namespace member compatibility
   - object/index/member overlap behavior that currently allows unsound acceptance.
3. Re-run:
   - `./scripts/conformance.sh run --error-code 2322 --filter assignmentCompatability --verbose`
   - `./scripts/conformance.sh analyze --error-code 2322 --top 1000`
4. Gate:
   - `assignmentCompatability*` missing must move to 0
   - no increase in `TS2322` extras.

### Step 3 – broad TS2322 sweep (non-extra)
1. Move to conformance family concentration:
   - `./scripts/conformance.sh run --error-code 2322 --filter conformance/types --verbose`
   - `./scripts/conformance.sh run --error-code 2322 --filter conformance/es6 --verbose`
2. Capture before/after deltas and keep a list of remaining misses.
3. Re-run `analyze` slice and compare `missing`/`both`/`extra` totals per family.

### Step 4 – address TS2322 extras and JS JSDoc regression
1. Focus on diagnostics currently present but not expected:
   - contextual/inference-heavy suites where TS2322 appears spuriously.
2. Add/validate unit-level guard with `tests/ts2322_tests.rs` target case:
   - keep this failing check-js generic test green while restoring missing errors elsewhere.
3. Re-run:
   - `cargo test test_ts2322_check_js_true_does_not_relabel_with_unrelated_diagnostics --package tsz-checker -- --nocapture`
   - full TS2322 analyze slice if gate passes.

### Step 5 – final acceptance before broadening to TS2564/TS2454
1. Acceptance criteria:
   - `TS2322 missing` reduced substantially from 755 (target 50%+ per plan state)
   - `TS2322 extra` not increased
   - regression suite (`test_ts2322_check_js_true_does_not_relabel_with_unrelated_diagnostics`) passes
2. Then continue with Step 2/3 of broader plan for `TS2564`/`TS2454` coupling.


## Test ignore / skip pressure (new signal for investigation quality)

Current ignored test inventory snapshot (non-parser `#[ignore]`):

- 99 ignores with explicit reason strings: `#[ignore = "..."]`
- 9 ignores without explicit reason: bare `#[ignore]`
- No `cfg_attr(..., ignore(...))` skip patterns found.

Top skip hotspots:
- `tests/checker_state_tests.rs:63` (largest concentration)
- `crates/tsz-checker/tests/control_flow_tests.rs:9`
- `crates/tsz-checker/tests/generic_tests.rs:6`
- `crates/tsz-checker/tests/freshness_stripping_tests.rs:6`
- `crates/tsz-lsp/tests/project_tests.rs:4`
- `crates/tsz-lsp/tests/signature_help_tests.rs:3`
- `crates/tsz-checker/tests/conformance_issues.rs:3`
- Remaining files are mostly single-digit or one-off ignores.

Unreasoned `#[ignore]` locations (9 total):
- `tests/checker_state_tests.rs:677`
- `tests/checker_state_tests.rs:6061`
- `tests/checker_state_tests.rs:6108`
- `tests/checker_state_tests.rs:12604`
- `tests/checker_state_tests.rs:15593`
- `tests/checker_state_tests.rs:15670`
- `tests/checker_state_tests.rs:16755`
- `crates/tsz-checker/tests/ts2304_tests.rs:232`
- `crates/tsz-solver/tests/evaluate_tests.rs:9716`

Why it matters for this regression:
- The current TS2322 drop is concentrated in tested behavior, not skipped behavior.
- The presence of bare ignores in solver/control-flow paths increases confidence risk for regression tracking.
- Keep this list as a separate quality gate while fixing conformance gaps; convert bare ignores only when behavior is intentionally unsupported.

## Latest focused pass (post-breakdown)

Focused run artifacts captured on current `main`:

### `assignmentCompatability` slice
- Command: `./scripts/conformance.sh run --error-code 2322 --filter assignmentCompatability --verbose`
- Result: `TS2322` missing=31, extra=0
- Also observed: `TS2454` missing=3, `TS2741` missing=2 (no TS2322 extra)
- Failing fixture names observed: 31/33 compiler assignment files, including:
  - `TypeScript/tests/cases/compiler/assignmentCompatability11.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability12.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability13.ts`
  - `...`
  - `TypeScript/tests/cases/compiler/assignmentCompatability45.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability_checking-apply-member-off-of-function-interface.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability_checking-call-member-off-of-function-interface.ts`

### `typeTagPrototypeAssignment` slice
- Command: `./scripts/conformance.sh run --error-code 2322 --filter typeTagPrototypeAssignment --verbose`
- Result: `TS2322` missing=1, extra=0
- Failing fixture:
  - `TypeScript/tests/cases/conformance/jsdoc/typeTagPrototypeAssignment.ts`
  - `expected: [TS2322], actual: []` under options `{allowjs: true, checkjs: true, noemit: true, target: es2015}`

Implication:
- The assignment compatibility block confirms a deterministic, high-confidence miss cluster where assignability is not rejecting invalid assignment in expected spots (not reporting enough errors).
- This is stronger evidence that the next fix should stay in the solver/checker assignability boundary and not in parser/infrastructure layers.

## Concrete diagnosis from `assignmentCompatability11.ts`

- Command: `./scripts/conformance.sh run --error-code 2322 --filter assignmentCompatability11 --verbose`
- TS2322 expected: `[TS2322]`, actual: `[]`
- Source snippet under test:
  - `interfaceWithPublicAndOptional<T,U> { one: T; two?: U; }`
  - assign `interfaceWithPublicAndOptional<number,string>` to `{ two: number }`
- This should fail because target property `two` is required, while source `two` is optional.
- Current behavior indicates the assignment logic is treating optional-source object members as satisfying required-target members in this path.
- Interpretation:
  - likely object member relation bug around requiredness propagation.
  - high confidence this is in `assignability` / property relation handling rather than parser/infrastructure.

### Suggested micro-step for next implementation cycle
1. Open the property relation path used for `is_assignable` on interfaces/object types.
2. Verify requiredness checks are not bypassed by union normalization or member fallback.
3. Ensure object-with-optional property is not considered assignable to object-with-required of same name unless `--exactOptionalPropertyTypes` semantics are engaged.
4. Validate by re-running:
   - `./scripts/conformance.sh run --error-code 2322 --filter assignmentCompatability11 --verbose`
   - `./scripts/conformance.sh run --error-code 2322 --filter assignmentCompatability --verbose`
