//! Return context substitution methods for generic call inference.

use crate::inference::infer::InferenceContext;
use crate::inference::infer::InferenceVar;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::types::{FunctionShape, ParamInfo, TupleElement, TypeData, TypeId, TypeParamInfo};

use super::{GenericCallRequest, GenericCallResult};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;

// Reusable scratch `FxHashSet<TypeId>` for the three DFS walkers in this
// module. Mirrors the pool pattern from #4722 / #4790 / #4801 / #4805 /
// #4807 / #4810 / #4816 / #4818.
thread_local! {
    static RETURN_CONTEXT_VISITED_POOL: RefCell<Option<FxHashSet<TypeId>>> =
        const { RefCell::new(None) };
}

/// #14345 HKT-Application unknown-drop flag (default-OFF, reuses
/// `TSZ_TYPEPARAM_DECL_IDENTITY`). When OFF the `target_contains_untracked`
/// relaxation below is never applied, so the return-context substitution is
/// byte-identical to pre-#14345. Under the construction stamp a generic call's
/// own type param (e.g. `Functor.map<A, B>`'s `B`, placeholder-renamed) and the
/// OUTER-scope param it should bind to (e.g. the `B` of an enclosing
/// `flap<F>(): <A>(a) => <B>(...) => HKT<F, B>`) intern to DISTINCT `DeclScoped`
/// ids, so the call return `HKT<F, B_call>` and the contextual `HKT<F, B_outer>`
/// are distinct Applications whose arg-by-arg match reaches `B_call` (a tracked
/// placeholder) vs `B_outer` (a bare outer param). Flag-OFF those two `B`s are
/// the SAME structural id, so the call return == the contextual type and the
/// substitution binds `B` trivially; flag-ON the bind is blocked by the
/// untracked-target guard and `B_call` collapses to `unknown` (`HKT<F, unknown>`).
fn hkt_application_unknown_drop_fix_enabled() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("TSZ_TYPEPARAM_DECL_IDENTITY").is_ok_and(|v| v == "1"))
}

#[inline]
fn with_return_context_visited<R>(f: impl FnOnce(&mut FxHashSet<TypeId>) -> R) -> R {
    let mut visited = RETURN_CONTEXT_VISITED_POOL
        .with(|p| p.borrow_mut().take())
        .unwrap_or_default();
    visited.clear();
    let r = f(&mut visited);
    RETURN_CONTEXT_VISITED_POOL.with(|p| {
        let mut slot = p.borrow_mut();
        let keep = match &*slot {
            None => true,
            Some(existing) => visited.capacity() >= existing.capacity(),
        };
        if keep {
            *slot = Some(visited);
        }
    });
    r
}

