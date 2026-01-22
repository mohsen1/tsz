# Project Zang

TypeScript compiler rewritten in Rust, compiled to WebAssembly. Goal: TSC compatibility with better performance.

## Address code review comments

See `docs/CODE_REVIEW_efd539a62.md` to find and address outstanding code review comments.q

## Strategy Shift: Solver Foundation First

**We are prioritizing solver/checker completion over conformance parity.**

Chasing conformance percentages before the type system solver is solid is premature. A sound, complete solver is the foundation - conformance improvements will follow naturally from proper type system mechanics.

**Key principle:** Understand the type system deeply, implement correctly, then validate with conformance tests.

## Top Priority: Complete the Solver

The solver (`src/solver/`) is the heart of the type system. It must implement:
- **Semantic subtyping** (types as sets of values, set inclusion for subtyping)
- **Coinductive semantics** (for recursive types)
- **Structural typing** with proper canonicalization
- **Advanced TypeScript features** (conditionals, mapped types, template literals, inference)

**Status:** Core solver architecture is sound but incomplete. See `specs/SOLVER_ROADMAP.md` for comprehensive implementation plan.

## Secondary Priority: Test Coverage for Solver

Conformance tests are useful for validation, but should not drive implementation. Use them to:
- Validate solver correctness after implementation
- Catch edge cases in type system mechanics
- Ensure TypeScript compatibility behaviors are wired in compat layer

**Current Test Status:**

**Docker-isolated by default** for safety. Tests run in parallel across all CPU cores.

```bash
# Docker + Native Binary (default: fast, stable)
./conformance/run-conformance.sh --all

# Docker + WASM (slower, for WASM-specific testing)
./conformance/run-conformance.sh --wasm --all

# No Docker (unsafe: vulnerable to infinite loops/OOM)
./conformance/run-conformance.sh --no-sandbox --all
```

**Performance (1000 tests, 14 CPU cores):**
- Native: ~26 tests/sec (38.8s total)
- WASM: ~22 tests/sec (45.7s total)
- Native is ~18% faster but both run in parallel across all cores

**Test Coverage:**
- Currently testing: 12,093 files (60% of TypeScript/tests)
  - `conformance/`: 5,691 files
  - `compiler/`: 6,402 files
- Not testing: 7,975 files (40%)
  - `fourslash/`: 6,563 files (IDE features)
  - `projects/`: 175 files (module resolution - HIGH VALUE)
  - `transpile/`: 22 files (JS output)

***

## Lower Priority Items

These tasks are important but should be pursued **after** solver is solid:

### Code Hygiene

* Remove `#![allow(dead_code)]` and fix unused code
* Add proper tracing infrastructure (replace print statements)
* Clean up Clippy ignores in `clippy.toml`
* Test-awareness cleanup: sweep checker/binder for path heuristics or test-specific workarounds (per AGENTS rules)

### Transform Pipeline Fix

`src/transforms/` mixes AST manipulation with string emission. Transforms should produce a lowered AST, then the printer should emit strings.

### Compat Layer Audit

Audit `compat` module against `TS_UNSOUNDNESS_CATALOG.md` to ensure all rules are wired and option-driven (weak types, template literal limits, rest bivariance, exactOptionalPropertyTypes).

### Expand Test Coverage (After Solver is Solid)

**Recommended additions:**
- **Add `projects/` category** (175 files) - HIGH VALUE
  - Tests module resolution, compiler options, multi-file projects
  - Validates real-world project scenarios
  - Estimated effort: 2-4 hours
- **Evaluate `fourslash/` subset** (selective from 6,563 files)
  - Tests complex multi-file language features
  - Skip IDE-specific tests (completion, navigation)
  - Estimated effort: 8-16 hours for selective integration

***

## Test Coverage Research Findings

### TypeScript/tests Directory Structure

| Category | Files | Purpose | Status |
|----------|-------|---------|--------|
| `conformance/` | 5,691 | Language spec compliance (52 categories) | ✅ Testing |
| `compiler/` | 6,402 | Compiler behavior and API | ✅ Testing |
| `fourslash/` | 6,563 | IDE features (completion, navigation, refactor) | ⚠️ Not tested |
| `projects/` | 175 | Module resolution, project configuration | ⚠️ Not tested |
| `transpile/` | 22 | JavaScript output generation | ❌ Not relevant |

