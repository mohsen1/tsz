use super::super::DeclarationEmitter;
use tsz_binder::BinderState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn call_initializer_types_versions_self_reference_falls_back_to_any(
        &self,
        initializer: NodeIndex,
        type_text: &str,
    ) -> bool {
        let Some((import_specifier, _)) = self.parse_import_type_text(type_text) else {
            return false;
        };
        if import_specifier != Self::bare_package_specifier(&import_specifier) {
            return false;
        }

        let Some(call_module_specifier) =
            self.call_expression_imported_module_specifier(initializer)
        else {
            return false;
        };
        if call_module_specifier != import_specifier {
            return false;
        }

        let Some(package_root) = self.find_package_root_for_name(&import_specifier) else {
            return false;
        };

        Self::package_root_has_types_versions_self_back_reference(std::path::Path::new(
            &package_root,
        ))
    }

    fn call_expression_imported_module_specifier(&self, initializer: NodeIndex) -> Option<String> {
        let init_node = self.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(init_node)?;
        self.imported_value_module_specifier_from_syntax(call.expression)
            .or_else(|| {
                let binder = self.binder?;
                let sym_id = self.value_reference_symbol(call.expression)?;
                self.imported_value_module_specifier(sym_id, binder)
            })
            .filter(|specifier| !specifier.starts_with('.') && !specifier.starts_with('/'))
    }

    pub(in crate::declaration_emitter) fn package_root_has_types_versions_self_back_reference(
        package_root: &std::path::Path,
    ) -> bool {
        let pkg_json_path = package_root.join("package.json");
        let Ok(pkg_content) = std::fs::read_to_string(pkg_json_path) else {
            return false;
        };
        let Ok(pkg_json) = serde_json::from_str::<serde_json::Value>(&pkg_content) else {
            return false;
        };
        let Some(types_versions) = pkg_json
            .get("typesVersions")
            .and_then(|value| value.as_object())
        else {
            return false;
        };

        for mappings in types_versions.values() {
            let Some(mappings) = mappings.as_object() else {
                continue;
            };
            for (mapping_key, targets) in mappings {
                if mapping_key != "*" {
                    continue;
                }
                let Some(targets) = targets.as_array() else {
                    continue;
                };
                for target in targets {
                    let Some(target_str) = target.as_str() else {
                        continue;
                    };
                    if Self::types_versions_target_reexports_package_root(package_root, target_str)
                    {
                        return true;
                    }
                }
            }
        }

        false
    }

    fn types_versions_target_reexports_package_root(
        package_root: &std::path::Path,
        target: &str,
    ) -> bool {
        let target_prefix = target.trim_end_matches('*').trim_end_matches('/');
        if target_prefix.is_empty() {
            return false;
        }

        let candidates = if target_prefix.ends_with(".d.ts") || target_prefix.ends_with(".ts") {
            vec![package_root.join(target_prefix)]
        } else {
            vec![
                package_root.join(target_prefix).join("index.d.ts"),
                package_root.join(target_prefix).join("index.ts"),
                package_root.join(format!("{target_prefix}.d.ts")),
                package_root.join(format!("{target_prefix}.ts")),
            ]
        };

        candidates.iter().any(|candidate| {
            std::fs::read_to_string(candidate)
                .is_ok_and(|content| Self::module_text_reexports_parent_root(&content))
        })
    }

    fn module_text_reexports_parent_root(content: &str) -> bool {
        content.lines().any(|line| {
            let trimmed = line.trim().trim_end_matches(';').trim();
            if !trimmed.starts_with("export ") {
                return false;
            }
            let Some((_, module_specifier)) = trimmed.rsplit_once(" from ") else {
                return false;
            };
            let module_specifier = module_specifier.trim().trim_matches('"').trim_matches('\'');
            module_specifier == ".." || module_specifier == "../"
        })
    }

    pub(in crate::declaration_emitter) fn imported_call_public_type_text(
        &self,
        initializer: NodeIndex,
        type_text: &str,
    ) -> String {
        let Some(init_node) = self.arena.get(initializer) else {
            return type_text.to_string();
        };
        let expression = if init_node.kind == syntax_kind_ext::CALL_EXPRESSION {
            let Some(call) = self.arena.get_call_expr(init_node) else {
                return type_text.to_string();
            };
            call.expression
        } else if init_node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS {
            let Some(expr) = self.arena.get_expr_type_args(init_node) else {
                return type_text.to_string();
            };
            expr.expression
        } else {
            return type_text.to_string();
        };
        let Some(module_specifier) = self
            .imported_value_module_specifier_from_syntax(expression)
            .or_else(|| {
                let binder = self.binder?;
                let sym_id = self.value_reference_symbol(expression)?;
                self.imported_value_module_specifier(sym_id, binder)
            })
        else {
            return type_text.to_string();
        };
        if module_specifier.starts_with('.') || module_specifier.starts_with('/') {
            return type_text.to_string();
        }

        let simplified = Self::simplify_empty_object_false_conditionals(type_text);
        let rewritten =
            self.rewrite_unqualified_typeof_module_exports(&simplified, &module_specifier);
        let rewritten = self.rewrite_any_members_for_module_exports(&rewritten, &module_specifier);
        Self::normalize_nested_arrow_object_type_text(&rewritten)
    }

    fn simplify_empty_object_false_conditionals(type_text: &str) -> String {
        const NEEDLE: &str = "{} extends ";
        let mut output = String::with_capacity(type_text.len());
        let mut remaining = type_text;

        while let Some(start) = remaining.find(NEEDLE) {
            output.push_str(&remaining[..start]);
            let after_extends = &remaining[start + NEEDLE.len()..];
            let Some(question_rel) = after_extends.find('?') else {
                output.push_str(&remaining[start..]);
                return output;
            };
            let extends_text = after_extends[..question_rel].trim();
            if !Self::empty_object_extends_is_known_false(extends_text) {
                output.push_str(&remaining[start..start + NEEDLE.len() + question_rel + 1]);
                remaining = &after_extends[question_rel + 1..];
                continue;
            }
            let after_question = &after_extends[question_rel + 1..];
            let Some(colon_rel) = after_question.find(':') else {
                output.push_str(&remaining[start..]);
                return output;
            };
            let after_colon = &after_question[colon_rel + 1..];
            let false_end = after_colon
                .char_indices()
                .find_map(|(idx, ch)| matches!(ch, ';' | ',' | '\n' | '}').then_some(idx))
                .unwrap_or(after_colon.len());
            let false_text = after_colon[..false_end].trim();
            if false_text.is_empty() {
                output.push_str(&remaining[start..]);
                return output;
            }
            output.push_str(false_text);
            remaining = &after_colon[false_end..];
        }

        output.push_str(remaining);
        output
    }

    fn empty_object_extends_is_known_false(extends_text: &str) -> bool {
        let normalized = extends_text.trim();
        !(normalized == "{}"
            || normalized == "object"
            || normalized == "unknown"
            || normalized == "any")
    }

    fn rewrite_unqualified_typeof_module_exports(
        &self,
        type_text: &str,
        module_specifier: &str,
    ) -> String {
        let Some(binder) = self.binder else {
            return type_text.to_string();
        };

        let mut output = String::with_capacity(type_text.len());
        let mut remaining = type_text;
        while let Some(start) = remaining.find("typeof ") {
            output.push_str(&remaining[..start + "typeof ".len()]);
            let after = &remaining[start + "typeof ".len()..];
            let name_end = after
                .char_indices()
                .find_map(|(idx, ch)| {
                    (!Self::is_type_reference_identifier_continue(ch)).then_some(idx)
                })
                .unwrap_or(after.len());
            let name = &after[..name_end];
            if !name.is_empty()
                && name != "import"
                && self.imported_module_may_export_name(binder, module_specifier, name)
                && !self.current_file_imports_name_from_module(module_specifier, name)
            {
                output.push_str(&format!("import(\"{module_specifier}\").{name}"));
            } else {
                output.push_str(name);
            }
            remaining = &after[name_end..];
        }
        output.push_str(remaining);
        output
    }

    fn rewrite_any_members_for_module_exports(
        &self,
        type_text: &str,
        module_specifier: &str,
    ) -> String {
        let Some(binder) = self.binder else {
            return type_text.to_string();
        };

        let mut rewritten = String::with_capacity(type_text.len());
        for (line_idx, line) in type_text.lines().enumerate() {
            if line_idx > 0 {
                rewritten.push('\n');
            }
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();
            let Some((name, suffix)) = trimmed.split_once(": any") else {
                rewritten.push_str(line);
                continue;
            };
            if !Self::is_identifier_text(name)
                || !(suffix.starts_with(';') || suffix.starts_with(','))
                || !self.imported_module_may_export_name(binder, module_specifier, name)
                || self.current_file_imports_name_from_module(module_specifier, name)
            {
                rewritten.push_str(line);
                continue;
            }
            rewritten.push_str(&line[..indent_len]);
            rewritten.push_str(name);
            rewritten.push_str(": typeof import(\"");
            rewritten.push_str(module_specifier);
            rewritten.push_str("\").");
            rewritten.push_str(name);
            rewritten.push_str(suffix);
        }

        if type_text.ends_with('\n') {
            rewritten.push('\n');
        }
        rewritten
    }

    fn current_file_imports_name_from_module(&self, module_specifier: &str, name: &str) -> bool {
        let Some(source_file) = self
            .current_source_file_idx
            .and_then(|source_file_idx| self.arena.get(source_file_idx))
            .and_then(|node| self.arena.get_source_file(node))
            .or_else(|| self.arena_source_file(self.arena))
        else {
            return false;
        };

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
            if module_lit.text != module_specifier {
                continue;
            }
            let Some(clause_node) = self.arena.get(import.import_clause) else {
                continue;
            };
            let Some(clause) = self.arena.get_import_clause(clause_node) else {
                continue;
            };
            if self.get_identifier_text(clause.name).as_deref() == Some(name) {
                return true;
            }
            if let Some(bindings_node) = self.arena.get(clause.named_bindings)
                && let Some(bindings) = self.arena.get_named_imports(bindings_node)
            {
                for &spec_idx in &bindings.elements.nodes {
                    let Some(spec_node) = self.arena.get(spec_idx) else {
                        continue;
                    };
                    let Some(specifier) = self.arena.get_specifier(spec_node) else {
                        continue;
                    };
                    if self.get_identifier_text(specifier.name).as_deref() == Some(name) {
                        return true;
                    }
                }
            }
        }

        false
    }

    fn imported_module_may_export_name(
        &self,
        binder: &BinderState,
        module_specifier: &str,
        name: &str,
    ) -> bool {
        if self.imported_module_exports_name(binder, module_specifier, name) {
            return true;
        }

        binder.symbols.iter().any(|symbol| {
            if symbol.escaped_name != name {
                return false;
            }
            let Some(module_path) = self.resolve_symbol_module_path(symbol.id) else {
                return false;
            };
            module_path == module_specifier
                || self.node_modules_path_matches_import_specifier(&module_path, module_specifier)
                || self.node_modules_package_path_matches_import_specifier(
                    &module_path,
                    module_specifier,
                )
                || self
                    .node_modules_package_contains_import_specifier(&module_path, module_specifier)
        })
    }

    fn is_identifier_text(text: &str) -> bool {
        let mut chars = text.chars();
        chars
            .next()
            .is_some_and(Self::is_type_reference_identifier_start)
            && chars.all(Self::is_type_reference_identifier_continue)
    }

    fn normalize_nested_arrow_object_type_text(type_text: &str) -> String {
        if !type_text.contains("=> {\n        ") {
            return type_text.to_string();
        }
        let mut normalized = type_text.replace("\n        ", "\n    ");
        if normalized.ends_with("\n    }") {
            let new_len = normalized.len() - "\n    }".len();
            normalized.truncate(new_len);
            normalized.push_str("\n}");
        }
        normalized
    }
}
