# TSZ Roadmap

Date: 2026-06-10

Status: single living roadmap. Keep durable architecture contracts in
`docs/architecture/`, behavior specs in `docs/specs/`, and product docs in
`docs/site/`. Do not use this file for routine PR status; update it only when
a goal, gate, or metric durably changes.

## North Star

tsz must become a real-project-compatible TypeScript compiler:

> Same project result as `tsc`, substantially faster when it succeeds, with
> clear failure categorization when it does not.

## The Four Goals

All active work serves exactly one of these goals. Every PR names its goal in
the PR body (`Goal: green | fast | grow | hold`).

### 1. Green — compile every benchmark project correctly

Every required benchmark row exits with the same result as `tsc` under the
accepted diagnostic policy.

- Source of truth: `node scripts/bench/project-row-summary.mjs --markdown`
  plus completed benchmark/compile-guard artifacts. Stale artifacts are triage
  input, not status.
- Row states: Green (same result as `tsc`), Yellow (exits, diagnostics
  differ), Red (crash/error/OOM/timeout), Gray (fixture/artifact missing).
- Every red/yellow row names its first blocker: exit class, first diagnostic
  deltas grouped by subsystem, owning semantic operation, and phase reached
  (parse, bind, check, emit).

Required project rows:

| Project | Exit Target |
| --- | --- |
| utility-types | exit success |
| rxjs | exit success |
| Kysely | exit success |
| Zod | exit success |
| ts-toolbelt | exit success |
| type-fest | exit success |
| ts-essentials | exit success |
| generated Vite app | exit success |
| generated Next app | exit success |
| large-ts-repo | exit success without OOM/timeout |
| Next.js full project | recorded green/yellow/red when enabled |

`scripts/bench/test-project-rows.mjs` keeps this table in sync with the
benchmark row metadata; change both together.

### 2. Fast — beat tsgo on every green row

Speed is a goal only where correctness is already proven.

- Target: every eligible green timed row at least `2x` faster than `tsgo`;
  the canonical `*.tsgo-winners.json` artifact must show zero
  `two_x_target.target_gaps`.
- Performance PRs record: row or benchmark family, before/after command, wall
  time, peak RSS when residency changes, diagnostic status before/after, and
  the cache-key or semantic-identity invariant protected.
- A faster red row is not a win; name the remaining correctness blocker.
- Red rows whose first blocker is runtime/OOM/timeout/residency take
  performance work before they are green.

### 3. Grow — prove general readiness with more real projects

Add real-world projects to the corpus to show tsz approaches general use
while staying fast and accurate.

- A new row counts toward Grow only when it reaches Green, and stays counted
  only while it holds Fast once timed.
- `scripts/bench/project-rows.mjs` is the single row metadata source;
  `node scripts/bench/validate-project-metadata.mjs` must pass when rows
  change.
- Prefer projects that exercise new surface (frameworks, monorepos, codegen
  output, large graphs) over near-duplicates of existing rows.

### 4. Hold — never regress the parity floor

Conformance, emit, and language-service parity are regression gates, not
active campaigns.

- Diagnostic conformance: exact `12,585 / 12,585`.
- JavaScript emit: exact `13,530 / 13,530`. Declaration emit: exact
  `1,669 / 1,669`.
- Fourslash: exact `6,562 / 6,562` (confirmed via a full
  `scripts/fourslash/run-fourslash.sh` run at exact head `50b76b8`,
  2026-08-16; the previously tracked 4-test gap, including
  `importNameCodeFix_importType`, is resolved on `main`).
- `scripts/conformance/conformance-accepted-regressions.txt` stays empty or
  every entry carries fresh exact-head CI evidence. Currently `0` active
  entries (every line is a `#` comment; confirmed via
  `grep -cv '^#\|^\s*$' scripts/conformance/conformance-accepted-regressions.txt`
  and `python3 scripts/conformance/query-conformance.py --dashboard` →
  "Accepted-regression gate: 0 listed tests"); keep it that way and require
  fresh exact-head CI evidence for any new entry.
- Output-surgery audit stays at zero unallowlisted calls and zero allowlist
  entries.
