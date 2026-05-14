# TSZ AGENT SPEC (LLM-COMPACT)

## 0) Mission
- Absolute target: match `tsc` behavior exactly.
- Must match diagnostics, inference, compatibility, narrowing, and edge cases.
- `docs/plan/ROADMAP.md` is the single living roadmap. Before starting
  conformance, emit, performance, architecture, LSP/WASM, Sound Mode, or DRY
  cleanup work, read it and keep your work aligned with it.
- Do **not** update `docs/plan/ROADMAP.md` for routine status, small fixes,
  ordinary cleanup, or PR bookkeeping. That creates avoidable conflicts across
  parallel PRs. Keep those details in draft PR bodies, PR comments, issues, or
  review comments.
- Update `docs/plan/ROADMAP.md` only when the work changes durable direction:
  public metrics, release gates, track sequencing, accepted architecture,
  active priorities, or a roadmap assumption that future agents would otherwise
  rely on incorrectly. Do not create new roadmap files under `docs/plan/`;
  update the living roadmap instead when a roadmap change is truly warranted.
- To avoid duplicate work, roadmap-adjacent implementation must be visible
  before coding starts: inspect open draft PRs, open PRs, recent merged PRs,
  and relevant GitHub issues for overlapping work. A GitHub issue is optional;
  a draft PR with a clear title/body is enough to claim active work.
- Do not add `[codex]` to PR titles. PR titles should follow the repository
  convention, e.g. `fix(checker): ...`, `chore(lsp-tests): ...`, or `[WIP]
  <scope>: <intent>` while the work is still WIP.
- While working, keep the draft PR current with new facts, root-cause
  discoveries, scope changes, and coordination notes. Other agents use draft
  PRs, PR comments, and review comments to decide whether their task duplicates
  active work.
- Never merge WIP branches. A branch is WIP if its PR is draft, has the `WIP`
  label, has a `[WIP]` title prefix, or the PR/branch description says it is
  WIP. Remove the label/prefix and mark the PR ready only after implementation,
  verification, and any justified roadmap update are complete.
- Draft PRs intentionally run only light CI: lint, dist-fast build, and unit
  tests. Marking a PR ready for review triggers the heavy suites: conformance,
  emit, fourslash, and WASM. See §19.5 for the rules around local vs. CI work.
- Keep DRY slices small and behavior-preserving unless explicitly fixing a bug.
  Let CI perform compile, lint, unit, conformance, emit, and fourslash verification.

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

## 19.5) Testing and CI
- **Never run full conformance, fourslash, or emit suites locally.** Those are
  CI's job. Local pre-commit is formatting-only.
- Prefer **not** having the full TypeScript submodule checked out locally
  unless you actually need its sources for a specific investigation. CI has it.
- Run local commands only when they answer a specific debugging question or
  provide fast targeted feedback. Prefer narrow filters and stop once the
  result informs the fix.
- Open a draft PR early. Draft CI runs lint, dist-fast build, and unit tests.
  When the PR is marked ready for review, CI runs conformance, emit, fourslash,
  WASM, and snapshot gates.
- **Never sit waiting for CI.** Push the draft PR, note the run URL if useful,
  then switch to non-overlapping work. Come back later to inspect status.
- **Use `cargo nextest run` instead of `cargo test`** for unit/integration runs.
  Better parallelism, output, and failure reporting.
  - Filtered runs: `cargo nextest run -E 'test(pattern)'` or `cargo nextest run -- pattern`
  - Single crate: `cargo nextest run -p tsz_checker`
  - Wrap full-suite runs with `scripts/safe-run.sh` (see §20.75).

## 19.6) Lint Hygiene For Doc Comments

The workspace lints turn `clippy::doc_markdown` and `clippy::print_stderr`
into hard errors. Both bite often enough to keep in mind upfront:

- **Backtick CamelCase identifiers, file names, and dotted paths in doc
  comments.** Bare `PerformanceDoc.md`, `MyType`, or `getrusage(RUSAGE_SELF)`
  in `///` comments fails `clippy::doc_markdown`. Wrap the identifier in
  backticks: `` `PerformanceDoc.md` ``. Inline code spans inside prose count
  too — `\`docs/PERFORMANCE_PLAN.md\`` over plain `docs/PERFORMANCE_PLAN.md`.
- **No `eprintln!` / `print!` to stderr.** `clippy::print_stderr` is denied.
  Use `tracing::warn!` / `tracing::error!` / a structured logger. Tests can
  use `eprintln!` only behind `#[cfg(test)]` (still preferred to avoid).
- **No `dbg!` / `println!` to stdout** outside a deliberate user-facing CLI
  surface — `clippy::print_stdout` is allowed today but `clippy::dbg_macro`
  is denied; if you add a temporary `dbg!`, remove it before committing.
- Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  locally before pushing when the change needs lint feedback. CI runs it too.

## 19.7) Tracing And Debug Instrumentation

The repo has real tracing infrastructure. Use it for internal diagnostics;
do not add ad-hoc print debugging.

