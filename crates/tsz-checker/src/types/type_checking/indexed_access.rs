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
    fn canonical_numeric_string_literal_valid_for_object(
        &self,
        index_type: TypeId,
        object_type: TypeId,
    ) -> bool {
        let Some(prop_atom) =
            tsz_solver::type_queries::get_string_literal_value(self.ctx.types, index_type)
        else {
            return false;
        };
        let property_name = self.ctx.types.resolve_atom(prop_atom);
        self.get_numeric_index_from_string(&property_name)
            .is_some_and(|_| self.is_element_indexable(object_type, false, true))
    }

    fn union_index_members_valid_for_object(
        &mut self,
        index_type: TypeId,
        object_type: TypeId,
        keyof_object: TypeId,
    ) -> bool {
        let Some(members) = tsz_solver::type_queries::get_union_members(self.ctx.types, index_type)
        else {
            return false;
        };

        members.iter().all(|&member| {
            self.is_assignable_to(member, keyof_object)
                || self
                    .get_index_key_kind(member)
                    .is_some_and(|(wants_string, wants_number)| {
                        self.is_element_indexable(object_type, wants_string, wants_number)
                    })
                || tsz_solver::type_queries::numeric_literal_index_valid_for_object(
                    self.ctx.types,
                    member,
                    object_type,
                )
                || self.canonical_numeric_string_literal_valid_for_object(member, object_type)
        })
    }

    fn union_restricted_literal_property_is_missing(
        &mut self,
        property_name: &str,
        object_type: TypeId,
    ) -> bool {
        use crate::query_boundaries::state::checking;

        if self.ctx.enclosing_class.is_some() {
            return false;
        }

        let Some(members) = checking::union_members(self.ctx.types, object_type) else {
            return false;
        };
        if members.len() < 2 {
            return false;
        }

        let is_static = self.is_constructor_type(object_type);
        let mut has_restricted = false;
        let mut has_other = false;
        let mut first_declaring_class: Option<NodeIndex> = None;

        for member in members {
            let member = self.resolve_type_for_property_access(member);
            let Some(class_idx) = self.get_class_decl_from_type(member) else {
                has_other = true;
                continue;
            };

            match self.find_member_access_info(class_idx, property_name, is_static) {
                Some(access_info) => {
                    has_restricted = true;
                    if let Some(first_decl) = first_declaring_class {
                        if first_decl != access_info.declaring_class_idx {
                            has_other = true;
                        }
                    } else {
                        first_declaring_class = Some(access_info.declaring_class_idx);
                    }
                }
                None => has_other = true,
            }
        }

        has_restricted && has_other
    }

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

    fn type_node_refers_to_type_parameter(&self, node_idx: NodeIndex) -> bool {
        use tsz_binder::symbol_flags;

        let Some(name) = self.simple_type_reference_name(node_idx) else {
            return false;
        };
        self.ctx
            .binder
            .get_symbols()
            .find_all_by_name(&name)
            .iter()
            .any(|&sym_id| {
                self.ctx
                    .binder
                    .get_symbol(sym_id)
                    .is_some_and(|symbol| (symbol.flags & symbol_flags::TYPE_PARAMETER) != 0)
            })
    }

    fn is_mapped_key_index_for_current_object(
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

        let mut current = self.ctx.arena.get_extended(node_idx).map(|ext| ext.parent);
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

    /// Check if the keyof target in a mapped type constraint matches the object being indexed.
    /// Handles: direct name match, indexed access type objects, and cross-type extends.
    fn mapped_keyof_target_matches_object(
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
            let object_constraint = tsz_solver::type_queries::get_type_parameter_constraint(
                self.ctx.types,
                object_type,
            )
            .or_else(|| {
                tsz_solver::type_queries::get_type_parameter_constraint(
                    self.ctx.types,
                    object_type_for_check,
                )
            });
            if let Some(constraint) = object_constraint {
                // Check if the constraint's type parameter name matches the keyof target
                if let Some(info) =
                    tsz_solver::type_queries::get_type_parameter_info(self.ctx.types, constraint)
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

    /// Check if an object type is a deferred indexed access that can't be resolved.
    /// Only suppresses TS2536 when the base of the indexed access is a type parameter
    /// (e.g., `Shape[k]` where Shape is a generic param), NOT when it's a concrete type
    /// (e.g., `DataFetchFns[T]` where `DataFetchFns` is a known type).
    fn is_deferred_indexed_access_object(&self, ty: TypeId) -> bool {
        if !tsz_solver::is_index_access_type(self.ctx.types, ty) {
            return false;
        }
        // Decompose the indexed access and check if the base is a type parameter
        if let Some((base, _index)) =
            tsz_solver::type_queries::get_index_access_types(self.ctx.types, ty)
        {
            return tsz_solver::type_queries::is_type_parameter_like(self.ctx.types, base);
        }
        false
    }

    /// Check if two AST nodes have the same text representation.
    fn nodes_have_same_text(&self, a: NodeIndex, b: NodeIndex) -> bool {
        let a_node = self.ctx.arena.get(a);
        let b_node = self.ctx.arena.get(b);
        match (a_node, b_node) {
            (Some(an), Some(bn)) if an.kind == bn.kind => {
                // Identifiers
                if let (Some(ai), Some(bi)) = (
                    self.ctx.arena.get_identifier(an),
                    self.ctx.arena.get_identifier(bn),
                ) {
                    return ai.escaped_text == bi.escaped_text;
                }
                // Literal types (e.g., LiteralType wrapping a string literal)
                if let (Some(alt), Some(blt)) = (
                    self.ctx.arena.get_literal_type(an),
                    self.ctx.arena.get_literal_type(bn),
                ) {
                    return self.nodes_have_same_text(alt.literal, blt.literal);
                }
                // String/number literals directly
                if let (Some(al), Some(bl)) = (
                    self.ctx.arena.get_literal(an),
                    self.ctx.arena.get_literal(bn),
                ) {
                    return al.text == bl.text;
                }
                false
            }
            _ => false,
        }
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
            // Mapped type key parameter: `[K in C]: ...` — extract constraint C
            if parent_node.kind == syntax_kind_ext::MAPPED_TYPE
                && let Some(mapped) = self.ctx.arena.get_mapped_type(parent_node)
                && let Some(tp_node) = self.ctx.arena.get(mapped.type_parameter)
                && let Some(tp) = self.ctx.arena.get_type_parameter(tp_node)
                && let Some(name_node) = self.ctx.arena.get(tp.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                && ident.escaped_text == index_name
                && tp.constraint != NodeIndex::NONE
            {
                let constraint_type = self.get_type_from_type_node(tp.constraint);
                if constraint_type != TypeId::ERROR {
                    return Some(constraint_type);
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

    /// Check if the indexed access `T[K]` is inside the true branch of a conditional type
    /// `K extends keyof T ? ... : ...`. In the true branch, `K` is narrowed to `keyof T`,
    /// so the index is valid.
    fn is_in_conditional_keyof_narrowing_context(
        &mut self,
        node_idx: NodeIndex,
        object_type: TypeId,
        object_type_for_check: TypeId,
        _index_type: TypeId,
    ) -> bool {
        let index_name = self.simple_type_reference_name(
            self.ctx
                .arena
                .get(node_idx)
                .and_then(|n| self.ctx.arena.get_indexed_access_type(n))
                .map(|iat| iat.index_type)
                .unwrap_or(NodeIndex::NONE),
        );

        let mut current = self.ctx.arena.get_extended(node_idx).map(|ext| ext.parent);
        while let Some(parent_idx) = current {
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };
            if parent_node.kind == syntax_kind_ext::CONDITIONAL_TYPE
                && let Some(cond) = self.ctx.arena.get_conditional_type(parent_node)
            {
                // Check if the indexed access is in the true branch
                // (the node_idx must be a descendant of cond.true_type)
                let in_true_branch = self.is_descendant_of(node_idx, cond.true_type);
                if in_true_branch {
                    // Check if the check type matches the index type
                    let check_name = self.simple_type_reference_name(cond.check_type);
                    if check_name.is_some() && check_name == index_name {
                        // Check if the extends type is `keyof T` for our object
                        let extends_type = self.get_type_from_type_node(cond.extends_type);
                        if self.is_keyof_for_current_object(
                            extends_type,
                            object_type,
                            object_type_for_check,
                        ) {
                            return true;
                        }
                        // Also check if extends type is keyof applied to the object
                        if let Some(extends_node) = self.ctx.arena.get(cond.extends_type)
                            && let Some(type_op) = self.ctx.arena.get_type_operator(extends_node)
                            && type_op.operator == SyntaxKind::KeyOfKeyword as u16
                        {
                            let keyof_target_type = self.get_type_from_type_node(type_op.type_node);
                            if same_object_key_space(self.ctx.types, keyof_target_type, object_type)
                                || same_object_key_space(
                                    self.ctx.types,
                                    keyof_target_type,
                                    object_type_for_check,
                                )
                            {
                                return true;
                            }
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
        false
    }

    /// Check if `node_a` is a descendant of `node_b` in the AST.
    fn is_descendant_of(&self, node_a: NodeIndex, node_b: NodeIndex) -> bool {
        let mut current = Some(node_a);
        while let Some(idx) = current {
            if idx == node_b {
                return true;
            }
            current = self.ctx.arena.get_extended(idx).map(|ext| ext.parent);
        }
        false
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
        let error_anchor = node_idx;
        let concrete_error_anchor = data.index_type;
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
        if index_type == TypeId::ANY {
            let supports_string_index =
                self.is_element_indexable(object_type_for_check, true, false);
            let supports_number_index =
                self.is_element_indexable(object_type_for_check, false, true);
            if !supports_string_index && !supports_number_index {
                let message_2538 = format_message(
                    diagnostic_messages::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                    &["any"],
                );
                self.error_at_node(
                    error_anchor,
                    &message_2538,
                    diagnostic_codes::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                );
                return;
            }
            return;
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
        // When the solver TypeData doesn't carry the constraint (common for type
        // parameters in generic signatures), use the AST-resolved constraint.
        // E.g. `emit<Event extends keyof M>(...args: M[Event])` — Event's
        // constraint `keyof M` is found from the AST but not in the TypeId.
        if let Some(constraint) = index_constraint {
            let constraint_eval = self.evaluate_type_with_env(constraint);
            if self.is_assignable_to(constraint_eval, keyof_object) {
                return;
            }
        }
        if self.is_mapped_key_index_for_current_object(
            node_idx,
            data.object_type,
            data.index_type,
            object_type,
            object_type_for_check,
        ) {
            return;
        }
        // When the constraint was resolved from AST, also check if it represents
        // a keyof for the current object type (catches deferred keyof patterns that
        // aren't directly assignable to the computed keyof).
        if let Some(constraint) = index_constraint {
            let evaluated_constraint = self.evaluate_type_with_env(constraint);
            if self.is_keyof_for_current_object(
                evaluated_constraint,
                object_type,
                object_type_for_check,
            ) || self.is_keyof_for_current_object(constraint, object_type, object_type_for_check)
            {
                return;
            }
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
            if self.canonical_numeric_string_literal_valid_for_object(
                index_type_for_check,
                object_type_for_check,
            ) {
                return;
            }
            if self.union_index_members_valid_for_object(
                index_type_for_check,
                object_type_for_check,
                keyof_object,
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
                    // When the nested index is a type parameter (e.g., k in a mapped
                    // type), the solver can't resolve `constraint[k]` directly.
                    // First try index signature lookup, then fall back to evaluation.
                    let evaluated_base = self.evaluate_type_with_env(constrained_base_type);
                    let index_info = self.ctx.types.get_index_signatures(evaluated_base);
                    if let Some(ref sig) = index_info.string_index {
                        sig.value_type
                    } else {
                        self.evaluate_type_with_env(
                            self.ctx
                                .types
                                .factory()
                                .index_access(constrained_base_type, nested_index_type),
                        )
                    }
                };
                // When the constrained object is still a deferred indexed access,
                // try evaluating it further. If it resolves to a concrete type,
                // use that for validation. Otherwise, check if the evaluated type
                // has index signatures or properties that validate the index.
                let constrained_object_type =
                    if tsz_solver::is_index_access_type(self.ctx.types, constrained_object_type) {
                        let evaluated =
                            self.evaluate_type_for_assignability(constrained_object_type);
                        if evaluated != TypeId::ERROR
                            && !tsz_solver::is_index_access_type(self.ctx.types, evaluated)
                        {
                            evaluated
                        } else {
                            constrained_object_type
                        }
                    } else {
                        constrained_object_type
                    };
                if constrained_object_type != TypeId::ERROR
                    // When the constrained object is still a deferred indexed access
                    // (e.g., T[keyof T] where T is unconstrained), or resolves to
                    // `any` (recursive/circular constraints), property lookups may
                    // spuriously succeed. Skip this block so the error is caught
                    // by the deferred-suppression or final error path below.
                    && constrained_object_type != TypeId::ANY
                    && !tsz_solver::is_index_access_type(
                        self.ctx.types,
                        constrained_object_type,
                    )
                {
                    // Check broad index types (string/number/symbol)
                    if is_broad_index_type(self.ctx.types, index_type_for_check)
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
                    // Check string literal indices via property access on the
                    // resolved constraint type. This handles generic class instances
                    // (e.g., ZodType<any>) where evaluate_keyof doesn't enumerate
                    // class members.
                    if let Some(prop_atom) = tsz_solver::type_queries::get_string_literal_value(
                        self.ctx.types,
                        index_type_for_check,
                    ) {
                        let property_name = self.ctx.types.resolve_atom(prop_atom);
                        let prop_result = self.resolve_property_access_with_env(
                            constrained_object_type,
                            &property_name,
                        );
                        if matches!(
                            prop_result,
                            tsz_solver::operations::property::PropertyAccessResult::Success { .. }
                        ) {
                            return;
                        }
                    }
                    // Fall back to keyof check for non-literal indices.
                    let constrained_keyof = self.ctx.types.evaluate_keyof(constrained_object_type);
                    if self.is_assignable_to(index_type_for_check, constrained_keyof) {
                        return;
                    }
                }
            }
            // When the index is a concrete string literal (not a type parameter or
            // deferred type), do NOT suppress TS2536 just because the object type
            // is a deferred indexed access — tsc still emits TS2536 for patterns
            // like `T[keyof T]["foo"]` where the literal can't be validated as a
            // key of the unresolved indexed access result.
            let index_is_concrete_literal = tsz_solver::type_queries::get_string_literal_value(
                self.ctx.types,
                index_type_for_check,
            )
            .is_some();
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
                || (is_deferred_object_type(object_type_for_check) && !index_is_concrete_literal)
                || (is_deferred_object_type(object_type) && !index_is_concrete_literal)
                || (self.is_deferred_indexed_access_object(object_type_for_check)
                    && !index_is_concrete_literal)
                // Only fall back to checking the pre-resolution object_type when the
                // resolved type is also still an indexed access. If constraint resolution
                // produced a concrete type (e.g., T['value'] → number), trust it.
                || (tsz_solver::is_index_access_type(self.ctx.types, object_type_for_check)
                    && self.is_deferred_indexed_access_object(object_type)
                    && !index_is_concrete_literal)
                || tsz_solver::is_index_access_type(self.ctx.types, index_type_for_check)
                || tsz_solver::is_index_access_type(self.ctx.types, index_type)
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
            // Check if we're inside a conditional type's true branch where the condition
            // narrows the index to `keyof T`. E.g., `key extends keyof T ? T[key] : never`.
            if self.is_in_conditional_keyof_narrowing_context(
                node_idx,
                object_type,
                object_type_for_check,
                index_type,
            ) {
                return;
            }

            if let Some(prop_atom) = tsz_solver::type_queries::get_string_literal_value(
                self.ctx.types,
                index_type_for_check,
            ) {
                let property_name = self.ctx.types.resolve_atom(prop_atom);
                if self.union_restricted_literal_property_is_missing(
                    &property_name,
                    object_type_for_check,
                ) {
                    let object_type_str = self.format_type(object_type);
                    let message = format_message(
                        diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                        &[property_name.as_str(), &object_type_str],
                    );
                    self.error_at_node(
                        concrete_error_anchor,
                        &message,
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                    );
                    return;
                }
                // Don't trust property access results on deferred types (indexed
                // access, conditional, generic application) — the solver may
                // spuriously report success on types it can't fully resolve.
                if !tsz_solver::is_index_access_type(self.ctx.types, object_type_for_check)
                    && !tsz_solver::is_conditional_type(self.ctx.types, object_type_for_check)
                    && !tsz_solver::is_generic_application(self.ctx.types, object_type_for_check)
                    && matches!(
                        self.resolve_property_access_with_env(
                            object_type_for_check,
                            &property_name
                        ),
                        tsz_solver::operations::property::PropertyAccessResult::Success { .. }
                    )
                {
                    return;
                }
                // For conditional types like Extract<X, Y> (i.e., X extends Y ? X : never),
                // check if the property exists on the check_type or extends_type.
                // When false_type is `never`, the result is always a subtype of check_type
                // (which extends extends_type), so if either has the property it's valid.
                // This handles patterns like Extract<TDef[I], FieldDefinition>["type"]
                // where FieldDefinition has a "type" property.
                if let Some(cond_id) = tsz_solver::type_queries::get_conditional_type_id(
                    self.ctx.types,
                    object_type_for_check,
                ) {
                    let cond = self.ctx.types.conditional_type(cond_id);
                    if cond.false_type == TypeId::NEVER {
                        // Check extends_type first (common for Extract/Filter patterns)
                        let extends_eval = self.evaluate_type_with_env(cond.extends_type);
                        if !tsz_solver::is_conditional_type(self.ctx.types, extends_eval)
                            && !tsz_solver::is_generic_application(self.ctx.types, extends_eval)
                            && matches!(
                                self.resolve_property_access_with_env(extends_eval, &property_name),
                                tsz_solver::operations::property::PropertyAccessResult::Success { .. }
                            )
                        {
                            return;
                        }
                        // Check check_type (handles cases where check_type's constraint
                        // has the property but extends_type doesn't)
                        let check_eval = self.evaluate_type_with_env(cond.check_type);
                        if !tsz_solver::is_conditional_type(self.ctx.types, check_eval)
                            && !tsz_solver::is_generic_application(self.ctx.types, check_eval)
                            && !tsz_solver::is_index_access_type(self.ctx.types, check_eval)
                            && matches!(
                                self.resolve_property_access_with_env(check_eval, &property_name),
                                tsz_solver::operations::property::PropertyAccessResult::Success { .. }
                            )
                        {
                            return;
                        }
                        // Also check the constraint of the check_type (for generic
                        // patterns like TDef[number] where TDef: readonly FieldDefinition[])
                        let check_constraint =
                            tsz_solver::type_queries::get_type_parameter_constraint(
                                self.ctx.types,
                                check_eval,
                            );
                        if let Some(constraint) = check_constraint {
                            let constraint_eval = self.evaluate_type_with_env(constraint);
                            if !tsz_solver::is_conditional_type(self.ctx.types, constraint_eval)
                                && !tsz_solver::is_generic_application(
                                    self.ctx.types,
                                    constraint_eval,
                                )
                                && matches!(
                                    self.resolve_property_access_with_env(
                                        constraint_eval,
                                        &property_name
                                    ),
                                    tsz_solver::operations::property::PropertyAccessResult::Success {
                                        ..
                                    }
                                )
                            {
                                return;
                            }
                        }
                    }
                }
            }

            if self.try_emit_concrete_index_access_error(
                concrete_error_anchor,
                object_type_for_check,
                index_type_for_check,
                self.type_node_refers_to_type_parameter(data.object_type),
            ) {
                return;
            }

            let obj_type_str = self.format_type(object_type);
            let evaluated_index_type = self.evaluate_type_for_assignability(index_type);
            let index_type_str = if evaluated_index_type != TypeId::ERROR
                && !tsz_solver::type_queries::contains_type_parameters_db(
                    self.ctx.types,
                    index_type,
                ) {
                self.format_type(evaluated_index_type)
            } else {
                let raw = self.format_type(index_type);
                let evaluated = self.format_type(evaluated_index_type);
                if raw != evaluated && raw.starts_with("keyof ") && evaluated.contains("keyof ") {
                    evaluated
                } else {
                    raw
                }
            };

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

    fn try_emit_concrete_index_access_error(
        &mut self,
        error_anchor: NodeIndex,
        object_type: TypeId,
        index_type: TypeId,
        object_is_type_parameter_ref: bool,
    ) -> bool {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        if object_type == TypeId::ERROR || index_type == TypeId::ERROR {
            return false;
        }

        let concrete_object_type =
            if tsz_solver::is_generic_application(self.ctx.types, object_type) {
                let evaluated = self.evaluate_type_with_env(object_type);
                if evaluated != TypeId::ERROR
                    && !tsz_solver::type_queries::contains_type_parameters_db(
                        self.ctx.types,
                        evaluated,
                    )
                {
                    evaluated
                } else {
                    object_type
                }
            } else {
                object_type
            };
        let object_shape =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, concrete_object_type);
        let object_has_shape = object_shape.is_some();
        let object_has_named_shape = object_shape.and_then(|shape| shape.symbol).is_some();
        let object_is_array_like = tsz_solver::is_array_type(self.ctx.types, concrete_object_type)
            || tsz_solver::type_queries::get_tuple_elements(self.ctx.types, concrete_object_type)
                .is_some();

        if tsz_solver::type_queries::contains_type_parameters_db(
            self.ctx.types,
            concrete_object_type,
        ) || tsz_solver::type_queries::is_type_parameter_like(
            self.ctx.types,
            concrete_object_type,
        ) || tsz_solver::is_index_access_type(self.ctx.types, concrete_object_type)
            || tsz_solver::is_conditional_type(self.ctx.types, concrete_object_type)
            || (tsz_solver::is_primitive_type(self.ctx.types, concrete_object_type)
                && !tsz_solver::type_queries::is_object_like_type(
                    self.ctx.types,
                    concrete_object_type,
                ))
        {
            return false;
        }

        if let Some(members) =
            tsz_solver::type_queries::get_union_members(self.ctx.types, index_type)
        {
            if members.len() == 2
                && members.contains(&TypeId::STRING)
                && members.contains(&TypeId::NUMBER)
                && !self.is_element_indexable(concrete_object_type, false, true)
            {
                let object_type_str = self.format_type(object_type);
                let message = format_message(
                    diagnostic_messages::TYPE_HAS_NO_MATCHING_INDEX_SIGNATURE_FOR_TYPE,
                    &[&object_type_str, "number"],
                );
                self.error_at_node(
                    error_anchor,
                    &message,
                    diagnostic_codes::TYPE_HAS_NO_MATCHING_INDEX_SIGNATURE_FOR_TYPE,
                );
                return true;
            }
            let mut emitted_any = false;
            for &member in members.iter() {
                if member == TypeId::BOOLEAN {
                    for boolean_member in ["false", "true"] {
                        let message = format_message(
                            diagnostic_messages::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                            &[boolean_member],
                        );
                        self.error_at_node(
                            error_anchor,
                            &message,
                            diagnostic_codes::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                        );
                    }
                    emitted_any = true;
                    continue;
                }

                emitted_any |= self.try_emit_concrete_index_access_error(
                    error_anchor,
                    concrete_object_type,
                    member,
                    object_is_type_parameter_ref,
                );
            }
            return emitted_any;
        }

        if index_type == TypeId::ANY {
            if self.is_element_indexable(concrete_object_type, true, false)
                || self.is_element_indexable(concrete_object_type, false, true)
            {
                return false;
            }
            let message = format_message(
                diagnostic_messages::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                &["any"],
            );
            self.error_at_node(
                error_anchor,
                &message,
                diagnostic_codes::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
            );
            return true;
        }

        if let Some(invalid_member) =
            tsz_solver::type_queries::get_invalid_index_type_member(self.ctx.types, index_type)
        {
            let index_type_str = self.format_type(invalid_member);
            let message = format_message(
                diagnostic_messages::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                &[&index_type_str],
            );
            self.error_at_node(
                error_anchor,
                &message,
                diagnostic_codes::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
            );
            return true;
        }

        if let Some(prop_atom) =
            tsz_solver::type_queries::get_string_literal_value(self.ctx.types, index_type)
        {
            let property_name = self.ctx.types.resolve_atom(prop_atom);
            if self
                .union_restricted_literal_property_is_missing(&property_name, concrete_object_type)
            {
                let object_type_str = self.format_type(object_type);
                let message = format_message(
                    diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                    &[property_name.as_str(), &object_type_str],
                );
                self.error_at_node(
                    error_anchor,
                    &message,
                    diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                );
                return true;
            }
            if self.get_numeric_index_from_string(&property_name).is_some()
                && self.is_element_indexable(concrete_object_type, false, true)
            {
                return false;
            }
            if !matches!(
                self.resolve_property_access_with_env(concrete_object_type, &property_name),
                tsz_solver::operations::property::PropertyAccessResult::Success { .. }
            ) && self.get_index_key_kind(index_type) == Some((true, false))
                && !self.is_element_indexable(concrete_object_type, true, false)
                && !object_is_type_parameter_ref
                && (object_has_named_shape || object_is_array_like)
            {
                let object_type_str = self.format_type(object_type);
                let message = format_message(
                    diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                    &[property_name.as_str(), &object_type_str],
                );
                self.error_at_node(
                    error_anchor,
                    &message,
                    diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                );
                return true;
            }
        }

        if let Some((wants_string, wants_number)) = self.get_index_key_kind(index_type)
            && !self.is_element_indexable(concrete_object_type, wants_string, wants_number)
        {
            let is_literal_index =
                tsz_solver::type_queries::get_string_literal_value(self.ctx.types, index_type)
                    .is_some()
                    || tsz_solver::type_queries::get_number_literal_value(
                        self.ctx.types,
                        index_type,
                    )
                    .is_some();
            if is_literal_index {
                return false;
            }
            if !object_has_shape && !object_is_array_like {
                return false;
            }
            let object_type_str = self.format_type(object_type);
            if wants_string {
                let message = format_message(
                    diagnostic_messages::TYPE_HAS_NO_MATCHING_INDEX_SIGNATURE_FOR_TYPE,
                    &[&object_type_str, "string"],
                );
                self.error_at_node(
                    error_anchor,
                    &message,
                    diagnostic_codes::TYPE_HAS_NO_MATCHING_INDEX_SIGNATURE_FOR_TYPE,
                );
            }
            if wants_number {
                let message = format_message(
                    diagnostic_messages::TYPE_HAS_NO_MATCHING_INDEX_SIGNATURE_FOR_TYPE,
                    &[&object_type_str, "number"],
                );
                self.error_at_node(
                    error_anchor,
                    &message,
                    diagnostic_codes::TYPE_HAS_NO_MATCHING_INDEX_SIGNATURE_FOR_TYPE,
                );
            }
            return wants_string || wants_number;
        }

        false
    }
}
