//! Indexed access type validation (`T[K]`).
//!
//! Validates that the index type `K` is assignable to `keyof T` for indexed
//! access type nodes, emitting TS2536 when the constraint is violated.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

fn is_broad_index_type(db: &dyn tsz_solver::TypeDatabase, ty: TypeId) -> bool {
    if matches!(ty, TypeId::STRING | TypeId::NUMBER | TypeId::SYMBOL) {
        return true;
    }

    tsz_solver::type_queries::get_union_members(db, ty).is_some_and(|members| {
        !members.is_empty()
            && members
                .iter()
                .all(|&member| is_broad_index_type(db, member))
    })
}

fn same_type_param_name(db: &dyn tsz_solver::TypeDatabase, left: TypeId, right: TypeId) -> bool {
    tsz_solver::type_queries::get_type_parameter_info(db, left)
        .zip(tsz_solver::type_queries::get_type_parameter_info(db, right))
        .is_some_and(|(l, r)| l.name == r.name)
}

fn same_object_key_space(db: &dyn tsz_solver::TypeDatabase, left: TypeId, right: TypeId) -> bool {
    left == right || same_type_param_name(db, left, right)
}

impl<'a> CheckerState<'a> {
    fn simple_type_reference_name(&self, node_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(node_idx)?;
        if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            let type_ref = self.ctx.arena.get_type_ref(node)?;
            let name_node = self.ctx.arena.get(type_ref.type_name)?;
            let ident = self.ctx.arena.get_identifier(name_node)?;
            return Some(ident.escaped_text.clone());
        }
        if node.kind == SyntaxKind::Identifier as u16 {
            let ident = self.ctx.arena.get_identifier(node)?;
            return Some(ident.escaped_text.clone());
        }
        None
    }

    fn is_mapped_key_index_for_current_object(
        &self,
        node_idx: NodeIndex,
        object_node_idx: NodeIndex,
        index_node_idx: NodeIndex,
    ) -> bool {
        let Some(index_name) = self.simple_type_reference_name(index_node_idx) else {
            return false;
        };
        let Some(object_name) = self.simple_type_reference_name(object_node_idx) else {
            return false;
        };

        let mut current = self.ctx.arena.get_extended(node_idx).map(|ext| ext.parent);
        while current.is_some() {
            let parent_idx = current.unwrap();
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
                let Some(type_operator) = self.ctx.arena.get_type_operator(constraint_node) else {
                    return false;
                };
                if type_operator.operator != SyntaxKind::KeyOfKeyword as u16 {
                    return false;
                }
                return self
                    .simple_type_reference_name(type_operator.type_node)
                    .as_deref()
                    .is_some_and(|name| name == object_name);
            }
            current = self
                .ctx
                .arena
                .get_extended(parent_idx)
                .map(|ext| ext.parent);
        }

        false
    }

    fn is_keyof_for_current_object(
        &mut self,
        ty: TypeId,
        object_type: TypeId,
        object_type_for_check: TypeId,
    ) -> bool {
        tsz_solver::type_queries::get_keyof_type(self.ctx.types, ty).is_some_and(|operand| {
            let evaluated_operand = self.evaluate_type_with_env(operand);
            same_object_key_space(self.ctx.types, operand, object_type)
                || same_object_key_space(self.ctx.types, operand, object_type_for_check)
                || same_object_key_space(self.ctx.types, evaluated_operand, object_type)
                || same_object_key_space(self.ctx.types, evaluated_operand, object_type_for_check)
        })
    }

    /// Resolve a type parameter's constraint from its AST declaration when the TypeId
    /// doesn't carry one. This handles cases where type parameters lose their constraints
    /// during type application argument resolution (e.g., `M[Event]` inside `Id<M[Event]>`).
    fn resolve_index_constraint_from_declaration(
        &mut self,
        index_node_idx: NodeIndex,
        _object_node_idx: NodeIndex,
    ) -> Option<TypeId> {
        let index_name = self.simple_type_reference_name(index_node_idx)?;

        let mut current = self
            .ctx
            .arena
            .get_extended(index_node_idx)
            .map(|ext| ext.parent);
        while let Some(parent_idx) = current {
            let parent_node = self.ctx.arena.get(parent_idx)?;
            // Extract type_parameters NodeList from any generic declaration kind
            let type_params: Option<&tsz_parser::parser::base::NodeList> = match parent_node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION =>
                {
                    self.ctx
                        .arena
                        .get_function(parent_node)
                        .and_then(|f| f.type_parameters.as_ref())
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::METHOD_SIGNATURE
                    || k == syntax_kind_ext::CALL_SIGNATURE
                    || k == syntax_kind_ext::CONSTRUCT_SIGNATURE =>
                {
                    self.ctx
                        .arena
                        .get_signature(parent_node)
                        .and_then(|s| s.type_parameters.as_ref())
                }
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => self
                    .ctx
                    .arena
                    .get_interface(parent_node)
                    .and_then(|i| i.type_parameters.as_ref()),
                k if k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION =>
                {
                    self.ctx
                        .arena
                        .get_class(parent_node)
                        .and_then(|c| c.type_parameters.as_ref())
                }
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => self
                    .ctx
                    .arena
                    .get_type_alias(parent_node)
                    .and_then(|ta| ta.type_parameters.as_ref()),
                k if k == syntax_kind_ext::FUNCTION_TYPE
                    || k == syntax_kind_ext::CONSTRUCTOR_TYPE =>
                {
                    self.ctx
                        .arena
                        .get_function_type(parent_node)
                        .and_then(|ft| ft.type_parameters.as_ref())
                }
                _ => None,
            };

            if let Some(tp_list) = type_params {
                for &tp_idx in &tp_list.nodes {
                    let Some(tp_node) = self.ctx.arena.get(tp_idx) else {
                        continue;
                    };
                    let Some(tp) = self.ctx.arena.get_type_parameter(tp_node) else {
                        continue;
                    };
                    let Some(name_node) = self.ctx.arena.get(tp.name) else {
                        continue;
                    };
                    let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                        continue;
                    };
                    if ident.escaped_text == index_name && tp.constraint != NodeIndex::NONE {
                        let constraint_type = self.get_type_from_type_node(tp.constraint);
                        if constraint_type != TypeId::ERROR {
                            return Some(constraint_type);
                        }
                    }
                }
            }
            current = self
                .ctx
                .arena
                .get_extended(parent_idx)
                .map(|ext| ext.parent);
        }
        None
    }

    /// Check if the index type parameter has a `keyof` constraint targeting the object type,
    /// resolved from the AST declaration. Returns true if `K extends keyof T` for the current
    /// object T.
    fn index_has_keyof_constraint_from_declaration(
        &mut self,
        index_node_idx: NodeIndex,
        object_node_idx: NodeIndex,
        object_type: TypeId,
        object_type_for_check: TypeId,
    ) -> bool {
        if let Some(constraint_type) =
            self.resolve_index_constraint_from_declaration(index_node_idx, object_node_idx)
        {
            // Check if the constraint is `keyof T` for our object
            if self.is_keyof_for_current_object(constraint_type, object_type, object_type_for_check)
            {
                return true;
            }
            // Also check if the constraint is directly assignable to keyof of the object
            // (handles cases like `K extends string` indexing `Record<string, V>`)
        }
        false
    }

    /// Check an indexed access type (T[K]).
    pub(crate) fn check_indexed_access_type(&mut self, node_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };
        let Some(data) = self.ctx.arena.get_indexed_access_type(node) else {
            return;
        };

        let object_type = self.get_type_from_type_node(data.object_type);
        let index_type = self.get_type_from_type_node(data.index_type);
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        if object_type == TypeId::ERROR
            || index_type == TypeId::ERROR
            || object_type == TypeId::ANY
            || index_type == TypeId::ANY
            || index_type == TypeId::NEVER
        {
            return;
        }

        let mut index_constraint =
            tsz_solver::type_queries::get_type_parameter_constraint(self.ctx.types, index_type);
        // Fallback: when the index type is a type parameter but its TypeId doesn't carry a
        // constraint (happens when T[K] appears inside type application arguments like
        // `Id<T[K]>`), resolve the constraint from the AST declaration.
        if index_constraint.is_none()
            && tsz_solver::type_queries::is_type_parameter_like(self.ctx.types, index_type)
        {
            index_constraint =
                self.resolve_index_constraint_from_declaration(data.index_type, data.object_type);
        }
        let error_anchor =
            if tsz_solver::type_queries::is_type_parameter_like(self.ctx.types, index_type) {
                node_idx
            } else {
                data.index_type
            };
        if tsz_solver::type_queries::is_type_parameter_like(self.ctx.types, object_type)
            && index_constraint.is_some_and(|constraint| {
                constraint == object_type
                    || same_type_param_name(self.ctx.types, constraint, object_type)
            })
        {
            let obj_type_str = self.format_type(object_type);
            let index_type_str = self.format_type(index_type);
            let message_2536 = format_message(
                diagnostic_messages::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
                &[&index_type_str, &obj_type_str],
            );
            self.error_at_node(
                error_anchor,
                &message_2536,
                diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
            );
            return;
        }

        // Fast path: when the index is a type parameter and the object type node
        // is a type literal, compute keyof from AST property names only (no
        // value-type evaluation needed). This avoids eagerly resolving complex
        // member types (e.g., generic type applications) just to check key validity.
        if tsz_solver::type_queries::is_type_parameter_like(self.ctx.types, index_type)
            && let Some(obj_node) = self.ctx.arena.get(data.object_type)
            && obj_node.kind == syntax_kind_ext::TYPE_LITERAL
            && let Some(type_lit) = self.ctx.arena.get_type_literal(obj_node)
        {
            let mut key_types = Vec::new();
            for &member_idx in &type_lit.members.nodes {
                if let Some(member_node) = self.ctx.arena.get(member_idx)
                    && let Some(sig) = self.ctx.arena.get_signature(member_node)
                    && let Some(name) = self.get_property_name(sig.name)
                {
                    let atom = self.ctx.types.intern_string(&name);
                    key_types.push(self.ctx.types.factory().literal_string_atom(atom));
                }
            }
            if !key_types.is_empty() {
                let keyof_type = self.ctx.types.factory().union(key_types);
                let check_index = index_constraint.unwrap_or(index_type);
                let check_index_eval = self.evaluate_type_with_env(check_index);
                if self.is_assignable_to(check_index_eval, keyof_type) {
                    return;
                }
            }
        }

        let mut object_type_for_check = self.evaluate_type_with_env(object_type);
        // Indexing `never` is always valid (produces `never`), so suppress TS2536.
        // This handles cases like `(A & B)['kind']` where `A & B` reduces to `never`
        // due to conflicting discriminant properties.
        if object_type_for_check == TypeId::NEVER {
            return;
        }
        object_type_for_check = tsz_solver::type_queries::get_type_parameter_constraint(
            self.ctx.types,
            object_type_for_check,
        )
        .unwrap_or(object_type_for_check);
        if let Some((base_object_type, access_index_type)) =
            tsz_solver::type_queries::get_index_access_types(self.ctx.types, object_type_for_check)
            && let Some(base_constraint) = tsz_solver::type_queries::get_type_parameter_constraint(
                self.ctx.types,
                base_object_type,
            )
        {
            let constrained_access = self
                .ctx
                .types
                .factory()
                .index_access(base_constraint, access_index_type);
            let evaluated_constrained_access =
                self.evaluate_type_for_assignability(constrained_access);
            if evaluated_constrained_access != TypeId::ERROR {
                object_type_for_check = evaluated_constrained_access;
            }
        }
        if tsz_solver::is_generic_application(self.ctx.types, object_type_for_check) {
            let expanded_object = self.evaluate_application_type(object_type_for_check);
            if expanded_object != TypeId::ERROR && expanded_object != TypeId::ANY {
                object_type_for_check = expanded_object;
            }
        }
        let keyof_object = if let Some(mapped_id) =
            tsz_solver::mapped_type_id(self.ctx.types, object_type_for_check)
        {
            let mapped = self.ctx.types.mapped_type(mapped_id);
            let mapped_constraint = mapped.constraint;
            let keyof = self.evaluate_mapped_constraint_with_resolution(mapped_constraint);

            // When the index is `keyof T` and the mapped type iterates over `keyof T`
            // (same T), the index is always valid. Check both the raw constraint and
            // the evaluated result for structural equivalence via same_object_key_space.
            if let Some(index_operand) =
                tsz_solver::type_queries::get_keyof_type(self.ctx.types, index_type)
            {
                if let Some(constraint_operand) =
                    tsz_solver::type_queries::get_keyof_type(self.ctx.types, mapped_constraint)
                    && same_object_key_space(self.ctx.types, index_operand, constraint_operand)
                {
                    return;
                }
                // Also check against the evaluated keyof result
                if let Some(keyof_operand) =
                    tsz_solver::type_queries::get_keyof_type(self.ctx.types, keyof)
                    && same_object_key_space(self.ctx.types, index_operand, keyof_operand)
                {
                    return;
                }
            }

            keyof
        } else {
            self.ctx.types.evaluate_keyof(object_type_for_check)
        };
        let is_self_derived_key_space = |candidate: TypeId| {
            tsz_solver::type_queries::get_index_access_types(self.ctx.types, candidate).is_some_and(
                |(derived_object, derived_index)| {
                    tsz_solver::type_queries::is_type_parameter_like(self.ctx.types, index_type)
                        && !tsz_solver::type_queries::is_type_parameter_like(
                            self.ctx.types,
                            derived_object,
                        )
                        && (derived_index == index_type
                            || same_type_param_name(self.ctx.types, derived_index, index_type))
                },
            )
        };
        if is_self_derived_key_space(keyof_object)
            || is_self_derived_key_space(self.evaluate_type_with_env(keyof_object))
        {
            let obj_type_str = self.format_type(object_type);
            let index_type_str = self.format_type(index_type);
            let message_2536 = format_message(
                diagnostic_messages::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
                &[&index_type_str, &obj_type_str],
            );
            self.error_at_node(
                error_anchor,
                &message_2536,
                diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
            );
            return;
        }

        let index_type_for_check = self.evaluate_type_with_env(index_type);
        // First check: raw index type against keyof.
        // This handles cases where keyof includes type parameters from mapped types
        // (e.g. keyof ({ [P in T]: P } & ...) = T | ...) and the index IS that parameter.
        if self.is_assignable_to(index_type_for_check, keyof_object) {
            return;
        }
        if self.is_mapped_key_index_for_current_object(node_idx, data.object_type, data.index_type)
        {
            return;
        }
        // Follow the constraint chain transitively (P -> K -> keyof T) so that
        // e.g. T[P] where P extends K extends keyof T doesn't false-positive.
        // At each level, check assignability to keyof or recognize deferred types.
        let mut index_type_for_check = index_type_for_check;
        for _ in 0..5 {
            let next = tsz_solver::type_queries::get_type_parameter_constraint(
                self.ctx.types,
                index_type_for_check,
            );
            let Some(next_constraint) = next else { break };
            let next_evaluated = self.evaluate_type_with_env(next_constraint);
            if self.is_assignable_to(next_evaluated, keyof_object) {
                return;
            }
            // If the constraint resolved to a deferred key space for THIS object,
            // suppress TS2536. For unrelated key spaces (e.g. `F extends keyof D[T]`
            // used as `D[F]`), we must keep checking and report TS2536.
            if self.is_keyof_for_current_object(next_evaluated, object_type, object_type_for_check)
                || self.is_keyof_for_current_object(
                    next_constraint,
                    object_type,
                    object_type_for_check,
                )
                || tsz_solver::is_conditional_type(self.ctx.types, next_evaluated)
                || tsz_solver::is_generic_application(self.ctx.types, next_evaluated)
                || tsz_solver::is_generic_application(self.ctx.types, next_constraint)
            {
                return;
            }
            // Continue following if still a type parameter.
            if !tsz_solver::type_queries::is_type_parameter_like(self.ctx.types, next_evaluated) {
                index_type_for_check = next_evaluated;
                break;
            }
            index_type_for_check = next_evaluated;
        }
        if !self.is_assignable_to(index_type_for_check, keyof_object) {
            if let Some((wants_string, wants_number)) =
                self.get_index_key_kind(index_type_for_check)
                && self.is_element_indexable(object_type_for_check, wants_string, wants_number)
            {
                return;
            }
            // Numeric-literal index keys may stringify differently from our keyof
            // representation; explicitly check if all literals are valid keys.
            if tsz_solver::type_queries::numeric_literal_index_valid_for_object(
                self.ctx.types,
                index_type_for_check,
                object_type_for_check,
            ) {
                return;
            }
            if let Some(object_type_node) = self.ctx.arena.get(data.object_type)
                && let Some(nested_indexed_access) =
                    self.ctx.arena.get_indexed_access_type(object_type_node)
            {
                let mut constrained_base_type =
                    self.get_type_from_type_node(nested_indexed_access.object_type);
                constrained_base_type = tsz_solver::type_queries::get_type_parameter_constraint(
                    self.ctx.types,
                    constrained_base_type,
                )
                .unwrap_or(constrained_base_type);

                let nested_index_type =
                    self.get_type_from_type_node(nested_indexed_access.index_type);
                let constrained_object_type = if let Some(prop_atom) =
                    tsz_solver::type_queries::get_string_literal_value(
                        self.ctx.types,
                        nested_index_type,
                    ) {
                    let property_name = self.ctx.types.resolve_atom(prop_atom);
                    match self
                        .resolve_property_access_with_env(constrained_base_type, &property_name)
                    {
                        tsz_solver::operations::property::PropertyAccessResult::Success {
                            type_id,
                            ..
                        } => type_id,
                        _ => self.evaluate_type_with_env(
                            self.ctx
                                .types
                                .factory()
                                .index_access(constrained_base_type, nested_index_type),
                        ),
                    }
                } else {
                    self.evaluate_type_with_env(
                        self.ctx
                            .types
                            .factory()
                            .index_access(constrained_base_type, nested_index_type),
                    )
                };
                if constrained_object_type != TypeId::ERROR
                    && is_broad_index_type(self.ctx.types, index_type_for_check)
                    && let Some((wants_string, wants_number)) =
                        self.get_index_key_kind(index_type_for_check)
                    && self.is_element_indexable(
                        constrained_object_type,
                        wants_string,
                        wants_number,
                    )
                {
                    return;
                }
            }
            // Suppress TS2536 when the index type is deferred — i.e., it involves
            // a conditional, application, keyof, or error type that can't be fully
            // resolved at the generic level. TSC defers these checks to instantiation
            // time.
            // Example: { 0: X; 1: Y }[HasTail<T> extends true ? 0 : 1]
            // KeyOf types remain deferred when wrapping type parameters (e.g.,
            // `keyof T` where T extends object) because the constraint has no
            // useful keys. This is valid for `K extends keyof T` patterns.
            // Check BOTH the evaluated type AND the original (pre-evaluation) type,
            // because evaluation may partially resolve an Application into a
            // Conditional, or may produce ERROR.
            let is_deferred_object_type = |ty: TypeId| -> bool {
                ty == TypeId::ERROR
                    || tsz_solver::is_conditional_type(self.ctx.types, ty)
                    || tsz_solver::is_generic_application(self.ctx.types, ty)
                    || tsz_solver::type_queries::is_keyof_type(self.ctx.types, ty)
            };
            let mut is_deferred_index_type = |ty: TypeId| -> bool {
                ty == TypeId::ERROR
                    || tsz_solver::is_conditional_type(self.ctx.types, ty)
                    || tsz_solver::is_generic_application(self.ctx.types, ty)
                    || self.is_keyof_for_current_object(ty, object_type, object_type_for_check)
            };
            // Suppress TS2536 for deferred types (conditional, application, keyof,
            // error, index-access). tsc defers these checks to instantiation time.
            if is_deferred_index_type(index_type_for_check)
                || is_deferred_index_type(index_type)
                || is_deferred_object_type(object_type_for_check)
                || is_deferred_object_type(object_type)
                || (tsz_solver::is_index_access_type(self.ctx.types, object_type_for_check)
                    && is_broad_index_type(self.ctx.types, index_type_for_check))
                || tsz_solver::is_index_access_type(self.ctx.types, index_type_for_check)
            {
                return;
            }
            // Last-resort: check if the index type parameter's AST declaration has a
            // `keyof` constraint targeting the current object type. This catches cases
            // where the TypeId lost its constraint during type application lowering.
            if self.index_has_keyof_constraint_from_declaration(
                data.index_type,
                data.object_type,
                object_type,
                object_type_for_check,
            ) {
                return;
            }

            let obj_type_str = self.format_type(object_type);
            let index_type_str = self.format_type(index_type);

            let message_2536 = format_message(
                diagnostic_messages::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
                &[&index_type_str, &obj_type_str],
            );
            self.error_at_node(
                error_anchor,
                &message_2536,
                diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
            );
        }
    }
}