#[inline]
fn sort_type_params_by_name(type_params: &mut [TypeParamInfo]) {
    type_params.sort_unstable_by_key(|type_param| type_param.name);
}

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    pub(super) fn hoist_resolved_type_params_into_return_type(
        &self,
        func: &FunctionShape,
        final_subst: &TypeSubstitution,
        return_type: TypeId,
    ) -> TypeId {
        let Some(TypeData::Function(shape_id)) = self.interner.lookup(return_type) else {
            return return_type;
        };

        let mut shape = self.interner.function_shape(shape_id).as_ref().clone();
        if !shape.type_params.is_empty() {
            return return_type;
        }

        let mut hoisted = Vec::new();
        let mut seen = FxHashSet::default();
        for tp in &func.type_params {
            let Some(resolved) = final_subst.get(tp.name) else {
                continue;
            };
            let Some(TypeData::TypeParameter(info)) = self.interner.lookup(resolved) else {
                continue;
            };
            // Re-generalize in exactly two cases. (1) The call type parameter
            // resolved to a synthetic inference placeholder — a higher-order
            // source parameter (`__infer_src_*`) minted for a generic function
            // argument, or a call-local inference variable. (2) The callee's own
            // type parameter stayed UNRESOLVED (identity substitution — the
            // context-sensitive-argument case, e.g. `arrayFilter(x => …)`
            // against a generic contextual signature): tsc keeps the call
            // generic, so the result re-quantifies the callee's own parameter.
            // When it instead resolved to a *free enclosing* type parameter,
            // tsc keeps it as a free reference in the result (`() => T`) rather
            // than quantifying it into a fresh signature (`<T>() => T`). Exact
            // declaration identity distinguishes that captured binder even when
            // it has the same spelling; unstamped parameters retain the legacy
            // name-keyed fallback.
            if !info.is_infer_placeholder() && !tp.is_same_binder(info) {
                continue;
            }
            if seen.insert(info.name)
                && crate::visitor::contains_type_parameter_binder(
                    self.interner.as_type_database(),
                    return_type,
                    info,
                )
            {
                hoisted.push(info);
            }
        }

        if hoisted.is_empty() {
            return return_type;
        }

        shape.type_params = hoisted;
        self.interner.function(shape)
    }

    /// Re-generalize a higher-order inference result.
    ///
    /// When a generic function argument's free type parameters survive into the
    /// call result as `__infer_src_*` placeholders (TypeScript 3.4 higher-order
    /// function type inference), turn them back into proper type parameters of
    /// the resulting function signature. Each surviving placeholder is renamed
    /// to its original source type-parameter name (recorded in the placeholder
    /// atom), so the result displays as tsc's `<T>(a: T) => { value: T[] }`
    /// rather than leaking the internal `__infer_src_*` name. Collisions are
    /// disambiguated with a numeric suffix.
    pub(super) fn hoist_source_placeholders_into_return_type(&self, return_type: TypeId) -> TypeId {
        let Some(TypeData::Function(shape_id)) = self.interner.lookup(return_type) else {
            return return_type;
        };

        let mut shape = self.interner.function_shape(shape_id).as_ref().clone();
        if !shape.type_params.is_empty() {
            return return_type;
        }

        let mut placeholders: Vec<TypeParamInfo> = Vec::new();
        let mut seen = FxHashSet::default();
        for referenced in
            crate::visitor::collect_all_types(self.interner.as_type_database(), return_type)
        {
            let Some(TypeData::TypeParameter(info)) = self.interner.lookup(referenced) else {
                continue;
            };
            if !info.is_infer_source() {
                continue;
            }
            if seen.insert(info.name) {
                placeholders.push(info);
            }
        }

        if placeholders.is_empty() {
            return return_type;
        }
        // Deterministic ordering: placeholder atoms are allocated in source
        // order, so sorting by name keeps the re-generalized parameter list
        // stable across runs.
        sort_type_params_by_name(&mut placeholders);

        let mut rename = TypeSubstitution::new();
        let mut hoisted = Vec::with_capacity(placeholders.len());
        let mut used_names: FxHashSet<tsz_common::Atom> = FxHashSet::default();
        for info in &placeholders {
            // The source placeholder records its origin type-parameter name as a
            // structured field; legacy `__infer_src_ctx_*` placeholders carry none
            // and fall back to `T`, matching the previous decode behaviour.
            let origin_atom = info.origin.infer_source_origin_name();
            let mut display_atom = origin_atom.unwrap_or_else(|| self.interner.intern_string("T"));
            let origin = self.interner.resolve_atom_ref(display_atom).to_string();
            let mut suffix = 1u32;
            while !used_names.insert(display_atom) {
                let candidate = format!("{origin}_{suffix}");
                display_atom = self.interner.intern_string(&candidate);
                suffix += 1;
            }
            // Re-generalized into a real (user-facing) type parameter; the
            // placeholder origin is intentionally dropped.
            let renamed = TypeParamInfo {
                name: display_atom,
                origin: crate::types::TypeParamOrigin::User,
                ..*info
            };
            let renamed_id = self.interner.type_param(renamed);
            rename.insert(info.name, renamed_id);
            hoisted.push(renamed);
        }

        // Rewrite the body so the placeholders read as their renamed
        // parameters. The signature itself was non-generic until now, so there
        // are no bound parameters that could shadow the rename.
        shape.params = shape
            .params
            .iter()
            .map(|param| ParamInfo {
                suppress_display_optional: false,
                name: param.name,
                type_id: instantiate_type(self.interner, param.type_id, &rename),
                optional: param.optional,
                rest: param.rest,
            })
            .collect();
        shape.return_type = instantiate_type(self.interner, shape.return_type, &rename);
        shape.this_type = shape
            .this_type
            .map(|this_type| instantiate_type(self.interner, this_type, &rename));
        shape.type_params = hoisted;
        self.interner.function(shape)
    }

    pub(super) fn normalize_function_shape_params_for_context(
        &self,
        shape: &FunctionShape,
    ) -> FunctionShape {
        use crate::type_queries::unpack_tuple_rest_parameter;

        let mut normalized = shape.clone();
        normalized.params = shape
            .params
            .iter()
            .flat_map(|param| unpack_tuple_rest_parameter(self.interner, param))
            .collect();
        normalized
    }

    fn get_overloaded_source_signature_for_arity(
        db: &dyn crate::construction::TypeDatabase,
        type_id: TypeId,
        arg_count: usize,
        prefer_construct: bool,
    ) -> Option<FunctionShape> {
        let call_signatures = || {
            crate::type_queries::get_call_signatures(db, type_id)
                .filter(|signatures| !signatures.is_empty())
                .map(|signatures| (signatures, false))
        };
        let construct_signatures = || {
            crate::type_queries::get_construct_signatures(db, type_id)
                .filter(|signatures| !signatures.is_empty())
                .map(|signatures| (signatures, true))
        };
        let (signatures, is_constructor) = if prefer_construct {
            construct_signatures().or_else(call_signatures)
        } else {
            call_signatures().or_else(construct_signatures)
        }?;
        let signature_accepts_arg_count = |params: &[crate::types::ParamInfo], count: usize| {
            let required_count = params.iter().filter(|p| !p.optional).count();
            let has_rest = params.iter().any(|p| p.rest);
            if has_rest {
                count >= required_count
            } else {
                count >= required_count && count <= params.len()
            }
        };
        let sig = signatures
            .iter()
            .rev()
            .find(|sig| signature_accepts_arg_count(&sig.params, arg_count))
            .or_else(|| signatures.last())?;
        Some(FunctionShape {
            type_params: sig.type_params.clone(),
            params: sig.params.clone(),
            this_type: sig.this_type,
            return_type: sig.return_type,
            type_predicate: sig.type_predicate,
            is_constructor,
            is_method: sig.is_method,
        })
    }

    pub(super) fn get_source_signature_for_target(
        db: &dyn crate::construction::TypeDatabase,
        source_type: TypeId,
        target_type: TypeId,
    ) -> Option<(FunctionShape, FunctionShape)> {
        let target_fn = Self::get_contextual_signature(db, target_type)?;
        let source_fn = Self::get_overloaded_source_signature_for_arity(
            db,
            source_type,
            target_fn.params.len(),
            target_fn.is_constructor,
        )
        .or_else(|| Self::get_contextual_signature(db, source_type))?;
        Some((source_fn, target_fn))
    }

    pub(super) fn should_use_contextual_return_substitution(
        &mut self,
        inferred: TypeId,
        contextual: TypeId,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
    ) -> bool {
        if inferred.is_any_unknown_or_error() {
            return true;
        }

        // Only check for inference placeholders from the CURRENT generic call,
        // not outer-scope type parameters. Outer-scope type parameters (e.g., `U`
        // from an enclosing `function test<U>(...)`) are concrete in this context
        // and should not trigger the contextual return substitution override.
        let contains_placeholder = with_return_context_visited(|visited| {
            self.type_contains_placeholder(inferred, var_map, visited)
        });
        if contains_placeholder
            || crate::type_queries::contains_infer_types_db(
                self.interner.as_type_database(),
                inferred,
            )
        {
            return true;
        }

        // If the inferred result only reached a broad fallback (typically the
        // declared constraint/default) and the contextual return substitution is
        // strictly narrower, prefer the contextual result. This keeps round-2
        // contextual typing from being discarded for deferred callback arguments.
        if self.checker.is_assignable_to(contextual, inferred)
            && !self.checker.is_assignable_to(inferred, contextual)
        {
            return true;
        }

        false
    }

    pub(super) fn contains_tuple_like_parameter_target(
        db: &dyn crate::construction::TypeDatabase,
        type_id: TypeId,
    ) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        if crate::type_queries::get_tuple_elements(db, type_id).is_some() {
            return true;
        }

        if let Some(members) = crate::type_queries::get_union_members(db, type_id) {
            return members
                .iter()
                .copied()
                .any(|member| Self::contains_tuple_like_parameter_target(db, member));
        }

        if let Some(members) = crate::type_queries::get_intersection_members(db, type_id) {
            return members
                .iter()
                .copied()
                .any(|member| Self::contains_tuple_like_parameter_target(db, member));
        }

        false
    }

    pub(super) fn can_apply_contextual_return_substitution(
        &mut self,
        infer_ctx: &mut InferenceContext<'_>,
        var: InferenceVar,
        inferred: TypeId,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
    ) -> bool {
        let has_non_return_candidates =
            infer_ctx.var_has_candidates(var) && !infer_ctx.all_candidates_are_return_type(var);

        if !has_non_return_candidates {
            return true;
        }

        if inferred.is_any_unknown_or_error() {
            return true;
        }

        // Only check for inference placeholders from the CURRENT generic call,
        // not outer-scope type parameters.
        let contains_placeholder = with_return_context_visited(|visited| {
            self.type_contains_placeholder(inferred, var_map, visited)
        });
        contains_placeholder
            || crate::type_queries::contains_infer_types_db(
                self.interner.as_type_database(),
                inferred,
            )
    }

    fn evaluate_return_context_match_type(&mut self, type_id: TypeId) -> TypeId {
        // #14346 global re-reduce depth budget: the flag-ON resolver evaluation
        // is the per-turn growth site feeding the return-context self-recursion.
        // Guard it on the shared native-depth budget; when exhausted, fall back
        // to the resolver-less interner evaluation (the flag-OFF form).
        if crate::instantiation::instantiate::flags::inst_resolver_rereduce_enabled()
            && let Some(_g) = crate::instantiation::instantiate::flags::rereduce_depth_try_enter()
        {
            let evaluated = self
                .checker
                .evaluate_type_for_return_context_substitution(type_id);
            if evaluated != type_id {
                return evaluated;
            }
        }
        self.interner.evaluate_type(type_id)
    }

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
    fn app_info_or_alias(&self, type_id: TypeId) -> Option<(TypeId, Vec<TypeId>)> {
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
    fn collect_aligned_return_application_substitution(
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

    fn collect_return_context_substitution(
        &mut self,
        source: TypeId,
        target: TypeId,
        tracked_type_params: &FxHashSet<tsz_common::Atom>,
        substitution: &mut TypeSubstitution,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
    ) {
        if !visited.insert((source, target)) {
            return;
        }

        // #14345 (flag-ON only): a target that is a BARE outer-scope `DeclScoped`
        // type parameter is a legitimate contextual binding, not nested-signature
        // contamination. The `target_contains_untracked` guard keys on the
        // tracked set (the call's placeholder-renamed param names), so an
        // outer-scope param like the `B` of an enclosing `flap` reads as
        // "untracked" and is rejected — leaving the call's own `B` (`source`, a
        // tracked placeholder) with no binding, so it collapses to `unknown`
        // inside the HKT Application (`HKT<F, unknown>`). Binding the placeholder
        // to that single bare outer param recovers the flag-OFF identity (where
        // both `B`s share one structural id and the bind is trivial) without
        // relating distinct decls: it is a direct 1:1 param-to-param binding with
        // no surrounding structure to contaminate. Restricted to a BARE target
        // param so structured targets (Applications/unions that could carry a
        // genuine nested-signature contaminant) keep the original guard.
        let target_is_bare_outer_param = hkt_application_unknown_drop_fix_enabled()
            && matches!(
                self.interner.lookup(target),
                Some(TypeData::TypeParameter(t))
                    if !substitution
                        .domain_contains_type_parameter(&t, tracked_type_params)
            );
        if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(source)
            && substitution.domain_contains_type_parameter(&tp, tracked_type_params)
            && target != TypeId::UNKNOWN
            && target != TypeId::ERROR
            && substitution.get(tp.name).is_none()
            // Don't insert if target contains untracked type parameters from
            // nested generic signatures (e.g., Promise.catch's TResult parameter
            // when matching through .then()). These would contaminate inference.
            // A bare outer-scope param target is exempt (flag-ON): it is the
            // contextual binding itself, not a nested contaminant.
            && (target_is_bare_outer_param
                || !self.target_contains_untracked_type_params(
                    target,
                    tracked_type_params,
                    substitution,
                ))
            // Don't insert if target contains OTHER tracked type parameters.
            // This prevents incorrect mappings when both TResult1 and TResult2
            // from a source union would be mapped to the same target that
            // references both of them.
            && !self.type_references_other_tracked_params(
                target,
                &tp,
                tracked_type_params,
                substitution,
            )
        {
            substitution.insert(tp.name, target);
            return;
        }

        // Source union decomposition: when the source return type is a union
        // of simple type parameters (like TResult1 | TResult2), decompose it
        // and match each member against the target. This is essential for
        // matching Application type args (e.g., Promise<TResult1 | TResult2>
        // vs Promise<DooDad>).
        // Guard: only decompose when ALL non-nullish members are tracked type
        // parameters. Complex unions (containing conditionals, applications,
        // etc.) should not be decomposed as the individual members lack the
        // context needed for correct matching.
        if let Some(source_members) =
            crate::type_queries::get_union_members(self.interner.as_type_database(), source)
        {
            let non_nullish: Vec<TypeId> = source_members
                .into_iter()
                .filter(|member| *member != TypeId::NULL && *member != TypeId::UNDEFINED)
                .collect();
            let all_tracked_type_params = !non_nullish.is_empty()
                && non_nullish.iter().all(|&member| {
                    if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(member) {
                        substitution.domain_contains_type_parameter(&tp, tracked_type_params)
                    } else {
                        false
                    }
                });
            if all_tracked_type_params {
                for &member in &non_nullish {
                    self.collect_return_context_substitution(
                        member,
                        target,
                        tracked_type_params,
                        substitution,
                        visited,
                    );
                }
                if !substitution.is_empty() {
                    return;
                }
            }
        }

        if let Some(target_members) =
            crate::type_queries::get_union_members(self.interner.as_type_database(), target)
        {
            let before_len = substitution.len();
            // Probe *every* non-nullish union arm and only bind a tracked
            // parameter from the return context when the arms agree on a single
            // value — rather than stopping at the first arm that produces a
            // binding.
            //
            // A nested generic call whose signature return is `U[]` checked
            // against a contextual union like `string[] | string[][]` matches
            // both arms but binds `U` differently (`U := string` from the
            // `string[]` arm, `U := string[]` from the `string[][]` arm). Taking
            // only the first arm pinned `U := string`, which contextually typed
            // the callback's return as `string` and spuriously rejected its body
            // (and, for a `U | U[]` callback target, leaked the outer type
            // parameter into the result — issue #14731). The arms are genuinely
            // ambiguous, so the return context must not pin `U`; leaving it
            // unbound lets argument inference (the callback body) decide it, as
            // `tsc` does. When every contributing arm agrees on the same value,
            // binding it is unambiguous and preserved.
            let mut per_param: FxHashMap<tsz_common::Atom, Vec<TypeId>> = FxHashMap::default();
            let mut param_order: Vec<tsz_common::Atom> = Vec::new();
            for member in target_members
                .into_iter()
                .filter(|member| *member != TypeId::NULL && *member != TypeId::UNDEFINED)
            {
                let mut member_substitution = substitution.empty_with_same_domain();
                let mut member_visited = FxHashSet::default();
                self.collect_return_context_substitution(
                    source,
                    member,
                    tracked_type_params,
                    &mut member_substitution,
                    &mut member_visited,
                );
                for (&name, &member_ty) in member_substitution.map() {
                    let values = per_param.entry(name).or_insert_with(|| {
                        param_order.push(name);
                        Vec::new()
                    });
                    if !values.contains(&member_ty) {
                        values.push(member_ty);
                    }
                }
            }
            for name in param_order {
                // Never override a binding an earlier block already produced,
                // and skip parameters the arms disagree on (genuine ambiguity).
                if substitution.get(name).is_some() {
                    continue;
                }
                let values = &per_param[&name];
                if values.len() == 1 {
                    substitution.insert(name, values[0]);
                }
            }
            if substitution.len() > before_len {
                return;
            }
        }

        if let Some(inner) = match self.interner.lookup(target) {
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => Some(inner),
            _ => None,
        } {
            self.collect_return_context_substitution(
                source,
                inner,
                tracked_type_params,
                substitution,
                visited,
            );
            if !substitution.is_empty() {
                return;
            }
        }

        if let Some(inner) = match self.interner.lookup(source) {
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => Some(inner),
            _ => None,
        } {
            self.collect_return_context_substitution(
                inner,
                target,
                tracked_type_params,
                substitution,
                visited,
            );
            if !substitution.is_empty() {
                return;
            }
        }

        let source_eval = self.evaluate_return_context_match_type(source);
        let target_eval = self.evaluate_return_context_match_type(target);
        let function_info = match (
            Self::get_contextual_signature_cached(self.interner, source),
            Self::get_contextual_signature_cached(self.interner, target),
        ) {
            (Some(source_fn), Some(target_fn)) => Some((source_fn, target_fn)),
            _ => match (
                Self::get_contextual_signature_cached(self.interner, source_eval),
                Self::get_contextual_signature_cached(self.interner, target_eval),
            ) {
                (Some(source_fn), Some(target_fn)) => Some((source_fn, target_fn)),
                _ => None,
            },
        };

        if let Some((source_fn, target_fn)) = function_info
            && (source_fn.params.len() <= target_fn.params.len()
                || source_fn.params.iter().any(|p| p.rest))
        {
            // When the target function is generic (e.g., `<A>(x: A) => Box<A>`),
            // directly insert mappings for source type parameters that appear in
            // parameter or return positions, bypassing the untracked-type-param
            // and references-other-tracked guards. These guards prevent
            // contamination from nested generic signatures, but contextual type
            // parameters like `A` from the variable's type annotation are
            // legitimate targets. Without this, inference variables (__infer_*)
            // leak into final types because e.g. `U -> Box<A>` gets blocked.
            if !target_fn.type_params.is_empty() {
                for (i, source_param) in source_fn.params.iter().enumerate() {
                    if let Some(TypeData::TypeParameter(tp)) =
                        self.interner.lookup(source_param.type_id)
                        && substitution.domain_contains_type_parameter(&tp, tracked_type_params)
                        && substitution.get(tp.name).is_none()
                        && let Some(target_type) = if source_param.rest {
                            if let Some(target_param) = target_fn.params.get(i)
                                && target_param.rest
                                && i + 1 == target_fn.params.len()
                            {
                                Some(target_param.type_id)
                            } else {
                                let remaining: Vec<TupleElement> = target_fn.params[i..]
                                    .iter()
                                    .map(|p| TupleElement {
                                        type_id: p.type_id,
                                        name: p.name,
                                        optional: p.optional,
                                        rest: p.rest,
                                    })
                                    .collect();
                                (!remaining.is_empty()).then(|| self.interner.tuple(remaining))
                            }
                        } else {
                            target_fn
                                .params
                                .get(i)
                                .map(|target_param| target_param.type_id)
                        }
                        && target_type != TypeId::UNKNOWN
                        && target_type != TypeId::ERROR
                    {
                        substitution.insert(tp.name, target_type);
                    }
                    if source_param.rest {
                        break;
                    }
                }
                if let Some(TypeData::TypeParameter(tp)) =
                    self.interner.lookup(source_fn.return_type)
                    && substitution.domain_contains_type_parameter(&tp, tracked_type_params)
                    && substitution.get(tp.name).is_none()
                    && target_fn.return_type != TypeId::UNKNOWN
                    && target_fn.return_type != TypeId::ERROR
                {
                    substitution.insert(tp.name, target_fn.return_type);
                }
                // Recurse for non-TypeParameter positions (nested structures).
                // Use an ungated helper that doesn't apply the
                // target-contains-untracked and references-other-tracked guards.
                // When the target function is generic, its type params (e.g. `A`
                // from `<A>(a: A[]) => Box<A>[]`) are legitimate targets, not
                // contaminants from nested generic signatures.
                for (source_param, target_param) in
                    source_fn.params.iter().zip(target_fn.params.iter())
                {
                    if !matches!(
                        self.interner.lookup(source_param.type_id),
                        Some(TypeData::TypeParameter(_))
                    ) {
                        self.collect_return_context_for_generic_target(
                            source_param.type_id,
                            target_param.type_id,
                            tracked_type_params,
                            substitution,
                        );
                    }
                }
                if !matches!(
                    self.interner.lookup(source_fn.return_type),
                    Some(TypeData::TypeParameter(_))
                ) {
                    self.collect_return_context_for_generic_target(
                        source_fn.return_type,
                        target_fn.return_type,
                        tracked_type_params,
                        substitution,
                    );
                }
            } else {
                for (i, source_param) in source_fn.params.iter().enumerate() {
                    if source_param.rest {
                        if let Some(target_param) = target_fn.params.get(i)
                            && target_param.rest
                            && i + 1 == target_fn.params.len()
                        {
                            self.collect_return_context_substitution(
                                source_param.type_id,
                                target_param.type_id,
                                tracked_type_params,
                                substitution,
                                visited,
                            );
                            break;
                        }
                        // Source has a rest parameter — collect remaining target
                        // params into a tuple so `Args` infers as e.g. `[string]`
                        // instead of `string`.
                        let remaining: Vec<TupleElement> = target_fn.params[i..]
                            .iter()
                            .map(|p| TupleElement {
                                type_id: p.type_id,
                                name: p.name,
                                optional: p.optional,
                                rest: p.rest,
                            })
                            .collect();
                        if !remaining.is_empty() {
                            let tuple_type = self.interner.tuple(remaining);
                            self.collect_return_context_substitution(
                                source_param.type_id,
                                tuple_type,
                                tracked_type_params,
                                substitution,
                                visited,
                            );
                        }
                        break;
                    } else if let Some(target_param) = target_fn.params.get(i) {
                        self.collect_return_context_substitution(
                            source_param.type_id,
                            target_param.type_id,
                            tracked_type_params,
                            substitution,
                            visited,
                        );
                    }
                }
                self.collect_return_context_substitution(
                    source_fn.return_type,
                    target_fn.return_type,
                    tracked_type_params,
                    substitution,
                    visited,
                );
            }
            return;
        }

        if let (Some(TypeData::Tuple(source_list_id)), Some(TypeData::Tuple(target_list_id))) =
            (self.interner.lookup(source), self.interner.lookup(target))
        {
            let source_elems = self.interner.tuple_list(source_list_id);
            let target_elems = self.interner.tuple_list(target_list_id);
            for (source_elem, target_elem) in source_elems.iter().zip(target_elems.iter()) {
                self.collect_return_context_substitution(
                    source_elem.type_id,
                    target_elem.type_id,
                    tracked_type_params,
                    substitution,
                    visited,
                );
            }
            return;
        }

        // Object-Object: match properties by name and recurse into their types.
        // This handles cases where applications are evaluated to structural object
        // types (e.g., Box<void, B> → { a: void, b: B }).
        if let (Some(TypeData::Object(s_shape_id)), Some(TypeData::Object(t_shape_id))) =
            (self.interner.lookup(source), self.interner.lookup(target))
        {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);
            for s_prop in &s_shape.properties {
                if let Some(t_prop) = t_shape.properties.iter().find(|p| p.name == s_prop.name) {
                    self.collect_return_context_substitution(
                        s_prop.type_id,
                        t_prop.type_id,
                        tracked_type_params,
                        substitution,
                        visited,
                    );
                }
            }
            if !substitution.is_empty() {
                return;
            }
        }

        if let (Some(source_elem), Some(target_elem)) = (
            crate::type_queries::get_array_element_type(self.interner.as_type_database(), source),
            crate::type_queries::get_array_element_type(self.interner.as_type_database(), target),
        ) {
            self.collect_return_context_substitution(
                source_elem,
                target_elem,
                tracked_type_params,
                substitution,
                visited,
            );
            return;
        }

        if let Some(source_elem) =
            crate::type_queries::get_array_element_type(self.interner.as_type_database(), source)
            && let Some((_target_base, target_args)) =
                crate::type_queries::get_application_info(self.interner.as_type_database(), target)
            && target_args.len() == 1
        {
            self.collect_return_context_substitution(
                source_elem,
                target_args[0],
                tracked_type_params,
                substitution,
                visited,
            );
            return;
        }

        if let Some(source_elem) =
            crate::type_queries::get_array_element_type(self.interner.as_type_database(), source)
            && let Some(iterator_info) =
                crate::operations::get_iterator_info(self.interner, target, false)
        {
            self.collect_return_context_substitution(
                source_elem,
                iterator_info.yield_type,
                tracked_type_params,
                substitution,
                visited,
            );
            return;
        }

        let source_eval = self.evaluate_return_context_match_type(source);
        let target_eval = self.evaluate_return_context_match_type(target);
        let app_info = match (
            crate::type_queries::get_application_info(self.interner.as_type_database(), source),
            crate::type_queries::get_application_info(self.interner.as_type_database(), target),
        ) {
            (Some(source_app), Some(target_app)) => Some((source_app, target_app)),
            _ => match (
                crate::type_queries::get_application_info(
                    self.interner.as_type_database(),
                    source_eval,
                ),
                crate::type_queries::get_application_info(
                    self.interner.as_type_database(),
                    target_eval,
                ),
            ) {
                (Some(source_app), Some(target_app)) => Some((source_app, target_app)),
                // Last resort: recover a baked structural object's originating
                // `Application` through its display-alias back-reference, so a
                // contextual return type that has been evaluated away from its
                // `Application` form still decomposes arg-by-arg. Reuse the
                // already-computed `source_eval`/`target_eval` rather than
                // re-evaluating; only the display-alias hop is genuinely new here.
                _ => self
                    .app_info_or_alias(source)
                    .or_else(|| self.app_info_or_alias(source_eval))
                    .zip(
                        self.app_info_or_alias(target)
                            .or_else(|| self.app_info_or_alias(target_eval)),
                    ),
            },
        };

        if let Some(((source_base, source_args), (target_base, mut target_args))) = app_info {
            // When same base but different arg counts (e.g., Box<void, B> vs Box<void>
            // where B has a default), try to pad with defaults from type params first.
            if source_base == target_base
                && source_args.len() > target_args.len()
                && let Some(def_id) = crate::type_queries::get_lazy_def_id(
                    self.interner.as_type_database(),
                    source_base,
                )
                && let Some(type_params) = self.interner.get_lazy_type_params(def_id)
                && type_params.len() == source_args.len()
            {
                let mut filled = target_args.clone();
                for param in &type_params[filled.len()..] {
                    filled.push(param.default.unwrap_or(TypeId::UNKNOWN));
                }
                target_args = filled;
            }
            // Fallback: when padding didn't equalize lengths (defaults unavailable),
            // evaluate both to structural form so Object-Object matching can find
            // the type parameter mappings through properties.
            if source_base == target_base && source_args.len() != target_args.len() {
                let eval_source = self.checker.evaluate_type(source);
                let eval_target = self.checker.evaluate_type(target);
                if eval_source != source || eval_target != target {
                    self.collect_return_context_substitution(
                        eval_source,
                        eval_target,
                        tracked_type_params,
                        substitution,
                        visited,
                    );
                    if !substitution.is_empty() {
                        return;
                    }
                }
            }

            if source_args.len() == target_args.len() && source_base == target_base {
                for (source_arg, target_arg) in source_args.iter().zip(target_args.iter()) {
                    self.collect_return_context_substitution(
                        *source_arg,
                        *target_arg,
                        tracked_type_params,
                        substitution,
                        visited,
                    );
                }
                return;
            }
            // When bases differ, match type arguments positionally if any source arg
            // is a tracked type parameter.
            if source_args.len() == target_args.len() {
                let has_tracked_source_arg = source_args.iter().any(|&arg| {
                    if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(arg) {
                        substitution.domain_contains_type_parameter(&tp, tracked_type_params)
                    } else {
                        false
                    }
                });
                if has_tracked_source_arg {
                    for (source_arg, target_arg) in source_args.iter().zip(target_args.iter()) {
                        self.collect_return_context_substitution(
                            *source_arg,
                            *target_arg,
                            tracked_type_params,
                            substitution,
                            visited,
                        );
                    }
                    if !substitution.is_empty() {
                        return;
                    }
                }
            }
        }

        // #14346 global re-reduce depth budget: this flag-ON self-recursion on
        // the resolver-evaluated (`source_eval`/`target_eval`) forms is the
        // cross-arena `URItoKindN` growth site — each turn interns a strictly
        // larger evaluated pair, so the `(source, target)` visited set never
        // trips. Guard the recursion on the shared native-depth budget and,
        // when exhausted, skip this re-reduce (the flag-OFF path never takes
        // this branch at all) and fall through to the deferred fallback below.
        if crate::instantiation::instantiate::flags::inst_resolver_rereduce_enabled()
            && (source_eval != source || target_eval != target)
            && let Some(_g) = crate::instantiation::instantiate::flags::rereduce_depth_try_enter()
        {
            self.collect_return_context_substitution(
                source_eval,
                target_eval,
                tracked_type_params,
                substitution,
                visited,
            );
            if !substitution.is_empty() {
                return;
            }
        }

        // Fallback: when source is an Application wrapping a single tracked type
        // parameter (e.g., Awaited<T>) and no structural match was found above,
        // try inferring the type parameter directly. This handles return context
        // inference for Promise.all where the return type contains Awaited<T> and
        // the contextual type is a concrete non-thenable type.
        // Guard: verify by evaluating Application(Base, [target]) and checking
        // it equals target — this ensures the alias is "transparent" (like
        // Awaited<X> = X for non-thenables) and not a structural wrapper (like
        // Task<X> which wraps X in a function type).
        if let Some((source_base, source_args)) =
            crate::type_queries::get_application_info(self.interner.as_type_database(), source)
                .or_else(|| {
                    crate::type_queries::get_application_info(
                        self.interner.as_type_database(),
                        source_eval,
                    )
                })
            && source_args.len() == 1
            && let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(source_args[0])
            && substitution.domain_contains_type_parameter(&tp, tracked_type_params)
            && substitution.get(tp.name).is_none()
            && !self.target_contains_untracked_type_params(
                target,
                tracked_type_params,
                substitution,
            )
        {
            // Verify: Application(Base, [target]) should evaluate to target
            // for the substitution to be correct.
            let test_app = self.interner.application(source_base, vec![target]);
            let evaluated = self.evaluate_return_context_match_type(test_app);
            if evaluated == target {
                substitution.insert(tp.name, target);
            }
        }
    }

    /// Structural matching helper for the generic-target-function case.
    /// Unlike `collect_return_context_substitution`, this does NOT apply the
    /// `target_contains_untracked_type_params` or `type_references_other_tracked_params`
    /// guards. Those guards exist to prevent contamination from nested generic
    /// signatures (e.g., `Promise.catch`'s `TResult`), but when the target is the
    /// contextual type's own generic function, its type params (like `A` in
    /// `<A>(a: A[]) => Box<A>[]`) are legitimate substitution targets.
    fn collect_return_context_for_generic_target(
        &self,
        source: TypeId,
        target: TypeId,
        tracked_type_params: &FxHashSet<tsz_common::Atom>,
        substitution: &mut TypeSubstitution,
    ) {
        // Direct TypeParameter leaf — insert without guards
        if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(source)
            && substitution.domain_contains_type_parameter(&tp, tracked_type_params)
            && target != TypeId::UNKNOWN
            && target != TypeId::ERROR
            && substitution.get(tp.name).is_none()
        {
            substitution.insert(tp.name, target);
            return;
        }

        // Array matching
        if let (Some(source_elem), Some(target_elem)) = (
            crate::type_queries::get_array_element_type(self.interner.as_type_database(), source),
            crate::type_queries::get_array_element_type(self.interner.as_type_database(), target),
        ) {
            self.collect_return_context_for_generic_target(
                source_elem,
                target_elem,
                tracked_type_params,
                substitution,
            );
            return;
        }

        // Tuple matching
        if let (Some(TypeData::Tuple(source_list_id)), Some(TypeData::Tuple(target_list_id))) =
            (self.interner.lookup(source), self.interner.lookup(target))
        {
            let source_elems = self.interner.tuple_list(source_list_id);
            let target_elems = self.interner.tuple_list(target_list_id);
            for (source_elem, target_elem) in source_elems.iter().zip(target_elems.iter()) {
                self.collect_return_context_for_generic_target(
                    source_elem.type_id,
                    target_elem.type_id,
                    tracked_type_params,
                    substitution,
                );
            }
            return;
        }

        // Application matching (same base, same arg count). Recover a baked
        // structural target through its display-alias back-reference so a
        // nested generic-signature position whose contextual type has been
        // evaluated away from its `Application` form still decomposes.
        if let (Some((source_base, source_args)), Some((target_base, target_args))) = (
            self.app_info_or_alias(source),
            self.app_info_or_alias(target),
        ) && source_base == target_base
            && source_args.len() == target_args.len()
        {
            for (source_arg, target_arg) in source_args.iter().zip(target_args.iter()) {
                self.collect_return_context_for_generic_target(
                    *source_arg,
                    *target_arg,
                    tracked_type_params,
                    substitution,
                );
            }
        }
    }

    /// Check if a type contains or IS a literal type (directly or in unions).
    pub(super) fn type_contains_literals(&self, type_id: TypeId) -> bool {
        match self.interner.lookup(type_id) {
            Some(TypeData::Literal(_)) => true,
            Some(TypeData::Union(members_id)) => {
                let members = self.interner.type_list(members_id);
                members.iter().any(|&m| self.type_contains_literals(m))
            }
            _ => false,
        }
    }

    /// Check if a type references tracked type parameters OTHER than `exclude_name`.
    fn type_references_other_tracked_params(
        &self,
        type_id: TypeId,
        exclude: &TypeParamInfo,
        tracked: &FxHashSet<tsz_common::Atom>,
        substitution: &TypeSubstitution,
    ) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(type_id) {
            return !tp.is_same_binder(*exclude)
                && substitution.domain_contains_type_parameter(&tp, tracked);
        }
        match self.interner.lookup(type_id) {
            Some(TypeData::Union(members_id) | TypeData::Intersection(members_id)) => {
                let members = self.interner.type_list(members_id);
                members.iter().any(|&m| {
                    self.type_references_other_tracked_params(m, exclude, tracked, substitution)
                })
            }
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                app.args.iter().any(|&arg| {
                    self.type_references_other_tracked_params(arg, exclude, tracked, substitution)
                })
            }
            _ => false,
        }
    }

    /// Check if a type contains `TypeParameter` references that are NOT in the
    /// tracked set. These are "foreign" type params from nested generic signatures
    /// (e.g., `Promise.catch`'s `TResult` when matching through `.then()`).
    fn target_contains_untracked_type_params(
        &self,
        type_id: TypeId,
        tracked: &FxHashSet<tsz_common::Atom>,
        substitution: &TypeSubstitution,
    ) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(type_id) {
            return !substitution.domain_contains_type_parameter(&tp, tracked);
        }
        match self.interner.lookup(type_id) {
            Some(TypeData::Union(members_id) | TypeData::Intersection(members_id)) => {
                let members = self.interner.type_list(members_id);
                members
                    .iter()
                    .any(|&m| self.target_contains_untracked_type_params(m, tracked, substitution))
            }
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                app.args.iter().any(|&arg| {
                    self.target_contains_untracked_type_params(arg, tracked, substitution)
                })
            }
            _ => false,
        }
    }

    pub(super) fn compute_return_context_substitution(
        &mut self,
        func: &FunctionShape,
        contextual_type: Option<TypeId>,
    ) -> TypeSubstitution {
        let Some(contextual_type) = contextual_type else {
            return TypeSubstitution::new();
        };

        let tracked_type_params: FxHashSet<_> = func.type_params.iter().map(|tp| tp.name).collect();
        if tracked_type_params.is_empty() {
            return TypeSubstitution::new();
        }

        let mut substitution = TypeSubstitution::new();
        substitution.protect_type_parameters(&func.type_params);
        if func.is_constructor {
            let return_type_eval = self.evaluate_return_context_match_type(func.return_type);
            let contextual_app = crate::type_queries::get_application_info(
                self.interner.as_type_database(),
                contextual_type,
            )
            .or_else(|| {
                let contextual_eval = self.evaluate_return_context_match_type(contextual_type);
                crate::type_queries::get_application_info(
                    self.interner.as_type_database(),
                    contextual_eval,
                )
            });
            if let Some((contextual_base, contextual_args)) = contextual_app
                && (contextual_base == func.return_type || contextual_base == return_type_eval)
                && contextual_args.len() == func.type_params.len()
            {
                for (type_param, contextual_arg) in
                    func.type_params.iter().zip(contextual_args.iter())
                {
                    if *contextual_arg != TypeId::UNKNOWN && *contextual_arg != TypeId::ERROR {
                        substitution.insert(type_param.name, *contextual_arg);
                    }
                }
                if !substitution.is_empty() {
                    return substitution;
                }
            }
        }

        let mut visited = FxHashSet::default();
        if !self.collect_aligned_return_application_substitution(
            func.return_type,
            contextual_type,
            &tracked_type_params,
            &mut substitution,
            &mut visited,
        ) {
            self.collect_return_context_substitution(
                func.return_type,
                contextual_type,
                &tracked_type_params,
                &mut substitution,
                &mut visited,
            );
        }
        substitution
    }

    pub(crate) fn resolve_with_request(
        &mut self,
        request: GenericCallRequest<'_>,
    ) -> GenericCallResult {
        let previous_defaulted = std::mem::take(&mut self.defaulted_placeholders);
        let call_result = self.resolve_generic_call_inner(request.func(), request.arg_types());
        self.defaulted_placeholders = previous_defaulted;
        GenericCallResult::new(call_result)
            .with_instantiated_predicate(self.last_instantiated_predicate.take())
            .with_instantiated_params(self.last_instantiated_params.take())
    }

    pub(crate) fn resolve_generic_call(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> GenericCallResult {
        self.resolve_with_request(GenericCallRequest::new(func, arg_types))
    }
}