### Fourslash Tests Analysis

**Purpose:** Specialized framework for testing IDE/editor functionality

**Format:**
- Uses `////` markers and `@Filename` directives
- DSL with verification API (`goTo.marker()`, `verify.quickInfoAt()`)
- Supports multi-file test scenarios

**Capabilities:**
- Code completion, navigation, refactoring
- Incremental edit testing
- Complex multi-file language features
- Error diagnostics and suggestions

**Recommendation:** Selective integration
- ✅ Extract language feature tests (multi-file scenarios)
- ❌ Skip IDE-specific tests (completion, navigation)
- ⚠️ Requires implementing fourslash DSL parser
- Estimated effort: 8-16 hours

### Projects Tests Analysis

**Purpose:** Module resolution and project configuration validation

**Format:** JSON configuration + multi-file project directories

**Capabilities:**
- Circular import handling
- Reference path resolution (`/// <reference>`)
- Module resolution algorithms (Node16, NodeNext, bundler)
- Compiler options validation (rootDir, outDir, declarationDir)
- Source map generation
- Declaration file organization

**Recommendation:** HIGH VALUE - Add this category
- ✅ Tests critical real-world scenarios
- ✅ Validates module resolution (essential for TypeScript)
- ✅ Multi-file project testing
- ⚠️ Requires JSON parser and project setup
- Estimated effort: 2-4 hours

### Implementation Priority

1. **Add `projects/` category** (175 files)
   - High value, medium effort
   - Tests essential module resolution features
   - Complements existing conformance tests

2. **Evaluate fourslash subset** (selective from 6,563)
   - Medium-high value, high effort
   - Focus on multi-file language feature tests
   - Skip IDE/editor-specific tests

***

## Key Files

| File/Directory | Purpose | Lines |
|----------------|---------|-------|
| `src/checker/` | Type checker | ~44,000 total |
| `src/parser/state.rs` | Parser implementation | 10,770 |
| `src/parser/node.rs` | AST node definitions | ~5,500 |
| `src/binder.rs` | Symbol binding | 587 |
| `src/solver/` | Type resolution | 37 files |
| `src/transforms/` | JavaScript transforms | ~850K total (inc. tests) |

***

## Commands

```bash
cargo build                              # Build
cargo test --lib                         # Run all tests
cargo test --lib solver::                # Run specific module
wasm-pack build --target nodejs          # Build WASM

# Conformance tests (Docker-isolated by default)
./conformance/run-conformance.sh --max=500       # Run 500 tests
./conformance/run-conformance.sh --all           # Run all tests
./conformance/run-conformance.sh --native        # Use native binary (faster)
./conformance/run-conformance.sh --wasm          # Use WASM (slower)
./conformance/run-conformance.sh --no-sandbox    # No Docker (unsafe)
./conformance/run-conformance.sh --workers=N     # Set worker count
```

***

## Git Hooks

Pre-commit hooks are available to enforce code quality before committing:

```bash
# Install hooks (run once after cloning)
./scripts/install-hooks.sh

# Now hooks will run automatically before each commit
```

**Pre-commit hook checks:**
1. `cargo fmt` - Applies code formatting
2. `cargo clippy --fix` - Auto-fixes linter warnings (fails if unfixable)
3. `cargo build --lib --bins --benches` - Checks for build warnings
4. `cargo test --lib` - Runs unit tests (in Docker)

**Fix formatting issues:**
```bash
# Format code
cargo fmt

# Then commit again
git commit -m "your message"
```

**Fix clippy issues:**
```bash
# Auto-fix clippy warnings
cargo clippy --all-targets --fix --allow-dirty --allow-staged
```

***

## Rules

* All commits must pass unit tests
* No test-aware code in source
* Fix root causes, not symptoms

| Don't | Do Instead |
|-------|------------|
| Check file names in checker | Fix the underlying logic |
| Suppress errors for specific tests | Implement correct behavior |

***

## Merge Criteria

1. `cargo build` passes
2. `cargo test` passes
3. Tests run in < 30 seconds
