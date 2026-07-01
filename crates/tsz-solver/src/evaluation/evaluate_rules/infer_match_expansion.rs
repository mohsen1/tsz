//! Infer-pattern application expansion and cross-evaluator depth ownership.

use crate::def::DefId;
use crate::evaluation::result::EvaluationMemoResult;
use crate::evaluation::session::EvaluationSession;
use crate::relations::subtype::{SubtypeChecker, TypeResolver};
use crate::types::{TypeApplication, TypeData, TypeId};
use rustc_hash::FxHashMap;
use tsz_common::interner::Atom;

use super::super::evaluate::TypeEvaluator;
use super::infer_pattern::InferPatternVisited;

impl<R: TypeResolver> TypeEvaluator<'_, R> {
    /// Maximum iterations for alias-application reduction loops.
    /// Bounds peel/reduce walks against pathological alias chains.
    pub(crate) const MAX_ALIAS_REDUCTION_STEPS: u32 = 8;

    /// Resolve an application/reference base (`Lazy(DefId)` or
    /// `TypeQuery(SymbolRef)`) to its defining [`DefId`]. Returns `None` for any
    /// other base shape, or a `TypeQuery` whose symbol has no `DefId` yet.
    pub(crate) fn application_base_def_id(&self, base: TypeId) -> Option<DefId> {
        match self.interner().lookup(base)? {
            TypeData::Lazy(def_id) => Some(def_id),
            TypeData::TypeQuery(sym_ref) => self.resolver().symbol_to_def_id(sym_ref),
            _ => None,
        }
    }

    /// Decode `Application(Lazy(DefId)/TypeQuery, args)` and substitute the
    /// alias's type-parameter args into its resolved body. Returns `None`
    /// when the base isn't a resolvable `DefId`, arities disagree, or the
    /// substitution is a no-op.
    pub(crate) fn alias_application_substituted_body(&self, ty: TypeId) -> Option<TypeId> {
        let Some(TypeData::Application(app_id)) = self.interner().lookup(ty) else {
            return None;
        };
        let app = self.interner().type_application(app_id);
        let def_id = self.application_base_def_id(app.base)?;
        let type_params = self.resolver().get_lazy_type_params(def_id)?;
        if type_params.len() != app.args.len() {
            return None;
        }
        let body = self.resolver().resolve_lazy(def_id, self.interner())?;
        let substituted = crate::instantiation::instantiate::instantiate_generic_cached(
            self.interner(),
            self.query_db(),
            body,
            &type_params,
            &app.args,
        );
        (substituted != ty).then_some(substituted)
    }

    /// Peel one alias layer off an application whose body is another
    /// application. We do not gate on `get_def_kind`: zombie `DefId` values from
    /// `interner.reference` are not tagged with `DefKind` in the definition
    /// store, but the application-vs-structural body shape is the reliable
    /// signal.
    pub(crate) fn peel_alias_application(&self, ty: TypeId) -> Option<TypeId> {
        let substituted = self.alias_application_substituted_body(ty)?;
        matches!(
            self.interner().lookup(substituted),
            Some(TypeData::Application(_))
        )
        .then_some(substituted)
    }

    /// Whether `base` names a generic wrapper alias whose resolved body is
    /// itself an `Application`.
    pub(crate) fn is_wrapper_alias_base(&self, base: TypeId) -> bool {
        let Some(def_id) = self.application_base_def_id(base) else {
            return false;
        };
        self.resolver()
            .resolve_lazy(def_id, self.interner())
            .is_some_and(|body| {
                matches!(self.interner().lookup(body), Some(TypeData::Application(_)))
            })
    }

    /// Recover an `Application` form from a non-`Application` type via the
    /// global display-alias map.
    pub(crate) fn try_recover_application_from_display_alias(&self, ty: TypeId) -> Option<TypeId> {
        if matches!(self.interner().lookup(ty), Some(TypeData::Application(_))) {
            return None;
        }
        let alias = self.interner().get_display_alias(ty)?;
        (alias != ty
            && matches!(
                self.interner().lookup(alias),
                Some(TypeData::Application(_))
            ))
        .then_some(alias)
    }

    /// Try to match a source application's type args against a pattern
    /// application's args.
    ///
    /// Returns `Some(true)` if all args matched, `Some(false)` if bases matched
    /// but an arg failed, `None` if the bases are incompatible and the caller
    /// should try another candidate.
    fn try_match_application_args_to_pattern(
        &self,
        source: &TypeApplication,
        pattern: &TypeApplication,
        pattern_base_is_wrapper_alias: bool,
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut InferPatternVisited,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> Option<bool> {
        if source.args.len() != pattern.args.len() {
            return None;
        }
        if source.base != pattern.base {
            if pattern_base_is_wrapper_alias {
                return None;
            }
            if !checker.is_subtype_of(source.base, pattern.base) {
                return None;
            }
        }
        for (source_arg, pattern_arg) in source.args.iter().zip(pattern.args.iter()) {
            if !self.match_infer_pattern(*source_arg, *pattern_arg, bindings, visited, checker) {
                return Some(false);
            }
        }
        Some(true)
    }

    pub(crate) fn match_application_infer_pattern(
        &self,
        source: TypeId,
        pattern: TypeId,
        pattern_app_id: crate::types::TypeApplicationId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut InferPatternVisited,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let pattern_app = self.interner().type_application(pattern_app_id);
        if pattern_app.args.len() == 1
            && let Some(TypeData::Lazy(def_id)) = self.interner().lookup(pattern_app.base)
            && self.resolver().is_builtin_readonly_array_def(def_id)
            && let Some(source_elem) =
                crate::type_queries::get_array_element_type(self.interner(), source)
        {
            return self.match_infer_pattern(
                source_elem,
                pattern_app.args[0],
                bindings,
                visited,
                checker,
            );
        }

        let pattern_base_is_wrapper_alias = self.is_wrapper_alias_base(pattern_app.base);
        if pattern_base_is_wrapper_alias
            && let Some(reduced_pattern) = self.peel_alias_application(pattern)
        {
            let mut reduced_bindings = bindings.clone();
            let reduced_checkpoint = visited.checkpoint();
            if self.match_infer_pattern(
                source,
                reduced_pattern,
                &mut reduced_bindings,
                visited,
                checker,
            ) {
                *bindings = reduced_bindings;
                return true;
            }
            visited.rollback_to(reduced_checkpoint);
        }

        let mut current_source = source;
        for _ in 0..Self::MAX_ALIAS_REDUCTION_STEPS {
            if let Some(TypeData::Application(source_app_id)) =
                self.interner().lookup(current_source)
            {
                let source_app = self.interner().type_application(source_app_id);
                if let Some(result) = self.try_match_application_args_to_pattern(
                    &source_app,
                    &pattern_app,
                    pattern_base_is_wrapper_alias,
                    bindings,
                    visited,
                    checker,
                ) {
                    return result;
                }
                if source_app.args.len() == pattern_app.args.len() && !pattern_base_is_wrapper_alias
                {
                    let candidate_pattern = self
                        .interner()
                        .application(pattern_app.base, source_app.args.clone());
                    if checker.is_subtype_of(current_source, candidate_pattern) {
                        for (source_arg, pattern_arg) in
                            source_app.args.iter().zip(pattern_app.args.iter())
                        {
                            if !self.match_infer_pattern(
                                *source_arg,
                                *pattern_arg,
                                bindings,
                                visited,
                                checker,
                            ) {
                                return false;
                            }
                        }
                        return true;
                    }
                }
            }
            let Some(peeled) = self.peel_alias_application(current_source) else {
                break;
            };
            current_source = peeled;
        }

        if let Some(recovered) = self.try_recover_application_from_display_alias(source)
            && let Some(TypeData::Application(recovered_app_id)) = self.interner().lookup(recovered)
        {
            let recovered_app = self.interner().type_application(recovered_app_id);
            if let Some(result) = self.try_match_application_args_to_pattern(
                &recovered_app,
                &pattern_app,
                pattern_base_is_wrapper_alias,
                bindings,
                visited,
                checker,
            ) {
                return result;
            }
        }

        if let Some(reduced_pattern) = self.alias_application_substituted_body(pattern)
            && reduced_pattern != pattern
            && matches!(
                self.interner().lookup(reduced_pattern),
                Some(TypeData::Application(_))
            )
        {
            let mut reduced_bindings = bindings.clone();
            let reduced_checkpoint = visited.checkpoint();
            if self.match_infer_pattern(
                source,
                reduced_pattern,
                &mut reduced_bindings,
                visited,
                checker,
            ) && reduced_bindings.len() >= bindings.len()
            {
                *bindings = reduced_bindings;
                return true;
            }
            visited.rollback_to(reduced_checkpoint);
        }

        let expanded_pattern = self.evaluate_for_infer_match(pattern);
        if expanded_pattern != pattern {
            if let Some(alias) = self.interner().get_display_alias(source)
                && alias != source
            {
                if visited.contains(&(alias, expanded_pattern)) {
                    return true;
                }
                let mut alias_bindings = bindings.clone();
                let alias_checkpoint = visited.checkpoint();
                if self.match_infer_pattern(
                    alias,
                    expanded_pattern,
                    &mut alias_bindings,
                    visited,
                    checker,
                ) {
                    visited.rollback_to(alias_checkpoint);
                    *bindings = alias_bindings;
                    return true;
                }
                visited.rollback_to(alias_checkpoint);
            }
            return self.match_infer_pattern(source, expanded_pattern, bindings, visited, checker);
        }

        false
    }

    /// Evaluate `type_id` in a fresh sub-evaluator during infer-pattern
    /// matching, bounded by the owning evaluation session's cross-evaluator
    /// expansion depth.
    pub(crate) fn evaluate_for_infer_match(&self, type_id: TypeId) -> TypeId {
        if let Some(session) = self.evaluation_session() {
            return self.evaluate_for_infer_match_with_session(type_id, session);
        }
        crate::evaluation::session::with_current_session(|session| {
            self.evaluate_for_infer_match_with_session(type_id, session)
        })
    }

    fn evaluate_for_infer_match_with_session(
        &self,
        type_id: TypeId,
        session: &EvaluationSession,
    ) -> TypeId {
        let nuia = self.no_unchecked_indexed_access();
        crate::evaluation::cross_eval_guard::memoized_eval(session, type_id, nuia, || {
            let Ok(_entry) = session.enter_infer_match_expansion_depth() else {
                return EvaluationMemoResult::unstable_complete(type_id);
            };
            let mut evaluator = TypeEvaluator::with_resolver(self.interner(), self.resolver())
                .with_evaluation_session(session);
            if let Some(query_db) = self.query_db() {
                evaluator = evaluator.with_query_db(query_db);
            }
            let request = crate::evaluation::request::EvaluationRequest::new(type_id)
                .with_no_unchecked_indexed_access(nuia);
            evaluator.evaluate_request_memo_result(request)
        })
        .unwrap_or(type_id)
    }
}

