# TSZ AGENT SPEC (LLM-COMPACT)

## 0) Mission
- Absolute target: match `tsc` behavior exactly.
- Must match diagnostics, inference, compatibility, narrowing, and edge cases.

## 1) North Star
- Solver-first.
- Thin wrappers.
- Visitor-driven type traversal.
- Arena/interning everywhere possible.
- One semantic `TypeId` universe (solver-canonical).
- Public solver API uses `TypeData` naming at boundaries; raw type internals are crate-private.

## 2) Canonical Pipeline
- `scanner -> parser -> binder -> checker -> solver -> emitter`
- LSP consumes project/checker/solver outputs; does not own type algorithms.

## 3) Responsibility Split (WHO/WHAT/WHERE/OUTPUT)
- Scanner: lex + token stream + string interning.
- Parser: syntax only, builds AST in `NodeArena`.
- Binder (`WHO`): symbols, scopes, flow graph; no type computation.
- Checker (`WHERE`): AST walk, context, diagnostics; delegates type logic.
- Solver (`WHAT`): all type relations/evaluation/inference/instantiation/narrowing.
- Emitter (`OUTPUT`): JS/transform emission; no semantic type validation.

## 4) Hard Architecture Rules
- If code computes type semantics, it belongs in Solver.
- Checker must not implement ad-hoc type algorithms.
- Checker must not pattern-match low-level type internals when Solver query exists.
- Checker must not import/construct raw `TypeKey` or perform direct solver interning.
- CLI and ancillary crates must consume checker diagnostics via `tsz_checker::diagnostics`.
- Use visitor helpers for type traversal; avoid repeated `TypeKey` matching.
- No forbidden shortcuts:
  - Binder importing Solver for semantic decisions.
  - Emitter importing Checker internals for semantic checks.
  - Any layer bypassing canonical query APIs.
  - Checker or CLI using `tsz_checker::types` internal diagnostic paths.

## 5) Judge/Lawyer Model (Compatibility)
- Judge (`SubtypeChecker`): strict structural/set-theoretic subtype logic.
- Lawyer (`CompatChecker` + `AnyPropagationRules`): TS legacy/compat quirks.
- Lawyer owns:
  - `any` propagation policy,
  - variance modes,
  - excess property/freshness,
  - `void` return exception,
  - weak type detection (TS2559).
- Preferred default: `any` must not silence structural mismatches unless compatibility mode requires it.

## 6) DefId-First Semantic Type Resolution
- Semantic refs in Solver are `TypeData::Lazy(DefId)` (with `TypeKey` sealed/private).
- Checker creates/stabilizes `DefId` and ensures environment mapping exists.
- `TypeEnvironment` resolves `DefId -> TypeId` during relation/evaluation.
- Type-shape traversal and `Lazy(DefId)` discovery must use solver visitors, not checker recursion.
- New semantic refs must not use ad-hoc symbol-backed shortcuts.

## 7) Core Data/Perf Contracts
- Identity handles:
  - `TypeId(u32)`, `SymbolId(u32)`, `FlowNodeId(u32)`, `Atom(u32)`.
- Single semantic type universe: no checker-local semantic `TypeId`/`TypeArena` in default pipeline.
- O(1): type equality, symbol lookup, node access, string equality.
- AST node header target: 16 bytes.
- Arena allocation for AST/symbols/flow.
- Global type interning for dedup + O(1) equality.

## 8) Scanner Contracts
- Zero-copy source handling (`Arc<str>` style).
- Identifiers/strings interned to `Atom`.
- Parser-driven scanning API.
- Contextual rescans supported (`>`, `/`, template).

## 9) Parser Contracts
- Produces syntax-only AST.
- Node header fixed/thin (`kind`, `flags`, `pos`, `end`, `data_index`).
- Typed side pools for payloads.
- Recursion guard limit enforced.

## 10) Binder Contracts
- Produces:
  - symbol arena,
  - persistent scope tree (not transient stack model),
  - control-flow graph.
- Handles hoisting with multi-pass strategy.
- No type inference/subtyping logic in binder.

