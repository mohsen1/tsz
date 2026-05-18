# TSZ Performance Plan

Status: durable performance engineering contract. The active execution roadmap
is `docs/plan/ROADMAP.md`, especially Phase 2 and Track 10. This file tells
agents how to design, measure, and review performance work so tsz can stay
`tsc`-correct while becoming consistently faster than `tsgo`.

Do not use this file as a run log. Keep dated raw output in PR bodies,
comments, CI artifacts, or local scratchpads. Update this file only when the
performance strategy, guardrails, measurement workflow, or subsystem playbooks
change durably.

## North Star

Performance work must preserve `tsc` parity. Large-project speed comes from
stable semantic identity, explicit request scopes, bounded residency, and
auditable caches. It must not come from checker-local shortcuts that bypass
solver semantics, source-text heuristics, stale cache answers, or skipped
diagnostics.

The target shape is:

1. every required project row compiles correctly,
2. green project rows are faster than `tsgo` in timing mode,
3. red runtime/OOM/timeout rows move toward green through bounded residency,
4. every hot path has a named complexity contract,
5. all caches have explicit keys, invalidation boundaries, memory accounting,
   and counter evidence,
6. tracing and attribution can explain why a row is slow without making timing
   claims from instrumentation-heavy runs.

Correctness is a performance feature: a faster red row is not a win unless the
first blocker is explicitly runtime, OOM, timeout, or residency.

## Audit Anchors From 2026-05-17

These are audit anchors, not permanent targets. Replace them when a fresher
audit materially changes the picture.

1. Latest public/local benchmark snapshot: 83 rows, 79 timed rows, 73 tsz wins,
   6 `tsgo` wins, and 4 error rows.
2. Current slower timed rows are concentrated in project-scale or semantic
   pressure cases: `ts-toolbelt-project` around 7.5x slower than `tsgo`,
   `vite-vanilla-ts-app` around 2.25x slower, `ts-essentials-project` around
   1.46x slower, `nextjs-fresh-app` around 1.36x slower, and BCT/class rows
   only slightly slower.
3. Active/recent nearby work already covers concrete slices:
   - #7796: flow all-path predicates should use memoized DP instead of
     branch-cloned visited vectors.
   - #7797: BCT pairwise subtype reduction can be skipped when a solver-owned
     structural proof says no member can reduce another.
   - #7804: `ts-toolbelt-project` is part of the default hotspot suite.
   - #7761: Vite remains a cross-file/lib-identity and child-checker residency
     problem, not merely a microbench problem.
4. Local architecture guard passed, but the guard does not yet enforce
   complexity budgets or counter coverage.
5. Static scans found measurable migration surfaces: many
   `with_parent_cache_attributed` call sites, many checker `TypeData::`
   matches, source-text/rendered-display decision sites, and remaining
   `visited.clone()` graph traversals.
6. Trace probe: the installed release binary had trace-level query logging
   statically disabled, while a debug binary emitted `tsz::query_json` start/end
   events. Trace evidence must therefore name the binary/profile used.
7. Counter probe: `TSZ_PERF_COUNTERS=1 --extendedDiagnostics` works on a tiny
   fixture and reports cross-arena delegation, checker construction,
   `compute_type_of_symbol`, interner, resolver, and by-reason child-checker
   attribution. Attribution mode is useful, but not comparable to `tsgo`
   timing.
8. A quick hotspot run in a reused worktree had to build a fresh `.target-bench`
   binary before it could measure anything. After rerunning with
   `TSC_NPM_SPEC=6.0.3`, tsz won 5 of 6 quick rows, but
   `ts-toolbelt-project` was `861.90ms` vs `99.30ms` (`tsgo` 8.68x faster).
   Build/cache hygiene and recursive type-evaluation pressure are both current
   performance priorities.

## Complexity Laws

These laws apply to checker, solver, binder, emitter, LSP, benchmark harnesses,
and CI scripts.

1. Default target complexity is O(N) or O(N log N) in the relevant input size:
   union members, flow nodes, declarations, project files, symbols, type
   arguments, signatures, properties, or dependency edges.
