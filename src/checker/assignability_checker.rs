//! Assignability Checking Module
//!
//! This module contains methods for checking type assignability and subtyping.
//! It handles:
//! - Basic assignability checks (`is_assignable_to`)
//! - Subtype checking (`is_subtype_of`)
//! - Type identity (`are_types_identical`)
//! - Union type assignability
//! - Excess property checking for object literals
//! - Weak type union violations
//!
//! This module extends CheckerState with assignability-related methods as part of
//! the Phase 2 architecture refactoring (task 2.3 - file splitting).

use crate::checker::state::{CheckerOverrideProvider, CheckerState};
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::solver::TypeId;

// =============================================================================
// Assignability Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Type Evaluation for Assignability
    // =========================================================================

    /// Evaluate a type for assignability checking.
    ///
    /// Determines if the type needs evaluation (applications, env-dependent types)
    /// and performs the appropriate evaluation.
    pub(crate) fn evaluate_type_for_assignability(&mut self, type_id: TypeId) -> TypeId {
        use crate::solver::type_queries::{AssignabilityEvalKind, classify_for_assignability_eval};

        match classify_for_assignability_eval(self.ctx.types, type_id) {
            AssignabilityEvalKind::Application => self.evaluate_type_with_resolution(type_id),
            AssignabilityEvalKind::NeedsEnvEval => self.evaluate_type_with_env(type_id),
            AssignabilityEvalKind::Resolved => type_id,
        }
    }

    // =========================================================================
    // Main Assignability Check
    // =========================================================================

    /// Check if source type is assignable to target type.
    ///
    /// This is the main entry point for assignability checking, used throughout
    /// the type system to validate assignments, function calls, returns, etc.
    /// Assignability is more permissive than subtyping.
    pub fn is_assignable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        use crate::solver::CompatChecker;

        self.ensure_application_symbols_resolved(source);
        self.ensure_application_symbols_resolved(target);

        let source = self.evaluate_type_for_assignability(source);
        let target = self.evaluate_type_for_assignability(target);

        let env = self.ctx.type_env.borrow();
        let overrides = CheckerOverrideProvider::new(self, Some(&*env));
        let mut checker = CompatChecker::with_resolver(self.ctx.types, &*env);
        checker.set_strict_function_types(self.ctx.strict_function_types());
        checker.set_strict_null_checks(self.ctx.strict_null_checks());
        checker.set_exact_optional_property_types(self.ctx.exact_optional_property_types());
        checker.set_no_unchecked_indexed_access(self.ctx.no_unchecked_indexed_access());
        checker.is_assignable_with_overrides(source, target, &overrides)
    }

    /// Check if `source` type is assignable to `target` type, resolving Ref types.
    ///
    /// Uses the provided TypeEnvironment to resolve type references.
    pub fn is_assignable_to_with_env(
        &self,
        source: TypeId,
        target: TypeId,
        env: &crate::solver::TypeEnvironment,
    ) -> bool {
        use crate::solver::CompatChecker;

        let overrides = CheckerOverrideProvider::new(self, Some(env));
        let mut checker = CompatChecker::with_resolver(self.ctx.types, env);
        checker.set_strict_function_types(self.ctx.strict_function_types());
        checker.set_strict_null_checks(self.ctx.strict_null_checks());
        checker.set_exact_optional_property_types(self.ctx.exact_optional_property_types());
        checker.set_no_unchecked_indexed_access(self.ctx.no_unchecked_indexed_access());
        checker.is_assignable_with_overrides(source, target, &overrides)
    }

    // =========================================================================
    // Weak Union and Excess Property Checking
    // =========================================================================

    /// Check if we should skip the general assignability error for an object literal.
    /// Returns true if:
    /// 1. It's a weak union violation (TypeScript shows excess property error instead)
    /// 2. OR if the object literal has excess properties (TypeScript prioritizes TS2353 over TS2345/TS2322)
    pub(crate) fn should_skip_weak_union_error(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> bool {
        use crate::solver::CompatChecker;

        let Some(node) = self.ctx.arena.get(source_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }

        // Check for weak union violation first (using scoped borrow)
        let is_weak_union_violation = {
            let env = self.ctx.type_env.borrow();
            let mut checker = CompatChecker::with_resolver(self.ctx.types, &*env);
            checker.set_strict_function_types(self.ctx.strict_function_types());
            checker.set_strict_null_checks(self.ctx.strict_null_checks());
            checker.set_exact_optional_property_types(self.ctx.exact_optional_property_types());
            checker.set_no_unchecked_indexed_access(self.ctx.no_unchecked_indexed_access());
            checker.is_weak_union_violation(source, target)
        };

        if is_weak_union_violation {
            return true;
        }

        // Check if there are excess properties.
        if !self.object_literal_has_excess_properties(source, target) {
            return false;
        }

        // There are excess properties. Check if all matching properties have compatible types.
        let Some(source_shape) =
            crate::solver::type_queries::get_object_shape(self.ctx.types, source)
        else {
            return true;
        };

        let resolved_target = self.resolve_type_for_property_access(target);
        let Some(target_shape) =
            crate::solver::type_queries::get_object_shape(self.ctx.types, resolved_target)
        else {
            return true;
        };

        let source_props = source_shape.properties.as_slice();
        let target_props = target_shape.properties.as_slice();

        // Check if any source property that exists in target has a wrong type
        for source_prop in source_props {
            if let Some(target_prop) = target_props.iter().find(|p| p.name == source_prop.name) {
                let source_prop_type = source_prop.type_id;
                let target_prop_type = target_prop.type_id;

                let effective_target_type = if target_prop.optional {
                    self.ctx
                        .types
                        .union(vec![target_prop_type, TypeId::UNDEFINED])
                } else {
                    target_prop_type
                };

                let is_assignable = {
                    let env = self.ctx.type_env.borrow();
                    let mut checker = CompatChecker::with_resolver(self.ctx.types, &*env);
                    checker.set_strict_function_types(self.ctx.strict_function_types());
                    checker.set_strict_null_checks(self.ctx.strict_null_checks());
                    checker.is_assignable(source_prop_type, effective_target_type)
                };

                if !is_assignable {
                    return false;
                }
            }
        }

        true
    }

    /// Check if source object literal has properties that don't exist in target.
    pub(crate) fn object_literal_has_excess_properties(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        use crate::solver::type_queries::{ExcessPropertiesKind, classify_for_excess_properties};

        if !self
            .ctx
            .freshness_tracker
            .should_check_excess_properties(source)
        {
            return false;
        }

        let Some(source_shape) =
            crate::solver::type_queries::get_object_shape(self.ctx.types, source)
        else {
            return false;
        };

        let source_props = source_shape.properties.as_slice();
        if source_props.is_empty() {
            return false;
        }

        let resolved_target = self.resolve_type_for_property_access(target);

        match classify_for_excess_properties(self.ctx.types, resolved_target) {
            ExcessPropertiesKind::Object(shape_id) => {
                let target_shape = self.ctx.types.object_shape(shape_id);
                let target_props = target_shape.properties.as_slice();

                if target_props.is_empty() {
                    return false;
                }

                if target_shape.string_index.is_some() || target_shape.number_index.is_some() {
                    return false;
                }

                source_props
                    .iter()
                    .any(|source_prop| !target_props.iter().any(|p| p.name == source_prop.name))
            }
            ExcessPropertiesKind::ObjectWithIndex(_shape_id) => false,
            ExcessPropertiesKind::Union(members) => {
                let mut target_shapes = Vec::new();

                for member in members {
                    let resolved_member = self.resolve_type_for_property_access(member);
                    let Some(shape) = crate::solver::type_queries::get_object_shape(
                        self.ctx.types,
                        resolved_member,
                    ) else {
                        continue;
                    };

                    if shape.properties.is_empty()
                        || shape.string_index.is_some()
                        || shape.number_index.is_some()
                    {
                        return false;
                    }

                    target_shapes.push(shape);
                }

                if target_shapes.is_empty() {
                    return false;
                }

                source_props.iter().any(|source_prop| {
                    !target_shapes.iter().any(|shape| {
                        shape
                            .properties
                            .iter()
                            .any(|prop| prop.name == source_prop.name)
                    })
                })
            }
            ExcessPropertiesKind::NotObject => false,
        }
    }

    // =========================================================================
    // Subtype Checking
    // =========================================================================

    /// Check if `source` type is a subtype of `target` type.
    ///
    /// This is the main entry point for subtype checking, used for type compatibility
    /// throughout the type system. Subtyping is stricter than assignability.
    pub fn is_subtype_of(&mut self, source: TypeId, target: TypeId) -> bool {
        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};
        use crate::solver::SubtypeChecker;
        let depth_exceeded = {
            let env = self.ctx.type_env.borrow();
            let mut checker = SubtypeChecker::with_resolver(self.ctx.types, &*env)
                .with_strict_null_checks(self.ctx.strict_null_checks());
            let result = checker.is_subtype_of(source, target);
            let depth_exceeded = checker.depth_exceeded;
            (result, depth_exceeded)
        };

        if depth_exceeded.1 {
            self.error_at_current_node(
                diagnostic_messages::TYPE_INSTANTIATION_EXCESSIVELY_DEEP,
                diagnostic_codes::TYPE_INSTANTIATION_EXCESSIVELY_DEEP,
            );
        }

        depth_exceeded.0
    }

    /// Check if source type is a subtype of target type with explicit environment.
    pub fn is_subtype_of_with_env(
        &mut self,
        source: TypeId,
        target: TypeId,
        env: &crate::solver::TypeEnvironment,
    ) -> bool {
        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};
        use crate::solver::SubtypeChecker;
        let mut checker = SubtypeChecker::with_resolver(self.ctx.types, env)
            .with_strict_null_checks(self.ctx.strict_null_checks());
        let result = checker.is_subtype_of(source, target);
        let depth_exceeded = checker.depth_exceeded;

        if depth_exceeded {
            self.error_at_current_node(
                diagnostic_messages::TYPE_INSTANTIATION_EXCESSIVELY_DEEP,
                diagnostic_codes::TYPE_INSTANTIATION_EXCESSIVELY_DEEP,
            );
        }

        result
    }

    // =========================================================================
    // Type Identity and Compatibility
    // =========================================================================

    /// Check if two types are identical (same TypeId).
    pub fn are_types_identical(&self, type1: TypeId, type2: TypeId) -> bool {
        type1 == type2
    }

    /// Check if variable declaration types are compatible (used for multiple declarations).
    pub(crate) fn are_var_decl_types_compatible(
        &mut self,
        prev_type: TypeId,
        current_type: TypeId,
    ) -> bool {
        let prev_type = self
            .enum_symbol_from_value_type(prev_type)
            .and_then(|sym_id| self.enum_object_type(sym_id))
            .unwrap_or(prev_type);
        let current_type = self
            .enum_symbol_from_value_type(current_type)
            .and_then(|sym_id| self.enum_object_type(sym_id))
            .unwrap_or(current_type);

        if prev_type == current_type {
            return true;
        }
        if matches!(prev_type, TypeId::ERROR) || matches!(current_type, TypeId::ERROR) {
            return true;
        }
        self.ensure_application_symbols_resolved(prev_type);
        self.ensure_application_symbols_resolved(current_type);
        self.is_assignable_to(prev_type, current_type)
            && self.is_assignable_to(current_type, prev_type)
    }

    /// Check if source type is assignable to ANY member of a target union.
    pub fn is_assignable_to_union(&self, source: TypeId, targets: &[TypeId]) -> bool {
        use crate::solver::CompatChecker;
        let env = self.ctx.type_env.borrow();
        let mut checker = CompatChecker::with_resolver(self.ctx.types, &*env);
        checker.set_strict_function_types(self.ctx.strict_function_types());
        checker.set_strict_null_checks(self.ctx.strict_null_checks());
        checker.set_exact_optional_property_types(self.ctx.exact_optional_property_types());
        checker.set_no_unchecked_indexed_access(self.ctx.no_unchecked_indexed_access());
        for &target in targets {
            if checker.is_assignable(source, target) {
                return true;
            }
        }
        false
    }
}
