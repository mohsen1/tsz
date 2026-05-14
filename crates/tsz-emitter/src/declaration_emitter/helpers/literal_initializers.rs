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
            k if k == SyntaxKind::NumericLiteral as u16 => self
                .arena
                .get_literal(expr_node)
                .map(|lit| Self::const_numeric_literal_initializer_text(&lit.text, lit.value)),
            k if k == SyntaxKind::StringLiteral as u16
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
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.arena.get_unary_expr(expr_node)?;
                let operand_node = self.arena.get(unary.operand)?;
                match (unary.operator, operand_node.kind) {
                    (op, k)
                        if op == SyntaxKind::PlusToken as u16
                            && k == SyntaxKind::NumericLiteral as u16 =>
                    {
                        self.const_literal_initializer_text(unary.operand)
                    }
                    (op, k)
                        if op == SyntaxKind::MinusToken as u16
                            && k == SyntaxKind::NumericLiteral as u16 =>
                    {
                        let lit = self.arena.get_literal(operand_node)?;
                        let text =
                            Self::const_numeric_literal_initializer_text(&lit.text, lit.value);
                        Some(format!("-{text}"))
                    }
                    (op, k)
                        if op == SyntaxKind::MinusToken as u16
                            && k == SyntaxKind::BigIntLiteral as u16 =>
                    {
                        let raw = self.get_source_slice_no_semi(expr_node.pos, expr_node.end)?;
                        if raw.contains('_') {
                            Some(raw.replace('_', ""))
                        } else {
                            Some(raw)
                        }
                    }
                    _ => None,
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

        if self.inline_identity_arrow_callee(call.expression) {
            return self.const_literal_initializer_text_deep_guarded(args.nodes[0], guard);
        }

        let func = self.identity_returning_function(call.expression)?;
        let returned_identifier = self.function_identity_return_identifier(func)?;
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

    fn inline_identity_arrow_callee(&self, callee_idx: NodeIndex) -> bool {
        let Some(raw) = self
            .arena
            .get(callee_idx)
            .and_then(|node| self.get_source_slice_no_semi(node.pos, node.end))
        else {
            return false;
        };
        let mut text = raw.trim();
        while text.starts_with('(') && text.ends_with(')') {
            text = text[1..text.len() - 1].trim();
        }
        let Some((left, right)) = text.split_once("=>") else {
            return false;
        };
        let mut left = left.trim();
        if left.starts_with('<')
            && let Some(end) = left.find('>')
        {
            left = left[end + 1..].trim();
        }
        if left.starts_with('(') && left.ends_with(')') {
            left = left[1..left.len() - 1].trim();
        }
        let param_name: String = left
            .chars()
            .take_while(|ch| *ch == '_' || *ch == '$' || ch.is_ascii_alphanumeric())
            .collect();
        if param_name.is_empty() {
            return false;
        }
        right.trim() == param_name
    }

    fn function_identity_return_identifier(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
    ) -> Option<NodeIndex> {
        let body_node = self.arena.get(func.body)?;
        if body_node.kind == SyntaxKind::Identifier as u16 {
            return Some(func.body);
        }
        self.function_body_unique_return_identifier(func.body)
    }

    pub(in crate::declaration_emitter) fn identity_returning_function(
        &self,
        callee_idx: NodeIndex,
    ) -> Option<&tsz_parser::parser::node::FunctionData> {
        if let Some(func) = self.identity_returning_function_expression(callee_idx) {
            return Some(func);
        }

        if self.binder.is_some() {
            return self.identity_returning_function_from_symbol(callee_idx);
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
            .find_map(|decl_idx| self.named_identity_function_decl(decl_idx, &callee_name))
    }

    fn named_identity_function_decl(
        &self,
        decl_idx: NodeIndex,
        callee_name: &str,
    ) -> Option<&tsz_parser::parser::node::FunctionData> {
        let decl_node = self.arena.get(decl_idx)?;
        if let Some(export_decl) = self.arena.get_export_decl(decl_node) {
            return self.named_identity_function_decl(export_decl.export_clause, callee_name);
        }
        if let Some(func) = self.arena.get_function(decl_node) {
            let same_name = self
                .get_identifier_text(func.name)
                .is_some_and(|name| name == callee_name);
            return (same_name && func.body.is_some() && func.parameters.nodes.len() == 1)
                .then_some(func);
        }
        if let Some(variable) = self.arena.get_variable(decl_node) {
            for &decl_list_idx in &variable.declarations.nodes {
                let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                    continue;
                };
                let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                    continue;
                };
                for &var_decl_idx in &decl_list.declarations.nodes {
                    let Some(var_decl_node) = self.arena.get(var_decl_idx) else {
                        continue;
                    };
                    let Some(var_decl) = self.arena.get_variable_declaration(var_decl_node) else {
                        continue;
                    };
                    if self.get_identifier_text(var_decl.name).as_deref() == Some(callee_name)
                        && let Some(func) =
                            self.identity_returning_function_expression(var_decl.initializer)
                    {
                        return Some(func);
                    }
                }
            }
        }
        None
    }

    fn identity_returning_function_expression(
        &self,
        callee_idx: NodeIndex,
    ) -> Option<&tsz_parser::parser::node::FunctionData> {
        let callee_idx = self.skip_parenthesized_expression(callee_idx)?;
        let callee_node = self.arena.get(callee_idx)?;
        if callee_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && callee_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return None;
        }
        let func = self.arena.get_function(callee_node)?;
        (func.body.is_some() && func.parameters.nodes.len() == 1).then_some(func)
    }

    fn identity_returning_function_from_symbol(
        &self,
        callee_idx: NodeIndex,
    ) -> Option<&tsz_parser::parser::node::FunctionData> {
        let sym_id = self.value_reference_symbol(callee_idx)?;
        let binder = self.binder?;
        let sym_id = self
            .resolve_portability_import_alias(sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));
        self.identity_returning_function_for_symbol(sym_id)
    }

    fn identity_returning_function_for_symbol(
        &self,
        sym_id: SymbolId,
    ) -> Option<&tsz_parser::parser::node::FunctionData> {
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        for decl_idx in symbol.all_declarations() {
            if let Some(func) = self.callable_identity_function_decl(decl_idx) {
                return Some(func);
            }
        }
        None
    }

    fn callable_identity_function_decl(
        &self,
        decl_idx: NodeIndex,
    ) -> Option<&tsz_parser::parser::node::FunctionData> {
        let decl_node = self.arena.get(decl_idx)?;
        if let Some(export_decl) = self.arena.get_export_decl(decl_node) {
            return self.callable_identity_function_decl(export_decl.export_clause);
        }
        if let Some(func) = self.arena.get_function(decl_node)
            && func.body.is_some()
            && func.parameters.nodes.len() == 1
        {
            return Some(func);
        }

        let mut current = decl_idx;
        for _ in 0..8 {
            let node = self.arena.get(current)?;
            if let Some(func) = self.arena.get_function(node)
                && func.body.is_some()
                && func.parameters.nodes.len() == 1
            {
                return Some(func);
            }
            if let Some(var_decl) = self.arena.get_variable_declaration(node)
                && let Some(func) =
                    self.identity_returning_function_expression(var_decl.initializer)
            {
                return Some(func);
            }
            current = self.arena.parent_of(current)?;
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

    pub(crate) fn declaration_numeric_literal_text(text: &str, value: Option<f64>) -> String {
        if text.contains('_') {
            if let Some(value) = value {
                Self::format_js_number(value)
            } else {
                text.replace('_', "")
            }
        } else {
            text.to_string()
        }
    }

    fn const_numeric_literal_initializer_text(text: &str, value: Option<f64>) -> String {
        let lower = text.to_ascii_lowercase();
        if text.contains('_')
            || lower.starts_with("0x")
            || lower.starts_with("0o")
            || lower.starts_with("0b")
            || text.chars().filter(|c| c.is_ascii_digit()).count() >= 21
        {
            if let Some(value) = value {
                Self::format_js_number(value)
            } else {
                let digits = lower
                    .strip_prefix("0x")
                    .map(|digits| (digits, 16))
                    .or_else(|| lower.strip_prefix("0o").map(|digits| (digits, 8)))
                    .or_else(|| lower.strip_prefix("0b").map(|digits| (digits, 2)));
                if let Some((digits, radix)) = digits
                    && let Ok(value) = i64::from_str_radix(&digits.replace('_', ""), radix)
                {
                    return value.to_string();
                }
                text.replace('_', "")
            }
        } else {
            text.to_string()
        }
    }
}
