impl<'a> LoweringPass<'a> {
    /// Visit a call expression and detect `super()` calls
    fn visit_call_expression(&mut self, node: &Node, idx: NodeIndex) {
        let Some(call) = self.arena.get_call_expr(node) else {
            return;
        };

        // Check if this is a super() call
        let is_super_call = if let Some(expr_node) = self.arena.get(call.expression) {
            expr_node.kind == SyntaxKind::SuperKeyword as u16
        } else {
            false
        };

        // Emit directive if conditions met:
        // 1. This is a super(...) call
        // 2. Target is ES5
        // 3. We're inside a constructor
        // 4. The current class has a base class (is_derived)
        if is_super_call
            && self.ctx.target_es5
            && self.in_constructor
            && self.current_class_is_derived
        {
            self.transforms
                .insert(idx, TransformDirective::ES5SuperCall);
        }

        // ES5 `super.m(...)` / `super[e](...)` lowers to
        // `_super.prototype.m.call(R, ...)`. When the call sits inside a
        // `this`-capturing arrow, `R` must be the captured lexical receiver
        // (`_this`), not the lowered function's own `this`. Mark the callee's
        // `super` keyword with the active capture name so the printer threads
        // `_this` into the synthesized `.call(...)` receiver.
        if self.ctx.target_es5
            && self.this_capture_level > 0
            && let Some(super_keyword_idx) = self.super_member_call_super_keyword(call.expression)
        {
            let capture_name = self
                .enclosing_capture_names
                .last()
                .cloned()
                .unwrap_or_else(|| Arc::from("_this"));
            self.transforms.insert(
                super_keyword_idx,
                TransformDirective::SubstituteThis { capture_name },
            );
        }

        // CJS-like dynamic import: import("mod") needs __importStar helper.
        // `--module none --outFile` uses the same lowering below native
        // dynamic import support without promoting the script to CJS.
        // This applies regardless of esModuleInterop setting.
        // Skip for node module CJS files where native import() is supported.
        if (self.commonjs_mode || (self.ctx.module_none_out_file && self.ctx.needs_es2020_lowering))
            && !self.ctx.options.resolved_node_module_to_cjs
            && !is_super_call
            && let Some(expr_node) = self.arena.get(call.expression)
            && expr_node.kind == SyntaxKind::ImportKeyword as u16
        {
            let helpers = self.transforms.helpers_mut();
            helpers.import_star = true;
            helpers.create_binding = true;
        }

        // __rewriteRelativeImportExtension helper: needed when
        // rewriteRelativeImportExtensions is set and a dynamic import() or
        // require() call has a non-string-literal specifier argument.
        if self.ctx.options.rewrite_relative_import_extensions
            && !is_super_call
            && let Some(expr_node) = self.arena.get(call.expression)
        {
            let is_import_call = expr_node.kind == SyntaxKind::ImportKeyword as u16;
            let is_require_call = !is_import_call
                && expr_node.kind == SyntaxKind::Identifier as u16
                && self
                    .arena
                    .get_identifier(expr_node)
                    .is_some_and(|id| id.escaped_text == "require");
            if is_import_call || is_require_call {
                let first_arg = call
                    .arguments
                    .as_ref()
                    .and_then(|args| args.nodes.iter().copied().find(|n| n.is_some()));
                let is_string_literal =
                    first_arg.and_then(|a| self.arena.get(a)).is_some_and(|n| {
                        n.kind == SyntaxKind::StringLiteral as u16
                            || n.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                    });
                if !is_string_literal {
                    self.transforms
                        .helpers_mut()
                        .rewrite_relative_import_extension = true;
                }
            }
        }

        // Check if call has spread arguments and needs ES5 transformation
        if self.ctx.target_es5
            && !is_super_call
            && let Some(ref args) = call.arguments
        {
            let has_spread = args
                .nodes
                .iter()
                .any(|&arg_idx| emit_utils::is_spread_element(self.arena, arg_idx));
            if has_spread {
                self.transforms
                    .insert(idx, TransformDirective::ES5CallSpread { call_expr: idx });
                // __spreadArray is needed when spread arguments must be merged
                // with additional segments. With downlevelIteration, even a
                // single spread must go through __read/__spreadArray so
                // iterable-but-not-array-like values are expanded before apply().
                if self.call_spread_needs_spread_array(args.nodes.as_slice())
                    || self.ctx.options.downlevel_iteration
                {
                    self.transforms.helpers_mut().mark_spread_array();
                    // When downlevelIteration is enabled, spread on iterables
                    // needs __read to convert iterator results to arrays.
                    if self.ctx.options.downlevel_iteration {
                        self.transforms.helpers_mut().mark_read();
                    }
                }
            }
        }

        // Continue traversal
        self.visit(call.expression);
        if let Some(ref args) = call.arguments {
            for &arg_idx in &args.nodes {
                self.visit(arg_idx);
            }
        }
    }

