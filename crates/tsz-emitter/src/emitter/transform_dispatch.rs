//! Transform dispatch logic for the emitter.
//!
//! Contains the `EmitDirective` enum and all methods related to applying
//! transform directives during emission (Phase 2 architecture).

use super::*;

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
    ModuleWrapper {
        format: crate::transform_context::ModuleFormat,
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
                // Delegate to existing ClassES5Emitter
                let mut es5_emitter = ClassES5Emitter::new(self.arena);
                es5_emitter.set_indent_level(self.writer.indent_level());
                es5_emitter.set_transforms(self.transforms.clone());
                if let Some(text) = self.source_text_for_map() {
                    if self.writer.has_source_map() {
                        es5_emitter
                            .set_source_map_context(text, self.writer.current_source_index());
                    } else {
                        es5_emitter.set_source_text(text);
                    }
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
                if let Some(ns_node) = self.arena.get(namespace_node)
                    && let Some(ns_data) = self.arena.get_module(ns_node)
                {
                    let ns_name = self.get_identifier_text_idx(ns_data.name);
                    if !ns_name.is_empty() {
                        self.declared_namespace_names.insert(ns_name);
                    }
                }
                let mut ns_emitter = NamespaceES5Emitter::with_commonjs(self.arena, true);
                ns_emitter.set_indent_level(self.writer.indent_level());
                if let Some(text) = self.source_text_for_map() {
                    ns_emitter.set_source_text(text);
                }
                ns_emitter.set_should_declare_var(should_declare_var);
                let output = ns_emitter.emit_namespace(namespace_node);
                self.write(output.trim_end_matches('\n'));
                // Skip comments within the namespace range - the ES5 namespace emitter
                // doesn't use the main comment system, so we must advance past them
                // to prevent them from being dumped at end of file.
                self.skip_comments_for_erased_node(node);
            }

            EmitDirective::ES5Enum { enum_node } => {
                let mut enum_emitter = EnumES5Emitter::new(self.arena);
                enum_emitter.set_indent_level(self.writer.indent_level());
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
                        let mut ns_emitter = NamespaceES5Emitter::with_commonjs(self.arena, true);
                        ns_emitter.set_indent_level(self.writer.indent_level());
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

                    // For non-default function declarations, the preamble already
                    // emitted `exports.X = X;` (function declarations are hoisted).
                    // Skip the per-statement export to avoid duplicates.
                    let is_hoisted_func =
                        node.kind == syntax_kind_ext::FUNCTION_DECLARATION && !is_default;
                    if is_hoisted_func {
                        let prev_module = self.ctx.options.module;
                        self.ctx.options.module = ModuleKind::None;
                        let export_name = names.first().copied();
                        self.emit_commonjs_inner(node, idx, inner.as_ref(), export_name);
                        self.ctx.options.module = prev_module;
                    } else if !is_default
                        && node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                        && let Some(inline_decls) = self.try_collect_inline_cjs_exports(node)
                    {
                        // Inline form: exports.x = initializer;
                        for (name, init_idx) in &inline_decls {
                            self.write("exports.");
                            self.write(name);
                            self.write(" = ");
                            if let Some(init_node) = self.arena.get(*init_idx)
                                && init_node.kind == SyntaxKind::Identifier as u16
                            {
                                let ident = self.get_identifier_text_idx(*init_idx);
                                if self
                                    .ctx
                                    .module_state
                                    .pending_exports
                                    .iter()
                                    .any(|n| n == &ident)
                                {
                                    self.write("exports.");
                                    self.write(&ident);
                                } else {
                                    self.emit(*init_idx);
                                }
                            } else {
                                self.emit(*init_idx);
                            }
                            self.write(";");
                            self.write_line();
                        }
                    } else {
                        let export_name = names.first().copied();
                        self.emit_commonjs_export(names.as_ref(), is_default, |this| {
                            this.emit_commonjs_inner(node, idx, inner.as_ref(), export_name);
                        });
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

                    self.emit_async_function_es5(func, &func_name, "this");
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
                    self.emit_object_literal_es5(
                        &literal.elements.nodes,
                        Some((node.pos, node.end)),
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

            EmitDirective::Chain(directives) => {
                self.emit_chained_directives(node, idx, directives.as_slice());
            }
        }
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
                let mut es5_emitter = ClassES5Emitter::new(self.arena);
                es5_emitter.set_indent_level(self.writer.indent_level());
                es5_emitter.set_transforms(self.transforms.clone());
                if let Some(text) = self.source_text_for_map() {
                    if self.writer.has_source_map() {
                        es5_emitter
                            .set_source_map_context(text, self.writer.current_source_index());
                    } else {
                        es5_emitter.set_source_text(text);
                    }
                }
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
                if let Some(ns_node) = self.arena.get(*namespace_node)
                    && let Some(ns_data) = self.arena.get_module(ns_node)
                {
                    let ns_name = self.get_identifier_text_idx(ns_data.name);
                    if !ns_name.is_empty() {
                        self.declared_namespace_names.insert(ns_name);
                    }
                }
                let mut ns_emitter =
                    NamespaceES5Emitter::with_commonjs(self.arena, self.ctx.is_commonjs());
                ns_emitter.set_indent_level(self.writer.indent_level());
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
            }
            EmitDirective::ES5AsyncFunction { function_node } => {
                if let Some(func_node) = self.arena.get(*function_node)
                    && let Some(func) = self.arena.get_function(func_node)
                {
                    if func.name.is_some() {
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
                let mut es5_emitter = ClassES5Emitter::new(self.arena);
                es5_emitter.set_indent_level(self.writer.indent_level());
                es5_emitter.set_transforms(self.transforms.clone());
                if let Some(text) = self.source_text_for_map() {
                    if self.writer.has_source_map() {
                        es5_emitter
                            .set_source_map_context(text, self.writer.current_source_index());
                    } else {
                        es5_emitter.set_source_text(text);
                    }
                }
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
                if let Some(ns_node) = self.arena.get(*namespace_node)
                    && let Some(ns_data) = self.arena.get_module(ns_node)
                {
                    let ns_name = self.get_identifier_text_idx(ns_data.name);
                    if !ns_name.is_empty() {
                        self.declared_namespace_names.insert(ns_name);
                    }
                }
                let mut ns_emitter =
                    NamespaceES5Emitter::with_commonjs(self.arena, self.ctx.is_commonjs());
                ns_emitter.set_indent_level(self.writer.indent_level());
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
            }
            EmitDirective::CommonJSExport {
                names,
                is_default,
                inner,
            } => {
                if node.kind == syntax_kind_ext::MODULE_DECLARATION && !*is_default {
                    let mut ns_emitter = NamespaceES5Emitter::with_commonjs(self.arena, true);
                    ns_emitter.set_indent_level(self.writer.indent_level());
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

                let export_name = names.first().copied();
                self.emit_commonjs_export(names.as_ref(), *is_default, |this| {
                    if index == 0 {
                        this.emit_commonjs_inner(node, idx, inner.as_ref(), export_name);
                    } else {
                        this.emit_chained_directive(node, idx, directives, index - 1);
                    }
                });
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

                    self.emit_async_function_es5(func, &func_name, "this");
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
                    self.emit_object_literal_es5(
                        &literal.elements.nodes,
                        Some((node.pos, node.end)),
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
