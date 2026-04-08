//! Parser state - import attribute and import type option parsing helpers.

use super::state::ParserState;
use crate::parser::{
    NodeIndex,
    node::{LiteralExprData, PropertyAssignmentData},
    syntax_kind_ext,
};
use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
use tsz_scanner::SyntaxKind;

impl ParserState {
    pub(crate) fn look_ahead_is_import_attributes_property(&mut self) -> bool {
        let matches_key = if self.is_token(SyntaxKind::StringLiteral) {
            matches!(self.scanner.get_token_value_ref(), "with" | "assert")
        } else if self.is_identifier_or_keyword() {
            matches!(self.scanner.get_token_value_ref(), "with" | "assert")
        } else {
            false
        };

        if !matches_key {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        if self.is_token(SyntaxKind::StringLiteral) {
            self.parse_string_literal();
        } else {
            self.parse_identifier_name();
        }

        let result = self.is_token(SyntaxKind::ColonToken);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    fn import_property_name_matches(&self, name: NodeIndex, expected: &str) -> bool {
        let Some(node) = self.arena.get(name) else {
            return false;
        };

        if let Some(ident) = self.arena.get_identifier(node) {
            return ident.escaped_text == expected;
        }

        if let Some(literal) = self.arena.get_literal(node) {
            return literal.text == expected;
        }

        false
    }

    pub(crate) fn parse_import_options_object_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        let mut properties = Vec::new();
        let mut aborted_after_nested_recovery = false;
        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            if self.is_token(SyntaxKind::CommaToken) {
                self.error_token_expected("}");
                self.next_token();
                self.abort_intersection_continuation = true;
                self.report_invalid_import_attribute_tail_recovery(None);
                aborted_after_nested_recovery = true;
                break;
            }

            let prop = if self.look_ahead_is_import_attributes_property() {
                self.parse_import_options_property_assignment()
            } else if self.is_property_start() {
                let mut semicolon_recovery = None;
                self.parse_error_at_current_token("'with' expected.", diagnostic_codes::EXPECTED);
                while !self.is_token(SyntaxKind::CloseBraceToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    if semicolon_recovery.is_none() && self.is_token(SyntaxKind::ColonToken) {
                        let start = self.u32_from_usize(self.scanner.get_token_start());
                        let end = self.u32_from_usize(self.scanner.get_token_end());
                        semicolon_recovery = Some((start, end - start));
                    }
                    self.next_token();
                }
                self.abort_intersection_continuation = true;
                self.report_invalid_import_attribute_tail_recovery(semicolon_recovery);
                aborted_after_nested_recovery = true;
                NodeIndex::NONE
            } else {
                self.parse_property_assignment()
            };

            if prop.is_some() {
                properties.push(prop);
            }

            if self.import_attribute_tail_recovered {
                aborted_after_nested_recovery = true;
                break;
            }

            if !self.parse_optional(SyntaxKind::CommaToken) {
                if self.is_token(SyntaxKind::SemicolonToken) {
                    let snapshot = self.scanner.save_state();
                    let saved_token = self.current_token;
                    self.next_token();
                    let should_continue =
                        self.is_property_start() || self.is_token(SyntaxKind::CloseBraceToken);
                    let follows_eof = self.is_token(SyntaxKind::EndOfFileToken);
                    self.scanner.restore_state(snapshot);
                    self.current_token = saved_token;

                    if should_continue {
                        self.parse_error_at_current_token(
                            "',' expected.",
                            diagnostic_codes::EXPECTED,
                        );
                        self.next_token();
                    } else if follows_eof {
                        self.error_comma_expected();
                        break;
                    } else {
                        break;
                    }
                } else if self.is_property_start() && !self.is_token(SyntaxKind::CloseBraceToken) {
                    self.error_comma_expected();
                } else if self.is_token(SyntaxKind::EndOfFileToken)
                    || self.is_token(SyntaxKind::CloseBraceToken)
                {
                    break;
                } else {
                    self.error_comma_expected();
                    self.next_token();
                }
            }
        }

        let end_pos = self.token_end();
        if !aborted_after_nested_recovery {
            self.parse_expected(SyntaxKind::CloseBraceToken);
        }

        self.arena.add_literal_expr(
            syntax_kind_ext::OBJECT_LITERAL_EXPRESSION,
            start_pos,
            end_pos,
            LiteralExprData {
                elements: self.make_node_list(properties),
                multi_line: false,
            },
        )
    }

