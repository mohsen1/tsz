# Project Zang

TypeScript compiler rewritten in Rust, compiled to WebAssembly. Goal: TSC compatibility with better performance.

**Current Status:** Type system solver is complete. Focus is now on production readiness: transform pipeline migration, diagnostic coverage, module resolution, and conformance validation.

**Conformance Baseline (2025-01-22):** 34.7% pass rate (4,182/12,053 tests)

---

## Top Priority: Transform Pipeline Migration

**Problem:** `src/transforms/` has ~7,500 lines of architectural debt blocking reliable ES5 JavaScript emission.

**Issue:** Legacy transforms (class_es5, async_es5, namespace_es5) mix AST manipulation with string emission, making them untestable and error-prone.

**Solution:** Migrate to IR (Intermediate Representation) pattern already proven in enum_es5, destructuring_es5, spread_es5.

### Transforms Requiring Migration

| Transform | Lines | Blocker | Complexity |
|-----------|-------|---------|------------|
| `class_es5.rs` | 4,849 | Classes, inheritance, private fields, getters/setters | **CRITICAL** |
| `async_es5.rs` | 1,491 | Async/await → __awaiter/__generator | **HIGH** |
| `namespace_es5.rs` | 1,169 | Nested IIFEs | **MEDIUM** |

### Reference Implementation (Already Working)

Study these files for the migration pattern:
- `src/transforms/enum_es5.rs` - Clean IR pattern
- `src/transforms/destructuring_es5.rs` - IR-based transform
- `src/transforms/ir.rs` - IR node definitions

### Migration Pattern

```rust
// Step 1: Create transformer (no strings, just IR)
pub struct ES5ClassTransformer<'a> {
    arena: &'a NodeArena,
    class_name: String,
    temp_var_counter: u32,
}

impl<'a> ES5ClassTransformer<'a> {
    pub fn transform_class(&mut self, idx: NodeIndex) -> Option<IRNode> {
        // Analyze AST, build IR tree
        IRNode::ES5ClassIIFE {
            name: self.class_name.clone(),
            base_class: /* ... */,
            body: /* ... */,
        }
    }
}

// Step 2: Wrap old Emitter for compatibility
pub struct ClassES5Emitter<'a> {
    transformer: ES5ClassTransformer<'a>,
}

impl<'a> ClassES5Emitter<'a> {
    pub fn emit_class(&mut self, idx: NodeIndex) -> String {
        let ir = self.transformer.transform_class(idx)?;
        IRPrinter::emit_to_string(&ir)
    }
}
```

### IR Nodes Already Defined

The following IR nodes exist in `src/transforms/ir.rs` and are ready to use:
- `ES5ClassIIFE` - Class IIFE pattern
- `ExtendsHelper` - `__extends` helper call
- `PrototypeMethod` - Method assignment to prototype
- `StaticMethod` - Static method assignment
- `AwaiterCall` - `__awaiter` helper
- `GeneratorBody` - Generator state machine
- `GeneratorOp` - `[opcode, value]` operations
- `NamespaceIIFE` - Namespace IIFE pattern

### Estimated Timeline

- **Week 1:** Migrate `class_es5.rs`
  - Build transformer struct
  - Construct IR nodes for IIFE, extends, methods
  - Test against existing class transform tests

- **Week 2:** Migrate `async_es5.rs`
  - Build generator state machine IR
  - Reuse AwaiterCall, GeneratorBody nodes

- **Week 3:** Migrate `namespace_es5.rs` + Integration
  - Complete transform migration
  - Update all callers
  - Run full test suite validation

### Success Criteria

- All 3 transforms use IR pattern (no direct string emission)
- All existing transform tests pass
- ES5 emission matches TSC output

---

## High Priority: Diagnostic Coverage & Symbol Resolution

**Problem:** Conformance tests show significant missing diagnostic coverage (65% of tests fail).

**Key Insight:** This is **NOT** a type system problem. The solver is complete and battle-tested (91K lines of tests pass).

**Root Cause:** Symbol resolution and diagnostic coverage gaps in the checker.

### Conformance Baseline

Run on 2025-01-22 with 12,053 tests:
- **Pass Rate:** 34.7% (4,182 passed)
- **Failed:** 7,351 tests
- **Crashed:** 519 tests (mostly ENOTDIR - infrastructure issues)

### Top Missing Errors (Symbol Resolution Issues)

| Error Code | Count | Description |
|------------|-------|-------------|
| TS2318 | 8,082x | Cannot find type '{0}' |
| TS2583 | 4,359x | Cannot find name '{0}'. Did you mean '{1}'? |
| TS2304 | 1,640x | Cannot find name '{0}' |
| TS2322 | 1,352x | Type '{0}' is not assignable to type '{1}' |
| TS2554 | 1,051x | Expected {0} arguments, but got {1} |

**Pattern:** 4 of top 5 are symbol/type lookup failures, not type checking failures.

### Architecture Assessment (Don't Repeat checker.rs Mistakes)

**TypeScript's checker.rs problems:**
- 25,000+ lines of monolithic imperative code
- Mixes symbol resolution, type checking, and emission
- Hard to test, hard to reason about