## 11) Solver Contracts
- Owns relation/evaluation/inference/instantiation/operations/narrowing.
- Owns type construction via safe factories/builders (`array`, `union`, `intersection`, `lazy`, etc.).
- Uses memoization and cycle/coinductive handling.
- Type graph represented through interned `TypeData` variants (crate-private internals; no raw `TypeKey` leakage).
- Owns algorithmic caches (relation/evaluation/instantiation/property/index/keyof/template).
- Provides structured relation failure reasons for checker diagnostics.
- Enforce recursion/fuel limits for subtype/evaluate/instantiate workloads.

## 12) Checker Contracts
- Thin orchestration only.
- Reads AST/symbol/flow facts; asks Solver for semantic answers.
- Tracks diagnostics and source locations.
- Maintains node/symbol/flow/diagnostic caches and recursion guards.
- Must not own algorithmic type caches or type-shape traversal logic.
- Checker files should stay under ~2000 LOC.

## 13) Emitter Contracts
- Prints/transforms output from checked structures.
- Supports downlevel transform pipeline (AST -> IR -> print) where applicable.
- No on-the-fly semantic type validation.

## 14) LSP Contracts
- Consumer/orchestrator over project state.
- Global type interning across files.
- Incremental updates via reverse deps.
- Should remain WASM-compatible constraints when required.

## 15) Dependency Policy (Review Gate)
For each change ask:
1. Is this `WHAT` (type algorithm) or `WHERE` (diagnostic location/orchestration)?
2. If `WHAT`: move to Solver/query helper.
3. If `WHERE`: keep in Checker and call Solver.
4. Does this introduce checker access to solver internals (`TypeKey`, raw interner)? If yes, reject.
5. Does assignability flow through the shared compatibility gate? If no, refactor first.

## 16) Type System Surface (Canonical Kinds)
- Must support primitives, literals, arrays/tuples, objects, unions/intersections,
  callables/functions, type params, lazy refs, applications, conditionals,
  mapped/index/keyof/template/this/unique symbol/readonly/string intrinsics/infer/error.
- Keep built-in TypeId reservations stable (0..99 range policy).

## 17) Flow/Symbol Contracts
- Symbols include declarations, parent linkage, member/export tables, flags/modifiers.
- Flow graph nodes include flags + antecedents + AST link.
- Preserve TS-relevant symbol/modifier semantics.

## 18) Performance Targets (Directional)
- Parse: high-throughput, parallel at file level.
- Bind/check: incremental-friendly with reverse dependency tracking.
- Hot paths avoid per-op heap allocation where practical.
- Maintain fast identity checks before structural checks.

## 19) Code Organization Guidance
- Keep modules aligned with pipeline ownership (`scanner`, `parser`, `binder`, `solver`, `checker`, `emitter`, `transforms`).
- Prefer dedicated files per major checker/solver concern.
- Avoid growth of monolith modules; split before crossing maintainability threshold.

## 20) Skills (Operational)
Available skills and triggers:
- `architecture-guardrails`: detect forbidden architecture patterns.
- `bench-gatekeeper`: benchmark vs baseline.
- `rust-test-runner`: run Rust tests via Docker wrapper.
- `sync-and-merge-assistant`: safe sync/merge workflow.
- `worker-assignment-orchestrator`: assign high-impact tasks.
- `skill-creator`: create/update skills.
- `skill-installer`: install curated/repo skills.

Skill usage rules:
- If user names a skill or task clearly matches it, use that skill this turn.
- Read `SKILL.md` minimally; load only needed referenced files.
- Reuse scripts/assets/templates from skill directories when available.
- If blocked/missing, state issue briefly and proceed with best fallback.

## 20.25) Multi-Session Work (Campaign System — v2 Post-90%)
- **Always use `ultrathink` at the start of every agent prompt.**
- **Max 3 concurrent agents** to avoid rate limit cascades. Use `launch-agents.sh`.
- **Campaign work is the DEFAULT**, not the exception. The remaining failures are architecture-shaped.
- **Multi-crate changes are EXPECTED.** Do not reroll because a fix touches solver + checker + boundary.
- Read `scripts/session/AGENT_PROTOCOL.md` for the full protocol.

