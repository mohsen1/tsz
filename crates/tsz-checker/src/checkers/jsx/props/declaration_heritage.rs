//! JSX declaration-surface helpers for prop existence checks.

use crate::state::CheckerState;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::{NodeArena, NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::checkers_domain::jsx) fn jsx_declared_interface_heritage_has_property(
        &self,
        type_id: TypeId,
        prop_name: &str,
    ) -> bool {
        let mut visited_types = rustc_hash::FxHashSet::default();
        let mut visited_symbols = rustc_hash::FxHashSet::default();
        self.jsx_declared_interface_heritage_has_property_inner(
            type_id,
            prop_name,
            &mut visited_types,
            &mut visited_symbols,
        )
    }

    fn jsx_declared_interface_heritage_has_property_inner(
        &self,
        type_id: TypeId,
        prop_name: &str,
        visited_types: &mut rustc_hash::FxHashSet<TypeId>,
        visited_symbols: &mut rustc_hash::FxHashSet<tsz_binder::SymbolId>,
    ) -> bool {
        if !visited_types.insert(type_id) {
            return false;
        }

        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
        {
            return members.iter().copied().any(|member| {
                self.jsx_declared_interface_heritage_has_property_inner(
                    member,
                    prop_name,
                    visited_types,
                    visited_symbols,
                )
            });
        }

        let symbol_type = crate::query_boundaries::state::type_environment::application_info(
            self.ctx.types,
            type_id,
        )
        .map(|(base, _)| base)
        .unwrap_or(type_id);
        let sym_id = self.ctx.resolve_type_to_symbol_id(symbol_type).or_else(|| {
            crate::query_boundaries::common::lazy_def_id(self.ctx.types, symbol_type)
                .and_then(|def_id| self.ctx.def_to_symbol_id_with_fallback(def_id))
        });
        let Some(sym_id) = sym_id else {
            return false;
        };
        self.jsx_interface_symbol_heritage_declares_property(sym_id, prop_name, visited_symbols)
    }

    fn jsx_interface_symbol_heritage_declares_property(
        &self,
        sym_id: tsz_binder::SymbolId,
        prop_name: &str,
        visited_symbols: &mut rustc_hash::FxHashSet<tsz_binder::SymbolId>,
    ) -> bool {
        if !visited_symbols.insert(sym_id) {
            return false;
        }
        let Some(symbol) = self.get_symbol_globally(sym_id) else {
            return false;
        };
        if symbol.flags & tsz_binder::symbol_flags::INTERFACE == 0 {
            return false;
        }

        let binder = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
            .unwrap_or(self.ctx.binder);
        let lib_binders = self.get_lib_binders();

        for decl_idx in symbol.declarations.iter().copied() {
            let arena = binder.arena_for_declaration_or(symbol.id, decl_idx, self.ctx.arena);
            let Some(node) = arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = arena.get_interface(node) else {
                continue;
            };

            for &member_idx in &interface.members.nodes {
                let Some(member_node) = arena.get(member_idx) else {
                    continue;
                };
                let Some(sig) = arena.get_signature(member_node) else {
                    continue;
                };
                if arena
                    .get_identifier_text(sig.name)
                    .is_some_and(|name| name == prop_name)
                {
                    return true;
                }
            }

            let Some(heritage_clauses) = interface.heritage_clauses.as_ref() else {
                continue;
            };
            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                for &type_idx in &heritage.types.nodes {
                    let Some(type_node) = arena.get(type_idx) else {
                        continue;
                    };
                    let expr_idx = if let Some(expr_type_args) = arena.get_expr_type_args(type_node)
                    {
                        expr_type_args.expression
                    } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                        arena
                            .get_type_ref(type_node)
                            .map(|type_ref| type_ref.type_name)
                            .unwrap_or(type_idx)
                    } else {
                        type_idx
                    };
                    let Some(base_name) = Self::jsx_entity_name_text_in_arena(arena, expr_idx)
                    else {
                        continue;
                    };
                    let base_sym = binder
                        .file_locals
                        .get(&base_name)
                        .or_else(|| binder.get_global_type_with_libs(&base_name, &lib_binders))
                        .or_else(|| {
                            base_name.rsplit('.').next().and_then(|tail| {
                                (tail != base_name).then(|| {
                                    binder.file_locals.get(tail).or_else(|| {
                                        binder.get_global_type_with_libs(tail, &lib_binders)
                                    })
                                })?
                            })
                        });
                    if let Some(base_sym) = base_sym
                        && self.jsx_interface_symbol_heritage_declares_property(
                            base_sym,
                            prop_name,
                            visited_symbols,
                        )
                    {
                        return true;
                    }
                }
            }
        }

        false
    }

    fn jsx_entity_name_text_in_arena(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
        let node = arena.get(idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.to_string());
        }
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = arena.get_qualified_name(node)?;
            let left = Self::jsx_entity_name_text_in_arena(arena, qn.left)?;
            let right = Self::jsx_entity_name_text_in_arena(arena, qn.right)?;
            return Some(format!("{left}.{right}"));
        }
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = arena.get_access_expr(node)
        {
            let left = Self::jsx_entity_name_text_in_arena(arena, access.expression)?;
            let right = arena
                .get(access.name_or_argument)
                .and_then(|right_node| arena.get_identifier(right_node))?;
            return Some(format!("{left}.{}", right.escaped_text));
        }
        None
    }
}
