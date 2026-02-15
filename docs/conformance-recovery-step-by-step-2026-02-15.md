# Conformance Recovery Plan (Step-by-Step)

Status: 2026-02-15 (analysis + plan, pre-fix)
Base branch: synced to `origin/main` (`d55dff00e`) before plan refresh.

## 1) Current conformance snapshot (this run)

Command run:
- `./scripts/conformance.sh analyze`

Output excerpt:
- Total failing tests analyzed: `5356`
- False positives (we emit errors): `743`
- All missing (we emit nothing): `2448`
- Wrong code: `2165`
- Close-to-passing (diff <= 2): `1415`

TS2322 slice:
- `./scripts/conformance.sh analyze --error-code 2322`
  - `missing=506`
  - `extra=95`
  - `both=51`

## 2) Highest-signal failure cluster

- `assignmentCompatability*.ts` family (TypeScript compiler suite) is a concentrated, high-value cluster.
  - `./scripts/conformance.sh analyze --error-code 2322 --filter assignmentCompatability`
  - `31` tests with:
    - `missing: [TS2322]` each
  - This aligns with a previously observed broad `TS2322` break and is likely where first high-value recovery should land.

Notable members in this cluster include (non-exhaustive):
`assignmentCompatability11.ts`, `assignmentCompatability12.ts`, `assignmentCompatability13.ts`, `...`, `assignmentCompatability45.ts` and variants.

Also visible:
- `assignmentCompatability_checking-apply-member-off-of-function-interface.ts`
- `assignmentCompatability_checking-call-member-off-of-function-interface.ts`
  - both now `expected [TS2322, TS2345, TS2741]` with `actual [TS2322, TS2345]` (`TS2741` missing only)

## 3) Why this points at namespace/value-member assignability semantics

Observed runtime flow on `assignmentCompatability11.ts`:
- `resolve_namespace_value_member` trace lines show successful lookup for:
  - `__test2__.__val__obj`
  - `__test1__.__val__obj4`
- `get_type_of_assignment_target` for the namespace member path is the active target typing path (via non-identifier property access).
- `assignment_checker` calls:
  - `check_assignable_or_report_at(source_type = RHS, target_type = LHS)` (direction is correct)
- Yet `assignability_checker` reports:
  - `is_assignable_to source=475 target=435 result=true` and a second successful assignability check later in same file.

Inference:
- Path plumbing is invoked, but one of these stages is over-accepting:
  1) namespace/value member type retrieval may produce a target type that drops required-property constraints, or
  2) assignability relation is not preserving required/optional property semantics for this namespace-driven value-object path.

Given file pattern, this is most likely a `TypeId`-resolved namespace member + object-assignability interaction, not an arbitrary parser or syntax-path issue.

## 4) Step-by-step recovery plan (with commit boundaries)

### Step 0 — Freeze this evidence slice (done)
- Keep `analyze` and `assignmentCompatability` reports as baselines (file artifacts retained under `/tmp/...`).
- Confirm branch sync state before each cycle.

### Step 1 — Narrow by owning pipeline stage (2 commits)
- 1.1 Add minimal logging in existing paths for one failing case:
  - `resolve_namespace_value_member` input/output types
  - `get_type_of_symbol` for `__val__obj` and `__val__obj4`
  - `check_assignment_compatibility` arguments in `assignment_checker`
  - `assignability_checker::is_assignable_to` source/target pre/post-evaluation IDs
- 1.2 Re-run one-file traces (`TSZ_LOG=...`, assignmentCompatability11.ts).
- 1.3 Decide if the fault is in namespace member type construction vs Solver compare.

### Step 2 — Namespace/member resolution validation in checker
- 2.1 Trace the `SymbolId` path for namespace exports:
  - ensure member symbol comes from the expected namespace-export table entry (not from import/module shim tables)
  - ensure `value_declaration`/initializer-based symbol type is used for exported `var` members.
- 2.2 Verify alias handling is not accidentally converting to `type-only` or erased value members.

### Step 3 — Solver object assignability validation
- 3.1 Create a tiny TS fixture harness for just:
  - optional-property source vs required-property target assignment through namespace exports.
- 3.2 In `tsz-solver`, add/extend a regression for object subtype path covering optional-required mismatch on namespace-exported values.
- 3.3 Confirm whether this path is going through object structural subtype or short-circuit that bypasses required checks.

### Step 4 — Targeted fix + regression lock-in
- 4.1 Patch only the failing layer identified in steps 1–3.
- 4.2 Re-run:
  - `./scripts/conformance.sh run --error-code 2322 --filter assignmentCompatability` (all 33 tests)
  - `./scripts/conformance.sh analyze --error-code 2322`
- 4.3 Expand to neighboring regression families once fixed.

## 5) Success criteria

Per checkpoint, require:
- `assignmentCompatability*.ts` family: TS2322 miss count decreases before moving on.
- No regression in close-to-pass / quick-win TS2322 families outside the namespace bucket (or explicit justification).
- Any change remains on assignability gateway (`query_boundaries` + `assignability_checker`) with no new checker-side structural type-shape logic.

