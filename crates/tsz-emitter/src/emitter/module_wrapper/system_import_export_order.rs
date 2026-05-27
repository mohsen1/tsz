use super::super::Printer;
use super::SystemDependencyPlan;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> Printer<'a> {
    pub(super) fn emit_system_import_binding_exports(
        &mut self,
        source: &tsz_parser::parser::node::SourceFileData,
        system_plan: &SystemDependencyPlan,
    ) {
        for &stmt_idx in &source.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            match stmt_node.kind {
                kind if kind == syntax_kind_ext::IMPORT_DECLARATION => {
                    self.emit_system_import_declaration_exports(stmt_node, system_plan);
                }
                kind if kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                    self.emit_system_import_equals_exports(stmt_node, system_plan);
                }
                _ => {}
            }
        }
    }

    fn emit_system_import_declaration_exports(
        &mut self,
        stmt_node: &tsz_parser::parser::node::Node,
        system_plan: &SystemDependencyPlan,
    ) {
        let Some(import_decl) = self.arena.get_import_decl(stmt_node) else {
            return;
        };
        if !self.import_decl_has_runtime_value(import_decl) {
            return;
        }
        let Some(dep_var) = system_plan.import_vars.get(&stmt_node.pos).cloned() else {
            return;
        };
        let Some(clause_node) = self.arena.get(import_decl.import_clause) else {
            return;
        };
        let Some(clause) = self.arena.get_import_clause(clause_node) else {
            return;
        };
        if clause.is_type_only {
            return;
        }

        if clause.name.is_some() {
            let local_name = self.get_identifier_text_idx(clause.name);
            if !local_name.is_empty() {
                let value = format!("{dep_var}.default");
                self.emit_system_import_alias_exports(&local_name, &value);
            }
        }

        let Some(bindings_node) = self.arena.get(clause.named_bindings) else {
            return;
        };
        let Some(named_imports) = self.arena.get_named_imports(bindings_node) else {
            return;
        };

        if named_imports.name.is_some() && named_imports.elements.nodes.is_empty() {
            let local_name = self.get_identifier_text_idx(named_imports.name);
            if !local_name.is_empty() {
                self.emit_system_import_alias_exports(&local_name, &dep_var);
            }
            return;
        }

        for &spec_idx in &self.collect_value_specifiers(&named_imports.elements) {
            let Some(spec) = self.arena.get_specifier_at(spec_idx) else {
                continue;
            };
            let local_name = self.get_identifier_text_idx(spec.name);
            if local_name.is_empty() {
                continue;
            }
            let import_name = if spec.property_name.is_some() {
                self.get_specifier_name_text(spec.property_name)
                    .unwrap_or_else(|| local_name.clone())
            } else {
                local_name.clone()
            };
            let value = if super::super::is_valid_identifier_name(&import_name) {
                format!("{dep_var}.{import_name}")
            } else {
                format!("{dep_var}[\"{import_name}\"]")
            };
            self.emit_system_import_alias_exports(&local_name, &value);
        }
    }

    fn emit_system_import_equals_exports(
        &mut self,
        stmt_node: &tsz_parser::parser::node::Node,
        system_plan: &SystemDependencyPlan,
    ) {
        let Some(import_decl) = self.arena.get_import_decl(stmt_node) else {
            return;
        };
        if !self.import_decl_has_runtime_value(import_decl) {
            return;
        }
        if !system_plan.import_vars.contains_key(&stmt_node.pos) {
            return;
        }
        let local_name = self.get_identifier_text_idx(import_decl.import_clause);
        if !local_name.is_empty() {
            self.emit_system_import_alias_exports(&local_name, &local_name);
        }
    }

    fn emit_system_import_alias_exports(&mut self, local_name: &str, value: &str) {
        let Some(export_names) = self.system_reexported_name_lists.get(local_name).cloned() else {
            return;
        };
        for export_name in export_names {
            self.write("exports_1(\"");
            self.write(&export_name);
            self.write("\", ");
            self.write(value);
            self.write(");");
            self.write_line();
        }
        self.system_folded_export_names
            .insert(local_name.to_string());
    }
}
