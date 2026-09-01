# Contributing to TSZ

TSZ is being rewritten from scratch to match the pinned TypeScript 7 compiler
exactly. Compatibility comes before throughput; performance claims apply only
to behavior that already agrees with the oracle.

## Prepare The Workspace

```bash
git clone https://github.com/tsz-org/tsz.git
cd tsz
./scripts/setup/setup.sh
```

The setup script installs the repository hooks and initializes the pinned
TypeScript submodule. Treat that submodule as read-only.

Before non-trivial work:

1. Read [`plan/ROADMAP.md`](plan/ROADMAP.md) and
   [`architecture/RESET.md`](architecture/RESET.md).
2. Inspect open pull requests, recently merged changes, and relevant issues.
3. Run the worktree/disk intake before a heavy build or a new worktree.
4. Keep unrelated or pre-existing changes out of your branch.

## Replacement Workspace

The active Cargo workspace has exactly three packages:

| Package | Responsibility |
| --- | --- |
| `tsz-core` | Syntax, program construction, binding, checking, emit, and the service API |
| `tsz-cli` | Thin adapters for `tsz`, `tsz-server`, `tsz-lsp`, and `try-tsz` |
| `tsz-conformance` | External-process oracle and comparison harness |

Compiler phases begin as modules in `tsz-core`. Do not recreate the deleted
crate-per-phase graph. A new package boundary needs measured build-time or API
isolation evidence. Browser/WASM bindings are deferred until R4 and a stable
service API.

There is one TypeScript-compatible semantics. The retired alternate-semantics
mode and its flags, tests, configuration, and product surface must not return.

## Choosing And Implementing Work

Every change serves one roadmap goal: `green`, `fast`, `grow`, or `hold`.
During the early rewrite, most compiler work is `green`; guardrails for an
already declared capability are `hold`.

For behavior work, record the rule before coding:

```text
When <structural condition>, TypeScript 7 does X; TSZ does X through <module/API>.
```

Use the pinned TypeScript 7.0.2 implementation and oracle output to establish
the preconditions, order of operations, diagnostics, and emit. Prefer a
source-linked port over a new general abstraction. Never make a semantic
decision from fixture paths, user spellings, source snippets, rendered types,
or formatted diagnostics.

Retained tests do not constrain internal APIs. Port them through the public
service or process surface when their behavior becomes supported.

## Validation

Run narrow checks that answer the question your change raises. The basic
rewrite gate is:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
python3 scripts/arch/arch_guard.py
python3 -m unittest discover scripts/arch -p 'test_arch_guard*.py' -v
```

Use `cargo nextest run`, never `cargo test`. Wrap long or memory-heavy commands
with `scripts/safe-run.sh`.

The retained broad suites are observations until their grammar or semantic
families are declared supported. Do not hide crashes or unsupported cases, and
do not compare a rewrite observation with the frozen legacy checkpoint as if it
were a regression floor.

Useful focused launches include:

```bash
./scripts/conformance/conformance.sh run --filter '<case>' --max 1 --workers 1
./scripts/emit/run.sh --filter='<case>' --max=1
./scripts/fourslash/run-fourslash.sh --filter='<case>' --max=1 --sequential
```

Never run two conformance invocations concurrently in one worktree.

## Pull Requests

The reset PR must not open until every item in the roadmap's R0 conviction gate
passes on its exact head. Later pull requests should be small enough that their
supported capability and oracle evidence are reviewable.

Every PR body includes:

```text
Goal: <green|fast|grow|hold>

## Verification
- <exact command and result>

## Provenance
Machine: <actual machine>
Assistant: <actual assistant>
Model: <actual model>
Effort: <actual effort>
```

Also state the structural rule, owning module/API, adjacent cases, unsupported
fallback, and performance evidence when performance motivated the change. Do
not imply broad compatibility from a seed or narrow-filter result.

Never merge draft, WIP, blocked, or stale-head work. The author reviews the
exact head and uses the repository's native merge queue after required checks
pass.

## Style And Repository Hygiene

- No hand-written Rust, test, script, or generated shard may exceed 2,000
  physical lines.
- Keep public APIs small; prefer module visibility until an item belongs to the
  service boundary.
- Use tracing in compiler internals; do not add print debugging or `dbg!`.
- Preserve the test, emit, fourslash, project, and performance harnesses.
- Run `scripts/agents/llm-context-audit.py` after changing agent instructions,
  startup hooks, or skills.
- Run `cargo fmt` before committing; the hook also formats staged Rust.

See [`DEVELOPMENT.md`](DEVELOPMENT.md) and
[`development/TOOLING.md`](development/TOOLING.md) for command details.