2. Any O(N^2) path must be explicitly admitted in code comments and PR body. It
   needs at least one of:
   - a small hard cap with a `tsc`-compatible fallback,
   - a structural proof that skips the loop,
   - partitioning that makes the common case subquadratic,
   - memoization keyed by every semantic input,
   - a counter proving the path is not reached on required project rows.
3. O(N^3), exponential, factorial, Cartesian-product, recursive distribution,
   or path-cloning work is forbidden unless it is behind a correctness-required
   cap and has a named follow-up to replace it.
4. Branch-local `visited.clone()` graph traversal is forbidden for all-path or
   any-path predicates unless the graph size is statically tiny. Use node-keyed
   memoized DP, worklists, SCCs, or bitsets.
5. Relation and evaluation loops must use identity checks, cheap leaf exits,
   and structural no-op proofs before constructing expensive cache keys or
   entering subtype/evaluation work.
6. If a bailout preserves correctness but keeps extra union/intersection
   members, that is acceptable only when diagnostics remain `tsc`-compatible
   or the diagnostic rendering path knows how to match `tsc`.
7. A cache is not a shortcut. A cache is valid only when its key includes every
   semantic mode that can change the answer and its invalidation boundary is
   explicit.
8. Performance counters and tracing must not become policy. They expose
   evidence; architecture and solver/checker contracts decide behavior.

## Measurement Modes

Always state which mode produced the evidence.

| Mode | Purpose | Counter state | Trace state | Comparable to `tsgo` timing? |
| --- | --- | --- | --- | --- |
| `timing` | wall time and RSS claims | off | off | yes |
| `attribution` | explain where time goes | on | usually off | no |
| `trace` | inspect query or subsystem sequence | optional | on | no |
| `scale-cliff` | detect superlinear ratios | counters on or JSON metrics | optional | only if run as timing |

Rules:

1. Never compare attribution-mode tsz to timing-mode `tsgo`.
2. If trace-level logging is requested, use a debug/dev or trace-enabled binary
   and record the binary/profile. Release binaries may compile trace events out.
3. Use `TSZ_LOG=tsz::query_json=trace TSZ_LOG_FORMAT=json` for solver query
   entry traces. Add more targeted spans only after counters identify an opaque
   hot path.
4. Use `TSZ_PERF_COUNTERS=1 --extendedDiagnostics` for human-readable
   attribution. Use `--perf-counters-json` only with a `perf-tools` build and
   analyze it with `scripts/perf/query-perf-counters.py`.
5. Use `scripts/safe-run.sh` for memory-intensive or long-running commands.
6. Do not run full conformance, full emit, or full fourslash locally.

## Required Evidence For Performance PRs

Every performance-motivated PR must record:

1. project row or benchmark family,
2. before/after command,
3. timing mode, attribution mode, or trace mode,
4. wall time when timing is claimed,
5. peak RSS or physical footprint when residency changes,
6. diagnostic status before and after,
7. cache/counter deltas when the change is counter-driven,
8. semantic identity, cache-key, invalidation, or complexity invariant,
9. known noise sources,
10. why the evidence covers the changed hot path rather than only the reported
    fixture spelling.

For any O(N^2)-admitted path, additionally record:

1. input size N and what N means,
2. cap or bailout threshold,
3. fallback behavior and correctness argument,
4. counter or test proving the cap/fallback path is exercised,
5. adjacent shape that would catch a spelling-only fast path.

## Tooling Workflow

Use the narrowest tool that answers the question.

1. Project correctness: `scripts/ci/project-compile-guard.sh` with
   `TSZ_PROJECT_COMPILE_FILTER='<row-regex>'`.
2. Public benchmark row timing: `scripts/bench/bench-vs-tsgo.sh --filter
   '<fixture>'`.
3. Hot family timing: `scripts/bench/perf-hotspots.sh --quick` during
   iteration, full hotspot filter only for final evidence.
