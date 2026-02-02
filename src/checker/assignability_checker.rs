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
use tracing::trace;

// =============================================================================
// Assignability Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Type Evaluation for Assignability
    // =========================================================================

    /// Ensure all Ref types in a type are resolved and in the type environment.
    ///
    /// This is critical for intersection/union type assignability. When we have
    /// `type AB = A & B`, the intersection contains Ref(A) and Ref(B). Before we
    /// can check assignability against the intersection, we need to ensure A and B
    /// are resolved and in type_env so the subtype checker can resolve them.
    fn ensure_refs_resolved(&mut self, type_id: TypeId) {
        use crate::solver::TypeKey;

        let Some(type_key) = self.ctx.types.lookup(type_id) else {
            return;
        };

        match type_key {
            // For Ref types, resolve the symbol to ensure it's in type_env
            TypeKey::Ref(symbol_ref) => {
                let sym_id = crate::binder::SymbolId(symbol_ref.0);
                // Call get_type_of_symbol which will resolve and cache in type_env
                // This is LAZY resolution - only resolve when needed for checking
                let _ = self.get_type_of_symbol(sym_id);
            }

            // For Lazy(DefId) types, resolve via the reverse DefId→SymbolId mapping
            TypeKey::Lazy(def_id) => {
                if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) {
                    let _ = self.get_type_of_symbol(sym_id);
                }
            }

            // For intersections, ensure all members are resolved
            TypeKey::Intersection(members) => {
                let member_list = self.ctx.types.type_list(members);
                for &member in member_list.iter() {
                    self.ensure_refs_resolved(member);
                }
            }

            // For unions, ensure all members are resolved
            TypeKey::Union(members) => {
                let member_list = self.ctx.types.type_list(members);
                for &member in member_list.iter() {
                    self.ensure_refs_resolved(member);
                }
            }

            // For type applications, ensure base and args are resolved
            TypeKey::Application(app_id) => {
                let app = self.ctx.types.type_application(app_id);
                self.ensure_refs_resolved(app.base);
                for &arg in &app.args {
                    self.ensure_refs_resolved(arg);
                }
            }

            // For functions, resolve parameter and return types
            TypeKey::Function(sig) => {
                let func_sig = self.ctx.types.function_shape(sig);
                self.ensure_refs_resolved(func_sig.return_type);
                for param in &func_sig.params {
                    self.ensure_refs_resolved(param.type_id);
                }
            }

            // For objects, resolve property types and index signatures
            TypeKey::Object(shape_id) => {
                let shape = self.ctx.types.object_shape(shape_id);
                for prop in &shape.properties {
                    self.ensure_refs_resolved(prop.type_id);
                }
                if let Some(ref sig) = shape.string_index {
                    self.ensure_refs_resolved(sig.value_type);
                }
                if let Some(ref sig) = shape.number_index {
                    self.ensure_refs_resolved(sig.value_type);
                }
            }

            // For object with index signature
            TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.ctx.types.object_shape(shape_id);
                for prop in &shape.properties {
                    self.ensure_refs_resolved(prop.type_id);
                }
                if let Some(ref sig) = shape.string_index {
                    self.ensure_refs_resolved(sig.value_type);
                }
                if let Some(ref sig) = shape.number_index {
                    self.ensure_refs_resolved(sig.value_type);
                }
            }

            // For type queries, resolve the referenced symbol
            TypeKey::TypeQuery(symbol_ref) => {
                let sym_id = crate::binder::SymbolId(symbol_ref.0);
                let _ = self.get_type_of_symbol(sym_id);
            }

            // For arrays, readonly, resolve the inner type
            TypeKey::Array(inner) | TypeKey::ReadonlyType(inner) | TypeKey::KeyOf(inner) => {
                self.ensure_refs_resolved(inner);
            }

            // For tuples, resolve each element type
            TypeKey::Tuple(tuple_id) => {
                let elems = self.ctx.types.tuple_list(tuple_id);
                for elem in elems.iter() {
                    self.ensure_refs_resolved(elem.type_id);
                }
            }

            // For callables (overloaded signatures), resolve all signatures
            TypeKey::Callable(callable_id) => {
                let callable = self.ctx.types.callable_shape(callable_id);
                for sig in &callable.call_signatures {
                    self.ensure_refs_resolved(sig.return_type);
                    for param in &sig.params {
                        self.ensure_refs_resolved(param.type_id);
                    }
                }
                for sig in &callable.construct_signatures {
                    self.ensure_refs_resolved(sig.return_type);
                    for param in &sig.params {
                        self.ensure_refs_resolved(param.type_id);
                    }
                }
            }

            // For mapped types, resolve constraint and template
            TypeKey::Mapped(mapped_id) => {
                let mapped = self.ctx.types.mapped_type(mapped_id);
                self.ensure_refs_resolved(mapped.constraint);
                self.ensure_refs_resolved(mapped.template);
                if let Some(name_type) = mapped.name_type {
                    self.ensure_refs_resolved(name_type);
                }
            }

            // For conditional types, resolve all four components
            TypeKey::Conditional(cond_id) => {
                let cond = self.ctx.types.conditional_type(cond_id);
                self.ensure_refs_resolved(cond.check_type);
                self.ensure_refs_resolved(cond.extends_type);
                self.ensure_refs_resolved(cond.true_type);
                self.ensure_refs_resolved(cond.false_type);
            }

            // For index access types, resolve both object and index
            TypeKey::IndexAccess(obj, idx) => {
                self.ensure_refs_resolved(obj);
                self.ensure_refs_resolved(idx);
            }

            // For type parameters, resolve constraints
            TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
                if let Some(constraint) = info.constraint {
                    self.ensure_refs_resolved(constraint);
                }
            }

            // For other types (primitives, literals, etc.), no resolution needed
            _ => {}
        }
    }

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

        // CRITICAL: Ensure all Ref types are resolved before assignability check.
        // This fixes intersection type assignability where `type AB = A & B` needs
        // A and B in type_env before we can check if a type is assignable to the intersection.
        self.ensure_refs_resolved(source);
        self.ensure_refs_resolved(target);

        self.ensure_application_symbols_resolved(source);
        self.ensure_application_symbols_resolved(target);

        let source = self.evaluate_type_for_assignability(source);
        let target = self.evaluate_type_for_assignability(target);

        let env = self.ctx.type_env.borrow();
        let overrides = CheckerOverrideProvider::new(self, Some(&*env));
        let mut checker = CompatChecker::with_resolver(self.ctx.types, &*env);
        self.ctx.configure_compat_checker(&mut checker);
        let result = checker.is_assignable_with_overrides(source, target, &overrides);
        trace!(
            source = source.0,
            target = target.0,
            result,
            "is_assignable_to"
        );
        result
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
        self.ctx.configure_compat_checker(&mut checker);
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
            self.ctx.configure_compat_checker(&mut checker);
            checker.is_weak_union_violation(source, target)
        };

        if is_weak_union_violation {
            return true;
        }

        // Check if there are excess properties.
        if !self.object_literal_has_excess_properties(source, target, source_idx) {
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
                    self.ctx.configure_compat_checker(&mut checker);
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
    ///
    /// Uses syntactic freshness checking - the source_idx must be provided to determine
    /// if the expression is a fresh object literal (not a variable reference).
    pub(crate) fn object_literal_has_excess_properties(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> bool {
        use crate::solver::type_queries::{ExcessPropertiesKind, classify_for_excess_properties};

        // Check syntactic freshness - only object literals directly in the source
        // should trigger excess property checking, not variables
        if !self.is_syntactically_fresh(source_idx) {
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
        use crate::binder::symbol_flags;
        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};
        use crate::solver::SubtypeChecker;
        use crate::solver::visitor::contains_infer_types;

        // Fast path: identity check
        if source == target {
            return true;
        }

        // Check relation cache for non-inference types
        // Key format: (source, target, relation_kind) where 0 = subtype
        const SUBTYPE_RELATION: u8 = 0;
        let cache_key = (source, target, SUBTYPE_RELATION);
        let is_cacheable = !contains_infer_types(self.ctx.types, source)
            && !contains_infer_types(self.ctx.types, target);

        if is_cacheable {
            if let Some(&cached) = self.ctx.relation_cache.borrow().get(&cache_key) {
                return cached;
            }
        }

        // CRITICAL: Before checking subtypes, ensure all Ref types in source and target
        // are resolved and in the type environment. This fixes intersection type
        // assignability where `type AB = A & B` needs A and B in type_env before
        // we can check if a type is assignable to the intersection.
        self.ensure_refs_resolved(source);
        self.ensure_refs_resolved(target);

        let depth_exceeded = {
            let env = self.ctx.type_env.borrow();
            let binder = self.ctx.binder;

            // Helper to check if a symbol is a class (for nominal subtyping)
            let is_class_fn = |sym_ref: crate::solver::types::SymbolRef| -> bool {
                let sym_id = crate::binder::SymbolId(sym_ref.0);
                if let Some(sym) = binder.get_symbol(sym_id) {
                    (sym.flags & symbol_flags::CLASS) != 0
                } else {
                    false
                }
            };

            let mut checker = SubtypeChecker::with_resolver(self.ctx.types, &*env)
                .with_strict_null_checks(self.ctx.strict_null_checks())
                .with_inheritance_graph(&self.ctx.inheritance_graph)
                .with_class_check(&is_class_fn);
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

        let result = depth_exceeded.0;

        // Cache the result for non-inference types
        if is_cacheable {
            self.ctx
                .relation_cache
                .borrow_mut()
                .insert(cache_key, result);
        }

        result
    }

    /// Check if source type is a subtype of target type with explicit environment.
    pub fn is_subtype_of_with_env(
        &mut self,
        source: TypeId,
        target: TypeId,
        env: &crate::solver::TypeEnvironment,
    ) -> bool {
        use crate::binder::symbol_flags;
        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};
        use crate::solver::SubtypeChecker;

        // CRITICAL: Before checking subtypes, ensure all Ref types are resolved
        self.ensure_refs_resolved(source);
        self.ensure_refs_resolved(target);

        // Helper to check if a symbol is a class (for nominal subtyping)
        let is_class_fn = |sym_ref: crate::solver::types::SymbolRef| -> bool {
            let sym_id = crate::binder::SymbolId(sym_ref.0);
            if let Some(sym) = self.ctx.binder.get_symbol(sym_id) {
                (sym.flags & symbol_flags::CLASS) != 0
            } else {
                false
            }
        };

        let mut checker = SubtypeChecker::with_resolver(self.ctx.types, env)
            .with_strict_null_checks(self.ctx.strict_null_checks())
            .with_inheritance_graph(&self.ctx.inheritance_graph)
            .with_class_check(&is_class_fn);
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
        // TypeScript allows var redeclarations when the new type is assignable to the
        // previous type (subtype relationship). Bidirectional check is too strict and
        // causes false positives with enum literals assigned to number types, etc.
        self.is_assignable_to(current_type, prev_type)
            || self.is_assignable_to(prev_type, current_type)
    }

    /// Check if source type is assignable to ANY member of a target union.
    pub fn is_assignable_to_union(&self, source: TypeId, targets: &[TypeId]) -> bool {
        use crate::solver::CompatChecker;
        let env = self.ctx.type_env.borrow();
        let mut checker = CompatChecker::with_resolver(self.ctx.types, &*env);
        self.ctx.configure_compat_checker(&mut checker);
        for &target in targets {
            if checker.is_assignable(source, target) {
                return true;
            }
        }
        false
    }
}
