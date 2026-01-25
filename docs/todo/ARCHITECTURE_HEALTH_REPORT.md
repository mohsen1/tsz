# Architecture Health Report: Project Zang (tsz)

**Date**: January 23, 2026  
**Report Type**: Deep Dive Architecture Assessment  
**Codebase Version**: Branch `main`

---

## Executive Summary

This report provides a comprehensive assessment of Project Zang's architecture health after reviewing documentation, specifications, and ongoing refactoring work. The analysis evaluates how well the compiler architecture aligns with its design goals, particularly the solver-based type system approach.

### Overall Assessment: **SOLID FOUNDATION WITH CLEAR PATH FORWARD**

The architecture demonstrates:
- ✅ **Strong theoretical foundation** - Set-theoretic solver design with Judge/Lawyer separation
- ✅ **Good progress on refactoring** - Phase 1 complete, Phase 2 in progress
- ✅ **Consistent transform pattern** - Transformer + IRPrinter architecture implemented
- ⚠️ **Critical gaps** - Missing error detection (41.5% conformance), incomplete unsoundness rules (33%)
- ⚠️ **Architectural debt** - God objects, missing abstractions, stability issues

---

## 1. Architecture Foundation: The Solver Idea

### 1.1 Theoretical Foundation ✅ **EXCELLENT**

The solver architecture is well-designed and grounded in solid theory:

**Set-Theoretic Foundation** (`specs/SOLVER.md`):
- Types as sets with coinduction for recursive types
- TypeId/TypeKey interned representation
- Judge vs Lawyer architecture for soundness/compatibility separation

**Implementation Status**:
- ✅ **Judge/Lawyer separation implemented** (`src/solver/lawyer.rs:1-12`)
  - Core solver (sound set theory) separated from compatibility layer
  - `AnyPropagationRules` handles TypeScript quirks
  - `CompatChecker` applies unsoundness rules before delegating to Judge

- ✅ **TypeDatabase abstraction** (`src/solver/db.rs:1-4`)
  - Trait-based design allows future Salsa integration
  - Current implementation uses `TypeInterner` with DashMap (64 shards)
  - Lock-free concurrent interning verified

**Assessment**: The solver idea is **very solid**. The theoretical foundation is sound, and the implementation follows the design. The abstraction layer (`TypeDatabase`) provides a clear path for incremental query system integration.

### 1.2 Compatibility Layer Status ⚠️ **INCOMPLETE**

**Implementation Progress** (`docs/UNSOUNDNESS_AUDIT.md:5-13`):
- **Total Rules**: 44
- **Fully Implemented**: 9 (20.5%)
- **Partially Implemented**: 11 (25.0%)
- **Not Implemented**: 24 (54.5%)
- **Overall Completion**: 33.0%

