//! TS1539: a literal (non-computed) `bigint` property name.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    /// TS1539: a literal (non-computed) `bigint` name cannot be used as a
    /// property name.
    ///
    /// Fires unconditionally for property-shaped members — interface/type-literal
    /// property signatures, class property declarations, and object-literal
    /// property assignments — regardless of `readonly`, `static`, `declare`,
    /// optionality, or decorators. Never fires on the method-shaped equivalent
    /// (methods, `get`/`set` accessors) in any of those containers, so callers
    /// must gate on the member being a property, not a method or accessor.
    pub(crate) fn check_bigint_literal_property_name(&mut self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != SyntaxKind::BigIntLiteral as u16 {
            return false;
        }
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        self.error_at_node(
            name_idx,
            diagnostic_messages::A_BIGINT_LITERAL_CANNOT_BE_USED_AS_A_PROPERTY_NAME,
            diagnostic_codes::A_BIGINT_LITERAL_CANNOT_BE_USED_AS_A_PROPERTY_NAME,
        );
        true
    }
}
