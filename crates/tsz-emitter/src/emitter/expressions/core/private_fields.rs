use super::super::super::{Printer, get_operator_text};
use crate::transforms::private_fields_es5::get_private_field_name;
use tsz_parser::parser::{NodeIndex, node::Node, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

/// Result of extracting a private field access from a (possibly parenthesized) node.
struct PrivateFieldAccess {
    /// The receiver expression node index (e.g., `this` or `A.getInstance()`)
    expression: NodeIndex,
    /// The cleaned field name (without `#`)
    clean_name: String,
    /// The weakmap variable name
    weakmap_name: String,
}

impl<'a> Printer<'a> {
    // =========================================================================
    // Expressions
    // =========================================================================

    /// Try to extract a private field access from a node, unwrapping parentheses
    /// and type assertions. Returns None if this isn't a private field access.
    fn try_extract_private_field_access(&self, idx: NodeIndex) -> Option<PrivateFieldAccess> {
        if self.private_field_weakmaps.is_empty() {
            return None;
        }
        let node = self.arena.get(idx)?;
        // Unwrap parenthesized expressions and type assertions
        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.arena.get_parenthesized(node)
        {
            return self.try_extract_private_field_access(paren.expression);
        }
        // Also unwrap type assertion expressions since these are erased in JS emit
        if (node.kind == syntax_kind_ext::TYPE_ASSERTION
            || node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION)
            && let Some(ta) = self.arena.get_type_assertion(node)
        {
            return self.try_extract_private_field_access(ta.expression);
        }
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(node)?;
        let name_node = self.arena.get(access.name_or_argument)?;
        if name_node.kind != SyntaxKind::PrivateIdentifier as u16 {
            return None;
        }
        let field_name = get_private_field_name(self.arena, access.name_or_argument)?;
        let clean_name = field_name
            .strip_prefix('#')
            .unwrap_or(&field_name)
            .to_string();
        let weakmap_name = self.private_field_weakmaps.get(&clean_name)?.clone();
        Some(PrivateFieldAccess {
            expression: access.expression,
            clean_name,
            weakmap_name,
        })
    }

    /// Check if a receiver expression is simple (this keyword or an identifier)
    /// and doesn't need to be cached in a temp variable to avoid double-evaluation.
    fn receiver_is_simple(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return true;
        };
        node.kind == SyntaxKind::ThisKeyword as u16
            || node.is_identifier()
            || node.kind == SyntaxKind::SuperKeyword as u16
    }

    fn emit_private_field_set_close(&mut self, clean_name: &str) {
        let info = self.private_member_info.get(clean_name).cloned();
        let kind = info.as_ref().map_or("f", |i| i.kind);
        self.write(", \"");
        self.write(kind);
        self.write("\"");
        if let Some(ref i) = info {
            if let Some(ref setter) = i.setter_ref {
                self.write(", ");
                self.write(setter);
            } else if kind == "a" || kind == "m" {
                // Accessor with no setter or method (read-only) -- omit fn_ref for SET
            } else if let Some(ref fn_ref) = i.fn_ref {
                self.write(", ");
                self.write(fn_ref);
            }
        }
        self.write(")");
    }

    /// Emit a private field unary mutation (++ or --).
    /// `is_prefix` indicates if it's prefix (++x) or postfix (x++).
    /// `is_statement` indicates if the result value is discarded (statement context).
    /// `operator` is `PlusPlusToken` or `MinusMinusToken`.
    fn emit_private_field_unary_mutation(
        &mut self,
        pfa: PrivateFieldAccess,
        operator: u16,
        is_prefix: bool,
        is_statement: bool,
    ) {
        let needs_receiver_temp = !self.receiver_is_simple(pfa.expression);
        let op_text = get_operator_text(operator);
        let expression = pfa.expression;
        let weakmap_name = pfa.weakmap_name.clone();
        let clean_name = pfa.clean_name;

        // For complex receivers, create a temp var so we only evaluate once
        let receiver_temp = if needs_receiver_temp {
            Some(self.make_unique_name_hoisted())
        } else {
            None
        };

        // For postfix-as-value, allocate old_val temp FIRST (matches tsc temp ordering)
        let old_val_temp = if !is_prefix && !is_statement {
            Some(self.make_unique_name_hoisted())
        } else {
            None
        };

        // Allocate temp for the value
        let val_temp = self.make_unique_name_hoisted();

        if is_prefix {
            // `++this.#x` → `__classPrivateFieldSet(this, _C_x, (_a = __classPrivateFieldGet(this, _C_x, "f"), ++_a), "f")`
            self.write_helper("__classPrivateFieldSet");
            self.write("(");
            self.emit_receiver_or_temp_assign(expression, receiver_temp.as_deref());
            self.write(", ");
            self.emit_private_state_var(&weakmap_name, &clean_name);
            self.write(", (");
            self.write(&val_temp);
            self.write(" = ");
            self.emit_private_field_get_inline(
                receiver_temp.as_deref(),
                expression,
                &weakmap_name,
                &clean_name,
            );
            self.write(", ");
            self.write(op_text);
            self.write(&val_temp);
            self.write(")");
            self.emit_private_field_set_close(&clean_name);
        } else if is_statement {
            // `this.#x++` (statement) → `__classPrivateFieldSet(this, _C_x, (_a = get(...), _a++, _a), "f")`
            self.write_helper("__classPrivateFieldSet");
            self.write("(");
            self.emit_receiver_or_temp_assign(expression, receiver_temp.as_deref());
            self.write(", ");
            self.emit_private_state_var(&weakmap_name, &clean_name);
            self.write(", (");
            self.write(&val_temp);
            self.write(" = ");
            self.emit_private_field_get_inline(
                receiver_temp.as_deref(),
                expression,
                &weakmap_name,
                &clean_name,
            );
            self.write(", ");
            self.write(&val_temp);
            self.write(op_text);
            self.write(", ");
            self.write(&val_temp);
            self.write(")");
            self.emit_private_field_set_close(&clean_name);
        } else {
            // `const a = this.#x++` → `(set(this, _C_x, (_b = get(...), _a = _b++, _b), "f"), _a)`
            let old_val = old_val_temp.as_ref().unwrap();
            self.write("(");
            self.write_helper("__classPrivateFieldSet");
            self.write("(");
            self.emit_receiver_or_temp_assign(expression, receiver_temp.as_deref());
            self.write(", ");
            self.emit_private_state_var(&weakmap_name, &clean_name);
            self.write(", (");
            self.write(&val_temp);
            self.write(" = ");
            self.emit_private_field_get_inline(
                receiver_temp.as_deref(),
                expression,
                &weakmap_name,
                &clean_name,
            );
            self.write(", ");
            self.write(old_val);
            self.write(" = ");
            self.write(&val_temp);
            self.write(op_text);
            self.write(", ");
            self.write(&val_temp);
            self.write(")");
            self.emit_private_field_set_close(&clean_name);
            self.write(", ");
            self.write(old_val);
            self.write(")");
        }
    }

    pub(crate) fn emit_private_receiver(&mut self, expression: NodeIndex, clean_name: &str) {
        let alias_replacement =
            self.private_static_class_alias
                .as_ref()
                .and_then(|(cls_name, alias)| {
                    let info = self.private_member_info.get(clean_name)?;
                    if !info.is_static {
                        return None;
                    }
                    let expr_node = self.arena.get(expression)?;
                    if !expr_node.is_identifier() {
                        return None;
                    }
                    let ident = self.arena.get_identifier(expr_node)?;
                    if ident.escaped_text == *cls_name {
                        Some(alias.clone())
                    } else {
                        None
                    }
                });
        if let Some(alias) = alias_replacement {
            self.write(&alias);
        } else {
            self.emit(expression);
        }
    }

    /// Emit the state variable (WeakMap/WeakSet) for a private field.
    fn emit_private_state_var(&mut self, weakmap_name: &str, clean_name: &str) {
        let info = self.private_member_info.get(clean_name).cloned();
        if let Some(ref sv) = info.as_ref().and_then(|i| i.state_var.clone()) {
            self.write(sv);
        } else {
            self.write(weakmap_name);
        }
    }

    /// Emit either `expr` directly or `_a = expr` for temp assignment.
    fn emit_receiver_or_temp_assign(&mut self, expression: NodeIndex, receiver_temp: Option<&str>) {
        if let Some(temp) = receiver_temp {
            self.write(temp);
            self.write(" = ");
            self.emit(expression);
        } else {
            self.emit(expression);
        }
    }

    /// Emit a `__classPrivateFieldGet(receiver, state, kind, fn_ref)` call
    /// using either the temp name or emitting the expression directly.
    fn emit_private_field_get_inline(
        &mut self,
        receiver_temp: Option<&str>,
        expression: NodeIndex,
        weakmap_name: &str,
        clean_name: &str,
    ) {
        let info = self.private_member_info.get(clean_name).cloned();
        self.write_helper("__classPrivateFieldGet");
        self.write("(");
        if let Some(temp) = receiver_temp {
            self.write(temp);
        } else {
            self.emit_private_receiver(expression, clean_name);
        }
        self.write(", ");
        let state_var = info.as_ref().and_then(|i| i.state_var.clone());
        if let Some(ref sv) = state_var {
            self.write(sv);
        } else {
            self.write(weakmap_name);
        }
        let kind = info.as_ref().map_or("f", |i| i.kind);
        self.write(", \"");
        self.write(kind);
        self.write("\"");
        if let Some(ref i) = info
            && let Some(ref fn_ref) = i.fn_ref
        {
            self.write(", ");
            self.write(fn_ref);
        }
        self.write(")");
    }

    pub(in crate::emitter) fn emit_binary_expression(&mut self, node: &Node) {
        let Some(binary) = self.arena.get_binary_expr(node) else {
            return;
        };

        // Private field lowering: `this.#field = value` → `__classPrivateFieldSet(this, _C_field, value, "f")`
        // Also handles `#field in obj` → `__classPrivateFieldIn(_C_field, obj)`
        if !self.private_field_weakmaps.is_empty() {
            // Handle `#field in obj` → `__classPrivateFieldIn(_C_field, obj)`
            // For methods/accessors, use the state_var (WeakSet/class alias) instead of the fn var.
            if binary.operator_token == SyntaxKind::InKeyword as u16
                && let Some(left_node) = self.arena.get(binary.left)
                && left_node.kind == SyntaxKind::PrivateIdentifier as u16
                && let Some(field_name) = get_private_field_name(self.arena, binary.left)
            {
                let clean_name = field_name.strip_prefix('#').unwrap_or(&field_name);
                if let Some(weakmap_name) = self.private_field_weakmaps.get(clean_name).cloned() {
                    self.write_helper("__classPrivateFieldIn");
                    self.write("(");
                    // For methods/accessors, use state_var (WeakSet or class alias)
                    let in_var = self
                        .private_member_info
                        .get(clean_name)
                        .and_then(|info| info.state_var.clone())
                        .unwrap_or(weakmap_name);
                    self.write(&in_var);
                    self.write(", ");
                    self.emit(binary.right);
                    self.write(")");
                    return;
                }
            }

            // Handle `this.#field = value` and `(this.#field) = value` (with parens/type assertions)
            // → `__classPrivateFieldSet(this, _C_field, value, "f")`
            if binary.operator_token == SyntaxKind::EqualsToken as u16
                && let Some(pfa) = self.try_extract_private_field_access(binary.left)
            {
                self.write_helper("__classPrivateFieldSet");
                self.write("(");
                self.emit_private_receiver(pfa.expression, &pfa.clean_name);
                self.write(", ");
                if let Some(info) = self.private_member_info.get(&pfa.clean_name).cloned() {
                    if let Some(ref state_var) = info.state_var {
                        self.write(state_var);
                    } else {
                        self.write(&pfa.weakmap_name);
                    }
                    self.write(", ");
                    self.emit(binary.right);
                    self.write(", \"");
                    self.write(info.kind);
                    self.write("\"");
                    if let Some(ref setter) = info.setter_ref {
                        self.write(", ");
                        self.write(setter);
                    } else if info.kind == "a" || info.kind == "m" {
                        // Accessor with no setter or method (read-only) -- omit fn_ref for SET
                    } else if let Some(ref fn_ref) = info.fn_ref {
                        self.write(", ");
                        self.write(fn_ref);
                    }
                } else {
                    self.write(&pfa.weakmap_name);
                    self.write(", ");
                    self.emit(binary.right);
                    self.write(", \"f\"");
                }
                self.write(")");
                return;
            }

            // Handle compound assignment: `this.#field += value` →
            // `__classPrivateFieldSet(this, _C_field, __classPrivateFieldGet(this, _C_field, "f") + value, "f")`
            // For complex receivers: `A.getInstance().#field += value` →
            // `__classPrivateFieldSet(_a = A.getInstance(), _C_field, __classPrivateFieldGet(_a, _C_field, "f") + value, "f")`
            // For `**=` with ES2016 lowering: uses Math.pow() instead of **
            if self.is_compound_assignment(binary.operator_token)
                && let Some(pfa) = self.try_extract_private_field_access(binary.left)
            {
                let is_exp_assign =
                    binary.operator_token == SyntaxKind::AsteriskAsteriskEqualsToken as u16;
                let use_math_pow = is_exp_assign && self.ctx.needs_es2016_lowering;
                let base_op = if use_math_pow {
                    String::new()
                } else {
                    self.get_compound_base_operator(binary.operator_token)
                };

                let needs_receiver_temp = !self.receiver_is_simple(pfa.expression);
                let receiver_temp = if needs_receiver_temp {
                    Some(self.make_unique_name_hoisted())
                } else {
                    None
                };
                let expression = pfa.expression;
                let weakmap_name = pfa.weakmap_name.clone();
                let clean_name = pfa.clean_name;

                self.write_helper("__classPrivateFieldSet");
                self.write("(");
                self.emit_receiver_or_temp_assign(expression, receiver_temp.as_deref());
                self.write(", ");
                self.emit_private_state_var(&weakmap_name, &clean_name);
                self.write(", ");

                if use_math_pow {
                    self.write("Math.pow(");
                    self.emit_private_field_get_inline(
                        receiver_temp.as_deref(),
                        expression,
                        &weakmap_name,
                        &clean_name,
                    );
                    self.write(", ");
                    self.emit(binary.right);
                    self.write(")");
                } else {
                    self.emit_private_field_get_inline(
                        receiver_temp.as_deref(),
                        expression,
                        &weakmap_name,
                        &clean_name,
                    );
                    self.write(" ");
                    self.write(&base_op);
                    self.write(" ");
                    self.emit(binary.right);
                }

                self.emit_private_field_set_close(&clean_name);
                return;
            }
        }

        // ES5: lower assignment destructuring patterns
        if self.ctx.target_es5
            && binary.operator_token == SyntaxKind::EqualsToken as u16
            && let Some(left_node) = self.arena.get(binary.left)
            && matches!(
                left_node.kind,
                syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    | syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    | syntax_kind_ext::ARRAY_BINDING_PATTERN
                    | syntax_kind_ext::OBJECT_BINDING_PATTERN
            )
        {
            self.emit_assignment_destructuring_es5(left_node, binary.right);
            return;
        }

        // ES2015-ES2020: lower logical assignment and nullish-coalescing operators.
        let is_logical_assignment = binary.operator_token
            == SyntaxKind::AmpersandAmpersandEqualsToken as u16
            || binary.operator_token == SyntaxKind::BarBarEqualsToken as u16
            || binary.operator_token == SyntaxKind::QuestionQuestionEqualsToken as u16;
        let is_exponentiation = binary.operator_token == SyntaxKind::AsteriskAsteriskToken as u16;
        let is_exponentiation_assignment =
            binary.operator_token == SyntaxKind::AsteriskAsteriskEqualsToken as u16;
        let is_nullish = binary.operator_token == SyntaxKind::QuestionQuestionToken as u16;
        let supports_logical_assignment =
            (self.ctx.options.target as u8) >= (super::super::super::ScriptTarget::ES2021 as u8);

        if (is_exponentiation || is_exponentiation_assignment) && self.ctx.needs_es2016_lowering {
            self.emit_exponentiation_expression(binary);
            return;
        }

        if is_logical_assignment && !supports_logical_assignment {
            self.emit_logical_assignment_expression(binary);
            return;
        }

        if is_nullish && !self.ctx.options.target.supports_es2020() {
            self.emit_nullish_coalescing_expression(binary);
            return;
        }

        // Assignment and comma operators accept AssignmentExpression operands,
        // which includes YieldExpression. So yield-from-await doesn't need
        // parens in those positions. Only non-assignment, non-comma binary
        // operators need the in_binary_operand flag to trigger yield wrapping.
        let op = binary.operator_token;
        let is_assignment_or_comma = op == SyntaxKind::CommaToken as u16
            || op == SyntaxKind::EqualsToken as u16
            || op == SyntaxKind::PlusEqualsToken as u16
            || op == SyntaxKind::MinusEqualsToken as u16
            || op == SyntaxKind::AsteriskEqualsToken as u16
            || op == SyntaxKind::SlashEqualsToken as u16
            || op == SyntaxKind::PercentEqualsToken as u16
            || op == SyntaxKind::AsteriskAsteriskEqualsToken as u16
            || op == SyntaxKind::LessThanLessThanEqualsToken as u16
            || op == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
            || op == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
            || op == SyntaxKind::AmpersandEqualsToken as u16
            || op == SyntaxKind::CaretEqualsToken as u16
            || op == SyntaxKind::BarEqualsToken as u16
            || op == SyntaxKind::BarBarEqualsToken as u16
            || op == SyntaxKind::AmpersandAmpersandEqualsToken as u16
            || op == SyntaxKind::QuestionQuestionEqualsToken as u16;

        let prev_in_binary = self.ctx.flags.in_binary_operand;
        if !is_assignment_or_comma {
            self.ctx.flags.in_binary_operand = true;
        }
        // When lowering optional chains in binary operands, the ternary must be
        // wrapped in parens to avoid precedence issues.
        // e.g., `x?.kind === null` → `(x === null || x === void 0 ? void 0 : x.kind) === null`
        let prev_optional = self.ctx.flags.optional_chain_needs_parens;
        let prev_nullish = self.ctx.flags.nullish_coalescing_needs_parens;
        if !is_assignment_or_comma {
            self.ctx.flags.optional_chain_needs_parens = true;
            self.ctx.flags.nullish_coalescing_needs_parens = true;
        }
        if self.assignment_left_is_recovered_super(binary.left, binary.operator_token) {
            self.write("super.");
        } else {
            self.emit(binary.left);
        }
        self.ctx.flags.optional_chain_needs_parens = prev_optional;
        self.ctx.flags.nullish_coalescing_needs_parens = prev_nullish;

        // Check if there's a line break between left operand and operator,
        // and between operator and right operand. TypeScript preserves these
        // line breaks and places the operator at the START of the continuation
        // line, not at the end of the current line.
        let (has_newline_before_op, has_newline_after_op) = if let Some(text) = self.source_text {
            if let (Some(left_node), Some(right_node)) =
                (self.arena.get(binary.left), self.arena.get(binary.right))
            {
                let left_end = left_node.end as usize;
                let right_start = right_node.pos as usize;
                let end = std::cmp::min(right_start, text.len());
                let start = std::cmp::min(left_end, end);
                let gap = &text[start..end];
                let gap_bytes = gap.as_bytes();
                // Find operator position by skipping trivia (whitespace + comments)
                let mut i = 0;
                let mut op_offset = None;
                while i < gap_bytes.len() {
                    match gap_bytes[i] {
                        b' ' | b'\t' | b'\r' | b'\n' => i += 1,
                        b'/' if i + 1 < gap_bytes.len() && gap_bytes[i + 1] == b'/' => {
                            i += 2;
                            while i < gap_bytes.len() && gap_bytes[i] != b'\n' {
                                i += 1;
                            }
                        }
                        b'/' if i + 1 < gap_bytes.len() && gap_bytes[i + 1] == b'*' => {
                            i += 2;
                            while i + 1 < gap_bytes.len()
                                && !(gap_bytes[i] == b'*' && gap_bytes[i + 1] == b'/')
                            {
                                i += 1;
                            }
                            if i + 1 < gap_bytes.len() {
                                i += 2;
                            }
                        }
                        _ => {
                            op_offset = Some(i);
                            break;
                        }
                    }
                }
                if let Some(off) = op_offset {
                    let op_len = get_operator_text(binary.operator_token).len();
                    let before = &gap[..off];
                    let after = &gap[off + op_len..];
                    (before.contains('\n'), after.contains('\n'))
                } else {
                    // Operator absorbed by left.end; gap is between
                    // operator end and right start. Any newlines are
                    // AFTER the operator.
                    (false, gap.contains('\n'))
                }
            } else {
                (false, false)
            }
        } else {
            (false, false)
        };
        let has_newline_before_right = has_newline_before_op || has_newline_after_op;

        // Comma operator: no space before, space after (e.g., `(1, 2, 3)`)
        if binary.operator_token == SyntaxKind::CommaToken as u16 {
            if has_newline_before_right {
                self.write(",");
                self.write_line();
                self.increase_indent();
                self.emit(binary.right);
                self.decrease_indent();
                self.ctx.flags.in_binary_operand = prev_in_binary;
                return;
            }
            self.write(", ");
        } else {
            // Map the operator region to its source position (at left operand end,
            // matching tsc's pattern of mapping the transition point)
            if let Some(left_node) = self.arena.get(binary.left) {
                self.map_source_offset(left_node.end);
            }
            if has_newline_before_op && has_newline_after_op {
                // Operator on its own line, right operand further indented
                // e.g., source: `a\n    +\n    b` → `a\n    +\n        b`
                self.write_line();
                self.increase_indent();
                self.write(get_operator_text(binary.operator_token));
                self.write_line();
                self.increase_indent();
                if !is_assignment_or_comma {
                    self.ctx.flags.optional_chain_needs_parens = true;
                    self.ctx.flags.nullish_coalescing_needs_parens = true;
                }
                self.emit(binary.right);
                self.ctx.flags.optional_chain_needs_parens = prev_optional;
                self.ctx.flags.nullish_coalescing_needs_parens = prev_nullish;
                self.decrease_indent();
                self.decrease_indent();
                self.ctx.flags.in_binary_operand = prev_in_binary;
                return;
            }
            if has_newline_before_op {
                // Operator at start of continuation line with right operand
                // e.g., source: `a\n    + b` → `a\n    + b`
                self.write_line();
                self.increase_indent();
                self.write(get_operator_text(binary.operator_token));
                self.write_space();
                if !is_assignment_or_comma {
                    self.ctx.flags.optional_chain_needs_parens = true;
                    self.ctx.flags.nullish_coalescing_needs_parens = true;
                }
                self.emit(binary.right);
                self.ctx.flags.optional_chain_needs_parens = prev_optional;
                self.ctx.flags.nullish_coalescing_needs_parens = prev_nullish;
                self.decrease_indent();
                self.ctx.flags.in_binary_operand = prev_in_binary;
                return;
            }
            if has_newline_after_op {
                // Operator at end of current line, right on next line
                // e.g., source: `a ||\n    b` → `a ||\n    b`
                self.write(" ");
                self.write(get_operator_text(binary.operator_token));
                self.write_line();
                self.increase_indent();
                if !is_assignment_or_comma {
                    self.ctx.flags.optional_chain_needs_parens = true;
                    self.ctx.flags.nullish_coalescing_needs_parens = true;
                }
                self.emit(binary.right);
                self.ctx.flags.optional_chain_needs_parens = prev_optional;
                self.ctx.flags.nullish_coalescing_needs_parens = prev_nullish;
                self.decrease_indent();
                self.ctx.flags.in_binary_operand = prev_in_binary;
                return;
            }
            self.write(" ");
            self.write(get_operator_text(binary.operator_token));
            self.write_space();
        }
        // Set parens flag for right operand of non-assignment/comma operators
        if !is_assignment_or_comma {
            self.ctx.flags.optional_chain_needs_parens = true;
            self.ctx.flags.nullish_coalescing_needs_parens = true;
        }
        self.emit(binary.right);
        self.ctx.flags.optional_chain_needs_parens = prev_optional;
        self.ctx.flags.nullish_coalescing_needs_parens = prev_nullish;
        self.ctx.flags.in_binary_operand = prev_in_binary;
    }

    fn assignment_left_is_recovered_super(&self, left: NodeIndex, op: u16) -> bool {
        if !self.is_assignment_operator(op) {
            return false;
        }
        self.arena
            .get(left)
            .is_some_and(|node| node.kind == SyntaxKind::SuperKeyword as u16)
    }

    const fn is_assignment_operator(&self, op: u16) -> bool {
        op == SyntaxKind::EqualsToken as u16
            || op == SyntaxKind::PlusEqualsToken as u16
            || op == SyntaxKind::MinusEqualsToken as u16
            || op == SyntaxKind::AsteriskEqualsToken as u16
            || op == SyntaxKind::SlashEqualsToken as u16
            || op == SyntaxKind::PercentEqualsToken as u16
            || op == SyntaxKind::AsteriskAsteriskEqualsToken as u16
            || op == SyntaxKind::LessThanLessThanEqualsToken as u16
            || op == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
            || op == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
            || op == SyntaxKind::AmpersandEqualsToken as u16
            || op == SyntaxKind::CaretEqualsToken as u16
            || op == SyntaxKind::BarEqualsToken as u16
            || op == SyntaxKind::BarBarEqualsToken as u16
            || op == SyntaxKind::AmpersandAmpersandEqualsToken as u16
            || op == SyntaxKind::QuestionQuestionEqualsToken as u16
    }

    pub(in crate::emitter) fn emit_prefix_unary(&mut self, node: &Node) {
        let Some(unary) = self.arena.get_unary_expr(node) else {
            return;
        };

        // Private field prefix mutation: `++this.#x` or `++(this.#x)`
        // → `__classPrivateFieldSet(this, _C_x, (_a = __classPrivateFieldGet(this, _C_x, "f"), ++_a), "f")`
        if (unary.operator == SyntaxKind::PlusPlusToken as u16
            || unary.operator == SyntaxKind::MinusMinusToken as u16)
            && let Some(pfa) = self.try_extract_private_field_access(unary.operand)
        {
            // For prefix, result is always the new value (same form for statement/value)
            self.emit_private_field_unary_mutation(pfa, unary.operator, true, false);
            return;
        }

        if self.in_system_execute_body
            && (unary.operator == SyntaxKind::PlusPlusToken as u16
                || unary.operator == SyntaxKind::MinusMinusToken as u16)
            && let Some(operand_node) = self.arena.get(unary.operand)
            && operand_node.kind == SyntaxKind::Identifier as u16
        {
            let local_name = self.get_identifier_text_idx(unary.operand);
            if let Some(export_name) = self.system_reexported_names.get(&local_name).cloned() {
                self.write("exports_1(\"");
                self.write(&export_name);
                self.write("\", ");
                self.write(get_operator_text(unary.operator));
                self.write(&local_name);
                self.write(")");
                return;
            }
        }

        self.write(get_operator_text(unary.operator));
        if unary.operator == SyntaxKind::AsteriskToken as u16 {
            self.write_space();
        }
        // Prevent `+ +x` from collapsing to `++x` (pre-increment) and
        // `- -x` from collapsing to `--x` (pre-decrement). When the operand
        // is also a prefix unary with the same sign (or is `++`/`--`),
        // insert a space to keep the tokens separate.
        if (unary.operator == SyntaxKind::PlusToken as u16
            || unary.operator == SyntaxKind::MinusToken as u16)
            && let Some(operand_node) = self.arena.get(unary.operand)
            && operand_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            && let Some(inner) = self.arena.get_unary_expr(operand_node)
        {
            let same_sign = inner.operator == unary.operator;
            let is_update = (unary.operator == SyntaxKind::PlusToken as u16
                && inner.operator == SyntaxKind::PlusPlusToken as u16)
                || (unary.operator == SyntaxKind::MinusToken as u16
                    && inner.operator == SyntaxKind::MinusMinusToken as u16);
            if same_sign || is_update {
                self.write_space();
            }
        }
        // Set flag so yield-from-await knows to wrap in parens
        // e.g., `!await x` → `!(yield x)` not `!yield x`
        let prev = self.ctx.flags.in_binary_operand;
        self.ctx.flags.in_binary_operand = true;
        // When lowering optional chains or nullish coalescing (e.g., `++o?.a`, `!(a ?? b)`),
        // the ternary must be wrapped in parens to preserve precedence.
        let prev_optional = self.ctx.flags.optional_chain_needs_parens;
        let prev_nullish = self.ctx.flags.nullish_coalescing_needs_parens;
        self.ctx.flags.optional_chain_needs_parens = true;
        self.ctx.flags.nullish_coalescing_needs_parens = true;
        self.emit(unary.operand);
        self.ctx.flags.optional_chain_needs_parens = prev_optional;
        self.ctx.flags.nullish_coalescing_needs_parens = prev_nullish;
        self.ctx.flags.in_binary_operand = prev;
    }

    pub(in crate::emitter) fn emit_postfix_unary(&mut self, node: &Node) {
        let Some(unary) = self.arena.get_unary_expr(node) else {
            return;
        };

        // Private field postfix mutation: `this.#x++` or `(this.#x)++`
        // Statement form: `__classPrivateFieldSet(this, _C_x, (_a = __classPrivateFieldGet(this, _C_x, "f"), _a++, _a), "f")`
        // Value form: `(__classPrivateFieldSet(this, _C_x, (_b = __classPrivateFieldGet(this, _C_x, "f"), _a = _b++, _b), "f"), _a)`
        if (unary.operator == SyntaxKind::PlusPlusToken as u16
            || unary.operator == SyntaxKind::MinusMinusToken as u16)
            && let Some(pfa) = self.try_extract_private_field_access(unary.operand)
        {
            let is_statement = self.ctx.flags.in_statement_expression;
            self.emit_private_field_unary_mutation(pfa, unary.operator, false, is_statement);
            return;
        }

        // When lowering optional chains or nullish coalescing (e.g., `o?.a++`, `(a ?? b)++`),
        // the ternary must be wrapped in parens to preserve precedence.
        let prev_optional = self.ctx.flags.optional_chain_needs_parens;
        let prev_nullish = self.ctx.flags.nullish_coalescing_needs_parens;
        self.ctx.flags.optional_chain_needs_parens = true;
        self.ctx.flags.nullish_coalescing_needs_parens = true;
        self.emit(unary.operand);
        self.ctx.flags.optional_chain_needs_parens = prev_optional;
        self.ctx.flags.nullish_coalescing_needs_parens = prev_nullish;
        // Map the postfix operator (e.g., ++ or --) to its source position
        if let Some(operand_node) = self.arena.get(unary.operand) {
            self.map_token_after_skipping_whitespace(operand_node.end, node.end);
        }
        self.write(get_operator_text(unary.operator));
    }

    pub(in crate::emitter) fn emit_new_expression(&mut self, node: &Node) {
        let Some(call) = self.arena.get_call_expr(node) else {
            return;
        };

        // Private field new: `new this.#C()` → `new (__classPrivateFieldGet(this, _C_C, "f"))()`
        let needs_private_parens = !self.private_field_weakmaps.is_empty()
            && self.arena.get(call.expression).is_some_and(|expr_node| {
                expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    && self
                        .arena
                        .get_access_expr(expr_node)
                        .and_then(|access| self.arena.get(access.name_or_argument))
                        .is_some_and(|name_node| {
                            name_node.kind == SyntaxKind::PrivateIdentifier as u16
                        })
            });

        self.write("new ");
        if needs_private_parens {
            self.write("(");
        }
        // Signal new-callee position so `emit_parenthesized` preserves parens
        // around call expressions: `new (x() as T)` → `new (x())` not `new x()`.
        let prev_new = self.paren_in_new_callee;
        self.paren_in_new_callee = true;
        if !self.emit_invalid_new_type_assertion_callee(call.expression) {
            self.emit(call.expression);
        }
        self.paren_in_new_callee = prev_new;
        if needs_private_parens {
            self.write(")");
        }
        if let Some(ref args) = call.arguments {
            // Map opening `(` — scan forward from callee end
            if let Some(expr_node) = self.arena.get(call.expression) {
                self.map_token_after(expr_node.end, node.end, b'(');
            }
            self.write("(");
            // The new expression's own parens provide grouping, so clear
            // the "needs parens" flags to avoid double-parenthesization
            // when an argument contains a downlevel optional chain or
            // nullish coalescing expression.
            let prev_optional = self.ctx.flags.optional_chain_needs_parens;
            let prev_nullish = self.ctx.flags.nullish_coalescing_needs_parens;
            self.ctx.flags.optional_chain_needs_parens = false;
            self.ctx.flags.nullish_coalescing_needs_parens = false;
            let valid_args: Vec<_> = args.nodes.iter().copied().filter(|n| n.is_some()).collect();
            self.emit_comma_separated(&valid_args);
            self.ctx.flags.optional_chain_needs_parens = prev_optional;
            self.ctx.flags.nullish_coalescing_needs_parens = prev_nullish;
            // Map closing `)` — scan backward from node end
            self.map_closing_paren(node);
            self.write(")");
            return;
        }

        if self.new_expression_has_explicit_parens(node, call.expression) {
            self.write("()");
        }
    }

    fn emit_invalid_new_type_assertion_callee(&mut self, expression: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expression) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::TYPE_ASSERTION {
            return false;
        }
        let Some(assertion) = self.arena.get_type_assertion(expr_node) else {
            return false;
        };

        self.write(" < ");
        self.emit(assertion.type_node);
        self.write(" > ");
        self.emit(assertion.expression);
        true
    }
}
