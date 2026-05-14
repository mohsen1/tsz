# TSZ Roadmap

Date: 2026-05-14

Status: single living roadmap. Keep durable architecture contracts in
`docs/architecture/`, behavior specs in `docs/specs/`, product docs in
`docs/site/`, and current execution strategy here. Do not use this file for
routine PR status. Update it only for durable changes to public metrics, release
gates, sequencing, architecture direction, active priorities, or assumptions
future work would otherwise inherit incorrectly.

## North Star

tsz must become a real-project-compatible TypeScript compiler:

> Same project result as `tsc`, substantially faster when it succeeds, with
> clear failure categorization when it does not.

The immediate project risk is no longer raw feature count. tsz is weak under
advanced TypeScript composition: recursive conditionals, mapped/key-remapped
types, template literals, `infer`, indexed access, contextual generic
instantiation, flow narrowing, relation variance, and cross-file lib/module
identity. These are semantic-substrate problems, so the roadmap is organized
as campaigns instead of isolated conformance picks.

## Current Public Metrics

Source: `README.md` on 2026-05-14.

| Surface | Current |
| --- | ---: |
| Diagnostic conformance | `100.0%` rounded (`12,581 / 12,582`) |
| JavaScript emit | `94.8%` (`12,820 / 13,530`) |
| Declaration emit | `91.7%` (`1,531 / 1,669`) |
| Fourslash / language service | `99.9%` (`6,558 / 6,562`) |

Conformance remains a hard regression gate. It is no longer the sole readiness
signal. The primary readiness signal for this phase is whether tsz can
successfully check real projects that `tsc` accepts. Rounded percentages are
communication aids only; release planning uses exact numerators, denominators,
and failure-family counts.

## Evidence From Current Audit

This section is intentionally short and current. Replace it when a fresher audit
changes the picture.

1. Recent `main` history is fix-heavy: the last 1000 commits sampled on
   2026-05-14 contained roughly `590` fixes, `210` chores, `100+`
   performance/benchmark commits, and many more checker-scoped changes than
   solver-scoped changes. That is a signal that parity work is still often
   landing as local repair instead of substrate consolidation.
2. Recent bug language is also diagnostic: `alias`, `display`, `recursive`,
   `suppress`, `skip`, and `guard` appear often in commit subjects. Some are
   correct fixes, but the pattern says relation policy, display policy, and
   recursion/fuel policy still need central ownership.
3. A removed `Comparable<number>` rewrite showed the failure mode clearly:
   hardcoded user names, source-text scans, printer-output decisions, and a
   synthesized type can pass one fingerprint while making the compiler less
   real. Treat that as the cautionary example for new work.
4. Current open PR state is noisy: many branches are draft or WIP, and some
   ready PRs still carry `WIP` labels. A ready PR with a `WIP` label is still
   not mergeable.
5. Emit is a concentrated architecture risk, not cleanup tail work. Public
   metrics show `710` JavaScript emit failures and `138` declaration emit
   failures in `scripts/emit/emit-detail.json`; active work is heavily skewed
   toward narrow emit/DTS fixes and helper splitting.
6. Declaration emit currently performs too much late semantic discovery:
   `TypeData` matching, direct type evaluation during printing, usage walks over
   inferred `TypeId`s, and text-based import usage heuristics. The target is a
   precomputed declaration/public-API summary, not a shadow checker.

## Coordination Model

GitHub is the coordination surface.

1. Pick a stable `AgentName` and include it in every PR body and substantive PR
   comment.
2. Check open draft PRs and recent merged PRs for overlap before starting.
3. A GitHub issue is optional. A draft PR with a clear title/body is enough to
   claim active work.
4. Open a draft PR early, even if it is initially empty or docs-only. Use the
   draft PR body to record scope, invariants, risks, and verification.
5. Do not create claim documents under `docs/plan/claims`; that system has been
   removed.
6. Long-running branches must periodically merge `main` and fix conflicts in
   their own PRs.
7. Agents coordinate through PR comments, review comments, and PR descriptions.
   Address other agents by `AgentName` when coordination matters.
8. Never merge work that is still draft, labeled `WIP`, titled with `[WIP]`, or
   described as not ready.
