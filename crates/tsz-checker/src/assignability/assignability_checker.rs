//! Type assignability and excess property checking.
//! Subtype, identity, and redeclaration compatibility live in `subtype_identity_checker`.

use crate::query_boundaries::assignability::{
    AssignabilityEvalKind, classify_for_assignability_eval, get_keyof_type,
    get_string_literal_value, get_union_members, keyof_object_properties, map_compound_members,
};
use crate::query_boundaries::definition_identity::symbol_ref_to_symbol_id;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

thread_local! {
    /// `TypeId`s whose assignability-display evaluation is in flight on this
    /// thread.
    ///
    /// Keys are interner-instance-local, so the set must be empty between
    /// compilations: a leaked entry would make a fresh `TypeId` reusing the
    /// same value read as `already_visiting` and return unevaluated,
    /// suppressing the normalized form an assignability diagnostic depends on.
    /// Membership is owned by the RAII [`AssignabilityEvalVisitGuard`], which
    /// removes the entry on drop — on the normal return path, the fuel-bail
    /// path, *and* when `evaluate_type_for_assignability_inner` unwinds via a
    /// panic a caller (`try_tsz`, LSP) catches and swallows mid-recursion.
    /// (Before #13368 the removals were manual post-call statements skipped on
    /// unwind, leaking the key into the next compilation on a reused worker
    /// thread.)
    static ASSIGNABILITY_EVAL_VISITING: std::cell::RefCell<FxHashSet<TypeId>> =
        std::cell::RefCell::new(FxHashSet::default());
}

/// RAII membership guard for the assignability-display evaluation walk.
///
/// [`enter`](Self::enter) returns `None` when `type_id` is already being
/// evaluated on this thread; the caller returns the type unevaluated. Otherwise
/// it records membership and clears it on drop, so the set is restored even if
/// evaluation unwinds.
#[must_use]
struct AssignabilityEvalVisitGuard(TypeId);

impl AssignabilityEvalVisitGuard {
    fn enter(type_id: TypeId) -> Option<Self> {
        ASSIGNABILITY_EVAL_VISITING.with(|visiting| {
            if visiting.borrow_mut().insert(type_id) {
                Some(Self(type_id))
            } else {
                None
            }
        })
    }

    /// Whether `type_id`'s evaluation is already in flight on this thread,
    /// without taking membership. Used for the pre-memo cycle short-circuit
    /// that must return the type opaque rather than serve a memoized result.
    fn is_visiting(type_id: TypeId) -> bool {
        ASSIGNABILITY_EVAL_VISITING.with(|visiting| visiting.borrow().contains(&type_id))
    }
}

impl Drop for AssignabilityEvalVisitGuard {
    fn drop(&mut self) {
        ASSIGNABILITY_EVAL_VISITING.with(|visiting| {
            visiting.borrow_mut().remove(&self.0);
        });
    }
}

impl<'a> CheckerState<'a> {
    /// Merge overflow flags into the checker context (sticky: only ever sets to `true`).
    ///
    /// Callers that need a fresh read must reset the context fields before
    /// invoking the relation.
    #[inline]
    pub(crate) fn propagate_overflow_flags(&self, depth_exceeded: bool, iteration_exceeded: bool) {
        let mut overflow = self.ctx.relation_overflow.get();
        overflow.merge(depth_exceeded, iteration_exceeded);
        self.ctx.relation_overflow.set(overflow);
    }

    pub(crate) fn callable_has_own_generic_signatures(&self, type_id: TypeId) -> bool {
        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
        {
            return !shape.type_params.is_empty();
        }
        if let Some(shape) =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
        {
            return shape
                .call_signatures
                .iter()
                .any(|sig| !sig.type_params.is_empty())
                || shape
                    .construct_signatures
                    .iter()
                    .any(|sig| !sig.type_params.is_empty());
        }
        false
    }

    /// Check if a callable type's parameters contain type parameters within intersections.
    /// This distinguishes narrowed callback parameters (e.g., `(x: number & T) => void`)
    /// from callbacks with standalone enclosing-scope type parameters (e.g., `(x: T) => void`).
    pub(crate) fn callable_params_contain_type_param_intersection(&self, type_id: TypeId) -> bool {
        let params = if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
        {
            shape.params.iter().map(|p| p.type_id).collect::<Vec<_>>()
        } else if let Some(shape) =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
        {
            shape
                .call_signatures
                .iter()
                .flat_map(|sig| sig.params.iter().map(|p| p.type_id))
                .collect::<Vec<_>>()
        } else {
            return false;
        };
        params.iter().any(|&param_type| {
            if let Some(members) =
                crate::query_boundaries::common::intersection_members(self.ctx.types, param_type)
            {
                members.iter().any(|&m| {
                    crate::query_boundaries::assignability::contains_type_parameters(
                        self.ctx.types,
                        m,
                    )
                })
            } else {
                false
            }
        })
    }

    /// Check if an argument node is a callback (arrow function or function expression)
    /// with unannotated parameters that rely on contextual typing.
    pub(crate) fn arg_is_callback_with_unannotated_params(&self, arg_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(arg_idx) else {
            return false;
        };

        let is_callback = node.kind == syntax_kind_ext::ARROW_FUNCTION
            || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION;

        if !is_callback {
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.ctx.arena.get_parenthesized(node)
            {
                return self.arg_is_callback_with_unannotated_params(paren.expression);
            }
            return false;
        }

        let Some(func) = self.ctx.arena.get_function(node) else {
            return false;
        };

        func.parameters.nodes.iter().any(|&param_idx| {
            self.ctx
                .arena
                .get(param_idx)
                .and_then(|pn| self.ctx.arena.get_parameter(pn))
                .is_some_and(|p| {
                    p.type_annotation.is_none()
                        && self.ctx.arena.get(p.name).is_some_and(|name_node| {
                            name_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN
                                && name_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN
                        })
                })
        })
    }

