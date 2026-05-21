//! Parser state - type parameter parsing.
//!
//! Owns the `<T extends Constraint = Default>` generic type-parameter
//! grammar. The grammar is shared by every callable that takes type
//! parameters (function declarations, function/arrow expressions,
//! class declarations/expressions, interface declarations, type-alias
//! declarations, call/construct signatures, JSX generic attribute
//! parsing, etc.), so it does not naturally belong to any single
//! owning state module.

use super::state::ParserState;
use crate::parser::{
    NodeIndex, NodeList,
    node::{IdentifierData, TypeParameterData},
    syntax_kind_ext,
};
use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
use tsz_common::interner::Atom;
use tsz_scanner::SyntaxKind;

impl ParserState {
    // Parse type parameters: <T, U extends Foo, V = `DefaultType`>
    pub(crate) fn parse_type_parameters(&mut self) -> NodeList {
        let mut params = Vec::new();
        let less_than_pos = self.token_pos();
        let mut has_trailing_comma = false;

        self.parse_expected(SyntaxKind::LessThanToken);

        // Check for empty type parameter list: <>
        // TypeScript reports TS1098: "Type parameter list cannot be empty"
        if self.is_token(SyntaxKind::GreaterThanToken) {
            self.parse_error_at(
                less_than_pos,
                1,
                "Type parameter list cannot be empty.",
                diagnostic_codes::TYPE_PARAMETER_LIST_CANNOT_BE_EMPTY,
            );
        }

        while !self.is_greater_than_or_compound() && !self.is_token(SyntaxKind::EndOfFileToken) {
            let param = self.parse_type_parameter();
            params.push(param);

            if !self.parse_optional(SyntaxKind::CommaToken) {
                if self.is_js_file() && self.is_token(SyntaxKind::ColonToken) {
                    self.error_comma_expected();
                }
                break;
            }
            // If the next token is `>`, the comma we just consumed was trailing.
            if self.is_greater_than_or_compound() {
                has_trailing_comma = true;
            }
        }

        self.parse_expected_greater_than();

        let mut list = self.make_node_list(params);
        list.has_trailing_comma = has_trailing_comma;
        list
    }

    // Parse a single type parameter: T or T extends U or T = Default or T extends U = Default
    // Also supports modifiers: `const T`, `in T`, `out T`, `in out T`, `const in T`, etc.
    pub(crate) fn parse_type_parameter(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse optional modifiers: const, in, out (TypeScript 4.7+ variance, 5.0+ const)
        let modifiers = self.parse_type_parameter_modifiers();

        // Parse the type parameter name.
        // When the current token is not an identifier, keyword, or reserved word,
        // emit TS1139 "Type parameter declaration expected" instead of the generic
        // TS1003 "Identifier expected", matching tsc behavior.
        let name = if !self.is_identifier_or_keyword() && !self.is_reserved_word() {
            let pos = self.token_pos();
            let end = self.token_end();
            if self.should_report_error() {
                self.parse_error_at(
                    pos,
                    end.saturating_sub(pos),
                    diagnostic_messages::TYPE_PARAMETER_DECLARATION_EXPECTED,
                    diagnostic_codes::TYPE_PARAMETER_DECLARATION_EXPECTED,
                );
            }
            self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                pos,
                pos,
                IdentifierData {
                    atom: Atom::NONE,
                    escaped_text: String::new(),
                    original_text: None,
                    type_arguments: None,
                },
            )
        } else if self.is_strict_mode_future_reserved_word() {
            self.parse_identifier_name()
        } else {
            self.parse_identifier()
        };

        // Parse optional constraint: extends SomeType
        let constraint = if self.parse_optional(SyntaxKind::ExtendsKeyword) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        // Parse optional default: = DefaultType
        let default = if self.parse_optional(SyntaxKind::EqualsToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();

        self.arena.add_type_parameter(
            syntax_kind_ext::TYPE_PARAMETER,
            start_pos,
            end_pos,
            TypeParameterData {
                modifiers,
                name,
                constraint,
                default,
            },
        )
    }

