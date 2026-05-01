# TSZ AGENT SPEC (LLM-COMPACT)

## 0) Mission
- Absolute target: match `tsc` behavior exactly.
- Must match diagnostics, inference, compatibility, narrowing, and edge cases.
- `docs/plan/ROADMAP.md` is the single living roadmap. Before starting
  conformance, emit, performance, architecture, LSP/WASM, Sound Mode, or DRY
  cleanup work, read it and keep your work aligned with it.
- Update `docs/plan/ROADMAP.md` in the same PR when your work changes roadmap
  status, metrics, sequencing, risks, active priorities, or invalidates a plan
  assumption. Do not create new roadmap files under `docs/plan/`; update the
  living roadmap instead.
- To avoid duplicate work, roadmap-adjacent implementation MUST be claimed
  before coding starts: create a branch, make a minimal claim under
  `docs/plan/ROADMAP.md` -> `Active Implementation Claims`, then open a draft
  PR with the GitHub label `WIP`. Use a title like `[WIP] <scope>: <intent>`.
- Claim entry format: prefix with `**YYYY-MM-DD HH:MM:SS**` (UTC, current
  wall-clock time). Each claim's unique second-precision timestamp gives a
  natural sort order and reduces ROADMAP.md merge conflicts when multiple
  agents claim work concurrently. Older entries without timestamps use
  `00:00:00` as a placeholder.
- Never merge WIP branches. A branch is WIP if its PR is draft, has the `WIP`
  label, has a `[WIP]` title prefix, or the PR/branch description says it is
  WIP. Remove the label/prefix and mark the PR ready only after implementation,
  verification, and roadmap status updates are complete.
- DRY cleanup claims also live in `docs/plan/ROADMAP.md`. Keep DRY slices small,
  behavior-preserving unless explicitly fixing a bug, and verify them with
  `scripts/session/verify-all.sh` before removing WIP status.

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

## 19.5) Testing
- **Use `cargo nextest run` instead of `cargo test`** for all unit/integration test runs.
- `cargo nextest run` provides better parallelism, output, and failure reporting.
- Wrap full-suite runs with `scripts/safe-run.sh`: `scripts/safe-run.sh cargo nextest run`
- Filtered runs: `cargo nextest run -E 'test(pattern)'` or `cargo nextest run -- pattern`
- Single crate: `cargo nextest run -p tsz_checker`

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

## 20.25) Conformance Work — Entry Point
- **Always use `ultrathink` at the start of every agent prompt.**
- **Read `scripts/session/conformance-agent-prompt.md` first.** It is the single
  source of truth for how to pick, diagnose, fix, test, and ship conformance work.
- **Pick a random failure** with `scripts/session/quick-pick.sh` and work on
  what it gives you. Do not reroll to avoid hard targets.
- **DO NOT create a new picker script.** `scripts/session/quick-pick.sh`
  (delegating to `scripts/session/pick.py`) is the canonical entry point.
  If you think it lacks a feature, add a flag to `pick.py` — never fork a
  new `random-*.sh` / `pick-*.sh` wrapper. PR #1957 had to delete 11
  orphan session scripts that accumulated this way; do not start the
  cycle again. See `scripts/session/conformance-agent-prompt.md` for the
  full rationale and the list of past duplicates.
- **Fix the root cause, not the symptom.** Multi-crate changes are expected.
  Checker symptoms often have solver or boundary-helper root causes.
- **Every fix ships with a unit test** in the owning crate (`tsz-solver`,
  `tsz-checker`, etc.) plus no-regression verification on conformance.
- **Architecture first.** Follow §3, §4, §11, §12, §22 of this file. Never
  bypass `query_boundaries/assignability`; never pattern-match solver
  internals from the checker.
- **Open a pull request.** Never push directly to `main`.
- **Do not bail.** If you understand the root cause, fix it. If the fix spans
  solver + checker + boundary, make the fix.

## 20.26) ANTI-BAIL DIRECTIVE — read every conformance turn

The agent's job on conformance work is to **land a merged PR**, not to write
research notes about why the work is hard. Every conformance task in this
codebase is non-trivial — the easy ones are already merged. The remaining
diff=1, fingerprint-only, and false-positive failures all touch deep solver,
checker, or boundary code, and that is **expected**, not a reason to stop.

### Forbidden bail-outs

The following are explicitly forbidden. If you catch yourself doing any of
these, stop, re-read this section, and resume the original target.

1. **"This requires deep solver/checker work, so I'll abandon and pick a
   simpler test."** No. There is no "simpler" test in the current corpus.
   Every remaining failure is multi-hour work. Pivoting just spends the
   next two hours on a different multi-hour problem.
2. **"I've narrowed the reproducer; the next agent can resume from here."**
   No. *You* are the next agent. Implement the fix. Investigation that
   doesn't ship is wasted budget.
