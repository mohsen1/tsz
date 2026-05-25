use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::DeclarationEmitter;

impl<'a> DeclarationEmitter<'a> {
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
            if stmt_node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
                self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            }
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
            .js_deferred_namespace_alias_declaration_stmts
            .contains(&stmt_idx)
        {
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
        if !self.emitting_js_default_export_declaration
            && self.js_default_export_declaration_should_defer_until_export(stmt_idx)
        {
            self.skip_comments_before_raw(stmt_node.pos);
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
        let has_effective_export = self.statement_has_effective_export(stmt_idx);
        let has_jsdoc_type_function_signature = self
            .statement_jsdoc_type_function_signature_node(stmt_idx)
            .is_some();
        let js_export_equals_declaration_name = match kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => self
                .arena
                .get_function(stmt_node)
                .map(|func| func.name)
                .filter(|&name| self.is_js_export_equals_name(name)),
            k if k == syntax_kind_ext::CLASS_DECLARATION => self
                .arena
                .get_class(stmt_node)
                .map(|class| class.name)
                .filter(|&name| self.is_js_export_equals_name(name)),
            _ => None,
        };
        if let Some(name) = js_export_equals_declaration_name {
            self.emit_pending_js_export_equals_for_name(name);
        } else if has_effective_export
            && !is_variable_like_export
            && !has_jsdoc_type_function_signature
        {
            self.emit_leading_jsdoc_type_aliases_for_pos(stmt_node.pos, has_effective_export);
        }
        if kind == syntax_kind_ext::VARIABLE_STATEMENT
            && let Some(var_stmt) = self.arena.get_variable(stmt_node)
        {
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
                    self.emit_pending_js_export_equals_for_name(decl.name);
                }
            }
        }
        if kind == syntax_kind_ext::CLASS_DECLARATION
            && let Some(class) = self.arena.get_class(stmt_node)
            && self.is_js_export_equals_name(class.name)
            && self
                .get_identifier_text(class.name)
                .and_then(|name| self.js_shadowed_export_equals_local_alias(&name))
                .is_none()
        {
            self.emit_pending_js_export_equals_for_name(class.name);
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
        // True when emit_leading_jsdoc_type_aliases_for_pos was called above, so the raw
        // @typedef comment blocks should be suppressed from the JSDoc chain.
        let emitted_leading_typedef_aliases = self.source_is_js_file
            && has_effective_export
            && !is_variable_like_export
            && js_export_equals_declaration_name.is_none()
            && !has_jsdoc_type_function_signature;
        let suppress_jsdoc_type_alias_comments = has_jsdoc_type_alias
            && (self.statement_emits_js_object_literal_namespace(stmt_idx)
                || emitted_leading_typedef_aliases);
        let jsdoc_overload_function_node =
            self.jsdoc_overload_function_node_for_statement(stmt_idx);
        let has_jsdoc_overload_signatures = jsdoc_overload_function_node
            .is_some_and(|func_idx| !self.jsdoc_overload_signatures_for_node(func_idx).is_empty());
        let should_join_single_line_jsdoc_type_comment = self.source_is_js_file
            && kind == syntax_kind_ext::VARIABLE_STATEMENT
            && !self.inside_declare_namespace
            && self.emitted_leading_single_line_jsdoc_type_comment_for_pos(stmt_node.pos)
            && self
                .jsdoc_type_text_for_node(stmt_idx)
                .is_none_or(|type_text| {
                    !self
                        .jsdoc_type_text_for_declaration_emit(&type_text)
                        .contains('\n')
                });
        if has_jsdoc_overload_signatures {
            // JSDoc overload comments are emitted once per structured signature
            // by `emit_function_declaration`.
        } else if has_jsdoc_type_function_signature || has_jsdoc_type_alias {
            self.emit_leading_jsdoc_comments(stmt_node.pos);
            self.writer.truncate(before_jsdoc_len);
            let mut filtered = if has_jsdoc_type_function_signature {
                Self::jsdoc_chain_without_type_or_alias_tags(&self.current_statement_jsdoc_chain)
            } else {
                self.current_statement_jsdoc_chain.clone()
            };
            if suppress_jsdoc_type_alias_comments {
                filtered.retain(|jsdoc| !Self::jsdoc_contains_type_alias_tag(jsdoc));
            }
            if has_jsdoc_type_function_signature {
                if !self.emit_jsdoc_comment_chain_preserving_source_for_pos_verbatim(
                    stmt_node.pos,
                    &filtered,
                ) {
                    self.emit_jsdoc_comment_chain(&filtered);
                }
            } else if !self
                .emit_jsdoc_comment_chain_preserving_source_for_pos(stmt_node.pos, &filtered)
            {
                self.emit_jsdoc_comment_chain(&filtered);
            }
        } else {
            self.emit_leading_jsdoc_comments(stmt_node.pos);
        }
        if should_join_single_line_jsdoc_type_comment {
            self.join_last_emitted_jsdoc_comment_to_next_declaration();
        }
        let before_len = self.writer.len();
        self.queue_source_mapping(stmt_node);
        self.suppress_current_statement_jsdoc_comments = false;

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
            if has_jsdoc_overload_signatures {
                self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            }
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
}
