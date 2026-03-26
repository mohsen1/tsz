//! Generic-call inference and round-2 contextual typing helpers.

use crate::call_checker::CallableContext;
use crate::context::TypingRequest;
use crate::query_boundaries::checkers::call as call_checker;
use crate::query_boundaries::checkers::call::is_type_parameter_type;
use crate::query_boundaries::common;
use crate::query_boundaries::common::CallResult;
use crate::query_boundaries::common::LiteralTypeKind;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_common::Atom;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{FunctionShape, TypeId};

/// Count the number of non-`any` parameter types in a callable type.
///
/// Used to compare contextual type candidates: whichever has more specific
/// (non-`any`) parameter types provides better contextual typing for callbacks.
/// Returns 0 for non-callable types.
fn callable_param_specificity(db: &dyn tsz_solver::QueryDatabase, ty: TypeId) -> usize {
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

fn contextual_constraint_preserves_literals(
    db: &dyn tsz_solver::QueryDatabase,
    type_id: TypeId,
) -> bool {
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
    db: &dyn tsz_solver::TypeDatabase,
    ty: TypeId,
) -> bool {
    common::contains_application_in_structure(db, ty)
}

fn instantiate_function_shape_with_substitution(
    types: &dyn tsz_solver::QueryDatabase,
    func: &tsz_solver::FunctionShape,
    substitution: &crate::query_boundaries::common::TypeSubstitution,
) -> tsz_solver::FunctionShape {
    common::instantiate_function_shape(types, func, substitution)
}

fn instantiate_contextual_target_shape_for_return_context(
    types: &dyn tsz_solver::QueryDatabase,
    func: &tsz_solver::FunctionShape,
) -> tsz_solver::FunctionShape {
    common::instantiate_shape_to_defaults(types, func)
}

impl<'a> CheckerState<'a> {
    fn is_builtin_object_entries_call(&self, callee_expr: NodeIndex) -> bool {
        let Some(callee_node) = self.ctx.arena.get(callee_expr) else {
            return false;
        };
        let Some(access) = self.ctx.arena.get_access_expr(callee_node) else {
            return false;
        };
        let Some(member_name) = self.identifier_name_text(access.name_or_argument) else {
            return false;
        };
        if member_name != "entries" {
            return false;
        }
        matches!(self.identifier_name_text(access.expression), Some("Object"))
    }

    fn identifier_name_text(&self, idx: NodeIndex) -> Option<&str> {
        self.ctx.arena.get_identifier_text(idx)
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
        if source_fn.type_params.is_empty() || source_fn.params.len() > target_fn.params.len() {
            return source_ty;
        }

        let target_param_types: Vec<_> = target_fn
            .params
            .iter()
            .take(source_fn.params.len())
            .map(|p| p.type_id)
            .collect();
        let env = self.ctx.type_env.borrow();
        let substitution = call_checker::compute_contextual_types_with_context(
            self.ctx.types,
            &self.ctx,
            &env,
            &source_fn,
            &target_param_types,
            Some(target_ty),
        );
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
        });
        if !has_concrete_param_context {
            return source_ty;
        }

        let env = self.ctx.type_env.borrow();
        let substitution = call_checker::compute_contextual_types_with_context(
            self.ctx.types,
            &self.ctx,
            &env,
            &source_fn,
            &target_param_types,
            None,
        );
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
        let normalize_contextual_param = |this: &mut Self, ty: TypeId| {
            let evaluated = this.evaluate_type_with_env(ty);
            if crate::query_boundaries::checkers::call::get_contextual_signature(
                this.ctx.types,
                evaluated,
            )
            .is_some()
            {
                this.normalize_contextual_signature_with_env(evaluated)
            } else {
                evaluated
            }
        };
        if std::env::var_os("TSZ_DEBUG_INSTANTIATED_CONTEXT").is_some()
            && self
                .ctx
                .file_name
                .contains("dependentDestructuredVariables")
        {
            let raw: Vec<_> = instantiated_params
                .iter()
                .map(|param| (self.format_type(param.type_id), param.rest))
                .collect();
            eprintln!(
                "instantiated-context file={} arg_count={} params={:?}",
                self.ctx.file_name, arg_count, raw
            );
        }
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
                        .map(|param| normalize_contextual_param(self, param.type_id))
                } else {
                    unpacked_params
                        .last()
                        .filter(|param| param.rest)
                        .map(|param| {
                            let rest_type = normalize_contextual_param(self, param.type_id);
                            self.rest_argument_element_type_with_env(rest_type)
                        })
                }
            })
            .collect();
        if std::env::var_os("TSZ_DEBUG_INSTANTIATED_CONTEXT").is_some()
            && self
                .ctx
                .file_name
                .contains("dependentDestructuredVariables")
        {
            let formatted: Vec<_> = contextuals
                .iter()
                .map(|ty| ty.map(|ty| self.format_type(ty)))
                .collect();
            eprintln!(
                "instantiated-context-result file={} arg_count={} contextuals={:?}",
                self.ctx.file_name, arg_count, formatted
            );
        }
        contextuals
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

    pub(crate) fn collect_return_context_substitution(
        &mut self,
        source: TypeId,
        target: TypeId,
        tracked_type_params: &FxHashSet<Atom>,
        substitution: &mut crate::query_boundaries::common::TypeSubstitution,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
    ) {
        if !visited.insert((source, target)) {
            return;
        }
        // Depth guard: evaluate_type_with_env can produce fresh TypeIds, defeating
        // the visited set and causing unbounded recursion.
        if !self.ctx.enter_recursion() {
            return;
        }
        self.collect_return_context_substitution_impl(
            source,
            target,
            tracked_type_params,
            substitution,
            visited,
        );
        self.ctx.leave_recursion();
    }

    fn collect_return_context_substitution_impl(
        &mut self,
        source: TypeId,
        target: TypeId,
        tracked_type_params: &FxHashSet<Atom>,
        substitution: &mut crate::query_boundaries::common::TypeSubstitution,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
    ) {
        if let Some(tp) = common::type_param_info(self.ctx.types, source)
            && tracked_type_params.contains(&tp.name)
            && target != TypeId::UNKNOWN
            && target != TypeId::ERROR
            && !self
                .target_contains_blocking_return_context_type_params(target, tracked_type_params)
        {
            if substitution.get(tp.name).is_none() {
                substitution.insert(tp.name, target);
            }
            return;
        }

        // When source (return type) is a union like `E | null`, decompose it
        // and try each non-nullish member against the target contextual type.
        // This handles the common pattern `querySelector<E>(...): E | null`
        // where the contextual type `SVGRectElement` should infer E = SVGRectElement.
        if let Some(source_members) = common::union_members(self.ctx.types, source) {
            for member in source_members
                .into_iter()
                .filter(|member| *member != TypeId::NULL && *member != TypeId::UNDEFINED)
            {
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

        if let Some(target_members) = common::union_members(self.ctx.types, target) {
            let before_len = substitution.len();
            for member in target_members
                .into_iter()
                .filter(|member| *member != TypeId::NULL && *member != TypeId::UNDEFINED)
            {
                self.collect_return_context_substitution(
                    source,
                    member,
                    tracked_type_params,
                    substitution,
                    visited,
                );
                if substitution.len() > before_len {
                    return;
                }
            }
        }

        if let Some(inner) = common::unwrap_readonly_or_noinfer(self.ctx.types, target) {
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

        if let Some(inner) = common::unwrap_readonly_or_noinfer(self.ctx.types, source) {
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

        let source_eval = self.evaluate_type_with_env(source);
        let target_eval = self.evaluate_type_with_env(target);
        let function_info = match (
            call_checker::get_contextual_signature(self.ctx.types, source),
            call_checker::get_contextual_signature(self.ctx.types, target),
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

        if let Some((source_fn, target_fn)) = function_info
            && source_fn.params.len() <= target_fn.params.len()
        {
            let target_fn =
                instantiate_contextual_target_shape_for_return_context(self.ctx.types, &target_fn);
            let mut target_index = 0usize;
            for source_param in &source_fn.params {
                let target_type = if source_param.rest {
                    let remaining = &target_fn.params[target_index..];
                    if remaining.len() == 1 && remaining[0].rest {
                        remaining[0].type_id
                    } else {
                        self.ctx
                            .types
                            .factory()
                            .tuple(common::params_to_tuple_elements(remaining))
                    }
                } else {
                    let Some(target_param) = target_fn.params.get(target_index) else {
                        break;
                    };
                    target_index += 1;
                    target_param.type_id
                };
                self.collect_return_context_substitution(
                    source_param.type_id,
                    target_type,
                    tracked_type_params,
                    substitution,
                    visited,
                );
                if source_param.rest {
                    break;
                }
            }
            self.collect_return_context_substitution(
                source_fn.return_type,
                target_fn.return_type,
                tracked_type_params,
                substitution,
                visited,
            );
            return;
        }

        if let (Some(source_elems), Some(target_elems)) = (
            common::tuple_elements(self.ctx.types, source),
            common::tuple_elements(self.ctx.types, target),
        ) {
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

        if let (Some(source_elem), Some(target_elem)) = (
            common::array_element_type(self.ctx.types, source),
            common::array_element_type(self.ctx.types, target),
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

        if let Some(source_elem) = common::array_element_type(self.ctx.types, source)
            && let Some((_target_base, target_args)) =
                common::application_info(self.ctx.types, target)
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

        if let Some(source_elem) = common::array_element_type(self.ctx.types, source)
            && let Some(iterator_info) = common::get_iterator_info(self.ctx.types, target, false)
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

        if let (Some((source_base, source_args)), Some((target_base, target_args))) = (
            common::application_info(self.ctx.types, source),
            common::application_info(self.ctx.types, target),
        ) && source_base == target_base
            && source_args.len() == target_args.len()
        {
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

        // Structural Object matching: when source is an Application (e.g., GenericClass<T>)
        // and target is an already-evaluated Object (e.g., GenericClass<[string, boolean]>
        // resolved to { from: ..., __schema: ... }), evaluate the source Application to get
        // its expanded object form and match property types structurally.
        // This handles the common pattern where the return context type from an outer call
        // has been evaluated while the generic return type is still an Application.
        if let (Some(source_shape), Some(target_shape)) = (
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, source_eval),
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, target_eval),
        ) {
            for source_prop in &source_shape.properties {
                if let Some(target_prop) =
                    common::find_matching_property(&target_shape.properties, source_prop.name)
                {
                    self.collect_return_context_substitution(
                        source_prop.type_id,
                        target_prop.type_id,
                        tracked_type_params,
                        substitution,
                        visited,
                    );
                }
            }
        }
    }

    pub(crate) fn compute_return_context_substitution_from_shape(
        &mut self,
        shape: &tsz_solver::FunctionShape,
        contextual_type: Option<TypeId>,
    ) -> crate::query_boundaries::common::TypeSubstitution {
        let Some(contextual_type) = contextual_type else {
            return crate::query_boundaries::common::TypeSubstitution::new();
        };
        let tracked_type_params: FxHashSet<_> =
            shape.type_params.iter().map(|tp| tp.name).collect();
        if tracked_type_params.is_empty() {
            return crate::query_boundaries::common::TypeSubstitution::new();
        }

        let mut substitution = crate::query_boundaries::common::TypeSubstitution::new();
        let mut visited = FxHashSet::default();
        self.collect_return_context_substitution(
            shape.return_type,
            contextual_type,
            &tracked_type_params,
            &mut substitution,
            &mut visited,
        );
        substitution
    }

    pub(crate) fn zero_param_callback_first_conditional_branch(
        &self,
        arg_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(arg_idx)?;
        if node.kind != syntax_kind_ext::ARROW_FUNCTION
            && node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return None;
        }
        let func = self.ctx.arena.get_function(node)?;
        if !func.parameters.nodes.is_empty() || func.type_annotation.is_some() {
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

    pub(crate) fn sanitize_generic_inference_arg_type(
        &mut self,
        arg_idx: NodeIndex,
        arg_type: TypeId,
    ) -> TypeId {
        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
            return arg_type;
        };
        if arg_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
            && arg_node.kind != syntax_kind_ext::ARROW_FUNCTION
        {
            return arg_type;
        }

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

    pub(crate) fn sanitize_generic_inference_arg_types(
        &mut self,
        callee_expr: NodeIndex,
        args: &[NodeIndex],
        arg_types: &[TypeId],
    ) -> (Vec<TypeId>, bool) {
        let sanitize_object_entries_any =
            self.is_builtin_object_entries_call(callee_expr) && arg_types.len() == 1;
        let mut changed = false;
        let sanitized = args
            .iter()
            .zip(arg_types.iter().copied())
            .enumerate()
            .map(|(index, (&arg_idx, arg_type))| {
                // Resolve enum types to their namespace object representation.
                // When an enum identifier (like `E1`) is used as a call argument,
                // it resolves to an Enum type with a DefId. For inference against
                // index-signature targets like `{ [x: string]: T }`, the inference
                // engine needs to see the namespace Object type with named member
                // properties. This mirrors tsc's behavior where `typeof E1`
                // (the enum namespace) has an implicit string index signature.
                let enum_def = common::enum_def_id(self.ctx.types, arg_type);
                let arg_type = if let Some(def_id) = enum_def {
                    let sym_id = self.ctx.def_to_symbol_id(def_id);
                    let ns_type =
                        sym_id.and_then(|sid| self.ctx.enum_namespace_types.get(&sid).copied());
                    if let Some(ns) = ns_type {
                        changed = true;
                        ns
                    } else {
                        arg_type
                    }
                } else {
                    arg_type
                };

                let arg_type =
                    if sanitize_object_entries_any && index == 0 && arg_type == TypeId::ANY {
                        changed = true;
                        TypeId::UNKNOWN
                    } else {
                        arg_type
                    };

                let sanitized = self.sanitize_generic_inference_arg_type(arg_idx, arg_type);
                if sanitized != arg_type {
                    changed = true;
                }
                sanitized
            })
            .collect();
        (sanitized, changed)
    }

    pub(crate) fn recheck_generic_call_arguments_with_real_types(
        &mut self,
        result: CallResult,
        instantiated_params: &[tsz_solver::ParamInfo],
        args: &[NodeIndex],
        arg_types: &[TypeId],
    ) -> CallResult {
        let success_return = if let CallResult::Success(return_type) = result {
            return_type
        } else {
            return result;
        };
        let expected_signature = (!instantiated_params.is_empty()).then(|| {
            self.ctx.types.factory().function(FunctionShape::new(
                instantiated_params.to_vec(),
                TypeId::UNKNOWN,
            ))
        });

        for (index, &cached_actual) in arg_types.iter().enumerate() {
            let expected = expected_signature.and_then(|signature| {
                self.contextual_parameter_type_for_call_with_env_from_expected(
                    signature,
                    index,
                    arg_types.len(),
                )
            });

            let Some(expected) = expected else {
                break;
            };

            let arg_idx = args.get(index).copied();
            let skip_unresolved_callable_recheck = arg_idx.is_some_and(|arg_idx| {
                self.ctx.arena.get(arg_idx).is_some_and(|node| {
                    matches!(
                        node.kind,
                        k if k == tsz_parser::parser::syntax_kind_ext::FUNCTION_EXPRESSION
                            || k == tsz_parser::parser::syntax_kind_ext::ARROW_FUNCTION
                    )
                }) && (common::contains_type_parameters(self.ctx.types, expected)
                    || common::contains_infer_types(self.ctx.types, expected))
                    && crate::query_boundaries::checkers::call::get_contextual_signature(
                        self.ctx.types,
                        expected,
                    )
                    .or_else(|| {
                        let evaluated = self.evaluate_type_with_env(expected);
                        crate::query_boundaries::checkers::call::get_contextual_signature(
                            self.ctx.types,
                            evaluated,
                        )
                    })
                    .is_some()
                    && crate::query_boundaries::checkers::call::get_contextual_signature(
                        self.ctx.types,
                        cached_actual,
                    )
                    .or_else(|| {
                        let evaluated = self.evaluate_type_with_env(cached_actual);
                        crate::query_boundaries::checkers::call::get_contextual_signature(
                            self.ctx.types,
                            evaluated,
                        )
                    })
                    .is_some()
            });
            if skip_unresolved_callable_recheck {
                continue;
            }

            let object_literal_function_param_spans = arg_idx
                .filter(|&arg_idx| {
                    self.ctx.arena.get(arg_idx).is_some_and(|node| {
                        node.kind == tsz_parser::parser::syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    })
                })
                .map(|arg_idx| self.object_literal_function_like_param_spans(arg_idx))
                .unwrap_or_default();
            let refresh_snap = self.ctx.snapshot_diagnostics();
            let actual = args
                .get(index)
                .copied()
                .map(|arg_idx| {
                    self.refreshed_generic_call_arg_type_with_context(
                        arg_idx,
                        cached_actual,
                        Some(expected),
                    )
                })
                .unwrap_or(cached_actual);
            let refreshed_object_literal_param_has_implicit_any = !object_literal_function_param_spans
                .is_empty()
                && self.ctx.speculative_diagnostics_since(&refresh_snap).iter().any(|diag| {
                    matches!(
                        diag.code,
                        crate::diagnostics::diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
                            | crate::diagnostics::diagnostic_codes::REST_PARAMETER_IMPLICITLY_HAS_AN_ANY_TYPE
                            | crate::diagnostics::diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE
                            | crate::diagnostics::diagnostic_codes::PARAMETER_HAS_A_NAME_BUT_NO_TYPE_DID_YOU_MEAN
                    ) && object_literal_function_param_spans
                        .iter()
                        .any(|(start, end)| diag.start >= *start && diag.start < *end)
                });
            let expected_is_concrete = expected != TypeId::UNKNOWN
                && expected != TypeId::ERROR
                && !common::contains_infer_types(self.ctx.types, expected)
                && !common::contains_type_parameters(self.ctx.types, expected);
            if expected_is_concrete && !refreshed_object_literal_param_has_implicit_any {
                self.ctx.diagnostics.retain(|diag| {
                    !matches!(
                        diag.code,
                        crate::diagnostics::diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
                            | crate::diagnostics::diagnostic_codes::REST_PARAMETER_IMPLICITLY_HAS_AN_ANY_TYPE
                            | crate::diagnostics::diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE
                            | crate::diagnostics::diagnostic_codes::PARAMETER_HAS_A_NAME_BUT_NO_TYPE_DID_YOU_MEAN
                    ) || !object_literal_function_param_spans
                        .iter()
                        .any(|(start, end)| diag.start >= *start && diag.start < *end)
                });
            }

            let is_assignable = self.is_assignable_to_with_env(actual, expected)
                || self.is_assignable_via_contextual_signatures(actual, expected);

            if !is_assignable {
                return CallResult::ArgumentTypeMismatch {
                    index,
                    expected,
                    actual,
                    fallback_return: success_return,
                };
            }
        }

        CallResult::Success(success_return)
    }

    pub(crate) fn compute_round2_contextual_types(
        &mut self,
        shape: &tsz_solver::FunctionShape,
        round1_instantiated_params: Option<&[tsz_solver::ParamInfo]>,
        sensitive_args: &[bool],
        current_substitution: &crate::query_boundaries::common::TypeSubstitution,
        arg_count: usize,
    ) -> Vec<Option<TypeId>> {
        let mut round2_contextual_types: Vec<Option<TypeId>> = Vec::with_capacity(arg_count);
        for i in 0..arg_count {
            let shape_round2_param =
                shape
                    .params
                    .get(i)
                    .map(|p| (p.type_id, p.rest))
                    .or_else(|| {
                        let last = shape.params.last()?;
                        last.rest.then_some((last.type_id, true))
                    });
            let round2_param = round1_instantiated_params
                .and_then(|params| {
                    params.get(i).map(|p| (p.type_id, p.rest)).or_else(|| {
                        let last = params.last()?;
                        last.rest.then_some((last.type_id, true))
                    })
                })
                .or(shape_round2_param);
            let is_sensitive = i < sensitive_args.len() && sensitive_args[i];
            let round2_param = if is_sensitive {
                match (shape_round2_param, round2_param) {
                    (Some(shape_param), Some(instantiated_param)) => {
                        let shape_is_genericish =
                            common::contains_infer_types(self.ctx.types, shape_param.0)
                                || common::contains_type_parameters(self.ctx.types, shape_param.0);
                        let instantiated_is_concrete =
                            !common::contains_infer_types(self.ctx.types, instantiated_param.0)
                                && !common::contains_type_parameters(
                                    self.ctx.types,
                                    instantiated_param.0,
                                );
                        if shape_is_genericish && instantiated_is_concrete {
                            Some(instantiated_param)
                        } else {
                            Some(shape_param)
                        }
                    }
                    (shape_param, None) => shape_param,
                    (None, instantiated_param) => instantiated_param,
                }
            } else {
                round2_param
            };
            let ctx_type = if let Some((param_type, is_rest_param)) = round2_param {
                let fresh_instantiated_from_shape =
                    shape_round2_param.map(|(shape_param_type, _)| {
                        crate::query_boundaries::common::instantiate_type(
                            self.ctx.types,
                            shape_param_type,
                            current_substitution,
                        )
                    });
                let round1_has_unknown =
                    common::contains_type_by_id(self.ctx.types, param_type, TypeId::UNKNOWN);
                let round1_has_error =
                    common::contains_type_by_id(self.ctx.types, param_type, TypeId::ERROR);
                let prefer_fresh_instantiation = is_sensitive
                    || round1_has_error
                    || common::contains_infer_types(self.ctx.types, param_type)
                    || common::contains_type_parameters(self.ctx.types, param_type)
                    || fresh_instantiated_from_shape.is_some_and(|fresh| {
                        (round1_has_unknown || round1_has_error)
                            && (common::contains_infer_types(self.ctx.types, fresh)
                                || common::contains_type_parameters(self.ctx.types, fresh))
                    });
                let instantiated = if round1_instantiated_params.is_some()
                    && !prefer_fresh_instantiation
                {
                    let original_param = shape_round2_param.map(|(type_id, _)| type_id);
                    if let Some(orig) = original_param
                        && let Some(tp_info) = common::type_param_info(self.ctx.types, orig)
                        && self
                            .unresolved_contextual_substitution_target(
                                &tp_info,
                                current_substitution,
                            )
                            .is_some()
                    {
                        let instantiated_constraint = match self
                            .instantiate_contextual_constraint_without_unresolved_self(
                                orig,
                                &tp_info,
                                current_substitution,
                            ) {
                            Some(instantiated_constraint) => instantiated_constraint,
                            None => param_type,
                        };
                        let evaluated_constraint =
                            self.evaluate_type_with_env(instantiated_constraint);
                        if !common::contains_type_parameters(self.ctx.types, evaluated_constraint) {
                            let constraint_specificity =
                                callable_param_specificity(self.ctx.types, evaluated_constraint);
                            let round1_specificity =
                                callable_param_specificity(self.ctx.types, param_type);
                            if constraint_specificity >= round1_specificity {
                                evaluated_constraint
                            } else {
                                param_type
                            }
                        } else {
                            param_type
                        }
                    } else {
                        param_type
                    }
                } else {
                    let base_param_type = if prefer_fresh_instantiation {
                        shape_round2_param
                            .map(|(type_id, _)| type_id)
                            .unwrap_or(param_type)
                    } else {
                        param_type
                    };
                    let inst = if let Some(tp_info) =
                        common::type_param_info(self.ctx.types, base_param_type)
                    {
                        if self
                            .unresolved_contextual_substitution_target(
                                &tp_info,
                                current_substitution,
                            )
                            .is_some()
                        {
                            self.instantiate_contextual_constraint_without_unresolved_self(
                                base_param_type,
                                &tp_info,
                                current_substitution,
                            )
                            .unwrap_or_else(|| {
                                crate::query_boundaries::common::instantiate_type(
                                    self.ctx.types,
                                    base_param_type,
                                    current_substitution,
                                )
                            })
                        } else {
                            crate::query_boundaries::common::instantiate_type(
                                self.ctx.types,
                                base_param_type,
                                current_substitution,
                            )
                        }
                    } else {
                        crate::query_boundaries::common::instantiate_type(
                            self.ctx.types,
                            base_param_type,
                            current_substitution,
                        )
                    };
                    if let Some(tp_info) = common::type_param_info(self.ctx.types, inst) {
                        let instantiated_constraint = match self
                            .instantiate_contextual_constraint_without_unresolved_self(
                                inst,
                                &tp_info,
                                current_substitution,
                            ) {
                            Some(instantiated_constraint) => instantiated_constraint,
                            None => inst,
                        };
                        let evaluated = self.evaluate_type_with_env(instantiated_constraint);
                        if !common::contains_type_parameters(self.ctx.types, evaluated) {
                            evaluated
                        } else {
                            inst
                        }
                    } else {
                        inst
                    }
                };
                let preserve_application_shape =
                    should_preserve_contextual_application_shape(self.ctx.types, instantiated);
                let evaluated = if common::contains_type_parameters(self.ctx.types, instantiated)
                    || common::contains_infer_types(self.ctx.types, instantiated)
                    || preserve_application_shape
                {
                    instantiated
                } else {
                    self.evaluate_type_with_env(instantiated)
                };
                Some(if is_rest_param {
                    self.rest_argument_element_type_with_env(evaluated)
                } else {
                    evaluated
                })
            } else {
                None
            };
            round2_contextual_types.push(ctx_type);
        }
        round2_contextual_types
    }

    pub(crate) fn compute_single_call_argument_type(
        &mut self,
        arg_idx: NodeIndex,
        expected_type: Option<TypeId>,
        check_excess_properties: bool,
        effective_index: usize,
        arg_count: usize,
        suppress_diagnostics: bool,
        callable_ctx: CallableContext,
    ) -> TypeId {
        use tsz_scanner::SyntaxKind;

        let syntax_needs_contextual = {
            let Some(node) = self.ctx.arena.get(arg_idx) else {
                return TypeId::ERROR;
            };
            let is_literal = matches!(
                node.kind,
                k if k == SyntaxKind::StringLiteral as u16
                    || k == SyntaxKind::NumericLiteral as u16
                    || k == SyntaxKind::BigIntLiteral as u16
                    || k == SyntaxKind::TrueKeyword as u16
                    || k == SyntaxKind::FalseKeyword as u16
                    || k == SyntaxKind::NullKeyword as u16
                    || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            );
            if is_literal {
                true
            } else {
                matches!(
                    node.kind,
                    k if k == syntax_kind_ext::ARROW_FUNCTION
                        || k == syntax_kind_ext::FUNCTION_EXPRESSION
                        || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                        || k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                        || k == syntax_kind_ext::CONDITIONAL_EXPRESSION
                        || k == syntax_kind_ext::CALL_EXPRESSION
                        || k == syntax_kind_ext::NEW_EXPRESSION
                        || k == syntax_kind_ext::YIELD_EXPRESSION
                        || k == syntax_kind_ext::TEMPLATE_EXPRESSION
                )
            }
        };
        let expected_is_unresolved = expected_type.is_some_and(|expected| {
            expected == TypeId::UNKNOWN
                || expected == TypeId::ERROR
                || common::contains_infer_types(self.ctx.types, expected)
        });
        if std::env::var_os("TSZ_DEBUG_CALL_ARG_CONTEXT").is_some()
            && self
                .ctx
                .file_name
                .contains("dependentDestructuredVariables")
            && let Some(expected) = expected_type
            && let Some(node) = self.ctx.arena.get(arg_idx)
            && (node.kind == syntax_kind_ext::ARROW_FUNCTION
                || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION)
        {
            eprintln!(
                "call-arg-context file={} arg_idx={} expected={} callable_ctx={:?}",
                self.ctx.file_name,
                arg_idx.0,
                self.format_type(expected),
                callable_ctx.callable_type.map(|ty| self.format_type(ty))
            );
        }
        let needs_contextual_signature_instantiation =
            self.expression_needs_contextual_signature_instantiation(arg_idx, expected_type);
        let apply_contextual = syntax_needs_contextual || needs_contextual_signature_instantiation;
        let expected_context_type =
            self.contextual_type_option_for_call_argument(expected_type, arg_idx, callable_ctx);
        let raw_context_requires_generic_epc_skip = expected_context_type.is_some_and(|ty| {
            common::contains_type_parameters(self.ctx.types, ty)
                || should_preserve_contextual_application_shape(self.ctx.types, ty)
        });
        let callable_context_requires_generic_epc_skip =
            callable_ctx.callable_type.is_some_and(|callable_type| {
                let ctx =
                    common::ContextualTypeContext::with_expected(self.ctx.types, callable_type);
                ctx.get_parameter_type_for_call(effective_index, arg_count)
                    .is_some_and(|param_type| {
                        common::contains_type_parameters(self.ctx.types, param_type)
                            || should_preserve_contextual_application_shape(
                                self.ctx.types,
                                param_type,
                            )
                    })
            });

        // Extract ThisType<T> marker from the unevaluated expected type BEFORE
        // contextual_type_for_expression evaluates it away. ThisType<T> is an empty
        // interface marker, so intersection simplification removes it. We need to
        // preserve it for object literal methods' `this` type.
        let pushed_this_type = if let Some(et) = expected_type {
            let ctx_helper = common::ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                et,
                self.ctx.compiler_options.no_implicit_any,
            );
            let _env = self.ctx.type_env.borrow();
            if let Some(this_type) = ctx_helper
                .get_this_type_from_marker()
                .or_else(|| ctx_helper.get_this_type_from_marker())
            {
                self.ctx.this_type_stack.push(this_type);
                true
            } else {
                false
            }
        } else {
            false
        };

        let skip_flow = if apply_contextual {
            false
        } else if let Some(node) = self.ctx.arena.get(arg_idx) {
            if node.kind != SyntaxKind::Identifier as u16 {
                false
            } else if let Some(sym_id) = self
                .ctx
                .binder
                .get_node_symbol(arg_idx)
                .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, arg_idx))
            {
                if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                    let value_decl = symbol.value_declaration;
                    if value_decl.is_none() || !self.is_const_variable_declaration(value_decl) {
                        false
                    } else if let Some(decl_node) = self.ctx.arena.get(value_decl) {
                        if let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) {
                            if var_decl.type_annotation.is_some() || var_decl.initializer.is_none()
                            {
                                false
                            } else if let Some(init_node) = self.ctx.arena.get(var_decl.initializer)
                            {
                                init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                    || init_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };
        let request = if apply_contextual {
            match expected_context_type {
                Some(ty) => TypingRequest::with_contextual_type(ty),
                None => TypingRequest::NONE,
            }
        } else if skip_flow {
            TypingRequest::for_write_context()
        } else {
            TypingRequest::NONE
        };

        // Snapshot diagnostic + closure state when in speculative round2.
        // Round2 marks closures as "already checked" even when their TS7006 diagnostics are later
        // dropped by the suppress filter. Without restoring, the final retry pass sees these
        // closures as already-checked and skips TS7006 — silencing real implicit-any errors
        // for parameters whose object-literal-property contextual type is never (e.g., an
        // extra key C in a negated-type-like constraint mapped type maps to never).
        let speculation_snap = suppress_diagnostics.then(|| self.ctx.snapshot_diagnostics());
        let implicit_any_closure_snapshot =
            suppress_diagnostics.then(|| self.ctx.implicit_any_checked_closures.clone());
        let provisional_context_snap =
            (!suppress_diagnostics && apply_contextual && expected_is_unresolved)
                .then(|| self.ctx.snapshot_diagnostics());
        let arg_type = self.get_type_of_node_with_request(arg_idx, &request);

        if check_excess_properties
            && let Some(expected) = expected_type
            && expected != TypeId::ANY
            && expected != TypeId::UNKNOWN
            && let Some(arg_node) = self.ctx.arena.get(arg_idx)
            && arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            && !is_type_parameter_type(self.ctx.types, expected)
            && !self
                .ctx
                .generic_excess_skip
                .as_ref()
                .is_some_and(|skip| effective_index < skip.len() && skip[effective_index])
            && !raw_context_requires_generic_epc_skip
            && !callable_context_requires_generic_epc_skip
            && !expected_is_unresolved
        {
            self.check_object_literal_excess_properties(arg_type, expected, arg_idx);
        }

        if let Some(snap) = &speculation_snap {
            let arg_node = self.ctx.arena.get(arg_idx);
            let object_literal_method_param_spans: Vec<(u32, u32)> = arg_node
                .filter(|node| node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
                .and_then(|node| self.ctx.arena.get_literal_expr(node))
                .map(|obj| {
                    obj.elements
                        .nodes
                        .iter()
                        .filter_map(|&element_idx| {
                            let element = self.ctx.arena.get(element_idx)?;
                            match element.kind {
                                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                                    .ctx
                                    .arena
                                    .get_method_decl(element)
                                    .map(|method| method.parameters.nodes.as_slice()),
                                k if k == syntax_kind_ext::GET_ACCESSOR
                                    || k == syntax_kind_ext::SET_ACCESSOR =>
                                {
                                    self.ctx
                                        .arena
                                        .get_accessor(element)
                                        .map(|accessor| accessor.parameters.nodes.as_slice())
                                }
                                _ => None,
                            }
                            .map(|params| {
                                params
                                    .iter()
                                    .filter_map(|&param_idx| {
                                        let param_node = self.ctx.arena.get(param_idx)?;
                                        Some((param_node.pos, param_node.end))
                                    })
                                    .collect::<Vec<_>>()
                            })
                        })
                        .flatten()
                        .collect()
                })
                .unwrap_or_default();
            let callback_body_start = arg_node
                .filter(|node| {
                    node.kind == syntax_kind_ext::ARROW_FUNCTION
                        || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                })
                .and_then(|node| self.ctx.arena.get_function(node))
                .and_then(|func| self.ctx.arena.get(func.body))
                .filter(|body_node| body_node.kind != syntax_kind_ext::BLOCK)
                .map(|body_node| body_node.pos);
            let diag_len = snap.diagnostics_len;
            // Build pre-existing diagnostic keys for exact dedup.
            let existing_diag_keys: Vec<_> = self
                .ctx
                .diagnostics
                .iter()
                .take(diag_len)
                .map(|d| (d.code, d.start, d.length, d.message_text.clone()))
                .collect();
            let mut seen_new_diags = FxHashSet::default();
            let mut seen_diag_keys = existing_diag_keys;
            self.ctx.rollback_diagnostics_filtered(snap, |diag| {
                if Self::should_preserve_speculative_call_diagnostic(diag) {
                    return true;
                }
                // --- Phase 1: dedup by (code, start) against pre-existing + already-kept ---
                let key = (diag.code, diag.start);
                if !seen_new_diags.insert(key) {
                    return false;
                }
                // Duplicate of a pre-speculation diagnostic — drop.
                if seen_diag_keys.iter().any(|existing| existing.0 == diag.code && existing.1 == diag.start) {
                    return false;
                }
                // --- Phase 2: classify the diagnostic ---
                let is_provisional_implicit_any = matches!(
                    diag.code,
                    diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
                        | diagnostic_codes::REST_PARAMETER_IMPLICITLY_HAS_AN_ANY_TYPE
                        | diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE
                        | diagnostic_codes::PARAMETER_HAS_A_NAME_BUT_NO_TYPE_DID_YOU_MEAN
                );
                let is_assignability = diag.code
                    == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                    || diag.code
                        == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE;
                let is_object_literal_diag = arg_node.is_some_and(|node| {
                    node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        && diag.start >= node.pos
                        && diag.start < node.end
                });
                let is_function_arg_implicit_any_diag = arg_node.is_some_and(|node| {
                    (node.kind == syntax_kind_ext::ARROW_FUNCTION
                        || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION)
                        && is_provisional_implicit_any
                        && diag.start >= node.pos
                        && diag.start < node.end
                });
                let is_function_arg_diag = arg_node.is_some_and(|node| {
                    (node.kind == syntax_kind_ext::ARROW_FUNCTION
                        || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION)
                        && diag.start >= node.pos
                        && diag.start < node.end
                });
                // Round-2/single-arg recomputes are speculative for direct callback
                // arguments. Keep their diagnostics owned by the final contextual
                // recheck so stale wide-generic errors (for example TS2339 from
                // `ClientEvents[string]`) do not leak past the instantiated retry.
                if is_function_arg_diag {
                    return false;
                }
                if expected_is_unresolved
                    && (is_function_arg_diag
                        || (is_object_literal_diag && is_provisional_implicit_any))
                {
                    return false;
                }
                // Keep implicit-any diagnostics (TS7006/TS7019/TS7031) from inside object
                // literals even in round2 speculative passes. Unlike assignability errors
                // (which get a definitive check in resolve_call_with_checker_adapter), TS7006
                // is determined by whether the contextual type is available in THIS pass.
                let implicit_any_in_object_literal =
                    is_provisional_implicit_any && is_object_literal_diag;
                let implicit_any_in_object_literal_method =
                    implicit_any_in_object_literal
                        && object_literal_method_param_spans
                            .iter()
                            .any(|(start, end)| diag.start >= *start && diag.start < *end);
                let keep = (!is_assignability && !is_provisional_implicit_any)
                    || (implicit_any_in_object_literal
                        && !implicit_any_in_object_literal_method)
                    || callback_body_start.is_some_and(|start| diag.start == start)
                    || !(is_object_literal_diag || is_function_arg_implicit_any_diag);
                // --- Phase 3: exact-message dedup for kept diagnostics ---
                if keep {
                    let full_key = (
                        diag.code,
                        diag.start,
                        diag.length,
                        diag.message_text.clone(),
                    );
                    if seen_diag_keys.iter().any(|existing| existing == &full_key) {
                        return false;
                    }
                    seen_diag_keys.push(full_key);
                }
                keep
            });
            // Restore implicit-any closure tracking to the pre-round2 state so the final
            // retry pass can re-emit TS7006 for closures whose diagnostics were suppressed.
            if let Some(snapshot) = implicit_any_closure_snapshot {
                let contextual_closures: Vec<_> = self
                    .ctx
                    .implicit_any_contextual_closures
                    .iter()
                    .copied()
                    .collect();
                self.ctx.restore_implicit_any_closures(&snapshot);
                self.ctx
                    .implicit_any_checked_closures
                    .extend(contextual_closures);
            }
        }
        if let Some(snap) = &provisional_context_snap {
            let arg_node = self.ctx.arena.get(arg_idx);
            self.ctx.rollback_diagnostics_filtered(snap, |diag| {
                Self::should_preserve_speculative_call_diagnostic(diag)
                    || !arg_node.is_some_and(|node| {
                        diag.start >= node.pos
                            && diag.start < node.end
                            && (node.kind == syntax_kind_ext::ARROW_FUNCTION
                                || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                                || node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                || node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION)
                    })
            });
        }

        if pushed_this_type {
            self.ctx.this_type_stack.pop();
        }
        arg_type
    }
}
