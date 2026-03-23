# Contributing to tsz

Thank you for your interest in contributing to tsz, a TypeScript compiler written in Rust.

## Quick Start

```bash
git clone https://github.com/mohsen1/tsz.git
cd tsz
./scripts/setup/setup.sh   # installs hooks, initializes TypeScript submodule
cargo build                 # verify everything compiles
cargo test -p tsz-checker -p tsz-solver  # run core tests
```

See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for the full development guide.

## How tsz Works

tsz follows a pipeline architecture where each stage has strict ownership boundaries:

```
source -> scanner -> parser -> binder -> checker <-> solver -> emitter
```

| Stage | Crate | Owns |
|-------|-------|------|
| Scanner | `tsz-scanner` | Tokenization, string interning |
| Parser | `tsz-parser` | Syntax-only AST construction |
| Binder | `tsz-binder` | Symbols, scopes, control-flow graph |
| Checker | `tsz-checker` | AST traversal, diagnostics orchestration |
| Solver | `tsz-solver` | All type relations, inference, evaluation |
| Emitter | `tsz-emitter` | JS and declaration output |

The most important rule: **if code computes type semantics, it belongs in the Solver.** The Checker is thin orchestration — it asks questions, the Solver answers them.

See [docs/architecture/BOUNDARIES.md](docs/architecture/BOUNDARIES.md) for the full boundary model.

## What to Work On

### Conformance Tests

The primary measure of progress is conformance with `tsc`. Each test compares tsz's diagnostics against TypeScript's expected output.

To find good first issues, use the offline analysis tools:

```bash
# Find tests where we emit 1 extra error (false positives — usually simpler to fix)
python3 scripts/conformance/query-conformance.py --one-extra

# Find tests closest to passing
python3 scripts/conformance/query-conformance.py --close 1

# Deep-dive a specific error code
python3 scripts/conformance/query-conformance.py --code TS2322
```

### Workflow for Conformance Fixes

1. **Research** — use offline analysis tools (zero CPU cost)
2. **Pick one test** — read the TypeScript source, understand the expected behavior
3. **Understand the root cause** — read the relevant checker/solver code
4. **Fix the root cause** — not a symptom. Follow architecture rules
5. **Verify** — run the specific test, check broader area for regressions
6. **Run unit tests** — `cargo test -p tsz-checker -p tsz-solver`

```bash
# Build the conformance runner
cargo build --profile dist-fast -p tsz-conformance

# Run a specific test
.target/dist-fast/tsz-conformance --filter "testName" --verbose \
  --cache-file scripts/conformance/tsc-cache-full.json

# Check for regressions in the same area
.target/dist-fast/tsz-conformance --filter "relatedArea" \
  --cache-file scripts/conformance/tsc-cache-full.json
```

### Architecture Contributions

Before making changes, review:
- [docs/architecture/CONTRIBUTION_CHECKLIST.md](docs/architecture/CONTRIBUTION_CHECKLIST.md)
- [docs/architecture/NORTH_STAR.md](docs/architecture/NORTH_STAR.md)

Key questions for every semantic PR:
1. Is this `WHAT` (type algorithm → Solver) or `WHERE` (orchestration → Checker)?
2. Does it route through canonical query boundaries?
3. Does it preserve `DefId`-first resolution?

## Code Style

- Run `cargo fmt` before committing (hooks auto-fix)
- `cargo clippy` with `-D warnings` must pass
- Checker files should stay under ~2000 LOC
- Prefer dedicated files per major concern
- Use visitor helpers for type traversal — avoid repeated `TypeKey` matching

## Pre-commit Hooks

Hooks run automatically and check:
- Formatting (`cargo fmt`)
- Linting (`cargo clippy` with deny warnings)
- Architecture boundaries
- WASM compatibility
- Unit tests for affected crates

To skip hooks in emergencies: `TSZ_SKIP_HOOKS=1 git commit -m "message"`

## Getting Help

- Open an issue for bugs or questions
- Check existing docs in the `docs/` directory
- The conformance analysis tools can help identify good areas to contribute
