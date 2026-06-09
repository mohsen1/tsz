//! Object literal parsing extracted from `state_expressions_literals.rs`.

use super::state::ParserState;
use crate::parser::{
    NodeIndex,
    node::{IdentifierData, LiteralExprData},
    syntax_kind_ext,
};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_scanner::SyntaxKind;

impl ParserState {
    /// Whether the current token can start an object-literal element, mirroring
    /// tsc's `isListElement(ParsingContext.ObjectLiteralMembers)`.
    pub(crate) const fn is_object_literal_element_start(&self) -> bool {
        matches!(
            self.token(),
            SyntaxKind::OpenBracketToken
                | SyntaxKind::AsteriskToken
                | SyntaxKind::DotDotDotToken
                | SyntaxKind::DotToken
                | SyntaxKind::NoSubstitutionTemplateLiteral
                | SyntaxKind::TemplateHead
        ) || self.is_property_name()
    }

    /// Parse object literal
    pub(crate) fn parse_object_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        let mut properties = Vec::new();
        while !self.is_token(SyntaxKind::CloseBraceToken) {
            if !self.is_object_literal_element_start()
                && !self.is_token(SyntaxKind::SemicolonToken)
                && !self.is_token(SyntaxKind::EndOfFileToken)
            {
                self.parse_error_at_current_token(
                    "Property assignment expected.",
                    diagnostic_codes::PROPERTY_ASSIGNMENT_EXPECTED,
                );
                self.next_token();
                self.suppress_object_literal_comma_once = true;
                continue;
            }

            let prop = self.parse_property_assignment();
            if prop.is_some() {
                properties.push(prop);
            }
            if self.abort_object_literal_recovery_once {
                self.abort_object_literal_recovery_once = false;
                break;
            }

            if !self.parse_optional(SyntaxKind::CommaToken) {
                if self.suppress_object_literal_comma_once && self.is_property_start() {
                    self.suppress_object_literal_comma_once = false;
                    continue;
                }
                self.suppress_object_literal_comma_once = false;

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
                        let diag_count_before = self.parse_diagnostics.len();
                        self.error_comma_expected();
                        let comma_error_emitted = self.parse_diagnostics.len() > diag_count_before;
                        self.next_token();
                        if comma_error_emitted {
                            self.last_error_pos = self.token_pos();
                        }
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
                    if self.is_token(SyntaxKind::DotToken) {
                        self.recovered_object_literal_dot_tail_once = true;
                    }
                    self.parse_error_at_current_token("',' expected.", diagnostic_codes::EXPECTED);
                    self.next_token();
                }
            }
        }

        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::CloseBraceToken);

        self.arena.add_literal_expr(
            syntax_kind_ext::OBJECT_LITERAL_EXPRESSION,
            start_pos,
            end_pos,
            LiteralExprData {
                elements: Self::make_node_list(properties),
                multi_line: false,
            },
        )
    }

    pub(crate) fn create_missing_property_value(&mut self, pos: u32) -> NodeIndex {
        let atom = self.scanner.interner_mut().intern("");
        self.arena.add_identifier(
            SyntaxKind::Identifier as u16,
            pos,
            pos,
            IdentifierData {
                atom,
                escaped_text: String::new(),
                original_text: None,
                type_arguments: None,
            },
        )
    }
}