9. Treat `ready` plus a `WIP` label as WIP. Remove the label before merge.
10. When ready, remove `WIP` labeling/title text, update the PR body with final
   scope and verification, mark ready, and let heavy CI run.
11. If a track is abandoned, close the draft PR with the reason and any useful
    findings.

Draft PR body shape:

```markdown
## Agent
AgentName: <stable-name>

## Track
<benchmark blocker | semantic campaign | emit/dts | refactor>

## Invariant
When <structural condition>, `tsc` <does X>; this PR makes tsz do X through
<owning layer>.

## Scope
- <files/systems expected to change>

## Verification
- <targeted local commands or CI gates>

## Coordination Notes
- <overlap, dependencies, follow-ups>
```

## Work Intake Rules

Every non-trivial PR declares exactly one type:

1. **Benchmark blocker**: names the project and before/after failure class.
2. **Semantic campaign**: names the invariant and owning layer.
3. **Emit/DTS parity**: names the baseline family and confirms no checker
   regression.
4. **Refactor only**: proves behavior unchanged and names the future campaign it
   enables.

For checker/solver fixes, the PR body must include:

1. Structural rule, not one-test symptom.
2. Owning layer: solver/query boundary/checker orchestration.
3. Adjacent-case matrix when behavior changes.
4. Cache-enabled/cache-disabled or order-independence plan when the bug touches
   generic instantiation, aliases, globals, or relation/evaluation caches.
5. Project-corpus smoke plan when the subsystem affects Kysely, Zod,
   ts-toolbelt, type-fest, ts-essentials, or large repo.

For emit/DTS fixes, the PR body must include:

1. Failure family: JS transform family or DTS nameability/portability/JSDoc/type
   display family.
2. Output layer: direct AST print, lowering directive, IR plan, declaration
   summary, or parser recovery fact.
3. Why the fix does not add semantic validation or late semantic discovery in
   emitter code.
4. Baseline-style verification plan; fragment `contains` tests are smoke tests,
   not proof of parity.

Symptom-patch freeze:

1. No new diagnostic decisions from file names, source text snippets, rendered
   type strings, or single conformance test names.
2. Existing `rewrite_*_fingerprints` and source-text/display-string decisions
   are finite migration debt. New work should remove one, route around one
   through a structural query, or explicitly list the temporary shortcut in the
   PR body with owner and removal condition.
3. Query-boundary modules may expose domain classifiers. They should not become
   broad re-export barrels for checker-local semantic traversal.

## Phase 0: Stabilize The Runway

Near-term priority order:

1. Merge or close current active PRs into coherent campaign ownership; remove
   stale `WIP` labels before any ready merge.
2. Freeze new symptom patches and start burning down existing fingerprint/source
   text/rendered-type rewrites.
3. Stop starting broad DTS cleanup unless it removes an emitter boundary
   violation, reduces ambient state, improves a release gate, or unblocks a
   named failure family.
4. Convert noisy planning state into draft PRs, PR comments, and this roadmap
   only when the update is durable enough to justify the shared-file conflict
   risk.
5. Build a red/yellow/green real-project dashboard.
6. Add reduced benchmark failures to targeted tests as they are understood.

## Phase 1: Project Corpus Gate

The benchmark dashboard must distinguish correctness from speed.

| Status | Meaning |
| --- | --- |
| Green | tsz and `tsc` both exit successfully with accepted diagnostic policy |
| Yellow | tsz exits but diagnostics differ |
| Red | tsz crashes, errors, OOMs, or times out |
| Gray | fixture or artifact is missing/incomplete |

Required project rows:

| Project | Current Strategic Read | Primary Owner Track | Exit Target |
| --- | --- | --- | --- |
| Kysely | likely contextual generics, guards, indexed access | Tracks 1, 3, 5 | exit success |
| Zod | recursive conditionals, object guards, class/generic identity | Tracks 1, 3, 6 | exit success |
| ts-toolbelt | recursive type evaluation pressure | Tracks 1, 2 | exit success |
| type-fest | broad mapped/conditional utility surface | Tracks 1, 2, 6 | exit success |
| ts-essentials | utility types plus recursive JSON shapes | Tracks 1, 2 | exit success |
| large-ts-repo | residency/runtime/project graph stress | Tracks 6, 7, 8 | exit success without OOM/timeout |
| Next.js full project | module graph plus generated app dependencies | Tracks 6, 7, 9 | recorded green/yellow/red |

