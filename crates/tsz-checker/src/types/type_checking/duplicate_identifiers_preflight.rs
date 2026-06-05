//! Preflight helpers for duplicate identifier checking.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

use super::DuplicateDeclarationOrigin;

impl DuplicateDeclarationOrigin {
    pub(crate) const fn is_targeted_module_augmentation(self) -> bool {
        matches!(
            self,
            DuplicateDeclarationOrigin::TargetedModuleAugmentation
                | DuplicateDeclarationOrigin::CurrentFileAugmentationTargetExport(_)
        )
    }
}

impl<'a> CheckerState<'a> {
    pub(crate) fn is_global_symbol_constructor_interface_group(
        &self,
        scope: tsz_binder::SymbolId,
        declarations: &[NodeIndex],
    ) -> bool {
        if scope != tsz_binder::SymbolId::NONE || self.ctx.binder.is_external_module() {
            return false;
        }

        declarations.iter().all(|&decl_idx| {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                return false;
            };
            let Some(interface) = self.ctx.arena.get_interface(node) else {
                return false;
            };
            self.ctx
                .arena
                .get(interface.name)
                .and_then(|name| self.ctx.arena.get_identifier(name))
                .is_some_and(|ident| ident.escaped_text == "SymbolConstructor")
        })
    }

    pub(crate) fn is_symbol_constructor_symbol_refinement_pair(
        &self,
        left: TypeId,
        right: TypeId,
    ) -> bool {
        (left == TypeId::SYMBOL
            && crate::query_boundaries::common::is_unique_symbol_type(self.ctx.types, right))
            || (right == TypeId::SYMBOL
                && crate::query_boundaries::common::is_unique_symbol_type(self.ctx.types, left))
    }

    pub(crate) fn function_decl_has_body_for_duplicate_symbol(
        &self,
        sym_id: tsz_binder::SymbolId,
        decl_idx: NodeIndex,
        is_local: bool,
    ) -> bool {
        use tsz_parser::parser::{NodeArena, syntax_kind_ext};

        fn function_has_body_in_arena(arena: &NodeArena, decl_idx: NodeIndex) -> bool {
            arena
                .get(decl_idx)
                .filter(|node| node.kind == syntax_kind_ext::FUNCTION_DECLARATION)
                .and_then(|node| arena.get_function(node))
                .is_some_and(|func| func.body.is_some())
        }

        fn function_matches_name_and_has_body(
            arena: &NodeArena,
            decl_idx: NodeIndex,
            name: &str,
        ) -> bool {
            let Some(node) = arena.get(decl_idx) else {
                return false;
            };
            if node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
                return false;
            }
            arena.get_function(node).is_some_and(|func| {
                func.body.is_some()
                    && arena
                        .get_identifier_at(func.name)
                        .is_some_and(|ident| ident.escaped_text == name)
            })
        }

        if is_local {
            return function_has_body_in_arena(self.ctx.arena, decl_idx);
        }

        if self
            .ctx
            .binder
            .declaration_arenas
            .get(&(sym_id, decl_idx))
            .is_some_and(|arenas| {
                arenas.iter().any(|arena| {
                    let arena: &NodeArena = arena;
                    !std::ptr::eq(arena, self.ctx.arena)
                        && function_has_body_in_arena(arena, decl_idx)
                })
            })
        {
            return true;
        }

        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        self.ctx.all_arenas.as_ref().is_some_and(|all_arenas| {
            all_arenas.iter().enumerate().any(|(file_idx, arena)| {
                file_idx != self.ctx.current_file_idx
                    && function_matches_name_and_has_body(
                        arena.as_ref(),
                        decl_idx,
                        &symbol.escaped_name,
                    )
            })
        })
    }

    pub(crate) fn current_file_has_named_default_export_identifier(&self) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        fn statement_has_named_default_export(
            state: &CheckerState<'_>,
            stmt_idx: NodeIndex,
            depth: u8,
        ) -> bool {
            if depth > 12 {
                return false;
            }
            let Some(stmt_node) = state.ctx.arena.get(stmt_idx) else {
                return false;
            };

            if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                return state
                    .ctx
                    .arena
                    .get_export_decl(stmt_node)
                    .is_some_and(|export_decl| {
                        export_decl.is_default_export
                            && state
                                .ctx
                                .arena
                                .get_identifier_at(export_decl.export_clause)
                                .is_some()
                    });
            }

            if stmt_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT {
                return state
                    .ctx
                    .arena
                    .get_export_assignment(stmt_node)
                    .is_some_and(|export_assign| {
                        !export_assign.is_export_equals
                            && state
                                .ctx
                                .arena
                                .get_identifier_at(export_assign.expression)
                                .is_some()
                    });
            }

            if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                let Some(module_decl) = state.ctx.arena.get_module(stmt_node) else {
                    return false;
                };
                let Some(body_node) = state.ctx.arena.get(module_decl.body) else {
                    return false;
                };
                if body_node.kind == syntax_kind_ext::MODULE_BLOCK {
                    let Some(block) = state.ctx.arena.get_module_block(body_node) else {
                        return false;
                    };
                    return block.statements.as_ref().is_some_and(|statements| {
                        statements.nodes.iter().any(|&inner_idx| {
                            statement_has_named_default_export(state, inner_idx, depth + 1)
                        })
                    });
                }
                if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                    return statement_has_named_default_export(state, module_decl.body, depth + 1);
                }
            }

            false
        }

        self.ctx
            .arena
            .source_files
            .first()
            .is_some_and(|source_file| {
                source_file
                    .statements
                    .nodes
                    .iter()
                    .any(|&stmt_idx| statement_has_named_default_export(self, stmt_idx, 0))
            })
    }
}
