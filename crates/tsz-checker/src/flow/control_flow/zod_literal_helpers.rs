//! Flow-narrowing recovery for the Zod `arrayToEnum` mapped-enum shape.
//!
//! See [`crate::symbols_domain::name_text`] for the shared, single-source-of-truth
//! syntactic recovery primitives (tracked by #13045). This module only adapts
//! them to the flow analyzer's accessors and member-name lookup.

use super::FlowAnalyzer;
use crate::symbols_domain::name_text::array_to_enum_call_literal_names;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> FlowAnalyzer<'a> {
    pub(crate) fn array_to_enum_member_literal_type(
        &self,
        initializer: NodeIndex,
        property_name_node: NodeIndex,
    ) -> Option<TypeId> {
        let property_name = self.property_name_text(property_name_node)?;
        let initializer = self.skip_parens_and_assertions(initializer);
        let names = array_to_enum_call_literal_names(self.arena, initializer)?;
        names
            .into_iter()
            .find(|name| *name == property_name)
            .map(|name| self.interner.literal_string(&name))
    }

    fn property_name_text(&self, name: NodeIndex) -> Option<String> {
        let name = self.skip_parens_and_assertions(name);
        let node = self.arena.get(name)?;
        if let Some(ident) = self.arena.get_identifier(node) {
            return Some(ident.escaped_text.to_string());
        }
        if node.kind == SyntaxKind::StringLiteral as u16
            || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            || node.kind == SyntaxKind::NumericLiteral as u16
        {
            return self.arena.get_literal(node).map(|lit| lit.text.clone());
        }
        None
    }
}
