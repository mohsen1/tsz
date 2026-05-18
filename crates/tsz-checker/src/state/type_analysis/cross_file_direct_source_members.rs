//! Direct source-file interface member admission for import-aware annotations.

use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use tsz_binder::{BinderState, SymbolId, symbol_flags};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;
use tsz_solver::def::DefId;

use super::cross_file_direct_files::is_direct_actual_lib_alias_body_admitted;

#[derive(Clone, Copy, Eq, PartialEq)]
struct SourceSymbolKey {
    file_idx: usize,
    sym_id: SymbolId,
}

impl<'a> CheckerState<'a> {
    pub(super) fn prepare_direct_source_file_interface_declarations_for_lowering(
        &mut self,
        declarations: &[(NodeIndex, &NodeArena)],
        delegate_binder: &BinderState,
        symbol_arena: &NodeArena,
    ) -> Option<FxHashMap<NodeIndex, TypeId>> {
        if Self::source_file_interface_declarations_are_direct_lowerable(
            declarations,
            delegate_binder,
        ) {
            return Some(FxHashMap::default());
        }

        let source_file_idx = self.source_file_idx_for_direct_arena(symbol_arena)?;
        let mut seen = Vec::new();
        let mut type_query_overrides = FxHashMap::default();
        for (decl_idx, arena) in declarations {
            if !std::ptr::eq(*arena, symbol_arena) {
                return None;
            }
            let node = arena.get(*decl_idx)?;
            let interface = arena.get_interface(node)?;
            if interface
                .type_parameters
                .as_ref()
                .is_some_and(|params| !params.nodes.is_empty())
            {
                return None;
            }
            let interface_name = arena
                .get(interface.name)
                .and_then(|name_node| arena.get_identifier(name_node))
                .map(|ident| ident.escaped_text.as_str())?;
            let interface_sym = delegate_binder.file_locals.get(interface_name)?;
            let key = SourceSymbolKey {
                file_idx: source_file_idx,
                sym_id: interface_sym,
            };
            if seen.contains(&key) {
                return None;
            }
            seen.push(key);
            let members_ok = interface.members.nodes.iter().copied().all(|member_idx| {
                self.source_file_interface_member_is_cross_file_direct_lowerable(
                    symbol_arena,
                    delegate_binder,
                    member_idx,
                    &mut seen,
                    &mut type_query_overrides,
                )
            });
            seen.pop();
            if !members_ok {
                return None;
            }
        }

        Some(type_query_overrides)
    }

    pub(super) fn prepare_direct_source_file_interface_members_for_lowering(
        &mut self,
        arena: &NodeArena,
        delegate_binder: &BinderState,
        member_indices: &[NodeIndex],
    ) -> Option<FxHashMap<NodeIndex, TypeId>> {
        if Self::source_file_interface_members_are_direct_lowerable(
            arena,
            delegate_binder,
            member_indices,
        ) {
            return Some(FxHashMap::default());
        }

        self.source_file_idx_for_direct_arena(arena)?;
        let mut seen = Vec::new();
        let mut type_query_overrides = FxHashMap::default();
        for member_idx in member_indices.iter().copied() {
            if !self.source_file_interface_member_is_cross_file_direct_lowerable(
                arena,
                delegate_binder,
                member_idx,
                &mut seen,
                &mut type_query_overrides,
            ) {
                return None;
            }
        }

        Some(type_query_overrides)
    }

    pub(super) fn source_file_name_def_id_for_cross_file_lowering(
        &self,
        delegate_binder: &BinderState,
        symbol_arena: &NodeArena,
        type_name: &str,
    ) -> Option<DefId> {
        self.source_file_local_name_def_id_for_lowering(delegate_binder, symbol_arena, type_name)
            .or_else(|| {
                self.resolve_source_file_imported_symbol(delegate_binder, symbol_arena, type_name)
                    .and_then(|(sym_id, _)| {
                        let symbol = self
                            .get_cross_file_symbol(sym_id)
                            .or_else(|| delegate_binder.get_symbol(sym_id))?;
                        let allowed_flags = symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS;
                        let disallowed_flags = symbol_flags::VALUE
                            | symbol_flags::CLASS
                            | symbol_flags::VALUE_MODULE
                            | symbol_flags::NAMESPACE_MODULE;
                        (symbol.flags & allowed_flags != 0 && symbol.flags & disallowed_flags == 0)
                            .then(|| self.ctx.get_or_create_def_id(sym_id))
                    })
            })
            .or_else(|| {
                (!self.source_file_type_name_shadows_actual_lib(delegate_binder, type_name))
                    .then(|| self.resolve_actual_lib_name_to_def_id_for_lowering(type_name))
                    .flatten()
                    .or_else(|| self.resolve_entity_name_text_to_def_id_for_lowering(type_name))
            })
    }

