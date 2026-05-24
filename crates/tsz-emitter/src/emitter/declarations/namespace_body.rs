//! Namespace body emission helpers split out of `namespace.rs` for file-size hygiene.

use super::super::Printer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    /// Emit body statements of a namespace IIFE, handling exports.
    pub(in crate::emitter) fn emit_namespace_body_statements(
        &mut self,
        module: &tsz_parser::parser::node::ModuleData,
        ns_name: &str,
    ) {
        let ns_name = ns_name.to_string();
        if let Some(body_node) = self.arena.get(module.body)
            && let Some(block) = self.arena.get_module_block(body_node)
            && let Some(ref stmts) = block.statements
        {
            // Find the closing brace position of the body block.
            // This is used to constrain trailing comment search for the last statement
            // so that comments on the closing `}` line are not attributed to inner statements.
            let body_close_pos = self.find_token_end_before_trivia(body_node.pos, body_node.end);
            // Collect exported names for identifier qualification in emit_identifier
            let prev_exported = std::mem::take(&mut self.namespace_exported_names);
            let prev_parent_exported = std::mem::take(&mut self.namespace_parent_exported_names);
            let prev_ancestor_qualifiers =
                std::mem::take(&mut self.namespace_ancestor_export_qualifiers);
            let prev_current_class_fn_enum =
                std::mem::take(&mut self.namespace_current_class_fn_enum_names);
            let mut local_exports = self.collect_namespace_exported_names(module);
            if let Some(source_path) = self.current_namespace_source_path.as_ref()
                && let Some(exports) = self.namespace_all_exported_names.get(source_path)
            {
                local_exports.extend(exports.iter().cloned());
            }
            let leaf_name = self.get_identifier_text_idx(module.name);
            if !leaf_name.is_empty() {
                local_exports
                    .extend(self.collect_dotted_namespace_children_from_source(&leaf_name));
                local_exports
                    .extend(self.collect_namespace_exported_value_members_from_source(&leaf_name));
            }
            let mut ancestor_qualifiers = prev_ancestor_qualifiers.clone();
            let mut parent_exports = self
                .parent_namespace_name
                .as_ref()
                .and_then(|parent| {
                    self.namespace_prior_exports.get(parent).or_else(|| {
                        self.namespace_prior_exports
                            .get(Self::namespace_iife_param_base(parent))
                    })
                })
                .map(|exports| {
                    exports
                        .iter()
                        .cloned()
                        .collect::<rustc_hash::FxHashSet<_>>()
                })
                .unwrap_or_default();
            if let Some(parent) = self.parent_namespace_name.as_ref() {
                let parent_base = Self::namespace_iife_param_base(parent);
                if parent_base != parent {
                    for name in
                        self.collect_namespace_exported_value_members_from_source(parent_base)
                    {
                        parent_exports.insert(name.clone());
                        ancestor_qualifiers.insert(name, parent.clone());
                    }
                }
            }
            if let Some(parent) = self.parent_namespace_name.as_ref()
                && let Some(exports) = self.namespace_prior_exports.get(parent).or_else(|| {
                    self.namespace_prior_exports
                        .get(Self::namespace_iife_param_base(parent))
                })
            {
                for name in exports {
                    ancestor_qualifiers.insert(name.clone(), parent.clone());
                }
            }
            // Also merge class/fn/enum names from PRIOR blocks of the parent
            // namespace. These names live on the parent namespace object once
            // the prior IIFE has exited, so a nested namespace inside a
            // reopened parent block must qualify them as `parent.Name`.
            // Note: this map is populated AFTER the statement iteration loop
            // completes (see end of this function), so during nested-namespace
            // emission within the parent's current block, this entry only
            // contains names from genuinely PRIOR blocks — not the parent's
            // current-block class/fn/enum names (which remain in lexical
            // scope as IIFE locals).
            if let Some(parent) = self.parent_namespace_name.as_ref()
                && let Some(class_exports) = self
                    .namespace_prior_class_fn_enum_exports
                    .get(parent)
                    .or_else(|| {
                        self.namespace_prior_class_fn_enum_exports
                            .get(Self::namespace_iife_param_base(parent))
                    })
            {
                for name in class_exports.iter() {
                    parent_exports.insert(name.clone());
                    ancestor_qualifiers.insert(name.clone(), parent.clone());
                }
            }
            parent_exports.remove(&leaf_name);
            ancestor_qualifiers.remove(&leaf_name);
            // Collect class/function/enum names for future reopenings (before mutable borrow)
            let class_fn_enum_names = self.collect_namespace_class_fn_enum_names(module);
            self.namespace_current_class_fn_enum_names = self
                .collect_namespace_current_class_fn_enum_names(module)
                .into_iter()
                .collect();
            for name in &self.namespace_current_class_fn_enum_names {
                local_exports.remove(name);
                parent_exports.remove(name);
                ancestor_qualifiers.remove(name);
            }
            if let Some(source_path) = self.current_namespace_source_path.as_ref()
                && let Some((parent_source_path, _)) = source_path.rsplit_once('.')
                && let Some(parent_qualifier) = self.parent_namespace_name.as_ref()
                && let Some(exports) = self.namespace_all_exported_names.get(parent_source_path)
            {
                parent_exports.extend(exports.iter().cloned());
                for name in exports {
                    ancestor_qualifiers.insert(name.clone(), parent_qualifier.clone());
                }
                parent_exports.remove(&leaf_name);
                ancestor_qualifiers.remove(&leaf_name);
            }
            for name in &prev_current_class_fn_enum {
                parent_exports.remove(name);
                ancestor_qualifiers.remove(name);
            }
            // Merge prior same-scope namespace exports for reopened blocks.
            let class_fn_enum_root_name = if let Some(ref parent) = self.parent_namespace_name {
                format!("{parent}.{leaf_name}")
            } else {
                leaf_name.clone()
            };
            if !leaf_name.is_empty() {
                let entry = self
                    .namespace_prior_exports
                    .entry(class_fn_enum_root_name.clone())
                    .or_default();
                for name in entry.iter() {
                    local_exports.insert(name.clone());
                }
                entry.extend(
                    local_exports
                        .iter()
                        .filter(|name| !class_fn_enum_names.contains(*name))
                        .cloned(),
                );
                if ns_name != leaf_name {
                    self.namespace_prior_exports
                        .entry(ns_name.clone())
                        .or_default()
                        .extend(
                            local_exports
                                .iter()
                                .filter(|name| !class_fn_enum_names.contains(*name))
                                .cloned(),
                        );
                }

                // Prior class/function/enum exports qualify in reopened blocks;
                // this block's own declarations are recorded only after emission.
                let class_entry = self
                    .namespace_prior_class_fn_enum_exports
                    .entry(class_fn_enum_root_name.clone())
                    .or_default();
                for name in class_entry.iter() {
                    local_exports.insert(name.clone());
                }
            }
            // Remove locally-declared non-exported names — they shadow prior exports
            let local_names = self.collect_namespace_local_var_names(body_node);
            let local_var_shadow_names =
                self.collect_namespace_non_exported_local_var_names(body_node);
            for name in &local_names {
                local_exports.remove(name);
                parent_exports.remove(name);
                ancestor_qualifiers.remove(name);
            }
            for name in self.collect_namespace_local_module_names(body_node) {
                local_exports.remove(&name);
                parent_exports.remove(&name);
                ancestor_qualifiers.remove(&name);
            }
            self.namespace_exported_names = local_exports;
            self.namespace_parent_exported_names = parent_exports;
            self.namespace_ancestor_export_qualifiers = ancestor_qualifiers;
            self.namespace_local_var_shadow_stack
                .push(local_var_shadow_names);
            let (destructuring_export_temps, destructuring_export_temp_names) =
                self.reserve_namespace_destructuring_export_temps(module);

            // Skip comments on the same line as the opening `{` of the module block.
            // When the namespace is transformed to an IIFE, tsc drops trailing
            // comments on the opening brace (e.g., `namespace _this { //Error`
            // becomes `(function (_this) {` without `//Error`).
            // Only skip comments on the `{` line — comments on subsequent lines
            // (e.g., JSDoc before the first statement) must be preserved.
            if let Some(text) = self.source_text {
                let bytes = text.as_bytes();
                let brace_pos = body_node.pos as usize;
                // Find the end of the `{` line
                let mut brace_line_end = brace_pos;
                while brace_line_end < bytes.len()
                    && bytes[brace_line_end] != b'\n'
                    && bytes[brace_line_end] != b'\r'
                {
                    brace_line_end += 1;
                }
                // Only skip comments that start on the `{` line AND before the first
                // statement. Comments after `}` on the same line (single-line namespaces)
                // should not be skipped.
                let first_stmt_pos = stmts
                    .nodes
                    .first()
                    .and_then(|&idx| self.arena.get(idx))
                    .map_or(body_close_pos, |n| n.pos);
                let skip_boundary = std::cmp::min(brace_line_end as u32, first_stmt_pos);
                while self.comment_emit_idx < self.all_comments.len() {
                    let c_pos = self.all_comments[self.comment_emit_idx].pos;
                    if c_pos < skip_boundary {
                        self.comment_emit_idx += 1;
                    } else {
                        break;
                    }
                }
            }

            if !destructuring_export_temp_names.is_empty() {
                self.write("var ");
                for (i, temp) in destructuring_export_temp_names.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.write(temp);
                }
                self.write(";");
                self.write_line();
            }

            let has_using_declarations = self.block_has_using_declarations(stmts)
                || self
                    .reserved_disposable_env_names
                    .contains_key(&module.body)
                || self.namespace_body_has_using_declarations(module.body);
            let namespace_using_region =
                if !self.ctx.options.target.supports_es2025() && has_using_declarations {
                    let using_async = self.block_has_await_using(stmts);
                    let (env_name, error_name, result_name) =
                        self.disposable_env_names_for_node(module.body);
                    let env_decl_keyword = if self.ctx.target_es5 { "var" } else { "const" };
                    let prev_block_using_env = self
                        .block_using_env
                        .replace((env_name.clone(), using_async));

                    self.write(env_decl_keyword);
                    self.write(" ");
                    self.write(&env_name);
                    self.write(" = { stack: [], error: void 0, hasError: false };");
                    self.write_line();
                    self.write("try {");
                    self.write_line();
                    self.increase_indent();

                    Some((
                        env_name,
                        error_name,
                        result_name,
                        using_async,
                        prev_block_using_env,
                    ))
                } else {
                    None
                };

            for (stmt_i, &stmt_idx) in stmts.nodes.iter().enumerate() {
                let Some(stmt_node) = self.arena.get(stmt_idx) else {
                    continue;
                };

                // Skip erased declarations (type-only, ambient, etc.) and their comments
                if self.is_erased_statement(stmt_node) {
                    self.skip_comments_for_erased_node(stmt_node);
                    continue;
                }

                // Also handle export wrapping an erased declaration
                if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                    && let Some(export) = self.arena.get_export_decl(stmt_node)
                    && let Some(inner_node) = self.arena.get(export.export_clause)
                    && self.is_erased_statement(inner_node)
                {
                    self.skip_comments_for_erased_node(stmt_node);
                    continue;
                }

                // Skip `export * from ...` re-exports inside namespaces.
                // This syntax is invalid in namespace scope (only valid at
                // module level) and tsc erases it.
                if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                    && let Some(export) = self.arena.get_export_decl(stmt_node)
                    && export.export_clause.is_none()
                    && export.module_specifier.is_some()
                {
                    self.skip_comments_for_erased_node(stmt_node);
                    continue;
                }

                // Skip exported variable statements where all declarations have no
                // initializer (e.g., `export var b: number;`).  These emit no code, so
                // their leading JSDoc comment must be suppressed rather than orphaned.
                if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                    && let Some(export) = self.arena.get_export_decl(stmt_node)
                    && let Some(inner_node) = self.arena.get(export.export_clause)
                    && inner_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                    && self.namespace_variable_has_no_initializers(export.export_clause)
                {
                    self.skip_comments_for_erased_node(stmt_node);
                    continue;
                }

                // Compute upper bound for trailing comment scan: use the next statement's
                // position to avoid scanning past the current statement into the next line.
                // For the last statement, use the body's closing brace position to avoid
                // picking up comments that belong on the IIFE closing line.
                let next_pos = stmts
                    .nodes
                    .get(stmt_i + 1)
                    .and_then(|&next_idx| self.arena.get(next_idx))
                    .map(|n| n.pos);
                let upper_bound = next_pos.unwrap_or(body_close_pos);

                // Emit leading comments before this statement.
                // Save state so we can undo if the statement produces no output.
                let pre_comment_writer_len = self.writer.len();
                let pre_comment_idx = self.comment_emit_idx;
                self.emit_comments_before_pos(stmt_node.pos);

                if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                    // Strip "export" and handle inner clause
                    if let Some(export) = self.arena.get_export_decl(stmt_node) {
                        let inner_idx = export.export_clause;
                        let inner_kind = self.arena.get(inner_idx).map_or(0, |n| n.kind);

                        if inner_kind == syntax_kind_ext::VARIABLE_STATEMENT {
                            // export var x = 10; → ns.x = 10;
                            let before_len = self.writer.len();
                            self.emit_namespace_exported_variable(
                                inner_idx,
                                &ns_name,
                                stmt_node,
                                upper_bound,
                                &destructuring_export_temps,
                            );
                            if self.writer.len() == before_len {
                                if self.writer.len() > pre_comment_writer_len {
                                    self.writer.truncate(pre_comment_writer_len);
                                    self.comment_emit_idx = pre_comment_idx;
                                }
                                self.skip_comments_for_erased_node(stmt_node);
                            }
                        } else if inner_kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                            // export import X = Y; → ns.X = Y;
                            self.emit_namespace_exported_import_alias(
                                inner_idx,
                                &ns_name,
                                Some(module.body),
                            );
                        } else if inner_kind == syntax_kind_ext::NAMED_EXPORTS {
                            // export { X as y }; inside a namespace IIFE.
                            // Named re-exports don't produce runtime code in namespace
                            // context — the declarations they reference are already
                            // bound to the namespace via `ns.X = X;` assignments.
                            // tsc elides these entirely.
                        } else if inner_kind == syntax_kind_ext::FUNCTION_DECLARATION
                            && self.emit_recovered_namespace_function_arrow_body(inner_idx)
                        {
                            let inner_upper = next_pos.unwrap_or(body_close_pos);
                            let token_end =
                                self.find_token_end_before_trivia(stmt_node.pos, inner_upper);
                            self.emit_trailing_comments_before(token_end, body_close_pos);
                            if !self.writer.is_at_line_start() {
                                self.write_line();
                            }
                        } else if export.is_default_export
                            && !matches!(
                                inner_kind,
                                k if k == syntax_kind_ext::CLASS_DECLARATION
                                    || k == syntax_kind_ext::FUNCTION_DECLARATION
                                    || k == syntax_kind_ext::ENUM_DECLARATION
                                    || k == syntax_kind_ext::MODULE_DECLARATION
                            )
                        {
                            // Invalid namespace-scope `export default expr`
                            // recovers as an export declaration. Tsc preserves
                            // that syntax verbatim instead of treating it as a
                            // namespace property export.
                            self.write("export default ");
                            self.emit_expression(inner_idx);
                            self.write_semicolon();
                            self.write_line();
                        } else {
                            // class/function/enum: emit without export, then add assignment
                            let recovered_anonymous_default_class_name =
                                if inner_kind == syntax_kind_ext::CLASS_DECLARATION {
                                    self.arena.get_class_at(inner_idx).and_then(|class| {
                                        if class.name.is_none()
                                            && (export.is_default_export
                                                || self.arena.has_modifier(
                                                    &class.modifiers,
                                                    SyntaxKind::DefaultKeyword,
                                                ))
                                        {
                                            Some(self.next_anonymous_default_export_name())
                                        } else {
                                            None
                                        }
                                    })
                                } else {
                                    None
                                };
                            let recovered_anonymous_default_function_name =
                                if inner_kind == syntax_kind_ext::FUNCTION_DECLARATION {
                                    self.arena.get_function_at(inner_idx).and_then(|func| {
                                        if func.name.is_none()
                                            && (export.is_default_export
                                                || self.arena.has_modifier(
                                                    &func.modifiers,
                                                    SyntaxKind::DefaultKeyword,
                                                ))
                                        {
                                            Some(self.next_anonymous_default_export_name())
                                        } else {
                                            None
                                        }
                                    })
                                } else {
                                    None
                                };
                            let export_names = recovered_anonymous_default_class_name
                                .clone()
                                .or_else(|| recovered_anonymous_default_function_name.clone())
                                .map_or_else(
                                    || self.get_export_names_from_clause(inner_idx),
                                    |name| vec![name],
                                );

                            // For exported enums in namespace, fold the export into the
                            // IIFE closing pattern instead of emitting a separate assignment.
                            let is_enum = inner_kind == syntax_kind_ext::ENUM_DECLARATION;
                            if is_enum {
                                self.enum_namespace_export = Some(ns_name.clone());
                            }

                            // For exported namespaces, signal that the IIFE should
                            // use parent assignment (e.g., `m3.m4 || (m3.m4 = {})`).
                            let is_ns = inner_kind == syntax_kind_ext::MODULE_DECLARATION;
                            if is_ns {
                                self.namespace_export_inner = true;
                            }

                            let before_len = self.writer.len();
                            let prev = self.in_namespace_iife;
                            let prev_anonymous_default_export_name =
                                self.anonymous_default_export_name.clone();
                            if let Some(name) = recovered_anonymous_default_class_name.as_ref() {
                                self.anonymous_default_export_name = Some(name.clone());
                            }
                            self.in_namespace_iife = true;
                            if recovered_anonymous_default_function_name.is_some() {
                                self.write("default ");
                            }
                            self.emit(inner_idx);
                            self.in_namespace_iife = prev;
                            if recovered_anonymous_default_class_name.is_some() {
                                self.anonymous_default_export_name =
                                    prev_anonymous_default_export_name;
                            }
                            let emitted = self.writer.len() > before_len;
                            // Emit trailing comments on the same line,
                            // but don't consume comments past the body's closing brace
                            if emitted && let Some(inner_node) = self.arena.get(inner_idx) {
                                let inner_upper = next_pos.unwrap_or(body_close_pos);
                                let token_end =
                                    self.find_token_end_before_trivia(inner_node.pos, inner_upper);
                                self.emit_trailing_comments_before(token_end, body_close_pos);
                            }

                            // If the enum absorbed the namespace export into its IIFE,
                            // skip the separate assignment statement.
                            let skip_export = is_enum && self.enum_namespace_export.is_none();

                            if !export_names.is_empty() && !skip_export {
                                if !self.writer.is_at_line_start() {
                                    self.write_line();
                                }
                                for export_name in &export_names {
                                    self.write(&ns_name);
                                    self.write(".");
                                    self.write(export_name);
                                    self.write(" = ");
                                    self.write(export_name);
                                    self.write(";");
                                    self.write_line();
                                }
                            } else if emitted
                                && inner_kind != syntax_kind_ext::MODULE_DECLARATION
                                && !self.writer.is_at_line_start()
                            {
                                // Don't write extra newline for namespaces - they already call write_line()
                                // Also don't write newline if emit produced nothing (e.g., non-instantiated import alias)
                                // Also skip if already at line start (class with lowered static fields)
                                self.write_line();
                            }
                            // Clean up in case the enum emitter didn't consume it
                            self.enum_namespace_export = None;
                        }
                    }
                } else if stmt_node.kind == syntax_kind_ext::CLASS_DECLARATION {
                    let class_export_name = self.arena.get_class(stmt_node).and_then(|class| {
                        if self
                            .arena
                            .has_modifier(&class.modifiers, SyntaxKind::ExportKeyword)
                        {
                            self.get_identifier_text_opt(class.name).or_else(|| {
                                if self
                                    .arena
                                    .has_modifier(&class.modifiers, SyntaxKind::DefaultKeyword)
                                {
                                    Some(self.next_anonymous_default_export_name())
                                } else {
                                    None
                                }
                            })
                        } else {
                            None
                        }
                    });
                    let prev_anonymous_default_export_name =
                        self.anonymous_default_export_name.clone();
                    if let Some(name) = class_export_name.as_ref()
                        && self
                            .arena
                            .get_class(stmt_node)
                            .is_some_and(|class| class.name.is_none())
                    {
                        self.anonymous_default_export_name = Some(name.clone());
                    }

                    // Class declarations in namespace: emit local binding, then
                    // attach exported classes to the namespace object.
                    let prev = self.in_namespace_iife;
                    self.in_namespace_iife = true;
                    self.emit(stmt_idx);
                    self.in_namespace_iife = prev;
                    if class_export_name.is_some()
                        && self
                            .arena
                            .get_class(stmt_node)
                            .is_some_and(|class| class.name.is_none())
                    {
                        self.anonymous_default_export_name = prev_anonymous_default_export_name;
                    }
                    let token_end = self.find_token_end_before_trivia(stmt_node.pos, upper_bound);
                    self.emit_trailing_comments_before(token_end, body_close_pos);
                    // Only write newline if not already at line start (class
                    // declarations with lowered static fields already end with
                    // write_line after the last ClassName.field = value;).
                    if !self.writer.is_at_line_start() {
                        self.write_line();
                    }
                    if let Some(name) = class_export_name {
                        self.write(&ns_name);
                        self.write(".");
                        self.write(&name);
                        self.write(" = ");
                        self.write(&name);
                        self.write(";");
                        self.write_line();
                    }
                } else if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                    // Nested namespace: recurse (emit_namespace_iife adds its own newline)
                    self.emit(stmt_idx);
                } else if stmt_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                    && self.emit_recovered_namespace_function_arrow_body(stmt_idx)
                {
                    let token_end = self.find_token_end_before_trivia(stmt_node.pos, upper_bound);
                    self.emit_trailing_comments_before(token_end, body_close_pos);
                    if !self.writer.is_at_line_start() {
                        self.write_line();
                    }
                } else {
                    // Regular statement - emit trailing comments on same line,
                    // but don't consume comments past the body's closing brace.
                    // Guard with before_len: some statements (e.g., type-only
                    // import-equals aliases like `import T = M1.I;`) produce no
                    // output but aren't caught by is_erased_statement(). Without
                    // this check, write_line() would emit a phantom blank line.
                    let before_len = self.writer.len();
                    self.emit(stmt_idx);
                    if self.writer.len() > before_len {
                        let token_end =
                            self.find_token_end_before_trivia(stmt_node.pos, upper_bound);
                        self.emit_trailing_comments_before(token_end, body_close_pos);
                        self.write_line();
                    } else {
                        // Statement produced no output — undo any leading comments
                        // emitted at line 600 and skip trailing same-line comments.
                        if self.writer.len() > pre_comment_writer_len {
                            self.writer.truncate(pre_comment_writer_len);
                            self.comment_emit_idx = pre_comment_idx;
                        }
                        self.skip_comments_for_erased_node(stmt_node);
                    }
                }
            }

            if let Some((env_name, error_name, result_name, using_async, prev_block_using_env)) =
                namespace_using_region
            {
                self.decrease_indent();
                self.write("}");
                self.write_line();
                self.write("catch (");
                self.write(&error_name);
                self.write(") {");
                self.write_line();
                self.increase_indent();
                self.write(&env_name);
                self.write(".error = ");
                self.write(&error_name);
                self.write(";");
                self.write_line();
                self.write(&env_name);
                self.write(".hasError = true;");
                self.write_line();
                self.decrease_indent();
                self.write("}");
                self.write_line();
                self.write("finally {");
                self.write_line();
                self.increase_indent();
                if using_async {
                    let await_kw =
                        if self.ctx.emit_await_as_yield || self.ctx.emit_await_as_yield_await {
                            "yield"
                        } else {
                            "await"
                        };
                    self.write(if self.ctx.target_es5 { "var" } else { "const" });
                    self.write(" ");
                    self.write(&result_name);
                    self.write(" = ");
                    self.write_helper("__disposeResources");
                    self.write("(");
                    self.write(&env_name);
                    self.write(");");
                    self.write_line();
                    self.write("if (");
                    self.write(&result_name);
                    self.write(")");
                    self.write_line();
                    self.increase_indent();
                    self.write(await_kw);
                    self.write(" ");
                    if self.ctx.emit_await_as_yield_await {
                        self.write_helper("__await");
                        self.write("(");
                        self.write(&result_name);
                        self.write(")");
                    } else {
                        self.write(&result_name);
                    }
                    self.write(";");
                    self.write_line();
                    self.decrease_indent();
                } else {
                    self.write_helper("__disposeResources");
                    self.write("(");
                    self.write(&env_name);
                    self.write(");");
                    self.write_line();
                }
                self.decrease_indent();
                self.write("}");
                self.write_line();
                self.block_using_env = prev_block_using_env;
            }

            // Record this block's class/fn/enum names only after nested namespaces
            // have emitted so same-block lexical references stay bare.
            if !leaf_name.is_empty() {
                if ns_name != leaf_name {
                    self.namespace_prior_class_fn_enum_exports
                        .entry(ns_name.clone())
                        .or_default()
                        .extend(class_fn_enum_names.iter().cloned());
                }
                self.namespace_prior_class_fn_enum_exports
                    .entry(class_fn_enum_root_name)
                    .or_default()
                    .extend(class_fn_enum_names);
            }

            // Restore previous exported names
            self.namespace_local_var_shadow_stack.pop();
            self.namespace_exported_names = prev_exported;
            self.namespace_parent_exported_names = prev_parent_exported;
            self.namespace_ancestor_export_qualifiers = prev_ancestor_qualifiers;
            self.namespace_current_class_fn_enum_names = prev_current_class_fn_enum;
        }
    }

    fn emit_recovered_namespace_function_arrow_body(&mut self, function_idx: NodeIndex) -> bool {
        let Some(func) = self.arena.get_function_at(function_idx) else {
            return false;
        };
        let Some(body_node) = self.arena.get(func.body) else {
            return false;
        };
        if body_node.kind == syntax_kind_ext::BLOCK {
            return false;
        }

        self.emit_expression_in_statement_position(func.body);
        self.write_semicolon();
        true
    }

    /// Check if a namespace import-alias target resolves to a runtime value.
    /// This mirrors TypeScript behavior for `export import X = Y;` inside namespaces:
    /// when `Y` is type-only (e.g. non-instantiated namespace), no runtime assignment
    /// should be emitted.
    /// Check whether `export default <identifier>` should emit runtime code.
    ///
    /// For `export default`, only purely type-level declarations (interface, type alias)
    /// should be skipped. Ambient value declarations (`declare function`, `declare class`,
    /// `declare var`) still represent runtime values and should emit `exports.default = X;`.
    /// Emit exported import alias as namespace property assignment.
    /// `export import X = Y;` → `ns.X = Y;`
    fn emit_namespace_exported_import_alias(
        &mut self,
        import_idx: NodeIndex,
        ns_name: &str,
        scope_body: Option<NodeIndex>,
    ) {
        let Some(import_node) = self.arena.get(import_idx) else {
            return;
        };
        let Some(import) = self.arena.get_import_decl(import_node) else {
            return;
        };

        // Get the alias name
        let alias_name = self.get_identifier_text_idx(import.import_clause);
        if alias_name.is_empty() {
            return;
        }

        // Check if the referenced value has runtime semantics
        if !self.import_decl_has_runtime_value(import) {
            return;
        }
        if !self.namespace_alias_target_has_runtime_value(import.module_specifier, scope_body) {
            return;
        }

        // Emit: ns.X = Y;
        self.write(ns_name);
        self.write(".");
        self.write(&alias_name);
        self.write(" = ");
        self.emit_entity_name(import.module_specifier);
        self.write(";");
        self.write_line();
    }

    /// Emit exported variable as namespace property assignment.
    /// `export var x = 10;` → `ns.x = 10;`
    fn emit_namespace_exported_variable(
        &mut self,
        var_stmt_idx: NodeIndex,
        ns_name: &str,
        outer_stmt: &Node,
        comment_upper_bound: u32,
        destructuring_export_temps: &rustc_hash::FxHashMap<NodeIndex, String>,
    ) {
        let Some(var_node) = self.arena.get(var_stmt_idx) else {
            return;
        };
        let Some(var_stmt) = self.arena.get_variable(var_node) else {
            return;
        };

        let mut wrote_any = false;

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

                if decl.initializer.is_none() {
                    continue;
                }

                if let Some(temp) = destructuring_export_temps.get(&decl_idx) {
                    self.write_namespace_export_separator(&mut wrote_any);
                    self.write(temp);
                    self.write(" = ");
                    self.emit_expression(decl.initializer);
                    self.emit_namespace_binding_pattern_assignments(
                        ns_name,
                        temp,
                        decl.name,
                        &mut wrote_any,
                    );
                } else if let Some(binding) = self.simple_namespace_binding_export(decl.name)
                    && self.can_inline_simple_namespace_binding_initializer(decl.initializer)
                {
                    self.emit_simple_namespace_binding_export(
                        ns_name,
                        decl.initializer,
                        &binding,
                        &mut wrote_any,
                    );
                } else {
                    let mut names = Vec::new();
                    self.collect_binding_names(decl.name, &mut names);
                    for name in names {
                        self.write_namespace_export_separator(&mut wrote_any);
                        self.write(ns_name);
                        self.write(".");
                        self.write(&name);
                        self.write(" = ");
                        self.emit_expression(decl.initializer);
                    }
                }
            }
        }

        if wrote_any {
            self.write(";");
            let token_end = self.find_token_end_before_trivia(outer_stmt.pos, comment_upper_bound);
            self.emit_trailing_comments_before(token_end, comment_upper_bound);
            self.write_line();
        }
    }
}
