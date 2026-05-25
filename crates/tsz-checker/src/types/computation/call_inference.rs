//! Generic-call inference and round-2 contextual typing helpers.

mod argument_context;
mod indexed_callback;
mod iterable_substitution;
mod return_context;
mod return_context_substitution;
mod unknown_callback;

use crate::call_checker::CallableContext;
use crate::context::TypingRequest;
use crate::context::speculation::{DiagnosticSpeculationSnapshot, ImplicitAnyClosureSnapshot};
use crate::query_boundaries::checkers::call as call_checker;
use crate::query_boundaries::checkers::call::is_type_parameter_type;
use crate::query_boundaries::common;
use crate::query_boundaries::common::CallResult;
use crate::query_boundaries::common::LiteralTypeKind;
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use std::borrow::Cow;
use tsz_common::Atom;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::construction::{QueryDatabase, TypeDatabase};
use tsz_solver::{FunctionShape, TypeId};

/// Detect spread marker tuples `[...T]` created by the checker for generic
/// `TypeParameter` spreads. A spread marker is a 1-element tuple where the single
/// element is a rest element whose inner type is a `TypeParameter`.
fn is_spread_marker_tuple(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if let Some(elems) = common::tuple_elements(db, type_id) {
        elems.len() == 1 && elems[0].rest && is_type_parameter_type(db, elems[0].type_id)
    } else {
        false
    }
}

/// Count the number of non-`any` parameter types in a callable type.
///
/// Used to compare contextual type candidates: whichever has more specific
/// (non-`any`) parameter types provides better contextual typing for callbacks.
/// Returns 0 for non-callable types.
fn callable_param_specificity(db: &dyn QueryDatabase, ty: TypeId) -> usize {
    if let Some(shape) = common::function_shape_for_type(db, ty) {
        shape
            .params
            .iter()
            .filter(|p| p.type_id != TypeId::ANY)
            .count()
    } else {
        0
    }
}

fn contextual_constraint_preserves_literals(db: &dyn QueryDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::STRING
        || type_id == TypeId::NUMBER
        || type_id == TypeId::BOOLEAN
        || type_id == TypeId::BIGINT
    {
        return true;
    }

    if matches!(
        common::classify_literal_type(db, type_id),
        LiteralTypeKind::String(_)
            | LiteralTypeKind::Number(_)
            | LiteralTypeKind::BigInt(_)
            | LiteralTypeKind::Boolean(_)
    ) {
        return true;
    }

    if let Some(members) = common::union_members(db, type_id) {
        return members
            .iter()
            .copied()
            .any(|member| contextual_constraint_preserves_literals(db, member));
    }

    false
}

fn sanitize_function_shape_binding_pattern_params(
    shape: &tsz_solver::FunctionShape,
    binding_pattern_param_positions: &[usize],
) -> tsz_solver::FunctionShape {
    shape.with_replaced_params(common::sanitize_params_at_positions(
        &shape.params,
        binding_pattern_param_positions,
        TypeId::UNKNOWN,
    ))
}

pub(crate) fn should_preserve_contextual_application_shape(
    db: &dyn TypeDatabase,
    ty: TypeId,
) -> bool {
    common::contains_application_in_structure(db, ty)
}

fn instantiate_function_shape_with_substitution(
    types: &dyn QueryDatabase,
    func: &tsz_solver::FunctionShape,
    substitution: &crate::query_boundaries::common::TypeSubstitution,
) -> tsz_solver::FunctionShape {
    common::instantiate_function_shape(types, func, substitution)
}

fn instantiate_contextual_target_shape_for_return_context(
    types: &dyn QueryDatabase,
    func: &tsz_solver::FunctionShape,
) -> tsz_solver::FunctionShape {
    common::instantiate_shape_to_defaults(types, func)
}

impl<'a> CheckerState<'a> {
    fn substitution_with_source_constraint_fallbacks(
        &mut self,
        source_fn: &tsz_solver::FunctionShape,
        substitution: &crate::query_boundaries::common::TypeSubstitution,
    ) -> crate::query_boundaries::common::TypeSubstitution {
        let mut constrained = substitution.clone();
        for tp in &source_fn.type_params {
            let Some(candidate) = constrained.get(tp.name) else {
                continue;
            };
            let Some(raw_constraint) = tp.constraint else {
                continue;
            };

            let constraint = common::instantiate_type(self.ctx.types, raw_constraint, &constrained);
            if !self.is_assignable_to_with_env(candidate, constraint) {
                constrained.insert(tp.name, constraint);
            }
        }
        constrained
    }

