use super::*;

impl<'a> CheckerState<'a> {
    // Extracted from `binary.rs` to keep the binary expression module under the file-size cap.

    pub(crate) fn resolve_literal_index_access_property_type(
        &mut self,
        type_id: TypeId,
    ) -> Option<TypeId> {
        let (object_type, index_type) =
            crate::query_boundaries::common::index_access_parts(self.ctx.types, type_id)?;
        let atom = crate::query_boundaries::type_computation::access::literal_property_name(
            self.ctx.types,
            index_type,
        )?;
        let property_name = self.ctx.types.resolve_atom(atom);

        self.contextual_object_literal_property_type(object_type, property_name.as_ref())
            .or_else(|| {
                self.ctx
                    .types
                    .contextual_property_type(object_type, property_name.as_ref())
            })
    }

    pub(crate) fn reduce_literal_index_access_property_types(&mut self, type_id: TypeId) -> TypeId {
        if let Some(resolved) = self.resolve_literal_index_access_property_type(type_id) {
            return resolved;
        }

        let Some(members) = crate::query_boundaries::common::union_members(self.ctx.types, type_id)
        else {
            return type_id;
        };

        let mut changed = false;
        let reduced = members
            .into_iter()
            .map(|member| {
                if let Some(resolved) = self.resolve_literal_index_access_property_type(member) {
                    changed = true;
                    resolved
                } else {
                    member
                }
            })
            .collect::<Vec<_>>();

        if changed {
            self.ctx.types.factory().union_preserve_members(reduced)
        } else {
            type_id
        }
    }

    pub(super) fn global_function_interface_type_for_instanceof(&mut self) -> Option<TypeId> {
        if !self.ctx.compiler_options.no_lib {
            return Some(TypeId::FUNCTION);
        }

        let function_sym_id = self.ctx.binder.lib_symbol_ids.iter().find_map(|&sym_id| {
            self.ctx.binder.get_symbol(sym_id).and_then(|symbol| {
                (symbol.escaped_name == "Function" && symbol.has_any_flags(symbol_flags::INTERFACE))
                    .then_some(sym_id)
            })
        });

        function_sym_id
            .map(|sym_id| self.get_type_of_symbol(sym_id))
            .or_else(|| {
                self.resolve_actual_lib_name_to_def_id_for_lowering("Function")
                    .map(|def_id| self.ctx.types.lazy(def_id))
            })
            .or_else(|| self.resolve_lib_type_by_name("Function"))
    }

    pub(super) fn declared_instanceof_left_operand_type(
        &mut self,
        left_idx: NodeIndex,
        left_type: TypeId,
    ) -> TypeId {
        let evaluator = crate::query_boundaries::common::new_binary_op_evaluator(self.ctx.types);
        if evaluator.is_valid_instanceof_left_operand(left_type) {
            return left_type;
        }

        let Some(node) = self.ctx.arena.get(left_idx) else {
            return left_type;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return left_type;
        }

        let Some(sym_id) = self.resolve_identifier_symbol(left_idx) else {
            return left_type;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return left_type;
        };

        let mut decl_idx = symbol.value_declaration;
        let Some(mut decl_node) = self.ctx.arena.get(decl_idx) else {
            return left_type;
        };
        if decl_node.kind == SyntaxKind::Identifier as u16
            && let Some(ext) = self.ctx.arena.get_extended(decl_idx)
            && ext.parent.is_some()
            && let Some(parent_node) = self.ctx.arena.get(ext.parent)
            && parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
        {
            decl_idx = ext.parent;
            decl_node = parent_node;
        }
        if !self.is_const_variable_declaration(decl_idx) {
            return left_type;
        }
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return left_type;
        };
        if var_decl.type_annotation.is_none() {
            return left_type;
        }
        if !self
            .ctx
            .binder
            .get_node_flow(left_idx)
            .and_then(|flow_id| self.ctx.binder.flow_nodes.get(flow_id))
            .is_some_and(|flow| flow.has_any_flags(tsz_binder::flow_flags::ASSIGNMENT))
        {
            return left_type;
        }

