impl<'a> CheckerState<'a> {
    /// Resolve the base constraint of an `IndexAccess` type for display purposes.
    ///
    /// For `T[K]` where `T extends C` and `K extends D`, resolves through the
    /// constraint chain to produce the concrete type (e.g., `C[D]` evaluated).
    /// This matches tsc's behavior of showing the apparent type in error messages.
    pub(crate) fn resolve_index_access_base_constraint(&mut self, type_id: TypeId) -> TypeId {
        // First try standard evaluation (resolves T to its constraint)
        let evaluated = self.evaluate_type_with_env(type_id);

        // If fully resolved (no longer an IndexAccess), use it
        if !crate::query_boundaries::common::is_index_access_type(self.ctx.types, evaluated) {
            return evaluated;
        }

        // Still an IndexAccess — try resolving the index type parameter's constraint.
        // E.g., {[s:string]:V}[K] where K extends keyof T => evaluate {[s:string]:V}[keyof T] => V
        if let Some((ia_obj, ia_idx)) =
            crate::query_boundaries::common::index_access_parts(self.ctx.types, evaluated)
            && let Some(constraint) =
                access_query::type_parameter_constraint(self.ctx.types, ia_idx)
        {
            let resolved = self
                .ctx
                .types
                .evaluate_index_access_with_options(ia_obj, constraint, false);
            if !crate::query_boundaries::common::is_index_access_type(self.ctx.types, resolved) {
                return resolved;
            }
        }

        type_id
    }

    /// Check if a symbol has any exported value declarations.
    ///
    /// For merged symbols (e.g., namespace + interface with same name), only the
    /// interface part may be exported while the namespace is not. This helper
    /// checks whether any VALUE-contributing declaration (namespace, function,
    /// class, etc.) has an export modifier.
    ///
    /// Returns `true` if:
    /// - The symbol has no TYPE flags (pure value symbol - trust `is_exported`)
    /// - The symbol has at least one value declaration with export modifier
    pub(crate) fn symbol_has_exported_value_declaration(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // If the symbol only has VALUE flags (no TYPE flags), we can trust is_exported
        let has_type_flags = symbol.has_any_flags(symbol_flags::TYPE);
        if !has_type_flags {
            return symbol.is_exported;
        }

        // For symbols that are both VALUE and TYPE by design (CLASS, ENUM, ENUM_MEMBER),
        // not due to merging with an interface/type-alias, we can trust is_exported.
        // Enum members are considered exported if they're in the enum's exports table.
        // We only need special handling for namespace + interface/type-alias merges.
        let is_merged_with_type_only =
            symbol.has_any_flags(symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS);
        if !is_merged_with_type_only {
            // Enum members may not have is_exported set, but they're accessible
            // if they're in the enum's exports table (which they must be to get here)
            if symbol.has_any_flags(symbol_flags::ENUM_MEMBER) {
                return true;
            }
            return symbol.is_exported;
        }

        // For lib symbols (decl_file_idx == u32::MAX), trust is_exported since
        // lib declarations have proper export semantics by construction.
        if symbol.decl_file_idx == u32::MAX {
            return symbol.is_exported;
        }

        // For cross-file merged symbols, trust is_exported since declarations
        // may be in different arenas. The cross-file merge logic in the binder
        // correctly tracks export status.
        if self.ctx.all_arenas.is_some() {
            // Check if this looks like a cross-file merged symbol by seeing if
            // any declarations can't be found in the current arena
            let has_cross_file_decl = symbol
                .declarations
                .iter()
                .any(|&decl_idx| self.ctx.arena.get(decl_idx).is_none());
            if has_cross_file_decl {
                return symbol.is_exported;
            }
        }

        // Single-file merged symbol - check declarations individually
        for &decl_idx in &symbol.declarations {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };

            // Check if this is a value declaration with export modifier
            if let Some(true) =
                self.check_value_decl_has_export_in_arena(self.ctx.arena, decl_idx, decl_node)
            {
                return true;
            }
        }

