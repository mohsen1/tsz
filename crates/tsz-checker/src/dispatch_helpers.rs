//! Helpers for the expression type computation dispatcher.

use std::collections::BTreeSet;

use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

use super::dispatch::ExpressionDispatcher;

impl<'a, 'b> ExpressionDispatcher<'a, 'b> {
    pub(crate) fn dispatch_regular_expression_literal(&mut self, idx: NodeIndex) -> TypeId {
        if let Some(node) = self.checker.ctx.arena.get(idx)
            && let Some(literal) = self.checker.ctx.arena.get_literal(node)
            && let Some(raw_text) = literal.raw_text.as_deref()
        {
            let bytes = raw_text.as_bytes();
            let mut body_end = bytes.len();
            let mut in_escape = false;
            let mut in_character_class = false;

            for (i, ch) in bytes.iter().enumerate().skip(1) {
                let ch = *ch;
                if in_escape {
                    in_escape = false;
                    continue;
                }
                if ch == b'\\' {
                    in_escape = true;
                } else if ch == b'[' && !in_character_class {
                    in_character_class = true;
                } else if ch == b']' && in_character_class {
                    in_character_class = false;
                } else if ch == b'/' && !in_character_class {
                    body_end = i;
                    break;
                }
            }

            self.check_regular_expression_v_flag(node.pos, bytes, body_end);
            self.check_regular_expression_named_groups(node.pos, raw_text, bytes, body_end);
        }

        self.checker
            .resolve_lib_type_by_name("RegExp")
            .unwrap_or(TypeId::ANY)
    }

    fn check_regular_expression_v_flag(&mut self, node_pos: u32, bytes: &[u8], body_end: usize) {
        if self.checker.ctx.compiler_options.target.supports_es2024() {
            return;
        }

        let flag_start = body_end.saturating_add(1);
        let Some(v_offset) = bytes
            .get(flag_start..)
            .and_then(|flags| flags.iter().position(|&flag| flag == b'v'))
        else {
            return;
        };

        let message = format_message(
            diagnostic_messages::THIS_REGULAR_EXPRESSION_FLAG_IS_ONLY_AVAILABLE_WHEN_TARGETING_OR_LATER,
            &["es2024"],
        );
        self.checker.error_at_position(
            node_pos + (flag_start + v_offset) as u32,
            1,
            &message,
            diagnostic_codes::THIS_REGULAR_EXPRESSION_FLAG_IS_ONLY_AVAILABLE_WHEN_TARGETING_OR_LATER,
        );
    }

    fn check_regular_expression_named_groups(
        &mut self,
        node_pos: u32,
        raw_text: &str,
        bytes: &[u8],
        body_end: usize,
    ) {
        let mut group_names = BTreeSet::new();
        let mut i = 1usize;
        let target_supports_named_groups =
            self.checker.ctx.compiler_options.target.supports_es2018();

        while i < body_end {
            if bytes[i] == b'\\' {
                if i + 2 < body_end && bytes[i + 1] == b'k' && bytes[i + 2] == b'<' {
                    let name_start = i + 3;
                    let mut name_end = name_start;
                    while name_end < body_end && bytes[name_end] != b'>' {
                        name_end += 1;
                    }
                    if name_end < body_end {
                        let name = &raw_text[name_start..name_end];
                        if !group_names.contains(name) {
                            let message = format_message(
                                diagnostic_messages::THERE_IS_NO_CAPTURING_GROUP_NAMED_IN_THIS_REGULAR_EXPRESSION,
                                &[name],
                            );
                            self.checker.error_at_position(
                                node_pos + name_start as u32,
                                1,
                                &message,
                                diagnostic_codes::THERE_IS_NO_CAPTURING_GROUP_NAMED_IN_THIS_REGULAR_EXPRESSION,
                            );
                        }
                        i = name_end + 1;
                        continue;
                    }
                }
                i += 2;
                continue;
            }

            if i + 3 < body_end
                && bytes[i] == b'('
                && bytes[i + 1] == b'?'
                && bytes[i + 2] == b'<'
                && !matches!(bytes[i + 3], b'=' | b'!')
            {
                if !target_supports_named_groups {
                    self.checker.error_at_position(
                        node_pos + (i + 2) as u32,
                        1,
                        diagnostic_messages::NAMED_CAPTURING_GROUPS_ARE_ONLY_AVAILABLE_WHEN_TARGETING_ES2018_OR_LATER,
                        diagnostic_codes::NAMED_CAPTURING_GROUPS_ARE_ONLY_AVAILABLE_WHEN_TARGETING_ES2018_OR_LATER,
                    );
                }

                let name_start = i + 3;
                let mut name_end = name_start;
                while name_end < body_end && bytes[name_end] != b'>' {
                    name_end += 1;
                }
                if name_end < body_end {
                    group_names.insert(raw_text[name_start..name_end].to_string());
                    i = name_end + 1;
                    continue;
                }
            }

            i += 1;
        }
    }

    fn property_name_matches_atom(&self, name_idx: NodeIndex, target: Atom) -> bool {
        let Some(name_node) = self.checker.ctx.arena.get(name_idx) else {
            return false;
        };
        let resolved = self.checker.ctx.types.resolve_atom_ref(target);
        let target_str: &str = &resolved;
        if let Some(ident) = self.checker.ctx.arena.get_identifier(name_node) {
            return ident.escaped_text.as_str() == target_str;
        }
        if let Some(literal) = self.checker.ctx.arena.get_literal(name_node) {
            return literal.text.as_str() == target_str;
        }
        false
    }

    pub(crate) fn object_literal_this_property_blocks_assertion_overlap(
        &mut self,
        expr_idx: NodeIndex,
        target_type: TypeId,
    ) -> bool {
        let expr_idx = self
            .checker
            .ctx
            .arena
            .skip_parenthesized_and_assertions(expr_idx);
        let Some(expr_node) = self.checker.ctx.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }

