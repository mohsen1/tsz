//! ES5 class, async/generator, and `ASTRef` emit helpers for [`IRPrinter`].
//!
//! Extracted from `ir_printer.rs` to keep file sizes manageable.

use super::IRPrinter;
use crate::emitter::Printer as AstPrinter;
use crate::transforms::ir::{IRMethodName, IRNode};
use tsz_parser::syntax_kind_ext;

impl<'a> IRPrinter<'a> {
    pub(super) fn emit_es5_class_iife_node(&mut self, node: &IRNode) {
        let IRNode::ES5ClassIIFE {
            name,
            binding_name,
            base_class,
            super_param,
            body,
            weakmap_decls,
            computed_prop_temp_decls,
            computed_prop_temp_inits,
            weakmap_inits,
            leading_comment,
            deferred_static_blocks,
            deferred_block_class_alias,
        } = node
        else {
            return;
        };
        // Emit WeakMap declarations if any
        if !weakmap_decls.is_empty() {
            self.write("var ");
            self.write(&weakmap_decls.join(", "));
            self.write(";");
            self.write_line();
        }
        if !computed_prop_temp_decls.is_empty() {
            self.write("var ");
            self.write(&computed_prop_temp_decls.join(", "));
            self.write(";");
            self.write_line();
        }
        // Issue #3967: declare the class self-reference alias used
        // by deferred static-block IIFEs, BEFORE the class IIFE so
        // it is hoisted into scope for the assignment below.
        if let Some(alias) = deferred_block_class_alias {
            self.write("var ");
            self.write(alias);
            self.write(";");
            self.write_line();
        }
        if !self.remove_comments
            && let Some(comment) = leading_comment
        {
            self.write(comment);
            self.write_line();
        }

        // var ClassName = /** @class */ (function (_super) { ... }(BaseClass));
        let class_binding_name = binding_name.as_ref().unwrap_or(name);
        self.write("var ");
        self.write(class_binding_name);
        self.write(" = ");
        self.emit_es5_class_expression(name, base_class.as_deref(), super_param.as_deref(), body);
        self.write(";");

        for init in computed_prop_temp_inits {
            self.write_line();
            self.write_indent();
            self.emit_node(init);
        }

        // Emit WeakMap instantiations if any
        if !weakmap_inits.is_empty() {
            self.write_line();
            self.write(&weakmap_inits.join(", "));
            self.write(";");
        }

        // Issue #3967: assign the alias to the class instance
        // AFTER the IIFE so deferred static-block IIFEs can read it.
        if let Some(alias) = deferred_block_class_alias {
            self.write_line();
            self.write_indent();
            self.write(alias);
            self.write(" = ");
            self.write(class_binding_name);
            self.write(";");
        }
        // Emit deferred static block IIFEs after the class IIFE
        for deferred in deferred_static_blocks {
            self.write_line();
            self.write_indent();
            self.emit_node(deferred);
        }
    }

