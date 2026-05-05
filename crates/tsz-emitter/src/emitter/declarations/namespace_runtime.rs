//! Runtime-value lookup helpers for namespace/module declaration emit.

use super::super::Printer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    /// This is more permissive than `namespace_alias_target_has_runtime_value` which
    /// treats `declare function` as having no runtime emit (correct for namespace aliasing
    /// but not for `export default`).
    pub(in crate::emitter) fn export_default_target_has_runtime_value(
        &self,
        target: NodeIndex,
    ) -> bool {
        let node = match self.arena.get(target) {
            Some(n) => n,
            None => return true, // conservative default
        };

        if node.kind != SyntaxKind::Identifier as u16 {
            return true; // qualified names etc. are conservatively treated as runtime
        }

        let name = self.get_identifier_text_idx(target);
        if name.is_empty() {
            return true;
        }

        // Search source file statements for the declaration
        let statements = self.scope_statements_for_runtime_lookup(None);
        if statements.is_empty() {
            return true; // conservative: can't resolve, assume runtime
        }

        let mut found_type_only = false;
        let mut found_value = false;

        for stmt_idx in &statements {
            let Some(stmt_node) = self.arena.get(*stmt_idx) else {
                continue;
            };

            // Unwrap export declarations to find the inner declaration
            let check_node = if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                if let Some(export) = self.arena.get_export_decl(stmt_node) {
                    self.arena.get(export.export_clause)
                } else {
                    None
                }
            } else {
                Some(stmt_node)
            };

            let Some(check) = check_node else { continue };

            match check.kind {
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                    if let Some(iface) = self.arena.get_interface(check)
                        && self.get_identifier_text_idx(iface.name) == name
                    {
                        found_type_only = true;
                    }
                }
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                    if let Some(ta) = self.arena.get_type_alias(check)
                        && self.get_identifier_text_idx(ta.name) == name
                    {
                        found_type_only = true;
                    }
                }
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                    if let Some(func) = self.arena.get_function(check)
                        && self.get_identifier_text_idx(func.name) == name
                    {
                        found_value = true;
                    }
                }
                k if k == syntax_kind_ext::CLASS_DECLARATION => {
                    if let Some(class) = self.arena.get_class(check)
                        && self.get_identifier_text_idx(class.name) == name
                    {
                        found_value = true;
                    }
                }
                k if k == syntax_kind_ext::ENUM_DECLARATION => {
                    if let Some(enum_decl) = self.arena.get_enum(check)
                        && self.get_identifier_text_idx(enum_decl.name) == name
                    {
                        found_value = true;
                    }
                }
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                    let names = self.collect_variable_names_from_node(check);
                    if names.contains(&name.to_string()) {
                        found_value = true;
                    }
                }
                k if k == syntax_kind_ext::MODULE_DECLARATION => {
                    if let Some(module) = self.arena.get_module(check)
                        && self.get_identifier_text_idx(module.name) == name
                    {
                        // Ambient namespaces with value members still represent
                        // a runtime object that may exist elsewhere, so aliases
                        // to their value members must be preserved.
                        if self.module_decl_has_runtime_alias_target(module) {
                            found_value = true;
                        } else {
                            found_type_only = true;
                        }
                    }
                }
                _ => {}
            }
        }

        // If we found a value declaration, it has runtime value
        // even if there's also a type declaration with the same name
        if found_value {
            return true;
        }
        // If we only found type declarations, it's type-only
        if found_type_only {
            return false;
        }
        // Unresolved: conservative default - assume runtime value
        true
    }

    pub(in crate::emitter::declarations) fn module_decl_has_runtime_alias_target(
        &self,
        module: &tsz_parser::parser::node::ModuleData,
    ) -> bool {
        if self.arena.is_declare(&module.modifiers) {
            return self.ambient_module_body_has_runtime_value(module.body);
        }

        self.is_instantiated_module(module.body)
    }

    pub(in crate::emitter::declarations) fn ambient_module_body_has_runtime_value(
        &self,
        module_body: NodeIndex,
    ) -> bool {
        let Some(body_node) = self.arena.get(module_body) else {
            return false;
        };

        if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            let Some(inner_module) = self.arena.get_module(body_node) else {
                return false;
            };
            return self.module_decl_has_runtime_alias_target(inner_module);
        }

        let Some(block) = self.arena.get_module_block(body_node) else {
            return false;
        };
        let Some(statements) = &block.statements else {
            return false;
        };

        statements.nodes.iter().any(|&stmt_idx| {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                return false;
            };

            match stmt_node.kind {
                k if k == syntax_kind_ext::INTERFACE_DECLARATION
                    || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION =>
                {
                    false
                }
                k if k == syntax_kind_ext::EXPORT_DECLARATION => self
                    .arena
                    .get_export_decl(stmt_node)
                    .filter(|export| !export.is_type_only)
                    .and_then(|export| self.arena.get(export.export_clause))
                    .is_some_and(|inner| self.ambient_namespace_statement_has_runtime_value(inner)),
                _ => self.ambient_namespace_statement_has_runtime_value(stmt_node),
            }
        })
    }

    pub(in crate::emitter::declarations) fn ambient_namespace_statement_has_runtime_value(
        &self,
        stmt_node: &Node,
    ) -> bool {
        match stmt_node.kind {
            k if k == syntax_kind_ext::INTERFACE_DECLARATION
                || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION =>
            {
                false
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => self
                .arena
                .get_module(stmt_node)
                .is_some_and(|module| self.module_decl_has_runtime_alias_target(module)),
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => self
                .arena
                .get_import_decl(stmt_node)
                .is_some_and(|import| !import.is_type_only),
            _ => true,
        }
    }

    pub(in crate::emitter) fn namespace_alias_target_has_runtime_value(
        &self,
        target: NodeIndex,
        scope_body: Option<NodeIndex>,
    ) -> bool {
        if let Some((has_runtime, _)) = self.resolve_entity_runtime_value(target, scope_body) {
            return has_runtime;
        }

        if scope_body.is_some() {
            return self
                .resolve_entity_runtime_value(target, None)
                .is_none_or(|(has_runtime, _)| has_runtime);
        }

        true
    }

    /// Resolve whether an entity name has runtime value semantics in a scope.
    /// Returns:
    /// - `None`: unresolved (caller should be conservative)
    /// - `(has_runtime, nested_scope)`:
    ///   - `has_runtime`: whether the resolved symbol exists at runtime
    ///   - `nested_scope`: module body for namespace-qualified lookup continuation
    pub(in crate::emitter::declarations) fn resolve_entity_runtime_value(
        &self,
        entity: NodeIndex,
        scope_body: Option<NodeIndex>,
    ) -> Option<(bool, Option<NodeIndex>)> {
        let node = self.arena.get(entity)?;

        if let Some(qualified) = self.arena.get_qualified_name(node) {
            let left = self.resolve_entity_runtime_value(qualified.left, scope_body)?;
            if !left.0 {
                return Some((false, None));
            }
            if let Some(next_scope) = left.1 {
                return self
                    .resolve_entity_runtime_value(qualified.right, Some(next_scope))
                    .or(Some((true, None)));
            }
            return Some((true, None));
        }

        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let name = self.get_identifier_text_idx(entity);
        if name.is_empty() {
            return None;
        }

        let statements = self.scope_statements_for_runtime_lookup(scope_body);
        if statements.is_empty() {
            return None;
        }

        let mut matched = false;
        let mut has_runtime = false;
        let mut nested_scope = None;

        for stmt_idx in statements {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let Some((stmt_runtime, stmt_scope)) =
                self.statement_runtime_for_name(stmt_node, &name, scope_body)
            else {
                continue;
            };

            matched = true;
            if stmt_runtime {
                has_runtime = true;
                if nested_scope.is_none() {
                    nested_scope = stmt_scope;
                }
            }
        }

        if matched {
            Some((has_runtime, nested_scope))
        } else {
            None
        }
    }

    pub(in crate::emitter::declarations) fn scope_statements_for_runtime_lookup(
        &self,
        scope_body: Option<NodeIndex>,
    ) -> Vec<NodeIndex> {
        if let Some(scope_idx) = scope_body {
            let Some(scope_node) = self.arena.get(scope_idx) else {
                return Vec::new();
            };

            if let Some(module) = self.arena.get_module(scope_node) {
                return self.scope_statements_for_runtime_lookup(Some(module.body));
            }

            if let Some(block) = self.arena.get_module_block(scope_node)
                && let Some(stmts) = &block.statements
            {
                return stmts.nodes.clone();
            }

            if let Some(source) = self.arena.get_source_file(scope_node) {
                return source.statements.nodes.clone();
            }

            return Vec::new();
        }

        for node in &self.arena.nodes {
            if node.kind == syntax_kind_ext::SOURCE_FILE
                && let Some(source) = self.arena.get_source_file(node)
            {
                return source.statements.nodes.clone();
            }
        }

        Vec::new()
    }

    pub(in crate::emitter::declarations) fn statement_runtime_for_name(
        &self,
        stmt_node: &Node,
        name: &str,
        scope_body: Option<NodeIndex>,
    ) -> Option<(bool, Option<NodeIndex>)> {
        match stmt_node.kind {
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                let export = self.arena.get_export_decl(stmt_node)?;
                let inner = self.arena.get(export.export_clause)?;
                self.statement_runtime_for_name(inner, name, scope_body)
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                let module = self.arena.get_module(stmt_node)?;
                if self.get_identifier_text_idx(module.name) != name {
                    return None;
                }
                let runtime = self.module_decl_has_runtime_alias_target(module);
                Some((runtime, if runtime { Some(module.body) } else { None }))
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                let class = self.arena.get_class(stmt_node)?;
                if self.get_identifier_text_idx(class.name) != name {
                    return None;
                }
                let runtime = !self.arena.is_declare(&class.modifiers);
                Some((runtime, None))
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                let func = self.arena.get_function(stmt_node)?;
                if self.get_identifier_text_idx(func.name) != name {
                    return None;
                }
                let runtime = !self.arena.is_declare(&func.modifiers) && func.body.is_some();
                Some((runtime, None))
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                let enum_decl = self.arena.get_enum(stmt_node)?;
                if self.get_identifier_text_idx(enum_decl.name) != name {
                    return None;
                }
                let runtime = !self.arena.is_declare(&enum_decl.modifiers)
                    && !self
                        .arena
                        .has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword);
                Some((runtime, None))
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                let iface = self.arena.get_interface(stmt_node)?;
                if self.get_identifier_text_idx(iface.name) != name {
                    return None;
                }
                Some((false, None))
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                let type_alias = self.arena.get_type_alias(stmt_node)?;
                if self.get_identifier_text_idx(type_alias.name) != name {
                    return None;
                }
                Some((false, None))
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                // `var X`, `let X`, `export var X`, etc.
                // Structure: VariableStatement → declarations: [VariableDeclarationList]
                //            VariableDeclarationList → declarations: [VariableDeclaration, ...]
                let var_stmt = self.arena.get_variable(stmt_node)?;
                let is_declare = self.arena.is_declare(&var_stmt.modifiers);
                for &list_or_decl_idx in &var_stmt.declarations.nodes {
                    let Some(list_or_decl_node) = self.arena.get(list_or_decl_idx) else {
                        continue;
                    };
                    // May be a VariableDeclarationList wrapping individual declarations
                    if list_or_decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                        let Some(decl_list) = self.arena.get_variable(list_or_decl_node) else {
                            continue;
                        };
                        for &decl_idx in &decl_list.declarations.nodes {
                            let Some(decl_node) = self.arena.get(decl_idx) else {
                                continue;
                            };
                            let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                                continue;
                            };
                            if self.get_identifier_text_idx(decl.name) == name {
                                return Some((!is_declare, None));
                            }
                        }
                    } else {
                        // Direct VariableDeclaration
                        let Some(decl) = self.arena.get_variable_declaration(list_or_decl_node)
                        else {
                            continue;
                        };
                        if self.get_identifier_text_idx(decl.name) == name {
                            return Some((!is_declare, None));
                        }
                    }
                }
                None
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                let import = self.arena.get_import_decl(stmt_node)?;
                if self.get_identifier_text_idx(import.import_clause) != name {
                    return None;
                }
                let runtime = if let Some(spec_node) = self.arena.get(import.module_specifier) {
                    match spec_node.kind {
                        kind if kind == SyntaxKind::Identifier as u16
                            || kind == syntax_kind_ext::QUALIFIED_NAME =>
                        {
                            self.namespace_alias_target_has_runtime_value(
                                import.module_specifier,
                                scope_body,
                            )
                        }
                        _ => self.import_decl_has_runtime_value(import),
                    }
                } else {
                    self.import_decl_has_runtime_value(import)
                };
                Some((runtime, None))
            }
            _ => None,
        }
    }
}
