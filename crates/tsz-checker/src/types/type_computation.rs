//! Type computation helpers, relationship queries, and format utilities.
//! This module extends `CheckerState` with additional methods for type-related
//! operations, providing cleaner APIs for common patterns.

use crate::diagnostics::Diagnostic;
use crate::query_boundaries::type_computation::evaluate_contextual_structure_with;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::Visibility;
use tsz_solver::{ContextualTypeContext, TupleElement, TypeId, expression_ops};

// =============================================================================
// Type Computation Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    pub(crate) fn is_identifier_reference_to_global_nan(&self, node_idx: NodeIndex) -> bool {
        let mut current_idx = node_idx;
        while let Some(node) = self.ctx.arena.get(current_idx) {
            if node.kind == tsz_parser::syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(expr) = self.ctx.arena.get_parenthesized(node)
            {
                current_idx = expr.expression;
                continue;
            }
            break;
        }

        if let Some(node) = self.ctx.arena.get(current_idx)
            && node.kind == tsz_scanner::SyntaxKind::Identifier as u16
            && let Some(ident) = self.ctx.arena.get_identifier(node)
            && ident.escaped_text == "NaN"
        {
            if let Some(sym_id) = self.resolve_identifier_symbol(current_idx) {
                let is_global = self
                    .ctx
                    .binder
                    .get_symbol(sym_id)
                    .is_none_or(|s| s.parent.is_none());
                return self.ctx.symbol_is_from_lib(sym_id) || is_global;
            }
            return true; // Unresolved NaN treated as global
        }
        false
    }

    // Core Type Computation
    // =========================================================================

    /// Evaluate a type deeply for binary operation checking.
    ///
    /// Unlike `evaluate_type_with_resolution` which only handles the top-level type,
    /// this also evaluates individual members of union types. This is needed because
    /// types like `DeepPartial<number> | number` are stored as a union where one
    /// member is an unevaluated Application type that the solver's `NumberLikeVisitor`
    /// can't handle.
    pub(crate) fn evaluate_type_for_binary_ops(&mut self, type_id: TypeId) -> TypeId {
        let db = self.ctx.types;
        let mut evaluate_leaf = |leaf_type: TypeId| self.evaluate_type_with_resolution(leaf_type);
        evaluate_contextual_structure_with(db, type_id, &mut evaluate_leaf)
    }

    /// Evaluate a contextual type that may contain unevaluated mapped/conditional types.
    ///
    /// When a generic function's parameter type is instantiated (e.g., `{ [K in keyof P]: P[K] }`
    /// with P=Props), the result may be a mapped type with `Lazy` references that need a
    /// full resolver to evaluate. The solver's default `contextual_property_type` uses
    /// `NoopResolver` and can't resolve these. This method uses the Judge (which has access
    /// to the `TypeEnvironment` resolver) to evaluate such types into concrete object types.
    pub(crate) fn evaluate_contextual_type(&self, type_id: TypeId) -> TypeId {
        let mut evaluate_leaf = |leaf_type: TypeId| self.judge_evaluate(leaf_type);
        evaluate_contextual_structure_with(self.ctx.types, type_id, &mut evaluate_leaf)
    }

    /// Get the type of a conditional expression (ternary operator).
    ///
    /// Computes the type of `condition ? whenTrue : whenFalse`.
    /// Returns the union of the two branch types if they differ.
    ///
    /// When a contextual type is available, each branch is checked against it
    /// to catch type errors (TS2322).
    ///
    /// Uses `solver::compute_conditional_expression_type` for type computation
    /// as part of the Solver-First architecture migration.
    pub(crate) fn get_type_of_conditional_expression(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(cond) = self.ctx.arena.get_conditional_expr(node) else {
            return TypeId::ERROR;
        };

        // Get condition type for type computation
        let condition_type = self.get_type_of_node(cond.condition);
        self.check_truthy_or_falsy_with_type(cond.condition, condition_type);

        // Apply contextual typing to each branch for better inference,
        // but don't check assignability here - that happens at the call site.
        // This allows `cond ? "a" : "b"` to infer as `"a" | "b"` and then
        // the union is checked against the contextual type.
        let prev_context = self.ctx.contextual_type;

        // Preserve literal types in conditional branches so that
        // `const x = cond ? "a" : "b"` infers `"a" | "b"` (tsc behavior).
        let prev_preserve = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;

        // Compute branch types with the outer contextual type for inference.
        // Branch typing may mutate contextual state while recursing, so restore
        // it explicitly before each branch.
        let when_true = self.get_type_of_node(cond.when_true);

        self.ctx.contextual_type = prev_context;
        let when_false = self.get_type_of_node(cond.when_false);

        self.ctx.contextual_type = prev_context;
        self.ctx.preserve_literal_types = prev_preserve;

        // Use Solver API for type computation (Solver-First architecture)
        expression_ops::compute_conditional_expression_type(
            self.ctx.types,
            condition_type,
            when_true,
            when_false,
        )
    }

    /// Get type of array literal.
    ///
    /// Computes the type of array literals like `[1, 2, 3]` or `["a", "b"]`.
    /// Handles:
    /// - Empty arrays (infer from context or use never[])
    /// - Tuple contexts (e.g., `[string, number]`)
    /// - Spread elements (`[...arr]`)
    /// - Common type inference for mixed elements
    pub(crate) fn get_type_of_array_literal(&mut self, idx: NodeIndex) -> TypeId {
        let factory = self.ctx.types.factory();
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(array) = self.ctx.arena.get_literal_expr(node) else {
            return TypeId::ERROR;
        };

        if array.elements.nodes.is_empty() {
            // Empty array literal: infer from context or use never[]/any[]
            // TypeScript uses "evolving array types" where [] starts as never[] and widens
            // via control flow.
            if let Some(contextual) = self.ctx.contextual_type {
                let resolved = self.resolve_type_for_property_access(contextual);
                let resolved = self.resolve_lazy_type(resolved);
                if tsz_solver::type_queries::is_tuple_type(self.ctx.types, resolved) {
                    return factory.tuple(vec![]);
                } else if let Some(t_elem) =
                    tsz_solver::type_queries::get_array_element_type(self.ctx.types, resolved)
                {
                    return factory.array(t_elem);
                }
            }

            // When noImplicitAny is off, empty array literals without contextual type
            // are typed as any[] (matching tsc behavior). With noImplicitAny on, use never[]
            // which is the "evolving array type" starting point.
            if !self.ctx.no_implicit_any() {
                return factory.array(TypeId::ANY);
            }
            return factory.array(TypeId::NEVER);
        }

        // Resolve lazy type aliases once and reuse for both tuple_context and ctx_helper
        // This ensures type aliases (e.g., type Tup = [string, number]) are expanded
        // before checking for tuple elements and providing contextual typing
        let resolved_contextual_type = self
            .ctx
            .contextual_type
            .map(|ctx_type| self.resolve_lazy_type(ctx_type));

        // When the contextual type is a union like `[number] | string`, narrow it to
        // only the array/tuple constituents applicable to an array literal. This ensures
        // `[1]` with contextual type `[number] | string` is typed as `[number]` not `number[]`.
        let applicable_contextual_type = resolved_contextual_type.and_then(|resolved| {
            let evaluated = self.evaluate_application_type(resolved);
            tsz_solver::type_queries::get_array_applicable_type(self.ctx.types, evaluated)
        });

        let tuple_context = match applicable_contextual_type {
            Some(applicable) => {
                tsz_solver::type_queries::get_tuple_elements(self.ctx.types, applicable)
            }
            None => None,
        };

        // Use the applicable (narrowed) type for contextual typing when available,
        // falling back to the full resolved contextual type
        let effective_contextual = applicable_contextual_type.or(resolved_contextual_type);
        let ctx_helper = match effective_contextual {
            Some(resolved) => Some(ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                resolved,
                self.ctx.compiler_options.no_implicit_any,
            )),
            None => None,
        };

        // Get types of all elements, applying contextual typing when available.
        // Track (type, node_index) pairs for excess property checking on array elements.
        let mut element_types = Vec::new();
        let mut element_nodes = Vec::new();
        let mut tuple_elements = Vec::new();
        for (index, &elem_idx) in array.elements.nodes.iter().enumerate() {
            if elem_idx.is_none() {
                continue;
            }

            let prev_context = self.ctx.contextual_type;
            if let Some(ref helper) = ctx_helper {
                if tuple_context.is_some() {
                    self.ctx.contextual_type = helper.get_tuple_element_type(index);
                } else {
                    self.ctx.contextual_type = helper.get_array_element_type();
                }
            }

            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            let elem_is_spread = elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT;

            // Handle spread elements - expand tuple types
            if elem_is_spread && let Some(spread_data) = self.ctx.arena.get_spread(elem_node) {
                let spread_expr_type = self.get_type_of_node(spread_data.expression);
                let spread_expr_type = self.resolve_lazy_type(spread_expr_type);
                // Check if spread argument is iterable, emit TS2488 if not.
                // Skip this check when the array is a destructuring target
                // (e.g., `[...c] = expr`), since the spread element is an assignment
                // target, not a value being spread into a new array.
                if !self.ctx.in_destructuring_target {
                    self.check_spread_iterability(spread_expr_type, spread_data.expression);
                }

                // If it's a tuple type, expand its elements
                if let Some(elems) =
                    tsz_solver::type_queries::get_tuple_elements(self.ctx.types, spread_expr_type)
                {
                    if let Some(ref _expected) = tuple_context {
                        // For tuple context, add each element with spread flag
                        for elem in &elems {
                            let (name, optional) =
                                match tuple_context.as_ref().and_then(|tc| tc.get(index)) {
                                    Some(el) => (el.name, el.optional),
                                    None => (None, false),
                                };
                            tuple_elements.push(TupleElement {
                                type_id: elem.type_id,
                                name,
                                optional,
                                rest: false, // Individual tuple elements are not spreads
                            });
                            // Don't increment index here - each tuple element maps to position
                        }
                    } else {
                        // For array context, add element types
                        for elem in &elems {
                            element_types.push(elem.type_id);
                        }
                    }
                    self.ctx.contextual_type = prev_context;
                    continue;
                }

                // For non-tuple spreads in array context, use element type
                // For tuple context, use the spread type itself
                let elem_type = if tuple_context.is_some() {
                    spread_expr_type
                } else {
                    self.for_of_element_type(spread_expr_type)
                };

                self.ctx.contextual_type = prev_context;

                if let Some(ref _expected) = tuple_context {
                    let (name, optional) = match tuple_context.as_ref().and_then(|tc| tc.get(index))
                    {
                        Some(el) => (el.name, el.optional),
                        None => (None, false),
                    };
                    tuple_elements.push(TupleElement {
                        type_id: elem_type,
                        name,
                        optional,
                        rest: true, // Mark as spread for non-tuple spreads in tuple context
                    });
                } else {
                    element_types.push(elem_type);
                }
                continue;
            }

            // Regular (non-spread) element
            let elem_type = self.get_type_of_node(elem_idx);

            self.ctx.contextual_type = prev_context;

            if let Some(ref _expected) = tuple_context {
                let (name, optional) = match tuple_context.as_ref().and_then(|tc| tc.get(index)) {
                    Some(el) => (el.name, el.optional),
                    None => (None, false),
                };
                tuple_elements.push(TupleElement {
                    type_id: elem_type,
                    name,
                    optional,
                    rest: false,
                });
            } else {
                element_types.push(elem_type);
                element_nodes.push(elem_idx);
            }
        }

        if tuple_context.is_some() {
            return factory.tuple(tuple_elements);
        }

        // When in a const assertion context, array literals become tuples (not arrays)
        // This allows [1, 2, 3] as const to become readonly [1, 2, 3] instead of readonly Array<number>
        if self.ctx.in_const_assertion {
            // Convert element_types to tuple_elements
            let const_tuple_elements: Vec<tsz_solver::TupleElement> = element_types
                .iter()
                .map(|&type_id| tsz_solver::TupleElement {
                    type_id,
                    name: None,
                    optional: false,
                    rest: false,
                })
                .collect();
            return factory.tuple(const_tuple_elements);
        }

        // Use contextual element type when available for better inference
        if let Some(ref helper) = ctx_helper
            && let Some(context_element_type) = helper.get_array_element_type()
        {
            // Check if all elements are structurally compatible with the contextual type.
            // IMPORTANT: Use is_subtype_of (structural check) instead of is_assignable_to
            // because is_assignable_to includes excess property checking which would
            // reject fresh object literals like `{a: 1, b: 2}` against `Foo {a: number}`.
            // Excess properties should be checked separately, not block contextual typing.
            if element_types
                .iter()
                .all(|&elem_type| self.is_subtype_of(elem_type, context_element_type))
            {
                // Check excess properties on each element before collapsing to contextual type.
                // Fresh object literal types would be lost after returning Array<ContextualType>,
                // so we must check excess properties here while the fresh types are still available.
                for (elem_type, elem_node) in element_types.iter().zip(element_nodes.iter()) {
                    self.check_object_literal_excess_properties(
                        *elem_type,
                        context_element_type,
                        *elem_node,
                    );
                }
                return factory.array(context_element_type);
            }
        }

        // Use Solver API for Best Common Type computation (Solver-First architecture)
        let element_type = expression_ops::compute_best_common_type(
            self.ctx.types,
            &element_types,
            Some(&self.ctx), // Pass TypeResolver for class hierarchy BCT
        );

        factory.array(element_type)
    }

    /// Get type of prefix unary expression.
    ///
    /// Computes the type of unary expressions like `!x`, `+x`, `-x`, `~x`, `++x`, `--x`, `typeof x`.
    /// Returns boolean for `!`, number for arithmetic operators, string for `typeof`.
    pub(crate) fn get_type_of_prefix_unary(&mut self, idx: NodeIndex) -> TypeId {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_scanner::SyntaxKind;
        use tsz_solver::type_queries::{LiteralTypeKind, classify_literal_type};

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(unary) = self.ctx.arena.get_unary_expr(node) else {
            return TypeId::ERROR;
        };

        match unary.operator {
            // ! returns boolean — also check operand for always-truthy/falsy (TS2872/TS2873)
            k if k == SyntaxKind::ExclamationToken as u16 => {
                // Type-check operand fully so inner expression diagnostics fire
                // (e.g. TS18050 for `!(null + undefined)`).
                self.get_type_of_node(unary.operand);
                self.check_truthy_or_falsy(unary.operand);
                TypeId::BOOLEAN
            }
            // typeof returns string but still type-check operand for flow/node types.
            k if k == SyntaxKind::TypeOfKeyword as u16 => {
                self.get_type_of_node(unary.operand);
                TypeId::STRING
            }
            // Unary + and - return number unless contextual typing expects a numeric literal.
            // Note: tsc does NOT validate operand types for unary +/-. Unary + is a
            // common idiom for number conversion (+someString), and tsc allows it freely.
            k if k == SyntaxKind::PlusToken as u16 || k == SyntaxKind::MinusToken as u16 => {
                // Evaluate operand for side effects / flow analysis but don't type-check it
                self.get_type_of_node(unary.operand);

                if let Some(literal_type) = self.literal_type_from_initializer(idx) {
                    if self.contextual_literal_type(literal_type).is_some() {
                        return literal_type;
                    }

                    if matches!(
                        classify_literal_type(self.ctx.types, literal_type),
                        LiteralTypeKind::BigInt(_)
                    ) {
                        if unary.operator == SyntaxKind::PlusToken as u16 {
                            if let Some(node) = self.ctx.arena.get(idx) {
                                let message = format_message(
                                    diagnostic_messages::OPERATOR_CANNOT_BE_APPLIED_TO_TYPE,
                                    &["+", "bigint"],
                                );
                                self.ctx.error(
                                    node.pos,
                                    node.end.saturating_sub(node.pos),
                                    message,
                                    diagnostic_codes::OPERATOR_CANNOT_BE_APPLIED_TO_TYPE,
                                );
                            }
                            return TypeId::ERROR;
                        }

                        // Preserve bigint literals for unary +/- to avoid widening to number in
                        // numeric-literal assignments (`const negZero: 0n = -0n`).
                        return literal_type;
                    }
                }

                TypeId::NUMBER
            }
            // ~ (bitwise NOT) returns number
            // Note: tsc does NOT validate operand types for ~, same as unary +/-
            k if k == SyntaxKind::TildeToken as u16 => {
                // Evaluate operand for side effects / flow analysis but don't type-check it
                self.get_type_of_node(unary.operand);
                TypeId::NUMBER
            }
            // ++ and -- require numeric operand and valid l-value
            k if k == SyntaxKind::PlusPlusToken as u16
                || k == SyntaxKind::MinusMinusToken as u16 =>
            {
                // TS1100: Invalid use of 'eval'/'arguments' in strict mode.
                // Must come before TS2356 to match TSC's diagnostic priority.
                let mut emitted_strict = false;
                if let Some(operand_node) = self.ctx.arena.get(unary.operand)
                    && operand_node.kind == SyntaxKind::Identifier as u16
                    && let Some(id_data) = self.ctx.arena.get_identifier(operand_node)
                    && (id_data.escaped_text == "eval" || id_data.escaped_text == "arguments")
                    && self.is_strict_mode_for_node(unary.operand)
                {
                    use crate::diagnostics::diagnostic_codes;
                    let code = if self.ctx.enclosing_class.is_some() {
                        diagnostic_codes::CODE_CONTAINED_IN_A_CLASS_IS_EVALUATED_IN_JAVASCRIPTS_STRICT_MODE_WHICH_DOES_NOT
                    } else {
                        diagnostic_codes::INVALID_USE_OF_IN_STRICT_MODE
                    };
                    self.error_at_node_msg(unary.operand, code, &[&id_data.escaped_text]);
                    emitted_strict = true;
                }

                // Get operand type for validation.
                // TSC checks arithmetic type BEFORE lvalue — if the type check
                // fails (TS2356), the lvalue check (TS2357) is skipped.
                let operand_type = self.get_type_of_node(unary.operand);
                let mut arithmetic_ok = true;

                if !emitted_strict {
                    use tsz_solver::BinaryOpEvaluator;
                    let evaluator = BinaryOpEvaluator::new(self.ctx.types);
                    // When strictNullChecks is off, null/undefined are silently
                    // assignable to number, so skip arithmetic check for them.
                    let is_valid = evaluator.is_arithmetic_operand(operand_type)
                        || (!self.ctx.strict_null_checks()
                            && (operand_type == TypeId::NULL || operand_type == TypeId::UNDEFINED));

                    if !is_valid {
                        arithmetic_ok = false;
                        // Emit TS2356 for invalid increment/decrement operand type
                        if let Some(loc) = self.get_source_location(unary.operand) {
                            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                            self.ctx.diagnostics.push(Diagnostic::error(self.ctx.file_name.clone(), loc.start, loc.length(), diagnostic_messages::AN_ARITHMETIC_OPERAND_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT_OR_AN_ENUM_TYPE.to_string(), diagnostic_codes::AN_ARITHMETIC_OPERAND_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT_OR_AN_ENUM_TYPE));
                        }
                    }
                }

                // Only check lvalue and assignment restrictions when arithmetic
                // type is valid (matches TSC: TS2357 is skipped when TS2356 fires).
                if arithmetic_ok {
                    let emitted_lvalue = self.check_increment_decrement_operand(unary.operand);

                    if !emitted_lvalue {
                        // TS2588: Cannot assign to 'x' because it is a constant.
                        let is_const = self.check_const_assignment(unary.operand);

                        // TS2630: Cannot assign to 'x' because it is a function.
                        self.check_function_assignment(unary.operand);

                        // TS2540: Cannot assign to readonly property
                        if !is_const {
                            self.check_readonly_assignment(unary.operand, idx);
                        }
                    }
                }

                TypeId::NUMBER
            }
            // delete returns boolean and checks that operand is a property reference
            k if k == SyntaxKind::DeleteKeyword as u16 => {
                // Evaluate operand for side effects / flow analysis
                self.get_type_of_node(unary.operand);

                // TS1102: delete cannot be called on an identifier in strict mode.
                let is_identifier_operand = unary.operand.is_some()
                    && self
                        .ctx
                        .arena
                        .get(unary.operand)
                        .is_some_and(|operand_node| {
                            operand_node.kind == SyntaxKind::Identifier as u16
                        });
                if is_identifier_operand && self.is_strict_mode_for_node(idx) {
                    self.error_at_node(
                        unary.operand,
                        crate::diagnostics::diagnostic_messages::DELETE_CANNOT_BE_CALLED_ON_AN_IDENTIFIER_IN_STRICT_MODE,
                        crate::diagnostics::diagnostic_codes::DELETE_CANNOT_BE_CALLED_ON_AN_IDENTIFIER_IN_STRICT_MODE,
                    );
                }

                // TS2703: The operand of a 'delete' operator must be a property reference.
                // Valid operands: property access (obj.prop), element access (obj["prop"]),
                // or optional chain (obj?.prop). All other expressions are invalid.
                let is_property_reference = unary.operand.is_some()
                    && self
                        .ctx
                        .arena
                        .get(unary.operand)
                        .is_some_and(|operand_node| {
                            use tsz_parser::parser::syntax_kind_ext;
                            operand_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                                || operand_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                        });

                if !is_property_reference {
                    self.error_at_node(
                        unary.operand,
                        crate::diagnostics::diagnostic_messages::THE_OPERAND_OF_A_DELETE_OPERATOR_MUST_BE_A_PROPERTY_REFERENCE,
                        crate::diagnostics::diagnostic_codes::THE_OPERAND_OF_A_DELETE_OPERATOR_MUST_BE_A_PROPERTY_REFERENCE,
                    );
                }
                // TS2790: In strictNullChecks, delete is only allowed for optional properties.
                // With exactOptionalPropertyTypes disabled, properties whose declared type
                // includes `undefined` are also treated as deletable.
                if self.ctx.compiler_options.strict_null_checks
                    && let Some(operand_node) = self.ctx.arena.get(unary.operand)
                    && operand_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    && let Some(access) = self.ctx.arena.get_access_expr(operand_node)
                {
                    use tsz_solver::operations::property::PropertyAccessResult;
                    let prop_name = self
                        .ctx
                        .arena
                        .get_identifier_at(access.name_or_argument)
                        .map(|ident| ident.escaped_text.clone())
                        .or_else(|| self.get_literal_string_from_node(access.name_or_argument));
                    if let Some(prop_name) = prop_name {
                        let object_type = self.get_type_of_node(access.expression);
                        if object_type != TypeId::ANY
                            && object_type != TypeId::UNKNOWN
                            && object_type != TypeId::ERROR
                            && object_type != TypeId::NEVER
                            && let PropertyAccessResult::Success { type_id, .. } =
                                self.resolve_property_access_with_env(object_type, &prop_name)
                        {
                            let is_optional = self.is_property_optional(object_type, &prop_name);
                            let optional_via_undefined =
                                !self.ctx.compiler_options.exact_optional_property_types
                                    && tsz_solver::type_queries::type_includes_undefined(
                                        self.ctx.types,
                                        type_id,
                                    );
                            if !is_optional && !optional_via_undefined {
                                self.error_at_node(
                                    unary.operand,
                                    crate::diagnostics::diagnostic_messages::THE_OPERAND_OF_A_DELETE_OPERATOR_MUST_BE_OPTIONAL,
                                    crate::diagnostics::diagnostic_codes::THE_OPERAND_OF_A_DELETE_OPERATOR_MUST_BE_OPTIONAL,
                                );
                            }
                        }
                    }
                }

                TypeId::BOOLEAN
            }
            // void returns undefined
            k if k == SyntaxKind::VoidKeyword as u16 => {
                // Evaluate operand for side effects / flow analysis
                self.get_type_of_node(unary.operand);
                TypeId::UNDEFINED
            }
            _ => TypeId::ANY,
        }
    }

    pub(crate) fn is_strict_mode_for_node(&self, idx: NodeIndex) -> bool {
        if self.ctx.compiler_options.always_strict {
            return true;
        }

        let is_external_module = self
            .ctx
            .is_external_module_by_file
            .as_ref()
            .is_some_and(|map| map.get(&self.ctx.file_name).is_some_and(|is_ext| *is_ext))
            || self.ctx.binder.is_external_module();

        if is_external_module {
            return true;
        }

        let statement_is_use_strict = |stmt_idx: NodeIndex, ctx: &Self| -> bool {
            ctx.ctx
                .arena
                .get(stmt_idx)
                .filter(|stmt| stmt.kind == syntax_kind_ext::EXPRESSION_STATEMENT)
                .and_then(|stmt| ctx.ctx.arena.get_expression_statement(stmt))
                .and_then(|expr_stmt| ctx.ctx.arena.get(expr_stmt.expression))
                .filter(|expr_node| expr_node.kind == SyntaxKind::StringLiteral as u16)
                .and_then(|expr_node| ctx.ctx.arena.get_literal(expr_node))
                .is_some_and(|lit| lit.text == "use strict")
        };
        let block_has_use_strict = |block_idx: NodeIndex, ctx: &Self| -> bool {
            let Some(block_node) = ctx.ctx.arena.get(block_idx) else {
                return false;
            };
            let Some(block) = ctx.ctx.arena.get_block(block_node) else {
                return false;
            };
            for &stmt_idx in &block.statements.nodes {
                if statement_is_use_strict(stmt_idx, ctx) {
                    return true;
                }
                let Some(stmt_node) = ctx.ctx.arena.get(stmt_idx) else {
                    return false;
                };
                if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                    break;
                }
            }
            false
        };

        let mut current = idx;
        loop {
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };

            if node.kind == syntax_kind_ext::CLASS_DECLARATION
                || node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                return true;
            }

            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };

            match parent_node.kind {
                k if k == syntax_kind_ext::SOURCE_FILE => {
                    if let Some(sf) = self.ctx.arena.get_source_file(parent_node)
                        && sf
                            .statements
                            .nodes
                            .iter()
                            .any(|stmt_idx| statement_is_use_strict(*stmt_idx, self))
                    {
                        return true;
                    }
                    return false;
                }
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION =>
                {
                    if let Some(func) = self.ctx.arena.get_function(parent_node)
                        && func.body.is_some()
                        && block_has_use_strict(func.body, self)
                    {
                        return true;
                    }
                }
                _ => {}
            }

            current = parent;
        }
    }

    /// Get type of template expression (template literal with substitutions).
    ///
    /// Type-checks all expressions within template spans to emit errors like TS2304.
    /// Template expressions always produce string type.
    ///
    /// Uses `solver::compute_template_expression_type` for type computation
    /// as part of the Solver-First architecture migration.
    pub(crate) fn get_type_of_template_expression(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::STRING;
        };

        let Some(template) = self.ctx.arena.get_template_expr(node) else {
            return TypeId::STRING;
        };

        // Type-check each template span's expression and collect types for solver
        let mut part_types = Vec::new();
        for &span_idx in &template.template_spans.nodes {
            let Some(span_node) = self.ctx.arena.get(span_idx) else {
                continue;
            };

            let Some(span) = self.ctx.arena.get_template_span(span_node) else {
                continue;
            };

            // Type-check the expression - this will emit TS2304 if name is unresolved
            let part_type = self.get_type_of_node(span.expression);
            part_types.push(part_type);
        }

        // Use Solver API for type computation (Solver-First architecture)
        // Template literals always produce string type, but we check for ERROR/NEVER propagation
        expression_ops::compute_template_expression_type(self.ctx.types, &part_types)
    }

    /// Get type of variable declaration.
    ///
    /// Computes the type of variable declarations like `let x: number = 5` or `const y = "hello"`.
    /// Returns the type annotation if present, otherwise infers from the initializer.
    pub(crate) fn get_type_of_variable_declaration(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) else {
            return TypeId::ERROR;
        };

        // First check type annotation - this takes precedence
        if var_decl.type_annotation.is_some() {
            return self.get_type_from_type_node(var_decl.type_annotation);
        }

        if self.is_catch_clause_variable_declaration(idx)
            && self.ctx.use_unknown_in_catch_variables()
        {
            return TypeId::UNKNOWN;
        }

        // Infer from initializer
        if var_decl.initializer.is_some() {
            let init_type = self.get_type_of_node(var_decl.initializer);

            // Rule #10: Literal Widening (with freshness)
            // For mutable bindings (let/var), widen literals to their primitive type
            // ONLY when the initializer is a "fresh" literal expression (direct literal
            // in source code). Types from variable references, narrowing, or computed
            // expressions are "non-fresh" and should NOT be widened.
            // For const bindings, preserve literal types (unless in array/object context)
            if !self.is_const_variable_declaration(idx) {
                let widened = if self.is_fresh_literal_expression(var_decl.initializer) {
                    self.widen_initializer_type_for_mutable_binding(init_type)
                } else {
                    init_type
                };
                // When strictNullChecks is off, undefined and null widen to any
                // (always, regardless of freshness)
                if !self.ctx.strict_null_checks()
                    && tsz_solver::type_queries::is_only_null_or_undefined(self.ctx.types, widened)
                {
                    return TypeId::ANY;
                }
                return widened;
            }

            // const: preserve literal type
            init_type
        } else {
            // No initializer - use UNKNOWN to enforce strict checking
            // This requires explicit type annotation or prevents unsafe usage
            TypeId::UNKNOWN
        }
    }

    /// Get the type of an assignment target without definite assignment checks.
    ///
    /// Computes the type of the left-hand side of an assignment expression.
    /// Handles identifier resolution and type-only alias checking.
    pub(crate) fn get_type_of_assignment_target(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_scanner::SyntaxKind;

        if let Some(node) = self.ctx.arena.get(idx)
            && node.kind == SyntaxKind::Identifier as u16
        {
            // Check for local variable first (including "arguments" shadowing).
            // This handles: `const arguments = ...; arguments = foo;`
            if let Some(sym_id) = self.resolve_identifier_symbol_for_write(idx) {
                if self.alias_resolves_to_type_only(sym_id) {
                    if let Some(ident) = self.ctx.arena.get_identifier(node) {
                        self.error_type_only_value_at(&ident.escaped_text, idx);
                    }
                    return TypeId::ERROR;
                }

                // Check if this is "arguments" in a function body with a local declaration
                if let Some(ident) = self.ctx.arena.get_identifier(node) {
                    if ident.escaped_text == "arguments" && self.is_in_regular_function_body(idx) {
                        // Check if the declaration is local to the current function
                        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                            && !symbol.declarations.is_empty()
                        {
                            let decl_node = symbol.declarations[0];
                            if let Some(current_fn) = self.find_enclosing_function(idx)
                                && let Some(decl_fn) = self.find_enclosing_function(decl_node)
                                && current_fn == decl_fn
                            {
                                // Local "arguments" declaration - use it
                                let declared_type = self.get_type_of_symbol(sym_id);
                                return declared_type;
                            }
                        }
                        // Symbol found but not local - fall through to IArguments check below
                    } else {
                        // Not "arguments" or not in function - use the symbol
                        let declared_type = self.get_type_of_symbol(sym_id);
                        return declared_type;
                    }
                } else {
                    // Use the resolved symbol
                    let declared_type = self.get_type_of_symbol(sym_id);
                    return declared_type;
                }
            }

            // Inside a regular function body, `arguments` is the implicit IArguments object,
            // overriding any outer `arguments` declaration (but not local ones, checked above).
            if let Some(ident) = self.ctx.arena.get_identifier(node)
                && ident.escaped_text == "arguments"
                && self.is_in_regular_function_body(idx)
            {
                let lib_binders = self.get_lib_binders();
                if let Some(sym_id) = self
                    .ctx
                    .binder
                    .get_global_type_with_libs("IArguments", &lib_binders)
                {
                    return self.type_reference_symbol_type(sym_id);
                }
                return TypeId::ANY;
            }
        }

        // Instantiation expressions on the left side (e.g. `fn<T> = ...`) are invalid (TS2364),
        // but the base expression is still a value read and must participate in
        // definite assignment checks (TS2454).
        if let Some(node) = self.ctx.arena.get(idx)
            && node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS
            && let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(node)
            && expr_type_args
                .type_arguments
                .as_ref()
                .is_some_and(|args| !args.nodes.is_empty())
        {
            let base_expr = expr_type_args.expression;
            let _ = self.get_type_of_node(base_expr);

            // In assignment-target context, flow nodes may attach to the outer
            // instantiation expression rather than the inner identifier. Force
            // definite-assignment checking for `id<T> = ...` to match tsc.
            if let Some(base_node) = self.ctx.arena.get(base_expr)
                && base_node.kind == SyntaxKind::Identifier as u16
                && let Some(sym_id) = self.resolve_identifier_symbol(base_expr)
            {
                let declared_type = self.get_type_of_symbol(sym_id);
                let _ = self.check_flow_usage(base_expr, declared_type, sym_id);
            }
        }

        // For non-identifier assignment targets (property access, element access, etc.),
        // we need the declared type without control-flow narrowing.
        // Example: After `if (foo[x] === undefined)`, when checking `foo[x] = 1`,
        // we should check against the declared type (e.g., `number | undefined` from index signature)
        // not the narrowed type (e.g., `undefined`).
        //
        // However, if the target is invalid (e.g. `getValue<number> = ...` parsed as BinaryExpression),
        // we should NOT skip narrowing because we want to treat it as an expression read
        // to catch errors like TS2454 (used before assigned).
        let prev_skip_narrowing = self.ctx.skip_flow_narrowing;
        if self.is_valid_assignment_target(idx) {
            self.ctx.skip_flow_narrowing = true;
        }
        let result = self.get_type_of_node(idx);
        self.ctx.skip_flow_narrowing = prev_skip_narrowing;
        result
    }

    /// Get the type of a class member.
    ///
    /// Computes the type for class property declarations, method declarations, and getters.
    pub(crate) fn get_type_of_class_member(&mut self, member_idx: NodeIndex) -> TypeId {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return TypeId::ANY;
        };

        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                    return TypeId::ANY;
                };

                // Get the type: either from annotation or inferred from initializer
                if prop.type_annotation.is_some() {
                    self.get_type_from_type_node(prop.type_annotation)
                } else if prop.initializer.is_some() {
                    self.get_type_of_node(prop.initializer)
                } else {
                    TypeId::ANY
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                    return TypeId::ANY;
                };
                let signature = self.call_signature_from_method(method, member_idx);
                use tsz_solver::FunctionShape;
                let factory = self.ctx.types.factory();
                factory.function(FunctionShape {
                    type_params: signature.type_params,
                    params: signature.params,
                    this_type: signature.this_type,
                    return_type: signature.return_type,
                    type_predicate: signature.type_predicate,
                    is_constructor: false,
                    is_method: true,
                })
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                    return TypeId::ANY;
                };

                if accessor.type_annotation.is_some() {
                    self.get_type_from_type_node(accessor.type_annotation)
                } else {
                    self.infer_getter_return_type(accessor.body)
                }
            }
            _ => TypeId::ANY,
        }
    }

    /// Get the simple type of an interface member (without wrapping in object type).
    ///
    /// For property signatures: returns the property type
    /// For method signatures: returns the function type
    pub(crate) fn get_type_of_interface_member_simple(&mut self, member_idx: NodeIndex) -> TypeId {
        use tsz_parser::parser::syntax_kind_ext::{METHOD_SIGNATURE, PROPERTY_SIGNATURE};
        use tsz_solver::FunctionShape;
        let factory = self.ctx.types.factory();

        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return TypeId::ANY;
        };

        if member_node.kind == METHOD_SIGNATURE {
            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                return TypeId::ANY;
            };

            let (type_params, type_param_updates) = self.push_type_parameters(&sig.type_parameters);
            let (params, this_type) = self.extract_params_from_signature(sig);
            let (return_type, type_predicate) =
                self.return_type_and_predicate(sig.type_annotation, &params);

            let shape = FunctionShape {
                type_params,
                params,
                this_type,
                return_type,
                type_predicate,
                is_constructor: false,
                is_method: true,
            };
            self.pop_type_parameters(type_param_updates);
            return factory.function(shape);
        }

        if member_node.kind == PROPERTY_SIGNATURE {
            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                return TypeId::ANY;
            };

            if sig.type_annotation.is_some() {
                return self.get_type_from_type_node(sig.type_annotation);
            }
            return TypeId::ANY;
        }

        TypeId::ANY
    }

    /// Get the type of an interface member.
    ///
    /// Returns an object type containing the member. For method signatures,
    /// creates a callable type. For property signatures, creates a property type.
    pub(crate) fn get_type_of_interface_member(&mut self, member_idx: NodeIndex) -> TypeId {
        use tsz_parser::parser::syntax_kind_ext::{METHOD_SIGNATURE, PROPERTY_SIGNATURE};
        use tsz_solver::{FunctionShape, PropertyInfo};
        let factory = self.ctx.types.factory();

        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return TypeId::ERROR;
        };

        if member_node.kind == METHOD_SIGNATURE || member_node.kind == PROPERTY_SIGNATURE {
            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                return TypeId::ERROR;
            };
            let name = self.get_property_name(sig.name);
            let Some(name) = name else {
                return TypeId::ERROR;
            };
            let name_atom = self.ctx.types.intern_string(&name);

            if member_node.kind == METHOD_SIGNATURE {
                let (type_params, type_param_updates) =
                    self.push_type_parameters(&sig.type_parameters);
                let (params, this_type) = self.extract_params_from_signature(sig);
                let (return_type, type_predicate) =
                    self.return_type_and_predicate(sig.type_annotation, &params);

                let shape = FunctionShape {
                    type_params,
                    params,
                    this_type,
                    return_type,
                    type_predicate,
                    is_constructor: false,
                    is_method: true,
                };
                self.pop_type_parameters(type_param_updates);
                let method_type = factory.function(shape);

                let prop = PropertyInfo {
                    name: name_atom,
                    type_id: method_type,
                    write_type: method_type,
                    optional: sig.question_token,
                    readonly: self.has_readonly_modifier(&sig.modifiers),
                    is_method: true,
                    visibility: Visibility::Public,
                    parent_id: None,
                };
                return factory.object(vec![prop]);
            }

            let type_id = if sig.type_annotation.is_some() {
                self.get_type_from_type_node(sig.type_annotation)
            } else {
                TypeId::ANY
            };
            let prop = PropertyInfo {
                name: name_atom,
                type_id,
                write_type: type_id,
                optional: sig.question_token,
                readonly: self.has_readonly_modifier(&sig.modifiers),
                is_method: false,
                visibility: Visibility::Public,
                parent_id: None,
            };
            return factory.object(vec![prop]);
        }

        TypeId::ANY
    }
}