For every project row, capture:

1. exit code,
2. timeout/OOM/crash/diagnostic mismatch,
3. diagnostic status and first 20 diagnostic deltas grouped by subsystem,
4. JavaScript emit status when emit is in scope,
5. declaration emit status when DTS is in scope,
6. known checker/solver/emit/DTS blockers,
7. peak memory if measured,
8. number of files reached if available,
9. last successful phase: parse, bind, check, emit.

Speed is a secondary column until the row is green or explicitly out of scope.
Do not present a faster red project as a win without also naming the remaining
correctness blocker.

## Architecture Health Metrics

Track these as counters or periodic audit bullets. They are more useful than
subjective "cleanup" language.

1. Number of checker `source_text.contains` / file-name / rendered-message
   diagnostic decisions.
2. Number of post-check `rewrite_*_fingerprints` passes still active.
3. Direct `is_assignable_to` call sites on `TS2322`/`TS2345`/`TS2416` paths
   that need both relation result and failure reason.
4. Checker modules consuming broad traversal primitives instead of
   domain-specific query-boundary classifiers.
5. Direct `TypeData` pattern matching outside solver/query-boundary internals.
6. Actual-lib alias admissions and allowlists that should become stable lib
   identity queries.
7. Emitter/DTS direct solver imports, direct type evaluation during printing,
   and `TypeData`/`lookup()` guardrail exceptions.
8. `Printer` and `DeclarationEmitter` ambient state fields, especially fields
   added for one transform or one baseline family.
9. Emitter/DTS tests that assert fragments instead of exact output or structured
   plan/summary facts.

## Ten Parallel Agent Tracks

These tracks are designed for 10 concurrent agents. Each track can own multiple
small PRs, but each PR should state one invariant and avoid duplicating another
track's active draft PR.

### Track 1: Type Evaluator Correctness

Scope: conditional types, mapped types, template literal types, `infer`,
distributivity, key remapping, indexed access, utility types, intrinsics, and
recursive evaluation.

Core invariant: semantic type evaluation has one solver-owned entrypoint with
memoization keyed by expression identity, substitution environment, compatibility
mode, and recursion/fuel state.

Acceptance:

1. Reduced failures from ts-toolbelt, type-fest, ts-essentials, Kysely, and Zod
   move into focused tests.
2. Deferred/unresolved conditionals are represented explicitly rather than
   erased to `any` or `error`.
3. Checker-local evaluation shortcuts trend down.

### Track 2: Instantiation, Inference, And Cache Hygiene

Scope: generic call inference, constructor inference, overload inference,
contextual typing, class/mixin instantiation, `this` substitution, stale aliases,
and relation/evaluation/inference cache keys.

Core invariant: cache keys include every input that can change the answer:
substitution environment, relation/variance mode, compatibility mode, lib/module
context, and relevant flow/request context.

Acceptance:

1. Cache-enabled and cache-disabled modes agree on targeted semantic tests.
2. Reordered declarations/files produce stable diagnostics.
3. Self-contradictory errors such as `T` not assignable to `T` are treated as
   cache/keying bugs until proven otherwise.

### Track 3: Flow Graph And Narrowing

Scope: discriminated unions, destructured discriminants, user-defined
predicates, `in` narrowing, optional/truthiness narrowing, array/object guards,
exhaustive switch behavior, and alias-aware flow facts.

Core invariant: checker supplies flow facts and locations; solver-owned
narrowing queries compute semantic narrowed types without leaking branch state.

Acceptance:

1. Kysely/Zod guard reductions pass.
2. Destructured discriminant and mapped-union `in` narrowing cases pass.
3. Nested narrowing cannot corrupt outer flow state.

### Track 4: Relation, Variance, And Call Signatures

Scope: assignability, function parameter variance, callable interfaces, overload
implementation compatibility, `call`/`apply`/`bind`, method bivariance
exceptions, abstract construct signatures, freshness/excess-property policy, and
weak type detection.

