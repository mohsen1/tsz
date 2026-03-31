//! Subtype checker helper methods.
//!
//! Contains intersection optimization, cache key construction,
//! public entry points, and special-case subtype checks
//! (Object contract, generic index access).

use crate::def::resolver::TypeResolver;
use crate::relations::subtype::{
    AnyPropagationMode, INTERSECTION_OBJECT_FAST_PATH_THRESHOLD, SubtypeChecker, SubtypeResult,
};
use crate::types::{ObjectFlags, ObjectShape, RelationCacheKey, TypeId, Visibility};
use crate::visitor::{
    callable_shape_id, function_shape_id, index_access_parts, literal_string, object_shape_id,
    object_with_index_shape_id, type_param_info, union_list_id,
};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    pub(crate) fn can_use_object_intersection_fast_path(&self, members: &[TypeId]) -> bool {
        if members.len() < INTERSECTION_OBJECT_FAST_PATH_THRESHOLD {
            return false;
        }

        for &member in members {
            let resolved = self.resolve_lazy_type(member);

            // Callable requirements must remain explicit intersection members.
            // Collapsing to a merged object target would drop call signatures.
            if callable_shape_id(self.interner, resolved).is_some()
                || function_shape_id(self.interner, resolved).is_some()
            {
                return false;
            }

            let Some(shape_id) = object_shape_id(self.interner, resolved)
                .or_else(|| object_with_index_shape_id(self.interner, resolved))
            else {
                return false;
            };

            let shape = self.interner.object_shape(shape_id);
            if !shape.flags.is_empty() {
                return false;
            }
            if shape
                .properties
                .iter()
                .any(|prop| prop.visibility != Visibility::Public)
            {
                return false;
            }
        }

        true
    }

    pub(crate) fn build_object_intersection_target(
        &self,
        target_intersection: TypeId,
    ) -> Option<TypeId> {
        // Check the shared QueryCache first to avoid expensive property collection
        // for large intersections checked across multiple SubtypeChecker instances.
        if let Some(db) = self.query_db
            && let Some(cached) = db.lookup_intersection_merge(target_intersection)
        {
            return cached;
        }

        use crate::objects::{PropertyCollectionResult, collect_properties};

        let result = match collect_properties(target_intersection, self.interner, self.resolver) {
            PropertyCollectionResult::Properties {
                properties,
                string_index,
                number_index,
            } => {
                let shape = ObjectShape {
                    flags: ObjectFlags::empty(),
                    properties,
                    string_index,
                    number_index,
                    symbol: None,
                };

                if shape.string_index.is_some() || shape.number_index.is_some() {
                    Some(self.interner.object_with_index(shape))
                } else {
                    Some(self.interner.object(shape.properties))
                }
            }
            PropertyCollectionResult::Any => Some(TypeId::ANY),
            PropertyCollectionResult::NonObject => None,
        };

        // Cache the result for subsequent SubtypeChecker instances.
        if let Some(db) = self.query_db {
            db.insert_intersection_merge(target_intersection, result);
        }
        result
    }

    /// Check if two object types have overlapping properties.
    ///
    /// Returns false if any common property has non-overlapping types.
    /// Construct a `RelationCacheKey` for the current checker configuration.
    ///
    /// This packs the Lawyer-layer flags into a compact cache key to ensure that
    /// results computed under different rules (strict vs non-strict) don't contaminate each other.
    pub(crate) const fn make_cache_key(&self, source: TypeId, target: TypeId) -> RelationCacheKey {
        let mut flags: u16 = 0;
        if self.strict_null_checks {
            flags |= RelationCacheKey::FLAG_STRICT_NULL_CHECKS;
        }
        if self.strict_function_types {
            flags |= RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES;
        }
        if self.exact_optional_property_types {
            flags |= RelationCacheKey::FLAG_EXACT_OPTIONAL_PROPERTY_TYPES;
        }
        if self.no_unchecked_indexed_access {
            flags |= RelationCacheKey::FLAG_NO_UNCHECKED_INDEXED_ACCESS;
        }
        if self.disable_method_bivariance {
            flags |= RelationCacheKey::FLAG_DISABLE_METHOD_BIVARIANCE;
        }
        if self.allow_void_return {
            flags |= RelationCacheKey::FLAG_ALLOW_VOID_RETURN;
        }
        if self.allow_bivariant_rest {
            flags |= RelationCacheKey::FLAG_ALLOW_BIVARIANT_REST;
        }
        if self.allow_bivariant_param_count {
            flags |= RelationCacheKey::FLAG_ALLOW_BIVARIANT_PARAM_COUNT;
        }

        // CRITICAL: Calculate effective `any_mode` based on depth.
        // If `any_propagation` is `TopLevelOnly` but `depth > 0`, the effective mode is "None".
        // This ensures that top-level checks don't incorrectly hit cached results from nested checks.
        let any_mode = match self.any_propagation {
            AnyPropagationMode::All => 0,
            AnyPropagationMode::TopLevelOnly if self.guard.depth() == 0 => 1,
            AnyPropagationMode::TopLevelOnly => 2, // Disabled at depth > 0
        };

        RelationCacheKey::subtype(source, target, flags, any_mode)
    }

    /// Check if `source` is a subtype of `target`.
    /// This is the main entry point for subtype checking.
    ///
    /// When a `QueryDatabase` is available (via `with_query_db`), fast-path checks
    /// (identity, any, unknown, never) are done locally, then the full structural
    /// check is delegated to the internal `check_subtype` which may use Salsa
    /// memoization for `evaluate_type` calls.
    pub fn is_subtype_of(&mut self, source: TypeId, target: TypeId) -> bool {
        self.check_subtype(source, target).is_true()
    }

    /// Check if `source` is assignable to `target`.
    /// This is a strict structural check; use `CompatChecker` for TypeScript assignability rules.
    pub fn is_assignable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        self.is_subtype_of(source, target)
    }

    /// Internal subtype check with cycle detection
    ///
    /// # Cycle Detection Strategy (Coinductive Semantics)
    ///
    /// This function implements coinductive cycle handling for recursive types.
    /// The key insight is that we must check for cycles BEFORE evaluation to handle
    /// "expansive" types like `type Deep<T> = { next: Deep<Box<T>> }` that produce
    /// fresh `TypeIds` on each evaluation.
    ///
    /// The algorithm:
    /// 1. Fast paths (identity, any, unknown, never)
    /// 2. **Cycle detection FIRST** (before evaluation!)
    /// 3. Meta-type evaluation (keyof, conditional, mapped, etc.)
    /// 4. Structural comparison
    ///
    /// Check if source satisfies the Object contract (conflicting properties check).
    ///
    /// The `Object` interface allows assignment from almost anything, but if the source
    /// provides properties that overlap with `Object` (e.g. `toString`), they must be compatible.
    pub(crate) fn check_object_contract(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> SubtypeResult {
        use crate::visitor::{object_shape_id, object_with_index_shape_id};

        // Type parameters must NOT short-circuit here: an unconstrained T could be
        // instantiated with null/undefined/void, which are NOT assignable to Object.
        // For constrained T, check if the constraint is assignable to Object.
        let source_eval = self.evaluate_type(source);
        if let Some(info) = crate::visitor::type_param_info(self.interner, source_eval) {
            return match info.constraint {
                Some(constraint) => self.check_object_contract(constraint, target),
                None => SubtypeResult::False,
            };
        }

        // Resolve source shape first - if not an object, it's valid (primitives match Object)
        let s_shape_id = match object_shape_id(self.interner, source_eval)
            .or_else(|| object_with_index_shape_id(self.interner, source_eval))
        {
            Some(id) => id,
            None => return SubtypeResult::True,
        };
        let s_shape = self.interner.object_shape(s_shape_id);

        // Resolve Object shape (target)
        let target_eval = self.evaluate_type(target);
        let t_shape_id = match object_shape_id(self.interner, target_eval)
            .or_else(|| object_with_index_shape_id(self.interner, target_eval))
        {
            Some(id) => id,
            None => return SubtypeResult::True, // Should not happen for Object interface
        };
        let t_shape = self.interner.object_shape(t_shape_id);

        // Check for conflicting properties
        for s_prop in &s_shape.properties {
            // Find property in Object interface (target)
            if let Some(t_prop) =
                self.lookup_property(&t_shape.properties, Some(t_shape_id), s_prop.name)
            {
                // Found potential conflict: check compatibility
                let result = self.check_property_compatibility(s_prop, t_prop, None, None);
                if !result.is_true() {
                    return result;
                }
            }
        }

        SubtypeResult::True
    }

    /// Check if source is a subtype of an `IndexAccess` target where the index is generic.
    ///
    /// If `Target` is `Obj[K]` where `K` is generic, we check if `Source <: Obj[C]`
    /// where `C` is the constraint of `K`.
    /// Specifically, if `C` is a union of string literals `"a" | "b"`, we verify
    /// `Source <: Obj["a"]` AND `Source <: Obj["b"]`.
    pub(crate) fn check_generic_index_access_subtype(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some((t_obj, t_idx)) = index_access_parts(self.interner, target) else {
            return false;
        };

        // Special case: if source is also an index access with the same object type
        // but a different type parameter key, they are not subtypes even if they have
        // the same constraint. This prevents `T1[K] <: T2[K]` when T1 != T2.
        if let Some((s_obj, s_idx)) = index_access_parts(self.interner, source)
            && s_obj == t_obj
            && let Some(s_param) = type_param_info(self.interner, s_idx)
            && let Some(t_param) = type_param_info(self.interner, t_idx)
        {
            // Both keys are type parameters with different names - not subtypes
            if s_param.name != t_param.name {
                return false;
            }
        }

        // Check if index is a generic type parameter
        let Some(t_param) = type_param_info(self.interner, t_idx) else {
            return false;
        };

        let Some(constraint) = t_param.constraint else {
            return false;
        };

        // Evaluate the constraint to resolve any type aliases/applications
        let constraint = self.evaluate_type(constraint);

        // Collect all literal types from the constraint (if it's a union of literals)
        // If constraint is a single literal, treat as union of 1.
        let mut literals = Vec::new();

        if let Some(s) = literal_string(self.interner, constraint) {
            literals.push(self.interner.literal_string_atom(s));
        } else if let Some(union_id) = union_list_id(self.interner, constraint) {
            let members = self.interner.type_list(union_id);
            for &m in members.iter() {
                if let Some(s) = literal_string(self.interner, m) {
                    literals.push(self.interner.literal_string_atom(s));
                } else {
                    // Constraint contains non-string-literal (e.g. number, or generic).
                    // Can't distribute.
                    return false;
                }
            }
        } else {
            // Constraint is not a literal or union of literals.
            return false;
        }

        if literals.is_empty() {
            return false;
        }

        // Check source <: Obj[L] for all L in literals
        for lit_type in literals {
            // Create IndexAccess(Obj, L)
            // We use evaluate_type here to potentially resolve it to a concrete property type
            // (e.g. Obj["a"] -> string)
            let indexed_access = self.interner.index_access(t_obj, lit_type);
            let evaluated = self.evaluate_type(indexed_access);

            if !self.check_subtype(source, evaluated).is_true() {
                return false;
            }
        }

        true
    }

    pub(crate) fn check_index_access_source_upper_bound_subtype(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some((object_type, key_type)) = index_access_parts(self.interner, source) else {
            return false;
        };

        // Special case: if target is also an index access with the same object type
        // but a different type parameter key, they are not subtypes even if they have
        // the same constraint. This prevents `T1[K] <: T2[K]` when T1 != T2.
        if let Some((t_obj, t_idx)) = index_access_parts(self.interner, target)
            && object_type == t_obj
            && let Some(s_param) = type_param_info(self.interner, key_type)
            && let Some(t_param) = type_param_info(self.interner, t_idx)
        {
            // Both keys are type parameters with different names - not subtypes
            if s_param.name != t_param.name {
                return false;
            }
        }

        let original = self.interner.index_access(object_type, key_type);
        let mut candidates = Vec::new();
        self.collect_index_access_upper_bound_candidates(
            object_type,
            key_type,
            original,
            &mut candidates,
        );

        candidates.into_iter().any(|candidate| {
            candidate != original
                && candidate != TypeId::ERROR
                && self.check_subtype(candidate, target).is_true()
        })
    }

    fn collect_index_access_upper_bound_candidates(
        &mut self,
        object_type: TypeId,
        key_type: TypeId,
        original: TypeId,
        candidates: &mut Vec<TypeId>,
    ) {
        let evaluated = self.evaluate_type(self.interner.index_access(object_type, key_type));
        if evaluated != original && !candidates.contains(&evaluated) {
            candidates.push(evaluated);
        }

        if let Some(info) = type_param_info(self.interner, object_type) {
            if let Some(constraint) = info.constraint
                && !crate::visitor::is_type_parameter(self.interner, constraint)
                && !crate::visitor::is_this_type(self.interner, constraint)
            {
                self.collect_index_access_upper_bound_candidates(
                    constraint, key_type, original, candidates,
                );
            } else if info.constraint.is_none() {
                // Unconstrained type parameters have implicit constraint `unknown`.
                // T[K] for unconstrained T has upper bound `unknown` because T
                // could be any type and its properties could have any value.
                if !candidates.contains(&TypeId::UNKNOWN) {
                    candidates.push(TypeId::UNKNOWN);
                }
            }
        }

        if let Some(info) = type_param_info(self.interner, key_type)
            && let Some(constraint) = info.constraint
        {
            let constrained =
                self.evaluate_type(self.interner.index_access(object_type, constraint));
            if constrained != original && !candidates.contains(&constrained) {
                candidates.push(constrained);
            }
        }

        if let Some(intersection_id) =
            crate::visitor::intersection_list_id(self.interner, object_type)
        {
            let members = self.interner.type_list(intersection_id);
            for &member in members.iter() {
                self.collect_index_access_upper_bound_candidates(
                    member, key_type, original, candidates,
                );
            }
        }
    }
}
