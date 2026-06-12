use super::super::super::{SubtypeChecker, SubtypeResult, TypeResolver};
use super::args_contain_type_parameters;
use crate::def::DefId;
use crate::diagnostics::SubtypeFailureReason;
use crate::types::{TypeApplicationId, TypeData, TypeId, Variance};
use crate::visitor::{application_id, object_shape_id, object_with_index_shape_id};
use rustc_hash::FxHashSet;
use std::sync::Arc;

/// Maximum nesting depth, per generic `DefId`, for the one-sided application
/// expansion relation paths (`App <: T` and `T <: App`).
///
/// This mirrors tsc's `isDeeplyNestedType` recursion-identity bailout (default
/// `maxDepth = 5`). When the same generic definition is re-introduced by its own
/// structural expansion past this depth, the relation is assumed related
/// (`Ternary.Maybe`) instead of expanding further. The two-sided `App <: App`
/// comparison already bounds recursion through `def_guard`; this constant bounds
/// the previously-unguarded one-sided paths so a generic alias whose expansion
/// keeps re-introducing itself (recursive mapped/conditional/template
/// compositions) terminates cheaply rather than driving a terminating-but-
/// exponential evaluate<->subtype expansion.
pub(crate) const ONE_SIDED_APP_EXPANSION_MAX_DEPTH: u32 = 5;

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Same-base all-`any`-target-args lawyer shortcut.
    pub(crate) fn try_same_base_all_any_target_args(
        &mut self,
        source: TypeId,
        s_app_id: Option<TypeApplicationId>,
        t_app_id: TypeApplicationId,
    ) -> Option<SubtypeResult> {
        let t_app = self.interner.type_application(t_app_id);
        if t_app.args.is_empty() || !t_app.args.iter().all(|arg| arg.is_any()) {
            return None;
        }
        let allow_any = self
            .any_propagation
            .allows_any_source_at_depth(self.guard.depth())
            && self
                .any_propagation
                .allows_any_target_at_depth(self.guard.depth());
        if !allow_any {
            return None;
        }
        let same_definition = if let Some(s_app_id) = s_app_id {
            let s_app = self.interner.type_application(s_app_id);
            s_app.base == t_app.base
                || matches!(
                    (
                        crate::visitor::lazy_def_id(self.interner, s_app.base),
                        crate::visitor::lazy_def_id(self.interner, t_app.base),
                    ),
                    (Some(s_def), Some(t_def)) if self.resolver.defs_are_equivalent(s_def, t_def)
                )
        } else {
            let s_shape_id = object_shape_id(self.interner, source)
                .or_else(|| object_with_index_shape_id(self.interner, source));
            let s_symbol =
                s_shape_id.and_then(|shape_id| self.interner.object_shape(shape_id).symbol);
            let t_symbol = crate::visitor::lazy_def_id(self.interner, t_app.base)
                .and_then(|t_def| self.resolver.def_to_symbol_id(t_def));
            matches!((s_symbol, t_symbol), (Some(s_sym), Some(t_sym)) if s_sym == t_sym)
        };
        same_definition.then_some(SubtypeResult::True)
    }

    pub(super) fn check_expanded_application_subtype(
        &mut self,
        source_struct: TypeId,
        target_struct: TypeId,
        source_receiver: TypeId,
        target_receiver: TypeId,
    ) -> SubtypeResult {
        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_shape_id(self.interner, source_struct),
            object_shape_id(self.interner, target_struct),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);
            return self.check_object_subtype(
                &s_shape,
                Some(s_shape_id),
                Some(source_receiver),
                &t_shape,
                Some(target_receiver),
            );
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_with_index_shape_id(self.interner, source_struct),
            object_with_index_shape_id(self.interner, target_struct),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);
            return self.check_object_with_index_subtype(
                &s_shape,
                Some(s_shape_id),
                Some(source_receiver),
                &t_shape,
                Some(target_receiver),
            );
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_with_index_shape_id(self.interner, source_struct),
            object_shape_id(self.interner, target_struct),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);
            return self.check_object_with_index_to_object(
                &s_shape,
                s_shape_id,
                Some(source_receiver),
                &t_shape.properties,
                Some(target_receiver),
            );
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_shape_id(self.interner, source_struct),
            object_with_index_shape_id(self.interner, target_struct),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);
            return self.check_object_to_indexed(
                &s_shape.properties,
                Some(s_shape_id),
                Some(source_receiver),
                &t_shape,
                Some(target_receiver),
            );
        }

        self.check_subtype(source_struct, target_struct)
    }

    pub(super) fn expanded_application_pair_has_method_property(
        &mut self,
        source_type: TypeId,
        s_app_id: TypeApplicationId,
        target_type: TypeId,
        t_app_id: TypeApplicationId,
    ) -> bool {
        let source_struct = self.try_expand_application_type(source_type, s_app_id);
        let target_struct = self.try_expand_application_type(target_type, t_app_id);
        source_struct.is_some_and(|type_id| self.type_has_method_property(type_id))
            || target_struct.is_some_and(|type_id| self.type_has_method_property(type_id))
    }

    fn type_has_method_property(&self, type_id: TypeId) -> bool {
        object_shape_id(self.interner, type_id)
            .or_else(|| object_with_index_shape_id(self.interner, type_id))
            .map(|shape_id| self.interner.object_shape(shape_id))
            .is_some_and(|shape| shape.properties.iter().any(|prop| prop.is_method))
    }

    /// Enter a one-sided application expansion for `def_id`.
    ///
    /// Returns `true` if expansion may proceed and `false` when the same
    /// generic definition is already nested at or beyond
    /// [`ONE_SIDED_APP_EXPANSION_MAX_DEPTH`] (the caller should bail to
    /// [`Self::depth_result`]). Callers that receive `true` must pair this with
    /// [`Self::leave_app_expansion_depth`] once the expansion completes.
    pub(crate) fn enter_app_expansion_depth(&mut self, def_id: DefId) -> bool {
        let depth = self.app_expand_depth.get(&def_id).copied().unwrap_or(0);
        if depth >= ONE_SIDED_APP_EXPANSION_MAX_DEPTH {
            return false;
        }
        self.app_expand_depth.insert(def_id, depth + 1);
        true
    }

    /// Leave a one-sided application expansion previously entered via
    /// [`Self::enter_app_expansion_depth`].
    pub(crate) fn leave_app_expansion_depth(&mut self, def_id: DefId) {
        if let Some(depth) = self.app_expand_depth.get_mut(&def_id) {
            *depth = depth.saturating_sub(1);
        }
    }

    pub(super) fn iterator_protocol_mismatch_for_same_application_family(
        &mut self,
        source_type: TypeId,
        target_type: TypeId,
    ) -> bool {
        let Some(query_db) = self.query_db else {
            return false;
        };

        let iterator_mismatch = |checker: &mut Self, is_async: bool| {
            let source_eval = checker.evaluate_type(source_type);
            let target_eval = checker.evaluate_type(target_type);
            let source_info = crate::operations::get_iterator_info(query_db, source_eval, is_async)
                .or_else(|| crate::operations::get_iterator_info(query_db, source_type, is_async));
            let target_info = crate::operations::get_iterator_info(query_db, target_eval, is_async)
                .or_else(|| crate::operations::get_iterator_info(query_db, target_type, is_async));
            source_info
                .zip(target_info)
                .is_some_and(|(source, target)| {
                    !checker
                        .check_subtype(source.yield_type, target.yield_type)
                        .is_true()
                        || !checker
                            .check_subtype(source.return_type, target.return_type)
                            .is_true()
                        || !checker
                            .check_subtype(target.next_type, source.next_type)
                            .is_true()
                })
        };

        iterator_mismatch(self, false) || iterator_mismatch(self, true)
    }

    pub(super) fn application_cycle_with_concrete_differing_args_is_unsound(
        &self,
        s_app: &crate::types::TypeApplication,
        t_app: &crate::types::TypeApplication,
    ) -> bool {
        if s_app.args == t_app.args {
            return false;
        }

        if self.application_type_args_are_unwitnessed(s_app)
            && self.application_type_args_are_unwitnessed(t_app)
        {
            return false;
        }

        s_app.args.iter().chain(t_app.args.iter()).all(|&arg| {
            !crate::contains_type_parameters(self.interner, arg)
                && !crate::contains_this_type(self.interner, arg)
        })
    }

    fn application_type_args_are_unwitnessed(&self, app: &crate::types::TypeApplication) -> bool {
        let Some(def_id) = self.application_base_def_id(app.base) else {
            return false;
        };

        let variances = self.resolve_application_variances(def_id);

        variances.as_ref().is_some_and(|variances| {
            variances.len() == app.args.len() && variances.iter().all(|v| v.is_independent())
        })
    }

    /// Same-base identical-or-`any` argument lawyer shortcut.
    ///
    /// Generalizes [`Self::try_same_base_all_any_target_args`] to mixed
    /// argument lists: when source and target are applications of the SAME
    /// generic definition with equal arity and every argument pair is either
    /// the identical `TypeId` or has `any` on at least one side, tsc relates
    /// the two instantiations (`relateVariances`: an invariant-strength
    /// per-argument check passes in both directions for `any`, regardless of
    /// the measured variance and before any structural expansion). This is
    /// the kysely `ExpressionWrapper<DB, TB, any>` vs
    /// `ExpressionWrapper<DB, TB, O[K]>` shape, whose deferred-conditional
    /// members can never relate structurally.
    ///
    /// Accept-only by construction (returns `Some(True)` or `None`), so it is
    /// safe for provenance-recovered application identities. Gated on
    /// any-propagation being permissive on both sides at the current depth.
    pub(crate) fn try_same_base_args_identical_or_any(
        &mut self,
        s_app_id: TypeApplicationId,
        t_app_id: TypeApplicationId,
    ) -> Option<SubtypeResult> {
        let s_app = self.interner.type_application(s_app_id);
        let t_app = self.interner.type_application(t_app_id);

        if s_app.args.len() != t_app.args.len() || s_app.args.is_empty() {
            return None;
        }
        let same_definition = s_app.base == t_app.base
            || matches!(
                (
                    crate::visitor::lazy_def_id(self.interner, s_app.base),
                    crate::visitor::lazy_def_id(self.interner, t_app.base),
                ),
                (Some(s_def), Some(t_def)) if self.resolver.defs_are_equivalent(s_def, t_def)
            );
        if !same_definition {
            return None;
        }
        let args_identical_or_any = s_app
            .args
            .iter()
            .zip(t_app.args.iter())
            .all(|(&s_arg, &t_arg)| s_arg == t_arg || s_arg.is_any() || t_arg.is_any());
        if !args_identical_or_any {
            return None;
        }
        // The acceptance is justified by an `any` argument silencing the
        // differing slot. When EVERY pair is identical, the two sides are
        // structurally distinct for a NON-argument reason (e.g. the same
        // application evaluated under different checker contexts -
        // exactOptionalPropertyTypes can legally produce distinct shapes), so
        // argument reasoning proves nothing; fall through to the structural
        // comparison, which distinguishes those shapes correctly
        // (`inferenceExactOptionalProperties2.ts`).
        let any_pair_differs = s_app
            .args
            .iter()
            .zip(t_app.args.iter())
            .any(|(&s_arg, &t_arg)| s_arg != t_arg);
        if !any_pair_differs {
            return None;
        }
        let allow_any = self
            .any_propagation
            .allows_any_source_at_depth(self.guard.depth())
            && self
                .any_propagation
                .allows_any_target_at_depth(self.guard.depth());
        if !allow_any {
            return None;
        }
        Some(SubtypeResult::True)
    }

    pub(super) fn application_base_def_id(&self, base: TypeId) -> Option<DefId> {
        if base.is_intrinsic() {
            return None;
        }
        match self.interner.lookup(base) {
            Some(TypeData::Lazy(def_id)) => Some(def_id),
            Some(TypeData::TypeQuery(sym_ref)) => {
                let def_id = self.resolver.symbol_to_def_id(sym_ref)?;
                matches!(
                    self.resolver.get_def_kind(def_id),
                    Some(crate::def::DefKind::Interface | crate::def::DefKind::TypeAlias)
                )
                .then_some(def_id)
            }
            _ => None,
        }
    }

    pub(super) fn application_args_are_concrete(&self, args: &[TypeId]) -> bool {
        args.iter().all(|&arg| {
            !crate::contains_type_parameters(self.interner, arg)
                && !crate::contains_this_type(self.interner, arg)
        })
    }

    pub(super) fn recursive_mapped_alias_base_reaches_self(&self, base: TypeId) -> bool {
        let Some(def_id) = self.application_base_def_id(base) else {
            return false;
        };
        if self.resolver.get_def_kind(def_id) != Some(crate::def::DefKind::TypeAlias) {
            return false;
        }
        let Some(body) = self.resolver.resolve_lazy(def_id, self.interner) else {
            return false;
        };
        if !matches!(self.interner.lookup(body), Some(TypeData::Mapped(_))) {
            return false;
        }

        let mut visited = FxHashSet::default();
        self.type_reaches_def(body, def_id, &mut visited)
    }

    pub(super) fn conditional_infer_alias_base(&self, base: TypeId) -> bool {
        let Some(def_id) = self.application_base_def_id(base) else {
            return false;
        };
        let Some(body) = self
            .resolver
            .get_def_raw_body(def_id, self.interner)
            .or_else(|| self.resolver.resolve_lazy(def_id, self.interner))
        else {
            return false;
        };
        matches!(
            crate::type_queries::classify_body_for_arg_preservation(self.interner, body),
            crate::type_queries::BodyArgPreservation::ConditionalInfer
                | crate::type_queries::BodyArgPreservation::ConditionalApplicationInfer
        ) || crate::type_queries::contains_infer_types_db(self.interner, body)
    }

    fn type_reaches_def(
        &self,
        type_id: TypeId,
        target_def_id: DefId,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        if type_id.is_intrinsic() || !visited.insert(type_id) {
            return false;
        }
        match self.interner.lookup(type_id) {
            Some(TypeData::Lazy(def_id))
                if self.resolver.defs_are_equivalent(def_id, target_def_id) =>
            {
                return true;
            }
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                if let Some(TypeData::Lazy(def_id)) = self.interner.lookup(app.base)
                    && self.resolver.defs_are_equivalent(def_id, target_def_id)
                {
                    return true;
                }
            }
            _ => {}
        }

        let mut found = false;
        crate::visitor::for_each_child_by_id(self.interner, type_id, |child| {
            if !found {
                found = self.type_reaches_def(child, target_def_id, visited);
            }
        });
        found
    }

    /// Resolve the per-type-parameter variance mask for a generic definition,
    /// preferring declared/cached variances and computing (and caching) them
    /// only when missing. Shared by the variance-aware relation fast path and
    /// the same-generic error-elaboration path so both observe identical
    /// variance facts.
    pub(crate) fn resolve_application_variances(&self, def_id: DefId) -> Option<Arc<[Variance]>> {
        self.resolver.get_type_param_variance(def_id).or_else(|| {
            crate::relations::variance::compute_type_param_variances_with_resolver_cached(
                self.interner,
                self.resolver,
                self.query_db,
                def_id,
            )
        })
    }

    /// Explain a same-generic application failure (`C<A..>` vs `C<B..>`) by
    /// comparing the differing type **arguments** directly, mirroring tsc.
    ///
    /// Structural rule: when source and target are applications of the same
    /// generic target whose variances are reliably measured (no mapped-modifier
    /// structural fallback, no unreliable rejection) and whose arguments are
    /// concrete (no embedded type parameters), tsc reports the first
    /// variance-failing type argument as a direct nested line - e.g.
    /// `Type 'number' is not assignable to type 'string'.` - without the
    /// `Types of property 'x' are incompatible.` wrapper that structural
    /// expansion would produce.
    ///
    /// Returns `None` (so the caller falls back to structural elaboration)
    /// whenever the relation itself would fall through to structural
    /// comparison, keeping the existing property-based elaboration for those
    /// shapes. This mirrors the conclusive-rejection conditions in
    /// [`Self::check_application_to_application_subtype`].
    pub(crate) fn explain_same_generic_type_arguments(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<SubtypeFailureReason> {
        // Callers may pass raw (lazy) references or already-resolved types;
        // resolve lazy aliases so a `Lazy(DefId)` wrapping an application is
        // recognised, while leaving direct applications untouched.
        let resolved_source = self.resolve_lazy_type(source);
        let resolved_target = self.resolve_lazy_type(target);
        let s_app_id = application_id(self.interner, resolved_source)?;
        let t_app_id = application_id(self.interner, resolved_target)?;
        let s_app = self.interner.type_application(s_app_id);
        let t_app = self.interner.type_application(t_app_id);

        if s_app.base != t_app.base || s_app.args.len() != t_app.args.len() {
            return None;
        }

        let def_id = self.application_base_def_id(s_app.base)?;
        let variances = self.resolve_application_variances(def_id)?;
        if variances.len() != s_app.args.len() {
            return None;
        }

        // Mapped-modifier (`needs_structural_fallback`) and indexed-access /
        // intersection-normalization (`rejection_unreliable`) variances make a
        // variance-based rejection unreliable; tsc reports those structurally
        // (its variance carries the structural-fallback marker), so keep the
        // property-based elaboration for them.
        if variances
            .iter()
            .any(|v| v.needs_structural_fallback() || v.rejection_unreliable())
        {
            return None;
        }
        let source_args_contain_type_parameters =
            args_contain_type_parameters(self.interner, &s_app.args);
        let method_receiver_fallback = source_args_contain_type_parameters
            && self.expanded_application_pair_has_method_property(
                resolved_source,
                s_app_id,
                resolved_target,
                t_app_id,
            );
        let conditional_infer_alias = self.conditional_infer_alias_base(s_app.base)
            || self.conditional_infer_alias_base(t_app.base);
        if source_args_contain_type_parameters
            && (!variances.iter().any(|v| v.has_direct_usage())
                || conditional_infer_alias
                || method_receiver_fallback)
        {
            return None;
        }

        // `s_app`/`t_app` are owned `Arc`s, independent of `self`, so the
        // arguments can be indexed directly across the `&mut self` relation
        // calls below without cloning the argument vectors.
        for (i, variance) in variances.iter().enumerate() {
            let s_arg = s_app.args[i];
            let t_arg = t_app.args[i];

            // Orient the failing relation by the parameter's variance, matching
            // the per-argument direction used by the relation fast path.
            let failing_pair = if variance.is_invariant() {
                if !self.check_subtype(s_arg, t_arg).is_true() {
                    Some((s_arg, t_arg))
                } else if !self.check_subtype(t_arg, s_arg).is_true() {
                    Some((t_arg, s_arg))
                } else {
                    None
                }
            } else if variance.is_covariant() {
                (!self.check_subtype(s_arg, t_arg).is_true()).then_some((s_arg, t_arg))
            } else if variance.is_contravariant() {
                (!self.check_subtype(t_arg, s_arg).is_true()).then_some((t_arg, s_arg))
            } else {
                // Independent: argument does not constrain the relation.
                None
            };

            if let Some((fail_src, fail_tgt)) = failing_pair {
                // The type-argument elaboration is only reliable when this
                // parameter's variance comes from a *direct* usage (a property,
                // function parameter, or return type). Variances synthesized
                // purely from mapped-type / indexed-access positions lack
                // `DIRECT_USAGE`: there the differing arguments can normalize to
                // structurally distinct shapes, and tsc reports the structural
                // member difference (e.g. a missing property) rather than the
                // raw argument relation. Fall back to structural elaboration for
                // those, matching tsc.
                if !variance.has_direct_usage() {
                    return None;
                }
                let nested = self.explain_failure(fail_src, fail_tgt).unwrap_or(
                    SubtypeFailureReason::TypeMismatch {
                        source_type: fail_src,
                        target_type: fail_tgt,
                    },
                );
                return Some(SubtypeFailureReason::TypeArgumentMismatch {
                    source_arg: fail_src,
                    target_arg: fail_tgt,
                    nested_reason: Box::new(nested),
                });
            }
        }

        None
    }
}