4. Scale cliffs: `scripts/bench/scale-cliff/run-cliff.sh` after fixtures are
   generated; inspect per-file ratios for checker constructions, overlay
   entries, delegations, and `compute_type_of_symbol`.
5. Counter JSON analysis: `scripts/perf/query-perf-counters.py --json
   <artifact>`.
6. Boundary drift: `python3 scripts/arch/arch_guard.py`.
7. Static complexity scan: search for nested loops, `visited.clone()`, direct
   checker `TypeData::` matching, source-text decisions, rendered-display
   decisions, and new unmeasured caches.

Recommended audit loop for broad performance work:

1. Read `docs/plan/ROADMAP.md` Phase 2 and Track 10.
2. Inspect open and recently merged PRs for overlap.
3. Pick one row or family.
4. Establish correctness status before timing.
5. Run timing mode once to locate the gap.
6. Run attribution mode to find the subsystem.
7. Run trace mode only where attribution is too coarse.
8. Inspect code for complexity class and cache ownership.
9. State the structural invariant.
10. Implement or document the durable fix.
11. Verify with one reduced test and one row/family command.

## Hot Path Playbooks

### Recursive Type Evaluation And `ts-toolbelt`

Owner tracks: 2 and 3.

Problem shape: recursive conditionals, mapped/indexed access, key remapping,
template literals, `infer`, and repeated generic instantiation can revisit the
same semantic question under slightly different syntax wrappers. This is the
current `ts-toolbelt-project` class of risk.

Required direction:

1. Represent deferred operations explicitly; do not erase to `any`, `unknown`,
   or `error` to make recursion terminate.
2. Make normalization purpose-specific: relation input, property lookup,
   inference source/target, diagnostic display, or flow narrowing.
3. Cache by semantic operation plus purpose, substitution environment,
   compiler mode, resolver/lib context, fresh-literal state, `this` type, and
   recursion/fuel state.
4. Detect recursion through stable operation frames, not rendered type strings
   or syntax file names.
5. Preserve cheap leaf paths before cache-key construction.
6. Add scale tests that vary binder names and wrapper shape.

Big changes that are allowed:

1. a solver-owned evaluation DAG with request IDs and stable operation keys,
2. explicit lazy/deferred nodes for unresolved conditionals and mapped
   projections,
3. cache-disabled and cache-enabled differential tests for advanced type
   evaluation,
4. per-operation fuel accounting with structured partial-result reasons.

### Cross-File Residency, Lib Identity, And Generated Apps

Owner tracks: 7 and 10.

Problem shape: Vite, Next, RxJS, and large project rows pay for child checkers,
cross-arena delegation, repeated lib interface lowering, module resolution, and
overlay copying. The goal is not to make child checkers cheaper forever; it is
to answer more requests from stable project facts.

Required direction:

1. Stable project skeletons own file identity, declaration locations, exports,
   imports, and lib/global topology.
2. File sessions reuse long-lived project facts and reset file-local state at
   file boundaries.
3. Cross-file lookups should answer from skeleton/stable indexes first, then
   typed query caches, then full child checker fallback.
4. `with_parent_cache` and `copy_symbol_file_targets_to` are migration counters,
   not acceptable long-term architecture.
5. Every new cross-file shortcut must prove lib/interface identity and
   type-parameter preservation. Missing-interface probes must not become
   name-only allowlists.
6. Module resolution caches must report lookup, file/dir stat, package.json,
   and candidate-path counts.

Big changes that are allowed:

1. a project service front door shared by CLI, LSP, WASM, and benchmarks,
2. stable declaration summaries for cross-file type queries,
3. immutable lib/interface summaries keyed by lib set and compiler options,
4. replacing child-checker delegation with typed query handles.

### Pairwise Relations, BCT, Union, And Intersection Reduction

Owner tracks: 2, 3, 4, and 5.

Problem shape: subtype reduction, best common type, union/intersection
simplification, signature comparison, property comparison, and tuple/rest
comparison can all become pairwise relation storms.

Required direction:

1. No pairwise relation loop may be entered without cheap identity and leaf
   filters.