Core invariant: `TS2322`, `TS2345`, `TS2394`, `TS2416`, and related relation
paths flow through one assignability/relation gateway: relation -> structured
reason -> diagnostic rendering.

Acceptance:

1. Variance mode is explicit in relation context.
2. Bivariant and `any` propagation exceptions live in compatibility policy, not
   scattered call-site flags.
3. Callable interface assignment does not fall back to property comparison when
   `tsc` would compare signatures.
4. `TS2322`/`TS2345`/`TS2394`/`TS2416` paths that need relation plus failure
   reason use `RelationRequest`/`RelationOutcome` or a narrower wrapper, not raw
   boolean assignability followed by local semantic post-checks.
5. Display-string relation exceptions, iterator protocol special cases, and
   `keyof` post-checks move behind typed solver/query classifiers.

### Track 5: Query Boundaries And Checker Thinness

Scope: `query_boundaries`, checker orchestration, diagnostic source selection,
and any path where checker currently performs semantic shape analysis, source
text fingerprint rewriting, or rendered-type decision-making.

Core invariant: checker owns `WHERE`; solver owns `WHAT`. If checker needs to
branch on a type shape, add or use a solver/query-boundary classifier.

Acceptance:

1. New checker code does not match raw solver internals.
2. Central helpers cover repeated assignability/property/narrowing questions.
3. Diagnostic rendering stays downstream of semantic failure reasons.
4. The existing fingerprint-rewrite ledger trends down, and removed rewrites are
   replaced by structural solver/query behavior plus focused tests.
5. `query_boundaries/common.rs` shrinks toward explicit domain modules instead of
   exporting broad traversal internals for checker-side semantic recursion.

### Track 6: Symbol, Lib, Module, And Stable Identity

Scope: `import()` types, namespace/enum merging, module augmentations, DOM/lib
globals, symbol keys, global declarations, alias owners, `DefId` mapping, and
cross-file stable identity.

Core invariant: the same semantic entity has one identity across files/libs and
is referenced through stable binder/solver IDs, not recovered from syntax or
string names in hot paths.

Acceptance:

1. Lib selection and global scope construction are explicit and reproducible.
2. `import("./x").T` works inside conditional/keyof/Parameters/ReturnType-style
   contexts.
3. Unresolved identifiers do not silently become `any` unless `tsc` would do so.
4. Actual-lib alias admissions such as utility aliases and iterator/Intl rows
   are treated as transitional; stable lib identity queries replace allowlists.

### Track 7: Project Corpus And Benchmark Dashboard

Scope: project benchmark harness, public benchmark reporting, fixture status,
diagnostic-delta extraction, and reduction queue.

Core invariant: correctness status is reported separately from speed; no speed
headline is meaningful for a project until correctness status is green or
explicitly out of scope.

Acceptance:

1. Dashboard rows exist for Kysely, Zod, ts-toolbelt, type-fest, ts-essentials,
   large-ts-repo, and Next.js full.
2. Failed rows include exit class, first diagnostic deltas, and phase reached.
3. Benchmark reductions become owning-crate tests when root cause is known.

### Track 8: Residency, Performance, And Incremental Substrate

Scope: large-repo memory/runtime, stable skeleton indexes, bounded arena
residency, project graph reuse, compiler-service orchestration, and incremental
invalidations.

Core invariant: performance work must preserve semantic identity and correctness;
large-repo speed comes from stable semantic facts and bounded residency, not
from checker-local semantic shortcuts.

Acceptance:

1. Large repo finishes without OOM/timeout, then gets faster.
2. Cross-file lookups increasingly answer from skeleton/stable indexes.
3. Cache/residency changes include before/after measurements when practical.

### Track 9: Emit Robustness, DTS Boundary, And Consumers

Scope: JS emit, declaration emit, LSP, WASM, and compiler-service facade work.
Emit/DTS has enough failures and architectural risk to be its own recovery
campaign inside this track, not a bucket of baseline whack-a-mole.

Core invariant: emit, LSP, and WASM consume compiler outputs and semantic views;
they do not own type algorithms or rederive checker/solver facts.

Subtracks:

