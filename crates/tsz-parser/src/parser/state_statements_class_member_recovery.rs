//! Parser state - class member recovery helpers.

use super::state::ParserState;
use crate::parser::{NodeIndex, syntax_kind_ext};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_scanner::SyntaxKind;

impl ParserState {
    pub(crate) fn parse_optional_class_member_semicolon(&mut self, member: NodeIndex) -> bool {
        let is_semi_element = self
            .arena
            .get(member)
            .is_some_and(|node| node.kind == syntax_kind_ext::SEMICOLON_CLASS_ELEMENT);
        !is_semi_element && self.parse_optional(SyntaxKind::SemicolonToken)
    }

    pub(crate) fn recover_missing_semicolon_between_property_members(
        &mut self,
        member: NodeIndex,
        consumed_semicolon: bool,
    ) {
        if consumed_semicolon || self.scanner.has_preceding_line_break() || !self.is_property_name()
        {
            return;
        }
        if self
            .arena
            .get(member)
            .and_then(|node| self.arena.get_property_decl(node))
            .is_some()
        {
            self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
        }
    }
}
