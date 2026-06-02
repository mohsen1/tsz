//! Helpers for type-alias body validation that stay off the hot lowering path.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    pub(crate) fn check_explicit_type_reference_for_alias_body_validation(
        &mut self,
        ref_idx: NodeIndex,
        nested_in_type_literal: bool,
    ) -> bool {
        if nested_in_type_literal || self.is_inside_type_parameter_declaration(ref_idx) {
            return false;
        }
        let Some(node) = self.ctx.arena.get(ref_idx) else {
            return false;
        };
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return false;
        };
        let Some(args) = type_ref
            .type_arguments
            .clone()
            .filter(|args| !args.nodes.is_empty())
        else {
            return false;
        };
        let Some(raw) = self.resolve_type_symbol_for_lowering(type_ref.type_name) else {
            return false;
        };

        self.validate_type_reference_type_arguments(tsz_binder::SymbolId(raw), &args, ref_idx);
        true
    }
}
