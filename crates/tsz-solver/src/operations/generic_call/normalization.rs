//! Trivial call resolution, placeholder normalization, and contextual type computation.

use crate::inference::infer::{InferenceContext, InferenceVar};
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::{AssignabilityChecker, CallEvaluator, CallResult};
use crate::types::{FunctionShape, ParamInfo, TypeData, TypeId, TypeParamInfo};
use rustc_hash::{FxHashMap, FxHashSet};

use super::{constraint_is_primitive_type_with_resolver, write_placeholder_name};

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    /// Build the DELEGATING resolver that the generic-call inference sites
    /// borrow to expand a cross-arena `Lazy(DefId)` base, AUGMENTING the
    /// original per-arena `query_db` resolver rather than swapping it out
    /// (issue #14344 / #14345 HKT-reduce lever, default-OFF behind
    /// `TSZ_INFER_HKT_REDUCE`).
    ///
    /// Returns `None` unless all of `TSZ_INFER_HKT_REDUCE`,
    /// `TSZ_INST_RESOLVER_REREDUCE` and `TSZ_OPTIONB_STORE_RESOLVER` are ON and a
    /// shared `DefinitionStore` is attached — so the OFF / no-store path keeps
    /// the `InferenceContext`'s literal `with_query_db` resolver (the
    /// `QueryCache`, whose `resolve_lazy` returns `None` for a cross-arena
    /// `DefId`) and stays byte-identical.
    ///
    /// The returned [`DelegatingHktResolver`] holds BOTH the shared store AND
    /// the original `query_db` resolver (`self.interner` upcast to
    /// `&dyn TypeResolver`). It overrides only the three store-backed
    /// `Lazy(DefId)` reductions and delegates every other `TypeResolver` method
    /// (including `get_type_param_variance`, the variance source) to the wrapped
    /// per-arena resolver. A bare [`StoreOnlyResolver`] would instead drop ALL
    /// per-arena answers (variance collapsing to COVARIANT), which under RAYON>1
    /// leaked a not-yet-collapsed wide HKT union as a non-deterministic false
    /// positive; the delegation preserves per-arena state and correct variance.
    ///
    /// Both the store and the resolver borrow are taken from the `'a`-lived
    /// `interner` field (not a `&self` reborrow), so the returned resolver lives
    /// for `'a` and can be assigned to the `InferenceContext`'s `resolver`
    /// field. It borrows only `&'a` shared, program-global reads and never the
    /// `&mut C` checker, so there is no E0502 here.
    pub(crate) fn build_inference_hkt_reduce_shim(
        &self,
    ) -> Option<crate::caches::query_cache_evaluation::DelegatingHktResolver<'a>> {
        if !crate::instantiation::instantiate::flags::infer_hkt_reduce_enabled()
            || !crate::instantiation::instantiate::flags::inst_resolver_rereduce_enabled()
            || !crate::instantiation::instantiate::flags::optionb_store_resolver_enabled()
        {
            return None;
        }
        let interner: &'a dyn crate::construction::QueryDatabase = self.interner;
        let store = interner.definition_store_for_inference()?;
        let inner: &'a dyn crate::relations::subtype::TypeResolver = interner.as_type_resolver();
        Some(crate::caches::query_cache_evaluation::DelegatingHktResolver::new(store, inner))
    }

    /// Fast path for direct single-parameter generic calls:
    /// `<T extends C>(x: T | W<T>) => R<T>` with a single non-rest argument.
    ///
    /// This shape is common in constraint-heavy code and does not require full
    /// multi-pass inference machinery. We can infer `T` directly from the argument,
    /// validate the constraint once, and instantiate the return type.
    pub(crate) fn resolve_trivial_single_type_param_call(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> Option<CallResult> {
        if func.type_params.len() != 1 || func.params.len() != 1 || arg_types.len() != 1 {
            return None;
        }
        if func.params[0].rest || func.params[0].optional {
            return None;
        }
        if func.this_type.is_some() || func.type_predicate.is_some() {
            return None;
        }

        let tp = &func.type_params[0];
        let param_ty = func.params[0].type_id;
        let return_ty = func.return_type;
        let mut subst = TypeSubstitution::single(tp.name, TypeId::UNKNOWN);
        subst.protect_type_parameters(&func.type_params);

        let is_tp = |ty: TypeId| {
            matches!(
                self.interner.lookup(ty),
                Some(TypeData::TypeParameter(info)) if subst.binds_type_parameter(&info)
            )
        };
        let param_is_tp = is_tp(param_ty);
        let param_is_direct_union =
            !param_is_tp && self.union_contains_bound_type_parameter_member(param_ty, &subst);
        if !param_is_tp && !param_is_direct_union {
            return None;
        }
        let return_is_tp = is_tp(return_ty);
        if func.is_constructor && !return_is_tp {
            return None;
        }
        let return_contains_tp = return_is_tp
            || crate::visitors::visitor_predicates::contains_type_matching(
                self.interner.as_type_database(),
                return_ty,
                |type_data| {
                    matches!(
                        type_data,
                        TypeData::TypeParameter(info) if subst.binds_type_parameter(info)
                    )
                },
            );
        if !return_contains_tp {
            return None;
        }

        // Contextual return types can seed inference for wrapped returns. Keep
        // the older identity case on this path, but let compound return shapes
        // use the full pipeline when a meaningful expected type is present.
        if !return_is_tp
            && self.contextual_type.is_some_and(|contextual_type| {
                contextual_type != TypeId::ANY && contextual_type != TypeId::UNKNOWN
            })
        {
            return None;
        }

        if self.contextual_type.is_some_and(|contextual_type| {
            self.is_contextually_sensitive(arg_types[0])
                && Self::get_contextual_signature_cached(self.interner, contextual_type).is_some()
        }) {
            return None;
        }
        if param_is_direct_union {
            return None;
        }
        if !return_is_tp
            && !matches!(
                self.interner.lookup(arg_types[0]),
                Some(TypeData::TypeParameter(_))
            )
        {
            return None;
        }

        // Bail out for self-referential constraints like `T extends Test<keyof T>`.
        // The fast path cannot properly instantiate the constraint with the inferred
        // type (it checks the raw constraint), and it uses `widen_type` which
        // deep-widens object properties. The normal inference path handles this
        // correctly by instantiating the constraint with `final_subst`.
        if let Some(constraint) = tp.constraint
            && crate::visitors::visitor_predicates::contains_type_matching(
                self.interner.as_type_database(),
                constraint,
                |type_data| {
                    matches!(
                        type_data,
                        TypeData::TypeParameter(info) if subst.binds_type_parameter(info)
                    )
                },
            )
        {
            return None;
        }

        let arg_ty = arg_types[0];
        let constraint = tp.constraint.map(|constraint| {
            crate::type_queries::get_base_constraint_of_type(
                self.interner.as_type_database(),
                constraint,
            )
        });
        let constraint_allows_mutable_array = constraint.is_some_and(|c| {
            crate::type_queries::constraint_allows_mutable_array_like(
                self.interner.as_type_database(),
                c,
            )
        });
        let inferred_ty = if tp.is_const && !constraint_allows_mutable_array {
            crate::operations::widening::apply_const_assertion(
                self.interner.as_type_database(),
                arg_ty,
            )
        } else {
            // When the declared constraint is a primitive type (string, number,
            // boolean, bigint) or a union thereof, preserve literal types without
            // widening. This matches tsc's getInferredType which checks
            // isLiteralType(constraint) and skips getWidenedLiteralType.
            // Example: `<T extends string>(x: T): T` called with `"hello"`
            // should infer T = "hello", not T = string.
            let constraint_is_primitive = constraint.is_some_and(|c| {
                let resolver = self
                    .checker
                    .type_resolver()
                    .unwrap_or_else(|| self.interner.as_type_resolver());
                constraint_is_primitive_type_with_resolver(self.interner, resolver, c)
            });
            if constraint_is_primitive {
                arg_ty
            } else if let Some(constraint) = constraint {
                // Widen to check if widening would violate the constraint.
                // Fresh objects use widen_type (not widen_type_for_inference) to preserve
                // FRESH_LITERAL, which is required for excess-property checking against the constraint.
                let widened_for_check = crate::operations::widening::widen_type(
                    self.interner.as_type_database(),
                    arg_ty,
                );
                if !self.checker.is_assignable_to(widened_for_check, constraint)
                    && self.checker.is_assignable_to(arg_ty, constraint)
                {
                    arg_ty
                } else {
                    widened_for_check
                }
            } else if self.is_first_arg_type_annotated() {
                // Type assertions (e.g. `identity(1 as 1)`) produce non-fresh literals;
                // preserve them, matching the normal inference path's
                // `has_type_annotation_candidate` gate.
                arg_ty
            } else {
                // Only compute widen_type when a contextual return type exists — widening may
                // break assignability to it (e.g. `let v: DooDad = identity('ELSE')`).
                let should_preserve_literal = self.contextual_type.is_some_and(|ctx_type| {
                    if ctx_type == TypeId::ANY || ctx_type == TypeId::UNKNOWN {
                        return false;
                    }
                    let widened_for_check = crate::operations::widening::widen_type(
                        self.interner.as_type_database(),
                        arg_ty,
                    );
                    widened_for_check != arg_ty
                        && !self.checker.is_assignable_to(widened_for_check, ctx_type)
                        && self.checker.is_assignable_to(arg_ty, ctx_type)
                });
                if should_preserve_literal
                    || (return_is_tp
                        && crate::visitor::is_literal_type(
                            self.interner.as_type_database(),
                            arg_ty,
                        ))
                {
                    arg_ty
                } else {
                    crate::operations::widening::widen_type_for_inference(
                        self.interner.as_type_database(),
                        arg_ty,
                    )
                }
            }
        };
        let effective_arg_ty = if inferred_ty == TypeId::ANY || inferred_ty == TypeId::UNKNOWN {
            arg_ty
        } else {
            inferred_ty
        };
        subst.insert(tp.name, effective_arg_ty);
        if let Some(constraint) = constraint
            && !self.arg_satisfies_type_parameter_constraint(effective_arg_ty, constraint)
            && !self.is_function_union_compat(effective_arg_ty, constraint)
            && !self.callable_satisfies_top_rest_any_constraint(effective_arg_ty, constraint)
        {
            // In the trivial single-type-param fast path, the parameter IS the
            // type parameter itself, so a constraint violation means the argument
            // doesn't match the effective parameter type (the constraint).
            // tsc reports TS2345 ("Argument of type X is not assignable to
            // parameter of type Y") here, not TS2322.
            return Some(CallResult::ArgumentTypeMismatch {
                index: 0,
                expected: constraint,
                actual: effective_arg_ty,
                fallback_return: effective_arg_ty,
            });
        }

        if param_is_direct_union {
            let expected_param_ty = instantiate_type(self.interner, param_ty, &subst);
            if !self.checker.is_assignable_to(arg_ty, expected_param_ty) {
                return Some(CallResult::ArgumentTypeMismatch {
                    index: 0,
                    expected: expected_param_ty,
                    actual: arg_ty,
                    fallback_return: TypeId::ERROR,
                });
            }
        }

        let return_type = if return_is_tp {
            effective_arg_ty
        } else {
            instantiate_type(self.interner, return_ty, &subst)
        };

        Some(CallResult::Success(return_type))
    }

    fn union_contains_bound_type_parameter_member(
        &self,
        type_id: TypeId,
        substitution: &TypeSubstitution,
    ) -> bool {
        let Some(TypeData::Union(members_id)) = self.interner.lookup(type_id) else {
            return false;
        };
        self.interner.type_list(members_id).iter().any(|&member| {
            matches!(
                self.interner.lookup(member),
                Some(TypeData::TypeParameter(info)) if substitution.binds_type_parameter(&info)
            )
        })
    }

    /// Reduce an instantiated parameter type before the final argument check.
    ///
    /// Once generic inference has fixed a function's type parameters, a
    /// conditional / `Exclude` parameter that referenced those parameters
    /// becomes concrete and must be evaluated so it reduces to its real form
    /// (for example `'a' extends 'a' ? never : 'a'` reduces to `never`). Without
    /// this step the un-reduced conditional is treated as assignable from any
    /// argument, so a forbidden argument is silently accepted (missing TS2345).
    /// This mirrors tsc's instantiate-then-check: the parameter type is
    /// instantiated with the inferred type arguments and evaluated before the
    /// argument-assignability test.
    ///
    /// Higher-order callback parameters that still carry the function's tracked
    /// type parameters are left untouched so callback-inference placeholders
    /// survive. Genuinely deferred conditionals (still containing free type
    /// parameters from an outer scope) are also left alone — `evaluate_type`
    /// keeps those deferred.
    pub(super) fn finalize_instantiated_param_type(
        &mut self,
        param_type: TypeId,
        infer_subst: &TypeSubstitution,
        tracked_type_params: &[TypeParamInfo],
    ) -> TypeId {
        if self
            .function_like_type_param_appears_in_parameter_position(param_type, tracked_type_params)
        {
            return param_type;
        }
        let normalized = if infer_subst.is_empty() {
            param_type
        } else {
            self.normalize_inferred_placeholder_type(param_type, infer_subst)
        };
        // A conditional / `Exclude` (alias application of a conditional) parameter
        // type whose free type parameters are all fixed is fully concrete and must
        // be reduced (e.g. to `never`) before the argument check, so a forbidden
        // argument reaches a `never` parameter and is rejected (TS2345). Use the
        // checker's evaluator so that `extends` operands and alias applications
        // referencing named types (`keyof R`, `Exclude<K, 'a'>`, interface refs)
        // resolve through the type environment — the interner alone cannot resolve
        // `Lazy(DefId)` references. Restricting this to the conditional family
        // keeps every other parameter shape on its prior reduction path.
        if self.type_reduces_via_conditional(normalized)
            && !crate::visitor::contains_free_type_parameters(
                self.interner.as_type_database(),
                normalized,
            )
        {
            return self.checker.evaluate_type(normalized);
        }
        // Pre-existing behavior: top-level applications (e.g. `Promise<T>`) are
        // reduced through the interner once a substitution has been applied.
        if !infer_subst.is_empty()
            && matches!(
                self.interner.lookup(normalized),
                Some(TypeData::Application(_))
            )
        {
            return self.interner.evaluate_type(normalized);
        }
        normalized
    }

    /// Whether a fresh literal inferred for `tp_name` should be preserved (not
    /// widened to its primitive) so a conditional / `Exclude` parameter
    /// referencing it can reduce to `never` (issue #9652).
    ///
    /// This is a deliberately narrow stopgap, not the full tsc `widenLiteralTypes`
    /// / `inference.topLevel` model. tsc's `topLevel` is a *runtime* property of
    /// where each inference candidate came from (cleared by callback-return,
    /// array-element, intersection-member, … contributions), which a static
    /// signature inspection cannot reproduce faithfully. To avoid changing
    /// inference for unrelated shapes — and regressing conformance — preservation
    /// is restricted to the case the bug needs: the type parameter is at the top
    /// level of the return type and flows into a conditional / `Exclude`
    /// parameter that can reduce to `never`. Every other shape keeps its prior
    /// (widening) behavior. Generalizing to tsc's full model is a follow-up.
    pub(super) fn type_param_preserves_inferred_literal(
        &mut self,
        func: &FunctionShape,
        tp_name: tsz_common::Atom,
    ) -> bool {
        if !self.type_param_at_top_level_through_aliases(func.return_type, tp_name) {
            return false;
        }
        let param_types: Vec<TypeId> = func.params.iter().map(|p| p.type_id).collect();
        param_types.iter().any(|&param_type| {
            crate::visitor::contains_type_parameter_named(
                self.interner.as_type_database(),
                param_type,
                tp_name,
            ) && self.type_reduces_via_conditional(param_type)
        })
    }

    /// Whether `ty` is — or, for a top-level alias application such as
    /// `Exclude<K, 'a'>`, expands to — a type that contains a conditional. These
    /// are the parameter shapes that can reduce to `never` once their type
    /// arguments are fixed.
    fn type_reduces_via_conditional(&mut self, ty: TypeId) -> bool {
        if self.type_contains_conditional(ty) {
            return true;
        }
        if matches!(self.interner.lookup(ty), Some(TypeData::Application(_)))
            && let Some(expanded) = self.checker.expand_type_alias_application(ty)
            && expanded != ty
        {
            return self.type_contains_conditional(expanded);
        }
        false
    }

    /// Like `is_type_parameter_at_top_level`, but expands a top-level alias
    /// application (e.g. `Exclude<K, 'a'>` -> `K extends 'a' ? never : K`) so the
    /// type parameter's top-level position inside the alias body is visible.
    fn type_param_at_top_level_through_aliases(
        &mut self,
        ty: TypeId,
        tp_name: tsz_common::Atom,
    ) -> bool {
        if crate::visitor::is_type_parameter_at_top_level(
            self.interner.as_type_database(),
            ty,
            tp_name,
        ) {
            return true;
        }
        if matches!(self.interner.lookup(ty), Some(TypeData::Application(_)))
            && let Some(expanded) = self.checker.expand_type_alias_application(ty)
            && expanded != ty
        {
            return crate::visitor::is_type_parameter_at_top_level(
                self.interner.as_type_database(),
                expanded,
                tp_name,
            );
        }
        false
    }

    /// Mark the inference variable a packed rest-argument tuple constrains
    /// with its literal-preservation mode (tsc's `getSpreadArgumentType`).
    ///
    /// Marks nothing — leaving the previous blanket widening in force — when
    /// the packed tuple carries no literal element or the rest type parameter
    /// has no declared constraint, so the per-element gate could never
    /// preserve anything anyway. Otherwise the mode is `ContextuallyFixed`
    /// when the call has an outer contextual type and the rest type parameter
    /// occurs at top level of the return type: tsc then fixes the parameter
    /// before packing the arguments, the per-index contextual type `T[i]`
    /// instantiates to a concrete type, and bare primitive constraint
    /// elements (`string`, `number`, …) no longer preserve literal arguments.
    /// Without that early fixing `T[i]` stays instantiable and its base
    /// constraint preserves matching literal kinds (`Unfixed`).
    pub(super) fn mark_spread_rest_literal_mode(
        &mut self,
        infer_ctx: &mut InferenceContext,
        func: &FunctionShape,
        rest_target_type: TypeId,
        packed_tuple: TypeId,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        type_param_vars: &[crate::inference::infer::InferenceVar],
    ) {
        use crate::inference::spread_rest_literals::SpreadRestLiteralMode;
        let has_literal_element = match self.interner.lookup(packed_tuple) {
            Some(TypeData::Tuple(elems_id)) => {
                self.interner.tuple_list(elems_id).iter().any(|elem| {
                    !elem.rest
                        && crate::inference::spread_rest_literals::literal_primitive_of(
                            self.interner.as_type_database(),
                            elem.type_id,
                        )
                        .is_some()
                })
            }
            _ => false,
        };
        if !has_literal_element {
            return;
        }
        let Some(var) = self.spread_rest_infer_var(rest_target_type, var_map) else {
            return;
        };
        let Some(tp) = func
            .type_params
            .iter()
            .zip(type_param_vars.iter())
            .find_map(|(tp, &candidate)| (candidate == var).then_some(tp))
        else {
            return;
        };
        if tp.constraint.is_none() {
            return;
        }
        let mode = if self.contextual_type.is_some()
            && self.type_param_at_top_level_through_aliases(func.return_type, tp.name)
        {
            SpreadRestLiteralMode::ContextuallyFixed
        } else {
            SpreadRestLiteralMode::Unfixed
        };
        infer_ctx.mark_spread_rest_var(var, mode);
    }

    /// The inference variable a spread-built rest tuple is constrained
    /// against: the rest parameter's bare type-parameter placeholder, or the
    /// single variadic inference-variable element of a tuple-typed rest
    /// parameter (`...args: [number, ...T]`). The target arrives already
    /// readonly-unwrapped from `rest_tuple_inference_target`.
    fn spread_rest_infer_var(
        &self,
        rest_target_type: TypeId,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
    ) -> Option<crate::inference::infer::InferenceVar> {
        if let Some(&var) = var_map.get(&rest_target_type) {
            return Some(var);
        }
        if let Some(TypeData::Tuple(elems_id)) = self.interner.lookup(rest_target_type) {
            return self
                .interner
                .tuple_list(elems_id)
                .iter()
                .find(|elem| elem.rest && var_map.contains_key(&elem.type_id))
                .map(|elem| var_map[&elem.type_id]);
        }
        None
    }

    fn type_contains_conditional(&self, ty: TypeId) -> bool {
        crate::visitor::collect_all_types(self.interner.as_type_database(), ty)
            .into_iter()
            .any(|t| matches!(self.interner.lookup(t), Some(TypeData::Conditional(_))))
    }

    /// Collapse transient inference placeholders (like `__infer_src_*`) to stable types.
    ///
    /// The generic call pipeline uses temporary type parameter placeholders for
    /// contextually-instantiated callback arguments. If one of those placeholders
    /// survives as an inferred result, we normalize it through the current
    /// substitution map and fall back to `unknown` if it remains unresolved.
    ///
    /// Uses iterative `instantiate_type` to resolve placeholders within compound
    /// types (e.g., `Array(__infer_src_0)` → `Array(__infer_0)` → `Array(number)`).
    pub(super) fn normalize_inferred_placeholder_type(
        &self,
        ty: TypeId,
        infer_subst: &TypeSubstitution,
    ) -> TypeId {
        if infer_subst.is_empty() {
            return ty;
        }

        // Iteratively apply substitution to resolve transitive placeholders.
        // Each pass may resolve one level (e.g., __infer_src_0 → __infer_0[] → number[]).
        let mut current = ty;
        for _ in 0..8 {
            let next = instantiate_type(self.interner, current, infer_subst);
            if next == current {
                break;
            }
            current = next;
        }

        let preserved_source_placeholders: rustc_hash::FxHashSet<_> =
            if let Some(TypeData::Function(shape_id)) = self.interner.lookup(current) {
                self.interner
                    .function_shape(shape_id)
                    .type_params
                    .iter()
                    .filter_map(|tp| tp.is_infer_source().then_some(tp.name))
                    .collect()
            } else {
                rustc_hash::FxHashSet::default()
            };

        let mut source_placeholder_subst = TypeSubstitution::new();
        let mut contains_index_access = false;
        for ty in crate::visitor::collect_all_types(self.interner.as_type_database(), current) {
            contains_index_access |=
                matches!(self.interner.lookup(ty), Some(TypeData::IndexAccess(_, _)));
            if let Some(TypeData::TypeParameter(info)) = self.interner.lookup(ty)
                && info.is_infer_source()
                && !preserved_source_placeholders.contains(&info.name)
            {
                source_placeholder_subst.insert(info.name, TypeId::UNKNOWN);
            }
        }
        if !source_placeholder_subst.is_empty() {
            tsz_common::perf_counters::record_inference_source_placeholder_unknown_fallback(
                source_placeholder_subst.len() as u64,
                contains_index_access,
            );
            current = instantiate_type(self.interner, current, &source_placeholder_subst);
        }

        self.prune_placeholder_union_members(current)
    }

    /// Default any *free* inference placeholder that leaked into the finalized
    /// call **return type** to its constraint, or to `unknown` when it has none.
    ///
    /// When a type parameter appears only in a nested/curried position that
    /// never receives an inference candidate — e.g. `S` in
    /// `zipWith<T, S, U>(a: T[], f: (x: T) => (y: S) => U): U[]` called with a
    /// generic `pair: <T, S>(x: T) => (y: S) => { x: T; y: S }` — its resolution
    /// can settle on a bare, self-referential inference placeholder (`__infer_N`)
    /// that then rides into another inferred type parameter (here
    /// `U = { x: number; y: __infer_N }`). `tsc` resolves an uninferable type
    /// parameter to its constraint, or to `unknown` when it is unconstrained
    /// (`getInferredType`), so the internal placeholder must never survive into
    /// the result type. Constrained parameters already resolve to their
    /// constraint before this point and so never leak, but honoring
    /// `info.constraint` here keeps the fallback faithful regardless.
    ///
    /// Placeholders that
    /// [`Self::hoist_source_placeholders_into_return_type`] /
    /// [`Self::hoist_resolved_type_params_into_return_type`] re-generalized into
    /// the return function's own type-parameter list are genuine bound
    /// parameters (TypeScript 3.4 higher-order inference results), so they are
    /// preserved rather than defaulted.
    pub(super) fn default_leaked_return_type_placeholders(&self, return_type: TypeId) -> TypeId {
        // `free_type_parameter_ids_in` is binder-aware and memoized: it reports
        // only *free* type parameters and skips a nested generic signature's own
        // type-parameter list, so placeholders the `hoist_*` steps re-generalized
        // into the return function's own parameters are excluded automatically.
        // A fully-resolved return type contributes no ids, keeping the common
        // (leak-free) path a single cached lookup.
        let mut subst = TypeSubstitution::new();
        for ty in crate::visitors::visitor_predicates::free_type_parameter_ids_in(
            self.interner.as_type_database(),
            [return_type],
        ) {
            if let Some(TypeData::TypeParameter(info)) = self.interner.lookup(ty)
                && info.is_infer_placeholder()
            {
                subst.insert(info.name, info.constraint.unwrap_or(TypeId::UNKNOWN));
            }
        }
        if subst.is_empty() {
            return return_type;
        }
        instantiate_type(self.interner, return_type, &subst)
    }

    /// Default any *leaked* call-local inference placeholder (`InferPlaceholder`
    /// origin, historically `__infer_N`) that is NOT one of the current call's
    /// own tracked placeholders to its constraint (when concrete) or `unknown`.
    ///
    /// Such a placeholder is a call-local inference variable minted for a
    /// *nested* generic call — for example a curried callback's uninferable type
    /// parameter reachable only through an inner (contravariant) parameter slot —
    /// that never received an inference candidate. `tsc`'s `getInferredType`
    /// resolves an uninferable type parameter to its constraint or `unknown` and
    /// never lets an internal placeholder ride into the finalized result type,
    /// including when the placeholder was captured inside another type
    /// parameter's inferred value (issue #15461).
    ///
    /// The higher-order *source* placeholder (`__infer_src_*`) equivalent is
    /// already handled by [`Self::normalize_inferred_placeholder_type`]; this
    /// covers the call-local `InferPlaceholder` family. The current call's own
    /// placeholders are preserved (they are substituted to their resolved values
    /// through the normal placeholder substitution, and the tautology-breaking
    /// revert in `finish_generic_call_resolution` relies on them surviving).
    pub(super) fn default_leaked_inference_placeholders(
        &self,
        ty: TypeId,
        own_placeholder_atoms: &FxHashSet<tsz_common::Atom>,
    ) -> TypeId {
        // Cheap short-circuiting gate: the common (no-leak) resolved type carries
        // no call-local placeholder at all, so avoid materializing every subtype
        // via `collect_all_types` unless one is actually present.
        if !crate::type_queries::data::contains_current_infer_placeholder_db(
            self.interner.as_type_database(),
            ty,
        ) {
            return ty;
        }
        let mut subst = TypeSubstitution::new();
        for member in crate::visitor::collect_all_types(self.interner.as_type_database(), ty) {
            let Some(TypeData::TypeParameter(info)) = self.interner.lookup(member) else {
                continue;
            };
            if !info.is_current_infer_placeholder() || own_placeholder_atoms.contains(&info.name) {
                continue;
            }
            // An uninferable parameter defaults to its constraint when that
            // constraint is already concrete, otherwise to `unknown` (matches
            // `tsc`'s `getInferredType` / `getConstraintOfTypeParameter` fallback).
            let fallback = match info.constraint {
                Some(constraint)
                    if !crate::visitor::contains_type_parameters(
                        self.interner.as_type_database(),
                        constraint,
                    ) =>
                {
                    constraint
                }
                _ => TypeId::UNKNOWN,
            };
            subst.insert(info.name, fallback);
        }
        // `instantiate_type` no-ops on an empty substitution (all leaked
        // placeholders belonged to this call), so no explicit guard is needed.
        instantiate_type(self.interner, ty, &subst)
    }

    pub(super) fn normalize_inferred_placeholder_type_preserving_source_placeholders(
        &self,
        ty: TypeId,
        infer_subst: &TypeSubstitution,
    ) -> TypeId {
        if infer_subst.is_empty() {
            return ty;
        }

        let mut current = ty;
        for _ in 0..8 {
            let next = instantiate_type(self.interner, current, infer_subst);
            if next == current {
                break;
            }
            current = next;
        }

        self.prune_placeholder_union_members(current)
    }

    pub(super) fn remove_unresolved_source_placeholders_from_substitution(
        &self,
        subst: &mut TypeSubstitution,
    ) {
        let names = subst
            .map()
            .iter()
            .filter_map(|(&name, &ty)| {
                // Substitution keys are bare atoms; classify by name here.
                (ty == TypeId::UNKNOWN
                    && super::atom_names_source_inference_placeholder(
                        self.interner.resolve_atom(name).as_str(),
                    ))
                .then_some(name)
            })
            .collect::<Vec<_>>();
        for name in names {
            subst.remove(name);
        }
    }

    fn prune_placeholder_union_members(&self, ty: TypeId) -> TypeId {
        let Some(TypeData::Union(member_list_id)) = self.interner.lookup(ty) else {
            return ty;
        };

        let members = self.interner.type_list(member_list_id);
        // Prune only members carrying a *free* (live, transient) inference
        // placeholder. A bare `TypeData::Infer` reachable only through a type
        // parameter's conditional-alias constraint (e.g. `K extends Key` where
        // `type Key = R extends { key: infer T } ? T : …`) is definitional —
        // bound by that conditional, already resolved at the definition site —
        // not a leaked inference variable, so it must not drop the member.
        // `contains_infer_types_db` walked into those constraint chains and
        // collapsed `undefined | Opt<K>` to bare `undefined` for a cross-module
        // generic fn whose type parameter has a conditional-type constraint
        // (spurious TS2345, issue #14753). `contains_free_infer_types` treats
        // constraint/default and deferred-operation operands as bound, matching
        // tsc, while still pruning genuinely free leaked placeholders.
        let retained: Vec<_> = members
            .iter()
            .copied()
            .filter(|member| {
                !crate::visitor::contains_free_infer_types(
                    self.interner.as_type_database(),
                    *member,
                )
            })
            .collect();

        if retained.is_empty() || retained.len() == members.len() {
            return ty;
        }

        if retained.len() == 1 {
            retained[0]
        } else {
            self.interner.union_preserve_members(retained)
        }
    }

    /// Computes contextual types for function parameters after Round 1 inference.
    ///
    /// This is used by the Checker to implement two-pass argument checking:
    /// 1. Checker checks non-contextual arguments (arrays, primitives)
    /// 2. Checker calls this method to run Round 1 inference on those arguments
    /// 3. This method returns the current type substitution (with fixed variables)
    /// 4. Checker uses the substitution to construct contextual types for lambdas
    /// 5. Checker checks lambdas with those contextual types (Round 2)
    ///
    /// # Arguments
    /// * `func` - The function shape being called
    /// * `arg_types` - The types of all arguments (both contextual and non-contextual)
    ///
    /// # Returns
    /// A `TypeSubstitution` mapping type parameter placeholder names to their
    /// inferred types after Round 1 inference. The Checker can use this to
    /// instantiate parameter types for contextual arguments.
    pub fn compute_contextual_types(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> TypeSubstitution {
        use crate::types::InferencePriority;
        let _has_context_sensitive_args = arg_types
            .iter()
            .copied()
            .any(|arg| self.is_contextually_sensitive(arg));

        // Save state to prevent pollution if evaluator is reused
        let previous_defaulted = std::mem::take(&mut self.defaulted_placeholders);

        // #14344 / #14345 HKT-reduce lever (default-OFF, byte-parity): build the
        // arena-invariant store-only resolver shim BEFORE `infer_ctx` so it
        // outlives the inference context whose `resolver` field will borrow it.
        // OFF (or no store) leaves `shim = None`, so the literal `with_query_db`
        // resolver (the `QueryCache`) is kept and the path is byte-identical.
        let hkt_reduce_shim = self.build_inference_hkt_reduce_shim();
        let mut infer_ctx = InferenceContext::with_query_db(self.interner);
        if let Some(ref shim) = hkt_reduce_shim {
            infer_ctx.resolver = Some(shim);
        }
        let mut substitution = TypeSubstitution::new();
        substitution.protect_type_parameters(&func.type_params);
        let mut var_map: FxHashMap<TypeId, crate::inference::infer::InferenceVar> =
            FxHashMap::default();
        let mut type_param_vars = Vec::with_capacity(func.type_params.len());
        // Store each placeholder's atom *and* its mint-time inference id so that
        // reconstruction sites below re-intern the byte-identical
        // `TypeParamInfo` (same `origin`), preserving the placeholder's
        // interned `TypeId`. The `origin` field participates in interning
        // identity, so a reconstruction with a mismatched origin would split the
        // placeholder into two distinct types and break higher-order inference.
        let mut type_param_placeholder_atoms: Vec<(tsz_common::Atom, u64)> =
            Vec::with_capacity(func.type_params.len());

        self.constraint_pairs.borrow_mut().clear();
        self.constraint_fixed_union_members.borrow_mut().clear();
        self.constraint_recursion_depth.set(0);
        self.constraint_step_count.set(0);

        let mut placeholder_probe_map: FxHashMap<TypeId, InferenceVar> = FxHashMap::default();
        let mut placeholder_visited = FxHashSet::default();
        // Reusable buffer for placeholder names (avoids per-iteration String allocation)
        let mut placeholder_buf = String::with_capacity(24);

        // 1. Create inference variables and placeholders for each type parameter
        for tp in &func.type_params {
            let var = infer_ctx.fresh_var();
            type_param_vars.push(var);

            let placeholder_mint_id = self.checker.next_inference_placeholder_id();
            write_placeholder_name(&mut placeholder_buf, placeholder_mint_id);
            let placeholder_atom = self.interner.intern_string(&placeholder_buf);
            infer_ctx.register_type_param(placeholder_atom, var, tp.is_const);
            let placeholder_key = TypeData::TypeParameter(TypeParamInfo {
                is_const: tp.is_const,
                name: placeholder_atom,
                constraint: tp.constraint,
                default: None,
                origin: crate::types::TypeParamOrigin::InferPlaceholder {
                    id: placeholder_mint_id,
                },
            });
            let placeholder_id = self.interner.intern(placeholder_key);

            substitution.insert(tp.name, placeholder_id);
            var_map.insert(placeholder_id, var);
            type_param_placeholder_atoms.push((placeholder_atom, placeholder_mint_id));

            // Track defaulted placeholders to prevent union inference in constrain_types
            if tp.default.is_some() {
                self.defaulted_placeholders.insert(placeholder_id);
            }

            // NOTE: We intentionally do NOT add the type parameter constraint as an
            // upper bound here. The constraint is already part of the TypeParameter
            // declaration and is used as a fallback in Pass 2 (when no inference
            // candidates or upper bounds exist). Adding it as an upper bound during
            // inference initialization would pollute the contextual substitution:
            // when the return type context provides a specific type (e.g.,
            // `(a: number) => void`), creating an intersection with the constraint
            // (e.g., `(...args: any[]) => any`) produces a merged Callable whose
            // conflicting parameter types cause `get_parameter_type` to return None,
            // triggering false TS7006 errors.

            // Mirror tsc's `widenLiteralTypes` gate (checker.ts ~26595): if the
            // type parameter occurs at the top level of the signature's return
            // type and has not yet been fixed, fresh literal candidates must
            // NOT be widened during this Round 1 → Round 2 substitution. This
            // preserves literal target types like `U=1` for context-sensitive
            // arguments whose contextual signature embeds the type parameter
            // (e.g. `cb: (a: T) => U`). All type parameters are unfixed at this
            // point in `compute_contextual_types`, so the flag is conditioned
            // solely on the structural top-level test.
            if crate::visitor::is_type_parameter_at_top_level(
                self.interner.as_type_database(),
                func.return_type,
                tp.name,
            ) {
                infer_ctx.mark_top_level_in_return_type_unfixed(var);
            }
        }

        // 2. Instantiate parameters with placeholders
        let mut instantiated_params: Vec<ParamInfo> = func
            .params
            .iter()
            .map(|p| ParamInfo {
                suppress_display_optional: false,
                name: p.name,
                type_id: instantiate_type(self.interner, p.type_id, &substitution),
                optional: p.optional,
                rest: p.rest,
            })
            .collect();
        let mut round1_direct_seed_vars = FxHashSet::default();
        for (i, &arg_type) in arg_types.iter().enumerate() {
            let Some(target_type) =
                self.param_type_for_arg_index(&instantiated_params, i, arg_types.len())
            else {
                break;
            };
            if self
                .contextual_round1_arg_types(arg_type, target_type)
                .is_some()
            {
                round1_direct_seed_vars.extend(self.collect_placeholder_vars_in_type(
                    target_type,
                    &var_map,
                    &mut placeholder_probe_map,
                    &mut placeholder_visited,
                ));
            }
        }

        // 2.5. Seed contextual constraints from return type
        // Skip `any` and `unknown` — they don't contribute useful inference constraints.
        // Seed even when there are context-sensitive args: for patterns like
        //   assign<T>(fn: (x: T) => void): Action<T>
        // called in a context expecting Action<"counter">, the return context is the
        // only source of inference for T. Without seeding here, the Round 1 substitution
        // is empty and callback parameters lose their contextual types.
        if let Some(ctx_type) = self.contextual_type
            && ctx_type != TypeId::ANY
            && ctx_type != TypeId::UNKNOWN
        {
            let return_type_with_placeholders =
                instantiate_type(self.interner, func.return_type, &substitution);
            let return_seed_vars = self.collect_placeholder_vars_in_type(
                return_type_with_placeholders,
                &var_map,
                &mut placeholder_probe_map,
                &mut placeholder_visited,
            );
            // Same logic as primary resolve path: skip only when ALL return vars
            // are covered by round-1 inference.
            let all_return_vars_covered = !return_seed_vars.is_empty()
                && return_seed_vars
                    .iter()
                    .all(|var| round1_direct_seed_vars.contains(var));
            if !all_return_vars_covered {
                self.constrain_types(
                    &mut infer_ctx,
                    &var_map,
                    return_type_with_placeholders,
                    ctx_type,
                    InferencePriority::ReturnType,
                );

                self.constrain_return_context_structure(
                    &mut infer_ctx,
                    &var_map,
                    return_type_with_placeholders,
                    ctx_type,
                    InferencePriority::ReturnType,
                );
            }
        }

        let structural_return_subst = {
            // When the source function's return type is a bare type parameter
            // that also appears in its parameter list (like `f<T>(x: T): T`),
            // and the contextual type is a function (from
            // instantiate_generic_function_argument_against_target), extract
            // the function's RETURN TYPE for return context substitution.
            // Without this, `f<T>(x: T): T` passed as `(x: number) => number`
            // would substitute T → (x: number) => number (the full function type)
            // instead of T → number (the return type), causing false TS2322.
            let return_type_is_param_shared_with_params = func.type_params.iter().any(|tp| {
                let ret_is_this_param = matches!(
                    self.interner.lookup(func.return_type),
                    Some(TypeData::TypeParameter(ref info)) if info.name == tp.name
                );
                let param_uses_this_param = func.params.iter().any(|p| {
                    crate::visitor::collect_referenced_types(
                        self.interner.as_type_database(),
                        p.type_id,
                    )
                    .into_iter()
                    .any(|ty| {
                        crate::type_param_info(self.interner.as_type_database(), ty)
                            .is_some_and(|info| info.name == tp.name)
                    })
                });
                ret_is_this_param && param_uses_this_param
            });

            let ctx_for_return = if return_type_is_param_shared_with_params {
                self.contextual_type.map(|ctx| {
                    // Only extract return type from CONCRETE function types
                    // (no type parameters). When the contextual is generic
                    // (e.g. (x: U) => T from outer inference), keep the full
                    // function type so inner inference sees the structure.
                    if crate::visitor::contains_type_parameters(
                        self.interner.as_type_database(),
                        ctx,
                    ) {
                        ctx
                    } else if let Some(fn_shape) = crate::type_queries::get_function_shape(
                        self.interner.as_type_database(),
                        ctx,
                    ) {
                        fn_shape.return_type
                    } else {
                        ctx
                    }
                })
            } else {
                self.contextual_type
            };
            self.compute_return_context_substitution(func, ctx_for_return)
        };
        if !structural_return_subst.is_empty() {
            for (&name, &ty) in structural_return_subst.map().iter() {
                substitution.insert(name, ty);
            }
            instantiated_params = func
                .params
                .iter()
                .map(|p| ParamInfo {
                    suppress_display_optional: false,
                    name: p.name,
                    type_id: instantiate_type(self.interner, p.type_id, &substitution),
                    optional: p.optional,
                    rest: p.rest,
                })
                .collect();
        }

        // 3. Round 1: Process non-contextual arguments only
        let rest_tuple_inference =
            self.rest_tuple_inference_target(&instantiated_params, arg_types, &var_map);
        let rest_tuple_start = rest_tuple_inference.as_ref().map(|(start, _, _, _)| *start);

        for (i, &arg_type) in arg_types.iter().enumerate() {
            if rest_tuple_start.is_some_and(|start| i >= start) {
                continue;
            }
            let Some(target_type) =
                self.param_type_for_arg_index(&instantiated_params, i, arg_types.len())
            else {
                break;
            };
            if self.arg_targets_aggregate_rest_param(&instantiated_params, i, arg_type) {
                continue;
            }

            let Some((contextual_arg_type, contextual_target_type)) =
                self.contextual_round1_arg_types(arg_type, target_type)
            else {
                self.constrain_sensitive_function_return_types(
                    &mut infer_ctx,
                    &var_map,
                    arg_type,
                    target_type,
                    InferencePriority::NakedTypeVariable,
                );
                continue;
            };

            // Add constraint for non-contextual arguments
            self.constrain_types(
                &mut infer_ctx,
                &var_map,
                contextual_arg_type,
                contextual_target_type,
                InferencePriority::NakedTypeVariable,
            );

            let source_is_function = self.type_evaluates_to_function(contextual_arg_type);
            let target_is_function = self.type_evaluates_to_function(contextual_target_type);
            if source_is_function || target_is_function {
                self.constrain_return_context_structure(
                    &mut infer_ctx,
                    &var_map,
                    contextual_arg_type,
                    contextual_target_type,
                    InferencePriority::NakedTypeVariable,
                );
            }

            if let (
                Some(TypeData::Application(arg_app_id)),
                Some(TypeData::Application(target_app_id)),
            ) = (
                self.interner.lookup(contextual_arg_type),
                self.interner.lookup(contextual_target_type),
            ) {
                let arg_app = self.interner.type_application(arg_app_id);
                let target_app = self.interner.type_application(target_app_id);
                if arg_app.base == target_app.base
                    && arg_app.args.len() == target_app.args.len()
                    && self.should_directly_constrain_same_base_application(
                        contextual_arg_type,
                        contextual_target_type,
                    )
                {
                    for (arg_inner, target_inner) in arg_app.args.iter().zip(target_app.args.iter())
                    {
                        self.constrain_types(
                            &mut infer_ctx,
                            &var_map,
                            *arg_inner,
                            *target_inner,
                            InferencePriority::NakedTypeVariable,
                        );
                    }
                }
            }
        }

        // Process rest tuple in Round 1
        if let Some((_start, target_type, tuple_type, _inference_vars)) = rest_tuple_inference {
            self.mark_spread_rest_literal_mode(
                &mut infer_ctx,
                func,
                target_type,
                tuple_type,
                &var_map,
                &type_param_vars,
            );
            self.constrain_types(
                &mut infer_ctx,
                &var_map,
                tuple_type,
                target_type,
                InferencePriority::NakedTypeVariable,
            );
        }

        // 4. Fix variables with enough information from Round 1
        let _ = infer_ctx.fix_current_variables_with(Some(|source, target| {
            self.checker.is_assignable_to(source, target)
        }));

        // Restore state to prevent pollution if evaluator is reused
        self.defaulted_placeholders = previous_defaulted;

        // 5. Remap substitution to use original type parameter names.
        // get_current_substitution() returns keys like "__infer_0", but the Checker
        // needs keys matching the original type parameter names (e.g., "T", "U")
        // so that instantiate_type can find and replace TypeParameter nodes.
        let infer_subst = infer_ctx.get_current_substitution();
        let mut result_subst = TypeSubstitution::new();
        result_subst.protect_type_parameters(&func.type_params);

        // Pass 1: Collect all resolved (non-UNKNOWN) type parameters
        let mut unresolved_indices = Vec::new();
        for (i, tp) in func.type_params.iter().enumerate() {
            let (placeholder_atom, _placeholder_mint_id) = type_param_placeholder_atoms[i];
            // Skip the preferred_lower_bound optimization in compute_contextual_types.
            // Unlike resolve_generic_call_inner (which gates this on direct_param_vars
            // for parameters where the type IS the type parameter, like f<T>(x: T)),
            // compute_contextual_types lacks that tracking. Applying it unconditionally
            // can remove genuine candidates that happen to match the constraint type,
            // leading to false positives when the constraint is also a valid candidate
            // from object property inference.
            let preferred_lower_bound: Option<TypeId> = None;
            let resolved = preferred_lower_bound.or_else(|| {
                match infer_ctx.resolve_with_constraints_by(type_param_vars[i], |source, target| {
                    self.checker.is_assignable_to_strict(source, target)
                }) {
                    Ok(resolved) => Some(resolved),
                    Err(_) => self
                        .single_concrete_upper_bound(&mut infer_ctx, type_param_vars[i])
                        .or_else(|| infer_subst.get(placeholder_atom)),
                }
            });
            if let Some(resolved) = resolved {
                let resolved = self.normalize_inferred_placeholder_type(resolved, &infer_subst);
                let resolved = if let Some(contextual_ty) = structural_return_subst.get(tp.name) {
                    if self.can_apply_contextual_return_substitution(
                        &mut infer_ctx,
                        type_param_vars[i],
                        resolved,
                        &var_map,
                    ) && self.should_use_contextual_return_substitution(
                        resolved,
                        contextual_ty,
                        &var_map,
                    ) {
                        contextual_ty
                    } else {
                        resolved
                    }
                } else {
                    resolved
                };
                if resolved != TypeId::UNKNOWN {
                    result_subst.insert(tp.name, resolved);
                } else {
                    unresolved_indices.push(i);
                }
            } else {
                unresolved_indices.push(i);
            }
        }

        // Pass 2: For unresolved type params, try using the default or constraint
        // instantiated with already-resolved params as a contextual type.
        // Priority: default > constraint > placeholder (the default is what the type IS
        // when no argument is provided; the constraint is just an upper bound).
        // As a last resort, use the inference placeholder (__infer_N) so that callbacks
        // get unique placeholder types instead of the callee's raw type parameters,
        // which avoids name collisions with outer scope type parameters of the same name.
        for i in unresolved_indices {
            let tp = &func.type_params[i];
            // Try default first — this determines the contextual type when no inference
            // happened (e.g. `<T = TypegenDisabled>` should use TypegenDisabled, not the
            // constraint `TypegenEnabled | TypegenDisabled`).
            if let Some(default) = tp.default {
                let inst_default = instantiate_type(self.interner, default, &result_subst);
                if !crate::visitor::contains_type_parameters(
                    self.interner.as_type_database(),
                    inst_default,
                ) {
                    result_subst.insert(tp.name, inst_default);
                    continue;
                }
            }
            // Fall back to constraint if default didn't resolve.
            // This enables contextual typing for patterns like:
            //   test<TContext, TFn extends (ctx: TContext) => void>(context: TContext, fn: TFn)
            // where TContext is inferred in Round 1 but TFn needs its constraint.
            if let Some(constraint) = tp.constraint {
                let inst_constraint = instantiate_type(self.interner, constraint, &result_subst);
                if !crate::visitor::contains_type_parameters(
                    self.interner.as_type_database(),
                    inst_constraint,
                ) {
                    result_subst.insert(tp.name, inst_constraint);
                    continue;
                }
            }
            // Last resort: use the inference placeholder so callbacks get unique
            // placeholder types instead of the callee's raw type parameter.
            // This ensures that `foo((x) => 1, (x) => '')` produces arg types with
            // unique placeholder names instead of `T`, avoiding name collisions.
            {
                let (placeholder_atom, placeholder_mint_id) = type_param_placeholder_atoms[i];
                // Re-intern the byte-identical placeholder minted above (same
                // `origin`, so the same interned `TypeId`).
                let placeholder_key = TypeData::TypeParameter(TypeParamInfo {
                    is_const: tp.is_const,
                    name: placeholder_atom,
                    constraint: tp.constraint,
                    default: None,
                    origin: crate::types::TypeParamOrigin::InferPlaceholder {
                        id: placeholder_mint_id,
                    },
                });
                let placeholder_id = self.interner.intern(placeholder_key);
                result_subst.insert(tp.name, placeholder_id);
            }
        }

        result_subst
    }
}
