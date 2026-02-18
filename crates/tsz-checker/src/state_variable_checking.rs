//! Variable declaration and destructuring checking.

use crate::query_boundaries::state_checking as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    // Check a variable statement (var/let/const declarations).
    // Iterable/Iterator Type Checking Methods
    // ============================================================================
    // The following methods have been extracted to src/checker/iterable_checker.rs:
    // - is_iterable_type
    // - is_async_iterable_type
    // - for_of_element_type
    // - check_for_of_iterability
    // - check_spread_iterability
    //
    // These methods are now provided via a separate impl block in iterable_checker.rs
    // as part of Phase 2 architecture refactoring to break up the state.rs god object.
    // ============================================================================

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
            if single_declaration && !var_decl.initializer.is_none() {
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
            if !var_decl.type_annotation.is_none() {
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
                    tsz_solver::freshness::widen_freshness(self.ctx.types, element_type)
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

        // Valid types: any, unknown, object types, type parameters
        // Invalid types: primitive types like void, null, undefined, number, string, boolean, bigint, symbol
        let is_valid = expr_type == TypeId::ANY
            || expr_type == TypeId::UNKNOWN
            || query::is_type_parameter(self.ctx.types, expr_type)
            || query::is_object_like_type(self.ctx.types, expr_type)
            // Also allow union types that contain valid types
            || self.for_in_expr_type_is_valid_union(expr_type);

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
                    || query::is_type_parameter(self.ctx.types, member)
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
        // This applies to identifiers and property/element access expressions.
        if !is_for_of {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            let var_type = self.get_type_of_node(initializer);
            // The LHS type must be string, any, or a type assignable to string
            if var_type != TypeId::STRING
                && var_type != TypeId::ANY
                && var_type != TypeId::UNKNOWN
                && !self.is_assignable_to(TypeId::STRING, var_type)
            {
                self.error_at_node(
                    initializer,
                    diagnostic_messages::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_OF_TYPE_STRING_OR_ANY,
                    diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_OF_TYPE_STRING_OR_ANY,
                );
            }
        }

        // Get the type of the initializer expression (this evaluates `v`, `v++`, `obj.prop`, etc.)
        let var_type = self.get_type_of_node(initializer);
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

        // TS2322: Check element type is assignable to the variable's declared type
        if is_for_of
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

    /// Check a single variable declaration.
    #[tracing::instrument(level = "trace", skip(self), fields(decl_idx = ?decl_idx))]
    pub(crate) fn check_variable_declaration(&mut self, decl_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return;
        };

        let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) else {
            return;
        };

        // TS1155: Check if const declarations must be initialized
        // Skip check for ambient declarations (e.g., declare const x;)
        if !self.is_ambient_declaration(decl_idx) {
            // Get the parent node (VARIABLE_DECLARATION_LIST) to check flags
            if let Some(ext) = self.ctx.arena.get_extended(decl_idx)
                && let Some(parent_node) = self.ctx.arena.get(ext.parent)
            {
                use tsz_parser::parser::node_flags;
                let is_const = (parent_node.flags & node_flags::CONST as u16) != 0;

                if is_const && var_decl.initializer.is_none() {
                    // Skip for destructuring patterns - they get TS1182 from the parser
                    let is_binding_pattern =
                        if let Some(name_node) = self.ctx.arena.get(var_decl.name) {
                            name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                        } else {
                            false
                        };

                    // Check if this is in a for-in or for-of loop (allowed)
                    let is_in_for_loop =
                        if let Some(parent_ext) = self.ctx.arena.get_extended(ext.parent) {
                            if let Some(gp_node) = self.ctx.arena.get(parent_ext.parent) {
                                gp_node.kind == syntax_kind_ext::FOR_IN_STATEMENT
                                    || gp_node.kind == syntax_kind_ext::FOR_OF_STATEMENT
                            } else {
                                false
                            }
                        } else {
                            false
                        };

                    if !is_in_for_loop && !is_binding_pattern {
                        self.ctx.error(
                            node.pos,
                            node.end - node.pos,
                            "'const' declarations must be initialized.".to_string(),
                            1155,
                        );
                    }
                }
            }
        }

        // TS1255/TS1263/TS1264: Definite assignment assertion checks on variables
        if var_decl.exclamation_token {
            // TS1255: ! is not permitted in ambient context (declare let/var/const)
            if self.is_ambient_declaration(decl_idx) {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    var_decl.name,
                    diagnostic_messages::A_DEFINITE_ASSIGNMENT_ASSERTION_IS_NOT_PERMITTED_IN_THIS_CONTEXT,
                    diagnostic_codes::A_DEFINITE_ASSIGNMENT_ASSERTION_IS_NOT_PERMITTED_IN_THIS_CONTEXT,
                );
            }

            // TS1263: ! with initializer is contradictory
            if !var_decl.initializer.is_none() {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    var_decl.name,
                    diagnostic_messages::DECLARATIONS_WITH_INITIALIZERS_CANNOT_ALSO_HAVE_DEFINITE_ASSIGNMENT_ASSERTIONS,
                    diagnostic_codes::DECLARATIONS_WITH_INITIALIZERS_CANNOT_ALSO_HAVE_DEFINITE_ASSIGNMENT_ASSERTIONS,
                );
            }

            // TS1264: ! without type annotation is meaningless
            if var_decl.type_annotation.is_none() {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    var_decl.name,
                    diagnostic_messages::DECLARATIONS_WITH_DEFINITE_ASSIGNMENT_ASSERTIONS_MUST_ALSO_HAVE_TYPE_ANNOTATIONS,
                    diagnostic_codes::DECLARATIONS_WITH_DEFINITE_ASSIGNMENT_ASSERTIONS_MUST_ALSO_HAVE_TYPE_ANNOTATIONS,
                );
            }
        }

        // Check if this is a destructuring pattern (object/array binding)
        let is_destructuring = if let Some(name_node) = self.ctx.arena.get(var_decl.name) {
            name_node.kind != SyntaxKind::Identifier as u16
        } else {
            false
        };

        // Get the variable name for adding to local scope
        let var_name = if !is_destructuring {
            if let Some(name_node) = self.ctx.arena.get(var_decl.name) {
                self.ctx
                    .arena
                    .get_identifier(name_node)
                    .map(|ident| ident.escaped_text.clone())
            } else {
                None
            }
        } else {
            None
        };

        // TS1212/1213/1214: Identifier expected. '{0}' is a reserved word in strict mode.
        // Check if variable name is a strict-mode reserved word used in strict context.
        if self.is_strict_mode_for_node(var_decl.name)
            && let Some(ref name) = var_name
            && crate::state_checking::is_strict_mode_reserved_name(name)
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            if self.ctx.enclosing_class.is_some() {
                let message = format_message(
                    diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO,
                    &[name],
                );
                self.error_at_node(
                    var_decl.name,
                    &message,
                    diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO,
                );
            } else if self.ctx.binder.is_external_module() {
                let message = format_message(
                    diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY,
                    &[name],
                );
                self.error_at_node(
                    var_decl.name,
                    &message,
                    diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY,
                );
            } else {
                let message = format_message(
                    diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE,
                    &[name],
                );
                self.error_at_node(
                    var_decl.name,
                    &message,
                    diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE,
                );
            }
        }

        // TS2480: 'let' is not allowed to be used as a name in 'let' or 'const' declarations.
        if let Some(ref name) = var_name
            && name == "let"
            && let Some(ext) = self.ctx.arena.get_extended(decl_idx)
            && let Some(parent_node) = self.ctx.arena.get(ext.parent)
        {
            use tsz_parser::parser::node_flags;
            let parent_flags = parent_node.flags as u32;
            if parent_flags & node_flags::LET != 0 || parent_flags & node_flags::CONST != 0 {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                        var_decl.name,
                        diagnostic_messages::LET_IS_NOT_ALLOWED_TO_BE_USED_AS_A_NAME_IN_LET_OR_CONST_DECLARATIONS,
                        diagnostic_codes::LET_IS_NOT_ALLOWED_TO_BE_USED_AS_A_NAME_IN_LET_OR_CONST_DECLARATIONS,
                    );
            }
        }

        // TS1100/TS1210: invalid use of 'arguments'/'eval' in strict mode
        // Use class-specific messaging in class bodies.
        if self.is_strict_mode_for_node(var_decl.name)
            && let Some(ref name) = var_name
            && (name == "arguments" || name == "eval")
        {
            use crate::diagnostics::diagnostic_codes;
            if self.ctx.enclosing_class.is_some() {
                self.error_at_node_msg(
                    var_decl.name,
                    diagnostic_codes::CODE_CONTAINED_IN_A_CLASS_IS_EVALUATED_IN_JAVASCRIPTS_STRICT_MODE_WHICH_DOES_NOT,
                    &[name],
                );
            } else {
                self.error_at_node_msg(
                    var_decl.name,
                    diagnostic_codes::INVALID_USE_OF_IN_STRICT_MODE,
                    &[name],
                );
            }
        }

        let is_catch_variable = self.is_catch_clause_variable_declaration(decl_idx);

        // TS1039/TS1254: Check initializers in ambient contexts
        if !var_decl.initializer.is_none() && self.is_ambient_declaration(decl_idx) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            let is_const = self.is_const_variable_declaration(decl_idx);
            if is_const && var_decl.type_annotation.is_none() {
                // Ambient const without type annotation: only string/numeric literals allowed
                if !self.is_valid_ambient_const_initializer(var_decl.initializer) {
                    self.error_at_node(
                        var_decl.initializer,
                        diagnostic_messages::A_CONST_INITIALIZER_IN_AN_AMBIENT_CONTEXT_MUST_BE_A_STRING_OR_NUMERIC_LITERAL_OR,
                        diagnostic_codes::A_CONST_INITIALIZER_IN_AN_AMBIENT_CONTEXT_MUST_BE_A_STRING_OR_NUMERIC_LITERAL_OR,
                    );
                }
            } else {
                // Non-const or const with type annotation
                self.error_at_node(
                    var_decl.initializer,
                    diagnostic_messages::INITIALIZERS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS,
                    diagnostic_codes::INITIALIZERS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS,
                );
            }
        }

        let compute_final_type = |checker: &mut CheckerState| -> TypeId {
            let mut has_type_annotation = !var_decl.type_annotation.is_none();
            let mut declared_type = if has_type_annotation {
                // Check for undefined type names in nested types (e.g., function type parameters)
                // Skip top-level TYPE_REFERENCE to avoid duplicates with get_type_from_type_node
                checker.check_type_for_missing_names_skip_top_level_ref(var_decl.type_annotation);
                checker.check_type_for_parameter_properties(var_decl.type_annotation);
                let type_id = checker.get_type_from_type_node(var_decl.type_annotation);

                // TS1196: Catch clause variable type annotation must be 'any' or 'unknown'
                if is_catch_variable
                    && type_id != TypeId::ANY
                    && type_id != TypeId::UNKNOWN
                    && !checker.type_contains_error(type_id)
                {
                    use crate::diagnostics::diagnostic_codes;
                    checker.error_at_node(
                        var_decl.type_annotation,
                        "Catch clause variable type annotation must be 'any' or 'unknown' if specified.",
                        diagnostic_codes::CATCH_CLAUSE_VARIABLE_TYPE_ANNOTATION_MUST_BE_ANY_OR_UNKNOWN_IF_SPECIFIED,
                    );
                }

                type_id
            } else if is_catch_variable && checker.ctx.use_unknown_in_catch_variables() {
                TypeId::UNKNOWN
            } else {
                TypeId::ANY
            };
            if !has_type_annotation
                && let Some(jsdoc_type) = checker.jsdoc_type_annotation_for_node(decl_idx)
            {
                declared_type = jsdoc_type;
                has_type_annotation = true;
            }

            // If there's a type annotation, that determines the type (even for 'any')
            if has_type_annotation {
                if !var_decl.initializer.is_none() {
                    // Evaluate the declared type to resolve conditionals before using as context.
                    // This ensures types like `type C = string extends string ? "yes" : "no"`
                    // provide proper contextual typing for literals, preventing them from widening to string.
                    // Only evaluate conditional/mapped/index access types - NOT type aliases or interface
                    // references, as evaluating those can change their representation and break variance checking.
                    let evaluated_type = if declared_type != TypeId::ANY {
                        let should_evaluate =
                            crate::query_boundaries::state::should_evaluate_contextual_declared_type(
                                checker.ctx.types,
                                declared_type,
                            );
                        if should_evaluate {
                            checker.judge_evaluate(declared_type)
                        } else {
                            declared_type
                        }
                    } else {
                        declared_type
                    };

                    // Set contextual type for the initializer (but not for 'any')
                    let prev_context = checker.ctx.contextual_type;
                    if evaluated_type != TypeId::ANY {
                        checker.ctx.contextual_type = Some(evaluated_type);
                        // Clear cached type to force recomputation with contextual type
                        // This is necessary because the expression (especially arrow functions)
                        // might have been previously typed without contextual information
                        // (e.g., during symbol binding or early AST traversal)
                        checker.clear_type_cache_recursive(var_decl.initializer);
                    }
                    let init_type = checker.get_type_of_node(var_decl.initializer);
                    checker.ctx.contextual_type = prev_context;

                    // Check assignability (skip for 'any' since anything is assignable to any)
                    // This includes strict null checks - null/undefined should NOT be assignable to non-nullable types
                    if declared_type != TypeId::ANY && !checker.type_contains_error(declared_type) {
                        if let Some((source_level, target_level)) =
                            checker.constructor_accessibility_mismatch_for_var_decl(var_decl)
                        {
                            checker.error_constructor_accessibility_not_assignable(
                                init_type,
                                declared_type,
                                source_level,
                                target_level,
                                decl_idx,
                            );
                        } else if is_destructuring {
                            // For destructuring patterns, keep emitting a generic TS2322 error
                            // instead of detailed property mismatch errors (TS2326-style detail).
                            let _ = checker.check_assignable_or_report_generic_at(
                                init_type,
                                declared_type,
                                var_decl.initializer,
                                decl_idx,
                            );
                        } else if checker.check_assignable_or_report_at(
                            init_type,
                            declared_type,
                            var_decl.initializer,
                            decl_idx,
                        ) {
                            // assignable, keep going to excess-property checks
                            checker.check_object_literal_excess_properties(
                                init_type,
                                declared_type,
                                var_decl.initializer,
                            );
                        }
                    }

                    // Note: Freshness is tracked by the TypeId flags.
                    // Fresh vs non-fresh object types are interned distinctly.
                }
                // Type annotation determines the final type
                return declared_type;
            }

            // No type annotation - infer from initializer
            if !var_decl.initializer.is_none() {
                // Clear cache for closure initializers so TS7006 is properly emitted.
                // During build_type_environment, closures are typed without contextual info
                // and TS7006 is deferred. Now that we're in the checking phase, re-evaluate
                // so TS7006 can fire for closures that truly lack contextual types.
                if let Some(init_node) = checker.ctx.arena.get(var_decl.initializer)
                    && matches!(
                        init_node.kind,
                        syntax_kind_ext::FUNCTION_EXPRESSION | syntax_kind_ext::ARROW_FUNCTION
                    )
                {
                    checker.clear_type_cache_recursive(var_decl.initializer);
                }
                let init_type = checker.get_type_of_node(var_decl.initializer);

                // When strictNullChecks is off, undefined and null widen to any
                // (TypeScript treats `var x = undefined` as `any` without strict)
                if !checker.ctx.strict_null_checks()
                    && (init_type == TypeId::UNDEFINED || init_type == TypeId::NULL)
                {
                    return TypeId::ANY;
                }

                // Under noImplicitAny, mutable unannotated bindings initialized with
                // `undefined`/`null` should behave like evolving-any variables so later
                // assignments don't produce TS2322 (TypeScript reports implicit-any diagnostics).
                if checker.ctx.no_implicit_any()
                    && !checker.is_const_variable_declaration(decl_idx)
                    && var_decl.type_annotation.is_none()
                    && (init_type == TypeId::UNDEFINED || init_type == TypeId::NULL)
                {
                    return TypeId::ANY;
                }

                // Note: Freshness is tracked by the TypeId flags.
                // Fresh vs non-fresh object types are interned distinctly.

                if checker.is_const_variable_declaration(decl_idx) {
                    if let Some(literal_type) =
                        checker.literal_type_from_initializer(var_decl.initializer)
                    {
                        return literal_type;
                    }
                    return init_type;
                }

                // Only widen when the initializer is a "fresh" literal expression
                // (direct literal in source code). Types from variable references,
                // narrowing, or computed expressions are "non-fresh" and NOT widened.
                // EXCEPTION: Enum member types are always widened for mutable bindings.
                let is_enum_member = checker.is_enum_member_type_for_widening(init_type);
                let widened = if is_enum_member
                    || checker.is_fresh_literal_expression(var_decl.initializer)
                {
                    checker.widen_initializer_type_for_mutable_binding(init_type)
                } else {
                    init_type
                };
                // When strictNullChecks is off, undefined and null widen to any
                // regardless of freshness (this applies to destructured bindings too)
                if !checker.ctx.strict_null_checks()
                    && (widened == TypeId::UNDEFINED || widened == TypeId::NULL)
                {
                    TypeId::ANY
                } else {
                    widened
                }
            } else {
                // For for-in/for-of loop variables, the element type has already been cached
                // by assign_for_in_of_initializer_types. Use that instead of defaulting to any.
                if let Some(sym_id) = checker.ctx.binder.get_node_symbol(decl_idx)
                    && let Some(&cached) = checker.ctx.symbol_types.get(&sym_id)
                    && cached != TypeId::ANY
                    && cached != TypeId::ERROR
                {
                    return cached;
                }
                declared_type
            }
        };

        if let Some(sym_id) = self.ctx.binder.get_node_symbol(decl_idx) {
            self.push_symbol_dependency(sym_id, true);
            // Snapshot whether symbol was already cached BEFORE compute_final_type.
            // If it was, any ERROR in the cache is from earlier resolution (e.g., use-before-def),
            // not from circular detection during this declaration's initializer processing.
            let sym_already_cached = self.ctx.symbol_types.contains_key(&sym_id);
            let mut final_type = compute_final_type(self);
            // Check if get_type_of_symbol cached ERROR specifically DURING compute_final_type.
            // This happens when the initializer (directly or indirectly) references the variable,
            // causing the node-level cycle detection to return ERROR.
            let sym_cached_as_error =
                !sym_already_cached && self.ctx.symbol_types.get(&sym_id) == Some(&TypeId::ERROR);
            if !self.ctx.compiler_options.sound_mode {
                final_type = tsz_solver::freshness::widen_freshness(self.ctx.types, final_type);
            }
            self.pop_symbol_dependency();

            // FIX: Always cache the widened type, overwriting any fresh type that was
            // cached during compute_final_type. This prevents "Zombie Freshness" where
            // get_type_of_symbol returns the stale fresh type instead of the widened type.
            //
            // EXCEPT: For merged interface+variable symbols (e.g., `interface Error` +
            // `declare var Error: ErrorConstructor`), get_type_of_symbol already cached
            // the INTERFACE type (which is the correct type for type-position usage like
            // `var e: Error`). The variable declaration's type annotation resolves to
            // the constructor/value type, so overwriting would corrupt the cached interface
            // type. Value-position resolution (`new Error()`) is handled separately by
            // `get_type_of_identifier` which has its own merged-symbol path.
            {
                let is_merged_interface = self.ctx.binder.get_symbol(sym_id).is_some_and(|s| {
                    s.flags & tsz_binder::symbol_flags::INTERFACE != 0
                        && s.flags
                            & (tsz_binder::symbol_flags::FUNCTION_SCOPED_VARIABLE
                                | tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE)
                            != 0
                });
                if !is_merged_interface {
                    self.cache_symbol_type(sym_id, final_type);
                }
            }

            // FIX: Update node_types cache with the widened type
            self.ctx.node_types.insert(decl_idx.0, final_type);
            if !var_decl.name.is_none() {
                self.ctx.node_types.insert(var_decl.name.0, final_type);
            }

            // Capture the raw declared type of THIS specific declaration for TS2403.
            // A bare `var y;` (no annotation, no initializer) always declares `any`,
            // even if the symbol type was previously cached as a concrete type.
            // `compute_final_type` may return a cached type for for-in/for-of loops,
            // so we must override that for bare redeclarations.
            let raw_declared_type =
                if var_decl.type_annotation.is_none() && var_decl.initializer.is_none() {
                    TypeId::ANY
                } else {
                    final_type
                };

            // Variables without an initializer/annotation can still get a contextual type in some
            // constructs (notably `for-in` / `for-of` initializers). In those cases, the symbol
            // type may already be cached from the contextual typing logic; prefer that over the
            // default `any` so we match tsc and avoid spurious noImplicitAny errors.
            if var_decl.type_annotation.is_none()
                && var_decl.initializer.is_none()
                && final_type == TypeId::ANY
                && let Some(inferred) = self.ctx.symbol_types.get(&sym_id).copied()
                && inferred != TypeId::ERROR
            {
                final_type = inferred;
            }

            // TS7005: Variable implicitly has an 'any' type
            // Report this error when noImplicitAny is enabled and the variable has no type annotation
            // and the inferred type is 'any'.
            // Skip destructuring patterns - TypeScript doesn't emit TS7005 for them
            // because binding elements with default values can infer their types.
            //
            // For non-ambient declarations, `symbol_types` guards against emitting
            // TS7005 for control-flow typed variables (e.g., `var x;` later assigned).
            // For ambient declarations (`declare var foo;`), there's no control flow
            // so we always emit when the type is implicitly `any`.
            let is_ambient = self.is_ambient_declaration(decl_idx);
            if self.ctx.no_implicit_any()
                && var_decl.type_annotation.is_none()
                && var_decl.initializer.is_none()
                && final_type == TypeId::ANY
            {
                // Check if the variable name is a destructuring pattern
                let is_destructuring_pattern =
                    self.ctx.arena.get(var_decl.name).is_some_and(|name_node| {
                        name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                            || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                    });

                if !is_destructuring_pattern && let Some(ref name) = var_name {
                    if is_ambient {
                        // TS7005: Ambient declarations always emit at the declaration site.
                        use crate::diagnostics::diagnostic_codes;
                        self.error_at_node_msg(
                            var_decl.name,
                            diagnostic_codes::VARIABLE_IMPLICITLY_HAS_AN_TYPE,
                            &[name, "any"],
                        );
                    } else {
                        // Non-ambient: defer decision between TS7034 and no-error.
                        // TS7034 fires when the variable is captured by a nested function.
                        // Detection happens in get_type_of_identifier when a reference
                        // to this variable is found inside a nested function scope.
                        self.ctx
                            .pending_implicit_any_vars
                            .insert(sym_id, var_decl.name);
                    }
                }
            }

            // TS7022/TS7023: Circular initializer/return type implicit any diagnostics.
            // Gated by noImplicitAny (like all TS7xxx implicit-any diagnostics).
            //
            // Detection: During compute_final_type, if get_type_of_symbol was called for
            // this variable's symbol and cached ERROR (sym_cached_as_error), it means the
            // initializer references the variable creating a circular dependency.
            //
            // TS7022: Structural circularity — `var a = { f: a }`.
            // TS7023: Return-type circularity — `var f = () => f()` or
            //         `var f = function() { return f(); }`.
            if self.ctx.no_implicit_any()
                && var_decl.type_annotation.is_none()
                && !var_decl.initializer.is_none()
                && sym_cached_as_error
                && self.type_contains_error(final_type)
            {
                let is_deferred_initializer =
                    self.ctx.arena.get(var_decl.initializer).is_some_and(|n| {
                        matches!(
                            n.kind,
                            syntax_kind_ext::FUNCTION_EXPRESSION
                                | syntax_kind_ext::ARROW_FUNCTION
                                | syntax_kind_ext::CLASS_EXPRESSION
                        )
                    });
                if let Some(ref name) = var_name {
                    use crate::diagnostics::diagnostic_codes;
                    if is_deferred_initializer {
                        // TS7023: Function/arrow initializer with circular return type.
                        self.error_at_node_msg(
                            var_decl.name,
                            diagnostic_codes::IMPLICITLY_HAS_RETURN_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_RETURN_TYPE_ANNOTATION,
                            &[name],
                        );
                    } else {
                        // TS7022: Structural circularity in initializer.
                        self.error_at_node_msg(
                            var_decl.name,
                            diagnostic_codes::IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION_AND_IS_REFERE,
                            &[name],
                        );
                    }
                }
            }

            // Check for variable redeclaration in the current scope (TS2403).
            // Note: This applies specifically to 'var' merging where types must match.
            // let/const duplicates are caught earlier by the binder (TS2451).
            // Skip TS2403 for mergeable declarations (namespace, enum, class, interface, function overloads).
            let is_block_scoped =
                self.ctx.binder.symbols.get(sym_id).is_some_and(|s| {
                    s.flags & tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE != 0
                });
            if !is_block_scoped
                && let Some(prev_type) = self.ctx.var_decl_types.get(&sym_id).copied()
            {
                // Check if this is a mergeable declaration by looking at the node kind.
                // Mergeable declarations: namespace/module, enum, class, interface, function.
                // When these are declared with the same name, they merge instead of conflicting.
                let is_mergeable_declaration = if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                {
                    matches!(
                        decl_node.kind,
                        syntax_kind_ext::MODULE_DECLARATION  // namespace/module
                            | syntax_kind_ext::ENUM_DECLARATION // enum
                            | syntax_kind_ext::CLASS_DECLARATION // class
                            | syntax_kind_ext::INTERFACE_DECLARATION // interface
                            | syntax_kind_ext::FUNCTION_DECLARATION // function
                    )
                } else {
                    false
                };

                // Use raw_declared_type (before contextual override) for TS2403.
                // A bare `var y;` has declared type `any`, even if the symbol type
                // was previously cached as `string` from `var y = ""`.
                if !is_mergeable_declaration
                    && !self.are_var_decl_types_compatible(prev_type, raw_declared_type)
                {
                    if let Some(ref name) = var_name {
                        self.error_subsequent_variable_declaration(
                            name,
                            prev_type,
                            raw_declared_type,
                            decl_idx,
                        );
                    }
                } else {
                    let refined = self.refine_var_decl_type(prev_type, final_type);
                    if refined != prev_type {
                        self.ctx.var_decl_types.insert(sym_id, refined);
                    }
                }
            } else {
                self.ctx.var_decl_types.insert(sym_id, final_type);
            }
        } else {
            compute_final_type(self);
        }

        // If the variable name is a binding pattern, check binding element default values
        if let Some(name_node) = self.ctx.arena.get(var_decl.name)
            && (name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
        {
            // Prefer explicit type annotation; otherwise infer from initializer (matching tsc).
            // This type is used for both default-value checking and for assigning types to
            // binding element symbols created by the binder.
            let pattern_type = if !var_decl.type_annotation.is_none() {
                self.get_type_from_type_node(var_decl.type_annotation)
            } else if !var_decl.initializer.is_none() {
                self.get_type_of_node(var_decl.initializer)
            } else if is_catch_variable && self.ctx.use_unknown_in_catch_variables() {
                TypeId::UNKNOWN
            } else {
                TypeId::ANY
            };

            if name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
                self.check_destructuring_object_literal_computed_excess_properties(
                    var_decl.name,
                    var_decl.initializer,
                    pattern_type,
                );
            }

            // TS2488: Check array destructuring for iterability before assigning types
            if name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
                self.check_destructuring_iterability(
                    var_decl.name,
                    pattern_type,
                    var_decl.initializer,
                );
                self.report_empty_array_destructuring_bounds(var_decl.name, var_decl.initializer);
            }

            // Ensure binding element identifiers get the correct inferred types.
            self.assign_binding_pattern_symbol_types(var_decl.name, pattern_type);
            // Variable declaration destructuring: don't check default value assignability.
            // TypeScript only checks defaults against the element type in function parameter
            // destructuring, not in variable declarations.
            self.check_binding_pattern(var_decl.name, pattern_type, false);

            // Track destructured binding groups for correlated narrowing.
            // Only needed for union source types where narrowing one property affects others.
            let resolved_for_union = self.evaluate_type_for_assignability(pattern_type);
            if query::union_members(self.ctx.types, resolved_for_union).is_some() {
                // Check if this is a const declaration
                let is_const = if let Some(ext) = self.ctx.arena.get_extended(decl_idx) {
                    if let Some(parent_node) = self.ctx.arena.get(ext.parent) {
                        use tsz_parser::parser::node_flags;
                        (parent_node.flags & node_flags::CONST as u16) != 0
                    } else {
                        false
                    }
                } else {
                    false
                };

                self.record_destructured_binding_group(
                    var_decl.name,
                    resolved_for_union,
                    is_const,
                    name_node.kind,
                );
            }
        }
    }

    fn report_empty_array_destructuring_bounds(
        &mut self,
        pattern_idx: NodeIndex,
        initializer_idx: NodeIndex,
    ) {
        let Some(init_node) = self.ctx.arena.get(initializer_idx) else {
            return;
        };
        if init_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return;
        }
        let Some(init_lit) = self.ctx.arena.get_literal_expr(init_node) else {
            return;
        };
        if !init_lit.elements.nodes.is_empty() {
            return;
        }

        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };
        let Some(pattern) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        for (index, &element_idx) in pattern.elements.nodes.iter().enumerate() {
            if element_idx.is_none() {
                continue;
            }
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                continue;
            }
            let Some(element_data) = self.ctx.arena.get_binding_element(element_node) else {
                continue;
            };
            if element_data.dot_dot_dot_token {
                break;
            }
            // TS doesn't report tuple out-of-bounds for empty array destructuring
            // when the element has a default value.
            if !element_data.initializer.is_none() {
                continue;
            }

            self.error_at_node(
                element_data.name,
                &format!("Tuple type '[]' of length '0' has no element at index '{index}'."),
                crate::diagnostics::diagnostic_codes::TUPLE_TYPE_OF_LENGTH_HAS_NO_ELEMENT_AT_INDEX,
            );
        }
    }

    /// Check binding pattern elements and their default values for type correctness.
    ///
    /// This function traverses a binding pattern (object or array destructuring) and verifies
    /// that any default values provided in binding elements are assignable to their expected types.
    /// Assign inferred types to binding element symbols (destructuring).
    ///
    /// The binder creates symbols for identifiers inside binding patterns (e.g., `const [x] = arr;`),
    /// but their `value_declaration` is the identifier node, not the enclosing variable declaration.
    /// We infer the binding element type from the destructured value type and cache it on the symbol.
    pub(crate) fn assign_binding_pattern_symbol_types(
        &mut self,
        pattern_idx: NodeIndex,
        parent_type: TypeId,
    ) {
        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };
        let Some(pattern_data) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        let pattern_kind = pattern_node.kind;
        for (i, &element_idx) in pattern_data.elements.nodes.iter().enumerate() {
            if element_idx.is_none() {
                continue;
            }

            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                continue;
            }

            let Some(element_data) = self.ctx.arena.get_binding_element(element_node) else {
                continue;
            };

            let element_type = if parent_type == TypeId::ANY {
                TypeId::ANY
            } else {
                self.get_binding_element_type(pattern_kind, i, parent_type, element_data)
            };

            let Some(name_node) = self.ctx.arena.get(element_data.name) else {
                continue;
            };

            // Identifier binding: cache the inferred type on the symbol.
            if name_node.kind == SyntaxKind::Identifier as u16
                && let Some(sym_id) = self.ctx.binder.get_node_symbol(element_data.name)
            {
                // When strictNullChecks is off, undefined and null widen to any
                // for mutable destructured bindings (var/let).
                // This includes unions like `undefined | null`.
                let final_type = if !self.ctx.strict_null_checks()
                    && self.is_only_undefined_or_null(element_type)
                {
                    TypeId::ANY
                } else {
                    element_type
                };
                self.cache_symbol_type(sym_id, final_type);
            }

            // Nested binding patterns: check iterability for array patterns, then recurse
            if name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
                // Check iterability for nested array destructuring
                self.check_destructuring_iterability(
                    element_data.name,
                    element_type,
                    NodeIndex::NONE,
                );
                self.assign_binding_pattern_symbol_types(element_data.name, element_type);
            } else if name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
                self.assign_binding_pattern_symbol_types(element_data.name, element_type);
            }
        }
    }

    /// Record destructured binding group information for correlated narrowing.
    /// When `const { data, isSuccess } = useQuery()`, this records that both `data` and
    /// `isSuccess` come from the same union source and can be used for correlated narrowing.
    pub(crate) fn record_destructured_binding_group(
        &mut self,
        pattern_idx: NodeIndex,
        source_type: TypeId,
        is_const: bool,
        pattern_kind: u16,
    ) {
        use crate::context::DestructuredBindingInfo;

        let group_id = self.ctx.next_binding_group_id;
        self.ctx.next_binding_group_id += 1;

        let mut stack: Vec<(NodeIndex, TypeId, u16, String)> =
            vec![(pattern_idx, source_type, pattern_kind, String::new())];

        while let Some((curr_pattern_idx, curr_source_type, curr_kind, base_path)) = stack.pop() {
            let Some(curr_pattern_node) = self.ctx.arena.get(curr_pattern_idx) else {
                continue;
            };
            let Some(curr_pattern_data) = self.ctx.arena.get_binding_pattern(curr_pattern_node)
            else {
                continue;
            };

            let curr_is_object = curr_kind == syntax_kind_ext::OBJECT_BINDING_PATTERN;

            for (i, &element_idx) in curr_pattern_data.elements.nodes.iter().enumerate() {
                if element_idx.is_none() {
                    continue;
                }
                let Some(element_node) = self.ctx.arena.get(element_idx) else {
                    continue;
                };
                if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                    continue;
                }
                let Some(element_data) = self.ctx.arena.get_binding_element(element_node) else {
                    continue;
                };
                let Some(name_node) = self.ctx.arena.get(element_data.name) else {
                    continue;
                };

                let path_segment = if curr_is_object {
                    if !element_data.property_name.is_none() {
                        if let Some(prop_node) = self.ctx.arena.get(element_data.property_name) {
                            self.ctx
                                .arena
                                .get_identifier(prop_node)
                                .map(|ident| ident.escaped_text.clone())
                                .unwrap_or_default()
                        } else {
                            String::new()
                        }
                    } else {
                        self.ctx
                            .arena
                            .get_identifier(name_node)
                            .map(|ident| ident.escaped_text.clone())
                            .unwrap_or_default()
                    }
                } else {
                    String::new()
                };

                let property_name = if curr_is_object {
                    if base_path.is_empty() {
                        path_segment
                    } else if path_segment.is_empty() {
                        base_path.clone()
                    } else {
                        format!("{base_path}.{path_segment}")
                    }
                } else {
                    String::new()
                };

                if name_node.kind == SyntaxKind::Identifier as u16 {
                    if let Some(sym_id) = self.ctx.binder.get_node_symbol(element_data.name) {
                        self.ctx.destructured_bindings.insert(
                            sym_id,
                            DestructuredBindingInfo {
                                source_type,
                                property_name: property_name.clone(),
                                element_index: i as u32,
                                group_id,
                                is_const,
                            },
                        );
                    }
                    continue;
                }

                if name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                    || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                {
                    let nested_source_type =
                        self.get_binding_element_type(curr_kind, i, curr_source_type, element_data);
                    stack.push((
                        element_data.name,
                        nested_source_type,
                        name_node.kind,
                        property_name,
                    ));
                }
            }
        }
    }

    /// Get the expected type for a binding element from its parent type.
    pub(crate) fn get_binding_element_type(
        &mut self,
        pattern_kind: u16,
        element_index: usize,
        parent_type: TypeId,
        element_data: &tsz_parser::parser::node::BindingElementData,
    ) -> TypeId {
        // Resolve Application/Lazy types to their concrete form so that
        // union members, object shapes, and tuple elements are accessible.
        let parent_type = self.evaluate_type_for_assignability(parent_type);

        // Array binding patterns use the element position.
        if pattern_kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
            if parent_type == TypeId::UNKNOWN || parent_type == TypeId::ERROR {
                return parent_type;
            }

            // For union types of tuples/arrays, resolve element type from each member
            if let Some(members) = query::union_members(self.ctx.types, parent_type) {
                let mut elem_types = Vec::new();
                let factory = self.ctx.types.factory();
                for member in members {
                    let member = query::unwrap_readonly_deep(self.ctx.types, member);
                    if element_data.dot_dot_dot_token {
                        let elem_type = if let Some(elem) =
                            query::array_element_type(self.ctx.types, member)
                        {
                            factory.array(elem)
                        } else if let Some(elems) = query::tuple_elements(self.ctx.types, member) {
                            let rest_elem = elems
                                .iter()
                                .find(|e| e.rest)
                                .or_else(|| elems.last())
                                .map_or(TypeId::ANY, |e| e.type_id);
                            self.rest_binding_array_type(rest_elem)
                        } else {
                            continue;
                        };
                        elem_types.push(elem_type);
                    } else if let Some(elem) = query::array_element_type(self.ctx.types, member) {
                        elem_types.push(elem);
                    } else if let Some(elems) = query::tuple_elements(self.ctx.types, member)
                        && let Some(e) = elems.get(element_index)
                    {
                        elem_types.push(e.type_id);
                    }
                }
                return if elem_types.is_empty() {
                    TypeId::ANY
                } else if elem_types.len() == 1 {
                    elem_types[0]
                } else {
                    factory.union(elem_types)
                };
            }

            // Unwrap readonly wrappers for destructuring element access
            let array_like = query::unwrap_readonly_deep(self.ctx.types, parent_type);

            // Rest element: ...rest
            if element_data.dot_dot_dot_token {
                let elem_type =
                    if let Some(elem) = query::array_element_type(self.ctx.types, array_like) {
                        elem
                    } else if let Some(elems) = query::tuple_elements(self.ctx.types, array_like) {
                        // Best-effort: if the tuple has a rest element, use it; otherwise, fall back to last.
                        elems
                            .iter()
                            .find(|e| e.rest)
                            .or_else(|| elems.last())
                            .map_or(TypeId::ANY, |e| e.type_id)
                    } else {
                        TypeId::ANY
                    };
                return self.rest_binding_array_type(elem_type);
            }

            return if let Some(elem) = query::array_element_type(self.ctx.types, array_like) {
                elem
            } else if let Some(elems) = query::tuple_elements(self.ctx.types, array_like) {
                if let Some(e) = elems.get(element_index) {
                    e.type_id
                } else {
                    let has_rest_tail = elems.last().is_some_and(|element| element.rest);
                    if !has_rest_tail {
                        self.error_at_node(
                            element_data.name,
                            &format!(
                                "Tuple type of length '{}' has no element at index '{}'.",
                                elems.len(),
                                element_index
                            ),
                            crate::diagnostics::diagnostic_codes::TUPLE_TYPE_OF_LENGTH_HAS_NO_ELEMENT_AT_INDEX,
                        );
                    }
                    TypeId::ANY
                }
            } else {
                TypeId::ANY
            };
        }

        let property_optional_type = |property_type: TypeId, optional: bool| {
            if optional {
                self.ctx
                    .types
                    .factory()
                    .union(vec![property_type, TypeId::UNDEFINED])
            } else {
                property_type
            }
        };

        // Get the property name or index
        if !element_data.property_name.is_none() {
            // For computed keys in object binding patterns (e.g. `{ [k]: v }`),
            // check index signatures when the key is not a simple identifier key.
            // This aligns with TS2537 behavior for destructuring from `{}`.
            let computed_expr = self
                .ctx
                .arena
                .get(element_data.property_name)
                .and_then(|prop_node| self.ctx.arena.get_computed_property(prop_node))
                .map(|computed| computed.expression);
            let computed_is_identifier = computed_expr
                .and_then(|expr_idx| {
                    self.ctx
                        .arena
                        .get(expr_idx)
                        .and_then(|expr_node| self.ctx.arena.get_identifier(expr_node))
                })
                .is_some();

            if !computed_is_identifier {
                let key_type =
                    computed_expr.map_or(TypeId::ANY, |expr_idx| self.get_type_of_node(expr_idx));
                let key_is_string = key_type == TypeId::STRING;
                let key_is_number = key_type == TypeId::NUMBER;

                if key_is_string || key_is_number {
                    let has_matching_index = |ty: TypeId| {
                        query::object_shape(self.ctx.types, ty).is_some_and(|shape| {
                            if key_is_string {
                                shape.string_index.is_some()
                            } else {
                                shape.number_index.is_some() || shape.string_index.is_some()
                            }
                        })
                    };

                    let has_index_signature =
                        if let Some(members) = query::union_members(self.ctx.types, parent_type) {
                            members.into_iter().all(has_matching_index)
                        } else {
                            has_matching_index(parent_type)
                        };

                    if !has_index_signature
                        && parent_type != TypeId::ANY
                        && parent_type != TypeId::ERROR
                        && parent_type != TypeId::UNKNOWN
                    {
                        let mut formatter = self.ctx.create_type_formatter();
                        let object_str = formatter.format(parent_type);
                        let index_str = formatter.format(key_type);
                        let message = crate::diagnostics::format_message(
                            crate::diagnostics::diagnostic_messages::TYPE_HAS_NO_MATCHING_INDEX_SIGNATURE_FOR_TYPE,
                            &[&object_str, &index_str],
                        );
                        self.error_at_node(
                            element_data.property_name,
                            &message,
                            crate::diagnostics::diagnostic_codes::TYPE_HAS_NO_MATCHING_INDEX_SIGNATURE_FOR_TYPE,
                        );
                    }
                }
            }
        }

        let property_name = if !element_data.property_name.is_none() {
            // { x: a } - property_name is "x"
            if let Some(prop_node) = self.ctx.arena.get(element_data.property_name) {
                self.ctx
                    .arena
                    .get_identifier(prop_node)
                    .map(|ident| ident.escaped_text.clone())
            } else {
                None
            }
        } else {
            // { x } - the name itself is the property name
            if let Some(name_node) = self.ctx.arena.get(element_data.name) {
                self.ctx
                    .arena
                    .get_identifier(name_node)
                    .map(|ident| ident.escaped_text.clone())
            } else {
                None
            }
        };

        if parent_type == TypeId::UNKNOWN {
            if let Some(prop_name_str) = property_name.as_deref() {
                let error_node = if !element_data.property_name.is_none() {
                    element_data.property_name
                } else if !element_data.name.is_none() {
                    element_data.name
                } else {
                    NodeIndex::NONE
                };
                self.error_property_not_exist_at(prop_name_str, parent_type, error_node);
            }
            return TypeId::UNKNOWN;
        }

        if let Some(prop_name_str) = property_name {
            // Look up the property type in the parent type.
            // For union types, resolve the property in each member and union the results.
            if let Some(members) = query::union_members(self.ctx.types, parent_type) {
                let mut prop_types = Vec::new();
                let factory = self.ctx.types.factory();
                for member in members {
                    if let Some(shape) = query::object_shape(self.ctx.types, member) {
                        for prop in shape.properties.as_slice() {
                            if self.ctx.types.resolve_atom_ref(prop.name).as_ref() == prop_name_str
                            {
                                prop_types
                                    .push(property_optional_type(prop.type_id, prop.optional));
                                break;
                            }
                        }
                    }
                }
                if prop_types.is_empty() {
                    TypeId::ANY
                } else if prop_types.len() == 1 {
                    prop_types[0]
                } else {
                    factory.union(prop_types)
                }
            } else if let Some(shape) = query::object_shape(self.ctx.types, parent_type) {
                // Find the property by comparing names
                for prop in shape.properties.as_slice() {
                    if self.ctx.types.resolve_atom_ref(prop.name).as_ref() == prop_name_str {
                        return property_optional_type(prop.type_id, prop.optional);
                    }
                }
                TypeId::ANY
            } else {
                TypeId::ANY
            }
        } else {
            TypeId::ANY
        }
    }

    /// Rest bindings from tuple members should produce an array type.
    /// Variadic tuple members can already carry array types (`...T[]`), so avoid
    /// wrapping those into nested arrays.
    fn rest_binding_array_type(&self, tuple_member_type: TypeId) -> TypeId {
        let tuple_member_type = query::unwrap_readonly_deep(self.ctx.types, tuple_member_type);
        if query::array_element_type(self.ctx.types, tuple_member_type).is_some() {
            tuple_member_type
        } else {
            self.ctx.types.factory().array(tuple_member_type)
        }
    }

    /// Check if a type consists only of `undefined` and/or `null`.
    /// Used for widening to `any` under `strict: false`.
    /// Returns true for: `undefined`, `null`, `undefined | null`
    fn is_only_undefined_or_null(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::UNDEFINED || type_id == TypeId::NULL {
            return true;
        }
        // Check for union of undefined/null
        if let Some(members) = query::union_members(self.ctx.types, type_id) {
            return members
                .iter()
                .all(|&m| m == TypeId::UNDEFINED || m == TypeId::NULL || m == type_id);
        }
        false
    }
}
