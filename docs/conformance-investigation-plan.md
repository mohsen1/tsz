# Conformance Recovery Plan

## Baseline (latest `main`)

Last full run: `./scripts/conformance.sh analyze` after syncing to `origin/main`.

- Total analyzed: 5388
- Missing diagnostics: 2474
- Extra diagnostics: 745
- Wrong code: 2169
- Close-to-passing: 1421

Top buckets of concern:

- `TS2322`: missing=755, extra=95
- `TS2564`: missing=623, extra=4
- `TS2454`: missing=444, extra=8

### TS2322 breakdown (from verbose run)

- `MISSING`: 755
- `EXTRA`: 95
- `BOTH`: 91
- Missing split: compiler=394, conformance=361
- overlap with `TS2564`: 83
- overlap with `TS2454`: 56
- overlap with both: 22

### Step 0 execution log (current run, 2026-02-15)

- `./scripts/conformance.sh run --error-code 2322` produced:
  - `missing=755`, `extra=95`, `both=91`, total 941 categorized cases.
  - `missing-only` list path: `/tmp/fail_2322_missing_only.txt`
  - `extra-only` list path: `/tmp/fail_2322_extra_only.txt`
  - `both` list path: `/tmp/fail_2322_both_only.txt`
- Hot TS2322 families:
  - `TypeScript/tests/cases/compiler/assignmentCompatability*` (31 missing)
  - `TypeScript/tests/cases/conformance/types/typeRelationships/assignmentCompatibility/*` (31 missing, 3 both)
  - `TypeScript/tests/cases/conformance/es6/destructuring/*` (many hits in both missing and overlap)
  - `TypeScript/tests/cases/conformance/classes/*` (broad spread across member/declaration/construction paths)
- TS2322 missing distribution by suite:
  - `conformance`: 361
  - `compiler`: 394
- TS2322 extra distribution by suite:
  - `conformance`: 47
  - `compiler`: 95 - 47 = 48 (exactly 48)
- TS2322 overlap (`both`) distribution by suite:
  - `conformance`: 34
  - `compiler`: 57

#### Step 0 working hypotheses from this classification

1. `TS2322` misses are not random; there is a clear concentration in assignment/callability compatibility surfaces and destructuring/type-guard class of conformance tests.
2. `extra` is dominated by edge-flow/conformance diagnostics where checker and solver expectations currently diverge on contextual flow and symbol interactions.
3. The `query_boundaries`/`assignability` path should be first-order target for next fixes before broadening flow-only paths.

Representative hot files already observed:

- `TypeScript/tests/cases/compiler/assignmentCompatability11.ts`
- `TypeScript/tests/cases/compiler/arrayAssignmentTest1.ts`
- `TypeScript/tests/cases/compiler/addMoreCallSignaturesToBaseSignature.ts`
- `TypeScript/tests/cases/compiler/accessorDeclarationOrder.ts`
- `TypeScript/tests/cases/compiler/destructuringAssignmentWithDefault.ts`
- `TypeScript/tests/cases/conformance/classes/propertyMemberDeclarations/strictPropertyInitialization.ts`
- `TypeScript/tests/cases/conformance/types/typeRelationships/assignmentCompatibility/assignmentCompatWithCallSignatures.ts`

## Root-cause hypothesis from observed signatures

Regression appears centered in the compatibility/gatekeeping surface, not syntax or lowering:

1. `TS2322` missing diagnostics are concentrated in assignment/callability/structural compatibility.
2. `TS2564`/`TS2454` misses indicate flow and definite-assignment model drift, with substantial overlap into `TS2322` cases.
3. `NORTH_STAR.md` and current architecture policy already frame this as a Solver boundary problem:
   - `TS2322`/`TS2345`/`TS2416` must go through one compatibility gateway (`query_boundaries -> assignability`), not ad-hoc checker paths.
   - `TypeData`/`TypeKey` internals should stay Solver-private.

## Execution plan (step-by-step)

### Step 0 – Stability and instrumentation checkpoint

1. Re-run:
   - `./scripts/conformance.sh analyze`
   - `./scripts/conformance.sh compare --error-code 2322`
   - `./scripts/conformance.sh compare --error-code 2564`
   - `./scripts/conformance.sh compare --error-code 2454`
2. Capture canonical missing files into `/tmp/*_missing_only.txt` per run.
3. Add lightweight classification script outputs to make deltas reviewable.
4. Establish a “must-pass today” list:
   - no regression on already green files in the tracked sample set.
   - no `TS2322 extra` increase by code path touched in commit.

### Step 1 – Bring `TS2322` missing under control (highest priority)

1. Route all relevant checker paths through the designated assignability boundary if any still call compatibility internals directly.
2. Verify assignment, call signature, and indexed access compatibility queries consistently preserve:
   - variance behavior
   - literal/readonly handling
   - intersection/union normalization behavior
   - optional/rest/any compatibility policy in Lawyer + Judge split
3. Compare `TS2322` misses by family and fix in this order:
   - `assignmentCompatibility*` compiler tests
   - `addMoreCallSignaturesToBaseSignature*`
   - `assignmentCompatWithCallSignatures` conformance subset
4. For each fix:
   - run filtered conformance only for affected paths
   - update the `TS2322` missing list and record improvement in plan log

### Step 2 – Reclaim `TS2564` + `TS2454`

1. Audit flow graph nodes/symbol states feeding definite assignment checks.
2. Confirm checker only requests semantic answers from Solver and does not walk raw type internals.
3. Fix missing “must be assigned before use” (`TS2454`) and strict property init (`TS2564`) together where coupled.
4. Validate with targeted:
   - property/member initialization cases
   - destructuring/default/finally/capture control-flow cases
   - constructor and class fields paths

### Step 3 – Reduce `TS2322` extras (false positives)

1. Stop reporting via ad-hoc mismatch branches when the boundary already provides structured failures.
2. Strengthen diagnostic rendering priority from Solver failure reason:
   - prefer structural mismatch reasons
   - avoid `any`-based suppression unless via compatibility policy gate
3. Re-run conformance slices and ensure `extra` count does not trend up while fixing misses.

### Step 4 – Architecture audit + lock-down

1. Verify each touched file’s role against `NORTH_STAR.md`/AGENT rules:
   - “WHAT” code in Solver, “WHERE” code in Checker.
   - no direct `TypeKey`/raw interner usage in Checker pathways touched.
2. Add/adjust tests for each behavior fixed, with minimal scope.
3. Update internal notes and keep each checkpoint committed.

## Process protocol

- Work is commit-by-commit with one diagnostic family per cycle when possible.
- After each commit that changes behavior, run:
  - `./scripts/conformance.sh compare --error-code <family>` for the family touched
- Run full analyze every few commits (or when requested) before broader merge.
- Sync with `main` every cycle boundary or before any risky change.
- No broad architectural refactors without explicit `NORTH_STAR` alignment review.

## Success criteria

- `TS2322` missing reduced by at least ~50% before moving to broad reductions in `TS2564`/`TS2454`.
- `TS2564` + `TS2454` misses show sustained decline without increasing `TS2322` extras.
- No increase in `TS2322` extras from new changes.
- End-to-end conformance should move from current degraded state to a near-flat delta in `wrong-code` and `close-to-passing` buckets.
