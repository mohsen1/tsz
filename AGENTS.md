# Agent Rules for Project Zang

**Goal**: Match `tsc` behavior exactly. Every error, inference, and edge case must be identical to TypeScript's compiler.

## Required Reading

Before making changes, read these docs:

| Document | What It Covers |
|----------|----------------|
| [docs/architecture/NORTH_STAR.md](docs/architecture/NORTH_STAR.md) | Target architecture, component responsibilities, type system rules |
| [docs/architecture/MIGRATION_ROADMAP.md](docs/architecture/MIGRATION_ROADMAP.md) | Current state, migration phases, extraction patterns |
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

# Unit tests (use Docker wrapper)
./scripts/test.sh

# Conformance tests (fast iteration)
./conformance/run.sh --server --max=1000

# Conformance tests (verify WASM)
./conformance/run.sh --wasm --max=1000

# Linting
cargo clippy -- -D warnings
```

## Pre-commit Hooks

Installed automatically on first `cargo build`. Run:
1. TypeScript submodule check (blocks changes to `TypeScript/`)
2. `cargo fmt`
3. `cargo clippy --fix`
4. Unit tests

## When Is Work Done?

- All unit tests pass
- Zero clippy warnings
- Conformance doesn't drop significantly
- Type logic is in Solver, not Checker
- Visitor pattern used (no manual `TypeKey` matches)

## AI Tools

For deep architecture questions: `./scripts/ask-gemini.mjs --solver "your question"`

Available presets: `--solver`, `--checker`, `--binder`, `--parser`, `--emitter`, `--lsp`, `--types`, `--modules`

## Additional References

| Topic | Document |
|-------|----------|
| Walkthrough of each phase | [docs/walkthrough/](docs/walkthrough/) |
| TypeScript compatibility quirks | [docs/specs/TS_UNSOUNDNESS_CATALOG.md](docs/specs/TS_UNSOUNDNESS_CATALOG.md) |
| Diagnostic guidelines | [docs/specs/DIAGNOSTICS.md](docs/specs/DIAGNOSTICS.md) |
| Performance benchmarks | [docs/BENCHMARKS.md](docs/BENCHMARKS.md) |
