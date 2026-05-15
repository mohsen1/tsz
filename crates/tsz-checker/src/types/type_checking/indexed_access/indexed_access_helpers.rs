use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// Check if a property with the given name is private or protected on the given type.
/// Delegates to the solver's type query via `query_boundaries`.
pub(super) fn has_nonpublic_property(
    db: &dyn tsz_solver::TypeDatabase,
    type_id: TypeId,
    name: &str,
) -> bool {
    crate::query_boundaries::common::has_nonpublic_property(db, type_id, name)
}

pub(super) fn is_broad_index_type(db: &dyn tsz_solver::TypeDatabase, ty: TypeId) -> bool {
    if matches!(ty, TypeId::STRING | TypeId::NUMBER | TypeId::SYMBOL) {
        return true;
    }

    crate::query_boundaries::common::union_members(db, ty).is_some_and(|members| {
        !members.is_empty()
            && members
                .iter()
                .all(|&member| is_broad_index_type(db, member))
    })
}

pub(super) fn same_type_param_name(
    db: &dyn tsz_solver::TypeDatabase,
    left: TypeId,
    right: TypeId,
) -> bool {
    crate::query_boundaries::common::type_param_info(db, left)
        .zip(crate::query_boundaries::common::type_param_info(db, right))
        .is_some_and(|(l, r)| l.name == r.name)
}

pub(super) fn same_object_key_space(
    db: &dyn tsz_solver::TypeDatabase,
    left: TypeId,
    right: TypeId,
) -> bool {
    left == right || same_type_param_name(db, left, right)
}

pub(super) fn remapped_mapped_type_template_index_should_report_ts2536(
    db: &dyn tsz_solver::TypeDatabase,
    object_type_for_check: TypeId,
    index_type: TypeId,
    index_type_for_check: TypeId,
) -> bool {
    let Some(mapped_id) =
        crate::query_boundaries::common::mapped_type_id(db, object_type_for_check)
    else {
        return false;
    };
    let mapped = db.mapped_type(mapped_id);
    if mapped.name_type.is_none() {
        return false;
    }
    if !crate::query_boundaries::common::is_template_literal_type(db, index_type)
        && !crate::query_boundaries::common::is_template_literal_type(db, index_type_for_check)
    {
        return false;
    }
    crate::query_boundaries::common::contains_type_parameters(db, index_type)
        || crate::query_boundaries::common::contains_type_parameters(db, index_type_for_check)
}

pub(super) fn indexed_access_object_alias_application_exceeds_depth(
    checker: &mut CheckerState<'_>,
    object_node_idx: NodeIndex,
) -> bool {
    let Some(object_node) = checker.ctx.arena.get(object_node_idx) else {
        return false;
    };
    let type_name = checker
        .ctx
        .arena
        .get_type_ref(object_node)
        .map_or(object_node_idx, |type_ref| type_ref.type_name);
    let Some(raw_sym_id) = checker.resolve_type_symbol_for_lowering(type_name) else {
        return false;
    };
    let sym_id = tsz_binder::SymbolId(raw_sym_id);
    let Some(symbol) = checker.ctx.binder.get_symbol(sym_id) else {
        return false;
    };
    if !symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS) {
        return false;
    }
    if checker.ctx.symbol_resolution_set.contains(&sym_id) {
        return false;
    }
    let declarations = symbol.declarations.clone();

    declarations.into_iter().any(|decl_idx| {
        let Some(decl_node) = checker.ctx.arena.get(decl_idx) else {
            return false;
        };
        let Some(type_alias) = checker.ctx.arena.get_type_alias(decl_node) else {
            return false;
        };
        let body_type = checker.get_type_from_type_node(type_alias.type_node);
        let Some((base, _)) =
            crate::query_boundaries::common::application_info(checker.ctx.types, body_type)
        else {
            return false;
        };
        let Some(app_def_id) =
            crate::query_boundaries::common::lazy_def_id(checker.ctx.types, base)
        else {
            return false;
        };
        let Some(app_sym_id) = checker.ctx.def_to_symbol_id(app_def_id) else {
            return false;
        };
        if !checker.type_alias_symbol_direct_conditional_branches_are_array_like(app_sym_id) {
            return false;
        }
        checker.ctx.depth_exceeded.set(false);
        checker.evaluate_type_for_ts2589_check(body_type, app_def_id)
    })
}

