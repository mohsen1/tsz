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

impl<'a> CheckerState<'a> {
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
