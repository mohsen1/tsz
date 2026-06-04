impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_call_expression(&mut self, idx: NodeIndex, node: &Node) {
        let Some(call) = self.arena.get_call_expr(node) else {
            return;
        };

        if let Some(index_alias) = self.scoped_static_super_index_alias.as_ref().cloned()
            && let Some(expr_node) = self.arena.get(call.expression)
            && expr_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            && let Some(access) = self.arena.get_access_expr(expr_node)
            && let Some(base) = self.arena.get(access.expression)
            && base.kind == SyntaxKind::SuperKeyword as u16
        {
            self.write(&index_alias);
            self.write("(");
            self.emit(access.name_or_argument);
            self.write(")");
            if self.scoped_static_super_index_value_access {
                self.write(".value");
            }
            self.write(".call(");
            self.emit_scoped_static_super_receiver();
            if let Some(ref args) = call.arguments {
                for &arg_idx in &args.nodes {
                    self.write(", ");
                    self.emit(arg_idx);
                }
            }
            self.write(")");
            return;
        }

        if let Some(base_alias) = self.scoped_static_super_base_alias.as_ref().cloned()
            && let Some(expr_node) = self.arena.get(call.expression)
        {
            if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && let Some(access) = self.arena.get_access_expr(expr_node)
                && let Some(base) = self.arena.get(access.expression)
                && base.kind == SyntaxKind::SuperKeyword as u16
            {
                if self.scoped_static_super_direct_access {
                    if self.has_optional_call_token(node, call.expression, call.arguments.as_ref())
                    {
                        let func_temp = self.make_unique_name_hoisted();
                        self.write("(");
                        self.write(&func_temp);
                        self.write(" = ");
                        self.write(&base_alias);
                        self.write(".");
                        self.emit_property_name_without_import_substitution(
                            access.name_or_argument,
                        );
                        self.write(") === null || ");
                        self.write(&func_temp);
                        self.write(" === void 0 ? void 0 : ");
                        self.write(&func_temp);
                        self.write(".call(");
                        self.emit_scoped_static_super_receiver();
                        if let Some(ref args) = call.arguments {
                            for &arg_idx in &args.nodes {
                                self.write(", ");
                                self.emit(arg_idx);
                            }
                        }
                        self.write(")");
                        return;
                    }
                    self.write(&base_alias);
                    self.write(".");
                    self.emit_property_name_without_import_substitution(access.name_or_argument);
                    self.write(".call(");
                    self.emit_scoped_static_super_receiver();
                    if let Some(ref args) = call.arguments {
                        for &arg_idx in &args.nodes {
                            self.write(", ");
                            self.emit(arg_idx);
                        }
                    }
                    self.write(")");
                    return;
                }
                self.write("Reflect.get(");
                self.write(&base_alias);
                self.write(", ");
                self.emit_scoped_static_super_property_name(access.name_or_argument);
                self.write(", ");
                self.emit_scoped_static_super_receiver();
                self.write(").call(");
                self.emit_scoped_static_super_receiver();
                if let Some(ref args) = call.arguments {
                    for &arg_idx in &args.nodes {
                        self.write(", ");
                        self.emit(arg_idx);
                    }
                }
                self.write(")");
                return;
            }

            if expr_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                && let Some(access) = self.arena.get_access_expr(expr_node)
                && let Some(base) = self.arena.get(access.expression)
                && base.kind == SyntaxKind::SuperKeyword as u16
            {
                if self.scoped_static_super_direct_access {
                    if let Some(index_alias) =
                        self.scoped_static_super_index_alias.as_ref().cloned()
                    {
                        self.write(&index_alias);
                        self.write("(");
                        self.emit(access.name_or_argument);
                        self.write(")");
                        if self.scoped_static_super_index_value_access {
                            self.write(".value");
                        }
                        self.write(".call(");
                        self.emit_scoped_static_super_receiver();
                        if let Some(ref args) = call.arguments {
                            for &arg_idx in &args.nodes {
                                self.write(", ");
                                self.emit(arg_idx);
                            }
                        }
                        self.write(")");
                        return;
                    }
                    self.write(&base_alias);
                    self.write("[");
                    self.emit(access.name_or_argument);
                    self.write("].call(");
                    self.emit_scoped_static_super_receiver();
                    if let Some(ref args) = call.arguments {
                        for &arg_idx in &args.nodes {
                            self.write(", ");
                            self.emit(arg_idx);
                        }
                    }
                    self.write(")");
                    return;
                }
                self.write("Reflect.get(");
                self.write(&base_alias);
                self.write(", ");
                self.emit(access.name_or_argument);
                self.write(", ");
                self.emit_scoped_static_super_receiver();
                self.write(").call(");
                self.emit_scoped_static_super_receiver();
                if let Some(ref args) = call.arguments {
                    for &arg_idx in &args.nodes {
                        self.write(", ");
                        self.emit(arg_idx);
                    }
                }
                self.write(")");
                return;
            }
        }

        if self.is_optional_chain(node) {
            if self.ctx.options.target.supports_es2020() {
                self.emit_unwrapping_type_args(call.expression);
                if self.has_optional_call_token(node, call.expression, call.arguments.as_ref()) {
                    self.write("?.");
                }
                self.emit_call_arguments(node, call.arguments.as_ref());
                return;
            }

            let has_optional_call_token =
                self.has_optional_call_token(node, call.expression, call.arguments.as_ref());
            if has_optional_call_token
                && self
                    .emit_optional_private_field_call_expression(call.expression, &call.arguments)
            {
                return;
            }
            if let Some(call_expr) = self.arena.get(call.expression)
                && (call_expr.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || call_expr.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            {
                self.emit_optional_method_call_expression(
                    call_expr,
                    node,
                    &call.arguments,
                    has_optional_call_token,
                );
                return;
            }

            self.emit_optional_call_expression(node, call.expression, &call.arguments);
            return;
        }

        if self.emit_erased_object_literal_access_call(node, call.expression, &call.arguments) {
            return;
        }

        // Private field call lowering:
        // `this.#fn(args)` → `__classPrivateFieldGet(this, _C_fn, "f").call(this, args)`
        // `this.#method(args)` → `__classPrivateFieldGet(this, _C_instances, "m", _C_method).call(this, args)`
        if !self.private_field_weakmaps.is_empty()
            && let Some(expr_node) = self.arena.get(call.expression)
            && expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.arena.get_access_expr(expr_node)
            && let Some(name_node) = self.arena.get(access.name_or_argument)
            && name_node.kind == SyntaxKind::PrivateIdentifier as u16
            && let Some(field_name) = get_private_field_name(self.arena, access.name_or_argument)
        {
            let clean_name = field_name
                .strip_prefix('#')
                .unwrap_or(&field_name)
                .to_string();
            if let Some(weakmap_name) = self.private_field_weakmaps.get(&clean_name).cloned() {
                let expression = access.expression;
                // Side-effecting receivers must be captured once; `this` in `.call()` must
                // match the receiver used in `__classPrivateFieldGet`.
                let receiver_temp = if !self.private_call_receiver_is_simple(expression) {
                    Some(self.make_unique_name_hoisted())
                } else {
                    None
                };

                let receiver_temp_str = receiver_temp.as_deref();
                self.write_helper("__classPrivateFieldGet");
                self.write("(");
                if let Some(temp) = receiver_temp_str {
                    self.write("(");
                    self.write(temp);
                    self.write(" = ");
                    self.emit_private_receiver(expression, &clean_name);
                    self.write(")");
                } else {
                    self.emit_private_receiver(expression, &clean_name);
                }
                self.write(", ");
                if let Some(info) = self.private_member_info.get(&clean_name).cloned() {
                    if let Some(ref state_var) = info.state_var {
                        self.write(state_var);
                    } else {
                        self.write(&weakmap_name);
                    }
                    self.write(", \"");
                    self.write(info.kind);
                    self.write("\"");
                    if let Some(ref fn_ref) = info.fn_ref {
                        self.write(", ");
                        self.write(fn_ref);
                    }
                } else {
                    self.write(&weakmap_name);
                    self.write(", \"f\"");
                }
                self.write(").call(");
                if let Some(temp) = receiver_temp_str {
                    self.write(temp);
                } else {
                    self.emit_private_receiver(expression, &clean_name);
                }
                if let Some(ref args) = call.arguments {
                    for &arg_idx in &args.nodes {
                        if arg_idx.is_some() {
                            self.write(", ");
                            self.emit(arg_idx);
                        }
                    }
                }
                self.write(")");
                return;
            }
        }

        if self.ctx.target_es5
            && let Some(expr_node) = self.arena.get(call.expression)
        {
            if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && let Some(access) = self.arena.get_access_expr(expr_node)
                && let Some(base) = self.arena.get(access.expression)
                && base.kind == SyntaxKind::SuperKeyword as u16
            {
                self.emit_es5_super_property_base();
                self.write(".");
                self.emit(access.name_or_argument);
                self.write(".call(");
                self.emit_es5_super_call_receiver(access.expression);
                if let Some(ref args) = call.arguments {
                    for &arg_idx in &args.nodes {
                        self.write(", ");
                        self.emit(arg_idx);
                    }
                }
                self.write(")");
                return;
            }
            if expr_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                && let Some(access) = self.arena.get_access_expr(expr_node)
                && let Some(base) = self.arena.get(access.expression)
                && base.kind == SyntaxKind::SuperKeyword as u16
            {
                self.emit_es5_super_property_base();
                self.write("[");
                self.emit(access.name_or_argument);
                self.write("].call(");
                self.emit_es5_super_call_receiver(access.expression);
                if let Some(ref args) = call.arguments {
                    for &arg_idx in &args.nodes {
                        self.write(", ");
                        self.emit(arg_idx);
                    }
                }
                self.write(")");
                return;
            }
        }

        if !self.suppress_commonjs_named_import_substitution
            && let Some(expr_node) = self.arena.get(call.expression)
            && let Some(ident) = self.arena.get_identifier(expr_node)
            && let Some(subst) = self
                .commonjs_named_import_substitutions
                .get(&ident.escaped_text)
        {
            let subst = subst.clone();
            // In System modules, import substitutions are already property accesses
            // on module-scoped variables (e.g. `repeat_1.default`), so no `(0, ...)`
            // indirection is needed — `this` binding is not a concern.
            if self.in_system_execute_body {
                self.write(&subst);
            } else {
                self.write("(0, ");
                self.write(&subst);
                self.write(")");
            }
            self.emit_call_arguments(node, call.arguments.as_ref());
            return;
        }

        // CJS exported variable indirect call: `foo()` → `(0, exports.foo)()`
        // The `(0, ...)` wrapper prevents `this` binding to `exports`.
        if !self.suppress_ns_qualification
            && let Some(expr_node) = self.arena.get(call.expression)
            && let Some(ident) = self.arena.get_identifier(expr_node)
            && self
                .commonjs_exported_var_names
                .contains(ident.escaped_text.as_str())
        {
            self.write("(0, exports.");
            self.write_identifier(&ident.escaped_text);
            self.write(")");
            self.emit_call_arguments(node, call.arguments.as_ref());
            return;
        }

        if let Some(expr_node) = self.arena.get(call.expression)
            && expr_node.kind == SyntaxKind::ImportKeyword as u16
        {
            match self.ctx.original_module_kind {
                Some(ModuleKind::System) => {
                    self.emit_system_dynamic_import_call(call.arguments.as_ref());
                    return;
                }
                Some(ModuleKind::AMD | ModuleKind::UMD) => {
                    self.emit_amd_or_umd_dynamic_import_call(idx, call.arguments.as_ref());
                    return;
                }
                _ => {}
            }
        }

        let should_lower_dynamic_import_to_require = self.ctx.is_effectively_commonjs()
            || (self.ctx.module_none_out_file && self.ctx.needs_es2020_lowering);

        // CJS-like dynamic import: `import("mod")` → `Promise.resolve().then(() => __importStar(require("mod")))`
        // For non-string-literal specifiers, tsc evaluates the expression eagerly:
        //   `import(expr)` → `Promise.resolve(\`${expr}\`).then(s => __importStar(require(s)))`
        // In CommonJS module mode, dynamic import() expressions need to be transformed
        // to use require() wrapped in __importStar for proper ESM/CJS interop.
        // `--module none --outFile` script bundles use the same expression
        // lowering for targets below native dynamic import without making the
        // source a CommonJS module.
        // Use is_effectively_commonjs() to also catch the case where module is temporarily
        // set to None during CJS export body emission (e.g., inside exported async functions).
        // Skip for node module CJS files where native import() is supported.
        if should_lower_dynamic_import_to_require
            && !self.ctx.options.resolved_node_module_to_cjs
            && let Some(expr_node) = self.arena.get(call.expression)
            && expr_node.kind == SyntaxKind::ImportKeyword as u16
        {
            // Get the first valid argument (the module specifier). String
            // literals (and the no-argument case) lower to
            // `Promise.resolve().then(() => __importStar(require("mod")))`;
            // expression specifiers use the `Promise.resolve(`${spec}`)` form.
            let first_arg = call
                .arguments
                .as_ref()
                .and_then(|args| args.nodes.iter().copied().find(|n| n.is_some()));
            self.emit_dynamic_import_commonjs_promise(first_arg, None);
            return;
        }

        // rewriteRelativeImportExtensions: handle ESM import() and require() calls.
        // For string literal specifiers, rewrite the extension inline.
        // For non-literal specifiers, wrap with __rewriteRelativeImportExtension(expr).
        if self.ctx.options.rewrite_relative_import_extensions
            && let Some(expr_node) = self.arena.get(call.expression)
        {
            let is_import_keyword = expr_node.kind == SyntaxKind::ImportKeyword as u16;
            let is_require_ident = !is_import_keyword
                && expr_node.kind == SyntaxKind::Identifier as u16
                && self
                    .arena
                    .get_identifier(expr_node)
                    .is_some_and(|id| id.escaped_text == "require");

            if is_import_keyword || is_require_ident {
                let first_arg = call
                    .arguments
                    .as_ref()
                    .and_then(|args| args.nodes.iter().copied().find(|n| n.is_some()));
                let first_arg_node = first_arg.and_then(|idx| self.arena.get(idx));
                let is_string_literal = first_arg_node.is_some_and(|n| {
                    n.kind == SyntaxKind::StringLiteral as u16
                        || n.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                });

                if is_string_literal {
                    // Rewrite inline: import("./foo.ts") -> import("./foo.js")
                    if is_import_keyword {
                        self.write("import");
                    } else {
                        self.write("require");
                    }
                    self.write("(");
                    if let Some(first) = first_arg {
                        self.emit_maybe_rewritten_module_specifier_arg(first);
                    }
                    if let Some(ref args) = call.arguments {
                        let valid_args: Vec<_> =
                            args.nodes.iter().copied().filter(|n| n.is_some()).collect();
                        for &arg_idx in valid_args.iter().skip(1) {
                            self.write(", ");
                            self.emit(arg_idx);
                        }
                    }
                    self.write(")");
                    return;
                } else if first_arg.is_some() {
                    // Non-literal: wrap with __rewriteRelativeImportExtension
                    if is_import_keyword {
                        self.write("import");
                    } else {
                        self.write("require");
                    }
                    self.write("(");
                    if let Some(first) = first_arg {
                        self.emit_rewrite_helper_call(first);
                    }
                    if let Some(ref args) = call.arguments {
                        let valid_args: Vec<_> =
                            args.nodes.iter().copied().filter(|n| n.is_some()).collect();
                        for &arg_idx in valid_args.iter().skip(1) {
                            self.write(", ");
                            self.emit(arg_idx);
                        }
                    }
                    self.write(")");
                    return;
                }
            }
        }

        if !self.ctx.options.target.supports_es2020()
            && self.emit_parenthesized_optional_access_call_expression(
                call.expression,
                &call.arguments,
            )
        {
            return;
        }

        // Signal access position so `(new a)()` keeps parens (vs `new a()`).
        let prev = self.paren_in_access_position;
        let prev_call = self.paren_is_direct_call_callee;
        self.paren_in_access_position = true;
        self.paren_is_direct_call_callee = true;
        // When the callee is ExpressionWithTypeArguments (e.g., `f<T>(args)`),
        // unwrap without parens since the call parens provide grouping.
        self.emit_unwrapping_type_args(call.expression);
        self.paren_in_access_position = prev;
        self.paren_is_direct_call_callee = prev_call;
        // Map the opening `(` to its source position
        if let Some(expr_node) = self.arena.get(call.expression) {
            self.map_token_after(expr_node.end, node.end, b'(');
        }
        self.write("(");
        // The call's own parens provide grouping, so clear the "needs parens"
        // flags to avoid double-parenthesization when an argument contains a
        // downlevel optional chain or nullish coalescing expression.
        let prev_optional = self.ctx.flags.optional_chain_needs_parens;
        let prev_nullish = self.ctx.flags.nullish_coalescing_needs_parens;
        self.ctx.flags.optional_chain_needs_parens = false;
        self.ctx.flags.nullish_coalescing_needs_parens = false;
        if let Some(ref args) = call.arguments {
            // Filter out NodeIndex::NONE (omitted arguments from parser error recovery).
            // In call expressions, `foo(a,,b)` should emit `foo(a, b)`, not `foo(a, , b)`.
            let valid_args: Vec<_> = args
                .nodes
                .iter()
                .copied()
                .filter(|&idx| self.call_argument_should_emit(idx))
                .collect();
            // For the first argument, emit any comments between '(' and the argument
            // This handles: func(/*comment*/ arg)
            if let Some(first_arg) = valid_args.first()
                && let Some(arg_node) = self.arena.get(*first_arg)
            {
                let open_paren_pos = self
                    .find_call_open_paren_position(node, Some(args))
                    .unwrap_or(node.pos);
                self.emit_call_leading_argument_comments(open_paren_pos, arg_node.pos);
            }
            self.emit_comma_separated(&valid_args);
            if let Some(last_arg) = valid_args.last()
                && let Some(close_paren_pos) =
                    self.find_call_closing_paren_position(node, Some(args))
            {
                let last_arg_end = self.call_argument_comment_boundary(*last_arg);
                self.emit_call_trailing_argument_comments(last_arg_end, close_paren_pos);
            } else if valid_args.is_empty() {
                self.emit_empty_call_argument_comments(node, Some(args));
            }
        }
        self.ctx.flags.optional_chain_needs_parens = prev_optional;
        self.ctx.flags.nullish_coalescing_needs_parens = prev_nullish;
        // Map the closing `)` to its source position
        self.map_closing_paren(node);
        self.write(")");
    }

    fn emit_parenthesized_optional_access_call_expression(
        &mut self,
        callee: NodeIndex,
        args: &Option<tsz_parser::parser::NodeList>,
    ) -> bool {
        let unwrapped = self.unwrap_paren_and_type_assertion(callee);
        if unwrapped == callee {
            return false;
        }
        let Some(access_node) = self.arena.get(unwrapped).copied() else {
            return false;
        };
        if access_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && access_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return false;
        }
        let Some(access) = self.arena.get_access_expr(&access_node) else {
            return false;
        };
        let access_expression = access.expression;
        let access_name_or_argument = access.name_or_argument;
        let access_question_dot_token = access.question_dot_token;
        if !access_node.is_optional_chain()
            && !access_question_dot_token
            && !self.expression_is_optional_chain_continuation(access_expression)
        {
            return false;
        }

        if self.emit_parenthesized_optional_receiver_tail_call(
            access_node.kind,
            access_expression,
            access_name_or_argument,
            access_question_dot_token,
            args,
        ) {
            return true;
        }
        if self.emit_parenthesized_optional_receiver_access_tail_call(
            access_node.kind,
            access_expression,
            access_name_or_argument,
            access_question_dot_token,
            args,
        ) {
            return true;
        }

        if !access_question_dot_token {
            return false;
        }

        let receiver_temp = if self.is_simple_nullish_expression(access_expression) {
            None
        } else {
            Some(self.make_unique_name_hoisted())
        };

        self.write("(");
        if let Some(temp) = receiver_temp.as_deref() {
            self.write("(");
            self.write(temp);
            self.write(" = ");
            self.emit(access_expression);
            self.write(") === null || ");
            self.write(temp);
            self.write(" === void 0 ? void 0 : ");
            self.write(temp);
        } else {
            self.emit(access_expression);
            self.write(" === null || ");
            self.emit(access_expression);
            self.write(" === void 0 ? void 0 : ");
            self.emit(access_expression);
        }
        self.emit_access_suffix(access_node.kind, access_name_or_argument);
        self.write(").call(");
        if let Some(temp) = receiver_temp.as_deref() {
            self.write(temp);
        } else {
            self.emit(access_expression);
        }
        self.emit_optional_call_tail_arguments(args.as_ref());
        true
    }

    fn emit_parenthesized_optional_receiver_tail_call(
        &mut self,
        access_kind: u16,
        access_expression: NodeIndex,
        access_name_or_argument: NodeIndex,
        access_question_dot_token: bool,
        args: &Option<tsz_parser::parser::NodeList>,
    ) -> bool {
        if access_question_dot_token {
            return false;
        }
        let Some(receiver_node) = self.arena.get(access_expression).copied() else {
            return false;
        };
        if receiver_node.kind != syntax_kind_ext::CALL_EXPRESSION
            || !self.expression_is_optional_chain_continuation(access_expression)
        {
            return false;
        }
        let Some(receiver_call) = self.arena.get_call_expr(&receiver_node).cloned() else {
            return false;
        };
        let Some(receiver_access_node) = self.arena.get(receiver_call.expression).copied() else {
            return false;
        };
        if receiver_access_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && receiver_access_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return false;
        }
        let Some(receiver_access) = self.arena.get_access_expr(&receiver_access_node) else {
            return false;
        };
        let receiver_base_expression = receiver_access.expression;
        let receiver_name_or_argument = receiver_access.name_or_argument;
        if !receiver_access.question_dot_token {
            return false;
        }

        self.write("(");
        let receiver_temp;
        if self.is_simple_nullish_expression(receiver_base_expression) {
            receiver_temp = self.make_unique_name_hoisted();
            self.emit(receiver_base_expression);
            self.write(" === null || ");
            self.emit(receiver_base_expression);
            self.write(" === void 0 ? void 0 : ");
            self.write("(");
            self.write(&receiver_temp);
            self.write(" = ");
            self.emit(receiver_base_expression);
        } else {
            let base_temp = self.make_unique_name_hoisted();
            receiver_temp = self.make_unique_name_hoisted();
            self.write("(");
            self.write(&base_temp);
            self.write(" = ");
            self.emit(receiver_base_expression);
            self.write(") === null || ");
            self.write(&base_temp);
            self.write(" === void 0 ? void 0 : ");
            self.write("(");
            self.write(&receiver_temp);
            self.write(" = ");
            self.write(&base_temp);
        }
        self.emit_access_suffix(receiver_access_node.kind, receiver_name_or_argument);
        self.emit_call_arguments(&receiver_node, receiver_call.arguments.as_ref());
        self.write(")");
        self.emit_access_suffix(access_kind, access_name_or_argument);
        self.write(").call(");
        self.write(&receiver_temp);
        self.emit_optional_call_tail_arguments(args.as_ref());
        true
    }

    fn emit_parenthesized_optional_receiver_access_tail_call(
        &mut self,
        access_kind: u16,
        access_expression: NodeIndex,
        access_name_or_argument: NodeIndex,
        access_question_dot_token: bool,
        args: &Option<tsz_parser::parser::NodeList>,
    ) -> bool {
        if access_question_dot_token {
            return false;
        }
        let Some(receiver_access_node) = self.arena.get(access_expression).copied() else {
            return false;
        };
        if receiver_access_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && receiver_access_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return false;
        }
        let Some(receiver_access) = self.arena.get_access_expr(&receiver_access_node) else {
            return false;
        };
        if !receiver_access.question_dot_token {
            return false;
        }

        let receiver_base_expression = receiver_access.expression;
        let receiver_name_or_argument = receiver_access.name_or_argument;

        self.write("(");
        let receiver_temp;
        if self.is_simple_nullish_expression(receiver_base_expression) {
            receiver_temp = self.make_unique_name_hoisted();
            self.emit(receiver_base_expression);
            self.write(" === null || ");
            self.emit(receiver_base_expression);
            self.write(" === void 0 ? void 0 : ");
            self.write("(");
            self.write(&receiver_temp);
            self.write(" = ");
            self.emit(receiver_base_expression);
        } else {
            let base_temp = self.make_unique_name_hoisted();
            receiver_temp = self.make_unique_name_hoisted();
            self.write("(");
            self.write(&base_temp);
            self.write(" = ");
            self.emit(receiver_base_expression);
            self.write(") === null || ");
            self.write(&base_temp);
            self.write(" === void 0 ? void 0 : ");
            self.write("(");
            self.write(&receiver_temp);
            self.write(" = ");
            self.write(&base_temp);
        }
        self.emit_access_suffix(receiver_access_node.kind, receiver_name_or_argument);
        self.write(")");
        self.emit_access_suffix(access_kind, access_name_or_argument);
        self.write(").call(");
        self.write(&receiver_temp);
        self.emit_optional_call_tail_arguments(args.as_ref());
        true
    }

    fn emit_access_suffix(&mut self, kind: u16, name_or_argument: NodeIndex) {
        if kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            self.write(".");
            self.emit_property_name_without_import_substitution(name_or_argument);
        } else {
            self.write("[");
            self.emit(name_or_argument);
            self.write("]");
        }
    }

    /// Emit a dynamic `import()` for System output as `context_1.import(spec)`.
    ///
    /// The System loader's `import` hook only accepts the module id, so `tsc`
    /// drops any options/attributes (`import(spec, { with: ... })`) argument.
    fn emit_system_dynamic_import_call(&mut self, args: Option<&tsz_parser::parser::NodeList>) {
        self.write("context_1.import(");
        if let Some(first) = self.first_dynamic_import_argument(args) {
            self.emit_maybe_rewritten_module_specifier_arg(first);
        }
        self.write(")");
    }

    /// Emit a downlevel dynamic `import()` for AMD/UMD output.
    ///
    /// AMD emits a single `new Promise(...).then(__importStar)` call expression.
    /// UMD additionally guards it with `__syncRequire ? <cjs> : <amd>`, a
    /// `ConditionalExpression`. Because UMD repeats the specifier across both
    /// branches, a specifier that is neither a string literal nor a bare
    /// identifier is captured once into a hoisted temp (`_a = spec, ...`),
    /// turning the substitution into a comma `SequenceExpression`; this mirrors
    /// `tsc`. AMD never captures, and string-literal/identifier specifiers are
    /// repeated inline.
    fn emit_amd_or_umd_dynamic_import_call(
        &mut self,
        call_idx: NodeIndex,
        args: Option<&tsz_parser::parser::NodeList>,
    ) {
        let first_arg = self.first_dynamic_import_argument(args);
        let is_umd = matches!(self.ctx.original_module_kind, Some(ModuleKind::UMD));
        let capture = is_umd
            && first_arg.is_some_and(|arg| {
                !self.dynamic_import_arg_is_string_like(arg)
                    && !self.dynamic_import_arg_is_identifier(arg)
            });

        // A captured specifier yields a comma sequence, which needs parentheses
        // in more parent positions than a bare conditional, so the paren
        // decision depends on which substitution form is emitted.
        let needs_parens = is_umd && self.umd_dynamic_import_needs_parens(call_idx, capture);
        if needs_parens {
            self.write("(");
        }

        let temp = if capture {
            let temp = self.make_unique_name_hoisted();
            self.write(&temp);
            self.write(" = ");
            if self.ctx.options.rewrite_relative_import_extensions {
                if let Some(first) = first_arg {
                    self.emit_rewrite_helper_call(first);
                }
            } else if let Some(first) = first_arg {
                self.emit(first);
            }
            self.write(", ");
            Some(temp)
        } else {
            None
        };

        if is_umd {
            self.write("__syncRequire ? ");
            self.emit_dynamic_import_commonjs_promise(first_arg, temp.as_deref());
            self.write(" : ");
        }
        self.emit_dynamic_import_amd_branch(first_arg, temp.as_deref());

        if needs_parens {
            self.write(")");
        }
    }

    /// Whether the parent `await` keyword is lowered to a `yield` keyword in
    /// the current emit context, mirroring `emit_await_expression`.
    ///
    /// A `yield` operand binds looser than `?:`, so `yield a ? b : c` already
    /// parses as `yield (a ? b : c)`; a native `await` binds tighter than `?:`,
    /// so `await a ? b : c` parses as `(await a) ? b : c` and needs parens.
    /// This decides whether an `await`-parented UMD conditional needs wrapping.
    const fn await_parent_emits_as_yield(&self) -> bool {
        self.ctx.emit_await_as_yield
            || self.ctx.emit_await_as_yield_await
            || (self.ctx.needs_async_lowering && self.function_scope_depth > 0)
    }

    /// Whether a UMD dynamic-`import()` substitution must be parenthesized for
    /// the parent expression it sits in, matching `tsc`'s parenthesizer.
    ///
    /// Statement-level parents (expression statement, `return`, `throw`, `for`
    /// headers) accept a full expression — including a comma sequence — so
    /// neither form needs parentheses. Parents that bind tighter than `?:`
    /// (operand of native `await`/unary/binary, object of a member access,
    /// callee of a call/new, condition of another conditional) always need
    /// parentheses. A `yield` operand — whether a source-level `yield` or an
    /// `await` downleveled to `yield` for async-to-generator lowering — binds
    /// looser than `?:`, so a bare conditional there needs no parentheses.
    /// Remaining assignment-level parents (variable initializer, call argument,
    /// array element, property value, arrow body, …) accept a bare conditional
    /// but require parentheses around a comma sequence, so `is_sequence` decides
    /// those.
    fn umd_dynamic_import_needs_parens(&self, call_idx: NodeIndex, is_sequence: bool) -> bool {
        let Some(parent_idx) = self.arena.parent_of(call_idx) else {
            return false;
        };
        if parent_idx.is_none() {
            return false;
        }
        let Some(parent) = self.arena.get(parent_idx) else {
            return false;
        };
        let k = parent.kind;
        if k == syntax_kind_ext::EXPRESSION_STATEMENT
            || k == syntax_kind_ext::RETURN_STATEMENT
            || k == syntax_kind_ext::THROW_STATEMENT
            || k == syntax_kind_ext::FOR_STATEMENT
        {
            return false;
        }
        let tighter_than_conditional = match k {
            // A source-level `yield` operand binds looser than `?:`; a bare
            // conditional needs no parens (only a comma sequence does).
            k if k == syntax_kind_ext::YIELD_EXPRESSION => false,
            // `await` binds tighter than `?:` and needs parens, but when async
            // lowering rewrites it to `yield` the operand binds looser, so the
            // bare conditional needs no parens.
            k if k == syntax_kind_ext::AWAIT_EXPRESSION => !self.await_parent_emits_as_yield(),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::TYPE_OF_EXPRESSION
                || k == syntax_kind_ext::VOID_EXPRESSION
                || k == syntax_kind_ext::DELETE_EXPRESSION
                || k == syntax_kind_ext::BINARY_EXPRESSION =>
            {
                true
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                self.arena
                    .get_access_expr(parent)
                    .is_some_and(|access| access.expression == call_idx)
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION || k == syntax_kind_ext::NEW_EXPRESSION => {
                self.arena
                    .get_call_expr(parent)
                    .is_some_and(|call| call.expression == call_idx)
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => self
                .arena
                .get_conditional_expr(parent)
                .is_some_and(|cond| cond.condition == call_idx),
            _ => false,
        };
        tighter_than_conditional || is_sequence
    }

    /// Emit the CommonJS branch of a downlevel dynamic import as a Promise.
    ///
    /// Cases by specifier shape, matching tsc:
    ///
    /// - Captured temp / string literal / no-arg: lazy `Promise.resolve().then(() => require(x))`.
    /// - Non-string-like expressions: `Promise.resolve(coerced).then(s => require(s))`
    ///   where the coerced form wraps the expression in a template-string coercion.
    ///   If the expression is itself a template expression, that template remains
    ///   nested inside the coercion wrapper.
    fn emit_dynamic_import_commonjs_promise(
        &mut self,
        first_arg: Option<NodeIndex>,
        temp: Option<&str>,
    ) {
        if temp.is_some() || first_arg.is_none_or(|arg| self.dynamic_import_arg_is_string_like(arg))
        {
            self.emit_dynamic_import_commonjs_branch(first_arg, temp);
            return;
        }
        let first = first_arg.expect("non-string-like dynamic import has an argument");
        self.write("Promise.resolve(`${");
        if self.ctx.options.rewrite_relative_import_extensions {
            self.emit_rewrite_helper_call(first);
        } else {
            self.emit_dynamic_import_template_specifier(first);
        }
        self.write("}`).then(s => ");
        self.write_helper("__importStar");
        self.write("(require(s)))");
    }

    fn first_dynamic_import_argument(
        &self,
        args: Option<&tsz_parser::parser::NodeList>,
    ) -> Option<NodeIndex> {
        args.and_then(|args| {
            args.nodes
                .iter()
                .copied()
                .find(|&idx| self.call_argument_should_emit(idx))
        })
    }

    fn dynamic_import_arg_is_string_like(&self, arg: NodeIndex) -> bool {
        self.arena.get(arg).is_some_and(|node| {
            node.kind == SyntaxKind::StringLiteral as u16
                || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || node.end <= node.pos
        })
    }

    fn dynamic_import_arg_is_identifier(&self, arg: NodeIndex) -> bool {
        self.arena.get(arg).is_some_and(|node| node.is_identifier())
    }

    fn emit_dynamic_import_commonjs_branch(
        &mut self,
        first_arg: Option<NodeIndex>,
        temp: Option<&str>,
    ) {
        if self.ctx.target_es5 {
            self.write("Promise.resolve().then(function () { return ");
            self.write_helper("__importStar");
            self.write("(require(");
            self.emit_dynamic_import_require_specifier(first_arg, temp);
            self.write(")); })");
        } else {
            self.write("Promise.resolve().then(() => ");
            self.write_helper("__importStar");
            self.write("(require(");
            self.emit_dynamic_import_require_specifier(first_arg, temp);
            self.write(")))");
        }
    }

    fn emit_dynamic_import_amd_branch(&mut self, first_arg: Option<NodeIndex>, temp: Option<&str>) {
        let id = self.next_dynamic_import_promise_id;
        self.next_dynamic_import_promise_id += 1;
        let resolve = format!("resolve_{id}");
        let reject = format!("reject_{id}");

        if self.ctx.target_es5 {
            self.write("new Promise(function (");
        } else {
            self.write("new Promise((");
        }
        self.write(&resolve);
        self.write(", ");
        self.write(&reject);
        if self.ctx.target_es5 {
            self.write(") { require([");
        } else {
            self.write(") => { require([");
        }
        self.emit_dynamic_import_require_specifier(first_arg, temp);
        self.write("], ");
        self.write(&resolve);
        self.write(", ");
        self.write(&reject);
        self.write("); }).then(");
        self.write_helper("__importStar");
        self.write(")");
    }

    fn emit_dynamic_import_require_specifier(
        &mut self,
        first_arg: Option<NodeIndex>,
        temp: Option<&str>,
    ) {
        if let Some(temp) = temp {
            self.write(temp);
        } else if let Some(first) = first_arg {
            self.emit_maybe_rewritten_module_specifier_arg(first);
        }
    }

    fn emit_erased_object_literal_access_call(
        &mut self,
        call_node: &Node,
        callee: NodeIndex,
        args: &Option<tsz_parser::parser::NodeList>,
    ) -> bool {
        let Some((object_expr, dot_base, property_name)) =
            self.erased_object_literal_access_parts(callee)
        else {
            return false;
        };

        self.write("(");
        self.emit(object_expr);
        self.write_dot_token(dot_base);
        self.emit_property_name_without_import_substitution(property_name);
        self.emit_call_arguments(call_node, args.as_ref());
        self.write(")");
        true
    }

    pub(in crate::emitter) fn is_erased_object_literal_access_call_expression(
        &self,
        call_idx: NodeIndex,
    ) -> bool {
        let Some(call_node) = self.arena.get(call_idx) else {
            return false;
        };
        if call_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }
        let Some(call) = self.arena.get_call_expr(call_node) else {
            return false;
        };
        self.erased_object_literal_access_parts(call.expression)
            .is_some()
    }

    fn erased_object_literal_access_parts(
        &self,
        callee: NodeIndex,
    ) -> Option<(NodeIndex, NodeIndex, NodeIndex)> {
        let callee_node = self.arena.get(callee)?;
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(callee_node)?;
        let base_node = self.arena.get(access.expression)?;
        if base_node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            return None;
        }
        let paren = self.arena.get_parenthesized(base_node)?;
        let inner = self.arena.get(paren.expression)?;
        let inner_is_erasable = inner.kind == syntax_kind_ext::TYPE_ASSERTION
            || inner.kind == syntax_kind_ext::AS_EXPRESSION
            || inner.kind == syntax_kind_ext::SATISFIES_EXPRESSION
            || inner.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS;
        if !inner_is_erasable || !self.type_assertion_wraps_object_literal(paren.expression) {
            return None;
        }

        Some((paren.expression, access.expression, access.name_or_argument))
    }

    fn emit_call_arguments(&mut self, node: &Node, args: Option<&tsz_parser::parser::NodeList>) {
        self.write("(");
        // The call's own parens provide grouping, so clear the "needs parens"
        // flags to avoid double-parenthesization when an argument contains a
        // downlevel optional chain or nullish coalescing expression.
        let prev_optional = self.ctx.flags.optional_chain_needs_parens;
        let prev_nullish = self.ctx.flags.nullish_coalescing_needs_parens;
        self.ctx.flags.optional_chain_needs_parens = false;
        self.ctx.flags.nullish_coalescing_needs_parens = false;
        if let Some(args) = args {
            let valid_args: Vec<_> = args
                .nodes
                .iter()
                .copied()
                .filter(|&idx| self.call_argument_should_emit(idx))
                .collect();
            if let Some(first_arg) = valid_args.first()
                && let Some(arg_node) = self.arena.get(*first_arg)
            {
                let open_paren_pos = self
                    .find_call_open_paren_position(node, Some(args))
                    .unwrap_or(node.pos);
                self.emit_call_leading_argument_comments(open_paren_pos, arg_node.pos);
            }
            self.emit_comma_separated(&valid_args);
            if let Some(last_arg) = valid_args.last()
                && let Some(close_paren_pos) =
                    self.find_call_closing_paren_position(node, Some(args))
            {
                let last_arg_end = self.call_argument_comment_boundary(*last_arg);
                self.emit_call_trailing_argument_comments(last_arg_end, close_paren_pos);
            } else if valid_args.is_empty() {
                self.emit_empty_call_argument_comments(node, Some(args));
            }
        }
        self.ctx.flags.optional_chain_needs_parens = prev_optional;
        self.ctx.flags.nullish_coalescing_needs_parens = prev_nullish;
        self.write(")");
    }

    fn call_argument_should_emit(&self, idx: NodeIndex) -> bool {
        if idx.is_none() {
            return false;
        }
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.end <= node.pos {
            return false;
        }
        if node.kind == SyntaxKind::Unknown as u16 {
            return false;
        }
        self.arena
            .get_identifier(node)
            .is_none_or(|ident| !ident.escaped_text.is_empty())
    }

    fn emit_optional_call_expression(
        &mut self,
        node: &Node,
        callee: NodeIndex,
        args: &Option<tsz_parser::parser::NodeList>,
    ) {
        // Check if the callee is a type-asserted method call like `(foo.m as T)?.()`.
        // After unwrapping paren/type-assertion, if the underlying expression is a
        // property/element access, we need `.call(receiver)` for correct `this` binding.
        let unwrapped = self.unwrap_paren_and_type_assertion(callee);
        if let Some(unwrapped_node) = self.arena.get(unwrapped)
            && (unwrapped_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || unwrapped_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
        {
            // Route through method call path with `.call()` for `this` preservation
            self.emit_optional_method_call_expression(
                unwrapped_node,
                node,
                args,
                true, // has_optional_call_token — the `?.()` is on the call
            );
            return;
        }

        let needs_parens = self.ctx.flags.optional_chain_needs_parens;
        if needs_parens {
            self.write("(");
            self.ctx.flags.optional_chain_needs_parens = false;
        }
        if self.is_simple_nullish_expression(callee) {
            self.emit(callee);
            self.write(" === null || ");
            self.emit(callee);
            self.write(" === void 0 ? void 0 : ");
            self.emit(callee);
            self.emit_call_arguments(node, args.as_ref());
        } else {
            let temp = self.make_unique_name_hoisted();
            self.write("(");
            self.write(&temp);
            self.write(" = ");
            self.emit(callee);
            self.write(")");
            self.write(" === null || ");
            self.write(&temp);
            self.write(" === void 0 ? void 0 : ");
            self.write(&temp);
            self.emit_call_arguments(node, args.as_ref());
        }
        if needs_parens {
            self.write(")");
        }
    }

    fn emit_optional_method_call_expression(
        &mut self,
        access_node: &Node,
        call_node: &Node,
        args: &Option<tsz_parser::parser::NodeList>,
        has_optional_call_token: bool,
    ) {
        let Some(access) = self.arena.get_access_expr(access_node) else {
            return;
        };

        let needs_parens = self.ctx.flags.optional_chain_needs_parens;
        if needs_parens {
            self.write("(");
            self.ctx.flags.optional_chain_needs_parens = false;
        }

        if !has_optional_call_token {
            let is_simple = self.is_simple_nullish_expression(access.expression);
            if is_simple {
                // Simple identifier — no temp needed.
                // e.g., `o2?.b()` → `o2 === null || o2 === void 0 ? void 0 : o2.b()`
                if access.question_dot_token {
                    self.emit(access.expression);
                    self.write(" === null || ");
                    self.emit(access.expression);
                    self.write(" === void 0 ? void 0 : ");
                }
                self.emit(access.expression);
            } else {
                let this_temp = self.make_unique_name_hoisted();
                self.write("(");
                self.write(&this_temp);
                self.write(" = ");
                self.emit(access.expression);
                self.write(")");
                if access.question_dot_token {
                    self.write(" === null || ");
                    self.write(&this_temp);
                    self.write(" === void 0 ? void 0 : ");
                }
                if access.question_dot_token {
                    self.write(&this_temp);
                }
            }
            if access_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                self.write(".");
                self.emit(access.name_or_argument);
            } else {
                self.write("[");
                self.emit(access.name_or_argument);
                self.write("]");
            }
            self.emit_call_arguments(call_node, args.as_ref());
            if needs_parens {
                self.write(")");
            }
            return;
        }

        // Check if the base expression is `super` — it cannot be captured in a temp variable.
        // For `super.method?.()`, emit: `(_a = super.method) === null || _a === void 0 ? void 0 : _a.call(this)`
        let is_super = self
            .arena
            .get(access.expression)
            .is_some_and(|n| n.kind == SyntaxKind::SuperKeyword as u16);

        if is_super {
            let func_temp = self.make_unique_name_hoisted();
            self.write("(");
            self.write(&func_temp);
            self.write(" = ");
            // Capture `super.method` or `super["method"]` as a unit
            if access_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                self.write("super.");
                self.emit(access.name_or_argument);
            } else {
                self.write("super[");
                self.emit(access.name_or_argument);
                self.write("]");
            }
            self.write(") === null || ");
            self.write(&func_temp);
            self.write(" === void 0 ? void 0 : ");
            self.write(&func_temp);
            self.write(".call(");
            self.emit_es5_super_call_receiver(access.expression);
            self.emit_optional_call_tail_arguments(args.as_ref());
            if needs_parens {
                self.write(")");
            }
            return;
        }

        let is_simple = self.is_simple_nullish_expression(access.expression);

        if is_simple {
            // Simple identifier — only need one temp for the method capture.
            // e.g., `o3.b?.()` → `(_a = o3.b) === null || _a === void 0 ? void 0 : _a.call(o3)`
            let func_temp = self.make_unique_name_hoisted();
            self.write("(");
            self.write(&func_temp);
            self.write(" = ");
            if access.question_dot_token {
                self.emit(access.expression);
                self.write(" === null || ");
                self.emit(access.expression);
                self.write(" === void 0 ? void 0 : ");
                self.emit(access.expression);
            } else {
                self.emit(access.expression);
            }
            if access_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                self.write(".");
                self.emit(access.name_or_argument);
            } else {
                self.write("[");
                self.emit(access.name_or_argument);
                self.write("]");
            }
            self.write(") === null || ");
            self.write(&func_temp);
            self.write(" === void 0 ? void 0 : ");
            self.write(&func_temp);
            self.write(".call(");
            self.emit(access.expression);
            self.emit_optional_call_tail_arguments(args.as_ref());
        } else {
            let this_temp = self.make_unique_name_hoisted();
            let func_temp = self.make_unique_name_hoisted();
            self.write("(");
            self.write(&func_temp);
            self.write(" = ");
            self.write("(");
            self.write(&this_temp);
            self.write(" = ");
            self.emit(access.expression);
            self.write(")");
            if access.question_dot_token {
                self.write(" === null || ");
                self.write(&this_temp);
                self.write(" === void 0 ? void 0 : ");
            }
            if access_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                if access.question_dot_token {
                    self.write(&this_temp);
                }
                self.write(".");
                self.emit(access.name_or_argument);
            } else {
                if access.question_dot_token {
                    self.write(&this_temp);
                }
                self.write("[");
                self.emit(access.name_or_argument);
                self.write("]");
            }
            self.write(") === null || ");
            self.write(&func_temp);
            self.write(" === void 0 ? void 0 : ");
            self.write(&func_temp);
            self.write(".call(");
            self.write(&this_temp);
            self.emit_optional_call_tail_arguments(args.as_ref());
        }
        if needs_parens {
            self.write(")");
        }
    }

    fn emit_optional_call_tail_arguments(&mut self, args: Option<&tsz_parser::parser::NodeList>) {
        if let Some(args) = args
            && !args.nodes.is_empty()
        {
            self.write(", ");
            self.emit_comma_separated(&args.nodes);
        }
        self.write(")");
    }

    fn expression_is_optional_chain_continuation(&self, expression: NodeIndex) -> bool {
        let expression = self.unwrap_paren_and_type_assertion(expression);
        let Some(node) = self.arena.get(expression) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                self.arena.get_access_expr(node).is_some_and(|access| {
                    node.is_optional_chain()
                        || access.question_dot_token
                        || self.expression_is_optional_chain_continuation(access.expression)
                })
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                node.is_optional_chain()
                    || self.arena.get_call_expr(node).is_some_and(|call| {
                        self.expression_is_optional_chain_continuation(call.expression)
                    })
            }
            _ => false,
        }
    }

    const fn is_optional_chain(&self, node: &Node) -> bool {
        node.is_optional_chain()
    }
}