    pub(crate) fn resolve_signature_parameter_type_queries(
        &mut self,
        sig_params: &[tsz_solver::ParamInfo],
        instantiated_params: &[tsz_solver::ParamInfo],
    ) -> Vec<tsz_solver::ParamInfo> {
        let has_rest_function_parameter = sig_params.iter().any(|param| {
            common::function_shape_for_type(self.ctx.types, param.type_id)
                .is_some_and(|shape| shape.params.iter().any(|param| param.rest))
        });
        if !has_rest_function_parameter {
            return instantiated_params.to_vec();
        }

        let tracked_type_params: FxHashSet<_> = sig_params
            .iter()
            .flat_map(|param| common::collect_referenced_types(self.ctx.types, param.type_id))
            .filter_map(|type_id| common::type_param_info(self.ctx.types, type_id))
            .map(|info| info.name)
            .collect();
        let mut parameter_substitution = crate::query_boundaries::common::TypeSubstitution::new();
        if !tracked_type_params.is_empty() {
            let mut visited = FxHashSet::default();
            for (sig_param, instantiated_param) in sig_params.iter().zip(instantiated_params.iter())
            {
                self.collect_return_context_substitution(
                    sig_param.type_id,
                    instantiated_param.type_id,
                    &tracked_type_params,
                    &mut parameter_substitution,
                    &mut visited,
                );
            }
        }

        let mut replacements = FxHashMap::default();
        for (sig_param, instantiated_param) in sig_params.iter().zip(instantiated_params.iter()) {
            if let Some(name) = sig_param.name {
                let replacement = if parameter_substitution.is_empty() {
                    instantiated_param.type_id
                } else {
                    crate::query_boundaries::common::instantiate_type(
                        self.ctx.types,
                        instantiated_param.type_id,
                        &parameter_substitution,
                    )
                };
                replacements.insert(
                    self.ctx.types.resolve_atom_ref(name).to_string(),
                    replacement,
                );
            }
        }

        if replacements.is_empty() {
            return instantiated_params.to_vec();
        }

        instantiated_params
            .iter()
            .map(|param| {
                let mut resolved = *param;
                resolved.type_id = common::replace_type_queries_and_lazies_with(
                    self.ctx.types,
                    resolved.type_id,
                    |symbol| {
                        self.ctx
                            .binder
                            .symbols
                            .get(tsz_binder::SymbolId(symbol.0))
                            .and_then(|symbol| replacements.get(symbol.escaped_name.as_str()))
                            .copied()
                    },
                    |def_id| {
                        self.ctx.definition_store.get_name(def_id).and_then(|name| {
                            if tracked_type_params.contains(&name) {
                                parameter_substitution.get(name)
                            } else {
                                replacements
                                    .get(self.ctx.types.resolve_atom_ref(name).as_ref())
                                    .copied()
                            }
                        })
                    },
                );
                resolved
            })
            .collect()
    }

    pub(crate) fn widen_round2_contextual_substitution(
        &mut self,
        shape: &FunctionShape,
        substitution: &crate::query_boundaries::common::TypeSubstitution,
    ) -> crate::query_boundaries::common::TypeSubstitution {
        let mut widened = substitution.clone();

        for tp in &shape.type_params {
            if tp.is_const {
                continue;
            }

            let Some(current) = widened.get(tp.name) else {
                continue;
            };

            if current == TypeId::UNKNOWN || current == TypeId::ERROR {
                continue;
            }

            let preserve_literals = tp.constraint.is_some_and(|constraint| {
                let instantiated = crate::query_boundaries::common::instantiate_type(
                    self.ctx.types,
                    constraint,
                    substitution,
                );
                contextual_constraint_preserves_literals(self.ctx.types, instantiated) || {
                    let evaluated = self.evaluate_type_with_env(instantiated);
                    evaluated != instantiated
                        && contextual_constraint_preserves_literals(self.ctx.types, evaluated)
                }
            });
            if preserve_literals {
                continue;
            }

            let widened_current = common::widen_type(self.ctx.types, current);
            if widened_current == current {
                continue;
            }

            let chosen = if let Some(constraint) = tp.constraint {
                let instantiated_constraint = crate::query_boundaries::common::instantiate_type(
                    self.ctx.types,
                    constraint,
                    substitution,
                );
                let evaluated_constraint = self.evaluate_type_with_env(instantiated_constraint);
                if !self.is_assignable_to(widened_current, evaluated_constraint)
                    && self.is_assignable_to(current, evaluated_constraint)
                {
                    current
                } else {
                    widened_current
                }
            } else {
                widened_current
            };

            widened.insert(tp.name, chosen);
        }

        widened
    }

    fn fill_unresolved_contextual_substitution_from_constraints(
        &mut self,
        shape: &FunctionShape,
        substitution: &crate::query_boundaries::common::TypeSubstitution,
    ) -> crate::query_boundaries::common::TypeSubstitution {
        let mut filled = substitution.clone();

        for tp in &shape.type_params {
            let current = filled.get(tp.name);
            let unresolved = current.is_none_or(|mapped| {
                mapped == TypeId::UNKNOWN
                    || mapped == TypeId::ERROR
                    || common::contains_infer_types(self.ctx.types, mapped)
                    || common::contains_type_parameters(self.ctx.types, mapped)
            });

            let Some(fallback) = tp.default.or(tp.constraint) else {
                continue;
            };
            let instantiated = crate::query_boundaries::common::instantiate_type(
                self.ctx.types,
                fallback,
                &filled,
            );
            let fallback_name = self.format_type_diagnostic(instantiated);
            let resolved_fallback = self
                .is_well_known_lib_type_name(&fallback_name)
                .then(|| self.resolve_lib_type_by_name(&fallback_name))
                .flatten()
                .unwrap_or(instantiated);
            let evaluated = self.evaluate_type_with_env(resolved_fallback);
            let contextual_fallback =
                if evaluated == TypeId::ANY && resolved_fallback != TypeId::ANY {
                    resolved_fallback
                } else {
                    evaluated
                };
            let fallback_is_nominal_lib_object =
                self.is_nominal_lib_object_type_name(&fallback_name);
            if !unresolved {
                // A resolved inference candidate is better contextual information
                // than its broad constraint. Keep the nominal-lib primitive guard
                // that intentionally falls back to the library object shape.
                let primitive_fails_nominal_lib_object = current.is_some_and(|mapped| {
                    common::is_primitive_type(self.ctx.types, mapped)
                        && fallback_is_nominal_lib_object
                });
                if !primitive_fails_nominal_lib_object {
                    continue;
                }
            }
            if contextual_fallback == TypeId::UNKNOWN
                || contextual_fallback == TypeId::ERROR
                || common::contains_infer_types(self.ctx.types, contextual_fallback)
                || (!fallback_is_nominal_lib_object
                    && common::contains_type_parameters(self.ctx.types, contextual_fallback))
            {
                continue;
            }

            filled.insert(tp.name, contextual_fallback);
        }

        filled
    }

