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
| `checker/state.rs` | ~28,000 | 🚧 In Progress | **P1 (CURRENT)** |
| `parser/state.rs` | 10,762 | ⏳ Pending | P3 (low priority) |
| `solver/evaluate.rs` | 5,784 | ⏳ Pending | P2 (after checker) |
| `solver/subtype.rs` | 1,778 | ✅ **COMPLETE** | P1 (DONE) |
| `solver/operations.rs` | 3,416 | ⏳ Pending | P2 (after checker) |
| `emitter/mod.rs` | 2,040 | ⏳ Pending | P3 (acceptable) |

---

## Priority 1: solver/subtype.rs ✅ COMPLETE

**Goal**: Reduce `check_subtype_inner` from ~2,214 lines to ~500 lines (coordinator)
**Progress**: ✅ **COMPLETE** (~317 lines coordinator, ~3,781 lines in subtype_rules modules)

### Module Structure Created ✅

| Module | Lines | Purpose |
|--------|-------|---------|
| `intrinsics.rs` | 338 | Primitive/intrinsic type compatibility |
| `literals.rs` | 587 | Literal types and template literal matching |
| `unions.rs` | 361 | Union and intersection type logic |
| `tuples.rs` | 379 | Array and tuple compatibility |
| `objects.rs` | 544 | Object property matching and index signatures |
| `functions.rs` | 992 | Function/callable signature compatibility |
| `generics.rs` | 425 | Type parameters, references, and applications |
| `conditionals.rs` | 133 | Conditional type checking |
| **Total** | **3,781** | **Focused, maintainable code** |

### All Helper Methods Extracted ✅

**Step 1: Object Subtyping Logic** ✅ COMPLETE
- ✅ `check_object_subtype` - Core object property matching
- ✅ `check_property_compatibility` - Optional/readonly/type checking
- ✅ `check_string_index_compatibility` - String index signatures
- ✅ `check_number_index_compatibility` - Number index signatures
- ✅ `check_object_with_index_subtype` - Full object with index checking
- ✅ `check_object_to_indexed` - Simple object to indexed object
- ✅ `check_missing_property_against_index_signatures` - Index satisfaction

**Step 2: Template Literal Type Checking** ✅ COMPLETE
- ✅ `check_literal_to_intrinsic` - Literal to intrinsic conversion
- ✅ `check_literal_matches_template_literal` - Template literal pattern matching
- ✅ `match_template_literal_recursive` - Backtracking algorithm
- ✅ All `match_*_pattern` helper functions

**Step 3: Mapped/Conditional Type Evaluation** ✅ COMPLETE
- ✅ `check_conditional_subtype` - Conditional type checking
- ✅ `conditional_branches_subtype` - Source conditional distribution
- ✅ `subtype_of_conditional_target` - Target conditional handling

**Step 4: Primitive/Intrinsic Type Checking** ✅ COMPLETE
- ✅ `check_intrinsic_subtype` - Intrinsic type hierarchy
- ✅ `is_object_keyword_type` - Object keyword detection
- ✅ `is_callable_type` - Callable type detection
- ✅ `apparent_primitive_shape_for_key` - Apparent type handling

**Step 5: Union/Intersection/TypeParameter** ✅ COMPLETE
- ✅ `check_union_source_subtype` - Union source checking
- ✅ `check_union_target_subtype` - Union target checking
- ✅ `check_intersection_source_subtype` - Intersection source checking
- ✅ `check_intersection_target_subtype` - Intersection target checking
- ✅ `check_type_parameter_subtype` - Type parameter constraints
- ✅ `check_subtype_with_method_variance` - Method variance handling

**Step 6: Tuple/Array Checking** ✅ COMPLETE
- ✅ `check_tuple_subtype` - Tuple to tuple checking
- ✅ `check_tuple_to_array_subtype` - Tuple to array conversion
- ✅ `check_array_to_tuple_subtype` - Array to tuple conversion

**Step 7: Function/Callable Checking** ✅ COMPLETE
- ✅ `check_function_subtype` - Function signature compatibility
- ✅ `check_callable_subtype` - Overloaded callable checking
- ✅ `check_function_to_callable_subtype` - Single to overloaded
- ✅ `check_callable_to_function_subtype` - Overloaded to single
- ✅ `check_call_signature_subtype` - Call signature compatibility

