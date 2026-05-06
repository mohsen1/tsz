use super::super::DeclarationEmitter;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn const_literal_initializer_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        match expr_node.kind {
            k if k == syntax_kind_ext::AWAIT_EXPRESSION => self
                .arena
                .get_unary_expr_ex(expr_node)
                .and_then(|await_expr| self.const_literal_initializer_text(await_expr.expression)),
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => self
                .arena
                .get_parenthesized(expr_node)
                .and_then(|paren| self.const_literal_initializer_text(paren.expression)),
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::BigIntLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16 =>
            {
                self.js_literal_type_text(expr_idx)
            }
            // Template literal without substitutions: `hello` -> "hello"
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(expr_node) {
                    let escaped = super::escape_string_for_double_quote(&lit.text);
                    Some(format!("\"{escaped}\""))
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                && self.is_negative_literal(expr_node) =>
            {
                let raw = self.get_source_slice_no_semi(expr_node.pos, expr_node.end)?;
                if raw.contains('_') {
                    Some(raw.replace('_', ""))
                } else {
                    Some(raw)
                }
            }
            _ => self
                .enum_member_access_initializer_text(expr_idx)
                .or_else(|| self.simple_enum_access_member_text(expr_idx)),
        }
    }

    /// Like `const_literal_initializer_text` but also unwraps `as` and
    /// `satisfies` expressions to find the underlying literal.
    pub(in crate::declaration_emitter) fn const_literal_initializer_text_deep(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let mut guard = tsz_solver::recursion::RecursionGuard::with_profile(
            tsz_solver::recursion::RecursionProfile::ShallowTraversal,
        );
        self.const_literal_initializer_text_deep_guarded(expr_idx, &mut guard)
    }

    fn const_literal_initializer_text_deep_guarded(
        &self,
        expr_idx: NodeIndex,
        guard: &mut tsz_solver::recursion::RecursionGuard<NodeIndex>,
    ) -> Option<String> {
        use tsz_solver::recursion::RecursionResult;
        if !matches!(guard.enter(expr_idx), RecursionResult::Entered) {
            return None;
        }
        let result = self.const_literal_initializer_text_deep_inner(expr_idx, guard);
        guard.leave(expr_idx);
        result
    }

    fn const_literal_initializer_text_deep_inner(
        &self,
        expr_idx: NodeIndex,
        guard: &mut tsz_solver::recursion::RecursionGuard<NodeIndex>,
    ) -> Option<String> {
        // Try the normal path first
        if let Some(text) = self.const_literal_initializer_text(expr_idx) {
            return Some(text);
        }
        if let Some(text) = self.const_literal_identity_call_text(expr_idx, guard) {
            return Some(text);
        }
        // Unwrap as/satisfies expressions
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind == syntax_kind_ext::AS_EXPRESSION
            || expr_node.kind == syntax_kind_ext::SATISFIES_EXPRESSION
        {
            let assertion = self.arena.get_type_assertion(expr_node)?;
            return self.const_literal_initializer_text_deep_guarded(assertion.expression, guard);
        }

        // Chase identifiers to their const declaration initializer, matching
        // tsc behavior when a const variable references another const whose
        // literal value is known (e.g. `const a = "abc"; const b = a` ->
        // `declare const b = "abc"`).
        if expr_node.kind == SyntaxKind::Identifier as u16 {
            if self.binder.is_some() {
                let sym_id = self.value_reference_symbol(expr_idx)?;
                let initializer = self.const_variable_initializer_for_symbol(sym_id)?;
                return self.const_literal_initializer_text_deep_guarded(initializer, guard);
            }

            if let Some(initializer) = self.top_level_const_initializer_for_identifier(expr_idx) {
                return self.const_literal_initializer_text_deep_guarded(initializer, guard);
            }
        }

        None
    }

    fn const_variable_initializer_for_symbol(&self, sym_id: SymbolId) -> Option<NodeIndex> {
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        for decl_idx in symbol.all_declarations() {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                continue;
            };
            if self.arena.is_const_variable_declaration(decl_idx) && decl.initializer.is_some() {
                return Some(decl.initializer);
            }
        }
        None
    }

    fn top_level_const_initializer_for_identifier(&self, expr_idx: NodeIndex) -> Option<NodeIndex> {
        let name = self.get_identifier_text(expr_idx)?;
        let source_file_idx = self.current_source_file_idx?;
        let source_file_node = self.arena.get(source_file_idx)?;
        let source_file = self.arena.get_source_file(source_file_node)?;

        for &stmt_idx in &source_file.statements.nodes {
            let stmt_node = self.arena.get(stmt_idx)?;
            if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                continue;
            }
            let variable = self.arena.get_variable(stmt_node)?;
            for &decl_list_idx in &variable.declarations.nodes {
                let decl_list_node = self.arena.get(decl_list_idx)?;
                let decl_list = self.arena.get_variable(decl_list_node)?;
                for &decl_idx in &decl_list.declarations.nodes {
                    let decl_node = self.arena.get(decl_idx)?;
                    let decl = self.arena.get_variable_declaration(decl_node)?;
                    if self.arena.is_const_variable_declaration(decl_idx)
                        && self.get_identifier_text(decl.name).as_deref() == Some(&name)
                        && decl.initializer.is_some()
                    {
                        return Some(decl.initializer);
                    }
                }
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn const_literal_identity_call_text(
        &self,
        expr_idx: NodeIndex,
        guard: &mut tsz_solver::recursion::RecursionGuard<NodeIndex>,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.arena.get_call_expr(expr_node)?;
        let args = call.arguments.as_ref()?;
        if args.nodes.len() != 1 {
            return None;
        }

        let func = self.identity_returning_function(call.expression)?;
        let callee_body = func.body;
        let returned_identifier = self.function_body_unique_return_identifier(callee_body)?;
        let return_name = self.get_identifier_text(returned_identifier)?;
        let first_param_name = func
            .parameters
            .nodes
            .first()
            .copied()
            .and_then(|param_idx| self.arena.get(param_idx))
            .and_then(|param_node| self.arena.get_parameter(param_node))
            .and_then(|param| self.get_identifier_text(param.name))?;

        if first_param_name != return_name {
            return None;
        }

        let mut text = self.const_literal_initializer_text_deep_guarded(args.nodes[0], guard)?;
        if text.starts_with('-') {
            while text.ends_with(')') {
                text.pop();
            }
        }
        Some(text)
    }

    pub(in crate::declaration_emitter) fn identity_returning_function(
        &self,
        callee_idx: NodeIndex,
    ) -> Option<&tsz_parser::parser::node::FunctionData> {
        if self.binder.is_some() {
            let sym_id = self.value_reference_symbol(callee_idx)?;
            return self.identity_returning_function_for_symbol(sym_id);
        }

        let callee_name = self.get_identifier_text(callee_idx)?;
        let source_file_idx = self.current_source_file_idx?;
        let source_file_node = self.arena.get(source_file_idx)?;
        let source_file = self.arena.get_source_file(source_file_node)?;

        source_file
            .statements
            .nodes
            .iter()
            .copied()
            .find_map(|decl_idx| {
                let decl_node = self.arena.get(decl_idx)?;
                let func = self.arena.get_function(decl_node)?;
                let same_name = self
                    .get_identifier_text(func.name)
                    .is_some_and(|name| name == callee_name);
                (same_name && func.body.is_some() && func.parameters.nodes.len() == 1)
                    .then_some(func)
            })
    }

    fn identity_returning_function_for_symbol(
        &self,
        sym_id: SymbolId,
    ) -> Option<&tsz_parser::parser::node::FunctionData> {
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        for decl_idx in symbol.all_declarations() {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(func) = self.arena.get_function(decl_node) else {
                continue;
            };
            if func.body.is_some() && func.parameters.nodes.len() == 1 {
                return Some(func);
            }
        }
        None
    }

    /// True when `expr_idx` is a bare `globalThis` identifier. Used by variable
    /// declaration emit to render `const x = globalThis` as `: typeof globalThis`
    /// - without this check, the solver's fallback gives the emit path only
    ///   `any`, dropping the `globalThis` information tsc preserves in .d.ts.
    pub(in crate::declaration_emitter) fn initializer_is_global_this_identifier(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.arena.get(expr_idx) else {
            return false;
        };
        let Some(ident) = self.arena.get_identifier(node) else {
            return false;
        };
        ident.escaped_text == "globalThis"
    }

    pub(in crate::declaration_emitter) fn is_import_meta_url_expression(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.arena.get_access_expr(expr_node) else {
            return false;
        };
        if self.get_identifier_text(access.name_or_argument).as_deref() != Some("url") {
            return false;
        }

        let Some(base_node) = self.arena.get(access.expression) else {
            return false;
        };
        if base_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(base_access) = self.arena.get_access_expr(base_node) else {
            return false;
        };
        if self
            .get_identifier_text(base_access.name_or_argument)
            .as_deref()
            != Some("meta")
        {
            return false;
        }

        self.arena
            .get(base_access.expression)
            .is_some_and(|node| node.kind == SyntaxKind::ImportKeyword as u16)
    }

    /// Format a f64 value as JavaScript would display it.
    ///
    /// Matches JS `Number.prototype.toString()` behavior:
    /// - Infinity/NaN -> "Infinity"/"NaN"
    /// - Uses scientific notation for numbers with >= 21 integer digits
    /// - Uses scientific notation for very small numbers
    pub(crate) fn format_js_number(n: f64) -> String {
        if n.is_infinite() {
            if n.is_sign_positive() {
                "Infinity".to_string()
            } else {
                "-Infinity".to_string()
            }
        } else if n.is_nan() {
            "NaN".to_string()
        } else {
            let s = n.to_string();
            // Rust's default formatter doesn't use scientific notation for large
            // integers. JS switches to scientific notation when the integer part
            // has 21+ digits. Detect and convert.
            let abs_s = s.strip_prefix('-').unwrap_or(&s);
            let needs_scientific = if let Some(dot_pos) = abs_s.find('.') {
                dot_pos >= 21
            } else {
                abs_s.len() >= 21
            };
            if needs_scientific {
                Self::format_js_scientific(n)
            } else {
                s
            }
        }
    }

    /// Format a number in JavaScript-style scientific notation (e.g., `1.2345678912345678e+53`).
    pub(in crate::declaration_emitter) fn format_js_scientific(n: f64) -> String {
        let neg = n < 0.0;
        let abs_n = n.abs();
        // Use Rust's {:e} format which gives e.g. "1.2345678912345678e53"
        let s = format!("{abs_n:e}");
        // JS uses e+N for positive exponents, e-N for negative
        let result = if let Some(pos) = s.find('e') {
            let (mantissa, exp_part) = s.split_at(pos);
            let exp_str = &exp_part[1..]; // skip 'e'
            if exp_str.starts_with('-') {
                format!("{mantissa}e{exp_str}")
            } else {
                format!("{mantissa}e+{exp_str}")
            }
        } else {
            s
        };
        if neg { format!("-{result}") } else { result }
    }

    /// Normalize a numeric literal string through f64, matching tsc's JS round-trip behavior.
    /// E.g., `123456789123456789123456789123456789123456789123456789` -> `1.2345678912345678e+53`
    pub(crate) fn normalize_numeric_literal(text: &str) -> String {
        if let Ok(val) = text.parse::<f64>() {
            let normalized = Self::format_js_number(val);
            if normalized != text {
                return normalized;
            }
        }
        text.to_string()
    }
}