    pub(crate) fn direct_round1_literal_conflict_type_params(
        &mut self,
        shape: &FunctionShape,
        args: &[NodeIndex],
        arg_types: &[TypeId],
        sensitive_args: &[bool],
    ) -> crate::query_boundaries::common::TypeSubstitution {
        let mut seen_bases: Vec<(Atom, TypeId, TypeId)> = Vec::new();
        let mut conflicts = crate::query_boundaries::common::TypeSubstitution::new();

        for (i, &arg_type) in arg_types.iter().enumerate() {
            if sensitive_args.get(i).copied().unwrap_or(false) {
                continue;
            }

            let Some(param) = shape.params.get(i) else {
                continue;
            };
            if param.rest || arg_type.is_any_unknown_or_error() {
                continue;
            }
            let param_type = param.type_id;
            let Some(tp_info) = common::type_param_info(self.ctx.types, param_type) else {
                continue;
            };
            if !shape.type_params.iter().any(|tp| tp.name == tp_info.name) {
                continue;
            }

            let literal_arg_type = args
                .get(i)
                .and_then(|&arg_idx| self.literal_type_from_initializer(arg_idx))
                .unwrap_or(arg_type);
            let base = self.widen_literal_type(literal_arg_type);
            if base == literal_arg_type {
                continue;
            }

            if let Some((_, previous_base, first_arg_type)) =
                seen_bases.iter().find(|(name, _, _)| *name == tp_info.name)
            {
                if *previous_base != base {
                    conflicts.insert(tp_info.name, *first_arg_type);
                }
            } else {
                seen_bases.push((tp_info.name, base, literal_arg_type));
            }
        }

        conflicts
    }

    pub(crate) fn restore_conflicting_direct_literal_substitutions(
        &self,
        widened: &mut crate::query_boundaries::common::TypeSubstitution,
        conflicts: &crate::query_boundaries::common::TypeSubstitution,
    ) {
        for (&name, &original_type) in conflicts.map() {
            if original_type == TypeId::UNKNOWN
                || original_type == TypeId::ERROR
                || common::contains_infer_types(self.ctx.types, original_type)
            {
                continue;
            }
            widened.insert(name, original_type);
        }
    }

    pub(crate) fn rest_argument_element_type_with_env(&mut self, type_id: TypeId) -> TypeId {
        let evaluated = self.evaluate_type_with_env(type_id);
        common::rest_argument_element_type(self.ctx.types, evaluated)
    }

    pub(crate) fn target_contains_blocking_return_context_type_params(
        &self,
        target: TypeId,
        tracked_type_params: &FxHashSet<Atom>,
    ) -> bool {
        if common::contains_infer_types(self.ctx.types, target) {
            return true;
        }

        // A contextual callback target can mention an outer type parameter with the
        // same name as the callee's type parameter, e.g.
        // `outer<T>(obj: T) { id<U extends Function>(fn: U): U; ... }` where
        // `id` is contextually expected as `(target: T) => boolean`. At this point
        // type parameters are name-keyed, so treating every same-name reference as
        // self-recursive blocks the useful return-context substitution.
        if call_checker::get_contextual_signature(self.ctx.types, target).is_some() {
            return false;
        }

        common::references_any_type_param_named(self.ctx.types, target, tracked_type_params)
    }

    fn instantiate_contextual_constraint_without_unresolved_self(
        &mut self,
        type_param_type: TypeId,
        tp_info: &tsz_solver::TypeParamInfo,
        substitution: &crate::query_boundaries::common::TypeSubstitution,
    ) -> Option<TypeId> {
        let constraint = tp_info.constraint?;
        let should_drop_self = substitution.get(tp_info.name).is_some_and(|ty| {
            ty == TypeId::ERROR
                || ty == TypeId::UNKNOWN
                || common::contains_infer_types(self.ctx.types, ty)
        });
        let mut contextual_substitution = crate::query_boundaries::common::TypeSubstitution::new();
        for (&name, &type_id) in substitution.map() {
            let aliases_current_type_param = type_id == type_param_type;
            if (should_drop_self && name == tp_info.name) || aliases_current_type_param {
                continue;
            }
            contextual_substitution.insert(name, type_id);
        }
        // When the constraint is self-referential (contains the type parameter
        // itself) and the parameter is not yet resolved in the substitution,
        // substitute the self-reference with `unknown` to break the cycle.
        //
        // Example: `O extends NoExcessProperties<RepeatOptions<A>, O>` — the
        // constraint references `O` itself. Without this substitution, the
        // instantiated constraint still contains `O`, which is detected as
        // "contains type parameters" and discarded in favor of the unresolved
        // placeholder, yielding `any` as the contextual type for callbacks.
        // With `O → unknown`, the constraint evaluates to `RepeatOptions<A>`,
        // giving the correct contextual type for properties like `until`.
        if substitution.get(tp_info.name).is_none()
            && common::contains_type_parameter_named(self.ctx.types, constraint, tp_info.name)
        {
            contextual_substitution.insert(tp_info.name, TypeId::UNKNOWN);
        }
        Some(crate::query_boundaries::common::instantiate_type(
            self.ctx.types,
            constraint,
            &contextual_substitution,
        ))
    }