#[cfg(test)]
#[path = "return_context/aligned_application_tests.rs"]
mod aligned_application_tests;

#[cfg(test)]
mod tests {
    use super::sort_type_params_by_name;
    use crate::TypeInterner;
    use crate::caches::query_cache::QueryCache;
    use crate::def::{DefId, DefinitionStore};
    use crate::instantiation::instantiate::flags::InstResolverRereduceFlagGuard;
    use crate::operations::{AssignabilityChecker, CallEvaluator, CallResult, GenericCallRequest};
    use crate::types::{
        FunctionShape, ParamInfo, PropertyInfo, TypeId, TypeParamInfo, TypePredicate,
        TypePredicateTarget,
    };
    use tsz_common::interner::Atom;

    const fn tp(name: u32) -> TypeParamInfo {
        TypeParamInfo {
            name: Atom(name),
            constraint: Some(TypeId::UNKNOWN),
            default: Some(TypeId::ERROR),
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        }
    }

    fn object_with_value(interner: &TypeInterner, value_name: Atom, value_type: TypeId) -> TypeId {
        interner.object(vec![PropertyInfo::new(value_name, value_type)])
    }

    struct StoreBackedReturnChecker<'eval, 'cache> {
        db: &'eval QueryCache<'cache>,
    }

    impl AssignabilityChecker for StoreBackedReturnChecker<'_, '_> {
        fn is_assignable_to(&mut self, _source: TypeId, _target: TypeId) -> bool {
            true
        }

