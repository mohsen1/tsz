//! Binary operator error reporting (TS2362, TS2363, TS2365, TS2469).

use super::fingerprint_policy::DiagnosticRenderRequest;
use crate::diagnostics::diagnostic_codes;
use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Report TS2351: "This expression is not constructable. Type 'X' has no construct signatures."
    /// This is for `new` expressions where the expression type has no construct signatures.
    pub fn error_not_constructable_at(&mut self, type_id: TypeId, idx: NodeIndex) {
        if type_id == TypeId::ERROR || type_id == TypeId::ANY || type_id == TypeId::UNKNOWN {
            return;
        }

        let mut formatter = self.ctx.create_type_formatter();
        let type_str = formatter.format(type_id);

        self.emit_render_request(
            idx,
            DiagnosticRenderRequest::simple_msg(
                diagnostic_codes::THIS_EXPRESSION_IS_NOT_CONSTRUCTABLE,
                &[&type_str],
            ),
        );
    }

    // =========================================================================
    // Binary Operator Errors
    // =========================================================================

    /// Emits TS18050 or TS18048/TS18047 for null/undefined operands in binary operations.
    ///
    /// tsc distinguishes between:
    /// - **TS18050**: The literal `undefined`/`null` keyword is used directly (e.g., `undefined < 3`)
    /// - **TS18048**: A variable whose type is `undefined` (e.g., `x < 3` where `x: undefined`)
    /// - **TS18047**: A variable whose type is `null` (e.g., `x < 3` where `x: null`)
    pub(crate) fn check_and_emit_nullish_binary_operands(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
        left_type: TypeId,
        right_type: TypeId,
        op: &str,
    ) -> bool {
        if left_type == TypeId::ERROR
            || right_type == TypeId::ERROR
            || left_type == TypeId::UNKNOWN
            || right_type == TypeId::UNKNOWN
        {
            return false;
        }

        // For `+`, tsc generally bails out on nullish checks when one side is `any`.
        // But in chained arithmetic like `a + b + c`, the left side can become `any`
        // after reporting on `b`, and tsc still reports on `c`.
        if (left_type == TypeId::ANY || right_type == TypeId::ANY) && op == "+" {
            let left_any_from_nested_binary = left_type == TypeId::ANY
                && self
                    .ctx
                    .arena
                    .get(left_idx)
                    .is_some_and(|n| n.kind == syntax_kind_ext::BINARY_EXPRESSION);
            let right_any_from_nested_binary = right_type == TypeId::ANY
                && self
                    .ctx
                    .arena
                    .get(right_idx)
                    .is_some_and(|n| n.kind == syntax_kind_ext::BINARY_EXPRESSION);
            if !left_any_from_nested_binary && !right_any_from_nested_binary {
                return false;
            }
        }

        // Without strictNullChecks, null/undefined are in every type's domain (assignable
        // to number), so tsc does NOT emit TS18050 for binary operations.
        // Note: TS18050 for property access on literal null/undefined (`null.foo`) is
        // independent of strictNullChecks and handled separately in property_access_type.rs.
        if !self.ctx.compiler_options.strict_null_checks {
            return false;
        }

        // Standalone `void` should not produce TS18048/TS18047 in binary operators.
        // tsc handles void-typed variables through operator-specific checks (TS18050,
        // TS2362, TS2363, TS2365, etc.) rather than through the nullish operand path.
        // Only `void` inside unions (e.g., `string | void`) should be treated as nullable.
        let (_, left_cause) = if left_type == TypeId::VOID {
            (None, None)
        } else {
            self.split_nullish_type(left_type)
        };
        let (_, right_cause) = if right_type == TypeId::VOID {
            (None, None)
        } else {
            self.split_nullish_type(right_type)
        };
        let left_is_nullish = left_cause.is_some();
        let right_is_nullish = right_cause.is_some();
        let mut emitted_nullish_error = false;
        let should_emit_nullish_error = matches!(
            op,
            "+" | "-"
                | "*"
                | "/"
                | "%"
                | "**"
                | "&"
                | "|"
                | "^"
                | "<<"
                | ">>"
                | ">>>"
                | "<"
                | ">"
                | "<="
                | ">="
        );

        // For the `+` operator, tsc suppresses TS18050 when the other operand is a
        // string type — `+` becomes string concatenation, and null/undefined are
        // coerced to "null"/"undefined" strings. Only arithmetic `+` (both operands
        // number/bigint/enum) should emit TS18050.
        if op == "+" && should_emit_nullish_error {
            if left_is_nullish && self.is_string_like_type(right_type) {
                return false;
            }
            if right_is_nullish && self.is_string_like_type(left_type) {
                return false;
            }
        }

        if let Some(cause) = left_cause
            && should_emit_nullish_error
        {
            self.emit_nullish_operand_error(left_idx, cause);
            emitted_nullish_error = true;
        }

        if let Some(cause) = right_cause
            && should_emit_nullish_error
        {
            self.emit_nullish_operand_error(right_idx, cause);
            emitted_nullish_error = true;
        }

        emitted_nullish_error
    }

    /// Emit the appropriate diagnostic for a nullish binary operand.
    ///
    /// - If the expression is the literal `null`/`undefined` keyword → TS18050
    /// - If the expression is a variable with a null/undefined type → TS18048/TS18047
    pub(crate) fn emit_nullish_operand_error(&mut self, idx: NodeIndex, cause: TypeId) {
        // When TS2454 (variable used before being assigned) has already been
        // emitted for this expression, suppress TS18047/18048/18049.  tsc does
        // not stack "possibly undefined" on top of "used before assignment".
        if self.ctx.daa_error_nodes.contains(&idx.0) {
            return;
        }

        let is_literal = self.is_literal_null_or_undefined_node(idx);

        if is_literal {
            // Literal null/undefined keyword → TS18050 "The value 'X' cannot be used here."
            let value_name = if cause == TypeId::NULL {
                "null"
            } else if cause == TypeId::UNDEFINED {
                "undefined"
            } else {
                "null | undefined"
            };
            self.emit_render_request(
                idx,
                DiagnosticRenderRequest::simple_msg(
                    diagnostic_codes::THE_VALUE_CANNOT_BE_USED_HERE,
                    &[value_name],
                ),
            );
        } else {
            if let Some(node) = self.ctx.arena.get(idx) {
                if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                    if let Some(name) = self.expression_text(idx) {
                        let code = if cause == TypeId::NULL {
                            diagnostic_codes::IS_POSSIBLY_NULL
                        } else if cause == TypeId::UNDEFINED {
                            diagnostic_codes::IS_POSSIBLY_UNDEFINED
                        } else {
                            diagnostic_codes::IS_POSSIBLY_NULL_OR_UNDEFINED
                        };
                        self.emit_render_request(
                            idx,
                            DiagnosticRenderRequest::simple_msg(code, &[&name]),
                        );
                        return;
                    }
                } else if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
                    self.report_nullish_object(idx, cause, false);
                    return;
                }
            }

            // Variable/expression with nullish type → TS18047/TS18048/TS18049
            let name = self.expression_text(idx);

            if let Some(ref name) = name {
                let code = if cause == TypeId::NULL {
                    diagnostic_codes::IS_POSSIBLY_NULL
                } else if cause == TypeId::UNDEFINED {
                    diagnostic_codes::IS_POSSIBLY_UNDEFINED
                } else {
                    diagnostic_codes::IS_POSSIBLY_NULL_OR_UNDEFINED
                };
                self.emit_render_request(idx, DiagnosticRenderRequest::simple_msg(code, &[name]));
            } else {
                // Non-identifier expression with nullish type — fall back to TS18050
                let value_name = if cause == TypeId::NULL {
                    "null"
                } else if cause == TypeId::UNDEFINED {
                    "undefined"
                } else {
                    "null | undefined"
                };
                self.emit_render_request(
                    idx,
                    DiagnosticRenderRequest::simple_msg(
                        diagnostic_codes::THE_VALUE_CANNOT_BE_USED_HERE,
                        &[value_name],
                    ),
                );
            }
        }
    }

    /// Check if a type is string-like (intrinsic `string` or a string literal).
    /// Used to determine if `+` is string concatenation rather than arithmetic.
    fn is_string_like_type(&self, type_id: TypeId) -> bool {
        type_id == TypeId::STRING
            || crate::query_boundaries::checkers::iterable::is_string_literal_type(
                self.ctx.types,
                type_id,
            )
    }

    pub(crate) fn operator_operand_may_include_bigint(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::ANY || type_id == TypeId::ERROR || type_id == TypeId::UNKNOWN {
            return false;
        }

        let widened = crate::query_boundaries::common::widen_literal_type(self.ctx.types, type_id);
        if widened == TypeId::BIGINT {
            return true;
        }

        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, type_id)
        {
            return members
                .iter()
                .any(|&member| self.operator_operand_may_include_bigint(member));
        }

        if let Some(constraint) =
            crate::query_boundaries::common::type_parameter_constraint(self.ctx.types, type_id)
            && constraint != type_id
            && constraint != TypeId::UNKNOWN
        {
            return self.operator_operand_may_include_bigint(constraint);
        }

        false
    }

    pub(crate) fn operator_error_result_type(
        &self,
        left_type: TypeId,
        right_type: TypeId,
        fallback_without_bigint: TypeId,
    ) -> TypeId {
        if self.operator_operand_may_include_bigint(left_type)
            || self.operator_operand_may_include_bigint(right_type)
        {
            TypeId::ANY
        } else {
            fallback_without_bigint
        }
    }

    pub(crate) fn operator_surface_type_for_expression(
        &mut self,
        idx: NodeIndex,
        fallback: TypeId,
    ) -> TypeId {
        if self
            .ctx
            .arena
            .get(idx)
            .is_some_and(|node| node.kind == SyntaxKind::Identifier as u16)
            && let Some(sym_id) = self.resolve_identifier_symbol(idx)
        {
            let declared = self.get_type_of_symbol(sym_id);
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && let Some(decl_idx) = symbol.primary_declaration()
                && let Some(parameter_idx) = self.ctx.arena.get(decl_idx).and_then(|decl_node| {
                    if decl_node.kind == syntax_kind_ext::PARAMETER {
                        Some(decl_idx)
                    } else {
                        self.ctx.arena.get_extended(decl_idx).map(|ext| ext.parent)
                    }
                })
                && let Some(decl_node) = self.ctx.arena.get(parameter_idx)
                && let Some(parameter) = self.ctx.arena.get_parameter(decl_node)
                && parameter.type_annotation.is_some()
                && let Some(annotation_node) = self.ctx.arena.get(parameter.type_annotation)
                && let Some(type_ref) = self.ctx.arena.get_type_ref(annotation_node)
                && let TypeSymbolResolution::Type(annotation_sym_id) =
                    self.resolve_identifier_symbol_in_type_position(type_ref.type_name)
                && self
                    .ctx
                    .binder
                    .get_symbol(annotation_sym_id)
                    .is_some_and(|symbol| {
                        symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_PARAMETER)
                    })
            {
                let annotation_type = self.get_type_of_symbol(annotation_sym_id);
                if crate::query_boundaries::common::type_param_info(self.ctx.types, annotation_type)
                    .is_some()
                    && self.operator_operand_may_include_bigint(annotation_type)
                {
                    return annotation_type;
                }
            }
            if declared != TypeId::ERROR
                && declared != TypeId::UNKNOWN
                && self.operator_operand_may_include_bigint(declared)
            {
                return declared;
            }
        }
        fallback
    }

    pub(crate) fn operator_type_parameter_annotation_text_for_expression(
        &self,
        idx: NodeIndex,
    ) -> Option<String> {
        if self
            .ctx
            .arena
            .get(idx)
            .is_none_or(|node| node.kind != SyntaxKind::Identifier as u16)
        {
            return None;
        }
        let sym_id = self.resolve_identifier_symbol_without_tracking(idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        for decl_idx in symbol.all_declarations() {
            let mut current = decl_idx;
            for _ in 0..=2 {
                let Some(node) = self.ctx.arena.get(current) else {
                    break;
                };
                if node.kind == syntax_kind_ext::PARAMETER
                    && let Some(parameter) = self.ctx.arena.get_parameter(node)
                    && parameter.type_annotation.is_some()
                    && let Some(annotation_node) = self.ctx.arena.get(parameter.type_annotation)
                    && let Some(source) = self.ctx.arena.source_files.first()
                    && let Some(text) = source
                        .text
                        .get(annotation_node.pos as usize..annotation_node.end as usize)
                {
                    let text = text.trim();
                    if text.len() <= 3
                        && text
                            .chars()
                            .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
                    {
                        return Some(text.to_string());
                    }
                }
                let Some(parent) = self.ctx.arena.get_extended(current).map(|ext| ext.parent)
                else {
                    break;
                };
                if parent.is_none() {
                    break;
                }
                current = parent;
            }
        }
        None
    }

    /// Emit errors for binary operator type mismatches.
    /// TS2362 for left-hand side, TS2363 for right-hand side, or TS2365 for general operator errors.
    pub(crate) fn emit_binary_operator_error(
        &mut self,
        node_idx: NodeIndex,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
        left_type: TypeId,
        right_type: TypeId,
        op: &str,
        emitted_nullish_error: bool,
    ) {
        // tsc suppresses binary operator type errors in files with parse errors
        // to avoid cascading diagnostics from malformed AST nodes.
        if self.has_parse_errors() {
            return;
        }

        // Suppress cascade errors from unresolved types
        if left_type == TypeId::ERROR
            || right_type == TypeId::ERROR
            || left_type == TypeId::UNKNOWN
            || right_type == TypeId::UNKNOWN
        {
            return;
        }

        // Track nullish operands for proper error reporting
        let left_is_nullish = left_type == TypeId::NULL || left_type == TypeId::UNDEFINED;
        let right_is_nullish = right_type == TypeId::NULL || right_type == TypeId::UNDEFINED;

        // TS18050 for binary ops is gated on strictNullChecks (handled in
        // check_and_emit_nullish_binary_operands). Track which operators would
        // produce TS18050 to suppress redundant TS2362/TS2363 when it was emitted.
        let should_emit_nullish_error = matches!(
            op,
            "+" | "-"
                | "*"
                | "/"
                | "%"
                | "**"
                | "&"
                | "|"
                | "^"
                | "<<"
                | ">>"
                | ">>>"
                | "<"
                | ">"
                | "<="
                | ">="
        );

        let evaluator = crate::query_boundaries::common::new_binary_op_evaluator(self.ctx.types);

        // TS2469: Check if either operand is a symbol type.
        // tsc's behavior for TS2469 varies by operator category:
        //
        // Relational (<, >, <=, >=): emit TS2469 on the first symbol operand, no TS2365.
        // Binary + / +=: emit TS2469 only when one side is symbol and the other is string
        //   or any. If both symbol or symbol+number, fall through to TS2365.
        // Arithmetic (-, *, /, etc.): never TS2469 — use TS2362/TS2363 instead.
        //
        // Also check constraint-resolved types for type parameters like `S extends symbol`.
        // Without this, `S + ''` would emit TS2365 instead of TS2469.
        let resolve_tp_constraint = |type_id: TypeId| -> TypeId {
            crate::query_boundaries::common::type_parameter_constraint(self.ctx.types, type_id)
                .filter(|&c| c != TypeId::UNKNOWN && c != type_id)
                .unwrap_or(type_id)
        };
        // A type is "symbol-like" for TS2469 purposes if it is directly the
        // `symbol` primitive (or a unique symbol), if it is a type parameter
        // whose constraint resolves to one of those, or if it is a union that
        // includes such a member (e.g. `S | symbol` where `S extends string`).
        // tsc emits TS2469 in all of these cases when the other operand is
        // string-like, so we mirror that behavior here.
        let includes_symbol = |type_id: TypeId| -> bool {
            if evaluator.is_symbol_like(type_id)
                || evaluator.is_symbol_like(resolve_tp_constraint(type_id))
            {
                return true;
            }
            let check_union = |t: TypeId| -> bool {
                if let Some(members) =
                    crate::query_boundaries::common::union_members(self.ctx.types, t)
                {
                    members.iter().any(|&m| {
                        evaluator.is_symbol_like(m)
                            || evaluator.is_symbol_like(resolve_tp_constraint(m))
                    })
                } else {
                    false
                }
            };
            check_union(type_id) || check_union(resolve_tp_constraint(type_id))
        };
        let left_is_symbol = includes_symbol(left_type);
        let right_is_symbol = includes_symbol(right_type);

        if left_is_symbol || right_is_symbol {
            let is_relational = matches!(op, "<" | ">" | "<=" | ">=");
            let is_plus_like = matches!(op, "+" | "+=");

            if is_relational {
                // For relational operators: emit TS2469 on the first (leftmost) symbol
                // operand and return — tsc does not also emit TS2365.
                let target_idx = if left_is_symbol { left_idx } else { right_idx };
                self.emit_render_request(
                    target_idx,
                    DiagnosticRenderRequest::simple_msg(
                        diagnostic_codes::THE_OPERATOR_CANNOT_BE_APPLIED_TO_TYPE_SYMBOL,
                        &[op],
                    ),
                );
                return;
            }

            if is_plus_like {
                // For + / +=: emit TS2469 only when one side is symbol and the other
                // is string or any. If both symbol, or symbol+number, fall through to TS2365.
                let left_is_string_or_any =
                    left_type == TypeId::ANY || self.is_string_like_type(left_type);
                let right_is_string_or_any =
                    right_type == TypeId::ANY || self.is_string_like_type(right_type);

                let should_emit_2469 = (left_is_symbol && right_is_string_or_any)
                    || (right_is_symbol && left_is_string_or_any);

                if should_emit_2469 {
                    // Emit TS2469 on each symbol operand
                    if left_is_symbol {
                        self.emit_render_request(
                            left_idx,
                            DiagnosticRenderRequest::simple_msg(
                                diagnostic_codes::THE_OPERATOR_CANNOT_BE_APPLIED_TO_TYPE_SYMBOL,
                                &[op],
                            ),
                        );
                    }
                    if right_is_symbol {
                        self.emit_render_request(
                            right_idx,
                            DiagnosticRenderRequest::simple_msg(
                                diagnostic_codes::THE_OPERATOR_CANNOT_BE_APPLIED_TO_TYPE_SYMBOL,
                                &[op],
                            ),
                        );
                    }
                    return;
                }
                // Otherwise (both symbol, symbol+number): fall through to TS2365
            }

            // For arithmetic/bitwise operators (-, *, /, etc.): do NOT emit TS2469,
            // fall through to TS2362/TS2363 below.
        }

        // tsc uses getTypeOfNode (which widens literals) for TS2365 messages,
        // so literal types are widened to base types (e.g., `1` → `number`).
        // Exception: for `+` operator with number↔bigint mismatch, tsc preserves
        // the literal types (e.g., `1 + 2n` shows `'1' and '2n'`).
        // Enum member types (E.a) should widen to the parent enum (E).
        let is_number_bigint_mix = op == "+"
            && self.literal_type_from_initializer(left_idx).is_some()
            && self.literal_type_from_initializer(right_idx).is_some()
            && {
                let l = self
                    .literal_type_from_initializer(left_idx)
                    .expect("checked is_some above");
                let r = self
                    .literal_type_from_initializer(right_idx)
                    .expect("checked is_some above");
                let l_num = crate::query_boundaries::common::widen_literal_type(self.ctx.types, l)
                    == TypeId::NUMBER
                    || crate::query_boundaries::common::widen_literal_type(self.ctx.types, l)
                        == TypeId::BIGINT;
                let r_num = crate::query_boundaries::common::widen_literal_type(self.ctx.types, r)
                    == TypeId::NUMBER
                    || crate::query_boundaries::common::widen_literal_type(self.ctx.types, r)
                        == TypeId::BIGINT;
                let l_is_bigint =
                    crate::query_boundaries::common::widen_literal_type(self.ctx.types, l)
                        == TypeId::BIGINT;
                let r_is_bigint =
                    crate::query_boundaries::common::widen_literal_type(self.ctx.types, r)
                        == TypeId::BIGINT;
                l_num && r_num && (l_is_bigint != r_is_bigint)
            };

        let left_surface = self.operator_surface_type_for_expression(left_idx, left_type);
        let right_surface = self.operator_surface_type_for_expression(right_idx, right_type);
        let is_unsigned_shift_bigint_mix = op == ">>>"
            && (self.operator_operand_may_include_bigint(left_surface)
                || self.operator_operand_may_include_bigint(right_surface));

        let (left_diag, right_diag) = if is_number_bigint_mix {
            // Preserve literal types for number+bigint mix (e.g., '1' and '2n')
            let l = self
                .literal_type_from_initializer(left_idx)
                .expect("checked is_some above");
            let r = self
                .literal_type_from_initializer(right_idx)
                .expect("checked is_some above");
            (
                self.widen_enum_member_type(l),
                self.widen_enum_member_type(r),
            )
        } else if is_unsigned_shift_bigint_mix {
            let right = self
                .literal_type_from_initializer(right_idx)
                .filter(|&literal| self.operator_operand_may_include_bigint(literal))
                .unwrap_or(right_type);
            (left_surface, right)
        } else {
            // Widen literal types to base types for all other operator errors.
            // Important: try enum member widening BEFORE get_base_type_for_comparison,
            // because the latter unwraps Enum types to their structural member type
            // (e.g., Enum → number), losing the enum identity. tsc displays enum
            // names (e.g., 'E') in operator error messages, not the base type.
            (
                self.widen_type_for_operator_display(left_surface),
                self.widen_type_for_operator_display(right_surface),
            )
        };
        let left_str = if let Some(text) =
            self.operator_type_parameter_annotation_text_for_expression(left_idx)
        {
            text
        } else if is_number_bigint_mix || is_unsigned_shift_bigint_mix {
            self.format_type(left_diag)
        } else {
            self.format_type_for_operator_display(left_diag)
        };
        let right_str = if let Some(text) =
            self.operator_type_parameter_annotation_text_for_expression(right_idx)
        {
            text
        } else if is_number_bigint_mix || is_unsigned_shift_bigint_mix {
            self.format_type(right_diag)
        } else {
            self.format_type_for_operator_display(right_diag)
        };

        // Check if this is an arithmetic or bitwise operator
        // These operators require integer operands and emit TS2362/TS2363
        // Note: + is handled separately - it can be string concatenation or arithmetic
        let is_relational = matches!(op, "<" | ">" | "<=" | ">=");
        let is_arithmetic = matches!(op, "-" | "*" | "/" | "%" | "**");
        let is_bitwise = matches!(op, "&" | "|" | "^" | "<<" | ">>" | ">>>");
        let requires_numeric_operands = is_arithmetic || is_bitwise;

        // TS2447: For &, |, ^ with both boolean operands, emit special error
        // This must be checked before TS2362/TS2363 because boolean is not a valid arithmetic operand
        if is_bitwise {
            let left_is_boolean = evaluator.is_boolean_like(left_type);
            let right_is_boolean = evaluator.is_boolean_like(right_type);
            let is_boolean_bitwise =
                matches!(op, "&" | "|" | "^") && left_is_boolean && right_is_boolean;

            if is_boolean_bitwise {
                let suggestion = if op == "&" {
                    "&&"
                } else if op == "|" {
                    "||"
                } else {
                    "!=="
                };
                self.emit_render_request(
                    node_idx,
                    DiagnosticRenderRequest::simple_msg(
                        diagnostic_codes::THE_OPERATOR_IS_NOT_ALLOWED_FOR_BOOLEAN_TYPES_CONSIDER_USING_INSTEAD,
                        &[op, suggestion],
                    ),
                );
                return;
            }
        }

        // Evaluate types to resolve unevaluated conditional/mapped types before checking.
        // e.g., DeepPartial<number> | number → number
        let eval_left = self.evaluate_type_for_binary_ops(left_type);
        let eval_right = self.evaluate_type_for_binary_ops(right_type);
        let (left_non_null, left_cause) = self.split_nullish_type(eval_left);
        let (right_non_null, right_cause) = self.split_nullish_type(eval_right);
        let left_has_nullish = left_cause.is_some();
        let right_has_nullish = right_cause.is_some();

        // Suppress operator errors when an operand is an inference placeholder.
        //
        // `__infer_N` TypeParameters are tsz-internal markers representing a type
        // parameter that could not be fully resolved during generic call inference.
        // TypeScript itself would successfully infer the concrete type (e.g., `number`)
        // through contextual typing, so operator errors involving these placeholders
        // are false positives.
        //
        // We check both original and evaluated forms because evaluate_type_for_binary_ops
        // may partially resolve the type.
        let is_infer_placeholder = |type_id: TypeId| -> bool {
            crate::query_boundaries::common::type_param_info(self.ctx.types, type_id)
                .is_some_and(|tp| self.ctx.types.resolve_atom(tp.name).starts_with("__infer_"))
        };
        if is_infer_placeholder(eval_left)
            || is_infer_placeholder(eval_right)
            || is_infer_placeholder(left_type)
            || is_infer_placeholder(right_type)
        {
            return;
        }

        // Check if operands have valid arithmetic types using BinaryOpEvaluator
        // This properly handles number, bigint, any, and enum types (unions of number literals)
        // Note: evaluator was already created above for symbol checking
        // Skip arithmetic checks for symbol operands (we already emitted TS2469)
        // When strictNullChecks is off, null/undefined are implicitly assignable to
        // number, so they should not trigger arithmetic errors.
        let snc_off = !self.ctx.compiler_options.strict_null_checks;
        let left_is_valid_arithmetic = !left_is_symbol
            && (evaluator.is_arithmetic_operand(eval_left)
                || (snc_off && (eval_left == TypeId::NULL || eval_left == TypeId::UNDEFINED)));
        let right_is_valid_arithmetic = !right_is_symbol
            && (evaluator.is_arithmetic_operand(eval_right)
                || (snc_off && (eval_right == TypeId::NULL || eval_right == TypeId::UNDEFINED)));
        let left_non_null_is_valid_arithmetic =
            left_non_null.is_some_and(|t| evaluator.is_arithmetic_operand(t));
        let right_non_null_is_valid_arithmetic =
            right_non_null.is_some_and(|t| evaluator.is_arithmetic_operand(t));

        // For + operator, TSC emits TS2365 ("Operator '+' cannot be applied to types"),
        // never TS2362/TS2363. But if null/undefined operands already got TS18050,
        // don't also emit TS2365 - tsc only emits the per-operand TS18050 errors.
        if op == "+" {
            if !emitted_nullish_error {
                self.emit_render_request(
                    node_idx,
                    DiagnosticRenderRequest::simple_msg(
                        diagnostic_codes::OPERATOR_CANNOT_BE_APPLIED_TO_TYPES_AND,
                        &[op, &left_str, &right_str],
                    ),
                );
            }
            return;
        }

        if requires_numeric_operands {
            // For arithmetic and bitwise operators, emit specific left/right errors (TS2362, TS2363)
            // Skip operands that already got TS18050 (null/undefined with strictNullChecks)
            // tsc suppresses TS2362/TS2363 when TS18050 was already emitted for the operand.
            let mut emitted_specific_error = emitted_nullish_error;
            let mut emitted_operand_error = false;
            if !(left_is_valid_arithmetic
                || (left_has_nullish
                    && left_non_null_is_valid_arithmetic
                    && should_emit_nullish_error)
                || (emitted_nullish_error && left_is_nullish))
            {
                self.emit_render_request(
                    left_idx,
                    DiagnosticRenderRequest::simple_msg(
                        diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT,
                        &[],
                    ),
                );
                emitted_specific_error = true;
                emitted_operand_error = true;
            }
            if !(right_is_valid_arithmetic
                || (right_has_nullish
                    && right_non_null_is_valid_arithmetic
                    && should_emit_nullish_error)
                || (emitted_nullish_error && right_is_nullish))
            {
                self.emit_render_request(
                    right_idx,
                    DiagnosticRenderRequest::simple_msg(
                        diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT,
                        &[],
                    ),
                );
                emitted_specific_error = true;
                emitted_operand_error = true;
            }
            // If both operands are valid arithmetic types but the operation still failed
            // (e.g., mixing number and bigint), emit TS2365. tsc also emits TS2365
            // when a bigint-capable operation has one invalid side (`"x" & 1n`,
            // `1n ** false`): the per-side TS2362/TS2363 explains operand validity,
            // while TS2365 explains the incompatible operator pair.
            let should_emit_pair_error = !emitted_specific_error
                || (emitted_operand_error
                    && (self.operator_operand_may_include_bigint(left_type)
                        || self.operator_operand_may_include_bigint(right_type)));
            if should_emit_pair_error {
                self.emit_render_request(
                    node_idx,
                    DiagnosticRenderRequest::simple_msg(
                        diagnostic_codes::OPERATOR_CANNOT_BE_APPLIED_TO_TYPES_AND,
                        &[op, &left_str, &right_str],
                    ),
                );
            }
            return;
        }

        // Handle relational operators: <, >, <=, >=
        // These require both operands to be comparable. When types have no relationship,
        // emit TS2365: "Operator '<' cannot be applied to types 'X' and 'Y'."
        if is_relational && !emitted_nullish_error {
            self.emit_render_request(
                node_idx,
                DiagnosticRenderRequest::simple_msg(
                    diagnostic_codes::OPERATOR_CANNOT_BE_APPLIED_TO_TYPES_AND,
                    &[op, &left_str, &right_str],
                ),
            );
        }
    }

    /// Widen a type for display in operator error messages.
    ///
    /// tsc displays enum names (e.g., `'E'`) rather than their structural base
    /// type (`'number'`). We must try enum member widening BEFORE
    /// `get_base_type_for_comparison`, because the latter unwraps
    /// enum types to their member union (losing the enum identity).
    pub(crate) fn widen_type_for_operator_display(&mut self, type_id: TypeId) -> TypeId {
        let display_type = if crate::query_boundaries::common::type_param_info(
            self.ctx.types,
            type_id,
        )
        .is_some()
        {
            type_id
        } else {
            let evaluated = self.evaluate_type_for_binary_ops(type_id);
            if evaluated != TypeId::ERROR
                && evaluated != TypeId::UNKNOWN
                && self.operator_operand_may_include_bigint(evaluated)
                && crate::query_boundaries::common::union_members(self.ctx.types, evaluated)
                    .is_some()
            {
                evaluated
            } else {
                type_id
            }
        };

        // 1. Try widening enum members to their parent enum.
        //    Both parent enums (E) and members (E.A) are enum types —
        //    widen_enum_member_type correctly handles both: members widen to
        //    parent, parent enums return unchanged.
        let widened = self.widen_enum_member_type(display_type);
        if widened != display_type {
            return widened;
        }

        // 2. If it's a parent Enum type (widen_enum_member_type returned it
        //    unchanged because it has no parent), keep for display.
        if crate::query_boundaries::common::is_enum_type(self.ctx.types, display_type) {
            return display_type;
        }

        // 3. Fall back to standard literal-to-base-type widening
        crate::query_boundaries::common::get_base_type_for_comparison(self.ctx.types, display_type)
    }

    pub(crate) fn format_type_for_operator_display(&mut self, type_id: TypeId) -> String {
        let display_type = self.widen_type_for_operator_display(type_id);
        if crate::query_boundaries::common::type_param_info(self.ctx.types, display_type).is_none()
            && self.operator_operand_may_include_bigint(display_type)
            && let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, display_type)
        {
            return members
                .iter()
                .map(|&member| self.format_type_for_operator_display(member))
                .collect::<Vec<_>>()
                .join(" | ");
        }
        self.format_type(display_type)
    }
}
