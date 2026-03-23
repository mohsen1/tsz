//! Transform dispatch logic for the emitter.
//!
//! Contains the `EmitDirective` enum and all methods related to applying
//! transform directives during emission (Phase 2 architecture).

use super::*;
use std::sync::Arc;
use tracing::debug;

enum EmitDirective {
    Identity,
    ES5Class {
        class_node: NodeIndex,
    },
    ES5ClassExpression {
        class_node: NodeIndex,
    },
    ES5Namespace {
        namespace_node: NodeIndex,
        should_declare_var: bool,
    },
    ES5Enum {
        enum_node: NodeIndex,
    },
    CommonJSExport {
        names: Arc<[IdentifierId]>,
        is_default: bool,
        inner: Box<Self>,
    },
    CommonJSExportDefaultExpr,
    CommonJSExportDefaultClassES5 {
        class_node: NodeIndex,
    },
    ES5ArrowFunction {
        arrow_node: NodeIndex,
        captures_this: bool,
        captures_arguments: bool,
        class_alias: Option<Arc<str>>,
    },
    ES5AsyncFunction {
        function_node: NodeIndex,
    },
    ES5ForOf {
        for_of_node: NodeIndex,
    },
    ES5ObjectLiteral {
        object_literal: NodeIndex,
    },
    ES5ArrayLiteral {
        array_literal: NodeIndex,
    },
    ES5CallSpread {
        call_expr: NodeIndex,
    },
    ES5VariableDeclarationList {
        decl_list: NodeIndex,
    },
    ES5FunctionParameters {
        function_node: NodeIndex,
    },
    ES5TemplateLiteral,
    SubstituteThis {
        capture_name: Arc<str>,
    },
    SubstituteArguments,
    ES5SuperCall,
    TC39Decorators {
        class_node: NodeIndex,
        function_name: Option<String>,
    },
    ModuleWrapper {
        format: crate::context::transform::ModuleFormat,
        dependencies: Arc<[String]>,
    },
    Chain(Vec<Self>),
}

impl<'a> Printer<'a> {
    // =========================================================================
    // Transform Application (Phase 2 Architecture)
    // =========================================================================

    fn emit_directive_from_transform(directive: &TransformDirective) -> EmitDirective {
        match directive {
            TransformDirective::Identity => EmitDirective::Identity,
            TransformDirective::ES5Class { class_node, .. } => EmitDirective::ES5Class {
                class_node: *class_node,
            },
            TransformDirective::ES5ClassExpression { class_node } => {
                EmitDirective::ES5ClassExpression {
                    class_node: *class_node,
                }
            }
            TransformDirective::ES5Namespace {
                namespace_node,
                should_declare_var,
            } => EmitDirective::ES5Namespace {
                namespace_node: *namespace_node,
                should_declare_var: *should_declare_var,
            },
            TransformDirective::ES5Enum { enum_node } => EmitDirective::ES5Enum {
                enum_node: *enum_node,
            },
            TransformDirective::CommonJSExport {
                names,
                is_default,
                inner,
            } => EmitDirective::CommonJSExport {
                names: std::sync::Arc::clone(names),
                is_default: *is_default,
                inner: Box::new(Self::emit_directive_from_transform(inner.as_ref())),
            },
            TransformDirective::CommonJSExportDefaultExpr => {
                EmitDirective::CommonJSExportDefaultExpr
            }
            TransformDirective::CommonJSExportDefaultClassES5 { class_node } => {
                EmitDirective::CommonJSExportDefaultClassES5 {
                    class_node: *class_node,
                }
            }
            TransformDirective::ES5ArrowFunction {
                arrow_node,
                captures_this,
                captures_arguments,
                class_alias,
            } => EmitDirective::ES5ArrowFunction {
                arrow_node: *arrow_node,
                captures_this: *captures_this,
                captures_arguments: *captures_arguments,
                class_alias: class_alias.clone(),
            },
            TransformDirective::ES5AsyncFunction { function_node } => {
                EmitDirective::ES5AsyncFunction {
                    function_node: *function_node,
                }
            }
            TransformDirective::ES5ForOf { for_of_node } => EmitDirective::ES5ForOf {
                for_of_node: *for_of_node,
            },
            TransformDirective::ES5ObjectLiteral { object_literal } => {
                EmitDirective::ES5ObjectLiteral {
                    object_literal: *object_literal,
                }
            }
            TransformDirective::ES5ArrayLiteral { array_literal } => {
                EmitDirective::ES5ArrayLiteral {
                    array_literal: *array_literal,
                }
            }
            TransformDirective::ES5CallSpread { call_expr } => EmitDirective::ES5CallSpread {
                call_expr: *call_expr,
            },
            TransformDirective::ES5VariableDeclarationList { decl_list } => {
                EmitDirective::ES5VariableDeclarationList {
                    decl_list: *decl_list,
                }
            }
            TransformDirective::ES5FunctionParameters { function_node } => {
                EmitDirective::ES5FunctionParameters {
                    function_node: *function_node,
                }
            }
            TransformDirective::ES5TemplateLiteral { .. } => EmitDirective::ES5TemplateLiteral,
            TransformDirective::SubstituteThis { capture_name } => EmitDirective::SubstituteThis {
                capture_name: std::sync::Arc::clone(capture_name),
            },
            TransformDirective::SubstituteArguments => EmitDirective::SubstituteArguments,
            TransformDirective::ES5SuperCall => EmitDirective::ES5SuperCall,
            TransformDirective::ModuleWrapper {
                format,
                dependencies,
            } => EmitDirective::ModuleWrapper {
                format: *format,
                dependencies: std::sync::Arc::clone(dependencies),
            },
            TransformDirective::TC39Decorators {
                class_node,
                function_name,
            } => EmitDirective::TC39Decorators {
                class_node: *class_node,
                function_name: function_name.clone(),
            },
            TransformDirective::Chain(directives) => {
                let mut flattened = Vec::new();
                Self::flatten_emit_chain(directives.as_slice(), &mut flattened);
                EmitDirective::Chain(flattened)
            }
        }
    }

    fn flatten_emit_chain(directives: &[TransformDirective], out: &mut Vec<EmitDirective>) {
        for directive in directives {
            match directive {
                TransformDirective::Chain(inner) => {
                    Self::flatten_emit_chain(inner.as_slice(), out);
                }
                other => out.push(Self::emit_directive_from_transform(other)),
            }
        }
    }

    fn namespace_var_flag_from_directive(directive: &EmitDirective) -> Option<bool> {
        match directive {
            EmitDirective::ES5Namespace {
                should_declare_var, ..
            } => Some(*should_declare_var),
            EmitDirective::Chain(items) => {
                for item in items {
                    if let Some(flag) = Self::namespace_var_flag_from_directive(item) {
                        return Some(flag);
                    }
                }
                None
            }
            EmitDirective::CommonJSExport { inner, .. } => {
                Self::namespace_var_flag_from_directive(inner.as_ref())
            }
            _ => None,
        }
    }