1. **JS transform plan graph**: lowering produces ordered per-file/per-node
   transform actions; complex transforms converge on IR or structured output
   plans instead of hidden `Printer` state.
2. **Declaration summary boundary**: binder/checker/solver produce a
   `DeclarationSummary`/`PublicApiSummary` with exported declarations,
   nameability, import dependencies, portability diagnostics, inferred
   declaration display requests, and JSDoc-derived facts.
3. **Emit failure-family dashboard**: JS/DTS pass counts, timeout count, and top
   families are visible; every emit PR names the family it moves.
4. **Emitter state and guardrail burn-down**: remove `TypeData`/`lookup()`
   guardrail exceptions, reduce broad ambient fields, and retire text-based
   import/type usage heuristics except for explicitly structured parser-recovery
   facts.

Acceptance:

1. JS emit fixes are tied to transform families and either reduce a family count
   or unblock a named transform-plan migration.
2. DTS fixes consume documented semantic summaries instead of broad reach-through
   into solver internals or fresh type evaluation during printing.
3. LSP/WASM paths converge on one compiler service front door and consume
   semantic views rather than matching raw `TypeData`.
4. No new broad `Printer` or `DeclarationEmitter` state fields are added without
   an owner, invariant, and removal/migration condition.
5. Source-text recovery moves toward parser-provided facts; emitter code should
   not infer malformed syntax behavior by scanning substrings.

### Track 10: Guardrails, Refactors, And Tooling

Scope: architecture guardrails, test fixtures, cache/order test harnesses, docs
cleanup, CI ergonomics, and behavior-preserving refactors that unblock tracks
1-9.

Core invariant: refactors reduce the number of semantic paths or make invariants
measurable. Cosmetic cleanup is filler work, not the main campaign.

Acceptance:

1. Guardrails catch forbidden checker/solver/emitter boundary drift.
2. Test harnesses make cache-disabled and order-randomized checks easy to run.
3. Docs stay concise and do not recreate claim-file bookkeeping.
4. Guardrails cover source-text/rendered-type semantic decisions and emitter
   direct solver-internal access once the current baselines have owners.
5. Refactor PRs that only split files are accepted when they reduce measurable
   state, remove a boundary exception, or unblock a named campaign.

## Local Verification Rules

1. Never run full conformance, emit, or fourslash suites locally.
2. Use `cargo nextest run` instead of `cargo test`.
3. Run local commands only when they answer a specific debugging question.
4. Wrap full-suite or memory-intensive commands with `scripts/safe-run.sh`.
5. Draft PR CI runs light gates; ready-for-review CI runs heavy gates.
6. Do not wait idle for CI. Push, record the run URL if useful, and move to
   non-overlapping work.

## Definition Of Done

This roadmap is succeeding when:

1. Diagnostic conformance reaches exact `12,582 / 12,582`; rounded `100.0%` is
   not treated as exact completion.
2. JavaScript emit reaches exact `13,530 / 13,530` and declaration emit reaches
   exact `1,669 / 1,669`, or the roadmap names the remaining blocked families.
3. Fourslash reaches exact `6,562 / 6,562`, or the roadmap names the remaining
   blocked cases.
4. Kysely and Zod become green project-corpus rows.
5. At least one advanced-types corpus among ts-toolbelt, type-fest, and
   ts-essentials is green.
6. large-ts-repo exits successfully without OOM/timeout.
7. Cache-enabled/cache-disabled diagnostics agree on targeted advanced-type and
   project-corpus checks.
8. `TS2322`/`TS2345`/property/call failures route through shared relation and
   query-boundary paths.
9. Checker-local type semantics and direct solver-internal pattern matching
   trend down.
10. Fingerprint/source-text/rendered-type rewrites trend down and no new ones are
   added without an explicit temporary-shortcut ledger entry.
11. Emit/DTS work reduces named failure families while moving toward
   `EmitPlan`/`DeclarationSummary` boundaries, not merely helper splitting.
12. Emit/DTS work no longer consumes the majority of active PR bandwidth while
   checker/solver benchmark blockers remain red.
13. GitHub draft PRs and comments are sufficient to understand active ownership;
   no claim-doc system reappears.
