# Project Zang

TypeScript compiler rewritten in Rust, compiled to WebAssembly. Goal: TSC compatibility with better performance.


## Priority List (Current Focus)

Those things can be done in parallel, but this is the order of importance.

### 0. Address Critical Issues

This is a harsh, architectural-level code review of Project Zang. The goal is to build a "performance-first" TypeScript compiler, yet the codebase currently exhibits several anti-patterns regarding memory management, concurrency, and architectural consistency that undermine that goal.

#### 1. Fundamental Memory Architecture Violations

**The "Zero-Copy" Lie**
The documentation and comments claim "ZERO-COPY OPTIMIZATION" (`src/scanner_impl.rs`), but the implementation contradicts this.

*   **Scanner Owns String**: `ScannerState` holds `source: String`. This forces an allocation for every file scanned. A high-performance compiler must operate on `&str` or `&[u8]` owned by a central source manager (SourceFile), not cloned into the scanner.
*   **Excessive Cloning**:
    *   `ThinParserState::new` takes `String`.
    *   `src/interner.rs`: `intern` takes `&str` and calls `to_string()`. While interning *should* own the string, the lookup path shouldn't force allocation if the string is already interned.
    *   `ThinPrinter`: `take_output` returns `String`.
*   **AST Duality**: You have `src/parser/ast/node.rs` (Fat Nodes using Box/enum) AND `src/parser/thin_node.rs` (SoA ThinNodes). While the transition to ThinNodes is the right move for cache locality, the codebase currently carries the dead weight of the old AST structure. The `Node` enum in `ast/node.rs` is huge (200+ bytes). If this is legacy, delete it. If it's for serialization, it's too expensive.

#### 2. The Concurrency Bottleneck

The solver architecture in `src/solver/intern.rs` is a concurrency trap waiting to happen.

*   **RwLock Hell**: `TypeInterner` wraps every single internal vector (shards, type lists, tuple lists, object shapes, etc.) in an `RwLock`.
    *   In a parallel compilation environment (which `src/parallel.rs` attempts to implement), acquiring a write lock on `type_lists` stops *every other thread* from interning a type list.
    *   **Fix**: Use a sharded `DashMap` or a lock-free append-only arena (like `bumpalo` with interior mutability) for interning. The current fine-grained locking on resizeable `Vec`s is catastrophic for scaling.
*   **Rayon Misuse**: `src/parallel.rs` creates a new `ThinCheckerState` inside the parallel iterator. However, `ThinCheckerState` seems to rely on the global `TypeInterner`. Since the interner locks on writes, your parallel type checking will serialize on type creation.

#### 3. Parser & Error Recovery Logic

*   **Gaming the Error Budget**:
    In `src/thin_parser.rs`, `parse_statement` resets the error budgets:
    ```rust
    self.ts1109_statement_budget = 5;
    self.ts1005_statement_budget = 2;
    ```
    This is a "whack-a-mole" strategy. Resetting budgets per statement is arbitrary. If a file is garbage, the parser should bail or synchronize faster, not just reset a counter.
*   **Panic-driven logic**: The parser logic relies heavily on `unwrap` or array indexing without bounds checks in hot paths (though some are guarded). A compiler parsing user input **must never panic**, yet `src/thin_emitter/helpers.rs` has unsafe string slicing: `unsafe { std::str::from_utf8_unchecked(&buf[i..]) }`. While technically safe for digits, it sets a dangerous precedent.

#### 4. Transformation Architecture (The "String" problem)

The transformation logic (ES5 downleveling) in `src/transforms/` is mixing AST manipulation with string emission too early.

