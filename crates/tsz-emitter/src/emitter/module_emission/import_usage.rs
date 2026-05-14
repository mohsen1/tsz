use super::super::{JsxEmit, Printer};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn import_clause_is_empty_named_import(
        &self,
        clause: &tsz_parser::parser::node::ImportClauseData,
    ) -> bool {
        clause.name.is_none()
            && clause.named_bindings.is_some()
            && self
                .arena
                .get(clause.named_bindings)
                .and_then(|bindings_node| self.arena.get_named_imports(bindings_node))
                .is_some_and(|named_imports| {
                    named_imports.name.is_none() && named_imports.elements.nodes.is_empty()
                })
    }

    pub(in crate::emitter) fn import_has_value_usage_after_node(
        &self,
        node: &Node,
        clause: &tsz_parser::parser::node::ImportClauseData,
    ) -> bool {
        if self.import_clause_is_namespace_only(clause)
            && self
                .arena
                .get_import_decl(node)
                .is_some_and(|import| self.import_references_type_only_export_equals_module(import))
        {
            return false;
        }

        let mut names = Vec::new();
        if clause.name.is_some() {
            let default_name = self.get_identifier_text_idx(clause.name);
            if !default_name.is_empty() {
                names.push(default_name);
            }
        }
        if clause.named_bindings.is_some()
            && let Some(bindings_node) = self.arena.get(clause.named_bindings)
            && let Some(named_imports) = self.arena.get_named_imports(bindings_node)
        {
            if named_imports.name.is_some() && named_imports.elements.nodes.is_empty() {
                let ns_name = self.get_identifier_text_idx(named_imports.name);
                if !ns_name.is_empty() {
                    names.push(ns_name);
                }
            } else {
                for &spec_idx in &named_imports.elements.nodes {
                    let Some(spec_node) = self.arena.get(spec_idx) else {
                        continue;
                    };
                    let Some(spec) = self.arena.get_specifier(spec_node) else {
                        continue;
                    };
                    if spec.is_type_only {
                        continue;
                    }
                    let local_name = self.get_identifier_text_idx(spec.name);
                    if !local_name.is_empty() {
                        names.push(local_name);
                    }
                }
            }
        }
        if names.is_empty() {
            return !self.import_clause_is_empty_named_import(clause);
        }
        let Some(source_text) = self.source_text else {
            return true;
        };
        // Issue #3597: ES import declarations are module-scoped, so a
        // top-level use BEFORE the import is still a real value use. Scan
        // the entire source with the import declaration's text whited out
        // (including trailing comments on the same line as the import).
        let Some(import_decl) = self.arena.get_import_decl(node) else {
            return true;
        };
        let haystack =
            Self::source_excluding_import_decl(source_text, node, import_decl, self.arena);
        // Strip type-only content from the haystack so that identifiers
        // appearing only in type positions (type annotations, declare lines,
        // other import/export type statements, etc.) don't count as value usages.
        let value_haystack = crate::import_usage::strip_type_only_content(&haystack);
        let value_haystack = crate::import_usage::strip_qualified_accesses_for_names(
            &value_haystack,
            &self.ctx.options.external_const_enum_bindings,
        );
        let appears_in_value_haystack = names
            .iter()
            .any(|name| crate::import_usage::contains_identifier_occurrence(&value_haystack, name));
        if appears_in_value_haystack {
            return true;
        }
        // Under `--emitDecoratorMetadata`, type annotations on decorated
        // class members become *value* references at runtime via
        // `__metadata("design:type", X)`. The standard type-only strip would
        // elide those names; check separately whether any decorated-member
        // type annotation in the unstripped haystack references one of our
        // imported names.
        if self.ctx.options.emit_decorator_metadata
            && names.iter().any(|name| {
                crate::import_usage::name_appears_in_decorator_metadata_type(&haystack, name)
            })
        {
            return true;
        }
        self.ctx.target_es5
            && self.async_return_type_uses_imported_promise_constructor_after_node(node, &names)
    }

    pub(in crate::emitter) fn import_clause_is_namespace_only(
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

    pub(in crate::emitter) fn import_references_type_only_export_equals_module(
        &self,
        import: &tsz_parser::parser::node::ImportDeclData,
    ) -> bool {
        let Some(module_node) = self.arena.get(import.module_specifier) else {
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

    fn async_return_type_uses_imported_promise_constructor_after_node(
        &self,
        import_node: &Node,
        names: &[String],
    ) -> bool {
        self.arena.nodes.iter().any(|node| {
            if node.pos < import_node.end {
                return false;
            }
            match node.kind {
                kind if kind == syntax_kind_ext::FUNCTION_DECLARATION
                    || kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || kind == syntax_kind_ext::ARROW_FUNCTION =>
                {
                    self.arena.get_function(node).is_some_and(|func| {
                        func.is_async
                            && self
                                .promise_constructor_type_name(func.type_annotation)
                                .is_some_and(|name| names.iter().any(|import| import == &name))
                    })
                }
                kind if kind == syntax_kind_ext::METHOD_DECLARATION => {
                    self.arena.get_method_decl(node).is_some_and(|method| {
                        self.arena
                            .has_modifier(&method.modifiers, SyntaxKind::AsyncKeyword)
                            && self
                                .promise_constructor_type_name(method.type_annotation)
                                .is_some_and(|name| names.iter().any(|import| import == &name))
                    })
                }
                _ => false,
            }
        })
    }

    fn promise_constructor_type_name(&self, type_annotation: NodeIndex) -> Option<String> {
        let type_node = self.arena.get(type_annotation)?;
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let type_ref = self.arena.get_type_ref(type_node)?;
        let type_name_node = self.arena.get(type_ref.type_name)?;
        if type_name_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let name = self.get_identifier_text_idx(type_ref.type_name);
        if name.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
            && name != "Promise"
            && name != "PromiseLike"
            && !self.is_type_only_declaration_name(&name)
        {
            Some(name)
        } else {
            None
        }
    }

    /// Returns true when the given import clause's default or namespace
    /// binding matches the configured JSX factory root name (e.g. `React`).
    /// Such imports must be exempt from text-based value-usage elision since
    /// JSX elements reference the factory implicitly. Mirrors the logic at
    /// `crates/tsz-emitter/src/emitter/module_wrapper/wrapper_entry.rs`
    /// around the AMD/UMD dependency collection (`is_jsx_factory_import`).
    pub(in crate::emitter) fn is_jsx_factory_import_clause(
        &self,
        clause: &tsz_parser::parser::node::ImportClauseData,
    ) -> bool {
        if !matches!(
            self.ctx.options.jsx,
            JsxEmit::Preserve | JsxEmit::React | JsxEmit::ReactNative
        ) {
            return false;
        }
        let factory_root = self
            .ctx
            .options
            .jsx_factory
            .as_deref()
            .and_then(|f| f.split('.').next())
            .unwrap_or("React");

        if clause.name.is_some() {
            let name = self.get_identifier_text_idx(clause.name);
            if name == factory_root {
                return true;
            }
        }

        if clause.named_bindings.is_some()
            && let Some(bindings_node) = self.arena.get(clause.named_bindings)
            && let Some(named_imports) = self.arena.get_named_imports(bindings_node)
            && named_imports.name.is_some()
            && named_imports.elements.nodes.is_empty()
        {
            let ns_name = self.get_identifier_text_idx(named_imports.name);
            if ns_name == factory_root {
                return true;
            }
        }

        false
    }

    /// Whether the default binding of an import clause is referenced as a
    /// value in the rest of the file. Mirrors `filter_value_specs_by_usage`
    /// for the default binding so that an unused default beside a used named
    /// or namespace binding is elided (matching tsc).
    pub(in crate::emitter) fn default_binding_has_value_usage(
        &self,
        import_node: &Node,
        default_name_idx: NodeIndex,
    ) -> bool {
        let local_name = self.get_identifier_text_idx(default_name_idx);
        if local_name.is_empty() {
            return true;
        }
        let Some(source_text) = self.source_text else {
            return true;
        };
        let Some(import_data) = self.arena.get_import_decl(import_node) else {
            return true;
        };
        // Issue #3597: ES import declarations are module-scoped; a top-level
        // use BEFORE the import is still a real value use. Scan the entire
        // source with the import declaration's text whited out.
        let haystack =
            Self::source_excluding_import_decl(source_text, import_node, import_data, self.arena);
        let value_haystack = crate::import_usage::strip_type_only_content(&haystack);
        let value_haystack = crate::import_usage::strip_qualified_accesses_for_names(
            &value_haystack,
            &self.ctx.options.external_const_enum_bindings,
        );
        if crate::import_usage::contains_identifier_occurrence(&value_haystack, &local_name) {
            return true;
        }
        // Under `--emitDecoratorMetadata`, decorated-member type
        // annotations are *value* references; preserve the default whose
        // name appears in such an annotation.
        self.ctx.options.emit_decorator_metadata
            && crate::import_usage::name_appears_in_decorator_metadata_type(&haystack, &local_name)
    }

    /// Filter named import specifiers to only those with value-level usage
    /// in the rest of the file. Used in --noCheck mode.
    pub(in crate::emitter) fn filter_value_specs_by_usage(
        &self,
        import_node: &Node,
        specs: &[NodeIndex],
    ) -> Vec<NodeIndex> {
        let Some(source_text) = self.source_text else {
            return specs.to_vec();
        };
        let Some(import_data) = self.arena.get_import_decl(import_node) else {
            return specs.to_vec();
        };
        // Issue #3597: scan the entire module so a use BEFORE the import
        // still keeps the binding alive.
        let haystack =
            Self::source_excluding_import_decl(source_text, import_node, import_data, self.arena);
        let value_haystack = crate::import_usage::strip_type_only_content(&haystack);
        let value_haystack = crate::import_usage::strip_qualified_accesses_for_names(
            &value_haystack,
            &self.ctx.options.external_const_enum_bindings,
        );

        specs
            .iter()
            .copied()
            .filter(|&spec_idx| {
                let Some(spec_node) = self.arena.get(spec_idx) else {
                    return true;
                };
                let Some(spec) = self.arena.get_specifier(spec_node) else {
                    return true;
                };
                let local_name = self.get_identifier_text_idx(spec.name);
                if local_name.is_empty() {
                    return true;
                }
                if crate::import_usage::contains_identifier_occurrence(&value_haystack, &local_name)
                {
                    return true;
                }
                // Under `--emitDecoratorMetadata`, decorated-member type
                // annotations are *value* references; preserve specs whose
                // name appears in such an annotation.
                self.ctx.options.emit_decorator_metadata
                    && crate::import_usage::name_appears_in_decorator_metadata_type(
                        &haystack,
                        &local_name,
                    )
            })
            .collect()
    }

    pub(in crate::emitter) fn default_import_has_value_usage_after_node(
        &self,
        import_node: &Node,
        import_data: &tsz_parser::parser::node::ImportDeclData,
        name_idx: NodeIndex,
    ) -> bool {
        let name = self.get_identifier_text_idx(name_idx);
        if name.is_empty() {
            return true;
        }
        let Some(source_text) = self.source_text else {
            return true;
        };
        // Issue #3597: scan the entire module so a use BEFORE the import
        // still keeps the default binding alive.
        let haystack =
            Self::source_excluding_import_decl(source_text, import_node, import_data, self.arena);
        let value_haystack = crate::import_usage::strip_type_only_content(&haystack);
        let value_haystack = crate::import_usage::strip_qualified_accesses_for_names(
            &value_haystack,
            &self.ctx.options.external_const_enum_bindings,
        );

        crate::import_usage::contains_identifier_occurrence(&value_haystack, &name)
            || (self.ctx.options.emit_decorator_metadata
                && crate::import_usage::name_appears_in_decorator_metadata_type(&haystack, &name))
    }

    /// Check if an import-equals declaration's identifier is used after the import.
    pub(in crate::emitter) fn import_equals_has_value_usage_after_node(
        &self,
        node: &Node,
        import_data: &tsz_parser::parser::node::ImportDeclData,
    ) -> bool {
        if self.import_references_type_only_export_equals_module(import_data) {
            return false;
        }

        let name = self.get_identifier_text_idx(import_data.import_clause);
        if name.is_empty() {
            return true;
        }
        let Some(source_text) = self.source_text else {
            return true;
        };
        let haystack = Self::source_after_import(source_text, node, import_data, self.arena);
        let value_haystack = crate::import_usage::strip_type_only_content(haystack);
        crate::import_usage::contains_identifier_occurrence(&value_haystack, &name)
    }

    /// Check if an external import-equals alias is only value-used through a
    /// later namespace import alias, such as:
    /// `import ReactRouter = require("react-router");`
    /// `import Route = ReactRouter.Route;`
    /// `<Route />`
    pub(in crate::emitter) fn import_equals_has_value_usage_through_namespace_alias_after_node(
        &self,
        import_stmt_idx: NodeIndex,
        import_data: &tsz_parser::parser::node::ImportDeclData,
        source: &tsz_parser::parser::node::SourceFileData,
    ) -> bool {
        let root_name = self.get_identifier_text_idx(import_data.import_clause);
        if root_name.is_empty() {
            return false;
        }

        let mut after_import = false;
        for &stmt_idx in &source.statements.nodes {
            if stmt_idx == import_stmt_idx {
                after_import = true;
                continue;
            }
            if !after_import {
                continue;
            }

            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                continue;
            }
            let Some(alias_import) = self.arena.get_import_decl(stmt_node) else {
                continue;
            };
            if alias_import.is_type_only || !self.import_decl_has_runtime_value(alias_import) {
                continue;
            }
            if self
                .get_module_root_name(alias_import.module_specifier)
                .as_deref()
                != Some(root_name.as_str())
            {
                continue;
            }
            if self.import_equals_has_value_usage_after_node(stmt_node, alias_import) {
                return true;
            }
        }

        false
    }

    /// Check if an import alias name has value usage in the remaining source.
    /// Used for namespace-scoped import alias elision: tsc erases `import X = Y`
    /// inside namespaces when X is only used in type positions.
    /// The search is scope-limited via `namespace_scope_end` to prevent
    /// sibling namespace references from keeping an alias alive.
    pub(in crate::emitter) fn import_alias_is_referenced_after_node(
        &self,
        node: &Node,
        import_data: &tsz_parser::parser::node::ImportDeclData,
    ) -> bool {
        let name = self.get_identifier_text_idx(import_data.import_clause);
        if name.is_empty() {
            return true;
        }
        let Some(source_text) = self.source_text else {
            return true;
        };
        let full_haystack = Self::source_after_import(source_text, node, import_data, self.arena);
        // Limit the search to the current namespace body scope
        let haystack = if self.namespace_scope_end < u32::MAX {
            let full_start_in_source = source_text.len() - full_haystack.len();
            let scope_end_usize = self.namespace_scope_end as usize;
            if scope_end_usize <= full_start_in_source {
                ""
            } else {
                let end_in_full = scope_end_usize - full_start_in_source;
                &full_haystack[..end_in_full.min(full_haystack.len())]
            }
        } else {
            full_haystack
        };
        // Strip type-only content including inline type annotations so that
        // type-position references (e.g., `p1: modes.IMode`) don't count as
        // value usage. This matches tsc which erases namespace import aliases
        // when the alias is only referenced in type positions.
        let stripped = crate::import_usage::strip_type_only_content(haystack);
        Self::contains_alias_value_reference_before_shadow(&stripped, &name)
    }

    pub(in crate::emitter) fn contains_alias_value_reference_before_shadow(
        haystack: &str,
        ident: &str,
    ) -> bool {
        if ident.is_empty() {
            return false;
        }

        let mut search_from = 0usize;
        while let Some(rel) = haystack[search_from..].find(ident) {
            let pos = search_from + rel;
            if Self::is_standalone_identifier_at(haystack, ident, pos) {
                if Self::identifier_occurrence_is_binding(haystack, pos) {
                    // A second `import <ident> = ...` re-declares the same
                    // alias; it doesn't shadow the original import — both
                    // refer to the same name. tsc treats this as a duplicate
                    // diagnostic but still emits the first value-bearing
                    // import. Skip past this binding and keep searching for a
                    // genuine value reference (e.g., `<ident>.foo`).
                    if Self::binding_is_import_redeclaration(haystack, pos) {
                        search_from = pos + ident.len();
                        continue;
                    }
                    return false;
                }
                return true;
            }
            search_from = pos + ident.len();
        }
        false
    }

    fn binding_is_import_redeclaration(haystack: &str, pos: usize) -> bool {
        let bytes = haystack.as_bytes();
        let mut p = pos;
        while p > 0 && bytes[p - 1].is_ascii_whitespace() {
            p -= 1;
        }
        let preceding = &haystack[..p];
        if !preceding.ends_with("import") {
            return false;
        }
        let start = p - "import".len();
        let before_keyword_ok = start == 0
            || haystack[..start]
                .chars()
                .next_back()
                .is_none_or(|ch| !(ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()));
        if !before_keyword_ok {
            return false;
        }

        let mut after = pos;
        while after < bytes.len()
            && (bytes[after] == b'_'
                || bytes[after] == b'$'
                || bytes[after].is_ascii_alphanumeric())
        {
            after += 1;
        }
        while after < bytes.len() && bytes[after].is_ascii_whitespace() {
            after += 1;
        }
        bytes.get(after) == Some(&b'=')
    }

    fn is_standalone_identifier_at(haystack: &str, ident: &str, pos: usize) -> bool {
        let before_ok = if pos == 0 {
            true
        } else {
            haystack[..pos].chars().next_back().is_none_or(|ch| {
                !(ch == '_' || ch == '$' || ch == '.' || ch.is_ascii_alphanumeric())
            })
        };
        let after_idx = pos + ident.len();
        let after_ok = if after_idx >= haystack.len() {
            true
        } else {
            haystack[after_idx..]
                .chars()
                .next()
                .is_none_or(|ch| !(ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))
        };
        before_ok && after_ok
    }

    fn identifier_occurrence_is_binding(haystack: &str, pos: usize) -> bool {
        let bytes = haystack.as_bytes();
        let mut p = pos;
        while p > 0 && bytes[p - 1].is_ascii_whitespace() {
            p -= 1;
        }

        let preceding = &haystack[..p];
        for keyword in [
            "var",
            "let",
            "const",
            "function",
            "class",
            "enum",
            "namespace",
            "module",
            "import",
        ] {
            if !preceding.ends_with(keyword) {
                continue;
            }
            let start = p - keyword.len();
            let before_keyword_ok = start == 0
                || haystack[..start]
                    .chars()
                    .next_back()
                    .is_none_or(|ch| !(ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()));
            if before_keyword_ok {
                return true;
            }
        }

        false
    }

    /// Get the source text after an import node (skipping to the next line).
    pub(in crate::emitter) fn source_after_import<'b>(
        source_text: &'b str,
        node: &Node,
        import_data: &tsz_parser::parser::node::ImportDeclData,
        arena: &tsz_parser::parser::node::NodeArena,
    ) -> &'b str {
        let mut start = if let Some(module_node) = arena.get(import_data.module_specifier) {
            module_node.end as usize
        } else {
            node.end as usize
        };
        start = start.min(source_text.len());
        let bytes = source_text.as_bytes();
        // Skip past the entire import line (including trailing comments)
        while start < bytes.len() {
            match bytes[start] {
                b'\n' => {
                    start += 1;
                    break;
                }
                b'\r' => {
                    start += 1;
                    if start < bytes.len() && bytes[start] == b'\n' {
                        start += 1;
                    }
                    break;
                }
                _ => start += 1,
            }
        }
        &source_text[start..]
    }

    /// Get the full source text with the import declaration's text replaced
    /// by space characters (preserving line breaks). Used by ESM
    /// import-elision usage scans so that a top-level use BEFORE the import
    /// declaration still counts as a use (issue #3597). The replacement
    /// keeps byte offsets stable and stops the import declaration's own
    /// specifiers from showing up as uses.
    pub(in crate::emitter) fn source_excluding_import_decl(
        source_text: &str,
        node: &Node,
        import_data: &tsz_parser::parser::node::ImportDeclData,
        arena: &tsz_parser::parser::node::NodeArena,
    ) -> String {
        let import_start = (node.pos as usize).min(source_text.len());
        let after_slice = Self::source_after_import(source_text, node, import_data, arena);
        let import_end = source_text.len().saturating_sub(after_slice.len());
        let mut out = String::with_capacity(source_text.len());
        out.push_str(&source_text[..import_start]);
        let cleared = source_text
            .get(import_start..import_end)
            .unwrap_or("")
            .chars()
            .map(|c| if c == '\n' || c == '\r' { c } else { ' ' })
            .collect::<String>();
        out.push_str(&cleared);
        out.push_str(after_slice);
        out
    }
}
