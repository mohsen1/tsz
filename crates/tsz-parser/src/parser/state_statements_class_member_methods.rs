//! Parser state - class member method construction.

use super::state::{CONTEXT_FLAG_RECOVERED_IF_CLASS_MEMBER_PARAMETERS, ParserState};
use super::state_statements_class_members::ClassMemberModifierSet;
use crate::parser::{NodeIndex, node, syntax_kind_ext};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_scanner::SyntaxKind;

impl ParserState {
    /// Construct the body of a method class member: parse type params,
    /// parameter list, return-type annotation, and method body.
    pub(super) fn construct_class_member_method(
        &mut self,
        start_pos: u32,
        mods: ClassMemberModifierSet,
        asterisk_token: bool,
        name: NodeIndex,
        question_token: bool,
        method_saved_flags: u32,
    ) -> NodeIndex {
        // TS1031: 'declare' modifier cannot appear on class elements of this
        // kind (methods cannot be declared, only properties can). Suppressed
        // when a `static`/`async` ordering conflict or an `async`+`declare`
        // ambient conflict already fired while scanning modifiers — tsc's
        // grammar walk reports only the first problem found in source order,
        // so a third modifier joining an already-conflicting pair (e.g.
        // `declare async static m()`, or `async static declare m()`) must not
        // add this as a second diagnostic.
        if mods.has_declare && !mods.async_declare_order_conflict_reported {
            self.emit_declare_on_non_property_error(&mods.modifiers);
        }
        // TS1275: 'accessor' modifier can only appear on a property declaration.
        if mods.has_accessor {
            self.emit_accessor_modifier_only_on_property_error(&mods.modifiers);
        }

        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        let has_open_paren = self.parse_optional(SyntaxKind::OpenParenToken);
        let mut body_already_consumed_by_recovery = false;
        let parameters = if has_open_paren {
            let saved_flags = self.context_flags;
            if self.class_member_name_is_if_keyword(name) {
                self.context_flags |= CONTEXT_FLAG_RECOVERED_IF_CLASS_MEMBER_PARAMETERS;
            }
            let parameters = self.parse_parameter_list();
            self.context_flags = saved_flags;
            self.parse_expected(SyntaxKind::CloseParenToken);
            parameters
        } else if asterisk_token {
            // `async *` members must be methods. Missing `(` here should emit
            // one TS1005 and recover without producing a declaration node, so
            // we avoid downstream errors like TS2391 on malformed members.
            self.parse_error_at_current_token("'(' expected.", diagnostic_codes::EXPECTED);
            self.recover_from_missing_method_open_paren();
            self.context_flags = method_saved_flags;
            return NodeIndex::NONE;
        } else {
            self.parse_error_at_current_token("'(' expected.", diagnostic_codes::EXPECTED);
            body_already_consumed_by_recovery = self.recover_from_missing_method_open_paren();
            Self::make_node_list(vec![])
        };

        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };
        let recovered_if_comparison_tail =
            self.class_member_name_is_if_keyword(name) && self.current_token_is_comparison_tail();

        self.push_label_scope();
        let body = if body_already_consumed_by_recovery {
            NodeIndex::NONE
        } else if recovered_if_comparison_tail {
            self.arena.add_block(
                syntax_kind_ext::BLOCK,
                self.token_pos(),
                self.token_pos(),
                node::BlockData {
                    statements: Self::make_node_list(Vec::new()),
                    multi_line: false,
                },
            )
        } else if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            // Consume the semicolon if present (method signature). This handles
            // ASI the same way as tsc's parseFunctionBlockOrSemicolon.
            if self.can_parse_semicolon() {
                self.parse_semicolon();
            } else {
                self.parse_error_at_current_token(
                    "'{' or ';' expected.",
                    diagnostic_codes::OR_EXPECTED,
                );
            }
            NodeIndex::NONE
        };
        self.pop_label_scope();

        self.context_flags = method_saved_flags;

        let end_pos = self.token_full_start();
        self.arena.add_method_decl(
            syntax_kind_ext::METHOD_DECLARATION,
            start_pos,
            end_pos,
            node::MethodDeclData {
                modifiers: mods.modifiers,
                asterisk_token,
                name,
                question_token,
                type_parameters,
                parameters,
                type_annotation,
                body,
            },
        )
    }

    fn class_member_name_is_if_keyword(&self, name: NodeIndex) -> bool {
        let Some(name_node) = self.arena.get(name) else {
            return false;
        };
        name_node.kind == SyntaxKind::IfKeyword as u16
            || self
                .arena
                .get_identifier(name_node)
                .is_some_and(|ident| ident.escaped_text == "if")
    }
}
