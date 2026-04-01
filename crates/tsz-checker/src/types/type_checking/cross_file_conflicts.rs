//! Cross-file interface member conflict checking and utility helpers.
//!
//! Extracted from `duplicate_identifiers.rs` to keep file sizes manageable.
//! Contains:
//! - Cross-file interface/global/module augmentation member conflict detection
//! - Built-in global identifier conflict checking (TS2397)
//! - Declaration utility helpers (`function_has_body`, `get_access_modifier`, `is_declaration_optional`)

use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

const INTERFACE_MEMBER_KIND_PROPERTY: u8 = 1;
const INTERFACE_MEMBER_KIND_METHOD: u8 = 1 << 1;
const CROSS_FILE_INTERFACE_MEMBER_CONFLICT_LIMIT: usize = 8;

impl<'a> CheckerState<'a> {
    /// Check cross-file interface member conflicts (property vs method with same name).
    ///
    /// When the same interface is declared across files, and one file uses a property
    /// signature while the other uses a method signature for the same member name,
    /// tsc emits TS2300 "Duplicate identifier" on the local declarations.
    pub(crate) fn check_cross_file_interface_member_conflicts(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        local_interface_decls: &[NodeIndex],
        remote_interface_decls: &[NodeIndex],
    ) {
        let mut remote_members = FxHashMap::default();

        for &decl_idx in remote_interface_decls {
            let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) else {
                continue;
            };
            for remote_arena in arenas
                .iter()
                .filter(|arena| !std::ptr::eq(&***arena, self.ctx.arena))
            {
                self.collect_interface_member_kinds(remote_arena, decl_idx, &mut remote_members);
            }
        }

