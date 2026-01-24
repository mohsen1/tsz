# God Object Decomposition: Step-by-Step Guide

**Date**: 2026-01-24  
**Status**: Active  
**Current Phase**: Phase 2 - Break Up God Objects

---

## Overview

This document provides a step-by-step plan for decomposing the "Big 6" god objects in the codebase. Work should proceed incrementally, with each step verified by tests before moving to the next.

### The "Big 6" God Objects

| File | Lines | Status | Priority |
|------|-------|--------|----------|
| `checker/state.rs` | 27,525 | ⏳ Pending | P2 (after solver) |
| `parser/state.rs` | 10,762 | ⏳ Pending | P3 (low priority) |
| `solver/evaluate.rs` | 5,784 | ⏳ Pending | P2 (after solver) |
| `solver/subtype.rs` | 4,734 → ~2,214 | 🚧 In Progress | **P1 (CURRENT)** |
| `solver/operations.rs` | 3,416 | ⏳ Pending | P2 (after solver) |
| `emitter/mod.rs` | 2,040 | ⏳ Pending | P3 (acceptable) |

---

## Priority 1: solver/subtype.rs (CURRENT FOCUS)

**Goal**: Reduce `check_subtype_inner` from ~2,214 lines to ~500 lines (coordinator)  
**Progress**: 9% complete (7 helper methods extracted, ~223 lines)  
**Remaining**: ~1,700+ lines to extract

### Step 1: Extract Object Subtyping Logic (~400-600 lines)

**Target Extraction**: 400-600 lines

#### 1.1 Identify Object Subtyping Code
- [ ] Read `check_subtype_inner` and locate all object subtyping logic
- [ ] Look for code handling `TypeKey::Object` in both source and target
- [ ] Identify property matching logic
- [ ] Identify index signature checking
- [ ] Identify excess property checking
- [ ] Identify freshness handling

#### 1.2 Extract Property Matching Helper
- [ ] Create `check_object_properties` helper method (~150-200 lines)
- [ ] Move property-by-property comparison logic
- [ ] Handle required vs optional properties
- [ ] Handle readonly modifiers
- [ ] Run tests: `cargo test --lib`
- [ ] Run clippy: `cargo clippy -- -D warnings`
- [ ] Commit: `refactor(solver): Extract object property matching into helper method`

#### 1.3 Extract Index Signature Helper
- [ ] Create `check_object_index_signatures` helper method (~100-150 lines)
- [ ] Move string index signature checking
- [ ] Move number index signature checking
- [ ] Handle readonly index signatures
- [ ] Run tests: `cargo test --lib`
- [ ] Run clippy: `cargo clippy -- -D warnings`
- [ ] Commit: `refactor(solver): Extract index signature checking into helper method`

#### 1.4 Extract Excess Property Checking
- [ ] Create `check_excess_properties` helper method (~80-120 lines)
- [ ] Move excess property detection logic
- [ ] Handle freshness tracking
- [ ] Integrate with FreshnessTracker
- [ ] Run tests: `cargo test --lib`
- [ ] Run clippy: `cargo clippy -- -D warnings`
- [ ] Commit: `refactor(solver): Extract excess property checking into helper method`

#### 1.5 Update Documentation
- [ ] Update ARCHITECTURE_AUDIT_REPORT.md with progress
- [ ] Update line count metrics
- [ ] Document extracted helpers

### Step 2: Extract Template Literal Type Checking (~200-300 lines)

**Target Extraction**: 200-300 lines

#### 2.1 Identify Template Literal Code
- [ ] Locate template literal type handling in `check_subtype_inner`
- [ ] Identify pattern matching logic
- [ ] Identify backtracking logic
- [ ] Identify literal spans vs type holes

#### 2.2 Extract Template Literal Pattern Matching
- [ ] Create `check_template_literal_subtype` helper method (~150-200 lines)
- [ ] Move pattern matching logic
- [ ] Move backtracking algorithm
- [ ] Handle literal spans and wildcards
- [ ] Run tests: `cargo test --lib`
- [ ] Run clippy: `cargo clippy -- -D warnings`
- [ ] Commit: `refactor(solver): Extract template literal type checking into helper method`

