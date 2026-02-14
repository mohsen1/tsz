# TSZ Architecture Migration Plan

Status: Draft v1  
Owner: Compiler Team  
Primary Goal: Match `tsc` behavior exactly while keeping TSZ checker and solver architecture simple, explicit, and maintainable.

## 1. Outcomes

1. TSZ matches `tsc` diagnostics and type results across targeted suites.
2. Checker becomes a thin orchestration layer.
3. Solver becomes the only home for type computation.
4. Compatibility quirks are isolated in explicit Lawyer modules, not spread through Checker code.
5. Architecture guardrails are enforced in CI.

## 2. Non-Goals

1. Introducing behavior that is better than `tsc` but not identical.
2. Large-scale syntax or binder redesign unrelated to parity and architecture boundaries.
3. Optimizations that reduce correctness confidence.

## 3. Guardrails (Must Hold Throughout Migration)

1. No checker-side type algorithm implementations.
2. No checker direct matching on low-level type internals when solver queries can answer.
3. All semantic references represented via `Lazy(DefId)` on the solver graph.
4. All new parity fixes come with differential tests against `tsc`.
5. Every phase has an explicit rollback path.

## 4. Program Structure

Migration is executed in 12 phases.  
Each phase ships in small, reviewable PRs behind strict parity and performance gates.

## 5. Workstreams

1. WS-A: Solver Core (Judge, normalization, evaluation, relations).
2. WS-B: Compatibility Layer (Lawyer quirks and policy).
3. WS-C: Checker Decomposition (intent extraction, query orchestration, diagnostics mapping).
4. WS-D: DefId Resolution and environment guarantees.
5. WS-E: Test and Parity Infrastructure.
6. WS-F: Performance and memory regression control.
7. WS-G: Architecture enforcement and code health.

## 6. Phase Plan

## Phase 0: Baseline, Inventory, and Risk Map

Objective: establish measurable starting state and migration safety rails.

Steps:
1. Capture current parity baseline:
   1. total pass rate,
   2. top failing categories,
   3. highest-churn failure files.
2. Inventory checker locations where type internals are matched directly.
3. Inventory duplicated relation logic across checker and solver.
4. Build risk register for fragile areas:
   1. conditional types,
   2. overload resolution,
   3. contextual typing,
   4. flow narrowing,
   5. object literal freshness.
5. Define architecture lint checks that will be introduced later.

Exit Criteria:
1. Baseline parity report checked in as migration reference.
2. Initial architecture debt map with file-level ownership.
3. Priority list of first 20 migration targets.

Deliverables:
1. `docs/` report for parity baseline.
2. architecture debt inventory.
3. risk register with severity and owner.

---

## Phase 1: Define Stable Checker <-> Solver Query Boundary

Objective: convert ad-hoc calls into explicit query contracts.

Steps:
1. Define query types for all high-frequency checker asks:
   1. assignability,
   2. subtype,
   3. identity,
   4. call applicability,
   5. index/keyof access,
   6. narrowing requests.
2. Introduce request/response structs with reason payload support.
3. Replace direct helper calls in checker hot paths with query facade.
4. Keep behavior identical by preserving current semantics underneath the facade.
5. Add query-level tracing IDs for later differential debugging.

Exit Criteria:
1. Checker uses query facade in top-level expression and assignment paths.
2. No parity regressions from boundary introduction.
3. Query API reviewed and frozen for next phases.

Deliverables:
1. query contract document.
2. facade module and migration notes.

---

## Phase 2: Solver Normalization Pipeline

Objective: reduce branch complexity in relation/evaluation algorithms.

Steps:
1. Add normalization passes:
   1. resolve `Lazy(DefId)` when required,
   2. flatten union/intersection shape,
   3. remove redundant wrappers,
   4. canonicalize member ordering where legal.
2. Memoize normalized forms with recursion guards.
3. Ensure normalization is observationally equivalent to current behavior.
4. Route relation checks through normalization entrypoints.
5. Add targeted tests for normalization idempotence.

Exit Criteria:
1. Key relation functions consume normalized inputs.
2. Reduction in relation branch count in critical modules.
3. No change to diagnostic text or codes.

Deliverables:
1. normalization design note.
2. normalization performance snapshot.

---

## Phase 3: Judge Layer Consolidation

Objective: create one strict structural engine without TypeScript quirks mixed in.

Steps:
1. Isolate strict subtype/identity rules into Judge modules.
2. Move duplicated structural logic out of checker compatibility paths.
3. Add coinductive cycle handling and depth/fuel controls in one place.
4. Guarantee deterministic ordering for shape/member comparison.
5. Add focused Judge-only tests independent from TS compatibility quirks.

Exit Criteria:
1. Judge is the sole structural relation implementation.
2. Checker and Lawyer stop duplicating strict checks.
3. No performance regression beyond agreed budget.