*   **Stringly-Typed Emitter**: `ClassES5Emitter` and `AsyncES5Emitter` generate huge `String` buffers directly.
    *   *Problem*: You cannot easily run subsequent passes (e.g., minification, further lowering) on a string blob.
    *   *Problem*: Source maps are being manually hacked together (`record_mapping`, `source_position_from_offset`).
    *   *Correct Approach*: Transforms should produce a lowered AST (even if it's a specific "Low-Level AST" or IR), and the printer should blindly print nodes. Emitting strings inside the transform logic couples code generation with semantics.

#### 5. Type Solver Issues

*   **Any Propagation**: The `AnyPropagationRules` in `src/solver/lawyer.rs` is complex business logic mixed with type resolution.
    *   `has_structural_mismatch_despite_any` attempts to re-implement structural checking logic outside the `SubtypeChecker`. This logic belongs in the comparator, not in a pre-check.
*   **Cyclic Dependency**: The `TypeEvaluator` uses `evaluate` which calls `lookup`. `lookup` locks the interner. `evaluate` might intern new types (e.g., instantiating generics), which locks the interner (write). This recursive read-then-write pattern on `RwLock`s is a classic deadlock scenario if not handled with extreme care (e.g., upgrading locks, which standard `RwLock` doesn't support atomically).

#### 6. Specific Code Smells

*   **`src/thin_emitter/mod.rs`**: The `emit_node` function is a massive match statement dispatching on `kind` (u16). While fast, it's unmaintainable.
*   **`src/checker/flow_analyzer.rs`**: `check_flow` is recursive. Deeply nested control flow (common in generated code) will blow the stack. This needs to be an iterative worklist algorithm.
*   **`AGENTS.md` vs Reality**: The documentation forbids "Test-aware code", yet the sheer volume of logic deducing behavior from `file_name` strings (e.g., `is_jsx_file` in parser) feels fragile. Configuration should drive this, not file extensions (though standard in TS, it hinders library usage).

#### 7. Recommendations

1.  **Arena Overhaul**: Switch `TypeInterner` to use `bumpalo` or a lock-free arena. Remove `RwLock<Vec<T>>` immediately.
2.  **String Ownership**: Refactor `Scanner` and `Parser` to hold a reference to source text (`&str`) managed by a `SourceFile` struct. Stop cloning source text.
3.  **Transform Pipeline**: Stop emitting strings in `transforms/*.rs`. Create a `SyntheticNode` variant in `ThinNode` if necessary, or map to a Lowered AST, then print.
4.  **Flow Analysis**: Rewrite `check_flow` to be iterative.
5.  **Cleanup**: Delete `src/parser/ast` (Fat Nodes) if `src/parser/thin_node.rs` is the future. Having both is confusing and doubles the maintenance surface.

#### Verdict

**Project Status**: Alpha / Prototype.
**Performance**: Likely poor in parallel scenarios due to lock contention. High memory usage due to string cloning.
**Correctness**: High structural fidelity to TypeScript, but implementation details need rigor.

The project mimics TypeScript's architecture *too* closely in some places (like the massive switch statements) while deviating in dangerous ways (concurrency model) without solving the underlying data hazard problems.

### 1. Fix Hanging Tests

Several tests have infinite loops and hang forever. These must be identified and marked with `#[ignore]` before any other work.

**Currently Ignored (infinite loops):**
- `test_class_es5_commonjs_class_exports` (transforms/class_es5_tests.rs)
- `test_source_map_decorator_combined_advanced` (source_map_tests.rs)
- `test_source_map_decorator_composition_es5_comprehensive` (source_map_tests.rs)
- `test_source_map_decorator_composition_es5_method_params` (source_map_tests.rs)
- `test_source_map_decorator_metadata_es5_parameter_decorators` (source_map_tests.rs)

**Action:** Run tests with timeouts to find any remaining hanging tests.

### 2. Improve Conformance Test Pass Rate

Current pass rate is not close to our target of 95%+. Focus on fixing high-impact issues in the solver and checker to improve accuracy.

### 3. Clean Up Clippy Ignores

One by one go through rules ignored in `clippy.toml` and fix the underlying issues to enable the lints project-wide.

### 4. Complete TODOs in conformance/TEST_CATEGORIES.md
Finish implementing the unified test runner to handle `compiler/` and `projects/` tests in addition to `conformance/`.


### 5. Improve code hygiene

- Move all scripts to `scripts/` directory. no scripts in root. Including test.sh, build-wasm.sh, etc.
- Update AGENTS.md so agents do not produce .md files for results of their work.
- Update .gitignore to not allow any new files in root.
- Revisit scripts/ and conformance/ scripts and clean up as needed.
- Move docker files to scripts/docker/ or similar.


## Merge Criteria

**Before merging any branch:**
1. `cargo build` must pass with no errors
2. `cargo test` must pass with no failures
3. Tests must run fast (< 30 seconds for full test suite)
4. Individual tests must complete in < 5 seconds (mark slow tests with `#[ignore]`)

---

## Rules

### Never Break The Build
- All commits must pass unit tests
- No change should reduce conformance accuracy

### Keep Architecture Clean
- No shortcuts
- No test-aware code in source
- Fix root causes, not symptoms
- No whack-a-mole error suppression

### Anti-Patterns to Avoid

| Don't | Do Instead |
|-------|------------|
| Check file names in checker | Fix the underlying logic |
| Suppress errors for specific tests | Implement correct behavior |
| Add "Tier 0/1/2" patches | Fix root cause once |
| Add filtering for test patterns | Make checker correct for all code |
| Create infinite loops in transforms | Add recursion limits |

---

## Commands

```bash
# Build
cargo build

# Run all tests (Docker-based)
./scripts/test.sh

# Run all tests (local)
cargo test --lib

# Run specific test module
cargo test --lib solver::

# Build WASM (Docker-based)
./scripts/build-wasm.sh

# Build WASM (local)
wasm-pack build --target web --out-dir pkg

# Quick conformance test
cd conformance && npm run test:100

# Full conformance test
cd conformance && npm run test:1000
```

---

## Key Files

| File | Purpose | Lines |
|------|---------|-------|
| `src/thin_checker.rs` | Type checker (needs cleanup) | 24,579 |
| `src/thin_parser.rs` | Parser | 11,068 |
| `src/binder.rs` | Symbol binding | 2,108 |
| `src/solver/` | Type resolution (39 files) | ~15,000 |
| `src/transforms/` | ES5 downlevel transforms | ~10,000 |
| `conformance/` | Conformance test infrastructure | - |
| `AGENTS.md` | Architecture rules for AI agents | - |

---

## Test Results

| Metric | Meaning |
|--------|---------|
| Exact Match | Identical errors to TSC |
| Missing Errors | TSC emits, we don't |
| Extra Errors | We emit, TSC doesn't |

---

## Project Goals

**Target:** 95%+ exact match with TypeScript compiler on conformance tests, with clean architecture and maintainable codebase.

**Non-Goals:**
- 100% compatibility (edge cases acceptable)
- Supporting deprecated features
- Matching TSC performance exactly (we aim for better)