**Step 8: Reference/Application/Mapped Types** ✅ COMPLETE
- ✅ `check_ref_ref_subtype` - Ref to ref checking
- ✅ `check_ref_subtype` - Ref to structural
- ✅ `check_to_ref_subtype` - Structural to ref
- ✅ `check_application_to_application_subtype` - Application checking
- ✅ `check_application_expansion_target` - Application expansion
- ✅ `check_source_to_application_expansion` - Application expansion source
- ✅ `check_mapped_expansion_target` - Mapped type expansion
- ✅ `check_source_to_mapped_expansion` - Mapped expansion source

### Coordinator Status ✅

**check_subtype_inner**: ~317 lines
- Clean dispatcher that delegates to specialized helper methods
- All complex logic extracted to `subtype_rules/` modules
- Maintains 100% test pass rate
- All clippy warnings resolved

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

### Step 5: Verification and Assessment ✅ COMPLETE

#### 5.1 Verification ✅
- ✅ Run full test suite: `cargo test --lib` - All tests pass
- ✅ Run clippy: `cargo clippy -- -D warnings` - No warnings
- ✅ Check line count: `wc -l src/solver/subtype.rs` = 1,778 lines
- ✅ Verify `check_subtype_inner` is now ~317 lines (exceeds goal!)

#### 5.2 Assessment ✅
- ✅ Module hierarchy created with 9 focused modules
- ✅ ~3,781 lines of extracted logic organized by type category
- ✅ ARCHITECTURE_WORK_SUMMARY.md updated with metrics
- ✅ All helper methods are testable and documented

#### 5.3 Next Steps ✅
- ✅ Move to checker/state.rs decomposition (Priority 2)

### Step 6: Module Hierarchy ✅ COMPLETE

**Target**: Create `solver/subtype_rules/` module structure

#### 6.1 Module Structure ✅
- ✅ Created `src/solver/subtype_rules/` directory
- ✅ Created `src/solver/subtype_rules/mod.rs`
- ✅ Module organization complete:
  - `intrinsics.rs` — Primitive types (338 lines)
  - `literals.rs` — Literal types (587 lines)
  - `unions.rs` — Union/intersection logic (361 lines)
  - `tuples.rs` — Array/tuple checking (379 lines)
  - `objects.rs` — Object property matching (544 lines)
  - `functions.rs` — Callable signatures (992 lines)
  - `generics.rs` — Type params, applications, mapped types (425 lines)
  - `conditionals.rs` — Conditional type checking (133 lines)
  - `mod.rs` — Module exports (22 lines)

#### 6.2 Helpers Moved to Modules ✅
- ✅ All intrinsic helpers moved to `intrinsics.rs`
- ✅ All literal helpers moved to `literals.rs`
- ✅ All union/intersection helpers moved to `unions.rs`
- ✅ All tuple helpers moved to `tuples.rs`
- ✅ All object helpers moved to `objects.rs`
- ✅ All function helpers moved to `functions.rs`
- ✅ All template literal helpers moved to `literals.rs`
- ✅ All mapped/conditional helpers moved to `generics.rs` and `conditionals.rs`

#### 6.3 Imports Updated ✅
- ✅ Module enabled in `solver/mod.rs`
- ✅ All imports work correctly
- ✅ Helpers use `pub(crate)` visibility as needed

#### 6.4 Module Structure Verified ✅
- ✅ Full test suite passes: `cargo test --lib`
- ✅ Clippy passes: `cargo clippy -- -D warnings`
- ✅ All imports work correctly
- ✅ Code compiles without errors

#### 6.5 Final Documentation ✅
- ✅ ARCHITECTURE_AUDIT_REPORT.md updated with final structure
- ✅ ARCHITECTURE_WORK_SUMMARY.md updated
- ✅ solver/subtype.rs marked as ✅ Complete in tracking docs
- ✅ README.md created in subtype_rules/ directory

---

## Priority 1: checker/state.rs 🚧 CURRENT FOCUS

