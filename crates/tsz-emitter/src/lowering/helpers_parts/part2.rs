impl<'a> LoweringPass<'a> {
    // =========================================================================
    // Helper Methods
    // =========================================================================

    pub(super) fn collect_module_dependencies(&self, statements: &[NodeIndex]) -> Vec<String> {
        let mut deps = Vec::new();
        for &stmt_idx in statements {
            let Some(node) = self.arena.get(stmt_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::IMPORT_DECLARATION
                || node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                if let Some(import_decl) = self.arena.get_import_decl(node) {
                    if !self.import_should_schedule_runtime_dependency(node, import_decl) {
                        continue;
                    }
                    if let Some(text) =
                        emit_utils::module_specifier_text(self.arena, import_decl.module_specifier)
                        && !deps.contains(&text)
                    {
                        deps.push(text);
                    }
                }
                continue;
            }

            if node.kind == syntax_kind_ext::EXPORT_DECLARATION
                && let Some(export_decl) = self.arena.get_export_decl(node)
            {
                if !self.export_has_runtime_dependency(export_decl) {
                    continue;
                }
                if let Some(text) =
                    emit_utils::module_specifier_text(self.arena, export_decl.module_specifier)
                    && !deps.contains(&text)
                {
                    deps.push(text);
                }
            }
        }

        if self.jsx_automatic_runtime_makes_module() {
            let source = self
                .ctx
                .options
                .jsx_import_source
                .as_deref()
                .unwrap_or("react");
            let runtime = if matches!(self.ctx.options.jsx, JsxEmit::ReactJsxDev) {
                format!("{source}/jsx-dev-runtime")
            } else {
                format!("{source}/jsx-runtime")
            };
            if !deps.contains(&runtime) {
                deps.push(runtime);
            }
        }

        deps
    }

    pub(super) fn import_has_runtime_dependency(
        &self,
        import_decl: &tsz_parser::parser::node::ImportDeclData,
    ) -> bool {
        if import_decl.import_clause.is_none() {
            return true;
        }

        let Some(clause_node) = self.arena.get(import_decl.import_clause) else {
            return true;
        };

        if clause_node.kind != syntax_kind_ext::IMPORT_CLAUSE {
            return self.import_equals_has_external_module(import_decl.module_specifier);
        }

        let Some(clause) = self.arena.get_import_clause(clause_node) else {
            return true;
        };

        if clause.is_type_only {
            return false;
        }

        if clause.name.is_some() {
            return true;
        }

        if clause.named_bindings.is_none() {
            return false;
        }

        let Some(bindings_node) = self.arena.get(clause.named_bindings) else {
            return false;
        };

        let Some(named) = self.arena.get_named_imports(bindings_node) else {
            return true;
        };

        if named.name.is_some() {
            return true;
        }

        if named.elements.nodes.is_empty() {
            return true;
        }

        for &spec_idx in &named.elements.nodes {
            let Some(spec_node) = self.arena.get(spec_idx) else {
                continue;
            };
            if let Some(spec) = self.arena.get_specifier(spec_node)
                && !spec.is_type_only
            {
                return true;
            }
        }

        false
    }

    pub(super) fn import_should_schedule_runtime_dependency(
        &self,
        node: &tsz_parser::parser::node::Node,
        import_decl: &tsz_parser::parser::node::ImportDeclData,
    ) -> bool {
        if !self.import_has_runtime_dependency(import_decl) {
            return false;
        }

        let Some(clause_node) = self.arena.get(import_decl.import_clause) else {
            return true;
        };
        if clause_node.kind != syntax_kind_ext::IMPORT_CLAUSE {
            return true;
        }

        let Some(clause) = self.arena.get_import_clause(clause_node) else {
            return true;
        };
        if clause.is_type_only {
            return false;
        }
        if self.ctx.options.verbatim_module_syntax {
            return true;
        }
        if self.import_clause_is_empty_named_import(clause) {
            return false;
        }
        if self.import_clause_is_namespace_only(clause)
            && self.import_references_type_only_export_equals_module(import_decl)
        {
            return false;
        }

        self.import_has_value_usage_after_node(node, clause)
    }

    fn import_clause_is_namespace_only(
        &self,
        clause: &tsz_parser::parser::node::ImportClauseData,
    ) -> bool {
        clause.name.is_none()
            && clause.named_bindings.is_some()
            && self
                .arena
                .get(clause.named_bindings)
                .and_then(|bindings_node| self.arena.get_named_imports(bindings_node))
                .is_some_and(|named| named.name.is_some() && named.elements.nodes.is_empty())
    }

    fn import_clause_is_empty_named_import(
        &self,
        clause: &tsz_parser::parser::node::ImportClauseData,
    ) -> bool {
        clause.name.is_none()
            && clause.named_bindings.is_some()
            && self
                .arena
                .get(clause.named_bindings)
                .and_then(|bindings_node| self.arena.get_named_imports(bindings_node))
                .is_some_and(|named| named.name.is_none() && named.elements.nodes.is_empty())
    }

    fn import_references_type_only_export_equals_module(
        &self,
        import_decl: &tsz_parser::parser::node::ImportDeclData,
    ) -> bool {
        let Some(module_node) = self.arena.get(import_decl.module_specifier) else {
            return false;
        };
        let Some(lit) = self.arena.get_literal(module_node) else {
            return false;
        };
        self.ctx
            .options
            .type_only_export_equals_modules
            .contains(lit.text.as_str())
    }

    pub(super) fn import_equals_has_external_module(&self, module_specifier: NodeIndex) -> bool {
        if module_specifier.is_none() {
            // require(nonStringLiteral) — specifier failed to parse as string literal,
            // but the `import = require(...)` form still indicates an external module
            return true;
        }

        let Some(node) = self.arena.get(module_specifier) else {
            return true;
        };

        node.kind == SyntaxKind::StringLiteral as u16
    }

    #[allow(dead_code)]
    pub(super) fn export_decl_has_runtime_value(
        &self,
        export_decl: &tsz_parser::parser::node::ExportDeclData,
    ) -> bool {
        crate::transforms::emit_utils::export_decl_has_runtime_value(
            self.arena,
            export_decl,
            self.ctx.options.preserve_const_enums,
        )
    }

    pub(super) fn export_has_runtime_dependency(
        &self,
        export_decl: &tsz_parser::parser::node::ExportDeclData,
    ) -> bool {
        if export_decl.is_type_only {
            return false;
        }

        if export_decl.module_specifier.is_none() {
            return false;
        }

        if export_decl.export_clause.is_none() {
            return true;
        }

        let Some(clause_node) = self.arena.get(export_decl.export_clause) else {
            return true;
        };

        let Some(named) = self.arena.get_named_imports(clause_node) else {
            return true;
        };

        if named.name.is_some() {
            return true;
        }

        if named.elements.nodes.is_empty() {
            return true;
        }

        for &spec_idx in &named.elements.nodes {
            let Some(spec_node) = self.arena.get(spec_idx) else {
                continue;
            };
            if let Some(spec) = self.arena.get_specifier(spec_node)
                && !spec.is_type_only
            {
                return true;
            }
        }

        false
    }
}
