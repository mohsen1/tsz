# Architecture Audit Report: Project Zang (tsz)

**Date**: January 2026
**Auditor**: Claude Code Deep Analysis
**Codebase Version**: Branch `claude/architecture-audit-report-jluIf`

---

## Executive Summary

This report presents a comprehensive architecture audit of the Project Zang TypeScript compiler written in Rust. The analysis reveals **critical architectural debt** that, if left unaddressed, will significantly impede future development and maintenance.

### Key Metrics

| Metric | Value | Severity |
|--------|-------|----------|
| **God Object Files** | 6 files > 2,000 lines | CRITICAL |
| **Largest Function** | 2,437 lines (`check_subtype_inner`) | CRITICAL |
| **Total `unwrap()`/`expect()` calls** | 5,036 | HIGH |
| **Code Duplication Instances** | 60+ significant duplicates | HIGH |
| **TODO/FIXME Comments** | 49 unresolved | MEDIUM |
| **Circular Dependencies** | 2 identified | HIGH |

### Critical Files Requiring Immediate Attention

| File | Lines | Primary Issue |
|------|-------|---------------|
| `checker/state.rs` | **27,525** | God object with 554 functions |
| `parser/state.rs` | **10,762** | Massive code duplication |
| `solver/subtype.rs` | **4,734** | Single 2,437-line function |
| `solver/evaluate.rs` | **5,784** | Complex tangled logic |
| `solver/operations.rs` | **3,416** | API inconsistency |
| `transforms/class_es5_ir.rs` | **2,588** | 83 lines of exact duplication |

---

## Table of Contents