    pub(super) fn emit_es5_class_assignment_node(&mut self, node: &IRNode) {
        let IRNode::ES5ClassAssignment {
            name,
            base_class,
            super_param,
            body,
            computed_prop_temp_inits,
            weakmap_inits,
            leading_comment,
            deferred_static_blocks,
            deferred_static_result_temp,
            deferred_block_class_alias,
        } = node
        else {
            return;
        };
        if !self.remove_comments
            && let Some(comment) = leading_comment
        {
            self.write(comment);
            self.write_line();
            self.write_indent();
        }

        if let Some(result_temp) = deferred_static_result_temp
            && !deferred_static_blocks.is_empty()
            && computed_prop_temp_inits.is_empty()
            && weakmap_inits.is_empty()
            && deferred_block_class_alias.is_none()
        {
            self.write(name);
            self.write(" = (");
            self.write(result_temp);
            self.write(" = ");
            self.increase_indent();
            self.emit_es5_class_expression(
                name,
                base_class.as_deref(),
                super_param.as_deref(),
                body,
            );
            for deferred in deferred_static_blocks {
                self.write(",");
                self.write_line();
                self.write_indent();
                if let IRNode::StaticBlockIIFE { statements } = deferred {
                    self.emit_static_block_iife_expression(statements);
                } else {
                    self.emit_node(deferred);
                }
            }
            self.write(",");
            self.write_line();
            self.write_indent();
            self.write(result_temp);
            self.write(");");
            self.decrease_indent();
            return;
        }

        self.write(name);
        self.write(" = ");
        self.emit_es5_class_expression(name, base_class.as_deref(), super_param.as_deref(), body);
        self.write(";");

        for init in computed_prop_temp_inits {
            self.write_line();
            self.write_indent();
            self.emit_node(init);
        }

        if !weakmap_inits.is_empty() {
            self.write_line();
            self.write(&weakmap_inits.join(", "));
            self.write(";");
        }

        if let Some(alias) = deferred_block_class_alias {
            self.write_line();
            self.write_indent();
            self.write(alias);
            self.write(" = ");
            self.write(name);
            self.write(";");
        }

        for deferred in deferred_static_blocks {
            self.write_line();
            self.write_indent();
            self.emit_node(deferred);
        }
    }

    pub(super) fn emit_prototype_method_node(&mut self, node: &IRNode) {
        let IRNode::PrototypeMethod {
            class_name,
            method_name,
            function,
            leading_comment,
            trailing_comment,
        } = node
        else {
            return;
        };
        // Emit leading JSDoc comment if present
        if !self.remove_comments
            && let Some(comment) = leading_comment
        {
            self.emit_multiline_comment(comment);
            self.write_line();
            self.write_indent();
        }
        self.write(class_name);
        self.write(".prototype");
        self.emit_method_name(method_name);
        self.write(" = ");
        self.emit_node(function);
        self.write(";");
        if !self.remove_comments
            && let Some(comment) = trailing_comment
                .clone()
                .or_else(|| self.extract_trailing_comment_from_function(function))
        {
            self.write(" ");
            self.write(&comment);
        }
    }

    pub(super) fn emit_static_method_node(&mut self, node: &IRNode) {
        let IRNode::StaticMethod {
            class_name,
            method_name,
            function,
            leading_comment,
            trailing_comment,
        } = node
        else {
            return;
        };
        // Emit leading JSDoc comment if present
        if !self.remove_comments
            && let Some(comment) = leading_comment
        {
            self.emit_multiline_comment(comment);
            self.write_line();
            self.write_indent();
        }
        self.write(class_name);
        self.emit_method_name(method_name);
        self.write(" = ");
        self.emit_node(function);
        self.write(";");
        if !self.remove_comments
            && let Some(comment) = trailing_comment
                .clone()
                .or_else(|| self.extract_trailing_comment_from_function(function))
        {
            self.write(" ");
            self.write(&comment);
        }
    }