        fn evaluate_type_for_return_context_substitution(&mut self, type_id: TypeId) -> TypeId {
            self.db
                .store_backed_rereduce_evaluator()
                .map_or(type_id, |mut evaluator| evaluator.evaluate(type_id))
        }
    }

    fn return_context_substitution_for_lazy_pair(
        interner: &TypeInterner,
        db: &QueryCache<'_>,
        source_def: DefId,
        contextual_def: DefId,
        call_param: TypeParamInfo,
    ) -> crate::instantiation::instantiate::TypeSubstitution {
        let func = FunctionShape {
            type_params: vec![call_param],
            params: Vec::new(),
            this_type: None,
            return_type: interner.lazy(source_def),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        };
        let mut checker = StoreBackedReturnChecker { db };
        let mut evaluator = CallEvaluator::new(interner, &mut checker);
        evaluator.compute_return_context_substitution(&func, Some(interner.lazy(contextual_def)))
    }

    #[derive(Clone, Copy)]
    struct LazyWrapPair {
        wrap_def: DefId,
        source_def: DefId,
        contextual_def: DefId,
        wrap_param: TypeParamInfo,
        call_param: TypeParamInfo,
        property_name: Atom,
        contextual_value: TypeId,
    }

    fn publish_lazy_wrap_pair(
        interner: &TypeInterner,
        store: &DefinitionStore,
        case: LazyWrapPair,
    ) {
        let wrap_param_id = interner.type_param(case.wrap_param);
        store.set_body_with_params(
            case.wrap_def,
            object_with_value(interner, case.property_name, wrap_param_id),
            Some(vec![case.wrap_param]),
        );
        let wrap_base = interner.lazy(case.wrap_def);
        let call_param_id = interner.type_param(case.call_param);
        store.set_body(
            case.source_def,
            interner.application(wrap_base, vec![call_param_id]),
        );
        store.set_body(
            case.contextual_def,
            interner.application(wrap_base, vec![case.contextual_value]),
        );
    }