    /// Visit a new expression and traverse callee + arguments for nested transforms.
    fn visit_new_expression(&mut self, node: &Node, idx: NodeIndex) {
        let Some(new_expr) = self.arena.get_call_expr(node) else {
            return;
        };

        if self.ctx.target_es5
            && let Some(ref args) = new_expr.arguments
        {
            let has_spread = args
                .nodes
                .iter()
                .any(|&arg_idx| emit_utils::is_spread_element(self.arena, arg_idx));
            if has_spread {
                self.transforms
                    .insert(idx, TransformDirective::ES5NewSpread { new_expr: idx });
                // New expressions always need __spreadArray because we
                // prepend void 0 to the args array for bind().
                self.transforms.helpers_mut().mark_spread_array();
                if self.ctx.options.downlevel_iteration {
                    self.transforms.helpers_mut().mark_read();
                }
            }
        }

        self.visit(new_expr.expression);
        if let Some(ref args) = new_expr.arguments {
            for &arg_idx in &args.nodes {
                self.visit(arg_idx);
            }
        }
    }

    /// Visit a variable statement
    fn visit_variable_statement(&mut self, node: &Node, idx: NodeIndex) {
        self.lower_variable_statement(node, idx, false);
    }

    fn lower_variable_statement(&mut self, node: &Node, idx: NodeIndex, force_export: bool) {
        let Some(var_stmt) = self.arena.get_variable(node) else {
            return;
        };

        let is_exported = self.is_commonjs()
            && !self.has_export_assignment
            && (force_export
                || self
                    .arena
                    .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword));

        if is_exported {
            let export_names = self.collect_variable_names(&var_stmt.declarations);
            if !export_names.is_empty() {
                self.transforms.insert(
                    idx,
                    TransformDirective::CommonJSExport {
                        names: Arc::from(export_names),
                        is_default: false,
                        inner: Box::new(TransformDirective::Identity),
                    },
                );
            }
        }

        // Visit each declaration
        for &decl in &var_stmt.declarations.nodes {
            self.visit(decl);
        }
    }

    fn visit_function_expression(&mut self, node: &Node, idx: NodeIndex) {
        let Some(func) = self.arena.get_function(node) else {
            return;
        };

        // Save and reset in_constructor state for nested function scope
        let prev_in_constructor = self.in_constructor;
        let prev_in_static = self.in_static_context;
        let prev_class_alias = self.current_class_alias.take();
        self.in_constructor = false;
        self.in_static_context = false;

        if self.ctx.target_es5 {
            if func.is_async {
                self.mark_function_parameter_transform_helpers(&func.parameters);
                if func.asterisk_token {
                    self.mark_async_generator_helpers();
                } else {
                    self.mark_async_helpers();
                }
                self.transforms.insert(
                    idx,
                    TransformDirective::ES5AsyncFunction { function_node: idx },
                );
            } else if func.asterisk_token {
                self.transforms.helpers_mut().generator = true;
                self.mark_function_parameter_transform_helpers(&func.parameters);
                self.transforms.insert(
                    idx,
                    TransformDirective::ES5GeneratorFunction { function_node: idx },
                );
            } else if self.function_parameters_need_body_prologue_transform(&func.parameters) {
                self.mark_function_parameter_transform_helpers(&func.parameters);
                self.transforms.insert(
                    idx,
                    TransformDirective::ES5FunctionParameters { function_node: idx },
                );
            }
        } else if func.is_async
            && ((func.asterisk_token && self.ctx.needs_es2018_lowering)
                || (!func.asterisk_token && self.ctx.needs_async_lowering))
        {
            if func.asterisk_token {
                // ES2015+: async generators need __asyncGenerator + __await helpers
                self.mark_async_generator_helpers();
            } else {
                // ES2015/ES2016: non-generator async functions need __awaiter
                self.mark_async_helpers();
            }
        } else if self.function_parameters_need_body_prologue_transform(&func.parameters) {
            self.mark_function_parameter_transform_helpers(&func.parameters);
            self.transforms.insert(
                idx,
                TransformDirective::ES5FunctionParameters { function_node: idx },
            );
        }

        for &param_idx in &func.parameters.nodes {
            self.visit(param_idx);
        }

        if func.body.is_some() {
            // Track this function body as a potential _this capture scope
            if self.ctx.target_es5 {
                let cn =
                    self.compute_this_capture_name_with_params(func.body, Some(&func.parameters));
                self.enclosing_function_bodies.push(func.body);
                self.enclosing_capture_names.push(cn);
            }
            self.visit(func.body);
            if self.ctx.target_es5 {
                self.enclosing_function_bodies.pop();
                self.enclosing_capture_names.pop();
            }
        }

        // Restore in_constructor state
        self.in_constructor = prev_in_constructor;
        self.in_static_context = prev_in_static;
        self.current_class_alias = prev_class_alias;
    }
}
