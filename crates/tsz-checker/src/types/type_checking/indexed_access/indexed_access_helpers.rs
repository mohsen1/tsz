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