    /// Returns the parameter count of a callback argument's function expression.
    /// Returns `None` if `arg_idx` is not an arrow/function expression (or a
    /// parenthesized one).
    fn callback_argument_param_count(&self, arg_idx: NodeIndex) -> Option<usize> {
        let node = self.ctx.arena.get(arg_idx)?;
        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            let paren = self.ctx.arena.get_parenthesized(node)?;
            return self.callback_argument_param_count(paren.expression);
        }
        if node.kind != syntax_kind_ext::ARROW_FUNCTION
            && node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return None;
        }
        let func = self.ctx.arena.get_function(node)?;
        Some(func.parameters.nodes.len())
    }

    /// Returns true when `target` exposes at least one callable signature whose
    /// parameter list can supply contextual types for every parameter of the
    /// unannotated callback at `arg_idx`.
    ///
    /// A signature can supply contextual types when it has a rest parameter, or
    /// when its fixed parameter count is at least the source callback's
    /// parameter count. When the target is not a recognizably callable type, we
    /// conservatively answer `true` so that the existing suppression behavior
    /// for non-trivial target shapes (unions, generics, etc.) is preserved —
    /// the bug we are guarding against is the concrete case where the target
    /// has *fewer* parameters than the source.
    pub(crate) fn target_can_contextually_type_callback_params(
        &self,
        arg_idx: NodeIndex,
        target: TypeId,
    ) -> bool {
        let Some(source_param_count) = self.callback_argument_param_count(arg_idx) else {
            return true;
        };
        let db = self.ctx.types;
        if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(db, target) {
            return signature_has_param_capacity(&shape.params, source_param_count);
        }
        if let Some(shape) = crate::query_boundaries::common::callable_shape_for_type(db, target) {
            let any_call_ok = shape
                .call_signatures
                .iter()
                .any(|sig| signature_has_param_capacity(&sig.params, source_param_count));
            let any_construct_ok = shape
                .construct_signatures
                .iter()
                .any(|sig| signature_has_param_capacity(&sig.params, source_param_count));
            return any_call_ok || any_construct_ok;
        }
        true
    }

    /// Returns true when a callback-like function type still has unresolved
    /// `any`/`unknown` parameter types, meaning contextual typing did not
    /// concretely bind its parameters yet.
    pub(crate) fn callback_type_params_are_unresolved(&self, arg_type: TypeId) -> bool {
        if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(
            self.ctx.types.as_type_database(),
            arg_type,
        ) {
            shape.params.is_empty()
                || shape
                    .params
                    .iter()
                    .all(|p| matches!(p.type_id, TypeId::ANY | TypeId::UNKNOWN))
        } else {
            false
        }
    }

    fn normalize_nested_type_for_assignability(&mut self, type_id: TypeId) -> TypeId {
        // Depth guard: prevents stack overflow from mutually recursive types
        // (e.g., Foo<T> ↔ Bar<T>) where each fresh visited set misses
        // cross-function cycles. The decrement is owned by an RAII guard so the
        // depth is restored on every exit — including when the inner walk
        // unwinds via a panic a caller (`try_tsz`, LSP) catches mid-recursion.
        // A manual post-call decrement would be skipped on unwind, leaking a
        // positive depth into the next compilation on a reused batch worker
        // thread; later normalizations at that stale depth would bail at the cap
        // and return the type unnormalized, suppressing the assignability
        // comparison and making diagnostics schedule-dependent (#13368). The
        // counter is function-private, so the batch boundary reset cannot reach
        // it — RAII self-cleaning is the only correct isolation here.
        thread_local! { static DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) }; }
        struct DepthReset;
        impl Drop for DepthReset {
            fn drop(&mut self) {
                DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
            }
        }
        let depth = DEPTH.with(|d| {
            let v = d.get();
            d.set(v + 1);
            v
        });
        let _depth_reset = DepthReset;
        if depth >= 10 {
            return type_id;
        }
        let mut visited = FxHashSet::default();
        self.normalize_nested_type_for_assignability_inner(type_id, &mut visited)
    }

    fn normalize_nested_type_for_assignability_inner(
        &mut self,
        type_id: TypeId,
        visited: &mut FxHashSet<TypeId>,
    ) -> TypeId {
        if !visited.insert(type_id) {
            return type_id;
        }

        let resolved = self.resolve_type_query_type(type_id);
        let evaluated = if crate::query_boundaries::common::type_application(
            self.ctx.types,
            resolved,
        )
        .is_some()
        {
            self.evaluate_type_for_assignability(resolved)
        } else {
            self.evaluate_type_with_env(resolved)
        };
        let type_id = if evaluated == TypeId::UNKNOWN && resolved != TypeId::UNKNOWN {
            resolved
        } else if evaluated != resolved {
            evaluated
        } else {
            resolved
        };

        if let Some(inner) =
            crate::query_boundaries::common::get_readonly_inner(self.ctx.types, type_id)
        {
            let normalized = self.normalize_nested_type_for_assignability_inner(inner, visited);
            if normalized != inner {
                self.ctx.types.readonly_type(normalized)
            } else {
                type_id
            }
        } else if let Some(inner) =
            crate::query_boundaries::common::get_noinfer_inner(self.ctx.types, type_id)
        {
            let normalized = self.normalize_nested_type_for_assignability_inner(inner, visited);
            if normalized != inner {
                self.ctx.types.no_infer(normalized)
            } else {
                type_id
            }
        } else if let Some(elem) =
            crate::query_boundaries::common::array_element_type(self.ctx.types, type_id)
        {
            if crate::query_boundaries::common::is_array_type(self.ctx.types, type_id) {
                let normalized = self.normalize_nested_type_for_assignability_inner(elem, visited);
                if normalized != elem {
                    self.ctx.types.array(normalized)
                } else {
                    type_id
                }
            } else {
                type_id
            }
        } else if let Some(elements) =
            crate::query_boundaries::common::tuple_elements(self.ctx.types, type_id)
        {
            if crate::query_boundaries::common::is_tuple_type(self.ctx.types, type_id) {
                let mut changed = false;
                let normalized_elements: Vec<_> = elements
                    .iter()
                    .map(|elem| {
                        let normalized = self
                            .normalize_nested_type_for_assignability_inner(elem.type_id, visited);
                        if normalized != elem.type_id {
                            changed = true;
                        }
                        tsz_solver::TupleElement {
                            type_id: normalized,
                            name: elem.name,
                            optional: elem.optional,
                            rest: elem.rest,
                        }
                    })
                    .collect();
                if changed {
                    self.ctx.types.factory().tuple(normalized_elements)
                } else {
                    type_id
                }
            } else {
                type_id
            }
        } else if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, type_id)
        {
            let mut changed = false;
            let normalized_members: Vec<_> = members
                .iter()
                .map(|&member| {
                    let normalized =
                        self.normalize_nested_type_for_assignability_inner(member, visited);
                    if normalized != member {
                        changed = true;
                    }
                    normalized
                })
                .collect();
            if changed {
                self.ctx.types.factory().union(normalized_members)
            } else {
                type_id
            }
        } else if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
        {
            let mut changed = false;
            let normalized_members: Vec<_> = members
                .iter()
                .map(|&member| {
                    let normalized =
                        self.normalize_nested_type_for_assignability_inner(member, visited);
                    if normalized != member {
                        changed = true;
                    }
                    normalized
                })
                .collect();
            if changed {
                self.ctx.types.factory().intersection(normalized_members)
            } else {
                type_id
            }
        } else {
            type_id
        }
    }

    fn normalize_function_shape_for_assignability(
        &mut self,
        shape: &tsz_solver::FunctionShape,
    ) -> Option<tsz_solver::FunctionShape> {
        use crate::query_boundaries::construct_signatures::{
            FunctionShapeTypeSlot, map_function_shape_types,
        };

        let own_tp_names: Vec<_> = shape.type_params.iter().map(|tp| tp.name).collect();

        map_function_shape_types(shape, |slot, type_id| {
            // Component types that mention the shape's own type parameters
            // must stay as declared so inference can still bind them; type
            // queries and conditionals in return position are deferred forms
            // whose normalization is owned elsewhere.
            let references_own_tp = !own_tp_names.is_empty()
                && own_tp_names.iter().any(|&name| {
                    crate::query_boundaries::common::contains_type_parameter_named(
                        self.ctx.types,
                        type_id,
                        name,
                    )
                });
            let skip = match slot {
                FunctionShapeTypeSlot::Param => references_own_tp,
                FunctionShapeTypeSlot::Return => {
                    references_own_tp
                        || crate::query_boundaries::common::is_type_query_type(
                            self.ctx.types,
                            type_id,
                        )
                        || crate::query_boundaries::common::is_conditional_type(
                            self.ctx.types,
                            type_id,
                        )
                }
                FunctionShapeTypeSlot::This | FunctionShapeTypeSlot::PredicateTarget => false,
            };
            if skip {
                type_id
            } else {
                self.normalize_nested_type_for_assignability(type_id)
            }
        })
    }

    fn normalize_callable_type_for_assignability(&mut self, type_id: TypeId) -> TypeId {
        use crate::query_boundaries::construct_signatures::{
            call_signature_from_function_shape, callable_with_signatures_replaced,
            function_shape_from_call_signature, function_type_from_shape,
        };

        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
        {
            let result = self
                .normalize_function_shape_for_assignability(&shape)
                .map(|shape| function_type_from_shape(self.ctx.types, shape))
                .unwrap_or(type_id);
            return result;
        }
        if let Some(shape) =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
        {
            let mut changed = false;
            let mut normalize_signature =
                |checker: &mut Self, sig: &tsz_solver::CallSignature, is_constructor: bool| {
                    let normalized = checker.normalize_function_shape_for_assignability(
                        &function_shape_from_call_signature(sig, is_constructor),
                    );
                    if normalized.is_some() {
                        changed = true;
                    }
                    normalized.map_or_else(
                        || sig.clone(),
                        |shape| call_signature_from_function_shape(shape, sig.is_method),
                    )
                };
            let call_signatures: Vec<_> = shape
                .call_signatures
                .iter()
                .map(|sig| normalize_signature(self, sig, false))
                .collect();
            let construct_signatures: Vec<_> = shape
                .construct_signatures
                .iter()
                .map(|sig| normalize_signature(self, sig, true))
                .collect();

            if changed {
                callable_with_signatures_replaced(
                    self.ctx.types,
                    &shape,
                    call_signatures,
                    construct_signatures,
                )
            } else {
                type_id
            }
        } else {
            type_id
        }
    }

    pub(crate) fn get_keyof_type_keys(
        &mut self,
        type_id: TypeId,
        db: &dyn tsz_solver::construction::TypeDatabase,
    ) -> FxHashSet<Atom> {
        if let Some(keyof_type) = get_keyof_type(db, type_id)
            && let Some(key_type) = keyof_object_properties(db, keyof_type)
            && let Some(members) = get_union_members(db, key_type)
        {
            return members
                .into_iter()
                .filter_map(|m| {
                    if let Some(str_lit) = get_string_literal_value(db, m) {
                        return Some(str_lit);
                    }
                    None
                })
                .collect();
        }
        FxHashSet::default()
    }

    /// Ensure relation preconditions (lazy refs + application symbols) for one type.
    pub(crate) fn ensure_relation_input_ready(&mut self, type_id: TypeId) {
        if type_id.is_intrinsic() {
            return;
        }
        // Do NOT gate on lazy-resolution session fuel here. The inner
        // guards inside ensure_refs_resolved (and ensure_application_symbols_resolved)
        // already exit the materialization worklist when global fuel is exhausted,
        // bounding total work per call to O(1).  Gating the entire readiness step
        // here caused subsequent DOM/lib relation checks in the same file to skip
        // their input step entirely, silently dropping TS2322/TS2345 diagnostics
        // after the first large-lib type graph was materialized (issue #12144).
        self.ensure_refs_resolved(type_id);
        self.ensure_application_symbols_resolved(type_id);
    }

    /// Ensure relation preconditions (lazy refs + application symbols) for multiple types.
    pub(crate) fn ensure_relation_inputs_ready(&mut self, type_ids: &[TypeId]) {
        for &type_id in type_ids {
            self.ensure_relation_input_ready(type_id);
        }
    }

    /// Ready the relation inputs that *calling* `callee_type` consumes —
    /// argument checking against its parameters, `this`, type-parameter bounds,
    /// and predicate — while **deferring the return type** when the callee is
    /// directly a function type.
    ///
    /// # Why (issue #13983)
    ///
    /// A call's return type is the call *result*: it is selected and carried as
    /// the result type, not relationally consumed during argument checking.
    /// [`Self::ensure_relation_input_ready`] readies the *whole* callee type,
    /// which (transitively, via [`Self::ensure_refs_resolved`]) force-resolved
    /// the return interface and its currently pre-flattened heritage closure on
    /// every call — e.g. `document.getElementById("x")` lowered all of
    /// `HTMLElement`/`Element`/`Node`/`EventTarget` just to type a call whose
    /// result the caller may never touch. Property reads of the same interface
    /// already stay lazy via the single-member fast path
    /// ([`crate::state_checking::lazy_lib_member`]); this gives method-call
    /// returns the same treatment.
    ///
    /// # Soundness
    ///
    /// The deferral is transparent, **not** a bound (contrast the rejected
    /// static `ensure_relation_input_ready` bound in #13979): the carried result
    /// `TypeId` is the identical `Lazy(DefId)` reference either way — only the
    /// timing of populating the type environment with the def's body changes. A
    /// consuming relation/evaluation forces the def on demand at the
    /// [`crate::context::CheckerContext::resolve_lazy`] miss via
    /// `force_def_on_miss` (the #14016 mechanism), so every resolution-dependent
    /// path observes the same resolved type.
    ///
    /// Every *input* position is still readied eagerly. Only the callee's own
    /// top-level `return_type` is replaced (with an intrinsic) before the
    /// readiness walk; parameters keep their full structure, so a parameter that
    /// is itself a callback (`(e: HTMLElement) => void`) still has its nested
    /// return readied — a relation against that callback *does* consume it, and
    /// leaving it unresolved would reintroduce the #12144 "unresolved `Lazy`
    /// treated as compatible" hazard.
    ///
    /// The optimization applies only when `callee_type` is *directly* a function
    /// type ([`tsz_solver::TypeData::Function`]); overload sets, callable
    /// objects, and lazy/application/union callee forms take the unchanged
    /// full-readiness path.
    pub(crate) fn ensure_callee_relation_inputs_ready(&mut self, callee_type: TypeId) {
        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, callee_type)
            && self.call_return_is_lazy_lib_deferrable(shape.return_type)
        {
            let inputs_probe =
                self.ctx
                    .types
                    .factory()
                    .function(crate::query_boundaries::common::FunctionShape {
                        return_type: TypeId::UNKNOWN,
                        ..(*shape).clone()
                    });
            self.ensure_relation_input_ready(inputs_probe);
            return;
        }
        self.ensure_relation_input_ready(callee_type);
    }

    /// Whether a call's `return_type` may have its referenced interfaces left
    /// unresolved during argument-checking readiness (deferred to on-demand
    /// forcing) — see [`Self::ensure_callee_relation_inputs_ready`].
    ///
    /// Deferral is restricted to the **provably resolution-independent** shape:
    /// a union of intrinsics and *bare* `Lazy(DefId)` references to
    /// force-eligible simple lib interfaces (non-generic, unmerged, unaugmented,
    /// unshadowed — [`Self::force_eligible_lib_def`]), with at least one such
    /// lib reference to defer. A force-eligible lib interface resolves
    /// identically in every requester/arena context, so populating the type
    /// environment with its body on demand (at the consuming `resolve_lazy`
    /// miss) yields the same resolved type the eager pre-walk would have — the
    /// carried result `TypeId` is unchanged either way.
    ///
    /// Anything resolution-*dependent* in the return — type parameters,
    /// `Application`s (e.g. `Promise<T>`, `Array<T>`), conditionals, mapped /
    /// index-access / `infer` / template-literal types, function/object shapes,
    /// or a `Lazy` to a non-lib / generic / augmented def — disqualifies it.
    /// Those returns are read structurally by downstream computation (async
    /// Promise-unwrap, index-access elaboration, `instanceof`, tuple growth,
    /// utility-type expansion) whose results are cached as canonical node types;
    /// deferring their refs would change computed type identity (the hazard
    /// traced in #13979). They take the unchanged full-readiness path.
    fn call_return_is_lazy_lib_deferrable(&self, return_type: TypeId) -> bool {
        // Fast path for the most common return shapes (bare `void`/primitive): a
        // single intrinsic references no def to defer, so skip the classifier
        // walk and allocation on this per-call hot path.
        if return_type.is_intrinsic() {
            return false;
        }
        // Structural classification (union of intrinsics + bare `Lazy` refs) is
        // owned by the solver query boundary; the checker only layers the
        // force-eligibility (binder/symbol) judgement on each referenced def.
        let Some(def_ids) = crate::query_boundaries::common::union_of_bare_lazy_def_ids(
            self.ctx.types,
            return_type,
        ) else {
            return false;
        };
        !def_ids.is_empty()
            && def_ids
                .iter()
                .all(|&def_id| self.force_eligible_lib_def(def_id))
    }

    // =========================================================================
    // Type Evaluation for Assignability
    // =========================================================================

    /// Ensure the type's *root* Lazy/Ref refs are resolved into the type
    /// environment so a relation can start consuming it.
    ///
    /// # On-demand forcing (#12101)
    ///
    /// Historically this did an eager **transitive** pre-walk: it pushed every
    /// resolved `DefId` body back onto the worklist and recursively materialized
    /// the whole referenced graph (e.g. the entire DOM/webworker heritage
    /// closure) up front. That transitive walk was ~53% of comlink check time
    /// even though comlink structurally consumes only a handful of lib
    /// interfaces.
    ///
    /// With on-demand forcing enabled (the default; toggle via
    /// `TSZ_DISABLE_ON_DEMAND_FORCING`) the transitive push is dropped: only the
    /// `type_id`'s **own** directly-referenced `DefId`s are resolved here so the
    /// relation can begin. Tail interfaces reached transitively (a member's type,
    /// a heritage base) stay `Lazy(DefId)` and are materialized on demand when a
    /// relation/evaluation structurally consumes them — at the
    /// [`CheckerContext::resolve_lazy`] miss via
    /// [`CheckerContext::force_def_on_miss`], or explicitly by the `&mut`
    /// consumers that need the full shape (property-access materialization,
    /// keyof/spread, await-unwrap). `refs_resolved` therefore now means "root
    /// forced", not "transitively walked".
    pub(crate) fn ensure_refs_resolved(&mut self, type_id: TypeId) {
        use crate::state_checking::lazy_lib_member::on_demand_forcing_disabled;
        use crate::state_domain::type_environment::lazy_guard_state::{
            RefsResolutionWorkState, refs_resolution_work_state,
        };

        if self.ctx.refs_resolved.contains(&type_id) {
            return;
        }

        // Default: on-demand forcing. The legacy eager transitive pre-walk is
        // only used when the kill-switch is set, for byte-parity comparison.
        let transitive = on_demand_forcing_disabled();

        let eval_session = std::rc::Rc::clone(&self.ctx.eval_session);
        let _refs_scope = eval_session.enter_refs_resolution_scope();

        let mut visited_types = FxHashSet::default();
        let mut visited_def_ids = FxHashSet::default();
        let mut worklist = vec![type_id];

        while let Some(current) = worklist.pop() {
            match refs_resolution_work_state(eval_session.refs_resolution_fuel_exhausted(), false) {
                RefsResolutionWorkState::Continue => {}
                RefsResolutionWorkState::RefsFuelExhausted
                | RefsResolutionWorkState::GlobalFuelExhausted => break,
            }

            if !visited_types.insert(current) {
                continue;
            }

            for symbol_ref in self
                .ctx
                .collect_type_queries_cached(current)
                .iter()
                .copied()
            {
                let sym_id = symbol_ref_to_symbol_id(symbol_ref);
                let _ = self.get_type_of_symbol(sym_id);
                // Populate type_env with the VALUE type (constructor for classes) so that
                // TypeEvaluator::visit_type_query can resolve via TypeEnvironment::resolve_ref.
                // Without this, resolve_ref returns None and the fallback resolve_lazy returns
                // the INSTANCE type for classes, causing false TS2345 on `typeof ClassName` args.
                if let Some(value_type) = self.ctx.symbol_types.get(&sym_id) {
                    // Route through the env-write authority (dual-write + defer
                    // on borrow race instead of silently skipping; #14348).
                    self.ctx.register_symbol_type_in_envs(
                        tsz_solver::SymbolRef(sym_id.0),
                        value_type,
                        Vec::new(),
                    );
                }
            }

            for &def_id in self.ctx.collect_lazy_def_ids_cached(current).iter() {
                match refs_resolution_work_state(
                    eval_session.refs_resolution_fuel_exhausted(),
                    false,
                ) {
                    RefsResolutionWorkState::Continue => {}
                    RefsResolutionWorkState::RefsFuelExhausted
                    | RefsResolutionWorkState::GlobalFuelExhausted => break,
                }
                if !visited_def_ids.insert(def_id) {
                    continue;
                }
                eval_session.increment_refs_resolution_fuel();
                eval_session.increment_lazy_resolution_fuel();
                let at_fuel_limit = eval_session.lazy_resolution_fuel_exhausted();
                // Always call resolve_and_insert_def_type even when global fuel is
                // exhausted: the call is typically a fast cache hit for lib types that
                // were computed during type-environment building, and the resolver needs
                // the TypeEnvironment entry to evaluate a Lazy(def_id) during
                // assignability checks.  Without this, exhausted-fuel calls silently
                // leave subsequent DOM/lib type refs unresolvable, causing the relation
                // checker to treat unresolved Lazy types as compatible (issue #12144).
                // When at the fuel limit we still resolve the direct def_id but skip
                // adding its result to the worklist so transitive work stays bounded.
                //
                // On-demand forcing (#12101): when `def_id` is a force-eligible simple
                // lib interface, its referenced tail (members, heritage bases) is made
                // of lib refs that `CheckerContext::force_def_on_miss` materializes on
                // demand at the consuming `resolve_lazy` miss, so its body is NOT
                // pushed back onto the worklist — this is what drops the eager
                // DOM/webworker heritage-graph pre-walk. For every other def
                // (cross-file class/namespace, user types, generic/augmented lib
                // interfaces) the transitive push is preserved so its tail is
                // materialized exactly as the legacy eager path did, keeping
                // byte-parity. With the kill-switch set, `transitive` is always true
                // (legacy eager pre-walk).
                let push_tail = transitive || !self.force_eligible_lib_def(def_id);
                if let Some(result) = self.resolve_and_insert_def_type(def_id)
                    && result != TypeId::ERROR
                    && result != TypeId::ANY
                    && !at_fuel_limit
                    && push_tail
                {
                    worklist.push(result);
                }
                if at_fuel_limit {
                    match refs_resolution_work_state(false, at_fuel_limit) {
                        RefsResolutionWorkState::GlobalFuelExhausted
                        | RefsResolutionWorkState::RefsFuelExhausted => break,
                        RefsResolutionWorkState::Continue => {}
                    }
                }
            }
        }
        self.ctx.refs_resolved.insert(type_id);
    }

    /// Session-state stamp for the [`crate::context::AssignabilityEvalMemo`]
    /// and the [`crate::context::AssignabilityFailureMemo`].
    ///
    /// `None` when either type environment is currently mutably borrowed; the
    /// memos are skipped entirely for such re-entrant calls.
    pub(crate) fn assignability_eval_memo_stamp(
        &self,
    ) -> Option<crate::context::AssignabilityEvalStamp> {
        let env_generation = self.ctx.type_env.try_borrow().ok()?.generation();
        let environment_generation = self.ctx.type_environment.try_borrow().ok()?.generation();
        Some((
            env_generation,
            environment_generation,
            self.ctx.symbol_types.version(),
            self.ctx.symbol_instance_types.version(),
        ))
    }

    /// Evaluate a type for assignability checking.
    ///
    /// Determines if the type needs evaluation (applications, env-dependent types)
    /// and performs the appropriate evaluation.
    ///
    /// Completed calls are memoized per checker session: the recursive
    /// normalization below is deterministic while the type environments and
    /// symbol-type caches are unchanged, and assignability paths re-request
    /// the same `TypeId`s heavily (~94% repeated outermost calls on the
    /// ts-toolbelt project row, issue #8356, with nested repeats called out in
    /// issue #13243). The active recursion stack still wins over the memo, so
    /// re-entered types still evaluate to themselves.
    pub(crate) fn evaluate_type_for_assignability(&mut self, type_id: TypeId) -> TypeId {
        if type_id.is_intrinsic() {
            return type_id;
        }

        // Inside a diagnostic display-budget scope, evaluation results are
        // memoized and total evaluation work is fuel-bounded (issue #13040).
        // Self-expanding application chains intern fresh types per
        // evaluation, so the cycle set below never converges on them; the
        // fuel guarantees rendering one type does bounded work. Outside a
        // scope (relation/semantic paths) both are inert.
        if let Some(cached) = crate::error_reporter::display_budget::cached_eval(type_id) {
            return cached;
        }

        // Re-entrant cycle: return the type opaque. This check precedes the
        // memo lookup on purpose — an in-flight type must not be served a
        // memoized result computed by a now-superseded outer evaluation.
        if AssignabilityEvalVisitGuard::is_visiting(type_id) {
            return type_id;
        }

        if let Some(stamp) = self.assignability_eval_memo_stamp()
            && let Some(memoized) = self
                .ctx
                .type_reference_validation_caches
                .assignability_eval_memo
                .get(stamp, type_id)
        {
            return memoized;
        }

        // Take membership for the duration of the evaluation. The guard removes
        // the entry on every exit below (normal return, fuel bail, or unwind).
        let Some(_visit_guard) = AssignabilityEvalVisitGuard::enter(type_id) else {
            return type_id;
        };

        if !crate::error_reporter::display_budget::try_consume_eval_fuel() {
            return type_id;
        }

        let result = self.evaluate_type_for_assignability_inner(type_id);
        // Cycle-truncated returns above are never recorded — only complete
        // results are safe to replay for later calls in this scope.
        crate::error_reporter::display_budget::record_eval(type_id, result);

        // Memoize only clean completions: fuel-exhausted or depth-clamped
        // evaluations are degraded forms a fresher evaluation must improve on.
        // The stamp is recomputed on purpose: evaluation grows the type
        // environments, and the result is valid for that *post*-evaluation
        // state; the lookup-time stamp would file the entry as already stale.
        if result != TypeId::ERROR
            && !self.ctx.eval_session.refs_resolution_fuel_exhausted()
            && !self.ctx.eval_session.lazy_resolution_fuel_exhausted()
            && !self.ctx.depth_exceeded.get()
            && let Some(stamp) = self.assignability_eval_memo_stamp()
        {
            self.ctx
                .type_reference_validation_caches
                .assignability_eval_memo
                .insert(stamp, type_id, result);
        }
        result
    }

    pub(super) fn evaluate_type_for_assignability_inner(&mut self, type_id: TypeId) -> TypeId {
        if let Some(evaluated) = self.evaluate_lazy_alias_for_assignability(type_id) {
            return evaluated;
        }
        if let Some(distributed) = self.distribute_intersection_union_for_assignability(type_id) {
            return distributed;
        }

        let kind = classify_for_assignability_eval(self.ctx.types, type_id);
        let mut evaluated = match kind {
            AssignabilityEvalKind::Application => {
                let result = self.evaluate_type_with_resolution(type_id);
                // Guard: if evaluation degraded a valid type to ERROR (e.g., due to
                // stack overflow protection tripping during deep recursive type
                // resolution), preserve the original type. ERROR is treated as
                // assignable to/from everything by the subtype checker, which would
                // silently suppress real type errors like TS2322. Keeping the original
                // Lazy type allows the compat checker's resolver to resolve it from the
                // type environment (populated during earlier successful resolution).
                if result == TypeId::ERROR && type_id != TypeId::ERROR {
                    return type_id;
                }
                result
            }
            AssignabilityEvalKind::NeedsEnvEval => {
                // For TypeQuery (typeof), resolve the value type directly from
                // get_type_of_symbol. The TypeEnvironment's types map may contain
                // the instance type for class symbols (stored by type-position
                // resolution paths like resolve_lazy_def_for_type_env), but
                // TypeQuery needs the value-position type (constructor for classes).
                if let Some(symbol_ref) = crate::query_boundaries::common::type_query_symbol(
                    self.ctx.types.as_type_database(),
                    type_id,
                ) {
                    let sym_id = symbol_ref_to_symbol_id(symbol_ref);
                    // For merged TYPE_ALIAS + VARIABLE symbols (e.g.,
                    // `type Input = Static<typeof Input>` + `const Input = ...`),
                    // get_type_of_symbol may return the type alias's circular
                    // Lazy(DefId) instead of the value's concrete type. Since
                    // TypeQuery always refers to the value side, resolve directly
                    // from the value declaration to avoid TS2344 false positives.
                    let flags = self
                        .ctx
                        .binder
                        .get_symbol(sym_id)
                        .map(|s| s.flags)
                        .unwrap_or(0);
                    if (flags & tsz_binder::symbol_flags::TYPE_ALIAS) != 0
                        && (flags & tsz_binder::symbol_flags::VARIABLE) != 0
                    {
                        let value_decl = self
                            .ctx
                            .binder
                            .get_symbol(sym_id)
                            .map(|s| s.value_declaration)
                            .unwrap_or(tsz_parser::NodeIndex::NONE);
                        self.type_of_value_declaration_for_symbol(sym_id, value_decl)
                    } else {
                        self.get_type_of_symbol(sym_id)
                    }
                } else {
                    self.evaluate_type_with_env(type_id)
                }
            }
            AssignabilityEvalKind::Resolved => type_id,
        };

        if evaluated != type_id && evaluated != TypeId::ERROR && evaluated != TypeId::ANY {
            let further = self.evaluate_type_for_assignability(evaluated);
            if further != TypeId::ERROR && further != TypeId::ANY {
                evaluated = further;
            }
        }

        // Distribution pass: normalize compound types so mixed representations do not
        // leak into relation checks (for example, `Lazy(Class)` + resolved class object).
        if let Some(distributed) = self.distribute_intersection_union_for_assignability(evaluated) {
            evaluated = distributed;
        } else if let Some(distributed) =
            map_compound_members(self.ctx.types, evaluated, |member| {
                self.evaluate_type_for_assignability(member)
            })
        {
            evaluated = distributed;
        }

        // tsc expands homomorphic mapped type applications (e.g. `PassThrough<A|B>`)
        // before structural comparison; mirror that for tuple elements.
        if let Some(elements) =
            crate::query_boundaries::common::tuple_elements(self.ctx.types, evaluated)
        {
            let mut any_changed = false;
            let new_elements: Vec<tsz_solver::TupleElement> = elements
                .iter()
                .map(|elem| {
                    if matches!(
                        classify_for_assignability_eval(self.ctx.types, elem.type_id),
                        AssignabilityEvalKind::Resolved
                    ) {
                        return *elem;
                    }
                    let elem_eval = self.evaluate_type_for_assignability(elem.type_id);
                    if elem_eval != elem.type_id {
                        any_changed = true;
                    }
                    tsz_solver::TupleElement {
                        type_id: elem_eval,
                        ..*elem
                    }
                })
                .collect();
            if any_changed {
                evaluated = self.ctx.types.as_type_database().tuple(new_elements);
            }
        }

        if crate::query_boundaries::assignability::remapped_mapped_type_has_no_outer_type_params(
            self.ctx.types,
            evaluated,
        ) {
            let concrete = self.evaluate_concrete_remapped_mapped_type_with_resolution(evaluated);
            if concrete != evaluated {
                evaluated = concrete;
            }
        }

        evaluated = self.evaluate_awaited_application_for_assignability(evaluated);

        evaluated = self.normalize_callable_type_for_assignability(evaluated);

        evaluated
    }

    fn distribute_intersection_union_for_assignability(
        &mut self,
        type_id: TypeId,
    ) -> Option<TypeId> {
        let members =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)?;
        let mut evaluated_members = Vec::with_capacity(members.len());
        let mut union_member_index = None;

        for member in members {
            let evaluated = self.evaluate_type_for_assignability(member);
            if union_member_index.is_none() && self.object_union_has_branch_only_keys(evaluated) {
                union_member_index = Some(evaluated_members.len());
            }
            evaluated_members.push(evaluated);
        }

        let union_member_index = union_member_index?;
        let union_members = crate::query_boundaries::common::union_members(
            self.ctx.types,
            evaluated_members[union_member_index],
        )?;
        let mut distributed = Vec::with_capacity(union_members.len());
        for branch in union_members {
            let mut branch_members = evaluated_members.clone();
            branch_members[union_member_index] = branch;
            distributed.push(self.ctx.types.factory().intersection(branch_members));
        }

        Some(self.ctx.types.factory().union_preserve_members(distributed))
    }

    fn object_union_has_branch_only_keys(&self, type_id: TypeId) -> bool {
        let Some(members) = crate::query_boundaries::common::union_members(self.ctx.types, type_id)
        else {
            return false;
        };
        if members.len() < 2 {
            return false;
        }

        let mut first_keys = None;
        for member in members {
            let Some(shape_id) =
                crate::query_boundaries::common::object_shape_id(self.ctx.types, member)
            else {
                return false;
            };
            let keys: FxHashSet<_> = self
                .ctx
                .types
                .object_shape(shape_id)
                .properties
                .iter()
                .map(|prop| prop.name)
                .collect();
            match &first_keys {
                Some(first) if first != &keys => return true,
                None => first_keys = Some(keys),
                _ => {}
            }
        }
        false
    }

    pub(super) fn concrete_remapped_mapped_assignability_target(
        &mut self,
        target: TypeId,
    ) -> Option<TypeId> {
        let resolved = self.evaluate_type_with_resolution(target);
        let mapped_id = crate::query_boundaries::common::mapped_type_id(self.ctx.types, resolved)?;
        let mapped = self.ctx.types.mapped_type(mapped_id);
        mapped.name_type?;
        let concrete = self.evaluate_concrete_remapped_mapped_type_with_resolution(resolved);
        (concrete != resolved).then_some(concrete)
    }

    /// Recursively evaluate Lazy property types within an Object type so that
    /// the solver's `types_are_comparable_for_assertion` sees concrete types
    /// instead of opaque `Lazy(DefId)` references.
    ///
    /// Recurses up to `max_depth` levels into nested Object types whose
    /// properties are Lazy.  Returns the original type unchanged if it is not
    /// an object or has no Lazy property types.
    pub(crate) fn deep_evaluate_object_properties(&mut self, type_id: TypeId) -> TypeId {
        self.deep_evaluate_object_properties_inner(type_id, 0)
    }

    fn deep_evaluate_object_properties_inner(&mut self, type_id: TypeId, depth: u32) -> TypeId {
        const MAX_DEPTH: u32 = 3;
        if depth >= MAX_DEPTH {
            return type_id;
        }

        // Tuples carry their element types directly (not via Object shape),
        // so the property-shape walk below would skip them. Resolve each
        // tuple element first so downstream comparable-for-assertion checks
        // (e.g. tuple-to-tuple element-wise overlap in
        // `types_are_comparable_for_assertion`) see concrete types instead
        // of unresolved `Lazy(DefId)` class refs — those refs short-circuit
        // the solver's depth>0 Lazy heuristic to "comparable", masking real
        // mismatches like `[C, D] as [A, I]`.
        if let Some(elements) =
            crate::query_boundaries::common::tuple_elements(self.ctx.types, type_id)
        {
            let mut any_changed = false;
            let new_elements: Vec<tsz_solver::TupleElement> = elements
                .iter()
                .map(|elem| {
                    let mut eval_ty = elem.type_id;
                    if crate::query_boundaries::common::is_lazy_type(
                        self.ctx.types.as_type_database(),
                        eval_ty,
                    ) {
                        let resolved = self.evaluate_type_for_assignability(eval_ty);
                        if resolved != eval_ty {
                            any_changed = true;
                            eval_ty = resolved;
                        }
                    }
                    let deep = self.deep_evaluate_object_properties_inner(eval_ty, depth + 1);
                    if deep != eval_ty {
                        any_changed = true;
                        eval_ty = deep;
                    }
                    tsz_solver::TupleElement {
                        type_id: eval_ty,
                        ..*elem
                    }
                })
                .collect();
            if any_changed {
                return self.ctx.types.as_type_database().tuple(new_elements);
            }
            return type_id;
        }

        let db = self.ctx.types.as_type_database();
        // Use solver query API to get the shape id (handles Object and ObjectWithIndex)
        let shape_id = match crate::query_boundaries::common::object_shape_id(db, type_id) {
            Some(sid) => sid,
            None => return type_id,
        };

        let shape = db.object_shape(shape_id);
        let mut any_changed = false;
        let new_props: Vec<tsz_solver::PropertyInfo> = shape
            .properties
            .iter()
            .map(|p| {
                let mut eval_ty = p.type_id;
                // Resolve Lazy references (interface/type alias names)
                if crate::query_boundaries::common::is_lazy_type(
                    self.ctx.types.as_type_database(),
                    eval_ty,
                ) {
                    let resolved = self.evaluate_type_for_assignability(eval_ty);
                    if resolved != eval_ty {
                        any_changed = true;
                        eval_ty = resolved;
                    }
                }
                // Recurse into resolved Object types to resolve their properties too
                let deep = self.deep_evaluate_object_properties_inner(eval_ty, depth + 1);
                if deep != eval_ty {
                    any_changed = true;
                    eval_ty = deep;
                }

                let mut eval_write = p.write_type;
                if crate::query_boundaries::common::is_lazy_type(
                    self.ctx.types.as_type_database(),
                    eval_write,
                ) {
                    let resolved = self.evaluate_type_for_assignability(eval_write);
                    if resolved != eval_write {
                        any_changed = true;
                        eval_write = resolved;
                    }
                }

                tsz_solver::PropertyInfo {
                    type_id: eval_ty,
                    write_type: eval_write,
                    ..*p
                }
            })
            .collect();

        if !any_changed {
            return type_id;
        }

        // Re-intern the object with resolved property types
        self.ctx.types.as_type_database().object(new_props)
    }

    /// Resolve a deferred Mapped type by pre-resolving its constraint's Applications.
    ///
    /// When evaluation produces a deferred Mapped type (e.g., from Omit/Pick where
    /// the constraint contains Application types like `Exclude<keyof T, K>`), the
    /// solver's `TypeEvaluator` may have failed because lib type `DefIds` weren't
    /// registered in the `TypeEnvironment`. This method resolves the constraint through
    /// the checker's evaluation path and retries the Mapped type evaluation.
    pub(crate) fn resolve_deferred_mapped_type(&mut self, type_id: TypeId) -> TypeId {
        let Some(mapped_id) = crate::query_boundaries::state::type_environment::mapped_type_id(
            self.ctx.types.as_type_database(),
            type_id,
        ) else {
            return type_id;
        };
        let mapped = self.ctx.types.mapped_type(mapped_id);
        let constraint = mapped.constraint;
        let resolved_constraint = self.evaluate_mapped_constraint_with_resolution(constraint);
        if resolved_constraint != constraint {
            self.ctx
                .cache_env_eval_result_if_absent(constraint, resolved_constraint, false);
            let retry = self.evaluate_type_with_env_uncached(type_id);
            if retry != type_id {
                return retry;
            }
        }
        type_id
    }

    // =========================================================================
    // Main Assignability Check
    // =========================================================================

    /// Substitute `ThisType` in a type with the enclosing class instance type.
    ///
    /// When inside a class body, `ThisType` represents the polymorphic `this` type
    /// (a type parameter bounded by the class). Since the `this` expression evaluates
    /// to the concrete class instance type, we must substitute `ThisType` → class
    /// instance type before assignability checks. This matches tsc's behavior where
    /// `return this`, `f(this)`, etc. succeed when the target type is `this`.
    pub(super) fn substitute_this_type_if_needed(&mut self, type_id: TypeId) -> TypeId {
        // Fast path: intrinsic types can't contain ThisType
        if type_id.is_intrinsic() {
            return type_id;
        }
        let needs_substitution =
            crate::query_boundaries::common::contains_this_type(self.ctx.types, type_id);
        if !needs_substitution {
            return type_id;
        }
        let Some(class_info) = &self.ctx.enclosing_class else {
            return type_id;
        };
        let class_idx = class_info.class_idx;

        let Some(node) = self.ctx.arena.get(class_idx) else {
            return type_id;
        };
        let Some(class_data) = self.ctx.arena.get_class(node) else {
            return type_id;
        };

        let instance_type = self.get_class_instance_type(class_idx, class_data);

        if crate::query_boundaries::common::is_this_type(self.ctx.types, type_id) {
            // Substitute bare `ThisType` with the concrete class instance type so
            // that `return this` / `f(this)` assignability succeeds by identity check.
            instance_type
        } else if crate::query_boundaries::common::index_access_types(self.ctx.types, type_id)
            .is_some()
        {
            // A direct indexed access like `this["x"]` is still anchored in the
            // current class context. Resolve it to the concrete property type for
            // assignment/call-argument checks, while leaving more complex wrappers
            // such as `Unwrap<this["x"]>` deferred below.
            let substituted = crate::query_boundaries::common::substitute_this_type(
                self.ctx.types,
                type_id,
                instance_type,
            );
            self.evaluate_type_with_env_uncached(substituted)
        } else {
            // Do NOT substitute complex types that merely contain `ThisType` in nested
            // positions (e.g. `Builder_instance` whose methods return `this`).  The
            // solver's `bind_property_receiver_this` already substitutes `this` during
            // property comparison using the object shape's receiver symbol.
            // Pre-substituting here creates a new TypeId (Builder_instance_subst) with no
            // symbol, so the subsequent `bind_property_receiver_this` call on the *target*
            // produces a Lazy/ref TypeId while the source stays as the concrete TypeId,
            // causing spurious TS2322 errors for fluent/builder patterns.
            type_id
        }
    }
}

