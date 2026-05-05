//! Recovery for reads from object literals that are still being initialized.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn partial_object_literal_initializer_property_type(
        &self,
        expression: NodeIndex,
        name_or_argument: NodeIndex,
    ) -> Option<TypeId> {
        let expr_node = self.ctx.arena.get(expression)?;
        if expr_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let property_name = self
            .ctx
            .arena
            .get_identifier_at(name_or_argument)
            .map(|ident| ident.escaped_text.as_str())?;
        let variable_symbol = self.resolve_identifier_symbol_without_tracking(expression)?;
        let property_atom = self.ctx.types.intern_string(property_name);
        self.ctx
            .object_literal_tracking
            .partial_initializers
            .iter()
            .rev()
            .find(|active| {
                active.variable_symbol == variable_symbol
                    && active.properties.contains_key(&property_atom)
            })
            .and_then(|active| active.properties.get(&property_atom))
            .map(|prop| prop.type_id)
    }
}
