use super::super::Printer;
use super::super::core::JsxEmit;
use super::{AttrGroup, JsxAttrInfo, JsxUsage, extract_jsx_import_source, group_jsx_attrs};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> Printer<'a> {
    /// Emit classic spread attrs: handles mixes of spread and named attrs.
    /// Uses `Object.assign` when there are spreads mixed with named props.
    pub(in super::super) fn emit_jsx_spread_attrs_classic(&mut self, attrs: &[JsxAttrInfo]) {
        // Group consecutive named attrs and emit them as object literals,
        // interleaved with spread expressions.
        // ES2018+: { a: "1", ...x, b: "2" } (inline spread)
        // ES2015-ES2017: Object.assign({a: "1"}, x, {b: "2"})
        let groups = group_jsx_attrs(attrs);
        let groups = self.merge_inlinable_spread_groups(groups);

        // When all groups are Named or InlinedObjectLiteral (no real Spread),
        // we can emit them as a single object literal -- no Object.assign needed.
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
    /// tsc emits `{ named, ...spread, children }` -- a single object literal.
    pub(in super::super) fn emit_jsx_spread_attrs_automatic(
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
        // `{...{__proto__}}` -> `{ className: "T", __proto__, children: ... }`.
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

    pub(in super::super) fn emit_jsx_opening_element(&mut self, node: &Node) {
        let Some(jsx) = self.arena.get_jsx_opening(node) else {
            return;
        };

        self.write("<");
        self.emit(jsx.tag_name);
        self.emit(jsx.attributes);
        self.write(">");
    }

    pub(in super::super) fn emit_jsx_closing_element(&mut self, node: &Node) {
        let Some(jsx) = self.arena.get_jsx_closing(node) else {
            return;
        };

        self.write("</");
        self.emit(jsx.tag_name);
        self.write(">");
    }

    pub(in super::super) fn emit_jsx_attributes(&mut self, node: &Node) {
        let Some(attrs) = self.arena.get_jsx_attributes(node) else {
            return;
        };

        for &attr in &attrs.properties.nodes {
            self.write_space();
            self.emit(attr);
        }
    }

    pub(in super::super) fn emit_jsx_attribute(&mut self, node: &Node) {
        let Some(attr) = self.arena.get_jsx_attribute(node) else {
            return;
        };

        self.emit(attr.name);
        if attr.initializer.is_some() {
            self.write("=");
            self.emit(attr.initializer);
        }
    }

    pub(in super::super) fn emit_jsx_spread_attribute(&mut self, node: &Node) {
        let Some(spread) = self.arena.get_jsx_spread_attribute(node) else {
            return;
        };

        self.write("{...");
        self.emit(spread.expression);
        self.write("}");
    }

    pub(in super::super) fn emit_jsx_expression(&mut self, node: &Node) {
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

    pub(in super::super) fn emit_jsx_text(&mut self, node: &Node) {
        let Some(text) = self.arena.get_jsx_text(node) else {
            return;
        };

        self.write(&text.text);
    }

    pub(in super::super) fn emit_jsx_namespaced_name(&mut self, node: &Node) {
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
    pub(in super::super) fn source_line_col_pos(&self, pos: u32) -> (u32, u32) {
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
    /// e.g., "react/jsx-runtime" -> "`jsx_runtime_1`", "react/jsx-dev-runtime" -> "`jsx_dev_runtime_1`"
    pub(in super::super) fn jsx_cjs_runtime_var(&self) -> String {
        let suffix = match self.ctx.options.jsx {
            JsxEmit::ReactJsxDev => "jsx-dev-runtime",
            _ => "jsx-runtime",
        };
        let sanitized = crate::transforms::emit_utils::sanitize_module_name(suffix);
        format!("{sanitized}_1")
    }

    /// Get the CJS variable name for the base JSX module import.
    /// Used for createElement fallback when key appears after spread.
    /// e.g., "react" -> "`react_1`", "preact" -> "`preact_1`"
    pub(in super::super) fn jsx_cjs_base_var(&self) -> String {
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
    pub(in super::super) fn extract_jsx_import_source_pragma(&self) -> Option<String> {
        let text = self.source_text?;
        extract_jsx_import_source(text)
    }

    /// Check if the file needs JSX runtime auto-imports and return the import text.
    /// Called at the start of source file emission for jsx=react-jsx/react-jsxdev.
    /// Only imports the functions that are actually used in the file.
    pub(in super::super) fn jsx_auto_import_text(&self) -> Option<String> {
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
    pub(in super::super) fn scan_jsx_usage(&self) -> JsxUsage {
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
                    // Check for key-after-spread -> createElement fallback
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
                        // Check for key-after-spread -> createElement fallback
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
    pub(in super::super) fn jsx_attrs_has_key_after_spread(&self, attributes: NodeIndex) -> bool {
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