/// A target signature can supply contextual types for `source_param_count`
/// callback parameters when it has a rest parameter (which absorbs any
/// trailing positions) or its fixed parameter list is at least that long.
fn signature_has_param_capacity(
    params: &[tsz_solver::ParamInfo],
    source_param_count: usize,
) -> bool {
    if params.iter().any(|p| p.rest) {
        return true;
    }
    params.len() >= source_param_count
}

#[cfg(test)]
mod assignability_eval_visit_guard_tests {
    use super::{AssignabilityEvalVisitGuard, TypeId};

    #[test]
    fn reentry_of_in_flight_type_is_rejected() {
        let t = TypeId(4242);
        let outer = AssignabilityEvalVisitGuard::enter(t).expect("first entry succeeds");
        assert!(AssignabilityEvalVisitGuard::is_visiting(t));
        assert!(
            AssignabilityEvalVisitGuard::enter(t).is_none(),
            "re-entering an in-flight TypeId must short-circuit"
        );
        drop(outer);
        assert!(
            !AssignabilityEvalVisitGuard::is_visiting(t),
            "drop must restore membership"
        );
    }

    /// #13368: the guard must clear membership even when evaluation unwinds via
    /// a panic a caller (`try_tsz`, LSP) catches, so a stale interner-local key
    /// can never leak into the next compilation on a reused worker thread.
    #[test]
    fn membership_is_restored_on_unwind() {
        let t = TypeId(99);
        let result = std::panic::catch_unwind(|| {
            let _guard = AssignabilityEvalVisitGuard::enter(t).expect("entry succeeds");
            assert!(AssignabilityEvalVisitGuard::is_visiting(t));
            panic!("simulated mid-evaluation panic");
        });
        assert!(result.is_err(), "the closure panicked");
        assert!(
            !AssignabilityEvalVisitGuard::is_visiting(t),
            "guard Drop must remove the key during unwind"
        );
    }
}
