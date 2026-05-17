//! Direct source-file function declaration fast paths.

use crate::state::CheckerState;
use tsz_binder::{BinderState, SymbolId, symbol_flags};
use tsz_lowering::TypeLowering;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

use super::cross_file_direct_files::is_direct_lowering_source_file_arena;

impl<'a> CheckerState<'a> {
    pub(super) fn direct_source_file_function_declaration_type(
        &mut self,
        sym_id: SymbolId,
        delegate_binder: &BinderState,
        symbol_arena: &NodeArena,
        allow_source_file_arena: bool,
    ) -> Option<TypeId> {
        if !allow_source_file_arena || !is_direct_lowering_source_file_arena(symbol_arena) {
            return None;
        }
        let symbol = delegate_binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::FUNCTION == 0
            || symbol.flags & (symbol_flags::MODULE | symbol_flags::ALIAS) != 0
            || symbol.declarations.len() != 1
        {
            return None;
        }

        let decl_idx = symbol.declarations[0];
        let decl_node = symbol_arena.get(decl_idx)?;
        let function = symbol_arena.get_function(decl_node)?;
        if decl_node.kind != syntax_kind_ext::FUNCTION_DECLARATION
            || function.type_annotation == NodeIndex::NONE
            || function.parameters.nodes.iter().copied().any(|param_idx| {
                symbol_arena
                    .get(param_idx)
                    .and_then(|param_node| symbol_arena.get_parameter(param_node))
                    .is_none_or(|param| param.type_annotation == NodeIndex::NONE)
            })
        {
            return None;
        }

        let name_resolver = |type_name: &str| -> Option<tsz_solver::def::DefId> {
            self.source_file_local_name_def_id_for_lowering(
                delegate_binder,
                symbol_arena,
                type_name,
            )
        };
        let no_type_symbol = |_node_idx: NodeIndex| -> Option<u32> { None };
        let no_def_id = |_node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> { None };
        let no_value_symbol = |_node_idx: NodeIndex| -> Option<u32> { None };
        let lowering = TypeLowering::with_hybrid_resolver(
            symbol_arena,
            self.ctx.types,
            &no_type_symbol,
            &no_def_id,
            &no_value_symbol,
        )
        .with_builtin_iterator_return_type(self.builtin_iterator_return_intrinsic_type())
        .with_name_def_id_resolver(&name_resolver)
        .prefer_name_def_id_resolution();
        let ty = lowering.lower_signature_from_declaration(decl_idx, None);
        (ty != TypeId::UNKNOWN && ty != TypeId::ERROR).then_some(ty)
    }

    pub(super) fn direct_source_file_function_declaration_result(
        &mut self,
        sym_id: SymbolId,
        direct_target: Option<(&NodeArena, &BinderState, Option<usize>)>,
        allow_source_file_arena: bool,
    ) -> Option<TypeId> {
        let (symbol_arena, delegate_binder, _) = direct_target?;
        self.direct_source_file_function_declaration_type(
            sym_id,
            delegate_binder,
            symbol_arena,
            allow_source_file_arena,
        )
    }
}

#[cfg(test)]
#[path = "cross_file_direct_functions_tests.rs"]
mod tests;
