use super::cross_file_direct_alias_chain::SourceFileAliasProofContext;
use crate::state::CheckerState;
use tsz_binder::{BinderState, symbol_flags};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;

impl<'a> CheckerState<'a> {
    pub(super) fn source_file_type_query_targets_direct_lowerable_value<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        node_idx: NodeIndex,
        proof: &SourceFileAliasProofContext<'b>,
    ) -> bool {
        let Some(type_query) = arena
            .get(node_idx)
            .and_then(|node| arena.get_type_query(node))
        else {
            return false;
        };
        let Some(name) = arena
            .get(type_query.expr_name)
            .and_then(|expr| arena.get_identifier(expr))
            .map(|ident| ident.escaped_text.as_str())
        else {
            return false;
        };
        let Some(raw_sym_id) = binder.file_locals.get(name) else {
            return (proof.global_value_is_lowerable)(binder, name);
        };
        let Some(resolved) =
            Self::source_file_resolve_alias_symbol_for_lowering(arena, binder, raw_sym_id, proof)
        else {
            return false;
        };
        resolved
            .binder
            .get_symbol(resolved.sym_id)
            .is_some_and(|symbol| symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS) != 0)
    }
}