    fn unresolved_contextual_substitution_target(
        &self,
        tp_info: &tsz_solver::TypeParamInfo,
        substitution: &crate::query_boundaries::common::TypeSubstitution,
    ) -> Option<TypeId> {
        let ty = substitution.get(tp_info.name)?;
        (ty == TypeId::ERROR
            || ty == TypeId::UNKNOWN
            || common::contains_infer_types(self.ctx.types, ty))
        .then_some(ty)
    }

    fn evaluate_for_return_context_substitution(&mut self, ty: TypeId) -> TypeId {
        if common::application_info(self.ctx.types, ty).is_some() {
            crate::query_boundaries::state::type_environment::evaluate_type_with_resolver(
                self.ctx.types,
                &self.ctx,
                ty,
            )
        } else {
            self.evaluate_type_with_env(ty)
        }
    }

    pub(crate) fn instantiate_generic_function_argument_against_target_for_refinement(
        &mut self,
        source_ty: TypeId,
        target_ty: TypeId,
    ) -> TypeId {
        let source_eval = self.evaluate_type_with_env(source_ty);
        let target_eval = self.evaluate_type_with_env(target_ty);
        let function_info = match (
            call_checker::get_contextual_signature(self.ctx.types, source_ty),
            call_checker::get_contextual_signature(self.ctx.types, target_ty),
        ) {
            (Some(source_fn), Some(target_fn)) => Some((source_fn, target_fn)),
            _ => match (
                call_checker::get_contextual_signature(self.ctx.types, source_eval),
                call_checker::get_contextual_signature(self.ctx.types, target_eval),
            ) {
                (Some(source_fn), Some(target_fn)) => Some((source_fn, target_fn)),
                _ => None,
            },
        };

        let Some((source_fn, target_fn)) = function_info else {
            return source_ty;
        };
        let normalize = |shape: tsz_solver::FunctionShape| {
            let unpacked: Vec<_> = shape
                .params
                .iter()
                .flat_map(|param| common::unpack_tuple_rest_parameter(self.ctx.types, param))
                .collect();
            shape.with_replaced_params(unpacked)
        };
        let source_fn = normalize(source_fn);
        let target_fn = normalize(target_fn);
        if target_fn.params.iter().any(|param| param.rest) {
            return source_ty;
        }
        if source_fn.type_params.is_empty() || source_fn.params.len() > target_fn.params.len() {
            return source_ty;
        }
        if !target_fn.type_params.is_empty() {
            return source_ty;
        }
        // When every source type parameter is fixed by parameter positions,
        // the target return type should not feed inference. A generic like
        // `<A, B>(a: A, b: B) => A | B` against `(...args: [string, number])
        // => string | number` must infer A/B from the tuple-rest parameters,
        // not from the whole contextual function type.
        let source_type_params_fully_determined_by_params =
            source_fn.type_params.iter().all(|tp| {
                source_fn.params.iter().any(|param| {
                    common::collect_referenced_types(self.ctx.types, param.type_id)
                        .into_iter()
                        .any(|ty| {
                            common::type_param_info(self.ctx.types, ty)
                                .is_some_and(|info| info.name == tp.name)
                        })
                })
            });
        let target_params_are_concrete =
            target_fn
                .params
                .iter()
                .take(source_fn.params.len())
                .all(|param| {
                    !matches!(param.type_id, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)
                        && !common::contains_infer_types(self.ctx.types, param.type_id)
                        && !common::contains_type_parameters(self.ctx.types, param.type_id)
                });
        if source_type_params_fully_determined_by_params && target_params_are_concrete {
            return self
                .instantiate_generic_function_argument_against_target_params(source_ty, target_ty);
        }

        let target_param_types: Vec<_> = target_fn
            .params
            .iter()
            .take(source_fn.params.len())
            .map(|p| p.type_id)
            .collect();
        let substitution = {
            let env = self.ctx.type_env.borrow();
            call_checker::compute_contextual_types_with_context(
                self.ctx.types,
                &self.ctx,
                &env,
                &source_fn,
                &target_param_types,
                Some(target_ty),
            )
        };
        let substitution =
            self.substitution_with_source_constraint_fallbacks(&source_fn, &substitution);
        let instantiated =
            instantiate_function_shape_with_substitution(self.ctx.types, &source_fn, &substitution);
        self.ctx.types.factory().function(instantiated)
    }

