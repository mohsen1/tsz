# Conformance Recovery Plan (2026-02-15 Step-by-Step)

## 0) Starting point (after syncing to `origin/main`)

- Command executed: `./scripts/conformance.sh analyze --error-code 2322 --filter assignmentCompatability --top 1000`
- Result:
  - Total analyzed: `33`
  - Missing (expected diagnostics, none emitted): `31`
  - Wrong codes: `2`
  - False positives: `0`
- All 31 missing are `TS2322`.
- Missing files:
  - `TypeScript/tests/cases/compiler/assignmentCompatability11.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability12.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability13.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability14.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability15.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability16.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability17.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability18.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability19.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability20.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability21.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability22.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability23.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability24.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability25.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability26.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability27.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability28.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability29.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability30.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability31.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability32.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability33.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability34.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability35.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability37.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability38.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability39.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability43.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability44.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability45.ts`
- Close-to-pass (1 missing code):
  - `TypeScript/tests/cases/compiler/assignmentCompatability_checking-apply-member-off-of-function-interface.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability_checking-call-member-off-of-function-interface.ts`

## 1) Working hypothesis (aligned to NORTH_STAR)

The entire failure slice is assignability semantics in the solver boundary, likely in object relation evaluation, not checker orchestration.

- Expected behavior in all 31 files is `TS2322` only.
- Pattern aligns with required property checks not rejecting incompatible shapes (`two` requiredness, function-interface edges, class function-interface edges, and optional/readonly members).
- Prior trace observations indicate checker invokes `check_assignable_to` and gets `true`, so solver object subtype compatibility is likely returning permissive results.

## 2) Step-by-step execution plan

### Step A – Evidence capture (read-only)

1. Re-run focused conformance slice and capture canonical output:
   - `./scripts/conformance.sh analyze --error-code 2322 --filter assignmentCompatability --top 1000`
2. Re-run canonical failing single case to confirm expected/actual:
   - `./scripts/conformance.sh run --error-code 2322 --filter assignmentCompatability11.ts --verbose`
3. Record per-file signatures and target/source pairs if possible from logs.
4. No code changes in this step.
5. Commit this evidence snapshot as `docs: record step-a conformance evidence`.

### Step B – Path tracing on two anchors

1. Add temporary trace on:
   - `TypeScript/tests/cases/compiler/assignmentCompatability11.ts`
   - `TypeScript/tests/cases/compiler/assignmentCompatability21.ts`
2. Capture these values in trace:
   - resolved source/target `TypeId`
   - relation call chain in solver assignment/subtype
   - object shape/property pass/fail state
3. Confirm fault layer:
   - if source/target mapping is wrong -> checker/type-resolution fix
   - if mapping is correct and relation still true -> solver object rule fix
4. Commit trace findings in short notes under `docs/`.

### Step C – Fix scope A (solver object subtype)

1. If Step B points to solver, patch only object subtype compatibility:
   - `crates/tsz-solver/src/subtype_rules/objects.rs`
2. Restrict changes to required-property logic and optional-member compatibility in object/object-with-index paths.
3. Keep relation semantics; do not alter checker ownership.
4. Re-run:
   - `./scripts/conformance.sh run --error-code 2322 --filter assignmentCompatability11.ts --verbose`
   - `./scripts/conformance.sh run --error-code 2322 --filter assignmentCompatability21.ts --verbose`
5. If pass for both, commit with `solver: tighten object required-member assignability check`.

### Step D – Fix scope B (namespace member/type retrieval fallback)

1. If Step B shows namespace source/target type materialization issues, isolate only name resolution/symbol typing path used in assignment checking.
2. Inspect minimal path in `crates/tsz-checker` around `TypeData` boundaries and `query_boundaries::assignability`.
3. Keep edits constrained to one module and one failing callsite.
4. Re-run focused commands above.
5. Commit with `checker: preserve assignment namespace member shape` if validated.

### Step E – Acceptance gate

1. Re-run:
   - `./scripts/conformance.sh run --error-code 2322 --filter assignmentCompatability`
   - `./scripts/conformance.sh analyze --error-code 2322 --filter assignmentCompatability --top 1000`
2. Required passing condition before moving on:
   - missing in assignmentCompatability slice must drop from `31` to `0`
   - no increase in `TS2741` or other spurious misses in the two close-to-pass files.
3. If gate passes, next stage begins on broader `TS2322` and then `TS2564/TS2454` families.

## 3) Sync and commit protocol

- One commit per step.
- After each commit:
  - `git fetch origin main`
  - `git rebase origin/main`
- Continue only when rebase is clean.
- Keep edits minimal and local to touched file.
- Preserve architecture: any type relation behavior change must live in `tsz-solver`; checker changes should only enforce boundary orchestration.
