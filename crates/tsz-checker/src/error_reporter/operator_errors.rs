//! Binary operator error reporting (TS2362, TS2363, TS2365, TS2469).

use super::fingerprint_policy::{
    DiagnosticAnchorKind, DiagnosticRenderRequest, RelatedInformationPolicy,
};
use crate::diagnostics::{
    DiagnosticCategory, DiagnosticRelatedInformation, RelatedInformationKind, diagnostic_codes,
    diagnostic_messages, format_message,
};
use crate::query_boundaries::diagnostics as diagnostic_query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// Which invocation-error detail (`tsc`'s `invocationErrorDetails`) a non-union
/// callee/constructor source lacks: a call signature (`TS2349`) or a construct
/// signature (`TS2351`).
#[derive(Clone, Copy)]
pub(crate) enum InvocationSignatureKind {
    Call,
    Construct,
}

impl<'a> CheckerState<'a> {
    /// Span of the operator token inside a binary/compound-assignment
    /// expression: tsc anchors TS2447 at the operator, not the whole
    /// expression.
    pub(crate) fn operator_token_span(
        &self,
        node_idx: NodeIndex,
        op_str: &str,
    ) -> Option<(u32, u32)> {
        let node = self.ctx.arena.get(node_idx)?;
        let bin = self.ctx.arena.get_binary_expr(node)?;
        let left_end = self.ctx.arena.get(bin.left)?.end;
        let right_pos = self.ctx.arena.get(bin.right)?.pos;
        let text = self
            .ctx
            .arena
            .source_files
            .first()?
            .text
            .get(left_end as usize..right_pos as usize)?;
        let off = text.find(op_str)? as u32;
        Some((left_end + off, op_str.len() as u32))
    }

    /// Report TS2351: "This expression is not constructable. Type 'X' has no construct signatures."
    /// This is for `new` expressions where the expression type has no construct signatures.
    pub fn error_not_constructable_at(&mut self, type_id: TypeId, idx: NodeIndex) {
        if type_id == TypeId::ERROR
            || type_id == TypeId::ANY
            || (type_id == TypeId::UNKNOWN && self.ctx.compiler_options.strict_null_checks)
        {
            return;
        }

        let Some(anchor) = self.resolve_diagnostic_anchor(idx, DiagnosticAnchorKind::Exact) else {
            return;
        };

        let related = self
            .invocation_signature_detail(
                type_id,
                InvocationSignatureKind::Construct,
                anchor.start,
                anchor.length,
            )
            .map(|detail| vec![detail])
            .unwrap_or_default();

        let message = diagnostic_messages::THIS_EXPRESSION_IS_NOT_CONSTRUCTABLE.to_string();

        self.emit_render_request_at_anchor(
            anchor,
            DiagnosticRenderRequest::with_related(
                DiagnosticAnchorKind::Exact,
                diagnostic_codes::THIS_EXPRESSION_IS_NOT_CONSTRUCTABLE,
                message,
                related,
                RelatedInformationPolicy::ELABORATION,
            ),
        );
    }

