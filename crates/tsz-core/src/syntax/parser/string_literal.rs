use super::super::{
    ExtendedUnicodeStringLiteral, Statement, comments_form_extended_unicode_string_safe_file,
    statements_form_extended_unicode_string_safe_file,
};
use super::{Parser, literals::unquote};
use crate::syntax::Token;

impl Parser<'_> {
    pub(super) fn ordinary_string_literal_value(&self, token: Token) -> String {
        let cooked = self
            .line_continuation_string_literals
            .binary_search_by_key(&token.span.start, |literal| literal.span.start)
            .ok()
            .and_then(|index| {
                let scanned = &self.line_continuation_string_literals[index];
                (scanned.span == token.span).then(|| scanned.cooked.clone())
            });
        cooked.unwrap_or_else(|| unquote(self.text(token.span)))
    }

    pub(super) fn extended_unicode_string_literal(
        &self,
        token: Token,
    ) -> Option<ExtendedUnicodeStringLiteral> {
        let index = self
            .string_literals
            .binary_search_by_key(&token.span.start, |literal| literal.span.start)
            .ok()?;
        let scanned = &self.string_literals[index];
        (scanned.span == token.span).then(|| scanned.syntax_literal())
    }

    pub(super) fn finish_extended_unicode_string_source(
        &mut self,
        statements: &[Statement],
    ) -> bool {
        let has_authored_string = !self.string_literals.is_empty();
        if !has_authored_string {
            return false;
        }
        let owned = match self.string_literals.as_slice() {
            [literal] => {
                literal.syntax_literal().validation_supported()
                    && literal.owns_all_diagnostics(&self.diagnostics)
                    && !self.has_unmodeled_trivia
                    && !self.has_unmodeled_top_level_syntax
                    && comments_form_extended_unicode_string_safe_file(
                        self.source,
                        statements,
                        &self.comments,
                    )
                    && statements_form_extended_unicode_string_safe_file(self.source, statements, 1)
            }
            _ => false,
        };
        if !owned {
            self.product_capabilities
                .observe_unmodeled_extended_unicode_string();
        }
        true
    }
}
