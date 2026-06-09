//! Class index-signature compatibility checking for `extends` clauses
//! (`TS2415`): a derived class index signature must stay assignable to the base
//! class index signature it overrides.

use crate::query_boundaries::common::TypeSubstitution;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn check_class_index_signature_compatibility(
        &mut self,
        derived_class: &tsz_parser::parser::node::ClassData,
        base_class: &tsz_parser::parser::node::ClassData,
        derived_class_name: &str,
        base_class_name: &str,
        substitution: &TypeSubstitution,
        mut class_extends_error_reported: bool,
    ) {
        use crate::query_boundaries::common::instantiate_type;
        use tsz_parser::parser::syntax_kind_ext::INDEX_SIGNATURE;

        // Collect derived class index signatures
        let mut derived_string_index: Option<(TypeId, NodeIndex)> = None;
        let mut derived_number_index: Option<(TypeId, NodeIndex)> = None;

        for &member_idx in &derived_class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != INDEX_SIGNATURE {
                continue;
            }
            let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) else {
                continue;
            };
            if self.has_static_modifier(&index_sig.modifiers) {
                continue;
            }

            let param_idx = index_sig
                .parameters
                .nodes
                .first()
                .copied()
                .unwrap_or(NodeIndex::NONE);
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            let key_type = if param.type_annotation.is_none() {
                TypeId::ANY
            } else {
                self.get_type_from_type_node(param.type_annotation)
            };

            let value_type = if index_sig.type_annotation.is_none() {
                TypeId::ANY
            } else {
                self.get_type_from_type_node(index_sig.type_annotation)
            };

            if key_type == TypeId::NUMBER {
                derived_number_index = Some((value_type, member_idx));
            } else {
                derived_string_index = Some((value_type, member_idx));
            }
        }

        // Collect base class index signatures
        let mut base_string_index: Option<TypeId> = None;
        let mut base_number_index: Option<TypeId> = None;

        for &member_idx in &base_class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != INDEX_SIGNATURE {
                continue;
            }
            let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) else {
                continue;
            };
            if self.has_static_modifier(&index_sig.modifiers) {
                continue;
            }

            let param_idx = index_sig
                .parameters
                .nodes
                .first()
                .copied()
                .unwrap_or(NodeIndex::NONE);
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            let key_type = if param.type_annotation.is_none() {
                TypeId::ANY
            } else {
                self.get_type_from_type_node(param.type_annotation)
            };

            let value_type = if index_sig.type_annotation.is_none() {
                TypeId::ANY
            } else {
                self.get_type_from_type_node(index_sig.type_annotation)
            };

            if key_type == TypeId::NUMBER {
                base_number_index = Some(value_type);
            } else {
                base_string_index = Some(value_type);
            }
        }

        // Check string index signature compatibility
        if let (Some((derived_type, _derived_idx)), Some(base_type)) =
            (derived_string_index, base_string_index)
        {
            let base_type_instantiated = instantiate_type(self.ctx.types, base_type, substitution);
            if !self
                .class_extends_index_value_relation_outcome(derived_type, base_type_instantiated)
                .related
                && !class_extends_error_reported
            {
                let derived_type_str = self.format_type(derived_type);
                let base_type_str = self.format_type(base_type_instantiated);
                self.error_at_node(
                        derived_class.name,
                        &format!(
                            "Class '{derived_class_name}' incorrectly extends base class '{base_class_name}'.\n  'string' index signatures are incompatible.\n    Type '{derived_type_str}' is not assignable to type '{base_type_str}'."
                        ),
                        crate::diagnostics::diagnostic_codes::CLASS_INCORRECTLY_EXTENDS_BASE_CLASS,
                    );
                class_extends_error_reported = true;
            }
        }

        // Check number index signature compatibility
        if let (Some((derived_type, _derived_idx)), Some(base_type)) =
            (derived_number_index, base_number_index)
        {
            let base_type_instantiated = instantiate_type(self.ctx.types, base_type, substitution);
            if !self
                .class_extends_index_value_relation_outcome(derived_type, base_type_instantiated)
                .related
                && !class_extends_error_reported
            {
                let derived_type_str = self.format_type(derived_type);
                let base_type_str = self.format_type(base_type_instantiated);
                self.error_at_node(
                        derived_class.name,
                        &format!(
                            "Class '{derived_class_name}' incorrectly extends base class '{base_class_name}'.\n  'number' index signatures are incompatible.\n    Type '{derived_type_str}' is not assignable to type '{base_type_str}'."
                        ),
                        crate::diagnostics::diagnostic_codes::CLASS_INCORRECTLY_EXTENDS_BASE_CLASS,
                    );
            }
        }
    }
}
