use super::cross_file_direct_files::is_builtin_lib_declaration_arena;
use crate::state::CheckerState;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    pub(super) fn interface_declarations_have_heritage(
        declarations: &[(NodeIndex, &NodeArena)],
    ) -> bool {
        declarations.iter().any(|(decl_idx, arena)| {
            let Some(node) = arena.get(*decl_idx) else {
                return false;
            };
            let Some(interface) = arena.get_interface(node) else {
                return false;
            };
            interface
                .heritage_clauses
                .as_ref()
                .is_some_and(|clauses| !clauses.nodes.is_empty())
        })
    }

    pub(super) fn interface_declarations_have_computed_names(
        declarations: &[(NodeIndex, &NodeArena)],
    ) -> bool {
        declarations.iter().any(|(decl_idx, arena)| {
            let Some(node) = arena.get(*decl_idx) else {
                return false;
            };
            let Some(interface) = arena.get_interface(node) else {
                return false;
            };
            interface.members.nodes.iter().copied().any(|member_idx| {
                let Some(member_node) = arena.get(member_idx) else {
                    return false;
                };
                let name_idx = arena
                    .get_signature(member_node)
                    .map(|signature| signature.name)
                    .or_else(|| {
                        arena
                            .get_accessor(member_node)
                            .map(|accessor| accessor.name)
                    });
                name_idx
                    .and_then(|idx| arena.get(idx))
                    .is_some_and(|name_node| {
                        name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                    })
            })
        })
    }

    pub(super) fn entity_name_text_in_direct_arena(
        arena: &NodeArena,
        idx: NodeIndex,
    ) -> Option<String> {
        let node = arena.get(idx)?;
        if node.is_identifier() {
            return arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.clone());
        }
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qualified = arena.get_qualified_name(node)?;
            let left = Self::entity_name_text_in_direct_arena(arena, qualified.left)?;
            let right = Self::entity_name_text_in_direct_arena(arena, qualified.right)?;
            return Some(format!("{left}.{right}"));
        }
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = arena.get_access_expr(node)
        {
            let left = Self::entity_name_text_in_direct_arena(arena, access.expression)?;
            let right = arena
                .get(access.name_or_argument)
                .and_then(|right_node| arena.get_identifier(right_node))?;
            return Some(format!("{left}.{}", right.escaped_text));
        }
        None
    }

    pub(super) fn interface_declarations_have_unsafe_builtin_heritage_base(
        &self,
        declarations: &[(NodeIndex, &NodeArena)],
        self_name: &str,
    ) -> bool {
        declarations.iter().any(|(decl_idx, arena)| {
            let Some(node) = arena.get(*decl_idx) else {
                return false;
            };
            let Some(interface) = arena.get_interface(node) else {
                return false;
            };
            let Some(heritage_clauses) = interface.heritage_clauses.as_ref() else {
                return false;
            };

            heritage_clauses.nodes.iter().copied().any(|clause_idx| {
                let Some(clause_node) = arena.get(clause_idx) else {
                    return false;
                };
                let Some(heritage) = arena.get_heritage_clause(clause_node) else {
                    return false;
                };
                if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                    return false;
                }

                heritage.types.nodes.iter().copied().any(|type_idx| {
                    let Some(type_node) = arena.get(type_idx) else {
                        return false;
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
                    let Some(base_name) = Self::entity_name_text_in_direct_arena(arena, expr_idx)
                    else {
                        return true;
                    };
                    // Direct lowering may inspect a derived lib interface while
                    // one of its bases is still being merged. Heritage merging
                    // owns cycle handling, so this guard only rejects bases that
                    // cannot be resolved through the same builtin-lib path.
                    base_name == self_name
                        || self.builtin_heritage_base_requires_fallback(&base_name)
                })
            })
        })
    }

    fn builtin_heritage_base_requires_fallback(&self, base_name: &str) -> bool {
        let normalized = base_name.strip_prefix("globalThis.").unwrap_or(base_name);
        if normalized.contains('.') {
            return true;
        }
        if normalized == "Array"
            || normalized.starts_with("Intl.")
            || self.lib_name_has_local_augmentation(normalized)
        {
            return true;
        }
        let Some(sym_id) = self.resolve_lib_symbol_by_name(normalized) else {
            return true;
        };
        let Some(arena) = self
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref)
        else {
            return true;
        };
        if !is_builtin_lib_declaration_arena(arena) {
            return true;
        }
        let Some(declarations) =
            self.cross_file_interface_declarations(sym_id, self.ctx.binder, arena)
        else {
            return true;
        };
        if !Self::interface_declarations_have_heritage(&declarations) {
            return false;
        }
        if self
            .ctx
            .lib_type_resolution_cache
            .get(normalized)
            .copied()
            .flatten()
            .is_some_and(|cached| self.cached_lib_type_is_usable(normalized, Some(cached)))
        {
            return false;
        }
        if !self.lib_name_locally_augmented(normalized)
            && let Some(shared) = &self.ctx.shared_lib_type_cache
            && let Some(Some(cached)) = shared.get(normalized).map(|entry| *entry)
            && self.cached_lib_type_is_usable(normalized, Some(cached))
        {
            return false;
        }
        false
    }

    fn computed_property_name_is_well_known_symbol(arena: &NodeArena, name_idx: NodeIndex) -> bool {
        let Some(name_node) = arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }
        let Some(computed) = arena.get_computed_property(name_node) else {
            return false;
        };
        let Some(expr_node) = arena.get(computed.expression) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = arena.get_access_expr(expr_node) else {
            return false;
        };
        let Some(base_node) = arena.get(access.expression) else {
            return false;
        };
        let Some(base_ident) = arena.get_identifier(base_node) else {
            return false;
        };
        base_ident.escaped_text == "Symbol"
            && arena
                .get(access.name_or_argument)
                .and_then(|name_node| arena.get_identifier(name_node))
                .is_some()
    }

    pub(super) fn interface_declarations_have_unsupported_computed_names(
        declarations: &[(NodeIndex, &NodeArena)],
    ) -> bool {
        declarations.iter().any(|(decl_idx, arena)| {
            let Some(node) = arena.get(*decl_idx) else {
                return false;
            };
            let Some(interface) = arena.get_interface(node) else {
                return false;
            };
            interface.members.nodes.iter().copied().any(|member_idx| {
                let Some(member_node) = arena.get(member_idx) else {
                    return false;
                };
                let name_idx = arena
                    .get_signature(member_node)
                    .map(|signature| signature.name)
                    .or_else(|| {
                        arena
                            .get_accessor(member_node)
                            .map(|accessor| accessor.name)
                    });
                name_idx
                    .and_then(|idx| arena.get(idx).map(|name_node| (idx, name_node)))
                    .is_some_and(|(idx, name_node)| {
                        name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                            && !Self::computed_property_name_is_well_known_symbol(arena, idx)
                    })
            })
        })
    }
}
