# Development Guide

This guide covers setting up and working with the tsz codebase.

## Getting Started

```bash
# Clone the repository
git clone https://github.com/mohsen1/tsz.git
cd tsz

# Run the setup script (installs git hooks, initializes submodules)
./scripts/setup/setup.sh

# Build the project
cargo build
```

The setup script initializes the TypeScript submodule (pinned to a specific commit for conformance tests) and installs pre-commit hooks.

## Git Hooks

Pre-commit hooks run automatically on every commit. They enforce:
- `cargo fmt` — format code (auto-fixes and re-stages)
- `cargo clippy` — lint with `-D warnings` on directly changed crates by default
- Architecture boundary checks — prevents cross-layer imports when checker or architecture files changed
- Unit tests — runs tests for directly changed crates by default

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
- `TSZ_SKIP_BENCH=1` — skip microbenchmark regression check
- `TSZ_PRECOMMIT_FULL=1` — run the slower strict pre-commit path
- `TSZ_PRECOMMIT_TEST_SCOPE=affected` — test changed crates plus transitive dependents
- `TSZ_PRECOMMIT_TEST_SCOPE=all` or `TSZ_TEST_ALL=1` — test all workspace crates
- `TSZ_PRECOMMIT_CI_PARITY=1` — run full CI parity lint commands
- `TSZ_PRECOMMIT_WASM=1` — run the wasm32 lint gate
- `TSZ_PRECOMMIT_CLEAN=1` — run target cleanup
- `TSZ_PRECOMMIT_RESET_TYPESCRIPT=1` — reset/init the TypeScript submodule before checking
- `TSZ_GIT_HOOK_RESET_TYPESCRIPT=1` — reset/init TypeScript after merge or rewrite hooks

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

### Unit Tests

```bash
# Run all unit tests
cargo test

# Run tests for specific crates
cargo test -p tsz-checker -p tsz-solver

# Using nextest (recommended for CI-like behavior)
cargo install cargo-nextest
cargo nextest run
cargo nextest run --profile precommit  # fast profile with timeouts
```

### Conformance Tests

Conformance tests compare tsz diagnostics against the official TypeScript compiler (`tsc`).

```bash
# Build the conformance runner (fast profile)
cargo build --profile dist-fast -p tsz-conformance

# Run all conformance tests
.target/dist-fast/tsz-conformance --cache-file scripts/conformance/tsc-cache-full.json

# Run filtered tests (fast, for development)
.target/dist-fast/tsz-conformance --filter "controlFlow" --cache-file scripts/conformance/tsc-cache-full.json

# Verbose output (shows expected vs actual diagnostics)
.target/dist-fast/tsz-conformance --filter "testName" --verbose --cache-file scripts/conformance/tsc-cache-full.json

# Wrap heavy runs with memory guard
scripts/safe-run.sh .target/dist-fast/tsz-conformance --cache-file scripts/conformance/tsc-cache-full.json
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

# Update snapshots after code changes
scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot
```

Snapshot files:
- `scripts/conformance/conformance-snapshot.json` — high-level aggregates
- `scripts/conformance/conformance-detail.json` — per-test failure data
- `scripts/conformance/tsc-cache-full.json` — tsc expected diagnostics for every test

## Building

### Native Binary

```bash
# Debug build
cargo build

# Fast optimized build (for conformance testing)
cargo build --profile dist-fast -p tsz-cli

# Release build
cargo build --release -p tsz-cli

# Run tsz on a file
.target/dist-fast/tsz-cli check myfile.ts
```

### WASM Build

```bash
# Check WASM compatibility
cargo check -p tsz-wasm --target wasm32-unknown-unknown

# Build WASM package
wasm-pack build crates/tsz-wasm --target web --out-dir pkg --no-opt
```

## Architecture Rules

These are enforced by pre-commit hooks and CI:

1. **Solver owns type semantics** — if code computes type relations, evaluation, or inference, it goes in `tsz-solver`
2. **Checker is thin orchestration** — reads AST/symbols/flow, asks Solver for answers, tracks diagnostics
3. **No cross-layer imports** — Binder cannot import Solver, Emitter cannot import Checker internals
4. **Single type universe** — one `TypeId` space via the Solver's interner
5. **DefId-first resolution** — semantic references use `TypeData::Lazy(DefId)`, resolved through `TypeEnvironment`

See `docs/architecture/BOUNDARIES.md` and `docs/architecture/NORTH_STAR.md` for details.

## Memory-Guarded Execution

All long-running or memory-intensive commands should be wrapped with the memory guard:

```bash
scripts/safe-run.sh cargo test
scripts/safe-run.sh ./scripts/conformance/conformance.sh run
scripts/safe-run.sh --limit 8192 -- cargo build --release
```

This monitors RSS and kills the process if it exceeds the limit (default: 75% of system RAM).

## Tips

- Pre-commit hooks check directly changed crates by default; use `TSZ_PRECOMMIT_TEST_SCOPE=affected` to include transitive dependents
- Use `cargo check -p tsz-checker` for fast feedback during development
- The TypeScript submodule is read-only — never commit changes to it
- Conformance snapshot files are generated artifacts — update them with `conformance.sh snapshot`
- Run `cargo fmt` before committing (hooks auto-fix but it's faster to do it yourself)
