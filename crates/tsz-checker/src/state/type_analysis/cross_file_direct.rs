//! Direct cross-file query fast paths that avoid constructing child checkers.
use crate::query_boundaries::common;
use crate::state::CheckerState;
use tsz_binder::{BinderState, SymbolId, symbol_flags};
use tsz_common::perf_counters::{
    CrossArenaSymbolMissSource, DirectActualLibAliasBodyOutcome,
    DirectActualLibIntlInterfaceOutcome, DirectCrossFileInterfaceLoweringOutcome,
    record_direct_actual_lib_alias_body_outcome, record_direct_actual_lib_intl_interface_outcome,
};
use tsz_lowering::TypeLowering;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::{NodeAccess, NodeArena, TypeAliasData};
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::def::{DefId, DefKind};
use tsz_solver::{TypeId, TypeParamInfo};

#[cfg(test)]
use super::cross_file_direct_files::{
    DIRECT_ACTUAL_LIB_ALIAS_BODY_ADMISSIONS, is_external_package_declaration_file_name,
};
use super::cross_file_direct_files::{
    allow_actual_lib_declaration_proof_bypass, allow_generic_actual_lib_direct_fallback,
    is_direct_actual_lib_alias_body_admitted, is_direct_actual_lib_value_interface_name,
    is_direct_lowering_declaration_arena, is_direct_lowering_source_file_arena,
    is_direct_type_alias_declaration_arena, iterator_object_has_global_augmentations,
};
pub(crate) use super::cross_file_direct_files::{
    is_builtin_lib_declaration_arena, is_builtin_lib_file_name,
    is_direct_actual_lib_declaration_arena,
};

struct DirectActualLibAliasBodyProof {
    body: TypeId,
    type_params: Vec<TypeParamInfo>,
    def_id: DefId,
    outcome: DirectActualLibAliasBodyOutcome,
}
impl<'a> CheckerState<'a> {
    fn symbol_is_actual_lib_namespace_export(
        &self,
        namespace: &str,
        export_name: &str,
        sym_id: SymbolId,
    ) -> bool {
        self.resolve_lib_namespace_export_symbol(namespace, export_name)
            .is_some_and(|export_sym_id| export_sym_id == sym_id)
    }

    fn symbol_is_proven_direct_actual_lib_value_interface(
        &self,
        sym_id: SymbolId,
        symbol: &tsz_binder::Symbol,
        name: &str,
    ) -> bool {
        symbol.has_any_flags(symbol_flags::VALUE | symbol_flags::INTERFACE)
            && self.symbol_declarations_are_direct_actual_lib_only(sym_id, symbol, name)
    }