    #[derive(Clone, Copy)]
    struct NestedLazyWrapPair {
        inner_def: DefId,
        outer_def: DefId,
        source_def: DefId,
        contextual_def: DefId,
        inner_param: TypeParamInfo,
        outer_param: TypeParamInfo,
        call_param: TypeParamInfo,
        inner_property_name: Atom,
        outer_property_name: Atom,
        contextual_value: TypeId,
    }

    fn publish_nested_lazy_wrap_pair(
        interner: &TypeInterner,
        store: &DefinitionStore,
        case: NestedLazyWrapPair,
    ) {
        let inner_param_id = interner.type_param(case.inner_param);
        store.set_body_with_params(
            case.inner_def,
            object_with_value(interner, case.inner_property_name, inner_param_id),
            Some(vec![case.inner_param]),
        );
        let outer_param_id = interner.type_param(case.outer_param);
        store.set_body_with_params(
            case.outer_def,
            object_with_value(interner, case.outer_property_name, outer_param_id),
            Some(vec![case.outer_param]),
        );
        let inner_base = interner.lazy(case.inner_def);
        let outer_base = interner.lazy(case.outer_def);
        let call_param_id = interner.type_param(case.call_param);
        store.set_body(
            case.source_def,
            interner.application(
                outer_base,
                vec![interner.application(inner_base, vec![call_param_id])],
            ),
        );
        store.set_body(
            case.contextual_def,
            interner.application(
                outer_base,
                vec![interner.application(inner_base, vec![case.contextual_value])],
            ),
        );
    }