#### 2.3 Extract Template Literal Helpers
- [ ] Create `template_literal_matches_pattern` helper (~50-100 lines)
- [ ] Move pattern validation logic
- [ ] Run tests: `cargo test --lib`
- [ ] Run clippy: `cargo clippy -- -D warnings`
- [ ] Commit: `refactor(solver): Extract template literal pattern helpers`

#### 2.4 Update Documentation
- [ ] Update ARCHITECTURE_AUDIT_REPORT.md with progress
- [ ] Update line count metrics

### Step 3: Extract Mapped/Conditional Type Evaluation (~300-400 lines)

**Target Extraction**: 300-400 lines

#### 3.1 Identify Mapped Type Code
- [ ] Locate mapped type handling in `check_subtype_inner`
- [ ] Identify conditional type distribution
- [ ] Identify type parameter constraints
- [ ] Identify homomorphic mapped type logic

#### 3.2 Extract Mapped Type Evaluation
- [ ] Create `check_mapped_subtype` helper method (~100-150 lines)
- [ ] Move mapped type evaluation logic
- [ ] Handle homomorphic mapped types
- [ ] Run tests: `cargo test --lib`
- [ ] Run clippy: `cargo clippy -- -D warnings`
- [ ] Commit: `refactor(solver): Extract mapped type evaluation into helper method`

#### 3.3 Extract Conditional Type Distribution
- [ ] Create `check_conditional_subtype` helper method (~100-150 lines)
- [ ] Move conditional type distribution logic
- [ ] Handle distributive flags
- [ ] Handle branch compatibility
- [ ] Run tests: `cargo test --lib`
- [ ] Run clippy: `cargo clippy -- -D warnings`
- [ ] Commit: `refactor(solver): Extract conditional type distribution into helper method`

#### 3.4 Extract Type Parameter Constraint Checking
- [ ] Create `check_type_parameter_constraints` helper (~50-100 lines)
- [ ] Move constraint validation logic
- [ ] Run tests: `cargo test --lib`
- [ ] Run clippy: `cargo clippy -- -D warnings`
- [ ] Commit: `refactor(solver): Extract type parameter constraint checking`

#### 3.5 Update Documentation
- [ ] Update ARCHITECTURE_AUDIT_REPORT.md with progress
- [ ] Update line count metrics

### Step 4: Extract Primitive/Intrinsic Type Checking (~200-300 lines)

**Target Extraction**: 200-300 lines

#### 4.1 Identify Intrinsic Type Code
- [ ] Locate intrinsic type handling in `check_subtype_inner`
- [ ] Identify primitive hierarchy logic
- [ ] Identify literal to intrinsic conversion
- [ ] Identify apparent type handling

#### 4.2 Extract Intrinsic Type Hierarchy
- [ ] Create `check_intrinsic_hierarchy` helper method (~100-150 lines)
- [ ] Move intrinsic subtype hierarchy (never, void, null, undefined, etc.)
- [ ] Run tests: `cargo test --lib`
- [ ] Run clippy: `cargo clippy -- -D warnings`
- [ ] Commit: `refactor(solver): Extract intrinsic type hierarchy checking`

#### 4.3 Extract Literal to Intrinsic Conversion
- [ ] Create `check_literal_to_intrinsic` helper (~50-100 lines)
- [ ] Move literal type conversion logic
- [ ] Run tests: `cargo test --lib`
- [ ] Run clippy: `cargo clippy -- -D warnings`
- [ ] Commit: `refactor(solver): Extract literal to intrinsic conversion`

#### 4.4 Extract Apparent Type Handling
- [ ] Create `get_apparent_type` helper (~50-100 lines)
- [ ] Move primitive to interface conversion
- [ ] Run tests: `cargo test --lib`
- [ ] Run clippy: `cargo clippy -- -D warnings`
- [ ] Commit: `refactor(solver): Extract apparent type handling`

