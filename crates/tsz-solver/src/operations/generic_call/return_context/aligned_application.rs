//! Same-base aligned-application recovery for return-context inference.
//!
//! `tsc` treats aliases as transparent and admits a foreign outer-scope type
//! argument in an aligned same-base return application. These helpers recover a
//! type's canonical `(base, args)` application view (through display-alias
//! back-references and a single evaluation), decide whether two application
//! bases name the same declaration, and drive the aligned/combined bindings the
//! guarded per-arm probing in the parent module must not. Split out of
//! `return_context.rs` to keep that file under the source-size ceiling.

use crate::instantiation::instantiate::TypeSubstitution;
use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::types::{TypeData, TypeId};
use rustc_hash::FxHashSet;

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    fn return_context_application_base_def_id(&self, base: TypeId) -> Option<crate::def::DefId> {
        let resolver = self
            .checker
            .type_resolver()
            .unwrap_or_else(|| self.interner.as_type_resolver());
        match self.interner.lookup(base)? {
            TypeData::Lazy(def_id) => Some(resolver.canonical_def_id(def_id)),
            TypeData::TypeQuery(symbol) => resolver
                .symbol_to_def_id(symbol)
                .map(|def_id| resolver.canonical_def_id(def_id)),
            _ => None,
        }
    }

    fn return_context_application_bases_match(&self, source: TypeId, target: TypeId) -> bool {
        if source == target {
            return true;
        }

        let Some(source_def) = self.return_context_application_base_def_id(source) else {
            return false;
        };
        let Some(target_def) = self.return_context_application_base_def_id(target) else {
            return false;
        };
        let resolver = self
            .checker
            .type_resolver()
            .unwrap_or_else(|| self.interner.as_type_resolver());
        resolver.defs_are_equivalent(source_def, target_def)
    }

    /// Recover the canonical `Application(base, args)` for a type used during
    /// return-context inference.
    ///
    /// A contextual return type such as `GenericClass<[string, boolean]>` can
    /// exist in the interner in two shapes: the as-written
    /// `Application(GenericClass, [[string, boolean]])` and the *baked*
    /// (already-evaluated) structural object that merely displays as
    /// `GenericClass<[string, boolean]>`. The baked form has no
    /// `TypeData::Application`, so the application-aware matchers cannot
    /// decompose it and any tracked type parameter on the source side is left
    /// unbound — the inner generic call's own type parameter then falls back to
    /// its declared constraint (e.g. `T := {}`), spuriously rejecting a
    /// deferred callback argument.
    ///
    /// The evaluator records a display-alias back-reference from the baked form
    /// to its originating application, so consult it (and, as a last resort, a
    /// fresh evaluation) to restore the structural decomposition through the
    /// validated `Application`↔`Application` path instead of relying on rendered
    /// type text. This mirrors the checker-side
    /// `return_context_application_info` so both return-context implementations
    /// decompose the same baked contextual shapes.
    fn return_context_application_info(
        &mut self,
        type_id: TypeId,
    ) -> Option<(TypeId, Vec<TypeId>)> {
        if let Some(info) = self.app_info_or_alias(type_id) {
            return Some(info);
        }
        let evaluated = self.evaluate_return_context_match_type(type_id);
        if evaluated == type_id {
            return None;
        }
        self.app_info_or_alias(evaluated)
    }

    /// The non-evaluating half of `return_context_application_info`: take the
    /// application form directly, else through a single display-alias hop. A
    /// caller that already holds the evaluated form pairs a raw call with an
    /// `app_info_or_alias(eval)` call to skip the redundant re-evaluation.
    pub(super) fn app_info_or_alias(&self, type_id: TypeId) -> Option<(TypeId, Vec<TypeId>)> {
        let db = self.interner.as_type_database();
        crate::type_queries::get_application_info(db, type_id).or_else(|| {
            self.interner
                .get_display_alias(type_id)
                .and_then(|alias| crate::type_queries::get_application_info(db, alias))
        })
    }

    /// Match direct, same-base return applications before any structural
    /// expansion can expose foreign type parameters from nested members.
    ///
    /// For `G<TCall>` against the contextual `G<X>`, `X` is the whole aligned
    /// return argument. It may legitimately contain free parameters from the
    /// enclosing declaration, so the ordinary nested/member contamination
    /// guard must not reject it. Other tracked call parameters remain blocked,
    /// and nested structural matching continues through the guarded fallback.
    pub(super) fn collect_aligned_return_application_substitution(
        &mut self,
        source: TypeId,
        target: TypeId,
        tracked_type_params: &FxHashSet<tsz_common::Atom>,
        substitution: &mut TypeSubstitution,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
    ) -> bool {
        let Some((source_base, source_args)) = self.return_context_application_info(source) else {
            return false;
        };
        let Some((target_base, target_args)) = self.return_context_application_info(target) else {
            return false;
        };
        if source_args.len() != target_args.len()
            || !self.return_context_application_bases_match(source_base, target_base)
        {
            return false;
        }

        let has_aligned_tracked_param = source_args.iter().any(|&source_arg| {
            matches!(
                self.interner.lookup(source_arg),
                Some(TypeData::TypeParameter(tp))
                    if substitution.domain_contains_type_parameter(&tp, tracked_type_params)
            )
        });
        if !has_aligned_tracked_param {
            return false;
        }

        for (&source_arg, &target_arg) in source_args.iter().zip(&target_args) {
            if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(source_arg)
                && substitution.domain_contains_type_parameter(&tp, tracked_type_params)
            {
                if substitution.get(tp.name).is_none()
                    && !target_arg.is_any_unknown_or_error()
                    && !self.type_references_other_tracked_params(
                        target_arg,
                        &tp,
                        tracked_type_params,
                        substitution,
                    )
                {
                    substitution.insert(tp.name, target_arg);
                }
                continue;
            }

            self.collect_return_context_substitution(
                source_arg,
                target_arg,
                tracked_type_params,
                substitution,
                visited,
            );
        }
        true
    }

    /// Combine an ambiguous same-base union contextual return into the single
    /// merged application `Base<a0|b0, a1|b1, ..>` that `tsc` infers the tag's
    /// tracked parameters from (`PriorityImpliesCombination`; see the call site
    /// in [`collect_return_context_substitution`](Self::collect_return_context_substitution)).
    ///
    /// Returns the merged application when `source` is an application `Base<..>`
    /// and every non-nullish arm of `target` is an application of the same base
    /// declaration with matching arity — otherwise `None`, leaving a mixed-base
    /// or non-application union to the ordinary per-arm agreement probing. Arm
    /// applications are recovered through the same alias-transparent view
    /// (`return_context_application_info`) the direct aligned path uses, so an
    /// alias arm (`type StrRow = RawBuilder<string>`) decomposes like its
    /// underlying application.
    pub(super) fn ambiguous_same_base_union_merged_target(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<TypeId> {
        let (source_base, source_args) = self.return_context_application_info(source)?;
        let members =
            crate::type_queries::get_union_members(self.interner.as_type_database(), target)?;
        let mut per_position: Vec<Vec<TypeId>> = vec![Vec::new(); source_args.len()];
        let mut arm_count = 0usize;
        for member in members {
            if member == TypeId::NULL || member == TypeId::UNDEFINED {
                continue;
            }
            let (arm_base, arm_args) = self.return_context_application_info(member)?;
            if arm_args.len() != source_args.len()
                || !self.return_context_application_bases_match(source_base, arm_base)
            {
                return None;
            }
            for (slot, &arg) in per_position.iter_mut().zip(arm_args.iter()) {
                if !slot.contains(&arg) {
                    slot.push(arg);
                }
            }
            arm_count += 1;
        }
        if arm_count == 0 {
            return None;
        }
        let merged_args: Vec<TypeId> = per_position
            .iter()
            .map(|args| self.interner.union_from_slice(args))
            .collect();
        Some(self.interner.application(source_base, merged_args))
    }
}
