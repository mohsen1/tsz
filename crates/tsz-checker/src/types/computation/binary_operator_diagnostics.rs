use super::*;

impl<'a> CheckerState<'a> {
    // Extracted from `binary.rs` to keep operator diagnostics helpers under the file-size cap.

    /// Get the operator name for a unary operator token (for TS17006 error messages).
    ///
    /// Returns the string representation of unary operators that are not allowed
    /// on the left-hand side of exponentiation (`**`).
    pub(super) const fn unary_operator_name(op: u16) -> Option<&'static str> {
        match op {
            k if k == SyntaxKind::MinusToken as u16 => Some("-"),
            k if k == SyntaxKind::PlusToken as u16 => Some("+"),
            k if k == SyntaxKind::TildeToken as u16 => Some("~"),
            k if k == SyntaxKind::ExclamationToken as u16 => Some("!"),
            k if k == SyntaxKind::TypeOfKeyword as u16 => Some("typeof"),
            k if k == SyntaxKind::VoidKeyword as u16 => Some("void"),
            k if k == SyntaxKind::DeleteKeyword as u16 => Some("delete"),
            _ => None,
        }
    }

    /// Find the callable truthiness body for a logical operator expression.
    ///
    /// When a logical expression (`&&`, `||`, `??`) is part of an `if` condition,
    /// this returns the then-branch statement for callable truthiness checking.
    /// It walks up through nested logical expressions and parentheses to find
    /// the containing `if` statement.
    pub(super) fn find_callable_truthiness_body(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut parent_idx = self.ctx.arena.get_extended(idx)?.parent;
        if parent_idx.is_none() {
            return None;
        }

        loop {
            let parent = self.ctx.arena.get(parent_idx)?;
            if parent.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                || matches!(
                    self.ctx.arena.get_binary_expr(parent),
                    Some(bin)
                        if bin.operator_token == SyntaxKind::AmpersandAmpersandToken as u16
                            || bin.operator_token == SyntaxKind::BarBarToken as u16
                            || bin.operator_token == SyntaxKind::QuestionQuestionToken as u16
                )
            {
                parent_idx = self.ctx.arena.get_extended(parent_idx)?.parent;
                continue;
            }

            break if parent.kind == syntax_kind_ext::IF_STATEMENT {
                self.ctx
                    .arena
                    .get_if_statement(parent)
                    .map(|if_stmt| if_stmt.then_statement)
            } else {
                None
            };
        }
    }

    /// If `idx` is a `typeof` expression (`PREFIX_UNARY_EXPRESSION` with `TypeOfKeyword`),
    /// return the typeof result type:
    /// `"string" | "number" | "bigint" | "boolean" | "symbol" | "undefined" | "object" | "function"`.
    /// This is used for TS2367 overlap detection so that comparisons like
    /// `typeof x == "Object"` (capital O) correctly detect no overlap.
    pub(super) fn typeof_result_type_if_typeof(&self, idx: NodeIndex) -> Option<TypeId> {
        use tsz_scanner::SyntaxKind;
        let node = self.ctx.arena.get(idx)?;
        if node.kind != syntax_kind_ext::PREFIX_UNARY_EXPRESSION {
            return None;
        }
        let unary = self.ctx.arena.get_unary_expr(node)?;
        if unary.operator != SyntaxKind::TypeOfKeyword as u16 {
            return None;
        }
        let factory = self.ctx.types.factory();
        let members = vec![
            factory.literal_string("string"),
            factory.literal_string("number"),
            factory.literal_string("bigint"),
            factory.literal_string("boolean"),
            factory.literal_string("symbol"),
            factory.literal_string("undefined"),
            factory.literal_string("object"),
            factory.literal_string("function"),
        ];
        Some(factory.union(members))
    }

    /// Check if an identifier node's declared type overlaps with the given comparison type.
    /// Returns true if the identifier's declared type is wider than `narrow_type` and
    /// has overlap with `other_type`. This prevents false TS2367 when flow narrowing
    /// inside loops makes the narrowed type too specific (e.g., `0` instead of `0 | 1`).
    pub(super) fn declared_type_has_overlap_in_loop(
        &mut self,
        comparison_idx: NodeIndex,
        idx: NodeIndex,
        narrow_type: TypeId,
        other_type: TypeId,
    ) -> bool {
        if !self.is_inside_loop(comparison_idx) {
            return false;
        }

        let node = match self.ctx.arena.get(idx) {
            Some(n) => n,
            None => return false,
        };
        // Only applies to identifiers
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return false;
        }
        // Resolve the identifier to a symbol
        let sym_id = match self.ctx.binder.resolve_identifier(self.ctx.arena, idx) {
            Some(s) => s,
            None => return false,
        };
        // Get the symbol's value_declaration and its type (the declared type)
        let symbol = match self.ctx.binder.get_symbol(sym_id) {
            Some(s) => s,
            None => return false,
        };
        if symbol.value_declaration.is_none() {
            return false;
        }
        let declared_type = match self.ctx.node_types.get(&symbol.value_declaration.0) {
            Some(&t) => t,
            None => return false,
        };
        // Only relevant when the declared type is wider than the narrowed type
        if declared_type == narrow_type {
            return false;
        }
        // Check if the declared type overlaps with the other operand
        !self.types_have_no_overlap(declared_type, other_type)
    }

    pub(super) fn is_inside_loop(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent = ext.parent;
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };
            if matches!(
                parent_node.kind,
                k if k == syntax_kind_ext::WHILE_STATEMENT
                    || k == syntax_kind_ext::DO_STATEMENT
                    || k == syntax_kind_ext::FOR_STATEMENT
                    || k == syntax_kind_ext::FOR_IN_STATEMENT
                    || k == syntax_kind_ext::FOR_OF_STATEMENT
            ) {
                return true;
            }
            current = parent;
        }
        false
    }

    /// Get the primitive type family of a type: `TypeId::STRING` for string/string literals,
    /// `TypeId::NUMBER` for number/number literals, `TypeId::BOOLEAN` for boolean/boolean literals,
    /// `TypeId::BIGINT` for bigint/bigint literals, or `TypeId::ERROR` for non-primitive types.
    ///
    /// Used to determine if two types are from different primitive families (e.g., string vs number)
    /// for TS2367 display purposes. When types are from different families, tsc widens literals
    /// to their base primitive types in error messages.
    pub(super) fn get_primitive_family(&self, type_id: TypeId) -> TypeId {
        use crate::query_boundaries::common::LiteralTypeKind;
        use crate::query_boundaries::common::{
            classify_literal_type, is_string_intrinsic_type, is_template_literal_type,
            is_unique_symbol_type,
        };

        // Check direct primitive type IDs
        if type_id == TypeId::STRING
            || type_id == TypeId::NUMBER
            || type_id == TypeId::BOOLEAN
            || type_id == TypeId::BIGINT
            || type_id == TypeId::SYMBOL
        {
            return type_id;
        }

        // Boolean literal intrinsics (`true` / `false`) belong to the boolean
        // family. classify_literal_type below short-circuits on intrinsics,
        // so we'd otherwise miss them and TS2367 cross-family widening
        // would skip — leaving messages like `'symbol' and 'true'` instead
        // of tsc's `'symbol' and 'boolean'`.
        if type_id == TypeId::BOOLEAN_TRUE || type_id == TypeId::BOOLEAN_FALSE {
            return TypeId::BOOLEAN;
        }

        // Check literal types via query boundary
        match classify_literal_type(self.ctx.types, type_id) {
            LiteralTypeKind::String(_) => return TypeId::STRING,
            LiteralTypeKind::Number(_) => return TypeId::NUMBER,
            LiteralTypeKind::Boolean(_) => return TypeId::BOOLEAN,
            LiteralTypeKind::BigInt(_) => return TypeId::BIGINT,
            LiteralTypeKind::NotLiteral => {}
        }

        // Unique symbol literal types belong to the symbol family.
        if is_unique_symbol_type(self.ctx.types, type_id) {
            return TypeId::SYMBOL;
        }

        // Check template literals and string intrinsics
        if is_template_literal_type(self.ctx.types, type_id)
            || is_string_intrinsic_type(self.ctx.types, type_id)
        {
            return TypeId::STRING;
        }

        // Intersections narrow their members; if any member sits in a primitive
        // family, treat the intersection as belonging to that family (e.g.
        // `T & number` should count as number-family for TS2367 widening).
        if let Some(list_id) =
            crate::query_boundaries::common::intersection_list_id(self.ctx.types, type_id)
        {
            for member in self.ctx.types.type_list(list_id).iter() {
                let family = self.get_primitive_family(*member);
                if family != TypeId::ERROR {
                    return family;
                }
            }
        }

        TypeId::ERROR // Non-primitive types
    }

    /// Widen types for TS2367 display when they are from different primitive families.
    ///
    /// tsc's rule: when comparing types from different primitive families (e.g., string vs number),
    /// both types are widened to their base primitives in the error message. For same-family
    /// comparisons (e.g., `"foo"` vs `"bar"`), literal types are preserved.
    pub(super) fn widen_for_ts2367_cross_family_display(
        &self,
        left: TypeId,
        right: TypeId,
    ) -> (TypeId, TypeId) {
        let left_family = self.get_primitive_family(left);
        let right_family = self.get_primitive_family(right);

        // Both are primitives, but from different families → widen both
        if left_family != TypeId::ERROR
            && right_family != TypeId::ERROR
            && left_family != right_family
        {
            (
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, left),
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, right),
            )
        } else {
            // Same family (or non-primitives): preserve literal types
            (left, right)
        }
    }

    /// Check the `instanceof` operator.
    ///
    /// Validates:
    /// - TS2848: RHS is not an instantiation expression
    /// - TS2358: LHS is of type any, an object type, or a type parameter
    /// - RHS is assignable to Function or has [Symbol.hasInstance]
    /// - TS2860/TS2861: Symbol.hasInstance param/return type checks
    pub(super) fn check_instanceof_operator(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
        left_type: TypeId,
        right_type: TypeId,
    ) -> TypeId {
        use crate::diagnostics::diagnostic_codes;

        // TS2848: The right-hand side of an instanceof must not be an instantiation expression
        let unwrapped_right = self.ctx.arena.skip_parenthesized(right_idx);
        if let Some(right_node) = self.ctx.arena.get(unwrapped_right)
            && right_node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS
        {
            self.error_at_node(
                unwrapped_right,
                crate::diagnostics::diagnostic_messages::THE_RIGHT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_NOT_BE_AN_INSTANTIATION_EXP,
                diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_NOT_BE_AN_INSTANTIATION_EXP,
            );
        }

        // Validate left operand
        if left_type != TypeId::ERROR {
            let evaluator =
                crate::query_boundaries::common::new_binary_op_evaluator(self.ctx.types);
            let lhs_type = self.declared_instanceof_left_operand_type(left_idx, left_type);
            if !evaluator.is_valid_instanceof_left_operand(lhs_type) {
                self.error_at_node_msg(
                    left_idx,
                    diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_BE_OF_TYPE_ANY_AN_OBJECT_TYP,
                    &[],
                );
            }
        }

        let eval_right = self.evaluate_type_for_assignability(right_type);
        if eval_right != TypeId::ERROR {
            let mut is_valid_rhs = false;

            let func_ty_opt = self.global_function_interface_type_for_instanceof();

            if let Some(func_ty) = func_ty_opt {
                let evaluator =
                    crate::query_boundaries::common::new_binary_op_evaluator(self.ctx.types);
                is_valid_rhs = evaluator.is_valid_instanceof_right_operand(
                    eval_right,
                    func_ty,
                    &mut |src, tgt| self.is_assignable_to(src, tgt),
                );
            } else if self.ctx.compiler_options.no_lib {
                // Under `--noLib`, the global `Function` type is deliberately
                // absent. tsc suppresses TS2359 in that regime rather than
                // cascading on every `instanceof X`; mirror that.
                is_valid_rhs = true;
            } else if eval_right == TypeId::ANY
                || eval_right == TypeId::UNKNOWN
                || eval_right == TypeId::FUNCTION
            {
                is_valid_rhs = true;
            }

            if !is_valid_rhs
                && self.ctx.is_js_file()
                && self
                    .synthesize_js_constructor_instance_type(right_idx, eval_right, &[])
                    .is_some()
            {
                is_valid_rhs = true;
            }

            // Check for [Symbol.hasInstance] on the RHS type
            {
                use crate::query_boundaries::common::PropertyAccessResult;
                if let PropertyAccessResult::Success {
                    type_id: has_instance_type,
                    ..
                } = self.resolve_property_access_with_env(eval_right, "[Symbol.hasInstance]")
                {
                    is_valid_rhs = true;
                    let sig_info: Option<(Vec<tsz_solver::ParamInfo>, tsz_solver::TypeId)> =
                        if let Some(fn_id) = crate::query_boundaries::common::function_shape_id(
                            self.ctx.types,
                            has_instance_type,
                        ) {
                            let shape = self.ctx.types.function_shape(fn_id);
                            Some((shape.params.clone(), shape.return_type))
                        } else if let Some(shape_id) =
                            crate::query_boundaries::common::callable_shape_id(
                                self.ctx.types,
                                has_instance_type,
                            )
                        {
                            let shape = self.ctx.types.callable_shape(shape_id);
                            shape
                                .call_signatures
                                .first()
                                .map(|sig| (sig.params.clone(), sig.return_type))
                        } else {
                            None
                        };

                    if let Some((params, return_type)) = sig_info {
                        // TS2861: return type must be boolean
                        let ret = self.evaluate_type_for_assignability(return_type);
                        if ret != TypeId::BOOLEAN
                            && ret != TypeId::ANY
                            && ret != TypeId::ERROR
                            && !self.is_assignable_to(ret, TypeId::BOOLEAN)
                        {
                            self.error_at_node_msg(
                                right_idx,
                                diagnostic_codes::AN_OBJECTS_SYMBOL_HASINSTANCE_METHOD_MUST_RETURN_A_BOOLEAN_VALUE_FOR_IT_TO_BE_US,
                                &[],
                            );
                        }
                        // TS2860: LHS must be assignable to first parameter
                        if let Some(first_param) = params.first() {
                            let param_type =
                                self.evaluate_type_for_assignability(first_param.type_id);
                            let lhs_type =
                                self.declared_instanceof_left_operand_type(left_idx, left_type);
                            if lhs_type != TypeId::ANY
                                && lhs_type != TypeId::ERROR
                                && param_type != TypeId::ANY
                                && param_type != TypeId::UNKNOWN
                                && param_type != TypeId::ERROR
                                && !self.is_assignable_to(lhs_type, param_type)
                            {
                                self.error_at_node_msg(
                                    left_idx,
                                    diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_BE_ASSIGNABLE_TO_THE_FIRST_A,
                                    &[],
                                );
                            }
                        }
                    }
                }
            }

            if !is_valid_rhs {
                self.error_at_node_msg(
                    right_idx,
                    diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_BE_EITHER_OF_TYPE_ANY_A_CLA,
                    &[],
                );
            }
        }

        TypeId::BOOLEAN
    }

    /// Validate that the left operand of `in` is assignable to the property-key
    /// space (`string`, `number`, or `symbol`), which is what `in` probes. On
    /// failure tsc emits TS2322 at the left operand with the key union rendered
    /// structurally, since tsc strips the `PropertyKey` alias from this target.
    pub(super) fn check_in_operator_lhs_key_type(
        &mut self,
        left_idx: NodeIndex,
        left_type: TypeId,
    ) {
        if matches!(left_type, TypeId::ANY | TypeId::ERROR) {
            return;
        }
        // Mirror tsc's checkNonNullType: strip the nullish part before the key check
        // so `string | undefined` is not spuriously rejected. A purely nullish operand
        // contributes no key and is left to the existing nullish diagnostics.
        let Some(key_type) = self.split_nullish_type(left_type).0 else {
            return;
        };
        let target = self
            .ctx
            .types
            .union3(TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL);
        if self.is_assignable_to(key_type, target) {
            return;
        }
        // Source uses the widened diagnostic form so a fresh literal operand shows its
        // primitive (`boolean`, not `true`) against this non-literal target, matching
        // tsc. The target uses the constraint formatter, which renders the canonical
        // key union structurally (tsc strips its `PropertyKey` alias on this surface).
        let display_source = crate::query_boundaries::common::widen_argument_type_for_display(
            self.ctx.types,
            key_type,
        );
        let source_str = self.format_type_diagnostic_widened(display_source);
        let target_str = self.format_type_diagnostic_constraint(target);
        self.error_at_node_msg(
            left_idx,
            tsz_common::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, &target_str],
        );
    }

    /// Check the `in` operator.
    ///
    /// Validates:
    /// - TS18046: RHS is not `unknown`
    /// - TS2322: RHS is assignable to object
    /// - TS2638: RHS may not represent a primitive value
    /// - TS2322: LHS is assignable to `string | number | symbol`
    pub(super) fn check_in_operator(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
        left_type: TypeId,
        right_type: TypeId,
    ) -> TypeId {
        // TS1451: Private identifiers must be the direct LHS of `in`, not wrapped
        // in parentheses. `(#field) in v` is invalid — #field is a standalone expression.
        // Skip through parens to find if the LHS contains a private identifier.
        let left_stripped = self.ctx.arena.skip_parenthesized_and_assertions(left_idx);
        let left_node_kind = self
            .ctx
            .arena
            .get(left_stripped)
            .map(|n| n.kind)
            .unwrap_or(0);
        if left_node_kind == SyntaxKind::PrivateIdentifier as u16 && left_stripped != left_idx {
            // TS1451: private identifier wrapped in parens is a standalone expression
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node_msg(
                left_stripped,
                diagnostic_codes::PRIVATE_IDENTIFIERS_ARE_ONLY_ALLOWED_IN_CLASS_BODIES_AND_MAY_ONLY_BE_USED_AS_PAR,
                &[],
            );
        } else if left_node_kind == SyntaxKind::PrivateIdentifier as u16 {
            // Direct private identifier as LHS — validate it
            self.check_private_identifier_in_expression(left_stripped, right_idx, right_type);
        } else {
            self.check_in_operator_lhs_key_type(left_idx, left_type);
        }

        // TS18047/TS18049: RHS of `in` must not be possibly null (or null|undefined).
        // When strict null checks is enabled and the RHS includes null, emit TS18047.
        // tsc only emits this when there is a name for the expression (identifier etc.).
        if self.ctx.compiler_options.strict_null_checks && right_type != TypeId::UNKNOWN {
            let (_, nullish_cause) = self.split_nullish_type(right_type);
            if let Some(cause) = nullish_cause {
                // Only emit for null-involving cases (not pure undefined).
                // TS18047 = "is possibly null", TS18049 = "is possibly null or undefined"
                let includes_null = cause == TypeId::NULL
                    || (cause != TypeId::UNDEFINED
                        && crate::query_boundaries::common::union_members(self.ctx.types, cause)
                            .is_some_and(|members| members.contains(&TypeId::NULL)));
                if includes_null {
                    let name = self.expression_text(right_idx);
                    if let Some(ref name) = name {
                        use crate::diagnostics::diagnostic_codes;
                        let code = if cause == TypeId::NULL {
                            diagnostic_codes::IS_POSSIBLY_NULL
                        } else {
                            diagnostic_codes::IS_POSSIBLY_NULL_OR_UNDEFINED
                        };
                        self.emit_render_request(
                            right_idx,
                            crate::error_reporter::DiagnosticRenderRequest::simple_msg(
                                code,
                                &[name],
                            ),
                        );
                    }
                    return TypeId::BOOLEAN;
                }
            }
        }

        if right_type == TypeId::UNKNOWN {
            self.error_is_of_type_unknown(right_idx);
        } else {
            let type_may_represent_primitive = self.type_may_represent_primitive(right_type);
            let truthiness_narrowed_unknown = self
                .truthiness_narrowed_from_unknown(right_idx, right_type)
                && !self.in_rhs_has_typeof_object_guard(right_idx);
            if type_may_represent_primitive || truthiness_narrowed_unknown {
                let truthiness_guarded_type_parameter = self
                    .in_rhs_is_type_parameter_assignability_shape(right_type)
                    && self.in_rhs_has_direct_truthiness_guard(right_idx);
                // tsc reports TS2322 ("Type 'T' is not assignable to type
                // 'object'") rather than TS2638 ("may represent a primitive
                // value") for type-parameter-shaped RHS values: bare `T`,
                // unions of type parameters (`T | U`), unions mixing type
                // parameters with primitives (`string | number | T`), and
                // intersections of type parameters (`T & U`,
                // `T & (0 | 1 | 2)`). Intersections with empty-object
                // constraint shapes (`T & {}`, `NonNullable<T>` aliases)
                // keep the existing TS2638 path because tsc emits that code
                // with a `NonNullable<T>`-style message rather than a bare
                // assignability failure.
                if self.in_rhs_is_type_parameter_assignability_shape(right_type)
                    && !truthiness_guarded_type_parameter
                {
                    let _ = self.check_assignable_or_report_at_exact_anchor(
                        right_type,
                        TypeId::OBJECT,
                        right_idx,
                        right_idx,
                    );
                } else {
                    let type_str = if truthiness_narrowed_unknown {
                        "{}".to_string()
                    } else {
                        self.format_apparent_type_for_in_operator(right_type)
                    };
                    let code = tsz_common::diagnostics::diagnostic_codes::TYPE_MAY_REPRESENT_A_PRIMITIVE_VALUE_WHICH_IS_NOT_PERMITTED_AS_THE_RIGHT_OPERAND;
                    self.error_at_node_msg(right_idx, code, &[&type_str]);
                }
            } else if !self.is_valid_in_operator_rhs(right_type) {
                // Route through the check_assignable_or_report(...) gateway family
                // so computation-layer mismatches stay on the centralized path.
                let _ = self.check_assignable_or_report_at_exact_anchor(
                    right_type,
                    TypeId::OBJECT,
                    right_idx,
                    right_idx,
                );
            }
        }

        TypeId::BOOLEAN
    }

    /// Check a binary operation with `IndexAccess` operands is valid through assignability.
    pub(super) fn resolve_indexed_access_binary_op(
        &mut self,
        left: TypeId,
        right: TypeId,
        op: &str,
    ) -> bool {
        let left_is_index_access =
            crate::query_boundaries::common::is_index_access_type(self.ctx.types, left);
        let right_is_index_access =
            crate::query_boundaries::common::is_index_access_type(self.ctx.types, right);

        if !left_is_index_access && !right_is_index_access {
            return false;
        }

        match op {
            "+" | "-" | "*" | "/" | "%" | "**" => {
                let left_ok = crate::query_boundaries::type_computation::core::is_arithmetic_operand(
                    self.ctx.types,
                    left,
                )
                    || left_is_index_access && self.is_assignable_to(left, TypeId::NUMBER);
                let right_ok =
                    crate::query_boundaries::type_computation::core::is_arithmetic_operand(
                        self.ctx.types,
                        right,
                    ) || right_is_index_access && self.is_assignable_to(right, TypeId::NUMBER);
                left_ok && right_ok
            }
            _ => false,
        }
    }
}
