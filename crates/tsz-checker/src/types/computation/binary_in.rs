//! `in` binary-operator checks.

use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn is_valid_in_operator_rhs(&mut self, ty: TypeId) -> bool {
        use crate::query_boundaries::dispatch as query;

        if matches!(ty, TypeId::ANY | TypeId::ERROR | TypeId::OBJECT) {
            return true;
        }

        // For type parameters, check if their constraint is assignable to object.
        // Unconstrained type params are NOT valid (could be primitive) → TS2322.
        if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, ty) {
            return match crate::query_boundaries::state::checking::type_parameter_constraint(
                self.ctx.types,
                ty,
            ) {
                Some(c) => self.is_valid_in_operator_rhs(c),
                None => false,
            };
        }

        if query::is_object_like_type(self.ctx.types, ty) {
            return true;
        }

        if let Some(members) = query::union_members(self.ctx.types, ty) {
            return members
                .iter()
                .all(|&member| self.is_valid_in_operator_rhs(member));
        }

        if let Some(members) = query::intersection_members(self.ctx.types, ty) {
            return members
                .iter()
                .any(|&member| self.is_valid_in_operator_rhs(member));
        }

        let evaluated = crate::query_boundaries::dispatch::evaluate_type_with_resolver(
            self.ctx.types,
            &self.ctx,
            ty,
        );
        if evaluated != ty {
            return self.is_valid_in_operator_rhs(evaluated);
        }

        false
    }

    /// Check if a type "may represent a primitive value" for TS2638.
    ///
    /// In tsc, this fires for "instantiable" types (type parameters, conditional types)
    /// whose constraint is missing or could accept primitive values. Concrete object
    /// types like `{}` do NOT trigger TS2638 on their own — only when they appear as
    /// the constraint of a type parameter that could be instantiated with a primitive.
    fn type_may_represent_primitive(&self, ty: TypeId) -> bool {
        // The intrinsic `object` type excludes primitives by definition
        if ty == TypeId::OBJECT {
            return false;
        }

        // `unknown` can represent any value including primitives — TS2638
        if ty == TypeId::UNKNOWN {
            return true;
        }

        // Type parameters: check if constraint is missing or could be primitive.
        // A type param with no constraint or constraint `{}` may represent a primitive
        // because it could be instantiated with string, number, etc.
        if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, ty) {
            return match crate::query_boundaries::state::checking::type_parameter_constraint(
                self.ctx.types,
                ty,
            ) {
                None => true,                            // Unconstrained type param may be primitive
                Some(c) if c == TypeId::OBJECT => false, // `extends object` excludes primitives
                Some(c) => {
                    // Check if the constraint itself could accept primitives.
                    // This handles `T extends {}` (may represent primitive) vs
                    // `T extends object` (may not) vs `T extends { a: number }` (may not).
                    if self.type_may_represent_primitive(c) {
                        return true;
                    }
                    // For concrete constraints, check if a primitive is assignable
                    self.ctx.types.is_assignable_to(TypeId::STRING, c)
                }
            };
        }

        // Union: any member may represent primitive
        if let Some(members) = crate::query_boundaries::common::union_members(self.ctx.types, ty) {
            return members
                .iter()
                .any(|&m| self.type_may_represent_primitive(m));
        }

        // Intersection: `T & {}` still may represent a primitive because `{}`
        // only removes nullish values. However, `T & object`, `T & { x: ... }`,
        // and `T & Interface` exclude primitives through the object-like member.
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, ty)
        {
            let has_instantiable_primitive_member = members.iter().any(|&member| {
                crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, member)
                    && self.type_may_represent_primitive(member)
            });
            if has_instantiable_primitive_member
                && !members
                    .iter()
                    .any(|&member| self.in_operator_intersection_member_excludes_primitive(member))
            {
                return true;
            }

            return members
                .iter()
                .all(|&m| self.type_may_represent_primitive(m));
        }

        let evaluated = crate::query_boundaries::dispatch::evaluate_type_with_resolver(
            self.ctx.types,
            &self.ctx,
            ty,
        );
        if evaluated != ty {
            return self.type_may_represent_primitive(evaluated);
        }

        // Concrete object types are NOT considered "may represent primitive" —
        // only type parameters can be instantiated with primitives at runtime.
        false
    }

    /// True when `ty` is an `in`-operator RHS shape that tsc reports via
    /// TS2322 (assignability to `object`) rather than TS2638 (primitive
    /// runtime warning).
    ///
    /// tsc routes these to the assignability gateway:
    /// - bare type parameters (`T`)
    /// - unions that contain a type parameter/primitive assignability member
    ///   (`T | U`, `string | number | T`, `T | { a: string }`)
    /// - intersections whose every member is a type parameter or
    ///   primitive constraint (`T & U`, `T & (0 | 1 | 2)`)
    ///
    /// It keeps TS2638 for shapes whose apparent type is reported with a
    /// `NonNullable<T>`-style message — typically intersections with
    /// `{}`-shaped object constraints. For those, the empty-object
    /// member excludes some nullish cases without committing to the
    /// `object` constraint, and tsc emits TS2638 with the
    /// `NonNullable<T>` apparent-type display.
    fn in_rhs_is_type_parameter_assignability_shape(&self, ty: TypeId) -> bool {
        use crate::query_boundaries::common;

        if common::is_type_parameter_like(self.ctx.types, ty) {
            return true;
        }
        if let Some(members) = common::union_members(self.ctx.types, ty) {
            // A union containing a bare generic or primitive constituent is
            // reported through assignability to `object`, even when other
            // constituents are object-like. This is the shape produced by a
            // false branch of `("a" in x && "b" in x)`: `T | (T & Record<...>)`.
            return members
                .iter()
                .any(|&m| self.in_rhs_is_type_parameter_assignability_shape(m));
        }
        if let Some(members) = common::intersection_members(self.ctx.types, ty) {
            // Intersections with an empty-object-constraint member
            // (e.g. `T & {}`, `T & EmptyAlias`) collapse to
            // `NonNullable<T>` in tsc's apparent-type rendering and stay
            // on the TS2638 path. Recognize that by requiring every
            // member to be either a type parameter or a primitive — if
            // any member is an empty-object shape, defer to TS2638.
            if members.iter().any(|&m| {
                common::object_shape_for_type(self.ctx.types, m)
                    .is_some_and(|shape| shape.properties.is_empty())
            }) {
                return false;
            }
            return members
                .iter()
                .all(|&m| self.in_rhs_is_type_parameter_assignability_shape(m));
        }
        // Concrete primitives (string, number, ...) are also routed to
        // the assignability gateway when they're combined with type
        // parameters in a union / intersection that we already verified
        // above. A bare primitive (no generics involved) keeps the
        // TS2638 path because the user can fix it without touching a
        // generic position.
        common::is_primitive_type(self.ctx.types, ty)
    }

    fn in_operator_type_contains_empty_object_shape(&self, ty: TypeId) -> bool {
        use crate::query_boundaries::common;

        if common::is_empty_object_type(self.ctx.types, ty) {
            return true;
        }

        if let Some(members) = common::union_members(self.ctx.types, ty) {
            return members
                .iter()
                .any(|&member| self.in_operator_type_contains_empty_object_shape(member));
        }

        let evaluated = crate::query_boundaries::dispatch::evaluate_type_with_resolver(
            self.ctx.types,
            &self.ctx,
            ty,
        );
        evaluated != ty && self.in_operator_type_contains_empty_object_shape(evaluated)
    }

    fn in_operator_intersection_member_excludes_primitive(&self, ty: TypeId) -> bool {
        use crate::query_boundaries::{common, dispatch as query};

        if ty == TypeId::OBJECT {
            return true;
        }

        if common::is_type_parameter_like(self.ctx.types, ty) {
            return crate::query_boundaries::state::checking::type_parameter_constraint(
                self.ctx.types,
                ty,
            )
            .is_some_and(|constraint| {
                self.in_operator_intersection_member_excludes_primitive(constraint)
            });
        }

        if let Some(members) = query::union_members(self.ctx.types, ty) {
            return members
                .iter()
                .all(|&member| self.in_operator_intersection_member_excludes_primitive(member));
        }

        if let Some(members) = query::intersection_members(self.ctx.types, ty) {
            return members
                .iter()
                .any(|&member| self.in_operator_intersection_member_excludes_primitive(member));
        }

        query::is_object_like_type(self.ctx.types, ty)
            && !common::is_empty_object_type(self.ctx.types, ty)
    }

    /// Format the apparent type for display in TS2638 error messages.
    ///
    /// tsc uses the apparent type in the "may represent a primitive" message:
    /// - `unknown` → `{}`
    /// - unconstrained type parameter → `{}`
    /// - constrained type parameter → the constraint type string
    fn format_apparent_type_for_in_operator(&self, ty: TypeId) -> String {
        if ty == TypeId::UNKNOWN {
            return "{}".to_string();
        }
        if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, ty) {
            return match crate::query_boundaries::state::checking::type_parameter_constraint(
                self.ctx.types,
                ty,
            ) {
                None => format!("NonNullable<{}>", self.format_type(ty)),
                Some(c) => self.format_type(c),
            };
        }
        self.format_type(ty)
    }

    /// Check if an identifier was declared as `unknown` and has been truthiness-narrowed
    /// to an empty object `{}`.
    ///
    /// This is used for TS2638: when `unknown` is narrowed through truthiness to `{}`,
    /// we still need to emit TS2638 because the original declared type was `unknown`.
    /// However, `instanceof Object` narrowing produces a different type (the Object
    /// instance type, not `{}`) that does NOT trigger TS2638 because it genuinely
    /// excludes primitives.
    ///
    /// The two conditions we check:
    /// 1. The declared type is `unknown`
    /// 2. The narrowed type is an empty object `{}`
    ///
    /// If both are true, we emit TS2638. If the declared type is `unknown` but the
    /// narrowed type is not `{}` (e.g., after `instanceof Object`), we don't emit TS2638.
    fn truthiness_narrowed_from_unknown(
        &mut self,
        node_idx: NodeIndex,
        narrowed_type: TypeId,
    ) -> bool {
        // First check if the narrowed type includes the empty object `{}`.
        // After previous `in` checks, flow can preserve both the truthiness-
        // narrowed unknown branch and the record branch as a union.
        if !self.in_operator_type_contains_empty_object_shape(narrowed_type) {
            return false;
        }

        // Now check if the declared type is `unknown`
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(sym_id) = self.resolve_identifier_symbol(node_idx) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // Navigate to the declaration
        let decl_idx = symbol.value_declaration;
        let Some(mut decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };

        // Handle identifier inside variable declaration
        if decl_node.kind == SyntaxKind::Identifier as u16
            && let Some(ext) = self.ctx.arena.get_extended(decl_idx)
            && ext.parent.is_some()
            && let Some(parent_node) = self.ctx.arena.get(ext.parent)
        {
            decl_node = parent_node;
        }

        // Check parameter type annotation
        if let Some(param) = self.ctx.arena.get_parameter(decl_node)
            && param.type_annotation.is_some()
        {
            let declared_type = self.get_type_from_type_node(param.type_annotation);
            return declared_type == TypeId::UNKNOWN;
        }

        // Check variable declaration type annotation
        if let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node)
            && var_decl.type_annotation.is_some()
        {
            let declared_type = self.get_type_from_type_node(var_decl.type_annotation);
            return declared_type == TypeId::UNKNOWN;
        }

        false
    }

    fn expression_is_identifier_symbol(
        &mut self,
        expr_idx: NodeIndex,
        symbol_id: tsz_binder::SymbolId,
    ) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        node.kind == SyntaxKind::Identifier as u16
            && self.resolve_identifier_symbol(expr_idx) == Some(symbol_id)
    }

    fn node_matches_after_outer_expressions(&self, left: NodeIndex, right: NodeIndex) -> bool {
        self.ctx.arena.skip_parenthesized_and_assertions(left)
            == self.ctx.arena.skip_parenthesized_and_assertions(right)
    }

    fn in_rhs_has_direct_truthiness_guard(&mut self, right_idx: NodeIndex) -> bool {
        let stripped_right = self.ctx.arena.skip_parenthesized_and_assertions(right_idx);
        let Some(right_node) = self.ctx.arena.get(stripped_right) else {
            return false;
        };
        if right_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(right_symbol) = self.resolve_identifier_symbol(stripped_right) else {
            return false;
        };

        let mut current = right_idx;
        let in_expression = loop {
            let Some(parent_idx) = self.ctx.arena.get_extended(current).map(|ext| ext.parent)
            else {
                return false;
            };
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(parent_binary) = self.ctx.arena.get_binary_expr(parent_node)
                && parent_binary.operator_token == SyntaxKind::InKeyword as u16
                && self.node_matches_after_outer_expressions(parent_binary.right, current)
            {
                break parent_idx;
            }

            if matches!(
                parent_node.kind,
                syntax_kind_ext::PARENTHESIZED_EXPRESSION
                    | syntax_kind_ext::AS_EXPRESSION
                    | syntax_kind_ext::SATISFIES_EXPRESSION
                    | syntax_kind_ext::NON_NULL_EXPRESSION
                    | syntax_kind_ext::TYPE_ASSERTION
            ) {
                current = parent_idx;
                continue;
            }

            return false;
        };

        current = in_expression;
        loop {
            let Some(parent_idx) = self.ctx.arena.get_extended(current).map(|ext| ext.parent)
            else {
                return false;
            };
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };

            if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(parent_binary) = self.ctx.arena.get_binary_expr(parent_node)
                && parent_binary.operator_token == SyntaxKind::AmpersandAmpersandToken as u16
            {
                if self.node_matches_after_outer_expressions(parent_binary.right, current)
                    && self.expression_is_identifier_symbol(parent_binary.left, right_symbol)
                {
                    return true;
                }
                current = parent_idx;
                continue;
            }

            if matches!(
                parent_node.kind,
                syntax_kind_ext::PARENTHESIZED_EXPRESSION
                    | syntax_kind_ext::AS_EXPRESSION
                    | syntax_kind_ext::SATISFIES_EXPRESSION
                    | syntax_kind_ext::NON_NULL_EXPRESSION
                    | syntax_kind_ext::TYPE_ASSERTION
            ) {
                current = parent_idx;
                continue;
            }

            return false;
        }
    }

    fn expression_is_object_string_literal(&self, expr_idx: NodeIndex) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        node.kind == SyntaxKind::StringLiteral as u16
            && self
                .ctx
                .arena
                .get_literal(node)
                .is_some_and(|literal| literal.text == "object")
    }

    fn expression_is_typeof_identifier_symbol(
        &mut self,
        expr_idx: NodeIndex,
        symbol_id: tsz_binder::SymbolId,
    ) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::PREFIX_UNARY_EXPRESSION {
            return false;
        }
        let Some(unary) = self.ctx.arena.get_unary_expr(node) else {
            return false;
        };
        unary.operator == SyntaxKind::TypeOfKeyword as u16
            && self.expression_is_identifier_symbol(unary.operand, symbol_id)
    }

    fn expression_is_typeof_object_guard_for_symbol(
        &mut self,
        expr_idx: NodeIndex,
        symbol_id: tsz_binder::SymbolId,
    ) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }
        let Some(binary) = self.ctx.arena.get_binary_expr(node) else {
            return false;
        };
        if binary.operator_token != SyntaxKind::EqualsEqualsEqualsToken as u16
            && binary.operator_token != SyntaxKind::EqualsEqualsToken as u16
        {
            return false;
        }

        (self.expression_is_typeof_identifier_symbol(binary.left, symbol_id)
            && self.expression_is_object_string_literal(binary.right))
            || (self.expression_is_object_string_literal(binary.left)
                && self.expression_is_typeof_identifier_symbol(binary.right, symbol_id))
    }

    fn expression_contains_typeof_object_guard_for_symbol(
        &mut self,
        expr_idx: NodeIndex,
        symbol_id: tsz_binder::SymbolId,
    ) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        if self.expression_is_typeof_object_guard_for_symbol(expr_idx, symbol_id) {
            return true;
        }

        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
            && binary.operator_token == SyntaxKind::AmpersandAmpersandToken as u16
        {
            return self.expression_contains_typeof_object_guard_for_symbol(binary.left, symbol_id)
                || self
                    .expression_contains_typeof_object_guard_for_symbol(binary.right, symbol_id);
        }

        false
    }

    fn in_rhs_has_typeof_object_guard(&mut self, right_idx: NodeIndex) -> bool {
        let stripped_right = self.ctx.arena.skip_parenthesized_and_assertions(right_idx);
        let Some(right_node) = self.ctx.arena.get(stripped_right) else {
            return false;
        };
        if right_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(right_symbol) = self.resolve_identifier_symbol(stripped_right) else {
            return false;
        };

        let mut current = right_idx;
        let in_expression = loop {
            let Some(parent_idx) = self.ctx.arena.get_extended(current).map(|ext| ext.parent)
            else {
                return false;
            };
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(parent_binary) = self.ctx.arena.get_binary_expr(parent_node)
                && parent_binary.operator_token == SyntaxKind::InKeyword as u16
                && self.node_matches_after_outer_expressions(parent_binary.right, current)
            {
                break parent_idx;
            }

            if matches!(
                parent_node.kind,
                syntax_kind_ext::PARENTHESIZED_EXPRESSION
                    | syntax_kind_ext::AS_EXPRESSION
                    | syntax_kind_ext::SATISFIES_EXPRESSION
                    | syntax_kind_ext::NON_NULL_EXPRESSION
                    | syntax_kind_ext::TYPE_ASSERTION
            ) {
                current = parent_idx;
                continue;
            }

            return false;
        };

        current = in_expression;
        loop {
            let Some(parent_idx) = self.ctx.arena.get_extended(current).map(|ext| ext.parent)
            else {
                return false;
            };
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };

            if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(parent_binary) = self.ctx.arena.get_binary_expr(parent_node)
                && parent_binary.operator_token == SyntaxKind::AmpersandAmpersandToken as u16
            {
                if self.node_matches_after_outer_expressions(parent_binary.right, current)
                    && self.expression_contains_typeof_object_guard_for_symbol(
                        parent_binary.left,
                        right_symbol,
                    )
                {
                    return true;
                }
                current = parent_idx;
                continue;
            }

            if matches!(
                parent_node.kind,
                syntax_kind_ext::PARENTHESIZED_EXPRESSION
                    | syntax_kind_ext::AS_EXPRESSION
                    | syntax_kind_ext::SATISFIES_EXPRESSION
                    | syntax_kind_ext::NON_NULL_EXPRESSION
                    | syntax_kind_ext::TYPE_ASSERTION
            ) {
                current = parent_idx;
                continue;
            }

            return false;
        }
    }

    /// Validate that the left operand of `in` is assignable to the property-key
    /// space (`string`, `number`, or `symbol`), which is what `in` probes. On
    /// failure tsc emits TS2322 at the left operand with the key union rendered
    /// structurally, since tsc strips the `PropertyKey` alias from this target.
    fn check_in_operator_lhs_key_type(&mut self, left_idx: NodeIndex, left_type: TypeId) {
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
}