#[cfg(test)]
mod tests {
    use crate::evaluation::session::{
        EvaluationSession, InferMatchExpansionDepthEntry, InferMatchExpansionDepthState,
    };

    #[test]
    fn infer_match_expansion_state_limits_at_budget() {
        let session = EvaluationSession::new();
        let mut held = Vec::new();
        for expected_prev in 0..InferMatchExpansionDepthEntry::limit() {
            let entry = session
                .enter_infer_match_expansion_depth()
                .expect("enter within budget must succeed");
            assert_eq!(entry.prior_depth(), expected_prev);
            held.push(entry);
            assert_eq!(session.infer_match_expansion_depth(), expected_prev + 1);
        }

        assert!(
            matches!(
                session.enter_infer_match_expansion_depth(),
                Err(InferMatchExpansionDepthState::LimitExceeded)
            ),
            "enter at the budget must be denied so the caller stops expanding"
        );

        held.clear();
        assert_eq!(session.infer_match_expansion_depth(), 0);
        assert!(
            session.enter_infer_match_expansion_depth().is_ok(),
            "after unwinding, a fresh expansion must be allowed again"
        );
    }

    #[test]
    fn infer_match_expansion_depth_is_session_local() {
        let saturated = EvaluationSession::new();
        let separate = EvaluationSession::new();
        let mut held = Vec::new();
        for _ in 0..InferMatchExpansionDepthEntry::limit() {
            held.push(
                saturated
                    .enter_infer_match_expansion_depth()
                    .expect("saturating session enters"),
            );
        }

        assert!(matches!(
            saturated.enter_infer_match_expansion_depth(),
            Err(InferMatchExpansionDepthState::LimitExceeded)
        ));
        assert!(
            separate.enter_infer_match_expansion_depth().is_ok(),
            "a saturated infer-match depth in one session must not block another session"
        );
    }
}
