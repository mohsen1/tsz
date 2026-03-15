# Problem 1.1: Inventory & Classify Direct Solver Imports

**Date**: 2026-03-15
**Scope**: All `use tsz_solver::` imports in `crates/tsz-checker/src/` (excluding `query_boundaries/`, `tests/`, and `lib.rs`)

## Summary

| Category | Count | Percentage |
|----------|-------|------------|
| SAFE | 428 | 78.2% |
| COMPUTATION | 112 | 20.5% |
| CONSTRUCTION | 4 | 0.7% |
| INTERNAL | 3 | 0.5% |
| **TOTAL** | **547** | **100%** |

### Category Definitions

- **SAFE**: Type handles (`TypeId`, `TypeData`), structural shapes (`ObjectShape`, `FunctionShape`, etc.), visitor functions (`is_array_type`, `union_list_id`, etc.), diagnostic types, definition types, type query/classification functions. Read-only, no computation.
- **COMPUTATION**: Solver functions that perform type computation -- subtype checking, instantiation, evaluation, property access resolution, binary operations, freshness/widening, contextual typing, index signature resolution.
- **CONSTRUCTION**: Type construction via `TypeInterner` -- creates new interned types.
- **INTERNAL**: Solver-internal state -- `RelationCacheKey`, cache types that should not be accessed from checker code.

### Key Findings

- **428** imports (78%) are SAFE and could remain as direct imports under a policy allowlist.
- **112** imports (20%) are COMPUTATION and should route through `query_boundaries`.
- **4** imports (1%) are CONSTRUCTION (`TypeInterner`) -- used for building types directly.
- **3** imports (1%) are INTERNAL (`RelationCacheKey`) -- architecture violation, should be removed from checker.

Files with COMPUTATION imports: **43**
Files with CONSTRUCTION imports: **4**
Files with INTERNAL imports: **3**

## query_boundaries Wrapper Coverage

Checking whether each COMPUTATION/CONSTRUCTION/INTERNAL import has a corresponding wrapper in `query_boundaries/`.

| Import Item | Category | Direct Uses | In query_boundaries? |
|------------|----------|-------------|---------------------|
| `ApplicationEvaluator` | COMPUTATION | 1 | No |
| `AssignabilityChecker` | COMPUTATION | 1 | Yes |
| `BinaryOpEvaluator` | COMPUTATION | 7 | No |
| `BinaryOpResult` | COMPUTATION | 3 | No |
| `CallResult` | COMPUTATION | 5 | Yes |
| `IndexKind` | COMPUTATION | 2 | No |
| `IndexSignatureResolver` | COMPUTATION | 2 | No |
| `RelationCacheKey` | INTERNAL | 3 | No |
| `TypeEvaluator` | COMPUTATION | 3 | No |
| `TypeInterner` | CONSTRUCTION | 4 | No |
| `TypeResolver` | COMPUTATION | 2 | No |
| `TypeSubstitution` | COMPUTATION | 11 | Yes |
| `expression_ops` | COMPUTATION | 1 | No |
| `instantiate_generic` | COMPUTATION | 1 | No |
| `instantiate_type` | COMPUTATION | 14 | No |
| `instantiate_type_with_depth_status` | COMPUTATION | 1 | No |
| `objects::index_signatures::IndexKind` | COMPUTATION | 2 | No |
| `objects::index_signatures::IndexSignatureResolver` | COMPUTATION | 2 | No |
| `operations::CallResult` | COMPUTATION | 1 | Yes |
| `operations::property::PropertyAccessEvaluator` | COMPUTATION | 1 | No |
| `operations::property::PropertyAccessResult` | COMPUTATION | 40 | No |
| `operations::property::is_mapped_type_with_readonly_modifier` | COMPUTATION | 1 | No |
| `operations::property::is_readonly_tuple_fixed_element` | COMPUTATION | 2 | No |
| `relations::freshness` | COMPUTATION | 3 | No |
| `relations::freshness::is_fresh_object_type` | COMPUTATION | 2 | No |
| `relations::freshness::widen_freshness` | COMPUTATION | 2 | No |
| `substitute_this_type` | COMPUTATION | 1 | No |
| `widening::apply_const_assertion` | COMPUTATION | 1 | No |

## Detailed Import Inventory