    pub(super) fn emit_define_property_node(&mut self, node: &IRNode) {
        let IRNode::DefineProperty {
            target,
            property_name,
            descriptor,
            leading_comment,
        } = node
        else {
            return;
        };
        self.write("Object.defineProperty(");
        self.emit_node(target);
        self.write(", ");
        match property_name {
            IRMethodName::Identifier(name) | IRMethodName::StringLiteral(name) => {
                self.write("\"");
                self.write(name);
                self.write("\"");
            }
            IRMethodName::NumericLiteral(name) => {
                self.write(name);
            }
            IRMethodName::Computed(expr) => {
                self.emit_node(expr);
            }
        }
        self.write(", {");
        self.write_line();
        self.increase_indent();

        // Emit leading comment inside the descriptor (before get/set)
        if !self.remove_comments
            && let Some(comment) = leading_comment
        {
            self.write_indent();
            self.emit_multiline_comment(comment);
            self.write_line();
        }

        if let Some(get) = &descriptor.get {
            if !self.remove_comments
                && let Some(comment) = &descriptor.get_leading_comment
            {
                self.write_indent();
                self.emit_multiline_comment(comment);
                self.write_line();
            }
            self.write_indent();
            self.write("get: ");
            self.emit_node(get);
            let trailing_comment = if self.remove_comments {
                None
            } else {
                descriptor
                    .trailing_comment
                    .clone()
                    .or_else(|| self.extract_trailing_comment_from_function(get))
            };
            if let Some(comment) = trailing_comment.as_deref() {
                self.write(" ");
                self.write(comment);
                if comment.trim_start().starts_with("/*") {
                    self.write(",");
                } else {
                    self.write_line();
                    self.write_indent();
                    self.write(",");
                }
            } else {
                self.write(",");
            }
            self.write_line();
        }
        if let Some(set) = &descriptor.set {
            if !self.remove_comments
                && let Some(comment) = &descriptor.set_leading_comment
            {
                self.write_indent();
                self.emit_multiline_comment(comment);
                self.write_line();
            }
            self.write_indent();
            self.write("set: ");
            self.emit_node(set);
            if !self.remove_comments {
                if let Some(comment) = self.extract_trailing_comment_from_function(set) {
                    self.write(" ");
                    self.write(&comment);
                    if comment.trim_start().starts_with("/*") {
                        self.write(",");
                    } else {
                        self.write_line();
                        self.write_indent();
                        self.write(",");
                    }
                } else {
                    self.write(",");
                }
            } else {
                self.write(",");
            }
            self.write_line();
        }
        self.write_indent();
        self.write("enumerable: ");
        self.write(if descriptor.enumerable {
            "true"
        } else {
            "false"
        });
        self.write(",");
        self.write_line();
        self.write_indent();
        self.write("configurable: ");
        self.write(if descriptor.configurable {
            "true"
        } else {
            "false"
        });
        if let Some(value) = &descriptor.value {
            self.write(",");
            self.write_line();
            self.write_indent();
            self.write("writable: ");
            self.write(if descriptor.writable { "true" } else { "false" });
            self.write(",");
            self.write_line();
            self.write_indent();
            self.write("value: ");
            self.emit_node(value);
        }
        self.write_line();

        self.decrease_indent();
        self.write_indent();
        self.write("});");
    }

    pub(super) fn emit_awaiter_call_node(&mut self, node: &IRNode) {
        let IRNode::AwaiterCall {
            this_arg,
            generator_body,
            needs_lexical_this_capture,
            hoisted_var_groups,
            promise_constructor,
            multiline_callback,
            directives,
        } = node
        else {
            return;
        };
        let previous_generator_state_name = self.generator_state_name;
        let hoisted_vars: Vec<&str> = hoisted_var_groups
            .iter()
            .flat_map(|group| group.iter().map(String::as_str))
            .collect();
        self.generator_state_name = Self::generator_state_name_for_hoisted(&hoisted_vars);
        self.write("return __awaiter(");
        self.emit_node(this_arg);
        if let Some(ctor) = promise_constructor {
            self.write(", void 0, ");
            self.write(ctor);
            self.write(", function () {");
        } else {
            self.write(", void 0, void 0, function () {");
        }
        let needs_multiline = !hoisted_var_groups.is_empty()
            || *multiline_callback
            || *needs_lexical_this_capture
            || !directives.is_empty();
        if !needs_multiline {
            // TSC keeps the generator call on the awaiter callback's
            // opening line when no hoisted variables are needed.
            self.write(" ");
            self.emit_node(generator_body);
            self.write(" });");
        } else {
            // Multi-line format with directives, hoisted vars, then generator
            self.write_line();
            self.increase_indent();
            // Directive prologues come first, before any var declarations
            for directive in directives {
                self.write_indent();
                self.write("\"");
                self.write(directive);
                self.write("\";");
                self.write_line();
            }
            for group in hoisted_var_groups {
                if *needs_lexical_this_capture {
                    for name in group {
                        self.write_indent();
                        self.write("var ");
                        self.write(name);
                        self.write(";");
                        self.write_line();
                    }
                } else {
                    self.write_indent();
                    self.write("var ");
                    self.write(&group.join(", "));
                    self.write(";");
                    self.write_line();
                }
            }
            if *needs_lexical_this_capture {
                self.write_indent();
                self.write("var _this = this;");
                self.write_line();
            }
            self.write_indent();
            self.emit_node(generator_body);
            self.decrease_indent();
            self.write_line();
            self.write_indent();
            self.write("});");
        }
        self.generator_state_name = previous_generator_state_name;
    }

