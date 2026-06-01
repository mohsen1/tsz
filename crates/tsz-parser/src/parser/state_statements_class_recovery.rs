//! Parser state - class member recovery helpers.

use super::state::ParserState;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_scanner::SyntaxKind;

impl ParserState {
    pub(crate) fn recover_module_like_class_member_as_outer_statement(&mut self) -> bool {
        if !matches!(
            self.token(),
            SyntaxKind::GlobalKeyword | SyntaxKind::NamespaceKeyword | SyntaxKind::ModuleKeyword
        ) || !self.look_ahead_is_module_declaration()
        {
            return false;
        }
        self.parse_error_at_current_token(
            "Unexpected token. A constructor, method, accessor, or property was expected.",
            diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
        );
        let snapshot = self.scanner.save_state();
        let current_token = self.current_token;
        self.next_token();
        if !self.scanner.has_preceding_line_break()
            && self.is_identifier_or_keyword()
            && self.should_report_error()
        {
            self.error_token_expected(";");
        }
        self.scanner.restore_state(snapshot);
        self.current_token = current_token;
        self.suppress_next_missing_class_close_brace_error_once = true;
        true
    }

    pub(crate) fn recover_invalid_module_like_class_member(&mut self) {
        self.parse_error_at_current_token(
            "Unexpected token. A constructor, method, accessor, or property was expected.",
            diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
        );
        self.next_token();

        if !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
            && !self.scanner.has_preceding_line_break()
        {
            self.error_token_expected(";");

            while !self.is_token(SyntaxKind::CloseBraceToken)
                && !self.is_token(SyntaxKind::EndOfFileToken)
                && !self.scanner.has_preceding_line_break()
            {
                self.next_token();
            }
        }

        if self.is_token(SyntaxKind::CloseBraceToken) {
            self.parse_error_at_current_token(
                "Declaration or statement expected.",
                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            );
        }
    }
}
