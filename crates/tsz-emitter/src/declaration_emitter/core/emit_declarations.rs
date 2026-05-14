use rustc_hash::FxHashSet;
use tracing::debug;
use tsz_common::comments::is_jsdoc_comment;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::type_queries;

use super::{DeclarationEmitter, ImportPlan};

impl<'a> DeclarationEmitter<'a> {
    /// Emit declaration for a source file
    pub fn emit(&mut self, root_idx: NodeIndex) -> String {
        // Reset per-file emission state
        self.used_symbols = None;
        self.foreign_symbols = None;
        self.import_name_map.clear();
        self.import_symbol_map.clear();
        self.import_string_aliases.clear();
        self.reserved_names.clear();
        self.symbol_module_specifier_cache.clear();
        self.import_plan = ImportPlan::default();

        self.reset_writer();
        self.indent_level = 0;
        self.emitted_non_exported_declaration = false;
        self.emitted_scope_marker = false;
        self.emitted_module_indicator = false;

        // Seed overload tracking from precomputed ExportSurface if available.
        // This replaces the incremental on-the-fly detection for top-level
        // functions, ensuring overload grouping is correct even if the surface
        // was built in a previous pass.
        if let Some(ref surface) = self.export_surface {
            self.function_names_with_overloads = surface.overloaded_functions.clone();
        }

        // Prepare import metadata for elision BEFORE running UsageAnalyzer
        // This builds the SymbolId -> ModuleSpecifier map from existing imports
        self.prepare_import_metadata(root_idx);

        // Run usage analyzer if we have all required components AND haven't run yet
        if self.used_symbols.is_none() {
            debug!(
                "[DEBUG] emit: type_cache.is_none()={}",
                self.type_cache.is_none()
            );
            debug!(
                "[DEBUG] emit: type_interner.is_none()={}",
                self.type_interner.is_none()
            );
            debug!(
                "[DEBUG] emit: current_arena.is_none()={}",
                self.current_arena.is_none()
            );

            if let (Some(cache), Some(interner), Some(binder), Some(current_arena)) = (
                &self.type_cache,
                self.type_interner,
                self.binder,
                &self.current_arena,
            ) {
                debug!(
                    "[DEBUG] emit: import_name_map has {} entries: {:?}",
                    self.import_name_map.len(),
                    self.import_name_map
                );
                let mut analyzer = crate::declaration_emitter::usage_analyzer::UsageAnalyzer::new(
                    self.arena,
                    binder,
                    cache,
                    interner,
                    std::sync::Arc::clone(current_arena),
                    self.current_file_path.clone(),
                    &self.import_name_map,
                    self.arena
                        .get(root_idx)
                        .and_then(|node| self.arena.get_source_file(node))
                        .is_some_and(|source_file| self.source_file_is_js(source_file)),
                );
                let used = analyzer.analyze(root_idx).clone();
                let foreign = analyzer.get_foreign_symbols();
                debug!(
                    "[DEBUG] emit: foreign_symbols has {} symbols",
                    foreign.len()
                );
                self.used_symbols = Some(used);
                self.foreign_symbols = Some(foreign.clone());
            }
        }

        let Some(root_node) = self.arena.get(root_idx) else {
            return String::new();
        };

        let Some(source_file) = self.arena.get_source_file(root_node) else {
            return String::new();
        };

        if !self.source_file_is_js(source_file) {
            self.retain_synthetic_class_extends_alias_dependencies_in_statements(
                &source_file.statements,
            );
            self.retain_export_default_expression_type_dependencies_in_statements(
                &source_file.statements,
            );
            self.retain_synthetic_function_return_dependencies_in_statements(
                &source_file.statements,
            );
            self.retain_synthetic_variable_declaration_dependencies_in_statements(
                &source_file.statements,
            );
            self.retain_asserted_class_property_type_dependencies_in_statements(
                &source_file.statements,
            );
            self.retain_imported_static_call_dependencies_in_statements(&source_file.statements);
        }

        // Prepare aliases and build the import plan before emitting anything
        self.prepare_import_aliases(root_idx);
        self.prepare_import_plan();

        self.source_file_text = Some(source_file.text.clone());
        self.source_is_declaration_file = source_file.is_declaration_file;
        self.source_is_js_file = self.source_file_is_js(source_file);
        self.current_source_file_idx = Some(root_idx);
        // Prefer the pre-computed flag from ExportSurface when available;
        // fall back to the existing AST walk for JS files (which need
        // CommonJS-specific detection the surface doesn't cover yet).
        self.emit_public_api_only = if let Some(ref surface) = self.export_surface {
            if !self.source_is_js_file {
                surface.has_public_api_scope
            } else {
                self.has_public_api_exports(source_file)
            }
        } else {
            self.has_public_api_exports(source_file)
        };
        self.all_comments = source_file.comments.clone();
        self.comment_emit_idx = 0;
        if self.emit_js_cross_file_commonjs_merge_diagnostic(source_file) {
            return String::new();
        }
        let (js_named_export_names, folded_named_exports, deferred_named_exports) =
            self.collect_js_folded_named_exports(source_file);
        self.js_named_export_names = js_named_export_names;
        self.js_folded_named_export_statements = folded_named_exports;
        self.js_deferred_named_export_statements = deferred_named_exports;
        self.js_deferred_local_export_enum_statements =
            self.collect_js_local_export_enum_statements(source_file);
        let (local_export_aliases, skipped_local_export_aliases) =
            self.collect_js_local_export_aliases(source_file);
        self.js_local_export_aliases = local_export_aliases;
        self.js_skipped_local_export_aliases = skipped_local_export_aliases;
        self.js_export_equals_names = self.collect_js_export_equals_names(source_file);
        self.emitted_js_export_equals_names.clear();
        self.js_export_default_names = self.collect_js_export_default_names(source_file);
        self.emitted_js_export_default_names.clear();
        self.js_shadowed_export_equals_local_aliases.clear();
        let (
            js_commonjs_named_export_names,
            js_commonjs_named_function_exports,
            js_commonjs_named_value_exports,
        ) = self.collect_js_commonjs_named_exports(source_file);
        self.js_named_export_names
            .extend(js_commonjs_named_export_names);
        let js_hoistable_function_export_names = self.js_named_export_names.clone();
        let (module_exports_obj_names, module_exports_obj_stmts) =
            self.collect_js_module_exports_object_names(source_file);
        self.js_named_export_names.extend(module_exports_obj_names);
        self.js_module_exports_object_stmts = module_exports_obj_stmts;
        self.js_require_property_import_aliases.clear();
        let cjs_aliases = self.collect_js_cjs_export_aliases(source_file);
        self.js_cjs_export_aliases = cjs_aliases.aliases;
        self.js_cjs_export_alias_value_declarations = cjs_aliases.value_declarations;
        self.js_cjs_export_alias_statements = cjs_aliases.skipped_statements;
        // Mark CJS alias local names as used so they survive usage analysis pruning.
        if let Some(binder) = self.binder
            && let Some(ref mut used) = self.used_symbols
        {
            for (_export_name, local_name) in &self.js_cjs_export_aliases {
                if let Some(sym_id) = binder.file_locals.get(local_name) {
                    used.entry(sym_id).or_insert(
                        crate::declaration_emitter::usage_analyzer::UsageKind::VALUE
                            | crate::declaration_emitter::usage_analyzer::UsageKind::TYPE,
                    );
                }
            }
        }
        self.js_namespace_export_aliases =
            self.collect_js_namespace_export_aliases(source_file, &self.js_export_equals_names);
        let js_namespace_class_expando_declarations =
            self.collect_js_namespace_class_expando_declarations(source_file);
        let js_commonjs_expando_declarations = self
            .collect_js_commonjs_expando_declarations(source_file, &self.js_export_equals_names);
        self.js_deferred_function_export_statements = js_commonjs_expando_declarations
            .function_statements
            .into_iter()
            .map(|(stmt_idx, (name_idx, initializer))| (stmt_idx, (name_idx, initializer, false)))
            .collect();
        self.js_deferred_function_export_statements.extend(
            js_commonjs_named_function_exports.into_iter().map(
                |(stmt_idx, (name_idx, initializer))| (stmt_idx, (name_idx, initializer, true)),
            ),
        );
        self.js_deferred_value_export_statements = js_commonjs_expando_declarations
            .value_statements
            .into_iter()
            .map(|(stmt_idx, (name_idx, initializer))| (stmt_idx, (name_idx, initializer, false)))
            .collect();
        self.js_deferred_value_export_statements.extend(
            js_commonjs_named_value_exports.into_iter().map(
                |(stmt_idx, (name_idx, initializer))| (stmt_idx, (name_idx, initializer, true)),
            ),
        );
        self.js_deferred_value_export_statements.extend(
            js_namespace_class_expando_declarations.into_iter().map(
                |(stmt_idx, (name_idx, initializer))| (stmt_idx, (name_idx, initializer, false)),
            ),
        );
        // Remove CJS export alias statements from deferred maps.
        for &stmt_idx in &self.js_cjs_export_alias_statements {
            self.js_deferred_function_export_statements
                .remove(&stmt_idx);
            self.js_deferred_value_export_statements.remove(&stmt_idx);
        }
        self.js_deferred_prototype_method_statements =
            js_commonjs_expando_declarations.prototype_methods;
        let js_class_like =
            self.collect_js_class_like_prototype_members(source_file, &self.js_export_equals_names);
        self.js_class_like_prototype_members = js_class_like.members;
        self.js_class_like_prototype_stmts = js_class_like.consumed_stmts;
        let js_class_static = self.collect_js_class_static_members(source_file);
        self.js_class_static_members = js_class_static.members;
        self.js_class_static_member_stmts = js_class_static.consumed_stmts;
        for stmt_idx in &self.js_class_static_member_stmts {
            self.js_deferred_function_export_statements.remove(stmt_idx);
            self.js_deferred_value_export_statements.remove(stmt_idx);
        }
        let (js_class_define_property_accessors, js_class_define_property_accessor_stmts) =
            self.collect_js_class_define_property_accessors(source_file);
        self.js_class_define_property_accessors = js_class_define_property_accessors;
        self.js_class_define_property_accessor_stmts = js_class_define_property_accessor_stmts;
        let js_static_method_augmentations =
            self.collect_js_class_static_method_augmentations(source_file);
        self.js_static_method_augmentation_statements = js_static_method_augmentations.statements;
        self.js_skipped_static_method_augmentation_statements =
            js_static_method_augmentations.skipped_statements;
        self.js_augmented_static_method_nodes =
            js_static_method_augmentations.augmented_method_nodes;
        let (grouped_reexports, skipped_reexports) = self.collect_js_grouped_reexports(source_file);
        self.js_grouped_reexports = grouped_reexports;
        self.js_skipped_reexports = skipped_reexports;
        self.emitted_jsdoc_type_aliases.clear();
        self.emitted_synthetic_dependency_symbols.clear();
        let deferred_js_namespace_objects =
            self.collect_js_namespace_object_statements(source_file);
        let (nested_module_export_namespaces, skipped_nested_module_export_namespace_stmts) =
            self.collect_js_module_exports_nested_namespaces(source_file);
        for stmt_idx in &skipped_nested_module_export_namespace_stmts {
            self.js_deferred_function_export_statements.remove(stmt_idx);
            self.js_deferred_value_export_statements.remove(stmt_idx);
        }
        let js_commonjs_closure_export = self.js_commonjs_export_assignment_closure(source_file);

        debug!(
            "[DEBUG] source_file has {} comments",
            source_file.comments.len()
        );

        // Emit detached copyright comments (/*! ... */) at the very top
        self.emit_detached_copyright_comments(source_file);

        // Emit triple-slash directives at the very top (before imports)
        self.emit_triple_slash_directives(source_file);

        // Emit required imports first (before other declarations)
        let before_imports = self.writer.len();
        self.emit_required_imports();

        // Emit auto-generated imports for foreign symbols
        self.emit_auto_imports();
        if self.writer.len() > before_imports {
            // Auto-generated imports count as external module indicators
            self.emitted_module_indicator = true;
        }

        for &stmt_idx in &source_file.statements.nodes {
            if let Some((name_idx, initializer)) =
                self.js_named_export_equals_class_expression(stmt_idx)
            {
                if let Some(name) = self.get_identifier_text(name_idx) {
                    let _ = self.js_shadowed_export_equals_local_alias(&name);
                }
                self.emit_pending_js_export_equals_for_name(name_idx);
                let _ =
                    self.emit_js_named_class_expression_declaration(name_idx, initializer, false);
                self.emit_js_namespace_export_aliases_for_name(name_idx, false);
            } else if let Some(initializer) =
                self.js_anonymous_export_equals_class_expression_initializer(stmt_idx)
            {
                self.emit_js_anonymous_export_equals_class_expression_declaration(initializer);
            } else if let Some(initializer) =
                self.js_anonymous_export_equals_value_initializer(stmt_idx)
            {
                self.emit_js_anonymous_export_equals_value_declaration(initializer);
            }
        }
        if let Some((_, root_initializer, ref secondary_members)) = js_commonjs_closure_export {
            self.emit_js_commonjs_closure_export_assignment(root_initializer, secondary_members);
        }

        // For JS source files, hoist `export default <Identifier>` statements that
        // reference a top-level local declaration to the very top of the .d.ts.
        // This mirrors tsc's `transformDeclarations` behaviour for JS inputs.
        // The original ExportDeclaration statement is suppressed when the main loop
        // reaches it because `emit_export_declaration` checks
        // `emitted_js_export_default_names`.
        if self.source_is_js_file && !self.js_export_default_names.is_empty() {
            self.emit_jsdoc_default_typedef_aliases_for_hoisted_default_exports(source_file);
            self.emit_hoisted_js_export_default_statements(source_file);
        }

        if self.source_is_js_file {
            for &stmt_idx in &source_file.statements.nodes {
                let Some(stmt_node) = self.arena.get(stmt_idx) else {
                    continue;
                };
                // Function declarations whose JS-named-export is folded into
                // an `export { foo }` statement are emitted at the export
                // statement's source position via the unfold path. Hoisting
                // them to the top would put them before sibling inline-
                // exported declarations (`export const __esModule = false`)
                // and produce an order that disagrees with tsc.
                if self.js_deferred_named_export_statements.contains(&stmt_idx) {
                    continue;
                }
                let should_hoist = if stmt_node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
                    self.arena.get_function(stmt_node).is_some_and(|func| {
                        self.arena
                            .has_modifier(&func.modifiers, SyntaxKind::ExportKeyword)
                            || self.get_identifier_text(func.name).is_some_and(|name| {
                                js_hoistable_function_export_names.contains(&name)
                            })
                    })
                } else if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                    self.arena
                        .get_export_decl(stmt_node)
                        .and_then(|export| self.arena.get(export.export_clause))
                        .is_some_and(|clause| clause.kind == syntax_kind_ext::FUNCTION_DECLARATION)
                } else {
                    false
                };
                if !should_hoist {
                    continue;
                }
                self.js_hoisted_function_declarations.insert(stmt_idx);
                self.emit_hoisted_js_function_statement(stmt_idx);
            }
        }

        let mut deferred_js_import_declarations = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            if let Some(members) = nested_module_export_namespaces.get(&stmt_idx) {
                if let Some((root_name, _)) = self.js_module_exports_property_assignment(stmt_idx) {
                    self.write_indent();
                    self.write("export namespace ");
                    self.write(&root_name);
                    self.write(" {");
                    self.write_line();
                    self.increase_indent();
                    for &(member_name, initializer) in members {
                        self.emit_js_object_literal_namespace(
                            member_name,
                            initializer,
                            false,
                            false,
                        );
                    }
                    self.decrease_indent();
                    self.write_indent();
                    self.write("}");
                    self.write_line();
                    self.emitted_module_indicator = true;
                }
                continue;
            }
            if skipped_nested_module_export_namespace_stmts.contains(&stmt_idx) {
                continue;
            }
            if js_commonjs_closure_export
                .as_ref()
                .is_some_and(|(closure_stmt_idx, _, _)| *closure_stmt_idx == stmt_idx)
            {
                if let Some(stmt_node) = self.arena.get(stmt_idx) {
                    self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
                }
                continue;
            }
            if deferred_js_namespace_objects.contains(&stmt_idx)
                && !self.js_namespace_object_stmt_emits_in_source_order(stmt_idx)
            {
                continue;
            }
            if self.js_hoisted_function_declarations.contains(&stmt_idx) {
                if let Some(stmt_node) = self.arena.get(stmt_idx) {
                    self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
                }
                continue;
            }
            if self.js_cjs_export_alias_statements.contains(&stmt_idx) {
                if let Some(stmt_node) = self.arena.get(stmt_idx) {
                    self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
                }
                self.emit_js_cjs_export_aliases();
                continue;
            }
            if self.js_module_exports_object_stmts.contains(&stmt_idx) {
                if let Some(initializer) = self.js_module_exports_assignment_initializer(stmt_idx) {
                    self.emit_js_anonymous_module_exports_object_members(initializer);
                }
                continue;
            }
            if self
                .js_deferred_local_export_enum_statements
                .contains(&stmt_idx)
            {
                continue;
            }
            if self
                .js_named_export_equals_class_expression(stmt_idx)
                .is_some()
                || self
                    .js_anonymous_export_equals_class_expression_initializer(stmt_idx)
                    .is_some()
                || self
                    .js_anonymous_export_equals_value_initializer(stmt_idx)
                    .is_some()
            {
                continue;
            }
            if self.source_is_js_file
                && self
                    .arena
                    .get(stmt_idx)
                    .is_some_and(|stmt_node| stmt_node.kind == syntax_kind_ext::IMPORT_DECLARATION)
                && self
                    .arena
                    .get(stmt_idx)
                    .and_then(|stmt_node| self.arena.get_import_decl(stmt_node))
                    .is_some_and(|import| import.import_clause.is_some())
            {
                deferred_js_import_declarations.push(stmt_idx);
                continue;
            }
            self.emit_statement(stmt_idx);
        }
        for &stmt_idx in &source_file.statements.nodes {
            if deferred_js_namespace_objects.contains(&stmt_idx)
                && !self.js_namespace_object_stmt_emits_in_source_order(stmt_idx)
            {
                self.emit_statement(stmt_idx);
            }
        }
        for import_idx in deferred_js_import_declarations {
            if self.emit_deferred_js_import_declaration(import_idx) {
                self.emitted_module_indicator = true;
            }
        }

        self.emit_pending_top_level_jsdoc_type_aliases(source_file);
        self.emit_pending_jsdoc_callback_type_aliases(source_file);
        self.emit_trailing_top_level_jsdoc_type_aliases(source_file);
        self.emit_js_require_property_import_aliases();
        self.emit_deferred_js_local_export_enum_statements(source_file);
        self.emit_js_local_export_aliases();
        self.emit_js_cjs_export_aliases();
        if !self.source_is_js_file
            && let Ok(eof_pos) = u32::try_from(source_file.text.len())
        {
            self.emit_leading_jsdoc_comments(eof_pos);
        }

        // Add `export {};` scope fix marker when needed (mirrors tsc's transformDeclarations).
        // Uses emission-time tracking instead of source-file analysis.
        //
        // tsc logic: if isExternalModule(node) &&
        //   (!resultHasExternalModuleIndicator || (needsScopeFixMarker && !resultHasScopeMarker))
        let is_module = self.source_file_has_module_syntax(source_file);

        if is_module
            && (!self.emitted_module_indicator
                || (self.emitted_non_exported_declaration && !self.emitted_scope_marker))
        {
            self.write("export {};");
            self.write_line();
        }

        let mut output = self.writer.get_output().to_string();
        for line in source_file.text.lines() {
            let trimmed = line.trim();
            let Some(rest) = trimmed.strip_prefix("import ") else {
                continue;
            };
            let Some(named_start) = rest.find('{') else {
                continue;
            };
            let Some(named_end) = rest[named_start + 1..].find('}') else {
                continue;
            };
            let named = &rest[named_start + 1..named_start + 1 + named_end];
            let Some((_, module_part)) =
                rest[named_start + 1 + named_end + 1..].split_once(" from ")
            else {
                continue;
            };
            let module = module_part.trim().trim_end_matches(';').trim();
            for specifier in named.split(',') {
                let import_specifier = specifier.trim();
                let name = import_specifier
                    .split_once(" as ")
                    .map_or(import_specifier, |(_, alias)| alias.trim());
                if name.is_empty() {
                    continue;
                }
                let import_line = format!("import {{ {import_specifier} }} from {module};");
                if !output.contains(&import_line)
                    && output.contains(&format!(": {name}<"))
                    && !Self::type_reference_only_in_matching_ambient_module(&output, name, module)
                {
                    output.insert_str(0, &(import_line + "\n"));
                }
            }
        }
        output = Self::prune_unused_named_import_specifiers_from_output(&output);
        output
    }

    fn type_reference_only_in_matching_ambient_module(
        output: &str,
        name: &str,
        module: &str,
    ) -> bool {
        let reference = format!(": {name}<");
        let module_start = format!("declare module {module}");
        let mut ambient_depth = 0usize;
        let mut found_in_ambient_module = false;

        for line in output.lines() {
            let enters_matching_module =
                ambient_depth == 0 && line.trim_start().starts_with(&module_start);
            if enters_matching_module {
                ambient_depth = 1;
            }

            if line.contains(&reference) {
                if ambient_depth == 0 {
                    return false;
                }
                found_in_ambient_module = true;
            }

            if ambient_depth > 0 {
                if !enters_matching_module {
                    ambient_depth = ambient_depth.saturating_add(line.matches('{').count());
                }
                ambient_depth = ambient_depth.saturating_sub(line.matches('}').count());
            }
        }

        found_in_ambient_module
    }

    fn prune_unused_named_import_specifiers_from_output(output: &str) -> String {
        let lines = output.lines().collect::<Vec<_>>();
        let mut changed = false;
        let mut rewritten = Vec::with_capacity(lines.len());

        for (line_idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            let Some(rest) = trimmed.strip_prefix("import { ") else {
                rewritten.push((*line).to_string());
                continue;
            };
            let Some((named, module_part)) = rest.split_once(" } from ") else {
                rewritten.push((*line).to_string());
                continue;
            };
            let module_part = module_part.trim_end_matches(';');
            if module_part.is_empty() {
                rewritten.push((*line).to_string());
                continue;
            }

            let body = lines
                .iter()
                .enumerate()
                .filter_map(|(idx, body_line)| (idx != line_idx).then_some(*body_line))
                .collect::<Vec<_>>()
                .join("\n");
            let kept = named
                .split(',')
                .filter_map(|specifier| {
                    let specifier = specifier.trim();
                    if specifier.is_empty() {
                        return None;
                    }
                    let local_name = specifier
                        .split_once(" as ")
                        .map_or(specifier, |(_, alias)| alias.trim());
                    Self::contains_whole_word_in_text(&body, local_name).then_some(specifier)
                })
                .collect::<Vec<_>>();

            if kept.len()
                == named
                    .split(',')
                    .filter(|part| !part.trim().is_empty())
                    .count()
            {
                rewritten.push((*line).to_string());
                continue;
            }
            changed = true;
            if !kept.is_empty() {
                let indent_len = line.len() - trimmed.len();
                rewritten.push(format!(
                    "{}import {{ {} }} from {};",
                    &line[..indent_len],
                    kept.join(", "),
                    module_part
                ));
            }
        }

        if changed {
            let mut text = rewritten.join("\n");
            if output.ends_with('\n') {
                text.push('\n');
            }
            text
        } else {
            output.to_string()
        }
    }

    pub(in crate::declaration_emitter) fn emit_statement(&mut self, stmt_idx: NodeIndex) {
        self.emit_statement_with_options(stmt_idx, false);
    }

    pub(crate) fn emit_deferred_js_named_export_statement(&mut self, stmt_idx: NodeIndex) {
        self.emit_statement_with_options(stmt_idx, true);
    }

    pub(in crate::declaration_emitter) fn emit_statement_with_options(
        &mut self,
        stmt_idx: NodeIndex,
        allow_deferred_js_named_export: bool,
    ) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };

        if !allow_deferred_js_named_export
            && self.js_deferred_named_export_statements.contains(&stmt_idx)
        {
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            return;
        }
        if self
            .js_skipped_static_method_augmentation_statements
            .contains(&stmt_idx)
        {
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            return;
        }
        if self.js_class_like_prototype_stmts.contains(&stmt_idx) {
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            return;
        }
        if self.js_class_static_member_stmts.contains(&stmt_idx) {
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            return;
        }
        if self
            .js_class_define_property_accessor_stmts
            .contains(&stmt_idx)
        {
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            return;
        }

        let kind = stmt_node.kind;

        // For non-declaration statements (expression statements, assignments, etc.),
        // skip their comments entirely rather than emitting them as leading JSDoc.
        let has_synthetic_js_expression_declaration = kind == syntax_kind_ext::EXPRESSION_STATEMENT
            && self.has_synthetic_js_expression_declaration(stmt_idx);
        let is_declaration_kind = kind == syntax_kind_ext::FUNCTION_DECLARATION
            || kind == syntax_kind_ext::CLASS_DECLARATION
            || kind == syntax_kind_ext::INTERFACE_DECLARATION
            || kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
            || kind == syntax_kind_ext::ENUM_DECLARATION
            || kind == syntax_kind_ext::VARIABLE_STATEMENT
            || kind == syntax_kind_ext::EXPORT_DECLARATION
            || kind == syntax_kind_ext::EXPORT_ASSIGNMENT
            || kind == syntax_kind_ext::IMPORT_DECLARATION
            || kind == syntax_kind_ext::MODULE_DECLARATION
            || kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            || kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
            || has_synthetic_js_expression_declaration;

        if !is_declaration_kind {
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            return;
        }

        if self.has_internal_annotation(stmt_node.pos) {
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            return;
        }

        let is_variable_like_export = kind == syntax_kind_ext::VARIABLE_STATEMENT
            || (kind == syntax_kind_ext::EXPORT_DECLARATION
                && self
                    .arena
                    .get_export_decl(stmt_node)
                    .and_then(|export| self.arena.get(export.export_clause))
                    .is_some_and(|clause| clause.kind == syntax_kind_ext::VARIABLE_STATEMENT));
        if !is_variable_like_export {
            self.emit_leading_jsdoc_type_aliases_for_pos(stmt_node.pos);
        }

        if kind == syntax_kind_ext::FUNCTION_DECLARATION
            && let Some(func) = self.arena.get_function(stmt_node)
            && self.is_js_export_equals_name(func.name)
        {
            self.emit_pending_js_export_equals_for_name(func.name);
        }

        // Save position before JSDoc comments so we can undo them if the
        // declaration turns out to be invisible (non-exported in namespace, etc.)
        let before_jsdoc_len = self.writer.len();
        let saved_comment_idx = self.comment_emit_idx;
        self.current_statement_jsdoc_chain =
            self.emittable_jsdoc_comment_chain_for_pos(stmt_node.pos);
        let has_jsdoc_type_alias = self
            .current_statement_jsdoc_chain
            .iter()
            .any(|jsdoc| Self::jsdoc_contains_type_alias_tag(jsdoc));
        let suppress_jsdoc_type_alias_comments =
            has_jsdoc_type_alias && self.statement_emits_js_object_literal_namespace(stmt_idx);
        let has_jsdoc_type_function_signature = self
            .statement_jsdoc_type_function_signature_node(stmt_idx)
            .is_some();
        if has_jsdoc_type_function_signature || has_jsdoc_type_alias {
            self.emit_leading_jsdoc_comments(stmt_node.pos);
            self.writer.truncate(before_jsdoc_len);
            let mut filtered = if has_jsdoc_type_function_signature {
                Self::jsdoc_chain_without_type_tags(&self.current_statement_jsdoc_chain)
            } else {
                self.current_statement_jsdoc_chain.clone()
            };
            if suppress_jsdoc_type_alias_comments {
                filtered.retain(|jsdoc| !Self::jsdoc_contains_type_alias_tag(jsdoc));
            }
            if !self.emit_jsdoc_comment_chain_preserving_source_for_pos(stmt_node.pos, &filtered) {
                self.emit_jsdoc_comment_chain(&filtered);
            }
        } else {
            self.emit_leading_jsdoc_comments(stmt_node.pos);
        }
        let before_len = self.writer.len();
        self.queue_source_mapping(stmt_node);
        self.suppress_current_statement_jsdoc_comments = false;

        let has_effective_export = self.statement_has_effective_export(stmt_idx);
        match kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.emit_function_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.emit_class_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                self.emit_interface_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                self.emit_type_alias_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.emit_enum_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.emit_variable_declaration_statement(stmt_idx);
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                self.emit_export_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                self.emit_export_assignment(stmt_idx);
            }
            k if k == syntax_kind_ext::IMPORT_DECLARATION => {
                // Skip emitting import declarations here - they're handled by import elision
                // via emit_auto_imports() which only emits imports for symbols that are actually used
                // The import_symbol_map tracks which imports are part of the elision system
                // We still need to emit declarations that are NOT in import_symbol_map (but those should be rare)
                self.emit_import_declaration_if_needed(stmt_idx);
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.emit_module_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                self.emit_import_equals_declaration(stmt_idx, false);
            }
            k if k == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION => {
                self.emit_namespace_export_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                self.emit_js_synthetic_expression_statement(stmt_idx);
            }
            _ => unreachable!(),
        }

        let did_emit = self.writer.len() != before_len;
        if !did_emit {
            // The handler didn't emit anything (e.g., non-exported declaration in namespace).
            // Undo the speculatively emitted JSDoc comments and skip all comments in this
            // statement's range so they don't leak to the next declaration.
            self.writer.truncate(before_jsdoc_len);
            self.comment_emit_idx = saved_comment_idx;
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            self.pending_source_pos = None;
        } else {
            if self.suppress_current_statement_jsdoc_comments && before_len > before_jsdoc_len {
                let emitted = self.writer.get_output()[before_len..].to_string();
                self.writer.truncate(before_jsdoc_len);
                self.write(&emitted);
            }
            // Track whether we emitted a scope marker or a non-exported declaration.
            // This is used to decide whether `export {};` is needed at the end.
            let is_scope_marker = kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                || kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
                || (kind == syntax_kind_ext::EXPORT_DECLARATION && {
                    // Only pure export declarations count as scope markers,
                    // not `export class/function/etc` which are declarations with export
                    self.arena
                        .get(stmt_idx)
                        .and_then(|n| self.arena.get_export_decl(n))
                        .and_then(|ed| self.arena.get(ed.export_clause))
                        .is_none_or(|clause| {
                            let ck = clause.kind;
                            ck != syntax_kind_ext::INTERFACE_DECLARATION
                                && ck != syntax_kind_ext::TYPE_ALIAS_DECLARATION
                                && ck != syntax_kind_ext::CLASS_DECLARATION
                                && ck != syntax_kind_ext::FUNCTION_DECLARATION
                                && ck != syntax_kind_ext::ENUM_DECLARATION
                                && ck != syntax_kind_ext::VARIABLE_STATEMENT
                                && ck != syntax_kind_ext::MODULE_DECLARATION
                                && ck != syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                        })
                });

            if is_scope_marker {
                self.emitted_scope_marker = true;
                self.emitted_module_indicator = true;
            } else if has_effective_export
                || kind == syntax_kind_ext::EXPORT_DECLARATION
                || kind == syntax_kind_ext::IMPORT_DECLARATION
                || kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                // Any export/import statement is a module indicator
                self.emitted_module_indicator = true;
            }

            if !has_effective_export && kind != syntax_kind_ext::EXPORT_DECLARATION {
                // A declaration without export modifier was emitted.
                // Module augmentations (`declare global`, `declare module "foo"`)
                // are not regular declarations and should not trigger
                // the `export {};` scope-fix marker.
                let is_module_augmentation = kind == syntax_kind_ext::MODULE_DECLARATION
                    && self
                        .arena
                        .get(stmt_idx)
                        .and_then(|n| self.arena.get_module(n))
                        .and_then(|m| self.arena.get(m.name))
                        .is_some_and(|name_node| {
                            name_node.kind == SyntaxKind::StringLiteral as u16
                                || self
                                    .arena
                                    .get_identifier(name_node)
                                    .is_some_and(|id| id.escaped_text == "global")
                        });
                let is_declaration_kind = (kind == syntax_kind_ext::FUNCTION_DECLARATION
                    || kind == syntax_kind_ext::CLASS_DECLARATION
                    || kind == syntax_kind_ext::INTERFACE_DECLARATION
                    || kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || kind == syntax_kind_ext::ENUM_DECLARATION
                    || kind == syntax_kind_ext::VARIABLE_STATEMENT
                    || kind == syntax_kind_ext::MODULE_DECLARATION)
                    && !is_module_augmentation;
                if is_declaration_kind {
                    self.emitted_non_exported_declaration = true;
                }
            }
        }
        self.suppress_current_statement_jsdoc_comments = false;
        self.current_statement_jsdoc_chain.clear();
    }

    fn emit_hoisted_js_function_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };

        self.current_statement_jsdoc_chain =
            self.leading_jsdoc_comment_chain_for_pos(stmt_node.pos);
        let jsdoc_chain = self.current_statement_jsdoc_chain.clone();
        let has_jsdoc_type_function_signature = self
            .statement_jsdoc_type_function_signature_node(stmt_idx)
            .is_some();
        if has_jsdoc_type_function_signature {
            let filtered = Self::jsdoc_chain_without_type_tags(&jsdoc_chain);
            self.emit_jsdoc_comment_chain(&filtered);
        } else if jsdoc_chain.len() == 1
            && Self::jsdoc_has_function_signature_tags(jsdoc_chain[0].as_str())
            && self.hoisted_jsdoc_source_comment_is_multiline(stmt_node.pos)
        {
            self.emit_multiline_jsdoc_comment(&jsdoc_chain[0]);
        } else {
            self.emit_jsdoc_comment_chain(&jsdoc_chain);
        }
        let saved_comment_idx = self.comment_emit_idx;
        self.comment_emit_idx = self
            .all_comments
            .iter()
            .position(|comment| comment.end > stmt_node.pos)
            .unwrap_or(self.all_comments.len());

        match stmt_node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.emit_function_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                self.emit_export_declaration(stmt_idx);
            }
            _ => {}
        }

        self.emitted_module_indicator = true;
        self.comment_emit_idx = saved_comment_idx;
        self.current_statement_jsdoc_chain.clear();
    }

    fn hoisted_jsdoc_source_comment_is_multiline(&self, pos: u32) -> bool {
        let Some(text) = self.source_file_text.as_deref() else {
            return true;
        };
        let Some(comment) = self.all_comments.iter().rev().find(|comment| {
            comment.end <= pos
                && is_jsdoc_comment(comment, text)
                && text
                    .get(comment.end as usize..pos as usize)
                    .is_some_and(|between| between.trim().is_empty())
        }) else {
            return true;
        };
        text.get(comment.pos as usize..comment.end as usize)
            .is_none_or(|raw| raw.contains('\n'))
    }

    pub(in crate::declaration_emitter) fn emit_function_declaration(
        &mut self,
        func_idx: NodeIndex,
    ) {
        let Some(func_node) = self.arena.get(func_idx) else {
            return;
        };
        let Some(func) = self.arena.get_function(func_node) else {
            return;
        };

        // Check for export modifier
        let is_exported = self
            .arena
            .has_modifier(&func.modifiers, SyntaxKind::ExportKeyword)
            || self.is_js_named_exported_name(func.name);

        // `export default function() { ... }` — delegate to the export default handler
        // which correctly emits `export default function (): ReturnType;`
        let is_default = self
            .arena
            .has_modifier(&func.modifiers, SyntaxKind::DefaultKeyword);
        if is_exported && is_default {
            self.emit_export_default_function(func_idx);
            return;
        }

        let used_by_exported_object_literal =
            self.get_identifier_text(func.name).is_some_and(|name| {
                self.namespace_member_referenced_by_exported_object_literal(func_idx, &name)
            });
        if !is_exported
            && !self.should_emit_public_api_member(&func.modifiers)
            && !self.should_emit_public_api_dependency(func.name)
            && !used_by_exported_object_literal
        {
            return;
        }
        if self.should_skip_ns_internal_member(&func.modifiers, Some(func_idx)) {
            return;
        }
        let late_bound_members = self.collect_ts_late_bound_assignment_members(func.name);

        // Get function name as string for overload tracking
        let function_name = self.get_function_name(func_idx);

        // Check if this is an overload (no body) or implementation (has body)
        let is_overload = func.body.is_none();
        let is_implementation = !is_overload;
        let should_emit_late_bound_namespace =
            self.should_emit_ts_late_bound_function_namespace(func_idx, func.name, is_overload);

        // Overload handling:
        // - If this is an overload, emit it and mark that this function has overloads
        // - If this is an implementation and the function already has overloads, skip it
        // - If this is an implementation with no overloads, emit it
        if is_overload {
            // Mark that this function name has overload signatures
            if let Some(ref name) = function_name {
                self.function_names_with_overloads.insert(name.clone());
            }
        } else if is_implementation {
            // This is an implementation - check if we've seen overloads for this name
            if let Some(ref name) = function_name
                && self.function_names_with_overloads.contains(name)
            {
                self.skip_comments_in_node(func_node.pos, func_node.end);
                return;
            }
        }

        self.emit_pending_js_export_equals_for_name(func.name);
        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write("function ");

        // Function name
        self.emit_node(func.name);

        if self.source_is_js_file
            && let Some((type_params, params, return_type)) =
                self.jsdoc_function_type_signature_for_node(func_idx)
        {
            self.emit_jsdoc_function_type_signature(&type_params, &params, &return_type);
            self.write(";");
            self.write_line();
            if should_emit_late_bound_namespace {
                self.emit_ts_late_bound_function_namespace_from_members(
                    func.name,
                    is_exported,
                    &late_bound_members,
                );
            }
            if !self.emit_js_function_like_class_if_needed(
                func.name,
                &func.parameters,
                func.body,
                is_exported,
                func_idx,
            ) {
                self.emit_js_synthetic_prototype_class_if_needed(func.name, is_exported);
            }
            self.emit_js_class_static_members_namespace(func.name, is_exported);
            self.emit_js_namespace_export_aliases_for_name(func.name, is_exported);
            return;
        }

        // Type parameters
        let jsdoc_template_params = if func
            .type_parameters
            .as_ref()
            .is_none_or(|type_params| type_params.nodes.is_empty())
        {
            self.jsdoc_template_params_for_node(func_idx)
        } else {
            Vec::new()
        };
        if let Some(ref type_params) = func.type_parameters {
            if !type_params.nodes.is_empty() {
                self.emit_type_parameters(type_params);
            } else if !jsdoc_template_params.is_empty() {
                self.emit_jsdoc_template_parameters(&jsdoc_template_params);
            }
        } else if !jsdoc_template_params.is_empty() {
            self.emit_jsdoc_template_parameters(&jsdoc_template_params);
        }

        // Parameters
        self.write("(");
        self.emit_parameters_with_body(&func.parameters, func.body);
        self.write(")");

        // Return type
        let func_body = func.body;
        let func_name = func.name;
        if func.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(func.type_annotation);
        } else if let Some(return_type_text) = self.jsdoc_return_type_text_for_node(func_idx) {
            self.write(": ");
            self.write(&return_type_text);
        } else if let (Some(return_type_text), true) =
            self.function_body_return_hint(func, func_body)
        {
            self.write(": ");
            self.write(&return_type_text);
        } else if let Some(return_type_text) = self
            .js_function_body_preferred_return_text_for_declaration(
                func.body,
                func.name,
                &func.parameters,
            )
        {
            self.write(": ");
            self.write(&return_type_text);
        } else if let Some(return_type_text) = self.boolean_default_param_return_type_text(func) {
            self.write(": ");
            self.write(&return_type_text);
        } else if func_body.is_some()
            && self.emit_js_returned_define_property_function_type(func_body)
        {
        } else if func_body.is_some()
            && self
                .get_identifier_text(func.name)
                .is_some_and(|name| self.function_body_returns_identifier(func_body, &name))
        {
            self.write(": typeof ");
            self.emit_node(func.name);
        } else if let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache) {
            // No explicit return type, try to infer it
            let func_type_id = cache
                .node_types
                .get(&func_idx.0)
                .copied()
                .or_else(|| self.get_node_type_or_names(&[func_name]))
                .or_else(|| self.get_type_via_symbol_for_func(func_idx, func_name));
            if let Some(func_type_id) = func_type_id
                && let Some(return_type_id) = type_queries::get_return_type(*interner, func_type_id)
            {
                let effective_return_type_id = if func_body.is_some() {
                    self.refine_invokable_return_type_from_identifier(func_body, return_type_id)
                        .or_else(|| {
                            self.refine_object_rest_return_type_from_identifier(
                                func_body,
                                return_type_id,
                            )
                        })
                        .unwrap_or(return_type_id)
                } else {
                    return_type_id
                };
                let (preferred_return, direct_function_return) =
                    self.function_body_return_hint(func, func_body);
                let scoped_preferred_return = preferred_return.as_ref().map(|type_text| {
                    let (type_text, substituted_parameter_type_query) =
                        self.function_return_type_text_for_declaration_scope(func, type_text);
                    (
                        self.restore_mapped_return_type_param_constraints(func, &type_text),
                        substituted_parameter_type_query,
                    )
                });
                // If solver returned `any` OR `undefined` but the function body clearly
                // returns void (every control-flow exit is a bare `return;` or falls
                // off the end), prefer void. tsc's rule: an unannotated function whose
                // only return is `return;` (no expression) has return type `void`, not
                // `undefined` — the solver approximates it as `undefined` from the
                // runtime value, which we widen to `void` here. Matches
                // declFileTypeAnnotationBuiltInType.
                let solver_void_like = effective_return_type_id == tsz_solver::types::TypeId::ANY
                    || effective_return_type_id == tsz_solver::types::TypeId::UNDEFINED
                    || effective_return_type_id == tsz_solver::types::TypeId::NEVER;
                if solver_void_like && func_body.is_some() && self.body_returns_void(func_body) {
                    self.write(": void");
                } else if let Some(type_text) = func_body
                    .is_some()
                    .then(|| {
                        self.async_returned_function_initializer_promise_type_text(func, func_body)
                    })
                    .flatten()
                {
                    self.write(": ");
                    self.write(&type_text);
                } else if let Some((type_text, substituted_parameter_type_query)) =
                    scoped_preferred_return.as_ref()
                    && let Some(func_name_text) = self.get_identifier_text(func_name)
                    && let printed_return_type = self.print_type_id(effective_return_type_id)
                    && (direct_function_return
                        || printed_return_type == format!("ReturnType<typeof {func_name_text}>")
                        || self.should_prefer_source_return_type_text(
                            preferred_return.as_deref().unwrap_or(type_text),
                            effective_return_type_id,
                        )
                        || self.source_return_type_is_function_type_param(func, type_text)
                        || self.source_return_type_preserves_function_type_param(
                            func,
                            type_text,
                            effective_return_type_id,
                        )
                        || (*substituted_parameter_type_query && !type_text.contains("typeof ")))
                {
                    self.write(": ");
                    self.write(type_text);
                } else if self.emit_single_nameable_new_return_type_if_solver_any(
                    func,
                    func_body,
                    func_name,
                    effective_return_type_id,
                ) {
                } else if effective_return_type_id == tsz_solver::types::TypeId::ANY
                    && let Some(type_text) = preferred_return
                {
                    let (type_text, _) =
                        self.function_return_type_text_for_declaration_scope(func, &type_text);
                    if let Some(returned_identifier) =
                        self.function_body_unique_return_identifier(func_body)
                        && let Some(return_type_id) =
                            self.reference_declared_type_id(returned_identifier)
                        && let Some(name_text) = self.get_identifier_text(func_name)
                        && let Some(name_node) = self.arena.get(func_name)
                        && let Some(file_path) = self.current_file_path.clone()
                    {
                        self.check_non_portable_type_references(
                            return_type_id,
                            &name_text,
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        );
                    }
                    if let Some(name_text) = self.get_identifier_text(func_name)
                        && let Some(name_node) = self.arena.get(func_name)
                        && let Some(file_path) = self.current_file_path.clone()
                    {
                        self.check_non_portable_type_references(
                            effective_return_type_id,
                            &name_text,
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        );
                        let _ = self.emit_non_portable_import_type_text_diagnostics(
                            &type_text,
                            &name_text,
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        );
                    }
                    self.write(": ");
                    self.write(&type_text);
                } else if effective_return_type_id == tsz_solver::types::TypeId::ANY
                    && func_body.is_some()
                    && self
                        .get_identifier_text(func.name)
                        .is_some_and(|name| self.function_body_returns_identifier(func_body, &name))
                {
                    self.write(": typeof ");
                    self.emit_node(func.name);
                } else {
                    if func_body.is_some()
                        && let Some(name_text) = self.get_identifier_text(func_name)
                        && let Some(name_node) = self.arena.get(func_name)
                        && let Some(file_path) = self.current_file_path.clone()
                    {
                        self.check_non_portable_type_references(
                            effective_return_type_id,
                            &name_text,
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        );
                    }
                    self.write(": ");
                    if let Some(ref tp) = func.type_parameters
                        && !tp.nodes.is_empty()
                    {
                        let printed_type_text =
                            self.inferred_function_return_type_text(func, effective_return_type_id);
                        let printed_type_text = self
                            .expand_rest_tuple_parameters_in_function_type_text(
                                func_body,
                                &printed_type_text,
                            )
                            .unwrap_or(printed_type_text);
                        self.write(&printed_type_text);
                        if let Some(name_text) = self.get_identifier_text(func_name)
                            && let Some(name_node) = self.arena.get(func_name)
                            && let Some(file_path) = self.current_file_path.clone()
                        {
                            let _ = self.emit_non_portable_import_type_text_diagnostics(
                                &printed_type_text,
                                &name_text,
                                &file_path,
                                name_node.pos,
                                name_node.end - name_node.pos,
                            );
                        }
                    } else {
                        let printed_type_text = self.print_type_id(effective_return_type_id);
                        let printed_type_text = self
                            .rewrite_returned_auto_accessor_parameter_unknowns(
                                func,
                                &printed_type_text,
                            );
                        let printed_type_text = self
                            .expand_rest_tuple_parameters_in_function_type_text(
                                func_body,
                                &printed_type_text,
                            )
                            .unwrap_or(printed_type_text);
                        self.write(&printed_type_text);
                        if let Some(name_text) = self.get_identifier_text(func_name)
                            && let Some(name_node) = self.arena.get(func_name)
                            && let Some(file_path) = self.current_file_path.clone()
                        {
                            let _ = self.emit_non_portable_import_type_text_diagnostics(
                                &printed_type_text,
                                &name_text,
                                &file_path,
                                name_node.pos,
                                name_node.end - name_node.pos,
                            );
                        }
                    }
                }
            } else if func_body.is_some() {
                if self.body_returns_void(func_body) {
                    self.write(": void");
                } else if let Some(type_text) =
                    self.async_returned_function_initializer_promise_type_text(func, func_body)
                {
                    self.write(": ");
                    self.write(&type_text);
                } else if let Some(return_text) = self.function_body_return_hint(func, func_body).0
                {
                    let (return_text, _) =
                        self.function_return_type_text_for_declaration_scope(func, &return_text);
                    if let Some(name_text) = self.get_identifier_text(func_name)
                        && let Some(name_node) = self.arena.get(func_name)
                        && let Some(file_path) = self.current_file_path.clone()
                    {
                        if let Some(func_type_id) = self
                            .get_node_type_or_names(&[func_name])
                            .or_else(|| self.get_type_via_symbol_for_func(func_idx, func_name))
                        {
                            self.check_non_portable_type_references(
                                func_type_id,
                                &name_text,
                                &file_path,
                                name_node.pos,
                                name_node.end - name_node.pos,
                            );
                        }
                        let _ = self.emit_non_portable_import_type_text_diagnostics(
                            &return_text,
                            &name_text,
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        );
                    }
                    self.write(": ");
                    self.write(&return_text);
                }
            }
        } else if func_body.is_some() {
            // No type cache available, but we can infer from the body
            if self.body_returns_void(func_body) {
                self.write(": void");
            } else if let Some(type_text) =
                self.async_returned_function_initializer_promise_type_text(func, func_body)
            {
                self.write(": ");
                self.write(&type_text);
            } else if let Some(return_text) = self.function_body_return_hint(func, func_body).0 {
                let (return_text, _) =
                    self.function_return_type_text_for_declaration_scope(func, &return_text);
                if let Some(name_text) = self.get_identifier_text(func_name)
                    && let Some(name_node) = self.arena.get(func_name)
                    && let Some(file_path) = self.current_file_path.clone()
                {
                    if let Some(func_type_id) = self
                        .get_node_type_or_names(&[func_name])
                        .or_else(|| self.get_type_via_symbol_for_func(func_idx, func_name))
                    {
                        self.check_non_portable_type_references(
                            func_type_id,
                            &name_text,
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        );
                    }
                    let _ = self.emit_non_portable_import_type_text_diagnostics(
                        &return_text,
                        &name_text,
                        &file_path,
                        name_node.pos,
                        name_node.end - name_node.pos,
                    );
                }
                self.write(": ");
                self.write(&return_text);
            }
        }

        self.write(";");
        self.write_line();
        if should_emit_late_bound_namespace {
            self.emit_ts_late_bound_function_namespace_from_members(
                func.name,
                is_exported,
                &late_bound_members,
            );
        }
        if !self.emit_js_function_like_class_if_needed(
            func.name,
            &func.parameters,
            func.body,
            is_exported,
            func_idx,
        ) {
            self.emit_js_synthetic_prototype_class_if_needed(func.name, is_exported);
        }
        self.emit_js_class_static_members_namespace(func.name, is_exported);
        self.emit_js_namespace_export_aliases_for_name(func.name, is_exported);

        // Skip comments within the function body to prevent them from
        // leaking as leading comments on the next statement.
        if let Some(body_node) = self.arena.get(func_body) {
            self.skip_comments_in_node(body_node.pos, body_node.end);
        }
    }

    pub(crate) fn emit_class_declaration(&mut self, class_idx: NodeIndex) {
        let Some(class_node) = self.arena.get(class_idx) else {
            return;
        };
        let Some(class) = self.arena.get_class(class_node) else {
            return;
        };

        let is_exported = self
            .arena
            .has_modifier(&class.modifiers, SyntaxKind::ExportKeyword)
            || self.is_js_named_exported_name(class.name);
        if !is_exported
            && !self.should_emit_public_api_member(&class.modifiers)
            && !self.is_js_export_equals_name(class.name)
            && !self.is_confirmed_public_api_dependency(class.name)
        {
            return;
        }
        if self.should_skip_ns_internal_member(&class.modifiers, Some(class_idx)) {
            return;
        }
        let is_abstract = self
            .arena
            .has_modifier(&class.modifiers, SyntaxKind::AbstractKeyword);
        let extends_alias = self.emit_synthetic_class_extends_alias_if_needed(
            class.name,
            class.heritage_clauses.as_ref(),
            false,
        );
        let shadow_alias = self
            .get_identifier_text(class.name)
            .and_then(|name| self.js_shadowed_export_equals_local_alias(&name));

        if shadow_alias.is_none() {
            self.emit_pending_js_export_equals_for_name(class.name);
        }
        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        if is_abstract {
            self.write("abstract ");
        }
        self.write("class ");

        // Class name
        if let Some(alias) = shadow_alias.as_deref() {
            self.write(alias);
        } else {
            self.emit_node(class.name);
        }

        // Type parameters
        let jsdoc_template_anchor = class
            .modifiers
            .as_ref()
            .and_then(|mods| mods.nodes.first().copied())
            .and_then(|mod_idx| self.arena.get(mod_idx))
            .map(|mod_node| mod_node.pos)
            .unwrap_or(class_node.pos);
        let mut jsdoc_template_params = self
            .current_statement_jsdoc_chain
            .iter()
            .flat_map(|jsdoc| Self::parse_jsdoc_template_params(jsdoc))
            .collect::<Vec<_>>();
        if jsdoc_template_params.is_empty() {
            jsdoc_template_params = self.jsdoc_template_params_for_pos(jsdoc_template_anchor);
        }
        if jsdoc_template_params.is_empty() {
            jsdoc_template_params = self
                .nearest_jsdoc_comment_for_pos_relaxed(jsdoc_template_anchor)
                .map(|jsdoc| Self::parse_jsdoc_template_params(&jsdoc))
                .unwrap_or_default();
        }
        if jsdoc_template_params.is_empty() {
            jsdoc_template_params = self.jsdoc_template_params_for_node(class_idx);
        }
        if jsdoc_template_params.is_empty() {
            jsdoc_template_params = self.jsdoc_template_params_for_node(class.name);
        }
        if self.source_is_js_file && !jsdoc_template_params.is_empty() {
            self.emit_jsdoc_template_parameters(&jsdoc_template_params);
        } else if let Some(ref type_params) = class.type_parameters {
            if !type_params.nodes.is_empty() {
                self.emit_type_parameters(type_params);
            }
        }

        // Heritage clauses (extends, implements)
        if let Some(ref heritage) = class.heritage_clauses {
            self.emit_class_heritage_clauses(heritage, extends_alias.as_deref());
        }

        self.write(" {");
        self.write_line();
        self.increase_indent();

        // Reset constructor and method overload tracking for this class
        self.class_has_constructor_overloads = false;
        self.class_extends_another = class.heritage_clauses.as_ref().is_some_and(|hc| {
            hc.nodes.iter().any(|&clause_idx| {
                self.arena
                    .get_heritage_clause_at(clause_idx)
                    .is_some_and(|h| h.token == SyntaxKind::ExtendsKeyword as u16)
            })
        });
        self.method_names_with_overloads = FxHashSet::default();

        // Suppress method implementations that share a computed name with
        // an accessor (tsc emits only the accessor in .d.ts).
        let shadowed = self.computed_names_shadowed_by_accessors(&class.members);
        self.method_names_with_overloads.extend(shadowed);

        // Emit parameter properties from constructor first (before other members)
        self.emit_parameter_properties(&class.members);

        // Emit `#private;` if any member has a private identifier name (e.g., #foo)
        if self.class_has_private_identifier_member(&class.members) {
            self.write_indent();
            self.write("#private;");
            self.write_line();
        }

        self.emit_js_array_subclass_constructor_overloads_if_needed(
            &class.members,
            class.heritage_clauses.as_ref(),
        );
        self.emit_ordered_class_members_with_js_constructor_assignment_properties(&class.members);
        if self.source_is_js_file {
            self.emit_js_class_define_property_accessors_for_name(class.name);
            if let Some(name) = self.get_identifier_text(class.name)
                && let Some(methods) = self.js_class_like_prototype_members.get(&name).cloned()
            {
                let mut declared_names = class
                    .members
                    .nodes
                    .iter()
                    .filter_map(|&member_idx| self.get_member_name_idx(member_idx))
                    .filter_map(|name_idx| self.get_identifier_text(name_idx))
                    .collect::<FxHashSet<_>>();
                for (method_name, initializer) in methods {
                    let Some(method_name_text) = self.get_identifier_text(method_name) else {
                        continue;
                    };
                    if !declared_names.insert(method_name_text) {
                        continue;
                    }
                    self.emit_js_synthetic_class_method(method_name, initializer);
                }
            }
        }
        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
        self.emit_js_class_static_members_namespace(class.name, is_exported);
        if shadow_alias.is_none() {
            self.emit_js_namespace_export_aliases_for_name(class.name, is_exported);
        }
    }

    pub(in crate::declaration_emitter) fn emit_class_member(&mut self, member_idx: NodeIndex) {
        let Some(member_node) = self.arena.get(member_idx) else {
            return;
        };

        // Strip members annotated with @internal when --stripInternal is enabled
        if self.has_internal_annotation(member_node.pos) {
            return;
        }

        // Skip members with private identifier names (#foo) - these are replaced by `#private;`
        if self.member_has_private_identifier_name(member_idx) {
            return;
        }

        // Skip members with computed property names that are not emittable in .d.ts
        // (e.g., ["" + ""], [Symbol()], [variable] — only literals and well-known symbols survive)
        if self.member_has_non_emittable_computed_name(member_idx) {
            return;
        }

        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                self.emit_property_declaration(member_idx);
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                self.emit_method_declaration(member_idx);
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                self.emit_constructor_declaration(member_idx);
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                self.emit_accessor_declaration(member_idx, true);
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                self.emit_accessor_declaration(member_idx, false);
            }
            k if k == syntax_kind_ext::INDEX_SIGNATURE => {
                self.emit_index_signature(member_idx);
            }
            _ => {}
        }
    }

    /// Check if a member has a private identifier (#foo) name.
    pub(in crate::declaration_emitter) fn member_has_private_identifier_name(
        &self,
        member_idx: NodeIndex,
    ) -> bool {
        if let Some(name_idx) = self.get_member_name_idx(member_idx)
            && let Some(name_node) = self.arena.get(name_idx)
        {
            return name_node.kind == SyntaxKind::PrivateIdentifier as u16;
        }
        false
    }

    pub(in crate::declaration_emitter) fn emit_property_declaration(
        &mut self,
        prop_idx: NodeIndex,
    ) {
        let Some(prop_node) = self.arena.get(prop_idx) else {
            return;
        };
        let prop_node_end = prop_node.end;
        let Some(prop) = self.arena.get_property_decl(prop_node) else {
            return;
        };
        let prop_name_span = self
            .arena
            .get(prop.name)
            .map(|name_node| (name_node.pos, name_node.end - name_node.pos));

        self.write_indent();

        // Check if abstract for special handling
        let is_abstract = self
            .arena
            .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword);
        // Check if private for type annotation omission
        let is_private = self
            .arena
            .has_modifier(&prop.modifiers, SyntaxKind::PrivateKeyword);
        let has_explicit_accessibility = self
            .arena
            .has_modifier(&prop.modifiers, SyntaxKind::PrivateKeyword)
            || self
                .arena
                .has_modifier(&prop.modifiers, SyntaxKind::ProtectedKeyword)
            || self
                .arena
                .has_modifier(&prop.modifiers, SyntaxKind::PublicKeyword);

        let has_explicit_readonly = self
            .arena
            .has_modifier(&prop.modifiers, SyntaxKind::ReadonlyKeyword);

        // Modifiers
        if self.source_is_js_file
            && !has_explicit_accessibility
            && self.jsdoc_has_protected_for_node(prop_idx)
        {
            self.write("protected ");
        }
        self.emit_member_modifiers(&prop.modifiers);
        if !has_explicit_readonly
            && self.source_is_js_file
            && self.jsdoc_has_readonly_for_node(prop_idx)
        {
            self.write("readonly ");
        }

        // Name
        if self.source_is_js_file
            && let Some(name_text) = self.resolved_computed_property_name_text(prop.name)
        {
            self.write(&name_text);
        } else {
            self.emit_node(prop.name);
        }

        // Optional marker
        if prop.question_token {
            self.write("?");
        }

        // Check if readonly for literal initializer form
        let is_readonly = has_explicit_readonly
            || (self.source_is_js_file && self.jsdoc_has_readonly_for_node(prop_idx));
        let const_asserted_enum_member = prop
            .initializer
            .is_some()
            .then(|| self.const_asserted_enum_access_member_text(prop.initializer))
            .flatten();
        let widened_enum_type = prop
            .initializer
            .is_some()
            .then(|| self.simple_enum_access_base_name_text(prop.initializer))
            .flatten();

        // Type - use explicit annotation if present, otherwise use inferred type
        // SPECIAL CASE: For private properties, TypeScript omits type annotations in .d.ts
        if prop.type_annotation.is_some() && !is_private {
            self.write(": ");
            self.emit_type(prop.type_annotation);
        } else if !is_private && let Some(type_text) = self.jsdoc_type_text_for_node(prop_idx) {
            self.write(": ");
            self.write(&type_text);
            if prop.question_token && self.strict_null_checks && !type_text.ends_with("| undefined")
            {
                self.write(" | undefined");
            }
        } else if !is_private {
            // For readonly properties with an enum member access initializer (e.g., `readonly type = E.A`),
            // emit the initializer expression directly, matching tsc behavior.
            let use_enum_initializer = is_readonly
                && !is_abstract
                && !prop.question_token
                && prop.initializer.is_some()
                && self
                    .simple_enum_access_member_text(prop.initializer)
                    .is_some();

            if use_enum_initializer {
                self.write(" = ");
                self.emit_expression(prop.initializer);
            } else if let Some(enum_member_text) = const_asserted_enum_member {
                self.write(": ");
                self.write(&enum_member_text);
            } else if !is_readonly
                && !is_abstract
                && !prop.question_token
                && let Some(enum_type_text) = widened_enum_type
            {
                self.write(": ");
                self.write(&enum_type_text);
            } else if let Some(typeof_text) =
                self.shadowed_property_initializer_typeof_text(prop.name, prop.initializer)
            {
                self.write(": ");
                self.write(&typeof_text);
                if prop.question_token
                    && self.strict_null_checks
                    && !typeof_text.ends_with("| undefined")
                {
                    self.write(" | undefined");
                }
            } else if prop.initializer.is_some()
                && let Some(type_text) = self.explicit_asserted_type_text(prop.initializer)
            {
                self.write(": ");
                self.write(&type_text);
            } else if prop.initializer.is_some()
                && let Some(type_text) =
                    self.class_property_function_initializer_type_text(prop_idx, prop.initializer)
            {
                self.write(": ");
                self.write(&type_text);
            } else if let Some(type_id) = self.get_node_type_or_names(&[prop_idx, prop.name]) {
                // For readonly properties with literal types, use `= value` form
                // (same as const declarations in tsc)
                if is_readonly
                    && !is_abstract
                    && !prop.question_token
                    && let Some(interner) = self.type_interner
                    && let Some(lit) = tsz_solver::visitor::literal_value(interner, type_id)
                {
                    self.write(" = ");
                    self.write(&Self::format_literal_initializer(&lit, interner));
                } else if is_readonly
                    && !is_abstract
                    && !prop.question_token
                    && prop.initializer.is_some()
                    && let Some(lit_text) =
                        self.const_literal_initializer_text_deep(prop.initializer)
                {
                    // The type system widened the literal (e.g., `false` → `boolean`),
                    // but for readonly properties tsc preserves the `= value` form
                    // when the initializer is a simple literal.
                    self.write(" = ");
                    self.write(&lit_text);
                } else if let Some(typeof_text) = self.typeof_prefix_for_value_entity(
                    prop.initializer,
                    prop.initializer.is_some(),
                    Some(type_id),
                ) {
                    self.write(": ");
                    self.write(&typeof_text);
                    if prop.question_token
                        && self.strict_null_checks
                        && !typeof_text.ends_with("| undefined")
                    {
                        self.write(" | undefined");
                    }
                } else {
                    // For non-readonly properties without an explicit type annotation,
                    // widen literal types to their base types (e.g., `12` → `number`,
                    // `false` → `boolean`) matching tsc's DTS behaviour.
                    let effective_type = if !is_readonly {
                        self.type_interner
                            .map(|interner| {
                                tsz_solver::operations::widening::widen_literal_type(
                                    interner, type_id,
                                )
                            })
                            .unwrap_or(type_id)
                    } else {
                        type_id
                    };
                    let type_text = self
                        .rewrite_recursive_static_class_expression_type(prop_idx, effective_type);
                    let has_object_literal_initializer =
                        self.arena.get(prop.initializer).is_some_and(|node| {
                            node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        });
                    let type_text = if has_object_literal_initializer
                        && (type_text == "any"
                            || type_text.contains(": any;")
                            || self.object_literal_prefers_syntax_type_text(prop.initializer))
                    {
                        self.allowlisted_initializer_type_text(prop.initializer)
                            .unwrap_or(type_text)
                    } else {
                        type_text
                    };
                    let mut emitted_any_for_truncation = false;
                    if let Some(name_node) = self.arena.get(prop.name)
                        && let Some(file_path) = self.current_file_path.clone()
                    {
                        if self.emit_serialized_type_text_truncation_diagnostic_if_needed(
                            &type_text,
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        ) {
                            self.write(": any");
                            emitted_any_for_truncation = true;
                        }
                        if !emitted_any_for_truncation {
                            if self.emit_non_serializable_local_alias_diagnostic(
                                &type_text,
                                &file_path,
                                name_node.pos,
                                name_node.end - name_node.pos,
                            ) {
                                self.write(": any");
                                emitted_any_for_truncation = true;
                            }
                        }
                        if !emitted_any_for_truncation {
                            let _ = self.emit_non_serializable_import_type_diagnostic(
                                &type_text,
                                &file_path,
                                name_node.pos,
                                name_node.end - name_node.pos,
                            );
                        }
                    }
                    if emitted_any_for_truncation {
                    } else if self.printed_type_uses_private_import_type_root(&type_text)
                        && !self.isolated_declarations
                    {
                        if let (Some(file_path), Some((pos, length))) =
                            (self.current_file_path.as_deref(), prop_name_span)
                        {
                            self.diagnostics
                                .push(tsz_common::diagnostics::Diagnostic::from_code(
                                    7056,
                                    file_path,
                                    pos,
                                    length,
                                    &[],
                                ));
                        }
                        self.write(": any");
                    } else {
                        self.write(": ");
                        self.write(&type_text);
                    }
                    // For optional class properties without an explicit type annotation,
                    // tsc appends `| undefined` when the inferred type doesn't already
                    // include it (e.g., `c? = 2` → `c?: number | undefined`).
                    if prop.question_token
                        && self.strict_null_checks
                        && !type_text.ends_with("| undefined")
                    {
                        self.write(" | undefined");
                    }
                }
            } else if is_readonly
                && !is_abstract
                && !prop.question_token
                && prop.initializer.is_some()
                && let Some(lit_text) = self.const_literal_initializer_text_deep(prop.initializer)
            {
                // For readonly properties with simple literal initializers,
                // emit `= value` form (matching tsc's const-like literal
                // preservation for `static readonly` and `readonly` properties).
                self.write(" = ");
                self.write(&lit_text);
            } else if prop.initializer.is_some()
                && let Some(type_text) = self.allowlisted_initializer_type_text(prop.initializer)
            {
                let emitted_any_for_truncation = if let (Some(file_path), Some((pos, length))) =
                    (self.current_file_path.clone(), prop_name_span)
                {
                    if self.emit_serialized_type_text_truncation_diagnostic_if_needed(
                        &type_text, &file_path, pos, length,
                    ) {
                        self.write(": any");
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                if emitted_any_for_truncation {
                } else if self.printed_type_uses_private_import_type_root(&type_text)
                    && !self.isolated_declarations
                {
                    if let (Some(file_path), Some((pos, length))) =
                        (self.current_file_path.as_deref(), prop_name_span)
                    {
                        self.diagnostics
                            .push(tsz_common::diagnostics::Diagnostic::from_code(
                                7056,
                                file_path,
                                pos,
                                length,
                                &[],
                            ));
                    }
                    self.write(": any");
                } else {
                    self.write(": ");
                    self.write(&type_text);
                }
                // Same `| undefined` rule for fallback-inferred types on optional
                // class properties.
                if prop.question_token
                    && self.strict_null_checks
                    && !type_text.ends_with("| undefined")
                {
                    self.write(" | undefined");
                }
            }
        }

        self.write(";");
        if !prop.initializer.is_some() {
            self.emit_trailing_comment(prop_node_end);
        }
        self.write_line();
    }
}
