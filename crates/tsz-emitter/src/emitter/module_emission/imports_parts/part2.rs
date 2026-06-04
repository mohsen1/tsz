impl<'a> Printer<'a> {
    pub(in crate::emitter) fn source_needs_node_esm_create_require(
        &self,
        statements: &tsz_parser::parser::NodeList,
    ) -> bool {
        self.ctx.options.resolved_node_module_to_esm
            && statements.nodes.iter().any(|&stmt_idx| {
                self.arena.get(stmt_idx).is_some_and(|stmt| {
                    if stmt.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                        return self.import_equals_declaration_needs_node_esm_create_require(stmt);
                    }
                    if let Some(export) = self.arena.get_export_decl(stmt)
                        && let Some(clause_node) = self.arena.get(export.export_clause)
                        && clause_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                    {
                        // When an IMPORT_EQUALS_DECLARATION is the clause of an
                        // EXPORT_DECLARATION, the ExportKeyword is on the outer statement,
                        // not on the inner clause node. The declaration is always exported,
                        // so only check whether it is an external (require) reference.
                        return self.import_equals_declaration_is_external(clause_node);
                    }
                    false
                })
            })
    }

    fn import_equals_declaration_needs_node_esm_create_require(&self, node: &Node) -> bool {
        let Some(import) = self.arena.get_import_decl(node) else {
            return false;
        };
        if !self.import_equals_declaration_is_external(node) {
            return false;
        }
        if self
            .arena
            .has_modifier(&import.modifiers, SyntaxKind::ExportKeyword)
            || self.ctx.options.verbatim_module_syntax
            || self.source_is_js_file
        {
            return true;
        }
        self.import_equals_has_value_usage_after_node(node, import)
    }

    pub(in crate::emitter) fn import_equals_declaration_is_external(&self, node: &Node) -> bool {
        self.arena.get_import_decl(node).is_some_and(|import| {
            !import.is_type_only
                && self
                    .arena
                    .get(import.module_specifier)
                    .is_some_and(|module_node| {
                        module_node.is_string_literal()
                            || module_node.kind == syntax_kind_ext::EXTERNAL_MODULE_REFERENCE
                    })
        })
    }

    pub(in crate::emitter) fn emit_node_esm_create_require_preamble(&mut self) {
        let (create_require_name, require_name) = self.node_esm_create_require_names();
        self.write("import { createRequire as ");
        self.write(&create_require_name);
        self.write(" } from \"module\";");
        self.write_line();
        self.write_var_or_const();
        self.write(&require_name);
        self.write(" = ");
        self.write(&create_require_name);
        self.write("(import.meta.url);");
        self.write_line();
    }

    fn node_esm_require_name(&mut self) -> String {
        self.node_esm_create_require_names().1
    }

    fn node_esm_create_require_names(&mut self) -> (String, String) {
        if let Some(names) = &self.node_esm_create_require_names {
            return names.clone();
        }
        let create_require_name = self.make_unique_exact_or_numbered_name("_createRequire");
        let require_name = self.make_unique_exact_or_numbered_name("__require");
        let names = (create_require_name, require_name);
        self.node_esm_create_require_names = Some(names.clone());
        names
    }

    fn make_unique_exact_or_numbered_name(&mut self, base: &str) -> String {
        if !self.file_identifiers.contains(base) && !self.generated_temp_names.contains(base) {
            let name = base.to_string();
            self.generated_temp_names.insert(name.clone());
            return name;
        }
        for suffix in 1..=1000 {
            let candidate = format!("{base}_{suffix}");
            if !self.file_identifiers.contains(&candidate)
                && !self.generated_temp_names.contains(&candidate)
            {
                self.generated_temp_names.insert(candidate.clone());
                return candidate;
            }
        }
        self.make_unique_name_fresh()
    }

    fn namespace_has_prior_import_equals_alias(&self, node: &Node, alias_name: &str) -> bool {
        let Some(source_text) = self.source_text else {
            return false;
        };
        let end = (node.pos as usize).min(source_text.len());
        let prefix = &source_text[..end];
        let last_open = prefix.rfind('{').map_or(0, |pos| pos + 1);
        let last_close = prefix.rfind('}').map_or(0, |pos| pos + 1);
        let scope_start = last_open.max(last_close);
        let prior = &source_text[scope_start..end];
        prior.lines().any(|line| {
            let trimmed = line.trim_start();
            let trimmed = trimmed.strip_prefix("export ").unwrap_or(trimmed);
            let Some(rest) = trimmed.strip_prefix("import ") else {
                return false;
            };
            let rest = rest.trim_start();
            let Some(after_name) = rest.strip_prefix(alias_name) else {
                return false;
            };
            let next = after_name.as_bytes().first().copied();
            let boundary =
                next.is_none_or(|b| !b.is_ascii_alphanumeric() && b != b'_' && b != b'$');
            boundary && after_name.trim_start().starts_with('=')
        })
    }

    fn is_valid_import_equals_reference(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16 => true,
            k if k == SyntaxKind::Identifier as u16 => self
                .arena
                .get_identifier(node)
                .is_some_and(|id| !id.escaped_text.is_empty()),
            k if k == SyntaxKind::ThisKeyword as u16 || k == SyntaxKind::SuperKeyword as u16 => {
                true
            }
            k if k == syntax_kind_ext::QUALIFIED_NAME => {
                self.arena.get_qualified_name(node).is_some_and(|name| {
                    self.is_valid_import_equals_reference(name.left)
                        && self.is_valid_import_equals_reference(name.right)
                })
            }
            _ => false,
        }
    }

    fn is_import_equals_reference_missing_trailing_identifier(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::QUALIFIED_NAME {
            return false;
        }
        let Some(name) = self.arena.get_qualified_name(node) else {
            return false;
        };
        let left_is_valid = self.is_valid_import_equals_reference(name.left);
        if !left_is_valid {
            return false;
        }
        self.arena
            .get(name.right)
            .filter(|right| right.kind == SyntaxKind::Identifier as u16)
            .and_then(|right| self.arena.get_identifier(right))
            .is_some_and(|ident| ident.escaped_text.is_empty())
    }

    const fn is_recovered_import_equals_expression(&self, node: &Node) -> bool {
        matches!(
            node.kind,
            k if k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        )
    }

    fn recovered_import_equals_rhs_text(&self, import_node: &Node) -> Option<&'a str> {
        let source = self.source_text_for_map()?;
        let start = import_node.pos as usize;
        let end = (import_node.end as usize).min(source.len());
        if start >= end {
            return None;
        }

        let declaration_text = &source[start..end];
        let equals_pos = declaration_text.find('=')?;
        let rhs_with_suffix = &declaration_text[equals_pos + 1..];
        let rhs = rhs_with_suffix
            .split_once(';')
            .map_or(rhs_with_suffix, |(before_semicolon, _)| before_semicolon)
            .trim();

        (!rhs.is_empty()).then_some(rhs)
    }

    pub(in crate::emitter) fn emit_import_clause(&mut self, node: &Node) {
        let Some(clause) = self.arena.get_import_clause(node) else {
            return;
        };

        let mut has_default = false;

        // Default import
        if clause.name.is_some() {
            self.emit(clause.name);
            has_default = true;
        }

        // Named bindings
        if clause.named_bindings.is_some() {
            if has_default {
                self.write(", ");
            }
            self.emit(clause.named_bindings);
        }
    }

    pub(in crate::emitter) fn emit_wrapped_import_interop_prologue(
        &mut self,
        statements: &NodeList,
    ) {
        if !matches!(
            self.ctx.original_module_kind,
            Some(ModuleKind::AMD | ModuleKind::UMD | ModuleKind::System)
        ) {
            return;
        }

        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION {
                continue;
            }
            let Some(import_decl) = self.arena.get_import_decl(stmt_node) else {
                continue;
            };
            if !self.import_decl_has_runtime_value(import_decl) {
                continue;
            }
            let Some(clause_node) = self.arena.get(import_decl.import_clause) else {
                continue;
            };
            let Some(clause) = self.arena.get_import_clause(clause_node) else {
                continue;
            };
            if clause.is_type_only {
                continue;
            }

            if !self.ctx.options.verbatim_module_syntax
                && !self.source_is_js_file
                && !self.is_jsx_factory_import_clause(clause)
                && !self.import_has_value_usage_after_node(stmt_node, clause)
            {
                continue;
            }

            if clause.name.is_some() {
                let local_name = self.get_identifier_text_idx(clause.name);
                if !local_name.is_empty()
                    && let Some(subst) = self
                        .commonjs_named_import_substitutions
                        .get(local_name.as_str())
                    && let Some(dep_var) = subst.strip_suffix(".default")
                {
                    let dep_var = dep_var.to_string();
                    self.write(&dep_var);
                    self.write(" = ");
                    self.write_helper("__importDefault");
                    self.write("(");
                    self.write(&dep_var);
                    self.write(");");
                    self.write_line();
                }
            }

            if clause.named_bindings.is_some()
                && let Some(bindings_node) = self.arena.get(clause.named_bindings)
                && let Some(named_imports) = self.arena.get_named_imports(bindings_node)
                && named_imports.name.is_some()
                && named_imports.elements.nodes.is_empty()
            {
                let local_name = self.get_identifier_text_idx(named_imports.name);
                if !local_name.is_empty() {
                    self.write(&local_name);
                    self.write(" = ");
                    self.write_helper("__importStar");
                    self.write("(");
                    self.write(&local_name);
                    self.write(");");
                    self.write_line();
                }
            }
        }
    }

    pub(in crate::emitter) fn emit_named_imports(&mut self, node: &Node) {
        let Some(imports) = self.arena.get_named_imports(node) else {
            return;
        };

        // Filter out type-only import specifiers
        let value_imports: Vec<_> = imports
            .elements
            .nodes
            .iter()
            .filter(|&spec_idx| {
                if let Some(spec_node) = self.arena.get(*spec_idx) {
                    if let Some(spec) = self.arena.get_specifier(spec_node) {
                        !spec.is_type_only
                    } else {
                        true
                    }
                } else {
                    true
                }
            })
            .collect();

        // If all imports are type-only, don't emit the named bindings at all
        if value_imports.is_empty() {
            return;
        }

        if imports.name.is_some() && value_imports.is_empty() {
            self.write("* as ");
            self.emit(imports.name);
            return;
        }

        self.write("{ ");
        // Convert Vec<&NodeIndex> to Vec<NodeIndex> for emit_comma_separated
        let value_refs: Vec<NodeIndex> = value_imports.iter().map(|&&idx| idx).collect();
        self.emit_comma_separated(&value_refs);
        // Preserve trailing comma from source
        let has_trailing_comma = self.has_trailing_comma_in_source(node, &imports.elements.nodes);
        if has_trailing_comma {
            self.write(",");
        }
        self.write(" }");
    }

    /// Emit import attributes (e.g., `with { type: "json" }` or `assert { type: "json" }`)
    /// if the given `NodeIndex` points to an `IMPORT_ATTRIBUTES` node.
    pub(in crate::emitter) fn emit_import_attributes(&mut self, attributes: NodeIndex) {
        let Some(attr_node) = self.arena.get(attributes) else {
            return;
        };
        let Some(attrs) = self.arena.get_import_attributes_data(attr_node) else {
            return;
        };
        let keyword = if attrs.token == SyntaxKind::AssertKeyword as u16 {
            "assert"
        } else {
            "with"
        };
        self.write(" ");
        self.write(keyword);
        self.write(" { ");
        for (i, &elem_idx) in attrs.elements.nodes.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            if let Some(elem_node) = self.arena.get(elem_idx)
                && let Some(attr) = self.arena.get_import_attribute_data(elem_node)
            {
                self.emit(attr.name);
                self.write(": ");
                self.emit(attr.value);
            }
        }
        self.write(" }");
    }
}
