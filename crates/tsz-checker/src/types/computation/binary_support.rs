//! Binary expression type computation.
//! Extracted from `core.rs` — handles all binary operators including
//! arithmetic, comparison, logical, assignment, nullish coalescing, and comma.

use crate::context::TypingRequest;
use crate::query_boundaries::type_computation::core::{
    WriteTargetLogicalOperator, WriteTargetLogicalResult,
};
use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// Result of syntactic nullishness analysis, mirroring tsc's `PredicateSemantics`.
/// This is a purely syntactic check -- it does NOT look at types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum SyntacticNullishness {
    /// The expression is always nullish (e.g., `null`, `undefined`).
    #[allow(dead_code)]
    Always,
    /// The expression may or may not be nullish (e.g., identifiers, calls, property accesses).
    Sometimes,
    /// The expression is never nullish (e.g., literals, arithmetic results, `??` results).
    Never,
}

impl<'a> CheckerState<'a> {
    /// Recover the un-widened literal type of a logical-operator (`&&`/`||`/`??`)
    /// operand for the result union, when a literal-preserving context requested
    /// it (`ctx.preserve_logical_operand_literals`).
    ///
    /// tsc forms the logical result by unioning the operand types *as checked*,
    /// preserving fresh literal operands (`"yes"`, `9`); the widening to the base
    /// primitive only happens later at mutable (`let`/`var`) binding sites via
    /// `getWidenedLiteralType`. tsz types operands with literal widening already
    /// applied, so a literal operand arrives here as its base primitive
    /// (`"yes"` → `string`). For unannotated `const` initializers the result must
    /// stay precise (e.g. `const x = a && "yes"` with `a: 0 | 1` is `0 | "yes"`,
    /// not `0 | string`); for mutable bindings and other contexts the widened
    /// operand type is kept so the result matches tsc's `getWidenedLiteralType`.
    ///
    /// Only top-level primitive literals are recovered; object/array literal
    /// operands keep their widened shape, matching tsc (object widening is
    /// independent of `const`-ness).
    pub(super) fn preserve_logical_operand_literal(
        &self,
        operand: NodeIndex,
        widened: TypeId,
    ) -> TypeId {
        if !self.ctx.preserve_logical_operand_literals {
            return widened;
        }
        self.literal_type_from_initializer(operand)
            .unwrap_or(widened)
    }

    /// True when `idx` (looking through parentheses) is a `&&`/`||`/`??` logical
    /// binary expression. Used to scope `preserve_logical_operand_literals` to
    /// `const` initializers that are *themselves* a logical expression, so the
    /// flag never enters nested array/object/call initializer contexts (which
    /// keep their existing widening).
    pub(crate) fn is_logical_binary_expression(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            return self
                .ctx
                .arena
                .get_parenthesized(node)
                .is_some_and(|paren| self.is_logical_binary_expression(paren.expression));
        }
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
        {
            let op = binary.operator_token;
            return op == SyntaxKind::AmpersandAmpersandToken as u16
                || op == SyntaxKind::BarBarToken as u16
                || op == SyntaxKind::QuestionQuestionToken as u16;
        }
        false
    }

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

    fn global_function_interface_type_for_instanceof(&mut self) -> Option<TypeId> {
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

    fn declared_instanceof_left_operand_type(
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
    fn get_syntactic_nullishness(&self, idx: NodeIndex) -> SyntacticNullishness {
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

    fn is_inside_loop(&self, idx: NodeIndex) -> bool {
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
    fn get_primitive_family(&self, type_id: TypeId) -> TypeId {
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
