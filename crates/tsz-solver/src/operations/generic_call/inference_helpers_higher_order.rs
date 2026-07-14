//! Higher-order target classification and upper-bound resolution helpers.
//!
//! Predicates and small resolvers used during generic call inference to decide
//! whether a higher-order target position may re-generalize through outer
//! inference placeholders, and to pick a single concrete upper bound for an
//! inference variable.

use crate::inference::infer::{InferenceContext, InferenceVar};
use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::types::{ParamInfo, TypeData, TypeId};
use rustc_hash::{FxHashMap, FxHashSet};

impl<C: AssignabilityChecker> CallEvaluator<'_, C> {
    /// Whether a higher-order target position is built *entirely* from outer
    /// inference placeholders the current resolution may re-generalize through,
    /// carried only by inert structural wrappers (`Array`/tuple).
    ///
    /// A position qualifies (`true`) when it is a bare accepted placeholder
    /// (`__infer_1`) or a wrapper around qualifying children
    /// (`__infer_src_3#X[]`, `[__infer_1, __infer_2]`). The wrapper case arises
    /// in round 2 of a higher-order call: once the shared middle type is fixed
    /// (`B = X[]`), the next generic argument's target parameter becomes
    /// `X_src[]` rather than a bare placeholder. Both forms must route through
    /// the generic-source constraint branch so the surviving placeholder chains
    /// into the result; a nested instantiation against the wrapper drops the
    /// foreign placeholder to `unknown`.
    ///
    /// Returns `false` the moment a non-placeholder concrete leaf (object,
    /// primitive, application, function, …) or an unaccepted placeholder is
    /// reached: a concrete position carries independent inference evidence that
    /// must pin the source parameter by instantiation, so it must not take the
    /// re-generalization path. This keeps the bare-placeholder behavior a strict
    /// subset and the overall check fail-closed. `at_least_one` is set whenever
    /// an accepted placeholder leaf is seen so the caller can reject an
    /// all-empty match.
    pub(super) fn position_is_regeneralizable_higher_order_target(
        &self,
        ty: TypeId,
        at_least_one: &mut bool,
    ) -> bool {
        match self.interner.lookup(ty) {
            Some(TypeData::TypeParameter(info)) => {
                // A higher-order source placeholder is always accepted: it is
                // minted within this resolution to carry a generic argument's
                // free param into a later argument's target. A call-local outer
                // placeholder is accepted only when it belongs to THIS call
                // (fail-closed against nested/stale state).
                let accepted = info.origin.is_infer_source()
                    || (info.origin.is_current_infer_placeholder()
                        && self
                            .current_call_inference_placeholders
                            .contains(&info.name));
                if accepted {
                    *at_least_one = true;
                }
                accepted
            }
            Some(TypeData::Array(element)) => {
                self.position_is_regeneralizable_higher_order_target(element, at_least_one)
            }
            Some(TypeData::Tuple(list_id)) => {
                let elements = self.interner.tuple_list(list_id);
                // A tuple qualifies only when it has elements and every element
                // is itself qualifying (no rest spreads, which carry their own
                // array structure with potentially concrete content).
                !elements.is_empty()
                    && elements.iter().all(|element| {
                        !element.rest
                            && self.position_is_regeneralizable_higher_order_target(
                                element.type_id,
                                at_least_one,
                            )
                    })
            }
            _ => false,
        }
    }

    /// Inference vars appearing specifically in the *parameter* positions of a
    /// callback-typed target (the outer-call placeholders carried by a
    /// `(a: X) => Y` contextual parameter). Unlike
    /// `collect_placeholder_vars_in_type`, the return position (`Y`) is
    /// excluded: only a parameter-position type parameter can be pinned by a
    /// sibling argument and thus force `SkipGenericFunctions` deferral.
    pub(super) fn collect_callback_parameter_placeholder_vars(
        &self,
        ty: TypeId,
        var_map: &FxHashMap<TypeId, InferenceVar>,
        probe_map: &mut FxHashMap<TypeId, InferenceVar>,
        visited: &mut FxHashSet<TypeId>,
    ) -> FxHashSet<InferenceVar> {
        let mut result = FxHashSet::default();
        match self.interner.lookup(ty) {
            Some(TypeData::Function(shape_id)) => {
                let shape = self.interner.function_shape(shape_id);
                for param in &shape.params {
                    result.extend(self.collect_placeholder_vars_in_type(
                        param.type_id,
                        var_map,
                        probe_map,
                        visited,
                    ));
                }
            }
            Some(TypeData::Callable(shape_id)) => {
                let shape = self.interner.callable_shape(shape_id);
                for sig in shape
                    .call_signatures
                    .iter()
                    .chain(shape.construct_signatures.iter())
                {
                    for param in &sig.params {
                        result.extend(self.collect_placeholder_vars_in_type(
                            param.type_id,
                            var_map,
                            probe_map,
                            visited,
                        ));
                    }
                }
            }
            Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => {
                let members = self.interner.type_list(list_id).to_vec();
                for member in members {
                    result.extend(self.collect_callback_parameter_placeholder_vars(
                        member, var_map, probe_map, visited,
                    ));
                }
            }
            Some(
                TypeData::Application(_)
                | TypeData::Lazy(_)
                | TypeData::Mapped(_)
                | TypeData::Conditional(_)
                | TypeData::IndexAccess(_, _),
            ) => {
                let evaluated = self.interner.evaluate_type(ty);
                if evaluated != ty {
                    result.extend(self.collect_callback_parameter_placeholder_vars(
                        evaluated, var_map, probe_map, visited,
                    ));
                }
            }
            _ => {}
        }
        result
    }

    /// Whether a generic call argument is itself a generic function-like value
    /// (a generic call/construct signature). Such an argument produces its own
    /// placeholders and is subject to deferral, so it is not a concrete pin.
    pub(super) fn arg_is_generic_function_like(&self, arg_type: TypeId) -> bool {
        match self.interner.lookup(arg_type) {
            Some(TypeData::Function(shape_id)) => !self
                .interner
                .function_shape(shape_id)
                .type_params
                .is_empty(),
            Some(TypeData::Callable(shape_id)) => {
                let shape = self.interner.callable_shape(shape_id);
                shape
                    .call_signatures
                    .iter()
                    .any(|sig| !sig.type_params.is_empty())
                    || shape
                        .construct_signatures
                        .iter()
                        .any(|sig| !sig.type_params.is_empty())
            }
            _ => false,
        }
    }

    /// Whether a sibling argument (any index other than `generic_fn_index`) is
    /// a concrete inference source that pins one of `param_pos_vars` — the
    /// callback parameter-position type variables of the generic-function
    /// argument. Only a concrete value counts: a generic-function-like or
    /// context-sensitive sibling produces its own placeholders and yields no
    /// round-1 candidate.
    pub(super) fn callback_parameter_var_pinned_by_sibling_arg(
        &mut self,
        instantiated_params: &[ParamInfo],
        arg_types: &[TypeId],
        generic_fn_index: usize,
        param_pos_vars: &FxHashSet<InferenceVar>,
        var_map: &FxHashMap<TypeId, InferenceVar>,
    ) -> bool {
        if param_pos_vars.is_empty() {
            return false;
        }
        let mut probe_map: FxHashMap<TypeId, InferenceVar> = FxHashMap::default();
        let mut visited: FxHashSet<TypeId> = FxHashSet::default();
        for (j, &sibling_arg) in arg_types.iter().enumerate() {
            if j == generic_fn_index {
                continue;
            }
            if self.arg_is_generic_function_like(sibling_arg)
                || self.is_contextually_sensitive(sibling_arg)
            {
                continue;
            }
            let Some(sibling_target) =
                self.param_type_for_arg_index(instantiated_params, j, arg_types.len())
            else {
                continue;
            };
            let sibling_vars = self.collect_placeholder_vars_in_type(
                sibling_target,
                var_map,
                &mut probe_map,
                &mut visited,
            );
            if sibling_vars.iter().any(|var| param_pos_vars.contains(var)) {
                return true;
            }
        }
        false
    }

    pub(super) fn single_concrete_upper_bound(
        &self,
        infer_ctx: &mut InferenceContext<'_>,
        var: InferenceVar,
    ) -> Option<TypeId> {
        let constraints = infer_ctx.get_constraints(var)?;
        let mut concrete_upper_bounds = constraints
            .upper_bounds
            .iter()
            .copied()
            .filter(|upper| {
                !upper.is_any_unknown_or_error()
                    && !crate::visitor::contains_type_parameters(
                        self.interner.as_type_database(),
                        *upper,
                    )
                    && !crate::type_queries::contains_infer_types_db(
                        self.interner.as_type_database(),
                        *upper,
                    )
            })
            .collect::<Vec<_>>();
        concrete_upper_bounds.dedup();
        if concrete_upper_bounds.len() == 1 {
            concrete_upper_bounds.pop()
        } else {
            None
        }
    }
}