    // Parse type parameter modifiers: `const`, `in`, `out`
    //
    // Emits TS1029 ('in' must precede 'out') and TS1030 ('in'/'out' already seen)
    // when modifier ordering rules are violated. We still consume duplicates so a
    // subsequent identifier ends the modifier loop instead of producing cascading
    // errors from `in`/`out` being reserved keywords.
    //
    // CRITICAL: Before consuming a modifier keyword, peek at the next token to check
    // if it can follow a type parameter name (e.g., `>`, `,`, `extends`, `=`). If so,
    // the current keyword is the type parameter name, not a modifier. This avoids
    // greedily consuming keywords like `in in>` where the second `in` is the name.
    // Leaving it for `parse_identifier` produces TS1359 (reserved word) matching tsc.
    fn parse_type_parameter_modifiers(&mut self) -> Option<NodeList> {
        let mut modifiers = Vec::new();
        let mut seen_in = false;
        let mut seen_out = false;

        loop {
            match self.token() {
                SyntaxKind::ConstKeyword | SyntaxKind::InKeyword | SyntaxKind::OutKeyword => {
                    // Peek at the next token: if it can follow a type parameter name
                    // (>, ,, extends, =, EOF), then this keyword IS the name, not a modifier.
                    let saved_token = self.current_token;
                    let saved_state = self.scanner.save_state();
                    self.next_token();
                    let next = self.current_token;
                    self.scanner.restore_state(saved_state);
                    self.current_token = saved_token;

                    if matches!(
                        next,
                        SyntaxKind::GreaterThanToken
                            | SyntaxKind::CommaToken
                            | SyntaxKind::ExtendsKeyword
                            | SyntaxKind::EqualsToken
                            | SyntaxKind::EndOfFileToken
                    ) {
                        // This keyword is the type parameter name, not a modifier.
                        break;
                    }

                    let kind = self.token();
                    let pos = self.token_pos();
                    let end = self.token_end();
                    let length = end.saturating_sub(pos);
                    if kind == SyntaxKind::InKeyword {
                        if seen_in {
                            self.parse_error_at(
                                pos,
                                length,
                                "'in' modifier already seen.",
                                diagnostic_codes::MODIFIER_ALREADY_SEEN,
                            );
                        } else if seen_out {
                            self.parse_error_at(
                                pos,
                                length,
                                "'in' modifier must precede 'out' modifier.",
                                diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
                            );
                        }
                        seen_in = true;
                    } else if kind == SyntaxKind::OutKeyword {
                        if seen_out {
                            self.parse_error_at(
                                pos,
                                length,
                                "'out' modifier already seen.",
                                diagnostic_codes::MODIFIER_ALREADY_SEEN,
                            );
                        }
                        seen_out = true;
                    }
                    self.next_token();
                    modifiers.push(self.arena.add_token(kind as u16, pos, end));
                }
                // Handle invalid modifiers (public, private, static, etc.) - consume them
                // so the checker can emit TS1274. Only consume if followed by an identifier.
                SyntaxKind::PublicKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::StaticKeyword
                | SyntaxKind::ReadonlyKeyword
                | SyntaxKind::AsyncKeyword
                | SyntaxKind::DeclareKeyword
                | SyntaxKind::AbstractKeyword
                | SyntaxKind::OverrideKeyword
                | SyntaxKind::AccessorKeyword
                | SyntaxKind::ExportKeyword
                | SyntaxKind::DefaultKeyword => {
                    // Peek at the next token: only consume if followed by something
                    // that looks like a type parameter name (identifier/keyword)
                    let saved_token = self.current_token;
                    let saved_state = self.scanner.save_state();
                    self.next_token();
                    let next = self.current_token;
                    self.scanner.restore_state(saved_state);
                    self.current_token = saved_token;

                    // If next token could be a type parameter name, consume this as a modifier
                    if Self::token_can_start_type_parameter_name(next) {
                        let kind = self.token();
                        let pos = self.token_pos();
                        let end = self.token_end();
                        self.next_token();
                        modifiers.push(self.arena.add_token(kind as u16, pos, end));
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }

        if modifiers.is_empty() {
            None
        } else {
            Some(self.make_node_list(modifiers))
        }
    }

    #[inline]
    const fn token_can_start_type_parameter_name(token: SyntaxKind) -> bool {
        token as u16 >= SyntaxKind::Identifier as u16
            || (token as u16 >= SyntaxKind::FIRST_RESERVED_WORD as u16
                && token as u16 <= SyntaxKind::LAST_RESERVED_WORD as u16)
    }
}
