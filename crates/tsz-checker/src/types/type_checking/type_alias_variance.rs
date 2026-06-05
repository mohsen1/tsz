//! Type-alias variance annotation validation.

use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    pub(crate) fn check_variance_annotations_supported_for_type_alias(
        &mut self,
        alias: &tsz_parser::parser::node::TypeAliasData,
    ) -> bool {
        let Some(type_params) = &alias.type_parameters else {
            return true;
        };

        let variance_supported = self.type_alias_body_supports_variance_annotations(alias);
        if variance_supported {
            return true;
        }

        let mut emitted_unsupported_variance_diagnostic = false;
        for param_idx in type_params.nodes.iter().copied() {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                continue;
            };
            if self.node_contains_any_parse_error(param.name)
                || self.type_parameter_name_is_variance_keyword(param.name)
            {
                continue;
            }
            let Some(modifiers) = param.modifiers.as_ref() else {
                continue;
            };
            let Some(variance_modifier_idx) =
                modifiers.nodes.iter().copied().find(|&modifier_idx| {
                    self.ctx
                        .arena
                        .get(modifier_idx)
                        .is_some_and(|modifier_node| {
                            matches!(
                                modifier_node.kind,
                                k if k == SyntaxKind::InKeyword as u16
                                    || k == SyntaxKind::OutKeyword as u16
                            )
                        })
                })
            else {
                continue;
            };

            self.error_at_node(
                variance_modifier_idx,
                crate::diagnostics::diagnostic_messages::VARIANCE_ANNOTATIONS_ARE_ONLY_SUPPORTED_IN_TYPE_ALIASES_FOR_OBJECT_FUNCTION_CONS,
                crate::diagnostics::diagnostic_codes::VARIANCE_ANNOTATIONS_ARE_ONLY_SUPPORTED_IN_TYPE_ALIASES_FOR_OBJECT_FUNCTION_CONS,
            );
            emitted_unsupported_variance_diagnostic = true;
        }

        !emitted_unsupported_variance_diagnostic
    }

    pub(crate) fn type_alias_has_variance_annotation_to_check(
        &self,
        type_parameters: Option<&tsz_parser::parser::base::NodeList>,
    ) -> bool {
        let Some(type_params) = type_parameters else {
            return false;
        };

        type_params.nodes.iter().copied().any(|param_idx| {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                return false;
            };
            let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                return false;
            };
            let Some(modifiers) = &param.modifiers else {
                return false;
            };

            let mut declared_in = false;
            let mut declared_out = false;
            for modifier_idx in modifiers.nodes.iter().copied() {
                let Some(modifier_node) = self.ctx.arena.get(modifier_idx) else {
                    continue;
                };
                declared_in |= modifier_node.kind == SyntaxKind::InKeyword as u16;
                declared_out |= modifier_node.kind == SyntaxKind::OutKeyword as u16;
            }

            declared_in != declared_out
        })
    }

    fn type_alias_body_supports_variance_annotations(
        &self,
        alias: &tsz_parser::parser::node::TypeAliasData,
    ) -> bool {
        self.ctx.arena.kind_at(alias.type_node).is_some_and(|kind| {
            kind == syntax_kind_ext::TYPE_LITERAL
                || kind == syntax_kind_ext::FUNCTION_TYPE
                || kind == syntax_kind_ext::CONSTRUCTOR_TYPE
                || kind == syntax_kind_ext::MAPPED_TYPE
        })
    }

    fn type_parameter_name_is_variance_keyword(&self, name_idx: NodeIndex) -> bool {
        if matches!(
            self.get_identifier_text_from_idx(name_idx).as_deref(),
            Some("in" | "out")
        ) {
            return true;
        }
        self.ctx.arena.get(name_idx).is_some_and(|node| {
            node.kind == SyntaxKind::InKeyword as u16 || node.kind == SyntaxKind::OutKeyword as u16
        })
    }
}