### Campaign Tiers and Agent Allocation
| Tier | Allocation | Campaigns | Focus |
|------|-----------|-----------|-------|
| **1** | 50% of agents | big3-unification, request-transport, narrowing-boundary | Trunk: relation kernel, context transport, boundary cleanup |
| **2** | 30% of agents | node-declaration-emit, crash-zero, stable-identity | Subsystem: resolver, crashes, identity |
| **3** | 20% of agents | parser-diagnostics, false-positives, jsdoc-jsx-salsa | Leaf cleanup, parser, integration areas |

### KPIs (Track These, Not Overall %)
| KPI | Command | Target |
|-----|---------|--------|
| Wrong-code TS2322+TS2339+TS2345 | `query-conformance.py --dashboard` | Reduce by 50% |
| Crash count | `query-conformance.py --dashboard` | Zero |
| Node lane pass rate | `query-conformance.py --dashboard` | >75% |
| Close-to-passing (diff 0/1/2) | `query-conformance.py --close 2` | Reduce by 50% |
| Direct solver calls in narrowing | `rg "type_queries\." crates/tsz-checker/src/flow/` | Zero |
| Raw contextual mutations | `rg "contextual_type\b" crates/tsz-checker/src/` | Zero outside TypingRequest |

### Quick Reference
```bash
# KPI dashboard (primary daily signal):
python3 scripts/conformance/query-conformance.py --dashboard

# Health check (run before starting any campaign work):
scripts/session/healthcheck.sh

# See campaign status (what's claimed vs available):
scripts/session/check-status.sh

# Claim a campaign and create a worktree:
scripts/session/start-campaign.sh <campaign-name>

# Check/record session progress (MANDATORY before claiming session done):
scripts/session/campaign-checkpoint.sh <campaign-name> --status   # read progress
scripts/session/campaign-checkpoint.sh <campaign-name> --init     # initialize
scripts/session/campaign-checkpoint.sh <campaign-name>            # record checkpoint

# Launch agents with staggered starts (prevents rate limit cascade):
scripts/session/launch-agents.sh --max 3 --stagger 120

# Integrator: validate and merge campaign branches:
scripts/session/integrate.sh --auto

# Clean up disk (stale worktrees, old targets):
scripts/session/cleanup.sh --auto

# Setup a new machine:
scripts/session/setup-machine.sh
```

### Key Rules
- **Campaign work is default.** Leaf fixes are only for Tier 3 agents.
- **Follow root causes across crate boundaries.** Campaigns define missions, not file ownership.
- **Multi-crate changes are normal.** Do NOT reroll for "broad-surface" targets (Tier 1/2).
- **Track KPIs, not overall %.** Each campaign has a specific KPI in campaigns.yaml.
- **Find invariants that fix BOTH missing AND extra diagnostics.** One-directional fixes create drift.
- **Never declare a campaign "complete."** Only the integrator can. Run the checkpoint script.
- **Read the progress file before starting.** Don't re-investigate known dead ends.
- **Max 3 concurrent agents** to avoid rate limit cascades. Use `launch-agents.sh`.

### Periodic coordination via /loop
```bash
# Worker agents — rebase and check status:
/loop 30m run scripts/session/check-status.sh and rebase on origin/main if needed

# Integrator — validate, merge, and report KPIs:
/loop 30m run scripts/session/integrate.sh --auto && python3 scripts/conformance/query-conformance.py --dashboard

# Cleanup — free disk space:
/loop 4h run scripts/session/cleanup.sh --auto
```

## 20.5) Conformance Analysis Tools

### CRITICAL: Avoid re-running the full conformance suite
- The full conformance suite takes **minutes** to run. Do NOT run it for research, planning, or analysis.
- **All analysis can be done offline** from pre-computed snapshot files. Use the query tools below.
- Only run the full suite (`./scripts/conformance/conformance.sh run` or `snapshot`) when you need to **verify code changes** you've made.

### KPI Dashboard (primary daily signal)
```bash
# This replaces overall conformance % as the primary signal
python3 scripts/conformance/query-conformance.py --dashboard
```
The dashboard shows:
1. **Big3 wrong-code count** (TS2322+TS2339+TS2345 missing/extra breakdown)
2. **Crash count** (tests where we emit 0 but tsc expects diagnostics)
3. **Node lane pass rate** (NodeModulesSearch, jsFileCompilation, node, declarationEmit)
4. **Close-to-passing** (fingerprint-only, diff=1, diff=2)
5. **Failure categories** (false positives, all-missing, wrong-code, fingerprint-only)
6. **Campaign impact** (Tier 1 and Tier 2 campaign test counts)

