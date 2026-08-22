---
name: tsz-architecture
description: Plan, implement, or review TSZ compiler architecture involving capability/nonclaim ownership, completion propagation, forcing and recursion identity, semantic caches, phase boundaries, or central modules nearing the rewrite limits.
---

# TSZ Architecture

Use before changing capability policy, program-wide containment, semantic
forcing/recursion, caches, checker passes, or a phase boundary.

## Start

Read `AGENTS.md`, `docs/plan/ROADMAP.md`, and
`docs/architecture/RESET.md`. Run:

```bash
python3 scripts/arch/rewrite_architecture_metrics.py --check
python3 scripts/arch/arch_guard.py
```

State the structural TypeScript rule and draw the decision path from authored
syntax to semantic query, product plan, service response, and process result.
Name one owner for every decision.

## Capability And Completion

- AST and scanner data record authored facts and recovery, not product policy.
- Prefer typed event/feature records to adding another top-level `SourceUnit`
  boolean, even for an authored fact.
- Derive capability/nonclaim decisions once per program/options snapshot in
  immutable typed analysis.
- Key each claim by operation/product plus program/file/node scope. Every
  nonclaim carries a structural reason and deletion condition.
- Checker, EmitPlan, public `emit_file`, printer fallbacks, quick info,
  navigation services, and exit selection reuse the same analysis. A service
  does not reparse or recompile merely to rediscover capability policy.
- Do not add paired `has_authored_*`/`*_supported` booleans or a one-off service
  guard. Consolidate an existing decision instead.
- Defer the smallest incomplete query or owner. Do not skip the whole checker
  for a file-local gap.
- A program-wide skip requires evidence that unrelated files cannot be checked
  definitively.
- Every temporary nonclaim has a typed reason, a public fallback test, and a
  deletion condition recorded in the PR body.

## Evaluation And Identity

- One checker session owns the canonical forcing and recursion identity/key schema.
- Demand-scoped relation source/target frames and typed budget axes are valid;
  they must not mint a fresh identity universe or reset shared semantic work.
- Required-type, relation, projection, and display are demands on that session,
  not independent recursion universes or eager subtree prewalks.
- Keep traversal depth, expansion depth, and evaluator work as typed budget axes
  inside the session. Never pass a caller's depth as callee fuel or restart a
  nested force operation at depth zero.
- Visit required operands first; Deferred/Cycle/Limit stops owner forcing.
- Do not add a force entry point, recursion stack, depth/fuel counter, or fresh
  sentinel to contain one fixture. Consolidate or reuse the session owner.
- Deferred forms that represent distinct queries carry stable owner/query
  identity. Identity-free recovery sentinels stay fresh and noncacheable.
- Inventory `.force_type`, `.force_deferred`, raw zero-depth resets, every
  recursion owner, and every depth/fuel axis. Direct `force_deferred` calls stay
  behind the canonical force gateway.
- A required-type change states whether it adds a whole-tree pass or resolves a
  `TypeNode` more than once per lexical environment. Do not add an eager prewalk.

## Cache And Side-State Review

If changing a side table can change the semantic answer, include it in the
typed query input or remove the cache. Each cache states its question, full key,
lifetime/reset, incomplete-result policy, and residency bound. Test cold/warm,
cache-disabled or uncached agreement, repeated compilation, reversed roots,
and renamed declarations. If no uncached execution path exists, add one before
claiming cache agreement.

## Ratchet And Review

The architecture metric baseline is a no-growth ratchet, not a quota. Do not
raise it in a feature/campaign change. When consolidation lowers a metric,
lower the baseline in the same change.
Passing the ratchet only proves that measured debt did not grow; it does not
prove that the current mirrored capability or forcing architecture is complete.
The counters are lexical review indicators, not semantic proof; inspect the
owner path even when every count is unchanged.
Production or test shards with committed line metrics must split before growth;
do not spend the gap between the ratchet and the 2,000-line hard cap.

Before handoff, report:

- owner before/after and deleted mirrors;
- local versus program-global completion behavior;
- recursion/cache identity and cleanup behavior;
- architecture metrics before/after;
- exact tests and the temporary-nonclaim deletion condition, if any.

Run:

```bash
python3 scripts/arch/rewrite_architecture_metrics.py --check
python3 scripts/arch/arch_guard.py
python3 scripts/arch/test_arch_guard.py
scripts/agents/llm-context-audit.py
```

Also run focused public tests for any production change.
