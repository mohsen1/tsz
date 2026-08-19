use super::super::super::{SubtypeChecker, SubtypeResult, TypeResolver};
use super::args_contain_type_parameters;
use crate::def::DefId;
use crate::diagnostics::SubtypeFailureReason;
use crate::types::{TypeApplication, TypeApplicationId, TypeData, TypeId, Variance};
use crate::visitor::{application_id, lazy_def_id, object_shape_id, object_with_index_shape_id};
use rustc_hash::FxHashSet;
use smallvec::SmallVec;
use std::sync::Arc;

pub(crate) struct AnyNeverVarianceClassification {
    pub(crate) rejects: bool,
    pub(super) accepted_indices: SmallVec<[usize; 4]>,
    pub(super) has_unresolved_exceptional: bool,
    pub(super) variances: Arc<[Variance]>,
}

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

/// OR `Variance::BIVARIANT_USAGE` from `effective` onto `declared` wherever
/// the effective (context-aware) computation marks a position bivariant,
/// leaving every other position exactly as declared-mode measured it.
///
/// `BIVARIANT_USAGE` alone forces `Variance::needs_structural_fallback` for
/// that position, so this can only ever turn a conclusive variance-fast-path
/// rejection into a structural retry — it never manufactures a new
/// rejection, and it never touches positions declared-mode did not mark
/// bivariant-eligible for a reason of its own (mapped-type modifiers,
/// unreliable rejection, ...).
pub(crate) fn merge_bivariant_usage(
    declared: &[Variance],
    effective: &[Variance],
) -> Arc<[Variance]> {
    if declared.len() != effective.len() {
        return Arc::from(declared);
    }
    declared
        .iter()
        .zip(effective.iter())
        .map(|(&d, &e)| {
            if e.has_bivariant_usage() {
                d | Variance::BIVARIANT_USAGE
            } else {
                d
            }
        })
        .collect()
}

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Reject a same-base application when a direct `any`/`never` argument pair
    /// is incompatible in a reliably measured variance position.
    ///
    /// The two orientations cannot use the generic `any` shortcut: covariance
    /// rejects `any -> never`; contravariance rejects `never -> any`, and
    /// invariance rejects both. The effective mask has already applied callback
    /// bivariance for the active `strictFunctionTypes` setting and retained
    /// explicit `in`/`out` annotations. An independent position accepts both.
    /// Unknown or structurally unreliable variance remains undecided.
    pub(crate) fn try_same_base_any_never_variance_result(
        &mut self,
        s_app_id: TypeApplicationId,
        t_app_id: TypeApplicationId,
    ) -> Option<SubtypeResult> {
        let s_app = self.interner.type_application(s_app_id);
        let t_app = self.interner.type_application(t_app_id);
        if s_app.base != t_app.base || s_app.args.len() != t_app.args.len() {
            return None;
        }
        if !s_app
            .args
            .iter()
            .zip(t_app.args.iter())
            .any(|(&source, &target)| {
                (source.is_any() && target == TypeId::NEVER)
                    || (source == TypeId::NEVER && target.is_any())
            })
        {
            return None;
        }
        // Indexed-access aliases are transparent transforms. Their expanded
        // result can normalize an apparent raw-argument mismatch away, so the
        // established structural fallback remains authoritative.
        if self.is_indexed_access_alias_base_inline(s_app.base) {
            return None;
        }
        let def_id = self.application_base_def_id(s_app.base)?;
        let classification =
            self.classify_application_args_any_never_variance(def_id, &s_app.args, &t_app.args)?;
        if classification.rejects {
            return Some(SubtypeResult::False);
        }
        if !classification.has_unresolved_exceptional
            && let Some(result) = self.try_application_variance_with_mask(
                &classification.variances,
                &s_app.args,
                &t_app.args,
            )
        {
            return Some(result);
        }
        if classification.accepted_indices.is_empty() {
            return None;
        }
        let mut masked_source_args = s_app.args.to_vec();
        for index in classification.accepted_indices {
            masked_source_args[index] = t_app.args[index];
        }
        let masked_source = self.interner.application(s_app.base, masked_source_args);
        let target = self.interner.application(t_app.base, t_app.args.to_vec());
        Some(self.check_subtype(masked_source, target))
    }

    /// Apply a pre-resolved variance mask to every application argument.
    ///
    /// The exceptional `any`/`never` positions have already been classified;
    /// this second pass is what keeps mismatches in other slots visible,
    /// including structurally filled holes of partial variance declarations.
    pub(super) fn try_application_variance_with_mask(
        &mut self,
        variances: &[Variance],
        source_args: &[TypeId],
        target_args: &[TypeId],
    ) -> Option<SubtypeResult> {
        if variances.len() != source_args.len() || source_args.len() != target_args.len() {
            return None;
        }
        let needs_structural_fallback = variances.iter().any(Variance::needs_structural_fallback);
        let rejection_unreliable = variances.iter().any(Variance::rejection_unreliable);
        let outcome = crate::relations::variance::run_application_variance_arg_loop(
            variances,
            source_args,
            target_args,
            |source, target| self.check_subtype(source, target).is_true(),
        );
        if outcome.all_ok
            && !needs_structural_fallback
            && (outcome.any_checked || !variances.is_empty())
        {
            return Some(SubtypeResult::True);
        }
        if outcome.any_checked
            && !outcome.all_ok
            && !needs_structural_fallback
            && !rejection_unreliable
            && !args_contain_type_parameters(self.interner, source_args)
        {
            return Some(SubtypeResult::False);
        }
        None
    }

    /// Reject `any`/`never` through reliable nested application variance.
    ///
    /// Each level must be the same generic identity or an exact one-parameter
    /// pass-through alias of the opposite identity. Indexed-access transforms,
    /// unreliable variance, and unknown application shapes remain undecided so
    /// the ordinary structural relation owns them. A cycle-safe worklist avoids
    /// treating arbitrary nesting depth or traversal fuel as compatibility.
    pub(crate) fn application_any_never_variance_rejects(
        &self,
        s_app_id: TypeApplicationId,
        t_app_id: TypeApplicationId,
    ) -> bool {
        if !self.application_pair_reaches_any_never(s_app_id, t_app_id) {
            // This query also runs for ordinary erased overload return pairs.
            // Keep pairs without a corresponding exceptional leaf on the
            // shape-only path with no `DefId` or variance resolution.
            return false;
        }

        let mut pending = SmallVec::<[(TypeApplicationId, TypeApplicationId); 8]>::new();
        let mut seen = FxHashSet::default();
        pending.push((s_app_id, t_app_id));
        while let Some((s_app_id, t_app_id)) = pending.pop() {
            if !seen.insert((s_app_id, t_app_id)) {
                continue;
            }
            let s_app = self.interner.type_application(s_app_id);
            let t_app = self.interner.type_application(t_app_id);
            if s_app.args.len() != t_app.args.len() {
                continue;
            }

            let (Some(s_def), Some(t_def)) = (
                self.application_base_def_id(s_app.base),
                self.application_base_def_id(t_app.base),
            ) else {
                continue;
            };
            let def_id = if s_app.base == t_app.base {
                if self.is_indexed_access_alias_base_inline(s_app.base) {
                    continue;
                }
                s_def
            } else if self.alias_body_forwards_positionally_to_generic(s_def, t_def) {
                if self.is_indexed_access_alias_base_inline(t_app.base) {
                    continue;
                }
                t_def
            } else if self.alias_body_forwards_positionally_to_generic(t_def, s_def) {
                if self.is_indexed_access_alias_base_inline(s_app.base) {
                    continue;
                }
                s_def
            } else {
                continue;
            };
            let Some(def_id) = self.any_never_variance_owner_def(def_id) else {
                // A non-pass-through type alias is a transparent transform.
                // Its expanded result, rather than raw arguments, is authoritative.
                continue;
            };

            let Some(variances) = self.resolve_effective_application_variances(def_id) else {
                continue;
            };
            if variances.len() != s_app.args.len() {
                continue;
            }

            for ((&source_arg, &target_arg), variance) in s_app
                .args
                .iter()
                .zip(t_app.args.iter())
                .zip(variances.iter())
            {
                if variance.needs_structural_fallback() || variance.rejection_unreliable() {
                    continue;
                }
                let forward = variance.is_covariant() || variance.is_invariant();
                let reverse = variance.is_contravariant() || variance.is_invariant();

                if (forward && source_arg.is_any() && target_arg == TypeId::NEVER)
                    || (reverse && source_arg == TypeId::NEVER && target_arg.is_any())
                {
                    return true;
                }
                let (Some(source_child), Some(target_child)) = (
                    application_id(self.interner, source_arg),
                    application_id(self.interner, target_arg),
                ) else {
                    continue;
                };
                if forward {
                    pending.push((source_child, target_child));
                }
                if reverse {
                    pending.push((target_child, source_child));
                }
            }
        }
        false
    }

    /// Shape-only prefilter for the exceptional classifier. It follows paired
    /// application arguments without alias, `DefId`, or variance queries and
    /// uses application-pair identity to terminate cycles at any finite depth.
    fn application_pair_reaches_any_never(
        &self,
        s_app_id: TypeApplicationId,
        t_app_id: TypeApplicationId,
    ) -> bool {
        let mut pending = SmallVec::<[(TypeId, TypeId); 16]>::new();
        let mut seen = FxHashSet::default();
        let s_app = self.interner.type_application(s_app_id);
        let t_app = self.interner.type_application(t_app_id);
        if s_app.args.len() != t_app.args.len() {
            return false;
        }
        pending.extend(s_app.args.iter().copied().zip(t_app.args.iter().copied()));
        while let Some((source, target)) = pending.pop() {
            if (source.is_any() && target == TypeId::NEVER)
                || (source == TypeId::NEVER && target.is_any())
            {
                return true;
            }
            if source == target {
                continue;
            }
            let (Some(source_app_id), Some(target_app_id)) = (
                application_id(self.interner, source),
                application_id(self.interner, target),
            ) else {
                continue;
            };
            if !seen.insert((source_app_id, target_app_id)) {
                continue;
            }
            let source_app = self.interner.type_application(source_app_id);
            let target_app = self.interner.type_application(target_app_id);
            if source_app.args.len() == target_app.args.len() {
                pending.extend(
                    source_app
                        .args
                        .iter()
                        .copied()
                        .zip(target_app.args.iter().copied()),
                );
            }
        }
        false
    }

    /// Apply the `any`/`never` exception using the variance of `def_id`.
    ///
    /// Accepts argument slices separately from their application bases so a
    /// transparent pass-through alias can be compared against the generic body
    /// it forwards to without losing argument orientation.
    pub(crate) fn classify_application_args_any_never_variance(
        &self,
        def_id: DefId,
        source_args: &[TypeId],
        target_args: &[TypeId],
    ) -> Option<AnyNeverVarianceClassification> {
        if source_args.len() != target_args.len()
            || !source_args
                .iter()
                .zip(target_args.iter())
                .any(|(&s_arg, &t_arg)| {
                    (s_arg.is_any() && t_arg == TypeId::NEVER)
                        || (s_arg == TypeId::NEVER && t_arg.is_any())
                })
        {
            // Keep the common same-application path O(arity) with no resolver
            // or variance-cache traffic unless the exceptional pair exists.
            return None;
        }
        let def_id = self.any_never_variance_owner_def(def_id)?;
        let variances = self.resolve_effective_application_variances(def_id)?;
        if variances.len() != source_args.len() {
            return None;
        }

        let mut accepted_indices = SmallVec::<[usize; 4]>::new();
        let mut has_unresolved_exceptional = false;
        for (index, ((&s_arg, &t_arg), variance)) in source_args
            .iter()
            .zip(target_args.iter())
            .zip(variances.iter())
            .enumerate()
        {
            let source_any_to_never = s_arg.is_any() && t_arg == TypeId::NEVER;
            let source_never_to_any = s_arg == TypeId::NEVER && t_arg.is_any();
            if !source_any_to_never && !source_never_to_any {
                continue;
            }
            if variance.is_pure_bivariant_usage() {
                accepted_indices.push(index);
                continue;
            }
            let reliable =
                !variance.needs_structural_fallback() && !variance.rejection_unreliable();
            if !reliable {
                has_unresolved_exceptional = true;
                continue;
            }
            let covariant = variance.is_covariant();
            let contravariant = variance.is_contravariant();
            let invariant = variance.is_invariant();
            if (source_any_to_never && (covariant || invariant))
                || (source_never_to_any && (contravariant || invariant))
            {
                return Some(AnyNeverVarianceClassification {
                    rejects: true,
                    accepted_indices,
                    has_unresolved_exceptional,
                    variances,
                });
            }
            if (source_any_to_never && contravariant)
                || (source_never_to_any && covariant)
                || variance.is_independent()
            {
                accepted_indices.push(index);
            } else {
                has_unresolved_exceptional = true;
            }
        }

        Some(AnyNeverVarianceClassification {
            rejects: false,
            accepted_indices,
            has_unresolved_exceptional,
            variances,
        })
    }

    /// Resolve the context-aware ("effective") variance mask for `def_id`.
    ///
    /// The result merges declared annotations with structural holes and
    /// observes the active `strictFunctionTypes`/method-bivariance settings —
    /// a function-typed property compares bivariantly here whenever
    /// `strictFunctionTypes` is off, exactly like a method. The
    /// generation-scoped evaluation-session cache keeps repeated queries
    /// O(arity) without retaining stale publication generations.
    fn resolve_effective_application_variances(&self, def_id: DefId) -> Option<Arc<[Variance]>> {
        let outcome =
            crate::relations::variance::compute_effective_type_param_variances_with_resolver_cached(
                self.interner,
                self.resolver,
                self.eval_session,
                def_id,
                self.strict_function_types,
                self.disable_method_bivariance,
            );
        let Some(outcome) = outcome else {
            // The effective query is registration-dependent. Taint the
            // enclosing relation so a provisional structural fallback cannot
            // be persisted before the definition finishes registration.
            self.note_unresolved_lazy_relation_event();
            return None;
        };
        if outcome.incomplete {
            self.note_unresolved_lazy_relation_event();
            return None;
        }
        Some(outcome.variances)
    }

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
        if s_app_id.is_some_and(|s_app_id| {
            self.interner
                .type_application(s_app_id)
                .args
                .contains(&TypeId::NEVER)
        }) {
            // `never` against target `any` depends on the generic parameter's
            // variance. Leave that pair to the variance-aware relation.
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
    /// the identical `TypeId` or has a compatible `any`, tsc relates the two
    /// instantiations. An `any`/`never` pair in either orientation must fall
    /// through to the ordinary variance relation because covariance and
    /// contravariance give those two orientations opposite answers. This is
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
        let args_identical_or_any =
            s_app
                .args
                .iter()
                .zip(t_app.args.iter())
                .all(|(&s_arg, &t_arg)| {
                    s_arg == t_arg
                        || ((s_arg.is_any() || t_arg.is_any())
                            && s_arg != TypeId::NEVER
                            && t_arg != TypeId::NEVER)
                });
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

    /// Acceptance-only unification of an exact positional pass-through type alias
    /// against the body generic it forwards to, restricted to the permissive
    /// `any`-argument shortcut.
    ///
    /// Fires only when `try_variance_fast_path` is about to bail on a
    /// different-base pair whose bases neither share a raw definition nor unify
    /// through import-alias forwarding. It recognizes the
    /// `Alias<X, Y> = Body<X, Y>` shape: a `DefKind::TypeAlias` whose resolved
    /// body is an `Application` of the *other* side's base, where every differing
    /// alias argument is `any`. Because the alias is a pass-through, each
    /// `any` argument flows unchanged into the body application, so the pair is
    /// `Body<any>` vs `Body<X>` — true under any-propagation. `tsc` never relates
    /// an alias nominally (it substitutes the body), so `Async<any>` and
    /// `Promise<X>` are the same type and `Async<any>` relates to anything; tsz
    /// must recover that here because the asymmetric evaluation of the two sides
    /// (one keeps the alias `Application`, the other unwraps to the `Promise`
    /// body) otherwise degrades to a structural `then`-callable comparison that
    /// can spuriously fail for a deferred-conditional type argument. Returns
    /// `None` for every other shape so the caller keeps its historical
    /// structural path. A proven `any`/`never` variance mismatch returns
    /// `False`; every other non-accepting shape remains undecided.
    pub(super) fn try_pass_through_alias_any_unification(
        &mut self,
        s_app: &TypeApplication,
        t_app: &TypeApplication,
        s_def: DefId,
        t_def: DefId,
    ) -> Option<SubtypeResult> {
        let allow_any = self
            .any_propagation
            .allows_any_source_at_depth(self.guard.depth())
            && self
                .any_propagation
                .allows_any_target_at_depth(self.guard.depth());
        if !allow_any {
            return None;
        }

        // Source is the alias side: `Alias<any, X>` vs `Body<Y, X>`.
        if s_app.args.len() == t_app.args.len()
            && s_app.args.iter().any(|arg| arg.is_any())
            && s_app
                .args
                .iter()
                .zip(t_app.args.iter())
                .all(|(&source, &target)| source.is_any() || source == target)
            && self.alias_body_forwards_positionally_to_generic(s_def, t_def)
        {
            if s_app
                .args
                .iter()
                .zip(t_app.args.iter())
                .any(|(&source, &target)| source.is_any() && target == TypeId::NEVER)
            {
                let classification = self.classify_application_args_any_never_variance(
                    t_def,
                    &s_app.args,
                    &t_app.args,
                )?;
                if classification.rejects {
                    return Some(SubtypeResult::False);
                }
                return (!classification.has_unresolved_exceptional).then_some(SubtypeResult::True);
            }
            return Some(SubtypeResult::True);
        }
        // Target is the alias side: `Body<X, Y>` vs `Alias<any, Y>`.
        if t_app.args.len() == s_app.args.len()
            && t_app.args.iter().any(|arg| arg.is_any())
            && s_app
                .args
                .iter()
                .zip(t_app.args.iter())
                .all(|(&source, &target)| target.is_any() || source == target)
            && self.alias_body_forwards_positionally_to_generic(t_def, s_def)
        {
            if s_app
                .args
                .iter()
                .zip(t_app.args.iter())
                .any(|(&source, &target)| source == TypeId::NEVER && target.is_any())
            {
                let classification = self.classify_application_args_any_never_variance(
                    s_def,
                    &s_app.args,
                    &t_app.args,
                )?;
                if classification.rejects {
                    return Some(SubtypeResult::False);
                }
                return (!classification.has_unresolved_exceptional).then_some(SubtypeResult::True);
            }
            return Some(SubtypeResult::True);
        }
        None
    }

    /// Whether `alias_def`'s body is an `Application` whose base canonically
    /// names `target_def` and whose parameters are forwarded exactly by
    /// position (for example `Alias<X, Y> = Pair<X, Y>`). An unresolvable alias body records the
    /// undetermined-result event so the enclosing relation does not persist a
    /// registration-window verdict.
    fn alias_body_forwards_positionally_to_generic(
        &self,
        alias_def: DefId,
        target_def: DefId,
    ) -> bool {
        self.positional_pass_through_alias_body_def(alias_def)
            .is_some_and(|body_def| {
                self.resolver.canonical_def_id(body_def)
                    == self.resolver.canonical_def_id(target_def)
            })
    }

    /// Find the generic definition whose variance governs an `any`/`never`
    /// application pair.
    ///
    /// TypeScript measures type-alias variance only for the alias body kinds
    /// that support variance annotations: object, function/constructor,
    /// callable, and mapped types. Those aliases own the variance decision.
    /// An exact positional application alias forwards to its body's owner.
    /// Normalizing transforms (union, tuple, conditional, `keyof`, indexed
    /// access, and similar bodies) must expand instead. Unknown registration
    /// state and cycles conservatively remain undecided.
    pub(super) fn any_never_variance_owner_def(&self, mut def_id: DefId) -> Option<DefId> {
        let mut seen = FxHashSet::default();
        loop {
            match self.resolver.get_def_kind(def_id) {
                Some(crate::def::DefKind::TypeAlias) => {}
                Some(_) => return Some(def_id),
                None => {
                    // Definition registration can be temporarily incomplete in
                    // cross-file/re-entrant checks. Unknown kind is not proof
                    // that this is a nominal generic: keep the relation
                    // undecided so no transform-alias verdict is cached.
                    self.note_unresolved_lazy_relation_event();
                    return None;
                }
            }
            if !seen.insert(def_id) {
                return None;
            }
            let body = self.alias_body_for_variance_classification(def_id)?;
            match self.interner.lookup(body) {
                Some(
                    TypeData::Object(_)
                    | TypeData::ObjectWithIndex(_)
                    | TypeData::Function(_)
                    | TypeData::Callable(_)
                    | TypeData::Mapped(_),
                ) => return Some(def_id),
                Some(TypeData::Application(_)) => {
                    def_id = self.positional_pass_through_alias_body_def_from_body(def_id, body)?;
                }
                Some(_) => return None,
                None => {
                    self.note_unresolved_lazy_relation_event();
                    return None;
                }
            }
        }
    }

    fn positional_pass_through_alias_body_def(&self, alias_def: DefId) -> Option<DefId> {
        if self.resolver.get_def_kind(alias_def) != Some(crate::def::DefKind::TypeAlias) {
            return None;
        }
        let body = self.alias_body_for_variance_classification(alias_def)?;
        self.positional_pass_through_alias_body_def_from_body(alias_def, body)
    }

    fn alias_body_for_variance_classification(&self, def_id: DefId) -> Option<TypeId> {
        if let Some(body) = self.resolver.get_def_raw_body(def_id, self.interner) {
            return Some(body);
        }
        let Some(body) = self.resolver.resolve_lazy(def_id, self.interner) else {
            self.note_unresolved_lazy_relation_event();
            return None;
        };
        if matches!(self.interner.lookup(body), Some(TypeData::Lazy(body_def)) if body_def == def_id)
        {
            // A self wrapper means the alias body is not published yet. Treat
            // the relation as registration-dependent so it cannot be cached.
            self.note_unresolved_lazy_relation_event();
            return None;
        }
        Some(body)
    }

    fn positional_pass_through_alias_body_def_from_body(
        &self,
        alias_def: DefId,
        body: TypeId,
    ) -> Option<DefId> {
        let body_app_id = application_id(self.interner, body)?;
        let body_app = self.interner.type_application(body_app_id);
        if body_app.args.is_empty() {
            return None;
        }
        let Some(alias_params) = self.resolver.get_lazy_type_params(alias_def) else {
            self.note_unresolved_lazy_relation_event();
            return None;
        };
        if alias_params.len() != body_app.args.len()
            || !alias_params
                .iter()
                .zip(body_app.args.iter())
                .all(|(&alias_param, &body_arg)| {
                    crate::type_param_info(self.interner, body_arg) == Some(alias_param)
                })
        {
            // Reordering, duplication, constants, and transforms are not
            // pass-through. Only the exact positional binder leaves prove it.
            return None;
        }
        self.application_base_def_id(body_app.base)
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
        matches!(self.interner.lookup(body), Some(TypeData::Conditional(_)))
            || matches!(
                crate::type_queries::classify_body_for_arg_preservation(self.interner, body),
                crate::type_queries::BodyArgPreservation::ConditionalInfer
                    | crate::type_queries::BodyArgPreservation::ConditionalApplicationInfer
            )
            || crate::type_queries::contains_infer_types_db(self.interner, body)
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
    /// preferring an explicit declared `in`/`out` annotation and falling back
    /// to the structural ("declared-mode") computation. Shared by the
    /// variance-aware relation fast path and the same-generic
    /// error-elaboration path so both observe identical variance facts.
    ///
    /// Declared-mode is session-stable and backed by the universe-shared
    /// variance store, which is exactly the robustness a complex builtin
    /// generic (`AsyncGenerator`, `Map`, ...) needs — swapping it out
    /// wholesale for the context-aware ("effective") computation regressed
    /// generic call inference through such types (a not-yet-inferred type
    /// parameter on the target side lost its structural-fallback protection
    /// and got hard-rejected before inference could extract a candidate).
    ///
    /// But declared-mode also ignores `strictFunctionTypes`/method-bivariance
    /// entirely, so a function-typed property (`{ member: (cb: T) => void
    /// }`) always measures as strictly contravariant there even when
    /// `strictFunctionTypes` is off. The fix is a targeted *merge*, not a
    /// replacement: keep the declared mask as the base (preserving its other
    /// markers — `needs_structural_fallback`, `rejection_unreliable` — for
    /// every position untouched by this rule), and OR in `BIVARIANT_USAGE`
    /// only at positions the effective computation marks bivariant. That bit
    /// alone forces the position to structural fallback (see
    /// `Variance::needs_structural_fallback`), so it can only ever loosen a
    /// conclusive rejection into a structural retry, never introduce a new
    /// one.
    pub(crate) fn resolve_application_variances(&self, def_id: DefId) -> Option<Arc<[Variance]>> {
        if let Some(explicit) = self.resolver.get_type_param_variance(def_id) {
            return Some(explicit);
        }
        let declared =
            crate::relations::variance::compute_type_param_variances_with_resolver_cached(
                self.interner,
                self.resolver,
                self.query_db,
                def_id,
            )?;
        if self.strict_function_types {
            // No bivariance loosening applies under strict semantics; the
            // effective computation would not add anything here.
            return Some(declared);
        }
        let Some(effective) = self.resolve_effective_application_variances(def_id) else {
            return Some(declared);
        };
        Some(merge_bivariant_usage(&declared, &effective))
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
        // recognised, while leaving direct applications untouched. A side
        // whose application identity was erased by checker evaluation is
        // recovered through its display/eval-origin provenance channels —
        // tsc's relation never loses the reference (`source.target ===
        // target.target` still holds for instantiated references), so an
        // evaluated instantiation must still drill its type argument here
        // instead of falling into the property walk.
        let resolved_source = self.resolve_lazy_type(source);
        let resolved_target = self.resolve_lazy_type(target);
        let mut s_ty = if application_id(self.interner, resolved_source).is_some() {
            resolved_source
        } else {
            self.explain_provenance_application(resolved_source)?
        };
        let mut t_ty = if application_id(self.interner, resolved_target).is_some() {
            resolved_target
        } else {
            self.explain_provenance_application(resolved_target)?
        };

        let same_base = |checker: &Self, s: TypeId, t: TypeId| {
            let s_app = application_id(checker.interner, s);
            let t_app = application_id(checker.interner, t);
            match (s_app, t_app) {
                (Some(s_app), Some(t_app)) => {
                    checker.interner.type_application(s_app).base
                        == checker.interner.type_application(t_app).base
                }
                _ => false,
            }
        };
        if !same_base(self, s_ty, t_ty) {
            (s_ty, t_ty) = self.align_application_bases(s_ty, t_ty)?;
        }
        let s_app_id = application_id(self.interner, s_ty)?;
        let t_app_id = application_id(self.interner, t_ty)?;
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
        // Type-parameter arguments make a variance rejection inconclusive: the
        // expanded structural forms can introduce implicit index signatures
        // (homomorphic mapped types) or conditional-recursion identities that
        // change the outcome. The relation falls through to structural
        // comparison for these, so the explanation must do the same.
        if args_contain_type_parameters(self.interner, &s_app.args) {
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

    /// Application identity for explanation, recovered from the
    /// display-alias / eval-origin provenance channels when checker
    /// evaluation erased the `Application` head.
    ///
    /// Explanation-only: the relation verdict is already decided when this
    /// runs, so recovery can only change which elaboration frames render,
    /// never the outcome. Gated on the two channels agreeing on the
    /// operand's generic identity (#17614): byte-identical twin aliases
    /// intern instantiations to one `TypeId`, and a lone channel can name
    /// the wrong alias for such a shared shape.
    fn explain_provenance_application(&self, ty: TypeId) -> Option<TypeId> {
        let display = self.interner.get_display_alias(ty);
        let origin = self.interner.get_application_eval_origin(ty);
        let display_app = display.and_then(|alias| application_id(self.interner, alias));
        let origin_app = origin.and_then(|origin| application_id(self.interner, origin));
        if !self.recovered_provenance_channels_agree(display_app, origin_app) {
            return None;
        }
        match (display_app, origin_app) {
            (Some(_), _) => display,
            (None, Some(_)) => origin,
            (None, None) => None,
        }
    }

    /// Canonical definition of `ty`'s generic application base, for
    /// explanation-side same-reference matching. Follows the same identity
    /// recovery as [`Self::explain_same_generic_type_arguments`]: a direct
    /// (or lazily aliased) application reads its base; an evaluated
    /// instantiation recovers it through the agreeing provenance channels.
    pub(crate) fn explain_application_base_def(&mut self, ty: TypeId) -> Option<DefId> {
        let resolved = self.resolve_lazy_type(ty);
        let app_ty = if application_id(self.interner, resolved).is_some() {
            resolved
        } else {
            self.explain_provenance_application(resolved)?
        };
        let app_id = application_id(self.interner, app_ty)?;
        let base = self.interner.type_application(app_id).base;
        let def = self.application_base_def_id(base)?;
        Some(self.resolver.canonical_def_id(def))
    }

    /// Substitute an alias application's body once WITHOUT evaluating the
    /// result — [`Self::try_expand_application_type`] evaluates, which
    /// erases the underlying application the base-alignment walk is looking
    /// for (`type Row<P> = RawBuilder<P>`: the walk needs
    /// `RawBuilder<string>`, not its structural expansion).
    fn instantiate_alias_application_body(&mut self, app_id: TypeApplicationId) -> Option<TypeId> {
        use crate::instantiation::instantiate::TypeSubstitution;
        let app = self.interner.type_application(app_id);
        let def_id = self.application_base_def_id(app.base)?;
        let type_params = self.resolver.get_lazy_type_params(def_id)?;
        let body = self.resolver.resolve_lazy(def_id, self.interner)?;
        let substitution = TypeSubstitution::from_args(self.interner, &type_params, &app.args);
        Some(crate::instantiation::instantiate::instantiate_type_cached(
            self.interner,
            self.query_db,
            body,
            &substitution,
        ))
    }

    /// Whether two canonical application-base definitions name the same
    /// generic reference, treating a forwarding alias (`type Row<P> =
    /// RawBuilder<P>`) as its underlying base in either direction — tsc's
    /// `source.target === target.target` holds across alias spellings
    /// because instantiated references keep the underlying interface as
    /// their `target`.
    pub(crate) fn application_base_defs_match(&self, a: DefId, b: DefId) -> bool {
        a == b || self.alias_body_chain_reaches(a, b) || self.alias_body_chain_reaches(b, a)
    }

    /// Hop forwarding-alias application bases (`type Row<P> = RawBuilder<P>`)
    /// until both sides name the same base, so an alias-spelled
    /// instantiation drills its differing type argument exactly like its
    /// underlying reference. Each hop substitutes one alias body
    /// ([`Self::instantiate_alias_application_body`]); a body that is not
    /// itself an application stops that side's walk. Bounded, and only alias
    /// bases hop, so a genuine cross-base pair still declines the argument
    /// drill.
    fn align_application_bases(&mut self, s_ty: TypeId, t_ty: TypeId) -> Option<(TypeId, TypeId)> {
        const MAX_ALIAS_HOPS: usize = 8;
        let base_of = |checker: &Self, ty: TypeId| -> Option<TypeId> {
            let app_id = application_id(checker.interner, ty)?;
            Some(checker.interner.type_application(app_id).base)
        };
        let is_alias_base = |checker: &Self, base: TypeId| -> bool {
            checker.application_base_def_id(base).is_some_and(|def| {
                matches!(
                    checker.resolver.get_def_kind(def),
                    Some(crate::def::DefKind::TypeAlias)
                )
            })
        };
        let mut sides = [s_ty, t_ty];
        for _ in 0..MAX_ALIAS_HOPS {
            let s_base = base_of(self, sides[0])?;
            let t_base = base_of(self, sides[1])?;
            if s_base == t_base {
                return Some((sides[0], sides[1]));
            }
            let mut progressed = false;
            for (idx, base) in [(0, s_base), (1, t_base)] {
                if !is_alias_base(self, base) {
                    continue;
                }
                let ty = sides[idx];
                let app_id = application_id(self.interner, ty)?;
                if let Some(expanded) = self.instantiate_alias_application_body(app_id)
                    && application_id(self.interner, expanded).is_some()
                    && expanded != ty
                {
                    sides[idx] = expanded;
                    progressed = true;
                    break;
                }
            }
            if !progressed {
                return None;
            }
        }
        None
    }
}

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Whether two provenance channels of one provenance-recovered operand
    /// (its display-alias and application-eval-origin applications) agree on
    /// the operand's generic identity, for the #17614 ambiguity guard in the
    /// pre-evaluation variance fast path.
    ///
    /// Channels agree when either is absent, when they are the same
    /// application, or when their bases resolve to the same canonical
    /// definition. A base-definition DISAGREEMENT is still agreement — not a
    /// #17614 twin-alias weld — when one base is a transparent alias whose
    /// registered body reaches the other base through `Lazy`/`Application`
    /// heads (`AsyncR<T> = Promise<OK<T>>`: display names `AsyncR`, eval
    /// origin names the underlying `Promise`; both truthfully describe the
    /// same value, and gating general variance on their mismatch is what
    /// produced the false TS2416 on `any`-instantiated override returns,
    /// issue #17630). Byte-identical twin aliases never satisfy the walk:
    /// each twin's body is its own structural type (mapped/object), not a
    /// reference to the other definition, so the weld stays refused. The
    /// walk is a bounded pure lookup (`get_def_raw_body`), so an
    /// unregistered body simply keeps the pair gated.
    pub(crate) fn recovered_provenance_channels_agree(
        &self,
        a: Option<TypeApplicationId>,
        b: Option<TypeApplicationId>,
    ) -> bool {
        let (Some(a), Some(b)) = (a, b) else {
            return true;
        };
        if a == b {
            return true;
        }
        let canonical_base = |app_id: TypeApplicationId| {
            let app = self.interner.type_application(app_id);
            lazy_def_id(self.interner, app.base).map(|def| self.resolver.canonical_def_id(def))
        };
        match (canonical_base(a), canonical_base(b)) {
            (Some(da), Some(db)) => {
                da == db
                    || self.alias_body_chain_reaches(da, db)
                    || self.alias_body_chain_reaches(db, da)
            }
            _ => false,
        }
    }

    /// Whether following `from`'s registered raw body through
    /// `Lazy`/`Application` heads reaches the definition `to` within a
    /// bounded number of alias hops.
    fn alias_body_chain_reaches(&self, from: DefId, to: DefId) -> bool {
        let mut current = from;
        for _ in 0..8 {
            if current == to {
                return true;
            }
            let Some(body) = self.resolver.get_def_raw_body(current, self.interner) else {
                return false;
            };
            let next = match self.interner.lookup(body) {
                Some(TypeData::Lazy(def)) => def,
                Some(TypeData::Application(body_app_id)) => {
                    let app = self.interner.type_application(body_app_id);
                    match lazy_def_id(self.interner, app.base) {
                        Some(def) => def,
                        None => return false,
                    }
                }
                _ => return false,
            };
            current = self.resolver.canonical_def_id(next);
        }
        false
    }
}
