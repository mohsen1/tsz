use super::Printer;
use super::core::JsxEmit;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    // =========================================================================
    // JSX - Preserve Mode (default)
    // =========================================================================

    pub(super) fn emit_jsx_element(&mut self, node: &Node) {
        match self.ctx.options.jsx {
            JsxEmit::React => self.emit_jsx_element_classic(node),
            JsxEmit::ReactJsx | JsxEmit::ReactJsxDev => self.emit_jsx_element_automatic(node),
            _ => self.emit_jsx_element_preserve(node),
        }
    }

    pub(super) fn emit_jsx_self_closing_element(&mut self, node: &Node) {
        match self.ctx.options.jsx {
            JsxEmit::React => self.emit_jsx_self_closing_classic(node),
            JsxEmit::ReactJsx | JsxEmit::ReactJsxDev => {
                self.emit_jsx_self_closing_automatic(node);
            }
            _ => self.emit_jsx_self_closing_preserve(node),
        }
    }

    pub(super) fn emit_jsx_fragment(&mut self, node: &Node) {
        match self.ctx.options.jsx {
            JsxEmit::React => self.emit_jsx_fragment_classic(node),
            JsxEmit::ReactJsx | JsxEmit::ReactJsxDev => self.emit_jsx_fragment_automatic(node),
            _ => self.emit_jsx_fragment_preserve(node),
        }
    }

    // =========================================================================
    // JSX Preserve Mode
    // =========================================================================

    fn emit_jsx_element_preserve(&mut self, node: &Node) {
        let Some(jsx) = self.arena.get_jsx_element(node) else {
            return;
        };

        self.emit(jsx.opening_element);
        for &child in &jsx.children.nodes {
            self.emit(child);
        }
        self.emit(jsx.closing_element);
    }

    fn emit_jsx_self_closing_preserve(&mut self, node: &Node) {
        let Some(jsx) = self.arena.get_jsx_opening(node) else {
            return;
        };

        self.write("<");
        self.emit(jsx.tag_name);
        self.emit(jsx.attributes);
        // tsc always emits writeSpace() after tagName. Our emit_jsx_attributes
        // already prepends a space before each attribute, so the space is only
        // missing when there are no attributes at all.
        let has_attributes = self
            .arena
            .get(jsx.attributes)
            .and_then(|n| self.arena.get_jsx_attributes(n))
            .is_some_and(|a| !a.properties.nodes.is_empty());
        if !has_attributes {
            self.write(" ");
        }
        self.write("/>");
    }

    fn emit_jsx_fragment_preserve(&mut self, node: &Node) {
        let Some(jsx) = self.arena.get_jsx_fragment(node) else {
            return;
        };

        self.write("<>");
        for &child in &jsx.children.nodes {
            self.emit(child);
        }
        self.write("</>");
    }

    // =========================================================================
    // JSX Classic Transform (jsx=react)
    // Output: React.createElement(tag, props, ...children)
    // =========================================================================

    fn emit_jsx_element_classic(&mut self, node: &Node) {
        let Some(jsx) = self.arena.get_jsx_element(node) else {
            return;
        };

        let Some(opening_node) = self.arena.get(jsx.opening_element) else {
            return;
        };
        let Some(opening) = self.arena.get_jsx_opening(opening_node) else {
            return;
        };

        let tag_name = opening.tag_name;
        let attributes = opening.attributes;
        let children: Vec<NodeIndex> = jsx.children.nodes.to_vec();

        self.emit_create_element_call(tag_name, attributes, &children);
    }

    fn emit_jsx_self_closing_classic(&mut self, node: &Node) {
        let Some(jsx) = self.arena.get_jsx_opening(node) else {
            return;
        };

        let tag_name = jsx.tag_name;
        let attributes = jsx.attributes;

        self.emit_create_element_call(tag_name, attributes, &[]);
    }

    fn emit_jsx_fragment_classic(&mut self, node: &Node) {
        let Some(jsx) = self.arena.get_jsx_fragment(node) else {
            return;
        };

        let children: Vec<NodeIndex> = jsx.children.nodes.to_vec();
        let factory = self.get_jsx_factory();
        let fragment_factory = self.get_jsx_fragment_factory();

        self.write(&factory);
        self.write("(");
        self.write(&fragment_factory);
        self.write(", null");

        // Children
        let filtered_children = self.collect_jsx_children(&children);
        for child in &filtered_children {
            self.write(", ");
            self.emit_jsx_child_as_expression(*child);
        }

        self.write(")");
    }

    /// Emit `factory(tag, props, ...children)`
    fn emit_create_element_call(
        &mut self,
        tag_name: NodeIndex,
        attributes: NodeIndex,
        children: &[NodeIndex],
    ) {
        let factory = self.get_jsx_factory();

        // Collect attributes info
        let attrs_info = self.collect_jsx_attributes_info(attributes);
        let filtered_children = self.collect_jsx_children(children);
        let has_spread = attrs_info.has_spread;

        self.write(&factory);
        self.write("(");

        // Tag name
        self.emit_jsx_tag_name_as_argument(tag_name);

        // Props
        self.write(", ");
        if attrs_info.attrs.is_empty() && !has_spread {
            self.write("null");
        } else if has_spread {
            self.emit_jsx_spread_attrs_classic(&attrs_info.attrs);
        } else {
            self.emit_jsx_attrs_as_object(&attrs_info.attrs);
        }

        // Children — tsc formats multiple children on separate indented lines
        let multiline_children = filtered_children.len() > 1;
        if multiline_children {
            self.write(",");
            self.increase_indent();
        }
        for (i, child) in filtered_children.iter().enumerate() {
            if multiline_children {
                self.write_line();
            } else {
                self.write(", ");
            }
            self.emit_jsx_child_as_expression(*child);
            if multiline_children && i < filtered_children.len() - 1 {
                self.write(",");
            }
        }
        self.write(")");
        if multiline_children {
            self.decrease_indent();
        }
    }

    // =========================================================================
    // JSX Automatic Transform (jsx=react-jsx)
    // Output: _jsx(tag, { ...props, children }) or _jsxs(tag, { ...props, children: [...] })
    // =========================================================================

    fn emit_jsx_element_automatic(&mut self, node: &Node) {
        let Some(jsx) = self.arena.get_jsx_element(node) else {
            return;
        };

        let Some(opening_node) = self.arena.get(jsx.opening_element) else {
            return;
        };
        let Some(opening) = self.arena.get_jsx_opening(opening_node) else {
            return;
        };

        let tag_name = opening.tag_name;
        let attributes = opening.attributes;
        let children: Vec<NodeIndex> = jsx.children.nodes.to_vec();

        self.emit_jsx_automatic_call(tag_name, attributes, &children);
    }

    fn emit_jsx_self_closing_automatic(&mut self, node: &Node) {
        let Some(jsx) = self.arena.get_jsx_opening(node) else {
            return;
        };

        let tag_name = jsx.tag_name;
        let attributes = jsx.attributes;

        self.emit_jsx_automatic_call(tag_name, attributes, &[]);
    }

    fn emit_jsx_fragment_automatic(&mut self, node: &Node) {
        let Some(jsx) = self.arena.get_jsx_fragment(node) else {
            return;
        };

        let children: Vec<NodeIndex> = jsx.children.nodes.to_vec();
        let filtered_children = self.collect_jsx_children(&children);
        let is_jsxs = filtered_children.len() > 1;
        let is_cjs = self.ctx.is_commonjs();
        let func_name = if is_jsxs { "jsxs" } else { "jsx" };

        if is_cjs {
            let var_name = self.jsx_cjs_runtime_var();
            self.write(&format!("(0, {var_name}.{func_name})("));
        } else {
            self.write(if is_jsxs { "_jsxs(" } else { "_jsx(" });
        }

        // Fragment tag
        if is_cjs {
            let var_name = self.jsx_cjs_runtime_var();
            self.write(&format!("{var_name}.Fragment"));
        } else {
            self.write("_Fragment");
        }

        // Props with children
        self.write(", { ");
        if !filtered_children.is_empty() {
            self.write("children: ");
            if is_jsxs {
                self.write("[");
                for (i, child) in filtered_children.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.emit_jsx_child_as_expression(*child);
                }
                self.write("]");
            } else {
                self.emit_jsx_child_as_expression(filtered_children[0]);
            }
            self.write(" ");
        }
        self.write("}");

        self.write(")");
    }

    /// Emit `_jsx(tag, { ...props, children })` or `_jsxs(tag, { ...props, children: [...] })`
    fn emit_jsx_automatic_call(
        &mut self,
        tag_name: NodeIndex,
        attributes: NodeIndex,
        children: &[NodeIndex],
    ) {
        let attrs_info = self.collect_jsx_attributes_info(attributes);
        let filtered_children = self.collect_jsx_children(children);
        let is_jsxs = filtered_children.len() > 1;

        // Extract key from attributes
        let key_attr = attrs_info
            .attrs
            .iter()
            .find(|a| matches!(a, JsxAttrInfo::Named { name, .. } if name == "key"))
            .cloned();
        let non_key_attrs: Vec<JsxAttrInfo> = attrs_info
            .attrs
            .iter()
            .filter(|a| !matches!(a, JsxAttrInfo::Named { name, .. } if name == "key"))
            .cloned()
            .collect();
        let has_non_key_attrs = !non_key_attrs.is_empty() || attrs_info.has_spread;

        let is_cjs = self.ctx.is_commonjs();
        let func_name = if is_jsxs { "jsxs" } else { "jsx" };

        if is_cjs {
            // CJS: (0, jsx_runtime_1.jsx)(tag, props)
            let var_name = self.jsx_cjs_runtime_var();
            self.write(&format!("(0, {var_name}.{func_name})("));
        } else {
            // ESM: _jsx(tag, props)
            self.write(if is_jsxs { "_jsxs(" } else { "_jsx(" });
        }

        // Tag name
        self.emit_jsx_tag_name_as_argument(tag_name);

        // Props object (with children embedded)
        self.write(", ");
        let has_children = !filtered_children.is_empty();
        let has_spread_in_non_key = non_key_attrs
            .iter()
            .any(|a| matches!(a, JsxAttrInfo::Spread { .. }));

        if !has_non_key_attrs && !has_children {
            self.write("{}");
        } else if has_spread_in_non_key {
            // Inline spread: { named, ...spread, children }
            self.emit_jsx_spread_attrs_automatic(&non_key_attrs, &filtered_children, is_jsxs);
        } else {
            self.write("{ ");
            // Emit non-key attrs
            let mut first = true;
            for attr in &non_key_attrs {
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
            // Children
            if has_children {
                if !first {
                    self.write(", ");
                }
                self.write("children: ");
                if is_jsxs {
                    self.write("[");
                    for (i, child) in filtered_children.iter().enumerate() {
                        if i > 0 {
                            self.write(", ");
                        }
                        self.emit_jsx_child_as_expression(*child);
                    }
                    self.write("]");
                } else {
                    self.emit_jsx_child_as_expression(filtered_children[0]);
                }
            }
            self.write(" }");
        }

        // Key as third argument (for automatic transform)
        if let Some(JsxAttrInfo::Named { value, .. }) = &key_attr {
            self.write(", ");
            self.emit_jsx_attr_value(value);
        }

        self.write(")");
    }

    // =========================================================================
    // Shared JSX Helpers
    // =========================================================================

    /// Get the JSX factory function name (e.g. "React.createElement" or custom)
    fn get_jsx_factory(&self) -> String {
        self.ctx
            .options
            .jsx_factory
            .as_deref()
            .unwrap_or("React.createElement")
            .to_string()
    }

    /// Get the JSX fragment factory (e.g. "React.Fragment" or custom)
    fn get_jsx_fragment_factory(&self) -> String {
        self.ctx
            .options
            .jsx_fragment_factory
            .as_deref()
            .unwrap_or("React.Fragment")
            .to_string()
    }

    /// Emit a JSX tag name as a function argument.
    /// Intrinsic elements (lowercase) → string literal.
    /// Component elements (uppercase/dotted/namespaced) → identifier/expression.
    fn emit_jsx_tag_name_as_argument(&mut self, tag_name: NodeIndex) {
        let Some(node) = self.arena.get(tag_name) else {
            self.write("\"\"");
            return;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            // Check if it's an intrinsic element (starts with lowercase)
            if let Some(ident) = self.arena.get_identifier(node) {
                let text = self.arena.resolve_identifier_text(ident);
                if text.starts_with(|c: char| c.is_ascii_lowercase()) {
                    self.write("\"");
                    self.write(text);
                    self.write("\"");
                } else {
                    self.write(text);
                }
                return;
            }
        }

        // Property access (e.g. Foo.Bar) or other expression — emit as-is
        self.emit(tag_name);
    }

    /// Collect attributes from a JSX attributes node, returning info about each attribute.
    fn collect_jsx_attributes_info(&self, attributes: NodeIndex) -> JsxAttrsInfo {
        let mut result = JsxAttrsInfo {
            attrs: Vec::new(),
            has_spread: false,
        };

        let Some(attrs_node) = self.arena.get(attributes) else {
            return result;
        };
        let Some(attrs) = self.arena.get_jsx_attributes(attrs_node) else {
            return result;
        };

        for &prop in &attrs.properties.nodes {
            let Some(prop_node) = self.arena.get(prop) else {
                continue;
            };

            if prop_node.kind == syntax_kind_ext::JSX_ATTRIBUTE {
                if let Some(attr) = self.arena.get_jsx_attribute(prop_node) {
                    let name = self.get_jsx_attr_name(attr.name);
                    let value = if attr.initializer.is_some() {
                        self.get_jsx_attr_value_info(attr.initializer)
                    } else {
                        // Attribute without value (e.g. `<div disabled />`) → true
                        JsxAttrValue::Bool(true)
                    };
                    result.attrs.push(JsxAttrInfo::Named { name, value });
                }
            } else if prop_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE
                && let Some(spread) = self.arena.get_jsx_spread_attribute(prop_node)
            {
                result.has_spread = true;
                result.attrs.push(JsxAttrInfo::Spread {
                    expr: spread.expression,
                });
            }
        }

        result
    }

    /// Get the name of a JSX attribute (identifier or namespaced).
    fn get_jsx_attr_name(&self, name_idx: NodeIndex) -> String {
        let Some(node) = self.arena.get(name_idx) else {
            return String::new();
        };

        if node.kind == SyntaxKind::Identifier as u16
            && let Some(ident) = self.arena.get_identifier(node)
        {
            return self.arena.resolve_identifier_text(ident).to_string();
        }

        if node.kind == syntax_kind_ext::JSX_NAMESPACED_NAME
            && let Some(ns) = self.arena.get_jsx_namespaced_name(node)
        {
            let ns_text = self
                .arena
                .get(ns.namespace)
                .and_then(|n| self.arena.get_identifier(n))
                .map(|i| self.arena.resolve_identifier_text(i))
                .unwrap_or("");
            let name_text = self
                .arena
                .get(ns.name)
                .and_then(|n| self.arena.get_identifier(n))
                .map(|i| self.arena.resolve_identifier_text(i))
                .unwrap_or("");
            return format!("{ns_text}:{name_text}");
        }

        String::new()
    }

    /// Get value info from a JSX attribute initializer.
    fn get_jsx_attr_value_info(&self, init_idx: NodeIndex) -> JsxAttrValue {
        let Some(node) = self.arena.get(init_idx) else {
            return JsxAttrValue::Bool(true);
        };

        // String literal (e.g. `"hello"`) — preserve original node for quote fidelity
        if node.kind == SyntaxKind::StringLiteral as u16 {
            return JsxAttrValue::StringNode(init_idx);
        }

        // JSX expression container (e.g. `{expr}`)
        if node.kind == syntax_kind_ext::JSX_EXPRESSION {
            if let Some(expr) = self.arena.get_jsx_expression(node)
                && expr.expression.is_some()
            {
                return JsxAttrValue::Expr(expr.expression);
            }
            return JsxAttrValue::Bool(true);
        }

        // JSX element used as attr value
        JsxAttrValue::Expr(init_idx)
    }

    /// Collect non-trivial JSX children. Filters out whitespace-only text nodes
    /// (matching tsc's behavior of trimming JSX text).
    fn collect_jsx_children(&self, children: &[NodeIndex]) -> Vec<NodeIndex> {
        let mut result = Vec::new();
        for &child in children {
            let Some(node) = self.arena.get(child) else {
                continue;
            };

            if node.kind == SyntaxKind::JsxText as u16 {
                if let Some(text) = self.arena.get_jsx_text(node) {
                    if text.contains_only_trivia_white_spaces {
                        continue;
                    }
                    // Process JSX text: trim and normalize as tsc does
                    let processed = process_jsx_text(&text.text);
                    if processed.is_empty() {
                        continue;
                    }
                }
                result.push(child);
            } else if node.kind == syntax_kind_ext::JSX_EXPRESSION {
                // Empty JSX expression containers `{}` are skipped
                if let Some(expr) = self.arena.get_jsx_expression(node)
                    && expr.expression.is_some()
                {
                    result.push(child);
                }
            } else {
                result.push(child);
            }
        }
        result
    }

    /// Emit a JSX child as an expression in the `createElement` args.
    fn emit_jsx_child_as_expression(&mut self, child: NodeIndex) {
        let Some(node) = self.arena.get(child) else {
            return;
        };

        if node.kind == SyntaxKind::JsxText as u16
            && let Some(text) = self.arena.get_jsx_text(node)
        {
            let processed = process_jsx_text(&text.text);
            let decoded = decode_jsx_entities(&processed);
            self.write("\"");
            self.write(&escape_jsx_text_for_js_with_quote(&decoded, '"'));
            self.write("\"");
            return;
        }

        if node.kind == syntax_kind_ext::JSX_EXPRESSION
            && let Some(expr) = self.arena.get_jsx_expression(node)
            && expr.expression.is_some()
        {
            self.emit(expr.expression);
            return;
        }

        // JSX element, fragment, or self-closing element — emit recursively
        // This will hit the transform dispatch again for nested JSX.
        self.emit(child);
    }

    /// Emit JSX attributes as a JS object literal: `{ key: value, ... }`
    fn emit_jsx_attrs_as_object(&mut self, attrs: &[JsxAttrInfo]) {
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

    /// Emit a property name, quoting if needed.
    fn emit_jsx_prop_name(&mut self, name: &str) {
        if needs_quoting(name) {
            self.write("\"");
            self.write(name);
            self.write("\"");
        } else {
            self.write(name);
        }
    }

    /// Emit an attribute value, preserving original quote style for string literals.
    /// For string literals, decodes HTML entities and Unicode-escapes non-ASCII.
    fn emit_jsx_attr_value(&mut self, value: &JsxAttrValue) {
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
            JsxAttrValue::Expr(idx) => {
                self.emit(*idx);
            }
        }
    }

    /// Emit classic spread attrs: handles mixes of spread and named attrs.
    /// Uses `Object.assign` when there are spreads mixed with named props.
    fn emit_jsx_spread_attrs_classic(&mut self, attrs: &[JsxAttrInfo]) {
        // Group consecutive named attrs and emit them as object literals,
        // interleaved with spread expressions via Object.assign.
        // e.g. <div a="1" {...x} b="2" /> → Object.assign({a: "1"}, x, {b: "2"})
        // If only spreads: just the spread expression(s)
        // If single spread with no named: spread expr
        // Multiple: Object.assign(...)
        let groups = group_jsx_attrs(attrs);

        if groups.len() == 1 {
            match &groups[0] {
                AttrGroup::Named(named) => self.emit_jsx_attrs_as_object(named),
                AttrGroup::Spread(expr) => self.emit(*expr),
            }
        } else {
            self.write("Object.assign(");
            for (i, group) in groups.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                match group {
                    AttrGroup::Named(named) => self.emit_jsx_attrs_as_object(named),
                    AttrGroup::Spread(expr) => self.emit(*expr),
                }
            }
            self.write(")");
        }
    }

    /// Emit automatic transform props with inline spread syntax.
    /// tsc emits `{ named, ...spread, children }` — a single object literal.
    fn emit_jsx_spread_attrs_automatic(
        &mut self,
        attrs: &[JsxAttrInfo],
        children: &[NodeIndex],
        is_jsxs: bool,
    ) {
        self.write("{ ");
        let mut first = true;

        for attr in attrs {
            match attr {
                JsxAttrInfo::Named { name, value } => {
                    if !first {
                        self.write(", ");
                    }
                    first = false;
                    self.emit_jsx_prop_name(name);
                    self.write(": ");
                    self.emit_jsx_attr_value(value);
                }
                JsxAttrInfo::Spread { expr } => {
                    if !first {
                        self.write(", ");
                    }
                    first = false;
                    self.write("...");
                    self.emit(*expr);
                }
            }
        }

        // Add children prop
        if !children.is_empty() {
            if !first {
                self.write(", ");
            }
            self.write("children: ");
            if is_jsxs {
                self.write("[");
                for (i, child) in children.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.emit_jsx_child_as_expression(*child);
                }
                self.write("]");
            } else {
                self.emit_jsx_child_as_expression(children[0]);
            }
        }

        self.write(" }");
    }

    // =========================================================================
    // JSX Preserve Mode Helpers (existing, now called via preserve methods)
    // =========================================================================

    pub(super) fn emit_jsx_opening_element(&mut self, node: &Node) {
        let Some(jsx) = self.arena.get_jsx_opening(node) else {
            return;
        };

        self.write("<");
        self.emit(jsx.tag_name);
        self.emit(jsx.attributes);
        self.write(">");
    }

    pub(super) fn emit_jsx_closing_element(&mut self, node: &Node) {
        let Some(jsx) = self.arena.get_jsx_closing(node) else {
            return;
        };

        self.write("</");
        self.emit(jsx.tag_name);
        self.write(">");
    }

    pub(super) fn emit_jsx_attributes(&mut self, node: &Node) {
        let Some(attrs) = self.arena.get_jsx_attributes(node) else {
            return;
        };

        for &attr in &attrs.properties.nodes {
            self.write_space();
            self.emit(attr);
        }
    }

    pub(super) fn emit_jsx_attribute(&mut self, node: &Node) {
        let Some(attr) = self.arena.get_jsx_attribute(node) else {
            return;
        };

        self.emit(attr.name);
        if attr.initializer.is_some() {
            self.write("=");
            self.emit(attr.initializer);
        }
    }

    pub(super) fn emit_jsx_spread_attribute(&mut self, node: &Node) {
        let Some(spread) = self.arena.get_jsx_spread_attribute(node) else {
            return;
        };

        self.write("{...");
        self.emit(spread.expression);
        self.write("}");
    }

    pub(super) fn emit_jsx_expression(&mut self, node: &Node) {
        let Some(expr) = self.arena.get_jsx_expression(node) else {
            return;
        };

        self.write("{");
        if expr.dot_dot_dot_token {
            self.write("...");
        }

        if expr.expression.is_none() {
            // JSX expression with only trivia/comments, e.g. `{}` or `{ // comment }`.
            // Emit all comments inside the brace pair so they don't drift into the
            // parent JSX element as trailing comments.
            self.increase_indent();
            let (has_comment, last_comment_end, last_comment_has_newline) =
                self.emit_comments_in_range(node.pos + 1, node.end, false, true);
            self.decrease_indent();
            if has_comment
                && !last_comment_has_newline
                && self.should_emit_space_before_closing_jsx_brace(
                    node.pos + 1,
                    node.end,
                    last_comment_end,
                )
            {
                self.write(" ");
            }
        } else if let Some(expr_node) = self.arena.get(expr.expression) {
            // Emit comments between `{` and the expression, such as `{
            // /* comment */ expr }` in JSX context.
            self.emit_comments_in_range(node.pos + 1, expr_node.pos, false, false);
            self.emit(expr.expression);

            // Emit comments between the expression and the closing brace.
            let expr_token_end = self.find_token_end_before_trivia(expr_node.pos, expr_node.end);
            self.emit_comments_in_range(expr_token_end, node.end, true, false);
        }
        self.write("}");
    }

    fn should_emit_space_before_closing_jsx_brace(
        &self,
        expression_start: u32,
        expression_end: u32,
        last_comment_end: u32,
    ) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let bytes = text.as_bytes();
        let mut pos = last_comment_end as usize;
        let end = (expression_end as usize).min(bytes.len());

        // TSC adds a space before a final `}` only in multi-line JSX
        // expression trivia sections (for example, when a prior line comment
        // keeps the final comment on its own line).
        let has_line_break_in_expression = (expression_start as usize..last_comment_end as usize)
            .any(|i| matches!(bytes[i], b'\n' | b'\r'));
        if !has_line_break_in_expression {
            return false;
        }

        while pos < end {
            match bytes[pos] {
                b' ' | b'\t' => pos += 1,
                b'}' => return true,
                _ => return false,
            }
        }

        false
    }

    pub(super) fn emit_jsx_text(&mut self, node: &Node) {
        let Some(text) = self.arena.get_jsx_text(node) else {
            return;
        };

        self.write(&text.text);
    }

    pub(super) fn emit_jsx_namespaced_name(&mut self, node: &Node) {
        let Some(ns) = self.arena.get_jsx_namespaced_name(node) else {
            return;
        };

        self.emit(ns.namespace);
        self.write(":");
        self.emit(ns.name);
    }

    // =========================================================================
    // JSX Auto Import Injection
    // =========================================================================

    /// Get the CJS variable name for the JSX runtime module import.
    /// e.g., "react/jsx-runtime" → "`jsx_runtime_1`", "react/jsx-dev-runtime" → "`jsx_dev_runtime_1`"
    fn jsx_cjs_runtime_var(&self) -> String {
        let suffix = match self.ctx.options.jsx {
            JsxEmit::ReactJsxDev => "jsx-dev-runtime",
            _ => "jsx-runtime",
        };
        let sanitized = crate::transforms::emit_utils::sanitize_module_name(suffix);
        format!("{sanitized}_1")
    }

    /// Check if the file needs JSX runtime auto-imports and return the import text.
    /// Called at the start of source file emission for jsx=react-jsx/react-jsxdev.
    /// Only imports the functions that are actually used in the file.
    pub(super) fn jsx_auto_import_text(&self) -> Option<String> {
        let is_cjs = self.ctx.is_commonjs();
        match self.ctx.options.jsx {
            JsxEmit::ReactJsx => {
                let source = self
                    .ctx
                    .options
                    .jsx_import_source
                    .as_deref()
                    .unwrap_or("react");
                let usage = self.scan_jsx_usage();
                if !usage.needs_jsx && !usage.needs_jsxs && !usage.needs_fragment {
                    return None;
                }
                if is_cjs {
                    let var_name = self.jsx_cjs_runtime_var();
                    Some(format!(
                        "const {var_name} = require(\"{source}/jsx-runtime\");\n"
                    ))
                } else {
                    let mut imports = Vec::new();
                    if usage.needs_jsx {
                        imports.push("jsx as _jsx");
                    }
                    if usage.needs_jsxs {
                        imports.push("jsxs as _jsxs");
                    }
                    if usage.needs_fragment {
                        imports.push("Fragment as _Fragment");
                    }
                    Some(format!(
                        "import {{ {} }} from \"{source}/jsx-runtime\";\n",
                        imports.join(", ")
                    ))
                }
            }
            JsxEmit::ReactJsxDev => {
                let source = self
                    .ctx
                    .options
                    .jsx_import_source
                    .as_deref()
                    .unwrap_or("react");
                let usage = self.scan_jsx_usage();
                if !usage.needs_jsx && !usage.needs_jsxs && !usage.needs_fragment {
                    return None;
                }
                if is_cjs {
                    let var_name = self.jsx_cjs_runtime_var();
                    Some(format!(
                        "const {var_name} = require(\"{source}/jsx-dev-runtime\");\n"
                    ))
                } else {
                    let mut imports = Vec::new();
                    if usage.needs_jsx || usage.needs_jsxs {
                        imports.push("jsxDEV as _jsxDEV");
                    }
                    if usage.needs_fragment {
                        imports.push("Fragment as _Fragment");
                    }
                    Some(format!(
                        "import {{ {} }} from \"{source}/jsx-dev-runtime\";\n",
                        imports.join(", ")
                    ))
                }
            }
            _ => None,
        }
    }

    /// Scan the AST to determine which JSX runtime functions are needed.
    fn scan_jsx_usage(&self) -> JsxUsage {
        let mut usage = JsxUsage {
            needs_jsx: false,
            needs_jsxs: false,
            needs_fragment: false,
        };
        for i in 0..self.arena.len() {
            let nidx = tsz_parser::parser::NodeIndex(i as u32);
            let Some(node) = self.arena.get(nidx) else {
                continue;
            };
            match node.kind {
                k if k == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT => {
                    usage.needs_jsx = true;
                }
                k if k == syntax_kind_ext::JSX_ELEMENT => {
                    // Check if this element has multiple children (→ _jsxs) or not (→ _jsx)
                    if let Some(jsx) = self.arena.get_jsx_element(node) {
                        let children = self.collect_jsx_children(&jsx.children.nodes);
                        if children.len() > 1 {
                            usage.needs_jsxs = true;
                        } else {
                            usage.needs_jsx = true;
                        }
                    }
                }
                k if k == syntax_kind_ext::JSX_FRAGMENT => {
                    usage.needs_fragment = true;
                    // Fragments also use _jsx or _jsxs
                    if let Some(frag) = self.arena.get_jsx_fragment(node) {
                        let children = self.collect_jsx_children(&frag.children.nodes);
                        if children.len() > 1 {
                            usage.needs_jsxs = true;
                        } else {
                            usage.needs_jsx = true;
                        }
                    }
                }
                _ => {}
            }
        }
        usage
    }
}