        tracing::debug!(
            "symbol_has_exported_value_declaration: returning false for {:?}",
            symbol.escaped_name
        );
        false
    }

    /// Check if a declaration node has an export modifier using a specific arena.
    /// Also checks if the declaration is wrapped in an `EXPORT_DECLARATION` node,
    /// since `export namespace B` creates an `EXPORT_DECLARATION` wrapping `MODULE_DECLARATION`.
    fn check_value_decl_has_export_in_arena(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        decl_idx: tsz_parser::NodeIndex,
        decl_node: &tsz_parser::parser::node::Node,
    ) -> Option<bool> {
        // Helper to check if a node is wrapped in an EXPORT_DECLARATION
        let is_inside_export_decl = || -> bool {
            // Get parent node from extended info
            if let Some(ext) = arena.get_extended(decl_idx)
                && let Some(parent_node) = arena.get(ext.parent)
                && parent_node.kind == syntax_kind_ext::EXPORT_DECLARATION
            {
                return true;
            }
            false
        };

        // Helper to check if the declaration is inside a `declare` context (ambient).
        // In ambient contexts, members are implicitly exported.
        let is_inside_declare_context = || -> bool {
            let mut current = decl_idx;
            for _ in 0..10 {
                let Some(ext) = arena.get_extended(current) else {
                    break;
                };
                let Some(parent_node) = arena.get(ext.parent) else {
                    break;
                };
                // Check if parent is a module with `declare` modifier
                if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                    && let Some(m) = arena.get_module(parent_node)
                    && m.modifiers
                        .as_ref()
                        .is_some_and(|mods| arena.is_declare_ref(Some(mods)))
                {
                    return true;
                }
                current = ext.parent;
            }
            false
        };

        match decl_node.kind {
            syntax_kind_ext::MODULE_DECLARATION => {
                let module = arena.get_module(decl_node);
                if let Some(m) = module {
                    // Check direct modifiers, parent EXPORT_DECLARATION, or ambient context
                    let has_direct_export = m.modifiers.as_ref().is_some_and(|mods| {
                        arena.has_modifier_ref(Some(mods), SyntaxKind::ExportKeyword)
                    });
                    let has_declare = m
                        .modifiers
                        .as_ref()
                        .is_some_and(|mods| arena.is_declare_ref(Some(mods)));
                    Some(
                        has_direct_export
                            || has_declare
                            || is_inside_export_decl()
                            || is_inside_declare_context(),
                    )
                } else {
                    None
                }
            }
            syntax_kind_ext::FUNCTION_DECLARATION => arena.get_function(decl_node).map(|f| {
                let has_direct_export = f.modifiers.as_ref().is_some_and(|mods| {
                    arena.has_modifier_ref(Some(mods), SyntaxKind::ExportKeyword)
                });
                let has_declare = f
                    .modifiers
                    .as_ref()
                    .is_some_and(|mods| arena.is_declare_ref(Some(mods)));
                has_direct_export
                    || has_declare
                    || is_inside_export_decl()
                    || is_inside_declare_context()
            }),
            syntax_kind_ext::CLASS_DECLARATION => arena.get_class(decl_node).map(|c| {
                let has_direct_export = c.modifiers.as_ref().is_some_and(|mods| {
                    arena.has_modifier_ref(Some(mods), SyntaxKind::ExportKeyword)
                });
                let has_declare = c
                    .modifiers
                    .as_ref()
                    .is_some_and(|mods| arena.is_declare_ref(Some(mods)));
                has_direct_export
                    || has_declare
                    || is_inside_export_decl()
                    || is_inside_declare_context()
            }),
            syntax_kind_ext::ENUM_DECLARATION => arena.get_enum(decl_node).map(|e| {
                let has_direct_export = e.modifiers.as_ref().is_some_and(|mods| {
                    arena.has_modifier_ref(Some(mods), SyntaxKind::ExportKeyword)
                });
                let has_declare = e
                    .modifiers
                    .as_ref()
                    .is_some_and(|mods| arena.is_declare_ref(Some(mods)));
                has_direct_export
                    || has_declare
                    || is_inside_export_decl()
                    || is_inside_declare_context()
            }),
            syntax_kind_ext::VARIABLE_DECLARATION => {
                // For variable declarations, check if inside a declare context
                // (e.g., `declare namespace Foo { var x: number; }`)
                // The export modifier is on the parent VARIABLE_STATEMENT, not the declaration itself.
                // Walk up: VARIABLE_DECLARATION -> VARIABLE_DECLARATION_LIST -> VARIABLE_STATEMENT
                // and check if the VARIABLE_STATEMENT has an `export` modifier.
                let has_export_on_var_stmt = || -> bool {
                    // Walk from VariableDeclaration up to VariableStatement
                    let Some(ext1) = arena.get_extended(decl_idx) else {
                        return false;
                    };
                    // ext1.parent = VariableDeclarationList
                    let Some(ext2) = arena.get_extended(ext1.parent) else {
                        return false;
                    };
                    // ext2.parent = VariableStatement
                    let Some(var_stmt_node) = arena.get(ext2.parent) else {
                        return false;
                    };
                    if var_stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                        return false;
                    }
                    arena
                        .get_variable(var_stmt_node)
                        .and_then(|v| v.modifiers.as_ref())
                        .is_some_and(|mods| {
                            arena.has_modifier_ref(Some(mods), SyntaxKind::ExportKeyword)
                        })
                };
                Some(
                    has_export_on_var_stmt()
                        || is_inside_export_decl()
                        || is_inside_declare_context(),
                )
            }
            _ => Some(false), // Skip non-value declarations (interface, type alias)
        }
    }

    pub(crate) fn check_jsdoc_prototype_type_decl_constructor_assignment(
        &mut self,
        prototype_expr_idx: NodeIndex,
        property_name: &str,
        declared_type: TypeId,
    ) {
        let Some(prototype_node) = self.ctx.arena.get(prototype_expr_idx) else {
            return;
        };
        if prototype_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return;
        }
        let Some(prototype_access) = self.ctx.arena.get_access_expr(prototype_node) else {
            return;
        };
        if self
            .ctx
            .arena
            .get_identifier_at(prototype_access.name_or_argument)
            .is_none_or(|ident| ident.escaped_text != "prototype")
        {
            return;
        }
        let Some(func_idx) = self.js_prototype_owner_function_target(prototype_access.expression)
        else {
            return;
        };
        let Some(body_idx) = self
            .ctx
            .arena
            .get(func_idx)
            .and_then(|node| self.ctx.arena.get_function(node))
            .and_then(|func| func.body.is_some().then_some(func.body))
        else {
            return;
        };
        let Some((source_idx, diag_idx)) =
            self.constructor_this_assignment_for_property(body_idx, property_name)
        else {
            return;
        };

        let source_type = self.get_type_of_node(source_idx);
        let target_type =
            crate::query_boundaries::common::remove_undefined(self.ctx.types, declared_type);
        let _ = self.check_assignable_or_report_at_exact_anchor(
            source_type,
            target_type,
            source_idx,
            diag_idx,
        );
    }

    fn constructor_this_assignment_for_property(
        &mut self,
        body_idx: NodeIndex,
        property_name: &str,
    ) -> Option<(NodeIndex, NodeIndex)> {
        let body_node = self.ctx.arena.get(body_idx)?;
        let block = self.ctx.arena.get_block(body_node)?;
        let mut stmts = Vec::new();
        for &stmt_idx in &block.statements.nodes {
            self.collect_nested_js_this_assignment_statements(stmt_idx, &mut stmts);
        }
        let this_aliases = self.collect_this_aliases(&stmts);

        for stmt_idx in stmts {
            let Some((found_name, rhs_idx, is_private, _)) =
                self.extract_this_property_assignment(stmt_idx, &this_aliases)
            else {
                continue;
            };
            if is_private || found_name != property_name {
                continue;
            }
            let stmt_node = self.ctx.arena.get(stmt_idx)?;
            let expr_stmt = self.ctx.arena.get_expression_statement(stmt_node)?;
            let expr_node = self.ctx.arena.get(expr_stmt.expression)?;
            let binary = self.ctx.arena.get_binary_expr(expr_node)?;
            return Some((rhs_idx, binary.left));
        }

        None
    }
}
