use super::super::Printer;
use crate::transforms::private_fields_es5::get_private_field_name;
use tsz_parser::parser::{
    NodeIndex,
    node::{AccessExprData, Node, NodeAccess},
    syntax_kind_ext,
};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(super) fn emit_scoped_static_super_receiver(&mut self) {
        if let Some(alias) = self.scoped_static_this_alias.as_ref().cloned() {
            self.write(&alias);
        } else {
            self.write("this");
        }
    }

    pub(super) fn emit_scoped_static_super_property_name(&mut self, name_idx: NodeIndex) {
        let Some(name_node) = self.arena.get(name_idx) else {
            self.write("\"\"");
            return;
        };

        match name_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(identifier) = self.arena.get_identifier(name_node) {
                    self.emit_string_literal_text(&identifier.escaped_text);
                } else {
                    self.write("\"\"");
                }
            }
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                if let Some(text) = self.arena.get_literal_text(name_idx) {
                    self.emit_string_literal_text(text);
                } else {
                    self.write("\"\"");
                }
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                self.emit(name_idx);
            }
            _ => self.emit(name_idx),
        }
    }

    pub(in crate::emitter) fn emit_property_access(&mut self, node: &Node) {
        let Some(access) = self.arena.get_access_expr(node) else {
            return;
        };

        if let Some(base_alias) = self.scoped_static_super_base_alias.as_ref().cloned()
            && let Some(base_node) = self.arena.get(access.expression)
            && base_node.kind == SyntaxKind::SuperKeyword as u16
        {
            if self.scoped_static_super_direct_access {
                self.write(&base_alias);
                self.write(".");
                self.emit_property_name_without_import_substitution(access.name_or_argument);
                return;
            }
            self.write("Reflect.get(");
            self.write(&base_alias);
            self.write(", ");
            self.emit_scoped_static_super_property_name(access.name_or_argument);
            self.write(", ");
            self.emit_scoped_static_super_receiver();
            self.write(")");
            return;
        }

        // Private field lowering: `this.#field` → `__classPrivateFieldGet(this, _C_field, "f")`
        if !self.private_field_weakmaps.is_empty()
            && let Some(name_node) = self.arena.get(access.name_or_argument)
            && name_node.kind == SyntaxKind::PrivateIdentifier as u16
            && let Some(field_name) = get_private_field_name(self.arena, access.name_or_argument)
        {
            let clean_name = field_name.strip_prefix('#').unwrap_or(&field_name);
            if let Some(weakmap_name) = self.private_field_weakmaps.get(clean_name).cloned() {
                self.write_helper("__classPrivateFieldGet");
                self.write("(");
                self.emit(access.expression);
                self.write(", ");
                // For methods/accessors/static fields, use the state_var instead of weakmap_name
                if let Some(info) = self.private_member_info.get(clean_name).cloned() {
                    if let Some(ref state_var) = info.state_var {
                        self.write(state_var);
                    } else {
                        self.write(&weakmap_name);
                    }
                    self.write(", \"");
                    self.write(info.kind);
                    self.write("\"");
                    if let Some(ref fn_ref) = info.fn_ref {
                        self.write(", ");
                        self.write(fn_ref);
                    }
                } else {
                    self.write(&weakmap_name);
                    self.write(", \"f\"");
                }
                self.write(")");
                return;
            }
        }

        // Const enum inlining: replace `EnumName.Member` with `value /* EnumName.Member */`
        if !self.const_enum_values.is_empty()
            && let Some(inlined) = self.try_inline_const_enum_property_access(access)
        {
            self.write(&inlined);
            return;
        }

        if access.question_dot_token {
            if self.ctx.options.target.supports_es2020() {
                self.emit_optional_property_access(access, "?.");
            } else {
                self.emit_optional_property_access_downlevel(access);
            }
            return;
        }

        if self.emit_parenthesized_object_literal_access(access.expression, |this| {
            this.write_dot_token(access.expression);
            this.emit_property_name_without_import_substitution(access.name_or_argument);
        }) {
            return;
        }

        // Wrap negative const enum inline values in parens to avoid
        // ambiguity: `(-1 /* Foo.A */).toString()` vs `-1.toString()`.
        let needs_negative_parens =
            self.expression_is_negative_const_enum_inline(access.expression);

        if needs_negative_parens {
            self.write("(");
        }

        // In System.register modules, `import.meta` is replaced with `context_1.meta`.
        // The parser represents `import.meta` as PropertyAccessExpression with
        // expression=ImportKeyword, name=meta.
        if self.in_system_execute_body
            && let Some(expr_node) = self.arena.get(access.expression)
            && expr_node.kind == tsz_scanner::SyntaxKind::ImportKeyword as u16
        {
            self.write("context_1");
            self.write_dot_token(access.expression);
            self.emit_property_name_without_import_substitution(access.name_or_argument);
            return;
        }

        // Signal that the expression is in access position so `emit_parenthesized`
        // preserves parens around `new` expressions: `(new a).b` vs `new a.b`.
        let prev = self.paren_in_access_position;
        self.paren_in_access_position = true;
        // When the base expression is ExpressionWithTypeArguments (e.g.,
        // `List<number>.makeChild()`), unwrap it without parens since the
        // property access already provides grouping: emit `List.makeChild()`
        // not `(List).makeChild()`.
        self.emit_unwrapping_type_args(access.expression);
        self.paren_in_access_position = prev;

        if needs_negative_parens {
            self.write(")");
        }

        // Preserve multi-line property access chains from the original source.
        // TypeScript preserves the original line break pattern. If there's a
        // newline between expression end and the property name, we need to
        // reproduce the original layout:
        // - If dot is before newline: `expr.\n    name` -> emit ".\n    name"
        // - If dot is after newline: `expr\n    .name` -> emit "\n    .name"
        if let Some(text) = self.source_text
            && let Some(expr_node) = self.arena.get(access.expression)
            && let Some(name_node) = self.arena.get(access.name_or_argument)
        {
            let expr_end = expr_node.end as usize;
            let name_start = name_node.pos as usize;
            let between_end = std::cmp::min(name_start, text.len());
            let between_start = std::cmp::min(expr_end, between_end);
            let between = &text[between_start..between_end];
            if between.contains('\n') {
                // Find where the dot is relative to the newline
                if let Some(dot_pos) = between.find('.') {
                    let after_dot = &between[dot_pos + 1..];
                    if after_dot.contains('\n') {
                        // Dot before newline: `expr.\n    name`
                        self.write_dot_token(access.expression);
                        self.write_line();
                        self.increase_indent();
                        self.emit_property_name_without_import_substitution(
                            access.name_or_argument,
                        );
                        self.decrease_indent();
                    } else {
                        // Newline before dot: `expr\n    .name`
                        self.write_line();
                        self.increase_indent();
                        self.write_dot_token(access.expression);
                        self.emit_property_name_without_import_substitution(
                            access.name_or_argument,
                        );
                        self.decrease_indent();
                    }
                } else {
                    self.write_dot_token(access.expression);
                    self.emit_property_name_without_import_substitution(access.name_or_argument);
                }
                return;
            }
        }

        // Map the `.` token to its source position
        if let Some(expr_node) = self.arena.get(access.expression) {
            self.map_token_after(expr_node.end, node.end, b'.');
        }
        self.write_dot_token(access.expression);
        // When the property name is missing (error recovery, e.g. `bar.\n}`),
        // tsc emits the dot followed by a newline so the expression statement's
        // semicolon ends up on its own line: `bar.\n    ;`
        if access.name_or_argument.is_none() {
            self.write_line();
            return;
        }
        self.emit_property_name_without_import_substitution(access.name_or_argument);
    }

    /// Write the `.` token for property access, adding an extra `.` when the
    /// expression is a numeric literal without a decimal point or exponent.
    /// Without this, `0.toString()` would be parsed as the float `0.` followed
    /// by `toString()`, which is a syntax error.  tsc emits `0..toString()`.
    pub(in crate::emitter) fn write_dot_token(&mut self, expr_idx: NodeIndex) {
        // Unwrap parentheses, type assertions, and `as` expressions to find the
        // innermost expression. After type erasure, `(<any>1)` becomes just `1`.
        let mut idx = expr_idx;
        let mut is_parenthesized = false;
        while let Some(node) = self.arena.get(idx) {
            match node.kind {
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    is_parenthesized = true;
                    if let Some(paren) = self.arena.get_parenthesized(node) {
                        idx = paren.expression;
                        continue;
                    }
                }
                k if k == syntax_kind_ext::TYPE_ASSERTION
                    || k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
                {
                    // Type assertions are fully erased during emit — they and
                    // any surrounding parens do NOT survive in the output.
                    // `(<any>1).foo` becomes `1.foo` which needs `1..foo`.
                    // Reset is_parenthesized since the parens are erased too.
                    is_parenthesized = false;
                    if let Some(assert) = self.arena.get_type_assertion(node) {
                        idx = assert.expression;
                        continue;
                    }
                }
                _ => {}
            }
            break;
        }
        if let Some(inner_node) = self.arena.get(idx) {
            if inner_node.kind == SyntaxKind::NumericLiteral as u16 {
                // Check the source text of the numeric literal to see if it already
                // contains a decimal point or exponent.  If not, we need "..".
                let needs_extra_dot = if let Some(text) = self.source_text {
                    let start = inner_node.pos as usize;
                    let end = inner_node.end as usize;
                    if end <= text.len() && start < end {
                        let lit = &text[start..end];
                        let num_text = lit.trim();
                        // Only plain decimal integers need `..`.
                        // Hex (0x/0X), octal (0o/0O), and binary (0b/0B) never
                        // need it because their prefix already disambiguates.
                        let is_prefixed = num_text.starts_with("0x")
                            || num_text.starts_with("0X")
                            || num_text.starts_with("0o")
                            || num_text.starts_with("0O")
                            || num_text.starts_with("0b")
                            || num_text.starts_with("0B");
                        !is_prefixed
                            && !num_text.contains('.')
                            && !num_text.contains('e')
                            && !num_text.contains('E')
                    } else {
                        false
                    }
                } else {
                    false
                };
                // If the numeric literal is wrapped in parentheses (or a type
                // assertion that was erased), the parens already disambiguate and
                // a single dot suffices: `(1).toString()` not `(1)..toString()`.
                if needs_extra_dot && !is_parenthesized {
                    self.write("..");
                    return;
                }
            }
            // After const enum inlining, the expression is still a PropertyAccess/
            // ElementAccess AST node but the output is a plain integer like `100`.
            // We need `100..toString()`, not `100.toString()` (syntax error).
            // Only needed when comments are removed — with comments the inline
            // comment `/* Foo.X */` separates the number from the dot.
            if self.ctx.options.remove_comments
                && !is_parenthesized
                && self.resolve_const_enum_needs_double_dot(idx, inner_node)
            {
                self.write("..");
                return;
            }
        }
        self.write(".");
    }

    pub(in crate::emitter) fn emit_element_access(&mut self, node: &Node) {
        let Some(access) = self.arena.get_access_expr(node) else {
            return;
        };

        if let Some(base_alias) = self.scoped_static_super_base_alias.as_ref().cloned()
            && let Some(base_node) = self.arena.get(access.expression)
            && base_node.kind == SyntaxKind::SuperKeyword as u16
        {
            if self.scoped_static_super_direct_access {
                self.write(&base_alias);
                self.write("[");
                self.emit(access.name_or_argument);
                self.write("]");
                return;
            }
            self.write("Reflect.get(");
            self.write(&base_alias);
            self.write(", ");
            self.emit(access.name_or_argument);
            self.write(", ");
            self.emit_scoped_static_super_receiver();
            self.write(")");
            return;
        }

        // Const enum inlining: replace `EnumName["Member"]` with `value /* EnumName["Member"] */`
        if !self.const_enum_values.is_empty()
            && let Some(inlined) = self.try_inline_const_enum_element_access(access)
        {
            self.write(&inlined);
            return;
        }

        if access.question_dot_token {
            if self.ctx.options.target.supports_es2020() {
                self.emit(access.expression);
                self.write("?.[");
                self.emit(access.name_or_argument);
                self.write("]");
            } else {
                self.emit_optional_element_access_downlevel(access);
            }
            return;
        }

        if self.emit_parenthesized_object_literal_access(access.expression, |this| {
            this.write("[");
            this.emit(access.name_or_argument);
            this.write("]");
        }) {
            return;
        }

        let prev = self.paren_in_access_position;
        self.paren_in_access_position = true;
        self.emit(access.expression);
        self.paren_in_access_position = prev;
        self.write("[");
        self.emit(access.name_or_argument);
        self.write("]");
    }

    fn emit_parenthesized_object_literal_access<F>(
        &mut self,
        expr: NodeIndex,
        emit_suffix: F,
    ) -> bool
    where
        F: FnOnce(&mut Self),
    {
        let Some(expr_node) = self.arena.get(expr) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            return false;
        }
        let Some(paren) = self.arena.get_parenthesized(expr_node) else {
            return false;
        };
        if !self.type_assertion_wraps_object_literal(paren.expression) {
            return false;
        }

        // Determine if the paren wraps a type assertion (which gets erased) or
        // directly wraps an object literal.
        // - Type assertion case: `(<Type>{}).foo` → `({}.foo)` — suffix stays inside parens
        //   because the outer statement-level paren provides the disambiguation.
        // - Direct object literal: `({...})['hello']` → `({...})['hello']` — suffix goes
        //   outside since the parens just wrap the object literal.
        let inner_is_erasable = if let Some(inner) = self.arena.get(paren.expression) {
            inner.kind == syntax_kind_ext::TYPE_ASSERTION
                || inner.kind == syntax_kind_ext::AS_EXPRESSION
                || inner.kind == syntax_kind_ext::SATISFIES_EXPRESSION
                || inner.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS
        } else {
            false
        };

        self.write("(");
        self.emit(paren.expression);
        if inner_is_erasable {
            emit_suffix(self);
            self.write(")");
        } else {
            self.write(")");
            emit_suffix(self);
        }
        true
    }

    fn emit_optional_property_access(&mut self, access: &AccessExprData, token: &str) {
        if let Some(text) = self.source_text
            && let Some(expr_node) = self.arena.get(access.expression)
            && let Some(name_node) = self.arena.get(access.name_or_argument)
        {
            let expr_end = expr_node.end as usize;
            let name_start = name_node.pos as usize;
            let between_end = std::cmp::min(name_start, text.len());
            let between_start = std::cmp::min(expr_end, between_end);
            let between = &text[between_start..between_end];
            if between.contains('\n')
                && let Some(dot_pos) = between.find('.')
            {
                let after_dot = &between[dot_pos + 1..];
                if after_dot.contains('\n') {
                    self.emit(access.expression);
                    self.write(token);
                    self.write_line();
                    self.increase_indent();
                    self.emit_property_name_without_import_substitution(access.name_or_argument);
                    self.decrease_indent();
                    return;
                }
                self.emit(access.expression);
                self.write(token);
                self.emit_property_name_without_import_substitution(access.name_or_argument);
                return;
            }
        }

        self.emit(access.expression);
        self.write(token);
        self.emit_property_name_without_import_substitution(access.name_or_argument);
    }

    fn emit_optional_property_access_downlevel(&mut self, access: &AccessExprData) {
        // When the lowered ternary appears inside a prefix/postfix unary or
        // conditional condition, wrap in parens to preserve precedence.
        let needs_parens = self.ctx.flags.optional_chain_needs_parens;
        if needs_parens {
            self.write("(");
            self.ctx.flags.optional_chain_needs_parens = false;
        }
        let base_simple = self.is_simple_nullish_expression(access.expression);
        if base_simple {
            self.emit(access.expression);
            self.write(" === null || ");
            self.emit(access.expression);
            self.write(" === void 0 ? void 0 : ");
            self.emit(access.expression);
            self.write(".");
            self.emit_property_name_without_import_substitution(access.name_or_argument);
        } else {
            let base_temp = self.make_unique_name_hoisted();
            self.write("(");
            self.write(&base_temp);
            self.write(" = ");
            self.emit(access.expression);
            self.write(")");
            self.write(" === null || ");
            self.write(&base_temp);
            self.write(" === void 0 ? void 0 : ");
            self.write(&base_temp);
            self.write(".");
            self.emit_property_name_without_import_substitution(access.name_or_argument);
        }
        if needs_parens {
            self.write(")");
        }
    }

    fn emit_optional_element_access_downlevel(&mut self, access: &AccessExprData) {
        // When the lowered ternary appears inside a prefix/postfix unary or
        // conditional condition, wrap in parens to preserve precedence.
        let needs_parens = self.ctx.flags.optional_chain_needs_parens;
        if needs_parens {
            self.write("(");
            self.ctx.flags.optional_chain_needs_parens = false;
        }
        let base_simple = self.is_simple_nullish_expression(access.expression);
        if base_simple {
            self.emit(access.expression);
            self.write(" === null || ");
            self.emit(access.expression);
            self.write(" === void 0 ? void 0 : ");
            self.emit(access.expression);
            self.write("[");
            self.emit(access.name_or_argument);
            self.write("]");
        } else {
            let base_temp = self.make_unique_name_hoisted();
            self.write("(");
            self.write(&base_temp);
            self.write(" = ");
            self.emit(access.expression);
            self.write(")");
            self.write(" === null || ");
            self.write(&base_temp);
            self.write(" === void 0 ? void 0 : ");
            self.write(&base_temp);
            self.write("[");
            self.emit(access.name_or_argument);
            self.write("]");
        }
        if needs_parens {
            self.write(")");
        }
    }

    pub(super) fn emit_property_name_without_import_substitution(&mut self, node: NodeIndex) {
        let prev_import = self.suppress_commonjs_named_import_substitution;
        let prev_ns = self.suppress_ns_qualification;
        self.suppress_commonjs_named_import_substitution = true;
        self.suppress_ns_qualification = true;
        self.emit(node);
        self.suppress_commonjs_named_import_substitution = prev_import;
        self.suppress_ns_qualification = prev_ns;
    }

    /// Look up const enum member values for the given name, scoped to the position
    /// of the access expression. When multiple scoped entries exist for the same name,
    /// the tightest (most specific) scope containing `access_pos` is preferred.
    fn lookup_scoped_const_enum_values(
        &self,
        enum_path: &str,
        access_pos: u32,
    ) -> Option<&rustc_hash::FxHashMap<String, crate::enums::evaluator::EnumValue>> {
        if let Some(r) = self.lookup_scoped_const_enum_values_direct(enum_path, access_pos) {
            return Some(r);
        }
        if let Some(dot_pos) = enum_path.find('.') {
            let first = &enum_path[..dot_pos];
            let rest = &enum_path[dot_pos + 1..];
            if let Some(target) = self.const_enum_import_aliases.get(first) {
                let resolved = format!("{}.{}", target, rest);
                if let Some(r) = self.lookup_scoped_const_enum_values_direct(&resolved, access_pos)
                {
                    return Some(r);
                }
            }
        } else if let Some(target) = self.const_enum_import_aliases.get(enum_path) {
            if let Some(r) = self.lookup_scoped_const_enum_values_direct(target, access_pos) {
                return Some(r);
            }
        }
        None
    }
    fn lookup_scoped_const_enum_values_direct(
        &self,
        enum_path: &str,
        access_pos: u32,
    ) -> Option<&rustc_hash::FxHashMap<String, crate::enums::evaluator::EnumValue>> {
        let entries = self.const_enum_values.get(enum_path)?;
        let mut best: Option<&crate::emitter::core::ScopedConstEnum> = None;
        for entry in entries {
            if access_pos >= entry.scope_start && access_pos < entry.scope_end {
                if let Some(prev) = best {
                    if (entry.scope_end - entry.scope_start) < (prev.scope_end - prev.scope_start) {
                        best = Some(entry);
                    }
                } else {
                    best = Some(entry);
                }
            }
        }
        best.map(|e| &e.values)
    }
    fn build_access_chain_path(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return Some(self.arena.get_identifier(node)?.escaped_text.clone());
        }
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(node)?;
            let left = self.build_access_chain_path(access.expression)?;
            let nm = self.arena.get(access.name_or_argument)?;
            if nm.kind != SyntaxKind::Identifier as u16 {
                return None;
            }
            return Some(format!(
                "{}.{}",
                left,
                &self.arena.get_identifier(nm)?.escaped_text
            ));
        }
        None
    }

    /// Try to inline a property access to a const enum member.
    /// Returns `Some("value /* EnumName.Member */")` if the access targets a const enum.
    fn try_inline_const_enum_property_access(&self, access: &AccessExprData) -> Option<String> {
        let expr_node = self.arena.get(access.expression)?;
        let name_node = self.arena.get(access.name_or_argument)?;
        if name_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let member_name = &self.arena.get_identifier(name_node)?.escaped_text;
        let expr_path = self.build_access_chain_path(access.expression)?;
        let members = self.lookup_scoped_const_enum_values(expr_path.as_str(), expr_node.pos)?;
        let value = members.get(member_name.as_str())?;
        let dq = expr_path.clone();
        if self.ctx.options.remove_comments {
            Some(value.to_js_literal())
        } else {
            Some(format!(
                "{} /* {}.{} */",
                value.to_js_literal(),
                dq,
                member_name
            ))
        }
    }

    /// Try to inline an element access to a const enum member.
    /// Returns `Some("value /* EnumName[\"Member\"] */")` if the access targets a const enum.
    fn try_inline_const_enum_element_access(&self, access: &AccessExprData) -> Option<String> {
        // The expression must be a simple identifier (the enum name)
        let expr_node = self.arena.get(access.expression)?;
        if expr_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let enum_name = &self.arena.get_identifier(expr_node)?.escaped_text;

        // Look up in const enum values, scoped to the access position
        let members = self.lookup_scoped_const_enum_values(enum_name.as_str(), expr_node.pos)?;

        // The argument must be a string literal or no-substitution template literal
        let arg_node = self.arena.get(access.name_or_argument)?;
        let is_template = arg_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16;
        if arg_node.kind != SyntaxKind::StringLiteral as u16 && !is_template {
            return None;
        }
        let member_name = &self.arena.get_literal(arg_node)?.text;

        let value = members.get(member_name.as_str())?;
        if self.ctx.options.remove_comments {
            Some(value.to_js_literal())
        } else {
            // Use the original source text for the argument to preserve escape
            // sequences (e.g., "\u{44}") and quote style (backticks vs quotes).
            let arg_text = self
                .source_text
                .and_then(|text| {
                    let start = arg_node.pos as usize;
                    let end = arg_node.end as usize;
                    if end <= text.len() && start < end {
                        Some(text[start..end].trim())
                    } else {
                        None
                    }
                })
                .unwrap_or({
                    if is_template {
                        // Can't get source text; fall back
                        ""
                    } else {
                        ""
                    }
                });
            if !arg_text.is_empty() {
                Some(format!(
                    "{} /* {}[{}] */",
                    value.to_js_literal(),
                    enum_name,
                    arg_text
                ))
            } else if is_template {
                Some(format!(
                    "{} /* {}[`{}`] */",
                    value.to_js_literal(),
                    enum_name,
                    member_name
                ))
            } else {
                Some(format!(
                    "{} /* {}[\"{}\"] */",
                    value.to_js_literal(),
                    enum_name,
                    member_name
                ))
            }
        }
    }

    /// Check if a node expression would inline to a negative const enum value.
    /// Used to determine if parentheses are needed around the expression
    /// (e.g., `(-1 /* Foo.A */).toString()` vs `100 /* Foo.X */.toString()`).
    fn expression_is_negative_const_enum_inline(&self, idx: NodeIndex) -> bool {
        if self.const_enum_values.is_empty() {
            return false;
        }
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.arena.get_access_expr(node)
        {
            if let Some(ep) = self.build_access_chain_path(access.expression)
                && let Some(expr) = self.arena.get(access.expression)
                && let Some(name) = self.arena.get(access.name_or_argument)
                && name.kind == SyntaxKind::Identifier as u16
                && let Some(mi) = self.arena.get_identifier(name)
                && let Some(members) = self.lookup_scoped_const_enum_values(&ep, expr.pos)
                && let Some(value) = members.get(mi.escaped_text.as_str())
            {
                return value.is_negative();
            }
        }
        // Check element access: EnumName["Member"] or EnumName[`Member`]
        if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            && let Some(access) = self.arena.get_access_expr(node)
        {
            let expr_node = self.arena.get(access.expression);
            let arg_node = self.arena.get(access.name_or_argument);
            if let (Some(expr), Some(arg)) = (expr_node, arg_node)
                && expr.kind == SyntaxKind::Identifier as u16
                && (arg.kind == SyntaxKind::StringLiteral as u16
                    || arg.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16)
                && let Some(enum_ident) = self.arena.get_identifier(expr)
                && let Some(lit) = self.arena.get_literal(arg)
                && let Some(members) =
                    self.lookup_scoped_const_enum_values(enum_ident.escaped_text.as_str(), expr.pos)
                && let Some(value) = members.get(lit.text.as_str())
            {
                return value.is_negative();
            }
        }
        false
    }

    /// Check if a const enum access expression resolves to a non-negative integer,
    /// which would need double-dot for property access (e.g., `100..toString()`).
    fn resolve_const_enum_needs_double_dot(&self, idx: NodeIndex, node: &Node) -> bool {
        if self.const_enum_values.is_empty() {
            return false;
        }
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.arena.get_access_expr(node)
        {
            if let Some(ep) = self.build_access_chain_path(access.expression)
                && let Some(expr) = self.arena.get(access.expression)
                && let Some(name) = self.arena.get(access.name_or_argument)
                && name.kind == SyntaxKind::Identifier as u16
                && let Some(mi) = self.arena.get_identifier(name)
                && let Some(members) = self.lookup_scoped_const_enum_values(&ep, expr.pos)
                && let Some(value) = members.get(mi.escaped_text.as_str())
            {
                return value.needs_double_dot();
            }
        }
        // Check element access: EnumName["Member"] or EnumName[`Member`]
        if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            && let Some(access) = self.arena.get_access_expr(node)
            && let Some(expr) = self.arena.get(access.expression)
            && let Some(arg) = self.arena.get(access.name_or_argument)
            && expr.kind == SyntaxKind::Identifier as u16
            && (arg.kind == SyntaxKind::StringLiteral as u16
                || arg.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16)
            && let Some(enum_ident) = self.arena.get_identifier(expr)
            && let Some(lit) = self.arena.get_literal(arg)
            && let Some(members) =
                self.lookup_scoped_const_enum_values(enum_ident.escaped_text.as_str(), expr.pos)
            && let Some(value) = members.get(lit.text.as_str())
        {
            return value.needs_double_dot();
        }
        let _ = idx; // suppress unused warning
        false
    }

    /// Check if a type assertion/as/satisfies chain ultimately wraps an object literal.
    pub(in crate::emitter) fn type_assertion_wraps_object_literal(
        &self,
        mut idx: NodeIndex,
    ) -> bool {
        loop {
            let Some(node) = self.arena.get(idx) else {
                return false;
            };
            match node.kind {
                k if k == syntax_kind_ext::TYPE_ASSERTION
                    || k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
                {
                    if let Some(ta) = self.arena.get_type_assertion(node) {
                        idx = ta.expression;
                    } else {
                        return false;
                    }
                }
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    if let Some(p) = self.arena.get_parenthesized(node) {
                        idx = p.expression;
                    } else {
                        return false;
                    }
                }
                k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => return true,
                _ => return false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::output::printer::{PrintOptions, Printer};
    use tsz_parser::ParserState;

    /// When lowering optional property access (`?.`) to ES2019 and below for
    /// complex base expressions, the emitter uses a temp variable:
    /// `(temp = expr) === null || temp === void 0 ? void 0 : temp.prop`
    /// This temp must be declared as `var _a;` at the top of the enclosing scope.
    #[test]
    fn optional_chain_emits_hoisted_temp_var_decl() {
        // Multi-line function body to exercise the function-scoped hoisting path
        let source = "function h() {\n    let x = getObj()?.value;\n    return x;\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("var _a;"),
            "Optional chain lowering must emit `var _a;` for the hoisted temp.\nOutput:\n{output}"
        );
        assert!(
            output.contains("(_a = getObj())"),
            "Optional chain lowering must use temp in assignment.\nOutput:\n{output}"
        );
    }

    /// Optional method call on a simple identifier should NOT use a temp variable.
    /// `o?.b()` → `o === null || o === void 0 ? void 0 : o.b()` (no `_a`).
    #[test]
    fn optional_method_call_simple_identifier_no_temp() {
        let source = "declare const o: any;\no?.b();\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let opts = PrintOptions {
            target: tsz_common::common::ScriptTarget::ES2019,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("o === null || o === void 0 ? void 0 : o.b()"),
            "Simple identifier should be used directly, no temp var.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("_a"),
            "No temp variable should be allocated for simple identifier.\nOutput:\n{output}"
        );
    }

    /// Optional method call with `.call()` on a simple identifier should use the
    /// identifier directly as the `.call()` receiver.
    /// `o.b?.()` → `(_a = o.b) === null || _a === void 0 ? void 0 : _a.call(o)`
    #[test]
    fn optional_call_simple_receiver_uses_identifier_in_call() {
        let source = "declare const o: any;\no.b?.();\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let opts = PrintOptions {
            target: tsz_common::common::ScriptTarget::ES2019,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains(".call(o)"),
            "Simple identifier should be used directly as .call() receiver.\nOutput:\n{output}"
        );
        // Should only have one temp (_a for the method), not two
        assert!(
            !output.contains("_b"),
            "Only one temp var should be allocated.\nOutput:\n{output}"
        );
    }

    /// Complex (non-identifier) expression in optional method call MUST use a temp.
    /// `f()?.b()` needs a temp to avoid calling `f()` twice.
    #[test]
    fn optional_method_call_complex_expr_uses_temp() {
        let source = "declare function f(): any;\nf()?.b();\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let opts = PrintOptions {
            target: tsz_common::common::ScriptTarget::ES2019,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("_a = f()"),
            "Complex expression must be captured in temp var.\nOutput:\n{output}"
        );
        assert!(
            output.contains("=== null"),
            "Must have null check.\nOutput:\n{output}"
        );
    }

    /// When a downlevel optional chain is used as a ternary condition,
    /// the lowered ternary must be wrapped in parens to preserve precedence.
    /// `o?.b ? 1 : 0` → `(o === null || o === void 0 ? void 0 : o.b) ? 1 : 0`
    /// Without parens: `o === null || o === void 0 ? void 0 : o.b ? 1 : 0`
    /// would parse as the wrong ternary nesting.
    #[test]
    fn optional_chain_in_ternary_condition_gets_parens() {
        let source = "declare const o: any;\no?.b ? 1 : 0;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let opts = PrintOptions {
            target: tsz_common::common::ScriptTarget::ES2019,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("(o === null || o === void 0 ? void 0 : o.b) ? 1 : 0"),
            "Lowered optional chain in ternary condition must be wrapped in parens.\nOutput:\n{output}"
        );
    }

    /// When a downlevel optional chain is used as an operand of `===`,
    /// the lowered ternary must be wrapped in parens.
    /// `o?.x === 1` → `(o === null || o === void 0 ? void 0 : o.x) === 1`
    #[test]
    fn optional_chain_in_binary_equals_gets_parens() {
        let source = "declare const o: any;\no?.x === 1;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let opts = PrintOptions {
            target: tsz_common::common::ScriptTarget::ES2019,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("(o === null || o === void 0 ? void 0 : o.x) === 1"),
            "Lowered optional chain in === operand must be wrapped in parens.\nOutput:\n{output}"
        );
    }

    /// When a downlevel optional chain is used with postfix `++`,
    /// the lowered ternary must be wrapped in parens.
    /// `o?.a++` → `(o === null || o === void 0 ? void 0 : o.a)++`
    #[test]
    fn optional_chain_in_postfix_increment_gets_parens() {
        let source = "declare const o: any;\no?.a++;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let opts = PrintOptions {
            target: tsz_common::common::ScriptTarget::ES2019,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("(o === null || o === void 0 ? void 0 : o.a)++"),
            "Lowered optional chain in postfix ++ must be wrapped in parens.\nOutput:\n{output}"
        );
    }

    // =====================================================================
    // write_dot_token: numeric literal double-dot disambiguation
    // =====================================================================

    /// Plain integer property access needs `..` to disambiguate from float literal.
    /// `1.toString()` is a syntax error; `1..toString()` is correct.
    #[test]
    fn numeric_literal_property_access_plain_integer() {
        let source = "1 .foo;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("1..foo"),
            "Plain integer property access must use `..`.\nOutput:\n{output}"
        );
    }

    /// Float literals already have a `.`, so they need only one dot for property access.
    #[test]
    fn numeric_literal_property_access_float() {
        let source = "1.0 .foo;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("1.0.foo"),
            "Float literal should use single dot.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("1.0..foo"),
            "Float literal must NOT use double-dot.\nOutput:\n{output}"
        );
    }

    /// Exponent literals (e.g., `1e0`) don't need `..` because the exponent
    /// part prevents the parser from treating the dot as part of the number.
    #[test]
    fn numeric_literal_property_access_exponent() {
        let source = "1e0 .foo;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            !output.contains("1e0..foo"),
            "Exponent literal must NOT use double-dot.\nOutput:\n{output}"
        );
    }

    /// Hex literal `0xff` doesn't need `..` because the `0x` prefix disambiguates.
    #[test]
    fn numeric_literal_property_access_hex() {
        let source = "0xff .foo;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            !output.contains("0xff..foo"),
            "Hex literal must NOT use double-dot.\nOutput:\n{output}"
        );
    }

    /// Type assertion wrapping a numeric literal: `(<any>1).foo` → `1..foo`
    /// after type erasure removes the assertion and redundant parens.
    #[test]
    fn numeric_literal_property_access_through_type_assertion() {
        let source = "(<any>1).foo;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("1..foo"),
            "Type-asserted integer must use `..` after erasure.\nOutput:\n{output}"
        );
    }

    /// `as` assertion wrapping a numeric literal: `(1 as any).foo` → `1..foo`
    #[test]
    fn numeric_literal_property_access_through_as_expression() {
        let source = "(1 as any).foo;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("1..foo"),
            "`as` asserted integer must use `..` after erasure.\nOutput:\n{output}"
        );
    }

    // =====================================================================
    // Const enum inlining
    // =====================================================================

    /// Const enum property access is inlined: `G.A` → `1 /* G.A */`
    #[test]
    fn const_enum_property_access_inlined() {
        let source = "const enum G { A = 1, B = 2, C = A + B }\nvar a = G.A;\nvar c = G.C;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("1 /* G.A */"),
            "Const enum property access must be inlined with comment.\nOutput:\n{output}"
        );
        assert!(
            output.contains("3 /* G.C */"),
            "Computed const enum member (A+B=3) must be folded.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("= G.A"),
            "Original property access must not appear in output.\nOutput:\n{output}"
        );
    }

    /// Const enum element access is inlined: `G["A"]` → `1 /* G["A"] */`
    #[test]
    fn const_enum_element_access_inlined() {
        let source = "const enum G { A = 1, B = 2 }\nvar a = G[\"A\"];\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("1 /* G[\"A\"] */"),
            "Const enum element access must be inlined with comment.\nOutput:\n{output}"
        );
    }

    /// Const enum declaration is erased (not emitted) when `preserve_const_enums` is false.
    #[test]
    fn const_enum_declaration_erased() {
        let source = "const enum Direction { Up = 1, Down = 2 }\nvar x = Direction.Up;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            !output.contains("Direction)"),
            "Const enum IIFE must not appear in output.\nOutput:\n{output}"
        );
        assert!(
            output.contains("1 /* Direction.Up */"),
            "Const enum usage must be inlined.\nOutput:\n{output}"
        );
    }

    /// String const enum values are inlined with proper quoting.
    #[test]
    fn const_enum_string_values_inlined() {
        let source = "const enum S { Hello = \"hello\", World = \"world\" }\nvar x = S.Hello;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("\"hello\" /* S.Hello */"),
            "String const enum must be inlined with quoted value.\nOutput:\n{output}"
        );
    }
}