    pub(crate) fn instantiate_generic_function_argument_against_target_params(
        &mut self,
        source_ty: TypeId,
        target_ty: TypeId,
    ) -> TypeId {
        let source_eval = self.evaluate_type_with_env(source_ty);
        let target_eval = self.evaluate_type_with_env(target_ty);
        let function_info = match (
            call_checker::get_contextual_signature(self.ctx.types, source_ty),
            call_checker::get_contextual_signature(self.ctx.types, target_ty),
        ) {
            (Some(source_fn), Some(target_fn)) => Some((source_fn, target_fn)),
            _ => match (
                call_checker::get_contextual_signature(self.ctx.types, source_eval),
                call_checker::get_contextual_signature(self.ctx.types, target_eval),
            ) {
                (Some(source_fn), Some(target_fn)) => Some((source_fn, target_fn)),
                _ => None,
            },
        };

        let Some((source_fn, target_fn)) = function_info else {
            return source_ty;
        };
        let normalize = |shape: tsz_solver::FunctionShape| {
            let unpacked: Vec<_> = shape
                .params
                .iter()
                .flat_map(|param| common::unpack_tuple_rest_parameter(self.ctx.types, param))
                .collect();
            shape.with_replaced_params(unpacked)
        };
        let source_fn = normalize(source_fn);
        let target_fn = normalize(target_fn);
        if target_fn.params.iter().any(|param| param.rest) {
            return source_ty;
        }
        if source_fn.type_params.is_empty() || source_fn.params.len() > target_fn.params.len() {
            return source_ty;
        }

        let target_param_types: Vec<_> = target_fn
            .params
            .iter()
            .take(source_fn.params.len())
            .map(|p| p.type_id)
            .collect();

        let has_concrete_param_context = target_param_types.iter().any(|&param_ty| {
            param_ty != TypeId::ANY
                && param_ty != TypeId::UNKNOWN
                && !common::contains_infer_types(self.ctx.types, param_ty)
                && !common::contains_type_parameters(self.ctx.types, param_ty)
        });
        if !has_concrete_param_context {
            return source_ty;
        }

        let substitution = {
            let env = self.ctx.type_env.borrow();
            call_checker::compute_contextual_types_with_context(
                self.ctx.types,
                &self.ctx,
                &env,
                &source_fn,
                &target_param_types,
                None,
            )
        };
        let substitution =
            self.substitution_with_source_constraint_fallbacks(&source_fn, &substitution);
        let instantiated =
            instantiate_function_shape_with_substitution(self.ctx.types, &source_fn, &substitution);
        self.ctx.types.factory().function(instantiated)
    }

    pub(crate) fn target_has_concrete_return_context_for_generic_refinement(
        &mut self,
        target_ty: TypeId,
    ) -> bool {
        let target_eval = self.evaluate_type_with_env(target_ty);
        let target_fn = call_checker::get_contextual_signature(self.ctx.types, target_ty)
            .or_else(|| call_checker::get_contextual_signature(self.ctx.types, target_eval));
        let Some(target_fn) = target_fn else {
            return false;
        };

        let return_ty = target_fn.return_type;
        return_ty != TypeId::ANY
            && return_ty != TypeId::UNKNOWN
            && return_ty != TypeId::ERROR
            && !common::contains_infer_types(self.ctx.types, return_ty)
            && !common::contains_type_parameters(self.ctx.types, return_ty)
    }

    pub(crate) fn contextual_param_types_from_instantiated_params(
        &mut self,
        instantiated_params: &[tsz_solver::ParamInfo],
        arg_count: usize,
    ) -> Vec<Option<TypeId>> {
        let normalize_contextual_param = |this: &mut Self, param: &tsz_solver::ParamInfo| {
            let ty = param.type_id;
            let evaluated = this.evaluate_type_with_env(ty);
            let mut contextual =
                if crate::query_boundaries::checkers::call::get_contextual_signature(
                    this.ctx.types,
                    evaluated,
                )
                .is_some()
                {
                    this.normalize_contextual_signature_with_env(evaluated)
                } else {
                    evaluated
                };
            if param.optional && contextual != TypeId::ANY && contextual != TypeId::UNKNOWN {
                contextual = common::union_with_undefined(this.ctx.types, contextual);
            }
            contextual
        };
        let unpacked_params: Vec<_> = instantiated_params
            .iter()
            .flat_map(|param| common::unpack_tuple_rest_parameter(self.ctx.types, param))
            .collect();
        let rest_start = if unpacked_params.last().is_some_and(|param| param.rest) {
            unpacked_params.len().saturating_sub(1)
        } else {
            unpacked_params.len()
        };

        let contextuals: Vec<_> = (0..arg_count)
            .map(|index| {
                if index < rest_start {
                    unpacked_params
                        .get(index)
                        .map(|param| normalize_contextual_param(self, param))
                } else {
                    unpacked_params
                        .last()
                        .filter(|param| param.rest)
                        .map(|param| {
                            let rest_type = normalize_contextual_param(self, param);
                            self.rest_argument_element_type_with_env(rest_type)
                        })
                }
            })
            .collect();
        contextuals
    }

    pub(crate) fn compute_callback_argument_type_rollback_unknown_body_diagnostics(
        &mut self,
        arg_idx: NodeIndex,
        contextual_type: TypeId,
        check_excess_properties: bool,
        index: usize,
        args_len: usize,
        callable_ctx: CallableContext,
        callback_body_spans: &[(u32, u32)],
    ) {
        let snap = DiagnosticSpeculationSnapshot::new(&self.ctx);
        let _ = self.compute_single_call_argument_type(
            arg_idx,
            Some(contextual_type),
            check_excess_properties,
            index,
            args_len,
            false,
            callable_ctx,
        );
        snap.rollback_filtered(&mut self.ctx.diagnostic_state(), |diag| {
            matches!(
                diag.code,
                diagnostic_codes::IS_OF_TYPE_UNKNOWN | diagnostic_codes::OBJECT_IS_OF_TYPE_UNKNOWN
            ) && callback_body_spans
                .iter()
                .any(|(start, end)| diag.start >= *start && diag.start < *end)
        });
    }

