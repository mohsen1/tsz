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
use crate::solver::operations::AssignabilityChecker; // For is_assignable_to_bivariant_callback
use crate::solver::types::RelationCacheKey;
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
    pub(crate) fn ensure_refs_resolved(&mut self, type_id: TypeId) {
        let mut visited = rustc_hash::FxHashSet::default();
        self.ensure_refs_resolved_inner(type_id, &mut visited);
    }

    fn ensure_refs_resolved_inner(
        &mut self,
        type_id: TypeId,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) {
        // Cycle detection: skip types already visited to prevent infinite
        // recursion on self-referencing types (e.g., LinkedList<T>).
        if !visited.insert(type_id) {
            return;
        }

        use crate::solver::type_queries::{TypeTraversalKind, classify_for_traversal};

        // Classify the type to determine how to traverse it
        let traversal_kind = classify_for_traversal(self.ctx.types, type_id);

        match traversal_kind {
            // 1. Handle the specific "WHERE" logic (Lazy resolution)
            TypeTraversalKind::Lazy(def_id) => {
                if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) {
                    let result = self.get_type_of_symbol(sym_id);
                    // Explicitly insert the DefId→TypeId mapping into type_env.
                    // get_type_of_symbol may return a cached result, skipping the
                    // insert_def code path. We must ensure the mapping exists so
                    // the SubtypeChecker's TypeEnvironment resolver can resolve
                    // Lazy(DefId) types during assignability checks.
                    if result != TypeId::ERROR && result != TypeId::ANY {
                        if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
                            env.insert_def(def_id, result);
                        }
                        // Recurse into the resolved type to ensure nested Lazy types
                        // are also resolved.
                        self.ensure_refs_resolved_inner(result, visited);
                    }
                }
                return; // Lazy is a leaf in terms of children, the resolved type is handled above
            }

            // 2. Handle TypeQuery (value-space references)
            TypeTraversalKind::TypeQuery(symbol_ref) => {
                let sym_id = crate::binder::SymbolId(symbol_ref.0);
                let _ = self.get_type_of_symbol(sym_id);
                return;
            }

            // 3. Handle structured types - delegate the "WHAT" (traversal) to the Solver
            TypeTraversalKind::Application { base, args, .. } => {
                // Recurse into base type and arguments
                self.ensure_refs_resolved_inner(base, visited);
                for arg in args {
                    self.ensure_refs_resolved_inner(arg, visited);
                }
            }
            TypeTraversalKind::Members(members) => {
                for member in members {
                    self.ensure_refs_resolved_inner(member, visited);
                }
            }
            TypeTraversalKind::Function(shape_id) => {
                let shape = self.ctx.types.function_shape(shape_id);
                for param in &shape.params {
                    self.ensure_refs_resolved_inner(param.type_id, visited);
                }
                self.ensure_refs_resolved_inner(shape.return_type, visited);
            }
            TypeTraversalKind::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                // Handle call signatures
                for sig in &shape.call_signatures {
                    for param in &sig.params {
                        self.ensure_refs_resolved_inner(param.type_id, visited);
                    }
                    if let Some(this_type) = sig.this_type {
                        self.ensure_refs_resolved_inner(this_type, visited);
                    }
                    self.ensure_refs_resolved_inner(sig.return_type, visited);
                }
                // Handle construct signatures
                for sig in &shape.construct_signatures {
                    for param in &sig.params {
                        self.ensure_refs_resolved_inner(param.type_id, visited);
                    }
                    if let Some(this_type) = sig.this_type {
                        self.ensure_refs_resolved_inner(this_type, visited);
                    }
                    self.ensure_refs_resolved_inner(sig.return_type, visited);
                }
                // Handle properties
                for prop in &shape.properties {
                    self.ensure_refs_resolved_inner(prop.type_id, visited);
                }
            }
            TypeTraversalKind::Object(shape_id) => {
                let shape = self.ctx.types.object_shape(shape_id);
                for prop in &shape.properties {
                    self.ensure_refs_resolved_inner(prop.type_id, visited);
                }
            }
            TypeTraversalKind::Array(elem) => {
                self.ensure_refs_resolved_inner(elem, visited);
            }
            TypeTraversalKind::Tuple(list_id) => {
                let list = self.ctx.types.tuple_list(list_id);
                for elem in list.iter() {
                    self.ensure_refs_resolved_inner(elem.type_id, visited);
                }
            }
            TypeTraversalKind::Conditional(cond_id) => {
                let cond = self.ctx.types.conditional_type(cond_id);
                self.ensure_refs_resolved_inner(cond.check_type, visited);
                self.ensure_refs_resolved_inner(cond.extends_type, visited);
                self.ensure_refs_resolved_inner(cond.true_type, visited);
                self.ensure_refs_resolved_inner(cond.false_type, visited);
            }
            TypeTraversalKind::Mapped(mapped_id) => {
                let mapped = self.ctx.types.mapped_type(mapped_id);
                self.ensure_refs_resolved_inner(mapped.constraint, visited);
                self.ensure_refs_resolved_inner(mapped.template, visited);
                if let Some(name_type) = mapped.name_type {
                    self.ensure_refs_resolved_inner(name_type, visited);
                }
            }
            TypeTraversalKind::TypeParameter {
                constraint,
                default,
            } => {
                if let Some(c) = constraint {
                    self.ensure_refs_resolved_inner(c, visited);
                }
                if let Some(d) = default {
                    self.ensure_refs_resolved_inner(d, visited);
                }
            }
            TypeTraversalKind::Readonly(inner) => {
                self.ensure_refs_resolved_inner(inner, visited);
            }
            TypeTraversalKind::TemplateLiteral(types) => {
                for t in types {
                    self.ensure_refs_resolved_inner(t, visited);
                }
            }
            TypeTraversalKind::StringIntrinsic(inner) => {
                self.ensure_refs_resolved_inner(inner, visited);
            }
            TypeTraversalKind::IndexAccess { object, index } => {
                self.ensure_refs_resolved_inner(object, visited);
                self.ensure_refs_resolved_inner(index, visited);
            }
            TypeTraversalKind::KeyOf(inner) => {
                self.ensure_refs_resolved_inner(inner, visited);
            }
            TypeTraversalKind::SymbolRef(symbol_ref) => {
                let sym_id = crate::binder::SymbolId(symbol_ref.0);
                let _ = self.get_type_of_symbol(sym_id);
            }
            TypeTraversalKind::Terminal => {
                // No further traversal needed
            }
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

        // Use CheckerContext as the resolver instead of TypeEnvironment
        // This enables access to symbol information for enum type detection
        let overrides = CheckerOverrideProvider::new(self, None);
        let mut checker = CompatChecker::with_resolver(self.ctx.types, &self.ctx);
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

    /// Check if `source` type is assignable to `target` type with bivariant function parameter checking.
    ///
    /// This is used for class method override checking, where methods are always bivariant
    /// (unlike function properties which are contravariant with strictFunctionTypes).
    ///
    /// Follows the same pattern as `is_assignable_to` but calls `is_assignable_to_bivariant_callback`
    /// which disables strict_function_types for the check.
    pub fn is_assignable_to_bivariant(&mut self, source: TypeId, target: TypeId) -> bool {
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
        let mut checker = CompatChecker::with_resolver(self.ctx.types, &*env);
        self.ctx.configure_compat_checker(&mut checker);

        // Use bivariant callback which disables strict_function_types
        let result = checker.is_assignable_to_bivariant_callback(source, target);
        trace!(
            source = source.0,
            target = target.0,
            result,
            "is_assignable_to_bivariant"
        );
        result
    }

    /// Check if two types have any overlap (can ever be equal).
    ///
    /// Used for TS2367: "This condition will always return 'false'/'true' since
    /// the types 'X' and 'Y' have no overlap."
    ///
    /// Returns true if the types can potentially be equal, false if they can never
    /// have any common value.
    pub fn are_types_overlapping(&mut self, left: TypeId, right: TypeId) -> bool {
        // CRITICAL: Ensure all Ref types are resolved before overlap check.
        self.ensure_refs_resolved(left);
        self.ensure_refs_resolved(right);

        let env = self.ctx.type_env.borrow();
        let mut checker = crate::solver::SubtypeChecker::with_resolver(self.ctx.types, &*env);
        checker.strict_null_checks = self.ctx.strict_null_checks();

        checker.are_types_overlapping(left, right)
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
    /// Uses TypeId-based freshness tracking (fresh object literals only).
    pub(crate) fn object_literal_has_excess_properties(
        &mut self,
        source: TypeId,
        target: TypeId,
        _source_idx: NodeIndex,
    ) -> bool {
        use crate::solver::freshness;
        use crate::solver::type_queries::{ExcessPropertiesKind, classify_for_excess_properties};

        // Only fresh object literals trigger excess property checking.
        if !freshness::is_fresh_object_type(self.ctx.types, source) {
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
        // Construct RelationCacheKey with Lawyer-layer flags to prevent cache poisoning
        let is_cacheable = !contains_infer_types(self.ctx.types, source)
            && !contains_infer_types(self.ctx.types, target);

        if is_cacheable {
            // Pack boolean flags into a u16 bitmask for the cache key:
            // bit 0: strict_null_checks
            // bit 1: strict_function_types
            // bit 2: exact_optional_property_types
            // bit 3: no_unchecked_indexed_access
            // bit 4: disable_method_bivariance
            // bit 5: allow_void_return
            // bit 6: allow_bivariant_rest
            // bit 7: allow_bivariant_param_count
            let mut flags: u16 = 0;
            if self.ctx.strict_null_checks() {
                flags |= 1 << 0;
            }
            if self.ctx.strict_function_types() {
                flags |= 1 << 1;
            }
            if self.ctx.exact_optional_property_types() {
                flags |= 1 << 2;
            }
            if self.ctx.no_unchecked_indexed_access() {
                flags |= 1 << 3;
            }
            // Note: For subtype checks in the checker, we use AnyPropagationMode::All (0)
            // since the checker doesn't track depth like SubtypeChecker does
            let cache_key = RelationCacheKey::subtype(source, target, flags, 0);

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
            // Reconstruct the cache key with the same flags as the lookup
            let mut flags: u16 = 0;
            if self.ctx.strict_null_checks() {
                flags |= 1 << 0;
            }
            if self.ctx.strict_function_types() {
                flags |= 1 << 1;
            }
            if self.ctx.exact_optional_property_types() {
                flags |= 1 << 2;
            }
            if self.ctx.no_unchecked_indexed_access() {
                flags |= 1 << 3;
            }
            let cache_key = RelationCacheKey::subtype(source, target, flags, 0);

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
    ///
    /// Delegates to the Solver's CompatChecker to determine if two types are
    /// compatible for redeclaration (TS2403). This moves enum comparison logic
    /// from Checker to Solver per Phase 5 Anti-Pattern 8.1 removal.
    pub(crate) fn are_var_decl_types_compatible(
        &mut self,
        prev_type: TypeId,
        current_type: TypeId,
    ) -> bool {
        // Ensure Ref/Lazy types are resolved before checking compatibility
        self.ensure_refs_resolved(prev_type);
        self.ensure_refs_resolved(current_type);
        self.ensure_application_symbols_resolved(prev_type);
        self.ensure_application_symbols_resolved(current_type);

        // Delegate to the Solver's Lawyer layer for type identity checking
        let env = self.ctx.type_env.borrow();
        let mut checker = crate::solver::CompatChecker::with_resolver(self.ctx.types, &*env);
        self.ctx.configure_compat_checker(&mut checker);

        checker.are_types_identical_for_redeclaration(prev_type, current_type)
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