// =============================================================================
// Internal Data Types
// =============================================================================

struct JsxUsage {
    needs_jsx: bool,
    needs_jsxs: bool,
    needs_fragment: bool,
}

#[derive(Clone)]
enum JsxAttrInfo {
    Named { name: String, value: JsxAttrValue },
    Spread { expr: NodeIndex },
}

#[derive(Clone)]
enum JsxAttrValue {
    /// String literal attribute — carries the node index for quote-preserving emission
    StringNode(NodeIndex),
    Bool(bool),
    Expr(NodeIndex),
}

struct JsxAttrsInfo {
    attrs: Vec<JsxAttrInfo>,
    has_spread: bool,
}

enum AttrGroup {
    Named(Vec<JsxAttrInfo>),
    Spread(NodeIndex),
}

/// Group consecutive named attributes together, with spreads as separators.
fn group_jsx_attrs(attrs: &[JsxAttrInfo]) -> Vec<AttrGroup> {
    let mut groups: Vec<AttrGroup> = Vec::new();
    let mut current_named: Vec<JsxAttrInfo> = Vec::new();

    for attr in attrs {
        match attr {
            JsxAttrInfo::Spread { expr } => {
                if !current_named.is_empty() {
                    groups.push(AttrGroup::Named(std::mem::take(&mut current_named)));
                }
                groups.push(AttrGroup::Spread(*expr));
            }
            named => {
                current_named.push(named.clone());
            }
        }
    }

    if !current_named.is_empty() {
        groups.push(AttrGroup::Named(current_named));
    }

    // If the first element is a spread, prepend an empty object for Object.assign
    if !groups.is_empty() && matches!(groups[0], AttrGroup::Spread(_)) {
        groups.insert(0, AttrGroup::Named(Vec::new()));
    }

    groups
}

