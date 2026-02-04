# GOAL

**Goal**: Match `tsc` behavior exactly. Every error, inference, and edge case must be identical to TypeScript's compiler.

## CRITICAL: Check Session Coordination

Before starting work, check [docs/sessions/](docs/sessions/) to understand what other sessions are working on. Your session is determined by your directory name (tsz-1, tsz-2, tsz-3, tsz-4).

1. Make sure you have the latest session files from the repo's origin remote
2. Read all session files to avoid duplicate/conflicting work
3. When starting work, update your session file immediately with the current task, commit and push so others see
4. When finishing, move to history and note outcome

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
- Use tsz-gemini skill for 
  - codebase questions
  - architecture understanding
  - code reviews
  - implementation strategies
  - fixing bugs and failing tests


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
./scripts/conformance.sh --max=1000

# Linting
cargo clippy -- -D warnings

# Format code
cargo fmt
```

## Disk Space Protection

**CRITICAL**: The `.target` directory can grow to multi-GB sizes during builds and testing. Protect the disk from filling up:

### Check Disk Usage Before Builds

```bash
# Check target directory size
du -sh target/

# Check available disk space
df -h .
```

### Regular Cleanup Commands

```bash
# Clean all build artifacts (most aggressive)
cargo clean

# Clean only release artifacts (keep debug builds for faster iteration)
cargo clean --release

# Clean specific crate's artifacts
cargo clean -p tsz

# Remove old test binaries (nextest stores these)
rm -rf target/nextest
```

### Recommended Cleanup Schedule

1. **Before starting work**: Run `cargo clean --release` if disk space < 10GB
2. **After finishing work**: Run `cargo clean --release` to free up space
3. **Before large test runs**: Check `df -h .` and clean if needed
4. **Weekly**: Full `cargo clean` if working heavily

### Automatic Cleanup Script

Use the provided cleanup script for safe automatic cleanup:

```bash
./scripts/clean.sh --safe    # Remove release artifacts, keep debug
./scripts/clean.sh --full    # Full clean (cargo clean)
./scripts/clean.sh --check   # Check sizes before cleaning
```

### Warning Signs

- Target directory > 2GB: Consider `cargo clean --release`
- Available disk space < 5GB: **Must clean before building**
- Build failures with "No space left on device": Run `cargo clean` immediately


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

## CRITICAL: Git Workflow

- Make sure pre-commit hooks are installed (`./scripts/install-git-hooks.sh`)
- Commit frequently with clear messages
- Push branches to remote regularly and rebase from main before and after each commit
- Only add files you touched, do not `git add -A`
- Make semantic and short commit headers
- Important: When syncing, also push to remote