- **Use `tracing`, not `printf`-style debugging.** For Rust internals, prefer
  `tracing::trace!`, `debug!`, `info!`, `warn!`, `error!`, `trace_span!`, and
  `debug_span!` with structured fields:
  ```rust
  tracing::trace!(type_id = type_id.0, symbol = symbol_id.0, "resolved type");
  ```
- **Keep stdout for intentional user output only.** `println!` / `print!` are
  acceptable for tsc-compatible CLI output, help/version text, protocol output,
  and explicit tool reports. They are not acceptable as temporary compiler
  instrumentation. Shell scripts may still use `printf` for their own
  user-facing output.
- **Use the existing subscriber knobs.** `tsz`, `tsz-lsp`, and `tsz-server`
  initialize tracing through `tsz_cli::tracing_config::init_tracing()`. Run with
  `TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -p tsz-cli -- file.ts`, or narrow
  filters such as `TSZ_LOG="tsz_checker=debug,tsz_solver::narrowing=trace"`.
  Use `TSZ_LOG_FORMAT=json` for machine-readable traces and
  `TSZ_LOG=tsz::query_json=trace TSZ_LOG_FORMAT=json` for solver query events.
- **Avoid eager formatting in hot paths.** Put raw ids and small scalars in
  tracing fields. For type display in solver traces, use lazy helpers such as
  `TypeDisplay` and `RelationDisplay`; guard genuinely expensive trace-only
  work with `tracing::enabled!`.
- **If trace/debug output is missing, rebuild appropriately before adding
  prints.** Release-like profiles may compile low-level tracing out for
  performance; use `cargo run`, `cargo build`, or another debug/dev profile
  when investigating `trace!` / `debug!` instrumentation.
- **Tests that assert instrumentation should capture tracing, not stdout.** Use
  existing test tracing helpers where available instead of printing diagnostics
  and eyeballing `cargo nextest` output.

## 20) Repo-Local Skills

No repo-local `SKILL.md` files are currently checked in. Do not list or rely on
TSZ-specific skills here unless their implementation is committed in the repo.
Runtime-provided global skills may exist outside this checkout; do not document
them as TSZ repo skills.

## 20.1) Agent Identity & Collaboration

- **Pick an AgentName.** If you don't have one, derive one from the machine
  you're running on (CPU + RAM + model — e.g. `m3-max-64g-opus47`). Keep it
  stable across the session.
- **Sign your work.** Every PR body and GitHub issue you create or comment on
  must include your AgentName so humans (and other agents) can tell who did it.
- **Shared GitHub identity.** All agents push as the same GitHub user
  (`mohsen1`). Assume sibling agents are operating concurrently under the same
  account — check draft PRs, open PRs, recent merged PRs, and relevant issues
  before starting work, and address other agents by their AgentName when
  relevant.
- **Use `gh` for GitHub operations.** The GitHub CLI is available in this
  workspace and should be preferred over connector/integration tools for
  inspecting PRs/issues, creating or updating PRs, and checking CI status.
- **Stacked PRs for dependent work.** If your new PR depends on another PR
  that should land first, open it as a stacked PR (base = the dependency
  branch, not `main`). When the dependency merges, GitHub automatically
  rebases the base to `main`. Do not wait for the dependency to merge before
  starting; do not duplicate its changes into your branch.

## 20.2) Opportunistic Improvements

- If you spot a mistake while browsing code, fix it. If the fix is genuinely
  unrelated to your current PR, file a GitHub issue instead (with your
  AgentName in the body).
- If you spot a refactor that would improve the code, apply it **only if its
  blast radius fits inside your current PR**. Otherwise file an issue.
- Don't bundle a sprawling refactor into a narrow bug-fix PR — that defeats
  reviewability and breaks the stacked-PR model.

## 20.25) Conformance Maintenance

Conformance is at 100%. The daily mode is **regression prevention**, not
recovery. There is no session picker, campaign loop, or random-pick script.

- Preserve 100% when changing checker, solver, parser, binder, emitter,
  transforms, compiler diagnostics, conformance harness code, TypeScript
  baselines, or conformance snapshot files.
- Every behavior-changing fix needs an owning-crate unit test.
- **Do not run the full conformance suite locally** (see §19.5). Let
  ready-for-review CI run it.
- For local debugging only, use a narrow filter:
  ```bash
  ./scripts/conformance/conformance.sh run --filter "<name>" --verbose
  ```
- If you suspect a regression, the per-test detail is in
  `scripts/conformance/conformance-detail.json` (read it offline; don't
  re-run to inspect). Snapshot aggregates are in
  `scripts/conformance/conformance-snapshot.json`. The
  `python3 scripts/conformance/query-conformance.py --dashboard` command
  surfaces the standard KPIs (big3, crashes, fingerprint-only, etc.).
- If you do introduce a regression, fix it in the same PR or revert. Do not
  hide regressions inside "refresh snapshots" / "integrate batch" /
  "update baselines" commits — those must still be net-zero or net-positive.

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