    /// Build `tsc`'s `invocationErrorDetails` chain link for a non-union
    /// callee/constructor source: the `Type 'X' has no call signatures.`
    /// (`TS2757`) / `Type 'X' has no construct signatures.` (`TS2761`) note
    /// rendered one indent level beneath the `This expression is not
    /// callable/constructable.` headline. The source is displayed as its
    /// apparent type (`number` -> `Number`, `object` -> `{}`), matching
    /// `typeToString(getApparentType(type))`.
    ///
    /// Returns `None` for `any`/error sources and for unions: `tsc` renders a
    /// union callee through the distinct `Not all constituents of type 'U' are
    /// ...` / `No constituent of type 'U' is ...` shapes, so callers keep the
    /// existing union rendering untouched rather than mislabeling it here.
    pub(crate) fn invocation_signature_detail(
        &mut self,
        type_id: TypeId,
        kind: InvocationSignatureKind,
        start: u32,
        length: u32,
    ) -> Option<DiagnosticRelatedInformation> {
        let apparent =
            crate::query_boundaries::diagnostics::invocation_signature_detail_apparent_type(
                self.ctx.types,
                type_id,
            )?;
        let mut formatter = self.ctx.create_type_formatter();
        let type_str = formatter.format(apparent);

        let (code, template) = match kind {
            InvocationSignatureKind::Call => (
                diagnostic_codes::TYPE_HAS_NO_CALL_SIGNATURES,
                diagnostic_messages::TYPE_HAS_NO_CALL_SIGNATURES,
            ),
            InvocationSignatureKind::Construct => (
                diagnostic_codes::TYPE_HAS_NO_CONSTRUCT_SIGNATURES,
                diagnostic_messages::TYPE_HAS_NO_CONSTRUCT_SIGNATURES,
            ),
        };

        Some(DiagnosticRelatedInformation {
            category: DiagnosticCategory::Message,
            code,
            file: self.ctx.file_name.clone(),
            start,
            length,
            message_text: format_message(template, &[&type_str]),
            depth: 0,
            kind: RelatedInformationKind::ChainLink,
        })
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

        // An optional chain's own top-level result always carries `| undefined`
        // once *any* link in it uses `?.` (tsc's static chain typing), even
        // when that `undefined` already surfaced — and was already reported —
        // on an inner continuation (e.g. `h?.inner.leaf`, where `inner` is
        // genuinely optional: the possibly-nullish check on the `.leaf` access
        // already named `'h.inner'`). Reporting again here on the outer node
        // would stack a second, redundant diagnostic that tsc does not emit.
        // Suppress only when the earlier report is strictly *inside* this
        // node's span (a real inner continuation) — a same-span diagnostic
        // means nothing has fired for this exact node yet.
        if let Some((start, end)) = self.get_node_span(idx)
            && self.ctx.diagnostics.iter().any(|diag| {
                matches!(
                    diag.code,
                    diagnostic_codes::OBJECT_IS_POSSIBLY_NULL
                        | diagnostic_codes::OBJECT_IS_POSSIBLY_UNDEFINED
                        | diagnostic_codes::OBJECT_IS_POSSIBLY_NULL_OR_UNDEFINED
                        | diagnostic_codes::IS_POSSIBLY_NULL
                        | diagnostic_codes::IS_POSSIBLY_UNDEFINED
                        | diagnostic_codes::IS_POSSIBLY_NULL_OR_UNDEFINED
                ) && diag.start >= start
                    && diag.start + diag.length < end
            })
        {
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
                    // A property access whose receiver has no nameable text (e.g. a
                    // `this`-rooted `this.version`) is reported by tsc as the object
                    // being possibly nullish (TS2532/2531/2533), like an element
                    // access — not the literal-value TS18050 fallback below.
                    self.report_nullish_object(idx, cause, false);
                    return;
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
                // Unnamed non-literal expression with a nullish type (a call result,
                // parenthesized expression, etc.). tsc's `checkNonNullType` reports
                // these through `reportObjectPossiblyNullOrUndefinedError`, which has no
                // entity name and so emits TS2531/TS2532/TS2533 ("Object is possibly
                // 'null'/'undefined'/...") — not the literal-value TS18050. The literal
                // `null`/`undefined` keyword is handled by the `is_literal` branch above.
                self.report_nullish_object(idx, cause, false);
            }
        }
    }

    /// Emit the "possibly null/undefined" operand diagnostic when the operand of
    /// a unary `+`/`-`/`~` is nullable.
    ///
    /// tsc's `checkPrefixUnaryExpression` runs `checkNonNullType(operandType, operand)`
    /// **unconditionally** for `+`/`-`/`~` — there is no arithmetic-operand check for
    /// unary arithmetic (that only exists for binary `- * / % **`). So a nullable
    /// operand always reports TS18047/TS18048/TS18049 (named) or TS2531/TS2532/TS2533
    /// (unnamed), regardless of the non-nullish remainder's kind: `null`/`undefined`
    /// alone, `string | undefined`, `object | null`, `symbol | null` (alongside the
    /// separate TS2469), and type parameters whose constraint is nullable all report.
    ///
    /// `checkNonNullType` is not gated on `strictNullChecks` either: the strictness
    /// policy lives in tsc's `reportObjectPossiblyNullOrUndefinedError`, which reports
    /// the literal `null`/`undefined` KEYWORD as TS18050 under both settings and the
    /// type-driven family only under `strictNullChecks`. `emit_nullish_operand_error`
    /// already implements that routing, so only the keyword needs to reach it without
    /// `strictNullChecks`.
    ///
    /// `void` is not in tsc's `Nullable` flag set, so a `void` operand never triggers
    /// this (it is handled by the operator-specific paths instead).
    pub(crate) fn check_nullish_unary_operand(&mut self, operand: NodeIndex, operand_type: TypeId) {
        if operand_type == TypeId::VOID {
            return;
        }
        let (_non_nullish, nullish_cause) = self.split_nullish_type(operand_type);
        let Some(cause) = nullish_cause else { return };
        if !self.ctx.strict_null_checks()
            && !self.is_literal_null_or_undefined_node(operand)
            && !crate::query_boundaries::type_predicates::has_ts_nullable_flag(operand_type)
        {
            return;
        }
        self.emit_nullish_operand_error(operand, cause);
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
            && let Some(sym_id) = self
                .resolve_identifier_symbol(idx)
                .or_else(|| self.ctx.binder.get_node_symbol(idx))
        {
            let declared = self.get_type_of_symbol(sym_id);
            let parameter_declaration = self.ctx.binder.get_symbol(sym_id).and_then(|symbol| {
                for decl_idx in symbol.all_declarations() {
                    let mut current = decl_idx;
                    for _ in 0..=16 {
                        let Some(node) = self.ctx.arena.get(current) else {
                            break;
                        };
                        if node.kind == syntax_kind_ext::PARAMETER
                            && let Some(parameter) = self.ctx.arena.get_parameter(node)
                            && parameter.type_annotation.is_some()
                        {
                            return Some((current, parameter.type_annotation));
                        }
                        let Some(parent) =
                            self.ctx.arena.get_extended(current).map(|ext| ext.parent)
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
            });
            // Only preserve raw annotation text when the annotation names an
            // actual type parameter (e.g. `T`). A type-alias annotation such as
            // `type N = number` resolves to a concrete type, so tsc renders the
            // resolved type (`number`), not the alias name — keying on the
            // declared type being type-parameter-like keeps this structural.
            if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, declared)
                && let Some((_parameter_idx, type_annotation)) = parameter_declaration
                && let Some(annotation_node) = self.ctx.arena.get(type_annotation)
                && let Some(type_name) = self
                    .ctx
                    .arena
                    .get_identifier_at(
                        self.ctx
                            .arena
                            .get_type_ref(annotation_node)
                            .map_or(type_annotation, |type_ref| type_ref.type_name),
                    )
                    .map(|identifier| identifier.escaped_text.as_str())
            {
                let annotated_surface = diagnostic_query::diagnostic_user_type_param(
                    self.ctx.types,
                    self.ctx.types.intern_string(type_name),
                    Some(fallback),
                );
                return annotated_surface;
            }
            if let Some((parameter_idx, type_annotation)) = parameter_declaration
                && let Some(annotation_node) = self.ctx.arena.get(type_annotation)
                && let annotation_name = self
                    .ctx
                    .arena
                    .get_type_ref(annotation_node)
                    .map_or(type_annotation, |type_ref| type_ref.type_name)
                && let Some(type_name) = self
                    .ctx
                    .arena
                    .get_identifier_at(annotation_name)
                    .map(|identifier| identifier.escaped_text.as_str())
                && let Some(function_data) = self
                    .ctx
                    .arena
                    .get_extended(parameter_idx)
                    .map(|ext| ext.parent)
                    .filter(|parent| parent.is_some())
                    .and_then(|mut current| {
                        for _ in 0..=16 {
                            let node = self.ctx.arena.get(current)?;
                            if let Some(function_data) = self.ctx.arena.get_function(node) {
                                return Some(function_data);
                            }
                            let parent = self.ctx.arena.get_extended(current)?.parent;
                            if parent.is_none() {
                                return None;
                            }
                            current = parent;
                        }
                        None
                    })
                && let Some(type_parameters) = function_data.type_parameters.as_ref()
            {
                for &type_parameter_idx in &type_parameters.nodes {
                    let Some(type_parameter_node) = self.ctx.arena.get(type_parameter_idx) else {
                        continue;
                    };
                    let Some(type_parameter) =
                        self.ctx.arena.get_type_parameter(type_parameter_node)
                    else {
                        continue;
                    };
                    if self
                        .ctx
                        .arena
                        .get_identifier_at(type_parameter.name)
                        .is_none_or(|identifier| identifier.escaped_text != type_name)
                    {
                        continue;
                    }
                    let constraint = if type_parameter.constraint.is_some() {
                        Some(self.get_type_from_type_node(type_parameter.constraint))
                    } else {
                        None
                    };
                    let surface = diagnostic_query::diagnostic_user_type_param(
                        self.ctx.types,
                        self.ctx.types.intern_string(type_name),
                        constraint,
                    );
                    if self.operator_operand_may_include_bigint(surface) {
                        return surface;
                    }
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

    /// Emit TS2362/TS2363 for each invalid operand of an arithmetic
    /// (`- * / % **`) or bitwise (`& | ^ << >> >>>`) operator.
    ///
    /// This mirrors `tsc`'s `checkArithmeticOperandType`, which is invoked once
    /// per operand and is *independent of the other operand*: tsc validates each
    /// side by assignability to `number | bigint` (after `checkNonNullType`,
    /// which strips `null`/`undefined` and turns an unknown-under-strict-null
    /// operand into `error`). Numeric values — `number`, numeric enums, `bigint`
    /// — and the wildcards `any`/`unknown`/`error`/`never` are valid; `string`,
    /// `boolean`, object, `symbol`, `void`, string literals and *string* enums
    /// are not.
    ///
    /// tsz previously only ran this check when one operand was already `any`/
    /// `error`, and additionally treated every enum (including string enums) as
    /// valid, so it silently accepted e.g. `stringEnum - number`, `string &
    /// never`, and `unknown - string` (where the unknown side short-circuited
    /// before the other side was checked). Running the check here, once, for
    /// every operand pair restores parity with tsc.
    ///
    /// When both operands are boolean-like and the operator is `& | ^`, tsc
    /// reports TS2447 instead (handled on the `emit_binary_operator_error`
    /// path), so this skips those.
    ///
    /// `+` is intentionally excluded: it is string concatenation when either
    /// side is string-like and otherwise reports the whole-expression TS2365,
    /// never the per-operand TS2362/TS2363.
    ///
    /// This runs *before* the operator-specific handling (and so before the
    /// null/undefined TS18050 and unknown TS18046 operand errors are emitted),
    /// which is sound because a pure `null`/`undefined` operand is suppressed
    /// here directly via the `strictNullChecks` state rather than via an
    /// already-emitted flag.
    pub(crate) fn emit_arithmetic_operand_errors(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
        left_type: TypeId,
        right_type: TypeId,
        op: &str,
    ) {
        if self.has_parse_errors() {
            return;
        }
        debug_assert!(
            matches!(
                op,
                "-" | "*" | "/" | "%" | "**" | "&" | "|" | "^" | "<<" | ">>" | ">>>"
            ),
            "emit_arithmetic_operand_errors called with non-arithmetic operator {op}"
        );

        let evaluator = crate::query_boundaries::common::new_binary_op_evaluator(self.ctx.types);

        // Hot-path fast-out: when both operands are already valid arithmetic
        // operands (the overwhelmingly common `number - number` etc.), neither
        // side can error, so skip the conditional/mapped evaluation and
        // nullish-split below. Resolving the operands cannot turn a valid type
        // invalid, so this is behavior-preserving.
        if evaluator.is_arithmetic_operand(left_type) && evaluator.is_arithmetic_operand(right_type)
        {
            return;
        }

        // `& | ^` with two boolean operands is reported as TS2447 ("operator is
        // not allowed for boolean types"), not as a per-operand TS2362/TS2363.
        // Use the strict boolean test: tsc gates TS2447 on `flags & BooleanLike`,
        // so `boolean & any` falls through to the per-operand TS2362 instead.
        if matches!(op, "&" | "|" | "^")
            && evaluator.is_boolean_like_strict(left_type)
            && evaluator.is_boolean_like_strict(right_type)
        {
            return;
        }

        // Resolve conditional/mapped operands and detect each side's validity the
        // same way the `emit_binary_operator_error` mismatch path does, so the two
        // entry points stay in lockstep.
        let eval_left = self.evaluate_type_for_binary_ops(left_type);
        let eval_right = self.evaluate_type_for_binary_ops(right_type);
        let snc_off = !self.ctx.compiler_options.strict_null_checks;

        let (left_non_null, left_cause) = self.split_nullish_type(eval_left);
        let (right_non_null, right_cause) = self.split_nullish_type(eval_right);
        let left_has_nullish = left_cause.is_some();
        let right_has_nullish = right_cause.is_some();
        let left_is_nullish = left_type == TypeId::NULL || left_type == TypeId::UNDEFINED;
        let right_is_nullish = right_type == TypeId::NULL || right_type == TypeId::UNDEFINED;

        let left_is_valid_arithmetic = evaluator.is_arithmetic_operand(eval_left)
            || (snc_off && (eval_left == TypeId::NULL || eval_left == TypeId::UNDEFINED));
        let right_is_valid_arithmetic = evaluator.is_arithmetic_operand(eval_right)
            || (snc_off && (eval_right == TypeId::NULL || eval_right == TypeId::UNDEFINED));
        let left_non_null_is_valid_arithmetic =
            left_non_null.is_some_and(|t| evaluator.is_arithmetic_operand(t));
        let right_non_null_is_valid_arithmetic =
            right_non_null.is_some_and(|t| evaluator.is_arithmetic_operand(t));

        // Every operator this method handles is in the TS18050-emitting family,
        // so under `strictNullChecks` a pure `null`/`undefined` operand is
        // reported by `checkNonNullType` (TS18047/TS18048) and stripped to the
        // bottom type before the operand check — no TS2362/TS2363. A nullish
        // *union* (e.g. `number | null`) is suppressed only when its non-null
        // remainder is itself a valid arithmetic operand.
        let strict_null = self.ctx.compiler_options.strict_null_checks;
        if !(left_is_valid_arithmetic
            || (left_has_nullish && left_non_null_is_valid_arithmetic)
            || (strict_null && left_is_nullish))
        {
            self.emit_render_request(
                left_idx,
                DiagnosticRenderRequest::simple_msg(
                    diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT,
                    &[],
                ),
            );
        }
        if !(right_is_valid_arithmetic
            || (right_has_nullish && right_non_null_is_valid_arithmetic)
            || (strict_null && right_is_nullish))
        {
            self.emit_render_request(
                right_idx,
                DiagnosticRenderRequest::simple_msg(
                    diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT,
                    &[],
                ),
            );
        }
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
        if left_type == TypeId::ERROR || right_type == TypeId::ERROR {
            return;
        }

        // In strictNullChecks mode, unknown-specific unknown diagnostics are handled
        // by the binary evaluation gate above, so skip TS2365/TS2362/TS2363 here.
        // In non-strict mode, unknown should still participate in normal operator
        // diagnostics (e.g., `unknown + 1` -> TS2365).
        if self.ctx.compiler_options.strict_null_checks
            && (left_type == TypeId::UNKNOWN || right_type == TypeId::UNKNOWN)
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
                if crate::query_boundaries::common::type_param_info(self.ctx.types, left_surface)
                    .is_some()
                {
                    left_surface
                } else {
                    self.widen_type_for_operator_display(left_surface)
                },
                if crate::query_boundaries::common::type_param_info(self.ctx.types, right_surface)
                    .is_some()
                {
                    right_surface
                } else {
                    self.widen_type_for_operator_display(right_surface)
                },
            )
        };
        let mut format_operand = |diag| {
            if is_number_bigint_mix
                || is_unsigned_shift_bigint_mix
                || crate::query_boundaries::common::type_param_info(self.ctx.types, diag).is_some()
            {
                self.format_type(diag)
            } else {
                self.format_type_for_operator_display(diag)
            }
        };
        let left_str = format_operand(left_diag);
        let right_str = format_operand(right_diag);

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
                // tsc anchors TS2447 at the operator token.
                if let Some((start, length)) = self.operator_token_span(node_idx, op) {
                    let message = format!(
                        "The '{op}' operator is not allowed for boolean types. Consider using '{suggestion}' instead."
                    );
                    self.error_at_position(
                        start,
                        length,
                        &message,
                        diagnostic_codes::THE_OPERATOR_IS_NOT_ALLOWED_FOR_BOOLEAN_TYPES_CONSIDER_USING_INSTEAD,
                    );
                    return;
                }
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
                .is_some_and(|tp| tp.is_infer_placeholder())
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
            // Under strictNullChecks-off a bare `null`/`undefined` operand
            // borrows its numeric/string kind from the *other* operand: paired
            // with a real numeric/string/`any`/enum operand it is a well-typed
            // addition/concatenation and must not report TS2365 — `x + 1` for
            // `x: undefined` and the uncovered-optional IIFE param
            // `((k?) => k + 1)()` are both clean in tsc. The allowance requires
            // an actual operand to borrow from: two nullish operands
            // (`null + undefined`, `undefined + undefined`) have no numeric or
            // string side and still report TS2365, as does a nullish operand
            // paired with a genuinely invalid one (symbol, object). The mixed
            // `number + bigint` case (no nullish operand) is unaffected.
            // `left_is_valid_arithmetic` already folds in the snc-off nullish
            // allowance for the numeric side; the string check covers the
            // concatenation side.
            let left_ok_for_plus = left_is_valid_arithmetic || self.is_string_like_type(eval_left);
            let right_ok_for_plus =
                right_is_valid_arithmetic || self.is_string_like_type(eval_right);
            // Exactly one nullish operand: it borrows the other (real) operand's
            // numeric/string kind. Both nullish → nothing to borrow → still
            // TS2365; neither nullish → no allowance applies at all.
            let exactly_one_nullish = left_is_nullish != right_is_nullish;
            let plus_valid_via_snc_off_nullish =
                snc_off && exactly_one_nullish && left_ok_for_plus && right_ok_for_plus;
            if !emitted_nullish_error && !plus_valid_via_snc_off_nullish {
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
            // The per-operand TS2362/TS2363 errors are emitted up-front by
            // `emit_arithmetic_operand_errors` (tsc's `checkArithmeticOperandType`,
            // run once per operand independently of the other and of this
            // evaluator-driven mismatch path). Here we only re-derive operand
            // validity to decide whether the whole-expression TS2365 ("operator
            // cannot be applied to types") is *additionally* warranted — i.e. the
            // mixed number/bigint case, where tsc reports both the per-operand
            // error and TS2365.
            let left_operand_invalid = !(left_is_valid_arithmetic
                || (left_has_nullish
                    && left_non_null_is_valid_arithmetic
                    && should_emit_nullish_error)
                || (emitted_nullish_error && left_is_nullish));
            let right_operand_invalid = !(right_is_valid_arithmetic
                || (right_has_nullish
                    && right_non_null_is_valid_arithmetic
                    && should_emit_nullish_error)
                || (emitted_nullish_error && right_is_nullish));
            let emitted_operand_error = left_operand_invalid || right_operand_invalid;
            let emitted_specific_error = emitted_nullish_error || emitted_operand_error;
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
        if let Some(info) =
            crate::query_boundaries::common::type_param_info(self.ctx.types, type_id)
        {
            return self.ctx.types.resolve_atom(info.name);
        }
        let display_type = self.widen_type_for_operator_display(type_id);
        if self.operator_operand_may_include_bigint(display_type)
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
