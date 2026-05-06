use super::super::Printer;
use crate::transforms::ir::IRNode;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;

#[path = "namespace_export_destructuring.rs"]
mod namespace_export_destructuring;
#[cfg(test)]
#[path = "namespace_import_alias_tests.rs"]
mod namespace_import_alias_tests;

/// Rewrite enum IIFE IR from `E || (E = {})` to `E = NS.E || (NS.E = {})`
/// for exported enums in namespaces.
pub(in crate::emitter) fn rewrite_enum_iife_for_namespace_export(
    ir: &mut IRNode,
    enum_name: &str,
    ns_name: &str,
) {
    // The IR from EnumES5Transformer is:
    //   Sequence([VarDecl { name }, ExpressionStatement(CallExpr { callee, arguments: [iife_arg] })])
    // where iife_arg is: LogicalOr { left: Identifier(E), right: BinaryExpr(E = {}) }
    //
    // We need to transform it to:
    //   iife_arg = BinaryExpr(E = LogicalOr { left: NS.E, right: BinaryExpr(NS.E = {}) })
    let IRNode::Sequence(stmts) = ir else {
        return;
    };

    // Find the ExpressionStatement containing the CallExpr
    let Some(expr_stmt) = stmts.iter_mut().find_map(|s| match s {
        IRNode::ExpressionStatement(inner) => Some(inner),
        _ => None,
    }) else {
        return;
    };

    let IRNode::CallExpr { arguments, .. } = expr_stmt.as_mut() else {
        return;
    };

    if arguments.len() != 1 {
        return;
    }

    // Build the namespace-qualified property access: NS.E
    let ns_prop = || IRNode::PropertyAccess {
        object: Box::new(IRNode::Identifier(ns_name.to_string().into())),
        property: enum_name.to_string().into(),
    };

    // Replace the IIFE argument: E || (E = {}) → E = NS.E || (NS.E = {})
    arguments[0] = IRNode::BinaryExpr {
        left: Box::new(IRNode::Identifier(enum_name.to_string().into())),
        operator: "=".to_string().into(),
        right: Box::new(IRNode::LogicalOr {
            left: Box::new(ns_prop()),
            right: Box::new(IRNode::BinaryExpr {
                left: Box::new(ns_prop()),
                operator: "=".to_string().into(),
                right: Box::new(IRNode::empty_object()),
            }),
        }),
    };
}

fn find_unescaped_template_end(source: &str, template_start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut pos = template_start.checked_add(1)?;
    let mut escaped = false;
    while pos < bytes.len() {
        let byte = bytes[pos];
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'`' {
            return Some(pos);
        }
        pos += 1;
    }
    None
}

fn skip_quoted_source_text(source: &str, quote_start: usize) -> usize {
    let bytes = source.as_bytes();
    let quote = bytes[quote_start];
    if quote == b'`' {
        return find_unescaped_template_end(source, quote_start)
            .map(|end| end + 1)
            .unwrap_or(source.len());
    }

    let mut pos = quote_start + 1;
    while pos < bytes.len() {
        if bytes[pos] == b'\\' {
            pos += 2;
            continue;
        }
        if bytes[pos] == quote {
            return pos + 1;
        }
        pos += 1;
    }
    source.len()
}

