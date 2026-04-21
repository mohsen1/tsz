//! Small duplicate-identifier classifiers shared by large checker modules.

use super::duplicate_identifiers::DuplicateDeclarationOrigin;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    pub(super) fn has_targeted_aug(
        &self,
        declarations: &[(NodeIndex, u32, bool, bool, DuplicateDeclarationOrigin)],
    ) -> bool {
        declarations.iter().any(|(_, _, is_local, _, origin)| {
            !*is_local && *origin == DuplicateDeclarationOrigin::TargetedModuleAugmentation
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
