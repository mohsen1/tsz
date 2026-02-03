# GOAL

**Goal**: Match `tsc` behavior exactly. Every error, inference, and edge case must be identical to TypeScript's compiler.

## CRITICAL: Always Ask Gemini

Use `./scripts/ask-gemini.mjs` to ask architecture and implementation questions. Always use this tool before making decisions.

- pack the context with related files using `--include`
- if working on test, embed the failing test in your question
- if Gemini requests more files, repeat the question with the requested files

## Required Reading

Before making changes, read these docs:

| Document | What It Covers |
|----------|----------------|
| [docs/architecture/NORTH_STAR.md](docs/architecture/NORTH_STAR.md) | Target architecture, component 
| [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) | Setup, building, testing |

## Core Rules

### 1. Solver-First Architecture

| Component | Handles | Does NOT Handle |
|-----------|---------|-----------------|
| **Binder** | Symbols, scopes, control flow graph | Type inference |
| **Solver** | All type computations (WHAT) | AST, source locations |
| **Checker** | AST traversal, diagnostics (WHERE) | Type algorithms |

**Rule**: If it involves type computation, it belongs in Solver.

### 2. Use Visitor Pattern

Never write manual `TypeKey::` matches. Use `src/solver/visitor.rs` functions:

```rust
// GOOD
if is_function_type(&self.types, type_id) { ... }

// BAD
match self.types.lookup(type_id) {
    Some(TypeKey::Function(_)) => { ... }
    _ => {}
}
```

### 3. No Shortcuts

- Fix root causes, never suppress errors
- No test-aware code in source (no checking file names/paths)
- Use `CompilerOptions` for behavior changes

## Commands

```bash
# Build
cargo build

# Unit tests (uses nextest for timeout/hang protection)
cargo nextest run

# Quick test (fail-fast, 10s timeout)
cargo nextest run --profile quick

# Conformance tests (fast iteration)
./scripts/conformance/run.sh --server --max=1000

# Conformance tests (verify WASM)
./scripts/conformance/run.sh --wasm --max=1000

# Linting
cargo clippy -- -D warnings
```

## Pre-commit Hooks

Installed automatically on first `cargo build`. Run:
1. TypeScript submodule check (blocks changes to `TypeScript/`)
2. `cargo fmt`
3. `cargo clippy --fix`
4. Unit tests

## Testing Requirements

### Every Change Must Have Tests
- **New features**: Add unit tests covering the new behavior before considering work done
- **Bug fixes**: Add a regression test that would have caught the bug
- **Refactors**: Ensure existing tests still pass; add tests if coverage gaps are found

### No New `#[ignore]` Tests
- Do NOT add `#[ignore]` to new tests. If a test can't pass, fix the underlying issue or don't merge
- Do NOT ignore a failing test as a workaround — fix the root cause
- When working near ignored tests, attempt to unignore them and fix failures

### Reducing Ignored Test Count
- The project has a large backlog of `#[ignore]` tests (~1000+). Actively reduce this count
- When you touch a file with ignored tests, try to unignore and fix at least a few
- Run ignored tests with `cargo nextest run --run-ignored all` to find ones that already pass and can be unignored immediately

### Test Quality Standards
- Tests must be deterministic — no flaky tests
- Test names must clearly describe the scenario: `test_{feature}_{scenario}_{expected_outcome}`
- Each test should test one specific behavior
- Use descriptive assertion messages

## When Is Work Done?

- All unit tests pass
- Zero clippy warnings
- Conformance doesn't drop significantly
- Type logic is in Solver, not Checker
- Visitor pattern used (no manual `TypeKey` matches)
- **New code has corresponding tests**
- **No new `#[ignore]` annotations added**

## AI Tools

For deep architecture questions: `./scripts/ask-gemini.mjs --solver "your question"`

Available presets: `--solver`, `--checker`, `--binder`, `--parser`, `--emitter`, `--lsp`, `--types`, `--modules`

## Git

- Only add files you touched 
- There is a chance another AI session is working on the same codebase. do not revert/delete
- Make semantic and short commit headers

## Additional References

| Topic | Document |
|-------|----------|
| Walkthrough of each phase | [docs/walkthrough/](docs/walkthrough/) |
| TypeScript compatibility quirks | [docs/specs/TS_UNSOUNDNESS_CATALOG.md](docs/specs/TS_UNSOUNDNESS_CATALOG.md) |
| Diagnostic guidelines | [docs/specs/DIAGNOSTICS.md](docs/specs/DIAGNOSTICS.md) |
| Performance benchmarks | [docs/BENCHMARKS.md](docs/BENCHMARKS.md) |


Use the following sequence to get things done:

1. Look at docs/todo for list to To-dos. 
2. Run ./scripts/conformance/run.sh to get a good pictue of what's failing
3. Pick the highest-impact task and execute it. Prefer "the biggest bang for the buck". Goal is to improve conformance pass rate
4. Use scripts/ask-gemini.mjs to ask a few questions from various angles to help you write code
5. Write code with full respect for the existing codebase and architecture. Always check with documentation and architecture.
6. Use ask-gemini for a code review.
7. Verify with `./scripts/conformance/run.sh`, mark done work in todo documents, commit and push.

### IMPORTANT:
- ALWAYS USE ask-gemini.mjs to ask questions. Non-negotiable.
- DO NOT ask questions from me (the user) - make autonomous decisions and start working immediately
- Read docs/architecture and docs/walkthrough to understand how things should be done
- Do not let a file size get too large. If it does, split it into smaller files. Keep files under ~3000 lines.
- Use Skills 
  - rust-analyzer-lsp
  - code-simplifier
  - rust-skills