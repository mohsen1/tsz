//! Chained transform directive emission split out of `transform_dispatch.rs`.

use super::*;

impl<'a> Printer<'a> {
    pub(super) fn emit_chained_directives(
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
                let class_binding_name = self.register_es5_class_binding_name(*class_node);
                let mut es5_emitter = self.create_es5_class_emitter_with_decorators(*class_node);
                let es5_output = self.emit_es5_class_output(
                    &mut es5_emitter,
                    *class_node,
                    class_binding_name.as_deref(),
                );
                self.sync_es5_class_emitter_state(&mut es5_emitter);
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
                let class_close_pos = self.find_token_end_before_trivia(node.pos, node.end);
                while self.comment_emit_idx < self.all_comments.len()
                    && self.all_comments[self.comment_emit_idx].pos < class_close_pos
                {
                    self.comment_emit_idx += 1;
                }
                self.emit_trailing_comments(class_close_pos);
                self.skip_comments_for_erased_node(node);
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
                    if self.in_top_level_using_scope && self.ctx.target_es5 {
                        self.emit_namespace_iife(ns_data, None, None);
                        while self.comment_emit_idx < self.all_comments.len()
                            && self.all_comments[self.comment_emit_idx].end <= node.end
                        {
                            self.comment_emit_idx += 1;
                        }
                        return;
                    }
                }
                let mut ns_emitter =
                    NamespaceES5Emitter::with_commonjs(self.arena, self.ctx.is_commonjs());
                ns_emitter.set_module_kind(self.ctx.outer_module_kind());
                ns_emitter.set_const_enum_facts(
                    self.const_enum_values.clone(),
                    self.const_enum_import_aliases.clone(),
                );
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
                ns_emitter.set_emit_decorator_metadata(self.ctx.options.emit_decorator_metadata);
                ns_emitter.set_transforms(self.transforms.clone());
                if let Some(text) = self.source_text_for_map() {
                    ns_emitter.set_source_text(text);
                }
                ns_emitter
                    .set_should_declare_var(*should_declare_var && !self.in_top_level_using_scope);
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
                self.emit_es5_enum_directive(node, *enum_node);
            }
            EmitDirective::CommonJSExport {
                names,
                is_default,
                inner,
            } => {
                if node.kind == syntax_kind_ext::MODULE_DECLARATION && !*is_default {
                    let cjs_export_names = self.commonjs_export_name_strings(names.as_ref());
                    let mut ns_emitter = NamespaceES5Emitter::with_commonjs(self.arena, true);
                    ns_emitter.set_module_kind(self.ctx.outer_module_kind());
                    ns_emitter.set_const_enum_facts(
                        self.const_enum_values.clone(),
                        self.const_enum_import_aliases.clone(),
                    );
                    let mut should_declare_namespace_var = None;
                    // Cross-block export sharing
                    if let Some(module_decl) = self.arena.get_module(node) {
                        if let Some(ns_name) = self.get_module_root_name(module_decl.name) {
                            if self.declared_namespace_names.contains(&ns_name) {
                                should_declare_namespace_var = Some(false);
                            }
                            self.declared_namespace_names.insert(ns_name.clone());
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
                    ns_emitter
                        .set_emit_decorator_metadata(self.ctx.options.emit_decorator_metadata);
                    ns_emitter.set_commonjs_export_names(cjs_export_names.clone());
                    ns_emitter.set_transforms(self.transforms.clone());
                    self.configure_es5_namespace_emitter_block_scope(&mut ns_emitter);
                    if let Some(text) = self.source_text_for_map() {
                        ns_emitter.set_source_text(text);
                    }
                    if let Some(should_declare_var) = should_declare_namespace_var.or_else(|| {
                        Self::namespace_var_flag_from_directive(inner.as_ref()).or_else(|| {
                            directives[..index]
                                .iter()
                                .rev()
                                .find_map(Self::namespace_var_flag_from_directive)
                        })
                    }) {
                        ns_emitter.set_should_declare_var(should_declare_var);
                    }
                    let output = ns_emitter.emit_exported_namespace(idx);
                    self.sync_es5_namespace_emitter_block_scope(&ns_emitter);
                    if let Some(module_decl) = self.arena.get_module(node) {
                        let ns_name = self.get_identifier_text_idx(module_decl.name);
                        if !ns_name.is_empty() {
                            self.ctx
                                .module_state
                                .iife_exported_names
                                .insert(ns_name.clone());
                            let bindings = self
                                .ctx
                                .module_state
                                .iife_exported_bindings
                                .entry(ns_name.clone())
                                .or_default();
                            if cjs_export_names.is_empty() {
                                bindings.insert(ns_name);
                            } else {
                                for export_name in &cjs_export_names {
                                    bindings.insert(export_name.clone());
                                }
                            }
                        }
                    }
                    self.write(output.trim_end_matches('\n'));
                    self.skip_comments_for_erased_node(node);
                    return;
                }

                // The Chain dispatch arrives here when lowering produced
                // `Chain[ES5Enum, CommonJSExport]` for an `export enum E`
                // (target=es5). The chained `ES5Enum` would emit a separate
                // `exports.E = E;` line *after* the IIFE and miss
                // multi-alias folds; route through the same fold helper the
                // non-Chain path uses instead.
                if !*is_default
                    && node.kind == syntax_kind_ext::ENUM_DECLARATION
                    && let Some(enum_decl) = self.arena.get_enum(node)
                    && self.emit_cjs_enum_with_alias_fold(idx, names.as_ref(), enum_decl, true)
                {
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
                        self.pending_commonjs_class_export_name =
                            Some((idx, ident.escaped_text.clone()));
                    }
                    let export_name = names.first().copied();
                    self.with_cjs_export_body_mask(|this| {
                        if index == 0 {
                            this.emit_commonjs_inner(node, idx, inner.as_ref(), export_name);
                        } else {
                            this.emit_chained_directive(node, idx, directives, index - 1);
                        }
                    });
                    if let Some((_, class_name)) = self.pending_commonjs_class_export_name.take() {
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

                    if self
                        .should_emit_invalid_namespace_static_modifier(func_node, &func.modifiers)
                    {
                        self.write("static ");
                    }
                    if func.asterisk_token {
                        self.emit_async_generator_lowered(func, &func_name);
                    } else {
                        self.emit_async_function_es5(func, &func_name, "this");
                    }
                    return;
                }

                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::ES5GeneratorFunction { function_node } => {
                self.emit_generator_function_es5(*function_node);
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
            EmitDirective::ES5NewSpread { new_expr } => {
                if let Some(new_node) = self.arena.get(*new_expr) {
                    self.emit_new_expression_es5_spread(new_node);
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
                            self.emit_function_expression_es5_params(func_node, *function_node);
                            return;
                        }
                        k if k == syntax_kind_ext::ARROW_FUNCTION && !self.ctx.target_es5 => {
                            if let Some(func) = self.arena.get_function(func_node) {
                                self.emit_arrow_function_native_with_parameter_prologue(func);
                                return;
                            }
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

    /// Emit a non-default `export enum E { ... }` (CJS) using the
    /// alias-folded IIFE tail. Returns `true` when the helper handled the
    /// emit (callers should `return` immediately).
    ///
    /// `names` is the source-ordered alias list — direct export + any later
    /// `export { local as alias }`. When the list is empty (defensive
    /// fallback), the enum's local name is used. The helper deliberately
    /// owns the bookkeeping (`iife_exported_names` /
    /// `iife_exported_bindings`) so call sites stay a single-line dispatch.
    pub(super) fn emit_cjs_enum_with_alias_fold(
        &mut self,
        idx: NodeIndex,
        names: &[IdentifierId],
        enum_decl: &tsz_parser::parser::node::EnumData,
        skip_comments: bool,
    ) -> bool {
        let node = match self.arena.get(idx) {
            Some(n) => n,
            None => return false,
        };
        if node.kind != syntax_kind_ext::ENUM_DECLARATION {
            return false;
        }

        let enum_name = self.get_identifier_text_idx(enum_decl.name);
        if enum_name.is_empty() {
            return false;
        }

        let mut alias_strings: Vec<String> = names
            .iter()
            .filter_map(|name_id| {
                self.arena
                    .identifiers
                    .get(*name_id as usize)
                    .map(|ident| ident.escaped_text.clone())
            })
            .filter(|name| !name.is_empty())
            .collect();
        if alias_strings.is_empty() {
            alias_strings.push(enum_name.clone());
        }

        let mut enum_emitter = EnumES5Emitter::new(self.arena);
        enum_emitter.set_indent_level(self.writer.indent_level());
        enum_emitter.set_preserve_const_enums(self.ctx.options.preserve_const_enums);
        if let Some(text) = self.source_text_for_map() {
            enum_emitter.set_source_text(text);
        }
        enum_emitter.set_commonjs_export_folds(alias_strings.iter().map(String::as_str));
        enum_emitter.set_emit_var_declaration(!self.declared_namespace_names.contains(&enum_name));
        let output = enum_emitter.emit_enum(idx);
        self.declared_namespace_names.insert(enum_name.clone());
        self.ctx
            .module_state
            .iife_exported_names
            .insert(enum_name.clone());
        let bindings_entry = self
            .ctx
            .module_state
            .iife_exported_bindings
            .entry(enum_name)
            .or_default();
        for alias in alias_strings {
            bindings_entry.insert(alias);
        }
        self.write(output.trim_end_matches('\n'));
        if skip_comments {
            self.skip_comments_for_erased_node(node);
        }
        true
    }

    pub(super) fn commonjs_export_name_strings(&self, names: &[IdentifierId]) -> Vec<String> {
        names
            .iter()
            .filter_map(|name_id| {
                self.arena
                    .identifiers
                    .get(*name_id as usize)
                    .map(|ident| ident.escaped_text.clone())
            })
            .filter(|name| !name.is_empty())
            .collect()
    }

    pub(super) fn emit_es5_enum_directive(&mut self, node: &Node, enum_node: NodeIndex) {
        let mut enum_emitter = EnumES5Emitter::new(self.arena);
        enum_emitter.set_indent_level(self.writer.indent_level());
        enum_emitter.set_preserve_const_enums(self.ctx.options.preserve_const_enums);
        if let Some(text) = self.source_text {
            enum_emitter.set_source_text(text);
        }
        let mut enum_name_to_declare = None;
        if let Some(enum_decl) = self.arena.get_enum_at(enum_node) {
            let enum_name = self.get_identifier_text_idx(enum_decl.name);
            if !enum_name.is_empty() {
                enum_emitter
                    .set_emit_var_declaration(!self.declared_namespace_names.contains(&enum_name));

                if let Some(export_name) = self
                    .deferred_local_export_bindings
                    .as_ref()
                    .and_then(|bindings| bindings.get(&enum_name))
                {
                    enum_emitter.set_commonjs_export_fold(export_name);
                    self.ctx
                        .module_state
                        .iife_exported_names
                        .insert(enum_name.clone());
                    self.ctx
                        .module_state
                        .inline_exported_names
                        .insert(export_name.clone());
                }

                enum_name_to_declare = Some(enum_name);
            }
        }
        let output = enum_emitter.emit_enum(enum_node);
        if let Some(enum_name) = enum_name_to_declare {
            self.declared_namespace_names.insert(enum_name);
        }
        self.write(output.trim_end_matches('\n'));

        let enum_close_pos = self.find_token_end_before_trivia(node.pos, node.end);
        while self.comment_emit_idx < self.all_comments.len()
            && self.all_comments[self.comment_emit_idx].pos < enum_close_pos
        {
            self.comment_emit_idx += 1;
        }
        self.emit_trailing_comments(enum_close_pos);
        self.skip_comments_for_erased_node(node);
    }

    /// Emit ES5 super call: transform super(...) to _super.call(this, ...)
    pub(super) fn emit_super_call_es5(&mut self, node: &Node) {
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
    pub(in crate::emitter) fn emit_node_default(&mut self, node: &Node, idx: NodeIndex) {
        // Emit the node without consulting transform directives.
        let kind = node.kind;
        self.emit_node_by_kind(node, idx, kind);
    }
}