### Offline analysis (preferred — zero cost, instant)
Two snapshot files contain everything needed for analysis:
- **`scripts/conformance/conformance-snapshot.json`** — high-level aggregates (summary, areas, top failures, quick wins).
- **`scripts/conformance/conformance-detail.json`** — per-test failure data (expected/actual/missing/extra codes for every failing test, ~400KB).

### Preferred strategy: campaign-first, not whack-a-mole
- **Campaign work is the default.** Leaf fixes are only for Tier 3 agents.
- Start with the **KPI dashboard**, then deep-dive your assigned campaign:
  ```bash
  python3 scripts/conformance/query-conformance.py --dashboard
  python3 scripts/conformance/query-conformance.py --campaign big3
  python3 scripts/conformance/query-conformance.py --campaign contextual-typing
  python3 scripts/conformance/query-conformance.py --campaign narrowing-flow
  python3 scripts/conformance/query-conformance.py --campaign module-resolution
  python3 scripts/conformance/query-conformance.py --campaign parser-recovery
  python3 scripts/conformance/query-conformance.py --campaign jsdoc-jsx-salsa
  ```
- Treat **`TS2322` / `TS2339` / `TS2345`** as one family. Find invariants that fix BOTH missing AND extra.
- Do **not** pick work solely from one-extra/one-missing/close lists. Those are Tier 3 tools.
- For each campaign:
  1. Pick 8-15 representative failures spanning both missing and extra diagnostics.
  2. Write the shared semantic invariant first.
  3. Fix the invariant in Solver or a boundary helper, not in checker-local heuristics.
  4. Use targeted filtered runs only after code changes.
- `JSDoc`, `JSX`, and `Salsa` are **regression baskets** — most fixes come from Tier 1 campaigns.

**Query tool** (`python3 scripts/conformance/query-conformance.py`):
```bash
# KPI dashboard (primary daily signal)
python3 scripts/conformance/query-conformance.py --dashboard

# Overview: what to work on next
python3 scripts/conformance/query-conformance.py

# Recommended root-cause campaigns
python3 scripts/conformance/query-conformance.py --campaigns

# Deep-dive one campaign
python3 scripts/conformance/query-conformance.py --campaign big3

# Tests fixable by adding 1 missing code (Tier 3 only)
python3 scripts/conformance/query-conformance.py --one-missing

# Tests fixable by removing 1 extra code (Tier 3 only)
python3 scripts/conformance/query-conformance.py --one-extra

# False positive breakdown (expected 0, we emit errors)
python3 scripts/conformance/query-conformance.py --false-positives

# Deep-dive a specific error code (shows would-pass, also-needs, extras)
python3 scripts/conformance/query-conformance.py --code TS2454

# List tests where a code is falsely emitted
python3 scripts/conformance/query-conformance.py --extra-code TS7053

# Tests closest to passing (diff <= N)
python3 scripts/conformance/query-conformance.py --close 2

# Export paths for piping into conformance runner
python3 scripts/conformance/query-conformance.py --code TS2454 --paths-only
```

**Reading snapshot JSON directly** (for custom queries):
```python
import json
with open('scripts/conformance/conformance-snapshot.json') as f:
    snap = json.load(f)
# Keys: summary, areas_by_pass_rate, top_failures, not_implemented_codes,
#        partial_codes, one_missing_zero_extra, one_extra_zero_missing,
#        false_positive_codes, top_missing_codes, top_extra_codes, categories
```

**Reading detail JSON directly** (for per-test queries):
```python
import json
with open('scripts/conformance/conformance-detail.json') as f:
    detail = json.load(f)
# detail["failures"][test_path] = {"e": [...], "a": [...], "m": [...], "x": [...]}
# e=expected, a=actual, m=missing, x=extra
# PASS tests are not in the failures dict (implicit pass).
```