    pub(crate) fn refine_generic_function_args_against_instantiated_params(
        &mut self,
        arg_types: Vec<TypeId>,
        instantiated_params: &[tsz_solver::ParamInfo],
    ) -> Vec<TypeId> {
        let expected_types = self
            .contextual_param_types_from_instantiated_params(instantiated_params, arg_types.len());

        arg_types
            .into_iter()
            .enumerate()
            .map(|(i, arg_type)| {
                let expected = expected_types.get(i).copied().flatten();
                expected
                    .map(|expected| {
                        self.instantiate_generic_function_argument_against_target_params(
                            arg_type, expected,
                        )
                    })
                    .unwrap_or(arg_type)
            })
            .collect()
    }

    /// Build a substitution map that, when applied to `sig.params`, reproduces
    /// `instantiated_params`. Walks each `(sig_param, instantiated_param)` pair
    /// structurally and records the binding for any tracked type parameter that
    /// appears on the source side. This recovers round-1's inference as a
    /// substitution map so that it can be composed with a return-context
    /// substitution (where the latter takes precedence on overlap).
    ///
    /// Used by overload retry to retain type parameters inferred from
    /// non-callback arguments (e.g., `T` in `from<T,U>(ArrayLike<T>, mapfn): U[]`)
    /// alongside type parameters bound from the contextual return type
    /// (e.g., `U` from `A[]`). Without this composition the callback body
    /// would be evaluated with `T` still uninstantiated, producing spurious
    /// TS2339 / TS2769 inside the callback.
    pub(crate) fn extract_arg_inference_substitution(
        &mut self,
        sig_params: &[tsz_solver::ParamInfo],
        instantiated_params: &[tsz_solver::ParamInfo],
        type_params: &[tsz_solver::TypeParamInfo],
    ) -> crate::query_boundaries::common::TypeSubstitution {
        let tracked_type_params: FxHashSet<_> = type_params.iter().map(|tp| tp.name).collect();
        let mut substitution = crate::query_boundaries::common::TypeSubstitution::new();
        if tracked_type_params.is_empty() {
            return substitution;
        }
        let mut visited = FxHashSet::default();
        for (sig_param, inst_param) in sig_params.iter().zip(instantiated_params.iter()) {
            if sig_param.type_id == inst_param.type_id {
                continue;
            }
            self.collect_return_context_substitution(
                sig_param.type_id,
                inst_param.type_id,
                &tracked_type_params,
                &mut substitution,
                &mut visited,
            );
        }
        substitution
    }

    pub(crate) fn callback_first_conditional_branch(
        &self,
        arg_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let node = self
            .callback_function_index(arg_idx)
            .and_then(|idx| self.ctx.arena.get(idx))?;
        let func = self.ctx.arena.get_function(node)?;
        if func.type_annotation.is_some() {
            return None;
        }

        let body_node = self.ctx.arena.get(func.body)?;
        if body_node.kind == syntax_kind_ext::BLOCK {
            let block = self.ctx.arena.get_block(body_node)?;
            let ret_expr = block.statements.nodes.iter().find_map(|&stmt_idx| {
                let stmt_node = self.ctx.arena.get(stmt_idx)?;
                if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
                    return None;
                }
                self.ctx
                    .arena
                    .get_return_statement(stmt_node)
                    .and_then(|ret| (ret.expression != NodeIndex::NONE).then_some(ret.expression))
            })?;
            let ret_node = self.ctx.arena.get(ret_expr)?;
            if ret_node.kind != syntax_kind_ext::CONDITIONAL_EXPRESSION {
                return None;
            }
            return self
                .ctx
                .arena
                .get_conditional_expr(ret_node)
                .map(|cond| cond.when_true);
        }

