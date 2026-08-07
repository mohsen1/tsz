use super::{AssignabilityChecker, CallEvaluator};
use crate::types::TypeId;

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    fn bare_source_rest_targets_union_query(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> crate::type_queries::RestBinderQuery<bool> {
        if let Some(resolver) = self.checker.type_resolver() {
            crate::type_queries::bare_source_rest_targets_union_with_resolver_query(
                self.interner,
                &resolver,
                source,
                target,
            )
        } else {
            crate::type_queries::bare_source_rest_targets_union_with_resolver_query(
                self.interner,
                &self.interner,
                source,
                target,
            )
        }
    }

    pub(super) fn contains_declared_bare_function_rest_query(
        &self,
        type_id: TypeId,
    ) -> crate::type_queries::RestBinderQuery<bool> {
        if let Some(result) = self
            .declared_bare_rest_cache
            .borrow()
            .get(&type_id)
            .copied()
        {
            return result;
        }
        let result = if let Some(resolver) = self.checker.type_resolver() {
            crate::type_queries::contains_declared_bare_function_rest_with_resolver_query(
                self.interner,
                &resolver,
                type_id,
            )
        } else {
            crate::type_queries::contains_declared_bare_function_rest_with_resolver_query(
                self.interner,
                &self.interner,
                type_id,
            )
        };
        self.declared_bare_rest_cache
            .borrow_mut()
            .insert(type_id, result);
        result
    }

    /// Final strict argument relation for generic calls.
    ///
    /// Only types containing a declared bare callable rest need their raw
    /// surface preserved. All other arguments retain the existing prepared
    /// strict relation and contextual-signature fallback.
    pub(super) fn argument_assignable_preserving_rest_surface(
        &mut self,
        actual: TypeId,
        expected: TypeId,
        strict: bool,
        allow_contextual_retry: bool,
        provisional_site: bool,
    ) -> bool {
        if !strict {
            return self.checker.is_assignable_to(actual, expected);
        }
        let actual_surface = self
            .checker
            .expand_type_alias_application(actual)
            .unwrap_or(actual);
        let expected_surface = self
            .checker
            .expand_type_alias_application(expected)
            .unwrap_or(expected);
        let raw_pair_is_provisional = provisional_site
            && matches!(
                self.bare_source_rest_targets_union_query(actual, expected),
                crate::type_queries::RestBinderQuery::Complete(true)
            );
        let surface_pair_is_provisional = provisional_site
            && matches!(
                self.bare_source_rest_targets_union_query(actual_surface, expected_surface),
                crate::type_queries::RestBinderQuery::Complete(true)
            );
        let provisional_rest_union =
            provisional_site && (raw_pair_is_provisional || surface_pair_is_provisional);
        let raw_sensitive = provisional_rest_union
            || !matches!(
                self.contains_declared_bare_function_rest_query(actual),
                crate::type_queries::RestBinderQuery::Complete(false)
            )
            || (actual_surface != actual
                && !matches!(
                    self.contains_declared_bare_function_rest_query(actual_surface),
                    crate::type_queries::RestBinderQuery::Complete(false)
                ))
            || !matches!(
                self.contains_declared_bare_function_rest_query(expected),
                crate::type_queries::RestBinderQuery::Complete(false)
            )
            || (expected_surface != expected
                && !matches!(
                    self.contains_declared_bare_function_rest_query(expected_surface),
                    crate::type_queries::RestBinderQuery::Complete(false)
                ));
        if raw_sensitive {
            let target = if expected_surface != expected
                || (provisional_rest_union && !raw_pair_is_provisional)
            {
                expected_surface
            } else {
                expected
            };
            let related = self.checker.is_assignable_to_generic_call(
                actual,
                target,
                strict,
                provisional_rest_union,
            );
            tracing::trace!(
                ?actual,
                ?expected,
                ?actual_surface,
                ?expected_surface,
                ?target,
                strict,
                provisional_site,
                provisional_rest_union,
                related,
                "generic call argument used the raw rest-sensitive relation"
            );
            return related;
        }

        if strict {
            // A function-valued argument follows the compiler's
            // `strictFunctionTypes` option for parameter variance (see
            // `AssignabilityChecker::strict_function_types`): when it is off the
            // closest-miss check must compare bivariantly, or it manufactures a
            // `TS2345` the plain assignability path never reports (#16632). The
            // `strict` flag still governs rest-binder raw-surface exactness in the
            // `raw_sensitive` branch above; it must not force function
            // contravariance the user opted out of. Non-callable arguments keep
            // the existing strict relation.
            let honor_bivariant_callback = !self.checker.strict_function_types()
                && crate::type_queries::is_callable_type(self.interner, actual);
            let related = if honor_bivariant_callback {
                self.checker.is_assignable_to(actual, expected)
            } else {
                self.checker.is_assignable_to_strict(actual, expected)
            };
            related
                || (allow_contextual_retry
                    && self.is_assignable_via_contextual_signatures_strict(actual, expected))
        } else {
            self.checker.is_assignable_to(actual, expected)
        }
    }
}
