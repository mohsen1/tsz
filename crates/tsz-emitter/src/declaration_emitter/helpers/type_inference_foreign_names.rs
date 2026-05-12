use super::super::DeclarationEmitter;
use tsz_parser::parser::node::NodeArena;
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn type_text_contains_unqualified_foreign_value_export(
        &self,
        source_arena: &NodeArena,
        source_path: &str,
        text: &str,
    ) -> bool {
        let Some(current_path) = self.current_file_path.as_deref() else {
            return false;
        };
        if self.paths_refer_to_same_source_file(current_path, source_path) {
            return false;
        }

        let Some(source_file) = self.arena_source_file(source_arena) else {
            return false;
        };

        source_file
            .statements
            .nodes
            .iter()
            .copied()
            .any(|stmt_idx| {
                let Some(stmt_node) = source_arena.get(stmt_idx) else {
                    return false;
                };
                let export_name = if let Some(decl) = source_arena.get_function(stmt_node) {
                    source_arena
                        .has_modifier(&decl.modifiers, SyntaxKind::ExportKeyword)
                        .then_some(decl.name)
                } else if let Some(var_stmt) = source_arena.get_variable(stmt_node) {
                    if !source_arena.has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword) {
                        None
                    } else {
                        var_stmt.declarations.nodes.first().and_then(|decl_idx| {
                            let decl_node = source_arena.get(*decl_idx)?;
                            let decl = source_arena.get_variable_declaration(decl_node)?;
                            Some(decl.name)
                        })
                    }
                } else {
                    None
                }
                .and_then(|name_idx| self.identifier_text_from_arena(source_arena, name_idx));

                export_name.is_some_and(|name| Self::contains_whole_word_in_text(text, &name))
            })
    }

    pub(in crate::declaration_emitter) fn qualify_foreign_imported_names_in_text(
        &self,
        source_arena: &NodeArena,
        text: &str,
    ) -> String {
        let Some(source_file) = self.arena_source_file(source_arena) else {
            return text.to_string();
        };

        let mut replacements = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = source_arena.get(stmt_idx) else {
                continue;
            };
            let Some(import) = source_arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_node) = source_arena.get(import.module_specifier) else {
                continue;
            };
            let Some(module_lit) = source_arena.get_literal(module_node) else {
                continue;
            };
            let module_specifier = module_lit.text.as_str();
            let Some(clause_node) = source_arena.get(import.import_clause) else {
                continue;
            };
            let Some(clause) = source_arena.get_import_clause(clause_node) else {
                continue;
            };

            if clause.name.is_some()
                && let Some(local_name) = self.identifier_text_from_arena(source_arena, clause.name)
            {
                let qualified = format!("import(\"{module_specifier}\").default");
                replacements.push((local_name, qualified));
            }

            if clause.named_bindings.is_some()
                && let Some(bindings_node) = source_arena.get(clause.named_bindings)
                && let Some(bindings) = source_arena.get_named_imports(bindings_node)
            {
                if bindings.name.is_some() && bindings.elements.nodes.is_empty() {
                    if let Some(local_name) =
                        self.identifier_text_from_arena(source_arena, bindings.name)
                    {
                        let qualified = format!("typeof import(\"{module_specifier}\")");
                        replacements.push((local_name, qualified));
                    }
                } else {
                    for &spec_idx in &bindings.elements.nodes {
                        let Some(spec_node) = source_arena.get(spec_idx) else {
                            continue;
                        };
                        let Some(specifier) = source_arena.get_specifier(spec_node) else {
                            continue;
                        };
                        let Some(local_name) =
                            self.identifier_text_from_arena(source_arena, specifier.name)
                        else {
                            continue;
                        };
                        let imported_name = if specifier.property_name.is_some() {
                            self.identifier_text_from_arena(source_arena, specifier.property_name)
                                .unwrap_or(local_name.clone())
                        } else {
                            local_name.clone()
                        };
                        let qualified = format!("import(\"{module_specifier}\").{imported_name}");
                        replacements.push((local_name, qualified));
                    }
                }
            }
        }

        if let Some(binder) = self.binder {
            let current_path = self
                .current_file_path
                .as_deref()
                .unwrap_or(source_file.file_name.as_str());
            for symbol in binder.symbols.iter() {
                if !symbol.is_umd_export
                    || !Self::contains_whole_word_in_text(text, &symbol.escaped_name)
                {
                    continue;
                }
                let Some(module_path) = symbol.import_module.as_deref() else {
                    continue;
                };
                let Some(module_specifier) =
                    self.package_specifier_for_node_modules_path(current_path, module_path)
                else {
                    continue;
                };
                let qualified = format!("import(\"{module_specifier}\")");
                replacements.push((symbol.escaped_name.clone(), qualified));
            }
        }

        Self::replace_whole_words_in_text(text, &replacements)
    }
}
