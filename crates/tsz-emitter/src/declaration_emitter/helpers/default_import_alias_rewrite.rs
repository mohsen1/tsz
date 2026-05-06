//! Rewrite inferred import types that refer to default-import aliases.

use super::super::DeclarationEmitter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn call_receiver_default_import_alias(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<(String, String)> {
        let expr_idx = self.skip_parenthesized_expression(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;
        let receiver_idx = if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            self.arena.get_access_expr(expr_node)?.expression
        } else {
            expr_idx
        };
        let receiver_idx = self.skip_parenthesized_expression(receiver_idx)?;
        let local_name = self.get_identifier_text(receiver_idx)?;
        if let Some(module_specifier) =
            self.default_import_alias_module_from_variable_name(&local_name)
        {
            return Some((local_name, module_specifier));
        }
        let binder = self.binder?;
        let sym_id = binder.get_node_symbol(receiver_idx)?;
        let symbol = binder.symbols.get(sym_id)?;

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                continue;
            };
            let Some(init_idx) = self.skip_parenthesized_expression(decl.initializer) else {
                continue;
            };
            let Some(init_node) = self.arena.get(init_idx) else {
                continue;
            };
            if init_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                continue;
            }
            let Some(access) = self.arena.get_access_expr(init_node) else {
                continue;
            };
            if self.get_identifier_text(access.name_or_argument).as_deref() != Some("default") {
                continue;
            }
            let namespace_name = self.get_identifier_text(access.expression)?;
            let module_specifier = self.namespace_import_module_specifier(&namespace_name)?;
            return Some((local_name, module_specifier));
        }

        None
    }

    fn default_import_alias_module_from_variable_name(&self, local_name: &str) -> Option<String> {
        let source_file = self
            .current_source_file_idx
            .and_then(|source_file_idx| self.arena.get(source_file_idx))
            .and_then(|node| self.arena.get_source_file(node))
            .or_else(|| self.arena_source_file(self.arena))?;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
                continue;
            };
            for &decl_list_idx in &var_stmt.declarations.nodes {
                let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                    continue;
                };
                let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                    continue;
                };
                for &decl_idx in &decl_list.declarations.nodes {
                    let Some(decl_node) = self.arena.get(decl_idx) else {
                        continue;
                    };
                    let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                        continue;
                    };
                    if self.get_identifier_text(decl.name).as_deref() != Some(local_name) {
                        continue;
                    }
                    let Some(init_idx) = self.skip_parenthesized_expression(decl.initializer)
                    else {
                        continue;
                    };
                    let Some(init_node) = self.arena.get(init_idx) else {
                        continue;
                    };
                    if init_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                        continue;
                    }
                    let Some(access) = self.arena.get_access_expr(init_node) else {
                        continue;
                    };
                    if self.get_identifier_text(access.name_or_argument).as_deref()
                        != Some("default")
                    {
                        continue;
                    }
                    let namespace_name = self.get_identifier_text(access.expression)?;
                    return self.namespace_import_module_specifier(&namespace_name);
                }
            }
        }

        None
    }

    fn namespace_import_module_specifier(&self, namespace_name: &str) -> Option<String> {
        let source_file = self
            .current_source_file_idx
            .and_then(|source_file_idx| self.arena.get(source_file_idx))
            .and_then(|node| self.arena.get_source_file(node))
            .or_else(|| self.arena_source_file(self.arena))?;

        for &stmt_idx in &source_file.statements.nodes {
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
            let Some(bindings_node) = self.arena.get(clause.named_bindings) else {
                continue;
            };
            let Some(bindings) = self.arena.get_named_imports(bindings_node) else {
                continue;
            };
            if bindings.elements.nodes.is_empty()
                && self.get_identifier_text(bindings.name).as_deref() == Some(namespace_name)
            {
                return Some(module_lit.text.clone());
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn rewrite_import_type_export_to_default_alias(
        text: &str,
        export_name: &str,
        public_module: &str,
    ) -> String {
        let export_suffix = format!(".{export_name}");
        let mut rewritten = String::new();
        let mut remaining = text;

        while let Some((start, module_specifier, tail)) = Self::next_import_type_text(remaining) {
            let import_end = remaining.len() - tail.len();
            let Some(after_export) = tail.strip_prefix(&export_suffix) else {
                rewritten.push_str(&remaining[..import_end]);
                remaining = tail;
                continue;
            };
            if after_export
                .chars()
                .next()
                .is_some_and(Self::is_type_reference_identifier_continue)
                || !Self::import_type_module_is_public_subpath(&module_specifier, public_module)
            {
                rewritten.push_str(&remaining[..import_end]);
                remaining = tail;
                continue;
            }

            rewritten.push_str(&remaining[..start]);
            rewritten.push_str("import(\"");
            rewritten.push_str(public_module);
            rewritten.push_str("\").default");
            remaining = after_export;
        }

        rewritten.push_str(remaining);
        rewritten
    }

    fn import_type_module_is_public_subpath(module_specifier: &str, public_module: &str) -> bool {
        module_specifier == public_module
            || (!public_module.starts_with('.')
                && !public_module.starts_with('/')
                && module_specifier
                    .strip_prefix(public_module)
                    .is_some_and(|suffix| suffix.starts_with('/')))
    }

    pub(in crate::declaration_emitter) fn rewrite_call_receiver_default_import_aliases(
        &self,
        initializer: NodeIndex,
        type_text: String,
    ) -> String {
        let Some(init_idx) = self.skip_parenthesized_expression(initializer) else {
            return type_text;
        };
        let Some(init_node) = self.arena.get(init_idx) else {
            return type_text;
        };
        if init_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return type_text;
        }
        let Some(call) = self.arena.get_call_expr(init_node) else {
            return type_text;
        };
        let Some((alias_name, module_specifier)) =
            self.call_receiver_default_import_alias(call.expression)
        else {
            return type_text;
        };
        Self::rewrite_import_type_export_to_default_alias(
            &type_text,
            &alias_name,
            &module_specifier,
        )
    }
}
