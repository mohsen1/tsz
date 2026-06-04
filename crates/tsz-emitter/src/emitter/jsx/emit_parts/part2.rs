impl<'a> Printer<'a> {
    // =========================================================================
    // JSX - Preserve Mode (default)
    // =========================================================================

    /// Check if a JSX child is a truly empty expression container `{}` with no
    /// inner comments.  Used in preserve mode to strip bare `{}` from JSX output
    /// (matching tsc behavior) while keeping `{/* comment */}` intact.
    pub(in super::super) fn is_empty_jsx_expression_without_comments(
        &self,
        child: NodeIndex,
    ) -> bool {
        let Some(node) = self.arena.get(child) else {
            return false;
        };
        if node.kind != syntax_kind_ext::JSX_EXPRESSION {
            return false;
        }
        let Some(expr) = self.arena.get_jsx_expression(node) else {
            return false;
        };
        if expr.expression.is_some() {
            return false;
        }
        if !self.empty_jsx_expression_has_source_close_brace(node) {
            return false;
        }
        // Check that there are no comments inside the expression range
        let has_tracked_comment = self
            .all_comments
            .iter()
            .any(|c| c.pos >= node.pos && c.end <= node.end);
        if has_tracked_comment {
            return false;
        }
        let first_unfiltered = self
            .source_comment_ranges
            .partition_point(|comment| comment.end <= node.pos);
        if self.source_comment_ranges[first_unfiltered..]
            .iter()
            .take_while(|comment| comment.pos < node.end)
            .any(|comment| comment.pos >= node.pos && comment.end <= node.end)
        {
            return false;
        }
        true
    }

    fn empty_jsx_expression_has_source_close_brace(&self, node: &Node) -> bool {
        let Some(source) = self.source_text else {
            return true;
        };
        let start = std::cmp::min(node.pos as usize, source.len());
        let end = std::cmp::min(node.end as usize, source.len());
        source[start..end].bytes().any(|byte| byte == b'}')
    }

    /// Walk all JSX children in source order, emitting non-empty children and
    /// consuming comments from empty expressions.  This ensures `comment_emit_idx`
    /// advances monotonically.
    ///
    /// `separator` controls what's written before each child:
    ///  - `JsxChildSep::CommaSpace`: writes `, ` before each child (classic createElement extra args)
    ///  - `JsxChildSep::CommaNewline`: writes `,\n` before each child (multiline classic)
    ///  - `JsxChildSep::CommaBetween`: writes `, ` only between children, not before first (automatic array)
    ///  - `JsxChildSep::None`: no separator (single-child automatic)
    pub(in super::super) fn emit_jsx_children_interleaved(
        &mut self,
        all_children: &[NodeIndex],
        filtered_children: &[NodeIndex],
        sep: JsxChildSep,
    ) {
        let mut filtered_idx = 0;
        for &child in all_children {
            if filtered_idx >= filtered_children.len() {
                // All filtered children emitted; skip remaining empty exprs
                if self.is_empty_jsx_expression(child)
                    && let Some(node) = self.arena.get(child)
                {
                    self.skip_comments_for_empty_jsx_expr(node);
                }
                continue;
            }

            if child == filtered_children[filtered_idx] {
                // Write separator
                match sep {
                    JsxChildSep::CommaSpace => self.write(", "),
                    JsxChildSep::CommaNewline => self.write_line(),
                    JsxChildSep::CommaBetween => {
                        if filtered_idx > 0 {
                            self.write(", ");
                        }
                    }
                    JsxChildSep::None => {}
                }
                self.emit_jsx_child_as_expression(child);
                if matches!(sep, JsxChildSep::CommaNewline)
                    && filtered_idx < filtered_children.len() - 1
                {
                    self.write(",");
                }
                filtered_idx += 1;
            } else if self.is_empty_jsx_expression(child)
                && let Some(node) = self.arena.get(child)
            {
                self.skip_comments_for_empty_jsx_expr(node);
            }
        }
    }

    pub(in super::super) fn jsx_children_need_array(&self, children: &[NodeIndex]) -> bool {
        children.len() > 1 || self.jsx_children_have_spread(children)
    }

    pub(in super::super) fn jsx_children_have_spread(&self, children: &[NodeIndex]) -> bool {
        children
            .iter()
            .any(|&child| self.jsx_child_is_spread_expression(child))
    }

    pub(in super::super) fn jsx_child_is_spread_expression(&self, child: NodeIndex) -> bool {
        self.jsx_child_spread_expression(child).is_some()
    }

    fn jsx_child_spread_expression(&self, child: NodeIndex) -> Option<NodeIndex> {
        let node = self.arena.get(child)?;
        if node.kind != syntax_kind_ext::JSX_EXPRESSION {
            return None;
        }
        let expr = self.arena.get_jsx_expression(node)?;
        (expr.dot_dot_dot_token && expr.expression.is_some()).then_some(expr.expression)
    }

    pub(in super::super) fn emit_jsx_children_value(
        &mut self,
        all_children: &[NodeIndex],
        filtered_children: &[NodeIndex],
        as_array: bool,
    ) {
        if !as_array {
            self.emit_jsx_children_interleaved(all_children, filtered_children, JsxChildSep::None);
            return;
        }

        if self.ctx.target_es5 && self.jsx_children_have_spread(filtered_children) {
            self.skip_empty_jsx_children_comments(all_children);
            self.emit_jsx_children_array_es5(filtered_children);
            return;
        }

        self.write("[");
        self.emit_jsx_children_interleaved(
            all_children,
            filtered_children,
            JsxChildSep::CommaBetween,
        );
        self.write("]");
    }

    fn emit_jsx_children_array_es5(&mut self, filtered_children: &[NodeIndex]) {
        enum Segment {
            Elements(Vec<NodeIndex>),
            Spread(NodeIndex),
        }

        let mut segments = Vec::new();
        let mut pending_elements = Vec::new();
        for &child in filtered_children {
            if let Some(spread_expr) = self.jsx_child_spread_expression(child) {
                if !pending_elements.is_empty() {
                    segments.push(Segment::Elements(std::mem::take(&mut pending_elements)));
                }
                segments.push(Segment::Spread(spread_expr));
            } else {
                pending_elements.push(child);
            }
        }
        if !pending_elements.is_empty() {
            segments.push(Segment::Elements(pending_elements));
        }

        let Some(first_segment) = segments.first() else {
            self.write("[]");
            return;
        };

        let first_is_spread = matches!(first_segment, Segment::Spread(_));
        let wrapper_count = segments.len().saturating_sub(1) + usize::from(first_is_spread);
        for _ in 0..wrapper_count {
            self.write_helper("__spreadArray");
            self.write("(");
        }

        match first_segment {
            Segment::Elements(elements) => self.emit_jsx_children_plain_array(elements),
            Segment::Spread(expr) => {
                self.write("[], ");
                self.emit(self.unwrap_spread_argument(*expr));
                self.write(", true)");
            }
        }

        for segment in segments.iter().skip(1) {
            self.write(", ");
            match segment {
                Segment::Elements(elements) => {
                    self.emit_jsx_children_plain_array(elements);
                    self.write(", false)");
                }
                Segment::Spread(expr) => {
                    self.emit(self.unwrap_spread_argument(*expr));
                    self.write(", true)");
                }
            }
        }
    }

    fn emit_jsx_children_plain_array(&mut self, children: &[NodeIndex]) {
        self.write("[");
        for (i, &child) in children.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.emit_jsx_child_as_expression(child);
        }
        self.write("]");
    }

    /// Emit a JSX child as an expression in the `createElement` args.
    pub(in super::super) fn emit_jsx_child_as_expression(&mut self, child: NodeIndex) {
        let Some(node) = self.arena.get(child) else {
            return;
        };

        if node.kind == SyntaxKind::JsxText as u16
            && let Some(text) = self.arena.get_jsx_text(node)
        {
            let processed = process_jsx_text(&text.text);
            let decoded = decode_jsx_entities(&processed);
            self.emit_jsx_double_quoted_js_string(&decoded);
            return;
        }

        if node.kind == syntax_kind_ext::JSX_EXPRESSION
            && let Some(expr) = self.arena.get_jsx_expression(node)
            && expr.expression.is_some()
        {
            // Spread children in classic mode: `{...expr}` -> `...expr`.
            // tsc unwraps parens that exist solely because of an erased type
            // cast (`(x as any)` → `x`), so `{...(x as any)}` becomes `...x`,
            // not `...(x)`. Walk past parens + as/satisfies/type-assertion
            // wrappers so the spread argument prints unwrapped.
            let target_expr = if expr.dot_dot_dot_token {
                self.write("...");
                self.unwrap_spread_argument(expr.expression)
            } else {
                expr.expression
            };
            self.emit(target_expr);
            // Emit trailing comments between expression and closing `}` of the
            // JSX expression container, e.g. `{null /* preserved */}` should
            // produce `null /* preserved */` in the createElement args.
            if let Some(expr_node) = self.arena.get(expr.expression) {
                let expr_token_end =
                    self.find_token_end_before_trivia(expr_node.pos, expr_node.end);
                self.emit_comments_in_range(expr_token_end, node.end, true, false);
            }
            return;
        }

        // JSX element, fragment, or self-closing element -- emit recursively
        // This will hit the transform dispatch again for nested JSX.
        self.emit(child);
    }

    /// Strip outer parens and erased type-cast wrappers (`as`, `satisfies`,
    /// `<T>x` assertions) around a spread argument. `...(expr as T)` should
    /// emit as `...expr`; the parens only existed for the cast.
    fn unwrap_spread_argument(&self, mut idx: NodeIndex) -> NodeIndex {
        loop {
            let Some(n) = self.arena.get(idx) else {
                return idx;
            };
            if n.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                if let Some(paren) = self.arena.get_parenthesized(n) {
                    idx = paren.expression;
                    continue;
                }
                return idx;
            }
            if n.kind == syntax_kind_ext::AS_EXPRESSION
                || n.kind == syntax_kind_ext::SATISFIES_EXPRESSION
                || n.kind == syntax_kind_ext::TYPE_ASSERTION
            {
                if let Some(inner) = self.arena.get_unary_expr(n) {
                    idx = inner.operand;
                    continue;
                }
                return idx;
            }
            return idx;
        }
    }

    /// Emit JSX attributes as a JS object literal: `{ key: value, ... }`
    pub(in super::super) fn emit_jsx_attrs_as_object(&mut self, attrs: &[JsxAttrInfo]) {
        let named: Vec<_> = attrs
            .iter()
            .filter(|a| matches!(a, JsxAttrInfo::Named { .. }))
            .collect();
        if named.is_empty() {
            self.write("{}");
            return;
        }
        self.write("{ ");
        let mut first = true;
        for attr in &named {
            if let JsxAttrInfo::Named { name, value } = attr {
                if !first {
                    self.write(", ");
                }
                first = false;
                self.emit_jsx_prop_name(name);
                self.write(": ");
                self.emit_jsx_attr_value(value);
            }
        }
        self.write(" }");
    }

    /// Emit a property name, quoting (with JS-escape) if it isn't a valid JS
    /// identifier; otherwise pass through verbatim to preserve any source
    /// `\uXXXX` spelling.
    pub(in super::super) fn emit_jsx_prop_name(&mut self, name: &str) {
        if needs_quoting(name) {
            self.emit_jsx_double_quoted_js_string(name);
        } else {
            self.write(name);
        }
    }

    /// Emit an attribute value, preserving original quote style for string literals.
    /// For string literals, decodes HTML entities and Unicode-escapes non-ASCII.
    pub(in super::super) fn emit_jsx_attr_value(&mut self, value: &JsxAttrValue) {
        match value {
            JsxAttrValue::StringNode(idx) => {
                let node = self.arena.get(*idx);
                let lit = node.and_then(|n| self.arena.get_literal(n));
                if let (Some(n), Some(lit_data)) = (node, lit) {
                    let decoded = decode_jsx_entities(&lit_data.text);
                    let quote = self.detect_original_quote(n).unwrap_or('"');
                    self.write_char(quote);
                    self.write(&escape_jsx_text_for_js_with_quote(&decoded, quote));
                    self.write_char(quote);
                } else {
                    self.emit(*idx);
                }
            }
            JsxAttrValue::Bool(b) => {
                self.write(if *b { "true" } else { "false" });
            }
            JsxAttrValue::Expr {
                expr,
                trailing_comment_scope,
            } => {
                if let Some(scope_idx) = trailing_comment_scope
                    && let Some(scope_node) = self.arena.get(*scope_idx)
                {
                    self.skip_jsx_attribute_expr_trailing_comments(scope_node);
                }
                self.emit(*expr);
            }
            JsxAttrValue::EmptyExpression => {}
        }
    }

    fn skip_jsx_attribute_expr_trailing_comments(&mut self, scope_node: &Node) {
        let Some(text) = self.source_text else {
            return;
        };
        let actual_end = self.find_token_end_before_trivia(scope_node.pos, scope_node.end);
        let bytes = text.as_bytes();
        while self.comment_emit_idx < self.all_comments.len() {
            let c_pos = self.all_comments[self.comment_emit_idx].pos;
            if c_pos < actual_end {
                break;
            }
            if c_pos >= scope_node.end {
                break;
            }
            let gap_start = actual_end as usize;
            let gap_end = std::cmp::min(c_pos as usize, bytes.len());
            if bytes[gap_start..gap_end]
                .iter()
                .any(|&b| b == b'\n' || b == b'\r')
            {
                break;
            }
            self.comment_emit_idx += 1;
        }
    }
}