**Goal**: Reduce from ~28,000 lines to ~2,000 lines (coordinator)
**Progress**: 660 lines extracted (promise, iterable modules)
**Status**: 🚧 In Progress (PRIORITY #1)

### Step 7: Extract Type Computation Logic (~3,000-4,000 lines)

**Target**: Create/expand `checker/type_computation.rs`

#### 7.1 Identify `get_type_of_*` Functions ✅ COMPLETE
- [x] List all `get_type_of_*` functions in `checker/state.rs`
- [x] Count lines for each function
- [x] Identify dependencies between functions
- [x] Plan extraction order (least dependent first)

#### 7.2 Extract Basic Type Computation ✅ IN PROGRESS
- [x] `get_type_of_conditional_expression` (~18 lines) ✅ **EXTRACTED**
- [x] `get_type_of_array_literal` (~131 lines) ✅ **EXTRACTED**
- [x] `get_type_of_prefix_unary` (~37 lines) ✅ **EXTRACTED**
- [x] `get_type_of_template_expression` (~27 lines) ✅ **EXTRACTED**
- [x] `get_type_of_variable_declaration` (~29 lines) ✅ **EXTRACTED**
- [ ] `get_type_of_binary_expression` (~150 lines)
- [ ] `get_type_of_super_keyword` (~100 lines)
- [ ] `get_type_of_property_access_by_name` (~50 lines)
- [ ] `get_type_of_element_access` (~200 lines)
- [ ] `get_type_of_assignment_target` (~80 lines)

**Progress**: state.rs 26,217 → 24,496 lines (**-1,721 lines, 22 functions extracted**)
- `get_type_of_conditional_expression` (~18 lines) → type_computation.rs ✅
- `get_type_of_array_literal` (~131 lines) → type_computation.rs ✅
- `get_type_of_prefix_unary` (~37 lines) → type_computation.rs ✅
- `get_type_of_template_expression` (~27 lines) → type_computation.rs ✅
- `get_type_of_variable_declaration` (~29 lines) → type_computation.rs ✅
- `get_type_of_assignment_target` (~19 lines) → type_computation.rs ✅
- `get_type_of_property_access_by_name` (~53 lines) → type_computation.rs ✅
- `get_type_of_class_member` (~37 lines) → type_computation.rs ✅
- `get_type_of_interface_member_simple` (~41 lines) → type_computation.rs ✅
- `get_type_of_interface_member` (~65 lines) → type_computation.rs ✅
- `get_type_of_binary_expression` (~148 lines) → type_computation.rs ✅
- `get_type_of_element_access` (~209 lines) → type_computation.rs ✅
- `get_element_access_type` (~115 lines) → type_computation.rs ✅
- `get_type_of_super_keyword` (~55 lines) → type_computation.rs ✅
- `get_type_of_object_literal` (~278 lines) → type_computation.rs ✅
- `collect_object_spread_properties` (~29 lines) → type_computation.rs ✅
- `get_type_of_new_expression` (~220 lines) → type_computation.rs ✅
- `type_contains_abstract_class` (~35 lines) → type_computation.rs ✅
- `get_type_from_union_type` (~24 lines) → type_computation.rs ✅
- `get_type_from_intersection_type` (~24 lines) → type_computation.rs ✅
- `get_type_from_array_type` (~11 lines) → type_computation.rs ✅
- `get_type_from_type_operator` (~32 lines) → type_computation.rs ✅

**Helper methods made pub(crate)**:
- `literal_type_from_initializer`
- `contextual_literal_type`
- `is_catch_clause_variable_declaration`
- `alias_resolves_to_type_only`
- `is_assignment_operator`
- `check_assignment_expression`
- `check_compound_assignment_expression`
- `is_side_effect_free`
- `is_indirect_call`
- `check_private_identifier_in_expression`
- `split_nullish_type`
- `emit_binary_operator_error`
- `get_literal_string_from_node`
- `get_numeric_index_from_string`
- `get_literal_index_from_node`
- `report_possibly_nullish_object`
- `get_literal_key_union_from_type`
- `get_element_access_type_for_literal_keys`
- `get_element_access_type_for_literal_number_keys`
- `should_report_no_index_signature`
- `error_no_index_signature_at`
- `get_base_class_idx`
- `check_super_expression`
- `error_at_position`
- `validate_new_expression_type_arguments`
- `apply_type_arguments_to_constructor_type`
- `resolve_overloaded_call_with_signatures`
- `collect_call_argument_types_with_context`
- `ensure_application_symbols_resolved`
- `should_skip_weak_union_error`

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