    pub(super) fn emit_generator_body_node(&mut self, node: &IRNode) {
        let IRNode::GeneratorBody { has_await, cases } = node else {
            return;
        };
        self.write("return ");
        self.write_helper("__generator");
        self.write("(this, function (");
        self.write(self.generator_state_name);
        self.write(") {");
        if !*has_await || cases.is_empty() {
            // Simple body - always multi-line to match tsc
            if cases.is_empty() {
                self.write_line();
                self.increase_indent();
                self.write_indent();
                self.write("return [2 /*return*/];");
                self.write_line();
                self.decrease_indent();
                self.write_indent();
                self.write("});");
            } else {
                self.write_line();
                self.increase_indent();
                for stmt in &cases[0].statements {
                    self.write_indent();
                    self.emit_node(stmt);
                    self.write_line();
                }
                self.decrease_indent();
                self.write_indent();
                self.write("});");
            }
        } else {
            // Switch/case body
            self.write_line();
            self.increase_indent();
            self.write_indent();
            self.write("switch (");
            self.write(self.generator_state_name);
            self.write(".label) {");
            self.write_line();
            self.increase_indent();

            for case_item in cases {
                self.write_indent();
                self.write("case ");
                self.write(&case_item.label.to_string());
                self.write(":");
                // tsc puts simple single-statement cases on one line:
                //   case 0: return [4 /*yield*/, x];
                //   case 1: throw err;
                if case_item.statements.len() == 1
                    && Self::is_generator_inline_case_statement(&case_item.statements[0])
                {
                    self.write(" ");
                    self.emit_node(&case_item.statements[0]);
                    self.write_line();
                } else if !case_item.statements.is_empty() {
                    self.write_line();
                    self.increase_indent();
                    for stmt in &case_item.statements {
                        self.write_indent();
                        self.emit_node(stmt);
                        self.write_line();
                    }
                    self.decrease_indent();
                } else {
                    self.write_line();
                }
            }

            self.decrease_indent();
            self.write_indent();
            self.write("}");
            self.write_line();
            self.decrease_indent();
            self.write_indent();
            self.write("});");
        }
    }