    #[derive(Clone, Copy)]
    struct LazyPairApplication {
        pair_def: DefId,
        source_def: DefId,
        contextual_def: DefId,
        fixed_param: TypeParamInfo,
        value_param: TypeParamInfo,
        call_param: TypeParamInfo,
        fixed_property_name: Atom,
        value_property_name: Atom,
        fixed_value: TypeId,
        contextual_value: TypeId,
    }

    fn publish_lazy_pair_application(
        interner: &TypeInterner,
        store: &DefinitionStore,
        case: LazyPairApplication,
    ) {
        let fixed_param_id = interner.type_param(case.fixed_param);
        let value_param_id = interner.type_param(case.value_param);
        store.set_body_with_params(
            case.pair_def,
            interner.object(vec![
                PropertyInfo::new(case.fixed_property_name, fixed_param_id),
                PropertyInfo::new(case.value_property_name, value_param_id),
            ]),
            Some(vec![case.fixed_param, case.value_param]),
        );
        let pair_base = interner.lazy(case.pair_def);
        let call_param_id = interner.type_param(case.call_param);
        store.set_body(
            case.source_def,
            interner.application(pair_base, vec![case.fixed_value, call_param_id]),
        );
        store.set_body(
            case.contextual_def,
            interner.application(pair_base, vec![case.fixed_value, case.contextual_value]),
        );
    }

