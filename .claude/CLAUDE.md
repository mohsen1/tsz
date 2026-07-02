# TSZ Agent Contract

Goal: match `tsc` exactly. Preserve diagnostics, inference, compatibility,
narrowing, emit, edge cases, and project-row behavior.

## Read First
- Read `docs/plan/ROADMAP.md` before conformance, emit, performance,
  architecture, LSP/WASM, Sound Mode, or DRY cleanup work.
- Do not update the roadmap for routine status, small fixes, cleanup, or PR
  bookkeeping. Use PR bodies, PR/review comments, or issues.
- Inspect open PRs, recent merged PRs, and relevant issues before coding.
- Open or update a PR early for roadmap-adjacent implementation. Every open PR
  runs the full CI suite; do not open a draft to dodge heavy CI.

## Architecture
- Pipeline: `scanner -> parser -> binder -> checker -> solver -> emitter`.
- Scanner: lexing, token stream, string/identifier interning.
- Parser: syntax-only AST in arenas.
- Binder: symbols, scopes, hoisting, flow graph. No type computation.
- Checker: AST orchestration, context, diagnostics, source locations. Ask
  solver for semantic answers.
- Solver: relations, evaluation, inference, instantiation, operations,
  narrowing, semantic caches, type construction, relation failure reasons.
- Emitter: JS/DTS output and transforms. No semantic validation.
- LSP/WASM/CLI consume project/checker/solver APIs; they do not own type
  algorithms.

Hard rules:
- Type semantics belong in solver or a solver-backed query boundary.
- Checker must not pattern-match raw solver internals, construct raw `TypeKey`,
  directly intern solver types, or recurse type shapes when a solver visitor or
  query can own it.
- CLI and ancillary crates consume diagnostics through
  `tsz_checker::diagnostics`.
- Assignability paths for `TS2322`/`TS2345`/`TS2416` use the shared
  `query_boundaries/assignability` gateway: relation -> reason -> diagnostic.
- Emitter must not import checker internals or patch already emitted output to
  encode semantic policy.

## Compatibility Model
- Judge: strict structural/set-theoretic subtype logic.
- Lawyer: `CompatChecker` plus `AnyPropagationRules` for TS legacy quirks:
  `any`, variance, excess/freshness, `void` return exception, weak types.
- Default preference: `any` must not silence structural mismatches unless
  compatibility mode requires it.
- Semantic refs are `TypeData::Lazy(DefId)`. Checker stabilizes `DefId`;
  `TypeEnvironment` resolves `DefId -> TypeId`.

## Data And Organization
- Canonical identity handles: `TypeId(u32)`, `SymbolId(u32)`,
  `FlowNodeId(u32)`, `Atom(u32)`.
- One semantic type universe in the default pipeline.
- Prefer arenas, interning, O(1) identity checks, visitor traversal, and
  dedicated modules by concern.
- No hand-authored source, test, script, or generated-code shard may exceed
  2000 physical lines. Split instead of adding local allowlists or ceilings.

## Testing And Commands
- Never run full conformance, emit, or fourslash locally. Use narrow filters;
  ready-review CI owns broad suites.
- Use `cargo nextest run`, not `cargo test`.
- Wrap long or memory-heavy commands with `scripts/safe-run.sh`.
- Prefer not to checkout the full TypeScript submodule unless the specific
  task needs it.
- For tracing, use `TSZ_LOG`/`TSZ_LOG_FORMAT`; do not add print debugging.
- No temporary `dbg!`; no compiler instrumentation via `println!`, `print!`,
  or `eprintln!`. Use `tracing`.
- To read runtime state (a `TypeId`, a diagnostic's operand, a decision-site
  flag) prefer the `rust-debugger` skill (`rdbg`): break on a repro and inspect
  values in place, instead of instrumenting and rebuilding.
- In Rust doc comments, backtick CamelCase identifiers, file names, and dotted
  paths to satisfy `clippy::doc_markdown`.

## Worktree And Disk
- Before new worktrees, run:
  ```bash
  scripts/setup/disk-worktree-guard.sh
  git worktree list
  ```
- Reuse inactive sister worktrees when possible. New worktrees go beside this
  checkout: `git worktree add ../tsz-<scope> ...`.
- If disk is low, preserve caches first:
  `scripts/setup/disk-worktree-guard.sh --auto-prune` then
  `scripts/setup/clean.sh --quiet`.
- Avoid broad recursive `du` output unless exact ownership is needed.

## GitHub And PR Coordination
- Use `gh`. There are no ownership lanes, no manager, and no reviewer role.
- Every PR serves one of the four roadmap goals: `green` (benchmark rows
  compile like `tsc`), `fast` (green rows 2x faster than `tsgo`), `grow`
  (new corpus projects), `hold` (conformance/emit/fourslash parity floor).
- The `pr-body-gate` CI job greps the remote body for exact field formats;
  match them or the job fails. Required lines, each with a non-empty value:
  - `Goal: <green|fast|grow|hold>`
  - A `## Provenance` block with `Machine: <m1|m4|studio|cloud|hostname>`,
    `Assistant: <claude-code|codex>`, `Model: <model id>`, and
    `Effort: <low|medium|high|max>`. Report your actual runtime values; do
    not invent stable nicknames.