    pub(super) fn emit_ast_ref_node(&mut self, node: &IRNode) {
        let IRNode::ASTRef(idx) = node else {
            return;
        };
        // Check if this node has a transform directive that we should apply
        if let Some(arena) = self.arena
            && let Some(node) = arena.get(*idx)
        {
            // For variable statements, directives are often attached to the nested
            // declaration-list node. Delegate full statement emission to Printer so
            // ES5 variable-list transforms still apply while preserving comments.
            if node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                && let Some(ref transforms) = self.transforms
                && let Some(var_stmt) = arena.get_variable(node)
            {
                let has_es5_var_list_directive =
                    var_stmt.declarations.nodes.iter().any(|decl_idx| {
                        matches!(
                            transforms.get(*decl_idx),
                            Some(
                                crate::context::transform::TransformDirective::ES5VariableDeclarationList { .. }
                            )
                        )
                    });
                if has_es5_var_list_directive {
                    let mut printer = AstPrinter::with_transforms_and_options(
                        arena,
                        transforms.clone(),
                        self.make_ast_printer_options(),
                    );
                    if let Some(source_text) = self.source_text {
                        printer.set_source_text(source_text);
                    }
                    printer.seed_function_scope_shadowed_names(&self.block_scope_shadowed_names);
                    printer.seed_block_scope_reserved_names(&self.block_scope_reserved_names);
                    printer.emit(*idx);
                    self.merge_ast_printer_block_scope_reserved_names(&printer);
                    self.write(printer.get_output());
                    return;
                }
            }

            if node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
                && let Some(ref transforms) = self.transforms
                && let Some(stmt) = arena.get_expression_statement(node)
                && let Some(expr_node) = arena.get(stmt.expression)
            {
                let target_expr = if expr_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                    arena
                        .get_parenthesized(expr_node)
                        .map(|p| p.expression)
                        .unwrap_or(stmt.expression)
                } else {
                    stmt.expression
                };
                let is_destructuring_assignment = arena
                    .get(target_expr)
                    .is_some_and(|n| n.kind == syntax_kind_ext::BINARY_EXPRESSION)
                    && arena
                        .get(target_expr)
                        .and_then(|n| arena.get_binary_expr(n))
                        .is_some_and(|bin| {
                            bin.operator_token == tsz_scanner::SyntaxKind::EqualsToken as u16
                                && arena.get(bin.left).is_some_and(|left| {
                                    left.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                        || left.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                })
                        });
                if is_destructuring_assignment {
                    let mut printer = AstPrinter::with_transforms_and_options(
                        arena,
                        transforms.clone(),
                        self.make_ast_printer_options(),
                    );
                    if let Some(source_text) = self.source_text {
                        printer.set_source_text(source_text);
                    }
                    printer.seed_function_scope_shadowed_names(&self.block_scope_shadowed_names);
                    printer.seed_block_scope_reserved_names(&self.block_scope_reserved_names);
                    printer.emit(*idx);
                    self.merge_ast_printer_block_scope_reserved_names(&printer);
                    self.write(printer.get_output());
                    return;
                }
            }

            // Get the directive for this node (clone to avoid borrow issues)
            let directive = self.transforms.as_ref().and_then(|t| t.get(*idx).cloned());

            if let Some(directive) = directive {
                use tsz_parser::parser::syntax_kind_ext;

                // Handle ES5ArrowFunction directive
                if matches!(
                    directive,
                    crate::context::transform::TransformDirective::ES5ArrowFunction { .. }
                ) && node.kind == syntax_kind_ext::ARROW_FUNCTION
                    && let Some(func_data) = arena.get_function(node)
                {
                    // Async arrows need __awaiter/__generator lowering.
                    // Delegate to the full AstPrinter which has the
                    // emit_arrow_function_es5 → emit_async_arrow_es5_inline path.
                    if func_data.is_async
                        && let Some(ref transforms) = self.transforms
                    {
                        let mut printer = AstPrinter::with_transforms_and_options(
                            arena,
                            transforms.clone(),
                            self.make_ast_printer_options(),
                        );
                        if let Some(source_text) = self.source_text {
                            printer.set_source_text(source_text);
                        }
                        printer
                            .seed_function_scope_shadowed_names(&self.block_scope_shadowed_names);
                        printer.seed_block_scope_reserved_names(&self.block_scope_reserved_names);
                        printer.emit_expression(*idx);
                        self.merge_ast_printer_block_scope_reserved_names(&printer);
                        self.write_embedded_output(printer.get_output());
                        return;
                    }
                    self.emit_arrow_function_es5_with_flags(arena, func_data);
                    return;
                }

                // Handle SubstituteThis directive
                if let crate::context::transform::TransformDirective::SubstituteThis {
                    ref capture_name,
                } = directive
                    && let Some(_ident) = arena.get_identifier(node)
                {
                    self.write(capture_name);
                    return;
                }

                // Handle SubstituteArguments directive
                if matches!(
                    directive,
                    crate::context::transform::TransformDirective::SubstituteArguments
                ) && let Some(_ident) = arena.get_identifier(node)
                {
                    self.write("arguments");
                    return;
                }

                // Handle ES5TemplateLiteral directive — downlevel template literals
                // to string concatenation using .concat() calls
                if matches!(
                    directive,
                    crate::context::transform::TransformDirective::ES5TemplateLiteral { .. }
                ) && let Some(ref transforms) = self.transforms
                {
                    let mut printer = AstPrinter::with_transforms_and_options(
                        arena,
                        transforms.clone(),
                        self.make_ast_printer_options(),
                    );
                    if let Some(source_text) = self.source_text {
                        printer.set_source_text(source_text);
                    }
                    printer.seed_function_scope_shadowed_names(&self.block_scope_shadowed_names);
                    printer.seed_block_scope_reserved_names(&self.block_scope_reserved_names);
                    printer.emit(*idx);
                    self.merge_ast_printer_block_scope_reserved_names(&printer);
                    self.write(printer.get_output());
                    return;
                }

                if matches!(
                    directive,
                    crate::context::transform::TransformDirective::ES5ForOf { .. }
                ) && node.kind == syntax_kind_ext::FOR_OF_STATEMENT
                    && let Some(ref transforms) = self.transforms
                {
                    let mut printer = AstPrinter::with_transforms_and_options(
                        arena,
                        transforms.clone(),
                        self.make_ast_printer_options(),
                    );
                    self.configure_ast_printer_namespace(&mut printer);
                    printer.seed_function_scope_shadowed_names(&self.block_scope_shadowed_names);
                    printer.seed_block_scope_reserved_names(&self.block_scope_reserved_names);
                    printer.emit(*idx);
                    self.merge_ast_printer_block_scope_reserved_names(&printer);
                    let output = printer.get_output().trim_end();
                    self.write_embedded_output(output);
                    return;
                }

                // Note: For other directive types, fall through to source text copy
                // This is intentional - we only handle directives that are ready
            }
        }

