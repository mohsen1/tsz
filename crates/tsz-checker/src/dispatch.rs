//! Expression type computation dispatcher.

use crate::context::TypingRequest;
use crate::query_boundaries::checkers::generic as generic_query;
use crate::query_boundaries::dispatch as query;
use crate::query_boundaries::type_checking_utilities as query_utils;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// Dispatcher for expression type computation.
pub struct ExpressionDispatcher<'a, 'b> {
    pub checker: &'a mut CheckerState<'b>,
}

impl<'a, 'b> ExpressionDispatcher<'a, 'b> {
    pub const fn new(checker: &'a mut CheckerState<'b>) -> Self {
        Self { checker }
    }

    /// Resolve a literal type: preserve if const assertion or contextual typing expects it.
    fn resolve_literal(
        &mut self,
        request: &TypingRequest,
        literal_type: Option<TypeId>,
        widened: TypeId,
    ) -> TypeId {
        match literal_type {
            Some(lit)
                if self.checker.ctx.in_const_assertion
                    || self.checker.ctx.preserve_literal_types
                    || request.contextual_type.is_some_and(|ctx_type| {
                        self.checker.contextual_type_allows_literal(ctx_type, lit)
                    }) =>
            {
                lit
            }
            _ => widened,
        }
    }

    /// Dispatch type computation based on node kind.
    pub fn dispatch_type_computation(&mut self, idx: NodeIndex) -> TypeId {
        self.dispatch_type_computation_with_request(idx, &TypingRequest::NONE)
    }