- PR bodies also include a `## Verification` section with the targeted
  commands or CI gates that prove the change.
- Do not add `[codex]` to PR titles.
- Verify remote PR bodies after create/material edit:
  `gh pr view <n> --json body`.
- Land-and-continue: do not idle-wait on a PR's running CI. Push, start the
  next task immediately, and return to queue the merge when its exact-head
  `CI Summary` passes (or fix it if red). Drain owned PRs before starting
  unrelated work; do not park stale draft PRs.
- Never merge WIP: draft, `WIP` label, `[WIP]` title, or body/branch says WIP.
- Adding WIP/draft state requires an immediate comment with reason,
  blocker/current work, next action, and verification already run; include
  your `Machine:`/`Assistant:` provenance line in the comment.
- Close PRs/issues only when merged, user-requested, exact duplicate, or fully
  superseded with evidence and preserved findings.
- The PR author lands their own PR through GitHub's native merge queue. After
  exact-head PR-head checks pass and you reviewed the latest head, queue with
  `gh pr merge <pr> --match-head-commit <sha>` if it is not
  WIP/draft/blocked. Do not use auto-merge as a watcher.
- Native `merge_group` CI owns queued-merge summary validation after enqueue.

## Repo Skills
Use repo-local skills under `.agents/skills/` when the task matches:
- `tsz-worktree-intake`: start/resume safely, choose branch/worktree.
- `tsz-disk-cache-hygiene`: disk cleanup/cache preservation.
- `tsz-pr-coordination`: PR body, labels, WIP/draft/ready, reviews.
- `tsz-ci-pr`: CI, queue, landing.
- `tsz-architecture`: checker/solver/binder/emitter/LSP boundaries.
- `tsz-conformance`: diagnostic conformance and accepted-regression drift.
- `tsz-emit`: JS/DTS emit.
- `tsz-tracing`: tracing-driven debugging.
- `tsz-performance-engineering`: perf/cache/residency.
- `tsz-project-bench`: project corpus and benchmark rows.
- `tsz-iteration-audit`: workflow, guardrail, context debt.
- `rust-debugger`: break on a repro and read runtime state (types, diagnostics,
  checker/solver flow) with `rdbg` instead of instrument-and-rebuild loops.

Runtime/global skills may exist outside this checkout; do not document them as
TSZ repo skills.

## Bug And Fix Discipline
- Parity with `tsc` overrides convenience.
- The reported repro is one witness, not the scope.
- Before coding, identify the wrong semantic operation: relation, inference,
  narrowing, key/index/mapped/conditional/template/infer evaluation, property
  lookup, symbol resolution, diagnostic display, parser recovery, or emit
  transform.
- State the structural rule:
  `When <structural condition>, tsc does X; tsz does X through <owner>.`
- Behavior fixes need adjacent cases: renamed binders, alias/wrapper/nesting,
  generic and concrete forms, positive and negative/fallback cases.
- Performance fixes must address the repeated operation, state complexity/cache
  key/invalidation/fuel/residency expectations, and avoid fixture-name fast
  paths.
- If an observed behavior is a definite `tsc` bug, do not patch away from
  parity. File/comment in tsz with the `TypeScript bug` label and evidence.

## Anti-Hardcoding Gate
Forbidden in checker/solver/emitter decisions:
- user-chosen identifier, alias, type parameter, property, or file-name string
  checks;
- regex/`contains`/`starts_with` over rendered type/printer output;
- single-test/file/source-text scoped suppressions;
- cosmetic widening/dropping/synthesizing types only to match a message.

Use binder/global builtin ids for true built-ins. Use solver or query-boundary
structural helpers for shape decisions. The printer may read types; types must
not read printer output.

Review checklist:
1. No new user-name/file-name literals driving compiler logic.
2. No formatted diagnostic string used as a semantic predicate.
3. Tests vary binder names when binders are involved.
4. PR body states structural rule, owner layer, adjacent matrix, fallback, and
   performance numbers when performance-motivated.

## LLM Context Hygiene
- Keep always-loaded instructions small. Put repeatable workflows in skills,
  scripts, or focused docs.
- Startup hooks must not print `AGENTS.md`, `.claude/CLAUDE.md`, roadmap files,
  large JSON, or broad command output.
- Startup hooks must not mutate git state (`pull`, `rebase`, `merge`,
  checkout, labels, PR updates).
- Avoid project-level output-token overrides unless measured for a lane.
- After editing `.codex/`, `.claude/`, `AGENTS.md`, or startup hooks, run
  `scripts/agents/llm-context-audit.py`.

## PR-Type Notes
- Compiler PRs: structural rule, owner layer, adjacent-case matrix, cache/order
  plan when relevant, project-corpus smoke when a required row may be affected.
- Emit/DTS PRs: failure family, output owner, no semantic validation, no output
  surgery, targeted baseline/unit checks.
- Refactor PRs: behavior unchanged, future campaign enabled, focused checks.

## Local Setup
Run `./scripts/setup/setup.sh` once per workspace and keep
`scripts/githooks` active.
