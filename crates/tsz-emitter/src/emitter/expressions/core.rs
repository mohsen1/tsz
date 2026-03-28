use super::super::{Printer, get_operator_text};
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
            || node.kind == SyntaxKind::Identifier as u16
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
            } else if kind == "a" {
                // Accessor with no setter - omit
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
            self.emit(expression);
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
                self.emit(pfa.expression);
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
                    } else if info.kind == "a" {
                        // Accessor with no setter - omit the fn ref
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
            (self.ctx.options.target as u8) >= (super::super::ScriptTarget::ES2021 as u8);

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
        self.emit(binary.left);
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
        self.emit(call.expression);
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

    fn new_expression_has_explicit_parens(
        &self,
        node: &Node,
        callee: tsz_parser::parser::NodeIndex,
    ) -> bool {
        let Some(source) = self.source_text else {
            return false;
        };

        let Some(callee_node) = self.arena.get(callee) else {
            return false;
        };

        let bytes = source.as_bytes();
        let mut i = callee_node.end as usize;
        let end = std::cmp::min(node.end as usize, source.len());

        while i < end {
            match bytes[i] {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    i += 1;
                }
                b'/' if i + 1 < end && bytes[i + 1] == b'/' => {
                    // Line comment: skip to end of line
                    while i < end && bytes[i] != b'\n' {
                        i += 1;
                    }
                }
                b'/' if i + 1 < end && bytes[i + 1] == b'*' => {
                    // Block comment: skip to closing */
                    i += 2;
                    while i + 1 < end && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                        i += 1;
                    }
                    if i + 1 < end {
                        i += 2;
                    }
                }
                b'(' => return true,
                _ => return false,
            }
        }
        false
    }

    pub(in crate::emitter) fn emit_parenthesized(&mut self, node: &Node) {
        let Some(paren) = self.arena.get_parenthesized(node) else {
            return;
        };

        // If the inner expression is a type assertion/as/satisfies/instantiation expression,
        // the parens were only needed for the TS syntax (e.g., `(<Type>x).foo`).
        // In JS emit, the type assertion is stripped, making the parens unnecessary
        // UNLESS the underlying expression (after unwrapping type assertions) is:
        //   - An object literal (block ambiguity)
        //   - A binary/complex expression (operator precedence would change)
        if let Some(inner) = self.arena.get(paren.expression)
            && (inner.kind == syntax_kind_ext::TYPE_ASSERTION
                || inner.kind == syntax_kind_ext::AS_EXPRESSION
                || inner.kind == syntax_kind_ext::SATISFIES_EXPRESSION
                || inner.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS)
        {
            let unwrapped_kind = self.unwrap_type_assertion_kind(paren.expression);
            // Only strip parens if the unwrapped expression is a simple primary that
            // cannot change meaning without parens: identifiers, property access,
            // element access, `this`, template literals, literals, class/function expr.
            let can_strip = matches!(
                unwrapped_kind,
                Some(k) if k == SyntaxKind::Identifier as u16
                    || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                    || k == SyntaxKind::ThisKeyword as u16
                    || k == SyntaxKind::SuperKeyword as u16
                    || k == SyntaxKind::NullKeyword as u16
                    || k == SyntaxKind::TrueKeyword as u16
                    || k == SyntaxKind::FalseKeyword as u16
                    || k == SyntaxKind::NumericLiteral as u16
                    || k == SyntaxKind::BigIntLiteral as u16
                    || k == SyntaxKind::StringLiteral as u16
                    || k == SyntaxKind::RegularExpressionLiteral as u16
                    || k == syntax_kind_ext::TEMPLATE_EXPRESSION
                    || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                    || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    || k == syntax_kind_ext::NON_NULL_EXPRESSION
                    || k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                    // CALL_EXPRESSION can strip parens unless in new-callee position:
                    // `(x() as T).foo` → `x().foo` is fine, but `new (x() as T)` must keep
                    // parens because `new x()` has different semantics (constructs `x`).
                    || (k == syntax_kind_ext::CALL_EXPRESSION && !self.paren_in_new_callee)
                    // NEW_EXPRESSION can strip parens only when NOT in access position:
                    // `(new a)` → `new a` is fine, but `(new a).b` must keep parens
                    // because `new a.b` has different semantics (constructs `a.b`).
                    || (k == syntax_kind_ext::NEW_EXPRESSION && !self.paren_in_access_position)
                    // FunctionExpression and ClassExpression can strip parens when NOT in
                    // expression statement position (where they'd be ambiguous with declarations).
                    // `let a = (<T>function foo() {})` → `let a = function foo() {}`
                    || ((k == syntax_kind_ext::FUNCTION_EXPRESSION
                        || k == syntax_kind_ext::CLASS_EXPRESSION)
                        && !self.ctx.flags.paren_leftmost_function_or_object
                        && (!self.paren_in_access_position || self.paren_is_direct_call_callee))
            );

            if can_strip {
                // Before stripping parens, check if there are comments between
                // the `(` and the inner expression. tsc preserves parens when a
                // comment exists inside them for SOME cases, but NOT when the
                // inner expression is a type assertion/as/satisfies that will be
                // erased. In the erasure case, tsc strips both parens and type
                // syntax, hoisting the comment before the expression:
                //   `(/* TODO */ expr as T)` → `/* TODO */ expr`
                //   `(/* TODO */ expr satisfies T)` → `/* TODO */ expr`
                let actual_inner_start = self.skip_trivia_forward(inner.pos, inner.pos + 2048);
                let has_inner_comment = if actual_inner_start > node.pos {
                    self.all_comments
                        .iter()
                        .any(|c| c.pos >= node.pos && c.end <= actual_inner_start)
                } else {
                    false
                };
                if !has_inner_comment {
                    self.emit(paren.expression);
                    return;
                }
                // When there IS a comment but the inner expression is a type
                // assertion/as/satisfies that will be erased, still strip the
                // parens — UNLESS the comment introduces a newline (e.g., a
                // line comment `// ...`). A newline between a keyword like
                // `yield` and its operand triggers ASI, so the parens must be
                // preserved to keep the operand attached:
                //   `yield (// comment\n a as any)` → `yield (\n// comment\n a)`
                //   `yield (/* ok */ a as any)`     → `yield /* ok */ a`
                if inner.kind == syntax_kind_ext::TYPE_ASSERTION
                    || inner.kind == syntax_kind_ext::AS_EXPRESSION
                    || inner.kind == syntax_kind_ext::SATISFIES_EXPRESSION
                {
                    let has_newline_comment = self.all_comments.iter().any(|c| {
                        c.pos >= node.pos && c.end <= actual_inner_start && c.has_trailing_new_line
                    });
                    if !has_newline_comment {
                        self.emit(paren.expression);
                        return;
                    }
                    // Fall through to emit with parens preserved
                }
                // Fall through to emit with parens preserved (non-type-erasure case)
            }

            // Check if the unwrapped expression is already parenthesized
            if self.type_assertion_result_is_parenthesized(paren.expression) {
                self.emit(paren.expression);
                return;
            }
            // Fall through to emit with parens preserved
        }

        // If the inner expression is another ParenExpr wrapping a type assertion/instantiation,
        // and the inner paren would be stripped during emit (because the unwrapped expression
        // is simple), then the outer parens are also redundant.
        if let Some(inner) = self.arena.get(paren.expression)
            && inner.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(inner_paren) = self.arena.get_parenthesized(inner)
            && let Some(inner_inner) = self.arena.get(inner_paren.expression)
            && (inner_inner.kind == syntax_kind_ext::TYPE_ASSERTION
                || inner_inner.kind == syntax_kind_ext::AS_EXPRESSION
                || inner_inner.kind == syntax_kind_ext::SATISFIES_EXPRESSION
                || inner_inner.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS)
        {
            // Check if the inner paren would strip its own parens (object literal or simple expr)
            if self.type_assertion_wraps_object_literal(inner_paren.expression) {
                self.emit(paren.expression);
                return;
            }
            // Also strip if the unwrapped expression is simple and the inner paren will be elided
            let unwrapped_kind = self.unwrap_type_assertion_kind(inner_paren.expression);
            let inner_can_strip = matches!(
                unwrapped_kind,
                Some(k) if k == SyntaxKind::Identifier as u16
                    || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                    || k == SyntaxKind::ThisKeyword as u16
                    || k == SyntaxKind::SuperKeyword as u16
                    || k == SyntaxKind::NullKeyword as u16
                    || k == SyntaxKind::TrueKeyword as u16
                    || k == SyntaxKind::FalseKeyword as u16
                    || k == SyntaxKind::NumericLiteral as u16
                    || k == SyntaxKind::BigIntLiteral as u16
                    || k == SyntaxKind::StringLiteral as u16
                    || k == SyntaxKind::RegularExpressionLiteral as u16
                    || k == syntax_kind_ext::TEMPLATE_EXPRESSION
                    || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                    || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    || k == syntax_kind_ext::NON_NULL_EXPRESSION
                    || k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                    || (k == syntax_kind_ext::CALL_EXPRESSION && !self.paren_in_new_callee)
                    || (k == syntax_kind_ext::NEW_EXPRESSION && !self.paren_in_access_position)
            );
            if inner_can_strip {
                // Inner paren will be stripped, so our outer paren is redundant
                self.emit(paren.expression);
                return;
            }
        }

        // Emit comments that appear BEFORE the `(` in source text.
        // JSDoc type casts like `/** @type {T} */(expr)` have the comment
        // before the opening paren — tsc preserves this placement.
        // Comments AFTER the `(` (e.g., `( /* Preserve */ j = f())`) stay inside.
        let open_paren_source_pos = if let Some(text) = self.source_text {
            let bytes = text.as_bytes();
            let mut pos = node.pos as usize;
            let limit = std::cmp::min(node.end as usize, bytes.len());
            let mut found = None;
            while pos < limit {
                match bytes[pos] {
                    b'(' => {
                        found = Some(pos as u32);
                        break;
                    }
                    b'/' if pos + 1 < limit && bytes[pos + 1] == b'*' => {
                        pos += 2;
                        while pos + 1 < limit && !(bytes[pos] == b'*' && bytes[pos + 1] == b'/') {
                            pos += 1;
                        }
                        if pos + 1 < limit {
                            pos += 2;
                        }
                    }
                    b'/' if pos + 1 < limit && bytes[pos + 1] == b'/' => {
                        while pos < limit && bytes[pos] != b'\n' {
                            pos += 1;
                        }
                    }
                    _ => pos += 1,
                }
            }
            found
        } else {
            None
        };
        if let Some(paren_pos) = open_paren_source_pos
            && self.has_pending_comment_before(paren_pos)
        {
            self.emit_comments_before_pos(paren_pos);
            // Leave pending_block_comment_space intact — tsc's printer
            // normalizes block comment spacing: `/** ... */(x)` becomes
            // `/** ... */ (x)` with a space before the paren.
        }
        self.write("(");
        // Emit inline comments between `(` and inner expression
        // (e.g., `( /* Preserve */j = f())`)
        // When the comment has a trailing newline (line comment `// ...`),
        // emit a newline after `(` so that the output matches tsc:
        //   `yield (\n// comment\na)` instead of `yield ( // comment\na)`
        if let Some(inner_node) = self.arena.get(paren.expression)
            && self.has_pending_comment_before(inner_node.pos)
        {
            if self.has_newline_comment_in_range(node.pos, inner_node.pos) {
                self.write_line();
            } else {
                self.write(" ");
            }
            self.emit_comments_before_pos(inner_node.pos);
            self.pending_block_comment_space = false;
        }
        // The explicit parens already provide grouping, so clear the
        // "needs parens" flags to avoid double-parenthesization when the
        // inner expression is a downlevel optional chain, nullish coalescing,
        // or yield-from-await in binary operand.
        let prev_optional = self.ctx.flags.optional_chain_needs_parens;
        let prev_nullish = self.ctx.flags.nullish_coalescing_needs_parens;
        let prev_in_binary = self.ctx.flags.in_binary_operand;
        self.ctx.flags.optional_chain_needs_parens = false;
        self.ctx.flags.nullish_coalescing_needs_parens = false;
        self.ctx.flags.in_binary_operand = false;
        self.emit(paren.expression);
        self.ctx.flags.in_binary_operand = prev_in_binary;
        self.ctx.flags.optional_chain_needs_parens = prev_optional;
        self.ctx.flags.nullish_coalescing_needs_parens = prev_nullish;
        self.write(")");
    }

    /// Unwrap type assertion chain and return the kind of the underlying expression.
    pub(in crate::emitter) fn unwrap_type_assertion_kind(&self, mut idx: NodeIndex) -> Option<u16> {
        loop {
            let node = self.arena.get(idx)?;
            match node.kind {
                k if k == syntax_kind_ext::TYPE_ASSERTION
                    || k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
                {
                    if let Some(ta) = self.arena.get_type_assertion(node) {
                        idx = ta.expression;
                    } else {
                        return Some(node.kind);
                    }
                }
                k if k == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS => {
                    if let Some(data) = self.arena.get_expr_type_args(node) {
                        idx = data.expression;
                    } else {
                        return Some(node.kind);
                    }
                }
                _ => return Some(node.kind),
            }
        }
    }

    /// Check if unwrapping a type assertion chain yields a parenthesized expression.
    /// Used to detect redundant outer parens: `((<Error>({...})))` → the type
    /// assertion wraps `({...})` which is already parenthesized, so outer parens
    /// are redundant.
    fn type_assertion_result_is_parenthesized(&self, mut idx: NodeIndex) -> bool {
        // Unwrap type assertions to find the underlying expression
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
                k if k == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS => {
                    if let Some(data) = self.arena.get_expr_type_args(node) {
                        idx = data.expression;
                    } else {
                        return false;
                    }
                }
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => return true,
                _ => return false,
            }
        }
    }

    /// Walk the leftmost chain of an expression, unwrapping type assertions,
    /// and return the kind of the deepest leftmost expression after erasure.
    /// This traces through `CallExpression`, `PropertyAccessExpression`,
    /// `ElementAccessExpression`, `TaggedTemplateExpression`, `NonNullExpression`,
    /// and type assertions to find what will actually appear at the start of
    /// the emitted expression.
    pub(in crate::emitter) fn leftmost_expression_kind_after_erasure(
        &self,
        mut idx: NodeIndex,
    ) -> Option<u16> {
        loop {
            let node = self.arena.get(idx)?;
            match node.kind {
                // Type assertions are erased — follow inner expression
                k if k == syntax_kind_ext::TYPE_ASSERTION
                    || k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
                {
                    if let Some(ta) = self.arena.get_type_assertion(node) {
                        idx = ta.expression;
                    } else {
                        return Some(node.kind);
                    }
                }
                // Call/New have a left-side "expression" field
                k if k == syntax_kind_ext::CALL_EXPRESSION => {
                    if let Some(call) = self.arena.get_call_expr(node) {
                        idx = call.expression;
                    } else {
                        return Some(node.kind);
                    }
                }
                // Property/Element access have a left-side "expression" field
                k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
                {
                    if let Some(acc) = self.arena.get_access_expr(node) {
                        idx = acc.expression;
                    } else {
                        return Some(node.kind);
                    }
                }
                // NonNull: expr!
                k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                    if let Some(expr) = self.arena.get_unary_expr_ex(node) {
                        idx = expr.expression;
                    } else {
                        return Some(node.kind);
                    }
                }
                k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
                    if let Some(tt) = self.arena.get_tagged_template(node) {
                        idx = tt.tag;
                    } else {
                        return Some(node.kind);
                    }
                }
                // ParenthesizedExpression wrapping a type assertion: parens stripped.
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    if let Some(paren) = self.arena.get_parenthesized(node) {
                        if let Some(inner) = self.arena.get(paren.expression)
                            && (inner.kind == syntax_kind_ext::TYPE_ASSERTION
                                || inner.kind == syntax_kind_ext::AS_EXPRESSION
                                || inner.kind == syntax_kind_ext::SATISFIES_EXPRESSION
                                || inner.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS)
                        {
                            idx = paren.expression;
                        } else {
                            return Some(node.kind);
                        }
                    } else {
                        return Some(node.kind);
                    }
                }
                // Terminal — return this kind
                _ => return Some(node.kind),
            }
        }
    }

    pub(in crate::emitter) fn emit_type_assertion_expression(&mut self, node: &Node) {
        let Some(assertion) = self.arena.get_type_assertion(node) else {
            self.write("void 0");
            return;
        };
        // Emit comments in the erased type assertion region before the inner expression.
        // For `<T>expr` (TypeAssertion): comments inside `<T>` are before the expression.
        if let Some(expr_node) = self.arena.get(assertion.expression) {
            self.emit_comments_before_pos(expr_node.pos);
        }
        self.emit_expression(assertion.expression);
        // For `expr as T` and `expr satisfies T`: skip comments inside the erased
        // type annotation so they don't leak into subsequent output.
        if (node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION)
            && !self.ctx.options.remove_comments
            && assertion.type_node.is_some()
            && let Some(type_node) = self.arena.get(assertion.type_node)
        {
            self.skip_comments_in_range(type_node.pos, type_node.end);
        }
    }

    pub(in crate::emitter) fn emit_non_null_expression(&mut self, node: &Node) {
        let Some(unary) = self.arena.get_unary_expr_ex(node) else {
            self.write("void 0");
            return;
        };
        // Emit comments before the inner expression.
        if let Some(expr_node) = self.arena.get(unary.expression) {
            self.emit_comments_before_pos(expr_node.pos);
        }
        self.emit_expression(unary.expression);
        // Trailing comments are preserved by the statement emitter.
    }

    pub(in crate::emitter) fn emit_conditional(&mut self, node: &Node) {
        let Some(cond) = self.arena.get_conditional_expr(node) else {
            return;
        };

        let prev = self.ctx.flags.in_binary_operand;
        self.ctx.flags.in_binary_operand = true;

        // Detect newlines between the three operands in source to preserve
        // multiline ternary formatting (tsc preserves these line breaks).
        let (newline_before_question, newline_before_colon) =
            self.detect_conditional_newlines(cond.condition, cond.when_true, cond.when_false);

        // When lowering optional chains or nullish coalescing in the condition
        // (e.g., `o?.b ? 1 : 0` → `(o === null ... : o.b) ? 1 : 0`,
        //  `(a ?? 'foo') ? 1 : 2` → `(a !== null && a !== void 0 ? a : 'foo') ? 1 : 2`),
        // the ternary must be wrapped in parens to avoid ambiguity with
        // the outer conditional's `?`.
        let prev_optional = self.ctx.flags.optional_chain_needs_parens;
        let prev_nullish = self.ctx.flags.nullish_coalescing_needs_parens;
        self.ctx.flags.optional_chain_needs_parens = true;
        self.ctx.flags.nullish_coalescing_needs_parens = true;
        self.emit(cond.condition);
        self.ctx.flags.optional_chain_needs_parens = prev_optional;
        self.ctx.flags.nullish_coalescing_needs_parens = prev_nullish;
        // The true/false branches of a conditional don't need yield parens
        // because the ternary operator has very low precedence.
        self.ctx.flags.in_binary_operand = false;

        if newline_before_question {
            // Check if `?` is on the condition line (Case A) or the next line (Case B).
            // Case A: `a ?\n    b` → `?` before newline → emit `a ?` then newline + indent
            // Case B: `a\n    ? b` → `?` after newline → emit `a` then newline + indent + `?`
            let question_on_condition_line =
                self.question_on_condition_line(cond.condition, cond.when_true);
            if question_on_condition_line {
                // Case A: `?` trails on condition line — e.g.:
                //   var v = a ?
                //       b : c;
                self.write(" ?");
                self.write_line();
                self.increase_indent();
                self.emit(cond.when_true);
                if newline_before_colon {
                    let colon_on_true_line =
                        self.colon_on_true_line(cond.when_true, cond.when_false);
                    if colon_on_true_line {
                        // `:` trails on when_true line: `b :\n    c`
                        self.write(" :");
                        self.write_line();
                        self.emit(cond.when_false);
                    } else {
                        // `:` on new line: `b\n    : c`
                        self.write_line();
                        self.write(": ");
                        self.emit(cond.when_false);
                    }
                } else {
                    self.write(" : ");
                    self.emit(cond.when_false);
                }
                self.decrease_indent();
            } else {
                // Case B: `?` starts on new line — e.g.:
                //   var v = a
                //       ? b
                //       : c;
                self.write_line();
                self.increase_indent();
                self.write("? ");
                self.emit(cond.when_true);
                if newline_before_colon {
                    self.write_line();
                    self.write(": ");
                } else {
                    self.write(" : ");
                }
                self.emit(cond.when_false);
                self.decrease_indent();
            }
        } else if newline_before_colon {
            self.write(" ? ");
            self.emit(cond.when_true);
            let colon_on_new_line = !self.colon_on_true_line(cond.when_true, cond.when_false);
            if colon_on_new_line {
                // Newline before `:` — e.g.:
                //   var v = a ? b
                //       : c;
                self.write_line();
                self.increase_indent();
                self.write(": ");
                self.emit(cond.when_false);
                self.decrease_indent();
            } else {
                // `:` trailing on same line, alternate on next — e.g.:
                //   var v = a ? b :
                //       c;
                self.write(" :");
                self.write_line();
                self.increase_indent();
                self.emit(cond.when_false);
                self.decrease_indent();
            }
        } else {
            self.write(" ? ");
            self.emit(cond.when_true);
            self.write(" : ");
            self.emit(cond.when_false);
        }

        self.ctx.flags.in_binary_operand = prev;
    }

    /// Detect whether the source text has newlines between the parts of a
    /// conditional expression. Returns (`newline_before_question`, `newline_before_colon`).
    ///
    /// The parser's node.end extends past trailing trivia, so we scan the
    /// source text from condition.end forward, tracking ternary nesting depth,
    /// to find the OUTER `?` and `:` positions reliably.
    fn detect_conditional_newlines(
        &self,
        condition: NodeIndex,
        when_true: NodeIndex,
        _when_false: NodeIndex,
    ) -> (bool, bool) {
        let Some(text) = self.source_text else {
            return (false, false);
        };
        let cond_node = self.arena.get(condition);
        let true_node = self.arena.get(when_true);

        // For the `?`: check if there's a newline between the condition's actual
        // token end and when_true.pos.  The parser's condition.end can overshoot
        // past trivia AND the `?` token (since scanner.pos includes lookahead).
        // Use find_token_end_before_trivia with the `?` position as the upper
        // bound to get the condition's true last-token end.
        let newline_before_question = match (cond_node, true_node) {
            (Some(c), Some(t)) => {
                let range_end = std::cmp::min(t.pos as usize, text.len());
                let range_start = std::cmp::min(c.pos as usize, text.len());
                if range_start >= range_end {
                    false
                } else {
                    // Find the `?` scanning backward from the when_true node
                    let bytes = text.as_bytes();
                    let mut q_pos = None;
                    let mut j = range_end;
                    while j > range_start {
                        j -= 1;
                        if bytes[j] == b'?' {
                            q_pos = Some(j);
                            break;
                        }
                    }
                    if let Some(qp) = q_pos {
                        // Get the actual end of the condition content (before `?`)
                        let cond_end = self.find_token_end_before_trivia(c.pos, qp as u32) as usize;
                        let cond_end = std::cmp::min(cond_end, text.len());
                        cond_end < range_end && text[cond_end..range_end].contains('\n')
                    } else {
                        text[range_start..range_end].contains('\n')
                    }
                }
            }
            _ => false,
        };

        // For the `:`: find it by scanning backward from when_false.pos, then
        // check for a newline between the when_true content and the `:` position.
        let false_node = self.arena.get(_when_false);
        let newline_before_colon = match (true_node, false_node) {
            (Some(t), Some(f)) => {
                let bytes = text.as_bytes();
                let f_pos = std::cmp::min(f.pos as usize, bytes.len());
                // Find `:` scanning backward from when_false.pos
                let mut colon_pos = None;
                let mut j = f_pos;
                while j > t.pos as usize {
                    j -= 1;
                    if bytes[j] == b':' {
                        colon_pos = Some(j);
                        break;
                    }
                }
                if let Some(cp) = colon_pos {
                    // Get actual end of when_true content (before `:`)
                    let true_end = self.find_token_end_before_trivia(t.pos, cp as u32) as usize;
                    let true_end = std::cmp::min(true_end, text.len());
                    true_end < f_pos && text[true_end..f_pos].contains('\n')
                } else {
                    false
                }
            }
            _ => false,
        };

        (newline_before_question, newline_before_colon)
    }

    /// Check whether the `?` token in a conditional expression is on the
    /// condition's line (before the newline) or on the next line.
    /// Returns `true` for `a ?\n  b` (Case A), `false` for `a\n  ? b` (Case B).
    fn question_on_condition_line(&self, condition: NodeIndex, when_true: NodeIndex) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let Some(c) = self.arena.get(condition) else {
            return false;
        };
        let Some(t) = self.arena.get(when_true) else {
            return false;
        };
        let bytes = text.as_bytes();
        // Scan backward from when_true.pos to find the `?` operator
        let t_pos = std::cmp::min(t.pos as usize, bytes.len());
        let c_pos = c.pos as usize;
        let mut q_pos = None;
        let mut j = t_pos;
        while j > c_pos {
            j -= 1;
            if bytes[j] == b'?' {
                q_pos = Some(j);
                break;
            }
        }
        let Some(qp) = q_pos else { return false };
        // Get actual end of condition content, then check for newline between it and `?`
        let cond_end = self.find_token_end_before_trivia(c.pos, qp as u32) as usize;
        let cond_end = std::cmp::min(cond_end, bytes.len());
        // `?` is on condition line if NO newline between cond_end and `?`
        !text[cond_end..qp].contains('\n')
    }

    /// Check whether the `:` token in a conditional expression is on the
    /// `when_true` expression's line (before the newline), as in `b :\n  c`.
    /// Returns `true` for trailing colon: `b :\n  c`.
    /// Returns `false` for leading colon: `b\n  : c`.
    fn colon_on_true_line(&self, when_true: NodeIndex, when_false: NodeIndex) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let Some(t) = self.arena.get(when_true) else {
            return false;
        };
        let Some(f) = self.arena.get(when_false) else {
            return false;
        };
        let bytes = text.as_bytes();
        // Scan backward from when_false.pos to find the `:` operator
        let f_pos = std::cmp::min(f.pos as usize, bytes.len());
        let t_pos = t.pos as usize;
        let mut colon_pos = None;
        let mut j = f_pos;
        while j > t_pos {
            j -= 1;
            if bytes[j] == b':' {
                colon_pos = Some(j);
                break;
            }
        }
        let Some(cp) = colon_pos else { return false };
        // Get actual end of when_true content, then check for newline between it and `:`
        let true_end = self.find_token_end_before_trivia(t.pos, cp as u32) as usize;
        let true_end = std::cmp::min(true_end, bytes.len());
        // `:` is on true line if NO newline between true_end and `:`
        !text[true_end..cp].contains('\n')
    }
}