    fn parse_import_options_property_assignment(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let name = self.parse_property_name();

        let initializer = if self.parse_optional(SyntaxKind::ColonToken) {
            if (self.import_property_name_matches(name, "with")
                || self.import_property_name_matches(name, "assert"))
                && self.is_token(SyntaxKind::OpenBraceToken)
            {
                self.parse_import_attributes_value_object_literal()
            } else {
                self.parse_assignment_expression()
            }
        } else {
            self.parse_error_at_current_token("':' expected.", diagnostic_codes::EXPECTED);
            name
        };

        let end_pos = self.token_end();
        self.arena.add_property_assignment(
            syntax_kind_ext::PROPERTY_ASSIGNMENT,
            start_pos,
            end_pos,
            PropertyAssignmentData {
                modifiers: None,
                name,
                initializer,
            },
        )
    }

    fn parse_import_attributes_value_object_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        let mut properties = Vec::new();
        let mut aborted_on_invalid_key = false;

        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let prop_start = self.token_pos();
            let name = if self.is_token(SyntaxKind::StringLiteral) {
                self.parse_string_literal()
            } else if self.is_identifier_or_keyword() {
                self.parse_identifier_name()
            } else {
                let mut semicolon_recovery = None;
                self.parse_error_at_current_token(
                    diagnostic_messages::IDENTIFIER_OR_STRING_LITERAL_EXPECTED,
                    diagnostic_codes::IDENTIFIER_OR_STRING_LITERAL_EXPECTED,
                );
                while !self.is_token(SyntaxKind::CloseBraceToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    if semicolon_recovery.is_none() && self.is_token(SyntaxKind::ColonToken) {
                        let start = self.u32_from_usize(self.scanner.get_token_start());
                        let end = self.u32_from_usize(self.scanner.get_token_end());
                        semicolon_recovery = Some((start, end - start));
                    }
                    self.next_token();
                }
                self.abort_intersection_continuation = true;
                self.report_invalid_import_attribute_tail_recovery(semicolon_recovery);
                aborted_on_invalid_key = true;
                break;
            };

            self.parse_expected(SyntaxKind::ColonToken);
            let value = self.parse_assignment_expression();
            let end_pos = self
                .arena
                .get(value)
                .map_or_else(|| self.token_end(), |node| node.end);

            properties.push(self.arena.add_property_assignment(
                syntax_kind_ext::PROPERTY_ASSIGNMENT,
                prop_start,
                end_pos,
                PropertyAssignmentData {
                    modifiers: None,
                    name,
                    initializer: value,
                },
            ));

            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }

        let end_pos = self.token_end();
        if !aborted_on_invalid_key {
            self.parse_expected(SyntaxKind::CloseBraceToken);
        }

        self.arena.add_literal_expr(
            syntax_kind_ext::OBJECT_LITERAL_EXPRESSION,
            start_pos,
            end_pos,
            LiteralExprData {
                elements: self.make_node_list(properties),
                multi_line: false,
            },
        )
    }

    pub(crate) fn report_invalid_import_attribute_tail_recovery(
        &mut self,
        semicolon_recovery: Option<(u32, u32)>,
    ) {
        if let Some((start, length)) = semicolon_recovery {
            self.parse_error_at(start, length, "';' expected.", diagnostic_codes::EXPECTED);
        }

        let mut saw_dot = false;
        while matches!(
            self.token(),
            SyntaxKind::CloseBraceToken | SyntaxKind::CloseParenToken | SyntaxKind::DotToken
        ) {
            let token = self.token();
            self.parse_error_at_current_token(
                diagnostic_messages::DECLARATION_OR_STATEMENT_EXPECTED,
                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            );
            if token == SyntaxKind::CloseParenToken {
                self.import_attribute_tail_recovered = true;
            }
            saw_dot = token == SyntaxKind::DotToken;
            self.next_token();
            if self.scanner.has_preceding_line_break() {
                return;
            }
        }

        if saw_dot && self.is_identifier_or_keyword() {
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token();
            let next_has_line_break = self.scanner.has_preceding_line_break();
            let next_token = self.token();
            self.scanner.restore_state(snapshot);
            self.current_token = current;

            if !next_has_line_break {
                self.parse_error_at_current_token(
                    diagnostic_messages::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                    diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                );
            }
            self.next_token();
            if !next_has_line_break && next_token == SyntaxKind::CloseParenToken {
                self.parse_error_at_current_token(
                    diagnostic_messages::DECLARATION_OR_STATEMENT_EXPECTED,
                    diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                );
            }
        }
    }
}
