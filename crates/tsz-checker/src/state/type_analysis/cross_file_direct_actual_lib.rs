use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::parser::syntax_kind_ext;

use super::cross_file_direct_files::is_direct_actual_lib_declaration_arena;

impl<'a> CheckerState<'a> {
    pub(super) fn symbol_is_actual_lib_namespace_export(
        &self,
        namespace: &str,
        export_name: &str,
        sym_id: SymbolId,
    ) -> bool {
        self.resolve_lib_namespace_export_symbol(namespace, export_name)
            .is_some_and(|export_sym_id| export_sym_id == sym_id)
    }

    pub(super) fn symbol_is_proven_direct_actual_lib_value_interface(
        &self,
        sym_id: SymbolId,
        symbol: &tsz_binder::Symbol,
        name: &str,
    ) -> bool {
        symbol.has_any_flags(symbol_flags::VALUE | symbol_flags::INTERFACE)
            && self.symbol_declarations_are_direct_actual_lib_only(sym_id, symbol, name)
    }

    pub(super) fn symbol_has_direct_actual_lib_interface_type_parameters(
        &self,
        sym_id: SymbolId,
        symbol: &tsz_binder::Symbol,
    ) -> bool {
        symbol.has_any_flags(symbol_flags::INTERFACE)
            && symbol.declarations.iter().any(|&decl_idx| {
                self.ctx
                    .binder
                    .declaration_arenas
                    .get(&(sym_id, decl_idx))
                    .is_some_and(|arenas| {
                        arenas.iter().any(|arena| {
                            Self::direct_actual_lib_interface_has_type_parameters(
                                arena.as_ref(),
                                decl_idx,
                            )
                        })
                    })
            })
    }

    fn direct_actual_lib_interface_has_type_parameters(
        arena: &NodeArena,
        decl_idx: NodeIndex,
    ) -> bool {
        is_direct_actual_lib_declaration_arena(arena)
            && arena
                .get(decl_idx)
                .and_then(|node| arena.get_interface(node))
                .and_then(|interface| interface.type_parameters.as_ref())
                .is_some_and(|params| !params.nodes.is_empty())
    }

    pub(super) fn symbol_has_direct_actual_lib_iterator_object_heritage(
        &self,
        sym_id: SymbolId,
        symbol: &tsz_binder::Symbol,
    ) -> bool {
        symbol.declarations.iter().any(|&decl_idx| {
            self.ctx
                .binder
                .declaration_arenas
                .get(&(sym_id, decl_idx))
                .is_some_and(|arenas| {
                    arenas.iter().any(|arena| {
                        Self::direct_actual_lib_interface_has_iterator_object_heritage(
                            arena.as_ref(),
                            decl_idx,
                        )
                    })
                })
        })
    }

    fn direct_actual_lib_interface_has_iterator_object_heritage(
        arena: &NodeArena,
        decl_idx: NodeIndex,
    ) -> bool {
        if !is_direct_actual_lib_declaration_arena(arena) {
            return false;
        }
        let Some(interface) = arena
            .get(decl_idx)
            .and_then(|node| arena.get_interface(node))
        else {
            return false;
        };
        let Some(heritage_clauses) = interface.heritage_clauses.as_ref() else {
            return false;
        };
        heritage_clauses.nodes.iter().copied().any(|clause_idx| {
            let Some(clause) = arena
                .get(clause_idx)
                .and_then(|node| arena.get_heritage_clause(node))
            else {
                return false;
            };
            clause.types.nodes.iter().copied().any(|type_idx| {
                let Some(expr) = arena
                    .get(type_idx)
                    .and_then(|node| arena.get_expr_type_args(node))
                else {
                    return false;
                };
                arena.get_identifier_text(expr.expression) == Some("IteratorObject")
            })
        })
    }

