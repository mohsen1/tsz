# Conformance Recovery Plan (Step-by-Step)

## 1) Current baseline (after syncing to latest `main`)

Commands executed in this session:
- `git fetch origin main`
- `git rebase origin/main`
- `./scripts/conformance.sh analyze`
- `./scripts/conformance.sh analyze --error-code 2322 --filter assignmentCompatability`
- `./scripts/conformance.sh run --error-code 2322 --filter assignmentCompatability`
- `./scripts/conformance.sh analyze --error-code 2741 --filter assignmentCompatability`

### Snapshot

- Total conformance failures analyzed: `5356`
- False positives: `743`
- Missing diagnostics: `2448`
- Wrong-code: `2165`
- Close-to-passing (<=2 code diff): `1416`

`TS2322` status (from current baseline):
- Missing: `506`
- Extra: `44` (`TS2322` among extra causes)
- Partially implemented aggregate still: `327` single-code tests missing only `TS2322`

### Assignment compatibility focused slice (`--error-code 2322 --filter assignmentCompatability`)

- Total failures in slice: `33`
- Missing `[TS2322]`: `31`
- Wrong-code (diff=1, missing `TS2741`): `2`

Missing files (all expected `TS2322`, actual `[]`):
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

Close-to-pass files (diff=1, missing `TS2741`):
- `TypeScript/tests/cases/compiler/assignmentCompatability_checking-apply-member-off-of-function-interface.ts`
- `TypeScript/tests/cases/compiler/assignmentCompatability_checking-call-member-off-of-function-interface.ts`

Representative expected/actual sample (`assignmentCompatability11.ts`) confirms the pattern:
- Namespace exports + object assignment from a richer interface to a literal-shape object
- `expected [TS2322]`, `actual []`

## 2) Failure interpretation (current best hypothesis)

The current failure cluster is narrowly consistent with a relation-level object mismatch being over-accepted. Prior inspection indicates these assignments follow the same semantic shape:

- Source has required and optional members (interface with required `one`, optional `two?: ...`)
- Target is a narrow object-type value in another namespace
- Checker flow reaches solver assignment path
- Actual result is permissive despite required/optional incompatibility in source/target relation

Most likely fault sites:
1. Object property compatibility in solver subtype path (especially required-member enforcement and optional-handling)
2. Namespace/value-member retrieval producing a target/source `TypeId` shape that accidentally drops requiredness/flags before relation
3. Compatibility path not flowing through the expected assignability failure reason branch before returning success

## 3) Plan of record (step-by-step)

### Step 1 – Evidence freeze and reproducibility
- Commit after adding/reconfirming this report.
- Artifacts / verification commands:
  - `./scripts/conformance.sh analyze`
  - `./scripts/conformance.sh analyze --error-code 2322 --filter assignmentCompatability`
  - `./scripts/conformance.sh analyze --error-code 2741 --filter assignmentCompatability`
- No code changes yet in this step.

### Step 2 – Trace the active path on 1–2 canonical files
- Reproduce with a tight trace command on:
  - `TypeScript/tests/cases/compiler/assignmentCompatability11.ts`
  - `TypeScript/tests/cases/compiler/assignmentCompatability21.ts`
- Track:
  - resolved namespace member symbols
  - checker-provided source/target `TypeId`
  - solver relation result and failure reasons
- Confirm whether drop happens in checker type retrieval or solver object compatibility.

### Step 3 – Add regression fixture to isolate relation behavior
- Create/adjust a tiny solver-focused assignment test if absent:
  - interface with required/optional member assignment to object-like target
  - one variant using namespace/value member path
- Keep this test in a narrowly scoped conformance/smoke file so we can validate before/after every change.

### Step 4 – Fix targeted object compatibility condition (if trace points to solver relation)
- Focus likely files:
  - `crates/tsz-solver/src/subtype_rules/objects.rs`
  - related compatibility helper in `crates/tsz-solver/src/compat.rs`
- Keep change minimal and localized to required-property semantics for object-to-object checks.
- Preserve architecture: checker remains orchestration-only and route through `assignability` boundary helpers.

### Step 5 – If trace points to namespace member typing
- Localize to symbol/value-member resolution path in checker layer where namespace exports are projected into `TypeId`.
- Do not modify flow semantics unless trace proves this is the only broken path.

### Step 6 – Acceptance and promotion
- Re-run:
  - `./scripts/conformance.sh run --error-code 2322 --filter assignmentCompatability`
  - `./scripts/conformance.sh analyze --error-code 2322 --filter assignmentCompatability`
  - `./scripts/conformance.sh analyze`
- Target: reduce assignmentCompatability TS2322 misses to 0; then defer broader `TS2322` sweep.

## 4) Commit and sync protocol

For each step above:
1. Make only the smallest meaningful edits.
2. Commit with a scoped message.
3. `git fetch origin main && git rebase origin/main`
4. Continue only after rebase is clean.

## 5) Risk gate (mandatory)

- Must keep TSZ architecture invariant:
  - no checker type-algorithm ownership
  - relation logic in `tsz-solver`
  - `TypeKey` private boundaries preserved
- No broad parser/checker rewrites before object relation fault is confirmed with trace.
- If any step unexpectedly increases unrelated failures, pause and branch into a narrower diagnostic slice before further code changes.
