use crate::state::CheckerState;
use crate::types_domain::queries::core::GlobalReceiver;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn try_global_this_string_like_element_access(
        &mut self,
        idx: NodeIndex,
        is_element_access: bool,
        access_expression: NodeIndex,
        has_no_literal_string_key: bool,
        index_type: TypeId,
        is_this_global: bool,
        skip_flow_narrowing: bool,
    ) -> Option<TypeId> {
        if !has_no_literal_string_key
            || !is_element_access
            || !(self.is_global_this_expression(access_expression) || is_this_global)
            || !self.ctx.no_implicit_any()
            || self.is_js_file()
        {
            return None;
        }

        let string_literal_keys = self
            .get_literal_key_union_from_type(index_type)
            .filter(|(string_keys, number_keys)| number_keys.is_empty() && !string_keys.is_empty())
            .map(|(string_keys, _)| string_keys);
        let index_is_string_like = string_literal_keys.is_some()
            || crate::query_boundaries::common::is_string_type(self.ctx.types, index_type);
        if !index_is_string_like {
            return None;
        }

        if let Some(keys) = string_literal_keys {
            let mut resolved_types: Vec<TypeId> = Vec::with_capacity(keys.len());
            let mut all_resolved = true;
            for key_atom in &keys {
                let name = self.ctx.types.resolve_atom(*key_atom);
                let resolved = self.resolve_global_this_property_type(
                    &name,
                    idx,
                    true,
                    GlobalReceiver::GlobalThis,
                );
                if resolved != TypeId::ANY && resolved != TypeId::ERROR {
                    resolved_types.push(resolved);
                } else {
                    all_resolved = false;
                    break;
                }
            }
            if all_resolved && !resolved_types.is_empty() {
                return Some(if skip_flow_narrowing {
                    tsz_solver::utils::intersection_or_single(self.ctx.types, resolved_types)
                } else {
                    let combined =
                        tsz_solver::utils::union_or_single(self.ctx.types, resolved_types);
                    self.apply_flow_narrowing(idx, combined)
                });
            }
        }

        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        let index_str = self.format_type_diagnostic(index_type);
        self.error_at_node(
            idx,
            &format_message(
                diagnostic_messages::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN,
                &[&index_str, "typeof globalThis"],
            ),
            diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN,
        );
        Some(TypeId::ANY)
    }
}
