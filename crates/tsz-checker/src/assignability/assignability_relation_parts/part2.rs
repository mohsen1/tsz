impl<'a> CheckerState<'a> {
    /// Check assignability with the current `TypeEnvironment` but without
    /// consulting the checker's relation caches.
    ///
    /// Generic call/new inference uses this after instantiation to avoid stale
    /// relation answers while still going through the same input preparation as
    /// the normal assignability gateway.
    pub fn is_assignable_to_with_env(&mut self, source: TypeId, target: TypeId) -> bool {
        if source == target {
            return true;
        }
        self.ensure_relation_inputs_ready(&[source, target]);
        let target = self.substitute_this_type_if_needed(target);

        if source != TypeId::NEVER
            && self.is_concrete_source_to_deferred_keyof_index_access(source, target)
        {
            return false;
        }

        {
            let env = self.ctx.type_env.borrow();
            let flags = self.ctx.pack_relation_flags();
            let inputs = AssignabilityQueryInputs {
                db: self.ctx.types,
                resolver: &*env,
                source,
                target,
                flags,
                inheritance_graph: &self.ctx.inheritance_graph,
                sound_mode: self.ctx.sound_mode(),
            };
            if let Some(result) = check_application_variance_assignability(&inputs) {
                return result;
            }
        }

        let source = self.evaluate_type_for_assignability(source);
        let target = self.evaluate_type_for_assignability(target);

        let result = {
            let env = self.ctx.type_env.borrow();
            let flags = self.ctx.pack_relation_flags();
            let overrides = CheckerOverrideProvider::new(self, Some(&*env));
            let relation_result = is_assignable_with_overrides(
                &AssignabilityQueryInputs {
                    db: self.ctx.types,
                    resolver: &*env,
                    source,
                    target,
                    flags,
                    inheritance_graph: &self.ctx.inheritance_graph,
                    sound_mode: self.ctx.sound_mode(),
                },
                &overrides,
            );
            self.propagate_overflow_flags(
                relation_result.depth_exceeded,
                relation_result.iteration_exceeded,
            );
            relation_result.is_related()
        };

        if result
            && self
                .checker_only_assignability_failure_reason(source, target)
                .is_some()
        {
            return false;
        }

        if let Some(keyof_type) = get_keyof_type(self.ctx.types, target)
            && let Some(source_atom) = get_string_literal_value(self.ctx.types, source)
        {
            let source_str = self.ctx.types.resolve_atom(source_atom);
            let allowed_keys = get_allowed_keys(self.ctx.types, keyof_type);
            // Only reject when we could determine concrete keys. An empty set means
            // the inner type couldn't be resolved (e.g., ThisType, TypeParameter,
            // or Application). In that case, trust the solver's result.
            if !allowed_keys.is_empty() && !allowed_keys.contains(&source_str) {
                return false;
            }
        }

        result
    }

    /// Check if `source` type is assignable to `target` type with bivariant function parameter checking.
    ///
    /// This is used for class method override checking, where methods are always bivariant
    /// (unlike function properties which are contravariant with strictFunctionTypes).
    ///
    /// Follows the same pattern as `is_assignable_to` but calls `is_assignable_to_bivariant_callback`
    /// which disables `strict_function_types` for the check.
    pub fn is_assignable_to_bivariant(&mut self, source: TypeId, target: TypeId) -> bool {
        self.is_assignable_to_bivariant_with_extra_flags(source, target, 0)
    }

    /// Bivariant assignability that additionally keeps method-local generic
    /// type parameters opaque (`NO_ERASE_GENERICS`).
    ///
    /// Class method overrides are bivariant in their parameters, but the base
    /// method's universal quantification must still be preserved: a concrete
    /// `(x: string) => string` is not a valid override of a generic
    /// `<T extends string>(x: T) => T`, because a caller could instantiate `T`
    /// with a proper subtype of `string` and expect that subtype back. Erasing
    /// the base type parameter to its constraint (the default bivariant path)
    /// hides this `TS2416`. Mirrors the `no_erase_generics` relation used by the
    /// `implements` member-override path while retaining bivariant parameters.
    pub fn is_assignable_to_bivariant_no_erase_generics(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        self.is_assignable_to_bivariant_with_extra_flags(
            source,
            target,
            crate::query_boundaries::assignability::RelationFlags::NO_ERASE_GENERICS,
        )
    }

    fn is_assignable_to_bivariant_with_extra_flags(
        &mut self,
        source: TypeId,
        target: TypeId,
        extra_flags: u16,
    ) -> bool {
        if source == target {
            return true;
        }
        // CRITICAL: Ensure all Ref types are resolved before assignability check.
        // This fixes intersection type assignability where `type AB = A & B` needs
        // A and B in type_env before we can check if a type is assignable to the intersection.
        self.ensure_relation_inputs_ready(&[source, target]);

        let source = self.evaluate_type_for_assignability(source);
        let target = self.evaluate_type_for_assignability(target);

        // Check relation cache for non-inference types
        // Construct RelationCacheKey with Lawyer-layer flags to prevent cache poisoning
        // Note: Use ORIGINAL types for cache key, not evaluated types
        let is_cacheable = is_relation_cacheable(self.ctx.types, source, target);

        // For bivariant checks, we strip the strict_function_types flag
        // so the cache key is distinct from regular assignability checks.
        // `extra_flags` lets callers force additional policy (e.g.
        // `NO_ERASE_GENERICS`) while keeping the bivariant parameter behavior.
        let flags = (self.ctx.pack_relation_flags()
            & !crate::query_boundaries::assignability::RelationFlags::STRICT_FUNCTION_TYPES)
            | extra_flags;

        if is_cacheable {
            // Note: For assignability checks, we use AnyPropagationMode::All (0)
            // since the checker doesn't track depth like SubtypeChecker does
            let cache_key = assignability_cache_key(source, target, flags);

            if let Some(cached) = self.ctx.types.lookup_assignability_cache(cache_key) {
                return cached;
            }
        }

        let env = self.ctx.type_env.borrow();
        // Preserve existing behavior: bivariant path does not use checker overrides.
        let relation_result = is_assignable_bivariant_with_resolver(
            self.ctx.types,
            &*env,
            source,
            target,
            flags,
            &self.ctx.inheritance_graph,
            self.ctx.sound_mode(),
        );
        self.propagate_overflow_flags(
            relation_result.depth_exceeded,
            relation_result.iteration_exceeded,
        );
        let result = relation_result.is_related();

        // Cache the result for non-inference types
        // Use ORIGINAL types for cache key (not evaluated types)
        if is_cacheable {
            let cache_key = assignability_cache_key(source, target, flags);

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
        // Ensure centralized relation preconditions before overlap check.
        self.ensure_relation_input_ready(left);
        self.ensure_relation_input_ready(right);

        let env = self.ctx.type_env.borrow();
        are_types_overlapping_with_env(
            self.ctx.types,
            &env,
            left,
            right,
            self.ctx.strict_null_checks(),
        )
    }
}