| File | Import Item | Category | QB Wrapper? |
|------|-------------|----------|-------------|
| `assignability/assignability_checker.rs` | `TypeResolver` | COMPUTATION | No |
| `assignability/assignability_checker.rs` | `relations::freshness` | COMPUTATION | No |
| `assignability/assignability_checker.rs` | `RelationCacheKey` | INTERNAL | No |
| `assignability/assignability_checker.rs` | `NarrowingContext` | SAFE | No |
| `assignability/assignability_checker.rs` | `TypeId` | SAFE | Yes |
| `assignability/assignability_checker.rs` | `visitor::collect_lazy_def_ids` | SAFE | No |
| `assignability/assignability_checker.rs` | `visitor::collect_type_queries` | SAFE | No |
| `assignability/assignment_checker.rs` | `BinaryOpEvaluator` | COMPUTATION | No |
| `assignability/assignment_checker.rs` | `BinaryOpEvaluator` | COMPUTATION | No |
| `assignability/assignment_checker.rs` | `BinaryOpResult` | COMPUTATION | No |
| `assignability/assignment_checker.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `assignability/assignment_checker.rs` | `TypeId` | SAFE | Yes |
| `assignability/subtype_identity_checker.rs` | `RelationCacheKey` | INTERNAL | No |
| `assignability/subtype_identity_checker.rs` | `TypeId` | SAFE | Yes |
| `assignability/subtype_identity_checker.rs` | `type_queries` | SAFE | Yes |
| `assignability/subtype_identity_checker.rs` | `type_queries` | SAFE | Yes |
| `checkers/call_checker.rs` | `AssignabilityChecker` | COMPUTATION | Yes |
| `checkers/call_checker.rs` | `CallResult` | COMPUTATION | Yes |
| `checkers/call_checker.rs` | `operations::CallResult` | COMPUTATION | Yes |
| `checkers/call_checker.rs` | `ContextualTypeContext` | SAFE | No |
| `checkers/call_checker.rs` | `FunctionShape` | SAFE | Yes |
| `checkers/call_checker.rs` | `FunctionShape` | SAFE | Yes |
| `checkers/call_checker.rs` | `PendingDiagnosticBuilder` | SAFE | No |
| `checkers/call_checker.rs` | `TypeId` | SAFE | Yes |
| `checkers/enum_checker.rs` | `TypeId` | SAFE | Yes |
| `checkers/generic_checker.rs` | `TypeId` | SAFE | Yes |
| `checkers/generic_checker.rs` | `type_queries::ArrayLikeKind` | SAFE | Yes |
| `checkers/generic_checker.rs` | `type_queries::IndexKeyKind` | SAFE | Yes |
| `checkers/generic_checker.rs` | `type_queries::self as query` | SAFE | No |
| `checkers/generic_checker.rs` | `visitor::application_id` | SAFE | No |
| `checkers/generic_checker.rs` | `visitor::lazy_def_id` | SAFE | No |
| `checkers/generic_checker.rs` | `visitor::lazy_def_id` | SAFE | No |
| `checkers/iterable_checker.rs` | `operations::property::PropertyAccessEvaluator` | COMPUTATION | No |
| `checkers/iterable_checker.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `checkers/iterable_checker.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `checkers/iterable_checker.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `checkers/iterable_checker.rs` | `TypeId` | SAFE | Yes |
| `checkers/iterable_checker.rs` | `type_queries::data::get_call_signatures` | SAFE | No |
| `checkers/iterable_checker.rs` | `type_queries::data::get_call_signatures` | SAFE | No |
| `checkers/iterable_checker.rs` | `type_queries::data::get_function_shape` | SAFE | No |
| `checkers/jsx_checker.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `checkers/jsx_checker.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `checkers/jsx_checker.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `checkers/jsx_checker.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `checkers/jsx_checker.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `checkers/jsx_checker.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `checkers/jsx_checker.rs` | `TypeId` | SAFE | Yes |
| `checkers/jsx_checker_attrs.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `checkers/jsx_checker_attrs.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `checkers/jsx_checker_attrs.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `checkers/jsx_checker_attrs.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `checkers/jsx_checker_attrs.rs` | `TypeId` | SAFE | Yes |
| `checkers/parameter_checker.rs` | `TypeId` | SAFE | Yes |
| `checkers/promise_checker.rs` | `TypeId` | SAFE | Yes |
| `checkers/signature_builder.rs` | `TypeSubstitution` | COMPUTATION | Yes |
| `checkers/signature_builder.rs` | `instantiate_type` | COMPUTATION | No |
| `checkers/signature_builder.rs` | `ParamInfo` | SAFE | No |
| `checkers/signature_builder.rs` | `ParamInfo` | SAFE | No |
| `checkers/signature_builder.rs` | `TypeId` | SAFE | Yes |
| `checkers/signature_builder.rs` | `TypePredicate` | SAFE | No |
| `checkers/signature_builder.rs` | `TypePredicateTarget` | SAFE | No |
| `classes/class_abstract_checker.rs` | `TypeId` | SAFE | Yes |
| `classes/class_checker.rs` | `TypeSubstitution` | COMPUTATION | Yes |
| `classes/class_checker.rs` | `instantiate_type` | COMPUTATION | No |
| `classes/class_checker.rs` | `instantiate_type` | COMPUTATION | No |
| `classes/class_checker.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `classes/class_checker.rs` | `FunctionShape` | SAFE | Yes |
| `classes/class_checker.rs` | `TypeId` | SAFE | Yes |
| `classes/class_checker_compat.rs` | `TypeSubstitution` | COMPUTATION | Yes |
| `classes/class_checker_compat.rs` | `instantiate_type` | COMPUTATION | No |
| `classes/class_checker_compat.rs` | `instantiate_type` | COMPUTATION | No |
| `classes/class_checker_compat.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `classes/class_checker_compat.rs` | `TypeId` | SAFE | Yes |
| `classes/class_checker_compat.rs` | `recursion::RecursionGuard` | SAFE | No |
| `classes/class_checker_compat.rs` | `recursion::RecursionProfile` | SAFE | No |
| `classes/class_checker_compat.rs` | `recursion::RecursionResult` | SAFE | No |
| `classes/class_implements_checker.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `classes/class_implements_checker.rs` | `PropertyInfo` | SAFE | No |
| `classes/class_implements_checker.rs` | `TypeId` | SAFE | Yes |
| `classes/class_implements_checker.rs` | `Visibility` | SAFE | No |
| `classes/class_implements_helpers.rs` | `TypeId` | SAFE | Yes |
| `classes/class_inheritance.rs` | `TypeInterner` | CONSTRUCTION | No |
| `classes/constructor_checker.rs` | `TypeId` | SAFE | Yes |
| `classes/private_checker.rs` | `TypeId` | SAFE | Yes |
| `context/compiler_options.rs` | `RelationCacheKey` | INTERNAL | No |
| `context/compiler_options.rs` | `judge::JudgeConfig` | SAFE | No |
| `context/constructors.rs` | `QueryDatabase` | SAFE | Yes |
| `context/constructors.rs` | `TypeEnvironment` | SAFE | No |
| `context/constructors.rs` | `def::DefinitionStore` | SAFE | No |
| `context/core.rs` | `TypeId` | SAFE | Yes |
| `context/core.rs` | `TypeId` | SAFE | Yes |
| `context/def_mapping.rs` | `SymbolRef` | SAFE | No |
| `context/def_mapping.rs` | `TypeFormatter` | SAFE | No |
| `context/def_mapping.rs` | `TypeId` | SAFE | Yes |
| `context/def_mapping.rs` | `def::DefId` | SAFE | No |
| `context/def_mapping.rs` | `def::DefinitionInfo` | SAFE | No |
| `context/def_mapping.rs` | `type_queries` | SAFE | Yes |
| `context/mod.rs` | `QueryDatabase` | SAFE | Yes |
| `context/mod.rs` | `TypeEnvironment` | SAFE | No |
| `context/mod.rs` | `TypeId` | SAFE | Yes |
| `context/mod.rs` | `def::DefId` | SAFE | No |
| `context/mod.rs` | `def::DefinitionStore` | SAFE | No |
| `context/resolver.rs` | `type_queries` | SAFE | Yes |
| `context/resolver.rs` | `visitor` | SAFE | No |
| `context/resolver.rs` | `visitor::callable_shape_id` | SAFE | No |
| `context/resolver.rs` | `visitor::object_shape_id` | SAFE | No |
| `context/resolver.rs` | `visitor::object_with_index_shape_id` | SAFE | No |
| `declarations/import/core.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `declarations/import/declaration.rs` | `TypeId` | SAFE | Yes |
| `declarations/module_checker.rs` | `PropertyInfo` | SAFE | No |
| `declarations/module_checker.rs` | `TypeId` | SAFE | Yes |
| `declarations/module_checker.rs` | `TypeId` | SAFE | Yes |
| `declarations/module_checker.rs` | `TypeId` | SAFE | Yes |
| `declarations/module_checker.rs` | `Visibility` | SAFE | No |
| `declarations/namespace_checker.rs` | `CallableShape` | SAFE | Yes |
| `declarations/namespace_checker.rs` | `CallableShape` | SAFE | Yes |
| `declarations/namespace_checker.rs` | `PropertyInfo` | SAFE | No |
| `declarations/namespace_checker.rs` | `PropertyInfo` | SAFE | No |
| `declarations/namespace_checker.rs` | `TypeId` | SAFE | Yes |
| `declarations/namespace_checker.rs` | `Visibility` | SAFE | No |
| `dispatch.rs` | `BinaryOpEvaluator` | COMPUTATION | No |
| `dispatch.rs` | `widening::apply_const_assertion` | COMPUTATION | No |
| `dispatch.rs` | `TypeId` | SAFE | Yes |
| `error_reporter/assignability.rs` | `SubtypeFailureReason` | SAFE | Yes |
| `error_reporter/assignability.rs` | `TypeId` | SAFE | Yes |
| `error_reporter/assignability_helpers.rs` | `objects::index_signatures::IndexKind` | COMPUTATION | No |
| `error_reporter/assignability_helpers.rs` | `objects::index_signatures::IndexSignatureResolver` | COMPUTATION | No |
| `error_reporter/assignability_helpers.rs` | `TypeId` | SAFE | Yes |
| `error_reporter/call_errors.rs` | `PendingDiagnostic` | SAFE | No |
| `error_reporter/call_errors.rs` | `SubtypeFailureReason` | SAFE | Yes |
| `error_reporter/call_errors.rs` | `TypeId` | SAFE | Yes |
| `error_reporter/call_errors.rs` | `type_queries::get_callable_shape` | SAFE | No |
| `error_reporter/call_errors.rs` | `type_queries::get_function_shape` | SAFE | No |
| `error_reporter/call_errors.rs` | `type_queries::get_type_application` | SAFE | No |
| `error_reporter/core.rs` | `TypeId` | SAFE | Yes |
| `error_reporter/core.rs` | `type_queries::get_object_shape_id` | SAFE | No |
| `error_reporter/generics.rs` | `TypeSubstitution` | COMPUTATION | Yes |
| `error_reporter/generics.rs` | `instantiate_type` | COMPUTATION | No |
| `error_reporter/generics.rs` | `CallSignature` | SAFE | Yes |
| `error_reporter/generics.rs` | `CallableShape` | SAFE | Yes |
| `error_reporter/generics.rs` | `TypeId` | SAFE | Yes |
| `error_reporter/operator_errors.rs` | `BinaryOpEvaluator` | COMPUTATION | No |
| `error_reporter/operator_errors.rs` | `TypeId` | SAFE | Yes |
| `error_reporter/properties.rs` | `TypeId` | SAFE | Yes |
| `error_reporter/properties.rs` | `type_queries` | SAFE | Yes |
| `error_reporter/properties.rs` | `type_queries::NamespaceMemberKind` | SAFE | No |
| `error_reporter/properties.rs` | `type_queries::classify_namespace_member` | SAFE | No |
| `error_reporter/suggestions.rs` | `IntrinsicKind` | SAFE | No |
| `error_reporter/suggestions.rs` | `TypeId` | SAFE | Yes |
| `error_reporter/suggestions.rs` | `def::resolver::TypeResolver` | SAFE | No |
| `error_reporter/suggestions.rs` | `type_queries` | SAFE | Yes |
| `error_reporter/type_value.rs` | `TypeId` | SAFE | Yes |
| `expr.rs` | `TypeId` | SAFE | Yes |
| `expr.rs` | `recursion::DepthCounter` | SAFE | No |
| `expr.rs` | `recursion::RecursionProfile` | SAFE | No |
| `flow/control_flow/assignment.rs` | `ApplicationEvaluator` | COMPUTATION | No |
| `flow/control_flow/assignment.rs` | `BinaryOpEvaluator` | COMPUTATION | No |
| `flow/control_flow/assignment.rs` | `BinaryOpResult` | COMPUTATION | No |
| `flow/control_flow/assignment.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `flow/control_flow/assignment.rs` | `TupleElement` | SAFE | Yes |
| `flow/control_flow/assignment.rs` | `TypeId` | SAFE | Yes |
| `flow/control_flow/condition_narrowing.rs` | `GuardSense` | SAFE | No |
| `flow/control_flow/condition_narrowing.rs` | `NarrowingContext` | SAFE | No |
| `flow/control_flow/condition_narrowing.rs` | `TypeGuard` | SAFE | No |
| `flow/control_flow/condition_narrowing.rs` | `TypeId` | SAFE | Yes |
| `flow/control_flow/condition_narrowing.rs` | `TypeofKind` | SAFE | No |
| `flow/control_flow/core.rs` | `NarrowingContext` | SAFE | No |
| `flow/control_flow/core.rs` | `ParamInfo` | SAFE | No |
| `flow/control_flow/core.rs` | `QueryDatabase` | SAFE | Yes |
| `flow/control_flow/core.rs` | `TypeId` | SAFE | Yes |
| `flow/control_flow/core.rs` | `TypePredicate` | SAFE | No |
| `flow/control_flow/core.rs` | `type_queries::is_unit_type` | SAFE | No |
| `flow/control_flow/narrowing.rs` | `GuardSense` | SAFE | No |
| `flow/control_flow/narrowing.rs` | `NarrowingContext` | SAFE | No |
| `flow/control_flow/narrowing.rs` | `ParamInfo` | SAFE | No |
| `flow/control_flow/narrowing.rs` | `TypeGuard` | SAFE | No |
| `flow/control_flow/narrowing.rs` | `TypeId` | SAFE | Yes |
| `flow/control_flow/narrowing.rs` | `TypePredicate` | SAFE | No |
| `flow/control_flow/narrowing.rs` | `TypePredicateTarget` | SAFE | No |
| `flow/control_flow/narrowing.rs` | `type_queries::{
        PredicateSignatureKind, classify_for_predicate_signature, is_narrowing_literal,
        stringify_literal_type,
    }` | SAFE | No |
| `flow/control_flow/references.rs` | `type_queries::LiteralValueKind` | SAFE | No |
| `flow/control_flow/references.rs` | `type_queries::classify_for_literal_value` | SAFE | No |
| `flow/control_flow/type_guards.rs` | `TypeResolver` | COMPUTATION | No |
| `flow/control_flow/type_guards.rs` | `SymbolRef` | SAFE | No |
| `flow/control_flow/type_guards.rs` | `TypeGuard` | SAFE | No |
| `flow/control_flow/type_guards.rs` | `TypeId` | SAFE | Yes |
| `flow/control_flow/type_guards.rs` | `TypeofKind` | SAFE | No |
| `flow/control_flow/var_utils.rs` | `NarrowingContext` | SAFE | No |
| `flow/control_flow/var_utils.rs` | `TypeId` | SAFE | Yes |
| `flow/flow_analysis/definite.rs` | `TypeId` | SAFE | Yes |
| `flow/flow_analysis/usage.rs` | `TypeId` | SAFE | Yes |
| `flow/flow_analysis/usage.rs` | `TypeId` | SAFE | Yes |
| `flow/flow_analysis/usage.rs` | `type_contains_undefined` | SAFE | No |
| `flow/reachability_checker.rs` | `NarrowingContext` | SAFE | No |
| `flow/reachability_checker.rs` | `TypeId` | SAFE | Yes |
| `state/state.rs` | `relations::freshness::is_fresh_object_type` | COMPUTATION | No |
| `state/state.rs` | `relations::freshness::widen_freshness` | COMPUTATION | No |
| `state/state.rs` | `substitute_this_type` | COMPUTATION | No |
| `state/state.rs` | `QueryDatabase` | SAFE | Yes |
| `state/state.rs` | `TypeId` | SAFE | Yes |
| `state/state.rs` | `TypeId` | SAFE | Yes |
| `state/state_checking/class.rs` | `TypeId` | SAFE | Yes |
| `state/state_checking/heritage.rs` | `TypeId` | SAFE | Yes |
| `state/state_checking/property.rs` | `relations::freshness` | COMPUTATION | No |
| `state/state_checking/property.rs` | `relations::freshness` | COMPUTATION | No |
| `state/state_checking/property.rs` | `TypeId` | SAFE | Yes |
| `state/state_checking/readonly.rs` | `objects::index_signatures::IndexKind` | COMPUTATION | No |
| `state/state_checking/readonly.rs` | `objects::index_signatures::IndexSignatureResolver` | COMPUTATION | No |
| `state/state_checking/readonly.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `state/state_checking/readonly.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `state/state_checking/readonly.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `state/state_checking/readonly.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `state/state_checking/readonly.rs` | `operations::property::is_mapped_type_with_readonly_modifier` | COMPUTATION | No |
| `state/state_checking/readonly.rs` | `operations::property::is_readonly_tuple_fixed_element` | COMPUTATION | No |
| `state/state_checking/readonly.rs` | `operations::property::is_readonly_tuple_fixed_element` | COMPUTATION | No |
| `state/state_checking/readonly.rs` | `TypeInterner` | CONSTRUCTION | No |
| `state/state_checking/readonly.rs` | `TypeId` | SAFE | Yes |
| `state/state_checking/readonly.rs` | `is_type_parameter` | SAFE | Yes |
| `state/state_checking/readonly.rs` | `type_param_info` | SAFE | No |
| `state/state_checking_members/ambient_signature_checks.rs` | `ContextualTypeContext` | SAFE | No |
| `state/state_checking_members/ambient_signature_checks.rs` | `TypeId` | SAFE | Yes |
| `state/state_checking_members/index_signature_checks.rs` | `TypeId` | SAFE | Yes |
| `state/state_checking_members/member_access.rs` | `TypeId` | SAFE | Yes |
| `state/state_checking_members/member_declaration_checks.rs` | `TypeId` | SAFE | Yes |
| `state/state_checking_members/member_declaration_checks.rs` | `TypeParamInfo` | SAFE | No |
| `state/state_checking_members/overload_compatibility.rs` | `TypeId` | SAFE | Yes |
| `state/state_checking_members/statement_callback_bridge.rs` | `TypeId` | SAFE | Yes |
| `state/type_analysis/computed.rs` | `CallableShape` | SAFE | Yes |
| `state/type_analysis/computed.rs` | `PropertyInfo` | SAFE | No |
| `state/type_analysis/computed.rs` | `PropertyInfo` | SAFE | No |
| `state/type_analysis/computed.rs` | `PropertyInfo` | SAFE | No |
| `state/type_analysis/computed.rs` | `TypeId` | SAFE | Yes |
| `state/type_analysis/computed.rs` | `Visibility` | SAFE | No |
| `state/type_analysis/computed_alias.rs` | `TypeId` | SAFE | Yes |
| `state/type_analysis/computed_alias.rs` | `is_compiler_managed_type` | SAFE | No |
| `state/type_analysis/computed_commonjs.rs` | `PropertyInfo` | SAFE | No |
| `state/type_analysis/computed_commonjs.rs` | `TypeId` | SAFE | Yes |
| `state/type_analysis/computed_commonjs.rs` | `Visibility` | SAFE | No |
| `state/type_analysis/computed_helpers.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `state/type_analysis/computed_helpers.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `state/type_analysis/computed_helpers.rs` | `TypeId` | SAFE | Yes |
| `state/type_analysis/computed_helpers.rs` | `keyof_inner_type` | SAFE | No |
| `state/type_analysis/computed_helpers.rs` | `recursion::RecursionProfile` | SAFE | No |
| `state/type_analysis/computed_helpers.rs` | `recursion::RecursionProfile` | SAFE | No |
| `state/type_analysis/computed_helpers.rs` | `recursion::RecursionProfile` | SAFE | No |
| `state/type_analysis/computed_helpers.rs` | `type_queries::ContextualLiteralAllowKind` | SAFE | No |
| `state/type_analysis/computed_helpers.rs` | `type_queries::classify_for_contextual_literal` | SAFE | No |
| `state/type_analysis/computed_helpers_binding.rs` | `TypeId` | SAFE | Yes |
| `state/type_analysis/computed_loops.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `state/type_analysis/computed_loops.rs` | `TypeId` | SAFE | Yes |
| `state/type_analysis/core.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `state/type_analysis/core.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `state/type_analysis/core.rs` | `SymbolRef` | SAFE | No |
| `state/type_analysis/core.rs` | `SymbolRef` | SAFE | No |
| `state/type_analysis/core.rs` | `TypeId` | SAFE | Yes |
| `state/type_analysis/cross_file.rs` | `TypeId` | SAFE | Yes |
| `state/type_analysis/cross_file.rs` | `is_compiler_managed_type` | SAFE | No |
| `state/type_analysis/symbol_type_helpers.rs` | `CallableShape` | SAFE | Yes |
| `state/type_analysis/symbol_type_helpers.rs` | `FunctionShape` | SAFE | Yes |
| `state/type_analysis/symbol_type_helpers.rs` | `TypeId` | SAFE | Yes |
| `state/type_environment/core.rs` | `TypeSubstitution` | COMPUTATION | Yes |
| `state/type_environment/core.rs` | `TypeSubstitution` | COMPUTATION | Yes |
| `state/type_environment/core.rs` | `TypeSubstitution` | COMPUTATION | Yes |
| `state/type_environment/core.rs` | `instantiate_type` | COMPUTATION | No |
| `state/type_environment/core.rs` | `instantiate_type` | COMPUTATION | No |
| `state/type_environment/core.rs` | `instantiate_type` | COMPUTATION | No |
| `state/type_environment/core.rs` | `instantiate_type_with_depth_status` | COMPUTATION | No |
| `state/type_environment/core.rs` | `CallSignature` | SAFE | Yes |
| `state/type_environment/core.rs` | `CallableShape` | SAFE | Yes |
| `state/type_environment/core.rs` | `IndexSignature` | SAFE | No |
| `state/type_environment/core.rs` | `MappedTypeId` | SAFE | Yes |
| `state/type_environment/core.rs` | `ObjectFlags` | SAFE | No |
| `state/type_environment/core.rs` | `ObjectShape` | SAFE | Yes |
| `state/type_environment/core.rs` | `ObjectShape` | SAFE | Yes |
| `state/type_environment/core.rs` | `ParamInfo` | SAFE | No |
| `state/type_environment/core.rs` | `PropertyInfo` | SAFE | No |
| `state/type_environment/core.rs` | `PropertyInfo` | SAFE | No |
| `state/type_environment/core.rs` | `PropertyInfo` | SAFE | No |
| `state/type_environment/core.rs` | `SourceLocation` | SAFE | No |
| `state/type_environment/core.rs` | `TypeId` | SAFE | Yes |
| `state/type_environment/core.rs` | `Visibility` | SAFE | No |
| `state/type_environment/lazy.rs` | `TypeEvaluator` | COMPUTATION | No |
| `state/type_environment/lazy.rs` | `SymbolRef` | SAFE | No |
| `state/type_environment/lazy.rs` | `TypeId` | SAFE | Yes |
| `state/type_environment/lazy.rs` | `visitor::collect_enum_def_ids` | SAFE | No |
| `state/type_environment/lazy.rs` | `visitor::collect_lazy_def_ids` | SAFE | No |
| `state/type_environment/lazy.rs` | `visitor::collect_referenced_types` | SAFE | No |
| `state/type_environment/lazy.rs` | `visitor::collect_type_queries` | SAFE | No |
| `state/type_environment/lazy.rs` | `visitor::lazy_def_id` | SAFE | No |
| `state/type_resolution/constructors.rs` | `CallableShape` | SAFE | Yes |
| `state/type_resolution/constructors.rs` | `CallableShape` | SAFE | Yes |
| `state/type_resolution/constructors.rs` | `TypeId` | SAFE | Yes |
| `state/type_resolution/core.rs` | `TypeId` | SAFE | Yes |
| `state/type_resolution/core.rs` | `def::DefId` | SAFE | No |
| `state/type_resolution/core.rs` | `is_compiler_managed_type` | SAFE | No |
| `state/type_resolution/import_type.rs` | `TypeId` | SAFE | Yes |
| `state/type_resolution/judge.rs` | `TypeId` | SAFE | Yes |
| `state/type_resolution/judge.rs` | `judge::DefaultJudge` | SAFE | No |
| `state/type_resolution/judge.rs` | `judge::Judge` | SAFE | No |
| `state/type_resolution/judge.rs` | `judge::JudgeConfig` | SAFE | No |
| `state/type_resolution/module.rs` | `TypeId` | SAFE | Yes |
| `state/variable_checking/core.rs` | `TypeId` | SAFE | Yes |
| `state/variable_checking/destructuring.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `state/variable_checking/destructuring.rs` | `TypeId` | SAFE | Yes |
| `state/variable_checking/for_loop.rs` | `TypeId` | SAFE | Yes |
| `statements.rs` | `TypeId` | SAFE | Yes |
| `symbols/symbol_resolver.rs` | `TypeId` | SAFE | Yes |
| `symbols/symbol_resolver.rs` | `is_compiler_managed_type` | SAFE | No |
| `symbols/symbol_resolver_utils.rs` | `TypeId` | SAFE | Yes |
| `types/class_type/constructor.rs` | `TypeSubstitution` | COMPUTATION | Yes |
| `types/class_type/constructor.rs` | `instantiate_type` | COMPUTATION | No |
| `types/class_type/constructor.rs` | `CallSignature` | SAFE | Yes |
| `types/class_type/constructor.rs` | `CallableShape` | SAFE | Yes |
| `types/class_type/constructor.rs` | `IndexSignature` | SAFE | No |
| `types/class_type/constructor.rs` | `PropertyInfo` | SAFE | No |
| `types/class_type/constructor.rs` | `TypeId` | SAFE | Yes |
| `types/class_type/constructor.rs` | `TypeParamInfo` | SAFE | No |
| `types/class_type/constructor.rs` | `TypePredicate` | SAFE | No |
| `types/class_type/constructor.rs` | `Visibility` | SAFE | No |
| `types/class_type/constructor.rs` | `types::ParamInfo` | SAFE | No |
| `types/class_type/constructor.rs` | `visitor::is_template_literal_type` | SAFE | No |
| `types/class_type/core.rs` | `TypeSubstitution` | COMPUTATION | Yes |
| `types/class_type/core.rs` | `instantiate_type` | COMPUTATION | No |
| `types/class_type/core.rs` | `CallSignature` | SAFE | Yes |
| `types/class_type/core.rs` | `CallableShape` | SAFE | Yes |
| `types/class_type/core.rs` | `IndexSignature` | SAFE | No |
| `types/class_type/core.rs` | `ObjectFlags` | SAFE | No |
| `types/class_type/core.rs` | `ObjectShape` | SAFE | Yes |
| `types/class_type/core.rs` | `PropertyInfo` | SAFE | No |
| `types/class_type/core.rs` | `TypeId` | SAFE | Yes |
| `types/class_type/core.rs` | `TypeId` | SAFE | Yes |
| `types/class_type/core.rs` | `TypeParamInfo` | SAFE | No |
| `types/class_type/core.rs` | `Visibility` | SAFE | No |
| `types/class_type/core.rs` | `visitor::is_template_literal_type` | SAFE | No |
| `types/computation/access.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `types/computation/access.rs` | `TypeInterner` | CONSTRUCTION | No |
| `types/computation/access.rs` | `TypeId` | SAFE | Yes |
| `types/computation/access.rs` | `visitor` | SAFE | No |
| `types/computation/access.rs` | `visitor` | SAFE | No |
| `types/computation/access.rs` | `visitor` | SAFE | No |
| `types/computation/access.rs` | `visitor` | SAFE | No |
| `types/computation/binary.rs` | `BinaryOpEvaluator` | COMPUTATION | No |
| `types/computation/binary.rs` | `BinaryOpResult` | COMPUTATION | No |
| `types/computation/binary.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `types/computation/binary.rs` | `TypeId` | SAFE | Yes |
| `types/computation/call.rs` | `CallResult` | COMPUTATION | Yes |
| `types/computation/call.rs` | `TypeSubstitution` | COMPUTATION | Yes |
| `types/computation/call.rs` | `instantiate_type` | COMPUTATION | No |
| `types/computation/call.rs` | `ContextualTypeContext` | SAFE | No |
| `types/computation/call.rs` | `FunctionShape` | SAFE | Yes |
| `types/computation/call.rs` | `TypeId` | SAFE | Yes |
| `types/computation/call_display.rs` | `FunctionShape` | SAFE | Yes |
| `types/computation/call_display.rs` | `TypeId` | SAFE | Yes |
| `types/computation/call_helpers.rs` | `TypeId` | SAFE | Yes |
| `types/computation/call_inference.rs` | `CallResult` | COMPUTATION | Yes |
| `types/computation/call_inference.rs` | `FunctionShape` | SAFE | Yes |
| `types/computation/call_inference.rs` | `TypeId` | SAFE | Yes |
| `types/computation/call_result.rs` | `CallResult` | COMPUTATION | Yes |
| `types/computation/call_result.rs` | `TypeId` | SAFE | Yes |
| `types/computation/call_result.rs` | `visitor` | SAFE | No |
| `types/computation/complex.rs` | `CallResult` | COMPUTATION | Yes |
| `types/computation/complex.rs` | `ContextualTypeContext` | SAFE | No |
| `types/computation/complex.rs` | `PropertyInfo` | SAFE | No |
| `types/computation/complex.rs` | `PropertyInfo` | SAFE | No |
| `types/computation/complex.rs` | `TypeId` | SAFE | Yes |
| `types/computation/complex.rs` | `Visibility` | SAFE | No |
| `types/computation/complex.rs` | `type_queries::TypeResolutionKind` | SAFE | Yes |
| `types/computation/complex.rs` | `type_queries::classify_for_type_resolution` | SAFE | No |
| `types/computation/complex_constructors.rs` | `TypeId` | SAFE | Yes |
| `types/computation/helpers.rs` | `BinaryOpEvaluator` | COMPUTATION | No |
| `types/computation/helpers.rs` | `expression_ops` | COMPUTATION | No |
| `types/computation/helpers.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `types/computation/helpers.rs` | `ContextualTypeContext` | SAFE | No |
| `types/computation/helpers.rs` | `FunctionShape` | SAFE | Yes |
| `types/computation/helpers.rs` | `FunctionShape` | SAFE | Yes |
| `types/computation/helpers.rs` | `FunctionShape` | SAFE | Yes |
| `types/computation/helpers.rs` | `PropertyInfo` | SAFE | No |
| `types/computation/helpers.rs` | `TupleElement` | SAFE | Yes |
| `types/computation/helpers.rs` | `TypeId` | SAFE | Yes |
| `types/computation/helpers.rs` | `Visibility` | SAFE | No |
| `types/computation/helpers.rs` | `type_queries::LiteralTypeKind` | SAFE | Yes |
| `types/computation/helpers.rs` | `type_queries::classify_literal_type` | SAFE | No |
| `types/computation/identifier.rs` | `IndexKind` | COMPUTATION | No |
| `types/computation/identifier.rs` | `IndexKind` | COMPUTATION | No |
| `types/computation/identifier.rs` | `IndexSignatureResolver` | COMPUTATION | No |
| `types/computation/identifier.rs` | `IndexSignatureResolver` | COMPUTATION | No |
| `types/computation/identifier.rs` | `relations::freshness::is_fresh_object_type` | COMPUTATION | No |
| `types/computation/identifier.rs` | `relations::freshness::widen_freshness` | COMPUTATION | No |
| `types/computation/identifier.rs` | `TypeId` | SAFE | Yes |
| `types/computation/object_literal.rs` | `CallSignature` | SAFE | Yes |
| `types/computation/object_literal.rs` | `CallableShape` | SAFE | Yes |
| `types/computation/object_literal.rs` | `ContextualTypeContext` | SAFE | No |
| `types/computation/object_literal.rs` | `IndexSignature` | SAFE | No |
| `types/computation/object_literal.rs` | `ObjectFlags` | SAFE | No |
| `types/computation/object_literal.rs` | `ObjectShape` | SAFE | Yes |
| `types/computation/object_literal.rs` | `PropertyInfo` | SAFE | No |
| `types/computation/object_literal.rs` | `TypeId` | SAFE | Yes |
| `types/computation/object_literal.rs` | `Visibility` | SAFE | No |
| `types/computation/object_literal_context.rs` | `TypeEvaluator` | COMPUTATION | No |
| `types/computation/object_literal_context.rs` | `TypeEvaluator` | COMPUTATION | No |
| `types/computation/object_literal_context.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `types/computation/object_literal_context.rs` | `TypeId` | SAFE | Yes |
| `types/computation/tagged_template.rs` | `instantiate_type` | COMPUTATION | No |
| `types/computation/tagged_template.rs` | `ContextualTypeContext` | SAFE | No |
| `types/computation/tagged_template.rs` | `TypeId` | SAFE | Yes |
| `types/function_iife_inference.rs` | `TypeId` | SAFE | Yes |
| `types/function_type.rs` | `ContextualTypeContext` | SAFE | No |
| `types/function_type.rs` | `FunctionShape` | SAFE | Yes |
| `types/function_type.rs` | `ParamInfo` | SAFE | No |
| `types/function_type.rs` | `TypeId` | SAFE | Yes |
| `types/function_type.rs` | `TypeParamInfo` | SAFE | No |
| `types/function_type.rs` | `TypePredicate` | SAFE | No |
| `types/function_type.rs` | `TypePredicateTarget` | SAFE | No |
| `types/function_type.rs` | `type_queries::EvaluationNeeded` | SAFE | Yes |
| `types/function_type.rs` | `type_queries::classify_for_evaluation` | SAFE | Yes |
| `types/function_type.rs` | `type_queries::get_function_shape` | SAFE | No |
| `types/function_type.rs` | `type_queries::get_lazy_def_id` | SAFE | No |
| `types/function_type.rs` | `type_queries::get_type_application` | SAFE | No |
| `types/function_type_circular.rs` | `TypeId` | SAFE | Yes |
| `types/interface_type.rs` | `TypeSubstitution` | COMPUTATION | Yes |
| `types/interface_type.rs` | `instantiate_type` | COMPUTATION | No |
| `types/interface_type.rs` | `CallSignature as SolverCallSignature` | SAFE | No |
| `types/interface_type.rs` | `CallableShape` | SAFE | Yes |
| `types/interface_type.rs` | `CallableShape` | SAFE | Yes |
| `types/interface_type.rs` | `CallableShape` | SAFE | Yes |
| `types/interface_type.rs` | `IndexSignature` | SAFE | No |
| `types/interface_type.rs` | `ObjectFlags` | SAFE | No |
| `types/interface_type.rs` | `ObjectFlags` | SAFE | No |
| `types/interface_type.rs` | `ObjectFlags` | SAFE | No |
| `types/interface_type.rs` | `ObjectShape` | SAFE | Yes |
| `types/interface_type.rs` | `ObjectShape` | SAFE | Yes |
| `types/interface_type.rs` | `ObjectShape` | SAFE | Yes |
| `types/interface_type.rs` | `PropertyInfo` | SAFE | No |
| `types/interface_type.rs` | `PropertyInfo` | SAFE | No |
| `types/interface_type.rs` | `TypeId` | SAFE | Yes |
| `types/interface_type.rs` | `TypeId` | SAFE | Yes |
| `types/interface_type.rs` | `Visibility` | SAFE | No |
| `types/interface_type.rs` | `type_queries::AugmentationTargetKind` | SAFE | No |
| `types/interface_type.rs` | `type_queries::InterfaceMergeKind` | SAFE | No |
| `types/interface_type.rs` | `type_queries::InterfaceMergeKind` | SAFE | No |
| `types/interface_type.rs` | `type_queries::classify_for_augmentation` | SAFE | No |
| `types/interface_type.rs` | `type_queries::classify_for_interface_merge` | SAFE | No |
| `types/interface_type.rs` | `type_queries::classify_for_interface_merge` | SAFE | No |
| `types/interface_type.rs` | `type_queries::get_intersection_members` | SAFE | No |
| `types/interface_type.rs` | `visitor::is_template_literal_type` | SAFE | No |
| `types/object_type.rs` | `TypeId` | SAFE | Yes |
| `types/property_access_helpers.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `types/property_access_helpers.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `types/property_access_helpers.rs` | `TypeInterner` | CONSTRUCTION | No |
| `types/property_access_helpers.rs` | `TypeId` | SAFE | Yes |
| `types/property_access_helpers.rs` | `visitor::is_function_type` | SAFE | No |
| `types/property_access_type.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `types/property_access_type.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `types/property_access_type.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `types/property_access_type.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `types/property_access_type.rs` | `TypeId` | SAFE | Yes |
| `types/property_access_type.rs` | `is_compiler_managed_type` | SAFE | No |
| `types/property_access_type.rs` | `is_compiler_managed_type` | SAFE | No |
| `types/queries/binding.rs` | `TypeId` | SAFE | Yes |
| `types/queries/callable_truthiness.rs` | `TypeId` | SAFE | Yes |
| `types/queries/callable_truthiness.rs` | `type_queries::LiteralTypeKind` | SAFE | Yes |
| `types/queries/callable_truthiness.rs` | `type_queries::classify_literal_type` | SAFE | No |
| `types/queries/callable_truthiness.rs` | `type_queries::get_enum_member_type` | SAFE | No |
| `types/queries/class.rs` | `TypeId` | SAFE | Yes |
| `types/queries/core.rs` | `TypeId` | SAFE | Yes |
| `types/queries/lib_prime.rs` | `is_compiler_managed_type` | SAFE | No |
| `types/queries/lib_resolution.rs` | `TypeId` | SAFE | Yes |
| `types/queries/lib_resolution.rs` | `is_compiler_managed_type` | SAFE | No |
| `types/queries/type_only.rs` | `TypeId` | SAFE | Yes |
| `types/queries/type_only.rs` | `type_queries::NamespaceMemberKind` | SAFE | No |
| `types/queries/type_only.rs` | `type_queries::NamespaceMemberKind` | SAFE | No |
| `types/queries/type_only.rs` | `type_queries::NamespaceMemberKind` | SAFE | No |
| `types/queries/type_only.rs` | `type_queries::classify_namespace_member` | SAFE | No |
| `types/queries/type_only.rs` | `type_queries::classify_namespace_member` | SAFE | No |
| `types/queries/type_only.rs` | `type_queries::classify_namespace_member` | SAFE | No |
| `types/type_checking/core.rs` | `TypeId` | SAFE | Yes |
| `types/type_checking/declarations.rs` | `TypeId` | SAFE | Yes |
| `types/type_checking/declarations_utils.rs` | `TypeId` | SAFE | Yes |
| `types/type_checking/duplicate_identifiers.rs` | `TypeId` | SAFE | Yes |
| `types/type_checking/duplicate_identifiers_merge.rs` | `Visibility` | SAFE | No |
| `types/type_checking/global.rs` | `IntrinsicKind` | SAFE | No |
| `types/type_checking/global.rs` | `IntrinsicKind` | SAFE | No |
| `types/type_checking/global.rs` | `TypeId` | SAFE | Yes |
| `types/type_checking/indexed_access.rs` | `TypeId` | SAFE | Yes |
| `types/type_literal_checker.rs` | `CallSignature` | SAFE | Yes |
| `types/type_literal_checker.rs` | `CallableShape` | SAFE | Yes |
| `types/type_literal_checker.rs` | `FunctionShape` | SAFE | Yes |
| `types/type_literal_checker.rs` | `IndexSignature` | SAFE | No |
| `types/type_literal_checker.rs` | `ObjectFlags` | SAFE | No |
| `types/type_literal_checker.rs` | `ObjectShape` | SAFE | Yes |
| `types/type_literal_checker.rs` | `PropertyInfo` | SAFE | No |
| `types/type_literal_checker.rs` | `TypeId` | SAFE | Yes |
| `types/type_literal_checker.rs` | `Visibility` | SAFE | No |
| `types/type_literal_checker.rs` | `visitor::is_template_literal_type` | SAFE | No |
| `types/type_node.rs` | `CallSignature` | SAFE | Yes |
| `types/type_node.rs` | `CallableShape` | SAFE | Yes |
| `types/type_node.rs` | `FunctionShape` | SAFE | Yes |
| `types/type_node.rs` | `IndexSignature` | SAFE | No |
| `types/type_node.rs` | `ObjectFlags` | SAFE | No |
| `types/type_node.rs` | `ObjectShape` | SAFE | Yes |
| `types/type_node.rs` | `ParamInfo` | SAFE | No |
| `types/type_node.rs` | `PropertyInfo` | SAFE | No |
| `types/type_node.rs` | `TupleElement` | SAFE | Yes |
| `types/type_node.rs` | `TypeId` | SAFE | Yes |
| `types/type_node.rs` | `Visibility` | SAFE | No |
| `types/type_node.rs` | `is_compiler_managed_type` | SAFE | No |
| `types/type_node.rs` | `recursion::DepthCounter` | SAFE | No |
| `types/type_node.rs` | `recursion::RecursionProfile` | SAFE | No |
| `types/type_node_helpers.rs` | `TypeId` | SAFE | Yes |
| `types/type_node_resolution.rs` | `is_compiler_managed_type` | SAFE | No |
| `types/utilities/core.rs` | `operations::property::PropertyAccessResult` | COMPUTATION | No |
| `types/utilities/core.rs` | `ContextualTypeContext` | SAFE | No |
| `types/utilities/core.rs` | `TypeId` | SAFE | Yes |
| `types/utilities/core.rs` | `type_queries` | SAFE | Yes |
| `types/utilities/core.rs` | `type_queries` | SAFE | Yes |
| `types/utilities/core.rs` | `type_queries` | SAFE | Yes |
| `types/utilities/enum_utils.rs` | `TypeId` | SAFE | Yes |
| `types/utilities/jsdoc.rs` | `instantiate_generic` | COMPUTATION | No |
| `types/utilities/jsdoc.rs` | `FunctionShape` | SAFE | Yes |
| `types/utilities/jsdoc.rs` | `FunctionShape` | SAFE | Yes |
| `types/utilities/jsdoc.rs` | `FunctionShape` | SAFE | Yes |
| `types/utilities/jsdoc.rs` | `FunctionShape` | SAFE | Yes |
| `types/utilities/jsdoc.rs` | `FunctionShape` | SAFE | Yes |
| `types/utilities/jsdoc.rs` | `FunctionShape` | SAFE | Yes |
| `types/utilities/jsdoc.rs` | `IndexSignature` | SAFE | No |
| `types/utilities/jsdoc.rs` | `MappedType` | SAFE | Yes |
| `types/utilities/jsdoc.rs` | `ObjectFlags` | SAFE | No |
| `types/utilities/jsdoc.rs` | `ObjectShape` | SAFE | Yes |
| `types/utilities/jsdoc.rs` | `ParamInfo` | SAFE | No |
| `types/utilities/jsdoc.rs` | `ParamInfo` | SAFE | No |
| `types/utilities/jsdoc.rs` | `ParamInfo` | SAFE | No |
| `types/utilities/jsdoc.rs` | `ParamInfo` | SAFE | No |
| `types/utilities/jsdoc.rs` | `ParamInfo` | SAFE | No |
| `types/utilities/jsdoc.rs` | `ParamInfo` | SAFE | No |
| `types/utilities/jsdoc.rs` | `PropertyInfo` | SAFE | No |
| `types/utilities/jsdoc.rs` | `SymbolRef` | SAFE | No |
| `types/utilities/jsdoc.rs` | `TupleElement` | SAFE | Yes |
| `types/utilities/jsdoc.rs` | `TypeId` | SAFE | Yes |
| `types/utilities/jsdoc.rs` | `TypeParamInfo` | SAFE | No |
| `types/utilities/jsdoc.rs` | `TypePredicate` | SAFE | No |
| `types/utilities/jsdoc.rs` | `TypePredicateTarget` | SAFE | No |
| `types/utilities/jsdoc.rs` | `Visibility` | SAFE | No |
| `types/utilities/jsdoc.rs` | `def::DefinitionInfo` | SAFE | No |
| `types/utilities/jsdoc_params.rs` | `TypeId` | SAFE | Yes |
| `types/utilities/return_type.rs` | `SymbolRef` | SAFE | No |
| `types/utilities/return_type.rs` | `TypeId` | SAFE | Yes |
| `types/utilities/return_type.rs` | `lazy_def_id` | SAFE | No |

## Architecture Violations (INTERNAL)

These imports access solver-internal state and should be removed:

- **`crates/tsz-checker/src/context/compiler_options.rs`:179** -- `RelationCacheKey`
- **`crates/tsz-checker/src/assignability/assignability_checker.rs`:25** -- `RelationCacheKey`
- **`crates/tsz-checker/src/assignability/subtype_identity_checker.rs`:12** -- `RelationCacheKey`

## Construction Imports (CONSTRUCTION)

These imports use `TypeInterner` to construct types directly:

- **`crates/tsz-checker/src/types/property_access_helpers.rs`:555** -- `TypeInterner`
- **`crates/tsz-checker/src/types/computation/access.rs`:1696** -- `TypeInterner`
- **`crates/tsz-checker/src/classes/class_inheritance.rs`:539** -- `TypeInterner`
- **`crates/tsz-checker/src/state/state_checking/readonly.rs`:1055** -- `TypeInterner`

## Top COMPUTATION Imports by Frequency

| Import | Count | Files |
|--------|-------|-------|
| `operations::property::PropertyAccessResult` | 40 | 21 files |
| `instantiate_type` | 14 | 10 files |
| `TypeSubstitution` | 11 | 9 files |
| `BinaryOpEvaluator` | 7 | 6 files |
| `CallResult` | 5 | 5 files |
| `BinaryOpResult` | 3 | 3 files |
| `TypeEvaluator` | 3 | 2 files |
| `relations::freshness` | 3 | 2 files |
| `IndexKind` | 2 | 1 files |
| `IndexSignatureResolver` | 2 | 1 files |
| `relations::freshness::is_fresh_object_type` | 2 | 2 files |
| `relations::freshness::widen_freshness` | 2 | 2 files |
| `operations::property::is_readonly_tuple_fixed_element` | 2 | 1 files |
| `objects::index_signatures::IndexKind` | 2 | 2 files |
| `objects::index_signatures::IndexSignatureResolver` | 2 | 2 files |
| `TypeResolver` | 2 | 2 files |
| `widening::apply_const_assertion` | 1 | 1 files |
| `AssignabilityChecker` | 1 | 1 files |
| `operations::CallResult` | 1 | 1 files |
| `operations::property::PropertyAccessEvaluator` | 1 | 1 files |