Deliverables:
1. Judge module map.
2. strict relation conformance tests.

---

## Phase 4: Lawyer Layer Formalization

Objective: make `tsc` quirks explicit, configurable, and auditable.

Steps:
1. Introduce `CompatProfile` covering:
   1. any suppression policy,
   2. function variance mode,
   3. void-return exception,
   4. excess property checks,
   5. weak type detection.
2. Move current compatibility behavior into named policy modules.
3. Ensure default profile is exactly `tsc` parity mode.
4. Add per-policy fixtures for regression safety.
5. Add trace output that records which policy caused acceptance/rejection.

Exit Criteria:
1. Compatibility behavior exists only in Lawyer modules.
2. Default profile reproduces previous runtime behavior.
3. Every policy has direct tests and docs.

Deliverables:
1. `CompatProfile` contract.
2. policy matrix and example cases.

---

## Phase 5: DefId-First Resolution Completion

Objective: eliminate fragile symbol-backed shortcuts in type references.

Steps:
1. Audit all semantic type references for non-`Lazy(DefId)` usage.
2. Replace direct symbol links with canonical lazy references.
3. Harden `TypeEnvironment` guarantees for relation/evaluation entrypoints.
4. Add preflight assertions in checker before deep relation checks.
5. Add regression tests for recursive and cross-file reference cycles.

Exit Criteria:
1. All semantic references flow through `DefId`.
2. Missing mapping failures are caught at preflight points.
3. Cross-module recursion behavior remains stable.

Deliverables:
1. DefId migration checklist.
2. environment guarantee spec.

---

## Phase 6: Checker Decomposition by Intent

Objective: remove "god function" patterns and enforce thin orchestration.

Steps:
1. Split checker into intent modules:
   1. assignment intent,
   2. call/new intent,
   3. control-flow dependent intent,
   4. declaration intent,
   5. type expression intent.
2. Move type-decision code to solver queries where found.
3. Keep checker responsibilities:
   1. AST extraction,
   2. symbol lookup orchestration,
   3. diagnostic location mapping.
4. Enforce per-file size and complexity ceilings.
5. Add architecture tests to block new direct type-internal matches in checker.

Exit Criteria:
1. Checker files are under configured size limits.
2. Zero new checker-side TypeKey branching violations.
3. Migration of highest-complexity checker paths completed.

Deliverables:
1. checker decomposition map.
2. complexity trend dashboard.

---

## Phase 7: Diagnostic Reason Pipeline

Objective: separate failure reasoning from diagnostic rendering.

Steps:
1. Solver returns structured relation failure reasons.
2. Checker translates reasons into exact `tsc` diagnostic codes/messages/spans.
3. Add fallback reason for unknown internal failure states.
4. Snapshot-test diagnostics against `tsc` for unstable categories.
5. Add telemetry for top reason categories to guide next parity work.

Exit Criteria:
1. Critical relation paths use structured reasons, not bool-only failures.
2. Diagnostic rendering remains parity-accurate.
3. Reason coverage metrics available in CI logs.

Deliverables:
1. reason taxonomy document.
2. diagnostic mapping table.

---

## Phase 8: Flow and Narrowing Unification

Objective: make narrowing solver-owned while checker provides flow context only.

Steps:
1. Define narrowing query contracts with flow facts as input.
2. Move narrowing algorithms to solver modules.
3. Keep checker responsible for selecting applicable flow nodes.
4. Validate tricky control-flow cases:
   1. switch discriminants,
   2. assignment invalidation,
   3. call effects,
   4. loops and join points.
5. Add parity-focused suites for control-flow-heavy tests.

Exit Criteria:
1. Narrowing logic consolidated under solver.
2. Control-flow regressions remain within agreed threshold.
3. Perf remains stable on flow-heavy benchmarks.

Deliverables:
1. narrowing API docs.
2. flow-parity test report.

---

## Phase 9: Performance and Memory Hardening

Objective: preserve speed and memory while architecture becomes cleaner.

Steps:
1. Establish performance budgets:
   1. parse,
   2. bind,
   3. check,
   4. incremental check.
2. Add microbenchmarks for high-frequency solver queries.
3. Profile normalization and compatibility overhead.
4. Introduce caching where profitable and semantically safe.
5. Track memory for arenas and interner growth across representative corpora.

Exit Criteria:
1. No P95 regression beyond agreed budget.
2. No sustained memory growth anomalies.
3. Query cache hit rates meet target ranges.

Deliverables:
1. benchmark baseline and trend report.
2. memory profile snapshots.

---

## Phase 10: Architecture Guardrails in CI

Objective: prevent architecture backsliding after migration.

Steps:
1. Add CI checks for forbidden patterns:
   1. checker direct type-internal matching,
   2. solver bypass helpers,
   3. binder invoking solver type algorithms.