2. For N greater than the documented small cap, use partitioning, proof of
   no-op, cache lookup, or bailout.
3. Pairwise loops that use `SubtypeChecker` must state which relation mode,
   compatibility policy, resolver identity, and compiler flags affect the
   answer.
4. Repeated list reductions should use sorted-`TypeId` or stable list IDs, not
   ad hoc `Vec<TypeId>` identity.
5. If a loop is skipped because it is only an optimization, tests must prove
   diagnostics stay stable with unreduced members.

Big changes that are allowed:

1. relation request batching,
2. union/intersection partition indexes by discriminant/key-space facts,
3. solver-owned key-space summaries reused by relation/property/indexed-access
   paths,
4. relation result caches keyed by relation mode and structured failure reason.

### Flow Graph And Narrowing

Owner track: 6.

Problem shape: all-path/any-path flow predicates, `typeof` exclusion checks,
nullish exclusion chains, aliased guards, and loop back-edges can become
exponential when each branch clones path-local visited state.

Required direction:

1. Use node-keyed memoized DP for graph predicates.
2. Use worklists or SCCs for cyclic graphs; back-edge sentinels must be
   conservative and documented.
3. Cache keys include target reference, input type, predicate payload, compiler
   flags, and resolver generation when semantics can change.
4. Checker owns flow locations and facts; solver owns semantic narrowing
   predicates.
5. Flow caches must not store speculative answers that can leak across branch
   or file sessions.

Big changes that are allowed:

1. a solver-owned predicate application cache,
2. compact flow fact bitsets for common primitive/nullish predicates,
3. order-randomized flow tests to catch accidental traversal-order semantics.

### Property, Key-Space, And Indexed Access

Owner track: 5.

Problem shape: repeated property lookup over unions/intersections, template
literal keys, numeric/string key compatibility, index signatures, and excess
property checks can multiply relation and string-map work.

Required direction:

1. Model property identity as solver-owned key-space facts, not ad hoc strings.
2. Reuse the same key-space summaries for `keyof`, `T[K]`, mapped projection,
   excess-property classification, relation property comparison, and
   diagnostics.
3. Avoid owned `String` maps in hot paths when atoms, symbols, or key-space
   handles are available.
4. Cache property lookup by object identity, key-space identity, lookup mode,
   optional/readonly policy, and compiler flags.
5. Template literal pattern keys need structural classification and caps on
   expansion.

Big changes that are allowed:

1. interned key-space handles,
2. property summary indexes per object/union/intersection,
3. typed diagnostic classifiers that consume key-space failure reasons.

### Template Literal And Distributive Explosion

Owner track: 2.

Problem shape: template literal expansion and distributive conditionals can
create Cartesian products or repeated nested expansions.

Required direction:

1. Cap expansion by product size and report a structured "too complex" reason.
2. Keep symbolic/deferred representations when expansion is not needed for the
   caller's purpose.
3. Cache extraction of literal string sets and template interpolation positions.
4. Never drive semantic decisions from printed template displays.
5. Test renamed type parameters and alias/wrapper variants.

Big changes that are allowed:

1. symbolic template automata for pattern compatibility,
2. lazy product iterators that can short-circuit relation/property queries,
3. per-purpose template normalization.

## Cache Contract

Every cache added or modified must document:

1. owner (`QueryCache`, solver interner, project service, file session, or
   request transaction),
2. key fields,
3. inputs intentionally excluded from the key and why they cannot change the
   answer,
4. invalidation boundary,
5. memory accounting,
6. hit/miss or size counters,
7. recursion/fuel behavior,
8. behavior when the cache is disabled or absent,
9. test that proves two different semantic modes do not alias.

Do not store substitution environments on `TypeInterner`. Do not compare
`TypeId`s across distinct `TypeInterner`s in tests. Do not cache a
depth-exceeded, fuel-exhausted, or speculative partial result unless the
partial-result kind is part of the key and caller contract.

## Residency Contract

The target shape is bounded file-session reuse:

