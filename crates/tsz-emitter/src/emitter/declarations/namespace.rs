use super::super::Printer;
use crate::transforms::ir::IRNode;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

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

    /// Emit a namespace/module as an IIFE for ES6+ targets.
    /// `parent_name` is set when this is a nested namespace (e.g., Bar inside Foo).
    fn emit_namespace_iife(
        &mut self,
        module: &tsz_parser::parser::node::ModuleData,
        parent_name: Option<&str>,
    ) {
        let name = self.get_identifier_text_idx(module.name);

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
                if let Some(inner_module) = self.arena.get_module(body_node) {
                    let inner_module = inner_module.clone();
                    let prev_declared = std::mem::take(&mut self.declared_namespace_names);
                    self.emit_namespace_iife(&inner_module, Some(&name));
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
                self.parent_namespace_name = prev_ns_name.clone();
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
    fn namespace_body_has_name_conflict(
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
                return inner_name == ns_name;
            }
            return false;
        }
        // Use source text scan: search for the identifier as a binding in the body.
        // This catches parameters, local vars, nested functions/classes at any depth.
        if let Some(text) = self.source_text {
            // safe_slice: C → migrated. A bad span here would silently report
            // "no binding found", which can change namespace shadowing
            // decisions and emit incorrectly. Surface span errors instead of
            // returning a false-negative; fall back to false only when source
            // text is literally unavailable.
            return match crate::safe_slice::slice(
                text,
                body_node.pos as usize,
                body_node.end as usize,
            ) {
                Ok(body_text) => Self::text_has_binding_named(body_text, ns_name),
                Err(_) => false,
            };
        }
        false
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
        names
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
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export) = self.arena.get_export_decl(stmt_node) else {
                continue;
            };
            let Some(inner_node) = self.arena.get(export.export_clause) else {
                continue;
            };
            let name = match inner_node.kind {
                k if k == syntax_kind_ext::CLASS_DECLARATION => self
                    .arena
                    .get_class(inner_node)
                    .map(|c| self.get_identifier_text_idx(c.name)),
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => self
                    .arena
                    .get_function(inner_node)
                    .map(|f| self.get_identifier_text_idx(f.name)),
                k if k == syntax_kind_ext::ENUM_DECLARATION => self
                    .arena
                    .get_enum(inner_node)
                    .map(|e| self.get_identifier_text_idx(e.name)),
                _ => None,
            };
            if let Some(n) = name
                && !n.is_empty()
            {
                names.push(n);
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
            let mut local_exports = self.collect_namespace_exported_names(module);
            // Collect class/function/enum names for future reopenings (before mutable borrow)
            let class_fn_enum_names = self.collect_namespace_class_fn_enum_names(module);
            // Merge in exports from prior blocks of the same namespace (cross-block sharing)
            {
                let leaf_name = self.get_identifier_text_idx(module.name);
                // Use scope-qualified key to distinguish same-named namespaces
                // at different scopes (e.g., m1.m2 vs m4.m2). Reopenings at the
                // same scope share the same parent, so they get the same key.
                let root_name = if let Some(ref parent) = self.parent_namespace_name {
                    format!("{parent}.{leaf_name}")
                } else {
                    leaf_name.clone()
                };
                if !leaf_name.is_empty() {
                    let entry = self.namespace_prior_exports.entry(root_name).or_default();
                    // Merge PRIOR exports into local set BEFORE adding this block's names.
                    // This ensures names from earlier blocks are qualified in this block,
                    // but this block's own declarations are NOT qualified here.
                    for name in entry.iter() {
                        local_exports.insert(name.clone());
                    }
                    // Add this block's variable exports for future reopenings
                    entry.extend(local_exports.iter().cloned());
                    // Register class/function/enum names for qualification in
                    // subsequent reopenings only (not in this block's local_exports).
                    entry.extend(class_fn_enum_names);
                }
            }
            // Remove locally-declared non-exported names — they shadow prior exports
            let local_names = self.collect_namespace_local_var_names(body_node);
            for name in &local_names {
                local_exports.remove(name);
            }
            self.namespace_exported_names = local_exports;

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
            // Restore previous exported names
            self.namespace_exported_names = prev_exported;
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
    /// This is more permissive than `namespace_alias_target_has_runtime_value` which
    /// treats `declare function` as having no runtime emit (correct for namespace aliasing
    /// but not for `export default`).
    pub(in crate::emitter) fn export_default_target_has_runtime_value(
        &self,
        target: NodeIndex,
    ) -> bool {
        let node = match self.arena.get(target) {
            Some(n) => n,
            None => return true, // conservative default
        };

        if node.kind != SyntaxKind::Identifier as u16 {
            return true; // qualified names etc. are conservatively treated as runtime
        }

        let name = self.get_identifier_text_idx(target);
        if name.is_empty() {
            return true;
        }

        // Search source file statements for the declaration
        let statements = self.scope_statements_for_runtime_lookup(None);
        if statements.is_empty() {
            return true; // conservative: can't resolve, assume runtime
        }

        let mut found_type_only = false;
        let mut found_value = false;

        for stmt_idx in &statements {
            let Some(stmt_node) = self.arena.get(*stmt_idx) else {
                continue;
            };

            // Unwrap export declarations to find the inner declaration
            let check_node = if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                if let Some(export) = self.arena.get_export_decl(stmt_node) {
                    self.arena.get(export.export_clause)
                } else {
                    None
                }
            } else {
                Some(stmt_node)
            };

            let Some(check) = check_node else { continue };

            match check.kind {
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                    if let Some(iface) = self.arena.get_interface(check)
                        && self.get_identifier_text_idx(iface.name) == name
                    {
                        found_type_only = true;
                    }
                }
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                    if let Some(ta) = self.arena.get_type_alias(check)
                        && self.get_identifier_text_idx(ta.name) == name
                    {
                        found_type_only = true;
                    }
                }
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                    if let Some(func) = self.arena.get_function(check)
                        && self.get_identifier_text_idx(func.name) == name
                    {
                        found_value = true;
                    }
                }
                k if k == syntax_kind_ext::CLASS_DECLARATION => {
                    if let Some(class) = self.arena.get_class(check)
                        && self.get_identifier_text_idx(class.name) == name
                    {
                        found_value = true;
                    }
                }
                k if k == syntax_kind_ext::ENUM_DECLARATION => {
                    if let Some(enum_decl) = self.arena.get_enum(check)
                        && self.get_identifier_text_idx(enum_decl.name) == name
                    {
                        found_value = true;
                    }
                }
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                    let names = self.collect_variable_names_from_node(check);
                    if names.contains(&name.to_string()) {
                        found_value = true;
                    }
                }
                k if k == syntax_kind_ext::MODULE_DECLARATION => {
                    if let Some(module) = self.arena.get_module(check)
                        && self.get_identifier_text_idx(module.name) == name
                    {
                        // Ambient namespaces with value members still represent
                        // a runtime object that may exist elsewhere, so aliases
                        // to their value members must be preserved.
                        if self.module_decl_has_runtime_alias_target(module) {
                            found_value = true;
                        } else {
                            found_type_only = true;
                        }
                    }
                }
                _ => {}
            }
        }

        // If we found a value declaration, it has runtime value
        // even if there's also a type declaration with the same name
        if found_value {
            return true;
        }
        // If we only found type declarations, it's type-only
        if found_type_only {
            return false;
        }
        // Unresolved: conservative default - assume runtime value
        true
    }

    fn module_decl_has_runtime_alias_target(
        &self,
        module: &tsz_parser::parser::node::ModuleData,
    ) -> bool {
        if self.arena.is_declare(&module.modifiers) {
            return self.ambient_module_body_has_runtime_value(module.body);
        }

        self.is_instantiated_module(module.body)
    }

    fn ambient_module_body_has_runtime_value(&self, module_body: NodeIndex) -> bool {
        let Some(body_node) = self.arena.get(module_body) else {
            return false;
        };

        if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            let Some(inner_module) = self.arena.get_module(body_node) else {
                return false;
            };
            return self.module_decl_has_runtime_alias_target(inner_module);
        }

        let Some(block) = self.arena.get_module_block(body_node) else {
            return false;
        };
        let Some(statements) = &block.statements else {
            return false;
        };

        statements.nodes.iter().any(|&stmt_idx| {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                return false;
            };

            match stmt_node.kind {
                k if k == syntax_kind_ext::INTERFACE_DECLARATION
                    || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION =>
                {
                    false
                }
                k if k == syntax_kind_ext::EXPORT_DECLARATION => self
                    .arena
                    .get_export_decl(stmt_node)
                    .filter(|export| !export.is_type_only)
                    .and_then(|export| self.arena.get(export.export_clause))
                    .is_some_and(|inner| self.ambient_namespace_statement_has_runtime_value(inner)),
                _ => self.ambient_namespace_statement_has_runtime_value(stmt_node),
            }
        })
    }

    fn ambient_namespace_statement_has_runtime_value(&self, stmt_node: &Node) -> bool {
        match stmt_node.kind {
            k if k == syntax_kind_ext::INTERFACE_DECLARATION
                || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION =>
            {
                false
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => self
                .arena
                .get_module(stmt_node)
                .is_some_and(|module| self.module_decl_has_runtime_alias_target(module)),
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => self
                .arena
                .get_import_decl(stmt_node)
                .is_some_and(|import| !import.is_type_only),
            _ => true,
        }
    }

    pub(in crate::emitter) fn namespace_alias_target_has_runtime_value(
        &self,
        target: NodeIndex,
        scope_body: Option<NodeIndex>,
    ) -> bool {
        if let Some((has_runtime, _)) = self.resolve_entity_runtime_value(target, scope_body) {
            return has_runtime;
        }

        if scope_body.is_some() {
            return self
                .resolve_entity_runtime_value(target, None)
                .is_none_or(|(has_runtime, _)| has_runtime);
        }

        true
    }

    /// Resolve whether an entity name has runtime value semantics in a scope.
    /// Returns:
    /// - `None`: unresolved (caller should be conservative)
    /// - `(has_runtime, nested_scope)`:
    ///   - `has_runtime`: whether the resolved symbol exists at runtime
    ///   - `nested_scope`: module body for namespace-qualified lookup continuation
    fn resolve_entity_runtime_value(
        &self,
        entity: NodeIndex,
        scope_body: Option<NodeIndex>,
    ) -> Option<(bool, Option<NodeIndex>)> {
        let node = self.arena.get(entity)?;

        if let Some(qualified) = self.arena.get_qualified_name(node) {
            let left = self.resolve_entity_runtime_value(qualified.left, scope_body)?;
            if !left.0 {
                return Some((false, None));
            }
            if let Some(next_scope) = left.1 {
                return self
                    .resolve_entity_runtime_value(qualified.right, Some(next_scope))
                    .or(Some((true, None)));
            }
            return Some((true, None));
        }

        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let name = self.get_identifier_text_idx(entity);
        if name.is_empty() {
            return None;
        }

        let statements = self.scope_statements_for_runtime_lookup(scope_body);
        if statements.is_empty() {
            return None;
        }

        let mut matched = false;
        let mut has_runtime = false;
        let mut nested_scope = None;

        for stmt_idx in statements {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let Some((stmt_runtime, stmt_scope)) =
                self.statement_runtime_for_name(stmt_node, &name, scope_body)
            else {
                continue;
            };

            matched = true;
            if stmt_runtime {
                has_runtime = true;
                if nested_scope.is_none() {
                    nested_scope = stmt_scope;
                }
            }
        }

        if matched {
            Some((has_runtime, nested_scope))
        } else {
            None
        }
    }

    fn scope_statements_for_runtime_lookup(&self, scope_body: Option<NodeIndex>) -> Vec<NodeIndex> {
        if let Some(scope_idx) = scope_body {
            let Some(scope_node) = self.arena.get(scope_idx) else {
                return Vec::new();
            };

            if let Some(module) = self.arena.get_module(scope_node) {
                return self.scope_statements_for_runtime_lookup(Some(module.body));
            }

            if let Some(block) = self.arena.get_module_block(scope_node)
                && let Some(stmts) = &block.statements
            {
                return stmts.nodes.clone();
            }

            if let Some(source) = self.arena.get_source_file(scope_node) {
                return source.statements.nodes.clone();
            }

            return Vec::new();
        }

        for node in &self.arena.nodes {
            if node.kind == syntax_kind_ext::SOURCE_FILE
                && let Some(source) = self.arena.get_source_file(node)
            {
                return source.statements.nodes.clone();
            }
        }

        Vec::new()
    }

    fn statement_runtime_for_name(
        &self,
        stmt_node: &Node,
        name: &str,
        scope_body: Option<NodeIndex>,
    ) -> Option<(bool, Option<NodeIndex>)> {
        match stmt_node.kind {
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                let export = self.arena.get_export_decl(stmt_node)?;
                let inner = self.arena.get(export.export_clause)?;
                self.statement_runtime_for_name(inner, name, scope_body)
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                let module = self.arena.get_module(stmt_node)?;
                if self.get_identifier_text_idx(module.name) != name {
                    return None;
                }
                let runtime = self.module_decl_has_runtime_alias_target(module);
                Some((runtime, if runtime { Some(module.body) } else { None }))
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                let class = self.arena.get_class(stmt_node)?;
                if self.get_identifier_text_idx(class.name) != name {
                    return None;
                }
                let runtime = !self.arena.is_declare(&class.modifiers);
                Some((runtime, None))
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                let func = self.arena.get_function(stmt_node)?;
                if self.get_identifier_text_idx(func.name) != name {
                    return None;
                }
                let runtime = !self.arena.is_declare(&func.modifiers) && func.body.is_some();
                Some((runtime, None))
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                let enum_decl = self.arena.get_enum(stmt_node)?;
                if self.get_identifier_text_idx(enum_decl.name) != name {
                    return None;
                }
                let runtime = !self.arena.is_declare(&enum_decl.modifiers)
                    && !self
                        .arena
                        .has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword);
                Some((runtime, None))
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                let iface = self.arena.get_interface(stmt_node)?;
                if self.get_identifier_text_idx(iface.name) != name {
                    return None;
                }
                Some((false, None))
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                let type_alias = self.arena.get_type_alias(stmt_node)?;
                if self.get_identifier_text_idx(type_alias.name) != name {
                    return None;
                }
                Some((false, None))
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                // `var X`, `let X`, `export var X`, etc.
                // Structure: VariableStatement → declarations: [VariableDeclarationList]
                //            VariableDeclarationList → declarations: [VariableDeclaration, ...]
                let var_stmt = self.arena.get_variable(stmt_node)?;
                let is_declare = self.arena.is_declare(&var_stmt.modifiers);
                for &list_or_decl_idx in &var_stmt.declarations.nodes {
                    let Some(list_or_decl_node) = self.arena.get(list_or_decl_idx) else {
                        continue;
                    };
                    // May be a VariableDeclarationList wrapping individual declarations
                    if list_or_decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                        let Some(decl_list) = self.arena.get_variable(list_or_decl_node) else {
                            continue;
                        };
                        for &decl_idx in &decl_list.declarations.nodes {
                            let Some(decl_node) = self.arena.get(decl_idx) else {
                                continue;
                            };
                            let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                                continue;
                            };
                            if self.get_identifier_text_idx(decl.name) == name {
                                return Some((!is_declare, None));
                            }
                        }
                    } else {
                        // Direct VariableDeclaration
                        let Some(decl) = self.arena.get_variable_declaration(list_or_decl_node)
                        else {
                            continue;
                        };
                        if self.get_identifier_text_idx(decl.name) == name {
                            return Some((!is_declare, None));
                        }
                    }
                }
                None
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                let import = self.arena.get_import_decl(stmt_node)?;
                if self.get_identifier_text_idx(import.import_clause) != name {
                    return None;
                }
                let runtime = if let Some(spec_node) = self.arena.get(import.module_specifier) {
                    match spec_node.kind {
                        kind if kind == SyntaxKind::Identifier as u16
                            || kind == syntax_kind_ext::QUALIFIED_NAME =>
                        {
                            self.namespace_alias_target_has_runtime_value(
                                import.module_specifier,
                                scope_body,
                            )
                        }
                        _ => self.import_decl_has_runtime_value(import),
                    }
                } else {
                    self.import_decl_has_runtime_value(import)
                };
                Some((runtime, None))
            }
            _ => None,
        }
    }

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
    ) {
        let Some(var_node) = self.arena.get(var_stmt_idx) else {
            return;
        };
        let Some(var_stmt) = self.arena.get_variable(var_node) else {
            return;
        };

        // Collect all initialized (name, initializer) pairs across declaration lists.
        // TSC emits multiple exports as a comma expression: `ns.a = 1, ns.c = 2;`
        let mut assignments: Vec<(String, NodeIndex)> = Vec::new();

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

                let mut names = Vec::new();
                self.collect_binding_names(decl.name, &mut names);
                for name in names {
                    assignments.push((name, decl.initializer));
                }
            }
        }

        // Emit as comma expression: ns.a = 1, ns.c = 2;
        if !assignments.is_empty() {
            for (i, (name, init)) in assignments.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.write(ns_name);
                self.write(".");
                self.write(name);
                self.write(" = ");
                self.emit_expression(*init);
            }
            self.write(";");
            let token_end = self.find_token_end_before_trivia(outer_stmt.pos, comment_upper_bound);
            self.emit_trailing_comments_before(token_end, comment_upper_bound);
            self.write_line();
        }
    }

    /// Returns true when a variable statement node has no initializers in any of its
    /// declarators (e.g., `export var b: number;`).  Used to suppress orphaned leading
    /// comments for exported variable declarations that produce no runtime code.
    fn namespace_variable_has_no_initializers(&self, var_stmt_idx: NodeIndex) -> bool {
        let Some(var_node) = self.arena.get(var_stmt_idx) else {
            return false;
        };
        let Some(var_stmt) = self.arena.get_variable(var_node) else {
            return false;
        };
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
                if decl.initializer.is_some() {
                    return false;
                }
            }
        }
        true
    }

    /// Get export names from a declaration clause (function, class, variable, enum)
    fn get_export_names_from_clause(&self, clause_idx: NodeIndex) -> Vec<String> {
        let Some(node) = self.arena.get(clause_idx) else {
            return Vec::new();
        };
        match node.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = self.arena.get_variable(node) {
                    return self.collect_variable_names(&var_stmt.declarations);
                }
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = self.arena.get_function(node)
                    && let Some(name_node) = self.arena.get(func.name)
                    && let Some(ident) = self.arena.get_identifier(name_node)
                {
                    return vec![ident.escaped_text.clone()];
                }
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = self.arena.get_class(node)
                    && let Some(name_node) = self.arena.get(class.name)
                    && let Some(ident) = self.arena.get_identifier(name_node)
                {
                    return vec![ident.escaped_text.clone()];
                }
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = self.arena.get_enum(node)
                    && let Some(name_node) = self.arena.get(enum_decl.name)
                    && let Some(ident) = self.arena.get_identifier(name_node)
                {
                    return vec![ident.escaped_text.clone()];
                }
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                if let Some(import_decl) = self.arena.get_import_decl(node) {
                    let name = self.get_identifier_text_idx(import_decl.import_clause);
                    if !name.is_empty() {
                        return vec![name];
                    }
                }
            }
            _ => {}
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::emitter::ModuleKind;
    use crate::output::printer::{PrintOptions, Printer};
    use tsz_parser::ParserState;

    /// Regression test: type-only import-equals inside a namespace must not
    /// leave a phantom blank line. The import `import T = M1.I;` produces no
    /// JS output (type-only alias), but `emit_namespace_body_statements` used
    /// to call `write_line()` unconditionally, inserting an empty line between
    /// the IIFE opening brace and the first real statement.
    #[test]
    fn no_blank_line_for_type_only_import_alias_in_namespace() {
        let source = "namespace M1 {\n    export interface I {\n        foo();\n    }\n}\n\nnamespace M2 {\n    import T = M1.I;\n    class C implements T {\n        foo() {}\n    }\n}";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        // The IIFE body should NOT have a blank line after the opening brace.
        assert!(
            !output.contains("(function (M2) {\n\n"),
            "Should not have blank line after IIFE opening brace.\nOutput:\n{output}"
        );

        // The class should still be emitted correctly inside M2's IIFE
        assert!(
            output.contains("class C {"),
            "Class C should be emitted inside namespace M2.\nOutput:\n{output}"
        );
    }

    #[test]
    fn top_level_import_alias_to_ambient_namespace_value_emits_runtime_alias() {
        let source = "declare namespace foo { const await: any; }\n\n// await allowed in import=namespace when not a module\nimport await = foo.await;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(
            &parser.arena,
            PrintOptions {
                module: ModuleKind::ESNext,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("var await = foo.await;"),
            "Ambient namespace value aliases should be preserved in JS emit.\nOutput:\n{output}"
        );
    }

    #[test]
    fn top_level_import_alias_to_ambient_namespace_value_is_erased_in_modules() {
        let source = "export {};\ndeclare namespace foo { const await: any; }\n\n// await disallowed in import=namespace when in a module\nimport await = foo.await;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(
            &parser.arena,
            PrintOptions {
                module: ModuleKind::ESNext,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            !output.contains("var await = foo.await;"),
            "Module-scoped ambient namespace aliases should still be erased when unused.\nOutput:\n{output}"
        );
        assert!(
            output.contains("export {};"),
            "Module marker should be preserved when the alias is erased.\nOutput:\n{output}"
        );
    }

    /// When a namespace body has a variable with the same name as the namespace,
    /// the IIFE parameter must be renamed to avoid collision.
    /// E.g., `namespace m { export var m = ''; }` should emit `(function (m_1) { m_1.m = ''; })`.
    #[test]
    fn namespace_iife_param_renamed_for_variable_conflict() {
        let source = "namespace m {\n  export var m = '';\n}";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("(function (m_1)"),
            "Namespace IIFE parameter should be renamed to m_1 when body has 'var m'.\nOutput:\n{output}"
        );
        assert!(
            output.contains("m_1.m = '';"),
            "Exported variable should use renamed parameter m_1.\nOutput:\n{output}"
        );
    }

    /// When a namespace body has an import-equals with the same name as the namespace,
    /// the IIFE parameter must be renamed.
    /// E.g., `namespace A.M { import M = Z.M; ... }` should emit `(function (M_1) { ... })`.
    #[test]
    fn namespace_iife_param_renamed_for_import_equals_conflict() {
        let source = "namespace Z {\n  export namespace M {\n    export function bar() { return ''; }\n  }\n}\nnamespace A {\n  export namespace M {\n    import M = Z.M;\n    export function bar() {}\n    M.bar();\n  }\n}";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        // The inner M namespace IIFE should have parameter renamed to M_1
        assert!(
            output.contains("(function (M_1)"),
            "Namespace IIFE parameter should be renamed to M_1 when body has 'import M = ...'.\nOutput:\n{output}"
        );
    }
}
