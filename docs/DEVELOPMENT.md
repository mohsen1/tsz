# Development Guide

This guide covers setting up and working with the tsz codebase.

## Getting Started

```bash
# Clone the repository
git clone https://github.com/mohsen1/tsz.git
cd tsz

# Run the setup script (installs git hooks, initializes submodules)
./scripts/setup/setup.sh
```

The setup script initializes the TypeScript submodule (pinned to a specific commit for conformance tests) and installs pre-commit hooks.

## Git Hooks

Pre-commit hooks run automatically on every commit. They keep local commits
cheap and enforce:
- `cargo fmt` — format code (auto-fixes and re-stages)
- TypeScript submodule guard — prevents accidental submodule edits

Build, lint, unit tests, WASM, conformance, emit, and fourslash run in CI.
Draft PRs run the light suite: lint, dist-fast build, and unit tests. Marking
a PR ready for review runs the heavy suites: WASM, conformance, emit,
fourslash, and snapshot gates.

To manually install hooks:
```bash
./scripts/setup/setup.sh
```

To skip hooks for a single commit (use sparingly):
```bash
TSZ_SKIP_HOOKS=1 git commit -m "message"
```

Environment variables for hook control:
- `TSZ_SKIP_HOOKS=1` — skip all pre-commit checks

## Project Structure

tsz is a Cargo workspace with each pipeline stage in its own crate:

```
tsz/
├── crates/
│   ├── tsz-common/        # Shared types, IDs, diagnostic codes
│   ├── tsz-scanner/       # Lexer/tokenizer, string interning
│   ├── tsz-parser/        # Syntax-only AST construction
│   ├── tsz-binder/        # Symbols, scopes, control-flow graph
│   ├── tsz-solver/        # All type relations, inference, evaluation
│   ├── tsz-checker/       # AST walk, diagnostics, delegates to solver
│   ├── tsz-lowering/      # AST transforms (downlevel emit)
│   ├── tsz-emitter/       # JS/declaration output
│   ├── tsz-lsp/           # Language server protocol
│   ├── tsz-cli/           # CLI binary (tsz command)
│   ├── tsz-core/          # Integration crate, root tests
│   ├── tsz-wasm/          # WASM target bindings
│   ├── tsz-website/       # Website/playground
│   └── conformance/       # Conformance test runner binary
├── TypeScript/             # TypeScript submodule (test source, read-only)
├── docs/                   # Documentation
│   ├── architecture/       # Architecture decisions and boundaries
│   ├── plan/              # Roadmaps and planning docs
│   ├── specs/             # TypeScript behavior specifications
│   └── site/              # Website content
├── scripts/
│   ├── conformance/       # Conformance test runner and analysis tools
│   ├── setup/             # Setup and installation scripts
│   ├── arch/              # Architecture boundary checking
│   └── bench/             # Benchmarking scripts
└── .claude/               # AI assistant configuration
```

### Pipeline Architecture

```
scanner → parser → binder → checker → solver → emitter
                                ↕
                          (query boundary)
```

- **Scanner**: Lexes source into tokens, interns strings to `Atom`
- **Parser**: Builds syntax-only AST in `NodeArena`
- **Binder**: Creates symbols, scopes, and control-flow graph (no type computation)
- **Checker**: Walks AST, tracks diagnostics, delegates type questions to Solver
- **Solver**: Owns all type relations, evaluation, inference, instantiation, narrowing
- **Emitter**: Produces JS/declaration output from checked AST

Key rule: if code computes type semantics, it belongs in the Solver. The Checker is thin orchestration only.

## Running Tests

CI is the default place for broad verification. Use local commands when they
answer a specific debugging question, and prefer narrow filters over full
suites on a development machine.

### Unit Tests

```bash
# Install nextest if you need targeted local unit feedback
cargo install cargo-nextest

# Run a specific test while debugging
cargo nextest run -p tsz-checker --lib <test-name>
```

### Conformance Tests

Conformance tests compare tsz diagnostics against the official TypeScript compiler (`tsc`).

```bash
# Run one filtered test while debugging
./scripts/conformance/conformance.sh run --filter "testName" --verbose

# Full conformance runs in CI when a PR is marked ready for review
```

### Conformance Analysis (Offline)

Analysis tools work from pre-computed snapshot files — no CPU cost:

```bash
# Overview of conformance status
python3 scripts/conformance/query-conformance.py

# Root-cause campaign recommendations
python3 scripts/conformance/query-conformance.py --campaigns

# Tests fixable by removing 1 extra diagnostic
python3 scripts/conformance/query-conformance.py --one-extra

# Tests closest to passing (diff <= 2)
python3 scripts/conformance/query-conformance.py --close 2

# Deep-dive a specific error code
python3 scripts/conformance/query-conformance.py --code TS2322

# Snapshot refreshes are normally produced by CI/full verification batches
```

Snapshot files:
- `scripts/conformance/conformance-snapshot.json` — high-level aggregates
- `scripts/conformance/conformance-detail.json` — per-test failure data
- `scripts/conformance/tsc-cache-full.json` — tsc expected diagnostics for every test

## Building

### Native Binary

```bash
# Use CI for broad build verification.
# Build locally only when debugging a build-specific failure.
cargo build -p tsz-cli
```

### WASM Build

```bash
# CI checks WASM on ready-for-review PRs.
# Run locally only when debugging a WASM-specific failure.
cargo check -p tsz-wasm --target wasm32-unknown-unknown
```

## Architecture Rules

These are enforced by code review and CI:

1. **Solver owns type semantics** — if code computes type relations, evaluation, or inference, it goes in `tsz-solver`
2. **Checker is thin orchestration** — reads AST/symbols/flow, asks Solver for answers, tracks diagnostics
3. **No cross-layer imports** — Binder cannot import Solver, Emitter cannot import Checker internals
4. **Single type universe** — one `TypeId` space via the Solver's interner
5. **DefId-first resolution** — semantic references use `TypeData::Lazy(DefId)`, resolved through `TypeEnvironment`

See [`BOUNDARIES.md`](architecture/BOUNDARIES.md) and [`NORTH_STAR.md`](architecture/NORTH_STAR.md) for details.

## Memory-Guarded Execution

If you must run long-running or memory-intensive commands locally, wrap them
with the memory guard:

```bash
scripts/safe-run.sh --limit 8192 -- cargo build --release
```

This monitors RSS and kills the process if it exceeds the limit (default: 75% of system RAM).

## Tips

- Pre-commit hooks only format staged Rust changes; broad verification belongs in CI
- Use `cargo check -p tsz-checker` for fast feedback during development
- The TypeScript submodule is read-only — never commit changes to it
- Conformance snapshot files are generated artifacts — update them with `conformance.sh snapshot`
- Run `cargo fmt` before committing (hooks auto-fix but it's faster to do it yourself)