1. long-lived project facts and caches are shared,
2. file-local state resets at file boundaries,
3. speculative/request state is transaction-scoped,
4. full AST/binder residency is a fallback rather than the default answer path,
5. lib/interface facts are immutable and keyed by lib set and compiler options,
6. source text is loaded only for syntax traversal, diagnostics, or emit that
   truly needs it.

Residency work must prove diagnostic stability. Constructor-count reductions
are useful, but green project-corpus rows and stable diagnostics are stronger
evidence.

## CI And Build Cache Discipline

Performance includes developer and CI latency.

1. Docs-only, website-only, benchmark-script-only, and shell-only PRs should not
   run compiler-heavy CI unless they touch a compiler contract.
2. Draft PR CI should stay light. Ready-for-review CI owns heavy conformance,
   emit, fourslash, WASM, and project gates.
3. Unit CI must restore or reuse build artifacts where safe. Recompiling the
   dependency graph on every run is a CI performance bug.
4. Benchmark workflows must prove they target current `main` before reserving
   self-hosted runners.
5. Stale benchmark runs for obsolete SHAs should skip in a cheap gate.
6. Bench prep artifacts should be keyed by SHA so shards do not rebuild or
   recompile dependencies.
7. If a workflow claims "shell-only" or "docs-only", the required checks should
   prove that path directly rather than relying on a full workspace unit run.

## Guardrails To Add Or Strengthen

The current architecture guard is necessary but not enough. Add focused guards
over time:

1. forbid new branch-local `visited.clone()` graph predicates without an
   allowlist entry,
2. require comments for new nested relation/evaluation loops over type/member
   lists,
3. report checker `TypeData::` matches outside query-boundary internals,
4. report source-text and rendered-display semantic decisions,
5. report `with_parent_cache` and symbol-file-target copy call-site counts,
6. report caches without statistics or size accounting,
7. report `println!`, `eprintln!`, and `dbg!` in compiler internals,
8. make scale-cliff ratios easy to compare in CI artifacts.

Guardrails should fail only when the invariant is unambiguous. Otherwise they
should produce an audit report that a PR body can cite.

## Merge Discipline

Performance work is only useful after it lands on `main`. Each PR should stay
small enough to review and merge quickly.

Before opening or reviving a performance branch:

1. fetch `origin/main`,
2. inspect open and recently merged PRs for overlap,
3. rebase or merge onto current `main`,
4. choose one row, family, or invariant,
5. record the exact verification command that protects the invariant.

After opening a PR:

1. keep the branch synchronized with `main`,
2. treat failing required checks as the next task,
3. avoid stacking broad performance PRs behind unmerged ready PRs,
4. delete or close duplicate branches,
5. keep the PR body current with measurements and known gaps.

## Project Corpus Contract

Use `scripts/bench/project-rows.mjs` as the source of truth for required and
canary rows. The performance plan should not hand-maintain a stale project
table.

For every project row, record:

1. exit class,
2. phase reached,
3. last successful phase,
4. diagnostic status,
5. first diagnostic deltas grouped by subsystem,
6. known blockers,
7. exit codes,
8. files reached,
9. peak memory,
10. emit and DTS status when relevant.

Speed is secondary until the row is green or the first blocker is
runtime/residency.

## Definition Of Done For This Plan

This plan is working when:

1. no new unbounded O(N^2) semantic path lands without cap, proof, partition,
   memoization, or explicit bailout,
2. scale-cliff ratios stay roughly linear for required large-project fixtures,
3. `ts-toolbelt-project`, generated Vite, generated Next, and
   `ts-essentials-project` have current attribution artifacts naming their
   dominant subsystem,
4. cache-enabled and cache-disabled targeted tests agree for advanced type
   evaluation and relation hot paths,
5. `with_parent_cache`, overlay-copy, direct checker `TypeData::`, source-text
   decision, and rendered-display decision counts trend down,
6. docs/shell/website-only PRs do not burn full compiler CI,
7. green project rows are measured in timing mode and trend faster than `tsgo`,
8. performance PRs preserve `tsc` diagnostics and state the structural rule
   being protected.