#[cfg(test)]
mod tests {
    use crate::output::printer::{PrintOptions, Printer};
    use tsz_parser::ParserState;

    /// Dynamic `import('path')` expressions must emit the `import` keyword.
    /// Previously the emitter's `emit_node_by_kind` dispatch had no handler for
    /// `SyntaxKind::ImportKeyword`, so the keyword was silently dropped and the
    /// output became just `('path')`.
    #[test]
    fn dynamic_import_emits_import_keyword() {
        let source = r#"const m = import("./module");"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains(r#"import("./module")"#),
            "Dynamic import must emit the 'import' keyword.\nOutput:\n{output}"
        );
    }

    /// `import.meta` property access must emit the `import` keyword.
    #[test]
    fn import_meta_emits_import_keyword() {
        let source = r#"const url = import.meta.url;"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("import.meta.url"),
            "import.meta must emit the 'import' keyword.\nOutput:\n{output}"
        );
    }

    /// Dynamic import inside an async function body.
    #[test]
    fn dynamic_import_in_async_function() {
        let source = r#"async function load() { return await import("./lib"); }"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains(r#"import("./lib")"#),
            "Dynamic import inside async function must emit 'import' keyword.\nOutput:\n{output}"
        );
    }

    /// When async functions are lowered to generator functions (ES2015 target),
    /// `await expr` becomes `yield expr`. Yield has lower precedence than most
    /// operators, so it needs parens inside binary operators like `||`:
    /// `await p || a` → `(yield p) || a`. But assignment RHS and comma
    /// expression operands accept `AssignmentExpression` (which includes yield),
    /// so no extra parens are needed there.
    #[test]
    fn yield_from_await_no_extra_parens_in_assignment_rhs() {
        let source = r#"async function func() { o.a = await p; }"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("o.a = yield p;"),
            "yield-from-await in assignment RHS must NOT have extra parens.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("(yield p)"),
            "yield-from-await in assignment RHS should not be wrapped in parens.\nOutput:\n{output}"
        );
    }

    /// Yield-from-await in comma expression LHS should not have extra parens.
    /// `(await p, a)` → `(yield p, a)`, NOT `((yield p), a)`.
    #[test]
    fn yield_from_await_no_extra_parens_in_comma_expr() {
        let source = r#"async function func() { var b = (await p, a); }"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("(yield p, a)"),
            "yield-from-await in comma expression must NOT have extra parens.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("((yield p)"),
            "yield-from-await should not be double-wrapped.\nOutput:\n{output}"
        );
    }

    /// Yield-from-await inside a binary operator like `||` still NEEDS parens.
    /// `await p || a` → `(yield p) || a` (otherwise it would parse as `yield (p || a)`).
    #[test]
    fn yield_from_await_keeps_parens_in_binary_operator() {
        let source = r#"async function func() { var b = await p || a; }"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("(yield p) || a"),
            "yield-from-await in || operand MUST have parens for correct precedence.\nOutput:\n{output}"
        );
    }

    /// Preserve spacing and ordering around comments in `yield` expressions.
    #[test]
    fn yield_expression_comments_preserve_expected_spacing() {
        let source = r#"function * foo2() {
            /*comment1*/ yield 1;
            yield /*comment2*/ 2;
            yield 3 /*comment3*/
            yield */*comment4*/ [4];
            yield /*comment5*/* [5];
        }"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("/*comment1*/ yield 1;"),
            "Leading comment before `yield` should stay before keyword with spacing.\nOutput:\n{output}"
        );
        assert!(
            output.contains("yield /*comment2*/ 2"),
            "Inline comment after `yield` should keep a single separating space.\nOutput:\n{output}"
        );
        assert!(
            output.contains("yield 3; /*comment3*/"),
            "Trailing comment should remain after expression when `yield` has no right operand.\nOutput:\n{output}"
        );
        assert!(
            output.contains("yield* /*comment4*/ [4]"),
            "Comment after `yield*` should stay after `*`.\nOutput:\n{output}"
        );
        assert!(
            output.contains("yield /*comment5*/* [5]"),
            "Comment before `yield*` operator should stay before `*`.\nOutput:\n{output}"
        );
    }

    #[test]
    fn yield_without_operand_has_no_trailing_space() {
        let source = "function* foo() {\n    yield;\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("yield;"),
            "Yield without an operand must keep tight `yield;` form.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("yield ;"),
            "Yield without an operand must not include a separating space.\nOutput:\n{output}"
        );
    }

    /// When a parenthesized type assertion wraps a line comment between `yield`
    /// and its operand, the parens must be preserved to prevent ASI.
    /// `yield (// comment\n a as any)` -> `yield (\n// comment\n a)` (not `yield // comment\n a`)
    #[test]
    fn yield_preserves_parens_for_line_comment_in_type_assertion() {
        let source =
            "function *t1() {\n    yield (\n        // comment\n        a as any\n    );\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("yield ("),
            "yield with line comment before operand must preserve opening paren.\nOutput:\n{output}"
        );
        assert!(
            output.contains("// comment"),
            "Line comment must be preserved in output.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("yield // comment"),
            "yield must not be directly followed by the line comment (ASI hazard).\nOutput:\n{output}"
        );
    }

    /// Block comments on the same line as a statement must have a space after `*/`.
    /// This ensures `/*comment*/ var x` rather than `/*comment*/var x`.
    #[test]
    fn inline_block_comment_before_statement_gets_trailing_space() {
        // A block comment on the same line as a var declaration
        let source = "{\n    /*comment*/ var x = 1;\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("/*comment*/ var"),
            "Inline block comment must have a space before the next token.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("/*comment*/var"),
            "Block comment must not be glued to the next token.\nOutput:\n{output}"
        );
    }

    /// Multiline ternary: colon trailing on previous line, alternate on next.
    /// `a ? b :\n    c` must preserve the line break after `:`.
    #[test]
    fn conditional_preserves_newline_after_colon() {
        let source = "var v = a ? b :\n  c;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("a ? b :\n"),
            "Ternary with colon trailing must preserve newline after `:`.\nOutput:\n{output}"
        );
        assert!(
            output.contains("    c"),
            "Alternate must be indented on the new line.\nOutput:\n{output}"
        );
    }

    /// Multiline ternary: colon leading on new line.
    /// `a ? b\n    : c` must preserve the line break before `:`.
    #[test]
    fn conditional_preserves_newline_before_colon() {
        let source = "var v = a ? b\n  : c;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("a ? b\n"),
            "Ternary with colon leading must preserve newline before `:`.\nOutput:\n{output}"
        );
        assert!(
            output.contains("    : c"),
            "Colon must lead on the new indented line.\nOutput:\n{output}"
        );
    }

    /// Multiline ternary: both `?` and `:` on new lines.
    /// `a\n    ? b\n    : c` must preserve both line breaks.
    #[test]
    fn conditional_preserves_newline_before_question_and_colon() {
        let source = "var v = a\n  ? b\n  : c;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("a\n"),
            "Must preserve newline after condition.\nOutput:\n{output}"
        );
        assert!(
            output.contains("    ? b\n"),
            "Question mark must lead on the new indented line.\nOutput:\n{output}"
        );
        assert!(
            output.contains("    : c"),
            "Colon must lead on the new indented line.\nOutput:\n{output}"
        );
    }

    /// Type assertion around a call expression should strip parens:
    /// `(<any>a.b()).c` → `a.b().c` (not `(a.b()).c`).
    #[test]
    fn type_assertion_call_expression_strips_parens() {
        let source = "var b = (<any>a.b()).c;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("a.b().c"),
            "Parens around type-asserted call expression should be stripped.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("(a.b()).c"),
            "Should not have redundant parens around call expression.\nOutput:\n{output}"
        );
    }

    /// Type assertion around `new` expression strips parens when not in access position:
    /// `(<any>new a)` → `new a`.
    #[test]
    fn type_assertion_new_expression_strips_parens() {
        let source = "var b = (<any>new a);\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("var b = new a;"),
            "Parens around type-asserted new expression should be stripped.\nOutput:\n{output}"
        );
    }

    /// Type assertion around `new a.b` strips parens when not in access position:
    /// `(<any>new a.b)` → `new a.b`.
    #[test]
    fn type_assertion_new_expression_with_member_strips_parens() {
        let source = "var b = (<any>new a.b);\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("var b = new a.b;"),
            "Parens around type-asserted new a.b should be stripped.\nOutput:\n{output}"
        );
    }

    /// Type assertion around `new a` keeps parens when in property access position:
    /// `(<any>new a).b` → `(new a).b` (removing parens would change semantics).
    #[test]
    fn type_assertion_new_expression_keeps_parens_in_access() {
        let source = "var b = (<any>new a).b;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("(new a).b"),
            "Parens around new expression in access position must be preserved.\nOutput:\n{output}"
        );
    }

    /// Type assertion around call expression in `new` callee position keeps parens:
    /// `new (x() as any)` → `new (x())` (not `new x()` which has different semantics).
    #[test]
    fn type_assertion_call_in_new_callee_keeps_parens() {
        let source = "new (x() as any);\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("new (x())"),
            "Parens around call expression in new callee must be preserved.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("new x()"),
            "Should NOT strip parens to `new x()` (different semantics).\nOutput:\n{output}"
        );
    }

    /// `as` type assertion around call expression in `new` callee position keeps parens:
    /// `new (x() as any)` → `new (x())`.
    #[test]
    fn as_assertion_call_in_new_callee_keeps_parens() {
        // Use angle-bracket style too: `new (<any>x())`
        let source = "new (<any>x());\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("new (x())"),
            "Parens around angle-bracket-asserted call in new callee must be preserved.\nOutput:\n{output}"
        );
    }

    /// Call expressions with type assertions outside `new` context still strip parens:
    /// `(<any>x()).foo` → `x().foo`.
    #[test]
    fn type_assertion_call_outside_new_still_strips_parens() {
        let source = "var b = (<any>x()).foo;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("x().foo"),
            "Parens around type-asserted call in access position should still strip.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("(x()).foo"),
            "Should not have redundant parens.\nOutput:\n{output}"
        );
    }

    /// When lowering nullish coalescing (`??`) to ES2019 and below for complex
    /// (non-identifier) LHS expressions, the emitter uses a temp variable:
    /// `(temp = f()) !== null && temp !== void 0 ? temp : 'fallback'`
    /// This temp must be declared as `var _a;` at the top of the enclosing scope.
    #[test]
    fn nullish_coalescing_emits_hoisted_temp_var_decl() {
        // Top-level: hoisted temp goes at file scope
        let source = "let gg = f() ?? 'foo';\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("var _a;"),
            "Nullish coalescing lowering must emit `var _a;` for the hoisted temp.\nOutput:\n{output}"
        );
        assert!(
            output.contains("(_a = f())"),
            "Nullish coalescing lowering must use temp in assignment.\nOutput:\n{output}"
        );
    }

    /// Nested unary `+` operators must be separated by a space to prevent
    /// `+ +y` from collapsing to `++y` (pre-increment).
    #[test]
    fn prefix_plus_plus_gets_space() {
        let source = "var z = + +y;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("+ +y"),
            "Nested unary `+` must have space between to avoid `++y`.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("++y"),
            "Must NOT collapse `+ +y` into `++y` (pre-increment).\nOutput:\n{output}"
        );
    }

    /// Nested unary `-` operators must be separated by a space to prevent
    /// `- -y` from collapsing to `--y` (pre-decrement).
    #[test]
    fn prefix_minus_minus_gets_space() {
        let source = "var c = - -y;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("- -y"),
            "Nested unary `-` must have space between to avoid `--y`.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("--y"),
            "Must NOT collapse `- -y` into `--y` (pre-decrement).\nOutput:\n{output}"
        );
    }

    /// Unary `+` before `++` must insert a space: `+ ++x` not `+++x`.
    #[test]
    fn prefix_plus_before_increment_gets_space() {
        let source = "var z = + ++x;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("+ ++x"),
            "Unary `+` before `++x` must have space.\nOutput:\n{output}"
        );
    }

    // =====================================================================
    // Case A ternary formatting tests (question on condition line)
    // =====================================================================

    /// Case A with trailing colon: `a ?\n  b :\n  c` → `a ?\n    b :\n    c`
    /// This is the conditionalExpressionNewLine7 pattern.
    #[test]
    fn conditional_case_a_trailing_colon() {
        let source = "var v = a ?\n  b :\n  c;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("a ?\n"),
            "Case A: `?` must trail on condition line.\nOutput:\n{output}"
        );
        assert!(
            output.contains("    b :\n"),
            "Case A: `:` must trail on when_true line.\nOutput:\n{output}"
        );
        assert!(
            output.contains("    c;"),
            "Case A: when_false must be indented on new line.\nOutput:\n{output}"
        );
    }

    /// Case A with same-line colon: `a ?\n  b : c` → `a ?\n    b : c`
    #[test]
    fn conditional_case_a_inline_colon() {
        let source = "var v = a ?\n  b : c;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("a ?\n"),
            "Case A: `?` must trail on condition line.\nOutput:\n{output}"
        );
        assert!(
            output.contains("    b : c;"),
            "Case A: `:` and when_false inline.\nOutput:\n{output}"
        );
    }

    /// Case B with nested ternaries: `a\n  ? b ? d : e\n  : c ? f : g`
    #[test]
    fn conditional_case_b_nested_ternaries() {
        let source = "var v = a\n  ? b ? d : e\n  : c ? f : g;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("    ? b ? d : e\n"),
            "Case B: nested when_true must be on indented line.\nOutput:\n{output}"
        );
        assert!(
            output.contains("    : c ? f : g;"),
            "Case B: nested when_false must be on indented line.\nOutput:\n{output}"
        );
    }

    /// When `??` is lowered in a binary expression operand (e.g., `(a ?? b) || c`),
    /// the lowered ternary must be wrapped in parens to preserve precedence.
    /// Without parens: `a !== null && a !== void 0 ? a : b || c` (wrong — `||` binds to `b`)
    /// With parens: `(a !== null && a !== void 0 ? a : b) || c` (correct)
    #[test]
    fn nullish_coalescing_in_binary_gets_parens() {
        // a ?? b || c — the ?? is the left operand of ||, needs parens when lowered
        let source = "a ?? b || c;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("(a !== null && a !== void 0 ? a : b) || c"),
            "Lowered ?? in binary operand must be wrapped in parens.\nOutput:\n{output}"
        );
    }

    /// When `??` is lowered in the condition of a ternary, the lowered ternary
    /// must be wrapped in parens to avoid ambiguity with the outer `?:`.
    /// e.g., `a ?? 'foo' ? 1 : 2` → `(a !== null && a !== void 0 ? a : 'foo') ? 1 : 2`
    #[test]
    fn nullish_coalescing_in_conditional_condition_gets_parens() {
        let source = "const r = a ?? 'foo' ? 1 : 2;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("(a !== null && a !== void 0 ? a : 'foo') ? 1 : 2"),
            "Lowered ?? in conditional condition must be wrapped in parens.\nOutput:\n{output}"
        );
    }

    /// When the source already has explicit parens `(a ?? b)`, the lowered ternary
    /// must NOT be double-parenthesized. The `ParenthesizedExpression` provides the
    /// outer parens; the `nullish_coalescing_needs_parens` flag is cleared inside.
    #[test]
    fn nullish_coalescing_with_explicit_parens_no_double_wrap() {
        // Source has explicit parens: (a ?? b) || c
        let source = "(a ?? b) || c;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        // Should have single parens, not double
        assert!(
            output.contains("(a !== null && a !== void 0 ? a : b) || c"),
            "Must have single parens from source ParenthesizedExpression.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("((a !== null"),
            "Must NOT have double parens.\nOutput:\n{output}"
        );
    }
}