1. [Critical Issues](#1-critical-issues)
2. [God Object Anti-Pattern](#2-god-object-anti-pattern)
3. [Code Duplication Analysis](#3-code-duplication-analysis)
4. [Module Coupling & Dependencies](#4-module-coupling--dependencies)
5. [Function Complexity](#5-function-complexity)
6. [API Inconsistencies](#6-api-inconsistencies)
7. [Error Handling Concerns](#7-error-handling-concerns)
8. [Missing Abstractions](#8-missing-abstractions)
9. [Technical Debt Summary](#9-technical-debt-summary)
10. [Remediation Roadmap](#10-remediation-roadmap)

---

## 1. Critical Issues

### 1.1 The "Big 6" Monster Files

These six files account for **54,261 lines** of the most critical compiler logic and exhibit severe architectural problems:

```
checker/state.rs      27,525 lines  ████████████████████████████░░  51%
parser/state.rs       10,762 lines  ██████████░░░░░░░░░░░░░░░░░░░░  20%
solver/evaluate.rs     5,784 lines  █████░░░░░░░░░░░░░░░░░░░░░░░░░  11%
solver/subtype.rs      4,734 lines  ████░░░░░░░░░░░░░░░░░░░░░░░░░░   9%
solver/operations.rs   3,416 lines  ███░░░░░░░░░░░░░░░░░░░░░░░░░░░   6%
emitter/mod.rs         2,040 lines  ██░░░░░░░░░░░░░░░░░░░░░░░░░░░░   4%
```

### 1.2 Circular Dependencies

**Dependency 1: Emitter ↔ Transforms**
```
emitter/mod.rs:32-34
  ├── imports → transforms/class_es5
  ├── imports → transforms/enum_es5
  └── imports → transforms/namespace_es5

transforms/*
  └── uses emitter output formats
```

**Dependency 2: Lowering ↔ Transforms**
```
lowering_pass.rs:47-48
  ├── imports → transforms/arrow_es5::contains_this_reference
  └── imports → transforms/private_fields_es5::is_private_identifier
```

### 1.3 Conformance Gap

Current conformance test results show significant gaps:
- **4,636** missing TS2304 ("Cannot find name") errors
- **3,492** missing TS2318 ("Cannot find global type") errors
- **2,331** missing TS2307 ("Cannot find module") errors
- **54 tests** timeout without completion
- **112 worker crashes** (WASM panics/stack overflows)

---

## 2. God Object Anti-Pattern

### 2.1 CheckerState: The 27,525-Line Monster

`/home/user/tsz/src/checker/state.rs` is the worst offender in the codebase:

**Statistics:**
- **554 functions** in a single `impl` block
- **2,182+ `self.ctx` accesses** indicating heavy shared state
- **33 error/emit functions** for diagnostic reporting
- **64 `is_*` predicate functions**
- **27 `get_type_of_*` functions**

**Responsibilities Crammed Into One File:**
1. Type Computation (27+ functions)
2. Type Checking (100+ functions)
3. Symbol Resolution
4. Accessibility Checking
5. Flow Analysis
6. Error Reporting
7. Parameter Validation

**Largest Function: `get_type_of_identifier` (1,183 lines)**
```rust
// Lines 6076-7258 - Single function handling:
// - Global type resolution
// - Symbol lookup
// - TDZ violation checking
// - Import handling
// - Type-only member detection
// - ES2015+ type detection
// - Keyword handling (undefined, NaN, Infinity, Symbol)
// - Intermingled error reporting
```

### 2.2 ParserState: 10,762 Lines with Heavy Duplication

**24 look-ahead functions** all following identical pattern:
```rust
fn look_ahead_is_X(&mut self) -> bool {
    let snapshot = self.scanner.save_state();
    let current = self.current_token;
    self.next_token();
    let result = /* check condition */;
    self.scanner.restore_state(snapshot);
    self.current_token = current;
    result
}
```

**11 identical modifier parsing branches:**
```rust
// Lines 3005-3096: Copy-pasted for each modifier
SyntaxKind::StaticKeyword => {
    self.next_token();
    self.arena.create_modifier(SyntaxKind::StaticKeyword, start_pos)
}
SyntaxKind::PublicKeyword => {
    self.next_token();
    self.arena.create_modifier(SyntaxKind::PublicKeyword, start_pos)
}
// ... repeated 9 more times
```

### 2.3 SubtypeChecker: 2,437-Line Function

`check_subtype_inner()` in `/home/user/tsz/src/solver/subtype.rs` (lines 390-2827) handles:
- Intrinsic types
- Literal types
- Union/intersection distribution
- Type parameters
- Arrays/tuples
- Objects
- Functions

**This single function is untestable as a unit.**

---

## 3. Code Duplication Analysis

### 3.1 Critical Duplicates (Exact Copies)

#### `is_numeric_property_name` - 4 Identical Implementations

| Location | Lines |
|----------|-------|
| `solver/operations.rs` | 1621-1624 |
| `solver/evaluate.rs` | 5296-5299 |
| `solver/subtype.rs` | 2881-2884 |
| `solver/infer.rs` | 1522-1525 |

```rust
// IDENTICAL CODE in all 4 locations:
fn is_numeric_property_name(&self, name: Atom) -> bool {
    let prop_name = self.interner.resolve_atom_ref(name);
    InferenceContext::is_numeric_literal_name(prop_name.as_ref())
}
```

#### Parameter Extraction - 200+ Lines Duplicated

```
checker/state.rs:3499-3508  → extract_params_from_signature_in_type_literal
checker/state.rs:3510-3575  → extract_params_from_parameter_list_in_type_literal
checker/state.rs:4597-4606  → extract_params_from_signature
checker/state.rs:4659-4724  → extract_params_from_parameter_list
```

Functions 1+2 duplicate functions 3+4 with ~95% identical logic.

#### Accessor Map Collection - 83 Lines Duplicated

In `transforms/class_es5_ir.rs`:
- Lines 805-841: Instance accessor collection
- Lines 1001-1036: Static accessor collection

**36 lines of exact duplication** with only a modifier check inverted.

### 3.2 High-Priority Duplicates

#### `is_assignable_to` - 7 Different Implementations

| Location | Description |
|----------|-------------|
| `solver/compat.rs:175` | Main entry (is_assignable) |
| `solver/compat.rs:599` | Private wrapper |
| `solver/subtype.rs:270` | Delegates to is_subtype_of |
| `solver/subtype.rs:4695` | Standalone function |
| `solver/narrowing.rs:662` | Different implementation |
| `solver/operations.rs:38` | Trait definition |
| `checker/state.rs:12878` | Public method wrapper |

#### Generator Type Checking - 60+ Lines

`solver/contextual.rs:887-950`:
- `is_async_generator_type` (lines 887-914)
- `is_sync_generator_type` (lines 919-950)

**~80% identical logic**, only Promise detection differs.

### 3.3 Pattern Duplication Statistics

| Pattern | Occurrences | Impact |
|---------|-------------|--------|
| Save/restore scanner state | 60+ | 300+ redundant lines |
| `match node.kind` dispatchers | 48+ in checker/state.rs | Repeated match arms |
| Arena get/access pattern | 500+ | Boilerplate everywhere |
| Modifier parsing | 11 identical branches | 91 wasted lines |
| Diagnostic imports | 47 local imports | Maintenance burden |

---

## 4. Module Coupling & Dependencies

### 4.1 Layering Violations

**Expected Layer Order:**
```
Parser → Binder → Checker → Lowering → Transforms → Emitter
```

**Actual Violations:**

```
lowering_pass.rs:47-48
  └── imports from transforms/  (upward reference!)

emitter/mod.rs:32-34
  └── instantiates transform emitters directly

emit_context.rs:12-13
  └── imports transform state types
```

### 4.2 Feature Flags Scattered Across 5+ Modules

| Module | Flags |
|--------|-------|
| `emit_context.rs` | `target_es5`, `auto_detect_module`, `ModuleTransformState`, `ArrowTransformState` |
| `emitter/mod.rs` | `set_target_es5()`, `set_auto_detect_module()` |
| `lowering_pass.rs` | `commonjs_mode`, `has_export_assignment` |
| `transform_context.rs` | `TransformDirective::ModuleWrapper` |

**Problem:** No single source of truth. Inconsistent naming (`target_es5` vs `commonjs_mode` vs `is_commonjs()`).

### 4.3 WASM Feature Gates in Core lib.rs

```rust
// lib.rs:149-173 - 5 modules conditionally compiled
#[cfg(not(target_arch = "wasm32"))]
pub mod cli;

#[cfg(not(target_arch = "wasm32"))]
pub mod module_resolver;
// ... 3 more
```

Platform concerns mixed with core library organization.

---

## 5. Function Complexity

### 5.1 Functions Exceeding 200 Lines

| Function | File | Lines | Size |
|----------|------|-------|------|
| `check_subtype_inner` | solver/subtype.rs | 390-2827 | **2,437** |
| `get_type_of_identifier` | checker/state.rs | 6076-7258 | **1,183** |
| `check_property_inheritance_compatibility` | checker/state.rs | various | 304 |
| `get_type_of_object_literal` | checker/state.rs | 11447-11727 | 281 |
| `compute_type_of_node` | checker/state.rs | 1000-1275 | 276 |
| `parse_class_member` | parser/state.rs | 3335-3591 | 259 |
| `resolve_property_access_inner` | solver/operations.rs | 2051-2250 | 250+ |
| `constrain_types_impl` | solver/operations.rs | 1017-1250 | 233 |
| `emit_static_members_ir` | transforms/class_es5_ir.rs | 993-1206 | 213 |
| `resolve_generic_call_inner` | solver/operations.rs | 281-472 | 191 |

### 5.2 Deeply Nested Conditionals

**parser/state.rs:1194-1231 (5 levels deep):**
```rust
SyntaxKind::AsyncKeyword => {
    if self.look_ahead_is_async_function() {
        // ...
    } else if self.look_ahead_is_async_declaration() {
        match self.token() {
            SyntaxKind::ClassKeyword => { ... }
            SyntaxKind::NamespaceKeyword | SyntaxKind::ModuleKeyword => {
                if self.look_ahead_is_module_declaration() {
                    // 5 levels deep here
                }
            }
        }
    }
}
```

**checker/state.rs:20111-20250 (check_expression_for_early_property_access):**
- 9+ match arms
- Each recursively calling back to the same function
- Multiple nested if-let chains

---

## 6. API Inconsistencies

### 6.1 TypeResolver Trait Confusion

Three implementations with different semantics:

| Implementation | Behavior |
|----------------|----------|
| `NoopResolver` | Returns `None` (not an error) |
| `TypeEnvironment` | Returns cached types |
| `IndexSignatureResolver` | Returns `TypeId::ERROR` |

**No documentation on when to return `None` vs `Some(ERROR)`.**

### 6.2 TypeId Sentinel Values Used Inconsistently

| Scenario | Used Value | Location |
|----------|------------|----------|
| Missing type annotation | `ERROR` | lower.rs:297-302 |
| Failed inference | `ERROR` | operations.rs:407, 416 |
| Unknown property | `UNKNOWN` | operations.rs:2017 |
| Default for any[] | `ANY` | operations.rs:2001 |

**No documented semantics for which to use when.**

### 6.3 Multiple Competing Entry Points

In `solver/operations.rs`:
- Lines 1807-1820: Standalone function versions
- Lines 1920-1957: Struct-based implementations

No guidance on which to use.

---

## 7. Error Handling Concerns

### 7.1 Unsafe Unwrap Usage

**5,036 `unwrap()` or `expect()` calls** across the codebase.

Top offenders:
- Test files (expected, but ~3,500 calls)
- `solver/infer_tests.rs`: 525 calls
- `source_map_tests_*.rs`: ~1,768 calls combined
- Production code: ~1,500 calls (**concerning**)

### 7.2 Silent Error Swallowing

```rust
// solver/operations.rs:218
.unwrap_or(CallResult::NotCallable {..})

// solver/evaluate.rs:580, 591, 1824
.unwrap_or(TypeId::UNDEFINED)
.unwrap_or(TypeId::UNKNOWN)
```

Errors silently converted to sentinel values, masking root causes.

### 7.3 Lost Error Context

```rust
// solver/operations.rs:422-430
return CallResult::ArgumentTypeMismatch {
    index: 0, // Placeholder - loses which parameter failed
    expected: constraint_ty,
    actual: ty,
};
```

### 7.4 Unresolved Technical Debt

**49 TODO/FIXME/HACK comments** across 19 files:

| File | Count | Notable |
|------|-------|---------|
| `solver/evaluate_tests.rs` | 11 | Test coverage gaps |
| `checker/state.rs` | 6 | Core functionality incomplete |
| `checker_state_tests.rs` | 5 | Known test gaps |
| `solver/types.rs` | 4 | Type system TODOs |
| `solver/diagnostics.rs` | 3 | Error reporting incomplete |

---

## 8. Missing Abstractions

### 8.1 No Type Visitor Pattern

Instead of a visitor, massive match statements repeated throughout:

```rust
// checker/state.rs lines 1006-1273
match node.kind {
    k if k == SyntaxKind::Identifier as u16 => ...,
    k if k == SyntaxKind::ThisKeyword as u16 => ...,
    // ... 40+ branches inline
}
```

This pattern appears **48+ times** in checker/state.rs alone.

### 8.2 No Error Handler Abstraction

**33 separate error functions** in checker/state.rs:
```rust
error_type_not_assignable_at(...)
error_type_not_assignable_with_reason_at(...)
error_property_missing_at(...)
error_property_not_exist_at(...)
error_argument_not_assignable_at(...)
// ... 28 more
```

### 8.3 No Transform Interface

Transform emitters have no common trait:
- `ClassES5Emitter::emit_class()`
- `EnumES5Emitter::emit_enum()`
- `NamespaceES5Emitter::emit_namespace()`

Each implements its own API, making extension difficult.

### 8.4 No Feature Flag Manager

No unified API to query/set compilation features. Flags scattered and synchronized manually.

### 8.5 No Module Format Strategy

CommonJS, UMD, AMD, System formats hardcoded in Emitter rather than using strategy pattern.

---

## 9. Technical Debt Summary

### 9.1 Severity Matrix

| Category | Critical | High | Medium | Low |
|----------|----------|------|--------|-----|
| God Objects | 2 | 4 | - | - |
| Code Duplication | 4 | 6 | 8 | - |
| Circular Dependencies | - | 2 | - | - |
| Function Complexity | 3 | 7 | 10 | - |
| API Inconsistency | - | 4 | 6 | - |
| Error Handling | - | 2 | 3 | - |
| Missing Abstractions | - | 5 | 3 | - |
| **TOTALS** | **9** | **30** | **30** | **0** |

### 9.2 Risk Assessment

**Immediate Risks:**
1. **Maintainability**: Changes to checker/state.rs require understanding 27K lines
2. **Testability**: 2,437-line function cannot be unit tested
3. **Onboarding**: New developers face overwhelming complexity
4. **Bug Introduction**: Copy-paste duplication leads to divergent fixes

**Future Risks:**
1. **Performance**: Monolithic functions prevent targeted optimization
2. **Parallelization**: Tight coupling limits concurrent execution
3. **Feature Addition**: No clear extension points for new TypeScript features
4. **Debugging**: Tangled logic makes root cause analysis difficult

---

## 10. Remediation Roadmap

### Phase 1: Critical Stabilization (Immediate)

| Task | Effort | Impact |
|------|--------|--------|
| Extract `is_numeric_property_name` to shared utility | 1 day | Eliminates 4 duplicates |
| Consolidate parameter extraction functions | 2-3 days | Removes 200+ lines |
| Document TypeId sentinel semantics | 1 day | Prevents bugs |
| Fix accessor map duplication in class_es5_ir | 1 day | Removes 83 lines |

### Phase 2: Break Up God Objects (1-2 Sprints)

**checker/state.rs decomposition:**
```
checker/
├── state.rs           (reduced to orchestration ~2,000 lines)
├── type_computation.rs (get_type_of_* functions)
├── type_checking.rs    (check_* functions)
├── symbol_resolver.rs  (symbol resolution)
├── accessibility.rs    (access checking)
├── flow_analysis.rs    (control flow)
└── error_reporter.rs   (diagnostic emission)
```

**solver/subtype.rs decomposition:**
```
solver/
├── subtype.rs         (reduced coordinator)
├── subtype_rules/
│   ├── intrinsics.rs
│   ├── literals.rs
│   ├── unions.rs
│   ├── objects.rs
│   ├── functions.rs
│   └── tuples.rs
```

### Phase 3: Introduce Abstractions (2-3 Sprints)

1. **Type Visitor Pattern**: Replace 48+ match statements
2. **Error Handler Trait**: Consolidate 33 error functions
3. **Transform Interface**: Common trait for ES5 transforms
4. **Feature Flag Manager**: Single source of truth
5. **Module Format Strategy**: Pluggable module systems

### Phase 4: Resolve Coupling (1-2 Sprints)

1. Break Emitter ↔ Transforms circular dependency
2. Extract transform helpers from lowering_pass
3. Create platform abstraction layer for WASM
4. Consolidate caching strategies (3 different approaches currently)

### Phase 5: Testing & Documentation (Ongoing)

1. Add configuration-driven tests for feature flags
2. Document TypeResolver trait semantics
3. Add tests for recursion depth limits
4. Test error recovery paths

---

## Appendix A: File Size Distribution

```
Files > 5,000 lines:
  checker/state.rs        27,525 ████████████████████████████
  parser/state.rs         10,762 ███████████
  solver/evaluate.rs       5,784 ██████

Files 2,000-5,000 lines:
  solver/subtype.rs        4,734 █████
  solver/operations.rs     3,416 ███
  transforms/class_es5_ir  2,588 ███
  solver/lower.rs          2,417 ██
  emitter/mod.rs           2,040 ██
```

## Appendix B: Dependency Graph (Simplified)

```
                    ┌─────────────────────┐
                    │      CLI/Driver     │
                    └──────────┬──────────┘
                               │
           ┌───────────────────┼───────────────────┐
           ▼                   ▼                   ▼
    ┌──────────┐        ┌──────────┐        ┌──────────┐
    │  Parser  │───────►│  Binder  │───────►│ Checker  │
    └──────────┘        └──────────┘        └────┬─────┘
                                                 │
                                          ┌──────┴──────┐
                                          ▼             ▼
                                    ┌──────────┐  ┌──────────┐
                                    │  Solver  │  │ Lowering │◄─┐
                                    └──────────┘  └────┬─────┘  │
                                                       │        │
                                                       ▼        │
                                                 ┌──────────┐   │
                                    VIOLATION───►│Transforms│───┘
                                                 └────┬─────┘
                                                      │
                                          CIRCULAR ───┤
                                                      ▼
                                                 ┌──────────┐
                                                 │ Emitter  │
                                                 └──────────┘
```

## Appendix C: Recommended Reading Order for New Developers

Given the current architecture, the recommended order to understand the codebase:

1. `specs/` - Design documentation
2. `src/scanner.rs` - Token definitions
3. `src/parser/node.rs` - AST node types
4. `src/parser/base.rs` - Core parser types
5. `src/binder/` - Symbol table basics
6. `src/solver/types.rs` - Type system types
7. `PROJECT_DIRECTION.md` - Strategic roadmap

**Avoid starting with:**
- `checker/state.rs` (too large to comprehend)
- `solver/subtype.rs` (too complex without context)

---

*Report generated through comprehensive static analysis and manual code review.*
