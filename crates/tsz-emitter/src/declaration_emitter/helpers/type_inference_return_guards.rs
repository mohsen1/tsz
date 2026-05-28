//! Function return-type guard helpers for declaration inference.

use super::super::DeclarationEmitter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn array_element_type_text(
        type_text: &str,
    ) -> Option<String> {
        let trimmed = type_text.trim();
        if let Some(element) = trimmed.strip_suffix("[]") {
            let element = element.trim();
            if !element.is_empty() {
                return Some(element.to_string());
            }
        }
        for prefix in ["Array<", "ReadonlyArray<"] {
            if let Some(inner) = trimmed
                .strip_prefix(prefix)
                .and_then(|text| text.strip_suffix('>'))
            {
                let inner = inner.trim();
                if !inner.is_empty() {
                    return Some(inner.to_string());
                }
            }
        }
        None
    }

    pub(in crate::declaration_emitter) fn element_access_array_element_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(expr_node)?;
        let key_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(access.name_or_argument);
        let key_node = self.arena.get(key_idx)?;
        if key_node.kind != SyntaxKind::NumericLiteral as u16 {
            return None;
        }

        let receiver_text = self
            .preferred_expression_type_text(access.expression)
            .or_else(|| self.reference_declared_type_annotation_text(access.expression))?;
        let element_text = Self::array_element_type_text(&receiver_text)?;
        Some(
            Self::strip_parenthesized_union_element_type_text(&element_text)
                .unwrap_or(element_text),
        )
    }

    pub(in crate::declaration_emitter) fn array_filter_typeof_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(expr_node)?;
        let callee_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(call.expression);
        let callee_node = self.arena.get(callee_idx)?;
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(callee_node)?;
        if self.get_identifier_text(access.name_or_argument).as_deref() != Some("filter") {
            return None;
        }

        let receiver_text = self.preferred_expression_type_text(access.expression)?;
        let element_text = Self::array_element_type_text(&receiver_text)?;
        let callback_idx = call.arguments.as_ref()?.nodes.first().copied()?;
        let primitive = self.typeof_filter_callback_primitive(callback_idx)?;
        if !Self::array_element_type_includes_typeof_primitive(&element_text, primitive) {
            return None;
        }
        Some(format!("{primitive}[]"))
    }

    fn array_element_type_includes_typeof_primitive(element_text: &str, primitive: &str) -> bool {
        let element_text = Self::strip_parenthesized_union_element_type_text(element_text)
            .unwrap_or_else(|| element_text.trim().to_string());
        Self::split_top_level_union_type_parts(&element_text)
            .iter()
            .any(|part| matches!(part.as_str(), "any" | "unknown") || part == primitive)
    }

    fn typeof_filter_callback_primitive(&self, callback_idx: NodeIndex) -> Option<&'static str> {
        let callback_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(callback_idx);
        let callback_node = self.arena.get(callback_idx)?;
        if callback_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && callback_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return None;
        }
        let callback = self.arena.get_function(callback_node)?;
        let param_idx = callback.parameters.nodes.first().copied()?;
        let param_node = self.arena.get(param_idx)?;
        let param = self.arena.get_parameter(param_node)?;
        let param_name = self.get_identifier_text(param.name)?;
        let condition_idx = self.callback_predicate_expression(callback.body)?;
        self.typeof_equality_primitive(condition_idx, &param_name)
    }

    fn callback_predicate_expression(&self, body_idx: NodeIndex) -> Option<NodeIndex> {
        let body_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(body_idx);
        let body_node = self.arena.get(body_idx)?;
        if body_node.kind != syntax_kind_ext::BLOCK {
            return Some(body_idx);
        }

        let block = self.arena.get_block(body_node)?;
        let [stmt_idx] = block.statements.nodes.as_slice() else {
            return None;
        };
        let stmt_node = self.arena.get(*stmt_idx)?;
        let ret = self.arena.get_return_statement(stmt_node)?;
        self.skip_parenthesized_expression(ret.expression)
    }

    fn typeof_equality_primitive(
        &self,
        condition_idx: NodeIndex,
        param_name: &str,
    ) -> Option<&'static str> {
        let condition_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(condition_idx);
        let condition_node = self.arena.get(condition_idx)?;
        if condition_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let binary = self.arena.get_binary_expr(condition_node)?;
        if binary.operator_token != SyntaxKind::EqualsEqualsEqualsToken as u16 {
            return None;
        }

        self.typeof_primitive_pair(binary.left, binary.right, param_name)
            .or_else(|| self.typeof_primitive_pair(binary.right, binary.left, param_name))
    }

    fn typeof_primitive_pair(
        &self,
        typeof_idx: NodeIndex,
        literal_idx: NodeIndex,
        param_name: &str,
    ) -> Option<&'static str> {
        let typeof_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(typeof_idx);
        let typeof_node = self.arena.get(typeof_idx)?;
        if typeof_node.kind != syntax_kind_ext::PREFIX_UNARY_EXPRESSION {
            return None;
        }
        let unary = self.arena.get_unary_expr(typeof_node)?;
        if unary.operator != SyntaxKind::TypeOfKeyword as u16 {
            return None;
        }
        if self.get_identifier_text(unary.operand).as_deref() != Some(param_name) {
            return None;
        }

        let literal_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(literal_idx);
        let literal_node = self.arena.get(literal_idx)?;
        if literal_node.kind != SyntaxKind::StringLiteral as u16 {
            return None;
        }
        let literal = self.get_source_slice(literal_node.pos, literal_node.end)?;
        match literal.trim().trim_matches(['"', '\'']) {
            "string" => Some("string"),
            "number" => Some("number"),
            "boolean" => Some("boolean"),
            "bigint" => Some("bigint"),
            "symbol" => Some("symbol"),
            "undefined" => Some("undefined"),
            _ => None,
        }
    }

    fn strip_parenthesized_union_element_type_text(type_text: &str) -> Option<String> {
        let trimmed = type_text.trim();
        if !trimmed.starts_with('(') || !trimmed.ends_with(')') || !trimmed.contains('|') {
            return None;
        }

        let mut depth = 0usize;
        for (idx, ch) in trimmed.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 && idx != trimmed.len() - ch.len_utf8() {
                        return None;
                    }
                }
                _ => {}
            }
        }
        (depth == 0).then(|| trimmed[1..trimmed.len() - 1].trim().to_string())
    }

    /// Returns `true` when `func_body` is a block whose sole non-trivial
    /// return expression is an object literal that contains at least one
    /// method whose body only returns `this`.
    ///
    /// When this is true, the solver infers a recursive "self-referential"
    /// object type for the function.  Printing that type through the solver's
    /// `TypePrinter` (with `max_depth = 128`) produces an exponentially large
    /// string.  The AST-based path already handles these methods correctly by
    /// emitting `/*elided*/ any` for the `this`-returning slots, so we prefer
    /// the source-derived type text and skip the expensive solver print.
    ///
    /// This is intentionally conservative: it only matches single-return
    /// functions whose return is a direct object literal.  More complex shapes
    /// (multiple returns, nested wrappers, etc.) fall through to the normal
    /// solver path.
    pub(in crate::declaration_emitter) fn function_body_returns_object_with_this_only_methods(
        &self,
        func_body: NodeIndex,
    ) -> bool {
        let body_node = match self.arena.get(func_body) {
            Some(n) => n,
            None => return false,
        };
        let block = match self.arena.get_block(body_node) {
            Some(b) => b,
            None => return false,
        };

        // Collect all non-trivial statements; we expect exactly one return.
        let returns: Vec<_> = block
            .statements
            .nodes
            .iter()
            .copied()
            .filter_map(|stmt_idx| {
                let stmt_node = self.arena.get(stmt_idx)?;
                if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
                    return None;
                }
                let ret = self.arena.get_return_statement(stmt_node)?;
                ret.expression.is_some().then_some(ret.expression)
            })
            .collect();

        if returns.len() != 1 {
            return false;
        }

        let expr_idx = returns[0];
        let expr_node = match self.arena.get(expr_idx) {
            Some(n) => n,
            None => return false,
        };
        if expr_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let obj = match self.arena.get_literal_expr(expr_node) {
            Some(o) => o,
            None => return false,
        };

        obj.elements.nodes.iter().copied().any(|prop_idx| {
            let prop_node = match self.arena.get(prop_idx) {
                Some(n) => n,
                None => return false,
            };
            let method = match self.arena.get_method_decl(prop_node) {
                Some(m) => m,
                None => return false,
            };
            self.method_body_returns_this(method.body)
        })
    }
}