    #[derive(Clone, Copy)]
    struct TransparentLazyAlias {
        alias_def: DefId,
        source_def: DefId,
        contextual_def: DefId,
        alias_param: TypeParamInfo,
        call_param: TypeParamInfo,
        contextual_value: TypeId,
    }

    fn publish_transparent_lazy_alias(
        interner: &TypeInterner,
        store: &DefinitionStore,
        case: TransparentLazyAlias,
    ) {
        let alias_param_id = interner.type_param(case.alias_param);
        store.set_body_with_params(case.alias_def, alias_param_id, Some(vec![case.alias_param]));
        let alias_base = interner.lazy(case.alias_def);
        let call_param_id = interner.type_param(case.call_param);
        store.set_body(
            case.source_def,
            interner.application(alias_base, vec![call_param_id]),
        );
        store.set_body(case.contextual_def, case.contextual_value);
    }

    #[test]
    fn sort_type_params_by_name_orders_ascending_atom_ids() {
        let mut type_params = vec![tp(7), tp(1), tp(3)];
        sort_type_params_by_name(&mut type_params);

        let names: Vec<_> = type_params
            .iter()
            .map(|type_param| type_param.name)
            .collect();
        assert_eq!(names, vec![Atom(1), Atom(3), Atom(7)]);
    }

    #[test]
    fn return_context_substitution_resolves_store_backed_lazy_application_bodies() {
        let interner = TypeInterner::new();
        let store = DefinitionStore::new();
        let wrap_def = DefId(143_510);
        let source_def = DefId(143_511);
        let contextual_def = DefId(143_512);
        let call_param = tp(303);
        publish_lazy_wrap_pair(
            &interner,
            &store,
            LazyWrapPair {
                wrap_def,
                source_def,
                contextual_def,
                wrap_param: tp(101),
                call_param,
                property_name: Atom(202),
                contextual_value: TypeId::STRING,
            },
        );
        let db = QueryCache::new(&interner).with_definition_store(&store);

        let _flag = InstResolverRereduceFlagGuard::new(true);
        let substitution = return_context_substitution_for_lazy_pair(
            &interner,
            &db,
            source_def,
            contextual_def,
            call_param,
        );

        assert_eq!(substitution.get(call_param.name), Some(TypeId::STRING));
    }

    #[test]
    fn return_context_substitution_resolves_nested_store_backed_lazy_application_bodies() {
        let interner = TypeInterner::new();
        let store = DefinitionStore::new();
        let inner_def = DefId(143_540);
        let outer_def = DefId(143_541);
        let source_def = DefId(143_542);
        let contextual_def = DefId(143_543);
        let call_param = tp(1_103);
        publish_nested_lazy_wrap_pair(
            &interner,
            &store,
            NestedLazyWrapPair {
                inner_def,
                outer_def,
                source_def,
                contextual_def,
                inner_param: tp(901),
                outer_param: tp(902),
                call_param,
                inner_property_name: Atom(1_001),
                outer_property_name: Atom(1_002),
                contextual_value: TypeId::STRING,
            },
        );
        let db = QueryCache::new(&interner).with_definition_store(&store);

        let _flag = InstResolverRereduceFlagGuard::new(true);
        let substitution = return_context_substitution_for_lazy_pair(
            &interner,
            &db,
            source_def,
            contextual_def,
            call_param,
        );

        assert_eq!(substitution.get(call_param.name), Some(TypeId::STRING));
    }