        self.report_cross_file_interface_member_conflicts(local_interface_decls, &remote_members);
    }

    pub(crate) fn check_cross_file_global_augmentation_member_conflicts(&mut self) {
        let Some(_arenas) = self.ctx.all_arenas.as_ref() else {
            return;
        };

        let grouped_augmentations: Vec<
            Vec<(
                NodeIndex,
                std::sync::Arc<tsz_parser::parser::node::NodeArena>,
            )>,
        > = self
            .ctx
            .binder
            .global_augmentations
            .values()
            .map(|augmentations| {
                augmentations
                    .iter()
                    .filter_map(|augmentation| {
                        Some((augmentation.node, augmentation.arena.as_ref()?.clone()))
                    })
                    .collect()
            })
            .collect();

        for augmentations in grouped_augmentations {
            let mut local_interface_decls = Vec::new();
            let mut remote_members = FxHashMap::default();

            for (decl_idx, arena) in augmentations {
                let Some(node) = arena.get(decl_idx) else {
                    continue;
                };
                let is_local = std::ptr::eq(arena.as_ref(), self.ctx.arena);
                if node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
                    if is_local {
                        local_interface_decls.push(decl_idx);
                    } else {
                        self.collect_interface_member_kinds(
                            arena.as_ref(),
                            decl_idx,
                            &mut remote_members,
                        );
                    }
                    continue;
                }
            }

            self.report_cross_file_interface_member_conflicts(
                &local_interface_decls,
                &remote_members,
            );
        }

        self.check_cross_file_global_augmentation_namespace_member_conflicts();
    }

    pub(crate) fn check_cross_file_module_augmentation_member_conflicts(&mut self) {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(all_arenas) = self.ctx.all_arenas.clone() else {
            return;
        };

        let mut local_interface_decls_by_module = FxHashMap::default();
        let mut remote_members_by_module = FxHashMap::default();

        for arena in &*all_arenas {
            let is_local = std::ptr::eq(arena.as_ref(), self.ctx.arena);
            let Some(source_file) = arena.source_files.first() else {
                continue;
            };

            for &stmt_idx in &source_file.statements.nodes {
                let Some(module_node) = arena.get(stmt_idx) else {
                    continue;
                };
                let Some(module_decl) = arena.get_module(module_node) else {
                    continue;
                };
                if module_decl.body.is_none() {
                    continue;
                }

                let Some(module_name_node) = arena.get(module_decl.name) else {
                    continue;
                };
                if module_name_node.kind != SyntaxKind::StringLiteral as u16
                    && module_name_node.kind != SyntaxKind::NoSubstitutionTemplateLiteral as u16
                {
                    continue;
                }
                let Some(module_name_lit) = arena.get_literal(module_name_node) else {
                    continue;
                };
                if module_name_lit.text.is_empty() {
                    continue;
                }
                let module_spec = module_name_lit.text.clone();

                let Some(body_node) = arena.get(module_decl.body) else {
                    continue;
                };
                if body_node.kind != syntax_kind_ext::MODULE_BLOCK {
                    continue;
                }
                let Some(block) = arena.get_module_block(body_node) else {
                    continue;
                };
                let Some(statements) = &block.statements else {
                    continue;
                };

                for &inner_idx in &statements.nodes {
                    let Some(stmt_node) = arena.get(inner_idx) else {
                        continue;
                    };
                    let decl_idx = if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                        let Some(export_decl) = arena.get_export_decl(stmt_node) else {
                            continue;
                        };
                        export_decl.export_clause
                    } else {
                        inner_idx
                    };
                    let Some(inner_node) = arena.get(decl_idx) else {
                        continue;
                    };
                    let Some(iface) = arena.get_interface(inner_node) else {
                        continue;
                    };
                    if !self.is_declaration_exported(arena, decl_idx) {
                        continue;
                    }
                    let Some(interface_name) = arena
                        .get_identifier_at(iface.name)
                        .map(|ident| ident.escaped_text.clone())
                    else {
                        continue;
                    };

                    let key = (module_spec.clone(), interface_name);
                    if is_local {
                        local_interface_decls_by_module
                            .entry(key)
                            .or_insert_with(Vec::new)
                            .push(decl_idx);
                    } else {
                        self.collect_interface_member_kinds(
                            arena.as_ref(),
                            decl_idx,
                            remote_members_by_module.entry(key).or_default(),
                        );
                    }
                }
            }
        }

        for (key, local_interface_decls) in local_interface_decls_by_module {
            let Some(remote_members) = remote_members_by_module.get(&key) else {
                continue;
            };
            self.report_cross_file_interface_member_conflicts(
                &local_interface_decls,
                remote_members,
            );
        }

        self.check_cross_file_module_augmentation_top_level_name_conflicts();
    }

    fn check_cross_file_module_augmentation_top_level_name_conflicts(&mut self) {
        use crate::diagnostics::diagnostic_codes;
        use tsz_parser::parser::syntax_kind_ext;

        let Some(source_file) = self.ctx.arena.source_files.first() else {
            return;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(module_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            let Some(module_decl) = self.ctx.arena.get_module(module_node) else {
                continue;
            };
            if module_decl.body.is_none() {
                continue;
            }

            let Some(module_name_node) = self.ctx.arena.get(module_decl.name) else {
                continue;
            };
            if module_name_node.kind != SyntaxKind::StringLiteral as u16
                && module_name_node.kind != SyntaxKind::NoSubstitutionTemplateLiteral as u16
            {
                continue;
            }
            let Some(module_name_lit) = self.ctx.arena.get_literal(module_name_node) else {
                continue;
            };
            let Some(target_idx) = self
                .ctx
                .resolve_import_target_from_file(self.ctx.current_file_idx, &module_name_lit.text)
            else {
                continue;
            };

            let Some(body_node) = self.ctx.arena.get(module_decl.body) else {
                continue;
            };
            if body_node.kind != syntax_kind_ext::MODULE_BLOCK {
                continue;
            }
            let Some(block) = self.ctx.arena.get_module_block(body_node) else {
                continue;
            };
            let Some(statements) = &block.statements else {
                continue;
            };

            for &inner_idx in &statements.nodes {
                let Some(stmt_node) = self.ctx.arena.get(inner_idx) else {
                    continue;
                };
                let decl_idx = if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                    let Some(export_decl) = self.ctx.arena.get_export_decl(stmt_node) else {
                        continue;
                    };
                    export_decl.export_clause
                } else {
                    inner_idx
                };
                let Some(sym_id) = self.ctx.binder.get_node_symbol(decl_idx) else {
                    continue;
                };
                let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                    continue;
                };

                let has_conflict = self
                    .module_augmentation_top_level_name_conflicts_with_target_export_surface(
                        decl_idx,
                        target_idx,
                        &symbol.escaped_name,
                    );
                if !has_conflict {
                    continue;
                }

                let error_node = self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                self.error_at_node_msg(
                    error_node,
                    diagnostic_codes::DUPLICATE_IDENTIFIER,
                    &[&symbol.escaped_name],
                );
            }
        }

        self.check_target_file_exports_conflicting_with_module_augmentations();
    }

    fn check_target_file_exports_conflicting_with_module_augmentations(&mut self) {
        use crate::diagnostics::diagnostic_codes;
        use tsz_parser::parser::syntax_kind_ext;

        let Some(source_file) = self.ctx.arena.source_files.first() else {
            return;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export_decl) = self.ctx.arena.get_export_decl(stmt_node) else {
                continue;
            };
            let Some(clause_node) = self.ctx.arena.get(export_decl.export_clause) else {
                continue;
            };
            if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
                continue;
            }
            let Some(named_exports) = self.ctx.arena.get_named_imports(clause_node) else {
                continue;
            };

            for &spec_idx in &named_exports.elements.nodes {
                let Some(spec_node) = self.ctx.arena.get(spec_idx) else {
                    continue;
                };
                let Some(spec) = self.ctx.arena.get_specifier(spec_node) else {
                    continue;
                };
                let Some(export_name) = self
                    .ctx
                    .arena
                    .get(spec.property_name)
                    .and_then(|n| self.ctx.arena.get_identifier(n))
                    .or_else(|| {
                        self.ctx
                            .arena
                            .get(spec.name)
                            .and_then(|n| self.ctx.arena.get_identifier(n))
                    })
                    .map(|ident| ident.escaped_text.clone())
                else {
                    continue;
                };

                if self
                    .module_augmentation_conflict_declarations_for_current_file(&export_name)
                    .is_empty()
                {
                    continue;
                }

                self.error_at_node_msg(
                    spec_idx,
                    diagnostic_codes::DUPLICATE_IDENTIFIER,
                    &[&export_name],
                );
            }
        }
    }

    fn target_file_has_direct_export_named(&self, file_idx: usize, export_name: &str) -> bool {
        let Some(binder) = self.ctx.get_binder_for_file(file_idx) else {
            return false;
        };
        let arena = self.ctx.get_arena_for_file(file_idx as u32);
        let Some(file_name) = arena.source_files.first().map(|sf| sf.file_name.clone()) else {
            return false;
        };
        binder
            .module_exports
            .get(&file_name)
            .is_some_and(|exports| exports.get(export_name).is_some())
    }

    fn module_augmentation_top_level_name_conflicts_with_target_export_surface(
        &self,
        decl_idx: NodeIndex,
        target_idx: usize,
        export_name: &str,
    ) -> bool {
        let Some(local_flags) = self.declaration_symbol_flags(self.ctx.arena, decl_idx) else {
            return self.target_file_has_direct_export_named(target_idx, export_name);
        };

        let target_decls = self.export_surface_declarations_in_file(target_idx, export_name);
        if target_decls.is_empty() {
            return self.target_file_has_direct_export_named(target_idx, export_name);
        }

        target_decls.into_iter().all(|(_, target_flags, _)| {
            !tsz_binder::BinderState::can_merge_flags(target_flags, local_flags)
        })
    }

    fn collect_interface_member_kinds(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        decl_idx: NodeIndex,
        members: &mut FxHashMap<String, u8>,
    ) {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = arena.get(decl_idx) else {
            return;
        };
        let Some(iface) = arena.get_interface(node) else {
            return;
        };

        for &member_idx in &iface.members.nodes {
            let Some(member_node) = arena.get(member_idx) else {
                continue;
            };
            let kind = match member_node.kind {
                syntax_kind_ext::PROPERTY_SIGNATURE => INTERFACE_MEMBER_KIND_PROPERTY,
                syntax_kind_ext::METHOD_SIGNATURE => INTERFACE_MEMBER_KIND_METHOD,
                _ => continue,
            };
            let Some(sig) = arena.get_signature(member_node) else {
                continue;
            };
            let Some(name) =
                crate::types_domain::queries::core::get_literal_property_name(arena, sig.name)
            else {
                continue;
            };
            members
                .entry(name)
                .and_modify(|existing| *existing |= kind)
                .or_insert(kind);
        }
    }

    fn collect_namespace_member_flags(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        decl_idx: NodeIndex,
        members: &mut FxHashMap<String, u32>,
    ) {
        for (name, _name_idx, flags) in self.namespace_member_declarations(arena, decl_idx) {
            members
                .entry(name)
                .and_modify(|existing| *existing |= flags)
                .or_insert(flags);
        }
    }

    fn check_cross_file_global_augmentation_namespace_member_conflicts(&mut self) {
        use tsz_parser::parser::node_flags;

        let Some(all_arenas) = self.ctx.all_arenas.clone() else {
            return;
        };

        let mut local_namespaces = FxHashMap::default();
        let mut remote_namespaces = FxHashMap::default();

        for arena in &*all_arenas {
            let is_local = std::ptr::eq(arena.as_ref(), self.ctx.arena);
            let Some(source_file) = arena.source_files.first() else {
                continue;
            };

            for &stmt_idx in &source_file.statements.nodes {
                let Some(stmt_node) = arena.get(stmt_idx) else {
                    continue;
                };
                if stmt_node.kind != syntax_kind_ext::MODULE_DECLARATION {
                    continue;
                }
                let Some(module_decl) = arena.get_module(stmt_node) else {
                    continue;
                };
                let is_global_augmentation =
                    (u32::from(stmt_node.flags) & node_flags::GLOBAL_AUGMENTATION) != 0
                        || arena
                            .get(module_decl.name)
                            .and_then(|name_node| arena.get_identifier(name_node))
                            .is_some_and(|ident| ident.escaped_text == "global");
                if !is_global_augmentation {
                    continue;
                }
                let Some(body_node) = arena.get(module_decl.body) else {
                    continue;
                };
                let Some(block) = arena.get_module_block(body_node) else {
                    continue;
                };
                let Some(statements) = &block.statements else {
                    continue;
                };

                for &inner_idx in &statements.nodes {
                    let Some(inner_node) = arena.get(inner_idx) else {
                        continue;
                    };
                    if inner_node.kind != syntax_kind_ext::MODULE_DECLARATION {
                        continue;
                    }
                    let Some(namespace_decl) = arena.get_module(inner_node) else {
                        continue;
                    };
                    let Some(name) = arena
                        .get(namespace_decl.name)
                        .and_then(|name_node| arena.get_identifier(name_node))
                        .map(|ident| ident.escaped_text.to_string())
                    else {
                        continue;
                    };

                    if is_local {
                        local_namespaces
                            .entry(name)
                            .or_insert_with(Vec::new)
                            .push(inner_idx);
                    } else {
                        self.collect_namespace_member_flags(
                            arena.as_ref(),
                            inner_idx,
                            remote_namespaces.entry(name).or_default(),
                        );
                    }
                }
            }
        }

        for (name, local_namespace_decls) in local_namespaces {
            let Some(remote_members) = remote_namespaces.get(&name) else {
                continue;
            };
            self.report_cross_file_namespace_member_conflicts(
                &local_namespace_decls,
                remote_members,
            );
        }
    }

    fn report_cross_file_namespace_member_conflicts(
        &mut self,
        local_namespace_decls: &[NodeIndex],
        remote_members: &FxHashMap<String, u32>,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        if local_namespace_decls.is_empty() || remote_members.is_empty() {
            return;
        }

        for &decl_idx in local_namespace_decls {
            for (name, name_idx, flags) in
                self.namespace_member_declarations(self.ctx.arena, decl_idx)
            {
                let Some(&remote_flags) = remote_members.get(&name) else {
                    continue;
                };
                if !Self::declarations_conflict(flags, remote_flags) {
                    continue;
                }

                let message = format_message(diagnostic_messages::DUPLICATE_IDENTIFIER, &[&name]);
                self.error_at_node(name_idx, &message, diagnostic_codes::DUPLICATE_IDENTIFIER);
            }
        }
    }

    fn namespace_member_declarations(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        decl_idx: NodeIndex,
    ) -> Vec<(String, NodeIndex, u32)> {
        let Some(node) = arena.get(decl_idx) else {
            return Vec::new();
        };
        let Some(module_decl) = arena.get_module(node) else {
            return Vec::new();
        };
        let Some(body_node) = arena.get(module_decl.body) else {
            return Vec::new();
        };
        let Some(block) = arena.get_module_block(body_node) else {
            return Vec::new();
        };
        let Some(statements) = &block.statements else {
            return Vec::new();
        };

        let mut members = Vec::new();
        for &stmt_idx in &statements.nodes {
            self.collect_namespace_statement_declarations(arena, stmt_idx, &mut members);
        }
        members
    }

    fn collect_namespace_statement_declarations(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        stmt_idx: NodeIndex,
        members: &mut Vec<(String, NodeIndex, u32)>,
    ) {
        let Some(stmt_node) = arena.get(stmt_idx) else {
            return;
        };

        if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
            if let Some(export_decl) = arena.get_export_decl(stmt_node)
                && export_decl.export_clause.is_some()
            {
                self.collect_namespace_statement_declarations(
                    arena,
                    export_decl.export_clause,
                    members,
                );
            }
            return;
        }

        if stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
            let Some(var_stmt) = arena.get_variable(stmt_node) else {
                return;
            };
            for &decl_list_idx in &var_stmt.declarations.nodes {
                let Some(decl_list_node) = arena.get(decl_list_idx) else {
                    continue;
                };
                let Some(decl_list) = arena.get_variable(decl_list_node) else {
                    continue;
                };
                for &decl_idx in &decl_list.declarations.nodes {
                    let Some(decl_node) = arena.get(decl_idx) else {
                        continue;
                    };
                    let Some(var_decl) = arena.get_variable_declaration(decl_node) else {
                        continue;
                    };
                    let Some(ident) = arena.get_identifier_at(var_decl.name) else {
                        continue;
                    };
                    let Some(flags) = self.declaration_symbol_flags(arena, decl_idx) else {
                        continue;
                    };
                    members.push((
                        ident.escaped_text.to_string(),
                        var_decl.name,
                        self.normalize_namespace_member_flags(arena, decl_idx, flags),
                    ));
                }
            }
            return;
        }

        let Some(name_idx) = self.get_declaration_name_node_in_arena(arena, stmt_idx) else {
            return;
        };
        let Some(name) = arena
            .get(name_idx)
            .and_then(|name_node| arena.get_identifier(name_node))
            .map(|ident| ident.escaped_text.to_string())
        else {
            return;
        };
        let Some(flags) = self.declaration_symbol_flags(arena, stmt_idx) else {
            return;
        };
        members.push((
            name,
            name_idx,
            self.normalize_namespace_member_flags(arena, stmt_idx, flags),
        ));
    }

    fn normalize_namespace_member_flags(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        decl_idx: NodeIndex,
        mut flags: u32,
    ) -> u32 {
        let Some(node) = arena.get(decl_idx) else {
            return flags;
        };
        if node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            return flags;
        }
        let Some(import_decl) = arena.get_import_decl(node) else {
            return flags;
        };
        let Some(target_node) = arena.get(import_decl.module_specifier) else {
            return flags;
        };
        if target_node.kind == SyntaxKind::Identifier as u16
            || target_node.kind == syntax_kind_ext::QUALIFIED_NAME
        {
            // Namespace import aliases can contribute a type-space member even when the
            // binder stores them as pure aliases. Include that meaning so merged
            // namespace members like `export import VNode = react.ReactNode` conflict
            // with `type VNode = ...` the same way tsc reports TS2300.
            flags |= tsz_binder::symbol_flags::TYPE_ALIAS;
        }
        flags
    }

    fn get_declaration_name_node_in_arena(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        decl_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let node = arena.get(decl_idx)?;
        match node.kind {
            syntax_kind_ext::FUNCTION_DECLARATION => arena.get_function(node).map(|decl| decl.name),
            syntax_kind_ext::CLASS_DECLARATION => arena.get_class(node).map(|decl| decl.name),
            syntax_kind_ext::INTERFACE_DECLARATION => {
                arena.get_interface(node).map(|decl| decl.name)
            }
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                arena.get_type_alias(node).map(|decl| decl.name)
            }
            syntax_kind_ext::ENUM_DECLARATION => arena.get_enum(node).map(|decl| decl.name),
            syntax_kind_ext::MODULE_DECLARATION => arena.get_module(node).map(|decl| decl.name),
            syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                arena.get_import_decl(node).map(|decl| decl.import_clause)
            }
            _ => None,
        }
    }

    fn report_cross_file_interface_member_conflicts(
        &mut self,
        local_interface_decls: &[NodeIndex],
        remote_members: &FxHashMap<String, u8>,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_parser::parser::syntax_kind_ext;

        if local_interface_decls.is_empty() || remote_members.is_empty() {
            return;
        }

        let mut conflict_names = Vec::new();
        let mut seen_conflict_names = FxHashSet::default();
        let mut conflict_name_nodes = Vec::new();
        let mut anchor_nodes = Vec::new();
        let mut seen_anchor_nodes = FxHashSet::default();

        for &decl_idx in local_interface_decls {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(iface) = self.ctx.arena.get_interface(node) else {
                continue;
            };
            let anchor_node = self.interface_member_conflict_anchor_node(decl_idx);
            let mut decl_has_conflict = false;

            for &member_idx in &iface.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let local_kind = match member_node.kind {
                    syntax_kind_ext::PROPERTY_SIGNATURE => INTERFACE_MEMBER_KIND_PROPERTY,
                    syntax_kind_ext::METHOD_SIGNATURE => INTERFACE_MEMBER_KIND_METHOD,
                    _ => continue,
                };
                let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                    continue;
                };
                let Some(name) = self.get_property_name(sig.name) else {
                    continue;
                };
                let Some(&remote_kinds) = remote_members.get(&name) else {
                    continue;
                };
                let opposite_kind = if local_kind == INTERFACE_MEMBER_KIND_METHOD {
                    INTERFACE_MEMBER_KIND_PROPERTY
                } else {
                    INTERFACE_MEMBER_KIND_METHOD
                };
                if (remote_kinds & opposite_kind) == 0 {
                    continue;
                }

                decl_has_conflict = true;
                if seen_conflict_names.insert(name.clone()) {
                    conflict_names.push(name.clone());
                }
                conflict_name_nodes.push((name, sig.name));
            }

            if decl_has_conflict && seen_anchor_nodes.insert(anchor_node) {
                anchor_nodes.push(anchor_node);
            }
        }

        if conflict_name_nodes.is_empty() {
            return;
        }

        if conflict_names.len() >= CROSS_FILE_INTERFACE_MEMBER_CONFLICT_LIMIT {
            let list = conflict_names.join(", ");
            let message = format_message(
                diagnostic_messages::DEFINITIONS_OF_THE_FOLLOWING_IDENTIFIERS_CONFLICT_WITH_THOSE_IN_ANOTHER_FILE,
                &[&list],
            );
            for anchor_node in anchor_nodes {
                self.error_at_node(
                    anchor_node,
                    &message,
                    diagnostic_codes::DEFINITIONS_OF_THE_FOLLOWING_IDENTIFIERS_CONFLICT_WITH_THOSE_IN_ANOTHER_FILE,
                );
            }
            return;
        }

        for (name, node_idx) in conflict_name_nodes {
            let message = format_message(diagnostic_messages::DUPLICATE_IDENTIFIER, &[&name]);
            self.error_at_node(node_idx, &message, diagnostic_codes::DUPLICATE_IDENTIFIER);
        }
    }

    fn interface_member_conflict_anchor_node(&self, decl_idx: NodeIndex) -> NodeIndex {
        let enclosing_namespace = self.get_enclosing_namespace(decl_idx);
        if enclosing_namespace.is_none() {
            decl_idx
        } else {
            enclosing_namespace
        }
    }

    /// Check for declarations that conflict with built-in global identifiers (TS2397).
    ///
    /// TypeScript protects the built-in global names `undefined` and `globalThis`
    /// from being redeclared:
    /// - `var undefined = null;` → TS2397 (value declaration of `undefined`)
    /// - `namespace globalThis {}` → TS2397 (in non-module/script files)
    /// - `var globalThis;` → TS2397 (in non-module/script files)
    ///
    /// Type declarations (interfaces, type aliases, etc.) named `undefined` are
    /// allowed — `checkTypeNameIsReserved` handles those separately.
    pub(crate) fn check_built_in_global_identifier_conflicts(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_parser::parser::syntax_kind_ext;

        let is_external_module = self
            .ctx
            .is_external_module_by_file
            .as_ref()
            .and_then(|m| m.get(&self.ctx.file_name))
            .copied()
            .unwrap_or_else(|| self.ctx.binder.is_external_module());

        // Check `undefined` redeclaration.
        // tsc checks if `undefined` exists in globals and emits TS2397 for each
        // non-type declaration. We check the file-level locals.
        if let Some(sym_id) = self.ctx.binder.file_locals.get("undefined")
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            for &decl_idx in &symbol.declarations {
                let Some(node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                // Skip type and namespace declarations (interfaces, type aliases,
                // modules/namespaces).  TSC only emits TS2397 for value
                // declarations (var/let/const/function) of `undefined`.
                if node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                    || node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || node.kind == syntax_kind_ext::MODULE_DECLARATION
                {
                    continue;
                }
                let error_node = self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                let message = format_message(
                    diagnostic_messages::DECLARATION_NAME_CONFLICTS_WITH_BUILT_IN_GLOBAL_IDENTIFIER,
                    &["undefined"],
                );
                self.error_at_node(
                    error_node,
                    &message,
                    diagnostic_codes::DECLARATION_NAME_CONFLICTS_WITH_BUILT_IN_GLOBAL_IDENTIFIER,
                );
            }
        }

        // Check `globalThis` redeclaration (only in non-module files).
        // In module files (files with import/export), `globalThis` declarations
        // are allowed because they don't conflict with the global scope.
        if !is_external_module
            && let Some(sym_id) = self.ctx.binder.file_locals.get("globalThis")
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            for &decl_idx in &symbol.declarations {
                let error_node = self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                let message = format_message(
                    diagnostic_messages::DECLARATION_NAME_CONFLICTS_WITH_BUILT_IN_GLOBAL_IDENTIFIER,
                    &["globalThis"],
                );
                self.error_at_node(
                    error_node,
                    &message,
                    diagnostic_codes::DECLARATION_NAME_CONFLICTS_WITH_BUILT_IN_GLOBAL_IDENTIFIER,
                );
            }
        }
    }

    /// Check if a function declaration has a body (is an implementation, not just a signature).
    pub(crate) fn function_has_body(&self, decl_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
            return false;
        }
        let Some(func) = self.ctx.arena.get_function(node) else {
            return false;
        };
        func.body.is_some()
    }

    /// Get the access modifier of a declaration: 0 = public (default), 1 = private, 2 = protected.
    pub(crate) fn get_access_modifier(&self, decl_idx: NodeIndex) -> u8 {
        use tsz_parser::parser::syntax_kind_ext;
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return 0;
        };
        let modifiers = match node.kind {
            syntax_kind_ext::METHOD_DECLARATION => self
                .ctx
                .arena
                .get_method_decl(node)
                .and_then(|m| m.modifiers.as_ref()),
            syntax_kind_ext::FUNCTION_DECLARATION => self
                .ctx
                .arena
                .get_function(node)
                .and_then(|f| f.modifiers.as_ref()),
            syntax_kind_ext::METHOD_SIGNATURE => self
                .ctx
                .arena
                .get_signature(node)
                .and_then(|s| s.modifiers.as_ref()),
            _ => None,
        };
        let Some(mods) = modifiers else {
            return 0;
        };
        if self
            .ctx
            .arena
            .has_modifier_ref(Some(mods), SyntaxKind::PrivateKeyword)
        {
            1
        } else if self
            .ctx
            .arena
            .has_modifier_ref(Some(mods), SyntaxKind::ProtectedKeyword)
        {
            2
        } else {
            0
        }
    }

    /// Check if a method declaration or signature is optional (has `question_token`).
    pub(crate) fn is_declaration_optional(&self, decl_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        match node.kind {
            syntax_kind_ext::METHOD_DECLARATION => self
                .ctx
                .arena
                .get_method_decl(node)
                .is_some_and(|m| m.question_token),
            syntax_kind_ext::METHOD_SIGNATURE => self
                .ctx
                .arena
                .get_signature(node)
                .is_some_and(|s| s.question_token),
            _ => false,
        }
    }
}