    fn source_file_interface_member_is_cross_file_direct_lowerable(
        &mut self,
        arena: &NodeArena,
        delegate_binder: &BinderState,
        member_idx: NodeIndex,
        seen: &mut Vec<SourceSymbolKey>,
        type_query_overrides: &mut FxHashMap<NodeIndex, TypeId>,
    ) -> bool {
        let Some(member_node) = arena.get(member_idx) else {
            return false;
        };
        if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
            return false;
        }
        let Some(signature) = arena.get_signature(member_node) else {
            return false;
        };
        if signature
            .parameters
            .as_ref()
            .is_some_and(|params| !params.nodes.is_empty())
            || signature
                .type_parameters
                .as_ref()
                .is_some_and(|params| !params.nodes.is_empty())
        {
            return false;
        }

        self.source_file_type_node_is_cross_file_direct_lowerable(
            arena,
            delegate_binder,
            signature.type_annotation,
            seen,
            type_query_overrides,
        )
    }

    fn source_file_type_node_is_cross_file_direct_lowerable(
        &mut self,
        arena: &NodeArena,
        delegate_binder: &BinderState,
        node_idx: NodeIndex,
        seen: &mut Vec<SourceSymbolKey>,
        type_query_overrides: &mut FxHashMap<NodeIndex, TypeId>,
    ) -> bool {
        if Self::source_file_type_node_is_scope_independent(arena, node_idx) {
            return true;
        }
        let mut local_seen = Vec::new();
        if Self::source_file_type_node_is_option_bag_lowerable(
            arena,
            delegate_binder,
            node_idx,
            &mut local_seen,
        ) {
            return true;
        }

        let Some(node) = arena.get(node_idx) else {
            return false;
        };
        match node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let Some(type_ref) = arena.get_type_ref(node) else {
                    return false;
                };
                let Some(name) = arena
                    .get(type_ref.type_name)
                    .and_then(|name_node| arena.get_identifier(name_node))
                    .map(|ident| ident.escaped_text.as_str())
                else {
                    return false;
                };

                if matches!(name, "Array" | "ReadonlyArray") {
                    return type_ref.type_arguments.as_ref().is_some_and(|args| {
                        args.nodes.len() == 1
                            && self.source_file_type_node_is_cross_file_direct_lowerable(
                                arena,
                                delegate_binder,
                                args.nodes[0],
                                seen,
                                type_query_overrides,
                            )
                    });
                }

                if let Some(args) = type_ref.type_arguments.as_ref()
                    && !args.nodes.is_empty()
                {
                    return self
                        .source_file_actual_lib_alias_is_direct_lowerable(delegate_binder, name)
                        && args.nodes.iter().copied().all(|arg| {
                            self.source_file_type_node_is_cross_file_direct_lowerable(
                                arena,
                                delegate_binder,
                                arg,
                                seen,
                                type_query_overrides,
                            )
                        });
                }

                self.source_file_imported_type_ref_is_direct_lowerable(
                    arena,
                    delegate_binder,
                    name,
                    seen,
                )
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                arena.get_composite_type(node).is_some_and(|composite| {
                    composite.types.nodes.iter().copied().all(|member| {
                        self.source_file_type_node_is_cross_file_direct_lowerable(
                            arena,
                            delegate_binder,
                            member,
                            seen,
                            type_query_overrides,
                        )
                    })
                })
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                arena.get_array_type(node).is_some_and(|array| {
                    self.source_file_type_node_is_cross_file_direct_lowerable(
                        arena,
                        delegate_binder,
                        array.element_type,
                        seen,
                        type_query_overrides,
                    )
                })
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                arena.get_tuple_type(node).is_some_and(|tuple| {
                    tuple.elements.nodes.iter().copied().all(|element| {
                        self.source_file_type_node_is_cross_file_direct_lowerable(
                            arena,
                            delegate_binder,
                            element,
                            seen,
                            type_query_overrides,
                        )
                    })
                })
            }
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE
                || k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE =>
            {
                arena.get_wrapped_type(node).is_some_and(|wrapped| {
                    self.source_file_type_node_is_cross_file_direct_lowerable(
                        arena,
                        delegate_binder,
                        wrapped.type_node,
                        seen,
                        type_query_overrides,
                    )
                })
            }
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                arena.get_type_operator(node).is_some_and(|operator| {
                    operator.operator == tsz_scanner::SyntaxKind::ReadonlyKeyword as u16
                        && self.source_file_type_node_is_cross_file_direct_lowerable(
                            arena,
                            delegate_binder,
                            operator.type_node,
                            seen,
                            type_query_overrides,
                        )
                })
            }
            k if k == syntax_kind_ext::TYPE_QUERY => {
                arena.get_type_query(node).is_some_and(|type_query| {
                    self.direct_source_file_type_query_expr_type(
                        arena,
                        delegate_binder,
                        type_query.expr_name,
                    )
                    .inspect(|ty| {
                        type_query_overrides.insert(type_query.expr_name, *ty);
                    })
                    .is_some()
                })
            }
            _ => false,
        }
    }

    fn source_file_imported_type_ref_is_direct_lowerable(
        &mut self,
        arena: &NodeArena,
        delegate_binder: &BinderState,
        name: &str,
        seen: &mut Vec<SourceSymbolKey>,
    ) -> bool {
        let Some((sym_id, file_idx)) =
            self.resolve_source_file_imported_symbol(delegate_binder, arena, name)
        else {
            return false;
        };
        let key = SourceSymbolKey { file_idx, sym_id };
        if seen.contains(&key) {
            return false;
        }

        let Some((target_arena, target_binder)) = self.source_file_target_context(file_idx) else {
            return false;
        };
        let Some(symbol) = target_binder.get_symbol(sym_id) else {
            return false;
        };

        seen.push(key);
        let result = if symbol.flags & symbol_flags::INTERFACE != 0 {
            self.source_file_imported_interface_is_direct_lowerable(
                sym_id,
                target_arena.as_ref(),
                target_binder.as_ref(),
            )
        } else if symbol.flags & symbol_flags::TYPE_ALIAS != 0 {
            self.direct_source_file_type_alias_result(sym_id, Some(file_idx), true)
                .is_some()
        } else {
            false
        };
        seen.pop();
        result
    }

    fn source_file_imported_interface_is_direct_lowerable(
        &mut self,
        sym_id: SymbolId,
        target_arena: &NodeArena,
        target_binder: &BinderState,
    ) -> bool {
        let Some(symbol) = target_binder.get_symbol(sym_id) else {
            return false;
        };
        if symbol.flags & symbol_flags::INTERFACE == 0
            || symbol.flags
                & (symbol_flags::VALUE
                    | symbol_flags::CLASS
                    | symbol_flags::TYPE_ALIAS
                    | symbol_flags::VALUE_MODULE
                    | symbol_flags::NAMESPACE_MODULE)
                != 0
        {
            return false;
        }
        let declarations: Vec<_> = symbol
            .declarations
            .iter()
            .copied()
            .map(|decl_idx| (decl_idx, target_arena))
            .collect();
        if declarations.is_empty()
            || !Self::source_file_interface_declarations_are_direct_lowerable(
                &declarations,
                target_binder,
            )
        {
            return false;
        }

        self.direct_cross_file_interface_lowering(sym_id, target_binder, target_arena, false, true)
            .is_some()
            || self
                .ctx
                .get_existing_def_id(sym_id)
                .and_then(|def_id| self.ctx.definition_store.get_body(def_id))
                .is_some()
    }

    fn direct_source_file_type_query_expr_type(
        &mut self,
        arena: &NodeArena,
        delegate_binder: &BinderState,
        expr_name: NodeIndex,
    ) -> Option<TypeId> {
        let expr_node = arena.get(expr_name)?;
        let ident = arena.get_identifier(expr_node)?;
        let (sym_id, file_idx) =
            self.resolve_source_file_value_symbol(delegate_binder, arena, &ident.escaped_text)?;
        let (target_arena, target_binder) = self.source_file_target_context(file_idx)?;
        self.prewarm_source_file_function_return_annotation(
            sym_id,
            target_arena.as_ref(),
            target_binder.as_ref(),
        );
        self.direct_source_file_function_declaration_type(
            sym_id,
            target_binder.as_ref(),
            target_arena.as_ref(),
            true,
        )
    }

    fn prewarm_source_file_function_return_annotation(
        &mut self,
        sym_id: SymbolId,
        target_arena: &NodeArena,
        target_binder: &BinderState,
    ) {
        let Some(symbol) = target_binder.get_symbol(sym_id) else {
            return;
        };
        if symbol.flags & symbol_flags::FUNCTION == 0 || symbol.declarations.len() != 1 {
            return;
        }
        let decl_idx = symbol.declarations[0];
        let Some(function) = target_arena
            .get(decl_idx)
            .and_then(|node| target_arena.get_function(node))
        else {
            return;
        };
        let annotation = function.type_annotation;
        if annotation.is_none() {
            return;
        }
        let _ =
            self.direct_lower_source_file_annotation_type(annotation, target_binder, target_arena);
    }

    fn resolve_source_file_value_symbol(
        &self,
        delegate_binder: &BinderState,
        arena: &NodeArena,
        name: &str,
    ) -> Option<(SymbolId, usize)> {
        let sym_id = delegate_binder.file_locals.get(name)?;
        let symbol = delegate_binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::ALIAS != 0 {
            return self.resolve_source_file_imported_symbol(delegate_binder, arena, name);
        }
        let file_idx = self.source_file_idx_for_direct_arena(arena)?;
        self.ctx.register_symbol_file_target(sym_id, file_idx);
        Some((sym_id, file_idx))
    }

    fn resolve_source_file_imported_symbol(
        &self,
        delegate_binder: &BinderState,
        arena: &NodeArena,
        name: &str,
    ) -> Option<(SymbolId, usize)> {
        let alias_id = delegate_binder.file_locals.get(name)?;
        let alias = delegate_binder.get_symbol(alias_id)?;
        if alias.flags & symbol_flags::ALIAS == 0 {
            return None;
        }
        let module_specifier = alias.import_module.as_ref()?;
        let import_name = alias.import_name.as_deref().unwrap_or(&alias.escaped_name);
        if import_name == "*" {
            return None;
        }
        let source_file_idx = self.source_file_idx_for_direct_arena(arena)?;
        let target_idx = self
            .ctx
            .resolve_import_target_from_file(source_file_idx, module_specifier)?;
        let target_binder = self.ctx.get_binder_for_file(target_idx)?;
        let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
        let file_name = &target_arena.source_files.first()?.file_name;
        let result = target_binder
            .module_exports
            .get(file_name)
            .and_then(|exports| exports.get(import_name))
            .or_else(|| target_binder.file_locals.get(import_name))?;
        self.ctx.register_symbol_file_target(result, target_idx);
        Some((result, target_idx))
    }

    fn source_file_target_context(
        &self,
        file_idx: usize,
    ) -> Option<(std::sync::Arc<NodeArena>, std::sync::Arc<BinderState>)> {
        let arena = self.ctx.all_arenas.as_ref()?.get(file_idx)?.clone();
        let binder = self.ctx.all_binders.as_ref()?.get(file_idx)?.clone();
        Some((arena, binder))
    }

    fn source_file_idx_for_direct_arena(&self, arena: &NodeArena) -> Option<usize> {
        self.ctx
            .get_file_idx_for_arena(arena)
            .or_else(|| std::ptr::eq(arena, self.ctx.arena).then_some(self.ctx.current_file_idx))
    }

    fn source_file_actual_lib_alias_is_direct_lowerable(
        &self,
        delegate_binder: &BinderState,
        name: &str,
    ) -> bool {
        !self.source_file_type_name_shadows_actual_lib(delegate_binder, name)
            && is_direct_actual_lib_alias_body_admitted(name)
            && self
                .resolve_actual_lib_name_to_def_id_for_lowering(name)
                .is_some()
    }

    fn source_file_type_name_shadows_actual_lib(
        &self,
        delegate_binder: &BinderState,
        name: &str,
    ) -> bool {
        delegate_binder.file_locals.get(name).is_some_and(|sym_id| {
            if self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id) {
                return false;
            }
            delegate_binder.get_symbol(sym_id).is_some_and(|symbol| {
                symbol.flags
                    & (symbol_flags::TYPE
                        | symbol_flags::TYPE_ALIAS
                        | symbol_flags::INTERFACE
                        | symbol_flags::CLASS
                        | symbol_flags::ALIAS)
                    != 0
            })
        })
    }
}

#[cfg(test)]
#[path = "cross_file_direct_source_members_tests.rs"]
mod tests;