    #[test]
    fn return_context_substitution_matches_store_backed_lazy_pair_fixed_argument() {
        let interner = TypeInterner::new();
        let store = DefinitionStore::new();
        let pair_def = DefId(143_550);
        let source_def = DefId(143_551);
        let contextual_def = DefId(143_552);
        let call_param = tp(1_403);
        publish_lazy_pair_application(
            &interner,
            &store,
            LazyPairApplication {
                pair_def,
                source_def,
                contextual_def,
                fixed_param: tp(1_201),
                value_param: tp(1_202),
                call_param,
                fixed_property_name: Atom(1_301),
                value_property_name: Atom(1_302),
                fixed_value: TypeId::NUMBER,
                contextual_value: TypeId::STRING,
            },
        );
        let db = QueryCache::new(&interner).with_definition_store(&store);

        let _flag = InstResolverRereduceFlagGuard::new(true);
        let substitution = return_context_substitution_for_lazy_pair(
            &interner,
            &db,
            source_def,
            contextual_def,
            call_param,
        );

        assert_eq!(substitution.get(call_param.name), Some(TypeId::STRING));
    }

    #[test]
    fn return_context_substitution_resolves_transparent_store_backed_lazy_alias_body() {
        let interner = TypeInterner::new();
        let store = DefinitionStore::new();
        let alias_def = DefId(143_560);
        let source_def = DefId(143_561);
        let contextual_def = DefId(143_562);
        let call_param = tp(1_703);
        publish_transparent_lazy_alias(
            &interner,
            &store,
            TransparentLazyAlias {
                alias_def,
                source_def,
                contextual_def,
                alias_param: tp(1_501),
                call_param,
                contextual_value: TypeId::STRING,
            },
        );
        let db = QueryCache::new(&interner).with_definition_store(&store);

        {
            let _flag = InstResolverRereduceFlagGuard::new(false);
            let substitution = return_context_substitution_for_lazy_pair(
                &interner,
                &db,
                source_def,
                contextual_def,
                call_param,
            );
            assert!(substitution.get(call_param.name).is_none());
        }

        let _flag = InstResolverRereduceFlagGuard::new(true);
        let substitution = return_context_substitution_for_lazy_pair(
            &interner,
            &db,
            source_def,
            contextual_def,
            call_param,
        );

        assert_eq!(substitution.get(call_param.name), Some(TypeId::STRING));
    }

    #[test]
    fn return_context_substitution_keeps_lazy_bodies_deferred_without_rereduce_flag() {
        let interner = TypeInterner::new();
        let store = DefinitionStore::new();
        let wrap_def = DefId(143_520);
        let source_def = DefId(143_521);
        let contextual_def = DefId(143_522);
        let call_param = tp(603);
        publish_lazy_wrap_pair(
            &interner,
            &store,
            LazyWrapPair {
                wrap_def,
                source_def,
                contextual_def,
                wrap_param: tp(401),
                call_param,
                property_name: Atom(502),
                contextual_value: TypeId::STRING,
            },
        );
        let db = QueryCache::new(&interner).with_definition_store(&store);

        let _flag = InstResolverRereduceFlagGuard::new(false);
        let substitution = return_context_substitution_for_lazy_pair(
            &interner,
            &db,
            source_def,
            contextual_def,
            call_param,
        );

        assert!(substitution.get(call_param.name).is_none());
    }

    #[test]
    fn return_context_substitution_requires_store_and_published_lazy_bodies() {
        let interner = TypeInterner::new();
        let source_def = DefId(143_531);
        let contextual_def = DefId(143_532);
        let call_param = tp(803);
        let no_store_db = QueryCache::new(&interner);
        let empty_store = DefinitionStore::new();
        let missing_body_db = QueryCache::new(&interner).with_definition_store(&empty_store);

        let _flag = InstResolverRereduceFlagGuard::new(true);
        let no_store_substitution = return_context_substitution_for_lazy_pair(
            &interner,
            &no_store_db,
            source_def,
            contextual_def,
            call_param,
        );
        let missing_body_substitution = return_context_substitution_for_lazy_pair(
            &interner,
            &missing_body_db,
            source_def,
            contextual_def,
            call_param,
        );

        assert!(no_store_substitution.get(call_param.name).is_none());
        assert!(missing_body_substitution.get(call_param.name).is_none());
    }

    #[derive(Default)]
    struct BoundaryChecker;

    impl AssignabilityChecker for BoundaryChecker {
        fn is_assignable_to(&mut self, _source: TypeId, _target: TypeId) -> bool {
            true
        }
    }

    #[test]
    fn return_context_does_not_bind_foreign_same_named_scoped_parameter() {
        let interner = TypeInterner::new();
        let file = interner.intern_string("return-context-domain.ts");
        let name = interner.intern_string("U");
        let owned = TypeParamInfo {
            name,
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::DeclScoped { file, node: 1 },
        };
        let foreign = interner.fresh_type_param(TypeParamInfo {
            origin: crate::types::TypeParamOrigin::DeclScoped { file, node: 2 },
            ..owned
        });
        let func = FunctionShape {
            type_params: vec![owned],
            params: Vec::new(),
            this_type: None,
            return_type: foreign,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        };
        let mut checker = BoundaryChecker;
        let mut evaluator = CallEvaluator::new(&interner, &mut checker);

        let substitution =
            evaluator.compute_return_context_substitution(&func, Some(TypeId::STRING));

        assert!(substitution.get(name).is_none());
    }

    #[test]
    fn resolve_with_request_returns_instantiated_side_channel_data() {
        let interner = TypeInterner::new();
        let type_param = tp(11);
        let param_name = Atom(23);
        let type_param_id = interner.type_param(type_param);
        let func = FunctionShape {
            type_params: vec![type_param],
            params: vec![ParamInfo::required(param_name, type_param_id)],
            this_type: None,
            return_type: TypeId::BOOLEAN,
            type_predicate: Some(TypePredicate {
                asserts: false,
                target: TypePredicateTarget::Identifier(param_name),
                type_id: Some(type_param_id),
                parameter_index: Some(0),
            }),
            is_constructor: false,
            is_method: false,
        };
        let arg_types = [TypeId::STRING];
        let mut checker = BoundaryChecker;
        let mut evaluator = CallEvaluator::new(&interner, &mut checker);

        let mut result = evaluator.resolve_with_request(GenericCallRequest::new(&func, &arg_types));

        assert!(evaluator.last_instantiated_predicate.is_none());
        assert!(evaluator.last_instantiated_params.is_none());

        let (predicate, predicate_params) = result
            .take_instantiated_predicate()
            .expect("generic call should return the instantiated predicate");
        assert_eq!(predicate.type_id, Some(TypeId::STRING));
        assert_eq!(predicate_params[0].type_id, TypeId::STRING);
        assert!(result.take_instantiated_predicate().is_none());

        let params = result
            .take_instantiated_params()
            .expect("generic call should return instantiated params");
        assert_eq!(params[0].type_id, TypeId::STRING);
        assert!(result.take_instantiated_params().is_none());

        assert!(matches!(
            result.into_call_result(),
            CallResult::Success(TypeId::BOOLEAN)
        ));
    }
}
