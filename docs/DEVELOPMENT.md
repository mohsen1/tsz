# Development Guide

This guide describes the clean-slate TSZ workspace. Historical compiler source
and architecture live only in Git history.

## Setup

```bash
git clone https://github.com/tsz-org/tsz.git
cd tsz
./scripts/setup/setup.sh
```

Setup initializes the pinned TypeScript checkout and installs Git hooks. The
checkout supplies the TypeScript 7.0.2 oracle, test corpus, and library files;
do not edit or commit changes inside it.

Before creating a worktree or starting a heavy command, use the repository's
intake and disk guard:

```bash
scripts/setup/disk-worktree-guard.sh
git worktree list
```

## Workspace Shape

The root workspace has three packages:

```text
crates/
├── tsz-core/       compiler modules and stable service facade
├── tsz-cli/        native CLI, server, LSP, and try-tsz adapters
└── conformance/    external-process TypeScript oracle harness
```

Within `tsz-core`, the intended ownership is:

```text
syntax -> program -> checker -> emit
             \          /
              service
```

- `syntax` owns scanning, immutable syntax, and parser recovery.
- `program` owns normalized sources, options, root order, declarations, and
  project facts.
- checker/semantic modules own binding, type construction, inference,
  relations, flow, queries, and structured failures.
- `emit` transforms and prints syntax; declaration emit consumes explicit
  checked summaries.
- `service` is the only public compiler, project, and language-service facade.

The CLI package adapts the service API. It must not grow an alternate compiler
pipeline. The conformance harness communicates through native processes and
must not depend on compiler internals.

Do not add a package for a phase merely because the phase exists. Package
splits require evidence. Browser/WASM bindings return in R4, after the service
API is stable.

## Build And Run

Fast development checks:

```bash
cargo check --workspace --all-targets
cargo build -p tsz-cli
cargo run -p tsz-cli --bin tsz -- --help
```

Build every native process adapter:

```bash
cargo build -p tsz-cli \
  --bin tsz --bin tsz-server --bin tsz-lsp --bin try-tsz
```

Representative performance measurements use an immutable optimized binary,
normally the `dist` profile. Do not time debug builds or rebuild inside a timed
sample.

## Tests

Use nextest for Rust tests:

```bash
cargo nextest run -p tsz-core
cargo nextest run -p tsz-cli
cargo nextest run --workspace
```

The rewrite tests are intentionally explicit targets because retained legacy
tests are a disabled porting corpus:

```bash
cargo nextest run -p tsz-core --test rewrite_foundation
cargo nextest run -p tsz-cli --test rewrite_process_contract
```

The strict local gate is:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
python3 scripts/arch/arch_guard.py
```

Use a focused retained harness to prove it can launch the replacement binary:

```bash
./scripts/conformance/conformance.sh run --filter '<case>' --max 1 --workers 1
./scripts/emit/run.sh --filter='<case>' --max=1
./scripts/fourslash/run-fourslash.sh --filter='<case>' --max=1 --sequential
```

Broad conformance, emit, fourslash, and project results are observations during
R0/R1. Report unsupported and crashed cases; do not weaken a declared seed
capability or relabel the frozen legacy score.

Never run two conformance commands concurrently in one worktree. Use
`scripts/safe-run.sh` for long or memory-heavy commands.

## Oracle-First Development

For a new behavior family:

1. Minimize an input and run it with the pinned TypeScript 7 oracle.
2. Record exact diagnostic code, normalized span, message chain, exit status,
   and emitted text as applicable.
3. Identify the owning TypeScript operation and its ordering constraints.
4. Port the structural behavior to the corresponding `tsz-core` module.
5. Add renamed, nested/wrapped, generic and concrete, positive, and fallback
   cases where they apply.
6. Verify repeated and reversed-root-order fingerprints.

State the rule as:

```text
When <structural condition>, TypeScript 7 does X; TSZ does X through <module/API>.
```

Type handles are checker-session local. Deferred types stay symbolic until the
owning operation requires a view, and semantic work exposes `Complete`,
`Deferred`, `Cycle`, or `Limit` rather than turning incomplete work into a
type. See [`architecture/RESET.md`](architecture/RESET.md).

## Diagnostics, Logs, And Artifacts

Use tracing instead of print debugging:

```bash
TSZ_LOG=debug TSZ_LOG_FORMAT=tree \
  cargo run -p tsz-cli --bin tsz -- path/to/file.ts
```

Conformance, emit, and fourslash scripts can write machine-readable artifacts.
Keep these result classes separate:

- the frozen legacy checkpoint;
- exact rewrite capability tests;
- full-corpus rewrite observations.

A narrow pass is evidence for its declared capability only.

## Hooks And Git Hygiene

Run setup again to reinstall the hooks:

```bash
./scripts/setup/setup.sh
```

Use `TSZ_SKIP_HOOKS=1` only for a deliberate emergency. Never commit changes to
the TypeScript submodule, generated build output, or unrelated files. Follow
[`CONTRIBUTING.md`](CONTRIBUTING.md) for PR verification and provenance.
