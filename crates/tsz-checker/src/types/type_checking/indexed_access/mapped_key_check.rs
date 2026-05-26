//! Helpers for mapped-type key validity checks used by TS2536 validation.
//!
//! Extracted from `indexed_access.rs` to keep that file under the 2000-line
//! checker-boundary limit enforced by `scripts/arch/arch_guard.py`.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

use super::indexed_access_helpers::same_object_key_space;

impl<'a> CheckerState<'a> {
    /// Returns `true` when the indexed access `T[P]` (at `node_idx`) is valid because
    /// `P` iterates over a key space that is guaranteed to be a subset of `keyof T`.
    ///
    /// The check walks up the AST from the indexed access node to find the nearest
    /// enclosing mapped type `[P in C]: ...`. It then verifies that `C` is, or
    /// resolves to, `keyof T` for the same object being indexed.
    ///
    /// An additional outer-mapped-type walk handles the nested type-argument case:
    /// when `C` is a bare name (e.g. `K`) referencing an outer mapped-type iteration
    /// variable `[K in keyof T]`, this walk finds that outer definition and checks
    /// its constraint.
    pub(super) fn is_mapped_key_index_for_current_object(
        &mut self,
        node_idx: NodeIndex,
        object_node_idx: NodeIndex,
        index_node_idx: NodeIndex,
        object_type: TypeId,
        object_type_for_check: TypeId,
    ) -> bool {
        let Some(index_name) = self.simple_type_reference_name(index_node_idx) else {
            return false;
        };

        let mut current = self.ctx.arena.parent_of(node_idx);
        while current.is_some() {
            let parent_idx = current.expect("loop guard ensures current.is_some()");
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };
            if parent_node.kind == syntax_kind_ext::MAPPED_TYPE {
                let Some(mapped) = self.ctx.arena.get_mapped_type(parent_node) else {
                    return false;
                };
                let Some(tp_node) = self.ctx.arena.get(mapped.type_parameter) else {
                    return false;
                };
                let Some(tp) = self.ctx.arena.get_type_parameter(tp_node) else {
                    return false;
                };
                let Some(name_node) = self.ctx.arena.get(tp.name) else {
                    return false;
                };
                let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                    return false;
                };
                if ident.escaped_text != index_name || tp.constraint == NodeIndex::NONE {
                    return false;
                }
                let Some(constraint_node) = self.ctx.arena.get(tp.constraint) else {
                    return false;
                };
                // Check if the constraint is `keyof X` directly
                if let Some(type_operator) = self.ctx.arena.get_type_operator(constraint_node)
                    && type_operator.operator == SyntaxKind::KeyOfKeyword as u16
                {
                    return self.mapped_keyof_target_matches_object(
                        type_operator.type_node,
                        object_node_idx,
                        object_type,
                        object_type_for_check,
                    );
                }
                // Check if the constraint is an intersection containing `keyof X`
                // (e.g., `[K in keyof T & keyof U]`)
                if constraint_node.kind == syntax_kind_ext::INTERSECTION_TYPE
                    && let Some(composite) = self.ctx.arena.get_composite_type(constraint_node)
                {
                    return composite.types.nodes.iter().any(|&member_idx| {
                        self.ctx
                            .arena
                            .get(member_idx)
                            .and_then(|n| self.ctx.arena.get_type_operator(n))
                            .is_some_and(|op| {
                                op.operator == SyntaxKind::KeyOfKeyword as u16
                                    && self.mapped_keyof_target_matches_object(
                                        op.type_node,
                                        object_node_idx,
                                        object_type,
                                        object_type_for_check,
                                    )
                            })
                    });
                }
                if self.type_reference_filters_keyof_current_object(
                    tp.constraint,
                    object_node_idx,
                    object_type,
                    object_type_for_check,
                ) {
                    return true;
                }
                // AST-based outer mapped type walk (scope-independent).
                //
                // When the inner mapped type `[P in K]` has a simple-name constraint like
                // `K`, walk the AST upward from the current mapped type to find the outer
                // mapped type that defines `K` as its iteration variable. If that outer
                // mapped type constrains `K` to `keyof T` for our object, the index is
                // valid regardless of whether `K` is currently in type_parameter_scope.
                //
                // This handles nested mapped types in type-argument positions (e.g.
                // `Pair<{ [P in K]: T[P] }, ...>` inside `{ [K in keyof T]: ... }`) where
                // validate_type_args_against_params runs check_type_node on the inner
                // mapped type before the outer MAPPED_TYPE handler pushes K into scope.
                //
                // Shadowing guard: when an intervening function/constructor type declares a
                // type parameter with the same name as `K`, that parameter shadows the outer
                // mapped-type variable, so the walk stops.
                if let Some(constraint_ref_name) = self.simple_type_reference_name(tp.constraint) {
                    let mut outer_cur = self.ctx.arena.parent_of(parent_idx);
                    for _ in 0..8 {
                        let Some(outer_idx) = outer_cur else { break };
                        let Some(outer_node) = self.ctx.arena.get(outer_idx) else {
                            break;
                        };
                        let type_parameters_declares_name =
                            |checker: &CheckerState<'_>,
                             type_parameters: &tsz_parser::parser::NodeList,
                             name: &str| {
                                type_parameters.nodes.iter().any(|&tp_idx| {
                                    checker
                                        .ctx
                                        .arena
                                        .get(tp_idx)
                                        .and_then(|n| checker.ctx.arena.get_type_parameter(n))
                                        .and_then(|tp| checker.ctx.arena.get(tp.name))
                                        .and_then(|n| checker.ctx.arena.get_identifier(n))
                                        .is_some_and(|id| id.escaped_text == name)
                                })
                            };
                        // Stop when an intervening binder (function/constructor type) shadows
                        // the constraint name with its own type parameter.
                        if let Some(sig) = self.ctx.arena.get_signature(outer_node)
                            && let Some(ref tps) = sig.type_parameters
                            && type_parameters_declares_name(self, tps, &constraint_ref_name)
                        {
                            break;
                        }
                        if let Some(function_type) = self.ctx.arena.get_function_type(outer_node)
                            && let Some(ref tps) = function_type.type_parameters
                            && type_parameters_declares_name(self, tps, &constraint_ref_name)
                        {
                            break;
                        }
                        if outer_node.kind == syntax_kind_ext::MAPPED_TYPE {
                            let Some(outer_mapped) = self.ctx.arena.get_mapped_type(outer_node)
                            else {
                                break;
                            };
                            let Some(outer_tp_node) =
                                self.ctx.arena.get(outer_mapped.type_parameter)
                            else {
                                break;
                            };
                            let Some(outer_tp) = self.ctx.arena.get_type_parameter(outer_tp_node)
                            else {
                                break;
                            };
                            let Some(outer_name_node) = self.ctx.arena.get(outer_tp.name) else {
                                break;
                            };
                            let Some(outer_ident) = self.ctx.arena.get_identifier(outer_name_node)
                            else {
                                break;
                            };
                            if outer_ident.escaped_text == constraint_ref_name
                                && outer_tp.constraint != tsz_parser::parser::NodeIndex::NONE
                            {
                                let Some(outer_c_node) = self.ctx.arena.get(outer_tp.constraint)
                                else {
                                    break;
                                };
                                // Direct `keyof T` constraint on the outer mapped type.
                                if let Some(outer_type_op) =
                                    self.ctx.arena.get_type_operator(outer_c_node)
                                    && outer_type_op.operator == SyntaxKind::KeyOfKeyword as u16
                                    && self.mapped_keyof_target_matches_object(
                                        outer_type_op.type_node,
                                        object_node_idx,
                                        object_type,
                                        object_type_for_check,
                                    )
                                {
                                    return true;
                                }
                                // Semantic fallback for expression/alias outer constraints.
                                let outer_c_type =
                                    self.get_type_from_type_node(outer_tp.constraint);
                                if outer_c_type != tsz_solver::TypeId::ERROR
                                    && outer_c_type != tsz_solver::TypeId::UNKNOWN
                                {
                                    let outer_c_eval = self.evaluate_type_with_env(outer_c_type);
                                    let keyof_obj = self.ctx.types.factory().keyof(object_type);
                                    if self
                                        .diagnostic_relation_boolean_guard(outer_c_eval, keyof_obj)
                                        || self.is_keyof_for_current_object(
                                            outer_c_eval,
                                            object_type,
                                            object_type_for_check,
                                        )
                                        || self.is_keyof_for_current_object(
                                            outer_c_type,
                                            object_type,
                                            object_type_for_check,
                                        )
                                    {
                                        return true;
                                    }
                                }
                                break; // found the definition; did not satisfy check
                            }
                        }
                        outer_cur = self.ctx.arena.parent_of(outer_idx);
                    }
                }
                // Semantic fallback for alias/type-expression constraints
                // (e.g. `optionalKeys<T>`, `Extract<keyof T, string>`).
                if crate::query_boundaries::common::is_type_parameter_like(
                    self.ctx.types,
                    object_type,
                ) {
                    let constraint_type = self.get_type_from_type_node(tp.constraint);
                    let constraint_eval = self.evaluate_type_with_env(constraint_type);
                    let keyof_object_param = self.ctx.types.factory().keyof(object_type);
                    if self.diagnostic_relation_boolean_guard(constraint_eval, keyof_object_param) {
                        return true;
                    }
                    // Also handle constraints that structurally contain `keyof T`.
                    if self.is_keyof_for_current_object(
                        constraint_eval,
                        object_type,
                        object_type_for_check,
                    ) || self.is_keyof_for_current_object(
                        constraint_type,
                        object_type,
                        object_type_for_check,
                    ) || self.mapped_key_constraint_filters_current_object_keys(
                        constraint_type,
                        object_type,
                        object_type_for_check,
                    ) {
                        return true;
                    }
                    // Follow the constraint chain transitively (P → K → keyof T).
                    let mut chain = constraint_type;
                    for _ in 0..4 {
                        let Some(next) = crate::query_boundaries::common::type_parameter_constraint(
                            self.ctx.types,
                            chain,
                        ) else {
                            break;
                        };
                        let next_eval = self.evaluate_type_with_env(next);
                        if self.is_keyof_for_current_object(
                            next_eval,
                            object_type,
                            object_type_for_check,
                        ) || self.is_keyof_for_current_object(
                            next,
                            object_type,
                            object_type_for_check,
                        ) {
                            return true;
                        }
                        if self.diagnostic_relation_boolean_guard(next_eval, keyof_object_param) {
                            return true;
                        }
                        if !crate::query_boundaries::common::is_type_parameter_like(
                            self.ctx.types,
                            next_eval,
                        ) {
                            break;
                        }
                        chain = next_eval;
                    }
                }
                return false;
            }
            current = self
                .ctx
                .arena
                .get_extended(parent_idx)
                .map(|ext| ext.parent);
        }

        false
    }

    fn type_reference_filters_keyof_current_object(
        &mut self,
        node_idx: NodeIndex,
        object_node_idx: NodeIndex,
        object_type: TypeId,
        object_type_for_check: TypeId,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return false;
        };
        let Some(type_args) = type_ref.type_arguments.as_ref() else {
            return false;
        };
        let Some(&first_arg) = type_args.nodes.first() else {
            return false;
        };
        self.type_node_is_keyof_current_object(
            first_arg,
            object_node_idx,
            object_type,
            object_type_for_check,
        )
    }

    fn type_node_is_keyof_current_object(
        &mut self,
        node_idx: NodeIndex,
        object_node_idx: NodeIndex,
        object_type: TypeId,
        object_type_for_check: TypeId,
    ) -> bool {
        self.ctx
            .arena
            .get(node_idx)
            .and_then(|node| self.ctx.arena.get_type_operator(node))
            .is_some_and(|type_operator| {
                type_operator.operator == SyntaxKind::KeyOfKeyword as u16
                    && self.mapped_keyof_target_matches_object(
                        type_operator.type_node,
                        object_node_idx,
                        object_type,
                        object_type_for_check,
                    )
            })
    }

    /// Check if the keyof target in a mapped type constraint matches the object being indexed.
    /// Handles: direct name match, indexed access type objects, and cross-type extends.
    pub(super) fn mapped_keyof_target_matches_object(
        &mut self,
        keyof_target_node: NodeIndex,
        object_node_idx: NodeIndex,
        object_type: TypeId,
        object_type_for_check: TypeId,
    ) -> bool {
        // Direct name match: `keyof T` and object is `T`
        let keyof_target_name = self.simple_type_reference_name(keyof_target_node);
        let object_name = self.simple_type_reference_name(object_node_idx);
        if keyof_target_name.is_some() && object_name.is_some() && keyof_target_name == object_name
        {
            return true;
        }

        // Indexed access match: `keyof T["_type"]` and object is `T["_type"]`
        // Compare via AST structure for indexed access type objects.
        if let Some(keyof_target_node_data) = self.ctx.arena.get(keyof_target_node)
            && keyof_target_node_data.kind == syntax_kind_ext::INDEXED_ACCESS_TYPE
            && let Some(object_node_data) = self.ctx.arena.get(object_node_idx)
            && object_node_data.kind == syntax_kind_ext::INDEXED_ACCESS_TYPE
        {
            // Compare the indexed access types structurally via AST text
            if let Some(keyof_iat) = self
                .ctx
                .arena
                .get_indexed_access_type(keyof_target_node_data)
                && let Some(object_iat) = self.ctx.arena.get_indexed_access_type(object_node_data)
            {
                let keyof_obj_name = self.simple_type_reference_name(keyof_iat.object_type);
                let obj_obj_name = self.simple_type_reference_name(object_iat.object_type);
                if keyof_obj_name.is_some()
                    && keyof_obj_name == obj_obj_name
                    && self.nodes_have_same_text(keyof_iat.index_type, object_iat.index_type)
                {
                    return true;
                }
            }
        }

        // Cross-type extends: `keyof T` and object is `U` where `U extends T`.
        // Since U extends T, keyof T ⊆ keyof U, so a mapped key over keyof T can index U.
        if let Some(ref target_name) = keyof_target_name {
            // Check if the object type parameter has a constraint matching the keyof target
            let object_constraint = crate::query_boundaries::common::type_parameter_constraint(
                self.ctx.types,
                object_type,
            )
            .or_else(|| {
                crate::query_boundaries::common::type_parameter_constraint(
                    self.ctx.types,
                    object_type_for_check,
                )
            });
            if let Some(constraint) = object_constraint {
                // Check if the constraint's type parameter name matches the keyof target
                if let Some(info) =
                    crate::query_boundaries::common::type_param_info(self.ctx.types, constraint)
                {
                    let constraint_name = self.ctx.types.resolve_atom(info.name);
                    if constraint_name == *target_name {
                        return true;
                    }
                }
                // Also check by TypeId: resolve keyof target type and compare
                let keyof_target_type = self.get_type_from_type_node(keyof_target_node);
                if same_object_key_space(self.ctx.types, constraint, keyof_target_type) {
                    return true;
                }
            }
        }

        false
    }

    pub(super) fn mapped_key_constraint_filters_current_object_keys(
        &mut self,
        mut constraint_type: TypeId,
        object_type: TypeId,
        object_type_for_check: TypeId,
    ) -> bool {
        let mut seen = rustc_hash::FxHashSet::default();
        for _ in 0..8 {
            if !seen.insert(constraint_type) {
                return false;
            }

            if let Some((check_type, _extends_type, true_type, false_type)) =
                crate::query_boundaries::checkers::generic::full_conditional_type_components(
                    self.ctx.types.as_type_database(),
                    constraint_type,
                )
            {
                let keyof_object = self.ctx.types.factory().keyof(object_type);
                return [check_type, true_type, false_type]
                    .into_iter()
                    .filter(|&candidate| candidate != TypeId::NEVER)
                    .any(|candidate| {
                        let evaluated = self.evaluate_type_with_env(candidate);
                        self.is_keyof_for_current_object(
                            candidate,
                            object_type,
                            object_type_for_check,
                        ) || self.is_keyof_for_current_object(
                            evaluated,
                            object_type,
                            object_type_for_check,
                        ) || self.diagnostic_relation_boolean_guard(evaluated, keyof_object)
                    });
            }

            if let Some(param_info) =
                crate::query_boundaries::common::type_param_info(self.ctx.types, constraint_type)
                && let Some(constraint) = param_info.constraint
            {
                constraint_type = constraint;
                continue;
            }

            if let Some(name_atom) = crate::query_boundaries::checkers::generic::type_parameter_name(
                self.ctx.types.as_type_database(),
                constraint_type,
            ) {
                let name = self.ctx.types.resolve_atom(name_atom);
                if let Some(&scoped_type_id) = self.ctx.type_parameter_scope.get(&name)
                    && scoped_type_id != constraint_type
                    && let Some(constraint) =
                        crate::query_boundaries::common::type_parameter_constraint(
                            self.ctx.types,
                            scoped_type_id,
                        )
                {
                    constraint_type = constraint;
                    continue;
                }
            }

            let Some(app) =
                crate::query_boundaries::common::type_application(self.ctx.types, constraint_type)
            else {
                let evaluated = self.evaluate_type_with_env(constraint_type);
                if evaluated == constraint_type {
                    return false;
                }
                constraint_type = evaluated;
                continue;
            };
            let Some(def_id) =
                crate::query_boundaries::common::lazy_def_id(self.ctx.types, app.base)
            else {
                return false;
            };
            let body_and_params = self
                .ctx
                .definition_store
                .get(def_id)
                .and_then(|def| {
                    (def.kind == tsz_solver::def::DefKind::TypeAlias)
                        .then_some((def.body?, def.type_params))
                })
                .or_else(|| {
                    let body = self
                        .ctx
                        .type_env
                        .try_borrow()
                        .ok()
                        .and_then(|env| env.get_def(def_id))?;
                    let params = self.ctx.get_def_type_params(def_id)?;
                    Some((body, params))
                });
            let Some((body, params)) = body_and_params else {
                return false;
            };
            if params.len() != app.args.len() {
                return false;
            };
            let subst = crate::query_boundaries::common::TypeSubstitution::from_args(
                self.ctx.types,
                &params,
                &app.args,
            );
            let instantiated =
                crate::query_boundaries::common::instantiate_type(self.ctx.types, body, &subst);
            if instantiated == constraint_type {
                return false;
            }
            constraint_type = self.resolve_lazy_type(instantiated);
        }
        false
    }
}
