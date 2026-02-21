//! Variable declaration and destructuring checking.

use crate::query_boundaries::state_checking as query;
use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
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

        // Resolve lazy/application types before checking (e.g. Record<string, any>)
        let expr_type = self.resolve_type_for_property_access(expr_type);

        // Valid types: any, unknown, object (non-primitive), object types, type parameters, never
        // Invalid types: primitive types like void, null, undefined, number, string, boolean, bigint, symbol
        let is_valid = expr_type == TypeId::ANY
            || expr_type == TypeId::UNKNOWN
            || expr_type == TypeId::OBJECT
            || expr_type == TypeId::NEVER
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

    fn find_circular_reference_in_type_node(
        &self,
        type_idx: NodeIndex,
        target_sym: SymbolId,
        in_lazy_context: bool,
    ) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(type_idx)?;

        // Function types are safe boundaries (recursion always allowed)
        if matches!(
            node.kind,
            syntax_kind_ext::FUNCTION_TYPE | syntax_kind_ext::CONSTRUCTOR_TYPE
        ) {
            return None;
        }

        // Type literals and mapped types introduce a lazy context where "bare" recursion is allowed
        let is_lazy_boundary = matches!(
            node.kind,
            syntax_kind_ext::TYPE_LITERAL | syntax_kind_ext::MAPPED_TYPE
        );
        let current_lazy = in_lazy_context || is_lazy_boundary;

        if node.kind == syntax_kind_ext::TYPE_QUERY {
            if let Some(query) = self.ctx.arena.get_type_query(node) {
                // Check if the query references the target symbol
                // We need to know if it's a "bare" reference or a property access
                let expr_node = self.ctx.arena.get(query.expr_name)?;

                let is_bare_identifier =
                    expr_node.kind == tsz_scanner::SyntaxKind::Identifier as u16;

                // Extract the symbol referenced by the query
                let mut referenced_sym = None;
                let mut error_node = query.expr_name;

                if is_bare_identifier {
                    referenced_sym =
                        self.ctx
                            .binder
                            .get_node_symbol(query.expr_name)
                            .or_else(|| {
                                self.ctx
                                    .binder
                                    .resolve_identifier(self.ctx.arena, query.expr_name)
                            });
                } else if expr_node.kind == syntax_kind_ext::QUALIFIED_NAME {
                    if let Some(qn) = self.ctx.arena.get_qualified_name(expr_node) {
                        // Check left side
                        if let Some(node) = self.ctx.arena.get(qn.left)
                            && node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                        {
                            referenced_sym =
                                self.ctx.binder.get_node_symbol(qn.left).or_else(|| {
                                    self.ctx.binder.resolve_identifier(self.ctx.arena, qn.left)
                                });
                            error_node = qn.left;
                        }
                    }
                } else if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    && let Some(access) = self.ctx.arena.get_access_expr(expr_node)
                {
                    // Check expression
                    if let Some(node) = self.ctx.arena.get(access.expression)
                        && node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                    {
                        referenced_sym = self
                            .ctx
                            .binder
                            .get_node_symbol(access.expression)
                            .or_else(|| {
                                self.ctx
                                    .binder
                                    .resolve_identifier(self.ctx.arena, access.expression)
                            });
                        error_node = access.expression;
                    }
                }

                if let Some(sym) = referenced_sym
                    && sym == target_sym
                {
                    // Found a reference to the target symbol!
                    // If we are in a lazy context AND it's a bare identifier, it's safe.
                    if current_lazy && is_bare_identifier {
                        return None;
                    }
                    return Some(error_node);
                }

                // Also check type arguments if any (always recursive)
                if let Some(ref args) = query.type_arguments {
                    for &arg_idx in &args.nodes {
                        if let Some(found) = self.find_circular_reference_in_type_node(
                            arg_idx,
                            target_sym,
                            current_lazy,
                        ) {
                            return Some(found);
                        }
                    }
                }
            }
            return None;
        }

        // Explicitly recurse into type annotations of members, as generic get_children might miss them
        if matches!(
            node.kind,
            syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR
        ) {
            if let Some(accessor) = self.ctx.arena.get_accessor(node)
                && accessor.type_annotation.is_some()
                && let Some(found) = self.find_circular_reference_in_type_node(
                    accessor.type_annotation,
                    target_sym,
                    current_lazy,
                )
            {
                return Some(found);
            }
        } else if matches!(
            node.kind,
            syntax_kind_ext::PROPERTY_SIGNATURE | syntax_kind_ext::PROPERTY_DECLARATION
        ) && let Some(prop) = self.ctx.arena.get_property_decl(node)
            && prop.type_annotation.is_some()
            && let Some(found) = self.find_circular_reference_in_type_node(
                prop.type_annotation,
                target_sym,
                current_lazy,
            )
        {
            return Some(found);
        }

        // Recursive descent
        for child in self.ctx.arena.get_children(type_idx) {
            if let Some(found) =
                self.find_circular_reference_in_type_node(child, target_sym, current_lazy)
            {
                return Some(found);
            }
        }

        None
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
            if var_decl.initializer.is_some() {
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

        let mut is_ambient = self.ctx.file_name.ends_with(".d.ts");
        if !is_ambient {
            let mut current = decl_idx;
            let mut guard = 0;
            while current.is_some() {
                guard += 1;
                if guard > 256 {
                    break;
                }
                if let Some(node) = self.ctx.arena.get(current) {
                    if node.kind == tsz_parser::parser::syntax_kind_ext::MODULE_DECLARATION {
                        if let Some(module) = self.ctx.arena.get_module(node)
                            && self.ctx.has_modifier(
                                &module.modifiers,
                                tsz_scanner::SyntaxKind::DeclareKeyword as u16,
                            )
                        {
                            is_ambient = true;
                            break;
                        }
                    } else if node.kind == tsz_parser::parser::syntax_kind_ext::VARIABLE_STATEMENT {
                        if let Some(var_stmt) = self.ctx.arena.get_variable(node)
                            && self.ctx.has_modifier(
                                &var_stmt.modifiers,
                                tsz_scanner::SyntaxKind::DeclareKeyword as u16,
                            )
                        {
                            is_ambient = true;
                            break;
                        }
                    } else if node.kind == tsz_parser::parser::syntax_kind_ext::SOURCE_FILE {
                        break;
                    }
                }
                if let Some(ext) = self.ctx.arena.get_extended(current) {
                    current = ext.parent;
                } else {
                    break;
                }
            }
        }
        if !is_ambient
            && self.is_strict_mode_for_node(var_decl.name)
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
        if var_decl.initializer.is_some() && self.is_ambient_declaration(decl_idx) {
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
            let mut has_type_annotation = var_decl.type_annotation.is_some();
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
                if var_decl.initializer.is_some() {
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
            if var_decl.initializer.is_some() {
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
                let mut init_type = checker.get_type_of_node(var_decl.initializer);

                // TypeScript treats unannotated empty-array declaration initializers
                // (`let/var/const x = []`) as evolving-any arrays for subsequent writes.
                // Keep expression-level `[]` behavior unchanged by only applying this to
                // direct declaration initializers.
                let init_is_direct_empty_array = checker
                    .ctx
                    .arena
                    .get(var_decl.initializer)
                    .is_some_and(|init_node| {
                        init_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                            && checker
                                .ctx
                                .arena
                                .get_literal_expr(init_node)
                                .is_some_and(|lit| lit.elements.nodes.is_empty())
                    });
                if init_is_direct_empty_array
                    && tsz_solver::type_queries::get_array_element_type(
                        checker.ctx.types,
                        init_type,
                    ) == Some(TypeId::NEVER)
                {
                    init_type = checker.ctx.types.factory().array(TypeId::ANY);
                }

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
                    && tsz_solver::type_queries::is_only_null_or_undefined(
                        checker.ctx.types,
                        widened,
                    )
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

            // TS2502: 'x' is referenced directly or indirectly in its own type annotation.
            if var_decl.type_annotation.is_some() {
                // Try AST-based check first (catches complex circularities that confuse the solver)
                let ast_circular = self
                    .find_circular_reference_in_type_node(var_decl.type_annotation, sym_id, false)
                    .is_some();

                // Then try semantic check
                let semantic_circular = !ast_circular
                    && tsz_solver::type_queries::has_type_query_for_symbol(
                        self.ctx.types,
                        final_type,
                        sym_id.0,
                        |ty| self.resolve_lazy_type(ty),
                    );

                if (ast_circular || semantic_circular)
                    && let Some(ref name) = var_name
                {
                    let message = format!(
                        "'{name}' is referenced directly or indirectly in its own type annotation."
                    );
                    self.error_at_node(var_decl.name, &message, 2502);
                    final_type = TypeId::ANY;
                }
            }

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
            if var_decl.name.is_some() {
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
            let is_const = self.is_const_variable_declaration(decl_idx);
            if self.ctx.no_implicit_any()
                && !sym_already_cached
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
                    if is_ambient || is_const {
                        // TS7005: Ambient and const declarations always emit at the declaration site.
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
                && var_decl.initializer.is_some()
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
            let is_block_scoped = if let Some(ext) = self.ctx.arena.get_extended(decl_idx)
                && let Some(parent) = self.ctx.arena.get(ext.parent)
                && parent.kind == tsz_parser::parser::syntax_kind_ext::VARIABLE_DECLARATION_LIST
            {
                let flags = parent.flags as u32;
                use tsz_parser::parser::node_flags;
                (flags & (node_flags::LET | node_flags::CONST | node_flags::USING)) != 0
            } else {
                false
            };

            // TS2403 only applies to non-block-scoped variables (var)
            if !is_block_scoped {
                if let Some(prev_type) = self.ctx.var_decl_types.get(&sym_id).copied() {
                    // Check if this is a mergeable declaration by looking at the node kind.
                    // Mergeable declarations: namespace/module, enum, class, interface, function.
                    // When these are declared with the same name, they merge instead of conflicting.
                    let is_mergeable_declaration =
                        if let Some(decl_node) = self.ctx.arena.get(decl_idx) {
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
                    // If this is the first time we see this variable in the current check run,
                    // check if it has prior declarations (e.g. in lib.d.ts or earlier in the file)
                    // that establish its type.
                    let mut prior_type_found = None;
                    let symbol_name = self
                        .ctx
                        .binder
                        .get_symbol(sym_id)
                        .map(|s| s.escaped_name.clone());

                    // 1. Check lib contexts for prior declarations (e.g. 'var symbol' in lib.d.ts)
                    // Extract data to avoid holding borrow on self during loop
                    let types = self.ctx.types;
                    let compiler_options = self.ctx.compiler_options.clone();
                    let definition_store = self.ctx.definition_store.clone();
                    let lib_contexts = self.ctx.lib_contexts.clone();
                    let lib_contexts_data: Vec<_> = lib_contexts
                        .iter()
                        .map(|ctx| (ctx.arena.clone(), ctx.binder.clone()))
                        .collect();

                    if let Some(name) = symbol_name {
                        for (arena, binder) in lib_contexts_data {
                            // Lookup by name in lib binder to ensure we find the matching symbol
                            // even if SymbolIds are not perfectly aligned across contexts.
                            if let Some(lib_sym_id) = binder.file_locals.get(&name)
                                && let Some(lib_sym) = binder.get_symbol(lib_sym_id)
                            {
                                for &lib_decl in &lib_sym.declarations {
                                    if lib_decl.is_some()
                                        && CheckerState::enter_cross_arena_delegation()
                                    {
                                        let mut lib_checker =
                                            CheckerState::new_with_shared_def_store(
                                                &arena,
                                                &binder,
                                                types,
                                                "lib.d.ts".to_string(),
                                                compiler_options.clone(),
                                                definition_store.clone(),
                                            );
                                        // Ensure lib checker can resolve types from other lib files
                                        lib_checker.ctx.set_lib_contexts(lib_contexts.clone());

                                        let lib_type = lib_checker.get_type_of_node(lib_decl);
                                        CheckerState::leave_cross_arena_delegation();

                                        // Check compatibility
                                        if !self.are_var_decl_types_compatible(lib_type, final_type)
                                            && let Some(ref name) = var_name
                                        {
                                            self.error_subsequent_variable_declaration(
                                                name, lib_type, final_type, decl_idx,
                                            );
                                        }

                                        prior_type_found =
                                            Some(if let Some(prev) = prior_type_found {
                                                self.refine_var_decl_type(prev, lib_type)
                                            } else {
                                                lib_type
                                            });
                                    }
                                }
                            }
                        }
                    }

                    // 2. Check local declarations (in case of intra-file redeclaration)
                    if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                        for &other_decl in &symbol.declarations {
                            if other_decl == decl_idx {
                                break;
                            }
                            if other_decl.is_some() {
                                let other_type = self.get_type_of_node(other_decl);

                                // Check if other declaration is mergeable (namespace, etc.)
                                let other_node_kind =
                                    self.ctx.arena.get(other_decl).map_or(0, |n| n.kind);
                                let is_other_mergeable = matches!(
                                    other_node_kind,
                                    syntax_kind_ext::MODULE_DECLARATION
                                        | syntax_kind_ext::ENUM_DECLARATION
                                        | syntax_kind_ext::CLASS_DECLARATION
                                        | syntax_kind_ext::INTERFACE_DECLARATION
                                        | syntax_kind_ext::FUNCTION_DECLARATION
                                );

                                // Functions, classes, and enums don't merge with variables,
                                // so they should not establish a "previous variable type" for TS2403.
                                // Only other variables and namespaces (which DO merge with vars) establish this.
                                let establishes_var_type = matches!(
                                    other_node_kind,
                                    syntax_kind_ext::VARIABLE_DECLARATION
                                        | syntax_kind_ext::PARAMETER
                                        | syntax_kind_ext::BINDING_ELEMENT
                                        | syntax_kind_ext::MODULE_DECLARATION
                                );

                                if !establishes_var_type {
                                    continue;
                                }

                                if !is_other_mergeable
                                    && !self.are_var_decl_types_compatible(other_type, final_type)
                                    && let Some(ref name) = var_name
                                {
                                    self.error_subsequent_variable_declaration(
                                        name, other_type, final_type, decl_idx,
                                    );
                                }

                                prior_type_found = Some(if let Some(prev) = prior_type_found {
                                    self.refine_var_decl_type(prev, other_type)
                                } else {
                                    other_type
                                });
                            }
                        }
                    }

                    let type_to_store = if let Some(prior) = prior_type_found {
                        self.refine_var_decl_type(prior, final_type)
                    } else {
                        final_type
                    };
                    self.ctx.var_decl_types.insert(sym_id, type_to_store);
                }
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
            let pattern_type = if var_decl.type_annotation.is_some() {
                self.get_type_from_type_node(var_decl.type_annotation)
            } else if var_decl.initializer.is_some() {
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
            self.check_binding_pattern(var_decl.name, pattern_type, true);

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

    // Destructuring pattern methods (report_empty_array_destructuring_bounds,
    // assign_binding_pattern_symbol_types, record_destructured_binding_group,
    // get_binding_element_type, rest_binding_array_type, is_only_undefined_or_null)
    // are in `state_variable_checking_destructuring.rs`.
}