3. **"This is a known TS feature gap (e.g., type-predicate inference,
   homomorphic mapped indexing, JSX runtime resolution) — out of scope
   for a quick fix."** Nothing here is "out of scope". If tsc emits the
   diagnostic, tsz must too. Implement the feature.
4. **"Time-box exhausted, ending with research findings."** Time is not
   the constraint — task convergence is. If your current trace has
   localized the failure surface, push through. If it has not, take
   another instrumentation pass; do not switch tests.
5. **"The unit test harness has a limitation."** Then fix the harness.
   This codebase already has multiple commits that adjust the test
   harness as part of a larger checker fix.
6. **"Each candidate I look at involves multiple-fingerprint diffs / XFAIL /
   declaration merging / module resolution."** Yes. Pick one and fix all
   of its diffs. A merged PR that flips one XFAIL is worth ten abandoned
   investigations.

### When you genuinely cannot proceed

Two cases:

- **You hit an architectural rule that forbids the obvious fix.** Read
  §3, §4, §11, §12, §22, then route the fix through `query_boundaries`
  or a solver query helper. The architecture rules tell you *where* to
  fix, not *whether*.
- **The fix would cross 5+ files in unrelated subsystems** (e.g.,
  scanner + binder + emitter for a checker symptom). Even then, do the
  fix — multi-crate PRs are normal. The roadmap explicitly lists them
  as expected.

### Convergence budget

For a conformance fix:

- **0–60 min**: reproduce, instrument, locate the failure surface.
- **60–180 min**: implement the fix, including any required harness or
  helper plumbing.
- **180+ min**: keep going. The first good fix in a new area routinely
  takes a full day. Subsequent fixes in the same area are 10× faster
  because the instrumentation and mental model are already paid for.

If the loop fires multiple times for the same task, **continue the same
task each iteration**. Do not start fresh. Read the running plan or claim
file for context and resume where the prior iteration left off.

### What "shipping" means

A conformance task is shipped when:

1. The targeted test passes (`./scripts/conformance/conformance.sh run --filter <name>` shows `PASS` or `1/1 passed`).
2. A unit test in the owning crate locks the new behavior.
3. `cargo nextest run -p tsz-checker -p tsz-solver` is clean.
4. A non-draft PR is open against `main` with the diff and the unit
   test, branch pushed, CI started.

Once step 4 lands, the task is **shipped from the agent's perspective**.
Do not babysit CI, do not chase rebase, do not merge — the user handles
the merge queue. The agent's loop should immediately move on to the
next conformance task instead of camping on the prior PR's checks.

If CI later goes red on a PR you opened, the user will surface the
failure and you'll iterate on it then. Until that happens, the open PR
is "done" for loop purposes.

Research notes, draft/abandoned PRs, and "deferred" claim files do
**not** count as shipped. A green local build + a single-test
conformance pass + a non-draft PR pushed to `origin` does.

### Quick reference
```bash
# Pick one random failure (prints path + codes + a verbose-run command):
scripts/session/quick-pick.sh

# Offline research (never run the full suite for research):
python3 scripts/conformance/query-conformance.py --dashboard

# Verify a specific test:
./scripts/conformance/conformance.sh run --filter "<name>" --verbose

# Unit tests for the crates you changed:
cargo nextest run --package tsz-checker --lib
cargo nextest run --package tsz-solver --lib

# Full conformance (heavy — use the safe-run wrapper):
scripts/safe-run.sh ./scripts/conformance/conformance.sh run

# Refresh offline snapshot after a batch of fixes:
scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot

# Push and open a PR (never push to main):
git push -u origin <your-branch>
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

### Preferred strategy: fingerprint parity first, then wrong-code campaigns
- **Fingerprint parity is the #1 lever.** 617 tests (73.6%) already emit the right codes but wrong message/position/count.
- Start with the **KPI dashboard**, then check fingerprint-only failures:
  ```bash
  python3 scripts/conformance/query-conformance.py --dashboard
  python3 scripts/conformance/query-conformance.py --fingerprint-only
  python3 scripts/conformance/query-conformance.py --fingerprint-only --code TS2322
  python3 scripts/conformance/query-conformance.py --campaign type-display-parity
  python3 scripts/conformance/query-conformance.py --campaign diagnostic-count
  python3 scripts/conformance/query-conformance.py --campaign big3
  python3 scripts/conformance/query-conformance.py --campaign narrowing-flow
  ```
- **Fingerprint campaigns (Tier 1)**: Fix type printer display, diagnostic emission counts, or position anchoring.
  One good printer fix can flip 50+ tests. Target >1.0 tests per commit.
- **Wrong-code campaigns (Tier 2)**: Find invariants that fix BOTH missing AND extra diagnostics.
  Fix in Solver or boundary helpers, not checker-local heuristics.
- Do **not** pick work solely from one-extra/one-missing/close lists. Those are Tier 3 tools.

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
- This includes: full conformance runs, `cargo nextest run` (full suite), `cargo build --release`, and any multi-worker test runner.
- The wrapper monitors the process tree's total RSS and kills it if it exceeds the limit (default: 75% of system RAM).
- Overhead is negligible — one `ps` call every 5 seconds.
- Quick, filtered runs (`--filter`, `--max`) and `cargo check` generally don't need the wrapper.

```bash
# Wrap any heavy command
scripts/safe-run.sh cargo nextest run
scripts/safe-run.sh ./scripts/conformance/conformance.sh run
scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot

# Custom limit (absolute MB or percentage of RAM)
scripts/safe-run.sh --limit 8192 -- cargo nextest run --cargo-profile release
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
- See §25 for forbidden symptom-patching patterns (hardcoded identifier
  names, regexes over printer output, single-test scoped suppressions).

## 25) ANTI-HARDCODING DIRECTIVE — never patch a single test's symptom

The conformance corpus is hostile to point fixes. Almost every failing
test is one instance of a class of failures, and tsc's behaviour is
defined by the underlying type-system rule, not by the spelling that
happened to appear in *this* test. Fixes that only handle the spelling
in front of you do not converge — they pass one test, miss its 30
siblings, and add brittle code that later refactors must work around.

**These patterns are forbidden in checker, solver, and emitter code.**
If you catch yourself writing one, stop, restate the underlying rule
in one sentence, then write a fix that expresses *that* rule.

### Forbidden: hardcoded user-chosen names

User-chosen identifiers (type-parameter names, mapped-type iteration
variable names, property keys, alias names, file names) must never
appear as string literals or `==` checks in compiler logic.

```rust
// ❌ WRONG — only matches when the user chose `P` as the iteration var.
//    `Readonly<T> = { [K in keyof T]: T[K] }` (idiomatic) silently
//    bypasses the suppression.
fn looks_like_invalid_optional_mapped_display(display: &str) -> bool {
    display.starts_with("{ [P in ")
        && display.contains("]?: ")
        && display.ends_with(" | undefined; }")
}

// ❌ WRONG — same anti-pattern, different surface.
if type_param.name == "T" { /* special-case */ }
if alias_name == "Promise" { /* special-case */ }
if file_name.ends_with("/lib.es5.d.ts") { /* special-case */ }
```

If the rule is genuinely about a *built-in* well-known name (`Promise`,
`Iterable`, `Symbol.iterator`), resolve it through the binder/global
table or a builtin-id constant, not by string-matching.

### Forbidden: regexes / `starts_with` / `contains` over printer output

The type printer is the *output* of the type system, not its input.
Driving compiler decisions from rendered display strings is a sign the
fix belongs in the solver or in a boundary helper that operates on
`TypeId` / structural shapes.

```rust
// ❌ WRONG — interrogating the printed form of a type.
if self.format_type_diagnostic(target).contains("undefined; }") { ... }
if display.starts_with("{ [") && display.ends_with(" | undefined; }") { ... }

// ✅ RIGHT — ask the solver about the structural property.
if self.is_optional_mapped_with_undefined_member(target) { ... }
```

Add a query helper to `query_boundaries/` (or a structural inspector in
`tsz_solver`) and call it from the checker. The printer is allowed to
read types; types are not allowed to read the printer.

### Forbidden: single-test scoped suppressions

```rust
// ❌ WRONG — a fingerprint of one specific failing test.
if file_name.contains("contextualTyping33") { return; }
if source_str == "{ a: 1 }" && target_str == "{ a: true }" { ... }
```

If a diagnostic should be suppressed, the rule is "for sources/targets
*shaped like X*", never "for the literal types in this test file".
Express the shape via solver queries.

### Forbidden: cosmetic widening to silence a fingerprint

Do not widen a literal to its primitive, drop a property, or insert a
synthesized type, **only** so the rendered message matches tsc. The
solver's underlying types must remain accurate; the printer's display
policy is the place to align rendering with tsc.

### Required: state the rule before you write the code

Every checker/solver/emitter change that affects diagnostics must be
expressible as one sentence of the form:

> "When *<structural condition over types/symbols>*, tsc <does X>; this change makes tsz <do X> too."

If your one-sentence rule contains a specific identifier name, file
path, or rendered display fragment, the fix is in the wrong place.
Restate the rule structurally, then fix it where structures live (the
solver, the binder, or the boundary helpers).

### Review checklist (gate before merging)

Before opening a PR, verify each item:

1. No new string literals naming user-chosen identifiers, aliases, or
   files in checker/solver/emitter code.
2. No new `format_type_diagnostic` / printer-output `contains` /
   `starts_with` / regex calls used to drive a *decision* (vs. building
   the final user-facing message).
3. The unit test you added covers at least two name choices for any
   bound variable in the fix (`T`/`K`, `P`/`X`, etc.) — if changing the
   name breaks the fix, the fix is hardcoded.
4. The PR description states the structural rule in one sentence and
   does not refer to a single test name as the justification.

This directive supersedes any prior pattern in the codebase. Existing
hardcoded checks discovered during review should be flagged for
follow-up, not used as precedent for new ones.