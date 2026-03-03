use super::super::Printer;
use crate::transforms::private_fields_es5::get_private_field_name;
use tsz_parser::parser::{NodeIndex, node::Node, node_flags, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_call_expression(&mut self, node: &Node) {
        let Some(call) = self.arena.get_call_expr(node) else {
            return;
        };

        if self.is_optional_chain(node) {
            if self.ctx.options.target.supports_es2020() {
                self.emit(call.expression);
                if self.has_optional_call_token(node, call.expression, call.arguments.as_ref()) {
                    self.write("?.");
                }
                self.emit_call_arguments(node, call.arguments.as_ref());
                return;
            }

            let has_optional_call_token =
                self.has_optional_call_token(node, call.expression, call.arguments.as_ref());
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

        // Private field call lowering:
        // `this.#fn(args)` → `__classPrivateFieldGet(this, _C_fn, "f").call(this, args)`
        if !self.private_field_weakmaps.is_empty() {
            if let Some(expr_node) = self.arena.get(call.expression)
                && expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && let Some(access) = self.arena.get_access_expr(expr_node)
                && let Some(name_node) = self.arena.get(access.name_or_argument)
                && name_node.kind == SyntaxKind::PrivateIdentifier as u16
            {
                if let Some(field_name) =
                    get_private_field_name(self.arena, access.name_or_argument)
                {
                    let clean_name = field_name.strip_prefix('#').unwrap_or(&field_name);
                    if let Some(weakmap_name) = self.private_field_weakmaps.get(clean_name).cloned()
                    {
                        self.write("__classPrivateFieldGet(");
                        self.emit(access.expression);
                        self.write(", ");
                        self.write(&weakmap_name);
                        self.write(", \"f\").call(");
                        self.emit(access.expression);
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
                self.write("_super.prototype.");
                self.emit(access.name_or_argument);
                self.write(".call(");
                if self.ctx.arrow_state.this_capture_depth > 0 {
                    self.write("_this");
                } else {
                    self.write("this");
                }
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
                self.write("_super.prototype[");
                self.emit(access.name_or_argument);
                self.write("].call(");
                if self.ctx.arrow_state.this_capture_depth > 0 {
                    self.write("_this");
                } else {
                    self.write("this");
                }
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

        if self.ctx.is_commonjs()
            && !self.suppress_commonjs_named_import_substitution
            && let Some(expr_node) = self.arena.get(call.expression)
            && let Some(ident) = self.arena.get_identifier(expr_node)
            && let Some(subst) = self
                .commonjs_named_import_substitutions
                .get(&ident.escaped_text)
        {
            let subst = subst.clone();
            self.write("(0, ");
            self.write(&subst);
            self.write(")");
            self.emit_call_arguments(node, call.arguments.as_ref());
            return;
        }

        // Signal access position so `(new a)()` keeps parens (vs `new a()`).
        let prev = self.paren_in_access_position;
        self.paren_in_access_position = true;
        self.emit(call.expression);
        self.paren_in_access_position = prev;
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
            // For the first argument, emit any comments between '(' and the argument
            // This handles: func(/*comment*/ arg)
            if let Some(first_arg) = args.nodes.first()
                && let Some(arg_node) = self.arena.get(*first_arg)
            {
                // Use node.end of the call expression to approximate '(' position
                // Actually, we need to find the '(' position more carefully
                let paren_pos = self.find_open_paren_position(node.pos, arg_node.pos);
                self.emit_unemitted_comments_between(paren_pos, arg_node.pos);
            }
            self.emit_comma_separated(&args.nodes);
        }
        self.ctx.flags.optional_chain_needs_parens = prev_optional;
        self.ctx.flags.nullish_coalescing_needs_parens = prev_nullish;
        // Map the closing `)` to its source position
        self.map_closing_paren(node);
        self.write(")");
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
            if let Some(first_arg) = args.nodes.first()
                && let Some(arg_node) = self.arena.get(*first_arg)
            {
                let paren_pos = self.find_open_paren_position(node.pos, arg_node.pos);
                self.emit_unemitted_comments_between(paren_pos, arg_node.pos);
            }
            self.emit_comma_separated(&args.nodes);
        }
        self.ctx.flags.optional_chain_needs_parens = prev_optional;
        self.ctx.flags.nullish_coalescing_needs_parens = prev_nullish;
        self.write(")");
    }

    fn emit_optional_call_expression(
        &mut self,
        node: &Node,
        callee: NodeIndex,
        args: &Option<tsz_parser::parser::NodeList>,
    ) {
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
            if self.ctx.arrow_state.this_capture_depth > 0 {
                self.write("_this");
            } else {
                self.write("this");
            }
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

    const fn is_optional_chain(&self, node: &Node) -> bool {
        (node.flags as u32) & node_flags::OPTIONAL_CHAIN != 0
    }

    fn has_optional_call_token(
        &self,
        call_node: &Node,
        callee: NodeIndex,
        args: Option<&tsz_parser::parser::NodeList>,
    ) -> bool {
        let Some(source) = self.source_text_for_map() else {
            let Some(callee_node) = self.arena.get(callee) else {
                return false;
            };
            if self.arena.get_access_expr(callee_node).is_none() {
                return true;
            }
            return false;
        };

        let Some(callee_node) = self.arena.get(callee) else {
            return false;
        };
        let Some(open_paren) = self.find_call_open_paren_position(call_node, args) else {
            return false;
        };

        let bytes = source.as_bytes();
        let mut i = std::cmp::min(open_paren as usize, source.len());
        let start = std::cmp::min(callee_node.pos as usize, source.len());

        while i > start {
            if i == 0 {
                break;
            }
            match bytes[i - 1] {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    i -= 1;
                }
                b'/' if i >= 2 && bytes[i - 2] == b'/' => {
                    while i > start && bytes[i - 1] != b'\n' {
                        i -= 1;
                    }
                    if i > start {
                        i -= 1;
                    }
                }
                b'/' if i >= 2 && bytes[i - 2] == b'*' => {
                    if i >= 2 {
                        i -= 2;
                    }
                    while i >= 2 && !(bytes[i - 2] == b'*' && bytes[i - 1] == b'/') {
                        i -= 1;
                    }
                    if i >= 2 {
                        i -= 2;
                    }
                }
                b'?' if i >= 2 && bytes[i - 2] == b'.' => {
                    return true;
                }
                b'.' if i >= 2 && bytes[i - 2] == b'?' && bytes[i - 1] == b'.' => {
                    return true;
                }
                _ => return false,
            }
        }

        false
    }

    fn find_call_open_paren_position(
        &self,
        call_node: &Node,
        args: Option<&tsz_parser::parser::NodeList>,
    ) -> Option<u32> {
        let text = self.source_text_for_map()?;
        let bytes = text.as_bytes();
        let start = std::cmp::min(call_node.pos as usize, bytes.len());
        let mut end = std::cmp::min(call_node.end as usize, bytes.len());
        if let Some(args) = args
            && let Some(first) = args.nodes.first()
            && let Some(first_node) = self.arena.get(*first)
        {
            end = std::cmp::min(first_node.pos as usize, end);
        }
        (start..end)
            .position(|i| bytes[i] == b'(')
            .map(|offset| (start + offset) as u32)
    }

    /// Find the position of the opening parenthesis in a call expression.
    /// Scans forward from `start_pos` looking for '(' before `arg_pos`.
    fn find_open_paren_position(&self, start_pos: u32, arg_pos: u32) -> u32 {
        let Some(text) = self.source_text else {
            return start_pos;
        };
        let bytes = text.as_bytes();
        let start = start_pos as usize;
        let end = std::cmp::min(arg_pos as usize, bytes.len());

        if let Some(offset) = (start..end).position(|i| bytes[i] == b'(') {
            return (start + offset) as u32;
        }
        start_pos
    }
}
