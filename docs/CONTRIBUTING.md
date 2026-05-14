# Contributing to tsz

Thank you for your interest in contributing to tsz, a TypeScript compiler written in Rust.

## Quick Start

```bash
git clone https://github.com/mohsen1/tsz.git
cd tsz
./scripts/setup/setup.sh   # installs hooks, initializes TypeScript submodule
```

Open a draft PR to run the light CI suite: lint, dist-fast build, and unit
tests. Mark the PR ready for review when it should run the heavy suites:
WASM, conformance, emit, fourslash, and snapshot gates.

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

### Conformance Maintenance

tsz is expected to stay at 100% conformance with `tsc`. Each test compares
tsz's diagnostics against TypeScript's expected output.

Use the offline analysis tools to inspect the current snapshot:

```bash
python3 scripts/conformance/query-conformance.py --dashboard
```

### Workflow For Semantic Changes

1. **Check active work** — inspect open issues, draft PRs, and `WIP` labels/titles before starting
2. **Claim the scope** — create or update a GitHub issue, mark it `WIP`, and keep new findings there
3. **Research** — use offline analysis tools and existing tests before running heavy commands
4. **Understand the root cause** — read the relevant checker/solver code
5. **Fix the root cause** — not a symptom. Follow architecture rules
6. **Verify narrowly** — run only targeted local checks needed for debugging
7. **Push a draft PR** — let CI run build, lint, and unit tests; do not wait idle
8. **Mark ready for review** — triggers conformance, emit, fourslash, WASM, and snapshot gates

```bash
# Run a specific test when debugging the root cause
./scripts/conformance/conformance.sh run --filter "testName" --verbose
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
- `cargo clippy` with `-D warnings` must pass in CI
- Checker files should stay under ~2000 LOC
- Prefer dedicated files per major concern
- Use visitor helpers for type traversal — avoid repeated `TypeKey` matching

## Pre-commit Hooks

Hooks run automatically and check:
- Formatting (`cargo fmt`)
- TypeScript submodule guard

Build, lint, unit, WASM, conformance, emit, and fourslash verification runs in
CI. Draft PRs get the light CI suite; ready-for-review PRs get the full suite.

To skip hooks in emergencies: `TSZ_SKIP_HOOKS=1 git commit -m "message"`

## Getting Help

- Open an issue for bugs or questions
- Check existing docs in the `docs/` directory
- The conformance analysis tools can help identify good areas to contribute
