use super::super::ExtendedUnicodeStringLiteral;
use super::{Parser, literals::unquote, scan_at};
use crate::syntax::Token;

impl Parser<'_> {
    pub(super) fn cooked_string_literal(&self, token: Token) -> Option<super::super::Utf16String> {
        let literals = &self.cooked_string_literals;
        scan_at(literals, token.span, |literal| literal.span).map(|literal| literal.cooked.clone())
    }

    pub(super) fn ordinary_string_literal_value(&self, token: Token) -> String {
        let cooked = self
            .cooked_string_literal(token)
            .and_then(|cooked| cooked.as_string());
        cooked.unwrap_or_else(|| unquote(self.text(token.span)))
    }

    pub(super) fn extended_unicode_string_literal(
        &self,
        token: Token,
    ) -> Option<ExtendedUnicodeStringLiteral> {
        let literals = &self.string_literals;
        scan_at(literals, token.span, |literal| literal.span)
            .map(|literal| literal.syntax_literal())
    }
}
