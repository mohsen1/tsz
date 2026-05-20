//! Small duplicate-identifier classifiers shared by large checker modules.

use super::duplicate_identifiers::DuplicateDeclarationOrigin;
use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    /// Does any cross-file `TargetedModuleAugmentation` declaration contribute a
    /// non-block-scoped kind that should still select TS2300? Once a real
    /// block-scoped declaration participates, tsc keeps TS2451 for value
    /// declarations such as functions/classes/vars and reserves TS2300 for
    /// synthetic CommonJS object-property export conflicts.
    pub(super) fn targeted_aug_should_force_ts2300(
        &self,
        declarations: &[(NodeIndex, u32, bool, bool, DuplicateDeclarationOrigin)],
    ) -> bool {
        let has_block_scoped = declarations
            .iter()
            .any(|(_, flags, _, _, _)| (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0);

        declarations.iter().any(|(_, flags, is_local, _, origin)| {
            !*is_local
                && origin.is_targeted_module_augmentation()
                && (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) == 0
                && (!has_block_scoped || (flags & symbol_flags::PROPERTY) != 0)
        })
    }

    pub(super) fn local_augmentation_decl_symbol_id(
        &self,
        node: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        self.ctx
            .binder
            .node_symbols
            .get(&node.0)
            .copied()
            .or_else(|| {
                self.get_declaration_name_node(node)
                    .and_then(|name_idx| self.ctx.binder.node_symbols.get(&name_idx.0))
                    .copied()
            })
    }
}
