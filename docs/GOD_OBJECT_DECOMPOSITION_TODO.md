# God Object Decomposition: Step-by-Step Guide

**Date**: 2026-01-24
**Status**: Active
**Current Phase**: Phase 2 - Break Up God Objects
**Last Updated**: 2026-01-24 (Step 10: Flow analysis extraction complete)

---

## Overview

This document provides a step-by-step plan for decomposing the "Big 6" god objects in the codebase. Work should proceed incrementally, with each step verified by tests before moving to the next.

### The "Big 6" God Objects

| File | Lines | Status | Priority |
|------|-------|--------|----------|
| `checker/state.rs` | 14,024 | 🚧 In Progress (46.4% reduced) | **P1 (CURRENT)** |
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
**Progress**: ~10,159 lines extracted (38.7% reduction from original 26,217 lines)
**Current Line Count**: 16,058 lines
**Status**: 🚧 In Progress (PRIORITY #1)

### Extracted Modules

| Module | Lines | Purpose | Status |
|--------|-------|---------|--------|
| `type_computation.rs` | 2,992 | Type computation logic (get_type_of_*) | ✅ Complete |
| `type_checking.rs` | 5,636 | Type checking validation (26 sections) | ✅ Complete |
| `symbol_resolver.rs` | 1,272 | Symbol resolution (resolve_*) | ✅ Complete |
| `error_reporter.rs` | ~2,000 | Error reporting and diagnostics | ✅ Complete |
| **Total Extracted** | **11,900** | **Focused, maintainable code** | |

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

**Progress**: state.rs 26,217 → 23,921 lines (**-2,296 lines, 30 functions extracted**)
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
- `get_keyof_type` (~26 lines) → type_computation.rs ✅
- `extract_string_literal_keys` (~26 lines) → type_computation.rs ✅
- `get_symbol_constructor_type` (~59 lines) → type_computation.rs ✅
- `get_call_receiver_type` (~7 lines) → type_computation.rs ✅
- `get_class_decl_from_type` (~71 lines) → type_computation.rs ✅
- `get_class_name_from_type` (~3 lines) → type_computation.rs ✅
- `get_type_of_call_expression` (~214 lines) → type_computation.rs ✅
- `get_type_of_identifier` (~152 lines) → type_computation.rs ✅

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
- `is_class_derived_from`
- `get_class_declaration_from_symbol`
- `get_class_name_from_decl`
- `apply_this_substitution_to_call_return`
- `refine_mixin_call_return_type`
- `map_expanded_arg_index_to_original`
- `is_super_expression`
- `is_dynamic_import`
- `check_dynamic_import_module_specifier`
- `is_class_constructor_type`
- `validate_call_type_arguments`
- `is_variable_used_before_declaration_in_static_block`
- `is_variable_used_before_declaration_in_computed_property`
- `is_variable_used_before_declaration_in_heritage_clause`
- `should_check_definite_assignment`
- `is_definitely_assigned_at`
- `is_in_default_parameter`
- `is_unresolved_import_symbol`
- `is_known_global_value_name`
- `is_nodejs_runtime_global`
- `is_static_member`

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

### Step 8: Extract Type Checking Logic ✅ COMPLETE

**Target**: Create `checker/type_checking.rs`
**Result**: 5,636 lines with 26 organized sections

#### 8.1 Section Organization ✅
**✅ Sections 1-15**: Previous work (before this session)
**✅ Section 16**: Unreachable Code Detection (8 functions)
**✅ Section 17**: Property Initialization Checking (5 functions)
**✅ Section 18**: AST Context Checking (4 functions)
**✅ Section 19**: Type and Name Checking Utilities (8 functions)
**✅ Section 20**: Declaration and Node Checking Utilities (6 functions)
**✅ Section 21**: Property Name Utilities (2 functions)
**✅ Section 22**: Type Checking Utilities (2 functions)
**✅ Section 23**: Import and Private Brand Utilities (7 functions, moved to symbol_resolver.rs)
**✅ Section 24**: Module Detection Utilities (3 functions)
**✅ Section 25**: AST Traversal Utilities (5 functions + 6 from earlier work)
**✅ Section 26**: Class and Member Finding Utilities (10 functions)

#### 8.2 Key Functions Extracted ✅
**Assignment and Expression Checking**:
- check_assignment_statement, check_variable_statement
- check_binary_operator, check_compound_assignment
- check_assignment_expression, check_compound_assignment_expression

**Statement Validation**:
- check_statement_for_early_property_access
- check_expression_for_early_property_access
- check_if_statement, check_while_statement, check_do_statement

**Declaration Checking**:
- check_class_declaration, check_class_expression
- check_interface_declaration, check_type_alias_declaration
- check_enum_declaration, check_namespace_declaration

**Member Checking**:
- check_class_member, check_property_declaration
- check_method_declaration, check_constructor_declaration
- check_accessor_declaration

**Validation Utilities**:
- check_parameters, check_call_argument_excess_properties
- check_accessor_type_compatibility
- check_type_member_for_missing_names
- check_property_initialization

**Helper Functions Made pub(crate)**:
- find_enclosing_function, find_enclosing_non_arrow_function
- find_enclosing_variable_statement, find_enclosing_variable_declaration
- find_enclosing_source_file, find_enclosing_static_block
- find_enclosing_computed_property, find_enclosing_heritage_clause
- find_class_for_static_block, find_class_for_computed_property
- find_class_for_heritage_clause, find_constructor_impl
- find_method_impl, find_return_statement_pos, find_function_impl
- is_variable_captured_in_closure, is_narrowable_type
- is_node_within, should_validate_async_function_context
- is_file_module, has_export_modifier_on_modifiers
- get_require_module_specifier, is_require_call
- is_property_access_on_unresolved_import
- get_private_brand, types_have_same_private_brand
- get_private_field_name_from_brand, private_brand_mismatch_error

#### 8.3 Module Structure ✅
- ✅ 26 sections with clear organization
- ✅ All functions properly documented
- ✅ Helper methods made pub(crate) where needed
- ✅ All functions compile successfully

**State.rs reduction**: 26,217 → 16,058 lines (~10,159 lines extracted, 38.7% reduction)

### Step 9: Extract Symbol Resolution Logic ✅ COMPLETE

**Target**: Expand `checker/symbol_resolver.rs`
**Result**: symbol_resolver.rs created with 1,272 lines
**Progress**: state.rs reduced from 21,032 → 20,612 lines (~420 lines from symbol resolution)
**Final**: 16,058 lines (includes additional extractions)

#### 9.1 Identify Symbol Resolution Functions ✅
- [x] List all `resolve_*` functions
- [x] List all symbol lookup functions
- [x] Count lines for each function
- [x] Plan extraction order

#### 9.2 Extract Symbol Lookup ✅
- [x] Extract `lookup_type_parameter` (~3 lines)
- [x] Extract `get_type_param_bindings` (~6 lines)
- [x] Extract `entity_name_text` (~20 lines)
- [x] Extract `resolve_type_symbol_for_lowering` (~10 lines)
- [x] Extract `resolve_value_symbol_for_lowering` (~10 lines)
- [x] Extract `resolve_global_value_symbol` (~3 lines)
- [x] Run tests after extraction
- [x] Commit changes

#### 9.3 Extract Import Resolution ✅
- [x] Extract `get_require_module_specifier` (~27 lines)
- [x] Extract `resolve_require_call_symbol` (~13 lines)
- [x] Extract `is_require_call` (~25 lines)
- [x] Run tests after extraction
- [x] Commit changes

#### 9.4 Extract Type Resolution ✅
- [x] Extract `missing_type_query_left` (~21 lines)
- [x] Extract `report_type_query_missing_member` (~45 lines)
- [x] Extract `parse_test_option_bool` (~28 lines)
- [x] Extract `resolve_no_implicit_any_from_source` (~8 lines)
- [x] Extract `resolve_no_implicit_returns_from_source` (~6 lines)
- [x] Extract `resolve_use_unknown_in_catch_variables_from_source` (~8 lines)
- [x] Extract `resolve_duplicate_decl_node` (~20 lines)
- [x] Run tests after extraction
- [x] Commit changes

#### 9.5 Extract Class Resolution ✅
- [x] Extract `resolve_heritage_symbol` (~26 lines)
- [x] Extract `is_property_access_on_unresolved_import` (~32 lines)
- [x] Extract `resolve_class_for_access` (~35 lines)
- [x] Extract `resolve_receiver_class_for_access` (~24 lines)
- [x] Run tests after extraction
- [x] Commit changes

#### 9.6 Extract Private Brand Checking ✅
- [x] Extract `get_private_brand` (~29 lines)
- [x] Extract `types_have_same_private_brand` (~8 lines)
- [x] Extract `get_private_field_name_from_brand` (~25 lines)
- [x] Extract `private_brand_mismatch_error` (~23 lines)
- [x] Run tests after extraction
- [x] Commit changes

#### 9.7 Update Module Structure ✅
- [x] Ensure `checker/symbol_resolver.rs` has all extracted functions
- [x] Add `pub(crate)` visibility as needed
- [x] Update imports in `checker/state.rs`
- [x] Run full test suite (pre-existing failures unrelated to extraction)
- [x] Update documentation

**symbol_resolver.rs sections**:
- Symbol Type Resolution
- Global Symbol Detection
- Symbol Information Queries
- Identifier Symbol Resolution
- Qualified Name Resolution
- Private Identifier Resolution
- Type Parameter Resolution
- Library Type Resolution
- Type Query Resolution
- Namespace Member Resolution
- Global Value Resolution
- Heritage Symbol Resolution
- Access Class Resolution
- Import/Export Checking
- Private Brand Detection

#### 9.1 Identify Symbol Resolution Functions ✅
- [x] List all `resolve_*` functions
- [x] List all symbol lookup functions
- [x] Count lines for each function
- [x] Plan extraction order

#### 9.2 Extract Symbol Lookup ✅
- [x] Extract `lookup_type_parameter` (~3 lines)
- [x] Extract `get_type_param_bindings` (~6 lines)
- [x] Extract `entity_name_text` (~20 lines)
- [x] Extract `resolve_type_symbol_for_lowering` (~10 lines)
- [x] Extract `resolve_value_symbol_for_lowering` (~10 lines)
- [x] Extract `resolve_global_value_symbol` (~3 lines)
- [x] Run tests after extraction
- [x] Commit changes

#### 9.3 Extract Import Resolution ✅
- [x] Extract `get_require_module_specifier` (~27 lines)
- [x] Extract `resolve_require_call_symbol` (~13 lines)
- [x] Extract `is_require_call` (~25 lines)
- [x] Run tests after extraction
- [x] Commit changes

#### 9.4 Extract Type Resolution ✅
- [x] Extract `missing_type_query_left` (~21 lines)
- [x] Extract `report_type_query_missing_member` (~45 lines)
- [x] Extract `parse_test_option_bool` (~28 lines)
- [x] Extract `resolve_no_implicit_any_from_source` (~8 lines)
- [x] Extract `resolve_no_implicit_returns_from_source` (~6 lines)
- [x] Extract `resolve_use_unknown_in_catch_variables_from_source` (~8 lines)
- [x] Extract `resolve_duplicate_decl_node` (~20 lines)
- [x] Run tests after extraction
- [x] Commit changes

#### 9.5 Extract Class Resolution ✅
- [x] Extract `resolve_heritage_symbol` (~26 lines)
- [x] Extract `is_property_access_on_unresolved_import` (~32 lines)
- [x] Extract `resolve_class_for_access` (~35 lines)
- [x] Extract `resolve_receiver_class_for_access` (~24 lines)
- [x] Run tests after extraction
- [x] Commit changes

#### 9.6 Update Module Structure ✅
- [x] Ensure `checker/symbol_resolver.rs` has all extracted functions
- [x] Add `pub(crate)` visibility as needed
- [x] Update imports in `checker/state.rs`
- [x] Run full test suite (pre-existing failures unrelated to extraction)
- [x] Update documentation

### Step 10: Extract Flow Analysis Logic (~2,000-3,000 lines) ✅ COMPLETE

**Target**: Expand `checker/flow_analysis.rs` with definite assignment and narrowing functions
**Result**: Moved ~1,000 lines of flow analysis code from `state.rs` to `flow_analysis.rs`

#### 10.1 Identify Flow Analysis Functions ✅
- [x] List all flow analysis functions
- [x] List all type narrowing functions
- [x] Count lines for each function
- [x] Plan extraction order

#### 10.2 Extract Type Narrowing ✅
- [x] Extract `narrow_by_typeof` - thin wrapper around NarrowingContext
- [x] Extract `narrow_by_typeof_negation` - handles negated typeof checks
- [x] Extract `narrow_by_discriminant` - discriminated union narrowing
- [x] Extract `narrow_by_excluding_discriminant` - negative case
- [x] Extract `narrow_to_type` and `narrow_excluding_type`
- [x] Extract `find_discriminants` - discriminant property detection
- [x] Run tests after extraction
- [x] Commit after extraction

#### 10.3 Extract Control Flow Analysis ✅
- [x] Extract `apply_flow_narrowing` - main flow narrowing entry point
- [x] Extract `check_flow_usage` - definite assignment + narrowing
- [x] Run tests after extraction
- [x] Commit after extraction

#### 10.4 Extract Definite Assignment ✅
- [x] Extract `should_check_definite_assignment` - determines when to check
- [x] Extract `emit_definite_assignment_error` - error emission
- [x] Extract `is_definitely_assigned_at` - flow-based check
- [x] Extract helper functions:
  - `symbol_is_in_ambient_context`
  - `is_variable_captured_in_closure`
  - `symbol_type_allows_uninitialized`
  - `symbol_has_initializer`
  - `symbol_is_parameter`
  - `symbol_has_definite_assignment_assertion`
  - `node_is_or_within_kind`
  - `is_in_default_parameter`
- [x] Run tests after extraction
- [x] Commit after extraction

#### 10.5 Extract TDZ (Temporal Dead Zone) Checks ✅
- [x] Extract `is_variable_used_before_declaration_in_static_block`
- [x] Extract `is_variable_used_before_declaration_in_computed_property`
- [x] Extract `is_variable_used_before_declaration_in_heritage_clause`

#### 10.6 Update Module Structure ✅
- [x] Expand `src/checker/flow_analysis.rs` with new sections
- [x] Move all extracted functions
- [x] Add `pub(crate)` visibility as needed
- [x] Update imports in `checker/state.rs`
- [x] Run full test suite (2 pre-existing failures, not caused by this change)
- [x] Update documentation

**State.rs reduction**: 15,088 → 14,024 lines (~1,064 lines removed)

### Step 11: Extract Error Reporting (~1,000-1,500 lines) ✅ COMPLETE

**Target**: Expand `checker/error_reporter.rs`
**Result**: Moved ~1,450 lines from `state.rs` to `error_reporter.rs`

#### 11.1 Identify Error Functions ✅
- [x] List all `error_*` functions (~38 functions)
- [x] Count lines for each function
- [x] Verify ErrorHandler trait coverage

#### 11.2 Extract Error Emission ✅
- [x] Move all `error_*` functions to `error_reporter.rs`
- [x] Move supporting helper functions:
  - `diagnose_assignment_failure`, `render_failure_reason`
  - `find_similar_identifiers`, `calculate_string_similarity`, `levenshtein_distance`
  - `create_diagnostic_collector`, `merge_diagnostics`
- [x] Move `is_class_constructor_type` to `constructor_checker.rs`
- [x] Run tests after extraction
- [x] Commit changes

#### 11.3 Update Module Structure ✅
- [x] Ensure `checker/error_reporter.rs` has all error functions
- [x] Add `pub(crate)` visibility for helper functions:
  - `constructor_access_name`
  - `constructor_accessibility_mismatch`
  - `private_brand_mismatch_error`
- [x] Update imports in `checker/state.rs`
- [x] Run full test suite (4 pre-existing failures, not caused by this change)
- [x] Update documentation

**State.rs reduction**: 21,032 → 19,584 lines (~1,448 lines removed)

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
