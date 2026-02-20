//! Helper methods for the IR printer.
//!
//! Contains single-line detection, ES5 function body emission with default
//! parameters, multiline comment formatting, and arrow function ES5 emission.

use super::*;

impl<'a> IRPrinter<'a> {
    /// Check if a body source range represents a single-line block in the source text.
    /// Uses brace depth counting to find the matching `}` and skips leading trivia.
    /// Check if a source range is on a single line (for object literals, etc.)
    pub(super) fn is_single_line_range(&self, pos: u32, end: u32) -> bool {
        self.source_text.is_none_or(|text| {
            let start = pos as usize;
            let end = std::cmp::min(end as usize, text.len());
            if start < end {
                let slice = &text[start..end];
                !slice.contains('\n')
            } else {
                true // Empty range is considered single-line
            }
        }) // Default to single-line if no source text
    }

    pub(super) fn is_body_source_single_line(&self, body_source_range: Option<(u32, u32)>) -> bool {
        body_source_range
            .and_then(|(pos, end)| {
                self.source_text.map(|text| {
                    let start = pos as usize;
                    let end = std::cmp::min(end as usize, text.len());
                    if start < end {
                        let slice = &text[start..end];
                        if let Some(open) = slice.find('{') {
                            let mut depth = 1;
                            for (i, ch) in slice[open + 1..].char_indices() {
                                match ch {
                                    '{' => depth += 1,
                                    '}' => {
                                        depth -= 1;
                                        if depth == 0 {
                                            let inner = &slice[open..open + 1 + i + 1];
                                            return !inner.contains('\n');
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        !slice.contains('\n')
                    } else {
                        false
                    }
                })
            })
            .unwrap_or(false)
    }

    /// Emit function body with default parameter checks prepended (ES5 style)
    pub(super) fn emit_function_body_with_defaults(
        &mut self,
        params: &[IRParam],
        body: &[IRNode],
        body_source_range: Option<(u32, u32)>,
        force_multiline_empty_body: bool,
    ) {
        // Check if any params have defaults
        let has_defaults = params.iter().any(|p| p.default_value.is_some());

        // Check if the body was single-line in the source
        let is_body_source_single_line = self.is_body_source_single_line(body_source_range);

        // Empty body with no defaults: emit as single-line `{ }` if:
        // - source was single-line, OR
        // - there's no source range (synthetic/generated code like abstract accessor transforms).
        // TSC preserves multiline formatting from source but uses single-line for generated code.
        // Exception: IIFE constructors (force_multiline_empty_body) always need multiline.
        if !has_defaults && body.is_empty() {
            let use_single_line = !force_multiline_empty_body
                && (is_body_source_single_line || body_source_range.is_none());
            if use_single_line {
                self.write("{ }");
            } else {
                self.write("{");
                self.write_line();
                self.write_indent();
                self.write("}");
            }
            return;
        }

        // Single statement with no defaults: emit as single-line if source was single-line,
        // unless caller forced multiline style (used for class constructors in ES5 class IIFEs).
        if !has_defaults
            && body.len() == 1
            && is_body_source_single_line
            && !force_multiline_empty_body
        {
            self.write("{ ");
            self.emit_node(&body[0]);
            self.write(" }");
            return;
        }

        // Multi-line body (either has defaults, multiple statements, or wasn't single-line in source)
        self.write("{");
        self.write_line();
        self.increase_indent();

        // Emit default parameter checks: if (param === void 0) { param = default; }
        for param in params {
            if let Some(default) = &param.default_value {
                self.write_indent();
                self.write("if (");
                self.write(&param.name);
                self.write(" === void 0) { ");
                self.write(&param.name);
                self.write(" = ");
                self.emit_node(default);
                self.write("; }");
                self.write_line();
            }
        }

        // Emit the rest of the body
        for stmt in body {
            self.write_indent();
            self.emit_node(stmt);
            self.write_line();
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
    }

    pub(super) fn emit_comma_separated(&mut self, nodes: &[IRNode]) {
        for (i, node) in nodes.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.emit_node(node);
        }
    }

    pub(super) fn emit_object_literal_multiline(&mut self, properties: &[IRProperty]) {
        if properties.is_empty() {
            self.write("{}");
            return;
        }
        self.write("{");
        self.write_line();
        self.indent_level += 1;
        for (i, prop) in properties.iter().enumerate() {
            self.write_indent();
            self.emit_property(prop);
            if i < properties.len() - 1 {
                self.write(",");
            }
            self.write_line();
        }
        self.indent_level -= 1;
        self.write_indent();
        self.write("}");
    }

    pub(super) fn is_done_value_object_literal(properties: &[IRProperty]) -> bool {
        if properties.len() != 2 {
            return false;
        }
        let mut has_done = false;
        let mut has_value = false;
        for prop in properties {
            match (&prop.key, prop.kind) {
                (IRPropertyKey::Identifier(name), IRPropertyKind::Init) if name == "done" => {
                    has_done = true;
                }
                (IRPropertyKey::Identifier(name), IRPropertyKind::Init) if name == "value" => {
                    has_value = true;
                }
                _ => return false,
            }
        }
        has_done && has_value
    }

    pub(super) fn emit_parameters(&mut self, params: &[IRParam]) {
        for (i, param) in params.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            if param.rest {
                self.write("...");
            }
            self.write(&param.name);
        }
    }

    pub(super) fn emit_property(&mut self, prop: &IRProperty) {
        // Special case: spread property (key is "..." and value is SpreadElement)
        // Should emit as `...expr` not `"...": ...expr`
        if let IRPropertyKey::Identifier(name) = &prop.key
            && name == "..."
            && let IRNode::SpreadElement(inner) = &prop.value
        {
            self.write("...");
            self.emit_node(inner);
            return;
        }

        match &prop.key {
            IRPropertyKey::Identifier(name) => self.write(name),
            IRPropertyKey::StringLiteral(s) => {
                self.write("\"");
                self.write_escaped(s);
                self.write("\"");
            }
            IRPropertyKey::NumericLiteral(n) => self.write(n),
            IRPropertyKey::Computed(expr) => {
                self.write("[");
                self.emit_node(expr);
                self.write("]");
            }
        }

        match prop.kind {
            IRPropertyKind::Init => {
                self.write(": ");
                self.emit_node(&prop.value);
            }
            IRPropertyKind::Get | IRPropertyKind::Set => {
                self.write(" ");
                self.emit_node(&prop.value);
            }
        }
    }

    pub(super) fn emit_method_name(&mut self, name: &IRMethodName) {
        match name {
            IRMethodName::Identifier(n) => {
                self.write(".");
                self.write(n);
            }
            IRMethodName::StringLiteral(s) => {
                self.write("[\"");
                self.write_escaped(s);
                self.write("\"]");
            }
            IRMethodName::NumericLiteral(n) => {
                self.write("[");
                self.write(n);
                self.write("]");
            }
            IRMethodName::Computed(expr) => {
                self.write("[");
                self.emit_node(expr);
                self.write("]");
            }
        }
    }

    pub(super) fn emit_switch_case(&mut self, case: &IRSwitchCase) {
        self.write_indent();
        if let Some(test) = &case.test {
            self.write("case ");
            self.emit_node(test);
            self.write(":");
        } else {
            self.write("default:");
        }
        self.write_line();

        self.increase_indent();
        for stmt in &case.statements {
            self.write_indent();
            self.emit_node(stmt);
            self.write_line();
        }
        self.decrease_indent();
    }

    pub(super) fn write(&mut self, s: &str) {
        self.output.push_str(s);
    }

    pub(super) fn write_escaped(&mut self, s: &str) {
        for c in s.chars() {
            match c {
                '"' => self.output.push_str("\\\""),
                '\\' => self.output.push_str("\\\\"),
                '\n' => self.output.push_str("\\n"),
                '\r' => self.output.push_str("\\r"),
                '\t' => self.output.push_str("\\t"),
                '\0' => self.output.push_str("\\0"),
                c if (c as u32) < 0x20 || c == '\x7F' => {
                    // Escape control characters as \u00NN (matching TypeScript format)
                    write!(self.output, "\\u{:04X}", c as u32).unwrap();
                }
                _ => self.output.push(c),
            }
        }
    }

    pub(super) fn write_line(&mut self) {
        self.output.push('\n');
    }

    pub(super) fn write_indent(&mut self) {
        for _ in 0..self.indent_level {
            self.output.push_str(self.indent_str);
        }
    }

    pub(super) const fn increase_indent(&mut self) {
        self.indent_level += 1;
    }

    pub(super) const fn decrease_indent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    /// Emit a multiline comment with proper indentation for each line.
    /// Normalizes indentation to match TypeScript's output format:
    /// - First line: current indentation + comment start (`/**`)
    /// - Subsequent lines: current indentation + ` *` or ` */`
    pub(super) fn emit_multiline_comment(&mut self, comment: &str) {
        let mut first = true;
        for line in comment.split('\n') {
            if !first {
                self.write_line();
                self.write_indent();
            }
            // Strip leading whitespace, then add one space before * or */
            let trimmed = line.trim_start();
            if !first && (trimmed.starts_with('*') || trimmed.starts_with('/')) {
                self.write(" ");
            }
            self.write(trimmed.trim_end());
            first = false;
        }
    }
}

impl Default for IRPrinter<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> IRPrinter<'a> {
    /// Emit an arrow function as ES5 function expression using directive flags
    /// Transforms: () => expr  →  function () { return expr; }
    ///
    /// This is the NEW implementation that:
    /// 1. Uses flags from `TransformDirective` (doesn't re-calculate)
    /// 2. Uses recursive `emit_node` calls for the body (handles nested directives)
    /// 3. Supports `class_alias` for static class members
    pub(super) fn emit_arrow_function_es5_with_flags(
        &mut self,
        arena: &NodeArena,
        func: &tsz_parser::parser::node::FunctionData,
    ) {
        use tsz_parser::parser::syntax_kind_ext;

        // Arrow functions are transformed to regular function expressions.
        // `this` capture is handled by `var _this = this;` at the enclosing
        // function scope. The lowering pass marks `this` references with
        // SubstituteThis to emit `_this` instead.

        self.write("function ");

        // Parameters
        self.write("(");
        let params = &func.parameters.nodes;
        for (i, &param_idx) in params.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            if let Some(param_node) = arena.get(param_idx)
                && let Some(_param) = arena.get_parameter(param_node)
                && let Some(ident) = arena.get_identifier(param_node)
            {
                self.write(&ident.escaped_text);
            }
        }
        self.write(") ");

        // Body - use recursive emit_node to handle nested directives
        let body_node = arena.get(func.body);
        let is_block = body_node.is_some_and(|n| n.kind == syntax_kind_ext::BLOCK);

        if is_block {
            // Block body - emit recursively to handle nested transforms
            self.emit_node(&IRNode::ASTRef(func.body));
        } else {
            // Concise body - wrap with return and emit recursively
            // If body resolves to an object literal, wrap in parens
            let needs_parens = Self::concise_body_needs_parens(arena, func.body);
            if needs_parens {
                self.write("{ return (");
                self.emit_node(&IRNode::ASTRef(func.body));
                self.write("); }");
            } else {
                self.write("{ return ");
                self.emit_node(&IRNode::ASTRef(func.body));
                self.write("; }");
            }
        }
    }

    /// Check if a concise arrow body resolves to an object literal expression
    /// and needs wrapping in parens. Returns false if already parenthesized.
    pub(super) fn concise_body_needs_parens(arena: &NodeArena, body_idx: NodeIndex) -> bool {
        let mut idx = body_idx;
        loop {
            let Some(node) = arena.get(idx) else {
                return false;
            };
            match node.kind {
                k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => return true,
                k if k == syntax_kind_ext::TYPE_ASSERTION
                    || k == syntax_kind_ext::AS_EXPRESSION =>
                {
                    if let Some(ta) = arena.get_type_assertion(node) {
                        idx = ta.expression;
                    } else {
                        return false;
                    }
                }
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => return false,
                _ => return false,
            }
        }
    }
}
