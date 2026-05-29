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

    /// Namespace-qualifier of a lib interface symbol (e.g. `Temporal`), derived
    /// from the enclosing `module`/`namespace` declarations of its declarations.
    fn lib_symbol_namespace_prefix(
        &self,
        sym_id: SymbolId,
        symbol: &tsz_binder::Symbol,
    ) -> Option<String> {
        symbol.declarations.iter().find_map(|&decl_idx| {
            self.ctx
                .binder
                .declaration_arenas
                .get(&(sym_id, decl_idx))
                .and_then(|arenas| {
                    arenas.iter().find_map(|arena| {
                        Self::lib_interface_namespace_prefix(&[(decl_idx, arena.as_ref())])
                    })
                })
        })
    }

    /// Whether any declaration of a lib interface symbol has an `extends` clause.
    fn lib_interface_declarations_have_heritage(
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
                        let arena = arena.as_ref();
                        arena
                            .get(decl_idx)
                            .and_then(|node| arena.get_interface(node))
                            .and_then(|interface| interface.heritage_clauses.as_ref())
                            .is_some_and(|clauses| !clauses.nodes.is_empty())
                    })
                })
        })
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
                crate::query_boundaries::definition_identity::lazy_def_id(
                    self.ctx.types,
                    alias_type,
                )
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
            && (generic_actual_lib_alias_body_has_direct_shape(self.ctx.types, body)
                // Lib string intrinsic aliases lower from the `intrinsic`
                // marker and get their structural representation at use sites.
                // This helper is restricted to compiler-managed built-ins and
                // still runs after actual-lib declaration proof above.
                || common::is_compiler_managed_type(name));
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
        if let Some(result) = self.direct_builtin_lib_interface_symbol_type(
            sym_id,
            delegate_arena_source,
            delegate_arena,
            needs_cross_file_delegation,
        ) {
            return Some(result);
        }
        if let Some(result) = self.direct_value_merged_builtin_lib_interface_symbol_type(
            sym_id,
            delegate_arena_source,
            delegate_arena,
            needs_cross_file_delegation,
        ) {
            return Some(result);
        }

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
        let intl_namespace_export =
            self.symbol_is_actual_lib_namespace_export("Intl", &name, sym_id);
        if !symbol.has_any_flags(symbol_flags::TYPE) {
            return None;
        }

        // Namespaced lib interfaces (e.g. `Temporal.RoundingOptionsWithLargestUnit`,
        // `Intl.*`) that `extends` a base interface declared in the same namespace
        // need their inherited members merged in. The lightweight direct lowerings
        // below emit only an interface's own body, so the qualified name is
        // computed here and used to run the namespace-aware heritage merge on the
        // lowered body before returning (see the end of this method).
        let heritage_merge_name = (symbol.has_any_flags(symbol_flags::INTERFACE)
            && !symbol.has_any_flags(symbol_flags::TYPE_ALIAS)
            && self.lib_interface_declarations_have_heritage(sym_id, &symbol))
        .then(|| self.lib_symbol_namespace_prefix(sym_id, &symbol))
        .flatten()
        .filter(|prefix| self.symbol_is_actual_lib_namespace_export(prefix, &name, sym_id))
        .map(|prefix| format!("{prefix}.{name}"));
        let proven_value_interface =
            self.symbol_is_proven_direct_actual_lib_value_interface(sym_id, &symbol, &name);
        let protocol_method_interface =
            self.symbol_declares_direct_actual_lib_protocol_method(sym_id, &symbol, delegate_arena);
        let admitted_value_interface = proven_value_interface || protocol_method_interface;
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
            && iterator_object_has_global_augmentations(&self.ctx)
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
        // Merge inherited members for a namespaced interface that `extends` a
        // base declared in the same namespace. The body-only lowerings above
        // drop them; this runs the namespace-aware heritage merge keyed by the
        // qualified name so the base interface members are instantiated in.
        let (direct_type, params) = match heritage_merge_name {
            Some(merge_name) => {
                let merged = self.merge_lib_interface_heritage(direct_type, &merge_name);
                (merged, params)
            }
            None => (direct_type, params),
        };
        self.ctx.symbol_types.insert(sym_id, direct_type);
        self.ctx
            .lib_delegation_cache
            .insert_symbol_type(sym_id, (direct_type, params.clone()));
        Some((direct_type, params))
    }
}
