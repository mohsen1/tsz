use super::super::ExtendedUnicodeStringLiteral;
use super::{Parser, literals::unquote};
use crate::syntax::Token;

impl Parser<'_> {
    pub(super) fn cooked_string_literal(&self, token: Token) -> Option<super::super::Utf16String> {
        let index = self
            .cooked_string_literals
            .binary_search_by_key(&token.span.start, |literal| literal.span.start)
            .ok()?;
        let scanned = &self.cooked_string_literals[index];
        (scanned.span == token.span).then(|| scanned.cooked.clone())
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
        let index = self
            .string_literals
            .binary_search_by_key(&token.span.start, |literal| literal.span.start)
            .ok()?;
        let scanned = &self.string_literals[index];
        (scanned.span == token.span).then(|| scanned.syntax_literal())
    }
}
