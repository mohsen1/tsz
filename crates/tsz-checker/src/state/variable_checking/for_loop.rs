//! For-in / for-of loop variable checking.
//!
//! Extracted from `core.rs` to keep that file focused on
//! general variable declaration checking (`check_variable_declaration`).

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Assign the inferred loop-variable type for `for-in` / `for-of` initializers.
    ///
    /// The initializer is a `VariableDeclarationList` in the Thin AST.
    /// `is_for_in` should be true for for-in loops (to emit TS2404 on type annotations).
    pub(crate) fn assign_for_in_of_initializer_types(
        &mut self,
        decl_list_idx: NodeIndex,
        element_type: TypeId,
        is_for_in: bool,
    ) {
        let Some(list_node) = self.ctx.arena.get(decl_list_idx) else {
            return;
        };
        let Some(list) = self.ctx.arena.get_variable(list_node) else {
            return;
        };
        // When there are multiple declarations, TS1188 is already reported by the parser.
        // TSC suppresses per-declaration grammar errors (TS1189/TS1190/TS2483) in this case.
        let single_declaration = list.declarations.nodes.len() == 1;
        for &decl_idx in &list.declarations.nodes {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                continue;
            };

            // TS1189/TS1190: The variable declaration of a for-in/for-of statement cannot have an initializer
            // Only check when there's a single declaration (TSC suppresses when TS1188 is reported)
            if single_declaration && var_decl.initializer.is_some() {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                if is_for_in {
                    self.error_at_node(
                        var_decl.initializer,
                        diagnostic_messages::THE_VARIABLE_DECLARATION_OF_A_FOR_IN_STATEMENT_CANNOT_HAVE_AN_INITIALIZER,
                        diagnostic_codes::THE_VARIABLE_DECLARATION_OF_A_FOR_IN_STATEMENT_CANNOT_HAVE_AN_INITIALIZER,
                    );
                } else {
                    self.error_at_node(
                        var_decl.initializer,
                        diagnostic_messages::THE_VARIABLE_DECLARATION_OF_A_FOR_OF_STATEMENT_CANNOT_HAVE_AN_INITIALIZER,
                        diagnostic_codes::THE_VARIABLE_DECLARATION_OF_A_FOR_OF_STATEMENT_CANNOT_HAVE_AN_INITIALIZER,
                    );
                }
            }

            // If there's a type annotation, check that the element type is assignable to it
            if var_decl.type_annotation.is_some() {
                // TS2404: The left-hand side of a 'for...in' statement cannot use a type annotation
                // TSC emits TS2404 and skips the assignability check for for-in loops.
                // TS2483: The left-hand side of a 'for...of' statement cannot use a type annotation
                // Only check with single declaration (TSC suppresses when TS1188 is reported)
                if is_for_in && single_declaration {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.error_at_node(
                        var_decl.type_annotation,
                        diagnostic_messages::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_USE_A_TYPE_ANNOTATION,
                        diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_USE_A_TYPE_ANNOTATION,
                    );
                } else if !is_for_in && single_declaration {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.error_at_node(
                        var_decl.type_annotation,
                        diagnostic_messages::THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_CANNOT_USE_A_TYPE_ANNOTATION,
                        diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_CANNOT_USE_A_TYPE_ANNOTATION,
                    );
                }

                let declared = self.get_type_from_type_node(var_decl.type_annotation);

                // TS2322: Check that element type is assignable to declared type
                // Skip for for-in loops — TSC only emits TS2404 (no assignability check).
                if !is_for_in
                    && declared != TypeId::ANY
                    && !self.type_contains_error(declared)
                    && !self.check_assignable_or_report(element_type, declared, var_decl.name)
                {
                    // Diagnostic emitted by check_assignable_or_report.
                }

                // Assign types for binding patterns (e.g., `for (const [a] of arr)`).
                if let Some(name_node) = self.ctx.arena.get(var_decl.name)
                    && (name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
                {
                    // TS2488: For array binding patterns, check if the element type is iterable
                    // Example: for (const [,] of []) where [] has type never[] with element type never
                    if name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
                        use tsz_parser::NodeIndex;
                        self.check_destructuring_iterability(
                            var_decl.name,
                            declared,
                            NodeIndex::NONE,
                        );
                    }
                    self.assign_binding_pattern_symbol_types(var_decl.name, declared);
                }

                if let Some(sym_id) = self.ctx.binder.get_node_symbol(decl_idx) {
                    self.cache_symbol_type(sym_id, declared);
                } else if let Some(sym_id) = self.ctx.binder.get_node_symbol(var_decl.name) {
                    self.cache_symbol_type(sym_id, declared);
                }
            } else {
                // No type annotation - use element type (with freshness stripped)
                let widened_element_type = if !self.ctx.compiler_options.sound_mode {
                    tsz_solver::relations::freshness::widen_freshness(self.ctx.types, element_type)
                } else {
                    element_type
                };

                // Assign types for binding patterns (e.g., `for (const [a] of arr)`).
                if let Some(name_node) = self.ctx.arena.get(var_decl.name)
                    && (name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
                {
                    // TS2488: For array binding patterns, check if the element type is iterable
                    // Example: for (const [,] of []) where [] has type never[] with element type never
                    if name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
                        use tsz_parser::NodeIndex;
                        self.check_destructuring_iterability(
                            var_decl.name,
                            widened_element_type,
                            NodeIndex::NONE,
                        );
                    }
                    self.assign_binding_pattern_symbol_types(var_decl.name, widened_element_type);
                }

                if let Some(sym_id) = self.ctx.binder.get_node_symbol(decl_idx) {
                    self.cache_symbol_type(sym_id, widened_element_type);
                } else if let Some(sym_id) = self.ctx.binder.get_node_symbol(var_decl.name) {
                    self.cache_symbol_type(sym_id, widened_element_type);
                }
            }
        }
    }

    /// TS2407: The right-hand side of a 'for...in' statement must be of type 'any',
    /// an object type or a type parameter.
    pub(crate) fn check_for_in_expression_type(
        &mut self,
        expr_type: TypeId,
        expression: NodeIndex,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use crate::query_boundaries::dispatch as query;

        // Skip if type is error
        if expr_type == TypeId::ERROR {
            return;
        }

        // Resolve lazy/application types before checking (e.g. Record<string, any>)
        let expr_type = self.resolve_type_for_property_access(expr_type);

        // Valid types: any, unknown, object (non-primitive), object types, type parameters, never
        // Invalid types: primitive types like void, null, undefined, number, string, boolean, bigint, symbol
        let is_valid = expr_type == TypeId::ANY
            || expr_type == TypeId::UNKNOWN
            || expr_type == TypeId::OBJECT
            || expr_type == TypeId::NEVER
            || query::is_type_parameter_like(self.ctx.types, expr_type)
            || query::is_object_like_type(self.ctx.types, expr_type)
            // Also allow union types that contain valid types
            || self.for_in_expr_type_is_valid_union(expr_type)
            // Intersection types like `object & T`: valid if ANY member is valid
            || self.for_in_expr_type_is_valid_intersection(expr_type);

        if !is_valid {
            let type_str = self.format_type(expr_type);
            let message = format_message(
                diagnostic_messages::THE_RIGHT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_OF_TYPE_ANY_AN_OBJECT_TYPE_OR,
                &[&type_str],
            );
            self.error_at_node(expression, &message, diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_OF_TYPE_ANY_AN_OBJECT_TYPE_OR);
        }
    }

    /// Helper for TS2407: Check if a union type contains at least one valid for-in expression type.
    fn for_in_expr_type_is_valid_union(&mut self, expr_type: TypeId) -> bool {
        use crate::query_boundaries::dispatch as query;

        if let Some(members) = query::union_members(self.ctx.types, expr_type) {
            for &member in &members {
                if member == TypeId::ANY
                    || member == TypeId::UNKNOWN
                    || query::is_type_parameter_like(self.ctx.types, member)
                    || query::is_object_like_type(self.ctx.types, member)
                {
                    return true;
                }
                // Recursively check nested unions
                if self.for_in_expr_type_is_valid_union(member) {
                    return true;
                }
            }
        }
        false
    }

    /// Helper for TS2407: Check if an intersection type contains at least one valid for-in member.
    /// `object & T` is valid because it contains `object`.
    fn for_in_expr_type_is_valid_intersection(&mut self, expr_type: TypeId) -> bool {
        use crate::query_boundaries::dispatch as query;

        if let Some(members) = query::intersection_members(self.ctx.types, expr_type) {
            for &member in &members {
                if member == TypeId::ANY
                    || member == TypeId::UNKNOWN
                    || member == TypeId::OBJECT
                    || query::is_type_parameter_like(self.ctx.types, member)
                    || query::is_object_like_type(self.ctx.types, member)
                {
                    return true;
                }
            }
        }
        false
    }

    /// Check assignability for for-in/of expression initializer (non-declaration case).
    ///
    /// For `for (v of expr)` where `v` is a pre-declared variable (not `var v`/`let v`/`const v`),
    /// this checks:
    /// - TS2588: Cannot assign to const variable
    /// - TS2322: Element type not assignable to variable type
    pub(crate) fn check_for_in_of_expression_initializer(
        &mut self,
        initializer: NodeIndex,
        element_type: TypeId,
        is_for_of: bool,
        has_await_modifier: bool,
    ) {
        // TS1106: The left-hand side of a 'for...of' statement may not be 'async'.
        // `for (async of expr)` is ambiguous with `for await (... of ...)`.
        // With `for await`, the `async` identifier is unambiguous, so skip the check.
        if is_for_of
            && !has_await_modifier
            && let Some(init_node) = self.ctx.arena.get(initializer)
            && init_node.kind == SyntaxKind::Identifier as u16
            && let Some(ident) = self.ctx.arena.get_identifier(init_node)
            && self.ctx.arena.resolve_identifier_text(ident) == "async"
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                initializer,
                diagnostic_messages::THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_MAY_NOT_BE_ASYNC,
                diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_MAY_NOT_BE_ASYNC,
            );
        }

        // TS2780/TS2781: The left-hand side of a 'for...in'/'for...of' statement
        // may not be an optional property access.
        if self.is_optional_chain_access(initializer) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            if is_for_of {
                self.error_at_node(
                    initializer,
                    diagnostic_messages::THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS,
                    diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS,
                );
            } else {
                self.error_at_node(
                    initializer,
                    diagnostic_messages::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS,
                    diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS,
                );
            }
        }

        // For-in specific LHS checks (TS2491, TS2406, TS2405)
        if !is_for_of && let Some(init_node) = self.ctx.arena.get(initializer) {
            let init_kind = init_node.kind;
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            use tsz_parser::parser::syntax_kind_ext;

            // TS2491: The left-hand side of a 'for...in' statement cannot be a destructuring pattern.
            if init_kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || init_kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            {
                self.error_at_node(
                    initializer,
                    diagnostic_messages::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_BE_A_DESTRUCTURING_PATTERN,
                    diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_BE_A_DESTRUCTURING_PATTERN,
                );
            }
            // TS2406: The left-hand side of a 'for...in' statement must be a variable or a property access.
            else if init_kind != SyntaxKind::Identifier as u16
                && init_kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && init_kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            {
                if init_kind == syntax_kind_ext::CALL_EXPRESSION
                    || init_kind == syntax_kind_ext::NEW_EXPRESSION
                {
                    self.error_at_node(
                        initializer,
                        diagnostic_messages::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS,
                        diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS,
                    );
                }
                // TS2405: The left-hand side of a 'for...in' statement must be of type 'string' or 'any'.
                // Applies to other expression types (BinaryExpression like `a=1`, `this`, etc.)
                else {
                    self.error_at_node(
                        initializer,
                        diagnostic_messages::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_OF_TYPE_STRING_OR_ANY,
                        diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_OF_TYPE_STRING_OR_ANY,
                    );
                }
            }
        }

        // TS2405: For for-in, also check that the LHS type is string or any.
        // This applies only to valid LHS forms (identifiers and property/element access).
        // Skip if we already emitted TS2491 (destructuring) or TS2406 (invalid form).
        if !is_for_of
            && let Some(init_node) = self.ctx.arena.get(initializer)
            && matches!(
                init_node.kind,
                k if k == SyntaxKind::Identifier as u16
                    || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            )
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            let var_type = self.get_type_of_assignment_target(initializer);
            // The LHS type must accept the for-in element type. TSC checks
            // `isTypeAssignableTo(indexType, variableType)` where indexType
            // comes from the source expression's key type (keyof T & string
            // for generic expressions, plain string otherwise).
            // Using `element_type` instead of hardcoded `string` correctly
            // handles `keyof T`, `K extends string`, `K extends keyof T`, etc.
            if var_type != TypeId::STRING
                && var_type != TypeId::ANY
                && var_type != TypeId::UNKNOWN
                && !self.is_assignable_to(element_type, var_type)
            {
                self.error_at_node(
                    initializer,
                    diagnostic_messages::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_OF_TYPE_STRING_OR_ANY,
                    diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_OF_TYPE_STRING_OR_ANY,
                );
            }
        }

        // Get the type of the initializer expression (this evaluates `v`, `v++`, `obj.prop`, etc.)
        let var_type = self.get_type_of_assignment_target(initializer);
        let target_type = if is_for_of
            && let Some(init_node) = self.ctx.arena.get(initializer)
            && init_node.kind == SyntaxKind::Identifier as u16
            && let Some(sym_id) = self
                .ctx
                .binder
                .resolve_identifier(self.ctx.arena, initializer)
        {
            // For `for (x of y)` with pre-declared identifier `x`, compare against
            // the declared type of `x` (not the current flow-narrowed type).
            self.get_type_of_symbol(sym_id)
        } else {
            var_type
        };

        // TS2588: Cannot assign to const variable
        if is_for_of {
            self.check_const_assignment(initializer);
        }

        // TS2322: Check element type is assignable to the variable's declared type.
        // Skip for destructuring patterns (array/object literal expressions) — those are
        // checked element-by-element during destructuring assignment processing, not as
        // a whole-type assignability check. Individual mismatches (e.g., wrong default
        // values) are caught by the assignment expression checker on each element.
        // Only skip for array destructuring — array literal elements like `k = false`
        // are BinaryExpressions that trigger individual assignment checks.
        // Object destructuring still needs the whole-type check because individual
        // property bindings don't go through the assignment expression checker.
        let is_array_destructuring_target = self
            .ctx
            .arena
            .get(initializer)
            .is_some_and(|n| n.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION);
        if is_for_of
            && !is_array_destructuring_target
            && target_type != TypeId::ANY
            && element_type != TypeId::ANY
            && element_type != TypeId::ERROR
            && !self.type_contains_error(target_type)
        {
            let _ = self.check_assignable_or_report(element_type, target_type, initializer);
        }
    }

    /// TS2491: The left-hand side of a 'for...in' statement cannot be a destructuring pattern.
    /// Checks variable declaration list form: `for (let {a, b} in obj)`
    pub(crate) fn check_for_in_destructuring_pattern(&mut self, initializer: NodeIndex) {
        let arena = self.ctx.arena;
        let Some(init_node) = arena.get(initializer) else {
            return;
        };
        let Some(var_data) = arena.get_variable(init_node) else {
            return;
        };
        // Check the first (and typically only) declaration
        if let Some(&first_decl_idx) = var_data.declarations.nodes.first()
            && let Some(decl_node) = arena.get(first_decl_idx)
            && let Some(var_decl) = arena.get_variable_declaration(decl_node)
            && let Some(name_node) = arena.get(var_decl.name)
            && (name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
        {
            self.error_at_node(
                var_decl.name,
                "The left-hand side of a 'for...in' statement cannot be a destructuring pattern.",
                crate::diagnostics::diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_BE_A_DESTRUCTURING_PATTERN,
            );
        }
    }

    /// TS2491: The left-hand side of a 'for...in' statement cannot be a destructuring pattern.
    /// Checks expression form: `for ([a, b] in obj)` or `for ({a, b} in obj)`
    pub(crate) fn check_for_in_expression_destructuring(&mut self, initializer: NodeIndex) {
        let arena = self.ctx.arena;
        if let Some(init_node) = arena.get(initializer)
            && (init_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
        {
            self.error_at_node(
                initializer,
                "The left-hand side of a 'for...in' statement cannot be a destructuring pattern.",
                crate::diagnostics::diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_BE_A_DESTRUCTURING_PATTERN,
            );
        }
    }
}