        let target_type = self.checker.evaluate_type_for_assignability(target_type);
        let Some(target_shape) = crate::query_boundaries::common::object_shape_for_type(
            self.checker.ctx.types,
            target_type,
        ) else {
            return false;
        };
        let Some(lit_data) = self.checker.ctx.arena.get_literal_expr(expr_node) else {
            return false;
        };

        let mut has_incompatible_this_property = false;
        let mut has_other_compatible_common_property = false;
        for &elem_idx in &lit_data.elements.nodes {
            let Some(elem_node) = self.checker.ctx.arena.get(elem_idx) else {
                continue;
            };
            if elem_node.kind != syntax_kind_ext::PROPERTY_ASSIGNMENT {
                continue;
            }
            let Some(prop) = self.checker.ctx.arena.get_property_assignment(elem_node) else {
                continue;
            };
            let Some(target_prop) = target_shape
                .properties
                .iter()
                .find(|target_prop| self.property_name_matches_atom(prop.name, target_prop.name))
            else {
                continue;
            };

            let prop_type = self.checker.get_type_of_node(prop.initializer);
            let prop_compatible = self
                .checker
                .is_assignable_for_type_assertion_overlap(prop_type, target_prop.type_id)
                || self
                    .checker
                    .is_assignable_for_type_assertion_overlap(target_prop.type_id, prop_type);

            let value_is_this_keyword = self
                .checker
                .ctx
                .arena
                .get(prop.initializer)
                .is_some_and(|node| node.kind == SyntaxKind::ThisKeyword as u16);
            if value_is_this_keyword && !prop_compatible {
                has_incompatible_this_property = true;
            } else if prop_compatible {
                has_other_compatible_common_property = true;
            }
        }

        has_incompatible_this_property && !has_other_compatible_common_property
    }

    /// TS1355: Check that an expression is a valid target for `as const`.
    pub(crate) fn check_const_assertion_expression(&mut self, expr_idx: NodeIndex) {
        if self.is_valid_const_assertion_arg(expr_idx) {
            return;
        }
        self.checker.error_at_node(
            expr_idx,
            diagnostic_messages::A_CONST_ASSERTION_CAN_ONLY_BE_APPLIED_TO_REFERENCES_TO_ENUM_MEMBERS_OR_STRING_NU,
            diagnostic_codes::A_CONST_ASSERTION_CAN_ONLY_BE_APPLIED_TO_REFERENCES_TO_ENUM_MEMBERS_OR_STRING_NU,
        );
    }

    fn is_valid_const_assertion_arg(&self, expr_idx: NodeIndex) -> bool {
        let Some(node) = self.checker.ctx.arena.get(expr_idx) else {
            return false;
        };
        match node.kind {
            // Literal types
            k if k == SyntaxKind::StringLiteral as u16 => true,
            k if k == SyntaxKind::NumericLiteral as u16 => true,
            k if k == SyntaxKind::BigIntLiteral as u16 => true,
            k if k == SyntaxKind::TrueKeyword as u16 => true,
            k if k == SyntaxKind::FalseKeyword as u16 => true,
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => true,
            // Compound literal types
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => true,
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => true,
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => true,
            // Prefix unary: `-` or `+` on numeric/bigint literal
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.checker.ctx.arena.get_unary_expr(node)
                    && (unary.operator == SyntaxKind::MinusToken as u16
                        || unary.operator == SyntaxKind::PlusToken as u16)
                    && let Some(operand) = self.checker.ctx.arena.get(unary.operand)
                {
                    return operand.kind == SyntaxKind::NumericLiteral as u16
                        || operand.kind == SyntaxKind::BigIntLiteral as u16;
                }
                false
            }
            // Parenthesized: recurse
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.checker.ctx.arena.get_parenthesized(node) {
                    return self.is_valid_const_assertion_arg(paren.expression);
                }
                false
            }
            // Property access: valid only if it's an enum member reference
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(access) = self.checker.ctx.arena.get_access_expr(node) {
                    return self.checker.is_enum_member_property(access.expression, "");
                }
                false
            }
            _ => false,
        }
    }
}

/// Maps a syntax kind to its keyword type name and `TypeId` for TS2693 checking.
pub(crate) const fn keyword_type_mapping(kind: u16) -> Option<(&'static str, TypeId)> {
    match kind {
        k if k == SyntaxKind::NumberKeyword as u16 => Some(("number", TypeId::NUMBER)),
        k if k == SyntaxKind::StringKeyword as u16 => Some(("string", TypeId::STRING)),
        k if k == SyntaxKind::BooleanKeyword as u16 => Some(("boolean", TypeId::BOOLEAN)),
        k if k == SyntaxKind::VoidKeyword as u16 => Some(("void", TypeId::VOID)),
        k if k == SyntaxKind::AnyKeyword as u16 => Some(("any", TypeId::ANY)),
        k if k == SyntaxKind::NeverKeyword as u16 => Some(("never", TypeId::NEVER)),
        k if k == SyntaxKind::UnknownKeyword as u16 => Some(("unknown", TypeId::UNKNOWN)),
        k if k == SyntaxKind::UndefinedKeyword as u16 => Some(("undefined", TypeId::UNDEFINED)),
        k if k == SyntaxKind::ObjectKeyword as u16 => Some(("object", TypeId::OBJECT)),
        k if k == SyntaxKind::BigIntKeyword as u16 => Some(("bigint", TypeId::BIGINT)),
        k if k == SyntaxKind::SymbolKeyword as u16 => Some(("symbol", TypeId::SYMBOL)),
        _ => None,
    }
}
