//! Declaration emitter - import, module, import-equals, and namespace export emission.

use super::super::DeclarationEmitter;
use crate::transforms::emit_utils::string_literal_text;
use rustc_hash::FxHashSet;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(crate) fn emit_import_declaration_if_needed(&mut self, import_idx: NodeIndex) {
        // Source imports carry fidelity that auto-generated imports cannot reproduce
        // (aliasing, `type` modifiers, attributes, and source ordering). Emit them
        // through the filtered declaration path and reserve auto-import synthesis for
        // genuinely foreign symbols that have no source import in this file.
        self.emit_import_declaration(import_idx);
    }

    pub(crate) fn emit_deferred_js_import_declaration(&mut self, import_idx: NodeIndex) -> bool {
        let Some(import_node) = self.arena.get(import_idx) else {
            return false;
        };
        let Some(import) = self.arena.get_import_decl(import_node) else {
            return false;
        };

        if import.import_clause.is_none() {
            let before = self.writer.len();
            self.emit_import_declaration(import_idx);
            return self.writer.len() > before;
        }

        let (default_used, named_used) = self.count_used_imports(import);
        if default_used == 0 && named_used == 0 {
            if self.is_import_required_by_augmentation(import.module_specifier) {
                self.write_indent();
                self.write("import ");
                self.emit_node(import.module_specifier);
                self.write(";");
                self.write_line();
                return true;
            }
            return false;
        }

        let Some(clause_node) = self.arena.get(import.import_clause) else {
            return false;
        };
        let Some(clause) = self.arena.get_import_clause(clause_node) else {
            return false;
        };

        let mut emitted = false;
        if clause.name.is_some() && default_used > 0 {
            self.write_indent();
            self.write("import ");
            if clause.is_type_only {
                self.write("type ");
            }
            if clause.is_deferred {
                self.write("defer ");
            }
            self.emit_node(clause.name);
            self.write(" from ");
            self.emit_node(import.module_specifier);
            self.emit_declaration_import_attributes(import.attributes);
            self.write(";");
            self.write_line();
            emitted = true;
        }

        if clause.named_bindings.is_some() && named_used > 0 {
            let Some(bindings_node) = self.arena.get(clause.named_bindings) else {
                return emitted;
            };
            let Some(bindings) = self.arena.get_named_imports(bindings_node) else {
                return emitted;
            };

            if bindings.name.is_some() && bindings.elements.nodes.is_empty() {
                self.write_indent();
                self.write("import ");
                if clause.is_type_only {
                    self.write("type ");
                }
                if clause.is_deferred {
                    self.write("defer ");
                }
                self.write("* as ");
                self.emit_node(bindings.name);
                self.write(" from ");
                self.emit_node(import.module_specifier);
                self.emit_declaration_import_attributes(import.attributes);
                self.write(";");
                self.write_line();
                emitted = true;
            } else {
                for &spec_idx in &bindings.elements.nodes {
                    if !self.should_emit_import_specifier(spec_idx) {
                        continue;
                    }
                    self.write_indent();
                    self.write("import ");
                    if clause.is_type_only {
                        self.write("type ");
                    }
                    if clause.is_deferred {
                        self.write("defer ");
                    }
                    self.write("{ ");
                    self.emit_specifier(spec_idx, !clause.is_type_only);
                    self.write(" } from ");
                    self.emit_node(import.module_specifier);
                    self.emit_declaration_import_attributes(import.attributes);
                    self.write(";");
                    self.write_line();
                    emitted = true;
                }
            }
        }

        emitted
    }

    pub(crate) fn emit_import_declaration(&mut self, import_idx: NodeIndex) {
        let Some(import_node) = self.arena.get(import_idx) else {
            return;
        };
        let Some(import) = self.arena.get_import_decl(import_node) else {
            return;
        };

        // Side-effect imports (no clause) are always emitted
        if import.import_clause.is_none() {
            self.write_indent();
            self.write("import ");
            self.emit_node(import.module_specifier);
            self.write(";");
            self.write_line();
            return;
        }

        if self.import_declaration_is_local_json_value_import(import) {
            return;
        }

        // Check if we should elide this import based on usage
        let (default_used, named_used) = self.count_used_imports(import);
        if default_used == 0 && named_used == 0 {
            // All bindings are unused -- but if the imported module contains
            // module augmentations we must preserve the import as a bare
            // side-effect import so the augmentations take effect at runtime.
            // This matches tsc's `isImportRequiredByAugmentation` behaviour.
            if self.is_import_required_by_augmentation(import.module_specifier) {
                self.write_indent();
                self.write("import ");
                self.emit_node(import.module_specifier);
                self.write(";");
                self.write_line();
                return;
            }
            // No used symbols and no augmentation dependency - elide it
            return;
        }

        // Emit the import with filtering
        self.write_indent();
        self.write("import ");

        if let Some(clause_node) = self.arena.get(import.import_clause)
            && let Some(clause) = self.arena.get_import_clause(clause_node)
        {
            if clause.is_type_only {
                self.write("type ");
            }
            if clause.is_deferred {
                self.write("defer ");
            }

            let mut has_default = false;

            // Default import (only if used)
            if clause.name.is_some() && default_used > 0 {
                self.emit_node(clause.name);
                has_default = true;
            }

            // Named imports (filter to used ones)
            if clause.named_bindings.is_some() && named_used > 0 {
                if has_default {
                    self.write(", ");
                }
                self.emit_named_imports_filtered(clause.named_bindings, !clause.is_type_only);
            }

            self.write(" from ");
        }

        self.emit_node(import.module_specifier);
        self.emit_declaration_import_attributes(import.attributes);
        self.write(";");
        self.write_line();
    }

    fn import_declaration_is_local_json_value_import(
        &self,
        import: &tsz_parser::parser::node::ImportDeclData,
    ) -> bool {
        let Some(module_node) = self.arena.get(import.module_specifier) else {
            return false;
        };
        if !self
            .arena
            .get_literal(module_node)
            .is_some_and(|literal| literal.text.ends_with(".json"))
        {
            return false;
        }
        let Some(clause_node) = self.arena.get(import.import_clause) else {
            return false;
        };
        let Some(clause) = self.arena.get_import_clause(clause_node) else {
            return false;
        };
        if clause.is_type_only {
            return false;
        }

        let mut imported_names = Vec::new();
        if let Some(name) = self.get_identifier_text(clause.name) {
            imported_names.push(name);
        }

        if let Some(named_bindings_node) = self.arena.get(clause.named_bindings)
            && let Some(named_bindings) = self.arena.get_named_imports(named_bindings_node)
        {
            if named_bindings.name.is_some() && named_bindings.elements.nodes.is_empty() {
                if let Some(name) = self.get_identifier_text(named_bindings.name) {
                    imported_names.push(name);
                }
            } else if !named_bindings.elements.nodes.is_empty() {
                return false;
            }
        }

        !imported_names.is_empty()
            && imported_names.iter().all(|name| {
                !self.public_api_type_surface_contains_typeof_name(name)
                    && !self.public_api_export_specifier_exports_name(name)
            })
    }

    /// Emit named imports, filtering out unused specifiers.
    ///
    /// This version only emits import specifiers that are in the `used_symbols` set.
    pub(crate) fn emit_named_imports_filtered(
        &mut self,
        imports_idx: NodeIndex,
        allow_type_prefix: bool,
    ) {
        let Some(imports_node) = self.arena.get(imports_idx) else {
            return;
        };
        let Some(imports) = self.arena.get_named_imports(imports_node) else {
            return;
        };

        // Handle namespace imports (* as ns)
        if imports.name.is_some() && imports.elements.nodes.is_empty() {
            // Check if namespace is used
            if self.should_emit_import_specifier(imports.name) {
                self.write("* as ");
                self.emit_node(imports.name);
            }
            return;
        }

        // Filter individual specifiers
        self.write("{ ");
        let mut first = true;
        for &spec_idx in &imports.elements.nodes {
            // Only emit if the specifier is used
            if !self.should_emit_import_specifier(spec_idx) {
                continue;
            }

            if !first {
                self.write(", ");
            }
            first = false;
            self.emit_specifier(spec_idx, allow_type_prefix);
        }
        self.write(" }");
    }

    /// Check whether an import must be preserved as a side-effect import because
    /// the target module contains module augmentations.
    fn is_import_required_by_augmentation(&self, module_specifier_idx: NodeIndex) -> bool {
        if self.files_with_augmentations.is_empty() {
            return false;
        }
        let spec_text = self
            .arena
            .get(module_specifier_idx)
            .and_then(|n| self.arena.get_literal(n))
            .map(|lit| lit.text.as_str());
        let Some(spec) = spec_text else {
            return false;
        };
        if !spec.starts_with('.') {
            return false;
        }
        let Some(ref current_path) = self.current_file_path else {
            return false;
        };
        let current_dir = std::path::Path::new(current_path.as_str())
            .parent()
            .unwrap_or(std::path::Path::new(""));
        // Normalize path to remove `.` and `..` segments without requiring
        // the path to exist on disk (unlike std::fs::canonicalize).
        let joined = current_dir.join(spec);
        let mut parts = Vec::new();
        for component in joined.components() {
            match component {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    parts.pop();
                }
                other => parts.push(other),
            }
        }
        let candidate_base: std::path::PathBuf = parts.iter().collect();
        for ext in &[".ts", ".tsx", ".mts", ".cts", ".d.ts", ".js", ".jsx"] {
            let candidate = format!("{}{}", candidate_base.display(), ext);
            if self.files_with_augmentations.contains(&candidate) {
                return true;
            }
        }
        let as_is = candidate_base.display().to_string();
        self.files_with_augmentations.contains(&as_is)
    }

    pub(crate) fn emit_module_declaration(&mut self, module_idx: NodeIndex) {
        self.emit_module_declaration_with_export(module_idx, false);
    }

    pub(super) fn emit_module_declaration_with_export(
        &mut self,
        module_idx: NodeIndex,
        already_exported: bool,
    ) {
        let Some(module_node) = self.arena.get(module_idx) else {
            return;
        };
        let Some(module) = self.arena.get_module(module_node) else {
            return;
        };

        let is_exported = already_exported
            || self
                .arena
                .has_modifier(&module.modifiers, SyntaxKind::ExportKeyword);

        // In module files (public API filter active), non-exported ambient
        // `declare namespace X { ... }` declarations are global augmentations
        // and must NOT be re-emitted in the .d.ts — tsc strips them because
        // they don't contribute to the module's type surface.
        // Only identifier-named namespaces are affected; string-literal
        // modules (`declare module "foo"`) and `declare global` are handled
        // separately inside `should_emit_public_api_module`.
        if !is_exported
            && self.public_api_filter_enabled()
            && self.arena.is_declare(&module.modifiers)
        {
            let is_identifier_namespace = self
                .arena
                .get(module.name)
                .is_some_and(|n| n.kind != SyntaxKind::StringLiteral as u16);
            let is_global = self
                .arena
                .get(module.name)
                .and_then(|n| self.arena.get_identifier(n))
                .is_some_and(|ident| ident.escaped_text == "global");
            let referenced_by_export_equals = self
                .current_source_file_idx
                .and_then(|source_idx| self.arena.get(source_idx))
                .and_then(|source_node| self.arena.get_source_file(source_node))
                .map(|source_file| self.source_file_export_equals_names(source_file))
                .and_then(|names| {
                    self.get_identifier_text(module.name)
                        .map(|name| names.contains(&name))
                })
                .unwrap_or(false);
            if is_identifier_namespace && !is_global && !referenced_by_export_equals {
                return;
            }
        }

        if !self.should_emit_public_api_module(is_exported, module.name) {
            return;
        }

        // Elide non-exported, non-declare inner namespaces nested inside a
        // non-ambient namespace unless they contribute to the exported type
        // surface. tsc omits these hidden source-level namespaces from .d.ts
        // output, even when they contain declarations.
        //
        // Keep the namespace when:
        //   - it's exported
        //   - the source carried `declare`
        //   - we're inside an ambient/`declare` namespace (ambient contexts
        //     preserve structure — e.g. declarationEmitLocalClassHasRequiredDeclare)
        //   - it's referenced by an exported import alias (e.g.
        //     aliasInaccessibleModule: `export import X = N` keeps `namespace N`)
        if !is_exported
            && self.inside_non_ambient_namespace
            && self.arena.is_declare(&module.modifiers)
            && self
                .arena
                .get(module.name)
                .is_some_and(|n| n.kind == SyntaxKind::StringLiteral as u16)
        {
            return;
        }

        if !is_exported
            && !self.arena.is_declare(&module.modifiers)
            && self.inside_non_ambient_namespace
        {
            if self.is_module_body_effectively_empty(module.body)
                && !self.is_empty_namespace_referenced_by_export_import_alias(module_idx)
            {
                return;
            }
            let referenced_by_export_surface = self.is_ns_member_used_by_exports(module_idx)
                || self.is_empty_namespace_referenced_by_export_import_alias(module_idx);
            if !referenced_by_export_surface {
                return;
            }
        }

        self.write_indent();
        if !self.inside_declare_namespace {
            if is_exported {
                self.write("export ");
            }
            if self.should_emit_declare_keyword(is_exported) {
                self.write("declare ");
            }
        } else if is_exported && self.should_emit_export_keyword() {
            self.write("export ");
        }

        // Determine keyword: "module" for string literals, "global" for
        // the `declare global` augmentation, "namespace" for other identifiers.
        let name_node = self.arena.get(module.name);
        let use_module_keyword =
            name_node.is_some_and(|n| n.kind == SyntaxKind::StringLiteral as u16);
        let is_global_augmentation = name_node
            .and_then(|n| self.arena.get_identifier(n))
            .is_some_and(|ident| ident.escaped_text == "global");

        if is_global_augmentation {
            // `declare global { ... }` — emit just "global" without
            // a module/namespace keyword prefix.
            self.write("global");
        } else {
            self.write(if use_module_keyword {
                "module "
            } else {
                "namespace "
            });
            self.emit_node(module.name);
        }

        // Collect dotted namespace name segments: namespace A.B.C { ... }
        // is represented as a chain of ModuleDeclaration nodes
        let mut current_body = module.body;
        let mut innermost_ns_idx = module_idx;
        loop {
            if !current_body.is_some() {
                break;
            }
            let Some(body_node) = self.arena.get(current_body) else {
                break;
            };
            if let Some(nested_mod) = self.arena.get_module(body_node) {
                // Body is another module declaration — emit dotted name
                self.write(".");
                self.emit_node(nested_mod.name);
                innermost_ns_idx = current_body;
                current_body = nested_mod.body;
            } else {
                break;
            }
        }

        if current_body.is_some() {
            // Check if the body is an empty block — tsc emits `namespace X { }` on one line
            let is_empty_body = self
                .arena
                .get(current_body)
                .and_then(|body_node| self.arena.get_module_block(body_node))
                .is_none_or(|module_block| {
                    module_block
                        .statements
                        .as_ref()
                        .is_none_or(|stmts| stmts.nodes.is_empty())
                });

            if is_empty_body {
                // tsc uses single-line `{ }` for empty namespaces nested inside
                // another declare namespace, but multi-line `{\n}` for top-level.
                if self.inside_declare_namespace && !use_module_keyword {
                    self.write(" { }");
                    self.write_line();
                } else {
                    self.write(" {");
                    self.write_line();
                    self.write_indent();
                    self.write("}");
                    self.write_line();
                }
                return;
            }

            self.write(" {");
            self.write_line();
            self.increase_indent();

            // Inside a declare namespace, don't emit 'declare' keyword for members
            let prev_inside_declare_namespace = self.inside_declare_namespace;
            self.inside_declare_namespace = true;
            let prev_ambient_module_specifier = self.current_ambient_module_specifier.clone();
            if use_module_keyword
                && let Some(specifier) = string_literal_text(self.arena, module.name)
            {
                self.current_ambient_module_specifier = Some(specifier);
            }
            // Track innermost namespace symbol for context-relative type names
            let prev_enclosing_ns = self.enclosing_namespace_symbol;
            if let Some(binder) = self.binder
                && let Some(ns_sym) = binder.get_node_symbol(innermost_ns_idx)
            {
                self.enclosing_namespace_symbol = Some(ns_sym);
            }
            let prev_public_api_scope_depth = self.public_api_scope_depth;
            let prev_inside_non_ambient_namespace = self.inside_non_ambient_namespace;
            // In declare/ambient namespaces, all members are implicitly public,
            // so disable the API filter (increment depth).
            // In non-declare namespaces, members must have `export` to be public.
            // A namespace is ambient if it has `declare`, or if the source
            // is a .d.ts file, or if it's nested inside an ambient namespace
            // (but NOT if it's nested inside a non-ambient namespace).
            let is_ambient_ns = self.arena.is_declare(&module.modifiers)
                || self.source_is_declaration_file
                || (prev_inside_declare_namespace && !prev_inside_non_ambient_namespace);
            if is_ambient_ns {
                self.public_api_scope_depth += 1;
                self.inside_non_ambient_namespace = false;
            } else {
                self.inside_non_ambient_namespace = true;
            }

            if let Some(body_node) = self.arena.get(current_body)
                && let Some(module_block) = self.arena.get_module_block(body_node)
                && let Some(ref stmts) = module_block.statements
            {
                // Save emission-tracking flags for this namespace scope.
                // `emitted_module_indicator` must also be saved/restored so
                // that `export` keywords on members inside an ambient module
                // augmentation (`declare module "foo" { export function f(); }`)
                // do not leak into the file-level flag and suppress the
                // top-level `export {};` marker.
                let prev_emitted_non_exported = self.emitted_non_exported_declaration;
                let prev_emitted_scope_marker = self.emitted_scope_marker;
                let prev_emitted_module_indicator = self.emitted_module_indicator;
                let prev_function_names_with_overloads = self.function_names_with_overloads.clone();
                self.emitted_non_exported_declaration = false;
                self.emitted_scope_marker = false;
                self.function_names_with_overloads.clear();

                // Pre-scan to check if the body has a mix of exported and
                // non-exported members. When it does, tsc preserves `export`
                // keywords on individual members; otherwise it strips them.
                // Applies to both ambient string-named modules and non-ambient namespaces.
                let prev_ambient_scope_marker = self.ambient_module_has_scope_marker;
                if !is_ambient_ns || use_module_keyword {
                    self.ambient_module_has_scope_marker =
                        self.module_body_has_scope_marker(stmts, !is_ambient_ns);
                }

                let prev_self_import_alias = self.current_namespace_self_import_alias.clone();
                let prev_self_export_names = self.current_namespace_self_export_names.clone();
                let prev_shadowed_default_name =
                    self.current_namespace_shadowed_default_name.clone();
                if let Some((alias, default_name, export_names)) =
                    self.shadowed_default_self_import_context(stmts)
                {
                    self.current_namespace_self_import_alias = Some(alias);
                    self.current_namespace_shadowed_default_name = Some(default_name);
                    self.current_namespace_self_export_names = export_names;
                }

                let scoped_js_named_exports = self.js_named_export_names_for_module_body(stmts);
                let scoped_js_named_export_targets =
                    self.js_named_export_targets_for_module_body(stmts);
                let prev_js_named_export_names = if scoped_js_named_exports.is_empty() {
                    None
                } else {
                    let previous = self.js_named_export_names.clone();
                    self.js_named_export_names.extend(scoped_js_named_exports);
                    Some(previous)
                };

                let should_emit_body_statements =
                    is_ambient_ns || self.module_body_has_exported_member(stmts);
                if should_emit_body_statements {
                    for &stmt_idx in &stmts.nodes {
                        if scoped_js_named_export_targets
                            .iter()
                            .any(|(_, targets)| targets.contains(&stmt_idx))
                        {
                            if let Some(stmt_node) = self.arena.get(stmt_idx) {
                                self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
                            }
                            continue;
                        }
                        self.emit_statement(stmt_idx);
                        if let Some((_, targets)) = scoped_js_named_export_targets
                            .iter()
                            .find(|(export_idx, _)| *export_idx == stmt_idx)
                        {
                            for &target_idx in targets {
                                self.emit_statement_with_options(target_idx, true);
                            }
                        }
                    }
                }

                if let Some(previous) = prev_js_named_export_names {
                    self.js_named_export_names = previous;
                }

                // tsc emits `export {};` inside a non-ambient namespace
                // body when there is a mix of exported and non-exported
                // members (the "scope-fix marker").
                // Use emission-time tracking instead of source analysis.
                let is_ambient_module = self.arena.is_declare(&module.modifiers)
                    || self.source_is_declaration_file
                    || (prev_inside_declare_namespace && !prev_inside_non_ambient_namespace);

                if !is_ambient_module
                    && self.emitted_non_exported_declaration
                    && !self.emitted_scope_marker
                {
                    self.write_indent();
                    self.write("export {};");
                    self.write_line();
                }

                // Restore tracking flags
                self.emitted_non_exported_declaration = prev_emitted_non_exported;
                self.emitted_scope_marker = prev_emitted_scope_marker;
                self.emitted_module_indicator = prev_emitted_module_indicator;
                self.ambient_module_has_scope_marker = prev_ambient_scope_marker;
                self.function_names_with_overloads = prev_function_names_with_overloads;
                self.current_namespace_self_import_alias = prev_self_import_alias;
                self.current_namespace_self_export_names = prev_self_export_names;
                self.current_namespace_shadowed_default_name = prev_shadowed_default_name;
            }

            self.public_api_scope_depth = prev_public_api_scope_depth;
            self.inside_non_ambient_namespace = prev_inside_non_ambient_namespace;
            self.inside_declare_namespace = prev_inside_declare_namespace;
            self.current_ambient_module_specifier = prev_ambient_module_specifier;
            self.enclosing_namespace_symbol = prev_enclosing_ns;
            self.decrease_indent();
            self.write_indent();
            self.write("}");
        } else {
            // Shorthand ambient module: declare module "foo";
            self.write(";");
        }

        self.write_line();
    }

    pub(in crate::declaration_emitter) fn namespace_import_needed_for_shadowed_self_type(
        &self,
        alias_idx: NodeIndex,
        module_specifier_idx: NodeIndex,
    ) -> bool {
        let Some(alias) = self.get_identifier_text(alias_idx) else {
            return false;
        };
        if !self.import_specifier_targets_current_file(module_specifier_idx) {
            return false;
        }
        let Some(default_name) = self.default_exported_local_name() else {
            return false;
        };
        self.source_has_namespace_shadowing_name(&default_name)
            && self.self_namespace_import_alias().as_deref() == Some(alias.as_str())
    }

    fn shadowed_default_self_import_context(
        &self,
        stmts: &NodeList,
    ) -> Option<(String, String, FxHashSet<String>)> {
        let alias = self.self_namespace_import_alias()?;
        let default_name = self.default_exported_local_name()?;
        if !self.namespace_body_shadows_name(stmts, &default_name) {
            return None;
        }
        let mut export_names = self.top_level_self_exported_names();
        export_names.insert(default_name.clone());
        Some((alias, default_name, export_names))
    }

    pub(in crate::declaration_emitter) fn self_namespace_import_alias(&self) -> Option<String> {
        let source_file = self.current_source_file()?;
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let Some(import) = self.arena.get_import_decl(stmt_node) else {
                continue;
            };
            if !self.import_specifier_targets_current_file(import.module_specifier) {
                continue;
            }
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
            if bindings.name.is_some() && bindings.elements.nodes.is_empty() {
                return self.get_identifier_text(bindings.name);
            }
        }
        None
    }

    fn import_specifier_targets_current_file(&self, module_specifier_idx: NodeIndex) -> bool {
        let Some(current_path) = self.current_file_path.as_deref() else {
            return false;
        };
        let Some(spec) = self
            .arena
            .get(module_specifier_idx)
            .and_then(|node| self.arena.get_literal(node))
            .map(|lit| lit.text.as_str())
        else {
            return false;
        };
        if !spec.starts_with('.') {
            return false;
        }

        let current = std::path::Path::new(current_path);
        let current_dir = current.parent().unwrap_or(std::path::Path::new(""));
        let joined = current_dir.join(spec);
        let normalized = Self::normalize_path_text(&joined);
        let current_no_ext = self.strip_ts_extensions(current_path);
        let normalized_no_ext = self.strip_ts_extensions(&normalized);
        if normalized_no_ext == current_no_ext {
            return true;
        }

        let spec_stem = std::path::Path::new(spec)
            .file_stem()
            .and_then(|stem| stem.to_str());
        let current_stem = std::path::Path::new(current_path)
            .file_stem()
            .and_then(|stem| stem.to_str());
        spec_stem.is_some() && spec_stem == current_stem
    }

    fn normalize_path_text(path: &std::path::Path) -> String {
        let mut parts = Vec::new();
        for component in path.components() {
            match component {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    parts.pop();
                }
                other => parts.push(other.as_os_str().to_string_lossy().into_owned()),
            }
        }
        parts.join("/")
    }

    pub(in crate::declaration_emitter) fn default_exported_local_name(&self) -> Option<String> {
        let source_file = self.current_source_file()?;
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if let Some(export) = self.arena.get_export_decl(stmt_node)
                && export.is_default_export
                && export.export_clause.is_some()
            {
                let mut names = FxHashSet::default();
                self.collect_declaration_names(export.export_clause, &mut names);
                if let Some(name) = names.into_iter().next() {
                    return Some(name);
                }
            }
            if let Some(class) = self.arena.get_class(stmt_node)
                && self
                    .arena
                    .has_modifier(&class.modifiers, SyntaxKind::ExportKeyword)
                && self
                    .arena
                    .has_modifier(&class.modifiers, SyntaxKind::DefaultKeyword)
            {
                return self.get_identifier_text(class.name);
            }
            if let Some(func) = self.arena.get_function(stmt_node)
                && self
                    .arena
                    .has_modifier(&func.modifiers, SyntaxKind::ExportKeyword)
                && self
                    .arena
                    .has_modifier(&func.modifiers, SyntaxKind::DefaultKeyword)
            {
                return self.get_identifier_text(func.name);
            }
        }
        None
    }

    pub(in crate::declaration_emitter) fn top_level_self_exported_names(
        &self,
    ) -> FxHashSet<String> {
        let mut names = FxHashSet::default();
        let Some(source_file) = self.current_source_file() else {
            return names;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if let Some(export) = self.arena.get_export_decl(stmt_node)
                && export.export_clause.is_some()
                && export.module_specifier.is_none()
            {
                if export.is_default_export
                    || self.arena.get(export.export_clause).is_some_and(|node| {
                        matches!(
                            node.kind,
                            k if k == syntax_kind_ext::INTERFACE_DECLARATION
                                || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                                || k == syntax_kind_ext::CLASS_DECLARATION
                                || k == syntax_kind_ext::FUNCTION_DECLARATION
                                || k == syntax_kind_ext::ENUM_DECLARATION
                                || k == syntax_kind_ext::VARIABLE_STATEMENT
                        )
                    })
                {
                    self.collect_declaration_names(export.export_clause, &mut names);
                }
                continue;
            }
            if !self.stmt_has_export_modifier(stmt_node) {
                continue;
            }
            self.collect_declaration_names(stmt_idx, &mut names);
        }
        names
    }

    pub(in crate::declaration_emitter) fn source_has_namespace_shadowing_name(
        &self,
        name: &str,
    ) -> bool {
        let Some(source_file) = self.current_source_file() else {
            return false;
        };
        source_file
            .statements
            .nodes
            .iter()
            .copied()
            .any(|stmt_idx| {
                let module_idx = self
                    .arena
                    .get(stmt_idx)
                    .and_then(|node| self.arena.get_module(node).map(|_| stmt_idx))
                    .or_else(|| {
                        let node = self.arena.get(stmt_idx)?;
                        let export = self.arena.get_export_decl(node)?;
                        self.arena
                            .get(export.export_clause)
                            .and_then(|clause| self.arena.get_module(clause))
                            .map(|_| export.export_clause)
                    });
                module_idx
                    .and_then(|idx| self.arena.get(idx))
                    .and_then(|node| self.arena.get_module(node))
                    .and_then(|module| self.arena.get(module.body))
                    .and_then(|body_node| self.arena.get_module_block(body_node))
                    .and_then(|block| {
                        self.namespace_body_shadows_name(block.statements.as_ref()?, name)
                            .then_some(())
                    })
                    .is_some()
            })
    }

    fn namespace_body_shadows_name(&self, stmts: &NodeList, name: &str) -> bool {
        stmts.nodes.iter().copied().any(|stmt_idx| {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                return false;
            };
            if self.stmt_has_export_modifier(stmt_node) {
                return false;
            }
            let mut names = FxHashSet::default();
            self.collect_declaration_names(stmt_idx, &mut names);
            names.contains(name)
        })
    }

    fn collect_declaration_names(&self, stmt_idx: NodeIndex, names: &mut FxHashSet<String>) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        if let Some(export) = self.arena.get_export_decl(stmt_node) {
            if export.export_clause.is_some() {
                self.collect_declaration_names(export.export_clause, names);
            }
            return;
        }
        if let Some(class) = self.arena.get_class(stmt_node) {
            if let Some(name) = self.get_identifier_text(class.name) {
                names.insert(name);
            }
            return;
        }
        if let Some(func) = self.arena.get_function(stmt_node) {
            if let Some(name) = self.get_identifier_text(func.name) {
                names.insert(name);
            }
            return;
        }
        if let Some(iface) = self.arena.get_interface(stmt_node) {
            if let Some(name) = self.get_identifier_text(iface.name) {
                names.insert(name);
            }
            return;
        }
        if let Some(alias) = self.arena.get_type_alias(stmt_node) {
            if let Some(name) = self.get_identifier_text(alias.name) {
                names.insert(name);
            }
            return;
        }
        if let Some(enum_data) = self.arena.get_enum(stmt_node) {
            if let Some(name) = self.get_identifier_text(enum_data.name) {
                names.insert(name);
            }
            return;
        }
        if let Some(var_stmt) = self.arena.get_variable(stmt_node) {
            for &decl_list_idx in &var_stmt.declarations.nodes {
                if let Some(decl_list_node) = self.arena.get(decl_list_idx)
                    && let Some(decl) = self.arena.get_variable_declaration(decl_list_node)
                    && let Some(name) = self.get_identifier_text(decl.name)
                {
                    names.insert(name);
                } else if let Some(decl_list_node) = self.arena.get(decl_list_idx)
                    && let Some(decl_list) = self.arena.get_variable(decl_list_node)
                {
                    for &decl_idx in &decl_list.declarations.nodes {
                        if let Some(decl_node) = self.arena.get(decl_idx)
                            && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                            && let Some(name) = self.get_identifier_text(decl.name)
                        {
                            names.insert(name);
                        }
                    }
                }
            }
        }
    }

    fn current_source_file(&self) -> Option<&tsz_parser::parser::node::SourceFileData> {
        self.arena
            .get(self.current_source_file_idx?)
            .and_then(|node| self.arena.get_source_file(node))
    }

    /// True when a module body walks down to an empty block (or is missing
    /// entirely). Dotted name chains like `namespace A.B.C { }` recurse
    /// through nested `ModuleDeclaration` bodies until the inner
    /// `ModuleBlock` is reached; every level must be empty.
    pub(super) fn is_module_body_effectively_empty(&self, body_idx: NodeIndex) -> bool {
        if !body_idx.is_some() {
            return true;
        }
        let Some(body_node) = self.arena.get(body_idx) else {
            return true;
        };
        if let Some(nested) = self.arena.get_module(body_node) {
            return self.is_module_body_effectively_empty(nested.body);
        }
        self.arena
            .get_module_block(body_node)
            .is_none_or(|block| block.statements.as_ref().is_none_or(|s| s.nodes.is_empty()))
    }

    pub(super) fn is_empty_namespace_referenced_by_export_import_alias(
        &self,
        module_idx: NodeIndex,
    ) -> bool {
        let Some(module_node) = self.arena.get(module_idx) else {
            return false;
        };
        let Some(module) = self.arena.get_module(module_node) else {
            return false;
        };
        let module_name = self.get_identifier_text(module.name);
        let module_symbol = self.binder.and_then(|binder| {
            binder
                .get_node_symbol(module_idx)
                .or_else(|| binder.get_node_symbol(module.name))
        });
        let Some(source_file) = self
            .current_source_file_idx
            .and_then(|source_idx| self.arena.get(source_idx))
            .and_then(|source_node| self.arena.get_source_file(source_node))
        else {
            return false;
        };

        self.statements_contain_export_import_alias_to_namespace(
            &source_file.statements,
            module_symbol,
            module_name.as_deref(),
        )
    }

    fn statements_contain_export_import_alias_to_namespace(
        &self,
        statements: &NodeList,
        module_symbol: Option<tsz_binder::SymbolId>,
        module_name: Option<&str>,
    ) -> bool {
        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                && let Some(export) = self.arena.get_export_decl(stmt_node)
                && let Some(clause_node) = self.arena.get(export.export_clause)
                && clause_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                if self.import_equals_references_namespace(
                    export.export_clause,
                    module_symbol,
                    module_name,
                ) {
                    return true;
                }
                if let Some(import) = self.arena.get_import_decl(clause_node)
                    && let Some(alias_name) =
                        self.get_identifier_text(self.get_rightmost_name(import.module_specifier))
                    && self.statements_contain_import_alias_to_namespace(
                        statements,
                        &alias_name,
                        module_symbol,
                        module_name,
                    )
                {
                    return true;
                }
            }

            if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module) = self.arena.get_module(stmt_node)
                && self.module_body_contains_export_import_alias_to_namespace(
                    module.body,
                    module_symbol,
                    module_name,
                )
            {
                return true;
            }
        }
        false
    }

    fn statements_contain_import_alias_to_namespace(
        &self,
        statements: &NodeList,
        alias_name: &str,
        module_symbol: Option<tsz_binder::SymbolId>,
        module_name: Option<&str>,
    ) -> bool {
        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                && let Some(import) = self.arena.get_import_decl(stmt_node)
                && self.get_identifier_text(import.import_clause).as_deref() == Some(alias_name)
                && self.import_equals_references_namespace(stmt_idx, module_symbol, module_name)
            {
                return true;
            }
        }
        false
    }

    fn module_body_contains_export_import_alias_to_namespace(
        &self,
        body_idx: NodeIndex,
        module_symbol: Option<tsz_binder::SymbolId>,
        module_name: Option<&str>,
    ) -> bool {
        let Some(body_node) = self.arena.get(body_idx) else {
            return false;
        };
        if let Some(nested) = self.arena.get_module(body_node) {
            return self.module_body_contains_export_import_alias_to_namespace(
                nested.body,
                module_symbol,
                module_name,
            );
        }
        self.arena
            .get_module_block(body_node)
            .and_then(|block| block.statements.as_ref())
            .is_some_and(|statements| {
                self.statements_contain_export_import_alias_to_namespace(
                    statements,
                    module_symbol,
                    module_name,
                )
            })
    }

    fn import_equals_references_namespace(
        &self,
        import_idx: NodeIndex,
        module_symbol: Option<tsz_binder::SymbolId>,
        module_name: Option<&str>,
    ) -> bool {
        let Some(import_node) = self.arena.get(import_idx) else {
            return false;
        };
        let Some(import) = self.arena.get_import_decl(import_node) else {
            return false;
        };
        if !import.module_specifier.is_some() {
            return false;
        }

        if let Some(module_symbol) = module_symbol
            && let Some(binder) = self.binder
        {
            let rightmost = self.get_rightmost_name(import.module_specifier);
            if binder.get_node_symbol(import.module_specifier) == Some(module_symbol)
                || binder.get_node_symbol(rightmost) == Some(module_symbol)
            {
                return true;
            }
        }

        module_name.is_some_and(|name| {
            self.get_identifier_text(self.get_rightmost_name(import.module_specifier))
                .is_some_and(|reference_name| reference_name == name)
        })
    }

    fn js_named_export_names_for_module_body(&self, stmts: &NodeList) -> Vec<String> {
        if !self.source_is_js_file {
            return Vec::new();
        }

        let mut names = Vec::new();
        for &stmt_idx in &stmts.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export) = self.arena.get_export_decl(stmt_node) else {
                continue;
            };
            if export.module_specifier.is_some() || export.export_clause.is_none() {
                continue;
            }
            let Some(clause_node) = self.arena.get(export.export_clause) else {
                continue;
            };
            if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
                continue;
            }
            let Some(named) = self.arena.get_named_imports(clause_node) else {
                continue;
            };
            if named.name.is_some() {
                continue;
            }

            for &spec_idx in &named.elements.nodes {
                let Some(spec_node) = self.arena.get(spec_idx) else {
                    continue;
                };
                let Some(spec) = self.arena.get_specifier(spec_node) else {
                    continue;
                };
                if spec.property_name.is_some() {
                    continue;
                }
                let Some(name_node) = self.arena.get(spec.name) else {
                    continue;
                };
                let Some(name_ident) = self.arena.get_identifier(name_node) else {
                    continue;
                };
                if !names.iter().any(|name| name == &name_ident.escaped_text) {
                    names.push(name_ident.escaped_text.clone());
                }
            }
        }
        names
    }

    fn js_named_export_targets_for_module_body(
        &self,
        stmts: &NodeList,
    ) -> Vec<(NodeIndex, Vec<NodeIndex>)> {
        if !self.source_is_js_file {
            return Vec::new();
        }

        let mut declarations = Vec::<(String, NodeIndex)>::new();
        for &stmt_idx in &stmts.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                continue;
            }
            let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
                continue;
            };

            let mut names = Vec::new();
            for &decl_list_idx in &var_stmt.declarations.nodes {
                let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                    names.clear();
                    break;
                };
                let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                    names.clear();
                    break;
                };
                for &decl_idx in &decl_list.declarations.nodes {
                    let Some(decl_node) = self.arena.get(decl_idx) else {
                        names.clear();
                        break;
                    };
                    let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                        names.clear();
                        break;
                    };
                    let Some(name) = self.get_identifier_text(decl.name) else {
                        names.clear();
                        break;
                    };
                    names.push(name);
                }
            }
            if names.len() == 1 {
                declarations.push((names.remove(0), stmt_idx));
            }
        }

        let mut exports = Vec::new();
        for &stmt_idx in &stmts.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export) = self.arena.get_export_decl(stmt_node) else {
                continue;
            };
            if export.module_specifier.is_some() || export.export_clause.is_none() {
                continue;
            }
            let Some(clause_node) = self.arena.get(export.export_clause) else {
                continue;
            };
            if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
                continue;
            }
            let Some(named) = self.arena.get_named_imports(clause_node) else {
                continue;
            };
            if named.name.is_some() {
                continue;
            }

            let mut targets = Vec::new();
            for &spec_idx in &named.elements.nodes {
                let Some(spec_node) = self.arena.get(spec_idx) else {
                    continue;
                };
                let Some(spec) = self.arena.get_specifier(spec_node) else {
                    continue;
                };
                if spec.property_name.is_some() {
                    continue;
                }
                let Some(name) = self.get_identifier_text(spec.name) else {
                    continue;
                };
                if let Some((_, target_idx)) = declarations
                    .iter()
                    .find(|(decl_name, _)| decl_name == &name)
                    && !targets.contains(target_idx)
                {
                    targets.push(*target_idx);
                }
            }
            if !targets.is_empty() {
                exports.push((stmt_idx, targets));
            }
        }
        exports
    }

    pub(crate) fn emit_import_equals_declaration(
        &mut self,
        import_idx: NodeIndex,
        already_exported: bool,
    ) {
        let Some(import_node) = self.arena.get(import_idx) else {
            return;
        };
        let Some(import_eq) = self.arena.get_import_decl(import_node) else {
            return;
        };

        let is_exported = self
            .arena
            .has_modifier(&import_eq.modifiers, SyntaxKind::ExportKeyword);
        let is_public_exported = is_exported && !already_exported;

        // Elide non-exported import equals declarations that are not used by the public API
        if !is_exported && !already_exported {
            let is_require_import = self
                .arena
                .get(import_eq.module_specifier)
                .is_some_and(|n| n.kind == SyntaxKind::StringLiteral as u16);

            if self.current_ambient_module_specifier.is_some() {
                if self.used_symbols.is_some() && !self.is_ns_member_used_by_exports(import_idx) {
                    return;
                }
            } else if self.inside_non_ambient_namespace {
                if is_require_import {
                    return;
                }
                // Inside a non-ambient namespace: if usage analysis is available, check
                // if the alias is referenced by an exported/emitted member.
                // If usage analysis is not available, fall through to the type-entity
                // check below (which will elide value-only targets).
                if self.used_symbols.is_some() && !self.is_ns_member_used_by_exports(import_idx) {
                    return;
                }
            } else {
                // Outside a namespace: apply standard elision heuristics.

                // When no usage tracking is available, non-exported `import = require(...)`
                // declarations are almost always value-level and not needed in .d.ts output.
                if self.used_symbols.is_none() {
                    return;
                }

                if !self.should_emit_public_api_dependency(import_eq.import_clause) {
                    return;
                }
            }

            // For namespace-path imports (import x = a.b, not import x = require("...")),
            // tsc only preserves them in .d.ts if the alias targets a type-level entity
            // (class, interface, enum, namespace, type alias, function). If the target is
            // a value-only entity (e.g., a variable), the emitted type resolves directly
            // to the underlying type (e.g., `number`) without needing the alias.
            if !is_require_import
                && !self.import_alias_targets_type_entity(import_eq.module_specifier)
            {
                return;
            }
        }

        // Only write indent if not already exported (caller handles indent for exported case)
        if !already_exported {
            self.write_indent();
        }
        if is_public_exported {
            self.write("export ");
        }
        if import_eq.is_type_only {
            self.write("import type ");
        } else {
            self.write("import ");
        }

        // Emit variable name from import_clause
        if import_eq.import_clause.is_some() {
            self.emit_node(import_eq.import_clause);
        }

        // Emit " = require(...)"
        if let Some(module_node) = self.arena.get(import_eq.module_specifier) {
            if module_node.kind == SyntaxKind::StringLiteral as u16 {
                self.write(" = require(");
                self.emit_node(import_eq.module_specifier);
                self.write(")");
            } else {
                self.write(" = ");
                self.emit_node(import_eq.module_specifier);
            }
        } else {
            self.write(" = ");
        }

        self.write(";");
        self.write_line();
    }

    pub(super) fn emit_import_equals_declaration_without_export(&mut self, import_idx: NodeIndex) {
        let Some(import_node) = self.arena.get(import_idx) else {
            return;
        };
        let Some(import_eq) = self.arena.get_import_decl(import_node) else {
            return;
        };

        self.write_indent();
        self.write("import ");

        if import_eq.import_clause.is_some() {
            self.emit_node(import_eq.import_clause);
        }

        if let Some(module_node) = self.arena.get(import_eq.module_specifier) {
            if module_node.kind == SyntaxKind::StringLiteral as u16 {
                self.write(" = require(");
                self.emit_node(import_eq.module_specifier);
                self.write(")");
            } else {
                self.write(" = ");
                self.emit_node(import_eq.module_specifier);
            }
        } else {
            self.write(" = ");
        }

        self.write(";");
        self.write_line();
    }

    pub(crate) fn emit_namespace_export_declaration(&mut self, export_idx: NodeIndex) {
        let Some(export_node) = self.arena.get(export_idx) else {
            return;
        };
        let Some(export) = self.arena.get_export_decl(export_node) else {
            return;
        };

        // For "export as namespace" declarations:
        // - export_clause is the namespace name (identifier)

        self.write_indent();
        self.write("export as namespace ");

        // Emit namespace name from export_clause
        if export.export_clause.is_some() {
            self.emit_node(export.export_clause);
        }

        self.write(";");
        self.write_line();
    }
}