**Critical Missing Rules**:
- ❌ **Enum rules** (all missing) - Blocks enum-heavy codebases
- ❌ **Class rules** (nominal classes, static side, abstract classes)
- ❌ **Phase 2 blockers**: Literal widening (#10), Covariant `this` (#19)
- ⚠️ **Error poisoning** (#11) - Partially implemented, `Union(Error, T)` suppression missing

**Impact**: While the architecture is sound, incomplete compatibility rules prevent matching TypeScript's behavior, limiting conformance.

---

## 2. Refactoring Progress

### 2.1 Phase 1: Critical Stabilization ✅ **100% COMPLETE**

All Phase 1 tasks completed (`docs/ARCHITECTURE_AUDIT_REPORT.md:12-21`):
- ✅ `is_numeric_property_name` consolidated
- ✅ Parameter extraction unified
- ✅ TypeId sentinel semantics documented
- ✅ Accessor map duplication fixed
- ✅ ErrorHandler trait implemented
- ✅ Recursion depth limits added

**Status**: Phase 1 successfully stabilized the codebase and eliminated critical duplication.

### 2.2 Phase 2: Break Up God Objects 🚧 **9% PROGRESS**

**Solver Subtype Decomposition** (`docs/ARCHITECTURE_AUDIT_REPORT.md:27`):
- **Status**: In Progress
- **Progress**: 2,437 → ~2,214 lines (9% reduction, ~223 lines extracted)
- **Methods Extracted**: 7 helper methods
  - Union/intersection subtype checking
  - Type parameter compatibility
  - Tuple-to-array conversion
  - Function/callable conversion

**Checker State Decomposition** (`docs/ARCHITECTURE_AUDIT_REPORT.md:28`):
- **Status**: Pending
- **Size**: 27,525 lines, 554 functions
- **Partial Extraction**: Some modules exist (`type_computation.rs`, `symbol_resolver.rs`, `error_reporter.rs`, `accessibility.rs`)
- **Remaining**: Main `state.rs` still contains massive functions like `get_type_of_identifier` (1,183 lines)

**Assessment**: Incremental extraction pattern is working. Need to continue `solver/subtype.rs` decomposition before tackling the larger `checker/state.rs` god object.

### 2.3 Phase 3: Introduce Abstractions ⚠️ **MIXED**

**Type Visitor Pattern** (`docs/ARCHITECTURE_AUDIT_REPORT.md:34`):
- **Status**: ⏳ Pending
- **Problem**: 48+ `match node.kind` statements duplicated in `checker/state.rs`
- **Impact**: High maintenance burden, error-prone

**Transform Interface** (`docs/ARCHITECTURE_AUDIT_REPORT.md:35`):
- **Status**: ✅ Implemented (pattern)
- **Implementation**: Transformer + IRPrinter pattern documented in `docs/TRANSFORM_ARCHITECTURE.md`
- **Pattern**: `*Transformer::transform_*` returns `Option<IRNode>`, `IRPrinter::emit_to_string` handles emission
- **Assessment**: Formal trait optional; current pattern provides sufficient abstraction

---

## 3. Conformance & Correctness

### 3.1 Current Conformance Status ⚠️ **41.5% PASS RATE**

**Baseline** (`PROJECT_DIRECTION.md:7`):
- **Pass Rate**: 41.5% (5,056/12,197 tests)
- **Trend**: Up from 36.3% (improving)
- **False Positives**: Fixed (no extra errors in top list)

### 3.2 Missing Error Detection 🔴 **CRITICAL BLOCKER**

**Top Missing Errors** (`PROJECT_DIRECTION.md:24-35`):

| Error Code | Missing Count | Description | Priority |
|------------|---------------|-------------|----------|
| TS2304 | 4,636x | Cannot find name | HIGH |
| TS2318 | 3,492x | Cannot find global type | HIGH |
| TS2307 | 2,331x | Cannot find module | HIGH |
| TS2583 | 1,913x | Change target library? | MEDIUM |
| TS2322 | 1,875x | Type not assignable (legitimate) | MEDIUM |

**The "Poisoning Effect"** (`specs/DIAGNOSTICS.md:23-30`):
1. Binder fails to load `lib.d.ts` correctly or merge scopes
2. Standard globals (`console`, `Promise`, `Array`) become unresolved (TS2304)
3. Solver defaults unresolved symbols to `Any`
4. **Result**: `Any` suppresses all further errors, creating false sense of conformance

**Impact**: This is the **#1 blocker** for correctness. Missing symbol resolution cascades into silent failures, masking real type errors.

### 3.3 Stability Issues ⚠️ **CONCERNING**

**Current Issues** (`PROJECT_DIRECTION.md:56-87`):
- **OOM Tests**: 4 tests (infinite type expansion)
- **Timeout Tests**: 54 tests (infinite loops in type resolution)
- **Worker Crashes**: 112 crashed, 113 respawned (panics/stack overflows)

**Root Causes**:
- Unbounded recursion in solver (partially addressed with depth limits)
- Missing cycle detection in type resolution
- Stack overflow on deep recursion

**Impact**: Stability issues limit architectural improvements and indicate fundamental limits in current implementation.

---

## 4. Architectural Gaps

### 4.1 Tracer Pattern ✅ **IMPLEMENTED**

**Status** (`specs/WASM_ARCHITECTURE.md:211-268`):
- **Documented**: ✅ Yes (aspirational design)
- **Implemented**: ✅ Yes (commit `ee561f158`)
- **Tests**: ✅ Yes (commit `f53d09404`, 250 lines of tests)
- **Risk**: ✅ Eliminated - fast and diagnostic paths now share logic

**Implementation**:
```rust
// Zero-cost abstraction for fast checks
pub struct FastTracer;
impl SubtypeTracer for FastTracer {
    #[inline(always)]
    fn on_mismatch(&mut self, _reason: impl FnOnce() -> SubtypeFailureReason) -> bool {
        false // Stop immediately, no allocation
    }
}

// Detailed diagnostics collection
pub struct DiagnosticTracer { failure: Option<SubtypeFailureReason> }
impl SubtypeTracer for DiagnosticTracer {
    fn on_mismatch(&mut self, reason: impl FnOnce() -> SubtypeFailureReason) -> bool {
        if self.failure.is_none() {
            self.failure = Some(reason());
        }
        false
    }
}
```

**Key Benefits**:
1. **Zero-Cost Abstraction**: FastTracer compiles to same code as direct boolean return
2. **No Logic Duplication**: Single code path for both fast and diagnostic checks
3. **Lazy Evaluation**: FailureReason only constructed when needed
4. **Prevents Drift**: Fast and diagnostic paths use identical logic

**Files Modified**:
- `src/solver/diagnostics.rs`: Added SubtypeTracer trait, FastTracer, DiagnosticTracer
- `src/solver/subtype.rs`: Removed duplicate SubtypeFailureReason, re-exported from diagnostics
- `src/solver/tracer_tests.rs`: Comprehensive test suite

**Impact**: ✅ Eliminated risk of logic drift between fast checking and diagnostic reporting

### 4.2 Emitter/Transform Separation ⚠️ **PARTIAL DEBT**

**Status** (`specs/WASM_ARCHITECTURE.md:260-264`):
- **Known Debt**: ⚠️ Transform pipeline still mixes lowering/printing
- **Transform Pattern**: ✅ Consistent Transformer + IRPrinter architecture exists
- **Remaining Issue**: Emitter still instantiates transform emitters directly (`src/emitter/mod.rs:32-34`)

**Assessment**: The transform layer itself is well-architected. The remaining debt is in how the emitter invokes transforms, not in the transform design.

### 4.3 Salsa Integration ⏳ **PREPARED BUT NOT STARTED**

**Status** (`specs/SOLVER.md:979-1003`):
- **Design**: ✅ Complete (Phase 7.5 execution plan documented)
- **Abstraction**: ✅ Ready (`TypeDatabase` trait allows swap)
- **Implementation**: ❌ Not started
- **Priority**: Low (current interning works, Salsa is optimization)

**Assessment**: The architecture is prepared for Salsa integration, but it's not blocking current work. Can be deferred until after conformance improvements.

---

## 5. Code Quality Metrics

### 5.1 God Objects 🔴 **CRITICAL**

**The "Big 6" Monster Files** (`docs/ARCHITECTURE_AUDIT_REPORT.md:84-95`):
- `checker/state.rs`: 27,525 lines (51% of total)
- `parser/state.rs`: 10,762 lines (20%)
- `solver/evaluate.rs`: 5,784 lines (11%)
- `solver/subtype.rs`: 4,734 lines (9%) - **In progress**
- `solver/operations.rs`: 3,416 lines (6%)
- `emitter/mod.rs`: 2,040 lines (4%)

**Largest Function**: `check_subtype_inner` - 2,437 lines (reduced to ~2,214, 9% progress)

**Impact**: God objects impede maintainability, testability, and onboarding.

### 5.2 Code Duplication ⚠️ **IMPROVING**

**Status** (`docs/ARCHITECTURE_AUDIT_REPORT.md:207-278`):
- **Critical Duplicates**: Mostly addressed (Phase 1)
- **Pattern Duplication**: 130 `match node.kind` statements (reduced from 135, 4% improvement)
- **Save/Restore Scanner**: 60+ instances (300+ redundant lines)
- **AST Traversal Helpers**: 12 helper functions added in type_checking.rs (commit 05f7010ad)

**Recent Progress (2026-01-24)**:
- ✅ Created `get_declaration_modifiers()` - Extract modifiers from any declaration node
- ✅ Created `get_member_modifiers()` - Extract modifiers from class member nodes
- ✅ Created `get_member_name_node()` - Get name node from class member nodes
- ✅ Created `get_declaration_name()` - Get name node from declaration nodes
- ✅ Created `has_modifier_kind()` - Generic modifier checking helper
- ✅ Created `for_each_binary_child()` - Traverse binary expression children
- ✅ Created `for_each_conditional_child()` - Traverse conditional expression children
- ✅ Created `for_each_call_child()` - Traverse call expression children
- ✅ Created `for_each_parenthesized_child()` - Traverse parenthesized expressions
- ✅ Refactored 9 functions to use new helpers (~160 lines eliminated)

**Assessment**: Phase 1 eliminated worst duplicates. AST traversal deduplication started with helper functions.
Target: Continue reducing `match node.kind` statements to < 20 total.

### 5.3 Error Handling ⚠️ **CONCERNING**

**Unsafe Unwrap Usage** (`docs/ARCHITECTURE_AUDIT_REPORT.md:411-419`):
- **Total**: 5,036 `unwrap()`/`expect()` calls
- **Production Code**: ~1,500 calls (concerning)
- **Test Code**: ~3,500 calls (expected)

**Silent Error Swallowing**: Errors converted to sentinel values (`TypeId::ERROR`, `TypeId::UNDEFINED`), masking root causes.

**Assessment**: High unwrap count indicates missing error handling paths. Silent conversions hide bugs.

---

## 6. Recommendations

### 🎯 Current Focus: Solver/Subtype.rs Decomposition (2026-01-24)

**Strategic Focus**: Break up `check_subtype_inner` function in `solver/subtype.rs`

**Why This Focus?**
- `check_subtype_inner` is still ~2,214 lines (reduced from 2,437, 9% progress)
- Largest function in codebase - critical for maintainability
- Breaking it up will improve testability and enable parallel work
- Establishes patterns for future refactoring

**Current Progress**:
- 7 helper methods already extracted (~223 lines)
- Remaining: ~1,700+ lines still in main function
- Target: Reduce to ~500 lines (coordinator function)

**Extraction Strategy**:
- **Incremental approach**: Extract 200-400 lines per commit
- **Focus areas**:
  1. **Object subtyping** (~400-600 lines): Property matching, index signatures, excess properties
  2. **Template literal types** (~200-300 lines): Pattern matching, backtracking
  3. **Mapped/conditional types** (~300-400 lines): Type evaluation, distribution
  4. **Primitive/intrinsic types** (~200-300 lines): Hierarchy, conversions
- **Final goal**: Move to `solver/subtype_rules/` module structure

**Expected Impact**:
- `check_subtype_inner`: ~2,214 → ~500 lines (coordinator)
- Improved testability (each helper independently testable)
- Reduced cognitive load (clearer function names)
- Pattern established for future god object decomposition

**Documentation Strategy**:
- Doc comments on public APIs only
- Do NOT make documentation the primary goal
- Focus on code reduction and modularity

---

### 6.1 Immediate Priorities (Next 2-4 Weeks)

1. **🎯 Solver/Subtype.rs Decomposition** 🔴 **CRITICAL - CURRENT FOCUS**
   - **Focus**: Extract 200-400 lines per commit from `check_subtype_inner`
   - **Impact**: Break up largest function, improve testability, establish patterns
   - **Files**: `src/solver/subtype.rs` → eventually `solver/subtype_rules/`
   - **Target**: Reduce from ~2,214 to ~500 lines (coordinator)
   - **Areas**: Object subtyping, template literals, mapped/conditional types, primitives
   - **Reference**: This document, "Current Focus" section above

2. **Fix Missing Error Detection** 🟢 **IN PROGRESS - lib.d.ts loading fixed**
   - **Focus**: TS2304/TS2318/TS2307 (symbol resolution, global types, modules)
   - **Impact**: Eliminates "Any poisoning", unlocks real type errors
   - **Status**: ✅ lib.d.ts loading fixed in TestContext (commit 3d453efb9)
     - Tests now load lib.d.ts by default via `TestContext::new()`
     - Created `tests/lib/lib.d.ts` with minimal lib definitions
     - Added `src/checker/ts2304_tests.rs` for verification
     - `Any poisoning` eliminated - TS2304 now properly emitted when lib not loaded
   - **Remaining**: Verify conformance test improvement, ensure WASM API loads lib by default
   - **Files**: `src/module_resolver.rs`, `src/checker/state.rs`, `src/binder/`
   - **Reference**: `PROJECT_DIRECTION.md:24-43`, `specs/DIAGNOSTICS.md:23-30`

2. **Complete Solver Subtype Decomposition** 🚧 **HIGH**
   - **Continue**: Extract remaining sections from `check_subtype_inner`
   - **Target**: Move to `solver/subtype_rules/` module structure
   - **Progress**: 9% → 50%+ (extract object subtyping, template literals)
   - **Reference**: `docs/ARCHITECTURE_AUDIT_REPORT.md:565-587`

3. **Implement Critical Unsoundness Rules** ⚠️ **HIGH**
   - **Phase 2 Blockers**: Literal widening (#10), Covariant `this` (#19)
   - **Error Poisoning**: Complete `Union(Error, T)` suppression (#11)
   - **Impact**: Blocks business logic conformance
   - **Reference**: `docs/UNSOUNDNESS_AUDIT.md:101-108`

### 6.2 Short-Term Priorities (1-2 Months)

4. **Start Checker State Decomposition** 🔴 **HIGH**
   - **Approach**: Incremental extraction (same pattern as solver)
   - **Target Modules**: `type_checking.rs`, `flow_analysis.rs` (extract from `state.rs`)
   - **Keep**: Orchestration in `state.rs` (~2,000 lines)
   - **Reference**: `docs/ARCHITECTURE_AUDIT_REPORT.md:553-563`

5. **Address Stability Issues** ⚠️ **MEDIUM**
   - **Focus**: Cycle detection, iteration limits, recursion bounds
   - **Impact**: Enables architectural improvements, reduces crashes
   - **Reference**: `PROJECT_DIRECTION.md:56-87`

6. ~~**Implement Tracer Pattern**~~ ✅ **COMPLETE**
   - **Goal**: Unify fast checking and diagnostic reporting
   - **Impact**: Prevents logic drift, enables zero-cost abstractions
   - **Status**: ✅ Implemented (commit `ee561f158`)
   - **Reference**: `specs/WASM_ARCHITECTURE.md:211-268`

### 6.3 Medium-Term Priorities (2-4 Months)

7. **Complete Unsoundness Rules** ⚠️ **MEDIUM**
   - **Focus**: Enum rules (#7, #24, #34), Class rules (#5, #18, #43)
   - **Impact**: Enables enum/class-heavy codebases
   - **Reference**: `docs/UNSOUNDNESS_AUDIT.md:79-99`

8. **Add Type Visitor Pattern** ⚠️ **MEDIUM**
   - **Goal**: Replace 48+ `match node.kind` statements
   - **Impact**: Reduces duplication, improves maintainability
   - **Reference**: `docs/ARCHITECTURE_AUDIT_REPORT.md:461-474`

9. **Resolve Circular Dependencies** ⚠️ **LOW**
   - **Issues**: Emitter ↔ Transforms, Lowering ↔ Transforms
   - **Approach**: Extract transform helpers, clarify boundaries
   - **Reference**: `docs/ARCHITECTURE_AUDIT_REPORT.md:97-115`

### 6.4 Long-Term Considerations (4+ Months)

10. **Salsa Integration** ⏳ **OPTIONAL**
    - **Status**: Architecture prepared, not blocking
    - **Benefit**: Incremental compilation, query caching
    - **Reference**: `specs/SOLVER.md:979-1113`

11. **Parser State Decomposition** 🔴 **LOW PRIORITY**
    - **Size**: 10,762 lines with heavy duplication
    - **Approach**: Similar to checker state decomposition
    - **Reference**: `docs/ARCHITECTURE_AUDIT_REPORT.md:163-190`

---

## 7. Architecture Strengths

### 7.1 Theoretical Foundation ✅

- **Set-theoretic solver design** - Mathematically sound
- **Judge/Lawyer separation** - Clean compatibility layer
- **Coinduction for recursion** - Handles recursive types correctly
- **Interned types** - Efficient representation

### 7.2 Data-Oriented Design ✅

- **16-byte Node headers** - Cache-efficient AST
- **Atom-based interning** - Zero-copy strings
- **Sharded interner** - Lock-free concurrent access
- **Arena allocation** - Predictable memory patterns

### 7.3 Transform Architecture ✅

- **Transformer + IRPrinter pattern** - Clean separation
- **Consistent API** - All transforms follow same pattern
- **Testable** - IR nodes can be verified independently
- **Extensible** - Easy to add new transforms

### 7.4 Incremental Refactoring ✅

- **Phase 1 complete** - Critical stabilization done
- **Phase 2 in progress** - Proven extraction pattern
- **Documented progress** - Clear roadmap visibility
- **Test-driven** - All refactoring verified

---

## 8. Architecture Risks

### 8.1 Conformance Gap 🔴 **HIGH RISK**

- **41.5% pass rate** - Significant gap from tsc
- **Missing errors** - 4,636 TS2304 errors alone
- **Any poisoning** - Cascading failures hide real issues
- **Impact**: Cannot claim TypeScript compatibility yet

### 8.2 God Objects 🔴 **HIGH RISK**

- **27,525-line file** - `checker/state.rs` is unmaintainable
- **2,437-line function** - `check_subtype_inner` untestable
- **Impact**: Changes are error-prone, onboarding is difficult

### 8.3 Stability Issues ⚠️ **MEDIUM RISK**

- **112 worker crashes** - Indicates architectural limits
- **54 timeout tests** - Infinite loops in type resolution
- **4 OOM tests** - Unbounded recursion
- **Impact**: Limits architectural improvements, user experience

### 8.4 Incomplete Compatibility Layer ⚠️ **MEDIUM RISK**

- **33% completion** - 24/44 unsoundness rules missing
- **Enum/Class gaps** - Blocks common code patterns
- **Impact**: Cannot match TypeScript behavior for many cases

---

## 9. Conclusion

### 9.1 Overall Assessment

**The solver idea is very solid.** The theoretical foundation is excellent, the Judge/Lawyer architecture is well-implemented, and the codebase shows good progress on refactoring. However, **critical gaps remain** that prevent claiming TypeScript compatibility:

1. **Missing error detection** (41.5% conformance) - The #1 blocker
2. **Incomplete unsoundness rules** (33% complete) - Blocks many TypeScript features
3. **God objects** (27K-line file) - Maintainability risk
4. **Stability issues** (crashes/timeouts) - Architectural limits

### 9.2 Path Forward

The architecture is **on the right track** with a clear remediation roadmap:

1. ✅ **Phase 1 Complete** - Foundation stabilized
2. 🚧 **Phase 2 In Progress** - God object decomposition (9% done)
3. ⏳ **Phase 3 Partial** - Abstractions (transform pattern done, visitor pending)
4. ⏳ **Phase 4 Planned** - Coupling resolution

**Recommendation**: Focus on **missing error detection** first (TS2304/TS2318/TS2307). This will:
- Eliminate "Any poisoning" effect
- Unlock real type errors currently hidden
- Improve conformance significantly
- Enable proper testing of type system

Then continue **incremental refactoring** (solver → checker decomposition) while implementing **critical unsoundness rules** in parallel.

### 9.3 Key Metrics to Track

- **Conformance**: 41.5% → Target: 60%+ (short-term), 80%+ (medium-term)
- **God Objects**: 6 files → Target: 2-3 files (after decomposition)
- **Unsoundness Rules**: 33% → Target: 60%+ (Phase 2 complete)
- **Stability**: 112 crashes → Target: <10 crashes
- **Largest Function**: 2,214 lines → Target: <500 lines

---

## Appendix: Key Documents Referenced

- `docs/ARCHITECTURE_AUDIT_REPORT.md` - Comprehensive architecture audit
- `docs/ARCHITECTURE_WORK_SUMMARY.md` - Refactoring progress tracking
- `docs/UNSOUNDNESS_AUDIT.md` - Compatibility layer implementation status
- `docs/TRANSFORM_ARCHITECTURE.md` - Transform system architecture
- `specs/SOLVER.md` - Solver design document (1,116 lines)
- `specs/WASM_ARCHITECTURE.md` - WASM build and runtime architecture
- `specs/TS_UNSOUNDNESS_CATALOG.md` - 44 unsoundness rules catalog
- `specs/DIAGNOSTICS.md` - Diagnostic code mapping
- `PROJECT_DIRECTION.md` - Current priorities and conformance status

---

**Report Generated**: 2026-01-23  
**Next Review**: After Phase 2 completion or significant conformance improvement
