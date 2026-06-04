impl<'a> EnumES5Transformer<'a> {
    /// Evaluate a string enum member initializer at compile time.
    /// Returns `Some(folded_string)` when the expression can be constant-folded.
    /// Handles: string literals, string concatenation (`"a" + "b"` → `"ab"`),
    /// mixed string+numeric (`"a" + 1` → `"a1"`), and references to
    /// previously evaluated string or numeric enum members.
    fn evaluate_string_expression(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                let lit = self.arena.get_literal(node)?;
                Some(lit.text.clone())
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.arena.get_literal(node)?;
                // Parse and format to match tsc behavior
                if let Ok(n) = lit.text.parse::<i64>() {
                    Some(n.to_string())
                } else if let Ok(f) = lit.text.parse::<f64>() {
                    Some(f.to_string())
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                // Template expression with substitutions: `head${expr}middle${expr}tail`
                let tmpl = self.arena.get_template_expr(node)?;
                let head_node = self.arena.get(tmpl.head)?;
                let head_lit = self.arena.get_literal(head_node)?;
                let mut result = head_lit.text.clone();
                for &span_idx in &tmpl.template_spans.nodes {
                    let span_node = self.arena.get(span_idx)?;
                    let span = self.arena.get_template_span(span_node)?;
                    // Evaluate the expression part
                    let expr_val = self.evaluate_string_expression(span.expression)?;
                    result.push_str(&expr_val);
                    // Get the literal tail part
                    let lit_node = self.arena.get(span.literal)?;
                    let lit = self.arena.get_literal(lit_node)?;
                    result.push_str(&lit.text);
                }
                Some(result)
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let bin = self.arena.get_binary_expr(node)?;
                if bin.operator_token != SyntaxKind::PlusToken as u16 {
                    return None;
                }
                let left = self.evaluate_string_expression(bin.left)?;
                let right = self.evaluate_string_expression(bin.right)?;
                Some(format!("{left}{right}"))
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let paren = self.arena.get_parenthesized(node)?;
                self.evaluate_string_expression(paren.expression)
            }
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                let assertion = self.arena.get_type_assertion(node)?;
                self.evaluate_string_expression(assertion.expression)
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                let unary = self.arena.get_unary_expr_ex(node)?;
                self.evaluate_string_expression(unary.expression)
            }
            k if k == SyntaxKind::Identifier as u16 => {
                let id = self.arena.get_identifier(node)?;
                // Check current enum members first
                if let Some(s) = self.string_member_values.get(id.escaped_text.as_str()) {
                    return Some(s.clone());
                }
                if let Some(&n) = self.member_values.get(id.escaped_text.as_str()) {
                    return Some(n.to_string());
                }
                // Check prior blocks of the same merged enum
                if let Some(prior) = self.prior_string_values.get(&self.current_enum_name)
                    && let Some(s) = prior.get(id.escaped_text.as_str())
                {
                    return Some(s.clone());
                }
                if let Some(prior) = self.prior_enum_values.get(&self.current_enum_name)
                    && let Some(&n) = prior.get(id.escaped_text.as_str())
                {
                    return Some(n.to_string());
                }
                None
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr(node)?;
                let obj_node = self.arena.get(access.expression)?;
                if !obj_node.is_identifier() {
                    return None;
                }
                let obj_id = self.arena.get_identifier(obj_node)?;
                let prop_node = self.arena.get(access.name_or_argument)?;
                let prop_id = self.arena.get_identifier(prop_node)?;

                // Same enum self-reference
                if obj_id.escaped_text == self.current_enum_name {
                    if let Some(s) = self.string_member_values.get(prop_id.escaped_text.as_str()) {
                        return Some(s.clone());
                    }
                    if let Some(prior) = self.prior_string_values.get(&self.current_enum_name)
                        && let Some(s) = prior.get(prop_id.escaped_text.as_str())
                    {
                        return Some(s.clone());
                    }
                    if let Some(&n) = self.member_values.get(prop_id.escaped_text.as_str()) {
                        return Some(n.to_string());
                    }
                }
                // Cross-enum reference
                if let Some(prior) = self.prior_string_values.get(obj_id.escaped_text.as_str())
                    && let Some(value) = prior.get(prop_id.escaped_text.as_str())
                {
                    return Some(value.clone());
                }
                if let Some(prior) = self.prior_enum_values.get(obj_id.escaped_text.as_str())
                    && let Some(&n) = prior.get(prop_id.escaped_text.as_str())
                {
                    return Some(n.to_string());
                }
                None
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr(node)?;
                let obj_node = self.arena.get(access.expression)?;
                if !obj_node.is_identifier() {
                    return None;
                }
                let obj_id = self.arena.get_identifier(obj_node)?;
                let member_name = self.string_literal_key(access.name_or_argument)?;

                if obj_id.escaped_text == self.current_enum_name {
                    if let Some(s) = self.string_member_values.get(member_name.as_str()) {
                        return Some(s.clone());
                    }
                    if let Some(prior) = self.prior_string_values.get(&self.current_enum_name)
                        && let Some(s) = prior.get(member_name.as_str())
                    {
                        return Some(s.clone());
                    }
                    if let Some(&n) = self.member_values.get(member_name.as_str()) {
                        return Some(n.to_string());
                    }
                }
                if let Some(prior) = self.prior_string_values.get(obj_id.escaped_text.as_str())
                    && let Some(value) = prior.get(member_name.as_str())
                {
                    return Some(value.clone());
                }
                if let Some(prior) = self.prior_enum_values.get(obj_id.escaped_text.as_str())
                    && let Some(&n) = prior.get(member_name.as_str())
                {
                    return Some(n.to_string());
                }
                None
            }
            _ => None,
        }
    }

    fn string_literal_key(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        if node.kind == SyntaxKind::StringLiteral as u16
            || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            return self.arena.get_literal(node).map(|lit| lit.text.clone());
        }
        None
    }

    /// Check if an expression is syntactically string-valued per tsc's rules.
    /// String-valued enum members do NOT get reverse mappings.
    /// Handles: string literals, template literals, string concatenation (`"x" + expr`),
    /// references to other string-valued enum members, and parenthesized wrappers.
    fn is_syntactically_string(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16 => true,
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => true,
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => true,
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                // Unwrap parens: (`${BAR}`) is still syntactically string
                if let Some(paren) = self.arena.get_parenthesized(node) {
                    self.is_syntactically_string(paren.expression)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                if let Some(assertion) = self.arena.get_type_assertion(node) {
                    self.is_syntactically_string(assertion.expression)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr_ex(node) {
                    self.is_syntactically_string(unary.expression)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                // String concatenation: "x" + expr is syntactically string
                if let Some(bin) = self.arena.get_binary_expr(node) {
                    let is_plus = bin.operator_token == SyntaxKind::PlusToken as u16;
                    if is_plus {
                        self.is_syntactically_string(bin.left)
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                // E.A where A is a known string member — syntactically string
                if let Some(access) = self.arena.get_access_expr(node) {
                    // Check if the object is the enum parameter name
                    let obj_node = self.arena.get(access.expression);
                    let obj_is_enum = obj_node.is_some_and(|n| {
                        n.is_identifier()
                            && self
                                .arena
                                .get_identifier(n)
                                .is_some_and(|id| id.escaped_text == self.current_enum_name)
                    });
                    let prop_name = self
                        .arena
                        .get(access.name_or_argument)
                        .and_then(|n| self.arena.get_identifier(n))
                        .map(|id| id.escaped_text.as_str());

                    if obj_is_enum && let Some(name) = prop_name {
                        return self.string_members.contains(name);
                    }

                    // Cross-enum reference: check prior enum string members
                    if let Some(obj_name) = obj_node
                        .and_then(|n| self.arena.get_identifier(n))
                        .map(|id| id.escaped_text.as_str())
                        && let Some(prior) = self.prior_string_members.get(obj_name)
                        && let Some(name) = prop_name
                    {
                        return prior.contains(name);
                    }
                    false
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    let obj_node = self.arena.get(access.expression);
                    let obj_name = obj_node
                        .and_then(|n| self.arena.get_identifier(n))
                        .map(|id| id.escaped_text.as_str());
                    let Some(member_name) = self.string_literal_key(access.name_or_argument) else {
                        return false;
                    };

                    if obj_name == Some(self.current_enum_name.as_str()) {
                        return self.string_members.contains(member_name.as_str());
                    }
                    if let Some(obj_name) = obj_name
                        && let Some(prior) = self.prior_string_members.get(obj_name)
                    {
                        return prior.contains(member_name.as_str());
                    }
                    false
                } else {
                    false
                }
            }
            k if k == SyntaxKind::Identifier as u16 => {
                // Bare identifier that matches a known string member
                if let Some(id) = self.arena.get_identifier(node) {
                    if self.string_members.contains(id.escaped_text.as_str()) {
                        return true;
                    }
                    // Check prior blocks of the same merged enum
                    if let Some(prior) = self.prior_string_members.get(&self.current_enum_name) {
                        return prior.contains(id.escaped_text.as_str());
                    }
                }
                false
            }
            _ => false,
        }
    }
}