**Your architecture (better!):**
```
Binder (symbols) → Checker (AST walker) → Solver (type operations)
     ↓                    ↓                      ↓
 SymbolTable         Diagnostics          TypeInterner
```

**What's complete:**
- ✅ **Solver:** Query-based structural type solver with Ena unification
- ✅ **Type operations:** Subtyping, instantiation, lowering all working
- ✅ **Test coverage:** 91K lines of solver tests pass
- ✅ **Diagnostic infrastructure:** Error codes and templates exist

**What needs work:**
- ⚠️ **Binder→Checker integration:** Symbol lookup coverage
- ⚠️ **Type lowering:** AST → Type conversion completeness
- ⚠️ **Diagnostic emission:** When to emit errors (precision)

### Strategy: Incremental Diagnostic Coverage

**Goal:** Improve pass rate from 34.7% → 60%+ without architectural changes.

**Approach:** Debug ONE error at a time, learn the pattern, fix similar cases.

#### Phase 1: Symbol Resolution Coverage (1-2 weeks)

Focus on the 8,082 missing TS2318 errors:

1. **Pick a failing test** with missing TS2318/TS2304
2. **Run with verbose output:**
   ```bash
   ./conformance/run-conformance.sh --native --max=1 --verbose \
     --filter=conformance/path/to/test.ts
   ```
3. **Compare TSC output vs TSZ output**
   - What error does TSC emit?
   - Where in your checker should this be caught?
4. **Fix the specific code path**
5. **Find similar cases** and fix them

**Focus areas:**
- `src/checker/state.rs` - Symbol lookup methods
- `src/solver/lower.rs` - Type reference resolution
- Import statement handling
- Module augmentation
- Ambient contexts

#### Phase 2: Type Checking Precision (2-3 weeks)

Focus on TS2322 (type not assignable) and TS2554 (argument count):
- Overload resolution coverage
- Generic type instantiation error paths
- Callable type checking

### What NOT to Do

| ❌ Don't | ✅ Do Instead |
|---------|---------------|
| Rewrite the solver | It's complete - 91K tests pass |
| Chase pass percentages | Fix root causes incrementally |
| Replicate checker.rs complexity | Your thin checker is better |
| Add test-specific workarounds | Fix underlying checker logic |
| Major architectural changes | Incremental coverage improvements |

### Success Criteria

