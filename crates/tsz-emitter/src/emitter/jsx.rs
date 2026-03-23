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
            // tsc strips empty JSX expression containers `{}` that have no
            // inner comments in preserve mode.  Expressions with comments
            // like `{/* comment */}` are kept.
            if self.is_empty_jsx_expression_without_comments(child) {
                continue;
            }
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
            if self.is_empty_jsx_expression_without_comments(child) {
                continue;
            }
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

        // Children — use multiline when multiple children or any child is a JSX element
        let filtered_children = self.collect_jsx_children(&children);
        let has_jsx_child = filtered_children.iter().any(|&idx| {
            self.arena.get(idx).is_some_and(|n| {
                n.kind == syntax_kind_ext::JSX_ELEMENT
                    || n.kind == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
                    || n.kind == syntax_kind_ext::JSX_FRAGMENT
            })
        });
        let multiline = filtered_children.len() > 1 || has_jsx_child;
        if multiline {
            self.write(",");
            self.increase_indent();
        }
        let sep = if multiline {
            JsxChildSep::CommaNewline
        } else {
            JsxChildSep::CommaSpace
        };
        self.emit_jsx_children_interleaved(&children, &filtered_children, sep);
        self.write(")");
        if multiline {
            self.decrease_indent();
        }
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

        // Children — tsc formats children on separate indented lines when there are
        // multiple children OR when any child is itself a JSX element (nested createElement).
        let has_jsx_child = filtered_children.iter().any(|&idx| {
            self.arena.get(idx).is_some_and(|n| {
                n.kind == syntax_kind_ext::JSX_ELEMENT
                    || n.kind == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
                    || n.kind == syntax_kind_ext::JSX_FRAGMENT
            })
        });
        let multiline_children = filtered_children.len() > 1 || has_jsx_child;
        if multiline_children {
            self.write(",");
            self.increase_indent();
        }
        let child_sep = if multiline_children {
            JsxChildSep::CommaNewline
        } else {
            JsxChildSep::CommaSpace
        };
        self.emit_jsx_children_interleaved(children, &filtered_children, child_sep);
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
        let element_pos = opening_node.pos;

        self.emit_jsx_automatic_call(tag_name, attributes, &children, element_pos);
    }

    fn emit_jsx_self_closing_automatic(&mut self, node: &Node) {
        let Some(jsx) = self.arena.get_jsx_opening(node) else {
            return;
        };

        let tag_name = jsx.tag_name;
        let attributes = jsx.attributes;
        let element_pos = node.pos;

        self.emit_jsx_automatic_call(tag_name, attributes, &[], element_pos);
    }

    fn emit_jsx_fragment_automatic(&mut self, node: &Node) {
        let Some(jsx) = self.arena.get_jsx_fragment(node) else {
            return;
        };

        let children: Vec<NodeIndex> = jsx.children.nodes.to_vec();
        let filtered_children = self.collect_jsx_children(&children);
        let is_jsxs = filtered_children.len() > 1;
        let is_cjs = self.ctx.is_effectively_commonjs();
        let is_dev = matches!(self.ctx.options.jsx, JsxEmit::ReactJsxDev);
        let func_name = if is_dev {
            "jsxDEV"
        } else if is_jsxs {
            "jsxs"
        } else {
            "jsx"
        };

        if is_cjs {
            let var_name = self.jsx_cjs_runtime_var();
            self.write(&format!("(0, {var_name}.{func_name})("));
        } else {
            self.write(&format!("_{func_name}("));
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
            let child_sep = if is_jsxs {
                JsxChildSep::CommaBetween
            } else {
                JsxChildSep::None
            };
            if is_jsxs {
                self.write("[");
            }
            self.emit_jsx_children_interleaved(&children, &filtered_children, child_sep);
            if is_jsxs {
                self.write("]");
            }
            self.write(" ");
        } else {
            self.skip_empty_jsx_children_comments(&children);
        }
        self.write("}");

        if is_dev {
            // jsxDEV extra args: key, isStaticChildren, source, self
            self.write(", void 0, ");
            self.write(if is_jsxs { "true" } else { "false" });
            // Source location for fragment - use the node's own position
            let (line, col) = self.source_line_col_pos(node.pos);
            self.write(&format!(
                ", {{ fileName: _jsxFileName, lineNumber: {line}, columnNumber: {col} }}"
            ));
            self.write(", this");
        }

        self.write(")");
    }

    /// Emit `_jsx(tag, { ...props, children })` or `_jsxs(tag, { ...props, children: [...] })`
    fn emit_jsx_automatic_call(
        &mut self,
        tag_name: NodeIndex,
        attributes: NodeIndex,
        children: &[NodeIndex],
        element_pos: u32,
    ) {
        // Key-after-spread: fall back to createElement from the base module
        if self.jsx_attrs_has_key_after_spread(attributes) {
            self.emit_jsx_create_element_fallback(tag_name, attributes, children);
            return;
        }

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

        let is_cjs = self.ctx.is_effectively_commonjs();
        let is_dev = matches!(self.ctx.options.jsx, JsxEmit::ReactJsxDev);
        let func_name = if is_dev {
            "jsxDEV"
        } else if is_jsxs {
            "jsxs"
        } else {
            "jsx"
        };

        if is_cjs {
            // CJS: (0, jsx_runtime_1.jsx)(tag, props)
            let var_name = self.jsx_cjs_runtime_var();
            self.write(&format!("(0, {var_name}.{func_name})("));
        } else {
            // ESM: _jsxDEV( / _jsx( / _jsxs(
            self.write(&format!("_{func_name}("));
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
                let child_sep = if is_jsxs {
                    JsxChildSep::CommaBetween
                } else {
                    JsxChildSep::None
                };
                if is_jsxs {
                    self.write("[");
                }
                self.emit_jsx_children_interleaved(children, &filtered_children, child_sep);
                if is_jsxs {
                    self.write("]");
                }
            } else {
                self.skip_empty_jsx_children_comments(children);
            }
            self.write(" }");
        }

        if is_dev {
            // jsxDEV extra args: key, isStaticChildren, source, self
            self.write(", ");
            if let Some(JsxAttrInfo::Named { value, .. }) = &key_attr {
                self.emit_jsx_attr_value(value);
            } else {
                self.write("void 0");
            }
            self.write(", ");
            self.write(if is_jsxs { "true" } else { "false" });
            // Source location: { fileName: _jsxFileName, lineNumber: N, columnNumber: N }
            let (line, col) = self.source_line_col_pos(element_pos);
            self.write(&format!(
                ", {{ fileName: _jsxFileName, lineNumber: {line}, columnNumber: {col} }}"
            ));
            self.write(", this");
        } else {
            // Key as third argument (for automatic transform)
            if let Some(JsxAttrInfo::Named { value, .. }) = &key_attr {
                self.write(", ");
                self.emit_jsx_attr_value(value);
            }
        }

        self.write(")");
    }

    /// Emit `createElement(tag, Object.assign({}, ...props, { key }), ...children)`
    /// Used when key appears after a spread attribute (key-after-spread fallback).
    fn emit_jsx_create_element_fallback(
        &mut self,
        tag_name: NodeIndex,
        attributes: NodeIndex,
        children: &[NodeIndex],
    ) {
        let attrs_info = self.collect_jsx_attributes_info(attributes);
        let filtered_children = self.collect_jsx_children(children);
        let is_cjs = self.ctx.is_effectively_commonjs();

        if is_cjs {
            let base_var = self.jsx_cjs_base_var();
            self.write(&format!("(0, {base_var}.createElement)("));
        } else {
            self.write("_createElement(");
        }

        // Tag name
        self.emit_jsx_tag_name_as_argument(tag_name);

        // Props (including key, NOT children) — classic createElement style
        self.write(", ");
        if attrs_info.attrs.is_empty() && !attrs_info.has_spread {
            self.write("null");
        } else if attrs_info.has_spread {
            // Use classic spread emission which includes key in the Object.assign
            self.emit_jsx_spread_attrs_classic(&attrs_info.attrs);
        } else {
            self.emit_jsx_attrs_as_object(&attrs_info.attrs);
        }

        // Children as separate args (classic style)
        self.emit_jsx_children_interleaved(children, &filtered_children, JsxChildSep::CommaSpace);

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
                // Flatten spread of object literal with only spread properties:
                // {...{...a, ...b}} → ...a, ...b
                if let Some(inner_spreads) = self.get_spread_only_object_literal(spread.expression)
                {
                    for inner_expr in inner_spreads {
                        result.attrs.push(JsxAttrInfo::Spread { expr: inner_expr });
                    }
                } else {
                    result.attrs.push(JsxAttrInfo::Spread {
                        expr: spread.expression,
                    });
                }
            }
        }

        result
    }

    /// Check if a node is an `ObjectLiteralExpression` with only `SpreadAssignment`
    /// properties. If so, return the expression of each spread. This enables
    /// JSX spread flattening: `{...{...a, ...b}}` → `...a, ...b`.
    fn get_spread_only_object_literal(&self, expr: NodeIndex) -> Option<Vec<NodeIndex>> {
        let node = self.arena.get(expr)?;
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let lit = self.arena.get_literal_expr(node)?;
        if lit.elements.nodes.is_empty() {
            return None;
        }
        let mut result = Vec::new();
        for &prop in &lit.elements.nodes {
            let prop_node = self.arena.get(prop)?;
            if prop_node.kind != syntax_kind_ext::SPREAD_ASSIGNMENT {
                return None;
            }
            let spread = self.arena.get_spread(prop_node)?;
            result.push(spread.expression);
        }
        Some(result)
    }

    /// Check if an object literal expression can be safely inlined into a
    /// parent object without needing spread wrapping.
    fn can_inline_jsx_spread_object(&self, expr: NodeIndex) -> bool {
        let Some(node) = self.arena.get(expr) else {
            return false;
        };
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(lit) = self.arena.get_literal_expr(node) else {
            return false;
        };
        if lit.elements.nodes.is_empty() {
            return false;
        }
        for &prop in &lit.elements.nodes {
            let Some(prop_node) = self.arena.get(prop) else {
                return false;
            };
            match prop_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    if let Some(pa) = self.arena.get_property_assignment(prop_node)
                        && self.is_literal_proto_name(pa.name)
                    {
                        return false;
                    }
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    // Shorthand `{__proto__}` is `{__proto__: __proto__}` which
                    // is safe to inline (it doesn't set the prototype — it creates
                    // a property named __proto__ with the variable's value).
                    // Only non-shorthand `{__proto__: value}` is dangerous.
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR => {}
                k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                    return false;
                }
                _ => {
                    return false;
                }
            }
        }
        true
    }

    /// Check if a property name is the literal (non-computed) `__proto__`.
    fn is_literal_proto_name(&self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }
        if name_node.kind == SyntaxKind::Identifier as u16
            && let Some(ident) = self.arena.get_identifier(name_node)
        {
            return ident.escaped_text == "__proto__";
        }
        if name_node.kind == SyntaxKind::StringLiteral as u16
            && let Some(lit) = self.arena.get_literal(name_node)
        {
            return lit.text == "__proto__";
        }
        false
    }

    /// Merge Spread groups with inlinable object literals into `InlinedObjectLiteral`.
    ///
    /// tsc only inlines spread object literals when there are no "real"
    /// (non-inlinable) spread groups. When real spreads like `...a` exist,
    /// object-literal spreads like `...{c, d}` are kept as-is to match tsc output.
    fn merge_inlinable_spread_groups(&self, groups: Vec<AttrGroup>) -> Vec<AttrGroup> {
        // Check if any Spread group is NOT inlinable (a "real" spread).
        let has_real_spread = groups.iter().any(
            |g| matches!(g, AttrGroup::Spread(expr) if !self.can_inline_jsx_spread_object(*expr)),
        );

        if has_real_spread {
            // Don't inline any spreads — keep them all as Spread.
            return groups;
        }

        groups
            .into_iter()
            .map(|g| match g {
                AttrGroup::Spread(expr) if self.can_inline_jsx_spread_object(expr) => {
                    AttrGroup::InlinedObjectLiteral(expr)
                }
                other => other,
            })
            .collect()
    }

    /// Emit object literal properties inline into a parent object literal.
    fn emit_jsx_inline_object_literal_props(&mut self, expr: NodeIndex, first: &mut bool) {
        let Some(node) = self.arena.get(expr) else {
            return;
        };
        let Some(lit) = self.arena.get_literal_expr(node) else {
            return;
        };
        for &prop in &lit.elements.nodes {
            if !*first {
                self.write(", ");
            }
            *first = false;
            self.emit(prop);
        }
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

    /// Skip comments for a single empty JSX expression node.
    /// Uses the full node range since the comment IS the content.
    fn skip_comments_for_empty_jsx_expr(&mut self, node: &Node) {
        while self.comment_emit_idx < self.all_comments.len() {
            let c = &self.all_comments[self.comment_emit_idx];
            if c.pos >= node.pos && c.end <= node.end {
                self.comment_emit_idx += 1;
            } else {
                break;
            }
        }
    }

    /// Skip comments for all empty JSX expression containers in a children list.
    /// Iterates children in source order to maintain monotonic `comment_emit_idx`.
    fn skip_empty_jsx_children_comments(&mut self, children: &[NodeIndex]) {
        for &child in children {
            if self.is_empty_jsx_expression(child)
                && let Some(node) = self.arena.get(child)
            {
                self.skip_comments_for_empty_jsx_expr(node);
            }
        }
    }

    /// Check if a JSX child is an empty expression container (no expression,
    /// only comments/whitespace).
    fn is_empty_jsx_expression(&self, child: NodeIndex) -> bool {
        let Some(node) = self.arena.get(child) else {
            return false;
        };
        if node.kind != syntax_kind_ext::JSX_EXPRESSION {
            return false;
        }
        self.arena
            .get_jsx_expression(node)
            .is_some_and(|e| e.expression.is_none())
    }

    /// Check if a JSX child is a truly empty expression container `{}` with no
    /// inner comments.  Used in preserve mode to strip bare `{}` from JSX output
    /// (matching tsc behavior) while keeping `{/* comment */}` intact.
    fn is_empty_jsx_expression_without_comments(&self, child: NodeIndex) -> bool {
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
        // Check that there are no comments inside the expression range
        !self
            .all_comments
            .iter()
            .any(|c| c.pos >= node.pos && c.end <= node.end)
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
    fn emit_jsx_children_interleaved(
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
        // interleaved with spread expressions.
        // ES2018+: { a: "1", ...x, b: "2" } (inline spread)
        // ES2015-ES2017: Object.assign({a: "1"}, x, {b: "2"})
        let groups = group_jsx_attrs(attrs);
        let groups = self.merge_inlinable_spread_groups(groups);

        // When all groups are Named or InlinedObjectLiteral (no real Spread),
        // we can emit them as a single object literal — no Object.assign needed.
        // This matches tsc behavior for `{...{__proto__}}` and similar patterns.
        let all_inlinable = groups.iter().all(|g| !matches!(g, AttrGroup::Spread(_)));
        if all_inlinable && groups.len() > 1 {
            self.write("{ ");
            let mut first = true;
            for group in &groups {
                match group {
                    AttrGroup::Named(named) => {
                        for attr in named {
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
                    }
                    AttrGroup::InlinedObjectLiteral(expr) => {
                        self.emit_jsx_inline_object_literal_props(*expr, &mut first);
                    }
                    AttrGroup::Spread(_) => unreachable!(),
                }
            }
            self.write(" }");
            return;
        }

        if groups.len() == 1 {
            match &groups[0] {
                AttrGroup::Named(named) => self.emit_jsx_attrs_as_object(named),
                AttrGroup::Spread(expr) | AttrGroup::InlinedObjectLiteral(expr) => self.emit(*expr),
            }
        } else if !self.ctx.needs_es2018_lowering {
            // ES2018+: inline spread syntax
            self.write("{ ");
            let mut first = true;
            for group in &groups {
                match group {
                    AttrGroup::Named(named) => {
                        for attr in named {
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
                    }
                    AttrGroup::Spread(expr) => {
                        if !first {
                            self.write(", ");
                        }
                        first = false;
                        self.write("...");
                        self.emit(*expr);
                    }
                    AttrGroup::InlinedObjectLiteral(expr) => {
                        self.emit_jsx_inline_object_literal_props(*expr, &mut first);
                    }
                }
            }
            self.write(" }");
        } else {
            // ES2015-ES2017: Object.assign
            self.write("Object.assign(");
            for (i, group) in groups.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                match group {
                    AttrGroup::Named(named) => self.emit_jsx_attrs_as_object(named),
                    AttrGroup::Spread(expr) | AttrGroup::InlinedObjectLiteral(expr) => {
                        self.emit(*expr)
                    }
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
        if self.ctx.needs_es2018_lowering {
            self.emit_jsx_spread_attrs_object_assign(attrs, children, is_jsxs);
            return;
        }

        // tsc only inlines spread object literals when there are no "real"
        // (non-inlinable) spreads among the attrs. This matches the classic
        // path's `merge_inlinable_spread_groups` behavior.
        let has_real_spread = attrs.iter().any(|a| {
            matches!(a, JsxAttrInfo::Spread { expr } if !self.can_inline_jsx_spread_object(*expr))
        });

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
                    if !has_real_spread && self.can_inline_jsx_spread_object(*expr) {
                        self.emit_jsx_inline_object_literal_props(*expr, &mut first);
                    } else {
                        if !first {
                            self.write(", ");
                        }
                        first = false;
                        self.write("...");
                        self.emit(*expr);
                    }
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

    /// Emit JSX spread props using `Object.assign` for targets below ES2018.
    ///
    /// Groups consecutive named props into object literals, spreads become
    /// separate arguments, and children is always a final `{ children: ... }`.
    /// E.g., `<div className="T1" {...a}>T1</div>` becomes:
    ///   `Object.assign({ className: "T1" }, a, { children: "T1" })`
    fn emit_jsx_spread_attrs_object_assign(
        &mut self,
        attrs: &[JsxAttrInfo],
        children: &[NodeIndex],
        is_jsxs: bool,
    ) {
        // When all spread attrs are inlinable (no "real" spreads), tsc emits
        // a single object literal instead of Object.assign. E.g.,
        // `{...{__proto__}}` → `{ className: "T", __proto__, children: ... }`.
        let has_real_spread = attrs.iter().any(|a| {
            matches!(a, JsxAttrInfo::Spread { expr } if !self.can_inline_jsx_spread_object(*expr))
        });
        if !has_real_spread {
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
                        self.emit_jsx_inline_object_literal_props(*expr, &mut first);
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
            return;
        }

        // Segment attrs into groups of consecutive Named and individual Spreads
        enum Segment {
            Named(usize, usize), // start..end indices into attrs
            Spread(usize),       // index into attrs
        }

        let mut segments: Vec<Segment> = Vec::new();
        for (i, attr) in attrs.iter().enumerate() {
            match attr {
                JsxAttrInfo::Named { .. } => {
                    if let Some(Segment::Named(_, end)) = segments.last_mut() {
                        *end = i + 1;
                    } else {
                        segments.push(Segment::Named(i, i + 1));
                    }
                }
                JsxAttrInfo::Spread { .. } => {
                    segments.push(Segment::Spread(i));
                }
            }
        }

        self.write("Object.assign(");

        // If first segment is a Spread (or no segments), start with empty object
        let starts_with_spread = matches!(segments.first(), Some(Segment::Spread(_)));
        if starts_with_spread {
            self.write("{}, ");
        }

        let mut first_seg = true;
        for seg in &segments {
            if !first_seg {
                self.write(", ");
            }
            first_seg = false;
            match seg {
                Segment::Named(start, end) => {
                    self.write("{ ");
                    let mut first_prop = true;
                    for attr in &attrs[*start..*end] {
                        if let JsxAttrInfo::Named { name, value } = attr {
                            if !first_prop {
                                self.write(", ");
                            }
                            first_prop = false;
                            self.emit_jsx_prop_name(name);
                            self.write(": ");
                            self.emit_jsx_attr_value(value);
                        }
                    }
                    self.write(" }");
                }
                Segment::Spread(idx) => {
                    if let JsxAttrInfo::Spread { expr } = &attrs[*idx] {
                        self.emit(*expr);
                    }
                }
            }
        }

        // Children as a final separate argument
        if !children.is_empty() {
            if !first_seg || starts_with_spread {
                self.write(", ");
            }
            self.write("{ children: ");
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
            self.write(" }");
        }

        self.write(")");
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
            if has_comment && last_comment_has_newline {
                // When the last comment had a trailing newline, the writer is at
                // line-start and will use ensure_indent() for the closing `}`.
                // Write `}` before decreasing indent so it aligns with the `{`.
                self.write("}");
                self.decrease_indent();
                return;
            }
            self.decrease_indent();
            if has_comment
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
    // JSX Dev Mode Source Location Helpers
    // =========================================================================

    /// Compute 1-based (line, column) from a raw source position.
    fn source_line_col_pos(&self, pos: u32) -> (u32, u32) {
        let Some(text) = self.source_text else {
            return (1, 1);
        };
        let bytes = text.as_bytes();
        let pos = (pos as usize).min(bytes.len());
        let mut line = 1u32;
        let mut col = 1u32;
        for &b in &bytes[..pos] {
            if b == b'\n' {
                line += 1;
                col = 1;
            } else if b != b'\r' {
                col += 1;
            }
        }
        (line, col)
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

    /// Get the CJS variable name for the base JSX module import.
    /// Used for createElement fallback when key appears after spread.
    /// e.g., "react" → "`react_1`", "preact" → "`preact_1`"
    fn jsx_cjs_base_var(&self) -> String {
        let pragma_source = self.extract_jsx_import_source_pragma();
        let source = pragma_source
            .as_deref()
            .or(self.ctx.options.jsx_import_source.as_deref())
            .unwrap_or("react");
        let sanitized = crate::transforms::emit_utils::sanitize_module_name(source);
        format!("{sanitized}_1")
    }

    /// Extract `@jsxImportSource <package>` pragma from the file's leading comments.
    /// Returns the package name (e.g. `"preact"`) or `None` if no pragma found.
    pub(super) fn extract_jsx_import_source_pragma(&self) -> Option<String> {
        let text = self.source_text?;
        extract_jsx_import_source(text)
    }

    /// Check if the file needs JSX runtime auto-imports and return the import text.
    /// Called at the start of source file emission for jsx=react-jsx/react-jsxdev.
    /// Only imports the functions that are actually used in the file.
    pub(super) fn jsx_auto_import_text(&self) -> Option<String> {
        let is_cjs = self.ctx.is_effectively_commonjs();
        // Per-file @jsxImportSource pragma overrides the global option
        let pragma_source = self.extract_jsx_import_source_pragma();
        match self.ctx.options.jsx {
            JsxEmit::ReactJsx => {
                let source = pragma_source
                    .as_deref()
                    .or(self.ctx.options.jsx_import_source.as_deref())
                    .unwrap_or("react");
                let usage = self.scan_jsx_usage();
                if !usage.needs_jsx
                    && !usage.needs_jsxs
                    && !usage.needs_fragment
                    && !usage.needs_create_element
                {
                    return None;
                }
                if is_cjs {
                    let mut text = String::new();
                    // Base module import for createElement fallback (key-after-spread)
                    if usage.needs_create_element {
                        let base_var = self.jsx_cjs_base_var();
                        text.push_str(&format!("const {base_var} = require(\"{source}\");\n"));
                    }
                    if usage.needs_jsx || usage.needs_jsxs || usage.needs_fragment {
                        let var_name = self.jsx_cjs_runtime_var();
                        text.push_str(&format!(
                            "const {var_name} = require(\"{source}/jsx-runtime\");\n"
                        ));
                    }
                    Some(text)
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
                    let mut text = String::new();
                    if usage.needs_create_element {
                        text.push_str(&format!(
                            "import {{ createElement as _createElement }} from \"{source}\";\n"
                        ));
                    }
                    if !imports.is_empty() {
                        text.push_str(&format!(
                            "import {{ {} }} from \"{source}/jsx-runtime\";\n",
                            imports.join(", ")
                        ));
                    }
                    Some(text)
                }
            }
            JsxEmit::ReactJsxDev => {
                let source = pragma_source
                    .as_deref()
                    .or(self.ctx.options.jsx_import_source.as_deref())
                    .unwrap_or("react");
                let usage = self.scan_jsx_usage();
                if !usage.needs_jsx
                    && !usage.needs_jsxs
                    && !usage.needs_fragment
                    && !usage.needs_create_element
                {
                    return None;
                }
                let file_name_line = self
                    .jsx_dev_file_name
                    .as_deref()
                    .map(|f| format!("const _jsxFileName = \"{f}\";\n"))
                    .unwrap_or_default();
                if is_cjs {
                    let mut text = String::new();
                    // Base module import for createElement fallback (key-after-spread)
                    if usage.needs_create_element {
                        let base_var = self.jsx_cjs_base_var();
                        text.push_str(&format!("const {base_var} = require(\"{source}\");\n"));
                    }
                    if usage.needs_jsx || usage.needs_jsxs || usage.needs_fragment {
                        let var_name = self.jsx_cjs_runtime_var();
                        text.push_str(&format!(
                            "const {var_name} = require(\"{source}/jsx-dev-runtime\");\n"
                        ));
                    }
                    text.push_str(&file_name_line);
                    Some(text)
                } else {
                    let mut imports = Vec::new();
                    if usage.needs_jsx || usage.needs_jsxs {
                        imports.push("jsxDEV as _jsxDEV");
                    }
                    if usage.needs_fragment {
                        imports.push("Fragment as _Fragment");
                    }
                    let mut text = String::new();
                    if usage.needs_create_element {
                        text.push_str(&format!(
                            "import {{ createElement as _createElement }} from \"{source}\";\n"
                        ));
                    }
                    if !imports.is_empty() {
                        text.push_str(&format!(
                            "import {{ {} }} from \"{source}/jsx-dev-runtime\";\n",
                            imports.join(", ")
                        ));
                    }
                    text.push_str(&file_name_line);
                    Some(text)
                }
            }
            _ => None,
        }
    }

    /// Scan the AST to determine which JSX runtime functions are needed.
    pub(super) fn scan_jsx_usage(&self) -> JsxUsage {
        let mut usage = JsxUsage {
            needs_jsx: false,
            needs_jsxs: false,
            needs_fragment: false,
            needs_create_element: false,
        };
        for i in 0..self.arena.len() {
            let nidx = tsz_parser::parser::NodeIndex(i as u32);
            let Some(node) = self.arena.get(nidx) else {
                continue;
            };
            match node.kind {
                k if k == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT => {
                    // Check for key-after-spread → createElement fallback
                    if let Some(jsx) = self.arena.get_jsx_opening(node) {
                        if self.jsx_attrs_has_key_after_spread(jsx.attributes) {
                            usage.needs_create_element = true;
                        } else {
                            usage.needs_jsx = true;
                        }
                    } else {
                        usage.needs_jsx = true;
                    }
                }
                k if k == syntax_kind_ext::JSX_ELEMENT => {
                    if let Some(jsx) = self.arena.get_jsx_element(node) {
                        // Check for key-after-spread → createElement fallback
                        let has_kas = self
                            .arena
                            .get(jsx.opening_element)
                            .and_then(|o| self.arena.get_jsx_opening(o))
                            .is_some_and(|o| self.jsx_attrs_has_key_after_spread(o.attributes));
                        if has_kas {
                            usage.needs_create_element = true;
                        } else {
                            let children = self.collect_jsx_children(&jsx.children.nodes);
                            if children.len() > 1 {
                                usage.needs_jsxs = true;
                            } else {
                                usage.needs_jsx = true;
                            }
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

    /// Check if a JSX attributes node has a key attribute after a spread attribute.
    /// When this pattern occurs, tsc falls back to createElement from the base module.
    fn jsx_attrs_has_key_after_spread(&self, attributes: NodeIndex) -> bool {
        let Some(attrs_node) = self.arena.get(attributes) else {
            return false;
        };
        let Some(attrs) = self.arena.get_jsx_attributes(attrs_node) else {
            return false;
        };
        let mut seen_spread = false;
        for &prop in &attrs.properties.nodes {
            let Some(prop_node) = self.arena.get(prop) else {
                continue;
            };
            if prop_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                seen_spread = true;
            } else if prop_node.kind == syntax_kind_ext::JSX_ATTRIBUTE
                && seen_spread
                && let Some(attr) = self.arena.get_jsx_attribute(prop_node)
            {
                let name = self.get_jsx_attr_name(attr.name);
                if name == "key" {
                    return true;
                }
            }
        }
        false
    }
}

// =============================================================================
// Pragma extraction
// =============================================================================

/// Extract `@jsxImportSource <package>` from leading block comments.
/// Mirrors tsc behavior: only block comments before any code are scanned.
fn extract_jsx_import_source(source: &str) -> Option<String> {
    let scan_limit = source.len().min(4096);
    let text = &source[..scan_limit];
    let bytes = text.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() {
        if bytes[pos].is_ascii_whitespace() {
            pos += 1;
            continue;
        }
        if pos + 1 < bytes.len() && bytes[pos] == b'/' && bytes[pos + 1] == b'*' {
            let comment_start = pos + 2;
            if let Some(end_offset) = text[comment_start..].find("*/") {
                let comment_body = &text[comment_start..comment_start + end_offset];
                if let Some(idx) = comment_body.find("@jsxImportSource") {
                    let after = &comment_body[idx + "@jsxImportSource".len()..];
                    let pkg: String = after
                        .trim_start()
                        .chars()
                        .take_while(|c| {
                            c.is_alphanumeric()
                                || *c == '_'
                                || *c == '-'
                                || *c == '/'
                                || *c == '@'
                                || *c == '.'
                        })
                        .collect();
                    if !pkg.is_empty() {
                        return Some(pkg);
                    }
                }
                pos = comment_start + end_offset + 2;
            } else {
                break;
            }
            continue;
        }
        if pos + 1 < bytes.len() && bytes[pos] == b'/' && bytes[pos + 1] == b'/' {
            if let Some(nl) = text[pos..].find('\n') {
                pos += nl + 1;
            } else {
                break;
            }
            continue;
        }
        break;
    }
    None
}

// =============================================================================
// Internal Data Types
// =============================================================================

pub(super) struct JsxUsage {
    pub(super) needs_jsx: bool,
    pub(super) needs_jsxs: bool,
    pub(super) needs_fragment: bool,
    pub(super) needs_create_element: bool,
}

#[derive(Clone)]
/// Separator style for JSX child emission.
enum JsxChildSep {
    /// `, ` before every child (classic createElement separate args)
    CommaSpace,
    /// newline before every child, `,` after all but last (multiline classic)
    CommaNewline,
    /// `, ` only between children (automatic `children: [a, b]`)
    CommaBetween,
    /// No separator (single-child automatic)
    None,
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
    /// An object literal from a spread that can be safely inlined.
    InlinedObjectLiteral(NodeIndex),
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
        // The comment should appear after `{` on a new line, and the closing `}`
        // should align with the comment (both at the increased indent level).
        assert!(
            output.contains("{") && output.contains("// ???") && output.contains("}"),
            "Comment should remain inside JSX expression braces.\nOutput: {output}"
        );
        // Closing `}` should be on its own line after the comment (not on the
        // same line), matching tsc's output for JSX expression comments.
        let comment_idx = output.find("// ???").unwrap();
        let after_comment = &output[comment_idx..];
        assert!(
            after_comment.contains('\n'),
            "There should be a newline after the comment before the closing brace.\nOutput: {output}"
        );
        let closing_brace = after_comment.find('}').unwrap();
        let between = &after_comment[..closing_brace];
        assert!(
            between.contains('\n'),
            "Closing brace should be on a separate line from the comment.\nOutput: {output}"
        );
    }

    #[test]
    fn jsx_expression_without_expression_normalizes_multiline_leading_comment_indentation() {
        let source = "let x = <div>{\n    // ??? 1\n            // ??? 2\n}</div>;";
        let output = emit_jsx(source);
        // Both comments should appear in the output and the closing `}` should
        // follow on its own line.
        assert!(
            output.contains("// ??? 1") && output.contains("// ??? 2"),
            "Both comment lines should be preserved.\nOutput: {output}"
        );
        // The two comments should be on separate lines with uniform indentation
        let idx1 = output.find("// ??? 1").unwrap();
        let idx2 = output.find("// ??? 2").unwrap();
        assert!(
            output[idx1..idx2].contains('\n'),
            "Comment-only JSX expression lines should be on separate lines.\nOutput: {output}"
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