    pub(super) fn symbol_declares_direct_actual_lib_protocol_method(
        &self,
        sym_id: SymbolId,
        symbol: &tsz_binder::Symbol,
        delegate_arena: &NodeArena,
    ) -> bool {
        if !symbol.has_any_flags(symbol_flags::INTERFACE) {
            return false;
        }

        symbol.declarations.iter().any(|&decl_idx| {
            if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx))
                && arenas.iter().any(|arena| {
                    Self::direct_actual_lib_interface_declares_protocol_method(
                        arena.as_ref(),
                        decl_idx,
                    )
                })
            {
                return true;
            }

            Self::direct_actual_lib_interface_declares_protocol_method(delegate_arena, decl_idx)
        }) || self.actual_lib_context_declares_protocol_method(symbol.escaped_name.as_str())
    }

    fn actual_lib_context_declares_protocol_method(&self, name: &str) -> bool {
        self.ctx
            .lib_contexts
            .iter()
            .take(self.ctx.actual_lib_file_count)
            .any(|lib_ctx| {
                let Some(sym_id) = lib_ctx.binder.file_locals.get(name) else {
                    return false;
                };
                let Some(symbol) = lib_ctx.binder.get_symbol(sym_id) else {
                    return false;
                };
                if !symbol.has_any_flags(symbol_flags::INTERFACE) {
                    return false;
                }

                symbol.declarations.iter().any(|&decl_idx| {
                    lib_ctx
                        .binder
                        .declaration_arenas
                        .get(&(sym_id, decl_idx))
                        .is_some_and(|arenas| {
                            arenas.iter().any(|arena| {
                                Self::direct_actual_lib_interface_declares_protocol_method(
                                    arena.as_ref(),
                                    decl_idx,
                                )
                            })
                        })
                        || Self::direct_actual_lib_interface_declares_protocol_method(
                            lib_ctx.arena.as_ref(),
                            decl_idx,
                        )
                })
            })
    }

    fn direct_actual_lib_interface_declares_protocol_method(
        arena: &NodeArena,
        decl_idx: NodeIndex,
    ) -> bool {
        if !is_direct_actual_lib_declaration_arena(arena) {
            return false;
        }
        let Some(interface) = arena
            .get(decl_idx)
            .and_then(|node| arena.get_interface(node))
        else {
            return false;
        };

        interface.members.nodes.iter().copied().any(|member_idx| {
            let Some(member_node) = arena.get(member_idx) else {
                return false;
            };
            if member_node.kind != syntax_kind_ext::METHOD_SIGNATURE {
                return false;
            }
            let Some(signature) = arena.get_signature(member_node) else {
                return false;
            };
            arena
                .get_identifier_text(signature.name)
                .is_some_and(|name| matches!(name, "next" | "then"))
        })
    }

    pub(super) fn symbol_declarations_are_direct_actual_lib_only(
        &self,
        sym_id: SymbolId,
        symbol: &tsz_binder::Symbol,
        name: &str,
    ) -> bool {
        !symbol.declarations.is_empty()
            && symbol.declarations.iter().all(|&decl_idx| {
                self.ctx
                    .binder
                    .declaration_arenas
                    .get(&(sym_id, decl_idx))
                    .is_some_and(|arenas| {
                        !arenas.is_empty()
                            && arenas.iter().all(|arena| {
                                is_direct_actual_lib_declaration_arena(arena.as_ref())
                                    && Self::lib_declaration_name_matches(
                                        arena.as_ref(),
                                        decl_idx,
                                        name,
                                    )
                            })
                    })
            })
    }

    pub(super) fn symbol_type_alias_declarations_are_proven_actual_lib_only(
        &self,
        sym_id: SymbolId,
        symbol: &tsz_binder::Symbol,
        name: &str,
        delegate_arena: &NodeArena,
    ) -> bool {
        !symbol.declarations.is_empty()
            && symbol.declarations.iter().all(|&decl_idx| {
                if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                    return !arenas.is_empty()
                        && arenas.iter().all(|arena| {
                            is_direct_actual_lib_declaration_arena(arena.as_ref())
                                && Self::lib_type_alias_declaration_name_matches(
                                    arena.as_ref(),
                                    decl_idx,
                                    name,
                                )
                        });
                }

                is_direct_actual_lib_declaration_arena(delegate_arena)
                    && Self::lib_type_alias_declaration_name_matches(delegate_arena, decl_idx, name)
            })
    }

    pub(super) fn lib_declaration_name_matches(
        arena: &NodeArena,
        decl_idx: NodeIndex,
        name: &str,
    ) -> bool {
        let Some(node) = arena.get(decl_idx) else {
            return false;
        };
        let name_node = arena
            .get_interface(node)
            .map(|decl| decl.name)
            .or_else(|| arena.get_type_alias(node).map(|decl| decl.name))
            .or_else(|| arena.get_class(node).map(|decl| decl.name))
            .or_else(|| arena.get_function(node).map(|decl| decl.name))
            .or_else(|| arena.get_enum(node).map(|decl| decl.name))
            .or_else(|| arena.get_module(node).map(|decl| decl.name))
            .or_else(|| arena.get_variable_declaration(node).map(|decl| decl.name));
        name_node.is_some_and(|name_node| {
            arena
                .get(name_node)
                .and_then(|name_node| arena.get_identifier(name_node))
                .is_some_and(|ident| ident.escaped_text == name)
        })
    }

    pub(super) fn lib_type_alias_declaration_name_matches(
        arena: &NodeArena,
        decl_idx: NodeIndex,
        name: &str,
    ) -> bool {
        let Some(node) = arena.get(decl_idx) else {
            return false;
        };
        let Some(alias) = arena.get_type_alias(node) else {
            return false;
        };
        arena
            .get(alias.name)
            .and_then(|name_node| arena.get_identifier(name_node))
            .is_some_and(|ident| ident.escaped_text == name)
    }
}