impl<'a> CheckerState<'a> {
    /// TS4105: Emit "Private or protected member '{name}' cannot be accessed on
    /// a type parameter." for each type-parameter portion of `object_type` whose
    /// constraint has a non-public property with the given `name`.
    ///
    /// For union object types (e.g. `(T | B)["a"]`), each member is checked
    /// individually. Only actual `TypeParameter` nodes trigger the diagnostic —
    /// concrete class types are skipped (tsc only reports TS4105 on type params).
    pub(super) fn check_ts4105_private_on_type_parameter(
        &mut self,
        error_node: NodeIndex,
        object_type: TypeId,
        property_name: &str,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        // Collect the type parameters to inspect: either the object type itself
        // (if it's a type parameter) or the type-parameter members of a union.
        let mut type_params_to_check: smallvec::SmallVec<[TypeId; 4]> = smallvec::SmallVec::new();

        if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, object_type) {
            type_params_to_check.push(object_type);
        } else if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, object_type)
        {
            for &member in &members {
                if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, member) {
                    type_params_to_check.push(member);
                }
            }
        }

        let mut emitted = false;
        for &tp in &type_params_to_check {
            if let Some(constraint) =
                crate::query_boundaries::common::type_parameter_constraint(self.ctx.types, tp)
                && has_nonpublic_property(self.ctx.types, constraint, property_name)
                && !emitted
            {
                let message = format_message(
                        diagnostic_messages::PRIVATE_OR_PROTECTED_MEMBER_CANNOT_BE_ACCESSED_ON_A_TYPE_PARAMETER,
                        &[property_name],
                    );
                self.error_at_node(
                        error_node,
                        &message,
                        diagnostic_codes::PRIVATE_OR_PROTECTED_MEMBER_CANNOT_BE_ACCESSED_ON_A_TYPE_PARAMETER,
                    );
                emitted = true;
            }
        }
    }

    pub(super) fn is_numeric_index_on_parameters_utility(
        &self,
        object_type_node: NodeIndex,
        index_type: TypeId,
    ) -> bool {
        crate::query_boundaries::common::number_literal_value(self.ctx.types, index_type).is_some()
            && self.node_text(object_type_node).is_some_and(|text| {
                let text = text.trim();
                text.starts_with("Parameters<") || text.starts_with("ConstructorParameters<")
            })
    }

    pub(super) fn index_constraint_keyof_matches_mapped_constraint(
        &mut self,
        index_constraint: Option<TypeId>,
        mapped_constraint: TypeId,
        keyof: TypeId,
    ) -> bool {
        let Some(index_constraint) = index_constraint else {
            return false;
        };
        let index_constraint_eval = self.evaluate_type_with_env(index_constraint);
        [index_constraint, index_constraint_eval]
            .into_iter()
            .filter_map(|candidate| {
                crate::query_boundaries::state::checking::keyof_target(self.ctx.types, candidate)
            })
            .any(|index_operand| {
                crate::query_boundaries::state::checking::keyof_target(
                    self.ctx.types,
                    mapped_constraint,
                )
                .is_some_and(|constraint_operand| {
                    same_object_key_space(self.ctx.types, index_operand, constraint_operand)
                }) || crate::query_boundaries::state::checking::keyof_target(self.ctx.types, keyof)
                    .is_some_and(|keyof_operand| {
                        same_object_key_space(self.ctx.types, index_operand, keyof_operand)
                    })
            })
    }

    pub(super) fn indexed_access_literal_property_exists_in_alias_union(
        &self,
        object_node_idx: NodeIndex,
        index_node_idx: NodeIndex,
    ) -> bool {
        let Some(property_name) = self.type_index_string_literal(index_node_idx) else {
            return false;
        };
        self.alias_body_for_non_generic_type_reference_from_node(object_node_idx)
            .is_some_and(|body_idx| {
                self.alias_union_members_have_property(body_idx, &property_name)
            })
    }

    fn type_index_string_literal(&self, node_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(node_idx)?;
        if let Some(literal) = self.ctx.arena.get_literal(node) {
            return Some(literal.text.to_string());
        }
        if let Some(literal_type) = self.ctx.arena.get_literal_type(node) {
            let literal_node = self.ctx.arena.get(literal_type.literal)?;
            let literal = self.ctx.arena.get_literal(literal_node)?;
            return Some(literal.text.to_string());
        }
        None
    }

    fn alias_union_members_have_property(
        &self,
        object_node_idx: NodeIndex,
        property_name: &str,
    ) -> bool {
        let Some(object_node) = self.ctx.arena.get(object_node_idx) else {
            return false;
        };

        if object_node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            && let Some(wrapped) = self.ctx.arena.get_wrapped_type(object_node)
        {
            return self.alias_union_members_have_property(wrapped.type_node, property_name);
        }

        if object_node.kind == syntax_kind_ext::TYPE_REFERENCE {
            return self
                .alias_body_for_non_generic_type_reference(object_node)
                .is_some_and(|body_idx| {
                    self.alias_union_members_have_property(body_idx, property_name)
                });
        }

        if object_node.kind == syntax_kind_ext::UNION_TYPE {
            let Some(composite) = self.ctx.arena.get_composite_type(object_node) else {
                return false;
            };
            return !composite.types.nodes.is_empty()
                && composite.types.nodes.iter().all(|&member_idx| {
                    self.alias_union_members_have_property(member_idx, property_name)
                });
        }

        if object_node.kind == syntax_kind_ext::TYPE_LITERAL {
            return self.type_literal_has_declared_property(object_node, property_name);
        }

        false
    }

    fn alias_body_for_non_generic_type_reference(
        &self,
        object_node: &tsz_parser::parser::node::Node,
    ) -> Option<NodeIndex> {
        let type_ref = self.ctx.arena.get_type_ref(object_node)?;
        if type_ref
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty())
        {
            return None;
        }

        let raw_sym_id = self.resolve_type_symbol_for_lowering(type_ref.type_name)?;
        let sym_id = tsz_binder::SymbolId(raw_sym_id);
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS)
            || symbol.declarations.len() != 1
        {
            return None;
        }

        let decl_node = self.ctx.arena.get(symbol.declarations[0])?;
        let type_alias = self.ctx.arena.get_type_alias(decl_node)?;
        if type_alias
            .type_parameters
            .as_ref()
            .is_some_and(|params| !params.nodes.is_empty())
        {
            return None;
        }
        Some(type_alias.type_node)
    }

    fn alias_body_for_non_generic_type_reference_from_node(
        &self,
        mut node_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        loop {
            let object_node = self.ctx.arena.get(node_idx)?;
            if object_node.kind == syntax_kind_ext::PARENTHESIZED_TYPE {
                node_idx = self.ctx.arena.get_wrapped_type(object_node)?.type_node;
                continue;
            }
            return self.alias_body_for_non_generic_type_reference(object_node);
        }
    }

    fn type_literal_has_declared_property(
        &self,
        type_literal_node: &tsz_parser::parser::node::Node,
        property_name: &str,
    ) -> bool {
        let Some(type_literal) = self.ctx.arena.get_type_literal(type_literal_node) else {
            return false;
        };
        type_literal.members.nodes.iter().any(|&member_idx| {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                return false;
            };
            let Some(signature) = self.ctx.arena.get_signature(member_node) else {
                return false;
            };
            crate::types_domain::queries::core::get_literal_property_name(
                self.ctx.arena,
                signature.name,
            )
            .as_deref()
                == Some(property_name)
        })
    }

    pub(super) fn type_literal_keyof_from_node(
        &mut self,
        type_node_idx: NodeIndex,
    ) -> Option<TypeId> {
        let obj_node = self.ctx.arena.get(type_node_idx)?;
        if obj_node.kind != syntax_kind_ext::TYPE_LITERAL {
            return None;
        }
        let type_lit = self.ctx.arena.get_type_literal(obj_node)?;
        let mut key_types = Vec::new();
        for &member_idx in &type_lit.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::INDEX_SIGNATURE {
                return None;
            }
            if let Some(sig) = self.ctx.arena.get_signature(member_node)
                && let Some(name) = self.get_property_name(sig.name)
            {
                let key_type = self
                    .ctx
                    .arena
                    .get(sig.name)
                    .filter(|name_node| name_node.kind == SyntaxKind::NumericLiteral as u16)
                    .and_then(|name_node| self.ctx.arena.get_literal(name_node))
                    .and_then(|lit| {
                        lit.value
                            .or_else(|| tsz_common::numeric::parse_numeric_literal_value(&lit.text))
                    })
                    .map(|value| self.ctx.types.factory().literal_number(value))
                    .unwrap_or_else(|| {
                        let atom = self.ctx.types.intern_string(&name);
                        self.ctx.types.factory().literal_string_atom(atom)
                    });
                key_types.push(key_type);
            }
        }

        if key_types.is_empty() {
            None
        } else {
            Some(self.ctx.types.factory().union(key_types))
        }
    }

    pub(super) fn type_literal_member_values_accept_index(
        &mut self,
        type_node_idx: NodeIndex,
        index_type: TypeId,
        index_constraint: Option<TypeId>,
    ) -> bool {
        let Some(obj_node) = self.ctx.arena.get(type_node_idx) else {
            return false;
        };
        if obj_node.kind != syntax_kind_ext::TYPE_LITERAL {
            return false;
        }
        let Some(type_lit) = self.ctx.arena.get_type_literal(obj_node) else {
            return false;
        };
        let index_for_check = self.evaluate_type_with_env(index_type);
        let constraint_for_check =
            index_constraint.map(|constraint| self.evaluate_type_with_env(constraint));
        let mut saw_value = false;

        for &member_idx in &type_lit.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                continue;
            };
            if sig.type_annotation == NodeIndex::NONE {
                return false;
            }
            let Some(value_keyof) = self.type_literal_keyof_from_node(sig.type_annotation) else {
                return false;
            };
            if !self.is_assignable_to(index_for_check, value_keyof)
                && !constraint_for_check
                    .is_some_and(|constraint| self.is_assignable_to(constraint, value_keyof))
            {
                return false;
            }
            saw_value = true;
        }

        saw_value
    }

    pub(super) fn nested_type_literal_index_access_allows_index(
        &mut self,
        object_type_node_idx: NodeIndex,
        outer_index_node_idx: NodeIndex,
        outer_index_type: TypeId,
    ) -> bool {
        let Some(object_node) = self.ctx.arena.get(object_type_node_idx) else {
            return false;
        };
        let Some(nested) = self.ctx.arena.get_indexed_access_type(object_node) else {
            return false;
        };

        let nested_index_type = self.get_type_from_type_node(nested.index_type);
        let mut nested_index_constraint =
            crate::query_boundaries::common::type_parameter_constraint(
                self.ctx.types,
                nested_index_type,
            );
        if crate::query_boundaries::common::is_type_parameter_like(
            self.ctx.types,
            nested_index_type,
        ) && nested_index_constraint.is_none()
        {
            nested_index_constraint = self
                .resolve_index_constraint_from_declaration(nested.index_type, nested.object_type);
        }

        let mut outer_index_constraint = crate::query_boundaries::common::type_parameter_constraint(
            self.ctx.types,
            outer_index_type,
        );
        if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, outer_index_type)
            && outer_index_constraint.is_none()
        {
            outer_index_constraint = self.resolve_index_constraint_from_declaration(
                outer_index_node_idx,
                object_type_node_idx,
            );
        }

        let Some(nested_base_keyof) = self.type_literal_keyof_from_node(nested.object_type) else {
            return false;
        };
        let nested_index_for_check = nested_index_constraint.unwrap_or(nested_index_type);
        let nested_index_for_check = self.evaluate_type_with_env(nested_index_for_check);

        self.is_assignable_to(nested_index_for_check, nested_base_keyof)
            && self.type_literal_member_values_accept_index(
                nested.object_type,
                outer_index_type,
                outer_index_constraint,
            )
    }

    fn array_like_kind_has_length(
        &self,
        kind: crate::query_boundaries::type_checking_utilities::ArrayLikeKind,
    ) -> bool {
        match kind {
            crate::query_boundaries::type_checking_utilities::ArrayLikeKind::Array(_)
            | crate::query_boundaries::type_checking_utilities::ArrayLikeKind::Tuple => true,
            crate::query_boundaries::type_checking_utilities::ArrayLikeKind::Readonly(inner) => {
                self.indexed_access_type_has_array_like_length(inner)
            }
            crate::query_boundaries::type_checking_utilities::ArrayLikeKind::Union(members) => {
                !members.is_empty()
                    && members
                        .iter()
                        .all(|&member| self.indexed_access_type_has_array_like_length(member))
            }
            crate::query_boundaries::type_checking_utilities::ArrayLikeKind::Intersection(
                members,
            ) => members
                .iter()
                .any(|&member| self.indexed_access_type_has_array_like_length(member)),
            crate::query_boundaries::type_checking_utilities::ArrayLikeKind::Other => false,
        }
    }

    fn indexed_access_type_has_array_like_length(&self, type_id: TypeId) -> bool {
        let kind = crate::query_boundaries::type_checking_utilities::classify_array_like(
            self.ctx.types,
            type_id,
        );
        self.array_like_kind_has_length(kind)
    }

    pub(super) fn indexed_access_object_allows_length_property(
        &mut self,
        object_type: TypeId,
        object_type_for_check: TypeId,
    ) -> bool {
        let candidates = [
            object_type,
            object_type_for_check,
            self.evaluate_type_with_env(object_type),
            self.evaluate_type_with_env(object_type_for_check),
        ];

        candidates.iter().copied().any(|candidate| {
            self.indexed_access_type_has_array_like_length(candidate)
                || crate::query_boundaries::common::type_parameter_constraint(
                    self.ctx.types,
                    candidate,
                )
                .is_some_and(|constraint| {
                    self.indexed_access_type_has_array_like_length(constraint)
                })
        })
    }

    pub(super) fn union_restricted_literal_property_is_missing(
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

    pub(super) fn error_at_index_type_span(
        &mut self,
        error_anchor: NodeIndex,
        message: &str,
        code: u32,
    ) {
        let Some(anchor_node) = self.ctx.arena.get(error_anchor) else {
            self.error_at_node(error_anchor, message, code);
            return;
        };
        let Some(source_file) = self.ctx.arena.source_files.first() else {
            self.error_at_node(error_anchor, message, code);
            return;
        };
        let source = source_file.text.as_ref();
        let start = anchor_node.pos as usize;
        let end = anchor_node.end as usize;
        let Some(text) = source.get(start..end) else {
            self.error_at_node(error_anchor, message, code);
            return;
        };
        let Some(open_bracket) = text.rfind('[') else {
            if let Some(index_text) = text.trim().strip_suffix(']').map(str::trim_end)
                && !index_text.is_empty()
            {
                let leading_ws = text.len() - text.trim_start().len();
                self.ctx.error(
                    (start + leading_ws) as u32,
                    index_text.len() as u32,
                    message.to_string(),
                    code,
                );
                return;
            }
            self.error_at_node(error_anchor, message, code);
            return;
        };
        let close_bracket = text.rfind(']').unwrap_or(text.len());
        if close_bracket <= open_bracket + 1 {
            self.error_at_node(error_anchor, message, code);
            return;
        }

        let inner = &text[open_bracket + 1..close_bracket];
        let leading_ws = inner.len() - inner.trim_start().len();
        let trailing_ws = inner.len() - inner.trim_end().len();
        let pos = start + open_bracket + 1 + leading_ws;
        let len = inner.len().saturating_sub(leading_ws + trailing_ws).max(1);
        self.ctx
            .error(pos as u32, len as u32, message.to_string(), code);
    }

    pub(super) fn canonical_numeric_string_literal_valid_for_object(
        &self,
        index_type: TypeId,
        object_type: TypeId,
    ) -> bool {
        let Some(prop_atom) =
            crate::query_boundaries::common::string_literal_value(self.ctx.types, index_type)
        else {
            return false;
        };
        let property_name = self.ctx.types.resolve_atom(prop_atom);
        self.get_numeric_index_from_string(&property_name)
            .is_some_and(|_| self.is_element_indexable(object_type, false, true))
    }

    pub(super) fn union_index_members_valid_for_object(
        &mut self,
        index_type: TypeId,
        object_type: TypeId,
        keyof_object: TypeId,
    ) -> bool {
        let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, index_type)
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
                || crate::query_boundaries::common::numeric_literal_index_valid_for_object(
                    self.ctx.types,
                    member,
                    object_type,
                )
                || self.canonical_numeric_string_literal_valid_for_object(member, object_type)
        })
    }

    pub(super) fn indexed_access_constraint_values_allow_index(
        &mut self,
        base_type: TypeId,
        index_type: TypeId,
    ) -> bool {
        if let Some(mapped_id) =
            crate::query_boundaries::common::mapped_type_id(self.ctx.types, base_type)
        {
            let mapped = self.ctx.types.mapped_type(mapped_id);
            let template_keyof = self.ctx.types.evaluate_keyof(mapped.template);
            return self.is_assignable_to(index_type, template_keyof);
        }

        let Some(constraint) =
            crate::query_boundaries::common::type_parameter_constraint(self.ctx.types, base_type)
        else {
            return false;
        };
        let constraint = self.evaluate_type_with_env(constraint);
        if matches!(constraint, TypeId::ERROR | TypeId::ANY) {
            return false;
        }

        let key_space = self.ctx.types.evaluate_keyof(constraint);
        let values = self
            .evaluate_type_with_env(self.ctx.types.factory().index_access(constraint, key_space));
        if matches!(values, TypeId::ERROR | TypeId::UNDEFINED) {
            return false;
        }
        self.is_assignable_to(index_type, self.ctx.types.evaluate_keyof(values))
    }

    pub(super) fn mapped_object_index_matches_own_key_constraint(
        &mut self,
        object_node_idx: NodeIndex,
        index_type: TypeId,
        index_type_for_check: TypeId,
    ) -> bool {
        let Some(object_node) = self.ctx.arena.get(object_node_idx) else {
            return false;
        };
        let Some(mapped) = self.ctx.arena.get_mapped_type(object_node) else {
            return false;
        };
        if mapped.name_type != NodeIndex::NONE {
            return false;
        }
        let Some(tp_node) = self.ctx.arena.get(mapped.type_parameter) else {
            return false;
        };
        let Some(tp) = self.ctx.arena.get_type_parameter(tp_node) else {
            return false;
        };
        if tp.constraint == NodeIndex::NONE {
            return false;
        }

        let constraint_type = self.get_type_from_type_node(tp.constraint);
        let constraint_eval = self.evaluate_type_with_env(constraint_type);

        index_type == constraint_type
            || index_type_for_check == constraint_eval
            || (self.is_assignable_to(index_type_for_check, constraint_eval)
                && self.is_assignable_to(constraint_eval, index_type_for_check))
    }

    pub(super) fn simple_type_reference_name(&self, node_idx: NodeIndex) -> Option<String> {
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

    pub(super) fn type_node_refers_to_type_parameter(&self, node_idx: NodeIndex) -> bool {
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
                    .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::TYPE_PARAMETER))
            })
    }
}
