use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::ModuleData;
use tsz_scanner::SyntaxKind;

use super::UsageAnalyzer;
use crate::transforms::emit_utils::string_literal_text;

impl<'a> UsageAnalyzer<'a> {
    pub(super) fn module_declaration_contributes_public_surface(
        &self,
        module: &ModuleData,
    ) -> bool {
        if !self.binder.is_external_module()
            || self
                .arena
                .has_modifier(&module.modifiers, SyntaxKind::ExportKeyword)
            || string_literal_text(self.arena, module.name).is_some()
            || self
                .arena
                .get(module.name)
                .and_then(|node| self.arena.get_identifier(node))
                .is_some_and(|ident| ident.escaped_text == "global")
        {
            return true;
        }
        self.identifier_text(module.name)
            .as_deref()
            .is_some_and(|name| self.source_file_exports_name(name))
    }

    pub(super) fn analyze_module_declaration(&mut self, module_idx: NodeIndex) {
        let Some(module_node) = self.arena.get(module_idx) else {
            return;
        };
        let Some(module) = self.arena.get_module(module_node) else {
            return;
        };

        let previous_ambient_module_specifier = self.current_ambient_module_specifier.clone();
        if let Some(module_specifier) = string_literal_text(self.arena, module.name) {
            self.current_ambient_module_specifier = Some(module_specifier);
        }

        if let Some(body_node) = self.arena.get(module.body) {
            if let Some(module_block) = self.arena.get_module_block(body_node) {
                if let Some(ref stmts) = module_block.statements {
                    for &stmt_idx in &stmts.nodes {
                        self.analyze_statement(stmt_idx);
                    }
                }
            } else if self.arena.get_module(body_node).is_some() {
                self.analyze_module_declaration(module.body);
            }
        }

        self.current_ambient_module_specifier = previous_ambient_module_specifier;
    }

    fn identifier_text(&self, idx: NodeIndex) -> Option<String> {
        self.arena
            .get(idx)
            .and_then(|node| self.arena.get_identifier(node))
            .map(|ident| ident.escaped_text.clone())
    }

    fn source_file_exports_name(&self, name: &str) -> bool {
        let Some(source_file) = self
            .arena
            .nodes
            .iter()
            .rev()
            .find_map(|node| self.arena.get_source_file(node))
        else {
            return false;
        };

        source_file
            .statements
            .nodes
            .iter()
            .copied()
            .any(|stmt_idx| self.export_statement_references_name(stmt_idx, name))
    }

    fn export_statement_references_name(&self, stmt_idx: NodeIndex, name: &str) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        if let Some(export) = self.arena.get_export_decl(stmt_node) {
            return self.export_clause_references_name(export.export_clause, name);
        }
        if let Some(export_assignment) = self.arena.get_export_assignment(stmt_node) {
            let expr_idx = self.unwrap_export_default_expression(export_assignment.expression);
            return self.identifier_text(expr_idx).as_deref() == Some(name);
        }
        false
    }

    fn export_clause_references_name(&self, clause_idx: NodeIndex, name: &str) -> bool {
        let Some(clause_node) = self.arena.get(clause_idx) else {
            return false;
        };
        if self.arena.get_identifier(clause_node).is_some() {
            return self.identifier_text(clause_idx).as_deref() == Some(name);
        }
        let Some(named) = self.arena.get_named_imports(clause_node) else {
            return false;
        };
        named.elements.nodes.iter().copied().any(|spec_idx| {
            let Some(spec_node) = self.arena.get(spec_idx) else {
                return false;
            };
            let Some(spec) = self.arena.get_specifier(spec_node) else {
                return false;
            };
            let local_name_idx = if spec.property_name.is_some() {
                spec.property_name
            } else {
                spec.name
            };
            self.identifier_text(local_name_idx).as_deref() == Some(name)
        })
    }
}
