//! Recovery logic for malformed `let [<non-binding-start>]` statement patterns.

use super::state::ParserState;
use crate::parser::parse_rules::is_identifier_or_keyword;
use crate::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

// ---------------------------------------------------------------------------
// Structural predicate (pure, no parser state needed)
// ---------------------------------------------------------------------------

/// Returns `true` when `let [<first_elem>` cannot begin a valid destructuring.
pub(super) fn is_invalid_let_array_start(next: SyntaxKind, first_elem: SyntaxKind) -> bool {
    if next != SyntaxKind::OpenBracketToken {
        return false;
    }
    let recoverable = matches!(
        first_elem,
        SyntaxKind::CloseBracketToken
            | SyntaxKind::CommaToken
            | SyntaxKind::DotDotDotToken
            | SyntaxKind::OpenBraceToken
            | SyntaxKind::OpenBracketToken
    ) || (is_identifier_or_keyword(first_elem)
        && !tsz_scanner::token_is_reserved_word(first_elem));
    !recoverable
}

// ---------------------------------------------------------------------------
// Recovery implementation on ParserState
// ---------------------------------------------------------------------------

impl ParserState {
    /// Returns `Some(empty_statement)` when `let [<non-binding>` is detected and recovered;
    /// returns `None` to let normal variable-declaration parsing proceed.
    pub(crate) fn try_parse_invalid_let_array_declaration_statement(
        &mut self,
    ) -> Option<NodeIndex> {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        let next = self.scanner.scan();
        let first_elem = if next == SyntaxKind::OpenBracketToken {
            self.scanner.scan()
        } else {
            SyntaxKind::Unknown
        };

        self.scanner.restore_state(snapshot);
        self.current_token = current;

        if !is_invalid_let_array_start(next, first_elem) {
            return None;
        }

        let start_pos = self.token_pos();
        self.consume_keyword();
        self.parse_expected(SyntaxKind::OpenBracketToken);
        self.error_array_element_destructuring_pattern_expected();

        // Advance past the bad first element unless we're already at a statement boundary.
        if !matches!(
            self.token(),
            SyntaxKind::CloseBracketToken | SyntaxKind::SemicolonToken | SyntaxKind::EndOfFileToken
        ) {
            self.next_token();
        }

        // `]` seen after the bad element: tsc emits "';' expected" instead of a bracket error.
        if self.is_token(SyntaxKind::CloseBracketToken) {
            self.parse_error_at_current_token(
                "';' expected.",
                tsz_common::diagnostics::diagnostic_codes::EXPECTED,
            );
            self.next_token();
        }

        // `=` after `let [bad]` looks like an assignment target: skip to next boundary.
        if self.is_token(SyntaxKind::EqualsToken) {
            self.parse_error_at_current_token(
                "Declaration or statement expected.",
                tsz_common::diagnostics::diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            );
            self.next_token();
            while !matches!(
                self.token(),
                SyntaxKind::SemicolonToken | SyntaxKind::EndOfFileToken
            ) {
                self.next_token();
            }
        }

        if self.is_token(SyntaxKind::SemicolonToken) {
            self.next_token();
        }

        let end_pos = self.token_end();
        Some(
            self.arena
                .add_token(syntax_kind_ext::EMPTY_STATEMENT, start_pos, end_pos),
        )
    }
}

// ---------------------------------------------------------------------------
// Unit tests for the structural predicate
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::is_invalid_let_array_start;
    use tsz_scanner::SyntaxKind;

    #[test]
    fn non_bracket_next_is_never_invalid() {
        for next in [
            SyntaxKind::OpenBraceToken,
            SyntaxKind::Identifier,
            SyntaxKind::Unknown,
        ] {
            assert!(
                !is_invalid_let_array_start(next, SyntaxKind::NumericLiteral),
                "next={next:?} should not trigger the invalid-let-array predicate",
            );
        }
    }

    #[test]
    fn close_bracket_first_elem_is_recoverable() {
        assert!(!is_invalid_let_array_start(
            SyntaxKind::OpenBracketToken,
            SyntaxKind::CloseBracketToken,
        ));
    }

    #[test]
    fn comma_first_elem_is_recoverable() {
        assert!(!is_invalid_let_array_start(
            SyntaxKind::OpenBracketToken,
            SyntaxKind::CommaToken,
        ));
    }

    #[test]
    fn dot_dot_dot_first_elem_is_recoverable() {
        assert!(!is_invalid_let_array_start(
            SyntaxKind::OpenBracketToken,
            SyntaxKind::DotDotDotToken,
        ));
    }

    #[test]
    fn open_brace_first_elem_is_recoverable() {
        assert!(!is_invalid_let_array_start(
            SyntaxKind::OpenBracketToken,
            SyntaxKind::OpenBraceToken,
        ));
    }

    #[test]
    fn open_bracket_first_elem_is_recoverable() {
        assert!(!is_invalid_let_array_start(
            SyntaxKind::OpenBracketToken,
            SyntaxKind::OpenBracketToken,
        ));
    }

    #[test]
    fn identifier_first_elem_is_recoverable() {
        assert!(!is_invalid_let_array_start(
            SyntaxKind::OpenBracketToken,
            SyntaxKind::Identifier,
        ));
    }

    #[test]
    fn reserved_word_first_elem_is_invalid() {
        assert!(is_invalid_let_array_start(
            SyntaxKind::OpenBracketToken,
            SyntaxKind::WhileKeyword,
        ));
    }

    #[test]
    fn for_keyword_first_elem_is_invalid() {
        assert!(is_invalid_let_array_start(
            SyntaxKind::OpenBracketToken,
            SyntaxKind::ForKeyword,
        ));
    }

    #[test]
    fn numeric_literal_first_elem_is_invalid() {
        assert!(is_invalid_let_array_start(
            SyntaxKind::OpenBracketToken,
            SyntaxKind::NumericLiteral,
        ));
    }

    #[test]
    fn string_literal_first_elem_is_invalid() {
        assert!(is_invalid_let_array_start(
            SyntaxKind::OpenBracketToken,
            SyntaxKind::StringLiteral,
        ));
    }

    #[test]
    fn plus_token_first_elem_is_invalid() {
        assert!(is_invalid_let_array_start(
            SyntaxKind::OpenBracketToken,
            SyntaxKind::PlusToken,
        ));
    }
}