// =============================================================================
// JSX Text Processing (matches tsc behavior)
// =============================================================================

/// Process JSX text content matching tsc's `getTransformedJsxText` algorithm:
///
/// - If the text has no newlines, return it as-is (preserving whitespace).
/// - If multi-line, trim each line's leading/trailing whitespace, skip empty
///   lines, and join with a single space.
fn process_jsx_text(text: &str) -> String {
    // No newlines at all → return as-is (even if whitespace-only)
    if !text.contains('\n') {
        return text.to_string();
    }

    // Multi-line processing (matches tsc's algorithm)
    let lines: Vec<&str> = text.split('\n').collect();
    let mut parts: Vec<String> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = if i == 0 {
            // First line: trim end only
            line.trim_end()
        } else if i == lines.len() - 1 {
            // Last line: trim start only
            line.trim_start()
        } else {
            // Middle lines: trim both
            line.trim()
        };

        if trimmed.is_empty() {
            continue;
        }
        parts.push(trimmed.to_string());
    }

    parts.join(" ")
}

/// Escape a string for JS string literal context with entity decoding and Unicode escaping.
/// `quote` is the surrounding quote char (' or ") so we know which to escape.
fn escape_jsx_text_for_js_with_quote(s: &str, quote: char) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c == quote => {
                result.push('\\');
                result.push(c);
            }
            // Non-ASCII chars get \uXXXX escaping (or surrogate pairs for > U+FFFF)
            c if c as u32 > 0x7E => {
                let cp = c as u32;
                if cp > 0xFFFF {
                    // UTF-16 surrogate pair
                    let hi = 0xD800 + ((cp - 0x10000) >> 10);
                    let lo = 0xDC00 + ((cp - 0x10000) & 0x3FF);
                    result.push_str(&format!("\\u{hi:04X}\\u{lo:04X}"));
                } else {
                    result.push_str(&format!("\\u{cp:04X}"));
                }
            }
            _ => result.push(c),
        }
    }
    result
}