#### 4.5 Update Documentation
- [ ] Update ARCHITECTURE_AUDIT_REPORT.md with progress
- [ ] Update line count metrics

### Step 5: Verify and Assess Progress

#### 5.1 Verification
- [ ] Run full test suite: `cargo test --lib`
- [ ] Run clippy: `cargo clippy -- -D warnings`
- [ ] Check line count: `wc -l src/solver/subtype.rs`
- [ ] Verify `check_subtype_inner` is now ~500-700 lines

#### 5.2 Assessment
- [ ] Calculate extraction percentage
- [ ] Identify any remaining large sections
- [ ] Update ARCHITECTURE_WORK_SUMMARY.md with metrics
- [ ] Create deep analysis report for commits

#### 5.3 Decide Next Steps
- [ ] If `check_subtype_inner` > 700 lines: Continue extraction
- [ ] If `check_subtype_inner` < 700 lines: Move to module restructure (Step 6)

### Step 6: Restructure into Module Hierarchy (FINAL)

**Target**: Create `solver/subtype_rules/` module structure

#### 6.1 Create Module Structure
- [ ] Create `src/solver/subtype_rules/` directory
- [ ] Create `src/solver/subtype_rules/mod.rs`
- [ ] Plan module organization:
  - `intrinsics.rs` — Primitive types
  - `literals.rs` — Literal types
  - `unions.rs` — Union/intersection logic
  - `tuples.rs` — Array/tuple checking
  - `objects.rs` — Object property matching
  - `functions.rs` — Callable signatures
  - `templates.rs` — Template literal types
  - `generics.rs` — Type params, applications, mapped types

#### 6.2 Move Helpers to Modules
- [ ] Move intrinsic helpers to `intrinsics.rs`
- [ ] Move literal helpers to `literals.rs`
- [ ] Move union/intersection helpers to `unions.rs`
- [ ] Move tuple helpers to `tuples.rs`
- [ ] Move object helpers to `objects.rs`
- [ ] Move function helpers to `functions.rs`
- [ ] Move template literal helpers to `templates.rs`
- [ ] Move mapped/conditional helpers to `generics.rs`

#### 6.3 Update Imports
- [ ] Update `src/solver/subtype.rs` to import from `subtype_rules`
- [ ] Update module exports in `subtype_rules/mod.rs`
- [ ] Make helpers `pub(crate)` as needed

#### 6.4 Verify Module Structure
- [ ] Run full test suite: `cargo test --lib`
- [ ] Run clippy: `cargo clippy -- -D warnings`
- [ ] Verify all imports work correctly

#### 6.5 Final Documentation
- [ ] Update ARCHITECTURE_AUDIT_REPORT.md with final structure
- [ ] Update ARCHITECTURE_WORK_SUMMARY.md
- [ ] Mark solver/subtype.rs as ✅ Complete in tracking docs
- [ ] Commit: `refactor(solver): Restructure subtype checking into module hierarchy`

---

## Priority 2: checker/state.rs

**Goal**: Reduce from 27,525 lines to ~2,000 lines (coordinator)  
**Progress**: 660 lines extracted (promise, iterable modules)  
**Status**: ⏳ Pending (start after solver/subtype.rs complete)

### Step 7: Extract Type Computation Logic (~3,000-4,000 lines)

**Target**: Create/expand `checker/type_computation.rs`

#### 7.1 Identify `get_type_of_*` Functions
- [ ] List all `get_type_of_*` functions in `checker/state.rs`
- [ ] Count lines for each function
- [ ] Identify dependencies between functions
- [ ] Plan extraction order (least dependent first)

#### 7.2 Extract Basic Type Computation
- [ ] Extract `get_type_of_literal` family (~200-300 lines)
- [ ] Extract `get_type_of_identifier` (~1,183 lines)
- [ ] Extract `get_type_of_property_access` (~200-300 lines)
- [ ] Extract `get_type_of_element_access` (~150-200 lines)
- [ ] Run tests after each extraction
- [ ] Commit after each extraction