### TSC cache for research (what does tsc expect?)
- `scripts/conformance/tsc-cache-full.json` contains tsc's expected diagnostics for every test.
- Each entry has `error_codes`, `diagnostic_fingerprints` (code, file, line, column, message_key).
- Use Python/jq to query the cache for tests expecting a specific error code without running anything:
  ```python
  python3 -c "
  import json
  with open('scripts/conformance/tsc-cache-full.json') as f:
      cache = json.load(f)
  for key, val in sorted(cache.items()):
      if CODE in val.get('error_codes', []):
          print(key)
  "
  ```

### Targeted testing (after code changes)
- `./scripts/conformance/conformance.sh run --filter "pattern"` — run only tests matching a filename pattern (fast, seconds).
- `./scripts/conformance/conformance.sh run --filter "pattern" --verbose` — see expected vs actual diagnostics for failures.
- Use `--max N` to limit test count for quick smoke tests.

### Full suite (use sparingly — only to verify changes)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run` — run all conformance tests (error-code level).
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot` — run + analyze + save all snapshot files. Run this after a batch of changes to update the offline data.

## 20.75) Memory-Guarded Execution (`scripts/safe-run.sh`)
- **All long-running or memory-intensive commands MUST be wrapped with `scripts/safe-run.sh`.**
- This includes: full conformance runs, `cargo test` (full suite), `cargo build --release`, and any multi-worker test runner.
- The wrapper monitors the process tree's total RSS and kills it if it exceeds the limit (default: 75% of system RAM).
- Overhead is negligible — one `ps` call every 5 seconds.
- Quick, filtered runs (`--filter`, `--max`) and `cargo check` generally don't need the wrapper.

```bash
# Wrap any heavy command
scripts/safe-run.sh cargo test
scripts/safe-run.sh ./scripts/conformance/conformance.sh run
scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot

# Custom limit (absolute MB or percentage of RAM)
scripts/safe-run.sh --limit 8192 -- cargo test --release
scripts/safe-run.sh --limit 50% -- ./scripts/conformance/conformance.sh run

# Debug memory usage
scripts/safe-run.sh --verbose -- cargo build
```

## 21) Non-Negotiables
- Parity with `tsc` overrides convenience.
- Architecture direction is one-way; no cross-layer semantic leakage.
- Solver is the single source of truth for type computation.

## 22) TS2322 Priority Rules
- `TS2322` parity is a top-level gate for checker/solver work.
- `TS2322`/`TS2345`/`TS2416` paths must use one compatibility gateway via `query_boundaries`.
- Gateway order is fixed: relation -> reason -> diagnostic rendering.
- Use the assignability gate helper as a single entrypoint for relation + failure analysis; avoid split ad-hoc calls.
- New checker code must not call `CompatChecker` directly for TS2322-family paths; route through `query_boundaries/assignability`.
- Checker must not instantiate solver internals in feature modules when a boundary helper can exist.
- Keep `TS2322` behavior centralized:
  - one suppression/prioritization policy,
  - one mismatch decision gate,
  - one explain-to-diagnostic rendering path.
- Prefer moving new fixes into boundary helpers and solver query logic, not ad-hoc checker branches.

## 23) TS2322 Change Review Checklist
1. Does this change alter `Assignable`/`Subtype` behavior in checker code?
2. If yes, can it be expressed as a solver relation query or boundary helper first?
3. Are weak-union/excess-property/any-propagation behaviors preserved and explicit?
4. Does the path resolve `Lazy(DefId)` via `TypeEnvironment` before relation checks?
5. Is traversal/precondition discovery delegated to solver visitors (no checker type recursion)?
6. Is the diagnostic generated from solver failure reason instead of checker-local heuristics?

## 24) Local Setup Requirements
- Run `./scripts/setup/setup.sh` once per workspace and keep `scripts/githooks` active.
- This ensures `pre-commit` checks run locally before commits.
- If hooks are not installed, local lint guardrails can be bypassed accidentally.


## 24) Critical: Work Philosophy
- Prioritize **architectural integrity** over quick fixes.
- When in doubt, choose the path that preserves the clean separation of concerns and long-term maintainability, even if it requires more upfront work.
- Avoid patching symptoms in the checker; instead, invest in the solver and boundary helpers to keep the architecture sound.
- Use the conformance analysis tools to guide work towards root causes, not just individual test failures.