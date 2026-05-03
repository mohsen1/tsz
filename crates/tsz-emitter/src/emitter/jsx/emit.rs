use super::super::Printer;
use super::super::core::JsxEmit;
use super::{
    AttrGroup, JsxAttrInfo, JsxAttrValue, JsxAttrsInfo, JsxChildSep, decode_jsx_entities,
    escape_jsx_text_for_js_with_quote, needs_quoting, process_jsx_text,
};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    // =========================================================================
    // JSX - Preserve Mode (default)
    // =========================================================================

    pub(in super::super) fn emit_jsx_element(&mut self, node: &Node) {
        match self.ctx.options.jsx {
            JsxEmit::React => self.emit_jsx_element_classic(node),
            JsxEmit::ReactJsx | JsxEmit::ReactJsxDev => self.emit_jsx_element_automatic(node),
            _ => self.emit_jsx_element_preserve(node),
        }
    }

    pub(in super::super) fn emit_jsx_self_closing_element(&mut self, node: &Node) {
        match self.ctx.options.jsx {
            JsxEmit::React => self.emit_jsx_self_closing_classic(node),
            JsxEmit::ReactJsx | JsxEmit::ReactJsxDev => {
                self.emit_jsx_self_closing_automatic(node);
            }
            _ => self.emit_jsx_self_closing_preserve(node),
        }
    }

    pub(in super::super) fn emit_jsx_fragment(&mut self, node: &Node) {
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
        let has_synthesized_empty_close = self
            .arena
            .get(jsx.closing_element)
            .and_then(|closing_node| {
                self.arena
                    .get_jsx_closing(closing_node)
                    .map(|closing| (closing_node, closing))
            })
            .is_some_and(|(_closing_node, closing)| closing.tag_name.is_none());
        for &child in &jsx.children.nodes {
            // tsc strips empty JSX expression containers `{}` that have no
            // inner comments in preserve mode.  Expressions with comments
            // like `{/* comment */}` are kept.
            if self.is_empty_jsx_expression_without_comments(child) {
                continue;
            }
            if has_synthesized_empty_close
                && self
                    .arena
                    .get(child)
                    .and_then(|child_node| self.arena.get_jsx_text(child_node))
                    .is_some_and(|text| {
                        text.contains_only_trivia_white_spaces || text.text.trim().is_empty()
                    })
            {
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

        // Children -- use multiline when multiple children or any child is a JSX element
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

        // Children -- tsc formats children on separate indented lines when there are
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

        // Props (including key, NOT children) -- classic createElement style
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
    pub(in super::super) fn get_jsx_factory(&self) -> String {
        self.ctx
            .options
            .jsx_factory
            .as_deref()
            .unwrap_or("React.createElement")
            .to_string()
    }

    /// Get the JSX fragment factory (e.g. "React.Fragment" or custom)
    pub(in super::super) fn get_jsx_fragment_factory(&self) -> String {
        self.ctx
            .options
            .jsx_fragment_factory
            .as_deref()
            .unwrap_or("React.Fragment")
            .to_string()
    }

    /// Emit a JSX tag name as a function argument.
    /// Intrinsic elements (lowercase) -> string literal.
    /// Component elements (uppercase/dotted/namespaced) -> identifier/expression.
    pub(in super::super) fn emit_jsx_tag_name_as_argument(&mut self, tag_name: NodeIndex) {
        let Some(node) = self.arena.get(tag_name) else {
            self.write("\"\"");
            return;
        };

        if node.is_identifier() {
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

        // Namespaced tag name (e.g. `<svg:path>`) -> emit as quoted string `"svg:path"`
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
            self.write("\"");
            self.write(ns_text);
            self.write(":");
            self.write(name_text);
            self.write("\"");
            return;
        }

        // Property access (e.g. Foo.Bar) or other expression -- emit as-is
        self.emit(tag_name);
    }

    /// Collect attributes from a JSX attributes node, returning info about each attribute.
    pub(in super::super) fn collect_jsx_attributes_info(
        &self,
        attributes: NodeIndex,
    ) -> JsxAttrsInfo {
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
                        // Attribute without value (e.g. `<div disabled />`) -> true
                        JsxAttrValue::Bool(true)
                    };
                    result.attrs.push(JsxAttrInfo::Named { name, value });
                }
            } else if prop_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE
                && let Some(spread) = self.arena.get_jsx_spread_attribute(prop_node)
            {
                result.has_spread = true;
                // Flatten spread of object literal with only spread properties:
                // {...{...a, ...b}} -> ...a, ...b
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
    /// JSX spread flattening: `{...{...a, ...b}}` -> `...a, ...b`.
    pub(in super::super) fn get_spread_only_object_literal(
        &self,
        expr: NodeIndex,
    ) -> Option<Vec<NodeIndex>> {
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
    pub(in super::super) fn can_inline_jsx_spread_object(&self, expr: NodeIndex) -> bool {
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
                    // is safe to inline (it doesn't set the prototype -- it creates
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
    pub(in super::super) fn is_literal_proto_name(&self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }
        if name_node.is_identifier()
            && let Some(ident) = self.arena.get_identifier(name_node)
        {
            return ident.escaped_text == "__proto__";
        }
        if name_node.is_string_literal()
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
    pub(in super::super) fn merge_inlinable_spread_groups(
        &self,
        groups: Vec<AttrGroup>,
    ) -> Vec<AttrGroup> {
        // Check if any Spread group is NOT inlinable (a "real" spread).
        let has_real_spread = groups.iter().any(
            |g| matches!(g, AttrGroup::Spread(expr) if !self.can_inline_jsx_spread_object(*expr)),
        );

        if has_real_spread {
            // Don't inline any spreads -- keep them all as Spread.
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
    pub(in super::super) fn emit_jsx_inline_object_literal_props(
        &mut self,
        expr: NodeIndex,
        first: &mut bool,
    ) {
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
    pub(in super::super) fn get_jsx_attr_name(&self, name_idx: NodeIndex) -> String {
        let Some(node) = self.arena.get(name_idx) else {
            return String::new();
        };

        if node.is_identifier()
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
    pub(in super::super) fn get_jsx_attr_value_info(&self, init_idx: NodeIndex) -> JsxAttrValue {
        let Some(node) = self.arena.get(init_idx) else {
            return JsxAttrValue::Bool(true);
        };

        // String literal (e.g. `"hello"`) -- preserve original node for quote fidelity
        if node.is_string_literal() {
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
    pub(in super::super) fn collect_jsx_children(&self, children: &[NodeIndex]) -> Vec<NodeIndex> {
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
    pub(in super::super) fn skip_comments_for_empty_jsx_expr(&mut self, node: &Node) {
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
    pub(in super::super) fn skip_empty_jsx_children_comments(&mut self, children: &[NodeIndex]) {
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
    pub(in super::super) fn is_empty_jsx_expression(&self, child: NodeIndex) -> bool {
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
            self.write("\"");
            self.write(&escape_jsx_text_for_js_with_quote(&decoded, '"'));
            self.write("\"");
            return;
        }

        if node.kind == syntax_kind_ext::JSX_EXPRESSION
            && let Some(expr) = self.arena.get_jsx_expression(node)
            && expr.expression.is_some()
        {
            // Spread children in classic mode: `{...expr}` -> `...expr`
            if expr.dot_dot_dot_token {
                self.write("...");
            }
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

        // JSX element, fragment, or self-closing element -- emit recursively
        // This will hit the transform dispatch again for nested JSX.
        self.emit(child);
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

    /// Emit a property name, quoting if needed.
    pub(in super::super) fn emit_jsx_prop_name(&mut self, name: &str) {
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
            JsxAttrValue::Expr(idx) => {
                self.emit(*idx);
            }
        }
    }
}