#### 7.3 Extract Object Type Computation
- [ ] Extract `get_type_of_object_literal` (~281 lines)
- [ ] Extract `get_type_of_array_literal` (~150-200 lines)
- [ ] Extract `get_type_of_tuple` (~100-150 lines)
- [ ] Run tests after each extraction
- [ ] Commit after each extraction

#### 7.4 Extract Function Type Computation
- [ ] Extract `get_type_of_function_expression` (~200-300 lines)
- [ ] Extract `get_type_of_arrow_function` (~150-200 lines)
- [ ] Extract `get_type_of_call_expression` (~200-300 lines)
- [ ] Run tests after each extraction
- [ ] Commit after each extraction

#### 7.5 Update Module Structure
- [ ] Ensure `checker/type_computation.rs` has all extracted functions
- [ ] Add `pub(crate)` visibility as needed
- [ ] Update imports in `checker/state.rs`
- [ ] Run full test suite
- [ ] Update documentation

### Step 8: Extract Type Checking Logic (~3,000-4,000 lines)

**Target**: Create `checker/type_checking.rs`

#### 8.1 Identify `check_*` Functions
- [ ] List all `check_*` functions in `checker/state.rs`
- [ ] Count lines for each function
- [ ] Identify dependencies between functions
- [ ] Plan extraction order

#### 8.2 Extract Assignment Checking
- [ ] Extract `check_assignment` (~200-300 lines)
- [ ] Extract `check_type_assignable_to` (~150-200 lines)
- [ ] Extract `check_property_assignment` (~100-150 lines)
- [ ] Run tests after each extraction
- [ ] Commit after each extraction

#### 8.3 Extract Expression Checking
- [ ] Extract `check_binary_expression` (~200-300 lines)
- [ ] Extract `check_unary_expression` (~100-150 lines)
- [ ] Extract `check_conditional_expression` (~150-200 lines)
- [ ] Run tests after each extraction
- [ ] Commit after each extraction

#### 8.4 Extract Declaration Checking
- [ ] Extract `check_variable_declaration` (~200-300 lines)
- [ ] Extract `check_function_declaration` (~300-400 lines)
- [ ] Extract `check_class_declaration` (~400-600 lines)
- [ ] Extract `check_interface_declaration` (~300-500 lines)
- [ ] Run tests after each extraction
- [ ] Commit after each extraction

#### 8.5 Update Module Structure
- [ ] Ensure `checker/type_checking.rs` has all extracted functions
- [ ] Add `pub(crate)` visibility as needed
- [ ] Update imports in `checker/state.rs`
- [ ] Run full test suite
- [ ] Update documentation

### Step 9: Extract Symbol Resolution Logic (~2,000-3,000 lines)

**Target**: Expand `checker/symbol_resolver.rs`

#### 9.1 Identify Symbol Resolution Functions
- [ ] List all `resolve_*` functions
- [ ] List all symbol lookup functions
- [ ] Count lines for each function
- [ ] Plan extraction order

#### 9.2 Extract Symbol Lookup
- [ ] Extract `resolve_name` (~200-300 lines)
- [ ] Extract `resolve_qualified_name` (~150-200 lines)
- [ ] Extract `resolve_entity_name` (~100-150 lines)
- [ ] Run tests after each extraction
- [ ] Commit after each extraction

#### 9.3 Extract Import Resolution
- [ ] Extract `resolve_import` (~200-300 lines)
- [ ] Extract `resolve_module_specifier` (~150-200 lines)
- [ ] Extract `resolve_external_module_name` (~100-150 lines)
- [ ] Run tests after each extraction
- [ ] Commit after each extraction

#### 9.4 Extract Type Resolution
- [ ] Extract `resolve_type_reference` (~200-300 lines)
- [ ] Extract `resolve_type_name` (~150-200 lines)
- [ ] Extract `resolve_lib_type` (~100-150 lines)
- [ ] Run tests after each extraction
- [ ] Commit after each extraction