    /// Apply a transform directive to a node.
    /// This is called when a node has an entry in the `TransformContext`.
    pub(super) fn apply_transform(&mut self, node: &Node, idx: NodeIndex) {
        let Some(directive) = self.transforms.get(idx) else {
            // No transform, emit normally (should not happen if has_transform returned true)
            self.emit_node_default(node, idx);
            return;
        };

        let directive = Self::emit_directive_from_transform(directive);

        match directive {
            EmitDirective::Identity => {
                // No transformation needed, emit as-is
                self.emit_node_default(node, idx);
            }

            EmitDirective::ES5Class { class_node } => {
                debug!(
                    "Printer ES5Class start (idx={}, class_node={})",
                    idx.0, class_node.0
                );
                // Collect leading comments before the class so the ES5 emitter
                // can place them after WeakMap storage declarations.
                let leading_comments = self.collect_leading_comments(node.pos);
                let leading_comment_text = if !leading_comments.is_empty() {
                    self.comment_emit_idx += leading_comments.len();
                    let combined: Vec<String> =
                        leading_comments.into_iter().map(|(text, _)| text).collect();
                    Some(combined.join("\n"))
                } else {
                    None
                };
                let mut es5_emitter = self.create_es5_class_emitter_with_decorators(class_node);
                if let Some(comment) = leading_comment_text {
                    es5_emitter.set_leading_comment(comment);
                }
                let es5_output = es5_emitter.emit_class(class_node);
                debug!(
                    "Printer ES5Class end (idx={}, class_node={}, output_len={})",
                    idx.0,
                    class_node.0,
                    es5_output.len()
                );
                let es5_mappings = es5_emitter.take_mappings();
                if !es5_mappings.is_empty() && self.writer.has_source_map() {
                    self.writer.write("");
                    let base_line = self.writer.current_line();
                    let base_column = self.writer.current_column();
                    self.writer
                        .add_offset_mappings(base_line, base_column, &es5_mappings);
                    self.writer.write(&es5_output);
                } else {
                    self.write(&es5_output);
                }
                // Emit any trailing comment from the class's closing `}` line
                // (e.g., `class Foo { ... } // comment` → `}()); // comment`).
                // The ES5 class emitter (IR printer) already handles comments INSIDE the
                // class body, so we need to:
                // 1. Advance comment_emit_idx past all inner class comments (< class_close_pos)
                // 2. Emit any trailing comment ON the closing `}` line
                // 3. Then skip_comments_for_erased_node cleans up any remaining
                let class_close_pos = self.find_token_end_before_trivia(node.pos, node.end);
                // Step 1: Skip all comments inside the class body (before class_close_pos)
                while self.comment_emit_idx < self.all_comments.len()
                    && self.all_comments[self.comment_emit_idx].pos < class_close_pos
                {
                    self.comment_emit_idx += 1;
                }
                // Step 2: Emit trailing comment on the class closing `}` line (if any)
                self.emit_trailing_comments(class_close_pos);
                // Skip comments within the class range - the ES5 class emitter
                // doesn't use the main comment system, so we must advance past them
                // to prevent them from being dumped at end of file.
                self.skip_comments_for_erased_node(node);
            }
            EmitDirective::ES5ClassExpression { class_node } => {
                self.emit_class_expression_es5(class_node);
            }

            EmitDirective::ES5Namespace {
                namespace_node,
                should_declare_var,
            } => {
                let mut ns_name_for_exports = String::new();
                if let Some(ns_node) = self.arena.get(namespace_node)
                    && let Some(ns_data) = self.arena.get_module(ns_node)
                {
                    let ns_name = self.get_identifier_text_idx(ns_data.name);
                    if !ns_name.is_empty() {
                        ns_name_for_exports = ns_name.clone();
                        self.declared_namespace_names.insert(ns_name);
                    }
                }
                let use_cjs = self.pending_cjs_namespace_export_fold;
                if use_cjs {
                    self.pending_cjs_namespace_export_fold = false;
                }
                let mut ns_emitter = NamespaceES5Emitter::with_commonjs(self.arena, true);
                // Collect this block's exported vars and accumulate for cross-block sharing
                if !ns_name_for_exports.is_empty() {
                    let block_exports = ns_emitter.collect_exported_var_names(namespace_node);
                    let entry = self
                        .namespace_prior_exports
                        .entry(ns_name_for_exports)
                        .or_default();
                    entry.extend(block_exports);
                    ns_emitter.set_prior_exported_vars(entry.clone());
                }
                ns_emitter.set_indent_level(self.writer.indent_level());
                ns_emitter.set_target_es5(self.ctx.target_es5);
                ns_emitter.set_remove_comments(self.ctx.options.remove_comments);
                ns_emitter.set_legacy_decorators(self.ctx.options.legacy_decorators);
                ns_emitter.set_transforms(self.transforms.clone());
                if let Some(text) = self.source_text_for_map() {
                    ns_emitter.set_source_text(text);
                }
                ns_emitter.set_should_declare_var(should_declare_var);
                let output = if use_cjs {
                    ns_emitter.emit_exported_namespace(namespace_node)
                } else {
                    ns_emitter.emit_namespace(namespace_node)
                };
                self.write(output.trim_end_matches('\n'));
                // Skip comments within the namespace range - the ES5 namespace emitter
                // doesn't use the main comment system, so we must advance past them
                // to prevent them from being dumped at end of file.
                self.skip_comments_for_erased_node(node);
            }

            EmitDirective::ES5Enum { enum_node } => {
                let mut enum_emitter = EnumES5Emitter::new(self.arena);
                enum_emitter.set_indent_level(self.writer.indent_level());
                enum_emitter.set_preserve_const_enums(self.ctx.options.preserve_const_enums);
                if let Some(text) = self.source_text {
                    enum_emitter.set_source_text(text);
                }
                let mut output = enum_emitter.emit_enum(enum_node);
                if let Some(enum_decl) = self.arena.get_enum_at(enum_node) {
                    let enum_name = self.get_identifier_text_idx(enum_decl.name);
                    if !enum_name.is_empty() {
                        if self.declared_namespace_names.contains(&enum_name) {
                            let var_prefix = format!("var {enum_name};\n");
                            if output.starts_with(&var_prefix) {
                                output = output[var_prefix.len()..].to_string();
                            }
                        }
                        self.declared_namespace_names.insert(enum_name);
                    }
                }
                self.write(output.trim_end_matches('\n'));
                // Emit any trailing comment on the enum's closing `}` line
                // (e.g., `enum E { ... } // trailing comment`).
                // The EnumES5Emitter handles comments INSIDE the enum body, so we need to:
                // 1. Advance comment_emit_idx past all inner enum body comments
                // 2. Emit any trailing comment ON the closing `}` line
                // 3. Then skip remaining comments in the enum node range
                let enum_close_pos = self.find_token_end_before_trivia(node.pos, node.end);
                // Step 1: Skip all comments inside the enum body (before enum_close_pos)
                while self.comment_emit_idx < self.all_comments.len()
                    && self.all_comments[self.comment_emit_idx].pos < enum_close_pos
                {
                    self.comment_emit_idx += 1;
                }
                // Step 2: Emit trailing comment on the enum closing `}` line (if any)
                self.emit_trailing_comments(enum_close_pos);
                // Step 3: Skip any remaining comments in the enum node range
                self.skip_comments_for_erased_node(node);
            }

            EmitDirective::CommonJSExport {
                names,
                is_default,
                inner,
            } => {
                // For exported variable declarations with no initializers (e.g.,
                // `export var x: number;`), skip entirely. The preamble
                // `exports.x = void 0;` already handles the forward declaration.
                let skip = node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                    && self.arena.get_variable(node).is_some_and(|var_data| {
                        self.all_declarations_lack_initializer(&var_data.declarations)
                    });

                if !skip {
                    if node.kind == syntax_kind_ext::MODULE_DECLARATION && !is_default {
                        if self.ctx.target_es5 {
                            // ES5: use the IR-based ES5 namespace emitter (only emits var)
                            let mut ns_emitter =
                                NamespaceES5Emitter::with_commonjs(self.arena, true);
                            // Cross-block export sharing
                            if let Some(module_decl) = self.arena.get_module(node) {
                                let ns_name = self.get_identifier_text_idx(module_decl.name);
                                if !ns_name.is_empty() {
                                    let block_exports = ns_emitter.collect_exported_var_names(idx);
                                    let entry =
                                        self.namespace_prior_exports.entry(ns_name).or_default();
                                    entry.extend(block_exports);
                                    ns_emitter.set_prior_exported_vars(entry.clone());
                                }
                            }
                            ns_emitter.set_indent_level(self.writer.indent_level());
                            ns_emitter.set_target_es5(true);
                            ns_emitter.set_remove_comments(self.ctx.options.remove_comments);
                            ns_emitter.set_legacy_decorators(self.ctx.options.legacy_decorators);
                            ns_emitter.set_transforms(self.transforms.clone());
                            if let Some(text) = self.source_text_for_map() {
                                ns_emitter.set_source_text(text);
                            }
                            if let Some(should_declare_var) =
                                Self::namespace_var_flag_from_directive(inner.as_ref())
                            {
                                ns_emitter.set_should_declare_var(should_declare_var);
                            }
                            // Record the name so `export { N }` re-export handler
                            // skips the now-redundant `exports.N = N;`.
                            if let Some(module_decl) = self.arena.get_module(node) {
                                let ns_name = self.get_identifier_text_idx(module_decl.name);
                                if !ns_name.is_empty() {
                                    self.ctx.module_state.iife_exported_names.insert(ns_name);
                                }
                            }
                            let output = ns_emitter.emit_exported_namespace(idx);
                            self.write(output.trim_end_matches('\n'));
                            self.skip_comments_for_erased_node(node);
                            return;
                        }
                        // ES2015+: use the regular IIFE path which preserves let/const.
                        // Set flag so the IIFE tail folds exports.N into the closing.
                        self.pending_cjs_namespace_export_fold = true;
                        // Record the name so `export { N }` re-export handler
                        // skips the now-redundant `exports.N = N;`.
                        if let Some(module_decl) = self.arena.get_module(node) {
                            let ns_name = self.get_identifier_text_idx(module_decl.name);
                            if !ns_name.is_empty() {
                                self.ctx
                                    .module_state
                                    .iife_exported_names
                                    .insert(ns_name.clone());
                            }
                            // Track whether the namespace var was already declared
                            // (merged with class/enum/function).
                            if let Some(should_declare_var) =
                                Self::namespace_var_flag_from_directive(inner.as_ref())
                                && !should_declare_var
                                && !ns_name.is_empty()
                            {
                                // Mark as already declared so emit_namespace_iife skips
                                // the `var N;` / `let N;` preamble.
                                self.declared_namespace_names.insert(ns_name);
                            }
                        }
                        self.emit_node_default(node, idx);
                        return;
                    }

                    // For non-default exported enums in CJS, fold exports.Name into
                    // the IIFE tail: (E || (exports.E = E = {})) instead of a
                    // separate `exports.E = E;` statement after the IIFE.
                    if node.kind == syntax_kind_ext::ENUM_DECLARATION
                        && !is_default
                        && let Some(enum_decl) = self.arena.get_enum(node)
                    {
                        let enum_name = self.get_identifier_text_idx(enum_decl.name);
                        if !enum_name.is_empty() {
                            let mut enum_emitter = EnumES5Emitter::new(self.arena);
                            enum_emitter.set_indent_level(self.writer.indent_level());
                            enum_emitter
                                .set_preserve_const_enums(self.ctx.options.preserve_const_enums);
                            if let Some(text) = self.source_text_for_map() {
                                enum_emitter.set_source_text(text);
                            }
                            let mut output = enum_emitter.emit_enum(idx);
                            // Fold exports binding into IIFE tail
                            let from = format!("({enum_name} || ({enum_name} = {{}}))");
                            let to = format!(
                                "({enum_name} || (exports.{enum_name} = {enum_name} = {{}}))"
                            );
                            output = output.replacen(&from, &to, 1);
                            // Handle namespace merge: strip var prefix if name
                            // was already declared
                            if self.declared_namespace_names.contains(&enum_name) {
                                let var_prefix = format!("var {enum_name};\n");
                                if output.starts_with(&var_prefix) {
                                    output = output[var_prefix.len()..].to_string();
                                }
                            }
                            self.declared_namespace_names.insert(enum_name.clone());
                            // Record the name so `export { E }` re-export handler
                            // skips the now-redundant `exports.E = E;`.
                            self.ctx.module_state.iife_exported_names.insert(enum_name);
                            self.write(output.trim_end_matches('\n'));
                            return;
                        }
                    }

                    // For non-default function declarations, the preamble already
                    // emitted `exports.X = X;` (function declarations are hoisted).
                    // Skip the per-statement export to avoid duplicates.
                    let is_hoisted_func =
                        node.kind == syntax_kind_ext::FUNCTION_DECLARATION && !is_default;
                    if is_hoisted_func {
                        let prev_module = self.ctx.options.module;
                        let prev_original = self.ctx.original_module_kind;
                        self.ctx.options.module = ModuleKind::None;
                        self.ctx.original_module_kind = Some(prev_module);
                        let export_name = names.first().copied();
                        self.emit_commonjs_inner(node, idx, inner.as_ref(), export_name);
                        self.ctx.options.module = prev_module;
                        self.ctx.original_module_kind = prev_original;
                    } else if !is_default
                        && node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                        && let Some(inline_decls) = self.try_collect_inline_cjs_exports(node)
                    {
                        // Inline form: exports.x = initializer;
                        let decl_count = inline_decls.len();
                        for (i, (decoded_name, emit_name, init_idx)) in
                            inline_decls.iter().enumerate()
                        {
                            // Track that this variable was inlined (no local declaration).
                            // Use decoded name for set tracking (matching uses decoded text).
                            self.ctx
                                .module_state
                                .inlined_var_exports
                                .insert(decoded_name.clone());
                            self.write("exports.");
                            // Use emit_name to preserve unicode escapes in output.
                            self.write(emit_name);
                            self.write(" = ");
                            // emit_identifier handles `x → exports.x` substitution
                            // for inline-exported variable names automatically.
                            self.emit(*init_idx);
                            self.write(";");
                            // Skip write_line() on the last declaration so the
                            // source_file.rs statement loop can emit trailing
                            // comments (e.g., `// error`) before the newline.
                            if i < decl_count - 1 {
                                self.write_line();
                            }
                        }
                    } else if !is_default
                        && node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                        && self.variable_stmt_has_binding_pattern(node)
                    {
                        // Destructuring export: emit as comma expression
                        // (e.g., `_a = expr, exports.x = _a.x, exports.rest = __rest(...)`)
                        self.emit_cjs_destructuring_export(node);
                    } else if !is_default && node.kind == syntax_kind_ext::CLASS_DECLARATION {
                        // For non-default class declarations, use the deferred export
                        // mechanism so exports.X = X; is emitted right after the class
                        // body but BEFORE any lowered static block IIFEs or static field
                        // initializers. emit_class_es6_with_options consumes this field
                        // at the class-body boundary.
                        if let Some(name_id) = names.first()
                            && let Some(ident) = self.arena.identifiers.get(*name_id as usize)
                        {
                            self.pending_commonjs_class_export_name =
                                Some(ident.escaped_text.clone());
                        }
                        let prev_module = self.ctx.options.module;
                        let prev_original = self.ctx.original_module_kind;
                        self.ctx.options.module = ModuleKind::None;
                        self.ctx.original_module_kind = Some(prev_module);
                        let export_name = names.first().copied();
                        self.emit_commonjs_inner(node, idx, inner.as_ref(), export_name);
                        self.ctx.options.module = prev_module;
                        self.ctx.original_module_kind = prev_original;
                        // If the deferred export was NOT consumed (e.g. the class had no
                        // static blocks/fields, so emit_class_es6_with_options was not
                        // reached, or the class was ambient), emit it now as a fallback.
                        if let Some(class_name) = self.pending_commonjs_class_export_name.take() {
                            if !self.writer.is_at_line_start() {
                                self.write_line();
                            }
                            self.write("exports.");
                            self.write(&class_name);
                            self.write(" = ");
                            self.write(&class_name);
                            self.write(";");
                            self.write_line();
                        }
                    } else {
                        let export_name = names.first().copied();
                        // Function declarations are hoisted — tsc emits
                        // `exports.default = f;` (default) or `exports.f = f;` (named)
                        // in the preamble before the function body.
                        let is_hoisted = node.kind == syntax_kind_ext::FUNCTION_DECLARATION;
                        self.emit_commonjs_export_with_hoisting(
                            names.as_ref(),
                            is_default,
                            is_hoisted,
                            &mut |this| {
                                this.emit_commonjs_inner(node, idx, inner.as_ref(), export_name);
                            },
                        );
                    }
                }
            }

            EmitDirective::CommonJSExportDefaultExpr => {
                // Check if this is an anonymous class/function that needs a synthetic name
                let is_anonymous = match node.kind {
                    k if k == syntax_kind_ext::CLASS_DECLARATION => {
                        self.arena.get_class(node).is_some_and(|c| c.name.is_none())
                    }
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                        self.arena.get_function(node).is_some_and(|f| {
                            let name = self.get_identifier_text_idx(f.name);
                            name.is_empty()
                                || name == "function"
                                || !is_valid_identifier_name(&name)
                        })
                    }
                    _ => false,
                };
                if is_anonymous {
                    self.emit_commonjs_anonymous_default_as_named(node, idx);
                } else {
                    self.emit_commonjs_default_export_expr(node, idx);
                }
            }

            EmitDirective::CommonJSExportDefaultClassES5 { class_node } => {
                self.emit_commonjs_default_export_class_es5(class_node);
            }

            EmitDirective::ES5ArrowFunction {
                arrow_node,
                captures_this,
                captures_arguments,
                class_alias,
            } => {
                if let Some(arrow_node) = self.arena.get(arrow_node)
                    && let Some(func) = self.arena.get_function(arrow_node)
                {
                    self.emit_arrow_function_es5(
                        arrow_node,
                        func,
                        captures_this,
                        captures_arguments,
                        &class_alias,
                    );
                    return;
                }

                self.emit_node_default(node, idx);
            }

            EmitDirective::ES5AsyncFunction { function_node } => {
                if let Some(func_node) = self.arena.get(function_node)
                    && let Some(func) = self.arena.get_function(func_node)
                {
                    let func_name = if func.name.is_some() {
                        self.get_identifier_text_idx(func.name)
                    } else {
                        String::new()
                    };

                    if func.asterisk_token {
                        self.emit_async_generator_lowered(func, &func_name);
                    } else {
                        self.emit_async_function_es5(func, &func_name, "this");
                    }
                    return;
                }

                self.emit_node_default(node, idx);
            }

            EmitDirective::ES5ForOf { for_of_node } => {
                if let Some(for_of_node_ref) = self.arena.get(for_of_node)
                    && let Some(for_in_of) = self.arena.get_for_in_of(for_of_node_ref)
                {
                    self.emit_for_of_statement_es5(for_of_node, for_in_of);
                    return;
                }

                self.emit_node_default(node, idx);
            }

            EmitDirective::ES5ObjectLiteral { object_literal } => {
                if let Some(literal_node) = self.arena.get(object_literal)
                    && let Some(literal) = self.arena.get_literal_expr(literal_node)
                {
                    let has_trailing_comma =
                        self.has_trailing_comma_in_source(literal_node, &literal.elements.nodes);
                    self.emit_object_literal_es5(
                        &literal.elements.nodes,
                        Some((node.pos, node.end)),
                        has_trailing_comma,
                    );
                    return;
                }

                self.emit_node_default(node, idx);
            }

            EmitDirective::ES5ArrayLiteral { array_literal } => {
                if let Some(literal_node) = self.arena.get(array_literal)
                    && let Some(literal) = self.arena.get_literal_expr(literal_node)
                {
                    self.emit_array_literal_es5(&literal.elements.nodes);
                    return;
                }

                self.emit_node_default(node, idx);
            }

            EmitDirective::ES5VariableDeclarationList { decl_list } => {
                if let Some(list_node) = self.arena.get(decl_list) {
                    self.emit_variable_declaration_list_es5(list_node);
                    return;
                }

                self.emit_node_default(node, idx);
            }

            EmitDirective::ES5FunctionParameters { function_node } => {
                if let Some(func_node) = self.arena.get(function_node) {
                    match func_node.kind {
                        k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                            self.emit_function_declaration_es5_params(func_node);
                            return;
                        }
                        k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
                            self.emit_function_expression_es5_params(func_node);
                            return;
                        }
                        _ => {}
                    }
                }