        if body_node.kind != syntax_kind_ext::CONDITIONAL_EXPRESSION {
            return None;
        }
        self.ctx
            .arena
            .get_conditional_expr(body_node)
            .map(|cond| cond.when_true)
    }

    pub(crate) fn unannotated_zero_param_callback_return_expression(
        &self,
        arg_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let node = self
            .callback_function_index(arg_idx)
            .and_then(|idx| self.ctx.arena.get(idx))?;
        let func = self.ctx.arena.get_function(node)?;
        if !func.parameters.nodes.is_empty() || func.type_annotation.is_some() {
            return None;
        }

        let body_node = self.ctx.arena.get(func.body)?;
        if body_node.kind != syntax_kind_ext::BLOCK {
            return Some(func.body);
        }

        let block = self.ctx.arena.get_block(body_node)?;
        block.statements.nodes.iter().find_map(|&stmt_idx| {
            let stmt_node = self.ctx.arena.get(stmt_idx)?;
            if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
                return None;
            }
            self.ctx
                .arena
                .get_return_statement(stmt_node)
                .and_then(|ret| (ret.expression != NodeIndex::NONE).then_some(ret.expression))
        })
    }

    pub(crate) fn sanitize_generic_inference_arg_type(
        &mut self,
        arg_idx: NodeIndex,
        arg_type: TypeId,
    ) -> TypeId {
        let Some(arg_node) = self
            .callback_function_index(arg_idx)
            .and_then(|idx| self.ctx.arena.get(idx))
        else {
            return arg_type;
        };

        let Some(func) = self.ctx.arena.get_function(arg_node) else {
            return arg_type;
        };

        let mut binding_pattern_param_positions = Vec::new();
        for (index, &param_idx) in func.parameters.nodes.iter().enumerate() {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            if param.type_annotation.is_some() {
                continue;
            }
            let Some(name_node) = self.ctx.arena.get(param.name) else {
                continue;
            };
            if name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            {
                binding_pattern_param_positions.push(index);
            }
        }

        if binding_pattern_param_positions.is_empty() {
            return arg_type;
        }

        if let Some(shape) = common::function_shape_for_type(self.ctx.types, arg_type) {
            let sanitized = sanitize_function_shape_binding_pattern_params(
                &shape,
                &binding_pattern_param_positions,
            );
            self.ctx.types.factory().function(sanitized)
        } else if let Some(shape) = common::callable_shape_for_type(self.ctx.types, arg_type) {
            let sanitized = common::sanitize_callable_shape_binding_pattern_params(
                &shape,
                &binding_pattern_param_positions,
                TypeId::UNKNOWN,
            );
            self.ctx.types.factory().callable(sanitized)
        } else {
            arg_type
        }
    }

    pub(crate) fn inference_type_is_anyish(&self, ty: TypeId) -> bool {
        common::is_type_deeply_any(self.ctx.types, ty)
    }

    pub(crate) fn sanitize_generic_inference_arg_types<'b>(
        &mut self,
        _callee_expr: NodeIndex,
        args: &[NodeIndex],
        arg_types: &'b [TypeId],
    ) -> (Cow<'b, [TypeId]>, bool) {
        let mut changed = false;
        let expanded_args;
        let source_args: &[NodeIndex] = if args.len() == arg_types.len() {
            args
        } else {
            expanded_args = self.build_expanded_args_for_error(args);
            &expanded_args
        };

        let mut sanitized: Option<Vec<TypeId>> = None;

        for (index, (&arg_idx, &original_arg_type)) in
            source_args.iter().zip(arg_types.iter()).enumerate()
        {
            // Resolve enum types to their namespace object representation.
            // When an enum identifier (like `E1`) is used as a call argument,
            // it resolves to an Enum type with a DefId. For inference against
            // index-signature targets like `{ [x: string]: T }`, the inference
            // engine needs to see the namespace Object type with named member
            // properties. This mirrors tsc's behavior where `typeof E1`
            // (the enum namespace) has an implicit string index signature.
            let enum_def = common::enum_def_id(self.ctx.types, original_arg_type);
            let mut arg_type = if let Some(def_id) = enum_def {
                let sym_id = self.ctx.def_to_symbol_id(def_id);
                let ns_type =
                    sym_id.and_then(|sid| self.ctx.enum_namespace_types.get(&sid).copied());
                if let Some(ns) = ns_type {
                    changed = true;
                    ns
                } else {
                    original_arg_type
                }
            } else {
                original_arg_type
            };

            if self
                .ctx
                .arena
                .get(arg_idx)
                .is_some_and(|node| node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16)
                && self.ctx.enclosing_class.is_some()
                && !self.is_this_in_nested_function_without_own_this_binding(arg_idx)
            {
                changed = true;
                arg_type = self.ctx.types.this_type();
            }

            let sanitized_arg = self.sanitize_generic_inference_arg_type(arg_idx, arg_type);
            if sanitized_arg != arg_type {
                changed = true;
            }

            if sanitized_arg != original_arg_type && sanitized.is_none() {
                let mut owned = Vec::with_capacity(arg_types.len());
                owned.extend_from_slice(&arg_types[..index]);
                sanitized = Some(owned);
            }
            if let Some(owned) = sanitized.as_mut() {
                owned.push(sanitized_arg);
            }
        }

        if let Some(owned) = sanitized {
            (Cow::Owned(owned), changed)
        } else {
            (Cow::Borrowed(arg_types), changed)
        }
    }

    /// When the checker's intra-expression Round 2 inferred concrete types for
    /// generic parameters that the solver's single-pass `resolve_call` could not
    /// recover, refine the solver's `instantiated_params` so the post-call
    /// assignability recheck sees the tighter expected types.
    ///
    /// The solver loses bindings in patterns where one object-literal property
    /// contributes a concrete type to a parameter (`setup(): { outputs: O }`
    /// pins `O = { ... }`) and another property's contextual signature uses the
    /// same parameter through a homomorphic mapped + `infer` shape that fails
    /// reverse inference (`map: (...) => Unwrap<O>`). The solver's defaulting
    /// then widens the parameter to its constraint, masking errors in the
    /// callback body. The checker's Round 2 substitution preserves the
    /// concrete binding from `setup`.
    ///
    /// We only adopt the checker's value when it is strictly more specific than
    /// the solver's (a fresh subtype). When the solver's inference is the same
    /// or more specific, leave it untouched.
    pub(crate) fn refine_instantiated_params_with_checker_substitution(
        &mut self,
        orig_shape: &FunctionShape,
        params: &mut [tsz_solver::ParamInfo],
        checker_sub: &common::TypeSubstitution,
    ) {
        // Build the merged substitution that would re-instantiate `orig_shape.params`
        // using the checker's intra-expression Round 2 inferences. Skip type
        // parameters where the checker's value is not concrete or where the
        // checker matches the constraint (no improvement).
        let mut merged = common::TypeSubstitution::new();
        let mut any_override = false;
        for tp in &orig_shape.type_params {
            let Some(checker_val) = checker_sub.get(tp.name) else {
                continue;
            };
            if checker_val == TypeId::UNKNOWN
                || checker_val == TypeId::ERROR
                || common::contains_infer_types(self.ctx.types, checker_val)
                || common::contains_type_parameters(self.ctx.types, checker_val)
            {
                continue;
            }
            if Some(checker_val) == tp.constraint {
                continue;
            }
            merged.insert(tp.name, checker_val);
            any_override = true;
        }
        if !any_override {
            return;
        }
        // Detect type parameters where the solver effectively defaulted (its
        // instantiation can be reproduced by `T => T.constraint`). Only those
        // are safe to override with the checker's value: when the solver
        // bound a type parameter to anything else, it had information from the
        // arg-type unification step that the checker doesn't see.
        let mut constraint_default = common::TypeSubstitution::new();
        for tp in &orig_shape.type_params {
            constraint_default.insert(tp.name, tp.constraint.unwrap_or(TypeId::UNKNOWN));
        }
        let mut solver_defaulted = rustc_hash::FxHashSet::default();
        for tp in &orig_shape.type_params {
            // Build a substitution that maps THIS tp to its constraint and
            // every OTHER tp to a fresh marker. Then compare the resulting
            // shape.params with the solver's instantiated_params at any
            // position whose type contains this tp. If they agree, the
            // solver defaulted this tp.
            let mut probe = common::TypeSubstitution::new();
            for other_tp in &orig_shape.type_params {
                if other_tp.name == tp.name {
                    probe.insert(
                        other_tp.name,
                        other_tp.constraint.unwrap_or(TypeId::UNKNOWN),
                    );
                } else if let Some(checker_val) = checker_sub.get(other_tp.name) {
                    probe.insert(other_tp.name, checker_val);
                } else {
                    probe.insert(
                        other_tp.name,
                        other_tp.constraint.unwrap_or(TypeId::UNKNOWN),
                    );
                }
            }
            let mut all_match = true;
            let mut any_referenced = false;
            for (i, orig_param) in orig_shape.params.iter().enumerate() {
                if i >= params.len() {
                    break;
                }
                let referenced =
                    common::collect_referenced_types(self.ctx.types, orig_param.type_id)
                        .into_iter()
                        .any(|ty| {
                            common::type_param_info(self.ctx.types, ty)
                                .is_some_and(|info| info.name == tp.name)
                        });
                if !referenced {
                    continue;
                }
                any_referenced = true;
                let probed = common::instantiate_type(self.ctx.types, orig_param.type_id, &probe);
                if probed == params[i].type_id {
                    continue;
                }
                // Different `TypeId`s can still represent the same type
                // (alias unfoldings, interner aliasing, etc.). Treat as equal
                // when each side is mutually assignable to the other.
                let mutually_assignable = self.is_assignable_to_with_env(probed, params[i].type_id)
                    && self.is_assignable_to_with_env(params[i].type_id, probed);
                if !mutually_assignable {
                    all_match = false;
                    break;
                }
            }
            if any_referenced && all_match {
                solver_defaulted.insert(tp.name);
            }
        }
        // Drop checker overrides for type params the solver did NOT default —
        // the solver had non-default information we shouldn't clobber.
        let mut filtered_merged = common::TypeSubstitution::new();
        let mut any_filtered_override = false;
        for tp in &orig_shape.type_params {
            if let Some(val) = merged.get(tp.name)
                && solver_defaulted.contains(&tp.name)
            {
                filtered_merged.insert(tp.name, val);
                any_filtered_override = true;
            } else if let Some(other) = checker_sub.get(tp.name) {
                // Even when we keep the solver's binding for this tp, we still
                // need a substitution entry so re-instantiating doesn't leave
                // bare `tp` refs in composite types referencing both this tp
                // and a tp we are overriding. Use the checker's value for
                // structural completeness; if the solver had more info this
                // entry won't be applied (the per-param assignability gate
                // below will reject any widening).
                filtered_merged.insert(tp.name, other);
            }
        }
        if !any_filtered_override {
            return;
        }
        for (i, orig_param) in orig_shape.params.iter().enumerate() {
            if i >= params.len() {
                break;
            }
            let new_type =
                common::instantiate_type(self.ctx.types, orig_param.type_id, &filtered_merged);
            if new_type == params[i].type_id {
                continue;
            }
            if common::contains_type_parameters(self.ctx.types, new_type) {
                continue;
            }
            // Final guard: only adopt when the checker's instantiation is a
            // fresh subtype of the solver's. This rejects any widening the
            // filtered substitution might still introduce.
            if self.is_assignable_to_with_env(new_type, params[i].type_id) {
                params[i].type_id = new_type;
            }
        }
    }

    pub(crate) fn refine_bare_instantiated_params_with_direct_literal_conflicts(
        &mut self,
        orig_shape: &FunctionShape,
        params: &mut [tsz_solver::ParamInfo],
        conflicts: &common::TypeSubstitution,
    ) {
        if conflicts.is_empty() {
            return;
        }

        for (i, orig_param) in orig_shape.params.iter().enumerate() {
            if i >= params.len() || orig_param.rest {
                break;
            }
            let Some(tp_info) = common::type_param_info(self.ctx.types, orig_param.type_id) else {
                continue;
            };
            let Some(literal_type) = conflicts.get(tp_info.name) else {
                continue;
            };
            if literal_type == TypeId::UNKNOWN
                || literal_type == TypeId::ERROR
                || common::contains_infer_types(self.ctx.types, literal_type)
                || common::contains_type_parameters(self.ctx.types, literal_type)
            {
                continue;
            }

            let current = params[i].type_id;
            if current == literal_type {
                continue;
            }
            if self.is_assignable_to_with_env(literal_type, current) {
                params[i].type_id = literal_type;
            }
        }
    }
}

#[cfg(test)]
#[path = "intra_expression_inference_tests.rs"]
mod intra_expression_inference_tests;
