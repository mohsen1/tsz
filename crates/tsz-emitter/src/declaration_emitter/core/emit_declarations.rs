use crate::enums::evaluator::EnumEvaluator;
use crate::output::source_writer::{SourcePosition, SourceWriter};
use crate::type_cache_view::TypeCacheView;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tracing::debug;
use tsz_binder::{BinderState, SymbolId};
use tsz_common::comments::CommentRange;
use tsz_common::diagnostics::Diagnostic;
use tsz_parser::parser::node::{MethodDeclData, Node, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeInterner;
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
                    &self.import_name_map,
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
        let (js_named_export_names, folded_named_exports, deferred_named_exports) =
            self.collect_js_folded_named_exports(source_file);
        self.js_named_export_names = js_named_export_names;
        self.js_folded_named_export_statements = folded_named_exports;
        self.js_deferred_named_export_statements = deferred_named_exports;
        self.js_export_equals_names = self.collect_js_export_equals_names(source_file);
        self.emitted_js_export_equals_names.clear();
        let (
            js_commonjs_named_export_names,
            js_commonjs_named_function_exports,
            js_commonjs_named_value_exports,
        ) = self.collect_js_commonjs_named_exports(source_file);
        self.js_named_export_names
            .extend(js_commonjs_named_export_names);
        let (module_exports_obj_names, module_exports_obj_stmts) =
            self.collect_js_module_exports_object_names(source_file);
        self.js_named_export_names.extend(module_exports_obj_names);
        self.js_module_exports_object_stmts = module_exports_obj_stmts;
        let (cjs_aliases, cjs_alias_stmts) = self.collect_js_cjs_export_aliases(source_file);
        self.js_cjs_export_aliases = cjs_aliases;
        self.js_cjs_export_alias_statements = cjs_alias_stmts;
        // Mark CJS alias local names as used so they survive usage analysis pruning.
        if let Some(ref binder) = self.binder {
            if let Some(ref mut used) = self.used_symbols {
                for (_export_name, local_name) in &self.js_cjs_export_aliases {
                    if let Some(sym_id) = binder.file_locals.get(local_name) {
                        used.entry(sym_id).or_insert(
                            crate::declaration_emitter::usage_analyzer::UsageKind::VALUE
                                | crate::declaration_emitter::usage_analyzer::UsageKind::TYPE,
                        );
                    }
                }
            }
        }
        self.js_namespace_export_aliases =
            self.collect_js_namespace_export_aliases(source_file, &self.js_export_equals_names);
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

        // Emit CJS export aliases before declarations.
        self.emit_js_cjs_export_aliases();

        for &stmt_idx in &source_file.statements.nodes {
            if deferred_js_namespace_objects.contains(&stmt_idx) {
                continue;
            }
            if self.js_cjs_export_alias_statements.contains(&stmt_idx) {
                continue;
            }
            if self.js_module_exports_object_stmts.contains(&stmt_idx) {
                continue;
            }
            self.emit_statement(stmt_idx);
        }
        for &stmt_idx in &source_file.statements.nodes {
            if deferred_js_namespace_objects.contains(&stmt_idx) {
                self.emit_statement(stmt_idx);
            }
        }

        self.emit_pending_top_level_jsdoc_type_aliases(source_file);
        self.emit_pending_jsdoc_callback_type_aliases(source_file);
        self.emit_trailing_top_level_jsdoc_type_aliases(source_file);

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

        self.writer.get_output().to_string()
    }

    /// Emits detached copyright comments (`/*! ... */`) at the top of the .d.ts file.
    ///
    /// TSC preserves `/*!` comments (copyright notices) at the very start of the file
    /// in declaration output, even when `--removeComments` is set.
    pub(in crate::declaration_emitter) fn emit_detached_copyright_comments(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) {
        // Find the position of the first statement
        let first_stmt_pos = source_file
            .statements
            .nodes
            .first()
            .and_then(|&idx| self.arena.get(idx))
            .map(|n| n.pos);

        for comment in &source_file.comments {
            // Only consider comments that appear before the first statement
            if let Some(stmt_pos) = first_stmt_pos
                && comment.pos >= stmt_pos
            {
                break;
            }

            // Only preserve /*! ... */ copyright comments
            if !comment.is_multi_line {
                continue;
            }
            let text = comment.get_text(&source_file.text);
            if !text.starts_with("/*!") {
                continue;
            }

            self.write(text);
            self.write_line();
        }
    }

    /// Emits triple-slash directives at the top of the .d.ts file.
    ///
    /// TypeScript uses triple-slash directives for:
    /// - File references: `/// <reference path="other.ts" />`
    /// - Type references: `/// <reference types="node" />`
    /// - Lib references: `/// <reference lib="es2015" />`
    /// - AMD directives: `/// <amd-module />`, `/// <amd-dependency />`
    ///
    /// These must appear at the very top of the file, before any imports or declarations.
    pub(in crate::declaration_emitter) fn emit_triple_slash_directives(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) {
        for comment in &source_file.comments {
            let text = &source_file.text[comment.pos as usize..comment.end as usize];

            // Triple-slash directives start with ///
            if let Some(stripped) = text.strip_prefix("///") {
                let trimmed = stripped.trim_start();

                // Preserve `<amd-module>` and `<amd-dependency>` directives.
                // Also preserve `<reference>` directives that have `preserve="true"`.
                let should_emit = trimmed.starts_with("<amd-module")
                    || trimmed.starts_with("<amd-dependency")
                    || (trimmed.starts_with("<reference") && trimmed.contains("preserve=\"true\""));

                if should_emit {
                    // Normalize spacing to match tsc:
                    // 1. Ensure space after `///`: `///<reference` → `/// <reference`
                    // 2. Ensure space before `/>`: `/>` → ` />`
                    let mut normalized = if !stripped.starts_with(' ') {
                        format!("/// {}", stripped.trim_start())
                    } else {
                        text.to_string()
                    };
                    if normalized.ends_with("/>") && !normalized.ends_with(" />") {
                        let base = &normalized[..normalized.len() - 2];
                        normalized = format!("{base} />");
                    }
                    self.write(&normalized);
                    self.write_line();
                }
            }
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

        // Save position before JSDoc comments so we can undo them if the
        // declaration turns out to be invisible (non-exported in namespace, etc.)
        let before_jsdoc_len = self.writer.len();
        let saved_comment_idx = self.comment_emit_idx;
        self.emit_leading_jsdoc_comments(stmt_node.pos);
        let before_len = self.writer.len();
        self.queue_source_mapping(stmt_node);

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

        if !is_exported
            && !self.should_emit_public_api_member(&func.modifiers)
            && !self.should_emit_public_api_dependency(func.name)
        {
            return;
        }
        if self.should_skip_ns_internal_member(&func.modifiers, Some(func_idx)) {
            return;
        }

        // Get function name as string for overload tracking
        let function_name = self.get_function_name(func_idx);

        // Check if this is an overload (no body) or implementation (has body)
        let is_overload = func.body.is_none();
        let is_implementation = !is_overload;

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
                        .unwrap_or(return_type_id)
                } else {
                    return_type_id
                };
                // If solver returned `any` but the function body clearly returns void,
                // prefer void (the solver's `any` is a fallback, not an actual inference)
                if effective_return_type_id == tsz_solver::types::TypeId::ANY
                    && func_body.is_some()
                    && self.body_returns_void(func_body)
                {
                    self.write(": void");
                } else if func_body.is_some()
                    && let Some(type_text) =
                        self.function_body_preferred_return_type_text(func_body)
                {
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
                    self.write(": ");
                    self.write(&self.print_type_id(effective_return_type_id));
                }
            } else if func_body.is_some() {
                if self.body_returns_void(func_body) {
                    self.write(": void");
                } else if let Some(return_text) =
                    self.function_body_preferred_return_type_text(func_body)
                {
                    self.write(": ");
                    self.write(&return_text);
                }
            }
        } else if func_body.is_some() {
            // No type cache available, but we can infer from the body
            if self.body_returns_void(func_body) {
                self.write(": void");
            } else if let Some(return_text) =
                self.function_body_preferred_return_type_text(func_body)
            {
                self.write(": ");
                self.write(&return_text);
            }
        }

        self.write(";");
        self.write_line();
        self.emit_js_synthetic_prototype_class_if_needed(func.name, is_exported);
        self.emit_js_namespace_export_aliases_for_name(func.name);

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
            && !self.should_emit_public_api_dependency(class.name)
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

        self.emit_pending_js_export_equals_for_name(class.name);
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
        self.emit_node(class.name);

        // Type parameters
        if let Some(ref type_params) = class.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
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

        // Members
        for &member_idx in &class.members.nodes {
            let before_jsdoc_len = self.writer.len();
            let saved_comment_idx = self.comment_emit_idx;
            if let Some(mn) = self.arena.get(member_idx) {
                self.emit_leading_jsdoc_comments(mn.pos);
            }
            let before_member_len = self.writer.len();
            self.emit_class_member(member_idx);
            if self.writer.len() == before_member_len {
                // Member didn't emit anything (e.g., skipped implementation overload).
                // Rollback the speculatively emitted JSDoc comments.
                self.writer.truncate(before_jsdoc_len);
                self.comment_emit_idx = saved_comment_idx;
                if let Some(mn) = self.arena.get(member_idx) {
                    self.skip_comments_in_node(mn.pos, mn.end);
                }
            }
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
        self.emit_js_namespace_export_aliases_for_name(class.name);
    }
    /// Pre-scan class members: when a computed property name appears on both
    /// a method implementation and a get/set accessor, tsc suppresses the
    /// method in the .d.ts output (the accessor wins). This returns the set
    /// of computed name texts that should be treated as "already declared"
    /// so the method implementation is skipped.
    pub(in crate::declaration_emitter) fn computed_names_shadowed_by_accessors(
        &self,
        members: &tsz_parser::parser::NodeList,
    ) -> rustc_hash::FxHashSet<String> {
        let mut accessor_names: rustc_hash::FxHashSet<String> = rustc_hash::FxHashSet::default();
        let mut method_impl_names: Vec<String> = Vec::new();
        for &m in &members.nodes {
            let Some(mn) = self.arena.get(m) else {
                continue;
            };
            let is_accessor = mn.kind == syntax_kind_ext::GET_ACCESSOR
                || mn.kind == syntax_kind_ext::SET_ACCESSOR;
            let is_method = mn.kind == syntax_kind_ext::METHOD_DECLARATION;
            if !is_accessor && !is_method {
                continue;
            }
            let name_idx = if is_accessor {
                self.arena.get_accessor(mn).map(|a| a.name)
            } else {
                self.arena.get_method_decl(mn).map(|md| md.name)
            };
            let Some(name_idx) = name_idx else {
                continue;
            };
            let Some(name_node) = self.arena.get(name_idx) else {
                continue;
            };
            if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                continue;
            }
            let Some(text) = self.get_source_slice(name_node.pos, name_node.end) else {
                continue;
            };
            if is_accessor {
                accessor_names.insert(text);
            } else if self
                .arena
                .get_method_decl(mn)
                .is_some_and(|md| md.body.is_some())
            {
                method_impl_names.push(text);
            }
        }
        let mut result = rustc_hash::FxHashSet::default();
        for name in method_impl_names {
            if accessor_names.contains(&name) {
                result.insert(name);
            }
        }
        result
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
        let Some(member_node) = self.arena.get(member_idx) else {
            return false;
        };
        let name_idx = if let Some(prop) = self.arena.get_property_decl(member_node) {
            Some(prop.name)
        } else if let Some(method) = self.arena.get_method_decl(member_node) {
            Some(method.name)
        } else {
            self.arena
                .get_accessor(member_node)
                .map(|accessor| accessor.name)
        };
        if let Some(name_idx) = name_idx
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

        // Modifiers
        self.emit_member_modifiers(&prop.modifiers);

        // Name
        self.emit_node(prop.name);

        // Optional marker
        if prop.question_token {
            self.write("?");
        }

        // Check if readonly for literal initializer form
        let is_readonly = self
            .arena
            .has_modifier(&prop.modifiers, SyntaxKind::ReadonlyKeyword);
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
                && let Some(type_text) = self.infer_fallback_type_text(prop.initializer)
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

    pub(in crate::declaration_emitter) fn rewrite_recursive_static_class_expression_type(
        &self,
        prop_idx: NodeIndex,
        type_id: tsz_solver::types::TypeId,
    ) -> String {
        let printed = self.print_type_id(type_id);
        let Some(prop_node) = self.arena.get(prop_idx) else {
            return printed;
        };
        let Some(prop) = self.arena.get_property_decl(prop_node) else {
            return printed;
        };
        let Some(property_name) = self
            .arena
            .get_identifier_at(prop.name)
            .map(|ident| ident.escaped_text.clone())
        else {
            return printed;
        };
        if !self.property_initializer_is_recursive_class_expression(prop_idx, prop.initializer) {
            return printed;
        }
        let Some(interner) = self.type_interner else {
            return printed;
        };
        let Some(callable) = type_queries::get_callable_shape(interner, type_id) else {
            return printed;
        };
        if !callable.properties.iter().any(|prop| {
            interner.resolve_atom(prop.name) == property_name
                && prop.type_id == tsz_solver::TypeId::ANY
        }) {
            return printed;
        }

        printed.replacen(
            &format!("{property_name}: any;"),
            &format!("{property_name}: /*elided*/ any;"),
            1,
        )
    }

    pub(in crate::declaration_emitter) fn property_initializer_is_recursive_class_expression(
        &self,
        prop_idx: NodeIndex,
        initializer_idx: NodeIndex,
    ) -> bool {
        let Some(class_expr) = self.arena.get_class_at(initializer_idx) else {
            return false;
        };
        let Some(enclosing_class_idx) = self
            .arena
            .get_extended(prop_idx)
            .map(|extended| extended.parent)
            .filter(|parent| {
                self.arena
                    .get(*parent)
                    .is_some_and(|node| node.kind == syntax_kind_ext::CLASS_DECLARATION)
            })
        else {
            return false;
        };
        let Some(enclosing_class_name) = self
            .arena
            .get_class_at(enclosing_class_idx)
            .and_then(|class| self.arena.get_identifier_at(class.name))
            .map(|ident| ident.escaped_text.clone())
        else {
            return false;
        };
        let Some(heritage_clauses) = class_expr.heritage_clauses.as_ref() else {
            return false;
        };

        heritage_clauses.nodes.iter().copied().any(|clause_idx| {
            self.arena
                .get_heritage_clause_at(clause_idx)
                .filter(|heritage| heritage.token == SyntaxKind::ExtendsKeyword as u16)
                .and_then(|heritage| heritage.types.nodes.first().copied())
                .map(|type_idx| {
                    self.arena
                        .get_expr_type_args_at(type_idx)
                        .map_or(type_idx, |expr_type_args| expr_type_args.expression)
                })
                .and_then(|expr_idx| self.arena.get_identifier_at(expr_idx))
                .is_some_and(|ident| ident.escaped_text == enclosing_class_name)
        })
    }
}