        let declared_type = self.get_type_of_symbol(sym_id);
        if evaluator.is_valid_instanceof_left_operand(declared_type) {
            declared_type
        } else {
            left_type
        }
    }

    pub(crate) fn get_type_of_write_target_base_expression(&mut self, idx: NodeIndex) -> TypeId {
        // PERF: For non-binary expressions, the write context doesn't change the
        // result type compared to the normal path. Check the node_types cache first
        // to avoid redundant type resolution through the full property-access pipeline.
        // This is especially impactful for deep optional chains like `a?.b?.c?.d`
        // where each level recursively calls this method on its base expression.
        let logical_idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        let is_binary = self
            .ctx
            .arena
            .get(logical_idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::BINARY_EXPRESSION);
        if !is_binary && let Some(&cached) = self.ctx.node_types.get(&idx.0) {
            return cached;
        }
        if let Some(node) = self.ctx.arena.get(logical_idx)
            && node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
            && matches!(
                binary.operator_token,
                k if k == SyntaxKind::BarBarToken as u16
                    || k == SyntaxKind::QuestionQuestionToken as u16
            )
        {
            let left_type = self.get_type_of_node_with_request(binary.left, &TypingRequest::NONE);
            let right_type = self.get_type_of_node_with_request(binary.right, &TypingRequest::NONE);
            let operator = if binary.operator_token == SyntaxKind::BarBarToken as u16 {
                WriteTargetLogicalOperator::LogicalOr
            } else {
                WriteTargetLogicalOperator::NullishCoalescing
            };
            match crate::query_boundaries::type_computation::core::write_target_logical_result_type(
                self.ctx.types,
                operator,
                left_type,
                right_type,
            ) {
                Some(WriteTargetLogicalResult::Type(result)) => return result,
                Some(WriteTargetLogicalResult::FallbackToLogicalExpression) => {
                    return self.get_type_of_node_with_request(
                        logical_idx,
                        &TypingRequest::for_write_context(),
                    );
                }
                None => {}
            }
        }

        self.get_type_of_node_with_request(idx, &TypingRequest::for_write_context())
    }

    /// Mirrors tsc's `getSyntacticNullishnessSemantics`. This is a purely syntactic check
    /// that determines whether an expression can ever be nullish, WITHOUT consulting the
    /// type system. For example, a variable `foo: string` returns `Sometimes` (it could
    /// theoretically be reassigned at runtime), while a literal `"hello"` returns `Never`.
    #[allow(dead_code)]
    pub(super) fn get_syntactic_nullishness(&self, idx: NodeIndex) -> SyntacticNullishness {
        let Some(node) = self.ctx.arena.get(idx) else {
            return SyntacticNullishness::Sometimes;
        };

        let kind = node.kind;

        // Skip parenthesized expressions (tsc's skipOuterExpressions)
        if kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.ctx.arena.get_parenthesized(node)
        {
            return self.get_syntactic_nullishness(paren.expression);
        }

        // Non-null assertions (!): always Never
        if kind == syntax_kind_ext::NON_NULL_EXPRESSION {
            return SyntacticNullishness::Never;
        }

        // Type assertions (as/satisfies/<T>x): tsc skips these via skipOuterExpressions
        if kind == syntax_kind_ext::AS_EXPRESSION
            || kind == syntax_kind_ext::SATISFIES_EXPRESSION
            || kind == syntax_kind_ext::TYPE_ASSERTION
        {
            return SyntacticNullishness::Sometimes;
        }

        // Expressions that may produce null/undefined at runtime
        if kind == syntax_kind_ext::AWAIT_EXPRESSION
            || kind == syntax_kind_ext::CALL_EXPRESSION
            || kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
            || kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            || kind == syntax_kind_ext::META_PROPERTY
            || kind == syntax_kind_ext::NEW_EXPRESSION
            || kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || kind == syntax_kind_ext::YIELD_EXPRESSION
            || kind == SyntaxKind::ThisKeyword as u16
        {
            return SyntacticNullishness::Sometimes;
        }

        // Binary expressions
        if kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
        {
            let op = binary.operator_token;
            // ||, ||=, &&, &&= can produce null/undefined
            if op == SyntaxKind::BarBarToken as u16
                || op == SyntaxKind::BarBarEqualsToken as u16
                || op == SyntaxKind::AmpersandAmpersandToken as u16
                || op == SyntaxKind::AmpersandAmpersandEqualsToken as u16
            {
                return SyntacticNullishness::Sometimes;
            }
            // For ??, ??=, =, comma: result nullishness is determined by right operand
            if op == SyntaxKind::CommaToken as u16
                || op == SyntaxKind::EqualsToken as u16
                || op == SyntaxKind::QuestionQuestionToken as u16
                || op == SyntaxKind::QuestionQuestionEqualsToken as u16
            {
                return self.get_syntactic_nullishness(binary.right);
            }
            // All other binary operators (arithmetic, comparison, bitwise, etc.)
            // never produce null/undefined
            return SyntacticNullishness::Never;
        }

        // Conditional expression: union of true and false branches
        if kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
            && let Some(cond) = self.ctx.arena.get_conditional_expr(node)
        {
            let when_true = self.get_syntactic_nullishness(cond.when_true);
            let when_false = self.get_syntactic_nullishness(cond.when_false);
            if when_true == SyntacticNullishness::Never && when_false == SyntacticNullishness::Never
            {
                return SyntacticNullishness::Never;
            }
            if when_true == SyntacticNullishness::Always
                && when_false == SyntacticNullishness::Always
            {
                return SyntacticNullishness::Always;
            }
            return SyntacticNullishness::Sometimes;
        }

        // null keyword
        if kind == SyntaxKind::NullKeyword as u16 {
            return SyntacticNullishness::Always;
        }

        // Identifier: check if it's `undefined`
        if kind == SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.ctx.arena.get_identifier(node)
                && ident.escaped_text == "undefined"
            {
                return SyntacticNullishness::Always;
            }
            return SyntacticNullishness::Sometimes;
        }

        // Everything else: literals (string, number, boolean, bigint, regex, template,
        // object literal, array literal, function expression, arrow function, class expression,
        // etc.) are never nullish.
        SyntacticNullishness::Never
    }
    pub(super) fn is_valid_in_operator_rhs(&mut self, ty: TypeId) -> bool {
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

    /// Check if an AST node is a nullish coalescing expression (`??`) or a
    /// literal value (string, number, boolean, bigint, template), unwrapping
    /// parentheses. TSC only emits TS2869 for these syntactic forms; general
    /// non-nullable expressions (identifiers, property access, `&&` chains)
    /// do not trigger TS2869 even when their type is never nullish.
    pub(super) fn is_nullish_coalescing_or_literal(
        arena: &tsz_parser::parser::NodeArena,
        node_idx: NodeIndex,
    ) -> bool {
        use tsz_scanner::SyntaxKind;

        let Some(node) = arena.get(node_idx) else {
            return false;
        };

        // Unwrap parentheses: (expr) -> expr
        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            if let Some(paren) = arena.get_parenthesized(node) {
                return Self::is_nullish_coalescing_or_literal(arena, paren.expression);
            }
            return false;
        }

        // Binary expression: check if it's a `??`
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            if let Some(binary) = arena.get_binary_expr(node) {
                return binary.operator_token == SyntaxKind::QuestionQuestionToken as u16;
            }
            return false;
        }

        // Literal values: string, number, bigint, template, true, false
        let kind = node.kind;
        kind == SyntaxKind::StringLiteral as u16
            || kind == SyntaxKind::NumericLiteral as u16
            || kind == SyntaxKind::BigIntLiteral as u16
            || kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            || kind == SyntaxKind::TrueKeyword as u16
            || kind == SyntaxKind::FalseKeyword as u16
            || kind == syntax_kind_ext::TEMPLATE_EXPRESSION
    }

    /// Check if a type "may represent a primitive value" for TS2638.
    ///
    /// In tsc, this fires for "instantiable" types (type parameters, conditional types)
    /// whose constraint is missing or could accept primitive values. Concrete object
    /// types like `{}` do NOT trigger TS2638 on their own — only when they appear as
    /// the constraint of a type parameter that could be instantiated with a primitive.
    pub(super) fn type_may_represent_primitive(&self, ty: TypeId) -> bool {
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
    pub(super) fn in_rhs_is_type_parameter_assignability_shape(&self, ty: TypeId) -> bool {
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

    pub(super) fn in_operator_type_contains_empty_object_shape(&self, ty: TypeId) -> bool {
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

    pub(super) fn in_operator_intersection_member_excludes_primitive(&self, ty: TypeId) -> bool {
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
    pub(super) fn format_apparent_type_for_in_operator(&self, ty: TypeId) -> String {
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
    pub(super) fn truthiness_narrowed_from_unknown(
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

    pub(super) fn expression_is_identifier_symbol(
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

    pub(super) fn node_matches_after_outer_expressions(
        &self,
        left: NodeIndex,
        right: NodeIndex,
    ) -> bool {
        self.ctx.arena.skip_parenthesized_and_assertions(left)
            == self.ctx.arena.skip_parenthesized_and_assertions(right)
    }

    pub(super) fn in_rhs_has_direct_truthiness_guard(&mut self, right_idx: NodeIndex) -> bool {
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

    pub(super) fn expression_is_object_string_literal(&self, expr_idx: NodeIndex) -> bool {
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

    pub(super) fn expression_is_typeof_identifier_symbol(
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

    pub(super) fn expression_is_typeof_object_guard_for_symbol(
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

    pub(super) fn expression_contains_typeof_object_guard_for_symbol(
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

    pub(super) fn in_rhs_has_typeof_object_guard(&mut self, right_idx: NodeIndex) -> bool {
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
}
