//! Parser state - enum declaration parsing methods.
//!
//! Split out of `state_declarations.rs` to keep each parser source file under
//! the 2000-line architecture ceiling. Behaviour is unchanged; these methods
//! remain `impl ParserState` and are called from the same sites as before.

use super::state::ParserState;
use crate::parser::{
    NodeIndex, NodeList,
    node::{EnumData, EnumMemberData, IdentifierData},
    syntax_kind_ext,
};
use tsz_common::interner::{AstAtom, IdentText};
use tsz_scanner::SyntaxKind;

impl ParserState {
    /// Parse enum declaration
    pub(crate) fn parse_enum_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_enum_declaration_with_modifiers(start_pos, None)
    }

    /// Parse enum declaration with explicit modifiers
    pub(crate) fn parse_enum_declaration_with_modifiers(
        &mut self,
        start_pos: u32,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        let enum_keyword_end = self.token_end();
        self.parse_expected(SyntaxKind::EnumKeyword);

        let name = self.parse_enum_declaration_name();

        let has_open_brace = self.parse_expected(SyntaxKind::OpenBraceToken);

        let members = if has_open_brace {
            self.parse_enum_members()
        } else {
            Self::make_node_list(Vec::new())
        };

        if has_open_brace {
            self.parse_expected(SyntaxKind::CloseBraceToken);
        }

        let end_pos = if has_open_brace {
            self.token_end()
        } else {
            enum_keyword_end
        };
        self.arena.add_enum(
            syntax_kind_ext::ENUM_DECLARATION,
            start_pos,
            end_pos,
            EnumData {
                modifiers,
                name,
                members,
            },
        )
    }

    fn parse_enum_declaration_name(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let end_pos = self.token_end();

        if self.is_reserved_word() {
            // `tsc` reports the missing enum name but leaves the reserved word
            // for the outer statement parser. This preserves recovered forms like
            // `enum void {}` as an anonymous enum plus a following `void {}`.
            self.error_identifier_expected();
            return self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                start_pos,
                end_pos,
                IdentifierData {
                    atom: AstAtom::NONE,
                    escaped_text: IdentText::empty(),
                    original_text: None,
                },
            );
        }

        self.parse_identifier()
    }

    /// Parse enum members
    pub(crate) fn parse_enum_members(&mut self) -> NodeList {
        use tsz_common::diagnostics::diagnostic_codes;
        let mut members = Vec::new();

        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let start_pos = self.token_pos();

            // Handle leading comma - emit TS1132 "Enum member expected" and skip
            if self.is_token(SyntaxKind::CommaToken) {
                self.parse_error_at_current_token(
                    "Enum member expected.",
                    diagnostic_codes::ENUM_MEMBER_EXPECTED,
                );
                self.next_token(); // Skip the comma
                continue;
            }

            // Handle @ inside enum body - not a valid enum member start.
            // Emit TS1132 and break out so the outer statement parser handles the
            // decorator-like syntax (producing TS1146 + TS1128 matching tsc).
            if self.is_token(SyntaxKind::AtToken) {
                self.parse_error_at_current_token(
                    "Enum member expected.",
                    diagnostic_codes::ENUM_MEMBER_EXPECTED,
                );
                break;
            }

            // Enum member names can be identifiers, string literals, or computed property names.
            // Numeric literals are parsed as names for error recovery (TS2452 reported by checker).
            // Computed property names ([x]) are not valid in enums but we recover gracefully.
            let name = if self.is_token(SyntaxKind::OpenBracketToken) {
                // Parse computed property name for recovery. TS1164 is emitted by the
                // checker (grammar check), not the parser, matching tsc's behavior.
                // This avoids position-based dedup conflicts with TS1357.
                self.parse_property_name()
            } else if self.is_token(SyntaxKind::StringLiteral) {
                self.parse_string_literal()
            } else if self.is_token(SyntaxKind::NumericLiteral) {
                // Parse numeric literal as name for recovery (checker emits TS2452)
                self.parse_numeric_literal()
            } else if self.is_token(SyntaxKind::BigIntLiteral) {
                // Parse bigint literal as name for recovery (checker emits TS2452)
                self.parse_bigint_literal()
            } else if self.is_token(SyntaxKind::PrivateIdentifier) {
                self.parse_error_at_current_token(
                    "An enum member cannot be named with a private identifier.",
                    diagnostic_codes::AN_ENUM_MEMBER_CANNOT_BE_NAMED_WITH_A_PRIVATE_IDENTIFIER,
                );
                self.parse_private_identifier()
            } else {
                self.parse_identifier_name()
            };

            // Check for unexpected token after enum member name - emit TS1357.
            // `tsc` still records the malformed member before recovering, so emit
            // continues to allocate enum values for invalid names such as
            // `name: 1` and `name;`.
            if !self.is_token(SyntaxKind::EqualsToken)
                && !self.is_token(SyntaxKind::CommaToken)
                && !self.is_token(SyntaxKind::CloseBraceToken)
                && !self.is_token(SyntaxKind::EndOfFileToken)
            {
                self.parse_error_at_current_token(
                    "An enum member name must be followed by a ',', '=', or '}'.",
                    diagnostic_codes::AN_ENUM_MEMBER_NAME_MUST_BE_FOLLOWED_BY_A_OR,
                );

                let member_end = self.arena.get(name).map_or(start_pos, |node| node.end);
                let member = self.arena.add_enum_member(
                    syntax_kind_ext::ENUM_MEMBER,
                    start_pos,
                    member_end,
                    EnumMemberData {
                        name,
                        initializer: NodeIndex::NONE,
                    },
                );
                members.push(member);

                // Recover by moving past one offending token unless that token
                // can itself start the next enum member. This keeps namelike
                // recovery tokens (`any`, `"hello"`, `1`) available to the next
                // iteration, matching `tsc`'s invalid-member AST.
                let starts_member = self.is_token(SyntaxKind::OpenBracketToken)
                    || self.is_token(SyntaxKind::StringLiteral)
                    || self.is_token(SyntaxKind::NumericLiteral)
                    || self.is_token(SyntaxKind::BigIntLiteral)
                    || self.is_token(SyntaxKind::PrivateIdentifier)
                    || self.is_identifier_or_keyword();
                if !starts_member {
                    self.next_token();
                }
                continue;
            }

            let initializer = if self.parse_optional(SyntaxKind::EqualsToken) {
                // An enum member initializer is its own container for the yield-grammar
                // check: tsc reports TS1163 for `enum E { A = yield x }` even inside a
                // `function*`, because the enclosing generator's yield context does not
                // reach it (the mirror of the `await`/enum-member own-container rule the
                // checker already owns via `check_await_expression_in_own_container`).
                // Clear the generator flag so a `yield` operand here parses outside a
                // generator body; restore it immediately after.
                let saved_flags = self.context_flags;
                self.context_flags &= !crate::parser::state::CONTEXT_FLAG_GENERATOR;
                let init = self.parse_assignment_expression();
                self.context_flags = saved_flags;
                init
            } else {
                NodeIndex::NONE
            };

            let end_pos = self.token_end();
            let member = self.arena.add_enum_member(
                syntax_kind_ext::ENUM_MEMBER,
                start_pos,
                end_pos,
                EnumMemberData { name, initializer },
            );
            members.push(member);

            // Parse comma or recover with missing comma
            if !self.parse_optional(SyntaxKind::CommaToken) {
                // Recovery: If the next token looks like the start of a valid enum member,
                // emit TS1357 and continue parsing instead of breaking.
                // tsc uses TS1357 (enum-specific) rather than generic TS1005 here.
                if self.is_token(SyntaxKind::Identifier)
                    || self.is_token(SyntaxKind::StringLiteral)
                    || self.is_token(SyntaxKind::PrivateIdentifier)
                    || self.is_token(SyntaxKind::OpenBracketToken)
                {
                    self.parse_error_at_current_token(
                        "An enum member name must be followed by a ',', '=', or '}'.",
                        diagnostic_codes::AN_ENUM_MEMBER_NAME_MUST_BE_FOLLOWED_BY_A_OR,
                    );
                    // Continue to next iteration to parse the next member
                    continue;
                }
                break;
            }
        }

        Self::make_node_list(members)
    }
}
