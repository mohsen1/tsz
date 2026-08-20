# TSZ Rewrite Contract

TSZ is being rewritten from scratch. Match the pinned TypeScript 7 compiler
exactly, then make correct projects faster. Read `docs/plan/ROADMAP.md` first.

## Non-Negotiable Direction

- Do not restore, copy, wrap, or incrementally migrate the deleted compiler implementation.
- Git history is the archive; retained legacy tests are specifications, not API constraints.
- There is one TypeScript-compatible semantics. Sound Mode and its flags must not return.
- The pinned TypeScript 7.0.2 oracle is the behavioral source of truth.
- Correctness precedes speed. A red, yellow, gray, or stubbed project is not a timing win.

## Replacement Architecture

- `tsz-core` owns syntax, program construction, binding, checking, emit, and the service API.
- `tsz-cli` is a thin process/protocol adapter for `tsz`, `tsz-server`, `tsz-lsp`, and `try-tsz`.
- `tsz-conformance` is an external-process harness and must not import compiler internals.
- Internal phases start as modules. Add a crate only with measured isolation or build-time evidence.
- Prefer a recognizable, source-linked port of TypeScript 7 behavior over a novel solver abstraction.

Semantic invariants:

- deferred type forms remain symbolic until the owning operation must force them;
- a declaration has one program-owned identity; nominal identity is separate from structural equality;
- `TypeId` never crosses checker/session boundaries;
- semantic completion is explicit: complete, deferred, cycle, or limit;
- incomplete/speculative results never enter definitive caches;
- union reduction is selected by the semantic call site, not automatic interning;
- relations produce structured reasons, then diagnostics choose code/message/span;
- emit consumes syntax and checked summaries and does no semantic validation;
- the deterministic single-thread path is authoritative until concurrency proves exact agreement.

## Scope And Size

- No hand-written Rust, test, script, or generated shard exceeds 2,000 physical lines.
- Do not introduce fixture names, file names, user identifiers, source snippets,
  regular expressions over rendered types, or formatted diagnostics as semantic inputs.
- Avoid ambient mutable request state, thread-local semantic fallbacks, mirrored stores,
  global cache epochs, and whole-cache clears.
- A cache requires a typed key, reset/dependency rule, residency expectation, and an uncached agreement test.

## Validation

- Preserve `scripts/conformance`, `scripts/emit`, `scripts/fourslash`, `scripts/bench`,
  `scripts/perf`, project compile guards, snapshots, and the pinned corpus.
- Keep legacy black-box tests as a disabled porting corpus until each is rewritten
  against the public service/CLI surface.
- Use `cargo nextest run`, not `cargo test`.
- Wrap long or memory-heavy commands with `scripts/safe-run.sh`.
- Never run two conformance invocations concurrently.
- Run focused local filters; CI owns broad suites.
- No `dbg!`, `println!`, `print!`, or `eprintln!` in compiler internals; use tracing.

Every behavior change states:

`When <structural condition>, TypeScript 7 does X; TSZ does X through <module/API>.`

Tests include renamed binders, wrappers/nesting, generic and concrete forms,
positive behavior, and negative/fallback behavior when relevant.

## Worktrees, Git, And PRs

- Start non-trivial work with the worktree intake and inspect current PRs/issues.
- Preserve user changes and keep unrelated work out of the branch.
- Every PR names one goal: `green`, `fast`, `grow`, or `hold`.
- PR bodies include `## Verification` and `## Provenance` with Machine,
  Assistant, Model, and Effort.
- Do not open the reset PR until the R0 conviction gate in the roadmap passes.
- Never merge draft/WIP/blocked work. The PR author queues an exact reviewed head.

## Instruction Hygiene

Keep always-loaded instructions short. Durable direction belongs in the roadmap
or focused architecture docs. After changing `.codex/`, `.claude/`, `AGENTS.md`,
skills, or startup hooks, run `scripts/agents/llm-context-audit.py`.
