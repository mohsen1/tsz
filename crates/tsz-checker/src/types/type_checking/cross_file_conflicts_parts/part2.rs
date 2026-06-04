impl<'a> CheckerState<'a> {
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
    /// from being redeclared in script (non-module) files:
    /// - `var undefined = null;` → TS2397 (script file only)
    /// - `namespace globalThis {}` → TS2397 (script file only)
    /// - `var globalThis;` → TS2397 (script file only)
    ///
    /// In external modules both names are module-scoped and do not conflict.
    /// Type declarations (interfaces, type aliases, etc.) named `undefined` are
    /// allowed — `checkTypeNameIsReserved` handles those separately.
    pub(crate) fn check_built_in_global_identifier_conflicts(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_parser::parser::syntax_kind_ext;

        let is_external_module = self
            .ctx
            .is_external_module_by_file
            .as_ref()
            .and_then(|m| crate::context::lookup_is_external_module_in_map(m, &self.ctx.file_name))
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
                // Skip pure type declarations and class declarations.
                // Interfaces get TS2427, classes get TS2414, type aliases are type-only.
                if node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                    || node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || node.kind == syntax_kind_ext::CLASS_DECLARATION
                {
                    continue;
                }
                // In module files, any declaration of `undefined` is module-scoped
                // and does not conflict with the global `undefined`.  tsc only emits
                // TS2397 for declarations of `undefined` in script (non-module) files.
                if is_external_module {
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