fn find_next_code_module_keyword(source: &str, mut cursor: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'/' if bytes.get(cursor + 1) == Some(&b'/') => {
                cursor += 2;
                while cursor < bytes.len() && !matches!(bytes[cursor], b'\n' | b'\r') {
                    cursor += 1;
                }
            }
            b'/' if bytes.get(cursor + 1) == Some(&b'*') => {
                cursor += 2;
                while cursor + 1 < bytes.len()
                    && !(bytes[cursor] == b'*' && bytes[cursor + 1] == b'/')
                {
                    cursor += 1;
                }
                cursor = (cursor + 2).min(bytes.len());
            }
            b'\'' | b'"' | b'`' => {
                cursor = skip_quoted_source_text(source, cursor);
            }
            b'm' if source[cursor..].starts_with("module")
                && cursor
                    .checked_sub(1)
                    .and_then(|prev| bytes.get(prev))
                    .is_none_or(|byte| {
                        !byte.is_ascii_alphanumeric() && *byte != b'_' && *byte != b'$'
                    })
                && bytes.get(cursor + "module".len()).is_none_or(|byte| {
                    !byte.is_ascii_alphanumeric() && *byte != b'_' && *byte != b'$'
                }) =>
            {
                return Some(cursor);
            }
            _ => {
                cursor += source[cursor..]
                    .chars()
                    .next()
                    .map(char::len_utf8)
                    .unwrap_or(1);
            }
        }
    }
    None
}

impl<'a> Printer<'a> {
    // =========================================================================
    // Namespace / Module Declarations
    // =========================================================================

