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

use crate::query_boundaries::assignability::{
    AssignabilityEvalKind, ExcessPropertiesKind, analyze_assignability_failure_with_context,
    are_types_overlapping_with_env, classify_for_assignability_eval,
    classify_for_excess_properties, is_assignable_bivariant_with_resolver,
    is_assignable_with_overrides, is_assignable_with_resolver, is_callable_type,
    is_redeclaration_identical_with_resolver, is_subtype_with_resolver, object_shape_for_type,
};
use crate::state::{CheckerOverrideProvider, CheckerState};
use rustc_hash::FxHashSet;
use tracing::trace;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::RelationCacheKey;
use tsz_solver::TypeId;
use tsz_solver::visitor::{collect_lazy_def_ids, collect_type_queries};

// =============================================================================
// Assignability Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    /// Centralized suppression for TS2322-style assignability diagnostics.
    pub(crate) fn should_suppress_assignability_diagnostic(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        matches!(source, TypeId::ERROR | TypeId::ANY | TypeId::UNKNOWN)
            || matches!(target, TypeId::ERROR | TypeId::ANY | TypeId::UNKNOWN)
    }

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
        let mut visited_types = FxHashSet::default();
        let mut visited_def_ids = FxHashSet::default();
        let mut worklist = vec![type_id];

        while let Some(current) = worklist.pop() {
            if !visited_types.insert(current) {
                continue;
            }

            for symbol_ref in collect_type_queries(self.ctx.types, current) {
                let sym_id = tsz_binder::SymbolId(symbol_ref.0);
                let _ = self.get_type_of_symbol(sym_id);
            }

            for def_id in collect_lazy_def_ids(self.ctx.types, current) {
                if !visited_def_ids.insert(def_id) {
                    continue;
                }
                if let Some(result) = self.resolve_and_insert_def_type(def_id)
                    && result != TypeId::ERROR
                    && result != TypeId::ANY
                {
                    worklist.push(result);
                }
            }
        }
    }

    /// Evaluate a type for assignability checking.
    ///
    /// Determines if the type needs evaluation (applications, env-dependent types)
    /// and performs the appropriate evaluation.
    pub(crate) fn evaluate_type_for_assignability(&mut self, type_id: TypeId) -> TypeId {
        match classify_for_assignability_eval(self.ctx.types, type_id) {
            AssignabilityEvalKind::Application => self.evaluate_type_with_resolution(type_id),
            AssignabilityEvalKind::NeedsEnvEval => self.evaluate_type_with_env(type_id),
            AssignabilityEvalKind::Resolved => type_id,
        }
    }

    /// Wrapper around solver `contains_infer_types` for assignability cache policy.
    pub(crate) fn contains_infer_types_cached(&mut self, type_id: TypeId) -> bool {
        use tsz_solver::visitor::contains_infer_types;
        contains_infer_types(self.ctx.types, type_id)
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
        // CRITICAL: Ensure all Ref types are resolved before assignability check.
        // This fixes intersection type assignability where `type AB = A & B` needs
        // A and B in type_env before we can check if a type is assignable to the intersection.
        self.ensure_refs_resolved(source);
        self.ensure_refs_resolved(target);

        self.ensure_application_symbols_resolved(source);
        self.ensure_application_symbols_resolved(target);

        // Pre-check: Function interface accepts any callable type.
        // Must check before evaluate_type_for_assignability resolves Lazy(DefId)
        // to ObjectShape, losing the DefId identity needed to recognize it as Function.
        {
            use tsz_solver::visitor::lazy_def_id;
            let is_function_target = lazy_def_id(self.ctx.types, target).is_some_and(|t_def| {
                self.ctx.type_env.try_borrow().ok().is_some_and(|env| {
                    env.is_boxed_def_id(t_def, tsz_solver::IntrinsicKind::Function)
                })
            });
            if is_function_target {
                let source_eval = self.evaluate_type_for_assignability(source);
                if is_callable_type(self.ctx.types, source_eval) {
                    return true;
                }
            }
        }

        // Save original types for cache key before evaluation
        let original_source = source;
        let original_target = target;

        let source = self.evaluate_type_for_assignability(source);
        let target = self.evaluate_type_for_assignability(target);

        // Check relation cache for non-inference types
        // Construct RelationCacheKey with Lawyer-layer flags to prevent cache poisoning
        // Note: Use ORIGINAL types for cache key, not evaluated types
        let is_cacheable =
            !self.contains_infer_types_cached(source) && !self.contains_infer_types_cached(target);

        let flags = self.ctx.pack_relation_flags();

        if is_cacheable {
            let cache_key =
                RelationCacheKey::assignability(original_source, original_target, flags, 0);

            if let Some(cached) = self.ctx.types.lookup_assignability_cache(cache_key) {
                return cached;
            }
        }

        // Use CheckerContext as the resolver instead of TypeEnvironment
        // This enables access to symbol information for enum type detection
        let overrides = CheckerOverrideProvider::new(self, None);
        let result = is_assignable_with_overrides(
            self.ctx.types,
            &self.ctx,
            source,
            target,
            flags,
            &self.ctx.inheritance_graph,
            self.ctx.sound_mode(),
            &overrides,
        );

        if is_cacheable {
            let cache_key =
                RelationCacheKey::assignability(original_source, original_target, flags, 0);

            self.ctx.types.insert_assignability_cache(cache_key, result);
        }

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
        env: &tsz_solver::TypeEnvironment,
    ) -> bool {
        let flags = self.ctx.pack_relation_flags();
        let overrides = CheckerOverrideProvider::new(self, Some(env));
        is_assignable_with_overrides(
            self.ctx.types,
            env,
            source,
            target,
            flags,
            &self.ctx.inheritance_graph,
            self.ctx.sound_mode(),
            &overrides,
        )
    }

    /// Check if `source` type is assignable to `target` type with bivariant function parameter checking.
    ///
    /// This is used for class method override checking, where methods are always bivariant
    /// (unlike function properties which are contravariant with strictFunctionTypes).
    ///
    /// Follows the same pattern as `is_assignable_to` but calls `is_assignable_to_bivariant_callback`
    /// which disables strict_function_types for the check.
    pub fn is_assignable_to_bivariant(&mut self, source: TypeId, target: TypeId) -> bool {
        // CRITICAL: Ensure all Ref types are resolved before assignability check.
        // This fixes intersection type assignability where `type AB = A & B` needs
        // A and B in type_env before we can check if a type is assignable to the intersection.
        self.ensure_refs_resolved(source);
        self.ensure_refs_resolved(target);

        self.ensure_application_symbols_resolved(source);
        self.ensure_application_symbols_resolved(target);

        // Save original types for cache key before evaluation
        let original_source = source;
        let original_target = target;

        let source = self.evaluate_type_for_assignability(source);
        let target = self.evaluate_type_for_assignability(target);

        // Check relation cache for non-inference types
        // Construct RelationCacheKey with Lawyer-layer flags to prevent cache poisoning
        // Note: Use ORIGINAL types for cache key, not evaluated types
        let is_cacheable =
            !self.contains_infer_types_cached(source) && !self.contains_infer_types_cached(target);

        // For bivariant checks, we strip the strict_function_types flag
        // so the cache key is distinct from regular assignability checks.
        let flags = self.ctx.pack_relation_flags() & !RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES;

        if is_cacheable {
            // Note: For assignability checks, we use AnyPropagationMode::All (0)
            // since the checker doesn't track depth like SubtypeChecker does
            let cache_key =
                RelationCacheKey::assignability(original_source, original_target, flags, 0);

            if let Some(cached) = self.ctx.types.lookup_assignability_cache(cache_key) {
                return cached;
            }
        }

        let env = self.ctx.type_env.borrow();
        // Preserve existing behavior: bivariant path does not use checker overrides.
        let result = is_assignable_bivariant_with_resolver(
            self.ctx.types,
            &*env,
            source,
            target,
            flags,
            &self.ctx.inheritance_graph,
            self.ctx.sound_mode(),
        );

        // Cache the result for non-inference types
        // Use ORIGINAL types for cache key (not evaluated types)
        if is_cacheable {
            let cache_key =
                RelationCacheKey::assignability(original_source, original_target, flags, 0);

            self.ctx.types.insert_assignability_cache(cache_key, result);
        }

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
        are_types_overlapping_with_env(
            self.ctx.types,
            &*env,
            left,
            right,
            self.ctx.strict_null_checks(),
        )
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
        let Some(node) = self.ctx.arena.get(source_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }

        // Check for weak union violation first (using scoped borrow)
        if self.is_weak_union_violation(source, target) {
            return true;
        }

        // Check if there are excess properties.
        if !self.object_literal_has_excess_properties(source, target, source_idx) {
            return false;
        }

        // There are excess properties. Check if all matching properties have compatible types.
        let Some(source_shape) = object_shape_for_type(self.ctx.types, source) else {
            return true;
        };

        let resolved_target = self.resolve_type_for_property_access(target);
        let Some(target_shape) = object_shape_for_type(self.ctx.types, resolved_target) else {
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

                let is_assignable =
                    { self.is_assignable_to(source_prop_type, effective_target_type) };

                if !is_assignable {
                    return false;
                }
            }
        }

        true
    }

    /// Check assignability and emit the standard TS2322/TS2345-style diagnostic when needed.
    ///
    /// Returns true when no diagnostic was emitted (assignable or intentionally skipped),
    /// false when an assignability diagnostic was emitted.
    pub(crate) fn check_assignable_or_report(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> bool {
        self.check_assignable_or_report_at(source, target, source_idx, source_idx)
    }

    /// Check assignability and emit TS2322/TS2345-style diagnostics with independent
    /// source and diagnostic anchors.
    ///
    /// `source_idx` is used for weak-union/excess-property prioritization.
    /// `diag_idx` is where the assignability diagnostic is anchored.
    pub(crate) fn check_assignable_or_report_at(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
    ) -> bool {
        if self.should_suppress_assignability_diagnostic(source, target) {
            return true;
        }
        if self.is_assignable_to(source, target)
            || self.should_skip_weak_union_error(source, target, source_idx)
        {
            return true;
        }
        self.error_type_not_assignable_with_reason_at(source, target, diag_idx);
        false
    }

    /// Check assignability and emit a generic TS2322 diagnostic at `diag_idx`.
    ///
    /// This is used for call sites that intentionally avoid detailed reason rendering
    /// but still share centralized mismatch/suppression behavior.
    pub(crate) fn check_assignable_or_report_generic_at(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
    ) -> bool {
        if self.should_suppress_assignability_diagnostic(source, target) {
            return true;
        }
        if self.is_assignable_to(source, target)
            || self.should_skip_weak_union_error(source, target, source_idx)
        {
            return true;
        }
        self.error_type_not_assignable_generic_at(source, target, diag_idx);
        false
    }

    /// Check assignability and emit argument-not-assignable diagnostics (TS2345-style).
    ///
    /// Returns true when no diagnostic was emitted (assignable or intentionally skipped),
    /// false when an argument-assignability diagnostic was emitted.
    pub(crate) fn check_argument_assignable_or_report(
        &mut self,
        source: TypeId,
        target: TypeId,
        arg_idx: NodeIndex,
    ) -> bool {
        if self.should_suppress_assignability_diagnostic(source, target) {
            return true;
        }
        if self.is_assignable_to(source, target)
            || self.should_skip_weak_union_error(source, target, arg_idx)
        {
            return true;
        }
        self.error_argument_not_assignable_at(source, target, arg_idx);
        false
    }

    /// Returns true when an assignability mismatch should produce a diagnostic.
    ///
    /// This centralizes the standard "not assignable + not weak-union/excess-property
    /// suppression" decision so call sites emitting different diagnostics can share it.
    pub(crate) fn should_report_assignability_mismatch(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> bool {
        if self.should_suppress_assignability_diagnostic(source, target) {
            return false;
        }
        !self.is_assignable_to(source, target)
            && !self.should_skip_weak_union_error(source, target, source_idx)
    }

    /// Returns true when a bivariant-assignability mismatch should produce a diagnostic.
    ///
    /// Mirrors `should_report_assignability_mismatch` but uses the bivariant relation
    /// entrypoint for method-compatibility scenarios.
    pub(crate) fn should_report_assignability_mismatch_bivariant(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> bool {
        if self.should_suppress_assignability_diagnostic(source, target) {
            return false;
        }
        !self.is_assignable_to_bivariant(source, target)
            && !self.should_skip_weak_union_error(source, target, source_idx)
    }

    /// Check bidirectional assignability.
    ///
    /// Useful in checker locations that need type comparability/equivalence-like checks.
    pub(crate) fn are_mutually_assignable(&mut self, left: TypeId, right: TypeId) -> bool {
        self.is_assignable_to(left, right) && self.is_assignable_to(right, left)
    }

    /// Check if two types are comparable (overlap).
    ///
    /// Corresponds to TypeScript's `isTypeComparableTo`: returns true if source is
    /// assignable to target OR target is assignable to source. This is the correct
    /// check for switch/case comparability (TS2678), equality narrowing, etc.
    pub(crate) fn is_type_comparable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        self.is_assignable_to(source, target) || self.is_assignable_to(target, source)
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
        use tsz_solver::freshness;
        // Only fresh object literals trigger excess property checking.
        if !freshness::is_fresh_object_type(self.ctx.types, source) {
            return false;
        }

        let Some(source_shape) = object_shape_for_type(self.ctx.types, source) else {
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
                    let Some(shape) = object_shape_for_type(self.ctx.types, resolved_member) else {
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
            ExcessPropertiesKind::Intersection(members) => {
                let mut target_shapes = Vec::new();

                for member in members {
                    let resolved_member = self.resolve_type_for_property_access(member);
                    let Some(shape) = object_shape_for_type(self.ctx.types, resolved_member) else {
                        continue;
                    };

                    if shape.string_index.is_some() || shape.number_index.is_some() {
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

    pub(crate) fn analyze_assignability_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::AssignabilityFailureAnalysis {
        let env = self.ctx.type_env.borrow();
        analyze_assignability_failure_with_context(self.ctx.types, &self.ctx, &*env, source, target)
    }

    pub(crate) fn is_weak_union_violation(&mut self, source: TypeId, target: TypeId) -> bool {
        self.analyze_assignability_failure(source, target)
            .weak_union_violation
    }

    // =========================================================================
    // Subtype Checking
    // =========================================================================

    /// Check if `source` type is a subtype of `target` type.
    ///
    /// This is the main entry point for subtype checking, used for type compatibility
    /// throughout the type system. Subtyping is stricter than assignability.
    pub fn is_subtype_of(&mut self, source: TypeId, target: TypeId) -> bool {
        use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_binder::symbol_flags;

        // Fast path: identity check
        if source == target {
            return true;
        }

        // Keep subtype preconditions aligned with assignability to avoid
        // caching relation answers before lazy/application refs are prepared.
        self.ensure_refs_resolved(source);
        self.ensure_refs_resolved(target);
        self.ensure_application_symbols_resolved(source);
        self.ensure_application_symbols_resolved(target);

        // Check relation cache for non-inference types
        // Construct RelationCacheKey with Lawyer-layer flags to prevent cache poisoning
        let is_cacheable =
            !self.contains_infer_types_cached(source) && !self.contains_infer_types_cached(target);
        let flags = self.ctx.pack_relation_flags();

        if is_cacheable {
            // Note: For subtype checks in the checker, we use AnyPropagationMode::All (0)
            // since the checker doesn't track depth like SubtypeChecker does
            let cache_key = RelationCacheKey::subtype(source, target, flags, 0);

            if let Some(cached) = self.ctx.types.lookup_subtype_cache(cache_key) {
                return cached;
            }
        }

        let env = self.ctx.type_env.borrow();
        let binder = self.ctx.binder;

        // Helper to check if a symbol is a class (for nominal subtyping)
        let is_class_fn = |sym_ref: tsz_solver::SymbolRef| -> bool {
            let sym_id = tsz_binder::SymbolId(sym_ref.0);
            if let Some(sym) = binder.get_symbol(sym_id) {
                (sym.flags & symbol_flags::CLASS) != 0
            } else {
                false
            }
        };
        let relation_result = is_subtype_with_resolver(
            self.ctx.types,
            &*env,
            source,
            target,
            flags,
            &self.ctx.inheritance_graph,
            Some(&is_class_fn),
        );

        if relation_result.depth_exceeded {
            self.error_at_current_node(
                diagnostic_messages::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
                diagnostic_codes::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
            );
        }

        let result = relation_result.is_related();

        // Cache the result for non-inference types
        if is_cacheable {
            let cache_key = RelationCacheKey::subtype(source, target, flags, 0);

            self.ctx.types.insert_subtype_cache(cache_key, result);
        }

        result
    }

    /// Check if source type is a subtype of target type with explicit environment.
    pub fn is_subtype_of_with_env(
        &mut self,
        source: TypeId,
        target: TypeId,
        env: &tsz_solver::TypeEnvironment,
    ) -> bool {
        use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_binder::symbol_flags;

        // CRITICAL: Before checking subtypes, ensure all Ref types are resolved
        self.ensure_refs_resolved(source);
        self.ensure_refs_resolved(target);
        self.ensure_application_symbols_resolved(source);
        self.ensure_application_symbols_resolved(target);

        // Helper to check if a symbol is a class (for nominal subtyping)
        let is_class_fn = |sym_ref: tsz_solver::SymbolRef| -> bool {
            let sym_id = tsz_binder::SymbolId(sym_ref.0);
            if let Some(sym) = self.ctx.binder.get_symbol(sym_id) {
                (sym.flags & symbol_flags::CLASS) != 0
            } else {
                false
            }
        };

        let result = is_subtype_with_resolver(
            self.ctx.types,
            env,
            source,
            target,
            self.ctx.pack_relation_flags(),
            &self.ctx.inheritance_graph,
            Some(&is_class_fn),
        );

        if result.depth_exceeded {
            self.error_at_current_node(
                diagnostic_messages::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
                diagnostic_codes::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
            );
        }

        result.is_related()
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

        let flags = self.ctx.pack_relation_flags();
        // Delegate to the Solver's Lawyer layer for redeclaration identity checking
        let env = self.ctx.type_env.borrow();
        is_redeclaration_identical_with_resolver(
            self.ctx.types,
            &*env,
            prev_type,
            current_type,
            flags,
            &self.ctx.inheritance_graph,
            self.ctx.sound_mode(),
        )
    }

    /// Check if source type is assignable to ANY member of a target union.
    pub fn is_assignable_to_union(&self, source: TypeId, targets: &[TypeId]) -> bool {
        let flags = self.ctx.pack_relation_flags();
        let env = self.ctx.type_env.borrow();

        for &target in targets {
            if is_assignable_with_resolver(
                self.ctx.types,
                &*env,
                source,
                target,
                flags,
                &self.ctx.inheritance_graph,
                self.ctx.sound_mode(),
            ) {
                return true;
            }
        }
        false
    }
}
