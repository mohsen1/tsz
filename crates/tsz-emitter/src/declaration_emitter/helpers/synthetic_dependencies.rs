//! Dependency retention for synthetic declaration surfaces

use super::super::{DeclarationEmitter, usage_analyzer::UsageKind};
use tsz_binder::symbol_flags;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn synthetic_class_extends_alias_type_id(
        &self,
        heritage: Option<&NodeList>,
    ) -> Option<tsz_solver::TypeId> {
        let heritage = heritage?;
        let (type_idx, expr_idx) = self.non_nameable_extends_heritage_type(heritage)?;
        self.get_node_type_or_names(&[expr_idx, type_idx])
    }

    pub(crate) fn retain_synthetic_class_extends_alias_dependencies_in_statements(
        &mut self,
        statements: &NodeList,
    ) {
        for &stmt_idx in &statements.nodes {
            self.retain_synthetic_class_extends_alias_dependencies_for_statement(stmt_idx);
        }
    }

    pub(in crate::declaration_emitter) fn retain_export_default_expression_type_dependencies_in_statements(
        &mut self,
        statements: &NodeList,
    ) {
        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let Some(export) = self.arena.get_export_decl(stmt_node) else {
                continue;
            };
            if !export.is_default_export || !export.export_clause.is_some() {
                continue;
            }
            let Some(clause_node) = self.arena.get(export.export_clause) else {
                continue;
            };
            if matches!(
                clause_node.kind,
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::INTERFACE_DECLARATION
                    || k == SyntaxKind::Identifier as u16
            ) {
                continue;
            }
            if let Some(resolved_type) = self
                .resolve_declaration_type_text(&[export.export_clause], Some(export.export_clause))
            {
                self.retain_direct_type_symbols_for_public_api(resolved_type.type_id);
                self.retain_local_type_names_for_public_api(&resolved_type.canonical_type_text);
                self.retain_local_type_names_for_public_api(&resolved_type.emitted_type_text);
            } else if let Some(type_id) = self.get_node_type(export.export_clause) {
                self.retain_direct_type_symbols_for_public_api(type_id);
                let type_text = self.print_type_id(type_id);
                self.retain_local_type_names_for_public_api(&type_text);
            }
        }
    }

    pub(in crate::declaration_emitter) fn retain_synthetic_function_return_dependencies_in_statements(
        &mut self,
        statements: &NodeList,
    ) {
        for &stmt_idx in &statements.nodes {
            self.retain_synthetic_function_return_dependencies_for_statement(stmt_idx);
        }
    }

    /// Walk exported `var`/`let`/`const` declarations whose type comes from
    /// initializer inference (no annotation) and pull every local type alias
    /// that the inferred type names into `used_symbols`. Without this, an
    /// `export const item = make()` whose return type is a *local* alias
    /// `Box` would leave the `.d.ts` referencing `Box` even though no `type
    /// Box = ...` declaration was emitted. See issue #3755.
    pub(in crate::declaration_emitter) fn retain_synthetic_variable_declaration_dependencies_in_statements(
        &mut self,
        statements: &NodeList,
    ) {
        for &stmt_idx in &statements.nodes {
            self.retain_synthetic_variable_declaration_dependencies_for_statement(stmt_idx);
        }
    }

    fn retain_synthetic_variable_declaration_dependencies_for_statement(
        &mut self,
        stmt_idx: NodeIndex,
    ) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let var_stmt_idx = match stmt_node.kind {
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                let Some(module) = self.arena.get_module(stmt_node) else {
                    return;
                };
                let previous_namespace = self.enclosing_namespace_symbol;
                if let Some(binder) = self.binder
                    && let Some(ns_sym) = binder.get_node_symbol(stmt_idx)
                {
                    self.enclosing_namespace_symbol = Some(ns_sym);
                }
                if let Some(body_node) = self.arena.get(module.body) {
                    if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                        self.retain_synthetic_variable_declaration_dependencies_for_statement(
                            module.body,
                        );
                    } else if let Some(block) = self.arena.get_module_block(body_node)
                        && let Some(statements) = block.statements.as_ref()
                    {
                        self.retain_synthetic_variable_declaration_dependencies_in_statements(
                            statements,
                        );
                    }
                }
                self.enclosing_namespace_symbol = previous_namespace;
                return;
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if !self.statement_has_effective_export(stmt_idx) {
                    return;
                }
                stmt_idx
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                let Some(export) = self.arena.get_export_decl(stmt_node) else {
                    return;
                };
                let Some(clause_node) = self.arena.get(export.export_clause) else {
                    return;
                };
                if clause_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                    return;
                }
                export.export_clause
            }
            _ => return,
        };
        let Some(var_stmt_node) = self.arena.get(var_stmt_idx) else {
            return;
        };
        let Some(var_stmt) = self.arena.get_variable(var_stmt_node) else {
            return;
        };

        let decl_list_indices: Vec<NodeIndex> = var_stmt.declarations.nodes.to_vec();
        for decl_list_idx in decl_list_indices {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };
            let var_decl_indices: Vec<NodeIndex> = decl_list.declarations.nodes.to_vec();
            for var_decl_idx in var_decl_indices {
                let Some(var_decl_node) = self.arena.get(var_decl_idx) else {
                    continue;
                };
                let Some(var_decl) = self.arena.get_variable_declaration(var_decl_node) else {
                    continue;
                };
                if var_decl.type_annotation.is_some() {
                    continue;
                }
                if !var_decl.initializer.is_some() {
                    continue;
                }
                self.retain_import_equals_aliases_from_public_initializer(var_decl.initializer);
                // For call-expression initializers, retain identifiers
                // referenced in the callee's declared return-type
                // annotation text. The annotation source text preserves
                // local alias names verbatim — without this pass an
                // `export const item = make()` whose callee returns a
                // local alias `Box` would leave the .d.ts referencing
                // `Box` even though no `type Box = ...` was emitted. See
                // issue #3755. This path is intentionally narrower than
                // a structural inference walk so it does not over-retain
                // siblings on test sources where the public-API filter
                // already preserves what's needed.
                if let Some(return_type_text) =
                    self.call_expression_declared_return_type_text(var_decl.initializer)
                {
                    self.retain_local_type_names_for_public_api(&return_type_text);
                }
            }
        }
    }

    pub(in crate::declaration_emitter) fn retain_asserted_class_property_type_dependencies_in_statements(
        &mut self,
        statements: &NodeList,
    ) {
        for &stmt_idx in &statements.nodes {
            self.retain_asserted_class_property_type_dependencies_for_statement(stmt_idx);
        }
    }

    fn retain_asserted_class_property_type_dependencies_for_statement(
        &mut self,
        stmt_idx: NodeIndex,
    ) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if self.statement_has_effective_export(stmt_idx)
                    && let Some(class) = self.arena.get_class(stmt_node)
                {
                    self.retain_asserted_class_property_type_dependencies(&class.members);
                }
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                if let Some(export) = self.arena.get_export_decl(stmt_node)
                    && let Some(clause_node) = self.arena.get(export.export_clause)
                    && clause_node.kind == syntax_kind_ext::CLASS_DECLARATION
                    && let Some(class) = self.arena.get_class(clause_node)
                {
                    self.retain_asserted_class_property_type_dependencies(&class.members);
                }
            }
            _ => {}
        }
    }

    fn retain_asserted_class_property_type_dependencies(&mut self, members: &NodeList) {
        for &member_idx in &members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let Some(prop) = self.arena.get_property_decl(member_node) else {
                continue;
            };
            if prop.type_annotation.is_some() || prop.initializer.is_none() {
                continue;
            }
            let is_private = self
                .arena
                .has_modifier(&prop.modifiers, SyntaxKind::PrivateKeyword)
                || self.member_has_private_identifier_name(prop.name);
            if is_private {
                continue;
            }
            if let Some(type_text) = self.explicit_asserted_type_text(prop.initializer) {
                self.retain_local_type_names_for_public_api(&type_text);
            }
        }
    }

    fn retain_synthetic_function_return_dependencies_for_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if self.statement_has_effective_export(stmt_idx)
                    && let Some(func) = self.arena.get_function(stmt_node)
                    && let Some(type_text) =
                        self.function_body_single_nameable_new_return_type_text(func.body)
                {
                    self.retain_local_type_names_for_public_api(&type_text);
                }
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                if let Some(export) = self.arena.get_export_decl(stmt_node)
                    && let Some(clause_node) = self.arena.get(export.export_clause)
                    && clause_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                    && let Some(func) = self.arena.get_function(clause_node)
                    && let Some(type_text) =
                        self.function_body_single_nameable_new_return_type_text(func.body)
                {
                    self.retain_local_type_names_for_public_api(&type_text);
                }
            }
            _ => {}
        }
    }

    fn retain_import_equals_aliases_from_public_initializer(&mut self, initializer: NodeIndex) {
        let Some(expr_idx) = self.skip_parenthesized_expression(initializer) else {
            return;
        };
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return;
        };

        let entity_idx = if expr_node.kind == syntax_kind_ext::NEW_EXPRESSION {
            let Some(call) = self.arena.get_call_expr(expr_node) else {
                return;
            };
            call.expression
        } else if expr_node.kind == syntax_kind_ext::CALL_EXPRESSION {
            return;
        } else {
            expr_idx
        };

        self.retain_import_equals_alias_entity_root(entity_idx);
    }

    fn retain_import_equals_alias_entity_root(&mut self, entity_idx: NodeIndex) {
        let Some(entity_node) = self.arena.get(entity_idx) else {
            return;
        };

        if entity_node.kind == SyntaxKind::Identifier as u16 {
            self.retain_import_equals_alias_identifier(entity_idx);
            return;
        }

        if let Some(access) = self.arena.get_access_expr(entity_node) {
            self.retain_import_equals_alias_entity_root(access.expression);
            return;
        }

        if let Some(qualified) = self.arena.get_qualified_name(entity_node) {
            self.retain_import_equals_alias_entity_root(qualified.left);
        }
    }

    fn retain_import_equals_alias_identifier(&mut self, name_idx: NodeIndex) {
        let Some(binder) = self.binder else {
            return;
        };
        let mut candidates = Vec::new();
        if let Some(sym_id) = binder.node_symbols.get(&name_idx.0) {
            candidates.push(*sym_id);
        }
        if let Some(name) = self.get_identifier_text(name_idx) {
            if let Some(sym_id) = binder.file_locals.get(&name)
                && !candidates.contains(&sym_id)
            {
                candidates.push(sym_id);
            }
            for scope in binder.scopes.iter() {
                if let Some(sym_id) = scope.table.get(&name)
                    && !candidates.contains(&sym_id)
                {
                    candidates.push(sym_id);
                }
            }
        }

        let retained: Vec<_> = candidates
            .into_iter()
            .filter(|sym_id| {
                let Some(symbol) = binder.symbols.get(*sym_id) else {
                    return false;
                };
                symbol.has_any_flags(symbol_flags::ALIAS)
                    && symbol.declarations.iter().copied().any(|decl_idx| {
                        self.arena
                            .get(decl_idx)
                            .and_then(|node| self.arena.get_import_decl(node))
                            .is_some_and(|import| {
                                self.import_alias_targets_type_entity(import.module_specifier)
                            })
                    })
            })
            .collect();

        let Some(used_symbols) = self.used_symbols.as_mut() else {
            return;
        };
        for sym_id in retained {
            used_symbols
                .entry(sym_id)
                .and_modify(|kind| *kind |= UsageKind::TYPE)
                .or_insert(UsageKind::TYPE);
        }
    }

    pub(in crate::declaration_emitter) fn retain_imported_static_call_dependencies_in_statements(
        &mut self,
        statements: &NodeList,
    ) {
        let imported_modules = self.named_import_modules_in_statements(statements);
        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
                continue;
            };
            if !self
                .arena
                .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword)
            {
                continue;
            }
            for &decl_idx in &var_stmt.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                if decl.initializer.is_none() {
                    continue;
                }
                self.retain_imported_static_call_dependency(decl.initializer, &imported_modules);
            }
        }
    }

    fn retain_imported_static_call_dependency(
        &mut self,
        initializer: NodeIndex,
        imported_modules: &rustc_hash::FxHashMap<String, String>,
    ) {
        let Some(init_idx) = self.skip_parenthesized_expression(initializer) else {
            return;
        };
        let Some(init_node) = self.arena.get(init_idx) else {
            return;
        };
        if init_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return;
        }
        let Some(call) = self.arena.get_call_expr(init_node) else {
            return;
        };
        let Some(callee_node) = self.arena.get(call.expression) else {
            return;
        };
        let Some(access) = self.arena.get_access_expr(callee_node) else {
            return;
        };
        if self
            .call_receiver_default_import_alias(call.expression)
            .is_some()
        {
            return;
        }
        let Some(receiver_name) = self.get_identifier_text(access.expression) else {
            return;
        };
        let Some(binder) = self.binder else {
            return;
        };
        if let Some(module) = imported_modules.get(&receiver_name).cloned() {
            self.required_imports
                .entry(module)
                .or_default()
                .push(receiver_name.clone());
        }
        let Some(sym_id) = binder.file_locals.get(receiver_name.as_str()) else {
            return;
        };
        let Some(used_symbols) = self.used_symbols.as_mut() else {
            return;
        };
        used_symbols
            .entry(sym_id)
            .and_modify(|kind| *kind |= UsageKind::TYPE)
            .or_insert(UsageKind::TYPE);
    }

    fn named_import_modules_in_statements(
        &self,
        statements: &NodeList,
    ) -> rustc_hash::FxHashMap<String, String> {
        let mut modules = rustc_hash::FxHashMap::default();
        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let Some(import) = self.arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_node) = self.arena.get(import.module_specifier) else {
                continue;
            };
            let Some(module_lit) = self.arena.get_literal(module_node) else {
                continue;
            };
            let Some(clause_node) = self.arena.get(import.import_clause) else {
                continue;
            };
            let Some(clause) = self.arena.get_import_clause(clause_node) else {
                continue;
            };
            if clause.named_bindings.is_some()
                && let Some(bindings_node) = self.arena.get(clause.named_bindings)
                && let Some(bindings) = self.arena.get_named_imports(bindings_node)
            {
                for &spec_idx in &bindings.elements.nodes {
                    let Some(spec_node) = self.arena.get(spec_idx) else {
                        continue;
                    };
                    let Some(specifier) = self.arena.get_specifier(spec_node) else {
                        continue;
                    };
                    if let Some(local_name) = self.get_identifier_text(specifier.name) {
                        modules.insert(local_name, module_lit.text.clone());
                    }
                }
            }
        }
        modules
    }

    pub(in crate::declaration_emitter) fn retain_synthetic_class_extends_alias_dependencies_for_statement(
        &mut self,
        stmt_idx: NodeIndex,
    ) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = self.arena.get_class(stmt_node)
                    && self.statement_has_effective_export(stmt_idx)
                    && let Some(type_id) =
                        self.synthetic_class_extends_alias_type_id(class.heritage_clauses.as_ref())
                {
                    self.retain_direct_type_symbols_for_public_api(type_id);
                }
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                if let Some(export) = self.arena.get_export_decl(stmt_node)
                    && let Some(clause_node) = self.arena.get(export.export_clause)
                {
                    if clause_node.kind == syntax_kind_ext::CLASS_DECLARATION
                        && let Some(class) = self.arena.get_class(clause_node)
                        && let Some(type_id) = self
                            .synthetic_class_extends_alias_type_id(class.heritage_clauses.as_ref())
                    {
                        self.retain_direct_type_symbols_for_public_api(type_id);
                    } else if clause_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                        self.retain_synthetic_module_extends_alias_dependencies(
                            export.export_clause,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.retain_synthetic_module_extends_alias_dependencies(stmt_idx);
            }
            _ => {}
        }
    }

    pub(in crate::declaration_emitter) fn retain_synthetic_module_extends_alias_dependencies(
        &mut self,
        module_idx: NodeIndex,
    ) {
        let Some(module_node) = self.arena.get(module_idx) else {
            return;
        };
        let Some(module) = self.arena.get_module(module_node) else {
            return;
        };

        let mut current_body = module.body;
        loop {
            let Some(body_node) = self.arena.get(current_body) else {
                return;
            };
            if let Some(nested_mod) = self.arena.get_module(body_node) {
                current_body = nested_mod.body;
                continue;
            }
            if let Some(block) = self.arena.get_module_block(body_node)
                && let Some(ref statements) = block.statements
            {
                self.retain_synthetic_class_extends_alias_dependencies_in_statements(statements);
            }
            return;
        }
    }
}
