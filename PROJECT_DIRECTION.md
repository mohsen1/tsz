# Project Zang

TypeScript compiler rewritten in Rust, compiled to WebAssembly. Goal: TSC compatibility with better performance.

## Current Test Status

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

## Gaps / Risks

* **Conformance gap**: Target is 50%+. Run tests to get current metrics.
* **WASM stability**: Previous runs showed 121 worker crashes in WASM mode - use native for faster, more stable testing.

* **Transform pipeline debt**: `src/transforms/` mixes AST manipulation with string emission. Should produce lowered AST, then printer emits strings.

* **Compat layer completeness**: `compat` module needs audit against `TS_UNSOUNDNESS_CATALOG.md` to ensure all rules (weak types, template literal limits, rest bivariance, exactOptionalPropertyTypes) are wired and option-driven.

***

## Priority List

### 1. Unblock Testing

Fix WASM initialization crash path to enable conformance runs. Unit tests require Docker (`./scripts/test.sh` - enforced at compile time).

### 2. Improve Conformance Test Pass Rate

**Target: 50%+**

Run tests to get current metrics and identify top issues:
```bash
./conformance/run-conformance.sh --all
```

Focus areas based on latest run (1000 tests):
- Library loading (TS2318: 860x missing, TS2583: 130x missing)
- Name/module resolution (TS2304: 130x missing, TS2307)
- Type assignability (check for extra errors)

### 3. Expand Test Coverage

**Current: 60% of TypeScript/tests (12,093 files)**

**Recommended additions:**
- **Add `projects/` category** (175 files) - HIGH VALUE
  - Tests module resolution, compiler options, multi-file projects
  - Validates real-world project scenarios
  - Estimated effort: 2-4 hours
- **Evaluate `fourslash/` subset** (selective from 6,563 files)
  - Tests complex multi-file language features
  - Skip IDE-specific tests (completion, navigation)
  - Estimated effort: 8-16 hours for selective integration

**See research findings below for detailed analysis.**

### 4. Fix Transform Pipeline

`src/transforms/` mixes AST manipulation with string emission. Transforms should produce a lowered AST, then the printer should emit strings.

### 5. Compat Layer Audit

Audit `compat` module against `TS_UNSOUNDNESS_CATALOG.md` to ensure all rules are wired and option-driven (weak types, template literal limits, rest bivariance, exactOptionalPropertyTypes).

### 6. Code Hygiene

* Remove `#![allow(dead_code)]` and fix unused code
* Add proper tracing infrastructure (replace print statements)
* Clean up Clippy ignores in `clippy.toml`
* Test-awareness cleanup: sweep checker/binder for path heuristics or test-specific workarounds (per AGENTS rules)

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
