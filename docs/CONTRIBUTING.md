# Contributing to tsz

Thank you for your interest in contributing to tsz, a TypeScript compiler written in Rust.

## Quick Start

```bash
git clone https://github.com/tsz-org/tsz.git
cd tsz
./scripts/setup/setup.sh   # installs hooks, initializes TypeScript submodule
```

Open a PR to run CI. Every open PR runs the full suite: lint, build, unit
tests, WASM, conformance, emit, fourslash, and snapshot gates. Path-based
skips still apply for docs-only or tooling-only changes.

When a ready PR's exact head has passed the PR-head gates (`CI Summary`,
and any review/body checks), the PR author
queues it with GitHub's native merge queue
(`gh pr merge <pr> --match-head-commit <sha>`). The native queue creates a
`merge_group` run that keeps the required queue summary check on the synthetic
merge before merging.

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

1. **Check active work** — inspect open PRs, recent merged PRs, and relevant issues before starting
2. **Claim the scope** — open a PR early; a GitHub issue is optional
3. **Research** — use offline analysis tools and existing tests before running heavy commands
4. **Understand the root cause** — read the relevant checker/solver code
5. **Fix the root cause** — not a symptom. Follow architecture rules
6. **Verify narrowly** — run only targeted local checks needed for debugging
7. **Push updates to the PR** — every push runs the full CI suite; do not wait idle
8. **Mark ready for review** — when the change is complete; CI already ran the full suite on every push
9. **Land your own PR** — after exact-head PR-head gates pass,
   queue the PR yourself with
   `gh pr merge <pr> --match-head-commit <sha>`; native queue admission and
   summary validation run through the `merge_group` CI event

Every PR body must include a `Goal: <green|fast|grow|hold>` line, a
`## Verification` section, and a `## Provenance` block with `Machine:`,
`Assistant:`, `Model:`, and `Effort:` lines reporting your actual runtime
values. Use the PR body for scope, invariants, findings, and verification.

These fields are a review convention, not an enforced gate: no `pr-body-gate`
job exists in `.github/workflows/`. Write them because reviewers and future
sessions read them, not because CI will stop you.

When adding or re-adding WIP state, leave a PR comment with the reason WIP
state changed, the current blocker or work, and the next action, signed with
a provenance line (e.g. `Machine: studio`). If the advisory WIP-state report
flags a missing comment, repair it by adding that comment; no code change is
required.

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
- No hand-authored code, test, script, or generated-code shard may exceed 2000
  physical lines. Split by concern before adding more code; do not add
  file-size ratchet exceptions or per-file ceilings.
- Prefer dedicated files per major concern
- Use visitor helpers for type traversal — avoid repeated `TypeKey` matching

## Pre-commit Hooks

Hooks run automatically and check:
- Formatting (`cargo fmt`)
- TypeScript submodule guard

Build, lint, unit, WASM, conformance, emit, and fourslash verification runs in
CI. Every open PR gets the full CI suite; path-based skips apply only to
docs-only or tooling-only changes.

To skip hooks in emergencies: `TSZ_SKIP_HOOKS=1 git commit -m "message"`

## Getting Help

- Open an issue for bugs or questions
- Check existing docs in the `docs/` directory
- The conformance analysis tools can help identify good areas to contribute