    fn symbol_has_direct_actual_lib_interface_type_parameters(
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

    fn symbol_has_direct_actual_lib_iterator_object_heritage(
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

    fn symbol_declares_direct_actual_lib_protocol_method(
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

    fn symbol_declarations_are_direct_actual_lib_only(
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

    fn symbol_type_alias_declarations_are_proven_actual_lib_only(
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

    fn lib_declaration_name_matches(arena: &NodeArena, decl_idx: NodeIndex, name: &str) -> bool {
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

    fn lib_type_alias_declaration_name_matches(
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

    fn direct_actual_lib_type_alias_body(
        &mut self,
        sym_id: SymbolId,
        symbol: &tsz_binder::Symbol,
        name: &str,
        delegate_arena: &NodeArena,
    ) -> Option<DirectActualLibAliasBodyProof> {
        if !symbol.has_any_flags(symbol_flags::TYPE_ALIAS) {
            record_direct_actual_lib_alias_body_outcome(
                DirectActualLibAliasBodyOutcome::NotTypeAlias,
            );
            return None;
        }
        if symbol.has_any_flags(symbol_flags::VALUE) {
            record_direct_actual_lib_alias_body_outcome(
                DirectActualLibAliasBodyOutcome::ValueMerge,
            );
            return None;
        }
        if !self.symbol_type_alias_declarations_are_proven_actual_lib_only(
            sym_id,
            symbol,
            name,
            delegate_arena,
        ) {
            record_direct_actual_lib_alias_body_outcome(
                DirectActualLibAliasBodyOutcome::UnprovenActualLibDeclarations,
            );
            return None;
        }

        let def_id = if let Some(alias_type) = self.resolve_lib_type_by_name(name) {
            let Some(def_id) =
                crate::query_boundaries::common::lazy_def_id(self.ctx.types, alias_type)
            else {
                record_direct_actual_lib_alias_body_outcome(
                    DirectActualLibAliasBodyOutcome::ResolverNotLazyDef,
                );
                return None;
            };
            def_id
        } else {
            let def_id = self.ctx.get_or_create_def_id(sym_id);
            // If resolver lookup misses (for example Intl.* aliases), lower the proven declaration arena directly.
            let mut lowered: Option<(TypeId, Vec<TypeParamInfo>)> = None;
            for &decl_idx in &symbol.declarations {
                let decl_arenas = self
                    .ctx
                    .binder
                    .declaration_arenas
                    .get(&(sym_id, decl_idx))
                    .map(|arenas| arenas.iter().map(std::convert::AsRef::as_ref).collect())
                    .unwrap_or_else(|| vec![delegate_arena]);
                for decl_arena in decl_arenas {
                    if !is_direct_actual_lib_declaration_arena(decl_arena) {
                        continue;
                    }
                    let Some(node) = decl_arena.get(decl_idx) else {
                        continue;
                    };
                    let Some(type_alias) = decl_arena.get_type_alias(node) else {
                        continue;
                    };
                    lowered = Some(self.lower_cross_arena_type_alias_declaration(
                        sym_id, decl_idx, decl_arena, type_alias,
                    ));
                    break;
                }
                if lowered.is_some() {
                    break;
                }
            }
            let Some((body, params)) = lowered else {
                record_direct_actual_lib_alias_body_outcome(
                    DirectActualLibAliasBodyOutcome::MissingResolverType,
                );
                return None;
            };
            self.ctx.insert_def_type_params(def_id, params);
            self.ctx.definition_store.set_body(def_id, body);
            def_id
        };
        let Some(def_info) = self.ctx.definition_store.get(def_id) else {
            record_direct_actual_lib_alias_body_outcome(
                DirectActualLibAliasBodyOutcome::MissingDefinition,
            );
            return None;
        };
        if !matches!(def_info.kind, DefKind::TypeAlias) {
            record_direct_actual_lib_alias_body_outcome(
                DirectActualLibAliasBodyOutcome::NonTypeAliasDefinition,
            );
            return None;
        }
        let Some(body) = self.ctx.definition_store.get_body(def_id) else {
            record_direct_actual_lib_alias_body_outcome(
                DirectActualLibAliasBodyOutcome::MissingBody,
            );
            return None;
        };

        let params = self.ctx.get_def_type_params(def_id).unwrap_or_default();
        let non_generic_alias_has_resolved_body = params.is_empty()
            && !matches!(
                body,
                TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR | TypeId::NEVER
            );
        let generic_alias_has_admitted_body = !params.is_empty()
            && (is_direct_actual_lib_alias_body_admitted(name)
                || common::mapped_type_id(self.ctx.types, body).is_some()
                || common::contains_conditional_type(self.ctx.types, body)
                || common::union_members(self.ctx.types, body).is_some());
        let outcome = if non_generic_alias_has_resolved_body || generic_alias_has_admitted_body {
            DirectActualLibAliasBodyOutcome::Success
        } else if !params.is_empty() {
            DirectActualLibAliasBodyOutcome::GenericAlias
        } else {
            DirectActualLibAliasBodyOutcome::NameNotAdmitted
        };
        record_direct_actual_lib_alias_body_outcome(outcome);
        Some(DirectActualLibAliasBodyProof {
            body,
            type_params: params,
            def_id,
            outcome,
        })
    }

    pub(super) fn direct_actual_lib_symbol_type(
        &mut self,
        sym_id: SymbolId,
        delegate_arena_source: CrossArenaSymbolMissSource,
        delegate_arena: Option<&NodeArena>,
        needs_cross_file_delegation: bool,
    ) -> Option<(TypeId, Vec<TypeParamInfo>)> {
        if needs_cross_file_delegation
            || delegate_arena_source != CrossArenaSymbolMissSource::SymbolArena
            || !delegate_arena.is_some_and(is_direct_actual_lib_declaration_arena)
            || !self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
        {
            return None;
        }

        let delegate_arena = delegate_arena?;
        let symbol = self.get_cross_file_symbol(sym_id)?.clone();
        let name = symbol.escaped_name.clone();
        if self.ctx.file_local_type_shadow_for_lib_name(&name) {
            return None;
        }
        let intl_namespace_export =
            self.symbol_is_actual_lib_namespace_export("Intl", &name, sym_id);
        if !symbol.has_any_flags(symbol_flags::TYPE) {
            return None;
        }
        let proven_value_interface =
            self.symbol_is_proven_direct_actual_lib_value_interface(sym_id, &symbol, &name);
        let protocol_method_interface =
            self.symbol_declares_direct_actual_lib_protocol_method(sym_id, &symbol, delegate_arena);
        let admitted_value_interface = proven_value_interface
            || protocol_method_interface
            || is_direct_actual_lib_value_interface_name(&name);
        if symbol.has_any_flags(symbol_flags::VALUE)
            && !admitted_value_interface
            && !allow_actual_lib_declaration_proof_bypass(&name)
        {
            if intl_namespace_export {
                record_direct_actual_lib_intl_interface_outcome(
                    DirectActualLibIntlInterfaceOutcome::ValueInterfaceNotAdmitted,
                );
            }
            return None;
        }
        // Only proof-backed aliases admitted by policy return here; other
        // generic utility aliases stay on fallback so application/indexed-access
        // behavior sees the declared alias shape with type parameters in scope.
        if symbol.has_any_flags(symbol_flags::TYPE_ALIAS) {
            let DirectActualLibAliasBodyProof {
                body: alias_type,
                type_params: params,
                def_id: _def_id,
                outcome,
            } = self.direct_actual_lib_type_alias_body(sym_id, &symbol, &name, delegate_arena)?;
            if outcome != DirectActualLibAliasBodyOutcome::Success {
                return None;
            }
            self.ctx.symbol_types.insert(sym_id, alias_type);
            self.ctx
                .lib_delegation_cache
                .insert_symbol_type(sym_id, (alias_type, params.clone()));
            return Some((alias_type, params));
        }
        if !proven_value_interface
            && !self.symbol_declarations_are_direct_actual_lib_only(sym_id, &symbol, &name)
            && !protocol_method_interface
            && !allow_actual_lib_declaration_proof_bypass(&name)
        {
            if intl_namespace_export {
                record_direct_actual_lib_intl_interface_outcome(
                    DirectActualLibIntlInterfaceOutcome::DeclarationNotProven,
                );
            }
            return None;
        }
        let mut intl_success_outcome = None;
        let has_interface_type_params =
            self.symbol_has_direct_actual_lib_interface_type_parameters(sym_id, &symbol);
        if has_interface_type_params
            && !protocol_method_interface
            && !allow_generic_actual_lib_direct_fallback(&name)
            && name == "IteratorObject"
        {
            return None;
        }
        if has_interface_type_params
            && !protocol_method_interface
            && !allow_generic_actual_lib_direct_fallback(&name)
            && self.symbol_has_direct_actual_lib_iterator_object_heritage(sym_id, &symbol)
            && iterator_object_has_global_augmentations(&self.ctx)
        {
            return None;
        }
        let (direct_type, params) = if has_interface_type_params {
            let (direct_type, params) = self.resolve_lib_type_with_params(&name);
            if let Some(direct_type) = direct_type {
                (direct_type, params)
            } else if protocol_method_interface
                || !self.symbol_has_direct_actual_lib_iterator_object_heritage(sym_id, &symbol)
                || !iterator_object_has_global_augmentations(&self.ctx)
            {
                self.direct_cross_file_interface_lowering(
                    sym_id,
                    self.ctx.binder,
                    delegate_arena,
                    true,
                    false,
                )?
            } else {
                return None;
            }
        } else {
            let direct_type = if intl_namespace_export {
                let Some(namespace_sym_id) =
                    self.resolve_lib_namespace_export_symbol("Intl", &name)
                else {
                    record_direct_actual_lib_intl_interface_outcome(
                        DirectActualLibIntlInterfaceOutcome::MissingNamespaceExport,
                    );
                    return None;
                };
                if namespace_sym_id != sym_id {
                    record_direct_actual_lib_intl_interface_outcome(
                        DirectActualLibIntlInterfaceOutcome::NamespaceSymbolMismatch,
                    );
                    return None;
                }
                let cache_name = format!("Intl.{name}");
                let Some(direct_type) =
                    self.resolve_lib_interface_type_by_symbol(&cache_name, namespace_sym_id)
                else {
                    record_direct_actual_lib_intl_interface_outcome(
                        DirectActualLibIntlInterfaceOutcome::MissingNamespaceInterfaceType,
                    );
                    return None;
                };
                intl_success_outcome =
                    Some(DirectActualLibIntlInterfaceOutcome::SuccessNamespaceExport);
                direct_type
            } else {
                self.resolve_lib_type_by_name(&name)?
            };
            let params = self.get_type_params_for_symbol(sym_id);
            (direct_type, params)
        };
        if direct_type == TypeId::UNKNOWN || direct_type == TypeId::ERROR {
            if intl_namespace_export {
                record_direct_actual_lib_intl_interface_outcome(
                    DirectActualLibIntlInterfaceOutcome::UnknownOrError,
                );
            }
            return None;
        }
        if let Some(outcome) = intl_success_outcome {
            record_direct_actual_lib_intl_interface_outcome(outcome);
        }
        self.ctx.symbol_types.insert(sym_id, direct_type);
        self.ctx
            .lib_delegation_cache
            .insert_symbol_type(sym_id, (direct_type, params.clone()));
        Some((direct_type, params))
    }

    pub(super) fn cross_file_interface_declarations<'b>(
        &self,
        sym_id: SymbolId,
        delegate_binder: &'b BinderState,
        fallback_arena: &'b NodeArena,
    ) -> Option<Vec<(NodeIndex, &'b NodeArena)>> {
        let symbol = delegate_binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::INTERFACE == 0 {
            return None;
        }

        let mut declarations = Vec::new();
        for decl_idx in symbol.declarations.iter().copied() {
            let mut found = false;
            if let Some(arenas) = delegate_binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                for arena in arenas.iter() {
                    if arena
                        .get(decl_idx)
                        .and_then(|node| arena.get_interface(node))
                        .is_some()
                    {
                        declarations.push((decl_idx, arena.as_ref()));
                        found = true;
                    }
                }
            }

            if !found
                && fallback_arena
                    .get(decl_idx)
                    .and_then(|node| fallback_arena.get_interface(node))
                    .is_some()
            {
                declarations.push((decl_idx, fallback_arena));
            }
        }

        (!declarations.is_empty()).then_some(declarations)
    }

    fn source_file_type_node_is_scope_independent(arena: &NodeArena, node_idx: NodeIndex) -> bool {
        if node_idx.is_none() {
            return false;
        }
        let Some(node) = arena.get(node_idx) else {
            return false;
        };

        match node.kind {
            k if k == tsz_scanner::SyntaxKind::AnyKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::UnknownKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::NeverKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::VoidKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::UndefinedKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::NullKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::BooleanKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::NumberKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::StringKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::BigIntKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::SymbolKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::ObjectKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::TrueKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::FalseKeyword as u16 => true,
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                arena.get_type_ref(node).is_some_and(|type_ref| {
                    let Some(name) = arena
                        .get(type_ref.type_name)
                        .and_then(|name_node| arena.get_identifier(name_node))
                        .map(|ident| ident.escaped_text.as_str())
                    else {
                        return false;
                    };
                    match name {
                        "any" | "unknown" | "never" | "void" | "undefined" | "null" | "boolean"
                        | "number" | "string" | "bigint" | "symbol" | "object" => type_ref
                            .type_arguments
                            .as_ref()
                            .is_none_or(|args| args.nodes.is_empty()),
                        "Array" | "ReadonlyArray" => {
                            type_ref.type_arguments.as_ref().is_some_and(|args| {
                                args.nodes.len() == 1
                                    && Self::source_file_type_node_is_scope_independent(
                                        arena,
                                        args.nodes[0],
                                    )
                            })
                        }
                        _ => false,
                    }
                })
            }
            k if k == syntax_kind_ext::LITERAL_TYPE => arena.get_literal_type(node).is_some(),
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                arena.get_composite_type(node).is_some_and(|composite| {
                    composite.types.nodes.iter().copied().all(|member| {
                        Self::source_file_type_node_is_scope_independent(arena, member)
                    })
                })
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                arena.get_array_type(node).is_some_and(|array| {
                    Self::source_file_type_node_is_scope_independent(arena, array.element_type)
                })
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                arena.get_tuple_type(node).is_some_and(|tuple| {
                    tuple.elements.nodes.iter().copied().all(|element| {
                        Self::source_file_type_node_is_scope_independent(arena, element)
                    })
                })
            }
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE
                || k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE =>
            {
                arena.get_wrapped_type(node).is_some_and(|wrapped| {
                    Self::source_file_type_node_is_scope_independent(arena, wrapped.type_node)
                })
            }
            _ => false,
        }
    }

    fn source_file_type_node_is_option_bag_lowerable<'b>(
        arena: &'b NodeArena,
        delegate_binder: &BinderState,
        node_idx: NodeIndex,
        seen_type_names: &mut Vec<&'b str>,
    ) -> bool {
        if Self::source_file_type_node_is_scope_independent(arena, node_idx) {
            return true;
        }
        if node_idx.is_none() {
            return false;
        }
        let Some(node) = arena.get(node_idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                arena.get_type_ref(node).is_some_and(|type_ref| {
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
                                && Self::source_file_type_node_is_option_bag_lowerable(
                                    arena,
                                    delegate_binder,
                                    args.nodes[0],
                                    seen_type_names,
                                )
                        });
                    }

                    if type_ref
                        .type_arguments
                        .as_ref()
                        .is_some_and(|args| !args.nodes.is_empty())
                    {
                        return false;
                    }

                    Self::source_file_type_reference_targets_option_bag_lowerable_declaration(
                        arena,
                        delegate_binder,
                        name,
                        seen_type_names,
                    )
                })
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                arena.get_composite_type(node).is_some_and(|composite| {
                    composite.types.nodes.iter().copied().all(|member| {
                        Self::source_file_type_node_is_option_bag_lowerable(
                            arena,
                            delegate_binder,
                            member,
                            seen_type_names,
                        )
                    })
                })
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                arena.get_array_type(node).is_some_and(|array| {
                    Self::source_file_type_node_is_option_bag_lowerable(
                        arena,
                        delegate_binder,
                        array.element_type,
                        seen_type_names,
                    )
                })
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                arena.get_tuple_type(node).is_some_and(|tuple| {
                    tuple.elements.nodes.iter().copied().all(|element| {
                        Self::source_file_type_node_is_option_bag_lowerable(
                            arena,
                            delegate_binder,
                            element,
                            seen_type_names,
                        )
                    })
                })
            }
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE
                || k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE =>
            {
                arena.get_wrapped_type(node).is_some_and(|wrapped| {
                    Self::source_file_type_node_is_option_bag_lowerable(
                        arena,
                        delegate_binder,
                        wrapped.type_node,
                        seen_type_names,
                    )
                })
            }
            _ => false,
        }
    }

    fn source_file_type_reference_targets_option_bag_lowerable_declaration<'b>(
        arena: &'b NodeArena,
        delegate_binder: &BinderState,
        name: &'b str,
        seen_type_names: &mut Vec<&'b str>,
    ) -> bool {
        if seen_type_names.contains(&name) {
            return false;
        }
        let Some(sym_id) = delegate_binder.file_locals.get(name) else {
            return false;
        };
        let Some(symbol) = delegate_binder.get_symbol(sym_id) else {
            return false;
        };
        let disallowed_flags = symbol_flags::VALUE
            | symbol_flags::CLASS
            | symbol_flags::VALUE_MODULE
            | symbol_flags::NAMESPACE_MODULE;
        if symbol.flags & disallowed_flags != 0 || symbol.declarations.len() != 1 {
            return false;
        }

        let decl_idx = symbol.declarations[0];
        if !Self::lib_declaration_name_matches(arena, decl_idx, name) {
            return false;
        }
        let Some(decl_node) = arena.get(decl_idx) else {
            return false;
        };

        if symbol.flags & symbol_flags::TYPE_ALIAS != 0 {
            seen_type_names.push(name);
            let result = arena.get_type_alias(decl_node).is_some_and(|type_alias| {
                type_alias
                    .type_parameters
                    .as_ref()
                    .is_none_or(|params| params.nodes.is_empty())
                    && !Self::source_file_type_node_contains_kind(
                        arena,
                        type_alias.type_node,
                        syntax_kind_ext::TYPE_QUERY,
                    )
                    && Self::source_file_type_node_is_option_bag_lowerable(
                        arena,
                        delegate_binder,
                        type_alias.type_node,
                        seen_type_names,
                    )
            });
            seen_type_names.pop();
            result
        } else if symbol.flags & symbol_flags::INTERFACE != 0 {
            arena.get_interface(decl_node).is_some()
                && Self::source_file_interface_declarations_are_direct_lowerable_with_seen(
                    &[(decl_idx, arena)],
                    delegate_binder,
                    seen_type_names,
                )
        } else {
            false
        }
    }

    pub(super) fn source_file_local_name_def_id_for_lowering(
        &self,
        delegate_binder: &BinderState,
        symbol_arena: &NodeArena,
        type_name: &str,
    ) -> Option<tsz_solver::def::DefId> {
        let sym_id = delegate_binder.file_locals.get(type_name)?;
        let symbol = delegate_binder.get_symbol(sym_id)?;
        let allowed_flags = symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS;
        let disallowed_flags = symbol_flags::VALUE
            | symbol_flags::CLASS
            | symbol_flags::VALUE_MODULE
            | symbol_flags::NAMESPACE_MODULE;
        if symbol.flags & allowed_flags == 0 || symbol.flags & disallowed_flags != 0 {
            return None;
        }
        if symbol
            .declarations
            .iter()
            .any(|&decl_idx| Self::lib_declaration_name_matches(symbol_arena, decl_idx, type_name))
        {
            Some(self.ctx.get_or_create_def_id(sym_id))
        } else {
            None
        }
    }

    fn source_file_type_node_is_generic_scope_independent(
        arena: &NodeArena,
        node_idx: NodeIndex,
        type_param_names: &[String],
    ) -> bool {
        if Self::source_file_type_node_is_scope_independent(arena, node_idx) {
            return true;
        }
        if node_idx.is_none() {
            return false;
        }
        let Some(node) = arena.get(node_idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                arena.get_type_ref(node).is_some_and(|type_ref| {
                    let Some(name) = arena
                        .get(type_ref.type_name)
                        .and_then(|name_node| arena.get_identifier(name_node))
                        .map(|ident| ident.escaped_text.as_str())
                    else {
                        return false;
                    };
                    if type_param_names.iter().any(|param| param == name) {
                        return type_ref
                            .type_arguments
                            .as_ref()
                            .is_none_or(|args| args.nodes.is_empty());
                    }
                    matches!(name, "Array" | "ReadonlyArray")
                        && type_ref.type_arguments.as_ref().is_some_and(|args| {
                            args.nodes.len() == 1
                                && Self::source_file_type_node_is_generic_scope_independent(
                                    arena,
                                    args.nodes[0],
                                    type_param_names,
                                )
                        })
                })
            }
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                arena.get_conditional_type(node).is_some_and(|conditional| {
                    let mut true_branch_names = type_param_names.to_vec();
                    Self::collect_infer_type_param_names(
                        arena,
                        conditional.extends_type,
                        &mut true_branch_names,
                    );
                    Self::source_file_type_node_is_generic_scope_independent(
                        arena,
                        conditional.check_type,
                        type_param_names,
                    ) && Self::source_file_type_node_is_generic_scope_independent(
                        arena,
                        conditional.extends_type,
                        type_param_names,
                    ) && Self::source_file_type_node_is_generic_scope_independent(
                        arena,
                        conditional.true_type,
                        &true_branch_names,
                    ) && Self::source_file_type_node_is_generic_scope_independent(
                        arena,
                        conditional.false_type,
                        type_param_names,
                    )
                })
            }
            k if k == syntax_kind_ext::INFER_TYPE => {
                arena.get_infer_type(node).is_some_and(|infer_type| {
                    let Some(type_param_node) = arena.get(infer_type.type_parameter) else {
                        return false;
                    };
                    let Some(type_param) = arena.get_type_parameter(type_param_node) else {
                        return false;
                    };
                    type_param.constraint.is_none() && type_param.default.is_none()
                })
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                arena.get_array_type(node).is_some_and(|array| {
                    Self::source_file_type_node_is_generic_scope_independent(
                        arena,
                        array.element_type,
                        type_param_names,
                    )
                })
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                arena.get_tuple_type(node).is_some_and(|tuple| {
                    tuple.elements.nodes.iter().copied().all(|element| {
                        Self::source_file_type_node_is_generic_scope_independent(
                            arena,
                            element,
                            type_param_names,
                        )
                    })
                })
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                arena.get_composite_type(node).is_some_and(|composite| {
                    composite.types.nodes.iter().copied().all(|member| {
                        Self::source_file_type_node_is_generic_scope_independent(
                            arena,
                            member,
                            type_param_names,
                        )
                    })
                })
            }
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE
                || k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE =>
            {
                arena.get_wrapped_type(node).is_some_and(|wrapped| {
                    Self::source_file_type_node_is_generic_scope_independent(
                        arena,
                        wrapped.type_node,
                        type_param_names,
                    )
                })
            }
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                arena.get_type_operator(node).is_some_and(|operator| {
                    Self::source_file_type_node_is_generic_scope_independent(
                        arena,
                        operator.type_node,
                        type_param_names,
                    )
                })
            }
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                arena.get_indexed_access_type(node).is_some_and(|indexed| {
                    Self::source_file_type_node_is_generic_scope_independent(
                        arena,
                        indexed.object_type,
                        type_param_names,
                    ) && Self::source_file_type_node_is_generic_scope_independent(
                        arena,
                        indexed.index_type,
                        type_param_names,
                    )
                })
            }
            _ => false,
        }
    }

    fn type_alias_type_param_names(arena: &NodeArena, type_alias: &TypeAliasData) -> Vec<String> {
        type_alias
            .type_parameters
            .as_ref()
            .into_iter()
            .flat_map(|params| params.nodes.iter().copied())
            .filter_map(|param_idx| {
                let param_node = arena.get(param_idx)?;
                let param = arena.get_type_parameter(param_node)?;
                let name_node = arena.get(param.name)?;
                let ident = arena.get_identifier(name_node)?;
                Some(ident.escaped_text.to_string())
            })
            .collect()
    }

    fn collect_infer_type_param_names(arena: &NodeArena, root: NodeIndex, names: &mut Vec<String>) {
        let mut stack = vec![root];
        while let Some(idx) = stack.pop() {
            let Some(node) = arena.get(idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::INFER_TYPE
                && let Some(infer_type) = arena.get_infer_type(node)
                && let Some(type_param_node) = arena.get(infer_type.type_parameter)
                && let Some(type_param) = arena.get_type_parameter(type_param_node)
                && let Some(name_node) = arena.get(type_param.name)
                && let Some(ident) = arena.get_identifier(name_node)
                && !names.iter().any(|name| name == &ident.escaped_text)
            {
                names.push(ident.escaped_text.to_string());
            }
            stack.extend(arena.get_children(idx));
        }
    }

    fn source_file_type_node_contains_kind(arena: &NodeArena, root: NodeIndex, kind: u16) -> bool {
        let mut stack = vec![root];
        while let Some(idx) = stack.pop() {
            if arena.get(idx).is_some_and(|node| node.kind == kind) {
                return true;
            }
            stack.extend(arena.get_children(idx));
        }
        false
    }

    fn source_file_type_node_contains_identifier_name(
        arena: &NodeArena,
        root: NodeIndex,
        name: &str,
    ) -> bool {
        let mut stack = vec![root];
        while let Some(idx) = stack.pop() {
            if arena
                .get(idx)
                .and_then(|node| arena.get_identifier(node))
                .is_some_and(|ident| ident.escaped_text == name)
            {
                return true;
            }
            stack.extend(arena.get_children(idx));
        }
        false
    }

    fn source_file_interface_declarations_are_direct_lowerable_with_seen<'b>(
        declarations: &[(NodeIndex, &'b NodeArena)],
        delegate_binder: &BinderState,
        seen_type_names: &mut Vec<&'b str>,
    ) -> bool {
        declarations.iter().all(|(decl_idx, arena)| {
            let Some(node) = arena.get(*decl_idx) else {
                return false;
            };
            let Some(interface) = arena.get_interface(node) else {
                return false;
            };
            let Some(interface_name) = arena
                .get(interface.name)
                .and_then(|name_node| arena.get_identifier(name_node))
                .map(|ident| ident.escaped_text.as_str())
            else {
                return false;
            };
            if seen_type_names.contains(&interface_name) {
                return false;
            }
            if interface
                .type_parameters
                .as_ref()
                .is_some_and(|params| !params.nodes.is_empty())
            {
                return false;
            }

            seen_type_names.push(interface_name);
            let result = interface.members.nodes.iter().copied().all(|member_idx| {
                let Some(member_node) = arena.get(member_idx) else {
                    return false;
                };
                if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
                    return false;
                }
                let Some(signature) = arena.get_signature(member_node) else {
                    return false;
                };
                signature
                    .parameters
                    .as_ref()
                    .is_none_or(|params| params.nodes.is_empty())
                    && signature
                        .type_parameters
                        .as_ref()
                        .is_none_or(|params| params.nodes.is_empty())
                    && Self::source_file_type_node_is_option_bag_lowerable(
                        arena,
                        delegate_binder,
                        signature.type_annotation,
                        seen_type_names,
                    )
            });
            seen_type_names.pop();
            result
        })
    }

    fn source_file_interface_declarations_are_direct_lowerable(
        declarations: &[(NodeIndex, &NodeArena)],
        delegate_binder: &BinderState,
    ) -> bool {
        let mut seen_type_names = Vec::new();
        Self::source_file_interface_declarations_are_direct_lowerable_with_seen(
            declarations,
            delegate_binder,
            &mut seen_type_names,
        )
    }

    fn direct_lower_source_file_annotation_type(
        &mut self,
        annotation: NodeIndex,
        delegate_binder: &BinderState,
        symbol_arena: &NodeArena,
    ) -> Option<TypeId> {
        if Self::source_file_type_node_is_scope_independent(symbol_arena, annotation) {
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
            .with_builtin_iterator_return_type(self.builtin_iterator_return_intrinsic_type());
            let lowered = lowering.lower_type(annotation);
            return (lowered != TypeId::UNKNOWN && lowered != TypeId::ERROR).then_some(lowered);
        }

        let type_ref = symbol_arena
            .get(annotation)
            .and_then(|node| symbol_arena.get_type_ref(node))?;
        if type_ref
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty())
        {
            return None;
        }
        let name = symbol_arena
            .get(type_ref.type_name)
            .and_then(|name_node| symbol_arena.get_identifier(name_node))
            .map(|ident| ident.escaped_text.as_str())?;
        let target_sym_id = delegate_binder.file_locals.get(name)?;
        let target_symbol = delegate_binder.get_symbol(target_sym_id)?;
        if target_symbol.flags & symbol_flags::INTERFACE == 0 {
            return None;
        }

        let (_interface_type, _params) = self.direct_cross_file_interface_lowering(
            target_sym_id,
            delegate_binder,
            symbol_arena,
            false,
            true,
        )?;
        let def_id = self.ctx.get_or_create_def_id(target_sym_id);
        Some(self.ctx.types.lazy(def_id))
    }

    pub(super) fn direct_source_file_variable_annotation_type(
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
        if symbol.flags & symbol_flags::VARIABLE == 0 {
            return None;
        }
        if symbol.flags & (symbol_flags::MODULE | symbol_flags::ALIAS) != 0 {
            return None;
        }
        if symbol.declarations.len() != 1 {
            return None;
        }

        let decl_idx = symbol.declarations[0];
        let decl_node = symbol_arena.get(decl_idx)?;
        let variable = symbol_arena.get_variable_declaration(decl_node)?;
        let annotation = variable.type_annotation.into_option()?;
        self.direct_lower_source_file_annotation_type(annotation, delegate_binder, symbol_arena)
    }

    pub(super) fn direct_source_file_variable_annotation_result(
        &mut self,
        sym_id: SymbolId,
        direct_target: Option<(&NodeArena, &BinderState, Option<usize>)>,
        allow_source_file_arena: bool,
    ) -> Option<TypeId> {
        let (symbol_arena, delegate_binder, _) = direct_target?;
        self.direct_source_file_variable_annotation_type(
            sym_id,
            delegate_binder,
            symbol_arena,
            allow_source_file_arena,
        )
    }

    pub(crate) fn direct_source_file_type_alias_result(
        &mut self,
        sym_id: SymbolId,
        target_file_idx: Option<usize>,
        allow_source_file_arena: bool,
    ) -> Option<(TypeId, Vec<TypeParamInfo>)> {
        let target_file_idx = target_file_idx?;
        let (symbol_arena_arc, delegate_binder_arc) = {
            let symbol_arena_arc = self.ctx.all_arenas.as_ref()?.get(target_file_idx)?.clone();
            let delegate_binder_arc = self.ctx.all_binders.as_ref()?.get(target_file_idx)?.clone();
            (symbol_arena_arc, delegate_binder_arc)
        };
        let symbol_arena = symbol_arena_arc.as_ref();
        let delegate_binder = delegate_binder_arc.as_ref();
        if !allow_source_file_arena || !is_direct_lowering_source_file_arena(symbol_arena) {
            return None;
        }

        let symbol = delegate_binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::TYPE_ALIAS == 0 {
            return None;
        }
        if symbol.flags
            & (symbol_flags::VALUE
                | symbol_flags::CLASS
                | symbol_flags::INTERFACE
                | symbol_flags::VALUE_MODULE
                | symbol_flags::NAMESPACE_MODULE)
            != 0
        {
            return None;
        }
        if symbol.declarations.len() != 1 {
            return None;
        }

        let name = symbol.escaped_name.clone();
        let decl_idx = symbol.declarations[0];
        if !Self::lib_type_alias_declaration_name_matches(symbol_arena, decl_idx, &name) {
            return None;
        }
        let decl_node = symbol_arena.get(decl_idx)?;
        let type_alias = symbol_arena.get_type_alias(decl_node)?;
        let type_param_names = Self::type_alias_type_param_names(symbol_arena, type_alias);
        let body_is_direct_lowerable = if type_param_names.is_empty() {
            Self::source_file_type_node_is_scope_independent(symbol_arena, type_alias.type_node)
        } else {
            Self::source_file_type_node_is_generic_scope_independent(
                symbol_arena,
                type_alias.type_node,
                &type_param_names,
            )
        };
        if !body_is_direct_lowerable {
            return None;
        }

        // Keep flow-sensitive `typeof` aliases and direct self/cycle cases on
        // the child-checker path, where the declaring file's diagnostics and
        // resolution stack are already handled.
        if Self::source_file_type_node_contains_kind(
            symbol_arena,
            type_alias.type_node,
            syntax_kind_ext::TYPE_QUERY,
        ) || Self::source_file_type_node_contains_identifier_name(
            symbol_arena,
            type_alias.type_node,
            &name,
        ) {
            return None;
        }

        let (alias_type, params) = self.lower_cross_arena_type_alias_declaration(
            sym_id,
            decl_idx,
            symbol_arena,
            type_alias,
        );
        if matches!(alias_type, TypeId::UNKNOWN | TypeId::ERROR) {
            return None;
        }

        let def_id = self.ctx.get_or_create_def_id(sym_id);
        if let Some(shape) = crate::query_boundaries::state::type_environment::object_shape(
            self.ctx.types,
            alias_type,
        ) {
            self.ctx.definition_store.set_instance_shape(def_id, shape);
        }
        self.ctx
            .register_def_auto_params_in_envs(def_id, alias_type, params.clone());
        self.ctx
            .definition_store
            .register_type_to_def(alias_type, def_id);

        Some((alias_type, params))
    }

    pub(crate) fn direct_declaration_file_type_alias_result(
        &mut self,
        sym_id: SymbolId,
        symbol_arena: &NodeArena,
    ) -> Option<(TypeId, Vec<TypeParamInfo>)> {
        if !is_direct_type_alias_declaration_arena(symbol_arena) {
            return None;
        }

        let symbol = self
            .get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol(sym_id))?
            .clone();
        if symbol.flags & symbol_flags::TYPE_ALIAS == 0 {
            return None;
        }
        if symbol.flags
            & (symbol_flags::VALUE
                | symbol_flags::CLASS
                | symbol_flags::INTERFACE
                | symbol_flags::VALUE_MODULE
                | symbol_flags::NAMESPACE_MODULE)
            != 0
        {
            return None;
        }
        if symbol.declarations.len() != 1 {
            return None;
        }

        let name = symbol.escaped_name.clone();
        let decl_idx = symbol.declarations[0];
        if !Self::lib_type_alias_declaration_name_matches(symbol_arena, decl_idx, &name) {
            return None;
        }
        let decl_node = symbol_arena.get(decl_idx)?;
        let type_alias = symbol_arena.get_type_alias(decl_node)?;
        if Self::source_file_type_node_contains_kind(
            symbol_arena,
            type_alias.type_node,
            syntax_kind_ext::TYPE_QUERY,
        ) || Self::source_file_type_node_contains_identifier_name(
            symbol_arena,
            type_alias.type_node,
            &name,
        ) {
            return None;
        }

        let (alias_type, params) = self.lower_cross_arena_type_alias_declaration(
            sym_id,
            decl_idx,
            symbol_arena,
            type_alias,
        );
        if matches!(alias_type, TypeId::UNKNOWN | TypeId::ERROR) {
            return None;
        }

        let def_id = self.ctx.get_or_create_def_id(sym_id);
        if let Some(shape) = crate::query_boundaries::state::type_environment::object_shape(
            self.ctx.types,
            alias_type,
        ) {
            self.ctx.definition_store.set_instance_shape(def_id, shape);
        }
        self.ctx
            .register_def_auto_params_in_envs(def_id, alias_type, params.clone());
        self.ctx
            .definition_store
            .register_type_to_def(alias_type, def_id);
        if symbol_arena
            .source_files
            .first()
            .is_some_and(|source_file| is_builtin_lib_file_name(&source_file.file_name))
        {
            self.ctx
                .lib_delegation_cache
                .insert_symbol_type(sym_id, (alias_type, params.clone()));
        }

        Some((alias_type, params))
    }

    pub(super) fn direct_cross_file_interface_lowering(
        &mut self,
        sym_id: SymbolId,
        delegate_binder: &BinderState,
        symbol_arena: &NodeArena,
        allow_complex_declarations: bool,
        allow_source_file_arena: bool,
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        let record = |outcome: DirectCrossFileInterfaceLoweringOutcome| {
            tsz_common::perf_counters::record_direct_cross_file_interface_lowering_outcome(outcome);
        };

        // Source and local test-fixture interfaces need exact binder-local symbol
        // resolution for diagnostics. Built-in libs may use this path only when
        // the declaration-shape guard below proves they do not need the mature
        // merged/heritage checker path.
        let direct_declaration_arena = is_direct_lowering_declaration_arena(symbol_arena)
            || is_builtin_lib_declaration_arena(symbol_arena);
        let direct_source_file_arena =
            allow_source_file_arena && is_direct_lowering_source_file_arena(symbol_arena);
        if !direct_declaration_arena && !direct_source_file_arena {
            record(DirectCrossFileInterfaceLoweringOutcome::RejectedNonDirectArena);
            return None;
        }

        let Some(symbol) = delegate_binder.get_symbol(sym_id) else {
            record(DirectCrossFileInterfaceLoweringOutcome::MissingSymbol);
            return None;
        };
        let disallowed_merge_flags = symbol_flags::CLASS
            | symbol_flags::TYPE_ALIAS
            | symbol_flags::VALUE_MODULE
            | symbol_flags::NAMESPACE_MODULE;
        if symbol.flags & symbol_flags::INTERFACE == 0 {
            record(DirectCrossFileInterfaceLoweringOutcome::NotInterface);
            return None;
        }
        if symbol.flags & disallowed_merge_flags != 0 {
            record(DirectCrossFileInterfaceLoweringOutcome::DisallowedMergeFlags);
            return None;
        }

        let Some(declarations) =
            self.cross_file_interface_declarations(sym_id, delegate_binder, symbol_arena)
        else {
            record(DirectCrossFileInterfaceLoweringOutcome::MissingDeclarations);
            return None;
        };
        let has_heritage = Self::interface_declarations_have_heritage(&declarations);
        let has_computed_names = Self::interface_declarations_have_computed_names(&declarations);
        let has_unsupported_computed_names =
            Self::interface_declarations_have_unsupported_computed_names(&declarations);
        let builtin_lib_declaration_arena = is_builtin_lib_declaration_arena(symbol_arena);
        if direct_source_file_arena {
            if has_heritage
                || has_computed_names
                || !Self::source_file_interface_declarations_are_direct_lowerable(
                    &declarations,
                    delegate_binder,
                )
            {
                record(DirectCrossFileInterfaceLoweringOutcome::ComplexDeclaration);
                return None;
            }
        } else if (!allow_complex_declarations
            && (has_unsupported_computed_names || (has_heritage && !builtin_lib_declaration_arena)))
            || (builtin_lib_declaration_arena
                && has_heritage
                && self.interface_declarations_have_unsafe_builtin_heritage_base(
                    &declarations,
                    &symbol.escaped_name,
                ))
        {
            record(DirectCrossFileInterfaceLoweringOutcome::ComplexDeclaration);
            return None;
        }

        let def_id = self.ctx.get_or_create_def_id(sym_id);
        let name_resolver = |type_name: &str| -> Option<tsz_solver::def::DefId> {
            if direct_source_file_arena {
                return self.source_file_local_name_def_id_for_lowering(
                    delegate_binder,
                    symbol_arena,
                    type_name,
                );
            }
            (!self.ctx.file_local_type_shadow_for_lib_name(type_name))
                .then(|| self.resolve_actual_lib_name_to_def_id_for_lowering(type_name))
                .flatten()
                .or_else(|| self.resolve_entity_name_text_to_def_id_for_lowering(type_name))
        };
        let no_type_symbol = |_node_idx: NodeIndex| -> Option<u32> { None };
        let no_def_id = |_node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> { None };
        let no_value_symbol = |_node_idx: NodeIndex| -> Option<u32> { None };
        let lazy_type_params_resolver =
            |def_id: tsz_solver::def::DefId| self.ctx.get_def_type_params(def_id);

        let lowering = TypeLowering::with_hybrid_resolver(
            symbol_arena,
            self.ctx.types,
            &no_type_symbol,
            &no_def_id,
            &no_value_symbol,
        )
        .with_builtin_iterator_return_type(self.builtin_iterator_return_intrinsic_type())
        .with_name_def_id_resolver(&name_resolver)
        .with_lazy_type_params_resolver(&lazy_type_params_resolver)
        .with_preferred_self_reference(symbol.escaped_name.clone(), def_id)
        .prefer_name_def_id_resolution();

        let (mut interface_type, params) =
            lowering.lower_merged_interface_declarations_with_symbol(&declarations, Some(sym_id));
        if interface_type == TypeId::UNKNOWN || interface_type == TypeId::ERROR {
            record(DirectCrossFileInterfaceLoweringOutcome::UnknownOrError);
            return None;
        }
        if builtin_lib_declaration_arena && has_heritage {
            interface_type =
                self.merge_lib_interface_heritage(interface_type, &symbol.escaped_name);
            if interface_type == TypeId::UNKNOWN || interface_type == TypeId::ERROR {
                record(DirectCrossFileInterfaceLoweringOutcome::UnknownOrError);
                return None;
            }
        }
        let shareable_builtin_lib_type = (builtin_lib_declaration_arena
            && !self.lib_name_locally_augmented(&symbol.escaped_name))
        .then(|| symbol.escaped_name.clone());
        if let Some(name) = shareable_builtin_lib_type.as_ref() {
            self.ctx
                .lib_type_resolution_cache
                .insert(name.clone(), Some(interface_type));
        }
        record(DirectCrossFileInterfaceLoweringOutcome::Success);

        self.ctx
            .register_def_auto_params_in_envs(def_id, interface_type, params.clone());
        self.ctx
            .definition_store
            .register_type_to_def(interface_type, def_id);
        if let Some(name) = shareable_builtin_lib_type.as_ref()
            && self.cached_lib_type_is_usable(name, Some(interface_type))
            && let Some(shared) = self.ctx.shared_lib_type_cache.as_ref()
        {
            shared.insert(name.clone(), Some(interface_type));
        }
        Some((interface_type, params))
    }

    pub(super) fn direct_cross_file_interface_member_simple_types(
        &mut self,
        interface_idx: NodeIndex,
        member_indices: &[NodeIndex],
        interface_arena: &NodeArena,
        delegate_binder: &BinderState,
        type_args: Option<&[TypeId]>,
        allow_source_file_arena: bool,
    ) -> Option<rustc_hash::FxHashMap<NodeIndex, TypeId>> {
        let direct_member_arena = is_builtin_lib_declaration_arena(interface_arena)
            || is_direct_lowering_declaration_arena(interface_arena)
            || (allow_source_file_arena && is_direct_lowering_source_file_arena(interface_arena));
        if direct_member_arena {
            let direct_source_file_arena =
                allow_source_file_arena && is_direct_lowering_source_file_arena(interface_arena);
            let name_resolver = |type_name: &str| -> Option<tsz_solver::def::DefId> {
                if direct_source_file_arena {
                    return self.source_file_local_name_def_id_for_lowering(
                        delegate_binder,
                        interface_arena,
                        type_name,
                    );
                }
                (!self.ctx.file_local_type_shadow_for_lib_name(type_name))
                    .then(|| self.resolve_actual_lib_name_to_def_id_for_lowering(type_name))
                    .flatten()
                    .or_else(|| self.resolve_entity_name_text_to_def_id_for_lowering(type_name))
            };
            let no_type_symbol = |_node_idx: NodeIndex| -> Option<u32> { None };
            let no_def_id = |_node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> { None };
            let no_value_symbol = |_node_idx: NodeIndex| -> Option<u32> { None };
            let lazy_type_params_resolver =
                |def_id: tsz_solver::def::DefId| self.ctx.get_def_type_params(def_id);
            let lowering = TypeLowering::with_hybrid_resolver(
                interface_arena,
                self.ctx.types,
                &no_type_symbol,
                &no_def_id,
                &no_value_symbol,
            )
            .with_builtin_iterator_return_type(self.builtin_iterator_return_intrinsic_type())
            .with_name_def_id_resolver(&name_resolver)
            .with_lazy_type_params_resolver(&lazy_type_params_resolver)
            .prefer_name_def_id_resolution();
            let (params, lowered_members) =
                lowering.lower_interface_members_simple_types(interface_idx, member_indices)?;
            let substitution = type_args
                .filter(|type_args| !params.is_empty() && type_args.len() <= params.len())
                .and_then(|type_args| {
                    crate::query_boundaries::type_defaults::fill_application_defaults(
                        self.ctx.types,
                        type_args,
                        &params,
                    )
                })
                .map(|type_args| {
                    crate::query_boundaries::common::TypeSubstitution::from_args(
                        self.ctx.types,
                        &params,
                        &type_args,
                    )
                });

            let mut results = rustc_hash::FxHashMap::default();
            for (member_idx, mut member_type) in lowered_members {
                if matches!(member_type, TypeId::UNKNOWN | TypeId::ERROR) {
                    return None;
                }
                if let Some(substitution) = substitution.as_ref() {
                    member_type = crate::query_boundaries::common::instantiate_type(
                        self.ctx.types,
                        member_type,
                        substitution,
                    );
                }
                if matches!(member_type, TypeId::UNKNOWN | TypeId::ERROR) {
                    return None;
                }
                results.insert(member_idx, member_type);
            }

            return (!results.is_empty()).then_some(results);
        }

        let sym_id = delegate_binder.get_node_symbol(interface_idx).or_else(|| {
            let arena_ptr = interface_arena as *const NodeArena as usize;
            self.ctx
                .cross_file_node_symbols_for_arena(delegate_binder, arena_ptr)
                .and_then(|symbols| symbols.get(&interface_idx.0).copied())
        })?;

        let (interface_type, params) = self.direct_cross_file_interface_lowering(
            sym_id,
            delegate_binder,
            interface_arena,
            true,
            allow_source_file_arena,
        )?;

        let substitution = type_args
            .filter(|type_args| !params.is_empty() && type_args.len() <= params.len())
            .and_then(|type_args| {
                crate::query_boundaries::type_defaults::fill_application_defaults(
                    self.ctx.types,
                    type_args,
                    &params,
                )
            })
            .map(|type_args| {
                crate::query_boundaries::common::TypeSubstitution::from_args(
                    self.ctx.types,
                    &params,
                    &type_args,
                )
            });

        let mut results = rustc_hash::FxHashMap::default();
        for &member_idx in member_indices {
            let Some(member_node) = interface_arena.get(member_idx) else {
                continue;
            };
            let name_idx = interface_arena
                .get_signature(member_node)
                .map(|signature| signature.name)
                .or_else(|| {
                    interface_arena
                        .get_accessor(member_node)
                        .map(|accessor| accessor.name)
                });
            let Some(name) = name_idx.and_then(|idx| {
                crate::types_domain::queries::core::get_literal_property_name(interface_arena, idx)
            }) else {
                continue;
            };
            let atom = self.ctx.types.intern_string(&name);
            let Some(mut member_type) = crate::query_boundaries::common::raw_property_type(
                self.ctx.types,
                interface_type,
                atom,
            ) else {
                continue;
            };
            if let Some(substitution) = substitution.as_ref() {
                member_type = crate::query_boundaries::common::instantiate_type(
                    self.ctx.types,
                    member_type,
                    substitution,
                );
            }
            if member_type != TypeId::UNKNOWN && member_type != TypeId::ERROR {
                results.insert(member_idx, member_type);
            }
        }

        (!results.is_empty()).then_some(results)
    }
}

#[cfg(test)]
#[path = "cross_file_direct_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "cross_file_direct_cached_base_tests.rs"]
mod cached_base_tests;
