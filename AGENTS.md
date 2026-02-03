# GOAL

**Goal**: Match `tsc` behavior exactly. Every error, inference, and edge case must be identical to TypeScript's compiler.

## CRITICAL: Always Ask Gemini

Use `./scripts/ask-gemini.mjs` to ask architecture and implementation questions. Always use this tool before making decisions.

- pack the context with related files using `--include`
- if working on test, embed the failing test in your question
- if Gemini requests more files, repeat the question with the requested files

## CRITICAL: Keep working, don't get blocked or stop

Your goal is to keep making progress. If you get stuck, do not stop. Instead:
- Ask Gemini for help
- Review related code and docs for insights
- Make incremental improvements or refactors that move the project forward
- Document your questions and findings for future reference

## Use Skills
- Use reasoning, planning, and coding skills
- Use code analysis skills to understand existing code
- Use debugging skills to trace and fix issues
- Use testing skills to write effective tests
- Use documentation skills to read and write docs

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

# Conformance tests 
./scripts/conformance/run.sh --server --max=1000

# Linting
cargo clippy -- -D warnings

# Format code
cargo fmt
```


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
- Zero clippy warnings and `cargo fmt` compliance
- Conformance doesn't drop significantly
- **New code has corresponding tests**
- **No new `#[ignore]` annotations added**

## Git

- Commit frequently with clear messages
- Push branches to remote regularly and rebase from main before and after each comm`
- Only add files you touched, do not `git add -A`
- There is a chance another AI session is working on the same codebase. do not revert/delete
- Make semantic and short commit headers

## Additional References

| Topic | Document |
|-------|----------|
| Walkthrough of each phase | [docs/walkthrough/](docs/walkthrough/) |
| TypeScript compatibility quirks | [docs/specs/TS_UNSOUNDNESS_CATALOG.md](docs/specs/TS_UNSOUNDNESS_CATALOG.md) |
| Diagnostic guidelines | [docs/specs/DIAGNOSTICS.md](docs/specs/DIAGNOSTICS.md) |
| Performance benchmarks | [docs/BENCHMARKS.md](docs/BENCHMARKS.md) |