- CheckerContext field-count guard is ratcheted at `255` fields after adding
  `type_position_deprecated_import_assert_files` (per-file cache for the
  TS2880 file-wide dynamic-import suppression fact; #16220). Future work
  should reduce this through capability extraction rather than silently
  adding checker-global state.

## How To Pick Work

1. Red or yellow required rows (Green) — the row's first blocker is the work
   item.
2. `two_x_target.target_gaps` entries (Fast).
3. Open `bug` / `false-positive` / `tech-debt` issues that block one of the
   four goals. Tech debt lives in issues, not in this file; an issue is worth
   doing when it names the goal it unblocks or the boundary counter it
   ratchets down.
4. New corpus candidates (Grow) once required rows are green.

Cluster issues by structural invariant rather than starting one branch per
issue. The reported repro is one witness, not the scope.

## Standing Rules

These survive any goal reshuffle:

1. **Parity over convenience.** If an observed behavior is a definite tsc
   bug, file it with the `TypeScript bug` label; do not patch away from
   parity.
2. **One invariant per PR.** State the structural rule: when <structural
   condition>, `tsc` does X; tsz does X through <owning layer>. Behavior
   fixes need adjacent cases: renamed binders, alias/wrapper/nesting, generic
   and concrete forms, positive and negative.
3. **Symptom-patch freeze.** No diagnostic decisions from file names, source
   text snippets, rendered type strings, or single test names. Existing
   fingerprint/source-text rewrites are finite migration debt: remove one,
   route around one, or ledger the shortcut with owner and removal condition.
4. **Owner layers.** Type semantics live in solver or a solver-backed query
   boundary; emit owns output, never semantic validation; relation failures
   route through the shared assignability gateway.
5. **Cache/order honesty.** Cache-enabled and cache-disabled runs agree;
   reordered declarations produce stable diagnostics; `T` not assignable to
   `T` is a cache/keying bug until proven otherwise.
6. **Local verification.** Local runs are the source of truth. CI's per-merge
   lane is `clippy` + `arch-size` only; conformance, emit, fourslash and unit
   run nightly and on `workflow_dispatch`. Run the suites a change can affect
   before pushing and record before/after in the PR body. Wrap heavy commands
   in `scripts/safe-run.sh`, and never run two `conformance.sh` invocations
   concurrently — it cleans the shared corpus tree on entry.

## Coordination

GitHub is the coordination surface. There are no ownership lanes, no manager,
and no reviewer role.

1. Every PR body carries `Goal:`, `## Verification`, and a `## Provenance`
   block (`Machine:`, `Assistant:`, `Model:`, `Effort:`). CI enforces this.
2. The PR author lands their own PR: when exact-head `CI Summary` passes,
   queue with `gh pr merge <pr> --match-head-commit <sha>`.
3. Land-and-continue: do not idle-wait on CI. Push, start the next task, and
   return to queue the merge when checks resolve.
4. Check open PRs and recent merges for overlap before starting. A PR
   with a clear body claims active work; drain owned PRs before unrelated new
   work.
5. Never merge WIP: draft state, `WIP` label, `[WIP]` title, or a body that
   says not ready. Adding WIP state requires a comment with reason, blocker,
   and next action.
6. Long-running branches periodically merge `main` in their own PRs.

## Plan Appendices

| Document | Role |
| --- | --- |
| `docs/plan/PERFORMANCE_PLAN.md` | Measurement and cache/residency review contract for Fast. |
| `docs/plan/LSP_ROADMAP.md` | LSP/WASM appendix; low-bandwidth until Green/Fast gates are met. |
| `docs/plan/SOUND_MODE.md` | Sound Mode appendix; not on the active critical path unless assigned. |

Appendices add durable contracts and detail; they do not promote their topic
into top-line work by themselves, and they must not accumulate dated run logs
or branch-local status.

## Definition Of Done

The roadmap is succeeding when:

1. Every required benchmark row is Green (Goal 1).
2. Every eligible green timed row shows zero `two_x_target.target_gaps`
   (Goal 2).
3. The corpus has grown beyond the required rows with new Green + Fast
   real-world projects (Goal 3).
4. Conformance, JS emit, DTS emit hold exact; fourslash reaches
   `6,562 / 6,562`; accepted regressions are empty or freshly evidenced;
   output-surgery audit stays clean (Goal 4).
