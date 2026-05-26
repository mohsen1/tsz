use super::super::Printer;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

#[path = "namespace_export_destructuring.rs"]
mod namespace_export_destructuring;
#[path = "namespace/helpers.rs"]
mod namespace_helpers;
#[cfg(test)]
#[path = "namespace_import_alias_tests.rs"]
mod namespace_import_alias_tests;

pub(in crate::emitter) use namespace_helpers::rewrite_enum_iife_for_namespace_export;
use namespace_helpers::{find_next_code_module_keyword, find_unescaped_template_end};

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn namespace_body_has_using_declarations(
        &self,
        body_idx: NodeIndex,
    ) -> bool {
        let Some(body_node) = self.arena.get(body_idx) else {
            return false;
        };
        let Some(block) = self.arena.get_module_block(body_node) else {
            return false;
        };
        block
            .statements
            .as_ref()
            .is_some_and(|statements| self.block_has_using_declarations(statements))
    }

    pub(in crate::emitter) fn emit_module_declaration(&mut self, node: &Node, idx: NodeIndex) {
        let Some(module) = self.arena.get_module(node) else {
            return;
        };

        if self.emit_recovered_template_module_declaration(node, node.end) {
            return;
        }

        if self.emit_recovered_anonymous_declare_module_declaration(node, module) {
            return;
        }

        // Skip ambient module declarations (declare namespace/module)
        if self.arena.is_declare(&module.modifiers) {
            self.skip_comments_for_erased_node(node);
            return;
        }

        if self.emit_recovered_anonymous_module_declaration(node, module) {
            return;
        }

        // Skip non-instantiated modules (type-only: interfaces, type aliases, empty)
        if !self.is_instantiated_module(module.body) {
            self.skip_comments_for_erased_node(node);
            return;
        }

        // Top-level using scopes already own disposable environment numbering.
        // Emit namespaces that contain `using` through the statement printer path
        // so the namespace-local resource region shares that numbering.
        if self.ctx.target_es5 && self.in_top_level_using_scope {
            self.emit_namespace_iife(module, None, None);
            return;
        }

        // ES5 target: Transform namespace to IIFE pattern
        if self.ctx.target_es5 {
            use crate::transforms::NamespaceES5Emitter;
            let use_cjs = self.pending_cjs_namespace_export_fold;
            if use_cjs {
                self.pending_cjs_namespace_export_fold = false;
            }
            let system_export_fold = self.pending_system_namespace_export_fold.take();
            let mut es5_emitter = NamespaceES5Emitter::with_commonjs(self.arena, use_cjs);
            es5_emitter.set_module_kind(self.ctx.outer_module_kind());
            es5_emitter.set_target_es5(self.ctx.target_es5);
            es5_emitter.set_remove_comments(self.ctx.options.remove_comments);
            es5_emitter.set_transforms(self.transforms.clone());
            // Do NOT seed the namespace emitter with outer visible_original_names() as
            // block_scope_shadowed_names. The namespace IIFE creates an independent function
            // scope; outer module-scope `var` names are naturally isolated by function
            // scoping and do not need to appear in the shadow set. The correct shadowed
            // names (references at function scope outside blocks inside the namespace body)
            // are collected per-namespace by configure_ir_printer_scope via
            // collect_namespace_block_scope_shadowed_names.
            es5_emitter.set_block_scope_reserved_names(
                self.ctx.block_scope_state.visible_reserved_names(),
            );
            es5_emitter.set_disposable_env_context(self.next_disposable_env_id);
            es5_emitter.set_const_enum_facts(
                self.const_enum_values.clone(),
                self.const_enum_import_aliases.clone(),
            );
            if use_cjs {
                let export_names = std::mem::take(&mut self.pending_cjs_namespace_export_names);
                es5_emitter.set_commonjs_export_names(export_names);
            }
            if let Some(export_names) = system_export_fold.as_deref() {
                es5_emitter.set_system_export_folds(export_names.iter().map(String::as_str));
            }
            if !self.ctx.module_state.default_exported_func_names.is_empty() {
                es5_emitter.set_default_exported_func_names(
                    self.ctx
                        .module_state
                        .default_exported_func_names
                        .iter()
                        .cloned()
                        .collect(),
                );
            }
            let ns_name = self.get_module_root_name(module.name).unwrap_or_default();
            if !ns_name.is_empty() {
                // When the namespace name was already declared (e.g., by a
                // function or class), suppress the `var` declaration.
                if self.declared_namespace_names.contains(&ns_name) || self.in_top_level_using_scope
                {
                    es5_emitter.set_should_declare_var(false);
                }
                // Cross-block export sharing for ES5 path
                let block_exports = es5_emitter.collect_exported_var_names(idx);
                let entry = self
                    .namespace_prior_exports
                    .entry(ns_name.clone())
                    .or_default();
                entry.extend(block_exports);
                es5_emitter.set_prior_exported_vars(entry.clone());
                self.declared_namespace_names.insert(ns_name);
            }

            // Set IRPrinter indent to 0 because we'll handle base indentation through
            // the writer when writing each line. This prevents double-indentation for
            // nested namespaces where the writer is already indented.
            es5_emitter.set_indent_level(0);

            if let Some(text) = self.source_text_for_map() {
                es5_emitter.set_source_text(text);
            }
            let output = if use_cjs {
                es5_emitter.emit_exported_namespace(idx)
            } else {
                es5_emitter.emit_namespace(idx)
            };
            self.next_disposable_env_id = es5_emitter.disposable_env_counter();
            for generated_name in es5_emitter.take_generated_disposable_env_names() {
                self.generated_temp_names.insert(generated_name);
            }
            // Do NOT propagate namespace-internal block-scope reserved names back
            // to the outer scope state. The namespace IIFE creates an independent
            // function scope in ES5, so suffix renames inside it (e.g. `y_2` from
            // `let [y] = ...` in N's body) are invisible to sibling namespaces.
            // Syncing them back would cause sibling-namespace `let y` bindings to
            // receive avoidable suffixes like `y_3` even when `y` is not in scope.

            // Write the namespace output line by line, letting the writer handle indentation.
            // IRPrinter generates relative indentation (nested constructs indented relative
            // to each other), and the writer adds the base indentation for our current scope.
            let trimmed = output.trim_end_matches('\n');
            for (i, line) in trimmed.lines().enumerate() {
                if i > 0 {
                    self.write_line();
                }
                self.write(line);
            }

            // Skip comments within the namespace body range since the ES5 namespace emitter
            // doesn't use the main comment system. Without this, comments would be dumped
            // at end of file.
            self.skip_comments_for_erased_node(node);
            return;
        }

        // ES6+: Emit namespace as IIFE, preserving ES6+ syntax inside
        let module = module.clone();
        // Only pass parent_name when the inner namespace is exported.
        // Non-exported namespaces get a standalone IIFE without parent assignment.
        // The export status is tracked via `namespace_export_inner` flag, set by
        // `emit_namespace_body_statements` when processing EXPORT_DECLARATION wrappers.
        let parent_name = if self.namespace_export_inner {
            self.namespace_export_inner = false;
            self.current_namespace_name.clone()
        } else {
            None
        };
        let parent_source_path = self.current_namespace_source_path.clone();
        self.emit_namespace_iife(
            &module,
            parent_name.as_deref(),
            parent_source_path.as_deref(),
        );
    }

    pub(in crate::emitter) fn emit_recovered_template_module_declaration(
        &mut self,
        node: &Node,
        scan_end: u32,
    ) -> bool {
        let Some(module) = self.arena.get_module(node) else {
            return false;
        };
        if !self
            .arena
            .has_modifier(&module.modifiers, SyntaxKind::DeclareKeyword)
        {
            return false;
        }
        let Some(text) = self.source_text else {
            return false;
        };
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        let scan_end = (scan_end as usize).min(text.len());
        let Ok(source) = crate::safe_slice::slice(text, start, scan_end) else {
            return false;
        };

        let mut cursor = 0;
        let mut wrote = false;
        while let Some(module_pos) = find_next_code_module_keyword(source, cursor) {
            let after_module = module_pos + "module".len();
            let rest = &source[after_module..];
            let rest_trimmed = rest.trim_start_matches(|ch: char| ch.is_whitespace());
            let skipped = rest.len() - rest_trimmed.len();
            if !rest_trimmed.starts_with('`') {
                cursor = after_module;
                continue;
            };
            let template_start = after_module + skipped;
            let Some(template_end) = find_unescaped_template_end(source, template_start) else {
                break;
            };
            let after_template = template_end + '`'.len_utf8();
            let Ok(template_text) =
                crate::safe_slice::slice(source, template_start, after_template)
            else {
                cursor = after_template;
                continue;
            };
            let body_starts_after_template = source[after_template..]
                .trim_start_matches(|ch: char| ch.is_whitespace())
                .starts_with('{');
            if !body_starts_after_template {
                cursor = after_template;
                continue;
            }

            self.write("declare;");
            self.write_line();
            self.write("module ");
            self.write(template_text);
            self.write(";");
            self.write_line();
            self.write("{");
            self.write_line();
            self.write("}");
            self.write_line();
            wrote = true;
            cursor = after_template;
        }

        if wrote {
            self.skip_comments_for_erased_node(node);
        }
        wrote
    }

    fn emit_recovered_anonymous_module_declaration(
        &mut self,
        node: &Node,
        module: &tsz_parser::parser::node::ModuleData,
    ) -> bool {
        if !self.get_identifier_text_idx(module.name).is_empty() {
            return false;
        }
        let Some(text) = self.source_text else {
            return false;
        };
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        let Ok(source) = crate::safe_slice::slice(text, start, node.end as usize) else {
            return false;
        };

        let mut wrote = false;
        for line in source.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if wrote {
                self.write_line();
            }
            let indent_level = line
                .chars()
                .take_while(|ch| matches!(ch, ' ' | '\t'))
                .map(|ch| if ch == '\t' { 4 } else { 1 })
                .sum::<usize>()
                / 4;
            for _ in 0..indent_level {
                self.write("    ");
            }
            if trimmed == "module {" {
                self.write("module;");
                self.write_line();
                for _ in 0..indent_level {
                    self.write("    ");
                }
                self.write("{");
            } else {
                self.write(trimmed);
            }
            wrote = true;
        }
        if wrote {
            self.skip_comments_for_erased_node(node);
        }
        wrote
    }

    fn emit_recovered_anonymous_declare_module_declaration(
        &mut self,
        node: &Node,
        module: &tsz_parser::parser::node::ModuleData,
    ) -> bool {
        if !self.is_recovered_anonymous_declare_module(module) {
            return false;
        }
        if !self
            .recovered_declare_module_name_starts_with(node, |byte| byte == b'{')
            .unwrap_or(false)
        {
            return false;
        }
        let Some(body_node) = self.arena.get(module.body) else {
            return false;
        };
        let Some(block) = self.arena.get_module_block(body_node) else {
            return false;
        };

        self.write("declare;");
        self.write_line();
        self.write("module;");
        self.write_line();
        self.write("{");
        self.write_line();
        self.increase_indent();

        if let Some(statements) = block.statements.as_ref() {
            for &stmt_idx in &statements.nodes {
                if let Some(stmt_node) = self.arena.get(stmt_idx)
                    && self.is_erased_statement(stmt_node)
                {
                    continue;
                }
                let before_len = self.writer.len();
                self.emit(stmt_idx);
                if self.writer.len() > before_len && !self.writer.is_at_line_start() {
                    self.write_line();
                }
            }
        }

        self.decrease_indent();
        self.write("}");
        self.skip_comments_for_erased_node(node);
        true
    }

    fn recovered_declare_module_name_starts_with(
        &self,
        node: &Node,
        pred: impl FnOnce(u8) -> bool,
    ) -> Option<bool> {
        let text = self.source_text?;
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        let end = (node.end as usize).min(text.len());
        let source = crate::safe_slice::slice(text, start, end).ok()?;
        let module_pos = find_next_code_module_keyword(source, 0)?;
        let name_start = module_pos + "module".len();
        let rest = &source[name_start..];
        let rest_trimmed = rest.trim_start_matches(|ch: char| ch.is_whitespace());
        Some(
            rest_trimmed
                .as_bytes()
                .first()
                .is_some_and(|byte| pred(*byte)),
        )
    }

    /// Emit a namespace/module as an IIFE for ES6+ targets.
    /// `parent_name` is set when this is a nested namespace (e.g., Bar inside Foo).
    pub(in crate::emitter) fn emit_namespace_iife(
        &mut self,
        module: &tsz_parser::parser::node::ModuleData,
        parent_name: Option<&str>,
        parent_source_path: Option<&str>,
    ) {
        let name = self.get_identifier_text_idx(module.name);
        let source_path = parent_source_path
            .filter(|parent| !parent.is_empty())
            .map_or_else(|| name.clone(), |parent| format!("{parent}.{name}"));
        if let Some(parent) = parent_name
            && !name.is_empty()
        {
            self.namespace_prior_exports
                .entry(parent.to_string())
                .or_default()
                .insert(name.clone());
        }

        // Capture and consume the CJS export fold flag at the TOP of the IIFE,
        // not in the tail. Without this, nested namespace IIFEs inside the body
        // would consume the flag before the outer namespace reaches its tail.
        let cjs_export_fold = if parent_name.is_none() {
            let v = self.pending_cjs_namespace_export_fold;
            self.pending_cjs_namespace_export_fold = false;
            v
        } else {
            false
        };
        let cjs_export_names = if parent_name.is_none() {
            std::mem::take(&mut self.pending_cjs_namespace_export_names)
        } else {
            Vec::new()
        };
        let system_export_fold = if parent_name.is_none() {
            self.pending_system_namespace_export_fold.take()
        } else {
            None
        };

        // Capture and consume: when an exported namespace merges with a
        // default-exported function, the IIFE closing uses the plain pattern.
        let suppress_default_merge = if parent_name.is_none() {
            let v = self.suppress_default_export_merge_iife;
            self.suppress_default_export_merge_iife = false;
            v
        } else {
            false
        };

        // Determine if we should emit a variable declaration for this namespace.
        // Skip if name already declared by class/function/enum (both at top level and
        // inside namespace IIFEs - e.g., merged class+namespace doesn't need extra let).
        let should_declare = !(self.declared_namespace_names.contains(&name)
            || self.in_top_level_using_scope && parent_name.is_none());
        if should_declare {
            let keyword = if (self.in_namespace_iife || self.function_scope_depth > 0)
                && !self.ctx.target_es5
            {
                "let"
            } else {
                "var"
            };
            if self.should_emit_invalid_namespace_static_modifier_before_name(
                module.name,
                &module.modifiers,
            ) {
                self.write("static ");
            }
            self.write(keyword);
            self.write(" ");
            self.write(&name);
            self.write(";");
            self.write_line();
            self.declared_namespace_names.insert(name.clone());
        }

        // Check if the IIFE parameter name conflicts with any declaration
        // inside the namespace body. TSC renames the parameter with incrementing
        // suffixes across reopenings: M_1, M_2, M_3, etc.
        let iife_param = if self.namespace_body_has_name_conflict(module, &name) {
            let counter = self
                .namespace_iife_param_counter
                .entry(name.clone())
                .or_insert(0);
            *counter += 1;
            format!("{name}_{counter}")
        } else {
            name.clone()
        };

        // Emit: (function (<iife_param>) {
        self.write("(function (");
        self.write(&iife_param);
        self.write(") {");
        self.write_line();
        self.increase_indent();

        // Check if body is another MODULE_DECLARATION (nested: namespace Foo.Bar)
        if let Some(body_node) = self.arena.get(module.body) {
            if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                // Nested namespace (e.g., namespace X.Y.Z expands to nested IIFEs).
                // Save/restore declared_namespace_names so names from the outer scope
                // don't suppress declarations inside the nested IIFE (each IIFE creates
                // a new function scope), and names declared inside don't leak out.
                //
                // Pass `iife_param` (not `name`) as the inner's parent: inside this
                // IIFE's body the outer namespace is bound under the renamed param
                // (e.g., `Y` → `Y_1`), so the inner's `(N = parent.N || ...)` argument
                // must reference that local binding to avoid shadowing by the
                // `var N;` we just emitted for the inner namespace.
                if let Some(inner_module) = self.arena.get_module(body_node) {
                    let inner_module = inner_module.clone();
                    let prev_declared = std::mem::take(&mut self.declared_namespace_names);
                    self.emit_namespace_iife(&inner_module, Some(&iife_param), Some(&source_path));
                    self.declared_namespace_names = prev_declared;
                }
            } else {
                // MODULE_BLOCK: emit body statements
                let prev = self.in_namespace_iife;
                let prev_ns_name = self.current_namespace_name.clone();
                // Save and restore declared_namespace_names for this IIFE scope.
                // Use `take` so outer names don't suppress declarations inside (each
                // IIFE creates a new function scope), and inner names don't leak out.
                let prev_declared = std::mem::take(&mut self.declared_namespace_names);
                let prev_scope_end = self.namespace_scope_end;
                let prev_source_path = self.current_namespace_source_path.clone();
                self.in_namespace_iife = true;
                // Set the scope end so import alias reference searching is
                // limited to this namespace body (not sibling namespaces).
                if let Some(body_node) = self.arena.get(module.body) {
                    let block_scope_end = self
                        .arena
                        .get_module_block(body_node)
                        .and_then(|block| block.statements.as_ref())
                        .and_then(|statements| statements.nodes.last())
                        .and_then(|last_stmt_idx| self.arena.get(*last_stmt_idx))
                        .map(|last_stmt| {
                            self.find_token_end_before_trivia(last_stmt.pos, last_stmt.end)
                        });
                    self.namespace_scope_end = block_scope_end.unwrap_or_else(|| {
                        self.find_token_end_before_trivia(body_node.pos, body_node.end)
                    });
                }
                let prev_parent_ns = self.parent_namespace_name.clone();
                self.parent_namespace_name = parent_name
                    .map(std::borrow::ToOwned::to_owned)
                    .or_else(|| prev_ns_name.clone());
                self.current_namespace_name = Some(iife_param.clone());
                self.current_namespace_source_path = Some(source_path.clone());
                let prev_block_using_env = self.block_using_env.take();
                self.emit_namespace_body_statements(module, &iife_param);
                self.block_using_env = prev_block_using_env;
                self.in_namespace_iife = prev;
                self.namespace_scope_end = prev_scope_end;
                self.current_namespace_name = prev_ns_name;
                self.parent_namespace_name = prev_parent_ns;
                self.current_namespace_source_path = prev_source_path;
                self.declared_namespace_names = prev_declared;
            }
        }

        self.decrease_indent();
        // Closing: })(name || (name = {})); or
        // })(name = parent.name || (parent.name = {}));
        self.write("})(");
        if let Some(parent) = parent_name {
            self.write(&name);
            self.write(" = ");
            self.write(parent);
            self.write(".");
            self.write(&name);
            self.write(" || (");
            self.write(parent);
            self.write(".");
            self.write(&name);
            self.write(" = {}));");
        } else if let Some(export_names) = system_export_fold.as_deref()
            && !export_names.is_empty()
        {
            self.write(&name);
            self.write(" || (");
            self.emit_system_export_folded_namespace_assignment(export_names, &name);
            self.write("));");
        } else if cjs_export_fold {
            // CJS export fold: (N || (exports.Alias = exports.N = N = {}))
            self.write(&name);
            self.write(" || (");
            self.emit_commonjs_export_folded_namespace_assignment(&cjs_export_names, &name);
            self.write("));");
        } else if !suppress_default_merge
            && self.ctx.is_commonjs()
            && self
                .ctx
                .module_state
                .default_exported_func_names
                .contains(&name)
        {
            // Non-exported namespace merging with default-exported function:
            // (exports.Foo || (exports.Foo = {}))
            self.write("exports.");
            self.write(&name);
            self.write(" || (exports.");
            self.write(&name);
            self.write(" = {}));");
        } else {
            self.write(&name);
            self.write(" || (");
            self.write(&name);
            self.write(" = {}));");
        }
        // Don't emit trailing comments here — the source_file statement
        // loop handles them with proper next-sibling bounds, preventing
        // us from stealing comments that belong to subsequent statements.
        self.write_line();
    }

    fn emit_system_export_folded_namespace_assignment(
        &mut self,
        export_names: &[String],
        name: &str,
    ) {
        let Some((export_name, inner_names)) = export_names.split_last() else {
            self.write(name);
            self.write(" = {}");
            return;
        };

        self.write("exports_1(\"");
        self.emit_escaped_string(export_name, '"');
        self.write("\", ");
        self.emit_system_export_folded_namespace_assignment(inner_names, name);
        self.write(")");
    }

    fn emit_commonjs_export_folded_namespace_assignment(
        &mut self,
        export_names: &[String],
        name: &str,
    ) {
        let Some((export_name, inner_names)) = export_names.split_last() else {
            self.write(name);
            self.write(" = {}");
            return;
        };

        self.write("exports.");
        self.write(export_name);
        self.write(" = ");
        self.emit_commonjs_export_folded_namespace_assignment(inner_names, name);
    }

    /// Check if any declaration at any depth in the namespace body has the same
    /// name as the namespace. TSC renames the IIFE parameter when this happens
    /// (e.g., `M` → `M_1`). Checks declarations, function parameters, and local
    /// variables at all depths — not just top-level.
    /// Variant of `namespace_body_has_name_conflict` for the dotted-name
    /// recursion path: walk through nested `MODULE_DECLARATIONs` and run a
    /// text scan over the innermost block. The text scan catches function
    /// parameters and any-depth bindings (function/class/enum/var/etc.).
    /// Crucially, it EXCLUDES `namespace`/`module` keywords — tsc
    /// deliberately doesn't rename an outer namespace IIFE param when
    /// the conflict comes from a nested sub-namespace (the sub-namespace
    /// has its own IIFE scope and doesn't shadow the outer param at
    /// call sites).
    fn dotted_namespace_innermost_block_conflicts_iife_param(
        &self,
        module: &tsz_parser::parser::node::ModuleData,
        ns_name: &str,
    ) -> bool {
        let Some(body_node) = self.arena.get(module.body) else {
            return false;
        };
        if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            if let Some(inner) = self.arena.get_module(body_node) {
                let inner_name = self.get_identifier_text_idx(inner.name);
                if inner_name == ns_name {
                    return true;
                }
                return self.dotted_namespace_innermost_block_conflicts_iife_param(inner, ns_name);
            }
            return false;
        }
        if let Some(text) = self.source_text {
            let declare_ranges = self.collect_declare_statement_ranges(body_node);
            return match crate::safe_slice::slice(
                text,
                body_node.pos as usize,
                body_node.end as usize,
            ) {
                Ok(body_text) => {
                    let body_pos = body_node.pos as usize;
                    let masked = Self::mask_ranges_static(body_text, body_pos, &declare_ranges);
                    Self::text_has_non_namespace_binding_named(&masked, ns_name)
                }
                Err(_) => false,
            };
        }
        false
    }

    /// Scan for runtime bindings while skipping `namespace`/`module` declarations.
    /// Nested sub-namespaces have their own IIFE scope and should not force the
    /// enclosing namespace's IIFE parameter to be renamed.
    fn text_has_non_namespace_binding_named(text: &str, name: &str) -> bool {
        let stripped = Self::strip_comments(text);
        let text = &stripped;
        let name_bytes = name.as_bytes();
        let text_bytes = text.as_bytes();
        let name_len = name_bytes.len();

        let mut i = 0;
        while i + name_len <= text_bytes.len() {
            if let Some(pos) = text[i..].find(name) {
                let abs = i + pos;
                let before_ok = abs == 0
                    || (!text_bytes[abs - 1].is_ascii_alphanumeric()
                        && text_bytes[abs - 1] != b'_'
                        && text_bytes[abs - 1] != b'$');
                let after_end = abs + name_len;
                let after_ok = after_end >= text_bytes.len()
                    || (!text_bytes[after_end].is_ascii_alphanumeric()
                        && text_bytes[after_end] != b'_'
                        && text_bytes[after_end] != b'$');

                if before_ok && after_ok {
                    let mut p = abs;
                    while p > 0 && text_bytes[p - 1].is_ascii_whitespace() {
                        p -= 1;
                    }
                    if p > 0 {
                        let prev_char = text_bytes[p - 1];
                        if prev_char == b'(' || prev_char == b',' {
                            return true;
                        }
                        let preceding = &text[..p];
                        let binding_keywords: &[&str] =
                            &["var", "let", "const", "function", "class", "import"];
                        for &kw in binding_keywords {
                            if preceding.ends_with(kw) {
                                let kw_start = p - kw.len();
                                let kw_before_ok = kw_start == 0
                                    || (!text_bytes[kw_start - 1].is_ascii_alphanumeric()
                                        && text_bytes[kw_start - 1] != b'_'
                                        && text_bytes[kw_start - 1] != b'$');
                                if kw_before_ok {
                                    return true;
                                }
                            }
                        }
                        let parameter_modifier_keywords: &[&str] =
                            &["private", "public", "protected", "readonly", "override"];
                        for &kw in parameter_modifier_keywords {
                            if preceding.ends_with(kw) {
                                let kw_start = p - kw.len();
                                let kw_before_ok = kw_start == 0
                                    || (!text_bytes[kw_start - 1].is_ascii_alphanumeric()
                                        && text_bytes[kw_start - 1] != b'_'
                                        && text_bytes[kw_start - 1] != b'$');
                                if kw_before_ok
                                    && Self::keyword_is_in_parameter_context(text_bytes, kw_start)
                                {
                                    return true;
                                }
                            }
                        }
                    }
                }
                i = abs + 1;
            } else {
                break;
            }
        }
        false
    }

    fn keyword_is_in_parameter_context(text_bytes: &[u8], kw_start: usize) -> bool {
        let mut p = kw_start;
        loop {
            while p > 0 && text_bytes[p - 1].is_ascii_whitespace() {
                p -= 1;
            }
            if p == 0 {
                return false;
            }
            let prev_char = text_bytes[p - 1];
            if prev_char == b'(' || prev_char == b',' {
                return true;
            }
            if !prev_char.is_ascii_alphanumeric() && prev_char != b'_' && prev_char != b'$' {
                return false;
            }

            let ident_end = p;
            let mut ident_start = ident_end - 1;
            while ident_start > 0
                && (text_bytes[ident_start - 1].is_ascii_alphanumeric()
                    || text_bytes[ident_start - 1] == b'_'
                    || text_bytes[ident_start - 1] == b'$')
            {
                ident_start -= 1;
            }
            let Ok(ident) = std::str::from_utf8(&text_bytes[ident_start..ident_end]) else {
                return false;
            };
            if !matches!(
                ident,
                "private" | "public" | "protected" | "readonly" | "override"
            ) {
                return false;
            }
            p = ident_start;
        }
    }

    fn namespace_body_has_name_conflict(
        &self,
        module: &tsz_parser::parser::node::ModuleData,
        ns_name: &str,
    ) -> bool {
        let Some(body_node) = self.arena.get(module.body) else {
            return false;
        };
        if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            // Dotted namespace (e.g. `namespace M.buz.plop`): the immediate
            // body is the next nested MODULE_DECLARATION, not a block. Check
            // the direct child's name against `ns_name` first; if it doesn't
            // match, recurse into the dotted chain and check ONLY the
            // innermost block's top-level statements via
            // `declaration_conflicts_iife_param` (function/class/enum/var/
            // import-equals). Crucially, we don't fall back to the text scan
            // there, because the text scan also matches `namespace A {}`
            // declarations as bindings, and tsc deliberately doesn't rename
            // an outer namespace IIFE param when the conflict comes from a
            // nested sub-namespace (the sub-namespace has its own IIFE
            // scope and doesn't shadow the outer param at call sites).
            if let Some(inner) = self.arena.get_module(body_node) {
                let inner_name = self.get_identifier_text_idx(inner.name);
                if inner_name == ns_name {
                    return true;
                }
                return self.dotted_namespace_innermost_block_conflicts_iife_param(inner, ns_name);
            }
            return false;
        }
        if let Some(block) = self.arena.get_module_block(body_node)
            && let Some(stmts) = &block.statements
            && stmts
                .nodes
                .iter()
                .copied()
                .any(|stmt| self.namespace_statement_conflicts_iife_param(stmt, ns_name))
        {
            return true;
        }
        if self.namespace_block_contains_instantiated_module_named(body_node, ns_name) {
            return true;
        }
        // Use source text scan for bindings in nested functions/classes at any depth.
        // Nested namespace/module declarations have their own IIFE scope and do not
        // shadow this IIFE parameter at call sites, so exclude those keywords here.
        if let Some(text) = self.source_text {
            // safe_slice: C → migrated. A bad span here would silently report
            // "no binding found", which can change namespace shadowing
            // decisions and emit incorrectly. Surface span errors instead of
            // returning a false-negative; fall back to false only when source
            // text is literally unavailable.
            let declare_ranges = self.collect_declare_statement_ranges(body_node);
            return match crate::safe_slice::slice(
                text,
                body_node.pos as usize,
                body_node.end as usize,
            ) {
                Ok(body_text) => {
                    let body_pos = body_node.pos as usize;
                    let masked = Self::mask_ranges_static(body_text, body_pos, &declare_ranges);
                    Self::text_has_non_namespace_binding_named(&masked, ns_name)
                }
                Err(_) => false,
            };
        }
        false
    }

    fn namespace_block_contains_instantiated_module_named(
        &self,
        body_node: &tsz_parser::parser::node::Node,
        ns_name: &str,
    ) -> bool {
        let Some(block) = self.arena.get_module_block(body_node) else {
            return false;
        };
        let Some(stmts) = &block.statements else {
            return false;
        };
        stmts.nodes.iter().copied().any(|stmt| {
            let (decl_idx, is_declare) = if let Some(stmt_node) = self.arena.get(stmt)
                && stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                && let Some(export) = self.arena.get_export_decl(stmt_node)
            {
                (
                    export.export_clause,
                    self.declaration_is_declare(export.export_clause),
                )
            } else {
                (stmt, self.declaration_is_declare(stmt))
            };
            !is_declare && self.module_decl_chain_contains_instantiated_name(decl_idx, ns_name)
        })
    }

    fn module_decl_chain_contains_instantiated_name(
        &self,
        module_idx: NodeIndex,
        ns_name: &str,
    ) -> bool {
        let Some(module_node) = self.arena.get(module_idx) else {
            return false;
        };
        if module_node.kind != syntax_kind_ext::MODULE_DECLARATION {
            return false;
        }
        let Some(module) = self.arena.get_module(module_node) else {
            return false;
        };
        if self.get_identifier_text_idx(module.name) == ns_name
            && self.is_instantiated_module(module.body)
        {
            return true;
        }
        let Some(body_node) = self.arena.get(module.body) else {
            return false;
        };
        if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            return self.module_decl_chain_contains_instantiated_name(module.body, ns_name);
        }
        self.namespace_block_contains_instantiated_module_named(body_node, ns_name)
    }

    /// Collect (pos, end) byte ranges of every statement inside a namespace
    /// body that is type-only (`declare`). Their bodies are erased at emit
    /// time, so identifiers introduced inside them — including a same-named
    /// inner namespace — must not be counted when deciding whether to rename
    /// the IIFE parameter.
    fn collect_declare_statement_ranges(
        &self,
        body_node: &tsz_parser::parser::node::Node,
    ) -> Vec<(usize, usize)> {
        let mut ranges: Vec<(usize, usize)> = Vec::new();
        let Some(block) = self.arena.get_module_block(body_node) else {
            return ranges;
        };
        let Some(stmts) = &block.statements else {
            return ranges;
        };
        for &stmt_idx in &stmts.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            // Resolve the inner declaration. `export declare ...` parses as an
            // EXPORT_DECLARATION wrapping the real decl, whose modifier list
            // carries `declare`.
            let (decl_node, decl_pos, decl_end) = if stmt_node.kind
                == syntax_kind_ext::EXPORT_DECLARATION
                && let Some(export) = self.arena.get_export_decl(stmt_node)
                && let Some(inner) = self.arena.get(export.export_clause)
            {
                (inner, stmt_node.pos as usize, stmt_node.end as usize)
            } else {
                (stmt_node, stmt_node.pos as usize, stmt_node.end as usize)
            };
            let modifiers = match decl_node.kind {
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => self
                    .arena
                    .get_variable(decl_node)
                    .and_then(|v| v.modifiers.clone()),
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => self
                    .arena
                    .get_function(decl_node)
                    .and_then(|f| f.modifiers.clone()),
                k if k == syntax_kind_ext::CLASS_DECLARATION => self
                    .arena
                    .get_class(decl_node)
                    .and_then(|c| c.modifiers.clone()),
                k if k == syntax_kind_ext::ENUM_DECLARATION => self
                    .arena
                    .get_enum(decl_node)
                    .and_then(|e| e.modifiers.clone()),
                k if k == syntax_kind_ext::MODULE_DECLARATION => self
                    .arena
                    .get_module(decl_node)
                    .and_then(|m| m.modifiers.clone()),
                _ => None,
            };
            if self
                .arena
                .has_modifier(&modifiers, SyntaxKind::DeclareKeyword)
            {
                ranges.push((decl_pos, decl_end));
            }
        }
        ranges
    }

    fn namespace_statement_conflicts_iife_param(&self, stmt_idx: NodeIndex, ns_name: &str) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
            && let Some(export) = self.arena.get_export_decl(stmt_node)
        {
            let Some(inner_node) = self.arena.get(export.export_clause) else {
                return false;
            };
            // `export import M = Z.M` emits as `M.M = Z.M` and should reuse
            // the namespace parameter. Non-exported `import M = Z.M` emits a
            // local `var M = Z.M`, so it does require parameter renaming.
            if inner_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                return false;
            }
            // `export declare ...` is type-only and erased at emit, so it
            // cannot shadow the IIFE parameter — even if the inner declaration
            // shares a name with the namespace.
            if self.declaration_is_declare(export.export_clause) {
                return false;
            }
            return self.declaration_conflicts_iife_param(export.export_clause, ns_name);
        }
        // Same rule for non-exported `declare` declarations.
        if self.declaration_is_declare(stmt_idx) {
            return false;
        }
        self.declaration_conflicts_iife_param(stmt_idx, ns_name)
    }

    fn declaration_is_declare(&self, decl_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(decl_idx) else {
            return false;
        };
        let modifiers = match node.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => self
                .arena
                .get_variable(node)
                .and_then(|v| v.modifiers.clone()),
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => self
                .arena
                .get_function(node)
                .and_then(|f| f.modifiers.clone()),
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.arena.get_class(node).and_then(|c| c.modifiers.clone())
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.arena.get_enum(node).and_then(|e| e.modifiers.clone())
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => self
                .arena
                .get_module(node)
                .and_then(|m| m.modifiers.clone()),
            _ => None,
        };
        self.arena
            .has_modifier(&modifiers, SyntaxKind::DeclareKeyword)
    }

    fn declaration_conflicts_iife_param(&self, decl_idx: NodeIndex, ns_name: &str) -> bool {
        let Some(node) = self.arena.get(decl_idx) else {
            return false;
        };
        match node.kind {
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                self.arena.get_import_decl(node).is_some_and(|import| {
                    self.get_identifier_text_idx(import.import_clause) == ns_name
                })
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.arena.get_variable(node).is_some_and(|var_stmt| {
                    self.collect_variable_names(&var_stmt.declarations)
                        .iter()
                        .any(|name| name == ns_name)
                })
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => self
                .arena
                .get_function(node)
                .is_some_and(|func| self.get_identifier_text_idx(func.name) == ns_name),
            k if k == syntax_kind_ext::CLASS_DECLARATION => self
                .arena
                .get_class(node)
                .is_some_and(|class| self.get_identifier_text_idx(class.name) == ns_name),
            k if k == syntax_kind_ext::ENUM_DECLARATION => self
                .arena
                .get_enum(node)
                .is_some_and(|enum_decl| self.get_identifier_text_idx(enum_decl.name) == ns_name),
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.arena.get_module(node).is_some_and(|module| {
                    self.get_identifier_text_idx(module.name) == ns_name
                        && self.is_instantiated_module(module.body)
                })
            }
            _ => false,
        }
    }

    /// Replace bytes inside `ranges` (absolute source positions) with spaces in
    /// `body_text`, where `body_text` starts at absolute offset `body_pos`.
    /// Used to neutralize identifiers that come from `declare` (ambient)
    /// declarations before running the source-text binding scan.
    fn mask_ranges_static(body_text: &str, body_pos: usize, ranges: &[(usize, usize)]) -> String {
        if ranges.is_empty() {
            return body_text.to_string();
        }
        let mut bytes = body_text.as_bytes().to_vec();
        for &(start, end) in ranges {
            let local_start = start.saturating_sub(body_pos);
            let local_end = end.saturating_sub(body_pos).min(bytes.len());
            if local_start >= bytes.len() {
                continue;
            }
            for b in &mut bytes[local_start..local_end] {
                if !b.is_ascii_whitespace() {
                    *b = b' ';
                }
            }
        }
        String::from_utf8(bytes).unwrap_or_else(|_| body_text.to_string())
    }

    /// Strip single-line and block comments from text, replacing them with spaces.
    fn strip_comments(text: &str) -> String {
        let bytes = text.as_bytes();
        let mut result = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
                // Single-line comment: replace with spaces until newline
                while i < bytes.len() && bytes[i] != b'\n' {
                    result.push(b' ');
                    i += 1;
                }
            } else if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                // Block comment: replace with spaces
                result.push(b' ');
                result.push(b' ');
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    result.push(b' ');
                    i += 1;
                }
                if i + 1 < bytes.len() {
                    result.push(b' ');
                    result.push(b' ');
                    i += 2;
                }
            } else {
                result.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8(result).unwrap_or_default()
    }

    /// Collect exported *variable* names from a namespace body for identifier qualification.
    ///
    /// Only `export var` names need qualification because their local declaration is replaced
    /// by a namespace property assignment (`ns.x = expr;`).
    /// Exported classes/functions/enums keep their local declaration, so their names
    /// remain in scope without qualification.
    pub(in crate::emitter) fn collect_namespace_exported_names(
        &self,
        module: &tsz_parser::parser::node::ModuleData,
    ) -> rustc_hash::FxHashSet<String> {
        let mut names = rustc_hash::FxHashSet::default();
        let Some(body_node) = self.arena.get(module.body) else {
            return names;
        };
        let Some(block) = self.arena.get_module_block(body_node) else {
            return names;
        };
        let Some(ref stmts) = block.statements else {
            return names;
        };
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
            let inner_kind = self.arena.get(export.export_clause).map_or(0, |n| n.kind);
            // Collect names that are emitted only as namespace property assignments.
            // These references must be qualified inside namespace IIFEs (`ns.x`).
            if inner_kind == syntax_kind_ext::VARIABLE_STATEMENT
                || inner_kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                let export_names = self.get_export_names_from_clause(export.export_clause);
                for name in export_names {
                    names.insert(name);
                }
            }
        }
        for &stmt_idx in &stmts.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if (stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                || stmt_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION)
                && self.statement_has_export_modifier(stmt_node)
            {
                let export_names = self.get_export_names_from_clause(stmt_idx);
                for name in export_names {
                    names.insert(name);
                }
            }
        }
        names
    }

    fn namespace_class_fn_enum_name(&self, node: &Node) -> Option<String> {
        let name = match node.kind {
            k if k == syntax_kind_ext::CLASS_DECLARATION => self
                .arena
                .get_class(node)
                .map(|c| self.get_identifier_text_idx(c.name)),
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => self
                .arena
                .get_function(node)
                .map(|f| self.get_identifier_text_idx(f.name)),
            k if k == syntax_kind_ext::ENUM_DECLARATION => self
                .arena
                .get_enum(node)
                .map(|e| self.get_identifier_text_idx(e.name)),
            k if k == syntax_kind_ext::MODULE_DECLARATION => self
                .arena
                .get_module(node)
                .map(|m| self.get_identifier_text_idx(m.name)),
            _ => None,
        }?;
        if name.is_empty() { None } else { Some(name) }
    }

    /// Collect names of exported classes, functions, and enums from a namespace.
    /// These names need qualification in REOPENED blocks of the same namespace
    /// but NOT in their own declaration block (since they're locally in scope).
    pub(in crate::emitter) fn collect_namespace_class_fn_enum_names(
        &self,
        module: &tsz_parser::parser::node::ModuleData,
    ) -> Vec<String> {
        let mut names = Vec::new();
        let Some(body_node) = self.arena.get(module.body) else {
            return names;
        };
        let Some(block) = self.arena.get_module_block(body_node) else {
            return names;
        };
        let Some(ref stmts) = block.statements else {
            return names;
        };
        for &stmt_idx in &stmts.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };

            if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                let Some(export) = self.arena.get_export_decl(stmt_node) else {
                    continue;
                };
                let Some(inner_node) = self.arena.get(export.export_clause) else {
                    continue;
                };
                if let Some(name) = self.namespace_class_fn_enum_name(inner_node) {
                    names.push(name);
                }
            } else if self.statement_has_export_modifier(stmt_node)
                && let Some(name) = self.namespace_class_fn_enum_name(stmt_node)
            {
                names.push(name);
            }
        }
        names
    }

    /// Collect class/function/enum names declared in the current namespace block.
    /// These are lexical value bindings for this IIFE and shadow parent namespace
    /// properties while printing heritage clauses.
    pub(in crate::emitter) fn collect_namespace_current_class_fn_enum_names(
        &self,
        module: &tsz_parser::parser::node::ModuleData,
    ) -> Vec<String> {
        let mut names = Vec::new();
        let Some(body_node) = self.arena.get(module.body) else {
            return names;
        };
        let Some(block) = self.arena.get_module_block(body_node) else {
            return names;
        };
        let Some(ref stmts) = block.statements else {
            return names;
        };
        for &stmt_idx in &stmts.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };

            if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                let Some(export) = self.arena.get_export_decl(stmt_node) else {
                    continue;
                };
                let Some(inner_node) = self.arena.get(export.export_clause) else {
                    continue;
                };
                if let Some(name) = self.namespace_class_fn_enum_name(inner_node) {
                    names.push(name);
                }
            } else if let Some(name) = self.namespace_class_fn_enum_name(stmt_node) {
                names.push(name);
            }
        }
        names
    }

    /// Collect non-exported variable names declared in a namespace body.
    /// These shadow any same-named exports from prior blocks.
    pub(in crate::emitter) fn collect_namespace_local_var_names(
        &self,
        body_node: &tsz_parser::parser::node::Node,
    ) -> rustc_hash::FxHashSet<String> {
        let mut names = rustc_hash::FxHashSet::default();
        let Some(block) = self.arena.get_module_block(body_node) else {
            return names;
        };
        let Some(ref stmts) = block.statements else {
            return names;
        };
        for &stmt_idx in &stmts.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            // Only collect non-exported variable declarations
            if stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                && let Some(var_data) = self.arena.get_variable(stmt_node)
            {
                for &decl_list_idx in &var_data.declarations.nodes {
                    if let Some(decl_list_node) = self.arena.get(decl_list_idx)
                        && let Some(decl_list) = self.arena.get_variable(decl_list_node)
                    {
                        for &decl_idx in &decl_list.declarations.nodes {
                            if let Some(decl_node) = self.arena.get(decl_idx)
                                && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                            {
                                let mut binding_names = Vec::new();
                                self.collect_binding_names(decl.name, &mut binding_names);
                                names.extend(binding_names);
                            }
                        }
                    }
                }
            }
        }
        names
    }

    pub(in crate::emitter) fn collect_namespace_non_exported_local_var_names(
        &self,
        body_node: &tsz_parser::parser::node::Node,
    ) -> rustc_hash::FxHashSet<String> {
        let mut names = rustc_hash::FxHashSet::default();
        let Some(block) = self.arena.get_module_block(body_node) else {
            return names;
        };
        let Some(ref stmts) = block.statements else {
            return names;
        };
        for &stmt_idx in &stmts.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                continue;
            }
            if self.statement_has_export_modifier(stmt_node) {
                continue;
            }
            let Some(var_data) = self.arena.get_variable(stmt_node) else {
                continue;
            };
            for &decl_list_idx in &var_data.declarations.nodes {
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
                    let mut binding_names = Vec::new();
                    self.collect_binding_names(decl.name, &mut binding_names);
                    names.extend(binding_names);
                }
            }
        }
        names
    }

    pub(in crate::emitter) fn is_shadowed_by_namespace_local_var(&self, name: &str) -> bool {
        self.namespace_local_var_shadow_stack
            .iter()
            .rev()
            .any(|scope| scope.contains(name))
    }

    pub(in crate::emitter) fn collect_namespace_local_module_names(
        &self,
        body_node: &tsz_parser::parser::node::Node,
    ) -> rustc_hash::FxHashSet<String> {
        let mut names = rustc_hash::FxHashSet::default();
        let Some(block) = self.arena.get_module_block(body_node) else {
            return names;
        };
        let Some(ref stmts) = block.statements else {
            return names;
        };
        for &stmt_idx in &stmts.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let module_node = if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                Some(stmt_node)
            } else if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                self.arena
                    .get_export_decl(stmt_node)
                    .and_then(|export| self.arena.get(export.export_clause))
                    .filter(|inner| inner.kind == syntax_kind_ext::MODULE_DECLARATION)
            } else {
                None
            };
            let Some(module_node) = module_node else {
                continue;
            };
            let Some(module) = self.arena.get_module(module_node) else {
                continue;
            };
            let name = self.get_identifier_text_idx(module.name);
            if !name.is_empty() {
                names.insert(name);
            }
        }
        names
    }

    pub(in crate::emitter) fn collect_dotted_namespace_children_from_source(
        &self,
        parent: &str,
    ) -> rustc_hash::FxHashSet<String> {
        let mut children = rustc_hash::FxHashSet::default();
        let Some(text) = self.source_text else {
            return children;
        };
        for keyword in ["namespace ", "module "] {
            let mut search_start = 0;
            while let Some(relative_pos) = text[search_start..].find(keyword) {
                let name_start = search_start + relative_pos + keyword.len();
                let Some((parts, _)) = Self::parse_namespace_path_and_body_start(text, name_start)
                else {
                    search_start = name_start;
                    continue;
                };
                for pair in parts.windows(2) {
                    if pair[0] == parent {
                        children.insert(pair[1].clone());
                    }
                }
                search_start = name_start;
            }
        }
        children
    }

    pub(in crate::emitter) fn collect_namespace_exported_value_members_from_source(
        &self,
        parent: &str,
    ) -> rustc_hash::FxHashSet<String> {
        let mut names = rustc_hash::FxHashSet::default();
        let Some(text) = self.source_text else {
            return names;
        };
        for keyword in ["namespace ", "module "] {
            let mut search_start = 0;
            while let Some(relative_pos) = text[search_start..].find(keyword) {
                let name_start = search_start + relative_pos + keyword.len();
                let Some((parts, open_brace)) =
                    Self::parse_namespace_path_and_body_start(text, name_start)
                else {
                    search_start = name_start;
                    continue;
                };
                if !parts.iter().any(|part| part == parent) {
                    search_start = name_start;
                    continue;
                }
                let Some(close_brace) = Self::find_matching_brace(text, open_brace) else {
                    search_start = open_brace + 1;
                    continue;
                };
                let body = &text[open_brace + 1..close_brace];
                Self::collect_exported_value_member_names_from_text(body, &mut names);
                search_start = close_brace + 1;
            }
        }
        names
    }

    fn parse_namespace_path_and_body_start(
        text: &str,
        name_start: usize,
    ) -> Option<(Vec<String>, usize)> {
        let bytes = text.as_bytes();
        let mut pos = name_start;
        let mut parts = Vec::new();
        loop {
            while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            let part_start = pos;
            while pos < bytes.len()
                && (bytes[pos].is_ascii_alphanumeric() || bytes[pos] == b'_' || bytes[pos] == b'$')
            {
                pos += 1;
            }
            if part_start == pos {
                return None;
            }
            parts.push(text[part_start..pos].to_string());
            while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            if bytes.get(pos) == Some(&b'.') {
                pos += 1;
                continue;
            }
            break;
        }
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if bytes.get(pos) == Some(&b'{') {
            Some((parts, pos))
        } else {
            None
        }
    }

    fn find_matching_brace(text: &str, open_brace: usize) -> Option<usize> {
        let bytes = text.as_bytes();
        let mut depth = 0_u32;
        for (offset, &byte) in bytes.get(open_brace..)?.iter().enumerate() {
            match byte {
                b'{' => depth += 1,
                b'}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(open_brace + offset);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn collect_exported_value_member_names_from_text(
        body: &str,
        names: &mut rustc_hash::FxHashSet<String>,
    ) {
        for marker in ["export class ", "export function ", "export enum "] {
            let mut search_start = 0;
            while let Some(relative_pos) = body[search_start..].find(marker) {
                let mut name_start = search_start + relative_pos + marker.len();
                if marker == "export class " && body[name_start..].starts_with("abstract ") {
                    name_start += "abstract ".len();
                }
                let name: String = body[name_start..]
                    .chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                    .collect();
                if !name.is_empty() {
                    names.insert(name);
                }
                search_start = name_start.saturating_add(1);
            }
        }
    }

    pub(in crate::emitter) fn namespace_iife_param_base(name: &str) -> &str {
        let Some((base, suffix)) = name.rsplit_once('_') else {
            return name;
        };
        if suffix.chars().all(|ch| ch.is_ascii_digit()) {
            base
        } else {
            name
        }
    }

    pub(in crate::emitter) fn collect_all_namespace_exports(&mut self, statements: &NodeList) {
        for &stmt_idx in &statements.nodes {
            self.collect_namespace_exports_from_statement(stmt_idx, None, false);
        }
    }

    fn collect_namespace_exports_from_statement(
        &mut self,
        stmt_idx: NodeIndex,
        parent_path: Option<&str>,
        exported_to_parent: bool,
    ) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
            if let Some(export) = self.arena.get_export_decl(stmt_node) {
                if let Some(inner_node) = self.arena.get(export.export_clause) {
                    if inner_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                        self.collect_namespace_exports_from_statement(
                            export.export_clause,
                            parent_path,
                            true,
                        );
                    } else if let Some(path) = parent_path {
                        let names = self.get_export_names_from_clause(export.export_clause);
                        self.namespace_all_exported_names
                            .entry(path.to_string())
                            .or_default()
                            .extend(names);
                    }
                }
            }
            return;
        }

        if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            self.collect_namespace_exports_from_module(stmt_node, parent_path, exported_to_parent);
            return;
        }

        if self.statement_has_export_modifier(stmt_node)
            && let Some(path) = parent_path
        {
            let names = self.get_export_names_from_clause(stmt_idx);
            self.namespace_all_exported_names
                .entry(path.to_string())
                .or_default()
                .extend(names);
        }
    }

    fn collect_namespace_exports_from_module(
        &mut self,
        module_node: &Node,
        parent_path: Option<&str>,
        exported_to_parent: bool,
    ) {
        let Some(module) = self.arena.get_module(module_node) else {
            return;
        };
        let name = self.get_identifier_text_idx(module.name);
        if name.is_empty() {
            return;
        }
        if exported_to_parent && let Some(parent) = parent_path {
            self.namespace_all_exported_names
                .entry(parent.to_string())
                .or_default()
                .insert(name.clone());
        }

        let path = parent_path.map_or_else(|| name.clone(), |parent| format!("{parent}.{name}"));
        let Some(body_node) = self.arena.get(module.body) else {
            return;
        };
        if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            self.collect_namespace_exports_from_module(body_node, Some(&path), true);
            return;
        }
        let Some(block) = self.arena.get_module_block(body_node) else {
            return;
        };
        let Some(stmts) = block.statements.clone() else {
            return;
        };
        for stmt_idx in stmts.nodes {
            self.collect_namespace_exports_from_statement(stmt_idx, Some(&path), false);
        }
    }
}

#[cfg(test)]
#[path = "namespace/tests.rs"]
mod tests;