    pub fn dispatch_type_computation_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        // Hard stack guard: bail when remaining stack is critically low.
        if crate::checkers_domain::stack_overflow_tripped()
            || stacker::remaining_stack().is_some_and(|r| r < 256 * 1024)
        {
            crate::checkers_domain::trip_stack_overflow();
            return TypeId::ERROR;
        }
        let Some(node) = self.checker.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };
        match node.kind {
            // Identifiers
            k if k == SyntaxKind::Identifier as u16 => self
                .checker
                .get_type_of_identifier_with_request(idx, request),
            k if k == SyntaxKind::RegularExpressionLiteral as u16 => self
                .checker
                .resolve_lib_type_by_name("RegExp")
                .unwrap_or(TypeId::ANY),
            k if k == SyntaxKind::ThisKeyword as u16 => {
                {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    // TS2465: 'this' cannot be referenced in a computed property name.
                    // Check this first — it takes priority over other `this` errors.
                    if self
                        .checker
                        .is_this_in_class_member_computed_property_name(idx)
                    {
                        self.checker.error_at_node(
                            idx,
                            diagnostic_messages::THIS_CANNOT_BE_REFERENCED_IN_A_COMPUTED_PROPERTY_NAME,
                            diagnostic_codes::THIS_CANNOT_BE_REFERENCED_IN_A_COMPUTED_PROPERTY_NAME,
                        );
                        return TypeId::ANY;
                    }
                    // TS2332: `this` inside enum member initializers is always invalid,
                    // even when the enum is nested in a namespace.
                    if self.checker.is_this_in_enum_member_initializer(idx) {
                        self.checker.error_at_node(
                            idx,
                            diagnostic_messages::THIS_CANNOT_BE_REFERENCED_IN_CURRENT_LOCATION,
                            diagnostic_codes::THIS_CANNOT_BE_REFERENCED_IN_CURRENT_LOCATION,
                        );
                        // tsc emits the companion TS2683 when `this` is directly in
                        // the enum initializer. When `this` is captured through an
                        // arrow function, TS2683 is NOT emitted (the arrow's `this`
                        // captures the outer context which is the enum — still invalid
                        // via TS2332, but not flagged for implicit-any).
                        if !self.checker.has_enclosing_arrow_before_enum(idx) {
                            self.checker.error_at_node(
                                idx,
                                diagnostic_messages::THIS_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION,
                                diagnostic_codes::THIS_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION,
                            );
                        }
                        return TypeId::ANY;
                    }
                    // TS2331: 'this' cannot be referenced in a module or namespace body
                    // In JS files, `namespace` is invalid syntax (TS8006) so tsc
                    // doesn't emit TS2331/TS2683 for `this` in namespace bodies.
                    if !self.checker.is_js_file() && self.checker.is_this_in_namespace_body(idx) {
                        self.checker.error_at_node(
                            idx,
                            diagnostic_messages::THIS_CANNOT_BE_REFERENCED_IN_A_MODULE_OR_NAMESPACE_BODY,
                            diagnostic_codes::THIS_CANNOT_BE_REFERENCED_IN_A_MODULE_OR_NAMESPACE_BODY,
                        );
                        // TSC always emits TS2683 as a companion to TS2331 in
                        // namespace bodies — `this` is inherently untyped here,
                        // regardless of noImplicitThis.
                        self.checker.error_at_node(
                            idx,
                            diagnostic_messages::THIS_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION,
                            diagnostic_codes::THIS_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION,
                        );
                        return TypeId::ANY;
                    }
                    // TS17009: 'super' must be called before accessing 'this'
                    if self
                        .checker
                        .is_this_before_super_in_derived_constructor(idx)
                    {
                        self.checker.error_at_node(
                            idx,
                            diagnostic_messages::SUPER_MUST_BE_CALLED_BEFORE_ACCESSING_THIS_IN_THE_CONSTRUCTOR_OF_A_DERIVED_CLASS,
                            diagnostic_codes::SUPER_MUST_BE_CALLED_BEFORE_ACCESSING_THIS_IN_THE_CONSTRUCTOR_OF_A_DERIVED_CLASS,
                        );
                    }
                    // TS2816: Cannot use 'this' in a static property initializer of a decorated class
                    if self.checker.ctx.compiler_options.experimental_decorators
                        && let Some(ref class_info) = self.checker.ctx.enclosing_class
                        && class_info.in_static_property_initializer
                        && !self.checker.is_this_in_nested_function_inside_class(idx)
                        && let Some(class_node) = self.checker.ctx.arena.get(class_info.class_idx)
                        && let Some(class_data) = self.checker.ctx.arena.get_class(class_node)
                        && let Some(ref modifiers) = class_data.modifiers
                    {
                        let has_class_decorator = modifiers.nodes.iter().any(|&mod_idx| {
                            self.checker.ctx.arena.get(mod_idx).is_some_and(|n| {
                                n.kind == tsz_parser::parser::syntax_kind_ext::DECORATOR
                            })
                        });
                        if has_class_decorator {
                            self.checker.error_at_node(
                                            idx,
                                            diagnostic_messages::CANNOT_USE_THIS_IN_A_STATIC_PROPERTY_INITIALIZER_OF_A_DECORATED_CLASS,
                                            diagnostic_codes::CANNOT_USE_THIS_IN_A_STATIC_PROPERTY_INITIALIZER_OF_A_DECORATED_CLASS,
                                        );
                        }
                    }
                }
                if let Some(this_type) = self.checker.current_this_type() {
                    // If `this` is inside a nested regular function, the this_type_stack
                    // from the enclosing class member doesn't apply — the function creates
                    // its own `this` binding.
                    if !self.checker.is_this_in_nested_function_inside_class(idx) {
                        return self.checker.apply_flow_narrowing(idx, this_type);
                    }
                    // Fall through — the nested function has its own `this`
                }
                if let Some(ref class_info) = self.checker.ctx.enclosing_class {
                    // Inside a class but no explicit this type on stack -
                    // return the class instance/constructor type depending on static context.
                    // BUT: if `this` is inside a nested regular function (not a class member),
                    // that function creates its own `this` binding, so don't use the class type.
                    let has_intermediate_function =
                        self.checker.is_this_in_nested_function_inside_class(idx);
                    // Walk the AST to determine static context — can't rely on
                    // in_static_member flag since it's only set during check_class_member.
                    let is_in_static = self.checker.is_this_in_static_class_member(idx);
                    if !has_intermediate_function {
                        if let Some(class_node) = self.checker.ctx.arena.get(class_info.class_idx)
                            && let Some(class_data) = self.checker.ctx.arena.get_class(class_node)
                        {
                            let this_type = if is_in_static {
                                self.checker
                                    .get_class_constructor_type(class_info.class_idx, class_data)
                            } else {
                                self.checker
                                    .get_class_instance_type(class_info.class_idx, class_data)
                            };
                            return self.checker.apply_flow_narrowing(idx, this_type);
                        }
                        TypeId::ANY
                    } else {
                        // Fall through to TS2683 / TS7041 checks below
                        // Suppress if the nested function has an explicit `this` parameter
                        // or a contextual `this` type from a parent type annotation
                        if self.checker.ctx.no_implicit_this()
                            && !self
                                .checker
                                .enclosing_function_has_explicit_this_parameter(idx)
                            && !self
                                .checker
                                .enclosing_function_has_contextual_this_type(idx)
                        {
                            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                            self.checker.error_at_node(
                                idx,
                                diagnostic_messages::THIS_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION,
                                diagnostic_codes::THIS_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION,
                            );
                        }
                        TypeId::ANY
                    }
                } else if self.checker.this_has_contextual_owner(idx).is_some() {
                    // `this` in a class or object literal member but enclosing_class
                    // not yet set. Suppress TS2683 - `this` is contextually typed.
                    TypeId::ANY
                } else if self.checker.ctx.no_implicit_this()
                    && !self.checker.is_js_file()
                    && self
                        .checker
                        .find_enclosing_non_arrow_function(idx)
                        .is_some()
                {
                    // TS2683: 'this' implicitly has type 'any'
                    // Suppressed in JS files: tsc infers `this` for constructor/prototype
                    // patterns and JSDoc-typed functions.
                    // Suppress if the enclosing function has an explicit `this` parameter
                    // or a contextual `this` type from a parent type annotation
                    if self
                        .checker
                        .enclosing_function_has_explicit_this_parameter(idx)
                        || self
                            .checker
                            .enclosing_function_has_contextual_this_type(idx)
                    {
                        TypeId::ANY
                    } else {
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                        self.checker.error_at_node(
                            idx,
                            diagnostic_messages::THIS_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION,
                            diagnostic_codes::THIS_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION,
                        );
                        TypeId::ANY
                    }
                } else if self.checker.ctx.no_implicit_this()
                    && !self.checker.is_js_file()
                    && self.checker.is_this_in_global_capturing_arrow(idx)
                {
                    // TS7041: 'this' in a top-level arrow function captures globalThis.
                    // Fires when noImplicitThis is on, `this` is inside an arrow function,
                    // and there is no enclosing class/object/function providing local `this`.
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.checker.error_at_node(
                        idx,
                        diagnostic_messages::THE_CONTAINING_ARROW_FUNCTION_CAPTURES_THE_GLOBAL_VALUE_OF_THIS,
                        diagnostic_codes::THE_CONTAINING_ARROW_FUNCTION_CAPTURES_THE_GLOBAL_VALUE_OF_THIS,
                    );
                    TypeId::ANY
                } else if self.checker.ctx.no_implicit_this()
                    && !self.checker.is_js_file()
                    && self
                        .checker
                        .find_enclosing_non_arrow_function(idx)
                        .is_none()
                {
                    // `this` at the top level of a script/module with noImplicitThis.
                    // tsc resolves this to `typeof globalThis` (an object type), not `any`.
                    // We approximate with TypeId::OBJECT since we don't have a full
                    // globalThis type yet. This ensures that operations like `++this`
                    // correctly emit TS2356 (arithmetic type error) instead of TS2357
                    // (invalid lvalue) — matching tsc behavior where the type check
                    // fires first and suppresses the lvalue check.
                    TypeId::OBJECT
                } else {
                    TypeId::ANY
                }
            }
            k if k == SyntaxKind::SuperKeyword as u16 => {
                self.checker.get_type_of_super_keyword(idx)
            }
            // Literals — preserve literal types when contextual typing expects them.
            k if k == SyntaxKind::NumericLiteral as u16 => self.resolve_literal(
                request,
                self.checker.literal_type_from_initializer(idx),
                TypeId::NUMBER,
            ),
            k if k == SyntaxKind::BigIntLiteral as u16 => {
                // TS2737: bigint literals require target >= ES2020 in non-ambient contexts.
                if (self.checker.ctx.compiler_options.target as u32)
                    < (tsz_common::common::ScriptTarget::ES2020 as u32)
                    && !self.checker.is_ambient_declaration(idx)
                {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.checker.error_at_node(
                        idx,
                        diagnostic_messages::BIGINT_LITERALS_ARE_NOT_AVAILABLE_WHEN_TARGETING_LOWER_THAN_ES2020,
                        diagnostic_codes::BIGINT_LITERALS_ARE_NOT_AVAILABLE_WHEN_TARGETING_LOWER_THAN_ES2020,
                    );
                }
                self.resolve_literal(
                    request,
                    self.checker.literal_type_from_initializer(idx),
                    TypeId::BIGINT,
                )
            }
            k if k == SyntaxKind::StringLiteral as u16 => self.resolve_literal(
                request,
                self.checker.literal_type_from_initializer(idx),
                TypeId::STRING,
            ),
            k if k == SyntaxKind::TrueKeyword as u16 => {
                let literal_type = self.checker.ctx.types.literal_boolean(true);
                self.resolve_literal(request, Some(literal_type), TypeId::BOOLEAN)
            }
            k if k == SyntaxKind::FalseKeyword as u16 => {
                let literal_type = self.checker.ctx.types.literal_boolean(false);
                self.resolve_literal(request, Some(literal_type), TypeId::BOOLEAN)
            }
            k if k == SyntaxKind::NullKeyword as u16 => TypeId::NULL,
            // Binary expressions
            k if k == syntax_kind_ext::BINARY_EXPRESSION => self
                .checker
                .get_type_of_binary_expression_with_request(idx, request),
            // Call expressions
            k if k == syntax_kind_ext::CALL_EXPRESSION => self
                .checker
                .get_type_of_call_expression_with_request(idx, request),
            // Tagged template expressions (e.g., `tag\`hello ${x}\``)
            k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => self
                .checker
                .get_type_of_tagged_template_expression_with_request(idx, request),
            // New expressions
            k if k == syntax_kind_ext::NEW_EXPRESSION => self
                .checker
                .get_type_of_new_expression_with_request(idx, request),
            // Class expressions
            k if k == syntax_kind_ext::CLASS_EXPRESSION => {
                if let Some(class) = self.checker.ctx.arena.get_class(node).cloned() {
                    self.checker
                        .check_class_expression_with_request(idx, &class, request);

                    // When a class extends a type parameter and adds no new instance members,
                    // type it as the type parameter to maintain generic compatibility
                    if let Some(base_type_param) = self
                        .checker
                        .get_extends_type_parameter_if_transparent(&class)
                    {
                        base_type_param
                    } else {
                        self.checker
                            .get_class_constructor_type_with_request(idx, &class, request)
                    }
                } else {
                    // Return ANY to prevent cascading TS2571 errors
                    TypeId::ANY
                }
            }
            // Property access
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => self
                .checker
                .get_type_of_property_access_with_request(idx, request),
            // Element access
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => self
                .checker
                .get_type_of_element_access_with_request(idx, request),
            // Conditional expression (ternary)
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => self
                .checker
                .get_type_of_conditional_expression_with_request(idx, request),
            // Variable declaration
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                self.checker.get_type_of_variable_declaration(idx)
            }
            // Function declaration
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.checker.get_type_of_function(idx)
            }
            // Method / constructor declarations can still reach expression typing
            // from JS/JSDoc helper paths that query declaration nodes directly.
            k if k == syntax_kind_ext::METHOD_DECLARATION || k == syntax_kind_ext::CONSTRUCTOR => {
                self.checker.get_type_of_function_with_request(idx, request)
            }
            // Function expression
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
                if self.checker.is_js_file() {
                    self.checker.check_js_grammar_function(idx, node);
                }
                self.checker.get_type_of_function_with_request(idx, request)
            }
            // Arrow function
            k if k == syntax_kind_ext::ARROW_FUNCTION => {
                if self.checker.is_js_file() {
                    self.checker.check_js_grammar_function(idx, node);
                }
                self.checker.get_type_of_function_with_request(idx, request)
            }
            // Array literal
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => self
                .checker
                .get_type_of_array_literal_with_request(idx, request),
            // Object literal
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => self
                .checker
                .get_type_of_object_literal_with_request(idx, request),
            // Prefix unary expression
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => self
                .checker
                .get_type_of_prefix_unary_with_request(idx, request),
            // Postfix unary expression - ++ and -- require numeric operand and valid l-value
            k if k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.checker.ctx.arena.get_unary_expr(node) {
                    self.checker
                        .check_strict_mode_eval_or_arguments_assignment(unary.operand);
                    if self.checker.check_function_assignment(unary.operand) {
                        return TypeId::NUMBER;
                    }

                    // TSC checks arithmetic type BEFORE lvalue — if the type check
                    // fails (TS2356), the lvalue check (TS2357) is skipped.
                    let operand_raw = self.checker.get_type_of_node(unary.operand);
                    let operand_type = self.checker.resolve_type_query_type(operand_raw);
                    // TS18046: postfix ++/-- on unknown is not allowed (strictNullChecks only).
                    // tsc emits TS18046 instead of TS2356 for unknown operands.
                    if operand_type == TypeId::UNKNOWN
                        && self.checker.error_is_of_type_unknown(unary.operand)
                    {
                        return TypeId::NUMBER;
                    }

                    // Determine result type: bigint for bigint operands, number otherwise.
                    let result_type = {
                        let evaluator = tsz_solver::BinaryOpEvaluator::new(self.checker.ctx.types);
                        let resolved = self.checker.evaluate_type_with_env(operand_type);
                        if evaluator.is_bigint_like(resolved) {
                            TypeId::BIGINT
                        } else {
                            TypeId::NUMBER
                        }
                    };
                    let mut arithmetic_ok = true;
                    {
                        use tsz_solver::BinaryOpEvaluator;
                        let evaluator = BinaryOpEvaluator::new(self.checker.ctx.types);
                        let (non_nullish, nullish_cause) =
                            self.checker.split_nullish_type(operand_type);
                        let nullish_can_flow_to_number = non_nullish.is_none_or(|ty| {
                            let evaluated = self.checker.evaluate_type_with_env(ty);
                            evaluator.is_arithmetic_operand(evaluated)
                                || (self.checker.is_enum_like_type(ty)
                                    && self.checker.is_unresolved_lazy_type(evaluated))
                        });
                        if self.checker.ctx.strict_null_checks()
                            && let Some(cause) = nullish_cause
                            && nullish_can_flow_to_number
                        {
                            arithmetic_ok = false;
                            self.checker
                                .emit_nullish_operand_error(unary.operand, cause);
                        }

                        // Evaluate the type to resolve Lazy(DefId) aliases before checking.
                        // Type aliases like `YesNo = Choice.Yes | Choice.No` may stay as
                        // Lazy(DefId) which the visitor can't recurse into.
                        let resolved_type = self.checker.evaluate_type_with_env(operand_type);
                        // Check if the type is a valid arithmetic operand.
                        // Also check is_enum_like_type on both the original and resolved
                        // types: the original may be a Lazy(DefId) for an enum, and the
                        // resolved may be a union of Lazy enum member refs that
                        // is_arithmetic_operand can't handle (solver can't resolve Lazy).
                        let is_valid = evaluator.is_arithmetic_operand(resolved_type)
                            || self.checker.is_enum_like_type(operand_type)
                            || self.checker.is_enum_like_type(resolved_type);
                        if arithmetic_ok && !is_valid {
                            arithmetic_ok = false;
                            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                            self.checker.error_at_node(
                                unary.operand,
                                diagnostic_messages::AN_ARITHMETIC_OPERAND_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT_OR_AN_ENUM_TYPE,
                                diagnostic_codes::AN_ARITHMETIC_OPERAND_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT_OR_AN_ENUM_TYPE,
                            );
                        }
                    }
                    // Only check lvalue and assignment restrictions when arithmetic
                    // type is valid (matches TSC: TS2357 is skipped when TS2356 fires).
                    if arithmetic_ok {
                        let emitted_lvalue = self
                            .checker
                            .check_increment_decrement_operand(unary.operand);
                        if !emitted_lvalue {
                            // TS2588: Cannot assign to 'x' because it is a constant.
                            let is_const = self.checker.check_const_assignment(unary.operand);
                            // TS2630: Cannot assign to 'x' because it is a function.
                            self.checker.check_function_assignment(unary.operand);
                            // TS2540: Cannot assign to readonly property
                            if !is_const {
                                self.checker.check_readonly_assignment(unary.operand, idx);
                            }
                        }
                    }
                    return result_type;
                }
                TypeId::NUMBER
            }
            // typeof expression
            k if k == syntax_kind_ext::TYPE_OF_EXPRESSION => TypeId::STRING,
            // void expression
            k if k == syntax_kind_ext::VOID_EXPRESSION => TypeId::UNDEFINED,
            // await expression - unwrap Promise<T> to get T, with contextual typing (Phase 6 - tsz-3)
            k if k == syntax_kind_ext::AWAIT_EXPRESSION => self
                .checker
                .get_type_of_await_expression_with_request(idx, request),
            // yield expression
            k if k == syntax_kind_ext::YIELD_EXPRESSION => {
                self.get_type_of_yield_expression(idx, request)
            }
            // Parenthesized expression - just pass through to inner expression
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.checker.ctx.arena.get_parenthesized(node) {
                    // Check if expression is missing (parse error: empty parentheses)
                    if paren.expression.is_none() {
                        // Parse error - return ERROR to suppress cascading errors
                        return TypeId::ERROR;
                    }
                    // In JS/checkJs, inline JSDoc casts like `/** @type {T} */(expr)`
                    // should behave as type assertions and produce the annotated type.
                    if let Some(jsdoc_type) =
                        self.checker.jsdoc_type_annotation_for_node_direct(idx)
                    {
                        // Set contextual type before evaluating the inner expression,
                        // mirroring `as` expression behavior. This allows arrow
                        // functions and object literals inside JSDoc @type casts
                        // to receive contextual typing (prevents false TS7006).
                        let needs_context = self.checker.argument_needs_contextual_type(
                            self.checker
                                .ctx
                                .arena
                                .skip_parenthesized_and_assertions(paren.expression),
                        );
                        let request = if needs_context {
                            request
                                .read()
                                .normal_origin()
                                .contextual(jsdoc_type)
                                .assertion()
                        } else {
                            request.read().normal_origin().contextual_opt(None)
                        };
                        let expr_type = self
                            .checker
                            .get_type_of_node_with_request(paren.expression, &request);
                        // TS2352: Check if conversion may be a mistake (same as `as` expressions)
                        self.checker.ensure_relation_input_ready(expr_type);
                        self.checker.ensure_relation_input_ready(jsdoc_type);
                        let should_check = !self.checker.type_contains_error(expr_type)
                            && !self.checker.type_contains_error(jsdoc_type)
                            && expr_type != TypeId::ANY
                            && jsdoc_type != TypeId::ANY
                            && expr_type != TypeId::UNKNOWN
                            && jsdoc_type != TypeId::UNKNOWN
                            && expr_type != TypeId::NEVER
                            && jsdoc_type != TypeId::NEVER
                            && !generic_query::contains_type_parameters(
                                self.checker.ctx.types,
                                expr_type,
                            )
                            && !generic_query::contains_type_parameters(
                                self.checker.ctx.types,
                                jsdoc_type,
                            );
                        if should_check {
                            let fwd = self.checker.is_assignable_to(expr_type, jsdoc_type);
                            let rev = self.checker.is_assignable_to(jsdoc_type, expr_type);
                            if !fwd && !rev {
                                // Check union member overlap (same as `as` expressions):
                                // if expr is a union, check if any member overlaps with target.
                                let mut have_overlap = false;
                                if let Some(members) =
                                    query::union_members(self.checker.ctx.types, expr_type)
                                {
                                    for member in members {
                                        if self.checker.is_assignable_to(member, jsdoc_type)
                                            || self.checker.is_assignable_to(jsdoc_type, member)
                                        {
                                            have_overlap = true;
                                            break;
                                        }
                                    }
                                }
                                // Fallback: structural property overlap check
                                if !have_overlap {
                                    let evaluated_expr =
                                        self.checker.evaluate_type_for_assignability(expr_type);
                                    let evaluated_jsdoc =
                                        self.checker.evaluate_type_for_assignability(jsdoc_type);
                                    have_overlap = query::types_are_comparable(
                                        self.checker.ctx.types,
                                        evaluated_expr,
                                        evaluated_jsdoc,
                                    );
                                }
                                if !have_overlap {
                                    self.checker.error_type_assertion_no_overlap(
                                        expr_type, jsdoc_type, idx,
                                    );
                                }
                            }
                        }
                        jsdoc_type
                    } else if let Some((satisfies_type, keyword_pos)) =
                        self.checker.jsdoc_satisfies_annotation_with_pos(idx)
                    {
                        // Set contextual type for JSDoc @satisfies, matching the
                        // `satisfies` expression handler behavior.
                        let satisfies_request =
                            request.read().normal_origin().contextual(satisfies_type);
                        let expr_type = self
                            .checker
                            .get_type_of_node_with_request(paren.expression, &satisfies_request);
                        // Ensure types are fully resolved (evaluate applications like
                        // Record<K,V>, Partial<T>, etc.) before assignability checks.
                        self.checker.ensure_relation_input_ready(expr_type);
                        self.checker.ensure_relation_input_ready(satisfies_type);
                        if !self.checker.type_contains_error(satisfies_type) {
                            let _ = self.checker.check_satisfies_assignable_or_report(
                                expr_type,
                                satisfies_type,
                                paren.expression,
                                Some(keyword_pos),
                            );
                        }
                        expr_type
                    } else {
                        self.checker
                            .get_type_of_node_with_request(paren.expression, request)
                    }
                } else {
                    // Missing parenthesized data - propagate error
                    TypeId::ERROR
                }
            }
            // Type assertions / `as` / `satisfies`
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION =>
            {
                if let Some(assertion) = self.checker.ctx.arena.get_type_assertion(node) {
                    // Check for const assertion BEFORE type-checking the expression
                    // so we can set the context flag to preserve literal types
                    let is_const_assertion =
                        if let Some(type_node) = self.checker.ctx.arena.get(assertion.type_node) {
                            type_node.kind == tsz_scanner::SyntaxKind::ConstKeyword as u16
                                || (type_node.kind == syntax_kind_ext::TYPE_REFERENCE
                                    && self.checker.ctx.arena.get_type_ref(type_node).is_some_and(
                                        |type_ref| {
                                            type_ref.type_arguments.is_none()
                                                && self
                                                    .checker
                                                    .ctx
                                                    .arena
                                                    .get_identifier_text(type_ref.type_name)
                                                    .is_some_and(|name| name == "const")
                                        },
                                    ))
                        } else {
                            false
                        };
                    // Set the in_const_assertion flag to preserve literal types in nested expressions
                    let prev_in_const_assertion = self.checker.ctx.in_const_assertion;
                    if is_const_assertion {
                        self.checker.ctx.in_const_assertion = true;
                    }

                    // In recovery scenarios we may not have a type node; fall back to the expression type.
                    if assertion.type_node.is_none() {
                        let expr_type = self
                            .checker
                            .get_type_of_node_with_request(assertion.expression, request);
                        self.checker.ctx.in_const_assertion = prev_in_const_assertion;
                        expr_type
                    } else if is_const_assertion {
                        // TS1355: Check that the expression is a valid const assertion target.
                        self.check_const_assertion_expression(assertion.expression);
                        let expr_type = self.checker.get_type_of_node_with_request(
                            assertion.expression,
                            &request.read().normal_origin().contextual_opt(None),
                        );
                        self.checker.ctx.in_const_assertion = prev_in_const_assertion;
                        use tsz_solver::widening::apply_const_assertion;
                        apply_const_assertion(self.checker.ctx.types, expr_type)
                    } else {
                        // Check for duplicate properties in type literal nodes (TS2300)
                        self.checker
                            .check_type_for_parameter_properties(assertion.type_node);
                        let asserted_type =
                            self.checker.get_type_from_type_node(assertion.type_node);
                        // Set contextual type before checking the operand only when the
                        // operand actually benefits from contextual typing (lambdas,
                        // object literals, arrays, etc.). Applying the asserted type
                        // to arbitrary expressions like `target ?? component` can
                        // manufacture spurious TS2322s inside an `as` assertion.
                        let needs_context = !is_const_assertion
                            && self.checker.argument_needs_contextual_type(
                                self.checker
                                    .ctx
                                    .arena
                                    .skip_parenthesized_and_assertions(assertion.expression),
                            );
                        let request = if needs_context {
                            // `satisfies` uses normal contextual typing (not assertion),
                            // while `as`/angle-bracket assertions mark assertion origin
                            // so function body return types are NOT checked against it.
                            if k == syntax_kind_ext::SATISFIES_EXPRESSION {
                                request.read().normal_origin().contextual(asserted_type)
                            } else {
                                request
                                    .read()
                                    .normal_origin()
                                    .contextual(asserted_type)
                                    .assertion()
                            }
                        } else {
                            request.read().normal_origin().contextual_opt(None)
                        };
                        // Always type-check the expression for side effects / diagnostics.
                        let expr_type = self
                            .checker
                            .get_type_of_node_with_request(assertion.expression, &request);
                        self.checker.ctx.in_const_assertion = prev_in_const_assertion;
                        if k == syntax_kind_ext::SATISFIES_EXPRESSION {
                            // TS8037: Type satisfaction expressions can only be used in TypeScript files
                            if self.checker.is_js_file() {
                                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                                self.checker.error_at_position(
                                    assertion.keyword_pos,
                                    9, // "satisfies".len()
                                    diagnostic_messages::TYPE_SATISFACTION_EXPRESSIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                                    diagnostic_codes::TYPE_SATISFACTION_EXPRESSIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                                );
                                return expr_type;
                            }
                            // `satisfies` keeps the expression type at runtime, but checks assignability.
                            // This is different from `as` which coerces the type.
                            self.checker.ensure_relation_input_ready(expr_type);
                            self.checker.ensure_relation_input_ready(asserted_type);
                            if !self.checker.type_contains_error(asserted_type) {
                                let _ = self.checker.check_satisfies_assignable_or_report(
                                    expr_type,
                                    asserted_type,
                                    assertion.expression,
                                    Some(assertion.keyword_pos),
                                );
                            }
                            expr_type
                        } else {
                            // `expr as T` / `<T>expr` yields `T`.
                            // TS2352: Check if conversion may be a mistake (types don't sufficiently overlap)
                            self.checker.ensure_relation_input_ready(expr_type);
                            self.checker.ensure_relation_input_ready(asserted_type);

                            // Don't check if either type is error, any, unknown, or never.
                            let should_check = !self.checker.type_contains_error(expr_type)
                                && !self.checker.type_contains_error(asserted_type)
                                && expr_type != TypeId::ANY
                                && asserted_type != TypeId::ANY
                                && expr_type != TypeId::UNKNOWN
                                && asserted_type != TypeId::UNKNOWN
                                && expr_type != TypeId::NEVER
                                && asserted_type != TypeId::NEVER
                                // Skip TS2352 when expr_type contains type parameters.
                                && !generic_query::contains_type_parameters(
                                    self.checker.ctx.types,
                                    expr_type,
                                )
                                // Suppress TS2352 when the assertion node is near a
                                // parse error — the type assertion may be a parser
                                // recovery artifact (e.g. `@g<number> class C {}`
                                // parsed as `<number>class C {}` when decorators are
                                // disabled).
                                && !self.checker.node_has_nearby_parse_error(idx)
                                // Suppress TS2352 when the type assertion is the
                                // left-hand side of `**`. The checker will emit
                                // TS17007 for this grammar error; emitting TS2352
                                // as well is a cascading false positive.
                                && !self.checker.is_lhs_of_exponentiation(idx);

                            // For asserted types containing type parameters, resolve
                            // the constraint and check overlap against it. E.g., for
                            // `x as T` where `T extends object | null`, TSC checks
                            // overlap of `x` with `object | null`.
                            // For unconstrained type parameters (no `extends`), skip —
                            // T could be anything.
                            let (should_check, effective_asserted) = if should_check {
                                if tsz_solver::is_this_type(self.checker.ctx.types, asserted_type) {
                                    // `this` type — substitute with class instance type
                                    // for the overlap check. `ThisType` is parameter-like
                                    // but `get_type_parameter_constraint` doesn't handle it,
                                    // so we resolve it here to the enclosing class type.
                                    // Skip in static context — `this` is invalid there
                                    // (TS2526 handles that) so no overlap check needed.
                                    let is_static_this_context =
                                        self.checker.find_enclosing_static_block(idx).is_some()
                                            || self.checker.is_this_in_static_class_member(idx);
                                    if let Some(class_info) = &self.checker.ctx.enclosing_class
                                        && !is_static_this_context
                                    {
                                        let class_idx = class_info.class_idx;
                                        if let Some(node) = self.checker.ctx.arena.get(class_idx)
                                            && let Some(class_data) =
                                                self.checker.ctx.arena.get_class(node)
                                        {
                                            let instance_type = self
                                                .checker
                                                .get_class_instance_type(class_idx, class_data);
                                            (true, instance_type)
                                        } else {
                                            (false, asserted_type)
                                        }
                                    } else {
                                        (false, asserted_type)
                                    }
                                } else if generic_query::contains_type_parameters(
                                    self.checker.ctx.types,
                                    asserted_type,
                                ) {
                                    // Only bare unconstrained type parameters suppress TS2352.
                                    // Structured targets like `T[]` or `(x: T) => T` still have
                                    // enough shape for overlap checking, and tsc reports TS2352
                                    // for assertions like `null as T[]`.
                                    if tsz_solver::type_queries::is_type_parameter_like(
                                        self.checker.ctx.types,
                                        asserted_type,
                                    ) {
                                        // Try resolving the type parameter's constraint.
                                        if let Some(constraint) =
                                            tsz_solver::type_queries::get_type_parameter_constraint(
                                                self.checker.ctx.types,
                                                asserted_type,
                                            )
                                        {
                                            // Only check if constraint is concrete (not itself generic)
                                            // and not too broad (unknown/any).
                                            if !generic_query::contains_type_parameters(
                                                self.checker.ctx.types,
                                                constraint,
                                            ) && constraint != TypeId::UNKNOWN
                                                && constraint != TypeId::ANY
                                            {
                                                // Use the ORIGINAL asserted type (not constraint)
                                                // for overlap checking. tsc's isTypeComparableTo
                                                // checks against the type parameter itself, not its
                                                // constraint. Using the constraint is too permissive
                                                // and prevents TS2352 from firing when the expression
                                                // type satisfies the constraint but not the type param.
                                                (true, asserted_type)
                                            } else {
                                                (false, asserted_type)
                                            }
                                        } else {
                                            // No constraint — unconstrained naked `T` is compatible
                                            // with anything for TS2352 purposes.
                                            (false, asserted_type)
                                        }
                                    } else {
                                        (true, asserted_type)
                                    }
                                } else {
                                    (true, asserted_type)
                                }
                            } else {
                                (false, asserted_type)
                            };
                            if should_check {
                                // TS2352 is emitted if neither type is assignable to the other
                                // (i.e., the types don't "sufficiently overlap").
                                // TSC uses isTypeComparableTo which is more relaxed than
                                // assignability: types are comparable if they share at least
                                // one common property.
                                // Use effective_asserted (which may be a resolved constraint)
                                // for the overlap check.
                                //
                                // When the asserted type contains unresolved type
                                // parameters in a structured way (mapped types like
                                // `Boxified<T>`, indexed access like `keyof T`,
                                // callables like `new () => T`, etc.), our overlap
                                // check can't meaningfully evaluate — the shape depends
                                // on the unknown type parameter. tsc's `isTypeComparableTo`
                                // handles these permissively; treat as overlapping to
                                // suppress false positive TS2352.
                                let structured_generic_assertion_target =
                                    generic_query::contains_type_parameters(
                                        self.checker.ctx.types,
                                        effective_asserted,
                                    ) && !tsz_solver::type_queries::is_type_parameter_like(
                                        self.checker.ctx.types,
                                        effective_asserted,
                                    );
                                let array_like_generic_assertion_target = matches!(
                                    query_utils::classify_array_like(
                                        self.checker.ctx.types,
                                        effective_asserted,
                                    ),
                                    query_utils::ArrayLikeKind::Array(_)
                                        | query_utils::ArrayLikeKind::Tuple
                                        | query_utils::ArrayLikeKind::Readonly(_)
                                );
                                let source_to_target = if structured_generic_assertion_target {
                                    if array_like_generic_assertion_target {
                                        self.checker.is_assignable_to(expr_type, effective_asserted)
                                    } else {
                                        true // can't evaluate — assume overlap
                                    }
                                } else {
                                    self.checker.is_assignable_to(expr_type, effective_asserted)
                                };
                                let target_to_source = if structured_generic_assertion_target {
                                    if array_like_generic_assertion_target {
                                        self.checker.is_assignable_to(effective_asserted, expr_type)
                                    } else {
                                        true // can't evaluate — assume overlap
                                    }
                                } else {
                                    self.checker.is_assignable_to(effective_asserted, expr_type)
                                };
                                if !source_to_target && !target_to_source {
                                    // TSC uses isTypeComparableTo which decomposes unions
                                    // and checks per-member overlap. For `X as A | B`, it
                                    // suffices if X overlaps with ANY member (A or B).
                                    let mut have_overlap = false;
                                    if structured_generic_assertion_target
                                        && tsz_solver::is_mapped_type(
                                            self.checker.ctx.types,
                                            effective_asserted,
                                        )
                                    {
                                        let source_is_array = matches!(
                                            query_utils::classify_array_like(
                                                self.checker.ctx.types,
                                                expr_type,
                                            ),
                                            query_utils::ArrayLikeKind::Array(_)
                                                | query_utils::ArrayLikeKind::Tuple
                                                | query_utils::ArrayLikeKind::Readonly(_)
                                        );
                                        if source_is_array {
                                            have_overlap = true;
                                        }
                                    }

                                    // When both source and target are arrays and the
                                    // target element type contains type parameters
                                    // (e.g. `string[] as (keyof T)[]`), the element-
                                    // level overlap cannot be meaningfully evaluated.
                                    // TSC's isTypeComparableTo checks element-type
                                    // comparability which succeeds when the generic
                                    // element could include the source element type.
                                    // Assume overlap to suppress false TS2352.
                                    if array_like_generic_assertion_target {
                                        let source_is_array = matches!(
                                            query_utils::classify_array_like(
                                                self.checker.ctx.types,
                                                expr_type,
                                            ),
                                            query_utils::ArrayLikeKind::Array(_)
                                                | query_utils::ArrayLikeKind::Tuple
                                                | query_utils::ArrayLikeKind::Readonly(_)
                                        );
                                        let target_is_generic_array_like =
                                            match query_utils::classify_array_like(
                                                self.checker.ctx.types,
                                                effective_asserted,
                                            ) {
                                                query_utils::ArrayLikeKind::Array(target_elem)
                                                | query_utils::ArrayLikeKind::Readonly(
                                                    target_elem,
                                                ) => generic_query::contains_type_parameters(
                                                    self.checker.ctx.types,
                                                    target_elem,
                                                ),
                                                query_utils::ArrayLikeKind::Tuple => true,
                                                query_utils::ArrayLikeKind::Union(_)
                                                | query_utils::ArrayLikeKind::Intersection(_)
                                                | query_utils::ArrayLikeKind::Other => false,
                                            };
                                        if source_is_array && target_is_generic_array_like {
                                            have_overlap = true;
                                        }
                                    }

                                    // Decompose target union: any member assignable in either direction?
                                    if let Some(members) = query::union_members(
                                        self.checker.ctx.types,
                                        effective_asserted,
                                    ) {
                                        for member in members {
                                            if self.checker.is_assignable_to(member, expr_type)
                                                || self.checker.is_assignable_to(expr_type, member)
                                            {
                                                have_overlap = true;
                                                break;
                                            }
                                        }
                                    }

                                    // Decompose source union: any member assignable in either direction?
                                    if !have_overlap
                                        && let Some(members) =
                                            query::union_members(self.checker.ctx.types, expr_type)
                                    {
                                        for member in members {
                                            if self
                                                .checker
                                                .is_assignable_to(member, effective_asserted)
                                                || self
                                                    .checker
                                                    .is_assignable_to(effective_asserted, member)
                                            {
                                                have_overlap = true;
                                                break;
                                            }
                                        }
                                    }

                                    // Final fallback: check structural property overlap.
                                    // Skip the comparable heuristic when both sides are
                                    // Callable types (constructor/class types) because the
                                    // property-overlap check is too permissive — shared
                                    // `prototype` properties mask real mismatches between
                                    // distinct generic instantiations. tsc uses a full
                                    // structural relation (isTypeComparableTo) instead.
                                    // Only skip for Callable; Object types need the check
                                    // for legitimate assertions like `{a: 1} as {a: number}`.
                                    if !have_overlap {
                                        let evaluated_expr =
                                            self.checker.evaluate_type_for_assignability(expr_type);
                                        let evaluated_asserted = self
                                            .checker
                                            .evaluate_type_for_assignability(effective_asserted);
                                        let both_callable = tsz_solver::callable_shape_id(
                                            self.checker.ctx.types,
                                            evaluated_expr,
                                        )
                                        .is_some()
                                            && tsz_solver::callable_shape_id(
                                                self.checker.ctx.types,
                                                evaluated_asserted,
                                            )
                                            .is_some();
                                        if !both_callable {
                                            have_overlap = query::types_are_comparable(
                                                self.checker.ctx.types,
                                                evaluated_expr,
                                                evaluated_asserted,
                                            );
                                        }
                                    }
                                    if !have_overlap {
                                        // tsc anchors TS2352 at the full assertion node
                                        // (`<T>expr` / `expr as T`), not just the inner
                                        // expression. See checkAssertionDeferred:
                                        //   errNode = isParenthesizedExpression(node) ? type : node
                                        self.checker.error_type_assertion_no_overlap(
                                            expr_type,
                                            asserted_type,
                                            idx,
                                        );
                                    }
                                }
                            }
                            asserted_type
                        }
                    }
                } else {
                    TypeId::ERROR
                }
            }
            // Template expression (e.g., `hello ${name}`)
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => self
                .checker
                .get_type_of_template_expression_with_request(idx, request),
            // No-substitution template literal - always preserve literal type.
            // Widening happens at binding sites, not at expression evaluation.
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => self.resolve_literal(
                request,
                self.checker.literal_type_from_initializer(idx),
                TypeId::STRING,
            ),
            // =========================================================================
            // Type Nodes - Delegate to TypeNodeChecker
            // =========================================================================
            // Type nodes that need binder resolution - delegate to get_type_from_type_node
            // which handles special cases with proper symbol resolution
            k if k == syntax_kind_ext::TYPE_REFERENCE => self.checker.get_type_from_type_node(idx),
            // Type nodes handled by TypeNodeChecker
            k if k == syntax_kind_ext::UNION_TYPE
                || k == syntax_kind_ext::INTERSECTION_TYPE
                || k == syntax_kind_ext::ARRAY_TYPE
                || k == syntax_kind_ext::FUNCTION_TYPE
                || k == syntax_kind_ext::CONSTRUCTOR_TYPE
                || k == syntax_kind_ext::TYPE_LITERAL
                || k == syntax_kind_ext::TYPE_QUERY
                || k == syntax_kind_ext::TYPE_OPERATOR =>
            {
                let mut checker = crate::TypeNodeChecker::new(&mut self.checker.ctx);
                checker.check(idx)
            }
            // Keyword types - when recovered into value positions, TypeScript emits TS2693.
            // NullKeyword has no value-position check (null is a valid value).
            k if k == SyntaxKind::NullKeyword as u16 => TypeId::NULL,
            k if keyword_type_mapping(k).is_some() => {
                let (name, type_id) = keyword_type_mapping(k).expect("is_some guard checked above");
                if self.checker.is_keyword_type_used_as_value_position(idx) {
                    // Route through wrong-meaning boundary: keyword type is type-only
                    use crate::query_boundaries::name_resolution::NameLookupKind;
                    self.checker
                        .report_wrong_meaning_diagnostic(name, idx, NameLookupKind::Type);
                    TypeId::ERROR
                } else {
                    type_id
                }
            }
            // Qualified name (A.B.C) - resolve namespace member access
            k if k == syntax_kind_ext::QUALIFIED_NAME => self.checker.resolve_qualified_name(idx),
            // Declaration nodes - not expressions, return VOID to avoid wasted work.
            // These are handled by check_statement → check_interface_declaration / check_class_declaration.
            // get_type_of_node may be called on them (e.g., for index signature compatibility checks),
            // but they don't have a meaningful expression type.
            k if k == syntax_kind_ext::INTERFACE_DECLARATION
                || k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                || k == syntax_kind_ext::ENUM_DECLARATION
                || k == syntax_kind_ext::MODULE_DECLARATION =>
            {
                TypeId::VOID
            }
            // JSX Elements (Rule #36: JSX Intrinsic Lookup)
            k if k == syntax_kind_ext::JSX_ELEMENT => {
                if let Some(jsx) = self.checker.ctx.arena.get_jsx_element(node) {
                    // Extract contextual type for children from the component's
                    // `children` prop BEFORE processing children, so arrow functions
                    // and other expressions get contextual parameter typing.
                    let children_ctx_type = if !jsx.children.nodes.is_empty() {
                        self.checker
                            .get_jsx_children_contextual_type(jsx.opening_element)
                    } else {
                        None
                    };
                    let children_request = request
                        .read()
                        .normal_origin()
                        .contextual_opt(children_ctx_type);
                    // Collect children types for children prop synthesis.
                    // tsc synthesizes a `children` prop from JSX element body children
                    // and validates it against the component's `children` prop type.
                    let mut child_types: Vec<TypeId> = Vec::new();
                    let mut has_text_child = false;
                    let mut text_child_indices: Vec<NodeIndex> = Vec::new();
                    let mut has_spread_child = false;
                    for &child in &jsx.children.nodes {
                        if let Some(child_node) = self.checker.ctx.arena.get(child) {
                            // Skip trivial whitespace JsxText — tsc ignores whitespace-only
                            // text that contains newlines (formatting indentation). But
                            // same-line whitespace (e.g., `<A />  <B />`) is preserved.
                            if child_node.kind == tsz_scanner::SyntaxKind::JsxText as u16
                                && let Some(text) = self.checker.ctx.arena.get_jsx_text(child_node)
                            {
                                let is_all_whitespace =
                                    text.text.chars().all(|c| c.is_ascii_whitespace());
                                let has_newline = text.text.contains('\n');
                                if is_all_whitespace && has_newline {
                                    continue;
                                }
                            }
                            // Skip empty JSX expressions (e.g., {/* comment */})
                            // — tsc does not count these as children.
                            if child_node.kind == syntax_kind_ext::JSX_EXPRESSION
                                && let Some(expr_data) =
                                    self.checker.ctx.arena.get_jsx_expression(child_node)
                                && expr_data.expression == NodeIndex::NONE
                            {
                                continue;
                            }
                        }
                        let child_type = if let Some(child_node) = self.checker.ctx.arena.get(child)
                            && child_node.kind == syntax_kind_ext::JSX_EXPRESSION
                            && let Some(expr_data) =
                                self.checker.ctx.arena.get_jsx_expression(child_node)
                            && expr_data.dot_dot_dot_token
                        {
                            has_spread_child = true;
                            let spread_type = self.checker.get_type_of_node_with_request(
                                expr_data.expression,
                                &children_request,
                            );
                            self.checker
                                .normalize_jsx_spread_child_type(child, spread_type)
                        } else if let Some(child_node) = self.checker.ctx.arena.get(child)
                            && child_node.kind == syntax_kind_ext::JSX_EXPRESSION
                            && let Some(expr_data) =
                                self.checker.ctx.arena.get_jsx_expression(child_node)
                            && expr_data.expression.is_some()
                            && self
                                .checker
                                .ctx
                                .arena
                                .get(expr_data.expression)
                                .is_some_and(|expr| {
                                    matches!(
                                        expr.kind,
                                        syntax_kind_ext::ARROW_FUNCTION
                                            | syntax_kind_ext::FUNCTION_EXPRESSION
                                    )
                                })
                        {
                            let has_function_context =
                                children_request.contextual_type.is_some_and(|ctx_type| {
                                    let ctx_type =
                                        self.checker.resolve_type_for_property_access(ctx_type);
                                    tsz_solver::type_queries::get_function_shape(
                                        self.checker.ctx.types,
                                        ctx_type,
                                    )
                                    .is_some()
                                        || tsz_solver::type_queries::get_call_signatures(
                                            self.checker.ctx.types,
                                            ctx_type,
                                        )
                                        .is_some_and(|sigs| !sigs.is_empty())
                                });
                            if has_function_context {
                                self.checker
                                    .get_type_of_node_with_request(child, &children_request)
                            } else {
                                TypeId::ANY
                            }
                        } else {
                            self.checker
                                .get_type_of_node_with_request(child, &children_request)
                        };
                        if let Some(child_node) = self.checker.ctx.arena.get(child)
                            && child_node.kind == tsz_scanner::SyntaxKind::JsxText as u16
                        {
                            has_text_child = true;
                            text_child_indices.push(child);
                        }
                        child_types.push(child_type);
                    }
                    // Synthesize the children type:
                    // - 0 children → None (no children prop synthesized)
                    // - 1 child → the child's type directly
                    // - 2+ children → array of union of child types
                    let children_ctx = if !child_types.is_empty() {
                        let synthesized_type = if child_types.len() == 1 && !has_spread_child {
                            child_types[0]
                        } else {
                            // Multiple children: synthesize as an array type.
                            // tsc uses the union of all child types as the element type.
                            let element_type =
                                self.checker.ctx.types.factory().union(child_types.clone());
                            self.checker.ctx.types.factory().array(element_type)
                        };
                        let normalized_child_count = if has_spread_child {
                            child_types.len().max(2)
                        } else {
                            child_types.len()
                        };
                        Some(crate::checkers_domain::JsxChildrenContext {
                            child_count: normalized_child_count,
                            has_text_child,
                            synthesized_type,
                            text_child_indices,
                        })
                    } else {
                        None
                    };
                    // Check closing element for TS7026 (tsc emits for both opening and closing tags)
                    self.checker
                        .check_jsx_closing_element_for_implicit_any(jsx.closing_element);
                    self.checker.get_type_of_jsx_opening_element_with_children(
                        jsx.opening_element,
                        request,
                        children_ctx,
                    )
                } else {
                    TypeId::ERROR
                }
            }
            k if k == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT => self
                .checker
                .get_type_of_jsx_opening_element_with_children(idx, request, None),
            k if k == syntax_kind_ext::JSX_FRAGMENT => {
                if let Some(jsx) = self.checker.ctx.arena.get_jsx_fragment(node) {
                    for &child in &jsx.children.nodes {
                        self.checker.get_type_of_node_with_request(child, request);
                    }
                }
                // JSX fragments resolve to JSX.Element type
                self.checker.get_jsx_element_type(idx)
            }
            k if k == syntax_kind_ext::JSX_EXPRESSION => {
                if let Some(jsx_expr) = self.checker.ctx.arena.get_jsx_expression(node) {
                    if jsx_expr.expression.is_some() {
                        self.checker
                            .get_type_of_node_with_request(jsx_expr.expression, request)
                    } else {
                        TypeId::ANY
                    }
                } else {
                    TypeId::ERROR
                }
            }
            k if k == tsz_scanner::SyntaxKind::JsxText as u16 => TypeId::STRING,
            // Non-null assertion: x!
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                // TS8013: Non-null assertions can only be used in TypeScript files
                if self.checker.is_js_file() {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.checker.error_at_node(
                        idx,
                        diagnostic_messages::NON_NULL_ASSERTIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                        diagnostic_codes::NON_NULL_ASSERTIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    );
                }
                // Get the operand type (strip the ! assertion — removes null/undefined)
                if let Some(unary) = self.checker.ctx.arena.get_unary_expr_ex(node) {
                    let inner_expr = self
                        .checker
                        .ctx
                        .arena
                        .skip_parenthesized_and_assertions(unary.expression);
                    let needs_context = request.contextual_type.is_some()
                        && (self.checker.argument_needs_contextual_type(inner_expr)
                            || self.checker.ctx.arena.get(inner_expr).is_some_and(|n| {
                                n.kind == syntax_kind_ext::CALL_EXPRESSION
                                    || n.kind == syntax_kind_ext::NEW_EXPRESSION
                            }));
                    if needs_context {
                        self.checker.clear_type_cache_recursive(unary.expression);
                    }
                    let operand_type = self
                        .checker
                        .get_type_of_node_with_request(unary.expression, request);
                    let evaluated_operand = self.checker.evaluate_type_with_env(operand_type);
                    let db = self.checker.ctx.types.as_type_database();
                    let result = crate::query_boundaries::flow::narrow_non_null_assertion(
                        db,
                        evaluated_operand,
                    );
                    // When the flow-narrowed type is purely nullish (e.g. after `x = undefined`),
                    // remove_nullish produces `never`. In tsc, `x!` in this scenario uses
                    // the declared type of the variable minus nullish instead of the
                    // flow-narrowed type. This prevents false TS2339 "Property does not exist
                    // on type 'never'" errors on expressions like `x!.slice()`.
                    if result == TypeId::NEVER
                        && operand_type != TypeId::NEVER
                        && let Some(expr_node) = self.checker.ctx.arena.get(unary.expression)
                        && expr_node.kind == SyntaxKind::Identifier as u16
                        && let Some(sym_id) =
                            self.checker.resolve_identifier_symbol(unary.expression)
                    {
                        let declared_type = self.checker.get_type_of_symbol(sym_id);
                        let declared_result =
                            crate::query_boundaries::flow::narrow_non_null_assertion(
                                db,
                                declared_type,
                            );
                        if declared_result != TypeId::NEVER {
                            return declared_result;
                        }
                    }
                    result
                } else {
                    TypeId::ERROR
                }
            }
            // Type predicate nodes appear in function return type positions
            // (`x is T` or `asserts x is T`). We delegate to type node resolution
            // to correctly get `boolean` or `void`.
            k if k == syntax_kind_ext::TYPE_PREDICATE => self.checker.get_type_from_type_node(idx),
            // ExpressionWithTypeArguments: `expr<T>` used as a standalone expression
            // (e.g., `List<number>.makeChild()`). Evaluate the inner expression
            // to trigger name resolution (TS2304) even though the overall node
            // produces a parse error (TS1477).
            k if k == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS => {
                if let Some(data) = self.checker.ctx.arena.get_expr_type_args(node) {
                    self.checker.get_type_of_node(data.expression)
                } else {
                    TypeId::ERROR
                }
            }
            // MetaProperty: `new.target` (import.meta is parsed as PROPERTY_ACCESS_EXPRESSION)
            k if k == syntax_kind_ext::META_PROPERTY => {
                // new.target returns the constructor function or undefined.
                // Return any as a safe fallback.
                TypeId::ANY
            }
            // Default case - unknown node kind is an error
            _ => {
                tracing::warn!(
                    idx = idx.0,
                    kind = node.kind,
                    "dispatch_type_computation: unknown expression kind"
                );
                TypeId::ERROR
            }
        }
    }
}

use crate::dispatch_helpers::keyword_type_mapping;