    pub(in crate::emitter) fn emit_module_declaration(&mut self, node: &Node, idx: NodeIndex) {
        let Some(module) = self.arena.get_module(node) else {
            return;
        };

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

        // ES5 target: Transform namespace to IIFE pattern
        if self.ctx.target_es5 {
            use crate::transforms::NamespaceES5Emitter;
            let use_cjs = self.pending_cjs_namespace_export_fold;
            if use_cjs {
                self.pending_cjs_namespace_export_fold = false;
            }
            let mut es5_emitter = NamespaceES5Emitter::with_commonjs(self.arena, use_cjs);
            es5_emitter.set_target_es5(self.ctx.target_es5);
            es5_emitter.set_remove_comments(self.ctx.options.remove_comments);
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
            let ns_name = self.get_identifier_text_idx(module.name);
            if !ns_name.is_empty() {
                // When the namespace name was already declared (e.g., by a
                // function or class), suppress the `var` declaration.
                if self.declared_namespace_names.contains(&ns_name) {
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
        self.emit_namespace_iife(&module, parent_name.as_deref());
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

    /// Emit a namespace/module as an IIFE for ES6+ targets.
    /// `parent_name` is set when this is a nested namespace (e.g., Bar inside Foo).
    fn emit_namespace_iife(
        &mut self,
        module: &tsz_parser::parser::node::ModuleData,
        parent_name: Option<&str>,
    ) {
        let name = self.get_identifier_text_idx(module.name);
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
        let should_declare = !self.declared_namespace_names.contains(&name);
        if should_declare {
            let keyword = if (self.in_namespace_iife || self.function_scope_depth > 0)
                && !self.ctx.target_es5
            {
                "let"
            } else {
                "var"
            };
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
                    self.emit_namespace_iife(&inner_module, Some(&iife_param));
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
                self.in_namespace_iife = true;
                // Set the scope end so import alias reference searching is
                // limited to this namespace body (not sibling namespaces).
                if let Some(body_node) = self.arena.get(module.body) {
                    self.namespace_scope_end = body_node.end;
                }
                let prev_parent_ns = self.parent_namespace_name.clone();
                self.parent_namespace_name = parent_name
                    .map(std::borrow::ToOwned::to_owned)
                    .or_else(|| prev_ns_name.clone());
                self.current_namespace_name = Some(iife_param.clone());
                self.emit_namespace_body_statements(module, &iife_param);
                self.in_namespace_iife = prev;
                self.namespace_scope_end = prev_scope_end;
                self.current_namespace_name = prev_ns_name;
                self.parent_namespace_name = prev_parent_ns;
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
        } else if cjs_export_fold {
            if self.in_system_execute_body {
                // System module: (N || (exports_1("N", N = {})))
                self.write(&name);
                self.write(" || (exports_1(\"");
                self.write(&name);
                self.write("\", ");
                self.write(&name);
                self.write(" = {})));");
            } else {
                // CJS export fold: (N || (exports.N = N = {}))
                self.write(&name);
                self.write(" || (exports.");
                self.write(&name);
                self.write(" = ");
                self.write(&name);
                self.write(" = {}));");
            }
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

    /// Like `text_has_binding_named` but skips `namespace`/`module`
    /// declarations. Used for dotted-namespace conflict detection where
    /// a nested sub-namespace shouldn't be treated as shadowing the
    /// enclosing namespace's IIFE param.
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
                        let keywords: &[&str] = &[
                            "var",
                            "let",
                            "const",
                            "function",
                            "class",
                            "import",
                            "private",
                            "public",
                            "protected",
                            "readonly",
                            "override",
                        ];
                        for &kw in keywords {
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
                    }
                }
                i = abs + 1;
            } else {
                break;
            }
        }
        false
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
        // Use source text scan: search for the identifier as a binding in the body.
        // This catches parameters, local vars, nested functions/classes at any depth.
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
                    Self::text_has_binding_named(&masked, ns_name)
                }
                Err(_) => false,
            };
        }
        false
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
            _ => false,
        }
    }

    /// Check if source text contains a binding (variable, function, class, parameter,
    /// catch clause, etc.) with the given name. Uses a simple text scan that looks
    /// for the identifier in declaration contexts.
    fn text_has_binding_named(text: &str, name: &str) -> bool {
        // Strip comments and string literals to avoid false positives from
        // commented-out code like `//import m6 = require('')`
        let stripped = Self::strip_comments(text);
        let text = &stripped;
        let name_bytes = name.as_bytes();
        let text_bytes = text.as_bytes();
        let name_len = name_bytes.len();

        // Scan for occurrences of the identifier that could be bindings
        let mut i = 0;
        while i + name_len <= text_bytes.len() {
            // Find next occurrence of the name
            if let Some(pos) = text[i..].find(name) {
                let abs = i + pos;
                // Check word boundaries
                let before_ok = abs == 0
                    || !text_bytes[abs - 1].is_ascii_alphanumeric()
                        && text_bytes[abs - 1] != b'_'
                        && text_bytes[abs - 1] != b'$';
                let after_end = abs + name_len;
                let after_ok = after_end >= text_bytes.len()
                    || !text_bytes[after_end].is_ascii_alphanumeric()
                        && text_bytes[after_end] != b'_'
                        && text_bytes[after_end] != b'$';

                if before_ok && after_ok {
                    // Check if this is a binding context by looking at what precedes it.
                    // Skip whitespace backwards to find the preceding token.
                    let mut p = abs;
                    while p > 0 && text_bytes[p - 1].is_ascii_whitespace() {
                        p -= 1;
                    }
                    // Check for binding keywords/contexts:
                    // - `var/let/const NAME`
                    // - `function NAME`
                    // - `class NAME`
                    // - `(NAME` or `, NAME` (function parameters)
                    // - `catch (NAME`
                    if p > 0 {
                        let prev_char = text_bytes[p - 1];
                        // Parameter context: `(NAME` or `, NAME`
                        if prev_char == b'(' || prev_char == b',' {
                            return true;
                        }
                        // Check for keywords ending at position p
                        let preceding = &text[..p];
                        let keywords: &[&str] = &[
                            "var",
                            "let",
                            "const",
                            "function",
                            "class",
                            "import",
                            "module",
                            "namespace",
                            // TS parameter modifiers
                            "private",
                            "public",
                            "protected",
                            "readonly",
                            "override",
                        ];
                        for &kw in keywords {
                            if preceding.ends_with(kw) {
                                let kw_start = p - kw.len();
                                let kw_before_ok = kw_start == 0
                                    || !text_bytes[kw_start - 1].is_ascii_alphanumeric()
                                        && text_bytes[kw_start - 1] != b'_'
                                        && text_bytes[kw_start - 1] != b'$';
                                if kw_before_ok {
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
    fn collect_namespace_exported_names(
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
            _ => None,
        }?;
        if name.is_empty() { None } else { Some(name) }
    }

    /// Collect names of exported classes, functions, and enums from a namespace.
    /// These names need qualification in REOPENED blocks of the same namespace
    /// but NOT in their own declaration block (since they're locally in scope).
    fn collect_namespace_class_fn_enum_names(
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
    fn collect_namespace_current_class_fn_enum_names(
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
    fn collect_namespace_local_var_names(
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
                                && let Some(name_node) = self.arena.get(decl.name)
                                && let Some(ident) = self.arena.get_identifier(name_node)
                            {
                                names.insert(ident.escaped_text.clone());
                            }
                        }
                    }
                }
            }
        }
        names
    }

    fn collect_namespace_local_module_names(
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

    fn collect_dotted_namespace_children_from_source(
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
                let Some(rest) = text.get(name_start..) else {
                    break;
                };
                let Some(after_parent) = rest.strip_prefix(parent) else {
                    search_start = name_start;
                    continue;
                };
                let Some(child_rest) = after_parent.strip_prefix('.') else {
                    search_start = name_start;
                    continue;
                };
                let child: String = child_rest
                    .chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                    .collect();
                if !child.is_empty() {
                    children.insert(child);
                }
                search_start = name_start;
            }
        }
        children
    }

    /// Emit body statements of a namespace IIFE, handling exports.
    fn emit_namespace_body_statements(
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
            let leaf_name = self.get_identifier_text_idx(module.name);
            if !leaf_name.is_empty() {
                local_exports
                    .extend(self.collect_dotted_namespace_children_from_source(&leaf_name));
            }
            let mut ancestor_qualifiers = prev_ancestor_qualifiers.clone();
            let mut parent_exports = self
                .parent_namespace_name
                .as_ref()
                .and_then(|parent| self.namespace_prior_exports.get(parent))
                .map(|exports| {
                    exports
                        .iter()
                        .cloned()
                        .collect::<rustc_hash::FxHashSet<_>>()
                })
                .unwrap_or_default();
            if let Some(parent) = self.parent_namespace_name.as_ref()
                && let Some(exports) = self.namespace_prior_exports.get(parent)
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
                && let Some(class_exports) = self.namespace_prior_class_fn_enum_exports.get(parent)
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
                entry.extend(local_exports.iter().cloned());
                if ns_name != leaf_name {
                    self.namespace_prior_exports
                        .entry(ns_name.clone())
                        .or_default()
                        .extend(local_exports.iter().cloned());
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
                            self.emit_namespace_exported_variable(
                                inner_idx,
                                &ns_name,
                                stmt_node,
                                upper_bound,
                                &destructuring_export_temps,
                            );
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
                        } else {
                            // class/function/enum: emit without export, then add assignment
                            let export_names = self.get_export_names_from_clause(inner_idx);

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
                            self.in_namespace_iife = true;
                            self.emit(inner_idx);
                            self.in_namespace_iife = prev;
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
                    // Non-exported class in namespace: just emit it
                    let prev = self.in_namespace_iife;
                    self.in_namespace_iife = true;
                    self.emit(stmt_idx);
                    self.in_namespace_iife = prev;
                    let token_end = self.find_token_end_before_trivia(stmt_node.pos, upper_bound);
                    self.emit_trailing_comments_before(token_end, body_close_pos);
                    // Only write newline if not already at line start (class
                    // declarations with lowered static fields already end with
                    // write_line after the last ClassName.field = value;).
                    if !self.writer.is_at_line_start() {
                        self.write_line();
                    }
                } else if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                    // Nested namespace: recurse (emit_namespace_iife adds its own newline)
                    self.emit(stmt_idx);
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
            self.namespace_exported_names = prev_exported;
            self.namespace_parent_exported_names = prev_parent_exported;
            self.namespace_ancestor_export_qualifiers = prev_ancestor_qualifiers;
            self.namespace_current_class_fn_enum_names = prev_current_class_fn_enum;
        }
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

#[cfg(test)]
#[path = "namespace/tests.rs"]
mod tests;