                self.emit_node_default(node, idx);
            }

            EmitDirective::ES5TemplateLiteral => {
                if !self.emit_template_literal_es5(node, idx) {
                    self.emit_node_default(node, idx);
                }
            }

            EmitDirective::SubstituteThis { ref capture_name } => {
                // Substitute 'this' with capture name (usually '_this', or '_this_1' on collision)
                self.write(capture_name);
            }

            EmitDirective::SubstituteArguments => {
                // TSC does not rename 'arguments' when lowering arrow functions.
                // It lets the lowered function's own 'arguments' binding take effect.
                self.write("arguments");
            }

            EmitDirective::ES5SuperCall => {
                // Transform super(...) to _super.call(this, ...)
                self.emit_super_call_es5(node);
            }

            EmitDirective::ModuleWrapper {
                format,
                dependencies,
            } => {
                if let Some(source) = self.arena.get_source_file(node) {
                    self.emit_module_wrapper(format, dependencies.as_ref(), node, source, idx);
                    return;
                }

                self.emit_node_default(node, idx);
            }

            EmitDirective::ES5CallSpread { call_expr } => {
                if let Some(call_node) = self.arena.get(call_expr) {
                    self.emit_call_expression_es5_spread(call_node);
                } else {
                    self.emit_node_default(node, idx);
                }
            }

            EmitDirective::TC39Decorators {
                class_node,
                function_name,
            } => {
                self.emit_tc39_decorators(node, idx, class_node, function_name.as_deref());
            }

            EmitDirective::Chain(directives) => {
                self.emit_chained_directives(node, idx, directives.as_slice());
            }
        }
    }

    /// Create an ES5 class emitter pre-configured with decorator info for the given class.
    fn create_es5_class_emitter_with_decorators(
        &self,
        class_node: NodeIndex,
    ) -> ClassES5Emitter<'a> {
        let mut es5_emitter = ClassES5Emitter::new(self.arena);
        es5_emitter.set_indent_level(self.writer.indent_level());
        es5_emitter.set_transforms(self.transforms.clone());
        es5_emitter.set_remove_comments(self.ctx.options.remove_comments);
        if self.ctx.options.import_helpers && self.ctx.is_effectively_commonjs() {
            es5_emitter.set_tslib_prefix(true);
        }
        if let Some(text) = self.source_text_for_map() {
            if self.writer.has_source_map() {
                es5_emitter.set_source_map_context(text, self.writer.current_source_index());
            } else {
                es5_emitter.set_source_text(text);
            }
        }

        // Pass legacy decorator info so __decorate calls are emitted inside the IIFE
        if self.ctx.options.legacy_decorators
            && let Some(class_node_ref) = self.arena.get(class_node)
            && let Some(class_data) = self.arena.get_class(class_node_ref)
        {
            let class_decorators = self.collect_class_decorators(&class_data.modifiers);
            let has_member_decorators = class_data.members.nodes.iter().any(|&m_idx| {
                let Some(m_node) = self.arena.get(m_idx) else {
                    return false;
                };
                let mods = match m_node.kind {
                    k if k == syntax_kind_ext::METHOD_DECLARATION => self
                        .arena
                        .get_method_decl(m_node)
                        .and_then(|m| m.modifiers.as_ref()),
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                        .arena
                        .get_property_decl(m_node)
                        .and_then(|p| p.modifiers.as_ref()),
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        self.arena
                            .get_accessor(m_node)
                            .and_then(|a| a.modifiers.as_ref())
                    }
                    _ => None,
                };
                let has_member_dec = mods.is_some_and(|m| {
                    m.nodes.iter().any(|&mod_idx| {
                        self.arena
                            .get(mod_idx)
                            .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                    })
                });
                if has_member_dec {
                    return true;
                }
                // Also check for parameter decorators on methods and constructors
                let params = match m_node.kind {
                    k if k == syntax_kind_ext::METHOD_DECLARATION => {
                        self.arena.get_method_decl(m_node).map(|m| &m.parameters)
                    }
                    k if k == syntax_kind_ext::CONSTRUCTOR => {
                        self.arena.get_constructor(m_node).map(|c| &c.parameters)
                    }
                    _ => None,
                };
                params.is_some_and(|p| {
                    p.nodes.iter().any(|&param_idx| {
                        let Some(param_node) = self.arena.get(param_idx) else {
                            return false;
                        };
                        let Some(param) = self.arena.get_parameter(param_node) else {
                            return false;
                        };
                        param.modifiers.as_ref().is_some_and(|m| {
                            m.nodes.iter().any(|&mod_idx| {
                                self.arena
                                    .get(mod_idx)
                                    .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                            })
                        })
                    })
                })
            });
            if !class_decorators.is_empty() || has_member_decorators {
                es5_emitter.set_decorator_info(ClassDecoratorInfo {
                    class_decorators,
                    has_member_decorators,
                    emit_decorator_metadata: self.ctx.options.emit_decorator_metadata,
                });
            }
        }

        es5_emitter
    }

    fn emit_tc39_decorators(
        &mut self,
        node: &tsz_parser::parser::node::Node,
        _idx: NodeIndex,
        class_node: NodeIndex,
        function_name: Option<&str>,
    ) {
        use crate::transforms::es_decorators::TC39DecoratorEmitter;

        let mut emitter = TC39DecoratorEmitter::new(self.arena);
        emitter.set_indent_level(self.writer.indent_level() as usize);
        // At ES2022+, use `static { }` blocks for decorator application.
        // At ES2015, use IIFE pattern with comma expressions.
        emitter.set_use_static_blocks(!self.ctx.needs_es2022_lowering);
        emitter.set_use_define_for_class_fields(self.ctx.options.use_define_for_class_fields);
        if self.ctx.options.import_helpers && self.ctx.is_effectively_commonjs() {
            emitter.set_tslib_prefix(true);
        }
        // For class expressions, emit as expression (no `let C = ` wrapper)
        if node.kind == syntax_kind_ext::CLASS_EXPRESSION {
            emitter.set_expression_mode(true);
            // Use function name from the directive (determined during lowering)
            if let Some(name) = function_name {
                emitter.set_function_name(name.to_string());
            } else if let Some(ref name) = self.anonymous_default_export_name {
                emitter.set_function_name(name.clone());
            } else if let Some(ref name) = self.pending_commonjs_class_export_name {
                emitter.set_function_name(name.clone());
            }
        }
        if let Some(text) = self.source_text_for_map() {
            emitter.set_source_text(text);
        }
        let output = emitter.emit_class(class_node);
        // Trim trailing newline from the output to avoid double-newlining
        // when the writer adds its own line termination
        let output = output.trim_end_matches('\n');
        self.write(output);
        // Skip comments within the class range - the TC39 decorator emitter
        // handles them separately.
        self.skip_comments_for_erased_node(node);
    }

    fn emit_commonjs_inner(
        &mut self,
        node: &Node,
        idx: NodeIndex,
        inner: &EmitDirective,
        export_name: Option<IdentifierId>,
    ) {
        match inner {
            EmitDirective::ES5Class { class_node } => {
                let mut es5_emitter = self.create_es5_class_emitter_with_decorators(*class_node);
                let es5_output = es5_emitter.emit_class(*class_node);
                let es5_mappings = es5_emitter.take_mappings();
                if !es5_mappings.is_empty() && self.writer.has_source_map() {
                    self.writer.write("");
                    let base_line = self.writer.current_line();
                    let base_column = self.writer.current_column();
                    self.writer
                        .add_offset_mappings(base_line, base_column, &es5_mappings);
                    self.writer.write(&es5_output);
                } else {
                    self.write(&es5_output);
                }
            }
            EmitDirective::ES5ClassExpression { class_node } => {
                self.emit_class_expression_es5(*class_node);
            }
            EmitDirective::ES5Namespace {
                namespace_node,
                should_declare_var,
            } => {
                let mut ns_name_for_exports = String::new();
                if let Some(ns_node) = self.arena.get(*namespace_node)
                    && let Some(ns_data) = self.arena.get_module(ns_node)
                {
                    let ns_name = self.get_identifier_text_idx(ns_data.name);
                    if !ns_name.is_empty() {
                        ns_name_for_exports = ns_name.clone();
                        self.declared_namespace_names.insert(ns_name);
                    }
                }
                let mut ns_emitter =
                    NamespaceES5Emitter::with_commonjs(self.arena, self.ctx.is_commonjs());
                // Collect this block's exported vars and accumulate for cross-block sharing
                if !ns_name_for_exports.is_empty() {
                    let block_exports = ns_emitter.collect_exported_var_names(*namespace_node);
                    let entry = self
                        .namespace_prior_exports
                        .entry(ns_name_for_exports)
                        .or_default();
                    entry.extend(block_exports);
                    ns_emitter.set_prior_exported_vars(entry.clone());
                }
                ns_emitter.set_indent_level(self.writer.indent_level());
                ns_emitter.set_target_es5(self.ctx.target_es5);
                ns_emitter.set_remove_comments(self.ctx.options.remove_comments);
                ns_emitter.set_legacy_decorators(self.ctx.options.legacy_decorators);
                ns_emitter.set_transforms(self.transforms.clone());
                if let Some(text) = self.source_text_for_map() {
                    ns_emitter.set_source_text(text);
                }
                ns_emitter.set_should_declare_var(*should_declare_var);
                let output = ns_emitter.emit_exported_namespace(*namespace_node);
                self.write(output.trim_end_matches('\n'));
                // Advance comment cursor past comments inside the namespace body,
                // since the sub-emitter already handled them.
                while self.comment_emit_idx < self.all_comments.len()
                    && self.all_comments[self.comment_emit_idx].end <= node.end
                {
                    self.comment_emit_idx += 1;
                }
            }
            EmitDirective::ES5Enum { enum_node } => {
                let mut enum_emitter = EnumES5Emitter::new(self.arena);
                enum_emitter.set_indent_level(self.writer.indent_level());
                enum_emitter.set_preserve_const_enums(self.ctx.options.preserve_const_enums);
                if let Some(text) = self.source_text {
                    enum_emitter.set_source_text(text);
                }
                let mut output = enum_emitter.emit_enum(*enum_node);
                if let Some(enum_decl) = self.arena.get_enum_at(*enum_node) {
                    let enum_name = self.get_identifier_text_idx(enum_decl.name);
                    if !enum_name.is_empty() {
                        if self.declared_namespace_names.contains(&enum_name) {
                            let var_prefix = format!("var {enum_name};\n");
                            if output.starts_with(&var_prefix) {
                                output = output[var_prefix.len()..].to_string();
                            }
                        }
                        self.declared_namespace_names.insert(enum_name);
                    }
                }
                self.write(output.trim_end_matches('\n'));
                // Advance comment_emit_idx past all inner enum body comments
                // and emit trailing comment on the closing `}` line.
                let enum_close_pos = self.find_token_end_before_trivia(node.pos, node.end);
                while self.comment_emit_idx < self.all_comments.len()
                    && self.all_comments[self.comment_emit_idx].pos < enum_close_pos
                {
                    self.comment_emit_idx += 1;
                }
                self.emit_trailing_comments(enum_close_pos);
                self.skip_comments_for_erased_node(node);
            }
            EmitDirective::ES5AsyncFunction { function_node } => {
                if let Some(func_node) = self.arena.get(*function_node)
                    && let Some(func) = self.arena.get_function(func_node)
                {
                    if func.asterisk_token {
                        let func_name = if func.name.is_some() {
                            self.get_identifier_text_idx(func.name)
                        } else if let Some(export_name) = export_name {
                            self.arena
                                .identifiers
                                .get(export_name as usize)
                                .map(|ident| ident.escaped_text.clone())
                                .unwrap_or_default()
                        } else {
                            String::new()
                        };
                        self.emit_async_generator_lowered(func, &func_name);
                    } else if func.name.is_some() {
                        let func_name = self.get_identifier_text_idx(func.name);
                        self.emit_async_function_es5(func, &func_name, "this");
                    } else if let Some(export_name) = export_name {
                        if let Some(ident) = self.arena.identifiers.get(export_name as usize) {
                            self.emit_async_function_es5(func, &ident.escaped_text, "this");
                        } else {
                            self.emit_async_function_es5(func, "", "this");
                        }
                    } else {
                        self.emit_async_function_es5(func, "", "this");
                    }
                }
            }
            EmitDirective::ES5ArrowFunction {
                arrow_node,
                captures_this,
                captures_arguments,
                class_alias,
            } => {
                if let Some(arrow_node) = self.arena.get(*arrow_node)
                    && let Some(func) = self.arena.get_function(arrow_node)
                {
                    self.emit_arrow_function_es5(
                        arrow_node,
                        func,
                        *captures_this,
                        *captures_arguments,
                        class_alias,
                    );
                }
            }
            EmitDirective::ES5FunctionParameters { function_node } => {
                if let Some(func_node) = self.arena.get(*function_node) {
                    match func_node.kind {
                        k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                            self.emit_function_declaration_es5_params(func_node);
                        }
                        k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
                            self.emit_function_expression_es5_params(func_node);
                        }
                        _ => {}
                    }
                }
            }
            EmitDirective::TC39Decorators {
                class_node,
                function_name,
            } => {
                self.emit_tc39_decorators(node, idx, *class_node, function_name.as_deref());
            }
            EmitDirective::Chain(directives) => {
                self.emit_chained_directives(node, idx, directives.as_slice());
            }
            _ => {
                self.emit_node_default(node, idx);
            }
        }
    }

    fn emit_chained_directives(
        &mut self,
        node: &Node,
        idx: NodeIndex,
        directives: &[EmitDirective],
    ) {
        if directives.is_empty() {
            self.emit_node_default(node, idx);
            return;
        }

        let last = directives.len() - 1;
        self.emit_chained_directive(node, idx, directives, last);
    }

    fn emit_chained_directive(
        &mut self,
        node: &Node,
        idx: NodeIndex,
        directives: &[EmitDirective],
        index: usize,
    ) {
        let directive = &directives[index];
        match directive {
            EmitDirective::Identity => {
                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::ES5Class { class_node } => {
                let mut es5_emitter = self.create_es5_class_emitter_with_decorators(*class_node);
                let es5_output = es5_emitter.emit_class(*class_node);
                let es5_mappings = es5_emitter.take_mappings();
                if !es5_mappings.is_empty() && self.writer.has_source_map() {
                    self.writer.write("");
                    let base_line = self.writer.current_line();
                    let base_column = self.writer.current_column();
                    self.writer
                        .add_offset_mappings(base_line, base_column, &es5_mappings);
                    self.writer.write(&es5_output);
                } else {
                    self.write(&es5_output);
                }
            }
            EmitDirective::ES5ClassExpression { class_node } => {
                self.emit_class_expression_es5(*class_node);
            }
            EmitDirective::ES5Namespace {
                namespace_node,
                should_declare_var,
            } => {
                let mut ns_name_for_exports = String::new();
                if let Some(ns_node) = self.arena.get(*namespace_node)
                    && let Some(ns_data) = self.arena.get_module(ns_node)
                {
                    let ns_name = self.get_identifier_text_idx(ns_data.name);
                    if !ns_name.is_empty() {
                        ns_name_for_exports = ns_name.clone();
                        self.declared_namespace_names.insert(ns_name);
                    }
                }
                let mut ns_emitter =
                    NamespaceES5Emitter::with_commonjs(self.arena, self.ctx.is_commonjs());
                // Collect this block's exported vars and accumulate for cross-block sharing
                if !ns_name_for_exports.is_empty() {
                    let block_exports = ns_emitter.collect_exported_var_names(*namespace_node);
                    let entry = self
                        .namespace_prior_exports
                        .entry(ns_name_for_exports)
                        .or_default();
                    entry.extend(block_exports);
                    ns_emitter.set_prior_exported_vars(entry.clone());
                }
                ns_emitter.set_indent_level(self.writer.indent_level());
                ns_emitter.set_target_es5(self.ctx.target_es5);
                ns_emitter.set_remove_comments(self.ctx.options.remove_comments);
                ns_emitter.set_legacy_decorators(self.ctx.options.legacy_decorators);
                ns_emitter.set_transforms(self.transforms.clone());
                if let Some(text) = self.source_text_for_map() {
                    ns_emitter.set_source_text(text);
                }
                ns_emitter.set_should_declare_var(*should_declare_var);
                let output = ns_emitter.emit_namespace(*namespace_node);
                self.write(output.trim_end_matches('\n'));
                // Advance comment cursor past comments inside the namespace body,
                // since the sub-emitter already handled them.
                while self.comment_emit_idx < self.all_comments.len()
                    && self.all_comments[self.comment_emit_idx].end <= node.end
                {
                    self.comment_emit_idx += 1;
                }
            }
            EmitDirective::ES5Enum { enum_node } => {
                let mut enum_emitter = EnumES5Emitter::new(self.arena);
                enum_emitter.set_indent_level(self.writer.indent_level());
                enum_emitter.set_preserve_const_enums(self.ctx.options.preserve_const_enums);
                if let Some(text) = self.source_text {
                    enum_emitter.set_source_text(text);
                }
                let mut output = enum_emitter.emit_enum(*enum_node);
                if let Some(enum_decl) = self.arena.get_enum_at(*enum_node) {
                    let enum_name = self.get_identifier_text_idx(enum_decl.name);
                    if !enum_name.is_empty() {
                        if self.declared_namespace_names.contains(&enum_name) {
                            let var_prefix = format!("var {enum_name};\n");
                            if output.starts_with(&var_prefix) {
                                output = output[var_prefix.len()..].to_string();
                            }
                        }
                        self.declared_namespace_names.insert(enum_name);
                    }
                }
                self.write(output.trim_end_matches('\n'));
                // Advance comment_emit_idx past all inner enum body comments
                // and emit trailing comment on the closing `}` line.
                let enum_close_pos = self.find_token_end_before_trivia(node.pos, node.end);
                while self.comment_emit_idx < self.all_comments.len()
                    && self.all_comments[self.comment_emit_idx].pos < enum_close_pos
                {
                    self.comment_emit_idx += 1;
                }
                self.emit_trailing_comments(enum_close_pos);
                self.skip_comments_for_erased_node(node);
            }
            EmitDirective::CommonJSExport {
                names,
                is_default,
                inner,
            } => {
                if node.kind == syntax_kind_ext::MODULE_DECLARATION && !*is_default {
                    let mut ns_emitter = NamespaceES5Emitter::with_commonjs(self.arena, true);
                    // Cross-block export sharing
                    if let Some(module_decl) = self.arena.get_module(node) {
                        let ns_name = self.get_identifier_text_idx(module_decl.name);
                        if !ns_name.is_empty() {
                            let block_exports = ns_emitter.collect_exported_var_names(idx);
                            let entry = self.namespace_prior_exports.entry(ns_name).or_default();
                            entry.extend(block_exports);
                            ns_emitter.set_prior_exported_vars(entry.clone());
                        }
                    }
                    ns_emitter.set_indent_level(self.writer.indent_level());
                    ns_emitter.set_target_es5(self.ctx.target_es5);
                    ns_emitter.set_remove_comments(self.ctx.options.remove_comments);
                    ns_emitter.set_legacy_decorators(self.ctx.options.legacy_decorators);
                    ns_emitter.set_transforms(self.transforms.clone());
                    if let Some(text) = self.source_text_for_map() {
                        ns_emitter.set_source_text(text);
                    }
                    if let Some(should_declare_var) =
                        Self::namespace_var_flag_from_directive(inner.as_ref())
                    {
                        ns_emitter.set_should_declare_var(should_declare_var);
                    }
                    let output = ns_emitter.emit_exported_namespace(idx);
                    self.write(output.trim_end_matches('\n'));
                    self.skip_comments_for_erased_node(node);
                    return;
                }

                if !*is_default
                    && node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                    && self.variable_stmt_has_binding_pattern(node)
                {
                    // Destructuring export: emit as comma expression
                    self.emit_cjs_destructuring_export(node);
                } else if !*is_default && node.kind == syntax_kind_ext::CLASS_DECLARATION {
                    // Use deferred export mechanism for class declarations so
                    // exports.X = X; appears before lowered static blocks/IIFEs.
                    if let Some(name_id) = names.first()
                        && let Some(ident) = self.arena.identifiers.get(*name_id as usize)
                    {
                        self.pending_commonjs_class_export_name = Some(ident.escaped_text.clone());
                    }
                    let prev_module = self.ctx.options.module;
                    let prev_original = self.ctx.original_module_kind;
                    self.ctx.options.module = ModuleKind::None;
                    self.ctx.original_module_kind = Some(prev_module);
                    let export_name = names.first().copied();
                    if index == 0 {
                        self.emit_commonjs_inner(node, idx, inner.as_ref(), export_name);
                    } else {
                        self.emit_chained_directive(node, idx, directives, index - 1);
                    }
                    self.ctx.options.module = prev_module;
                    self.ctx.original_module_kind = prev_original;
                    if let Some(class_name) = self.pending_commonjs_class_export_name.take() {
                        if !self.writer.is_at_line_start() {
                            self.write_line();
                        }
                        self.write("exports.");
                        self.write(&class_name);
                        self.write(" = ");
                        self.write(&class_name);
                        self.write(";");
                        self.write_line();
                    }
                } else {
                    let export_name = names.first().copied();
                    let is_hoisted = node.kind == syntax_kind_ext::FUNCTION_DECLARATION;
                    self.emit_commonjs_export_with_hoisting(
                        names.as_ref(),
                        *is_default,
                        is_hoisted,
                        &mut |this| {
                            if index == 0 {
                                this.emit_commonjs_inner(node, idx, inner.as_ref(), export_name);
                            } else {
                                this.emit_chained_directive(node, idx, directives, index - 1);
                            }
                        },
                    );
                }
            }
            EmitDirective::CommonJSExportDefaultExpr => {
                // Check if this is an anonymous class/function
                let is_anonymous = match node.kind {
                    k if k == syntax_kind_ext::CLASS_DECLARATION => {
                        self.arena.get_class(node).is_some_and(|c| c.name.is_none())
                    }
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                        self.arena.get_function(node).is_some_and(|f| {
                            let name = self.get_identifier_text_idx(f.name);
                            name.is_empty()
                                || name == "function"
                                || !is_valid_identifier_name(&name)
                        })
                    }
                    _ => false,
                };
                if is_anonymous {
                    self.emit_commonjs_anonymous_default_as_named(node, idx);
                } else {
                    self.emit_commonjs_default_export_assignment(|this| {
                        if index == 0 {
                            this.emit_commonjs_default_export_expr_inner(node, idx);
                        } else {
                            this.emit_chained_directive(node, idx, directives, index - 1);
                        }
                    });
                }
            }
            EmitDirective::CommonJSExportDefaultClassES5 { class_node } => {
                self.emit_commonjs_default_export_class_es5(*class_node);
            }
            EmitDirective::ES5ArrowFunction {
                arrow_node,
                captures_this,
                captures_arguments,
                class_alias,
            } => {
                if let Some(arrow_node) = self.arena.get(*arrow_node)
                    && let Some(func) = self.arena.get_function(arrow_node)
                {
                    self.emit_arrow_function_es5(
                        arrow_node,
                        func,
                        *captures_this,
                        *captures_arguments,
                        class_alias,
                    );
                    return;
                }

                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::ES5AsyncFunction { function_node } => {
                if let Some(func_node) = self.arena.get(*function_node)
                    && let Some(func) = self.arena.get_function(func_node)
                {
                    let func_name = if func.name.is_some() {
                        self.get_identifier_text_idx(func.name)
                    } else {
                        String::new()
                    };

                    if func.asterisk_token {
                        self.emit_async_generator_lowered(func, &func_name);
                    } else {
                        self.emit_async_function_es5(func, &func_name, "this");
                    }
                    return;
                }

                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::ES5ForOf { for_of_node } => {
                if let Some(for_of_node_ref) = self.arena.get(*for_of_node)
                    && let Some(for_in_of) = self.arena.get_for_in_of(for_of_node_ref)
                {
                    self.emit_for_of_statement_es5(*for_of_node, for_in_of);
                    return;
                }

                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::ES5ObjectLiteral { object_literal } => {
                if let Some(literal_node) = self.arena.get(*object_literal)
                    && let Some(literal) = self.arena.get_literal_expr(literal_node)
                {
                    let has_trailing_comma =
                        self.has_trailing_comma_in_source(literal_node, &literal.elements.nodes);
                    self.emit_object_literal_es5(
                        &literal.elements.nodes,
                        Some((node.pos, node.end)),
                        has_trailing_comma,
                    );
                    return;
                }

                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::ES5ArrayLiteral { array_literal } => {
                if let Some(literal_node) = self.arena.get(*array_literal)
                    && let Some(literal) = self.arena.get_literal_expr(literal_node)
                {
                    self.emit_array_literal_es5(&literal.elements.nodes);
                    return;
                }

                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::ES5CallSpread { call_expr } => {
                if let Some(call_node) = self.arena.get(*call_expr) {
                    self.emit_call_expression_es5_spread(call_node);
                    return;
                }

                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::ES5VariableDeclarationList { decl_list } => {
                if let Some(list_node) = self.arena.get(*decl_list) {
                    self.emit_variable_declaration_list_es5(list_node);
                    return;
                }

                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::ES5FunctionParameters { function_node } => {
                if let Some(func_node) = self.arena.get(*function_node) {
                    match func_node.kind {
                        k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                            self.emit_function_declaration_es5_params(func_node);
                            return;
                        }
                        k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
                            self.emit_function_expression_es5_params(func_node);
                            return;
                        }
                        _ => {}
                    }
                }

                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::ES5TemplateLiteral => {
                if self.emit_template_literal_es5(node, idx) {
                    return;
                }

                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::SubstituteThis { capture_name } => {
                // Substitute 'this' with capture name (usually '_this', or '_this_1' on collision)
                self.write(capture_name);
            }
            EmitDirective::SubstituteArguments => {
                // TSC does not rename 'arguments' when lowering arrow functions.
                // It lets the lowered function's own 'arguments' binding take effect.
                self.write("arguments");
            }
            EmitDirective::ES5SuperCall => {
                // Transform super(...) to _super.call(this, ...)
                self.emit_super_call_es5(node);
            }
            EmitDirective::TC39Decorators {
                class_node,
                function_name,
            } => {
                self.emit_tc39_decorators(node, idx, *class_node, function_name.as_deref());
            }
            EmitDirective::ModuleWrapper {
                format,
                dependencies,
            } => {
                if let Some(source) = self.arena.get_source_file(node) {
                    self.emit_module_wrapper(*format, dependencies.as_ref(), node, source, idx);
                    return;
                }

                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::Chain(nested) => {
                self.emit_chained_directives(node, idx, nested.as_slice());
            }
        }
    }

    fn emit_chained_previous(
        &mut self,
        node: &Node,
        idx: NodeIndex,
        directives: &[EmitDirective],
        index: usize,
    ) {
        if index == 0 {
            self.emit_node_default(node, idx);
        } else {
            self.emit_chained_directive(node, idx, directives, index - 1);
        }
    }

    /// Emit ES5 super call: transform super(...) to _super.call(this, ...)
    fn emit_super_call_es5(&mut self, node: &Node) {
        let Some(call) = self.arena.get_call_expr(node) else {
            return;
        };

        // Emit _super.call(this
        self.write("_super.call(this");

        // Emit arguments if any
        if let Some(ref args) = call.arguments
            && !args.nodes.is_empty()
        {
            self.write(", ");
            // Emit arguments separated by commas
            for (i, &arg_idx) in args.nodes.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.emit(arg_idx);
            }
        }

        // Close the call
        self.write(")");
    }

    /// Emit a node using default logic (no transforms).
    /// This is the old `emit_node` logic extracted for reuse.
    pub(super) fn emit_node_default(&mut self, node: &Node, idx: NodeIndex) {
        // Emit the node without consulting transform directives.
        let kind = node.kind;
        self.emit_node_by_kind(node, idx, kind);
    }
}
