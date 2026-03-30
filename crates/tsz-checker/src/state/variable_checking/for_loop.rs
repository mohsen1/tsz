//! For-in / for-of loop variable checking.
//!
//! Extracted from `core.rs` to keep that file focused on
//! general variable declaration checking (`check_variable_declaration`).

use crate::context::TypingRequest;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

#[derive(Clone, Copy, Eq, PartialEq)]
enum ForOfProtocolRole {
    Iterable,
    Iterator,
}

impl ForOfProtocolRole {
    const fn tag(self) -> u8 {
        match self {
            Self::Iterable => 0,
            Self::Iterator => 1,
        }
    }
}

impl<'a> CheckerState<'a> {
    pub(crate) fn resolve_for_of_header_expression_symbol(
        &self,
        idx: NodeIndex,
    ) -> Option<SymbolId> {
        let name = self.ctx.arena.get_identifier_at(idx)?.escaped_text.as_str();
        let mut current = idx;

        while current.is_some() {
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                break;
            }
            let parent = ext.parent;
            let parent_node = self.ctx.arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::FOR_OF_STATEMENT
                && let Some(for_data) = self.ctx.arena.get_for_in_of(parent_node)
                && for_data.expression == current
            {
                let list_node = self.ctx.arena.get(for_data.initializer)?;
                if list_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                    return None;
                }
                let list = self.ctx.arena.get_variable(list_node)?;
                for &decl_idx in &list.declarations.nodes {
                    let decl_node = match self.ctx.arena.get(decl_idx) {
                        Some(node) => node,
                        None => continue,
                    };
                    let var_decl = match self.ctx.arena.get_variable_declaration(decl_node) {
                        Some(decl) => decl,
                        None => continue,
                    };
                    let name_node = match self.ctx.arena.get(var_decl.name) {
                        Some(node) => node,
                        None => continue,
                    };
                    if name_node.kind != SyntaxKind::Identifier as u16 {
                        continue;
                    }
                    let ident = match self.ctx.arena.get_identifier(name_node) {
                        Some(ident) => ident,
                        None => continue,
                    };
                    if ident.escaped_text.as_str() != name {
                        continue;
                    }
                    return self
                        .ctx
                        .binder
                        .get_node_symbol(decl_idx)
                        .or_else(|| self.ctx.binder.get_node_symbol(var_decl.name))
                        .or_else(|| {
                            self.ctx
                                .binder
                                .resolve_identifier(self.ctx.arena, var_decl.name)
                        });
                }
                return None;
            }
            current = parent;
        }

        None
    }

    pub(crate) fn is_in_for_of_header_expression_of_declaration(
        &self,
        usage_idx: NodeIndex,
        decl_idx: NodeIndex,
    ) -> bool {
        let Some(decl_info) = self.ctx.arena.node_info(decl_idx) else {
            return false;
        };
        let decl_list_idx = decl_info.parent;
        let Some(decl_list_node) = self.ctx.arena.get(decl_list_idx) else {
            return false;
        };
        if decl_list_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return false;
        }
        let Some(for_info) = self.ctx.arena.node_info(decl_list_idx) else {
            return false;
        };
        let for_idx = for_info.parent;
        let Some(for_node) = self.ctx.arena.get(for_idx) else {
            return false;
        };
        if for_node.kind != syntax_kind_ext::FOR_OF_STATEMENT {
            return false;
        }
        let Some(for_data) = self.ctx.arena.get_for_in_of(for_node) else {
            return false;
        };

        let mut current = usage_idx;
        while current.is_some() {
            if current == for_data.expression {
                return true;
            }
            if current == for_idx {
                return false;
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            current = ext.parent;
        }

        false
    }

    pub(crate) fn is_deferred_object_like_for_in(&mut self, expr_type: TypeId) -> bool {
        use crate::query_boundaries::dispatch as query;

        if query::is_type_parameter_like(self.ctx.types, expr_type)
            || query::is_object_like_type(self.ctx.types, expr_type)
        {
            return true;
        }

        if let Some((base, _index)) = query::get_index_access_types(self.ctx.types, expr_type) {
            let evaluated_base = self.evaluate_type_with_env(base);
            return query::is_type_parameter_like(self.ctx.types, base)
                || query::is_type_parameter_like(self.ctx.types, evaluated_base)
                || query::is_object_like_type(self.ctx.types, evaluated_base);
        }

        if let Some(members) = query::union_members(self.ctx.types, expr_type) {
            return members
                .iter()
                .any(|&member| self.is_deferred_object_like_for_in(member));
        }

        if let Some(members) = query::intersection_members(self.ctx.types, expr_type) {
            return members
                .iter()
                .any(|&member| self.is_deferred_object_like_for_in(member));
        }

        false
    }

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
            // tsc anchors at the variable name (not the initializer expression).
            if single_declaration && var_decl.initializer.is_some() {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                if is_for_in {
                    self.error_at_node(
                        var_decl.name,
                        diagnostic_messages::THE_VARIABLE_DECLARATION_OF_A_FOR_IN_STATEMENT_CANNOT_HAVE_AN_INITIALIZER,
                        diagnostic_codes::THE_VARIABLE_DECLARATION_OF_A_FOR_IN_STATEMENT_CANNOT_HAVE_AN_INITIALIZER,
                    );
                } else {
                    self.error_at_node(
                        var_decl.name,
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
                    // tsc anchors this error at the `: type` annotation (including
                    // the colon). Our type_annotation node only covers the type
                    // itself (excluding colon). Use the variable name node — its
                    // end position is the colon, giving the closest match to tsc.
                    self.error_at_node(
                        var_decl.name,
                        diagnostic_messages::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_USE_A_TYPE_ANNOTATION,
                        diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_USE_A_TYPE_ANNOTATION,
                    );
                } else if !is_for_in && single_declaration {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.error_at_node(
                        var_decl.name,
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
                    let binding_request = if declared != TypeId::ANY
                        && declared != TypeId::UNKNOWN
                        && declared != TypeId::ERROR
                    {
                        TypingRequest::with_contextual_type(declared)
                    } else {
                        TypingRequest::NONE
                    };
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
                    self.assign_binding_pattern_symbol_types_with_request(
                        var_decl.name,
                        declared,
                        &binding_request,
                    );
                }

                if let Some(sym_id) = self.ctx.binder.get_node_symbol(decl_idx) {
                    self.cache_symbol_type(sym_id, declared);
                } else if let Some(sym_id) = self.ctx.binder.get_node_symbol(var_decl.name) {
                    self.cache_symbol_type(sym_id, declared);
                }
            } else {
                // No type annotation - use element type (with freshness stripped)
                let widened_element_type = if !self.ctx.compiler_options.sound_mode {
                    crate::query_boundaries::common::widen_freshness(self.ctx.types, element_type)
                } else {
                    element_type
                };

                // Assign types for binding patterns (e.g., `for (const [a] of arr)`).
                if let Some(name_node) = self.ctx.arena.get(var_decl.name)
                    && (name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
                {
                    let binding_request = if widened_element_type != TypeId::ANY
                        && widened_element_type != TypeId::UNKNOWN
                        && widened_element_type != TypeId::ERROR
                    {
                        TypingRequest::with_contextual_type(widened_element_type)
                    } else {
                        TypingRequest::NONE
                    };
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
                    self.assign_binding_pattern_symbol_types_with_request(
                        var_decl.name,
                        widened_element_type,
                        &binding_request,
                    );
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

        // Valid types: any, unknown, object (non-primitive), object types, type parameters
        // Invalid types: primitive types (void, null, undefined, number, string, boolean,
        // bigint, symbol) and `never` (tsc reports TS2407 for `never` as well)
        let is_valid = expr_type == TypeId::ANY
            || expr_type == TypeId::UNKNOWN
            || expr_type == TypeId::OBJECT
            || query::is_type_parameter_like(self.ctx.types, expr_type)
            || query::is_object_like_type(self.ctx.types, expr_type)
            || self.is_deferred_object_like_for_in(expr_type)
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
                    || self.is_deferred_object_like_for_in(member)
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
                    || self.is_deferred_object_like_for_in(member)
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

        // TS2487: For-of LHS must be a variable or a property access.
        // Unlike for-in, for-of allows destructuring patterns (array/object literals).
        if is_for_of && let Some(init_node) = self.ctx.arena.get(initializer) {
            let unwrapped = self
                .ctx
                .arena
                .skip_parenthesized_and_assertions(initializer);
            let init_kind = self
                .ctx
                .arena
                .get(unwrapped)
                .map_or(init_node.kind, |n| n.kind);
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            use tsz_parser::parser::syntax_kind_ext;

            if init_kind != SyntaxKind::Identifier as u16
                && init_kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && init_kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                && init_kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                && init_kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            {
                self.error_at_node(
                    initializer,
                    diagnostic_messages::THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS,
                    diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS,
                );
            }
        }

        // For-in specific LHS checks (TS2491, TS2406, TS2405)
        if !is_for_of && let Some(init_node) = self.ctx.arena.get(initializer) {
            // Unwrap parenthesized/satisfies/as wrappers before checking the kind,
            // so `for ((x satisfies string) in obj)` is treated like `for (x in obj)`.
            let unwrapped = self
                .ctx
                .arena
                .skip_parenthesized_and_assertions(initializer);
            let init_kind = self
                .ctx
                .arena
                .get(unwrapped)
                .map_or(init_node.kind, |n| n.kind);
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
        // Also skip for optional chain accesses — TS2777 already covers those.
        if !is_for_of
            && !self.is_optional_chain_access(initializer)
            && let Some(_init_node) = self.ctx.arena.get(initializer)
            && {
                let unwrapped = self
                    .ctx
                    .arena
                    .skip_parenthesized_and_assertions(initializer);
                self.ctx.arena.get(unwrapped).is_some_and(|n| {
                    let k = n.kind;
                    k == SyntaxKind::Identifier as u16
                        || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                })
            }
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
        // For destructuring patterns (array/object literals), set the destructuring
        // target flag so that downstream checks (e.g. TS2698 spread validation in
        // object literals) correctly treat `{ ...x }` as a rest binding, not a spread.
        let is_destructuring_init = self.ctx.arena.get(initializer).is_some_and(|n| {
            n.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || n.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        });
        let prev_destructuring = self.ctx.in_destructuring_target;
        if is_destructuring_init {
            self.ctx.in_destructuring_target = true;
        }
        let var_type = self.get_type_of_assignment_target(initializer);
        self.ctx.in_destructuring_target = prev_destructuring;
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

    pub(crate) fn begin_for_of_self_reference_tracking(
        &mut self,
        decl_list_idx: NodeIndex,
    ) -> usize {
        let Some(list_node) = self.ctx.arena.get(decl_list_idx) else {
            return 0;
        };
        let Some(list) = self.ctx.arena.get_variable(list_node) else {
            return 0;
        };

        let mut seen = FxHashSet::default();
        let mut tracked = 0;
        for &decl_idx in &list.declarations.nodes {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                continue;
            };
            if var_decl.type_annotation.is_some() {
                continue;
            }

            let sym_id = self
                .ctx
                .binder
                .get_node_symbol(decl_idx)
                .or_else(|| self.ctx.binder.get_node_symbol(var_decl.name))
                .or_else(|| {
                    self.ctx
                        .binder
                        .resolve_identifier(self.ctx.arena, var_decl.name)
                });
            let Some(sym_id) = sym_id else {
                continue;
            };

            if seen.insert(sym_id) {
                self.push_symbol_dependency(sym_id, false);
                tracked += 1;
            }
        }

        if tracked > 0 {
            self.ctx.non_closure_circular_return_tracking_depth += 1;
        }

        tracked
    }

    pub(crate) fn end_for_of_self_reference_tracking(&mut self, tracked_symbol_count: usize) {
        if tracked_symbol_count == 0 {
            return;
        }

        for _ in 0..tracked_symbol_count {
            self.pop_symbol_dependency();
        }
        self.ctx.non_closure_circular_return_tracking_depth = self
            .ctx
            .non_closure_circular_return_tracking_depth
            .saturating_sub(1);
    }

    /// TS7022: Detect self-referencing for-of loop variables.
    ///
    /// When `for (var v of v)` is written with `noImplicitAny`, the iterable
    /// expression `v` references the loop variable before it has a type,
    /// creating a circular dependency.  The element type resolves to `any`,
    /// and TS7022 should be emitted on the variable name.
    ///
    /// This also handles indirect circularity where the iterable expression
    /// contains a reference to the declared variable (e.g., via class methods
    /// that return `v`).
    pub(crate) fn check_for_of_self_reference_circularity(
        &mut self,
        decl_list_idx: NodeIndex,
        expression_idx: NodeIndex,
    ) {
        let Some(list_node) = self.ctx.arena.get(decl_list_idx) else {
            return;
        };
        let Some(list) = self.ctx.arena.get_variable(list_node) else {
            return;
        };

        for &decl_idx in &list.declarations.nodes {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                continue;
            };

            // Only applies when there's no type annotation
            if var_decl.type_annotation.is_some() {
                continue;
            }

            // Get the symbol for this declaration
            let sym_id = self
                .ctx
                .binder
                .get_node_symbol(decl_idx)
                .or_else(|| self.ctx.binder.get_node_symbol(var_decl.name))
                .or_else(|| {
                    self.ctx
                        .binder
                        .resolve_identifier(self.ctx.arena, var_decl.name)
                });
            let Some(sym_id) = sym_id else {
                continue;
            };

            // Get the variable name for the diagnostic
            let var_name = self.get_identifier_text_from_idx(var_decl.name);
            let mut circular_return_sites = self.take_pending_circular_return_sites(sym_id);
            for site_idx in
                self.collect_for_of_protocol_circular_return_sites(expression_idx, sym_id)
            {
                if !circular_return_sites.contains(&site_idx) {
                    circular_return_sites.push(site_idx);
                }
            }
            let has_direct_reference = self.expression_references_symbol(expression_idx, sym_id);
            let has_name_reference = var_name.as_ref().is_some_and(|name| {
                self.expression_references_identifier_name(expression_idx, name)
            });
            if circular_return_sites.is_empty() && !has_direct_reference && !has_name_reference {
                continue;
            }

            if let Some(name) = var_name {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node_msg(
                    var_decl.name,
                    diagnostic_codes::IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION_AND_IS_REFERE,
                    &[&name],
                );
                for site_idx in circular_return_sites {
                    self.emit_circular_return_site_diagnostic(
                        site_idx,
                        Some(name.as_str()),
                        var_decl.name,
                        expression_idx,
                    );
                }
            }
        }
    }

    fn collect_for_of_protocol_circular_return_sites(
        &mut self,
        expr_idx: NodeIndex,
        target_sym: SymbolId,
    ) -> Vec<NodeIndex> {
        let mut sites = Vec::new();
        let mut visited_symbols = FxHashSet::default();
        let mut visited_holders = FxHashSet::default();
        self.collect_for_of_protocol_sites_from_expression(
            expr_idx,
            target_sym,
            ForOfProtocolRole::Iterable,
            None,
            false,
            &mut sites,
            &mut visited_symbols,
            &mut visited_holders,
        );
        sites
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_for_of_protocol_sites_from_expression(
        &mut self,
        expr_idx: NodeIndex,
        target_sym: SymbolId,
        role: ForOfProtocolRole,
        owner_idx: Option<NodeIndex>,
        allow_function_returns: bool,
        sites: &mut Vec<NodeIndex>,
        visited_symbols: &mut FxHashSet<(SymbolId, u8)>,
        visited_holders: &mut FxHashSet<(NodeIndex, u8)>,
    ) {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return;
        };

        if node.kind == SyntaxKind::ThisKeyword as u16
            && role == ForOfProtocolRole::Iterator
            && let Some(owner_idx) = owner_idx
        {
            self.inspect_for_of_protocol_holder(
                owner_idx,
                target_sym,
                role,
                sites,
                visited_symbols,
                visited_holders,
            );
            return;
        }

        if node.kind == SyntaxKind::Identifier as u16 {
            let sym_id = self
                .resolve_for_of_header_expression_symbol(expr_idx)
                .or_else(|| self.resolve_identifier_symbol_without_tracking(expr_idx));
            if let Some(sym_id) = sym_id {
                self.collect_for_of_protocol_sites_from_symbol(
                    sym_id,
                    target_sym,
                    role,
                    allow_function_returns,
                    sites,
                    visited_symbols,
                    visited_holders,
                );
                return;
            }
        }

        if matches!(
            node.kind,
            syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::CLASS_EXPRESSION
                | syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        ) {
            self.inspect_for_of_protocol_holder(
                expr_idx,
                target_sym,
                role,
                sites,
                visited_symbols,
                visited_holders,
            );
            return;
        }

        if matches!(
            node.kind,
            syntax_kind_ext::CALL_EXPRESSION | syntax_kind_ext::NEW_EXPRESSION
        ) && let Some(call) = self.ctx.arena.get_call_expr(node)
        {
            self.collect_for_of_protocol_sites_from_expression(
                call.expression,
                target_sym,
                role,
                owner_idx,
                node.kind == syntax_kind_ext::CALL_EXPRESSION,
                sites,
                visited_symbols,
                visited_holders,
            );
            return;
        }

        for child_idx in self.ctx.arena.get_children(expr_idx) {
            self.collect_for_of_protocol_sites_from_expression(
                child_idx,
                target_sym,
                role,
                owner_idx,
                false,
                sites,
                visited_symbols,
                visited_holders,
            );
        }
    }

    fn collect_for_of_protocol_sites_from_symbol(
        &mut self,
        sym_id: SymbolId,
        target_sym: SymbolId,
        role: ForOfProtocolRole,
        allow_function_returns: bool,
        sites: &mut Vec<NodeIndex>,
        visited_symbols: &mut FxHashSet<(SymbolId, u8)>,
        visited_holders: &mut FxHashSet<(NodeIndex, u8)>,
    ) {
        if !visited_symbols.insert((sym_id, role.tag())) {
            return;
        }

        let Some(declarations) = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .map(|symbol| symbol.declarations.clone())
        else {
            return;
        };

        for decl_idx in declarations {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };

            if matches!(
                decl_node.kind,
                syntax_kind_ext::CLASS_DECLARATION
                    | syntax_kind_ext::CLASS_EXPRESSION
                    | syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            ) {
                self.inspect_for_of_protocol_holder(
                    decl_idx,
                    target_sym,
                    role,
                    sites,
                    visited_symbols,
                    visited_holders,
                );
                continue;
            }

            if let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node)
                && var_decl.initializer.is_some()
            {
                self.collect_for_of_protocol_sites_from_expression(
                    var_decl.initializer,
                    target_sym,
                    role,
                    None,
                    false,
                    sites,
                    visited_symbols,
                    visited_holders,
                );
                continue;
            }

            if allow_function_returns
                && let Some(func) = self.ctx.arena.get_function(decl_node)
                && func.body.is_some()
            {
                self.inspect_function_like_protocol_returns(
                    func.body,
                    decl_idx,
                    None,
                    Some(role),
                    target_sym,
                    sites,
                    visited_symbols,
                    visited_holders,
                );
            }
        }
    }

    fn inspect_for_of_protocol_holder(
        &mut self,
        holder_idx: NodeIndex,
        target_sym: SymbolId,
        role: ForOfProtocolRole,
        sites: &mut Vec<NodeIndex>,
        visited_symbols: &mut FxHashSet<(SymbolId, u8)>,
        visited_holders: &mut FxHashSet<(NodeIndex, u8)>,
    ) {
        if !visited_holders.insert((holder_idx, role.tag())) {
            return;
        }

        let Some(holder_node) = self.ctx.arena.get(holder_idx) else {
            return;
        };

        if let Some(class) = self.ctx.arena.get_class(holder_node) {
            for &member_idx in &class.members.nodes {
                self.inspect_for_of_protocol_member(
                    member_idx,
                    holder_idx,
                    target_sym,
                    role,
                    sites,
                    visited_symbols,
                    visited_holders,
                );
            }
            return;
        }

        if holder_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            && let Some(object_literal) = self.ctx.arena.get_literal_expr(holder_node)
        {
            for &member_idx in &object_literal.elements.nodes {
                self.inspect_for_of_protocol_member(
                    member_idx,
                    holder_idx,
                    target_sym,
                    role,
                    sites,
                    visited_symbols,
                    visited_holders,
                );
            }
        }
    }

    fn inspect_for_of_protocol_member(
        &mut self,
        member_idx: NodeIndex,
        owner_idx: NodeIndex,
        target_sym: SymbolId,
        role: ForOfProtocolRole,
        sites: &mut Vec<NodeIndex>,
        visited_symbols: &mut FxHashSet<(SymbolId, u8)>,
        visited_holders: &mut FxHashSet<(NodeIndex, u8)>,
    ) {
        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        match member_node.kind {
            syntax_kind_ext::METHOD_DECLARATION => {
                let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                    return;
                };
                let Some(name) = self.get_property_name_resolved(method.name) else {
                    return;
                };
                if !self.member_matches_for_of_protocol_role(&name, role) {
                    return;
                }
                self.inspect_function_like_protocol_returns(
                    method.body,
                    method.body,
                    Some(owner_idx),
                    self.next_protocol_role(name.as_str(), role),
                    target_sym,
                    sites,
                    visited_symbols,
                    visited_holders,
                );
            }
            syntax_kind_ext::GET_ACCESSOR => {
                let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                    return;
                };
                let Some(name) = self.get_property_name_resolved(accessor.name) else {
                    return;
                };
                if !self.member_matches_for_of_protocol_role(&name, role) {
                    return;
                }
                self.inspect_function_like_protocol_returns(
                    accessor.body,
                    accessor.body,
                    Some(owner_idx),
                    self.next_protocol_role(name.as_str(), role),
                    target_sym,
                    sites,
                    visited_symbols,
                    visited_holders,
                );
            }
            syntax_kind_ext::PROPERTY_DECLARATION => {
                let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                    return;
                };
                let Some(name) = self.get_property_name_resolved(prop.name) else {
                    return;
                };
                if !self.member_matches_for_of_protocol_role(&name, role) {
                    return;
                }
                self.inspect_function_like_protocol_initializer(
                    prop.initializer,
                    owner_idx,
                    name.as_str(),
                    role,
                    target_sym,
                    sites,
                    visited_symbols,
                    visited_holders,
                );
            }
            syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let Some(prop) = self.ctx.arena.get_property_assignment(member_node) else {
                    return;
                };
                let Some(name) = self.get_property_name_resolved(prop.name) else {
                    return;
                };
                if !self.member_matches_for_of_protocol_role(&name, role) {
                    return;
                }
                self.inspect_function_like_protocol_initializer(
                    prop.initializer,
                    owner_idx,
                    name.as_str(),
                    role,
                    target_sym,
                    sites,
                    visited_symbols,
                    visited_holders,
                );
            }
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn inspect_function_like_protocol_initializer(
        &mut self,
        initializer_idx: NodeIndex,
        owner_idx: NodeIndex,
        member_name: &str,
        role: ForOfProtocolRole,
        target_sym: SymbolId,
        sites: &mut Vec<NodeIndex>,
        visited_symbols: &mut FxHashSet<(SymbolId, u8)>,
        visited_holders: &mut FxHashSet<(NodeIndex, u8)>,
    ) {
        let initializer_idx = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(initializer_idx);
        let Some(init_node) = self.ctx.arena.get(initializer_idx) else {
            return;
        };
        if let Some(func) = self.ctx.arena.get_function(init_node)
            && func.body.is_some()
        {
            self.inspect_function_like_protocol_returns(
                func.body,
                initializer_idx,
                Some(owner_idx),
                self.next_protocol_role(member_name, role),
                target_sym,
                sites,
                visited_symbols,
                visited_holders,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn inspect_function_like_protocol_returns(
        &mut self,
        body_idx: NodeIndex,
        diagnostic_site_idx: NodeIndex,
        owner_idx: Option<NodeIndex>,
        next_role: Option<ForOfProtocolRole>,
        target_sym: SymbolId,
        sites: &mut Vec<NodeIndex>,
        visited_symbols: &mut FxHashSet<(SymbolId, u8)>,
        visited_holders: &mut FxHashSet<(NodeIndex, u8)>,
    ) {
        if body_idx.is_none() {
            return;
        }

        let mut return_exprs = Vec::new();
        self.collect_return_expressions_in_function_body(body_idx, &mut return_exprs);

        let mut has_circular_return = false;
        for expr_idx in return_exprs {
            if self.initializer_has_non_deferred_self_reference(expr_idx, target_sym) {
                has_circular_return = true;
            }
            if let Some(next_role) = next_role {
                self.collect_for_of_protocol_sites_from_expression(
                    expr_idx,
                    target_sym,
                    next_role,
                    owner_idx,
                    false,
                    sites,
                    visited_symbols,
                    visited_holders,
                );
            }
        }

        if has_circular_return && !sites.contains(&diagnostic_site_idx) {
            sites.push(diagnostic_site_idx);
        }
    }

    fn collect_return_expressions_in_function_body(
        &self,
        body_idx: NodeIndex,
        return_exprs: &mut Vec<NodeIndex>,
    ) {
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };

        if body_node.kind != syntax_kind_ext::BLOCK {
            return_exprs.push(body_idx);
            return;
        }

        if let Some(block) = self.ctx.arena.get_block(body_node) {
            for &stmt_idx in &block.statements.nodes {
                self.collect_return_expressions_in_statement(stmt_idx, return_exprs);
            }
        }
    }

    fn collect_return_expressions_in_statement(
        &self,
        stmt_idx: NodeIndex,
        return_exprs: &mut Vec<NodeIndex>,
    ) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.ctx.arena.get_return_statement(node)
                    && ret.expression.is_some()
                {
                    return_exprs.push(ret.expression);
                }
            }
            syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &stmt in &block.statements.nodes {
                        self.collect_return_expressions_in_statement(stmt, return_exprs);
                    }
                }
            }
            syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_data) = self.ctx.arena.get_if_statement(node) {
                    self.collect_return_expressions_in_statement(
                        if_data.then_statement,
                        return_exprs,
                    );
                    if if_data.else_statement.is_some() {
                        self.collect_return_expressions_in_statement(
                            if_data.else_statement,
                            return_exprs,
                        );
                    }
                }
            }
            syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch_data) = self.ctx.arena.get_switch(node)
                    && let Some(case_block_node) = self.ctx.arena.get(switch_data.case_block)
                    && let Some(case_block) = self.ctx.arena.get_block(case_block_node)
                {
                    for &clause_idx in &case_block.statements.nodes {
                        if let Some(clause_node) = self.ctx.arena.get(clause_idx)
                            && let Some(clause) = self.ctx.arena.get_case_clause(clause_node)
                        {
                            for &stmt in &clause.statements.nodes {
                                self.collect_return_expressions_in_statement(stmt, return_exprs);
                            }
                        }
                    }
                }
            }
            syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.ctx.arena.get_try(node) {
                    self.collect_return_expressions_in_statement(try_data.try_block, return_exprs);
                    if try_data.catch_clause.is_some() {
                        self.collect_return_expressions_in_statement(
                            try_data.catch_clause,
                            return_exprs,
                        );
                    }
                    if try_data.finally_block.is_some() {
                        self.collect_return_expressions_in_statement(
                            try_data.finally_block,
                            return_exprs,
                        );
                    }
                }
            }
            syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_data) = self.ctx.arena.get_catch_clause(node) {
                    self.collect_return_expressions_in_statement(catch_data.block, return_exprs);
                }
            }
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    self.collect_return_expressions_in_statement(loop_data.statement, return_exprs);
                }
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_for_in_of(node) {
                    self.collect_return_expressions_in_statement(loop_data.statement, return_exprs);
                }
            }
            syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled) = self.ctx.arena.get_labeled_statement(node) {
                    self.collect_return_expressions_in_statement(labeled.statement, return_exprs);
                }
            }
            _ => {}
        }
    }

    fn member_matches_for_of_protocol_role(
        &self,
        member_name: &str,
        role: ForOfProtocolRole,
    ) -> bool {
        match role {
            ForOfProtocolRole::Iterable => {
                matches!(member_name, "[Symbol.iterator]" | "[Symbol.asyncIterator]")
            }
            ForOfProtocolRole::Iterator => member_name == "next",
        }
    }

    fn next_protocol_role(
        &self,
        member_name: &str,
        role: ForOfProtocolRole,
    ) -> Option<ForOfProtocolRole> {
        match role {
            ForOfProtocolRole::Iterable
                if matches!(member_name, "[Symbol.iterator]" | "[Symbol.asyncIterator]") =>
            {
                Some(ForOfProtocolRole::Iterator)
            }
            _ => None,
        }
    }

    /// Check if an expression AST subtree contains a reference to the given symbol.
    fn expression_references_symbol(
        &self,
        node_idx: NodeIndex,
        target_sym: tsz_binder::SymbolId,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        // Check if this node is an identifier referencing the target symbol
        if node.kind == SyntaxKind::Identifier as u16 {
            let ref_sym = self
                .resolve_for_of_header_expression_symbol(node_idx)
                .or_else(|| self.resolve_identifier_symbol_without_tracking(node_idx));
            if ref_sym == Some(target_sym) {
                return true;
            }
        }

        if matches!(
            node.kind,
            syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::FUNCTION_EXPRESSION
                | syntax_kind_ext::ARROW_FUNCTION
                | syntax_kind_ext::METHOD_DECLARATION
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::CLASS_EXPRESSION
        ) {
            return false;
        }

        // Recurse into children
        for child_idx in self.ctx.arena.get_children(node_idx) {
            if self.expression_references_symbol(child_idx, target_sym) {
                return true;
            }
        }

        false
    }

    fn expression_references_identifier_name(
        &self,
        node_idx: NodeIndex,
        target_name: &str,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        if node.kind == SyntaxKind::Identifier as u16
            && self
                .ctx
                .arena
                .get_identifier(node)
                .is_some_and(|ident| ident.escaped_text.as_str() == target_name)
        {
            return true;
        }

        if matches!(
            node.kind,
            syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::FUNCTION_EXPRESSION
                | syntax_kind_ext::ARROW_FUNCTION
                | syntax_kind_ext::METHOD_DECLARATION
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::CLASS_EXPRESSION
        ) {
            return false;
        }

        for child_idx in self.ctx.arena.get_children(node_idx) {
            if self.expression_references_identifier_name(child_idx, target_name) {
                return true;
            }
        }

        false
    }
}
