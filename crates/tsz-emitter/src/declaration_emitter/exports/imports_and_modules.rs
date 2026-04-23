//! Declaration emitter - import, module, parameter, and heritage clause emission.

use super::super::DeclarationEmitter;
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
                if self.inside_declare_namespace {
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
                self.emitted_non_exported_declaration = false;
                self.emitted_scope_marker = false;

                // Pre-scan to check if the body has a mix of exported and
                // non-exported members. When it does, tsc preserves `export`
                // keywords on individual members; otherwise it strips them.
                // Applies to both ambient string-named modules and non-ambient namespaces.
                let prev_ambient_scope_marker = self.ambient_module_has_scope_marker;
                if !is_ambient_ns || use_module_keyword {
                    self.ambient_module_has_scope_marker =
                        self.module_body_has_scope_marker(stmts, !is_ambient_ns);
                }

                for &stmt_idx in &stmts.nodes {
                    self.emit_statement(stmt_idx);
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
            }

            self.public_api_scope_depth = prev_public_api_scope_depth;
            self.inside_non_ambient_namespace = prev_inside_non_ambient_namespace;
            self.inside_declare_namespace = prev_inside_declare_namespace;
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
            if self.inside_non_ambient_namespace {
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
            let is_require_import = self
                .arena
                .get(import_eq.module_specifier)
                .is_some_and(|n| n.kind == SyntaxKind::StringLiteral as u16);
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

    // Helper methods

    pub(crate) fn emit_parameters(&mut self, params: &NodeList) {
        self.emit_parameters_with_body(params, NodeIndex::NONE);
    }

    pub(crate) fn emit_parameters_with_body(&mut self, params: &NodeList, body_idx: NodeIndex) {
        // Find the index of the last required parameter (no ?, no initializer, no rest).
        // Parameters with initializers before the last required param cannot use `?` syntax;
        // instead they emit `param: Type | undefined` (matching tsc behavior).
        let last_required_idx = params
            .nodes
            .iter()
            .rposition(|&idx| {
                self.arena
                    .get(idx)
                    .and_then(|n| self.arena.get_parameter(n))
                    .is_some_and(|p| {
                        !p.question_token && p.initializer.is_none() && !p.dot_dot_dot_token
                    })
            })
            .unwrap_or(0);

        let mut first = true;
        for (i, &param_idx) in params.nodes.iter().enumerate() {
            if !first {
                self.write(", ");
            }
            first = false;

            if let Some(param_node) = self.arena.get(param_idx)
                && let Some(param) = self.arena.get_parameter(param_node)
            {
                let jsdoc_param = if self.source_is_js_file {
                    self.jsdoc_param_decl_for_parameter(param_idx, i)
                } else {
                    None
                };
                let is_parameter_property = self.in_constructor_params
                    && self.parameter_has_property_modifier(&param.modifiers);

                // For public parameter properties, tsc appends `| undefined` to the
                // constructor parameter type as well as the property declaration.
                // For private/protected parameter properties, the type is hidden on
                // the property (`private x?;`) so no `| undefined` is added to the
                // constructor parameter.
                let is_private_param_property = is_parameter_property
                    && param.modifiers.as_ref().is_some_and(|mods| {
                        mods.nodes.iter().any(|&mod_idx| {
                            self.arena
                                .get(mod_idx)
                                .is_some_and(|n| n.kind == SyntaxKind::PrivateKeyword as u16)
                        })
                    });

                // Inline JSDoc comment before parameter (e.g. /** comment */ a: string)
                self.emit_inline_parameter_comment(param_node.pos);

                // Modifiers (public, private, etc for constructor parameters)
                self.emit_member_modifiers(&param.modifiers);

                // Rest parameter
                if param.dot_dot_dot_token || jsdoc_param.as_ref().is_some_and(|decl| decl.rest) {
                    self.write("...");
                }

                // Name
                self.emit_node(param.name);

                // A parameter with an initializer that appears before the last required
                // parameter is NOT optional — you can't omit it. Instead, its type
                // gets `| undefined` appended. Explicitly optional (?) params always use `?`.
                let has_initializer_before_required =
                    param.initializer.is_some() && !param.question_token && i < last_required_idx;

                if param.question_token
                    || jsdoc_param
                        .as_ref()
                        .is_some_and(|decl| decl.optional && !decl.rest)
                    || (param.initializer.is_some() && !has_initializer_before_required)
                {
                    self.write("?");
                }

                // Type
                if param.type_annotation.is_some() {
                    self.write(": ");
                    if let Some(rescued) = self.rescued_asserts_parameter_type_text(param_idx) {
                        self.write(&rescued);
                    } else {
                        self.emit_type(param.type_annotation);
                    }
                    // For non-private parameter properties with `?`, tsc appends
                    // `| undefined` to both the property declaration and the constructor
                    // parameter type. For private params, the type is hidden so skip.
                    if is_parameter_property && !is_private_param_property && param.question_token {
                        let output = self.writer.get_output();
                        if !output.ends_with("| undefined") {
                            self.write(" | undefined");
                        }
                    }
                } else if let Some(ref jsdoc_param) = jsdoc_param
                    && !Self::jsdoc_type_needs_checker_resolution(&jsdoc_param.type_text)
                {
                    self.write(": ");
                    self.write(&jsdoc_param.type_text);
                } else if let Some(ref jsdoc_param) = jsdoc_param
                    && Self::jsdoc_type_needs_checker_resolution(&jsdoc_param.type_text)
                    && let Some(converted) =
                        Self::convert_jsdoc_function_type(&jsdoc_param.type_text)
                {
                    self.write(": ");
                    self.write(&converted);
                } else if let Some(type_id) = self
                    .get_node_type_or_names(&[param_idx, param.name])
                    .or_else(|| {
                        // Parameters with binding-pattern names store their inferred type in
                        // symbol_types (via cache_parameter_types), not in node_types.
                        // Try the parameter node's symbol first, then the name node's symbol.
                        let name_node = self.arena.get(param.name)?;
                        (name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                            || name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN)
                            .then(|| {
                                self.get_symbol_cached_type(param_idx)
                                    .or_else(|| self.get_symbol_cached_type(param.name))
                            })
                            .flatten()
                    })
                {
                    // Inferred type from type cache
                    self.write(": ");
                    self.write(&self.print_type_id(type_id));
                } else if param.initializer.is_some()
                    && let Some(type_text) =
                        self.allowlisted_initializer_type_text(param.initializer)
                {
                    self.write(": ");
                    self.write(&type_text);
                } else if param.dot_dot_dot_token {
                    // Rest parameters without explicit type → any[]
                    self.write(": any[]");
                } else if !self.source_is_declaration_file {
                    // Empty object binding pattern `{}` without a type annotation
                    // gets type `{}` (not `any`), matching tsc behavior.
                    let is_empty_object_binding = self.arena.get(param.name).is_some_and(|n| {
                        n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                            && self
                                .arena
                                .get_binding_pattern(n)
                                .is_none_or(|bp| bp.elements.nodes.is_empty())
                    });
                    if is_empty_object_binding {
                        self.write(": {}");
                    } else {
                        // In declaration emit from source, parameters without
                        // explicit type annotations default to `any` (matching tsc)
                        self.write(": any");
                    }
                }

                // When strictNullChecks is true and a parameter has an
                // initializer before the last required parameter, tsc appends
                // `| undefined` — but only when the type doesn't already
                // include undefined (to avoid `T | undefined | undefined`).
                if self.strict_null_checks && has_initializer_before_required {
                    let output = self.writer.get_output();
                    if !output.ends_with("| undefined") {
                        self.write(" | undefined");
                    }
                }
            }
        }

        if self.should_emit_js_arguments_rest_param(params, body_idx) {
            if !first {
                self.write(", ");
            }
            self.write("...args: any[]");
        }
    }

    pub(crate) fn parameter_has_property_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        modifiers.as_ref().is_some_and(|mods| {
            mods.nodes.iter().any(|&mod_idx| {
                self.arena.get(mod_idx).is_some_and(|mod_node| {
                    let kind = mod_node.kind;
                    kind == SyntaxKind::PublicKeyword as u16
                        || kind == SyntaxKind::PrivateKeyword as u16
                        || kind == SyntaxKind::ProtectedKeyword as u16
                        || kind == SyntaxKind::ReadonlyKeyword as u16
                        || kind == SyntaxKind::OverrideKeyword as u16
                })
            })
        })
    }

    /// Emit parameters without type annotations (used for private accessors)
    pub(crate) fn emit_parameters_without_types(&mut self, params: &NodeList, omit_types: bool) {
        if !omit_types {
            self.emit_parameters(params);
            return;
        }

        let mut first = true;
        for &param_idx in &params.nodes {
            if !first {
                self.write(", ");
            }
            first = false;

            if let Some(param_node) = self.arena.get(param_idx)
                && let Some(param) = self.arena.get_parameter(param_node)
            {
                // Rest parameter
                if param.dot_dot_dot_token {
                    self.write("...");
                }

                // Name only (no type)
                self.emit_node(param.name);

                // Optional marker still included
                if param.question_token {
                    self.write("?");
                }
            }
        }
    }

    fn should_emit_js_arguments_rest_param(&self, params: &NodeList, body_idx: NodeIndex) -> bool {
        if !self.source_is_js_file || body_idx.is_none() {
            return false;
        }

        let has_rest_param = params.nodes.iter().any(|&param_idx| {
            self.arena
                .get(param_idx)
                .and_then(|param_node| self.arena.get_parameter(param_node))
                .is_some_and(|param| param.dot_dot_dot_token)
        });
        if has_rest_param {
            return false;
        }

        tsz_parser::syntax::transform_utils::contains_arguments_reference(self.arena, body_idx)
    }

    pub(crate) fn emit_type_parameters(&mut self, type_params: &NodeList) {
        self.write("<");
        let mut first = true;
        for &param_idx in &type_params.nodes {
            if !first {
                self.write(", ");
            }
            first = false;

            if let Some(param_node) = self.arena.get(param_idx)
                && let Some(param) = self.arena.get_type_parameter(param_node)
            {
                // Inline JSDoc comment before type parameter
                self.emit_inline_parameter_comment(param_node.pos);

                // Emit variance/const modifiers (in, out, const)
                if let Some(ref mods) = param.modifiers {
                    for &mod_idx in &mods.nodes {
                        if let Some(mod_node) = self.arena.get(mod_idx) {
                            match mod_node.kind {
                                k if k == SyntaxKind::InKeyword as u16 => self.write("in "),
                                k if k == SyntaxKind::OutKeyword as u16 => self.write("out "),
                                k if k == SyntaxKind::ConstKeyword as u16 => self.write("const "),
                                _ => {}
                            }
                        }
                    }
                }

                self.emit_node(param.name);

                if param.constraint.is_some() {
                    self.write(" extends ");
                    self.emit_type(param.constraint);
                }

                if param.default.is_some() {
                    self.write(" = ");
                    self.emit_type(param.default);
                }
            }
        }
        self.write(">");
    }

    pub(crate) fn emit_heritage_clauses(&mut self, clauses: &NodeList) {
        self.emit_heritage_clauses_inner(clauses, false, None);
    }

    pub(crate) fn emit_class_heritage_clauses(
        &mut self,
        clauses: &NodeList,
        extends_alias: Option<&str>,
    ) {
        self.emit_heritage_clauses_inner(clauses, false, extends_alias);
    }

    pub(crate) fn emit_interface_heritage_clauses(&mut self, clauses: &NodeList) {
        self.emit_heritage_clauses_inner(clauses, true, None);
    }

    fn emit_heritage_clauses_inner(
        &mut self,
        clauses: &NodeList,
        is_interface: bool,
        extends_alias: Option<&str>,
    ) {
        for &clause_idx in &clauses.nodes {
            let Some(clause_node) = self.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = self.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            let keyword = match heritage.token {
                k if k == SyntaxKind::ExtendsKeyword as u16 => "extends",
                k if k == SyntaxKind::ImplementsKeyword as u16 => "implements",
                _ => continue,
            };

            // For interfaces, filter out heritage types with non-entity-name
            // expressions (e.g. `typeof X`, parenthesized expressions).
            // tsc strips these in declaration emit.
            let valid_types: Vec<_> = if is_interface {
                heritage
                    .types
                    .nodes
                    .iter()
                    .copied()
                    .filter(|&type_idx| self.is_entity_name_heritage(type_idx))
                    .collect()
            } else {
                heritage.types.nodes.clone()
            };

            if valid_types.is_empty() {
                continue;
            }

            self.write(" ");
            self.write(keyword);
            self.write(" ");

            if heritage.token == SyntaxKind::ExtendsKeyword as u16
                && let Some(alias_name) = extends_alias
            {
                self.write(alias_name);
                if let Some(&type_idx) = valid_types.first()
                    && let Some(type_node) = self.arena.get(type_idx)
                    && let Some(expr) = self.arena.get_expr_type_args(type_node)
                    && let Some(ref type_args) = expr.type_arguments
                    && !type_args.nodes.is_empty()
                {
                    self.emit_type_arguments(type_args);
                }
                continue;
            }

            let mut first = true;
            for &type_idx in &valid_types {
                if !first {
                    self.write(", ");
                }
                first = false;
                self.emit_type(type_idx);
            }
        }
    }

    /// Check if a heritage type expression is an entity name (identifier or
    /// property access chain). Non-entity-name expressions like `typeof X` or
    /// parenthesized expressions are invalid in interface `extends` clauses
    /// and should be stripped in .d.ts output.
    pub(crate) fn is_entity_name_heritage(&self, type_idx: NodeIndex) -> bool {
        let Some(type_node) = self.arena.get(type_idx) else {
            return false;
        };
        // Heritage types may be wrapped in ExpressionWithTypeArguments (when
        // type args are present, e.g. `extends Foo<T>`), or may be bare
        // identifiers / property access chains (e.g. `extends A, B`).
        if let Some(eta) = self.arena.get_expr_type_args(type_node) {
            self.is_entity_name_expr(eta.expression)
        } else {
            self.is_entity_name_expr(type_idx)
        }
    }

    fn is_entity_name_expr(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind == SyntaxKind::Identifier as u16
            || expr_node.kind == SyntaxKind::NullKeyword as u16
        {
            return true;
        }
        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.arena.get_access_expr(expr_node)
        {
            return self.is_entity_name_expr(access.expression);
        }
        false
    }

    /// Pre-scan a module body to determine if it has a "scope marker" —
    /// either an explicit `export {}` statement or a mix of exported and
    /// non-exported members. When true, `export` keywords should be preserved
    /// on individual members inside the ambient module.
    ///
    /// When `non_ambient` is true (non-ambient namespaces), only namespace
    /// declarations count as visible non-exported members. Other non-exported
    /// declarations (classes, interfaces, variables, etc.) are not emitted
    /// in the .d.ts output and should not trigger the scope marker.
    pub(crate) fn module_body_has_scope_marker(
        &self,
        stmts: &tsz_parser::parser::NodeList,
        non_ambient: bool,
    ) -> bool {
        let mut has_exported = false;
        let mut has_non_exported = false;

        for &stmt_idx in &stmts.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };

            match stmt_node.kind {
                k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                    if let Some(export) = self.arena.get_export_decl(stmt_node) {
                        // `export {}` — explicit scope marker
                        if let Some(clause_node) = self.arena.get(export.export_clause)
                            && clause_node.kind == syntax_kind_ext::NAMED_EXPORTS
                            && let Some(named) = self.arena.get_named_imports(clause_node)
                            && named.elements.nodes.is_empty()
                        {
                            return true;
                        }
                        // `export *` or `export * from "mod"` — scope marker
                        // (export_clause is None for bare `export *`)
                        if !export.export_clause.is_some()
                            || self
                                .arena
                                .get(export.export_clause)
                                .is_some_and(|n| n.kind == syntax_kind_ext::NAMESPACE_EXPORT)
                        {
                            return true;
                        }
                        // Check if export_clause wraps a declaration (e.g., `export class Foo`)
                        // — these count as exported members, not scope markers
                        if let Some(clause_node) = self.arena.get(export.export_clause) {
                            let ck = clause_node.kind;
                            if ck == syntax_kind_ext::CLASS_DECLARATION
                                || ck == syntax_kind_ext::FUNCTION_DECLARATION
                                || ck == syntax_kind_ext::INTERFACE_DECLARATION
                                || ck == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                                || ck == syntax_kind_ext::ENUM_DECLARATION
                                || ck == syntax_kind_ext::VARIABLE_STATEMENT
                                || ck == syntax_kind_ext::MODULE_DECLARATION
                                || ck == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                            {
                                has_exported = true;
                                if non_ambient
                                    && ck == syntax_kind_ext::CLASS_DECLARATION
                                    && let Some(class) = self.arena.get_class(clause_node)
                                    && class
                                        .heritage_clauses
                                        .as_ref()
                                        .and_then(|heritage| {
                                            self.non_nameable_extends_heritage_type(heritage)
                                        })
                                        .is_some()
                                {
                                    has_non_exported = true;
                                }
                            } else {
                                // Named exports like `export { a, b }` — scope marker
                                return true;
                            }
                        }
                    }
                }
                k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                    // `export = value` or `export default` — scope marker
                    return true;
                }
                _ => {
                    if self.stmt_has_export_modifier(stmt_node) {
                        has_exported = true;
                        if non_ambient
                            && stmt_node.kind == syntax_kind_ext::CLASS_DECLARATION
                            && let Some(class) = self.arena.get_class(stmt_node)
                            && class
                                .heritage_clauses
                                .as_ref()
                                .and_then(|heritage| {
                                    self.non_nameable_extends_heritage_type(heritage)
                                })
                                .is_some()
                        {
                            has_non_exported = true;
                        }
                    } else {
                        // Skip ImportDeclaration and ImportEqualsDeclaration
                        // as they don't count as non-exported members
                        if stmt_node.kind == syntax_kind_ext::IMPORT_DECLARATION
                            || stmt_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                        {
                            continue;
                        }
                        // In non-ambient namespaces, non-exported declarations
                        // are only emitted in .d.ts if they are referenced by
                        // exported members (via used_symbols). Namespace
                        // declarations are always visible.
                        if non_ambient {
                            if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION
                                || self.is_ns_member_used_by_exports(stmt_idx)
                            {
                                has_non_exported = true;
                            }
                        } else {
                            has_non_exported = true;
                        }
                    }
                }
            }

            if has_exported && has_non_exported {
                return true;
            }
        }

        false
    }

    pub(crate) fn emit_member_modifiers(&mut self, modifiers: &Option<NodeList>) {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    match mod_node.kind {
                        // In constructor parameters, strip accessibility and readonly modifiers
                        k if k == SyntaxKind::PublicKeyword as u16 => {
                            // In .d.ts files, `public` is the default and is omitted by tsc.
                            // Only emit it for constructor parameter properties
                            // (which is handled separately and already skips it).
                        }
                        k if k == SyntaxKind::PrivateKeyword as u16
                            && !self.in_constructor_params =>
                        {
                            self.write("private ");
                        }
                        k if k == SyntaxKind::ProtectedKeyword as u16
                            && !self.in_constructor_params =>
                        {
                            self.write("protected ");
                        }
                        k if k == SyntaxKind::ReadonlyKeyword as u16
                            && !self.in_constructor_params =>
                        {
                            self.write("readonly ");
                        }
                        k if k == SyntaxKind::StaticKeyword as u16 => self.write("static "),
                        k if k == SyntaxKind::AbstractKeyword as u16 => self.write("abstract "),
                        k if k == SyntaxKind::OverrideKeyword as u16 => {
                            // tsc strips `override` in .d.ts output — it is not
                            // part of the declaration surface.
                        }
                        k if k == SyntaxKind::AsyncKeyword as u16 => {
                            // tsc strips `async` in .d.ts — the return type already
                            // encodes Promise<T>, so the modifier is redundant.
                        }
                        k if k == SyntaxKind::AccessorKeyword as u16 => self.write("accessor "),
                        k if k == SyntaxKind::DeclareKeyword as u16 => {
                            // tsc strips `declare` from class members in .d.ts — it is
                            // only meaningful at the top-level statement level
                            // (`declare class`, `declare function`, etc.).
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}
