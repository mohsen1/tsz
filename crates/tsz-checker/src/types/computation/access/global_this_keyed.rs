use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::state::CheckerState;
use crate::types_domain::queries::core::GlobalReceiver;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

pub(super) enum GlobalThisAccessKind {
    Element,
    Other,
}

pub(super) enum GlobalThisKeyStatus {
    NoLiteralStringKey,
    HasLiteralStringKey,
}

pub(super) enum GlobalThisReceiverStatus {
    GlobalThisLike,
    Other,
}

pub(super) enum GlobalThisFlowMode {
    SkipFlowNarrowing,
    ApplyFlowNarrowing,
}

pub(super) struct GlobalThisStringLikeElementAccess {
    pub(super) idx: NodeIndex,
    pub(super) access_kind: GlobalThisAccessKind,
    pub(super) access_expression: NodeIndex,
    pub(super) key_status: GlobalThisKeyStatus,
    pub(super) index_type: TypeId,
    pub(super) receiver_status: GlobalThisReceiverStatus,
    pub(super) flow_mode: GlobalThisFlowMode,
}

impl<'a> CheckerState<'a> {
    pub(super) fn try_global_this_string_like_element_access(
        &mut self,
        request: GlobalThisStringLikeElementAccess,
    ) -> Option<TypeId> {
        if !matches!(request.key_status, GlobalThisKeyStatus::NoLiteralStringKey)
            || !matches!(request.access_kind, GlobalThisAccessKind::Element)
            || !(self.is_global_this_expression(request.access_expression)
                || matches!(
                    request.receiver_status,
                    GlobalThisReceiverStatus::GlobalThisLike
                ))
            || !self.ctx.no_implicit_any()
            || self.is_js_file()
        {
            return None;
        }

        let string_literal_keys = self
            .get_literal_key_union_from_type(request.index_type)
            .filter(|(string_keys, number_keys)| number_keys.is_empty() && !string_keys.is_empty())
            .map(|(string_keys, _)| string_keys);
        let index_is_string_like = string_literal_keys.is_some()
            || crate::query_boundaries::common::is_string_type(self.ctx.types, request.index_type);
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
                    request.idx,
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
                return Some(
                    if matches!(request.flow_mode, GlobalThisFlowMode::SkipFlowNarrowing) {
                        tsz_solver::utils::intersection_or_single(self.ctx.types, resolved_types)
                    } else {
                        let combined =
                            tsz_solver::utils::union_or_single(self.ctx.types, resolved_types);
                        self.apply_flow_narrowing(request.idx, combined)
                    },
                );
            }
        }

        let index_str = self.format_type_diagnostic(request.index_type);
        self.error_at_node(
            request.idx,
            &format_message(
                diagnostic_messages::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN,
                &[&index_str, "typeof globalThis"],
            ),
            diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN,
        );
        Some(TypeId::ANY)
    }
}
