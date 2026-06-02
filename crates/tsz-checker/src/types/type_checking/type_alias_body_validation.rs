//! Helpers for type-alias body validation that stay off the hot lowering path.

use crate::state::CheckerState;
use crate::state_type_analysis::cross_file_direct::is_builtin_lib_declaration_arena;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    pub(crate) fn check_explicit_type_reference_for_alias_body_validation(
        &mut self,
        ref_idx: NodeIndex,
        nested_in_type_literal: bool,
    ) -> bool {
        if nested_in_type_literal
            || self.is_inside_type_parameter_declaration(ref_idx)
            || !self.type_reference_is_in_type_alias_body(ref_idx)
            || is_builtin_lib_declaration_arena(self.ctx.arena)
        {
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

    fn type_reference_is_in_type_alias_body(&self, ref_idx: NodeIndex) -> bool {
        let mut current = ref_idx;
        while current.is_some() {
            let Some(parent_idx) = self
                .ctx
                .arena
                .get_extended(current)
                .map(|extended| extended.parent)
            else {
                return false;
            };
            if parent_idx.is_none() {
                return false;
            }
            let Some(parent) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            if parent.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                return true;
            }
            current = parent_idx;
        }
        false
    }
}