#### 9.5 Update Module Structure
- [ ] Ensure `checker/symbol_resolver.rs` has all extracted functions
- [ ] Add `pub(crate)` visibility as needed
- [ ] Update imports in `checker/state.rs`
- [ ] Run full test suite
- [ ] Update documentation

### Step 10: Extract Flow Analysis Logic (~2,000-3,000 lines)

**Target**: Create `checker/flow_analysis.rs`

#### 10.1 Identify Flow Analysis Functions
- [ ] List all flow analysis functions
- [ ] List all type narrowing functions
- [ ] Count lines for each function
- [ ] Plan extraction order

#### 10.2 Extract Type Narrowing
- [ ] Extract `narrow_by_typeof` (~150-200 lines)
- [ ] Extract `narrow_by_instanceof` (~150-200 lines)
- [ ] Extract `narrow_by_discriminant` (~200-300 lines)
- [ ] Extract `narrow_by_truthiness` (~100-150 lines)
- [ ] Run tests after each extraction
- [ ] Commit after each extraction

#### 10.3 Extract Control Flow Analysis
- [ ] Extract `check_flow_usage` (~200-300 lines)
- [ ] Extract `analyze_control_flow` (~300-400 lines)
- [ ] Extract `compute_flow_type` (~200-300 lines)
- [ ] Run tests after each extraction
- [ ] Commit after each extraction

#### 10.4 Extract Definite Assignment
- [ ] Extract `check_definite_assignment` (~200-300 lines)
- [ ] Extract `is_definitely_assigned` (~100-150 lines)
- [ ] Run tests after each extraction
- [ ] Commit after each extraction

#### 10.5 Update Module Structure
- [ ] Create `src/checker/flow_analysis.rs`
- [ ] Move all extracted functions
- [ ] Add `pub(crate)` visibility as needed
- [ ] Update imports in `checker/state.rs`
- [ ] Run full test suite
- [ ] Update documentation

### Step 11: Extract Error Reporting (~1,000-1,500 lines)

**Target**: Expand `checker/error_reporter.rs`

#### 11.1 Identify Error Functions
- [ ] List all `error_*` functions (~33 functions)
- [ ] Count lines for each function
- [ ] Verify ErrorHandler trait coverage

#### 11.2 Extract Error Emission
- [ ] Move all `error_*` functions to `error_reporter.rs`
- [ ] Ensure trait implementation is complete
- [ ] Run tests after each extraction
- [ ] Commit after each extraction

#### 11.3 Update Module Structure
- [ ] Ensure `checker/error_reporter.rs` has all error functions
- [ ] Add `pub(crate)` visibility as needed
- [ ] Update imports in `checker/state.rs`
- [ ] Run full test suite
- [ ] Update documentation

### Step 12: Create Orchestration Layer (~2,000 lines)

**Target**: Reduce `checker/state.rs` to orchestration only

#### 12.1 Review Remaining Code
- [ ] Identify what's left in `checker/state.rs`
- [ ] Categorize remaining functions
- [ ] Decide what stays vs what gets extracted

#### 12.2 Create Orchestration Methods
- [ ] Keep main entry points (`check_source_file`, etc.)
- [ ] Keep coordination logic
- [ ] Keep shared state management
- [ ] Delegate to extracted modules

#### 12.3 Final Cleanup
- [ ] Remove duplicate code
- [ ] Optimize imports
- [ ] Add module documentation
- [ ] Run full test suite
- [ ] Run clippy
- [ ] Update all tracking documentation

#### 12.4 Verification
- [ ] Verify `checker/state.rs` is now ~2,000 lines
- [ ] Verify all tests pass
- [ ] Verify no clippy warnings
- [ ] Mark checker/state.rs as ✅ Complete

---

## Priority 3: solver/evaluate.rs