        // Delegate to AstPrinter whenever an arena is available so
        // output is canonically formatted; `write_embedded_output`
        // re-applies the IR printer's current indent to every interior
        // newline. The raw source-text fallback below is only reached
        // for arena-less callers (mostly tests) and is otherwise unsafe
        // because a statement's `node.end` may extend past its
        // terminating `;` when that `;` was consumed via
        // `parse_optional`/`parse_semicolon`.
        if let Some(arena) = self.arena {
            let mut printer = self.build_nested_ast_printer(arena);
            self.configure_ast_printer_namespace(&mut printer);
            if let Some(defer_end) = self.ast_arrow_comment_defer_end {
                if let Some((comment_start, comment_end)) =
                    printer.rightmost_concise_arrow_deferred_comment_range(*idx, defer_end)
                {
                    printer.with_arrow_concise_body_trailing_comments_deferred(
                        comment_start,
                        comment_end,
                        |printer| {
                            printer.emit(*idx);
                        },
                    );
                } else {
                    printer.emit(*idx);
                }
            } else {
                printer.emit(*idx);
            }
            self.merge_ast_printer_block_scope_reserved_names(&printer);
            let trimmed = printer.get_output().trim();
            if !trimmed.is_empty() {
                self.write_embedded_output(trimmed);
                return;
            }
        }

        // Last-resort source-text fallback when no arena is attached
        // or AstPrinter produced empty output for the node.
        if let Some(arena) = self.arena
            && let Some(text) = self.source_text
            && let Some(node) = arena.get(*idx)
        {
            let start = node.pos as usize;
            let end = std::cmp::min(node.end as usize, text.len());
            if start < end {
                let raw = &text[start..end];
                let trimmed = raw.trim();
                if !trimmed.is_empty() {
                    self.write(trimmed);
                    return;
                }
            }
        }
        // Fallback: emit placeholder
        self.write("undefined");
    }
}