- **Short term (2 weeks):** Pass rate 34.7% → 45%
- **Medium term (4 weeks):** Pass rate → 60%+
- **Solver remains:** Untouched (it's good)
- **Architecture:** Clean separation maintained

---

## Secondary Priority: Module Resolution

**Goal:** Enable `projects/` conformance tests (175 files) for real-world project validation.

**What's Needed:**
1. Implement Node16/NodeNext module resolution algorithms
2. Add `projects/` category to conformance runner
3. Handle tsconfig.json `paths`, `baseUrl`, composite projects
4. Support circular import detection

**Estimated Effort:** 1 week

**Reference:** `specs/COMPILER_OPTIONS.md`, `src/module_resolver.rs`

---

## Secondary Priority: Conformance Test Infrastructure Fix

**Problem:** Test runner has JSON parsing bug preventing reliable baseline measurements.

**Action Plan:**
1. Debug `conformance/run-conformance.sh --max=10 --verbose`
2. Fix JSON parsing in `conformance/src/worker.ts`
3. Get baseline conformance metrics
4. Identify top 10 failing test categories

**Important:** Do NOT chase pass percentages. Fix root causes in solver/checker, not test-specific workarounds.

---

## Test Infrastructure

**Current Coverage:**
- `conformance/`: 5,691 files (language spec compliance)
- `compiler/`: 6,402 files (compiler behavior)
- **Not testing:** `projects/` (175 files) - module resolution
- **Not testing:** `fourslash/` (6,563 files) - IDE features

**Recommended Additions (After Transform Migration):**

### 1. Add `projects/` Category (HIGH VALUE)

**Purpose:** Module resolution, compiler options, multi-file projects

**Why:** Validates real-world project scenarios, essential for TypeScript usage

**Effort:** 2-4 hours

**Requirements:**
- JSON parser for project configuration
- Module resolution implementation (Node16/NodeNext)
- Multi-file project test harness

### 2. Evaluate `fourslash/` Subset

**Purpose:** Complex multi-file language features

**Why:** Validates edge cases in multi-file scenarios

**Effort:** 8-16 hours (selective integration)

**Strategy:**
- ✅ Extract language feature tests
- ❌ Skip IDE-specific tests (completion, navigation)
- ⚠️ Requires fourslash DSL parser

---

## Future Directions (Post-MVP)

After transform migration and module resolution are complete, choose one:

### Option A: LSP/IDE Integration

**Value:** User-facing, visible impact

**Effort:** 4-6 weeks

**Tasks:**
- Parent mapping for O(1) parent lookup
- LSP protocol handlers (hover, go-to-def)
- Incremental type checking
- WASM language service for browser tools

### Option B: Performance Benchmarking

**Value:** Credibility, marketing differentiation

**Effort:** 2-3 weeks

**Tasks:**
- Benchmark suite (parser speed, emitter throughput)
- Profile real codebases (React, Vue)
- WASM vs native comparison
- Memory profiling validation

### Option C: WASM Language Service

**Value:** Browser-based TypeScript (StackBlitz, CodeSandbox)

**Effort:** 3-4 weeks

**Tasks:**
- Browser-compatible WASM build
- Language service API
- Integration with web editors

---

## Immediate Action Plan (This Week)

### Day 1: Debug One Missing Error

```bash
# Find a test with missing TS2304/TS2318
./conformance/run-conformance.sh --native --max=10 --verbose 2>&1 | \
  grep -B3 "Missing: TS2304\|Missing: TS2318"

# Pick ONE failing test
./conformance/run-conformance.sh --native --max=1 --verbose \
  --filter=conformance/path/to/test.ts

# Compare TSC vs TSZ output
tsc --noEmit conformance/path/to/test.ts
# vs your output
```

**Goal:** Understand ONE missing error pattern end-to-end.

### Day 2-3: Fix Symbol Resolution Pattern

```bash
# Find where the error should be emitted
grep -r "TS2304\|TS2318" src/checker/

# Add the missing check
# Test against similar cases
```

### Day 4-5: Repeat for Top Error Patterns

- Import resolution errors
- Type reference lookup
- Module augmentation
- Ambient context handling

**Goal:** 5-10% improvement in pass rate.

---

## Commands Reference

```bash
# Build
cargo build                              # Native build
wasm-pack build --target nodejs          # WASM build

# Test
cargo test --lib                         # All unit tests
cargo test --lib solver::subtype_tests   # Specific module
./scripts/test.sh --lib                  # Docker-isolated tests

# Conformance (Docker-isolated for safety)
./conformance/run-conformance.sh --max=500       # Run 500 tests
./conformance/run-conformance.sh --all           # Run all tests
./conformance/run-conformance.sh --native        # Use native binary (faster)
./conformance/run-conformance.sh --wasm          # Use WASM (slower)
./conformance/run-conformance.sh --no-sandbox    # No Docker (unsafe - infinite loop risk)
./conformance/run-conformance.sh --workers=N     # Set worker count
```

---

## Git Hooks

Pre-commit hooks enforce code quality:

```bash
# Install (run once)
./scripts/install-hooks.sh

# Hooks run automatically:
# 1. cargo fmt (formatting)
# 2. cargo clippy --fix (linter)
# 3. cargo build (build warnings)
# 4. cargo test --lib (unit tests in Docker)
```

**Fix issues before committing:**
```bash
cargo fmt
cargo clippy --all-targets --fix --allow-dirty --allow-staged
```

---

## Rules

| Don't | Do Instead |
|-------|------------|
| Check file names in checker | Fix the underlying logic |
| Suppress errors for specific tests | Implement correct behavior |
| Chase conformance percentages | Fix root causes in solver/checker |

---

## Merge Criteria

1. `cargo build` passes
2. `cargo test` passes
3. Tests run in < 30 seconds
4. No test-aware code in source

---

## Key Files for Transform Migration

| File | Lines | Purpose |
|------|-------|---------|
| `src/transforms/class_es5.rs` | 4,849 | **MIGRATE FIRST** - Classes, inheritance |
| `src/transforms/async_es5.rs` | 1,491 | **MIGRATE SECOND** - Async/await |
| `src/transforms/namespace_es5.rs` | 1,169 | **MIGRATE THIRD** - Namespaces |
| `src/transforms/ir.rs` | 738 | IR node definitions (reference) |
| `src/transforms/enum_es5.rs` | ~800 | Clean IR pattern example |
| `src/transforms/mod.rs` | 105 | Migration status documentation |
| `src/transforms/ir_printer.rs` | 206 | IR → JavaScript printer |
| `src/transforms/destructuring_es5.rs` | ~900 | Another IR pattern example |
| `src/checker/state.rs` | 25,849 | Main checker (may need transform updates) |
| `src/emitter/mod.rs` | ~2,000 | Emitter entry points |

## Key Files for Diagnostic Coverage

| File | Lines | Purpose |
|------|-------|---------|
| `src/checker/state.rs` | 14,558 | **MAIN FOCUS** - Symbol lookup, diagnostic emission |
| `src/checker/expr.rs` | ~5,000 | Expression checking |
| `src/solver/lower.rs` | 2,408 | AST → Type conversion (type reference resolution) |
| `src/checker/types/diagnostics.rs` | 455 | Error codes and templates |
| `src/binder/` | ~3,000 | Symbol table (may need coverage improvements) |
| `src/checker/declarations.rs` | ~3,000 | Declaration checking |

**Architecture Summary:**
- Solver: ~30,000 lines (complete, tested)
- Checker: ~50,000 lines (needs diagnostic coverage)
- Binder: ~3,000 lines (symbol resolution)

---

**Total Codebase:** ~500,000 lines of Rust code