**Goal**: Reduce from 5,784 lines to ~2,000 lines  
**Status**: ⏳ Pending (start after checker/state.rs)

### Step 13: Plan solver/evaluate.rs Decomposition

#### 13.1 Analysis
- [ ] Read through `solver/evaluate.rs`
- [ ] Identify major sections
- [ ] Count lines for each section
- [ ] Identify extraction candidates

#### 13.2 Create Extraction Plan
- [ ] Document extraction targets
- [ ] Estimate effort for each extraction
- [ ] Plan module structure
- [ ] Update this TODO with detailed steps

---

## Priority 4: solver/operations.rs

**Goal**: Reduce from 3,416 lines to ~1,500 lines  
**Status**: ⏳ Pending (start after solver/evaluate.rs)

### Step 14: Plan solver/operations.rs Decomposition

#### 14.1 Analysis
- [ ] Read through `solver/operations.rs`
- [ ] Identify major sections
- [ ] Count lines for each section
- [ ] Identify extraction candidates

#### 14.2 Create Extraction Plan
- [ ] Document extraction targets
- [ ] Estimate effort for each extraction
- [ ] Plan module structure
- [ ] Update this TODO with detailed steps

---

## Priority 5: parser/state.rs (Low Priority)

**Goal**: Reduce from 10,762 lines to ~3,000 lines  
**Status**: ⏳ Pending (deferred until other god objects complete)

### Step 15: Plan parser/state.rs Decomposition

#### 15.1 Analysis
- [ ] Read through `parser/state.rs`
- [ ] Identify code duplication patterns
- [ ] Count lines for duplicate sections
- [ ] Identify extraction candidates

#### 15.2 Create Extraction Plan
- [ ] Document extraction targets
- [ ] Estimate effort for each extraction
- [ ] Plan module structure
- [ ] Update this TODO with detailed steps

---

## General Guidelines

### Before Each Extraction
1. Read the code thoroughly
2. Understand dependencies
3. Verify tests exist for the code
4. Plan the extraction

### During Each Extraction
1. Extract incrementally (200-400 lines at a time)
2. Run tests after each change
3. Run clippy after each change
4. Commit frequently with descriptive messages

### After Each Extraction
1. Verify all tests pass
2. Verify no clippy warnings
3. Update documentation
4. Update line count metrics
5. Assess progress

### Commit Message Format
- `refactor(module): Extract X into helper method` — For helper extractions
- `refactor(module): Move X to new module Y` — For module creation
- `docs: Update architecture metrics for X decomposition` — For documentation

### Testing Strategy
- Run unit tests: `cargo test --lib`
- Run with workers: `cargo test --lib -- --test-threads=8`
- Run clippy: `cargo clippy -- -D warnings`
- Run formatter: `cargo fmt`

### Documentation Updates
- Update ARCHITECTURE_AUDIT_REPORT.md after major milestones
- Update ARCHITECTURE_WORK_SUMMARY.md with progress
- Update line count metrics regularly
- Create deep analysis reports for commit batches

---

## Success Metrics

### Phase 2 Complete When:
- [ ] `solver/subtype.rs`: `check_subtype_inner` reduced to ~500 lines
- [ ] `solver/subtype.rs`: Module structure created (`subtype_rules/`)
- [ ] `checker/state.rs`: Reduced from 27,525 to ~2,000 lines
- [ ] `checker/state.rs`: Modules created (type_computation, type_checking, flow_analysis)
- [ ] `solver/evaluate.rs`: Reduced from 5,784 to ~2,000 lines
- [ ] `solver/operations.rs`: Reduced from 3,416 to ~1,500 lines
- [ ] All tests passing (100% pass rate)
- [ ] No clippy warnings
- [ ] Documentation updated

### Long-term Success Metrics
- [ ] Largest function < 500 lines
- [ ] God objects < 3 files > 2,000 lines
- [ ] Code duplication < 20 instances
- [ ] All architecture docs updated

---

**Last Updated**: 2026-01-24  
**Next Review**: After solver/subtype.rs completion