2. Add file complexity and size thresholds for checker modules.
3. Require parity evidence for any relation or inference change.
4. Require architecture annotation in PR templates for type-system changes.
5. Block merges on violated guardrails.

Exit Criteria:
1. Guardrail checks are mandatory and stable.
2. PR flow includes parity and architecture attestations.
3. No new violations for 4 consecutive weeks.

Deliverables:
1. CI rules and docs.
2. PR policy update.

---

## Phase 11: Parity Closure and Stabilization

Objective: close remaining parity gaps and declare migration complete.

Steps:
1. Burn down top parity deltas by impact tier.
2. Run full differential suites repeatedly across recent commits.
3. Triage flakes and nondeterminism.
4. Freeze compatibility behavior and annotate known intentional deviations, if any.
5. Publish final architecture and parity report.

Exit Criteria:
1. Target parity threshold achieved and stable.
2. Known-deviation list is explicit and approved.
3. Architecture contract enforced with zero critical exceptions.

Deliverables:
1. final parity report.
2. migration completion RFC.

---

## Phase 12: Post-Migration Operating Model

Objective: keep the system elegant under ongoing feature work.

Steps:
1. Add "owner per solver area" map.
2. Schedule monthly architecture drift audits.
3. Keep a rolling parity dashboard and top regressions list.
4. Require design notes for new TypeScript feature support.
5. Maintain a strict "query first, checker thin" onboarding guide.

Exit Criteria:
1. Sustainable ownership and review model is active.
2. Drift audits running on schedule.
3. New feature PRs consistently follow architecture contract.

Deliverables:
1. ownership matrix.
2. ongoing governance checklist.

## 7. Cross-Phase Milestones

1. M1: Query boundary live on core checker paths.
2. M2: Judge/Lawyer split complete for assignability and subtype.
3. M3: DefId-first semantics fully enforced.
4. M4: Checker decomposition complete for top-complexity files.
5. M5: Guardrails blocking architecture regressions in CI.
6. M6: Parity closure target reached.

## 8. Suggested PR Sizing

1. Keep most migration PRs under 800 changed lines.
2. Prefer one architectural move per PR.
3. Pair behavior changes with parity fixtures in the same PR.
4. Separate refactor-only and behavior-changing changes.

## 9. Test Strategy by Phase

1. Unit tests for solver internals (Judge, Lawyer, normalization, evaluate, narrow).
2. Differential tests against `tsc` for behavioral parity.
3. End-to-end conformance suites for regression confidence.
4. Benchmark gates for throughput and memory.
5. Architecture lint checks for layering rules.

## 10. Risks and Mitigations

1. Risk: hidden parity regressions during refactor.
   Mitigation: mandatory differential tests per PR plus canary suites.
2. Risk: performance drop from normalization or policy layering.
   Mitigation: query caching, profiling, and strict perf budgets.
3. Risk: architecture drift under delivery pressure.
   Mitigation: CI guardrails and required architecture attestations.
4. Risk: incomplete DefId mapping in edge recursion cases.
   Mitigation: preflight mapping checks and recursive stress fixtures.
5. Risk: diagnostic mismatches from reason pipeline change.
   Mitigation: exact code/message/span snapshot comparisons vs `tsc`.

## 11. Completion Definition

Migration is complete when all conditions hold:

1. `tsc` parity target met on agreed suite and stable for consecutive runs.
2. Checker files respect complexity/size rules and remain orchestration-only.
3. Solver owns type computations with Judge/Lawyer separation.
4. DefId-first lazy resolution is canonical and enforced.
5. CI prevents forbidden architecture shortcuts.
6. Performance and memory remain inside agreed envelopes.

## 12. Immediate Next 30-Day Plan

1. Week 1:
   1. finish Phase 0 inventory and baseline capture,
   2. draft query contracts.
2. Week 2:
   1. land Phase 1 facade for assignments and core expressions,
   2. begin normalization prototype.
3. Week 3:
   1. split Judge and Lawyer for assignability path,
   2. add first compatibility profile tests.
4. Week 4:
   1. migrate first high-complexity checker file to intent/query style,
   2. enable initial architecture CI checks in warning mode.

## 13. Decision Log Template (Use Per Architectural Change)

Use this template in each migration PR:

```md
### Architecture Change Summary
- Scope:
- Why now:
- Checker responsibilities changed:
- Solver responsibilities changed:
- Compatibility policy affected:
- DefId / TypeEnvironment implications:
- Parity evidence (tests and deltas):
- Perf evidence:
- Rollback plan:
```

## 14. Practical Principle for Day-to-Day Work

If a change computes `WHAT` a type means or how two types relate, it belongs in Solver.  
If a change computes `WHERE` to report or which AST node triggered it, it belongs in Checker.