/// Decode HTML/XML entities in JSX text.
/// Handles named entities (&amp; &lt; &gt; &quot; &middot; &hellip; etc.),
/// numeric decimal (&#123;), and hex (&#x7D;) references.
/// Unknown named entities are left as-is (e.g. &notAnEntity;).
fn decode_jsx_entities(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            // Collect entity body until ';' or non-entity char
            let mut body = String::new();
            let mut found_semi = false;
            while let Some(&next) = chars.peek() {
                if next == ';' {
                    chars.next();
                    found_semi = true;
                    break;
                }
                if next.is_alphanumeric() || next == '#' {
                    body.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            if found_semi {
                if let Some(decoded) = resolve_entity(&body) {
                    result.push_str(&decoded);
                } else {
                    // Unknown entity -- leave as-is
                    result.push('&');
                    result.push_str(&body);
                    result.push(';');
                }
            } else {
                result.push('&');
                result.push_str(&body);
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Resolve a single HTML entity body (without & and ;) to its character(s).
fn resolve_entity(body: &str) -> Option<String> {
    // Numeric: &#123; or &#x7D;
    if let Some(num_part) = body.strip_prefix('#') {
        let cp = if num_part.starts_with('x') || num_part.starts_with('X') {
            u32::from_str_radix(&num_part[1..], 16).ok()?
        } else {
            num_part.parse::<u32>().ok()?
        };
        return char::from_u32(cp).map(|c| c.to_string());
    }
    // Named entities
    let c = match body {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{00A0}',
        "middot" => '\u{00B7}',
        "mdash" => '\u{2014}',
        "ndash" => '\u{2013}',
        "hellip" => '\u{2026}',
        "laquo" => '\u{00AB}',
        "raquo" => '\u{00BB}',
        "bull" => '\u{2022}',
        "copy" => '\u{00A9}',
        "reg" => '\u{00AE}',
        "trade" => '\u{2122}',
        "hearts" => '\u{2665}',
        "larr" => '\u{2190}',
        "rarr" => '\u{2192}',
        "uarr" => '\u{2191}',
        "darr" => '\u{2193}',
        _ => return None,
    };
    Some(c.to_string())
}

/// Check if a property name needs quoting in an object literal.
fn needs_quoting(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    // Names with colons (namespaced), hyphens, or starting with digits need quoting
    name.contains(':') || name.contains('-') || name.starts_with(|c: char| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use crate::output::printer::{PrintOptions, Printer};
    use tsz_parser::ParserState;

    fn emit_jsx(source: &str) -> String {
        let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        printer.finish().code
    }

    #[test]
    fn self_closing_no_attributes_has_space_before_slash() {
        let output = emit_jsx("const x = <Tag />;");
        assert!(
            output.contains("<Tag />"),
            "Self-closing element without attributes should have space before />.\nOutput: {output}"
        );
    }

    #[test]
    fn self_closing_with_attributes_has_no_space_before_slash() {
        let output = emit_jsx("const x = <Tag foo=\"bar\"/>;");
        assert!(
            output.contains("<Tag foo=\"bar\"/>"),
            "Self-closing element with attributes should NOT have extra space before />.\nOutput: {output}"
        );
    }

    #[test]
    fn self_closing_with_expression_attribute_no_extra_space() {
        let output = emit_jsx("const x = <Tag value={42}/>;");
        assert!(
            output.contains("<Tag value={42}/>"),
            "Self-closing element with expression attribute should NOT have extra space before />.\nOutput: {output}"
        );
    }

    #[test]
    fn jsx_text_multiline_content_preserves_whitespace() {
        // tsc preserves JSX text content including leading/trailing whitespace and newlines.
        // The scanner's re_scan_jsx_token must reset to full_start_pos (before trivia)
        // so the text node captures the complete whitespace content.
        let source = "let k1 = <Comp a={10} b=\"hi\">\n        hi hi hi!\n    </Comp>;";
        let output = emit_jsx(source);
        assert!(
            output.contains("\n        hi hi hi!\n    "),
            "JSX text should preserve leading/trailing whitespace and newlines.\nOutput: {output}"
        );
    }

    #[test]
    fn jsx_text_single_line_content() {
        let output = emit_jsx("let x = <div>hello world</div>;");
        assert!(
            output.contains(">hello world</"),
            "JSX text on single line should be preserved.\nOutput: {output}"
        );
    }

    #[test]
    fn jsx_text_with_nested_elements() {
        let source = "let x = <Comp>\n        <div>inner</div>\n    </Comp>;";
        let output = emit_jsx(source);
        assert!(
            output.contains("\n        <div>inner</div>\n    "),
            "JSX text whitespace around nested elements should be preserved.\nOutput: {output}"
        );
    }

    #[test]
    fn jsx_text_whitespace_only_between_elements() {
        // Whitespace-only text nodes between JSX elements should be preserved
        let source = "let x = <div>\n    <span>a</span>\n    <span>b</span>\n</div>;";
        let output = emit_jsx(source);
        assert!(
            output.contains("<span>a</span>\n    <span>b</span>"),
            "Whitespace between JSX children should be preserved.\nOutput: {output}"
        );
    }

    #[test]
    fn jsx_expression_with_trailing_comment_in_expression_is_preserved() {
        let source = "let x = <div>{null/* preserved */}</div>;";
        let output = emit_jsx(source);
        assert!(
            output.contains("/* preserved */"),
            "Trailing comment inside JSX expression should be preserved.\nOutput: {output}"
        );
        assert!(
            !output.contains("{null}"),
            "Trailing comment should not be dropped from JSX expression.\nOutput: {output}"
        );
    }

    #[test]
    fn jsx_expression_without_expression_preserves_inner_comments() {
        let source = "let x = <div>{\n    // ???\n}</div>;";
        let output = emit_jsx(source);
        assert!(
            output.contains("// ???"),
            "Line comment inside a comment-only JSX expression should be preserved.\nOutput: {output}"
        );
        assert!(
            output.contains("{\n    // ???"),
            "Comment should remain inside JSX expression braces with preserved newline.\nOutput: {output}"
        );
        assert!(
            output.contains("{\n    // ???\n}"),
            "Expression comment should keep surrounding braces and newline.\nOutput: {output}"
        );
    }

    #[test]
    fn jsx_expression_without_expression_normalizes_multiline_leading_comment_indentation() {
        let source = "let x = <div>{\n    // ??? 1\n            // ??? 2\n}</div>;";
        let output = emit_jsx(source);
        assert!(
            output.contains("{\n    // ??? 1\n    // ??? 2\n}"),
            "Comment-only JSX expression lines should normalize leading indentation uniformly.\nOutput: {output}"
        );
    }

    #[test]
    fn jsx_expression_inline_block_comment_keeps_spacing() {
        let source = "let x = <div>{\n    // ???\n/* ??? */}</div>;";
        let output = emit_jsx(source);
        assert!(
            output.contains("/* ??? */ }"),
            "Trailing inline block comment inside JSX expression should keep leading space before closing brace.\nOutput: {output}"
        );
    }
}