## 20.8) Disk-Space And Worktree Hygiene
- **Do not burn tokens or terminal output on broad disk archaeology.** Avoid
  `du -sh *`, recursive `du`, or giant sorted size dumps unless a targeted
  cleanup needs exact ownership. Start with compact checks:
  ```bash
  df -h .
  scripts/setup/disk-worktree-guard.sh
  ```
- Before creating a new worktree, check reusable worktrees first:
  ```bash
  scripts/setup/disk-worktree-guard.sh
  git worktree list
  ```
- New worktrees must be created adjacent to this checkout, as sister
  directories under the parent of `tsz`, not nested inside the repo. Example:
  `git worktree add ../tsz-<short-scope> <branch>`.
- Prefer reusing an existing sister worktree that has been inactive for at
  least 4 hours. The guard script prints compact reuse candidates and excludes
  build/cache directories from its activity check.
- If disk is nearly full, do not destroy useful caches first. Run the
  cache-preserving cleanup path:
  ```bash
  scripts/setup/disk-worktree-guard.sh --auto-prune
  scripts/setup/clean.sh --quiet
  ```
  This prunes old Cargo incremental directories and normal debris while keeping
  `.target`, `.target-bench`, `target` artifacts, and the checked-in tsc cache
  unless `--full` is explicitly chosen.
- Use `scripts/setup/clean.sh --full` only as a deliberate last resort after
  confirming the repo/worktree is not being used for an active build.
- When a run fails immediately after a main merge or branch switch, rule out a
  stale binary before assuming a source regression. Prefer harnesses that
  already rebuild stale binaries, such as `scripts/emit/run.sh`; otherwise
  rebuild the narrow binary once and rerun the focused command.

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

## 24.5) Critical: Work Philosophy
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

## 26) GENERALIZATION GATE - the reported repro is an example, not the scope

When fixing a bug, conformance miss, or benchmark regression, treat the
reported input as one witness for a broader rule. A PR that only matches
the reported file, benchmark shape, library alias, or spelling is a
stopgap, not a complete fix.

Before writing code, identify the semantic operation that is wrong or too
slow:

- assignability/subtyping/compatibility decision,
- `keyof` or indexed-access key-space check,
- mapped/conditional/template/infer evaluation,
- property/index lookup or object-shape projection,
- narrowing/control-flow fact,
- symbol resolution/binding,
- diagnostic display policy,
- emit transform policy.

Then write the rule in structural terms:

> "When *<structural condition over syntax, symbols, and/or TypeId>*, tsc
> <does X>; this change makes tsz <do X> too."

If the rule only mentions a single test name, benchmark name, alias name,
type-parameter name, property spelling, or rendered diagnostic string, it
is not general enough. Restate it until the owner layer and equivalent
cases are clear.

### Required before implementation

1. List at least three adjacent cases that should share the same behavior.
   Vary names and shapes: inline vs alias, generic vs concrete, builtin vs
   user-defined, direct vs nested, union/intersection/mapped wrappers when
   relevant.
2. Decide the owning layer from the rule. Type semantics belong in solver
   or query-boundary helpers; checker code should orchestrate and choose
   diagnostic locations.
3. Prefer one shared query/helper over two local branches that recognize
   similar facts in different places. If checker validation and type
   construction both need the same fact, expose that fact once.
4. If a narrow fallback or fast path is necessary, name the semantic
   invariant that makes it safe and document what unsupported shapes fall
   back to the normal path.

### Test matrix for non-trivial fixes

Every non-trivial behavior or performance fix should include focused
coverage for:

1. The reported repro.
2. At least two equivalent shapes that prove the rule, not the spelling.
3. A renamed type-parameter/mapped-variable case when binders are involved.
4. An alias/wrapper/nesting case when aliases or lazy refs are involved.
5. A negative or fallback case proving unsupported shapes are not silently
   accepted.

If the matrix is intentionally smaller, the PR body must say why the
change is a narrow stopgap and what follow-up would make it fundamental.

### Performance fixes

Performance PRs must fix the expensive operation, not only the benchmark
fixture that exposed it.

- Profile or otherwise identify the repeated operation and its expected
  complexity.
- State the intended complexity improvement, for example "avoid lowering
  every union member just to answer a literal property-exists query."
- Benchmark the reported case, but also add or run a smaller equivalent
  fixture that would regress if the implementation only matched the
  benchmark's exact syntax.
- Fast paths must be keyed by structural invariants, not fixture names.
  They must preserve correctness by falling back when the invariant cannot
  be proven.
- If caching is introduced, state the cache key, invalidation assumptions,
  and cycle/fuel behavior.

### PR body requirements

PRs for fixes must include:

- the structural rule being implemented,
- the owner layer and why the logic belongs there,
- the adjacent-case test matrix,
- known unsupported shapes or fallback behavior,
- performance numbers when the PR is performance-motivated.

A PR body that says only "fixes <test>" or "makes <benchmark> pass" is not
enough unless the PR is explicitly marked as a narrow stopgap.
