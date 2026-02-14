//! Type Checking Module
//!
//! This module contains type checking methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Assignment checking
//! - Expression validation
//! - Statement checking
//! - Declaration validation
//!
//! This module extends CheckerState with additional methods for type-related
//! validation operations, providing cleaner APIs for common patterns.

use crate::query_boundaries::type_checking as query;
use crate::state::{CheckerState, ComputedKey, MAX_TREE_WALK_ITERATIONS, PropertyKey};
use rustc_hash::FxHashSet;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

// =============================================================================
// Type Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Utility Methods
    // =========================================================================

    // =========================================================================
    // AST Traversal Helper Methods (Consolidate Duplication)
    // =========================================================================

    /// Get modifiers from a declaration node, consolidating duplicated match statements.
    ///
    /// This helper eliminates the repeated pattern of matching declaration kinds
    /// and extracting their modifiers. Used in has_export_modifier and similar functions.
    pub(crate) fn get_declaration_modifiers(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> Option<&tsz_parser::parser::NodeList> {
        use tsz_parser::parser::syntax_kind_ext;
        match node.kind {
            syntax_kind_ext::FUNCTION_DECLARATION => self
                .ctx
                .arena
                .get_function(node)
                .and_then(|f| f.modifiers.as_ref()),
            syntax_kind_ext::CLASS_DECLARATION => self
                .ctx
                .arena
                .get_class(node)
                .and_then(|c| c.modifiers.as_ref()),
            syntax_kind_ext::VARIABLE_STATEMENT => self
                .ctx
                .arena
                .get_variable(node)
                .and_then(|v| v.modifiers.as_ref()),
            syntax_kind_ext::INTERFACE_DECLARATION => self
                .ctx
                .arena
                .get_interface(node)
                .and_then(|i| i.modifiers.as_ref()),
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => self
                .ctx
                .arena
                .get_type_alias(node)
                .and_then(|t| t.modifiers.as_ref()),
            syntax_kind_ext::ENUM_DECLARATION => self
                .ctx
                .arena
                .get_enum(node)
                .and_then(|e| e.modifiers.as_ref()),
            syntax_kind_ext::MODULE_DECLARATION => self
                .ctx
                .arena
                .get_module(node)
                .and_then(|m| m.modifiers.as_ref()),
            syntax_kind_ext::IMPORT_EQUALS_DECLARATION => self
                .ctx
                .arena
                .get_import_decl(node)
                .and_then(|i| i.modifiers.as_ref()),
            _ => None,
        }
    }

    /// Get the name node from a class member node.
    ///
    /// This helper eliminates the repeated pattern of matching member kinds
    /// and extracting their name nodes.
    pub(crate) fn get_member_name_node(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;
        match node.kind {
            syntax_kind_ext::PROPERTY_DECLARATION => {
                self.ctx.arena.get_property_decl(node).map(|p| p.name)
            }
            syntax_kind_ext::METHOD_DECLARATION => {
                self.ctx.arena.get_method_decl(node).map(|m| m.name)
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                self.ctx.arena.get_accessor(node).map(|a| a.name)
            }
            syntax_kind_ext::PROPERTY_SIGNATURE | syntax_kind_ext::METHOD_SIGNATURE => {
                self.ctx.arena.get_signature(node).map(|s| s.name)
            }
            _ => None,
        }
    }

    /// Get identifier text from a node, if it's an identifier.
    ///
    /// This helper eliminates the repeated pattern of checking for identifier
    /// and extracting escaped_text.
    pub(crate) fn get_identifier_text(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> Option<String> {
        self.ctx
            .arena
            .get_identifier(node)
            .map(|ident| ident.escaped_text.clone())
    }

    /// Get identifier text from a node index, if it's an identifier.
    pub(crate) fn get_identifier_text_from_idx(&self, idx: NodeIndex) -> Option<String> {
        self.ctx
            .arena
            .get(idx)
            .and_then(|node| self.get_identifier_text(&node))
    }

    /// Generic helper to check if modifiers include a specific keyword.
    ///
    /// This eliminates the duplicated pattern of checking for specific modifier keywords.
    pub(crate) fn has_modifier_kind(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
        kind: SyntaxKind,
    ) -> bool {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx)
                    && mod_node.kind == kind as u16
                {
                    return true;
                }
            }
        }
        false
    }

    // =========================================================================
    // Member and Declaration Validation
    // =========================================================================

    /// Check a class member name for computed property validation.
    ///
    /// This dispatches to check_computed_property_name for properties,
    /// methods, and accessors that use computed names.
    pub(crate) fn check_class_member_name(&mut self, member_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        // Use helper to get member name node
        if let Some(name_idx) = self.get_member_name_node(node) {
            self.check_computed_property_name(name_idx);
        }
    }

    /// Check for duplicate enum member names.
    ///
    /// This function validates that all enum members have unique names.
    /// If duplicates are found, it emits TS2308 errors for each duplicate.
    ///
    /// ## Duplicate Detection:
    /// - Collects all member names into a HashSet
    /// - Reports error for each name that appears more than once
    /// - Error TS2308: "Duplicate identifier '{name}'"
    pub(crate) fn check_enum_duplicate_members(&mut self, enum_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;

        let Some(enum_node) = self.ctx.arena.get(enum_idx) else {
            return;
        };
        let Some(enum_decl) = self.ctx.arena.get_enum(enum_node) else {
            return;
        };

        let mut seen_names = rustc_hash::FxHashSet::default();
        for &member_idx in &enum_decl.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
                continue;
            };

            // Get the member name
            let Some(name_node) = self.ctx.arena.get(member.name) else {
                continue;
            };
            let name_text = if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                ident.escaped_text.clone()
            } else {
                continue;
            };

            // Check for duplicate
            if seen_names.contains(&name_text) {
                self.error_at_node_msg(
                    member.name,
                    diagnostic_codes::DUPLICATE_IDENTIFIER,
                    &[&name_text],
                );
            } else {
                seen_names.insert(name_text);
            }
        }
    }

    // =========================================================================
    // Private Identifier Validation
    // =========================================================================

    /// Check that a private identifier expression is valid.
    ///
    /// Validates that private field/property access is used correctly:
    /// - The private identifier must be declared in a class
    /// - The object type must be assignable to the declaring class type
    /// - Emits appropriate errors for invalid private identifier usage
    ///
    /// ## Parameters:
    /// - `name_idx`: The private identifier node index
    /// - `rhs_type`: The type of the object on which the private identifier is accessed
    ///
    /// ## Validation:
    /// - Resolves private identifier symbols
    /// - Checks if the object type is assignable to the declaring class
    /// - Handles shadowed private members (from derived classes)
    /// - Emits property does not exist errors for invalid access
    pub(crate) fn check_private_identifier_in_expression(
        &mut self,
        name_idx: NodeIndex,
        rhs_type: TypeId,
    ) {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return;
        };
        let property_name = ident.escaped_text.clone();

        let (symbols, saw_class_scope) = self.resolve_private_identifier_symbols(name_idx);
        if symbols.is_empty() {
            if saw_class_scope {
                // Use original rhs_type for error message to preserve nominal identity (e.g., D<string>)
                self.error_property_not_exist_at(&property_name, rhs_type, name_idx);
            }
            return;
        }

        // Evaluate for type checking but keep original for error messages
        let evaluated_rhs_type = self.evaluate_application_type(rhs_type);
        if evaluated_rhs_type == TypeId::ANY
            || evaluated_rhs_type == TypeId::ERROR
            || evaluated_rhs_type == TypeId::UNKNOWN
        {
            return;
        }

        let declaring_type = match self.private_member_declaring_type(symbols[0]) {
            Some(ty) => ty,
            None => {
                if saw_class_scope {
                    // Use original rhs_type for error message to preserve nominal identity
                    self.error_property_not_exist_at(&property_name, rhs_type, name_idx);
                }
                return;
            }
        };

        if !self.is_assignable_to(evaluated_rhs_type, declaring_type) {
            let shadowed = symbols.iter().skip(1).any(|sym_id| {
                self.private_member_declaring_type(*sym_id)
                    .map(|ty| self.is_assignable_to(evaluated_rhs_type, ty))
                    .unwrap_or(false)
            });
            if shadowed {
                return;
            }

            // Use original rhs_type for error message to preserve nominal identity
            self.error_property_not_exist_at(&property_name, rhs_type, name_idx);
        }
    }

    // =========================================================================
    // Type Name Validation
    // =========================================================================

    /// Check a parameter's type annotation for missing type names.
    ///
    /// Validates that type references within a parameter's type annotation
    /// can be resolved. This helps catch typos and undefined types.
    ///
    /// ## Parameters:
    /// - `param_idx`: The parameter node index to check
    pub(crate) fn check_parameter_type_for_missing_names(&mut self, param_idx: NodeIndex) {
        let Some(param_node) = self.ctx.arena.get(param_idx) else {
            return;
        };
        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
            return;
        };
        if !param.type_annotation.is_none() {
            self.check_type_for_missing_names(param.type_annotation);
        }
    }

    /// Check a tuple element for missing type names.
    ///
    /// Validates that type references within a tuple element can be resolved.
    /// Handles both named tuple members and regular tuple elements.
    ///
    /// ## Parameters:
    /// - `elem_idx`: The tuple element node index to check
    pub(crate) fn check_tuple_element_for_missing_names(&mut self, elem_idx: NodeIndex) {
        let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
            return;
        };
        if elem_node.kind == syntax_kind_ext::NAMED_TUPLE_MEMBER
            && let Some(member) = self.ctx.arena.get_named_tuple_member(elem_node)
        {
            self.check_type_for_missing_names(member.type_node);
        }
    }

    /// Check type parameters for missing type names.
    ///
    /// Iterates through a list of type parameters and validates that
    /// their constraints and defaults reference valid types.
    ///
    /// ## Parameters:
    /// - `type_parameters`: The type parameter list to check
    pub(crate) fn check_type_parameters_for_missing_names(
        &mut self,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
    ) {
        let Some(list) = type_parameters else {
            return;
        };
        for &param_idx in &list.nodes {
            self.check_type_parameter_node_for_missing_names(param_idx);
        }
    }

    /// Check for duplicate type parameter names in a type parameter list (TS2300).
    ///
    /// This is used for type parameter lists that are NOT processed through
    /// `push_type_parameters` during the checking pass, such as interface method
    /// signatures and function type expressions.
    pub(crate) fn check_duplicate_type_parameters(
        &mut self,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
    ) {
        let Some(list) = type_parameters else {
            return;
        };
        let mut seen = FxHashSet::default();
        for &param_idx in &list.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(param.name) else {
                continue;
            };
            let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                continue;
            };
            let name = &ident.escaped_text;
            if !seen.insert(name.clone()) {
                self.error_at_node_msg(
                    param.name,
                    crate::diagnostics::diagnostic_codes::DUPLICATE_IDENTIFIER,
                    &[name],
                );
            }
        }
    }

    /// Check a single type parameter node for missing type names.
    ///
    /// Validates that the constraint and default type of a type parameter
    /// reference valid types.
    ///
    /// ## Parameters:
    /// - `param_idx`: The type parameter node index to check
    pub(crate) fn check_type_parameter_node_for_missing_names(&mut self, param_idx: NodeIndex) {
        let Some(param_node) = self.ctx.arena.get(param_idx) else {
            return;
        };
        let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
            return;
        };

        // Check constraint type
        if !param.constraint.is_none() {
            self.check_type_for_missing_names(param.constraint);
        }

        // Check default type
        if !param.default.is_none() {
            self.check_type_for_missing_names(param.default);
        }
    }

    // =========================================================================
    // Parameter Properties Validation
    // =========================================================================

    /// Check a type node for parameter properties.
    ///
    /// Recursively walks a type node and checks function/constructor types
    /// and type literals for parameter properties (public/private/protected/readonly
    /// parameters in class constructors).
    ///
    /// ## Parameters:
    /// - `type_idx`: The type node index to check
    ///
    /// ## Validation:
    /// - Checks function/constructor types for parameter property modifiers
    /// - Checks type literals for call/construct signatures with parameter properties
    /// - Recursively checks nested types (arrays, unions, intersections, etc.)
    pub(crate) fn check_type_for_parameter_properties(&mut self, type_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };

        // Check if this is a function type or constructor type
        if node.kind == syntax_kind_ext::FUNCTION_TYPE
            || node.kind == syntax_kind_ext::CONSTRUCTOR_TYPE
        {
            if let Some(func_type) = self.ctx.arena.get_function_type(node) {
                // Check each parameter for parameter property modifiers
                self.check_parameter_properties(&func_type.parameters.nodes);
                for &param_idx in &func_type.parameters.nodes {
                    if let Some(param_node) = self.ctx.arena.get(param_idx)
                        && let Some(param) = self.ctx.arena.get_parameter(param_node)
                    {
                        if !param.type_annotation.is_none() {
                            self.check_type_for_parameter_properties(param.type_annotation);
                        }
                        self.maybe_report_implicit_any_parameter(param, false);
                    }
                }
                // Recursively check the return type
                self.check_type_for_parameter_properties(func_type.type_annotation);
            }
        }
        // Check type literals (object types) for call/construct signatures and duplicate properties
        else if node.kind == syntax_kind_ext::TYPE_LITERAL {
            if let Some(type_lit) = self.ctx.arena.get_type_literal(node) {
                self.check_type_literal_duplicate_properties(&type_lit.members.nodes);
                for &member_idx in &type_lit.members.nodes {
                    self.check_type_member_for_parameter_properties(member_idx);
                }
            }
        }
        // Recursively check array types, union types, intersection types, etc.
        else if node.kind == syntax_kind_ext::ARRAY_TYPE {
            if let Some(arr) = self.ctx.arena.get_array_type(node) {
                self.check_type_for_parameter_properties(arr.element_type);
            }
        } else if node.kind == syntax_kind_ext::UNION_TYPE
            || node.kind == syntax_kind_ext::INTERSECTION_TYPE
        {
            if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                for &type_idx in &composite.types.nodes {
                    self.check_type_for_parameter_properties(type_idx);
                }
            }
        } else if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            && let Some(paren) = self.ctx.arena.get_wrapped_type(node)
        {
            self.check_type_for_parameter_properties(paren.type_node);
        } else if node.kind == syntax_kind_ext::TYPE_PREDICATE
            && let Some(pred) = self.ctx.arena.get_type_predicate(node)
            && !pred.type_node.is_none()
        {
            self.check_type_for_parameter_properties(pred.type_node);
        }
    }

    /// Check for duplicate property names in type literals (TS2300).
    /// e.g. `{ a: string; a: number; }` has duplicate property `a`.
    pub(crate) fn check_type_literal_duplicate_properties(&mut self, members: &[NodeIndex]) {
        use crate::diagnostics::diagnostic_codes;
        use tsz_parser::parser::syntax_kind_ext::{METHOD_SIGNATURE, PROPERTY_SIGNATURE};

        let mut seen: rustc_hash::FxHashMap<String, NodeIndex> = rustc_hash::FxHashMap::default();

        for &member_idx in members {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind != PROPERTY_SIGNATURE && member_node.kind != METHOD_SIGNATURE {
                continue;
            }

            let Some(name) = self.get_member_name(member_idx) else {
                continue;
            };

            if let Some(&prev_idx) = seen.get(&name) {
                // Report duplicate on the second occurrence
                let name_idx = if let Some(sig) = self.ctx.arena.get_signature(member_node) {
                    sig.name
                } else {
                    member_idx
                };
                self.error_at_node(
                    name_idx,
                    &format!("Duplicate identifier '{}'.", name),
                    diagnostic_codes::DUPLICATE_IDENTIFIER,
                );
                // Also mark the first occurrence
                if let Some(prev_node) = self.ctx.arena.get(prev_idx) {
                    let prev_name_idx = if let Some(sig) = self.ctx.arena.get_signature(prev_node) {
                        sig.name
                    } else {
                        prev_idx
                    };
                    self.error_at_node(
                        prev_name_idx,
                        &format!("Duplicate identifier '{}'.", name),
                        diagnostic_codes::DUPLICATE_IDENTIFIER,
                    );
                }
            } else {
                seen.insert(name, member_idx);
            }
        }
    }

    // =========================================================================
    // Destructuring Validation
    // =========================================================================

    /// Check a binding pattern for destructuring validity.
    ///
    /// Validates that destructuring patterns (object/array destructuring) are applied
    /// to valid types and that default values are assignable to their expected types.
    ///
    /// ## Parameters:
    /// - `pattern_idx`: The binding pattern node index to check
    /// - `pattern_type`: The type being destructured
    ///
    /// ## Validation:
    /// - Checks array destructuring target types (TS2461)
    /// - Validates default value assignability for binding elements
    /// - Recursively checks nested binding patterns
    pub(crate) fn check_binding_pattern(&mut self, pattern_idx: NodeIndex, pattern_type: TypeId) {
        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };

        let Some(pattern_data) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        // Traverse binding elements
        let pattern_kind = pattern_node.kind;

        // Note: Array destructuring iterability (TS2488) is checked by the caller
        // (state_checking.rs) via check_destructuring_iterability before invoking
        // check_binding_pattern, so we do NOT call check_array_destructuring_target_type
        // here to avoid duplicate TS2488 errors.

        for (i, &element_idx) in pattern_data.elements.nodes.iter().enumerate() {
            self.check_binding_element(element_idx, pattern_kind, i, pattern_type);
        }
    }

    /// Check a single binding element for default value assignability.
    ///
    /// Validates that default values in destructuring patterns are assignable
    /// to the expected property/element type.
    ///
    /// ## Parameters:
    /// - `element_idx`: The binding element node index to check
    /// - `pattern_kind`: The kind of binding pattern (object or array)
    /// - `element_index`: The index of this element in the pattern
    /// - `parent_type`: The type being destructured
    ///
    /// ## Validation:
    /// - Checks computed property names for unresolved identifiers
    /// - Validates default value type assignability
    /// - Recursively checks nested binding patterns
    pub(crate) fn check_binding_element(
        &mut self,
        element_idx: NodeIndex,
        pattern_kind: u16,
        element_index: usize,
        parent_type: TypeId,
    ) {
        let Some(element_node) = self.ctx.arena.get(element_idx) else {
            return;
        };

        // Handle holes in array destructuring: [a, , b]
        if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
            return;
        }

        let Some(element_data) = self.ctx.arena.get_binding_element(element_node) else {
            return;
        };

        // Check computed property name expression for unresolved identifiers (TS2304)
        // e.g., in `{[z]: x}` where `z` is undefined
        if !element_data.property_name.is_none() {
            self.check_computed_property_name(element_data.property_name);
        }

        // Get the expected type for this binding element from the parent type
        let element_type = if parent_type != TypeId::ANY {
            // For object binding patterns, look up the property type
            // For array binding patterns, look up the tuple element type
            self.get_binding_element_type(pattern_kind, element_index, parent_type, element_data)
        } else {
            TypeId::ANY
        };

        // Check if there's a default value (initializer)
        if !element_data.initializer.is_none() && element_type != TypeId::ANY {
            let default_value_type = self.get_type_of_node(element_data.initializer);
            let _ = self.check_assignable_or_report(
                default_value_type,
                element_type,
                element_data.initializer,
            );
        }

        // If the name is a nested binding pattern, recursively check it
        if let Some(name_node) = self.ctx.arena.get(element_data.name)
            && (name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
        {
            self.check_binding_pattern(element_data.name, element_type);
        }
    }

    // =========================================================================
    // Import Validation
    // =========================================================================
}

// =============================================================================
// Statement Validation
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Return Statement Validation
    // =========================================================================

    /// Check a return statement for validity.
    ///
    /// Validates that:
    /// - The return expression type is assignable to the function's return type
    /// - Await expressions are only used in async functions (TS1359)
    /// - Object literals don't have excess properties
    ///
    /// ## Parameters:
    /// - `stmt_idx`: The return statement node index to check
    ///
    /// ## Validation:
    /// - Checks return type assignability
    /// - Validates await expressions are in async context
    /// - Checks object literal excess properties
    pub(crate) fn check_return_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(return_data) = self.ctx.arena.get_return_statement(node) else {
            return;
        };

        // TS1108: A 'return' statement can only be used within a function body.
        if self.current_return_type().is_none() {
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node(
                stmt_idx,
                "A 'return' statement can only be used within a function body.",
                diagnostic_codes::A_RETURN_STATEMENT_CAN_ONLY_BE_USED_WITHIN_A_FUNCTION_BODY,
            );
            return;
        }

        // TS2408: Setters cannot return a value.
        if !return_data.expression.is_none() {
            use tsz_parser::parser::syntax_kind_ext;
            if let Some(enclosing_fn_idx) = self.find_enclosing_function(stmt_idx)
                && let Some(enclosing_fn_node) = self.ctx.arena.get(enclosing_fn_idx)
                && enclosing_fn_node.kind == syntax_kind_ext::SET_ACCESSOR
            {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node(
                    stmt_idx,
                    "Setters cannot return a value.",
                    diagnostic_codes::SETTERS_CANNOT_RETURN_A_VALUE,
                );
                return;
            }
        }

        // Get the expected return type from the function context
        let expected_type = self.current_return_type().unwrap_or(TypeId::UNKNOWN);

        // Get the type of the return expression (if any)
        let return_type = if !return_data.expression.is_none() {
            // TS1359: Check for await expressions outside async function
            self.check_await_expression(return_data.expression);

            let prev_context = self.ctx.contextual_type;
            let should_contextualize =
                self.ctx
                    .arena
                    .get(return_data.expression)
                    .is_some_and(|expr_node| {
                        expr_node.kind != tsz_scanner::SyntaxKind::Identifier as u16
                    });
            if should_contextualize
                && expected_type != TypeId::ANY
                && !self.type_contains_error(expected_type)
            {
                self.ctx.contextual_type = Some(expected_type);
                // Clear cached type to force recomputation with contextual type
                // This is necessary because the expression might have been previously typed
                // without contextual information (e.g., during function body analysis)
                self.clear_type_cache_recursive(return_data.expression);
            }
            let mut return_type = self.get_type_of_node(return_data.expression);
            if self.ctx.in_async_context() {
                return_type = self.unwrap_promise_type(return_type).unwrap_or(return_type);
            }
            self.ctx.contextual_type = prev_context;
            return_type
        } else {
            // `return;` without expression returns undefined
            TypeId::UNDEFINED
        };

        // Ensure relation preconditions before assignability check.
        self.ensure_relation_input_ready(return_type);
        self.ensure_relation_input_ready(expected_type);

        // Check if the return type is assignable to the expected type.
        // Constructors are validated holistically by TS2409 at declaration level,
        // so we suppress per-return TS2322 diagnostics here.
        let is_in_constructor = self
            .ctx
            .enclosing_class
            .as_ref()
            .map(|c| c.in_constructor)
            .unwrap_or(false);

        if expected_type != TypeId::ANY
            && !is_in_constructor
            && !self.check_assignable_or_report(
                return_type,
                expected_type,
                if !return_data.expression.is_none() {
                    return_data.expression
                } else {
                    stmt_idx
                },
            )
        {
            // Diagnostic emitted by check_assignable_or_report.
        }

        if expected_type != TypeId::ANY
            && expected_type != TypeId::UNKNOWN
            && !return_data.expression.is_none()
            && let Some(expr_node) = self.ctx.arena.get(return_data.expression)
            && expr_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        {
            self.check_object_literal_excess_properties(
                return_type,
                expected_type,
                return_data.expression,
            );
        }
    }

    // =========================================================================
    // Await Expression Validation
    // =========================================================================

    /// Check if current compiler options support top-level await.
    ///
    /// Top-level await is supported when:
    /// - module is ES2022, ESNext, System, Node16, NodeNext, or Preserve
    /// - target is ES2017 or higher
    fn supports_top_level_await(&self) -> bool {
        use tsz_common::common::{ModuleKind, ScriptTarget};

        // Check module kind supports top-level await
        let module_ok = matches!(
            self.ctx.compiler_options.module,
            ModuleKind::ES2022
                | ModuleKind::ESNext
                | ModuleKind::System
                | ModuleKind::Node16
                | ModuleKind::NodeNext
                | ModuleKind::Preserve
        );

        // Check target is ES2017 or higher
        let target_ok = self.ctx.compiler_options.target as u32 >= ScriptTarget::ES2017 as u32;

        module_ok && target_ok
    }

    /// Check an await expression for async context.
    ///
    /// Validates that await expressions are only used within async functions,
    /// recursively checking child expressions for nested await usage.
    ///
    /// ## Parameters:
    /// - `expr_idx`: The expression node index to check
    ///
    /// ## Validation:
    /// - Emits TS1308 if await is used outside async function
    /// - Iteratively checks child expressions for await expressions (no recursion)
    pub(crate) fn check_await_expression(&mut self, expr_idx: NodeIndex) {
        // Use iterative approach with explicit stack to handle deeply nested expressions
        // This prevents stack overflow for expressions like `0 + 0 + 0 + ... + 0` (50K+ deep)
        let mut stack = vec![expr_idx];

        while let Some(current_idx) = stack.pop() {
            let Some(node) = self.ctx.arena.get(current_idx) else {
                continue;
            };

            // Push child expressions onto stack for iterative processing
            match node.kind {
                syntax_kind_ext::BINARY_EXPRESSION => {
                    if let Some(bin_expr) = self.ctx.arena.get_binary_expr(node) {
                        if !bin_expr.right.is_none() {
                            stack.push(bin_expr.right);
                        }
                        if !bin_expr.left.is_none() {
                            stack.push(bin_expr.left);
                        }
                    }
                }
                syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                | syntax_kind_ext::POSTFIX_UNARY_EXPRESSION => {
                    if let Some(unary_expr) = self.ctx.arena.get_unary_expr_ex(node)
                        && !unary_expr.expression.is_none()
                    {
                        stack.push(unary_expr.expression);
                    }
                }
                syntax_kind_ext::AWAIT_EXPRESSION => {
                    // Validate await expression context
                    if !self.ctx.in_async_context() {
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

                        // Check if we're at top level of a module
                        let at_top_level = self.ctx.function_depth == 0;

                        if at_top_level {
                            // TS1378: Top-level await requires ES2022+/ESNext module and ES2017+ target
                            if !self.supports_top_level_await() {
                                self.error_at_node(
                                    current_idx,
                                    diagnostic_messages::TOP_LEVEL_AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ES,
                                    diagnostic_codes::TOP_LEVEL_AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ES,
                                );
                            }
                        } else {
                            // TS1308: 'await' expressions are only allowed within async functions
                            self.error_at_node(
                                current_idx,
                                diagnostic_messages::AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS,
                                diagnostic_codes::AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS,
                            );
                        }
                    }
                    if let Some(unary_expr) = self.ctx.arena.get_unary_expr_ex(node)
                        && !unary_expr.expression.is_none()
                    {
                        stack.push(unary_expr.expression);
                    }
                }
                syntax_kind_ext::CALL_EXPRESSION => {
                    if let Some(call_expr) = self.ctx.arena.get_call_expr(node) {
                        // Check arguments (push in reverse order for correct traversal)
                        if let Some(ref args) = call_expr.arguments {
                            for &arg in args.nodes.iter().rev() {
                                if !arg.is_none() {
                                    stack.push(arg);
                                }
                            }
                        }
                        if !call_expr.expression.is_none() {
                            stack.push(call_expr.expression);
                        }
                    }
                }
                syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                    if let Some(access_expr) = self.ctx.arena.get_access_expr(node)
                        && !access_expr.expression.is_none()
                    {
                        stack.push(access_expr.expression);
                    }
                }
                syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                    // Element access is stored differently - need to check the actual structure
                    // The expression and argument are stored in specific data_index positions
                    // For now, skip this to avoid breaking the build
                }
                syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    if let Some(paren_expr) = self.ctx.arena.get_parenthesized(node)
                        && !paren_expr.expression.is_none()
                    {
                        stack.push(paren_expr.expression);
                    }
                }
                _ => {
                    // For other expression types, don't recurse into children
                    // to avoid infinite recursion or performance issues
                }
            }
        }
    }

    // =========================================================================
    // Variable Statement Validation
    // =========================================================================

    /// Check a for-await statement for async context and module/target support.
    ///
    /// Validates that for-await loops are only used within async functions or at top level
    /// with appropriate compiler options.
    ///
    /// ## Parameters:
    /// - `stmt_idx`: The for-await statement node index to check
    ///
    /// ## Validation:
    /// - Emits TS1103 if for-await is used outside async function and not at top level
    /// - Emits TS1432 if for-await is at top level but module/target options don't support it
    pub(crate) fn check_for_await_statement(&mut self, stmt_idx: NodeIndex) {
        if !self.ctx.in_async_context() {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

            // Check if we're at top level of a module
            let at_top_level = self.ctx.function_depth == 0;

            if at_top_level {
                self.error_at_node(
                    stmt_idx,
                    diagnostic_messages::FOR_AWAIT_LOOPS_ARE_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_FILE_WHEN_THAT_FILE_IS_A,
                    diagnostic_codes::FOR_AWAIT_LOOPS_ARE_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_FILE_WHEN_THAT_FILE_IS_A,
                );

                // TS1432: Top-level for-await requires ES2022+/ESNext module and ES2017+ target
                if !self.supports_top_level_await() {
                    self.error_at_node(
                        stmt_idx,
                        diagnostic_messages::TOP_LEVEL_FOR_AWAIT_LOOPS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ES20,
                        diagnostic_codes::TOP_LEVEL_FOR_AWAIT_LOOPS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ES20,
                    );
                }
            } else {
                // TS1103: 'for await' loops are only allowed within async functions
                self.error_at_node(
                    stmt_idx,
                    diagnostic_messages::FOR_AWAIT_LOOPS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS_OF,
                    diagnostic_codes::FOR_AWAIT_LOOPS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS_OF,
                );
            }
        }
    }

    /// Check a variable statement.
    ///
    /// Iterates through variable declaration lists in a variable statement
    /// and validates each declaration.
    ///
    /// ## Parameters:
    /// - `stmt_idx`: The variable statement node index to check
    pub(crate) fn check_variable_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        if let Some(var) = self.ctx.arena.get_variable(node) {
            // VariableStatement.declarations contains VariableDeclarationList nodes
            for &list_idx in &var.declarations.nodes {
                self.check_variable_declaration_list(list_idx);
            }
        }
    }

    /// Check a variable declaration list (var/let/const x, y, z).
    ///
    /// Iterates through individual variable declarations in a list and
    /// validates each one.
    ///
    /// ## Parameters:
    /// - `list_idx`: The variable declaration list node index to check
    pub(crate) fn check_variable_declaration_list(&mut self, list_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(list_idx) else {
            return;
        };

        // Check if this is a using/await using declaration list
        use tsz_parser::parser::flags::node_flags;
        let is_using = (node.flags as u32 & node_flags::USING as u32) != 0;
        let is_await_using = (node.flags as u32 & node_flags::AWAIT_USING as u32) != 0;

        // VariableDeclarationList uses the same VariableData structure
        if let Some(var_list) = self.ctx.arena.get_variable(node) {
            // Now these are actual VariableDeclaration nodes
            for &decl_idx in &var_list.declarations.nodes {
                self.check_variable_declaration(decl_idx);

                // Check using/await using declarations have Symbol.dispose
                if is_using || is_await_using {
                    self.check_using_declaration_disposable(decl_idx, is_await_using);
                }
            }
        }
    }

    // =========================================================================
    // Using Declaration Validation (TS2804, TS2803)
    // =========================================================================

    /// Check if a using/await using declaration's initializer type has the required dispose method.
    ///
    /// ## Parameters
    /// - `decl_idx`: The variable declaration node index
    /// - `is_await_using`: Whether this is an await using declaration
    ///
    /// Checks:
    /// - `using` requires type to have `[Symbol.dispose]()` method
    /// - `await using` requires type to have `[Symbol.asyncDispose]()` or `[Symbol.dispose]()` method
    fn check_using_declaration_disposable(&mut self, decl_idx: NodeIndex, is_await_using: bool) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return;
        };

        let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) else {
            return;
        };

        // Skip if no initializer
        if var_decl.initializer.is_none() {
            return;
        }

        // Get the type of the initializer
        let init_type = self.get_type_of_node(var_decl.initializer);

        // Skip error type and any (suppressed by convention)
        if init_type == TypeId::ERROR || init_type == TypeId::ANY {
            return;
        }

        // Check for the required dispose method
        if !self.type_has_disposable_method(init_type, is_await_using) {
            let (message, code) = if is_await_using {
                (
                    diagnostic_messages::THE_INITIALIZER_OF_AN_AWAIT_USING_DECLARATION_MUST_BE_EITHER_AN_OBJECT_WITH_A_SY,
                    diagnostic_codes::THE_INITIALIZER_OF_AN_AWAIT_USING_DECLARATION_MUST_BE_EITHER_AN_OBJECT_WITH_A_SY,
                )
            } else {
                (
                    diagnostic_messages::THE_INITIALIZER_OF_A_USING_DECLARATION_MUST_BE_EITHER_AN_OBJECT_WITH_A_SYMBOL_DI,
                    diagnostic_codes::THE_INITIALIZER_OF_A_USING_DECLARATION_MUST_BE_EITHER_AN_OBJECT_WITH_A_SYMBOL_DI,
                )
            };
            self.error_at_node(var_decl.initializer, message, code);
        }
    }

    /// Check if a type has the appropriate dispose method.
    ///
    /// For `using`: checks for `[Symbol.dispose]()`
    /// For `await using`: checks for `[Symbol.asyncDispose]()` or `[Symbol.dispose]()`
    fn type_has_disposable_method(&mut self, type_id: TypeId, is_await_using: bool) -> bool {
        // Check intrinsic types
        if type_id == TypeId::ANY
            || type_id == TypeId::UNKNOWN
            || type_id == TypeId::ERROR
            || type_id == TypeId::NEVER
        {
            return true; // Suppress errors on these types
        }

        // null and undefined can be disposed (no-op)
        if type_id == TypeId::NULL || type_id == TypeId::UNDEFINED {
            return true;
        }

        // Only check for dispose methods if Symbol.dispose is available in the current environment
        // Check by looking for the dispose property on SymbolConstructor
        let symbol_type = if let Some(sym_id) = self.ctx.binder.file_locals.get("Symbol") {
            self.get_type_of_symbol(sym_id)
        } else {
            TypeId::ERROR
        };

        let symbol_has_dispose = self.object_has_property(symbol_type, "dispose")
            || self.object_has_property(symbol_type, "[Symbol.dispose]")
            || self.object_has_property(symbol_type, "Symbol.dispose");

        let symbol_has_async_dispose = self.object_has_property(symbol_type, "asyncDispose")
            || self.object_has_property(symbol_type, "[Symbol.asyncDispose]")
            || self.object_has_property(symbol_type, "Symbol.asyncDispose");

        // For await using, we need either Symbol.asyncDispose or Symbol.dispose
        if is_await_using && !symbol_has_async_dispose && !symbol_has_dispose {
            // Symbol.asyncDispose and Symbol.dispose are not available in this lib
            // Don't check for them (TypeScript will emit other errors about missing globals)
            return true;
        }

        // For regular using, we need Symbol.dispose
        if !is_await_using && !symbol_has_dispose {
            // Symbol.dispose is not available in this lib
            // Don't check for it
            return true;
        }

        // Check for the dispose method on the object type
        // Try both "[Symbol.dispose]" and "Symbol.dispose" formats
        let has_dispose = self.object_has_property(type_id, "[Symbol.dispose]")
            || self.object_has_property(type_id, "Symbol.dispose");

        if is_await_using {
            // await using accepts either Symbol.asyncDispose or Symbol.dispose
            return has_dispose
                || self.object_has_property(type_id, "[Symbol.asyncDispose]")
                || self.object_has_property(type_id, "Symbol.asyncDispose");
        }

        has_dispose
    }

    // =========================================================================
    // Super Expression Validation
    // =========================================================================

    /// Check if a super expression is inside a nested function within a constructor.
    ///
    /// Walks up the AST from the given node to determine if it's inside
    /// a nested function (function expression, arrow function) within a constructor.
    ///
    /// ## Parameters:
    // 17. Property Initialization Checking (5 functions)

    /// Check for TS2729: Property is used before its initialization.
    ///
    /// This checks if a property initializer references another property via `this.X`
    /// where X is declared after the current property.
    ///
    /// ## Parameters
    /// - `current_prop_idx`: The current property node index
    /// - `initializer_idx`: The initializer expression node index
    pub(crate) fn check_property_initialization_order(
        &mut self,
        current_prop_idx: NodeIndex,
        initializer_idx: NodeIndex,
    ) {
        use crate::diagnostics::diagnostic_codes;

        // Get class info to access member order
        let Some(class_info) = self.ctx.enclosing_class.clone() else {
            return;
        };

        // Find the position of the current property in the member list
        let Some(current_pos) = class_info
            .member_nodes
            .iter()
            .position(|&idx| idx == current_prop_idx)
        else {
            return;
        };

        // Collect all `this.X` property accesses in the initializer
        let accesses = self.collect_this_property_accesses(initializer_idx);

        for (name, access_node_idx) in accesses {
            // Find if this name refers to another property in the class
            for (target_pos, &target_idx) in class_info.member_nodes.iter().enumerate() {
                if let Some(member_name) = self.get_member_name(target_idx)
                    && member_name == name
                {
                    // Check if target is an instance property (not static, not a method)
                    if self.is_instance_property(target_idx) {
                        // Report 2729 if:
                        // 1. Target is declared after current property, OR
                        // 2. Target is an abstract property (no initializer in this class)
                        let should_error =
                            target_pos > current_pos || self.is_abstract_property(target_idx);
                        if should_error {
                            self.error_at_node(
                                access_node_idx,
                                &format!("Property '{}' is used before its initialization.", name),
                                diagnostic_codes::PROPERTY_IS_USED_BEFORE_ITS_INITIALIZATION,
                            );
                        }
                    }
                    break;
                }
            }
        }
    }

    /// Check if a property declaration is abstract (has abstract modifier).
    ///
    /// ## Parameters
    /// - `member_idx`: The class member node index
    ///
    /// Returns true if the member is an abstract property declaration.
    pub(crate) fn is_abstract_property(&self, member_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return false;
        };

        if node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
            return false;
        }

        if let Some(prop) = self.ctx.arena.get_property_decl(node) {
            return self.has_abstract_modifier(&prop.modifiers);
        }

        false
    }

    /// Collect all `this.propertyName` accesses in an expression.
    ///
    /// Stops at function boundaries where `this` context changes.
    ///
    /// ## Parameters
    /// - `node_idx`: The expression node index to search
    ///
    /// Returns a list of (property_name, access_node) tuples.
    pub(crate) fn collect_this_property_accesses(
        &self,
        node_idx: NodeIndex,
    ) -> Vec<(String, NodeIndex)> {
        let mut accesses = Vec::new();
        self.collect_this_accesses_recursive(node_idx, &mut accesses);
        accesses
    }

    /// Recursive helper to collect this.X accesses.
    ///
    /// Traverses the AST to find `this.property` expressions, stopping at
    /// function/class boundaries where `this` context changes (except arrow functions).
    ///
    /// ## Parameters
    /// - `node_idx`: The current node to examine
    /// - `accesses`: Accumulator for found accesses
    pub(crate) fn collect_this_accesses_recursive(
        &self,
        node_idx: NodeIndex,
        accesses: &mut Vec<(String, NodeIndex)>,
    ) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        // Stop at function boundaries where `this` context changes
        // (but not arrow functions, which preserve `this`)
        if node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.kind == syntax_kind_ext::CLASS_EXPRESSION
            || node.kind == syntax_kind_ext::CLASS_DECLARATION
        {
            return;
        }

        // Property access uses AccessExprData with expression and name_or_argument
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            if let Some(access) = self.ctx.arena.get_access_expr(node) {
                // Check if the expression is `this`
                if let Some(expr_node) = self.ctx.arena.get(access.expression) {
                    if expr_node.kind == SyntaxKind::ThisKeyword as u16 {
                        // Get the property name
                        if let Some(name_node) = self.ctx.arena.get(access.name_or_argument)
                            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                        {
                            accesses.push((ident.escaped_text.clone(), node_idx));
                        }
                    } else {
                        // Recurse into the expression part
                        self.collect_this_accesses_recursive(access.expression, accesses);
                    }
                }
            }
            return;
        }

        // For other nodes, recurse into children based on node type
        match node.kind {
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(binary) = self.ctx.arena.get_binary_expr(node) {
                    self.collect_this_accesses_recursive(binary.left, accesses);
                    self.collect_this_accesses_recursive(binary.right, accesses);
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.ctx.arena.get_call_expr(node) {
                    self.collect_this_accesses_recursive(call.expression, accesses);
                    if let Some(ref args) = call.arguments {
                        for &arg in &args.nodes {
                            self.collect_this_accesses_recursive(arg, accesses);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.collect_this_accesses_recursive(paren.expression, accesses);
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.ctx.arena.get_conditional_expr(node) {
                    self.collect_this_accesses_recursive(cond.condition, accesses);
                    self.collect_this_accesses_recursive(cond.when_true, accesses);
                    self.collect_this_accesses_recursive(cond.when_false, accesses);
                }
            }
            k if k == syntax_kind_ext::ARROW_FUNCTION => {
                // Arrow functions: while they preserve `this` context, property access
                // inside is deferred until the function is called. So we don't recurse
                // because the access doesn't happen during initialization.
                // (This matches TypeScript's behavior for error 2729)
            }
            _ => {
                // For other expressions, we don't recurse further to keep it simple
            }
        }
    }

    /// Check if a class member is an instance property (not static, not a method/accessor).
    ///
    /// ## Parameters
    /// - `member_idx`: The class member node index
    ///
    /// Returns true if the member is a non-static property declaration.
    pub(crate) fn is_instance_property(&self, member_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return false;
        };

        if node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
            return false;
        }

        if let Some(prop) = self.ctx.arena.get_property_decl(node) {
            // Check if it has a static modifier
            return !self.has_static_modifier(&prop.modifiers);
        }

        false
    }

    // 18. AST Context Checking (4 functions)

    /// Get the name of a method declaration.
    ///
    /// Handles both identifier names and numeric literal names
    /// (for methods like 0(), 1(), etc.).
    ///
    /// ## Parameters
    /// - `member_idx`: The class member node index
    ///
    /// Returns the method name if found.
    pub(crate) fn get_method_name_from_node(&self, member_idx: NodeIndex) -> Option<String> {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return None;
        };

        if let Some(method) = self.ctx.arena.get_method_decl(node) {
            let Some(name_node) = self.ctx.arena.get(method.name) else {
                return None;
            };
            // Try identifier first
            if let Some(id) = self.ctx.arena.get_identifier(name_node) {
                return Some(id.escaped_text.clone());
            }
            // Try numeric literal (for methods like 0(), 1(), etc.)
            if let Some(lit) = self.ctx.arena.get_literal(name_node) {
                return Some(lit.text.clone());
            }
        }
        None
    }

    /// Check if a function is within a namespace or module context.
    ///
    /// Uses AST-based parent traversal to detect ModuleDeclaration in the parent chain.
    ///
    /// ## Parameters
    /// - `func_idx`: The function node index
    ///
    /// Returns true if the function is inside a namespace/module declaration.
    pub fn is_in_namespace_context(&self, func_idx: NodeIndex) -> bool {
        // Walk up the parent chain looking for ModuleDeclaration nodes
        let mut current = func_idx;

        while !current.is_none() {
            if let Some(node) = self.ctx.arena.get(current) {
                // Check if this node is a ModuleDeclaration (namespace or module)
                if node.kind == syntax_kind_ext::MODULE_DECLARATION {
                    return true;
                }
            }

            // Move to the parent node
            if let Some(ext) = self.ctx.arena.get_extended(current) {
                current = ext.parent;
            } else {
                break;
            }
        }

        false
    }

    /// Check if a variable is declared in an ambient context (declare keyword).
    ///
    /// This uses proper AST-based detection by:
    /// 1. Checking the node's flags for the AMBIENT flag
    /// 2. Walking up the parent chain to find if enclosed in an ambient context
    /// 3. Checking modifiers on declaration nodes for DeclareKeyword
    ///
    /// ## Parameters
    /// - `var_idx`: The variable declaration node index
    ///
    /// Returns true if the declaration is in an ambient context.
    pub(crate) fn is_ambient_declaration(&self, var_idx: NodeIndex) -> bool {
        use tsz_parser::parser::node_flags;

        // Declarations inside .d.ts files are ambient by definition.
        if self.ctx.file_name.ends_with(".d.ts") {
            return true;
        }

        let mut current = var_idx;
        while !current.is_none() {
            if let Some(node) = self.ctx.arena.get(current) {
                // Check if this node has the AMBIENT flag set
                if (node.flags as u32) & node_flags::AMBIENT != 0 {
                    return true;
                }

                // Check modifiers on various declaration types for DeclareKeyword
                // Variable statements
                if let Some(var_stmt) = self.ctx.arena.get_variable(node)
                    && self.has_declare_modifier(&var_stmt.modifiers)
                {
                    return true;
                }
                // Function declarations
                if let Some(func) = self.ctx.arena.get_function(node)
                    && self.has_declare_modifier(&func.modifiers)
                {
                    return true;
                }
                // Class declarations
                if let Some(class) = self.ctx.arena.get_class(node)
                    && self.has_declare_modifier(&class.modifiers)
                {
                    return true;
                }
                // Enum declarations
                if let Some(enum_decl) = self.ctx.arena.get_enum(node)
                    && self.has_declare_modifier(&enum_decl.modifiers)
                {
                    return true;
                }
                // Interface declarations (interfaces are implicitly ambient)
                if self.ctx.arena.get_interface(node).is_some() {
                    return true;
                }
                // Type alias declarations (type aliases are implicitly ambient)
                if self.ctx.arena.get_type_alias(node).is_some() {
                    return true;
                }
                // Module/namespace declarations
                if let Some(module) = self.ctx.arena.get_module(node)
                    && self.has_declare_modifier(&module.modifiers)
                {
                    return true;
                }
            }

            // Move to parent node
            if let Some(ext) = self.ctx.arena.get_extended(current) {
                if ext.parent.is_none() {
                    break;
                }
                current = ext.parent;
            } else {
                break;
            }
        }

        false
    }

    // 19. Type and Name Checking Utilities (8 functions)

    /// Check if a type name is a mapped type utility.
    ///
    /// Mapped type utilities are TypeScript built-in utility types
    /// that transform mapped types.
    ///
    /// ## Parameters
    /// - `name`: The type name to check
    ///
    /// Returns true if the name is a mapped type utility.
    pub(crate) fn is_mapped_type_utility(&self, name: &str) -> bool {
        matches!(
            name,
            "Partial"
                | "Required"
                | "Readonly"
                | "Record"
                | "Pick"
                | "Omit"
                | "Extract"
                | "Exclude"
                | "NonNullable"
                | "ThisType"
                | "Infer"
        )
    }

    /// Check if a type name is a known global type.
    ///
    /// Known global types include built-in JavaScript/TypeScript types
    /// like Object, Array, Promise, Map, etc.
    ///
    /// ## Parameters
    /// - `name`: The type name to check
    ///
    /// Returns true if the name is a known global type.
    pub(crate) fn is_known_global_type_name(&self, name: &str) -> bool {
        if self.ctx.is_known_global_type(name) {
            return true;
        }

        matches!(
            name,
            // Core built-in objects
            "Object"
                | "String"
                | "Number"
                | "Boolean"
                | "Symbol"
                | "Function"
                | "Date"
                | "RegExp"
                | "RegExpExecArray"
                | "RegExpMatchArray"
                // Arrays and collections
                | "Array"
                | "ReadonlyArray"
                | "ArrayLike"
                | "ArrayBuffer"
                | "SharedArrayBuffer"
                | "DataView"
                | "TypedArray"
                | "Int8Array"
                | "Uint8Array"
                | "Uint8ClampedArray"
                | "Int16Array"
                | "Uint16Array"
                | "Int32Array"
                | "Uint32Array"
                | "Float32Array"
                | "Float64Array"
                | "BigInt64Array"
                | "BigUint64Array"
                // ES2015+ collection types
                | "Map"
                | "Set"
                | "WeakMap"
                | "WeakSet"
                | "WeakRef"
                | "ReadonlyMap"
                | "ReadonlySet"
                // Promise types
                | "Promise"
                | "PromiseConstructor"
                | "PromiseConstructorLike"
                | "Awaited"
                // Iterator/Generator types
                | "Iterator"
                | "IteratorResult"
                | "IteratorYieldResult"
                | "IteratorReturnResult"
                | "IterableIterator"
                | "AsyncIterator"
                | "AsyncIterable"
                | "AsyncIterableIterator"
                | "Generator"
                | "GeneratorFunction"
                | "AsyncGenerator"
                | "AsyncGeneratorFunction"
                // Utility types
                | "Partial"
                | "Required"
                | "Readonly"
                | "Record"
                | "Pick"
                | "Omit"
                | "NonNullable"
                | "Extract"
                | "Exclude"
                | "ReturnType"
                | "Parameters"
                | "ConstructorParameters"
                | "InstanceType"
                | "ThisParameterType"
                | "OmitThisParameter"
                | "ThisType"
                | "Uppercase"
                | "Lowercase"
                | "Capitalize"
                | "Uncapitalize"
                | "NoInfer"
                // Object types
                | "PropertyKey"
                | "PropertyDescriptor"
                | "PropertyDescriptorMap"
                | "ObjectConstructor"
                | "FunctionConstructor"
                // Error types
                | "Error"
                | "ErrorConstructor"
                | "TypeError"
                | "RangeError"
                | "EvalError"
                | "URIError"
                | "ReferenceError"
                | "SyntaxError"
                | "AggregateError"
                // Math and JSON
                | "Math"
                | "JSON"
                // Proxy and Reflect
                | "Proxy"
                | "ProxyHandler"
                | "Reflect"
                // BigInt
                | "BigInt"
                | "BigIntConstructor"
                // ES2021+
                | "FinalizationRegistry"
                // DOM types (commonly used)
                | "Element"
                | "HTMLElement"
                | "Document"
                | "Window"
                | "Event"
                | "EventTarget"
                | "NodeList"
                | "NodeListOf"
                | "Console"
                | "Atomics"
                // Primitive types (lowercase)
                | "number"
                | "string"
                | "boolean"
                | "void"
                | "null"
                | "undefined"
                | "never"
                | "unknown"
                | "any"
                | "object"
                | "bigint"
                | "symbol"
        )
    }

    /// Check if a type is a constructor type.
    ///
    /// A constructor type has construct signatures (can be called with `new`).
    ///
    /// ## Parameters
    /// - `type_id`: The type ID to check
    ///
    /// Returns true if the type is a constructor type.
    /// Replace `Function` type members with a callable type for call resolution.
    ///
    /// When the callee type is exactly the Function type, returns `TypeId::ANY` directly.
    /// When the callee type is a union containing Function members, replaces those
    /// members with a synthetic function `(...args: any[]) => any` so that
    /// `resolve_union_call` in the solver can handle it.
    pub(crate) fn replace_function_type_for_call(
        &mut self,
        callee_type_orig: TypeId,
        callee_type_for_call: TypeId,
    ) -> TypeId {
        // Direct Function type - return ANY (which is callable)
        if self.is_global_function_type(callee_type_orig) {
            return TypeId::ANY;
        }

        // Check if callee_type_for_call is a union containing Function members
        if let Some(members_vec) = query::union_members(self.ctx.types, callee_type_for_call) {
            let members = members_vec;
            let orig_members = query::union_members(self.ctx.types, callee_type_orig);
            let factory = self.ctx.types.factory();

            let mut has_function = false;
            let mut new_members = Vec::new();

            for (i, &member) in members.iter().enumerate() {
                // Check if this member corresponds to a Function type in the original
                let is_func = if let Some(ref orig) = orig_members {
                    i < orig.len() && self.is_global_function_type(orig[i])
                } else {
                    false
                };

                if is_func {
                    has_function = true;
                    // Replace Function member with a synthetic callable returning any
                    // Use a simple function: (...args: any[]) => any
                    let rest_param = tsz_solver::ParamInfo {
                        name: Some(self.ctx.types.intern_string("args")),
                        type_id: TypeId::ANY,
                        optional: false,
                        rest: true,
                    };
                    let func_shape = tsz_solver::FunctionShape {
                        params: vec![rest_param],
                        this_type: None,
                        return_type: TypeId::ANY,
                        type_params: vec![],
                        type_predicate: None,
                        is_constructor: false,
                        is_method: false,
                    };
                    let func_type = factory.function(func_shape);
                    new_members.push(func_type);
                } else {
                    new_members.push(member);
                }
            }

            if has_function {
                return factory.union(new_members);
            }
        }

        callee_type_for_call
    }

    /// Check if a type is the global `Function` interface type from lib.d.ts.
    ///
    /// In TypeScript, the `Function` type is callable (returns `any`) even though
    /// the `Function` interface has no call signatures. This method identifies
    /// the Function type so the caller can handle it specially.
    pub(crate) fn is_global_function_type(&mut self, type_id: TypeId) -> bool {
        // Quick check for the intrinsic Function type
        if type_id == TypeId::FUNCTION {
            return true;
        }

        // Check if the type matches the global Function interface type.
        // The Function type annotation resolves to a Lazy(DefId) pointing to the
        // Function symbol. We look up the global Function symbol and compare.
        let lib_binders = self.get_lib_binders();
        if let Some(func_sym_id) = self
            .ctx
            .binder
            .get_global_type_with_libs("Function", &lib_binders)
        {
            let func_type = self.type_reference_symbol_type(func_sym_id);
            if type_id == func_type {
                return true;
            }
        }

        // Also check union members: if all callable members resolve through Function
        // (e.g., `Function | (() => void)` should still be callable)
        false
    }

    pub(crate) fn is_constructor_type(&self, type_id: TypeId) -> bool {
        // Any type is always considered a constructor type (TypeScript compatibility)
        if type_id == TypeId::ANY {
            return true;
        }

        // First check if it directly has construct signatures
        if query::has_construct_signatures(self.ctx.types, type_id) {
            return true;
        }

        // Check if type has a prototype property (functions with prototype are constructable)
        // This handles cases like `function Foo() {}` where `Foo.prototype` exists
        if self.type_has_prototype_property(type_id) {
            return true;
        }

        // For type parameters, check if the constraint is a constructor type
        // For intersection types, check if any member is a constructor type
        // For application types, check if the base type is a constructor type
        match query::classify_for_constructor_check(self.ctx.types, type_id) {
            query::ConstructorCheckKind::TypeParameter { constraint } => {
                if let Some(constraint) = constraint {
                    self.is_constructor_type(constraint)
                } else {
                    false
                }
            }
            query::ConstructorCheckKind::Intersection(members) => {
                members.iter().any(|&m| self.is_constructor_type(m))
            }
            query::ConstructorCheckKind::Union(members) => {
                // Union types are constructable if ALL members are constructable
                // This matches TypeScript's behavior where `type A | B` used in extends
                // requires both A and B to be constructors
                !members.is_empty() && members.iter().all(|&m| self.is_constructor_type(m))
            }
            query::ConstructorCheckKind::Application { base } => {
                // For type applications like Ctor<{}>, check if the base type is a constructor
                // This handles cases like:
                //   type Constructor<T> = new (...args: any[]) => T;
                //   function f<T extends Constructor<{}>>(x: T) {
                //     class C extends x {}  // x should be valid here
                //   }
                // Only check the base - don't recurse further to avoid infinite loops
                // Check if base is a Lazy type to a type alias with constructor type body
                if let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(base)
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                    && let Some(decl_idx) = symbol.declarations.first().copied()
                    && let Some(decl_node) = self.ctx.arena.get(decl_idx)
                    && decl_node.kind == tsz_parser::parser::syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    && let Some(alias) = self.ctx.arena.get_type_alias(decl_node)
                    && let Some(body_node) = self.ctx.arena.get(alias.type_node)
                {
                    // Constructor type syntax: new (...args) => T
                    if body_node.kind == tsz_parser::parser::syntax_kind_ext::CONSTRUCTOR_TYPE {
                        return true;
                    }
                }
                // Also check if base is directly a Callable with construct signatures
                query::has_construct_signatures(self.ctx.types, base)
            }
            // Lazy reference (DefId) - check if it's a class or interface
            // This handles cases like:
            // 1. `class C extends MyClass` where MyClass is a class
            // 2. `function f<T>(ctor: T)` then `class B extends ctor` where ctor has a constructor type
            // 3. `class C extends Object` where Object is declared as ObjectConstructor interface
            query::ConstructorCheckKind::Lazy(def_id) => {
                let symbol_id = match self.ctx.def_to_symbol_id(def_id) {
                    Some(id) => id,
                    None => return false,
                };
                if let Some(symbol) = self.ctx.binder.get_symbol(symbol_id) {
                    // Check if this is a class symbol - classes are always constructors
                    if (symbol.flags & tsz_binder::symbol_flags::CLASS) != 0 {
                        return true;
                    }

                    // Check if this is an interface symbol with construct signatures
                    // This handles cases like ObjectConstructor, ArrayConstructor, etc.
                    // which are interfaces with `new()` signatures
                    if (symbol.flags & tsz_binder::symbol_flags::INTERFACE) != 0 {
                        // Check the cached type for interface - it should be Callable if it has construct signatures
                        if let Some(&cached_type) = self.ctx.symbol_types.get(&symbol_id) {
                            if cached_type != type_id {
                                // Interface type was already resolved - check if it has construct signatures
                                if query::has_construct_signatures(self.ctx.types, cached_type) {
                                    return true;
                                }
                            }
                        } else if !symbol.declarations.is_empty() {
                            // Interface not cached - check if it has construct signatures by examining declarations
                            // This handles lib.d.ts interfaces like ObjectConstructor that may not be resolved yet
                            // IMPORTANT: Use the correct arena for the symbol (may be different for lib types)
                            use tsz_lowering::TypeLowering;
                            let symbol_arena = self
                                .ctx
                                .binder
                                .symbol_arenas
                                .get(&symbol_id)
                                .map(|arena| arena.as_ref())
                                .unwrap_or(self.ctx.arena);

                            let type_param_bindings = self.get_type_param_bindings();
                            let type_resolver = |node_idx: tsz_parser::parser::NodeIndex| {
                                self.resolve_type_symbol_for_lowering(node_idx)
                            };
                            let value_resolver = |node_idx: tsz_parser::parser::NodeIndex| {
                                self.resolve_value_symbol_for_lowering(node_idx)
                            };
                            let lowering = TypeLowering::with_resolvers(
                                symbol_arena,
                                self.ctx.types,
                                &type_resolver,
                                &value_resolver,
                            )
                            .with_type_param_bindings(type_param_bindings);
                            let interface_type =
                                lowering.lower_interface_declarations(&symbol.declarations);
                            if query::has_construct_signatures(self.ctx.types, interface_type) {
                                return true;
                            }
                        }
                    }

                    // For other symbols (variables, parameters, type aliases), check their cached type
                    // This handles cases like:
                    //   function f<T extends typeof A>(ctor: T) {
                    //     class B extends ctor {}  // ctor should be recognized as constructible
                    //   }
                    if let Some(&cached_type) = self.ctx.symbol_types.get(&symbol_id) {
                        // Recursively check if the resolved type is a constructor
                        // Avoid infinite recursion by checking if cached_type == type_id
                        if cached_type != type_id {
                            return self.is_constructor_type(cached_type);
                        }
                    }
                }
                // For other symbols (namespaces, enums, etc.) without cached types, they're not constructors
                false
            }
            // TypeQuery (typeof X) - similar to Ref but for typeof expressions
            // This handles cases like:
            //   class A {}
            //   function f<T extends typeof A>(ctor: T) {
            //     class B extends ctor {}  // ctor: T where T extends typeof A
            //   }
            query::ConstructorCheckKind::TypeQuery(symbol_ref) => {
                use tsz_binder::SymbolId;
                let symbol_id = SymbolId(symbol_ref.0);
                if let Some(symbol) = self.ctx.binder.get_symbol(symbol_id) {
                    // Classes have constructor types
                    if (symbol.flags & tsz_binder::symbol_flags::CLASS) != 0 {
                        return true;
                    }

                    // Check cached type for variables/parameters with constructor types
                    if let Some(&cached_type) = self.ctx.symbol_types.get(&symbol_id) {
                        // Recursively check if the resolved type is a constructor
                        // Avoid infinite recursion by checking if cached_type == type_id
                        if cached_type != type_id {
                            return self.is_constructor_type(cached_type);
                        }
                    }
                }
                false
            }
            query::ConstructorCheckKind::Other => false,
        }
    }

    /// Check if an expression is a property access to a get accessor.
    ///
    /// Used to emit TS6234 instead of TS2349 when a getter is accidentally called:
    /// ```typescript
    /// class Test { get property(): number { return 1; } }
    /// x.property(); // TS6234: not callable because it's a get accessor
    /// ```
    pub(crate) fn is_get_accessor_call(&self, expr_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(expr_node) else {
            return false;
        };

        // Get the property name
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        let prop_name = &ident.escaped_text;

        // Check via symbol flags if the property is a getter
        if let Some(sym_id) = self
            .ctx
            .binder
            .node_symbols
            .get(&access.name_or_argument.0)
            .copied()
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && (symbol.flags & tsz_binder::symbol_flags::GET_ACCESSOR) != 0
        {
            return true;
        }

        // Check if the object type is a class instance with a get accessor for this property
        if let Some(&obj_type) = self.ctx.node_types.get(&access.expression.0)
            && let Some(class_idx) = self.ctx.class_instance_type_to_decl.get(&obj_type).copied()
            && let Some(class) = self.ctx.arena.get_class_at(class_idx)
        {
            for &member_idx in &class.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                    && let Some(accessor) = self.ctx.arena.get_accessor(member_node)
                    && let Some(acc_ident) = self.ctx.arena.get_identifier_at(accessor.name)
                    && acc_ident.escaped_text == *prop_name
                {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a type has a 'prototype' property.
    ///
    /// Functions with a prototype property can be used as constructors.
    /// This handles cases like:
    /// ```typescript
    /// function Foo() {}
    /// new Foo(); // Valid if Foo.prototype exists
    /// ```
    pub(crate) fn type_has_prototype_property(&self, type_id: TypeId) -> bool {
        // Check callable shape for prototype property
        if let Some(shape) = query::callable_shape_for_type(self.ctx.types, type_id) {
            let prototype_atom = self.ctx.types.intern_string("prototype");
            return shape.properties.iter().any(|p| p.name == prototype_atom);
        }

        // Function types typically have prototype
        query::has_function_shape(self.ctx.types, type_id)
    }

    /// Check if a symbol is a class symbol.
    ///
    /// ## Parameters
    /// - `symbol_id`: The symbol ID to check
    ///
    /// Returns true if the symbol represents a class.
    pub(crate) fn is_class_symbol(&self, symbol_id: tsz_binder::SymbolId) -> bool {
        use tsz_binder::symbol_flags;
        if let Some(symbol) = self.ctx.binder.get_symbol(symbol_id) {
            (symbol.flags & symbol_flags::CLASS) != 0
        } else {
            false
        }
    }

    /// Check if an expression is a numeric literal with value 0.
    ///
    /// ## Parameters
    /// - `expr_idx`: The expression node index
    ///
    /// Returns true if the expression is the literal 0.
    pub(crate) fn is_numeric_literal_zero(&self, expr_idx: NodeIndex) -> bool {
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if node.kind != SyntaxKind::NumericLiteral as u16 {
            return false;
        }
        let Some(lit) = self.ctx.arena.get_literal(node) else {
            return false;
        };
        lit.text == "0"
    }

    /// Check if an expression is a property or element access expression.
    ///
    /// ## Parameters
    /// - `expr_idx`: The expression node index
    ///
    /// Returns true if the expression is a property or element access.
    pub(crate) fn is_access_expression(&self, expr_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        matches!(
            node.kind,
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        )
    }

    /// Check if a statement is a super() call.
    ///
    /// ## Parameters
    /// - `stmt_idx`: The statement node index
    ///
    /// Returns true if the statement is an expression statement calling super().
    pub(crate) fn is_super_call_statement(&self, stmt_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
            return false;
        }
        let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) else {
            return false;
        };
        let Some(expr_node) = self.ctx.arena.get(expr_stmt.expression) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        };
        let Some(call) = self.ctx.arena.get_call_expr(expr_node) else {
            return false;
        };
        let Some(callee_node) = self.ctx.arena.get(call.expression) else {
            return false;
        };
        callee_node.kind == SyntaxKind::SuperKeyword as u16
    }

    /// Check if a parameter name is "this".
    ///
    /// ## Parameters
    /// - `name_idx`: The parameter name node index
    ///
    /// Returns true if the parameter name is "this".
    pub(crate) fn is_this_parameter_name(&self, name_idx: NodeIndex) -> bool {
        if let Some(name_node) = self.ctx.arena.get(name_idx) {
            if name_node.kind == SyntaxKind::ThisKeyword as u16 {
                return true;
            }
            if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                return ident.escaped_text == "this";
            }
        }
        false
    }

    // 20. Declaration and Node Checking Utilities (6 functions)

    /// Check if a variable declaration is in a const declaration list.
    ///
    /// ## Parameters
    /// - `var_decl_idx`: The variable declaration node index
    ///
    /// Returns true if the variable is declared with `const`.
    pub(crate) fn is_const_variable_declaration(&self, var_decl_idx: NodeIndex) -> bool {
        use tsz_parser::parser::node_flags;

        let Some(ext) = self.ctx.arena.get_extended(var_decl_idx) else {
            return false;
        };
        let parent_idx = ext.parent;
        if parent_idx.is_none() {
            return false;
        }
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        if parent_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return false;
        }
        (parent_node.flags as u32) & node_flags::CONST != 0
    }

    /// Check if an initializer is a valid const initializer for ambient contexts.
    /// Valid initializers are string/numeric/bigint literals and enum references.
    pub(crate) fn is_valid_ambient_const_initializer(&self, init_idx: NodeIndex) -> bool {
        use tsz_binder::symbol_flags;

        let Some(node) = self.ctx.arena.get(init_idx) else {
            return false;
        };
        match node.kind {
            k if k == tsz_scanner::SyntaxKind::StringLiteral as u16
                || k == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == tsz_scanner::SyntaxKind::NumericLiteral as u16
                || k == tsz_scanner::SyntaxKind::BigIntLiteral as u16 =>
            {
                true
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.ctx.arena.get_unary_expr(node)
                    && unary.operator == tsz_scanner::SyntaxKind::MinusToken as u16
                    && let Some(operand) = self.ctx.arena.get(unary.operand)
                {
                    return operand.kind == tsz_scanner::SyntaxKind::NumericLiteral as u16
                        || operand.kind == tsz_scanner::SyntaxKind::BigIntLiteral as u16;
                }
                false
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                let Some(access) = self.ctx.arena.get_access_expr(node) else {
                    return false;
                };
                let Some(sym_id) = self.resolve_identifier_symbol(access.expression) else {
                    return false;
                };
                let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                    return false;
                };
                if symbol.flags & symbol_flags::ENUM == 0 {
                    return false;
                }
                if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                    return true;
                }
                let Some(arg_node) = self.ctx.arena.get(access.name_or_argument) else {
                    return false;
                };
                arg_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
                    || arg_node.kind == tsz_scanner::SyntaxKind::NumericLiteral as u16
                    || arg_node.kind
                        == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16
            }
            _ => false,
        }
    }

    /// Check if a class declaration has the declare modifier (is ambient).
    ///
    /// ## Parameters
    /// - `decl_idx`: The declaration node index
    ///
    /// Returns true if the class is an ambient declaration.
    pub(crate) fn is_ambient_class_declaration(&self, decl_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::CLASS_DECLARATION {
            return false;
        }
        let Some(class) = self.ctx.arena.get_class(node) else {
            return false;
        };
        // Check for explicit `declare` modifier
        if self.has_declare_modifier(&class.modifiers) {
            return true;
        }
        // Check if the class is inside a `declare namespace`/`declare module`
        // by walking up the parent chain to find an ambient module declaration
        let mut current = decl_idx;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            if let Some(parent_node) = self.ctx.arena.get(parent)
                && parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module) = self.ctx.arena.get_module(parent_node)
                && self.has_declare_modifier(&module.modifiers)
            {
                return true;
            }
            current = parent;
        }
        false
    }

    /// Check if a function declaration has the declare modifier (is ambient).
    pub(crate) fn is_ambient_function_declaration(&self, decl_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
            return false;
        }
        let Some(function) = self.ctx.arena.get_function(node) else {
            return false;
        };
        if self.has_declare_modifier(&function.modifiers) {
            return true;
        }

        let mut current = decl_idx;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            if let Some(parent_node) = self.ctx.arena.get(parent)
                && parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module) = self.ctx.arena.get_module(parent_node)
                && self.has_declare_modifier(&module.modifiers)
            {
                return true;
            }
            current = parent;
        }
        false
    }

    /// Check whether a namespace declaration is instantiated (has runtime value declarations).
    pub(crate) fn is_namespace_declaration_instantiated(&self, namespace_idx: NodeIndex) -> bool {
        let Some(namespace_node) = self.ctx.arena.get(namespace_idx) else {
            return false;
        };
        if namespace_node.kind != syntax_kind_ext::MODULE_DECLARATION {
            return false;
        }
        let Some(module_decl) = self.ctx.arena.get_module(namespace_node) else {
            return false;
        };
        self.module_body_has_runtime_members(module_decl.body)
    }

    fn module_body_has_runtime_members(&self, body_idx: NodeIndex) -> bool {
        if body_idx.is_none() {
            return false;
        }
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return false;
        };

        if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            return self.is_namespace_declaration_instantiated(body_idx);
        }

        if body_node.kind != syntax_kind_ext::MODULE_BLOCK {
            return false;
        }

        let Some(module_block) = self.ctx.arena.get_module_block(body_node) else {
            return false;
        };
        let Some(statements) = &module_block.statements else {
            return false;
        };

        for &statement_idx in &statements.nodes {
            let Some(statement_node) = self.ctx.arena.get(statement_idx) else {
                continue;
            };
            match statement_node.kind {
                k if k == syntax_kind_ext::VARIABLE_STATEMENT
                    || k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::ENUM_DECLARATION
                    || k == syntax_kind_ext::EXPRESSION_STATEMENT
                    || k == syntax_kind_ext::EXPORT_ASSIGNMENT =>
                {
                    return true;
                }
                k if k == syntax_kind_ext::MODULE_DECLARATION => {
                    if self.is_namespace_declaration_instantiated(statement_idx) {
                        return true;
                    }
                }
                _ => {}
            }
        }

        false
    }

    /// Check if a method declaration has a body (is an implementation, not just a signature).
    ///
    /// ## Parameters
    /// - `decl_idx`: The method declaration node index
    ///
    /// Returns true if the method has a body.
    pub(crate) fn method_has_body(&self, decl_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::METHOD_DECLARATION {
            return false;
        }
        let Some(method) = self.ctx.arena.get_method_decl(node) else {
            return false;
        };
        !method.body.is_none()
    }

    /// Get the name node of a declaration for error reporting.
    ///
    /// ## Parameters
    /// - `decl_idx`: The declaration node index
    ///
    /// Returns the name node if the declaration has one.
    pub(crate) fn get_declaration_name_node(&self, decl_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(decl_idx)?;

        match node.kind {
            syntax_kind_ext::VARIABLE_DECLARATION => {
                let var_decl = self.ctx.arena.get_variable_declaration(node)?;
                Some(var_decl.name)
            }
            syntax_kind_ext::FUNCTION_DECLARATION => {
                let func = self.ctx.arena.get_function(node)?;
                Some(func.name)
            }
            syntax_kind_ext::CLASS_DECLARATION => {
                let class = self.ctx.arena.get_class(node)?;
                Some(class.name)
            }
            syntax_kind_ext::INTERFACE_DECLARATION => {
                let interface = self.ctx.arena.get_interface(node)?;
                Some(interface.name)
            }
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                let type_alias = self.ctx.arena.get_type_alias(node)?;
                Some(type_alias.name)
            }
            syntax_kind_ext::ENUM_DECLARATION => {
                let enum_decl = self.ctx.arena.get_enum(node)?;
                Some(enum_decl.name)
            }
            _ => None,
        }
    }

    /// Collect interface type parameter names in declaration order.
    fn interface_type_parameter_names(&self, decl_idx: NodeIndex) -> Option<Vec<String>> {
        let node = self.ctx.arena.get(decl_idx)?;
        let interface = self.ctx.arena.get_interface(node)?;
        let list = interface.type_parameters.as_ref()?;

        let mut names = Vec::with_capacity(list.nodes.len());
        for &param_idx in &list.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                return None;
            };
            let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                return None;
            };
            let Some(name_node) = self.ctx.arena.get(param.name) else {
                return None;
            };
            let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                return None;
            };
            names.push(self.ctx.arena.resolve_identifier_text(ident).to_string());
        }

        Some(names)
    }

    /// Verify that a declaration node actually has a name matching the expected symbol name.
    /// This is used to filter out false matches when lib declarations' NodeIndex values
    /// overlap with user arena indices and point to unrelated user nodes.
    fn declaration_name_matches(&self, decl_idx: NodeIndex, expected_name: &str) -> bool {
        let Some(name_node_idx) = self.get_declaration_name_node(decl_idx) else {
            // For declarations without extractable names (methods, properties, constructors, etc.),
            // fall back to checking the node's identifier directly
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                return false;
            };
            match node.kind {
                syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.ctx.arena.get_method_decl(node)
                        && let Some(name_node) = self.ctx.arena.get(method.name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        return self.ctx.arena.resolve_identifier_text(ident) == expected_name;
                    }
                    return false;
                }
                syntax_kind_ext::PROPERTY_DECLARATION => {
                    if let Some(prop) = self.ctx.arena.get_property_decl(node)
                        && let Some(name_node) = self.ctx.arena.get(prop.name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        return self.ctx.arena.resolve_identifier_text(ident) == expected_name;
                    }
                    return false;
                }
                syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                    if let Some(accessor) = self.ctx.arena.get_accessor(node)
                        && let Some(name_node) = self.ctx.arena.get(accessor.name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        return self.ctx.arena.resolve_identifier_text(ident) == expected_name;
                    }
                    return false;
                }
                syntax_kind_ext::MODULE_DECLARATION => {
                    if let Some(module) = self.ctx.arena.get_module(node)
                        && let Some(name_node) = self.ctx.arena.get(module.name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        return self.ctx.arena.resolve_identifier_text(ident) == expected_name;
                    }
                    return false;
                }
                _ => return false,
            }
        };
        // Check the name node is an identifier with the expected name
        if let Some(ident) = self.ctx.arena.get_identifier_at(name_node_idx) {
            return self.ctx.arena.resolve_identifier_text(ident) == expected_name;
        }
        false
    }

    /// Convert a floating-point number to a numeric index.
    ///
    /// ## Parameters
    /// - `value`: The floating-point value to convert
    ///
    /// Returns Some(index) if the value is a valid non-negative integer, None otherwise.
    pub(crate) fn get_numeric_index_from_number(&self, value: f64) -> Option<usize> {
        if !value.is_finite() || value.fract() != 0.0 || value < 0.0 {
            return None;
        }
        if value > (usize::MAX as f64) {
            return None;
        }
        Some(value as usize)
    }

    // 21. Property Name Utilities (2 functions)

    /// Get the display string for a property key.
    ///
    /// Converts a PropertyKey enum into its string representation
    /// for use in error messages and diagnostics.
    ///
    /// ## Parameters
    /// - `key`: The property key to convert
    ///
    /// Returns the string representation of the property key.
    pub(crate) fn get_property_name_from_key(&self, key: &PropertyKey) -> String {
        match key {
            PropertyKey::Ident(s) => s.clone(),
            PropertyKey::Computed(ComputedKey::Ident(s)) => {
                format!("[{}]", s)
            }
            PropertyKey::Computed(ComputedKey::String(s)) => {
                format!("[\"{}\"]", s)
            }
            PropertyKey::Computed(ComputedKey::Number(n)) => {
                format!("[{}]", n)
            }
            PropertyKey::Private(s) => format!("#{}", s),
        }
    }

    /// Get the Symbol property name from an expression.
    ///
    /// Extracts the name from a Symbol() expression, e.g., Symbol("foo") -> "Symbol.foo".
    ///
    /// ## Parameters
    /// - `expr_idx`: The expression node index
    ///
    /// Returns the Symbol property name if the expression is a Symbol() call.
    pub(crate) fn get_symbol_property_name_from_expr(&self, expr_idx: NodeIndex) -> Option<String> {
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return None;
        };

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            let paren = self.ctx.arena.get_parenthesized(node)?;
            return self.get_symbol_property_name_from_expr(paren.expression);
        }

        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return None;
        }

        let access = self.ctx.arena.get_access_expr(node)?;
        let base_node = self.ctx.arena.get(access.expression)?;
        let base_ident = self.ctx.arena.get_identifier(base_node)?;
        if base_ident.escaped_text != "Symbol" {
            return None;
        }

        let name_node = self.ctx.arena.get(access.name_or_argument)?;
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            return Some(format!("[Symbol.{}]", ident.escaped_text));
        }

        if matches!(
            name_node.kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        ) && let Some(lit) = self.ctx.arena.get_literal(name_node)
            && !lit.text.is_empty()
        {
            return Some(format!("[Symbol.{}]", lit.text));
        }

        None
    }

    // 22. Type Checking Utilities (2 functions)

    /// Check if a node is within another node in the AST tree.
    ///
    /// Traverses up the parent chain to check if `node_idx` is a descendant
    /// of `root_idx`. Used for scope checking and containment analysis.
    ///
    /// ## Parameters
    /// - `node_idx`: The potential descendant node
    /// - `root_idx`: The potential ancestor node
    ///
    /// Returns true if node_idx is within root_idx.
    pub(crate) fn is_node_within(&self, node_idx: NodeIndex, root_idx: NodeIndex) -> bool {
        if node_idx == root_idx {
            return true;
        }
        let mut current = node_idx;
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return false;
            }
            let ext = match self.ctx.arena.get_extended(current) {
                Some(ext) => ext,
                None => return false,
            };
            if ext.parent.is_none() {
                return false;
            }
            if ext.parent == root_idx {
                return true;
            }
            current = ext.parent;
        }
    }

    // =========================================================================
    // Symbol and Duplicate Checking (extracted from state.rs)
    // =========================================================================

    /// Check for missing global types (TS2318).
    ///
    /// When library files are not loaded or specific global types are unavailable,
    /// TypeScript emits TS2318 errors for essential global types at the beginning
    /// of the file (position 0).
    ///
    /// This function checks for:
    /// 1. Core 8 types when --noLib is used: Array, Boolean, Function, IArguments,
    ///    Number, Object, RegExp, String
    /// 2. ES2015+ types when they should be available but aren't: Awaited,
    ///    IterableIterator, AsyncIterableIterator, TypedPropertyDescriptor,
    ///    CallableFunction, NewableFunction, Disposable, AsyncDisposable
    ///
    /// This matches TypeScript's behavior in tests like noCrashOnNoLib.ts,
    /// generatorReturnTypeFallback.2.ts, missingDecoratorType.ts, etc.
    pub(crate) fn check_missing_global_types(&mut self) {
        use tsz_binder::lib_loader;

        // Core global types that TypeScript requires.
        // These are fundamental types that should always exist unless explicitly disabled.
        const CORE_GLOBAL_TYPES: &[&str] = &[
            "Array",
            "Boolean",
            "Function",
            "IArguments",
            "Number",
            "Object",
            "RegExp",
            "String",
        ];

        // Emit TS2318 errors when core global types are not available.
        // TypeScript always requires these core global types to exist.
        // tsc emits these errors BOTH with and without --noLib.
        //
        // We check if types exist globally (in libs or current file scope).
        // This matches tsc behavior where missing core types are reported
        // even when some libs are loaded (e.g., if --lib es6 is missing Array).
        for &type_name in CORE_GLOBAL_TYPES {
            // Check if the type is available in any loaded lib or current scope
            if !self.ctx.has_name_in_lib(type_name) {
                // Type not available globally - emit TS2318
                self.ctx
                    .push_diagnostic(lib_loader::emit_error_global_type_missing(
                        type_name,
                        self.ctx.file_name.clone(),
                        0,
                        0,
                    ));
            }
        }

        // Check for feature-specific global types that may be missing
        // These are checked regardless of --noLib, but only if the feature appears to be used
        self.check_feature_specific_global_types();
    }

    /// Register boxed types (String, Number, Boolean, etc.) from lib.d.ts in TypeEnvironment.
    ///
    /// This enables primitive property access to use lib.d.ts definitions instead of
    /// hardcoded lists. For example, "foo".length will look up the String interface
    /// from lib.d.ts and find the length property there.
    pub(crate) fn register_boxed_types(&mut self) {
        use tsz_solver::IntrinsicKind;

        // Only register if lib files are loaded
        if !self.ctx.has_lib_loaded() {
            return;
        }

        // 1. Resolve types first (avoids holding a mutable borrow on type_env while resolving)
        // resolve_lib_type_by_name handles looking up in lib.d.ts and merging declarations
        let string_type = self.resolve_lib_type_by_name("String");
        let number_type = self.resolve_lib_type_by_name("Number");
        let boolean_type = self.resolve_lib_type_by_name("Boolean");
        let symbol_type = self.resolve_lib_type_by_name("Symbol");
        let bigint_type = self.resolve_lib_type_by_name("BigInt");
        let object_type = self.resolve_lib_type_by_name("Object");
        let function_type = self.resolve_lib_type_by_name("Function");

        // For Array<T>, extract the actual type parameters from the interface definition
        // rather than synthesizing fresh ones. This ensures the T used in Array's method
        // signatures has the same TypeId as the T registered in TypeEnvironment.
        let (array_type, array_type_params) = self.resolve_lib_type_with_params("Array");

        // Pre-compute type parameters for commonly-used generic lib types.
        // This populates the def_type_params cache so that:
        // 1. validate_type_reference_type_arguments can check constraints (TS2344)
        // 2. Application(Lazy(DefId), Args) expansion works in the solver
        // Without this, cross-arena delegation in get_type_params_for_symbol fails
        // for lib symbols due to depth guards, causing constraint checks to be skipped.
        for type_name in &[
            "ReadonlyArray",
            "Promise",
            "PromiseLike",
            "Awaited",
            "Map",
            "Set",
            "WeakMap",
            "WeakSet",
            "WeakRef",
            "ReadonlyMap",
            "ReadonlySet",
            "Iterator",
            "IterableIterator",
            "AsyncIterator",
            "AsyncIterable",
            "AsyncIterableIterator",
            "Generator",
            "AsyncGenerator",
            "Partial",
            "Required",
            "Readonly",
            "Record",
            "Pick",
            "Omit",
            "Exclude",
            "Extract",
            "NonNullable",
            "ReturnType",
            "Parameters",
            "ConstructorParameters",
            "InstanceType",
            "ThisParameterType",
            "OmitThisParameter",
        ] {
            // resolve_lib_type_with_params internally caches type params via
            // insert_def_type_params, making them available for constraint checking
            let _ = self.resolve_lib_type_with_params(type_name);
        }

        // The Array type from lib.d.ts is a Callable with instance methods as properties
        // We register this type directly so that resolve_array_property can use it
        // No need to extract instance type from construct signatures - the methods
        // are already on the Callable itself
        let array_instance_type = array_type;

        // PropertyAccessEvaluator runs through multiple database backends
        // (query cache, interner, binder-backed resolver). Register Array<T>
        // through the query database so all backends see the same base type.
        if let Some(ty) = array_instance_type {
            self.ctx
                .types
                .register_array_base_type(ty, array_type_params.clone());
        }

        // 2. Populate the environment
        // We use try_borrow_mut to be safe, though at this stage it should be free
        if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
            if let Some(ty) = string_type {
                env.set_boxed_type(IntrinsicKind::String, ty);
            }
            if let Some(ty) = number_type {
                env.set_boxed_type(IntrinsicKind::Number, ty);
            }
            if let Some(ty) = boolean_type {
                env.set_boxed_type(IntrinsicKind::Boolean, ty);
            }
            if let Some(ty) = symbol_type {
                env.set_boxed_type(IntrinsicKind::Symbol, ty);
            }
            if let Some(ty) = bigint_type {
                env.set_boxed_type(IntrinsicKind::Bigint, ty);
            }
            if let Some(ty) = object_type {
                env.set_boxed_type(IntrinsicKind::Object, ty);
            }
            if let Some(ty) = function_type {
                env.set_boxed_type(IntrinsicKind::Function, ty);
            }
            // Register the Array<T> interface for array property resolution
            // Use the instance type (Array<T> interface), not the constructor (Callable)
            if let Some(ty) = array_instance_type {
                env.set_array_base_type(ty, array_type_params);
            }

            // 3. Register DefId mappings for non-generic boxed types.
            // When user code writes `a: Function`, the type annotation creates a
            // Lazy(DefId) referencing the global Function symbol. The CallEvaluator
            // uses TypeEnvironment as its resolver, which resolves Lazy types via
            // def_types. Without this registration, Lazy(DefId) for Function can't
            // be resolved, causing false TS2345/TS2322 errors.
            let boxed_names: &[(&str, Option<TypeId>, IntrinsicKind)] = &[
                ("String", string_type, IntrinsicKind::String),
                ("Number", number_type, IntrinsicKind::Number),
                ("Boolean", boolean_type, IntrinsicKind::Boolean),
                ("Symbol", symbol_type, IntrinsicKind::Symbol),
                ("BigInt", bigint_type, IntrinsicKind::Bigint),
                ("Object", object_type, IntrinsicKind::Object),
                ("Function", function_type, IntrinsicKind::Function),
            ];
            for &(name, type_opt, kind) in boxed_names {
                if let Some(ty) = type_opt {
                    // Register DefIds from ALL lib contexts, not just the first.
                    // Multiple lib files (es5, es2015, etc.) each have their own
                    // symbol for types like Function, String, etc. User code can
                    // reference any of them, so all must resolve to the same type.
                    for ctx in self.ctx.lib_contexts.iter() {
                        if let Some(sym_id) = ctx.binder.file_locals.get(name) {
                            let def_id = self.ctx.get_or_create_def_id(sym_id);
                            env.insert_def(def_id, ty);
                            env.register_boxed_def_id(kind, def_id);
                        }
                    }
                    // Also register from current file's binder (for global augmentations)
                    if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
                        let def_id = self.ctx.get_or_create_def_id(sym_id);
                        env.insert_def(def_id, ty);
                        env.register_boxed_def_id(kind, def_id);
                    }
                }
            }
        }
    }

    /// Prime boxed and Array base types before checking files.
    pub fn prime_boxed_types(&mut self) {
        self.register_boxed_types();
    }

    /// Check for feature-specific global types that may be missing.
    ///
    /// This function checks if certain global types that are required for specific
    /// TypeScript features are available. Unlike the core global types, these are
    /// only checked when the feature is potentially used in the code.
    ///
    /// Examples:
    /// - TypedPropertyDescriptor: Required for decorators
    /// - IterableIterator: Required for generators
    /// - AsyncIterableIterator: Required for async generators
    /// - Disposable/AsyncDisposable: Required for using declarations
    /// - Awaited: Required for await type operator
    pub(crate) fn check_feature_specific_global_types(&mut self) {
        use tsz_binder::lib_loader;

        // Types that are commonly referenced in TypeScript features
        // We check if these are available in lib contexts
        let feature_types = [
            // ES2015+ types that are commonly needed
            ("Awaited", "ES2022"),               // For await type operator
            ("IterableIterator", "ES2015"),      // For generators
            ("AsyncIterableIterator", "ES2018"), // For async generators
            ("TypedPropertyDescriptor", "ES5"),  // For decorators
            ("CallableFunction", "ES2015"),      // For strict function types
            ("NewableFunction", "ES2015"),       // For constructor types
            ("Disposable", "ES2022"),            // For using declarations
            ("AsyncDisposable", "ES2022"),       // For await using declarations
        ];

        for &(type_name, _es_version) in &feature_types {
            // Check if the type should be available but isn't
            // Only check if:
            // 1. The type is not in lib contexts (not available from loaded libs)
            // 2. The type is not declared in the current file
            // 3. This appears to be a scenario where the type would be referenced

            // Check if available in lib contexts
            if self.ctx.has_name_in_lib(type_name) {
                continue; // Type is available
            }

            // Check if declared in current file
            if self.ctx.binder.file_locals.has(type_name) {
                continue; // Type is declared locally
            }

            // At this point, the type is not available
            // TypeScript emits TS2318 at position 0 if the type would be referenced
            // For now, we'll emit based on certain heuristics:

            let should_emit = match type_name {
                // Always check these when libs are minimal (ES5 or noLib)
                "IterableIterator"
                | "AsyncIterableIterator"
                | "TypedPropertyDescriptor"
                | "Disposable"
                | "AsyncDisposable" => {
                    // These are emitted when the feature syntax is detected
                    // For simplicity, we check if any syntax that would need them exists
                    self.should_check_for_feature_type(type_name)
                }
                // Awaited is checked when using await type operator, async functions, or Promise-like types
                "Awaited" => {
                    // TSC emits TS2318 for Awaited when Promise-like types are used, even without explicit await
                    // Check if async/await is used OR if noLib is true (TSC checks it in that case)
                    self.ctx.async_depth > 0 || self.ctx.no_lib()
                }
                // CallableFunction/NewableFunction are needed for strict checks
                "CallableFunction" | "NewableFunction" => {
                    // These are emitted in certain strict scenarios
                    // Check if we're in a context that would use them
                    false // Don't emit for now - too broad
                }
                _ => false,
            };

            if should_emit {
                let diag = lib_loader::emit_error_global_type_missing(
                    type_name,
                    self.ctx.file_name.clone(),
                    0,
                    0,
                );
                // Use push_diagnostic for consistent deduplication
                self.ctx.push_diagnostic(diag);
            }
        }
    }

    /// Check if we should emit an error for a feature-specific global type.
    ///
    /// This heuristic determines if a feature that requires a specific global type
    /// is likely being used in the code. These errors are NOT emitted just because
    /// noLib is set — they require the actual feature to be used.
    pub(crate) fn should_check_for_feature_type(&self, type_name: &str) -> bool {
        use tsz_binder::FileFeatures;
        let features = self.ctx.binder.file_features;
        match type_name {
            "IterableIterator" => features.has(FileFeatures::GENERATORS),
            "AsyncIterableIterator" => features.has(FileFeatures::ASYNC_GENERATORS),
            "TypedPropertyDescriptor" => {
                self.ctx.compiler_options.experimental_decorators
                    && features.has(FileFeatures::DECORATORS)
            }
            "Disposable" => features.has(FileFeatures::USING),
            "AsyncDisposable" => features.has(FileFeatures::AWAIT_USING),
            _ => false,
        }
    }

    /// Check for duplicate identifiers (TS2300, TS2451, TS2392).
    /// Reports when variables, functions, classes, or other declarations
    /// have conflicting names within the same scope.
    pub(crate) fn check_duplicate_identifiers(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        // When lib contexts are loaded, skip symbols that come from lib files.
        // Lib types (Array, String, etc.) have multiple declarations from merged
        // lib files which are not actual duplicates.
        let has_libs = self.ctx.has_lib_loaded();

        let mut symbol_ids = FxHashSet::default();
        if !self.ctx.binder.scopes.is_empty() {
            for scope in &self.ctx.binder.scopes {
                // Skip class scopes - class member duplicates need specialized handling
                // (static vs instance separation, method overloads, get/set pairs, etc.)
                if scope.kind == tsz_binder::ContainerKind::Class {
                    continue;
                }
                for (_, &id) in scope.table.iter() {
                    symbol_ids.insert(id);
                }
            }
        } else {
            for (_, &id) in self.ctx.binder.file_locals.iter() {
                symbol_ids.insert(id);
            }
        }

        for sym_id in symbol_ids {
            // Skip symbols that come from lib files - they have multiple declarations
            // from different lib files (e.g. lib.es5.d.ts, lib.es2015.core.d.ts) that
            // are not actual duplicates.
            if has_libs && self.ctx.symbol_is_from_lib(sym_id) {
                continue;
            }

            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                continue;
            };

            if symbol.declarations.len() <= 1 {
                continue;
            }

            // Handle constructors separately - they use TS2392 (multiple constructor implementations), not TS2300
            if symbol.escaped_name == "constructor" {
                // Count only constructor implementations (with body), not overloads (without body)
                let implementations: Vec<NodeIndex> = symbol
                    .declarations
                    .iter()
                    .filter_map(|&decl_idx| {
                        let constructor = self.ctx.arena.get_constructor_at(decl_idx)?;
                        // Only count constructors with a body as implementations
                        if !constructor.body.is_none() {
                            Some(decl_idx)
                        } else {
                            None
                        }
                    })
                    .collect();

                // Report TS2392 for multiple constructor implementations (not overloads)
                if implementations.len() > 1 {
                    let message =
                        diagnostic_messages::MULTIPLE_CONSTRUCTOR_IMPLEMENTATIONS_ARE_NOT_ALLOWED;
                    for &decl_idx in &implementations {
                        self.error_at_node(
                            decl_idx,
                            message,
                            diagnostic_codes::MULTIPLE_CONSTRUCTOR_IMPLEMENTATIONS_ARE_NOT_ALLOWED,
                        );
                    }
                }
                continue;
            }

            let mut declarations = Vec::new();
            for &decl_idx in &symbol.declarations {
                if let Some(flags) = self.declaration_symbol_flags(decl_idx) {
                    // When libs are loaded, verify the declaration name matches the symbol.
                    // Lib declarations may have NodeIndex values that overlap with user arena
                    // indices, pointing to unrelated user nodes. Filter these out.
                    if has_libs && !self.declaration_name_matches(decl_idx, &symbol.escaped_name) {
                        continue;
                    }
                    declarations.push((decl_idx, flags));
                }
            }

            if declarations.len() <= 1 {
                continue;
            }

            // TS2428: interface merges must have identical type parameters.
            let interface_decls: Vec<NodeIndex> = declarations
                .iter()
                .filter(|(_, flags)| (flags & symbol_flags::INTERFACE) != 0)
                .map(|(decl_idx, _)| *decl_idx)
                .collect();
            if interface_decls.len() > 1 {
                let baseline = self.interface_type_parameter_names(interface_decls[0]);
                let mismatch = interface_decls[1..]
                    .iter()
                    .any(|&decl_idx| self.interface_type_parameter_names(decl_idx) != baseline);
                if mismatch {
                    let message = format_message(
                        diagnostic_messages::ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_TYPE_PARAMETERS,
                        &[&symbol.escaped_name],
                    );
                    for decl_idx in interface_decls {
                        let error_node =
                            self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                        self.error_at_node(
                            error_node,
                            &message,
                            diagnostic_codes::ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_TYPE_PARAMETERS,
                        );
                    }
                }
            }

            self.check_merged_enum_declaration_diagnostics(&declarations);

            let mut conflicts = FxHashSet::default();
            let mut namespace_order_errors = FxHashSet::default();
            for i in 0..declarations.len() {
                for j in (i + 1)..declarations.len() {
                    let (decl_idx, decl_flags) = declarations[i];
                    let (other_idx, other_flags) = declarations[j];

                    // Skip conflict check if declarations are in different files
                    // (external modules are isolated, same-name declarations don't conflict)
                    // We check if both declarations are in the current file's arena
                    let both_in_current_file = self.ctx.arena.get(decl_idx).is_some()
                        && self.ctx.arena.get(other_idx).is_some();

                    // If either declaration is not in the current file's arena, they can't conflict
                    // This handles external modules where declarations in different files are isolated
                    if !both_in_current_file {
                        continue;
                    }

                    // Check for function overloads - multiple function declarations are allowed
                    // if at most one of them has a body (is an implementation)
                    let both_functions = (decl_flags & symbol_flags::FUNCTION) != 0
                        && (other_flags & symbol_flags::FUNCTION) != 0;
                    if both_functions {
                        let decl_has_body = self.function_has_body(decl_idx);
                        let other_has_body = self.function_has_body(other_idx);
                        // Only conflict if BOTH have bodies (multiple implementations)
                        if !(decl_has_body && other_has_body) {
                            continue;
                        }
                        // Both have bodies -> duplicate function implementations
                        // Force-add to conflicts since declarations_conflict returns false
                        // for FUNCTION vs FUNCTION (they don't exclude each other).
                        conflicts.insert(decl_idx);
                        conflicts.insert(other_idx);
                        continue;
                    }

                    // Check for method overloads - multiple method declarations are allowed
                    // if at most one of them has a body (is an implementation)
                    let both_methods = (decl_flags & symbol_flags::METHOD) != 0
                        && (other_flags & symbol_flags::METHOD) != 0;
                    if both_methods {
                        let decl_has_body = self.method_has_body(decl_idx);
                        let other_has_body = self.method_has_body(other_idx);
                        // Only conflict if BOTH have bodies (multiple implementations)
                        if !(decl_has_body && other_has_body) {
                            continue;
                        }
                    }

                    // Check for interface merging - multiple interface declarations are allowed
                    let both_interfaces = (decl_flags & symbol_flags::INTERFACE) != 0
                        && (other_flags & symbol_flags::INTERFACE) != 0;
                    if both_interfaces {
                        continue; // Interface merging is always allowed
                    }

                    // Check for enum merging - multiple enum declarations are allowed
                    let both_enums = (decl_flags & symbol_flags::ENUM) != 0
                        && (other_flags & symbol_flags::ENUM) != 0;
                    if both_enums {
                        continue; // Enum merging is always allowed
                    }

                    // Check for namespace merging - namespaces can merge with functions, classes, and each other
                    let decl_is_namespace = (decl_flags
                        & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE))
                        != 0;
                    let other_is_namespace = (other_flags
                        & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE))
                        != 0;

                    // Namespace + Namespace merging is allowed
                    if decl_is_namespace && other_is_namespace {
                        continue;
                    }

                    // Namespace + Function merging is allowed only when the namespace
                    // is non-instantiated OR declared after the function.
                    let decl_is_function = (decl_flags & symbol_flags::FUNCTION) != 0;
                    let other_is_function = (other_flags & symbol_flags::FUNCTION) != 0;
                    if (decl_is_namespace && other_is_function)
                        || (decl_is_function && other_is_namespace)
                    {
                        let (namespace_idx, function_idx) = if decl_is_namespace {
                            (decl_idx, other_idx)
                        } else {
                            (other_idx, decl_idx)
                        };

                        let namespace_is_instantiated =
                            self.is_namespace_declaration_instantiated(namespace_idx);
                        if !namespace_is_instantiated {
                            continue;
                        }

                        if self.is_ambient_function_declaration(function_idx) {
                            continue;
                        }

                        let namespace_precedes_function = self
                            .ctx
                            .arena
                            .get(namespace_idx)
                            .zip(self.ctx.arena.get(function_idx))
                            .is_some_and(|(ns_node, fn_node)| ns_node.pos < fn_node.pos);

                        if namespace_precedes_function {
                            namespace_order_errors.insert(namespace_idx);
                        }
                        continue;
                    }

                    // Namespace + Class merging is allowed only when the namespace
                    // is non-instantiated OR declared after the class.
                    let decl_is_class = (decl_flags & symbol_flags::CLASS) != 0;
                    let other_is_class = (other_flags & symbol_flags::CLASS) != 0;
                    if (decl_is_namespace && other_is_class)
                        || (decl_is_class && other_is_namespace)
                    {
                        let (namespace_idx, class_idx) = if decl_is_namespace {
                            (decl_idx, other_idx)
                        } else {
                            (other_idx, decl_idx)
                        };

                        let namespace_is_instantiated =
                            self.is_namespace_declaration_instantiated(namespace_idx);
                        if !namespace_is_instantiated {
                            continue;
                        }

                        if self.is_ambient_class_declaration(class_idx) {
                            continue;
                        }

                        let namespace_precedes_class = self
                            .ctx
                            .arena
                            .get(namespace_idx)
                            .zip(self.ctx.arena.get(class_idx))
                            .is_some_and(|(ns_node, class_node)| ns_node.pos < class_node.pos);

                        if namespace_precedes_class {
                            namespace_order_errors.insert(namespace_idx);
                        }
                        continue;
                    }

                    // Namespace + Enum merging is allowed
                    let decl_is_enum = (decl_flags & symbol_flags::ENUM) != 0;
                    let other_is_enum = (other_flags & symbol_flags::ENUM) != 0;
                    if (decl_is_namespace && other_is_enum) || (decl_is_enum && other_is_namespace)
                    {
                        continue;
                    }

                    // Namespace + Variable merging is allowed only for non-instantiated
                    // namespaces. Instantiated namespaces conflict with variables.
                    let decl_is_variable = (decl_flags & symbol_flags::VARIABLE) != 0;
                    let other_is_variable = (other_flags & symbol_flags::VARIABLE) != 0;
                    if (decl_is_namespace && other_is_variable)
                        || (decl_is_variable && other_is_namespace)
                    {
                        let namespace_idx = if decl_is_namespace {
                            decl_idx
                        } else {
                            other_idx
                        };
                        let namespace_is_instantiated =
                            self.is_namespace_declaration_instantiated(namespace_idx);
                        if namespace_is_instantiated {
                            conflicts.insert(decl_idx);
                            conflicts.insert(other_idx);
                        }
                        continue;
                    }

                    // Non-ambient class + Function: emit TS2813 + TS2814
                    // Note: class & function don't exclude each other in declarations_conflict,
                    // so we handle this case specially with early continue.
                    if (decl_is_class && other_is_function) || (decl_is_function && other_is_class)
                    {
                        let class_idx = if decl_is_class { decl_idx } else { other_idx };
                        if self.is_ambient_class_declaration(class_idx) {
                            continue;
                        }
                        // Non-ambient class + function detected — mark both for TS2813/TS2814
                        conflicts.insert(decl_idx);
                        conflicts.insert(other_idx);
                        continue;
                    }

                    // In merged namespaces, classes with the same name in different
                    // namespace blocks don't conflict (one exported, one local).
                    if decl_is_class && other_is_class {
                        let decl_ns = self.get_enclosing_namespace(decl_idx);
                        let other_ns = self.get_enclosing_namespace(other_idx);
                        // Both inside namespaces, but different namespace declaration blocks
                        if decl_ns != NodeIndex::NONE
                            && other_ns != NodeIndex::NONE
                            && decl_ns != other_ns
                        {
                            continue;
                        }
                    }

                    // Skip conflict between declarations in different block scopes.
                    // The binder may merge declarations into the same symbol even when they're
                    // in different scopes (e.g., var+let in switch blocks, let in separate blocks).
                    // Check if declarations share the same enclosing block scope.
                    let decl_is_var = (decl_flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0;
                    let other_is_var = (other_flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0;
                    let decl_is_block = (decl_flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0;
                    let other_is_block = (other_flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0;

                    // var + let/const: check if they're in different scopes
                    if (decl_is_var && other_is_block) || (decl_is_block && other_is_var) {
                        let block_idx = if decl_is_block { decl_idx } else { other_idx };
                        let block_scope = self.get_enclosing_block_scope(block_idx);
                        // If the block-scoped variable is inside any block scope,
                        // it's in a nested scope relative to the var
                        if block_scope != NodeIndex::NONE {
                            continue;
                        }
                    }
                    // let/const + let/const: check if they share the same block scope
                    if decl_is_block && other_is_block {
                        let decl_scope = self.get_enclosing_block_scope(decl_idx);
                        let other_scope = self.get_enclosing_block_scope(other_idx);
                        if decl_scope != other_scope {
                            continue;
                        }
                    }

                    if Self::declarations_conflict(decl_flags, other_flags) {
                        conflicts.insert(decl_idx);
                        conflicts.insert(other_idx);
                    }
                }
            }

            for &namespace_idx in &namespace_order_errors {
                let error_node = self
                    .get_declaration_name_node(namespace_idx)
                    .unwrap_or(namespace_idx);
                self.error_at_node(
                    error_node,
                    diagnostic_messages::A_NAMESPACE_DECLARATION_CANNOT_BE_LOCATED_PRIOR_TO_A_CLASS_OR_FUNCTION_WITH_WHIC,
                    diagnostic_codes::A_NAMESPACE_DECLARATION_CANNOT_BE_LOCATED_PRIOR_TO_A_CLASS_OR_FUNCTION_WITH_WHIC,
                );
            }

            if conflicts.is_empty() {
                continue;
            }

            // Handle TS2393: Duplicate function implementation.
            // When 2+ function declarations with bodies share a name, emit TS2393 on each.
            // This runs BEFORE TS2813/TS2814 handling since that removes function indices.
            {
                let duplicate_func_impls: Vec<NodeIndex> = declarations
                    .iter()
                    .filter(|(decl_idx, flags)| {
                        conflicts.contains(decl_idx)
                            && (flags & symbol_flags::FUNCTION) != 0
                            && self.function_has_body(*decl_idx)
                    })
                    .map(|(idx, _)| *idx)
                    .collect();

                if duplicate_func_impls.len() > 1 {
                    for &idx in &duplicate_func_impls {
                        let error_node = self.get_declaration_name_node(idx).unwrap_or(idx);
                        self.error_at_node(
                            error_node,
                            diagnostic_messages::DUPLICATE_FUNCTION_IMPLEMENTATION,
                            diagnostic_codes::DUPLICATE_FUNCTION_IMPLEMENTATION,
                        );
                        conflicts.remove(&idx);
                    }
                    if conflicts.is_empty() {
                        continue;
                    }
                }
            }

            // Check for class + function conflicts (TS2813 + TS2814)
            // These get special diagnostics instead of the generic TS2300
            let has_class_function_conflict = {
                let has_class = declarations.iter().any(|(decl_idx, flags)| {
                    conflicts.contains(decl_idx) && (flags & symbol_flags::CLASS) != 0
                });
                let has_function = declarations.iter().any(|(decl_idx, flags)| {
                    conflicts.contains(decl_idx) && (flags & symbol_flags::FUNCTION) != 0
                });
                has_class && has_function
            };

            if has_class_function_conflict {
                let name = symbol.escaped_name.clone();

                // Emit TS2813 on class declarations
                for &(decl_idx, flags) in &declarations {
                    if conflicts.contains(&decl_idx) && (flags & symbol_flags::CLASS) != 0 {
                        let error_node =
                            self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                        let message = format_message(
                            diagnostic_messages::CLASS_DECLARATION_CANNOT_IMPLEMENT_OVERLOAD_LIST_FOR,
                            &[&name],
                        );
                        self.error_at_node(
                            error_node,
                            &message,
                            diagnostic_codes::CLASS_DECLARATION_CANNOT_IMPLEMENT_OVERLOAD_LIST_FOR,
                        );
                    }
                }

                // Emit TS2814 on function declarations
                for &(decl_idx, flags) in &declarations {
                    if conflicts.contains(&decl_idx) && (flags & symbol_flags::FUNCTION) != 0 {
                        let error_node =
                            self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                        self.error_at_node(
                            error_node,
                            diagnostic_messages::FUNCTION_WITH_BODIES_CAN_ONLY_MERGE_WITH_CLASSES_THAT_ARE_AMBIENT,
                            diagnostic_codes::FUNCTION_WITH_BODIES_CAN_ONLY_MERGE_WITH_CLASSES_THAT_ARE_AMBIENT,
                        );
                    }
                }

                // Remove class/function entries from conflicts so they don't also get TS2300
                let class_function_indices: Vec<NodeIndex> = declarations
                    .iter()
                    .filter(|(decl_idx, flags)| {
                        conflicts.contains(decl_idx)
                            && ((flags & symbol_flags::CLASS) != 0
                                || (flags & symbol_flags::FUNCTION) != 0)
                    })
                    .map(|(idx, _)| *idx)
                    .collect();
                for idx in class_function_indices {
                    conflicts.remove(&idx);
                }

                if conflicts.is_empty() {
                    continue;
                }
            }

            // Check if we have any non-block-scoped declarations (var, function, etc.)
            // Imports (ALIAS) and let/const (BLOCK_SCOPED_VARIABLE) are block-scoped
            let has_non_block_scoped = declarations.iter().any(|(decl_idx, flags)| {
                conflicts.contains(decl_idx) && {
                    (flags & (symbol_flags::BLOCK_SCOPED_VARIABLE | symbol_flags::ALIAS)) == 0
                }
            });

            let name = symbol.escaped_name.clone();

            // Check if any conflicting declaration is an enum
            let has_enum_conflict = declarations.iter().any(|(decl_idx, flags)| {
                conflicts.contains(decl_idx)
                    && (flags & (symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM)) != 0
            });

            let decl_is_exported = |decl_idx: NodeIndex| {
                let Some(node) = self.ctx.arena.get(decl_idx) else {
                    return false;
                };
                match node.kind {
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                        self.ctx.arena.get_function(node).is_some_and(|func| {
                            self.ctx
                                .has_modifier(&func.modifiers, SyntaxKind::ExportKeyword as u16)
                        })
                    }
                    k if k == syntax_kind_ext::CLASS_DECLARATION => {
                        self.ctx.arena.get_class(node).is_some_and(|class| {
                            self.ctx
                                .has_modifier(&class.modifiers, SyntaxKind::ExportKeyword as u16)
                        })
                    }
                    k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                        self.ctx.arena.get_interface(node).is_some_and(|iface| {
                            self.ctx
                                .has_modifier(&iface.modifiers, SyntaxKind::ExportKeyword as u16)
                        })
                    }
                    k if k == syntax_kind_ext::ENUM_DECLARATION => {
                        self.ctx.arena.get_enum(node).is_some_and(|enm| {
                            self.ctx
                                .has_modifier(&enm.modifiers, SyntaxKind::ExportKeyword as u16)
                        })
                    }
                    k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                        self.ctx.arena.get_type_alias(node).is_some_and(|alias| {
                            self.ctx
                                .has_modifier(&alias.modifiers, SyntaxKind::ExportKeyword as u16)
                        })
                    }
                    k if k == syntax_kind_ext::MODULE_DECLARATION => {
                        self.ctx.arena.get_module(node).is_some_and(|module| {
                            self.ctx
                                .has_modifier(&module.modifiers, SyntaxKind::ExportKeyword as u16)
                        })
                    }
                    k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                        if let Some(ext) = self.ctx.arena.get_extended(decl_idx)
                            && let Some(list_ext) = self.ctx.arena.get_extended(ext.parent)
                            && let Some(stmt_node) = self.ctx.arena.get(list_ext.parent)
                            && stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                            && let Some(var_stmt) = self.ctx.arena.get_variable(stmt_node)
                        {
                            self.ctx
                                .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword as u16)
                        } else {
                            false
                        }
                    }
                    _ => false,
                }
            };

            let has_variable_conflict = declarations.iter().any(|(decl_idx, flags)| {
                conflicts.contains(decl_idx) && (flags & symbol_flags::VARIABLE) != 0
            });
            let has_non_variable_conflict = declarations.iter().any(|(decl_idx, flags)| {
                conflicts.contains(decl_idx) && (flags & symbol_flags::VARIABLE) == 0
            });
            let has_exported_variable_conflict = declarations.iter().any(|(decl_idx, flags)| {
                conflicts.contains(decl_idx)
                    && (flags & symbol_flags::VARIABLE) != 0
                    && decl_is_exported(*decl_idx)
            });

            let (message, code) = if has_exported_variable_conflict
                && has_variable_conflict
                && !has_non_variable_conflict
            {
                (
                    format_message(
                        diagnostic_messages::CANNOT_REDECLARE_EXPORTED_VARIABLE,
                        &[&name],
                    ),
                    diagnostic_codes::CANNOT_REDECLARE_EXPORTED_VARIABLE,
                )
            } else if has_enum_conflict && has_non_block_scoped {
                // Enum merging conflict: TS2567
                (
                    diagnostic_messages::ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS
                        .to_string(),
                    diagnostic_codes::ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS,
                )
            } else if !has_non_block_scoped {
                // Pure block-scoped duplicates (let/const/import conflicts) emit TS2451
                (
                    format_message(
                        diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                        &[&name],
                    ),
                    diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                )
            } else {
                // Mixed or non-block-scoped duplicates emit TS2300
                (
                    format_message(diagnostic_messages::DUPLICATE_IDENTIFIER, &[&name]),
                    diagnostic_codes::DUPLICATE_IDENTIFIER,
                )
            };
            for (decl_idx, _) in declarations {
                if conflicts.contains(&decl_idx) {
                    let error_node = self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                    self.error_at_node(error_node, &message, code);
                }
            }
        }
    }

    /// Check if a function declaration has a body (is an implementation, not just a signature).
    pub(crate) fn function_has_body(&self, decl_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
            return false;
        }
        let Some(func) = self.ctx.arena.get_function(node) else {
            return false;
        };
        !func.body.is_none()
    }

    /// Get the NodeIndex of the nearest enclosing MODULE_DECLARATION (namespace) for a declaration.
    /// Returns NodeIndex::NONE if the declaration is not inside a namespace.
    fn get_enclosing_namespace(&self, decl_idx: NodeIndex) -> NodeIndex {
        use tsz_parser::parser::syntax_kind_ext;
        let mut current = decl_idx;
        loop {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return NodeIndex::NONE;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return NodeIndex::NONE;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return NodeIndex::NONE;
            };
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                return parent;
            }
            if parent_node.kind == syntax_kind_ext::SOURCE_FILE {
                return NodeIndex::NONE;
            }
            current = parent;
        }
    }

    /// Get the NodeIndex of the nearest enclosing block scope for a declaration.
    /// Returns the first Block, CaseBlock, ForStatement, etc. ancestor.
    /// Returns NodeIndex::NONE if the declaration is directly in a function/module scope.
    fn get_enclosing_block_scope(&self, decl_idx: NodeIndex) -> NodeIndex {
        use tsz_parser::parser::syntax_kind_ext;
        let mut current = decl_idx;
        loop {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return NodeIndex::NONE;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return NodeIndex::NONE;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return NodeIndex::NONE;
            };
            match parent_node.kind {
                // Block-creating scopes - return this as the enclosing scope
                syntax_kind_ext::BLOCK
                | syntax_kind_ext::CASE_BLOCK
                | syntax_kind_ext::FOR_STATEMENT
                | syntax_kind_ext::FOR_IN_STATEMENT
                | syntax_kind_ext::FOR_OF_STATEMENT => {
                    return parent;
                }
                // Function/module boundaries - no enclosing block scope
                syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::FUNCTION_EXPRESSION
                | syntax_kind_ext::ARROW_FUNCTION
                | syntax_kind_ext::MODULE_DECLARATION
                | syntax_kind_ext::SOURCE_FILE => {
                    return NodeIndex::NONE;
                }
                _ => {}
            }
            current = parent;
        }
    }

    /// Check diagnostics specific to merged enum declarations.
    ///
    /// - TS2432: In an enum with multiple declarations, only one declaration can
    ///   omit an initializer for its first enum element.
    /// - TS2300: Duplicate enum member names across different enum declarations.
    fn check_merged_enum_declaration_diagnostics(&mut self, declarations: &[(NodeIndex, u32)]) {
        use crate::diagnostics::diagnostic_codes;
        use rustc_hash::FxHashMap;

        let enum_declarations: Vec<NodeIndex> = declarations
            .iter()
            .filter_map(|(decl_idx, flags)| {
                if (flags & symbol_flags::ENUM) != 0 {
                    Some(*decl_idx)
                } else {
                    None
                }
            })
            .collect();

        if enum_declarations.len() <= 1 {
            return;
        }

        let mut first_member_without_initializer = Vec::new();
        let mut first_decl_for_member_by_name: FxHashMap<String, NodeIndex> = FxHashMap::default();

        for &enum_decl_idx in &enum_declarations {
            let Some(enum_decl_node) = self.ctx.arena.get(enum_decl_idx) else {
                continue;
            };
            let Some(enum_decl) = self.ctx.arena.get_enum(enum_decl_node) else {
                continue;
            };

            if let Some(&first_member_idx) = enum_decl.members.nodes.first()
                && let Some(first_member_node) = self.ctx.arena.get(first_member_idx)
                && let Some(first_member) = self.ctx.arena.get_enum_member(first_member_node)
                && first_member.initializer.is_none()
            {
                first_member_without_initializer.push(first_member_idx);
            }

            for &member_idx in &enum_decl.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
                    continue;
                };
                let Some(member_name_node) = self.ctx.arena.get(member.name) else {
                    continue;
                };

                let member_name =
                    if let Some(ident) = self.ctx.arena.get_identifier(member_name_node) {
                        ident.escaped_text.clone()
                    } else if let Some(literal) = self.ctx.arena.get_literal(member_name_node) {
                        literal.text.clone()
                    } else {
                        continue;
                    };

                if let Some(&first_decl_idx) = first_decl_for_member_by_name.get(&member_name) {
                    if first_decl_idx != enum_decl_idx {
                        self.error_at_node_msg(
                            member.name,
                            diagnostic_codes::DUPLICATE_IDENTIFIER,
                            &[&member_name],
                        );
                    }
                } else {
                    first_decl_for_member_by_name.insert(member_name.clone(), enum_decl_idx);
                }
            }
        }

        if first_member_without_initializer.len() > 1 {
            // The first declaration that omits an initializer is allowed;
            // only subsequent ones get TS2432.
            for &member_idx in &first_member_without_initializer[1..] {
                self.error_at_node_msg(
                    member_idx,
                    diagnostic_codes::IN_AN_ENUM_WITH_MULTIPLE_DECLARATIONS_ONLY_ONE_DECLARATION_CAN_OMIT_AN_INITIALIZ,
                    &[],
                );
            }
        }
    }

    /// Check for unused declarations (TS6133, TS6192).
    /// Reports variables, functions, classes, and other declarations that are never referenced.
    /// Also reports import declarations where ALL imports are unused (TS6192).
    pub(crate) fn check_unused_declarations(&mut self) {
        use crate::diagnostics::Diagnostic;
        use std::collections::{HashMap, HashSet};
        use tsz_binder::ContainerKind;
        use tsz_binder::symbol_flags;

        let check_locals = self.ctx.no_unused_locals();
        let check_params = self.ctx.no_unused_parameters();
        let is_module = self.ctx.binder.is_external_module();

        // Skip .d.ts files entirely (ambient declarations)
        if self.ctx.file_name.ends_with(".d.ts") {
            return;
        }

        // Collect symbols from scopes.
        // For script files (non-module), skip the root SourceFile scope since
        // top-level declarations are globals and not checked by noUnusedLocals.
        // For module files, check all scopes including root.
        let mut symbols_to_check: Vec<(tsz_binder::SymbolId, String)> = Vec::new();

        for scope in &self.ctx.binder.scopes {
            // Skip root scope in script files
            if !is_module && scope.kind == ContainerKind::SourceFile {
                continue;
            }
            for (name, &sym_id) in scope.table.iter() {
                // Skip lib-originating symbols (e.g. from lib.d.ts)
                if self.ctx.binder.lib_symbol_ids.contains(&sym_id) {
                    continue;
                }
                symbols_to_check.push((sym_id, name.clone()));
            }
        }

        let file_name = self.ctx.file_name.clone();

        // Track import declarations for TS6192.
        // Map from import declaration NodeIndex to (total_count, unused_count).
        let mut import_declarations: HashMap<NodeIndex, (usize, usize)> = HashMap::new();

        // Track variable declarations for TS6199.
        // Map from variable declaration NodeIndex to (total_count, unused_count).
        let mut variable_declarations: HashMap<NodeIndex, (usize, usize)> = HashMap::new();

        // Track destructuring patterns for TS6198.
        // Map from binding pattern NodeIndex to (total_elements, unused_elements).
        let destructuring_patterns: HashMap<NodeIndex, (usize, usize)> = HashMap::new();

        // First pass: identify ALL import symbols and track them by import declaration.
        // This includes both used and unused imports.
        for (_sym_id, _name) in &symbols_to_check {
            let sym_id = *_sym_id;
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                continue;
            };
            let flags = symbol.flags;

            // Only track ALIAS symbols (imports)
            if (flags & symbol_flags::ALIAS) == 0 {
                continue;
            }

            // Get the declaration node
            let decl_idx = if !symbol.value_declaration.is_none() {
                symbol.value_declaration
            } else if let Some(&first) = symbol.declarations.first() {
                first
            } else {
                continue;
            };

            // Find the parent IMPORT_DECLARATION node
            if let Some(import_decl_idx) = self.find_parent_import_declaration(decl_idx) {
                let is_used = self.ctx.referenced_symbols.borrow().contains(&sym_id);
                let entry = import_declarations.entry(import_decl_idx).or_insert((0, 0));
                entry.0 += 1; // total count
                if !is_used {
                    entry.1 += 1; // unused count
                }
            }
        }

        // Second pass: track variable declarations (for TS6199)
        // We need to track VARIABLE_DECLARATION nodes (not individual variables)
        // to distinguish `var x, y;` (2 decls) from `const {a, b} = obj;` (1 decl with multiple bindings)
        let mut var_decl_list_children: HashMap<NodeIndex, HashSet<NodeIndex>> = HashMap::new();
        let mut unused_var_decls: HashSet<NodeIndex> = HashSet::new();
        let mut pattern_children: HashMap<NodeIndex, HashSet<NodeIndex>> = HashMap::new();
        let mut unused_pattern_elements: HashSet<NodeIndex> = HashSet::new();

        for (_sym_id, _name) in &symbols_to_check {
            let sym_id = *_sym_id;
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                continue;
            };
            let flags = symbol.flags;

            // Only track variables (not imports, not parameters)
            let is_var = (flags
                & (symbol_flags::BLOCK_SCOPED_VARIABLE | symbol_flags::FUNCTION_SCOPED_VARIABLE))
                != 0;
            if !is_var {
                continue;
            }

            // Get the declaration node
            let decl_idx = if !symbol.value_declaration.is_none() {
                symbol.value_declaration
            } else if let Some(&first) = symbol.declarations.first() {
                first
            } else {
                continue;
            };

            // Skip if this is a parameter
            if self.is_parameter_declaration(decl_idx) {
                continue;
            }

            // Find the parent VARIABLE_DECLARATION and VARIABLE_DECLARATION_LIST
            if let Some(var_decl_node_idx) = self.find_parent_variable_decl_node(decl_idx)
                && let Some(var_decl_list_idx) =
                    self.find_parent_variable_declaration(var_decl_node_idx)
            {
                // Track this VARIABLE_DECLARATION node under its parent list
                var_decl_list_children
                    .entry(var_decl_list_idx)
                    .or_default()
                    .insert(var_decl_node_idx);

                // Check if this variable is unused
                let is_used = self.ctx.referenced_symbols.borrow().contains(&sym_id);
                if !is_used {
                    unused_var_decls.insert(var_decl_node_idx);
                }

                if let Some(pattern_idx) = self.find_parent_binding_pattern(decl_idx) {
                    pattern_children
                        .entry(pattern_idx)
                        .or_default()
                        .insert(decl_idx);
                    if !is_used {
                        unused_pattern_elements.insert(decl_idx);
                    }
                }
            }
        }

        // Now count VARIABLE_DECLARATION nodes (not variables) in each list
        for (var_decl_list_idx, decl_nodes) in &var_decl_list_children {
            let total_count = decl_nodes.len();
            let unused_count = decl_nodes
                .iter()
                .filter(|n| unused_var_decls.contains(n))
                .count();
            variable_declarations.insert(*var_decl_list_idx, (total_count, unused_count));
        }

        for (sym_id, name) in symbols_to_check {
            // Skip if already referenced
            if self.ctx.referenced_symbols.borrow().contains(&sym_id) {
                continue;
            }

            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                continue;
            };

            let flags = symbol.flags;

            // Skip exported symbols — they may be used externally
            if symbol.is_exported || (flags & symbol_flags::EXPORT_VALUE) != 0 {
                continue;
            }

            // Skip special/internal names
            if name == "default" || name == "__export" || name == "arguments" || name == "React"
            // JSX factory — always considered used when JSX is enabled
            {
                continue;
            }

            // Skip type parameters — they are handled separately (not in binder scope)
            if (flags & symbol_flags::TYPE_PARAMETER) != 0 {
                continue;
            }

            // Skip non-private members (constructors, signatures, enum members, prototype)
            // Private members ARE checked under noUnusedLocals (TS6133)
            let is_member = (flags
                & (symbol_flags::PROPERTY
                    | symbol_flags::METHOD
                    | symbol_flags::GET_ACCESSOR
                    | symbol_flags::SET_ACCESSOR))
                != 0;
            if is_member {
                // Only private members get unused checking — use PRIVATE flag set by binder
                let is_private = (flags & symbol_flags::PRIVATE) != 0;
                if !is_private {
                    continue; // Public/protected members may be used externally
                }
                // Setter-only private members are "used" by write accesses.
                // TSC never flags them as unused since writes count as usage.
                let is_setter_only = (flags & symbol_flags::SET_ACCESSOR) != 0
                    && (flags & symbol_flags::GET_ACCESSOR) == 0;
                if is_setter_only {
                    continue;
                }
                // Fall through to check private members
            }

            // Always skip constructors, signatures, enum members, prototype
            if (flags
                & (symbol_flags::CONSTRUCTOR
                    | symbol_flags::SIGNATURE
                    | symbol_flags::ENUM_MEMBER
                    | symbol_flags::PROTOTYPE))
                != 0
            {
                continue;
            }

            // Get the declaration node for position info
            let decl_idx = if !symbol.value_declaration.is_none() {
                symbol.value_declaration
            } else if let Some(&first) = symbol.declarations.first() {
                first
            } else {
                continue;
            };

            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };

            // Ambient declarations (`declare ...` or declarations nested in ambient
            // contexts) are not checked by noUnusedLocals.
            if self.is_ambient_declaration(decl_idx) {
                continue;
            }

            // Skip catch clause variables — TSC exempts them from unused checking
            if self.is_catch_clause_variable(decl_idx) {
                continue;
            }

            // Skip using/await using declarations — they always have dispose side effects
            if self.is_using_declaration(decl_idx) {
                continue;
            }

            // Skip named function expression names — TSC never flags these as unused.
            // `var x = function somefn() {}` binds `somefn` in its own scope.
            if decl_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                && (flags & symbol_flags::FUNCTION) != 0
            {
                continue;
            }

            // Determine what kind of symbol this is and whether we should check it
            if check_locals {
                // Check local variables, functions, classes, interfaces, type aliases, imports
                let is_checkable_local = (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0
                    || (flags & symbol_flags::FUNCTION) != 0
                    || (flags & symbol_flags::CLASS) != 0
                    || (flags & symbol_flags::INTERFACE) != 0
                    || (flags & symbol_flags::TYPE_ALIAS) != 0
                    || (flags & symbol_flags::ALIAS) != 0  // imports
                    || (flags & symbol_flags::REGULAR_ENUM) != 0
                    || (flags & symbol_flags::CONST_ENUM) != 0;

                // var declarations that aren't parameters
                let is_var = (flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0
                    && !self.is_parameter_declaration(decl_idx);

                // Private class members (property, method, accessor)
                let is_private_member = is_member;

                // Non-exported namespaces/modules
                let is_unused_namespace =
                    (flags & (symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE)) != 0;

                if is_checkable_local || is_var || is_private_member || is_unused_namespace {
                    // For imports, check if this is part of an import declaration where ALL imports are unused.
                    // If so, skip emitting TS6133 here because TS6192 will be emitted for the entire declaration.
                    // Only skip when there are MULTIPLE imports (single unused imports get TS6133).
                    let is_import = (flags & symbol_flags::ALIAS) != 0;
                    let skip_import_ts6133 = if is_import {
                        if let Some(import_decl_idx) = self.find_parent_import_declaration(decl_idx)
                        {
                            if let Some(&(total_count, unused_count)) =
                                import_declarations.get(&import_decl_idx)
                            {
                                // Skip TS6133 only if there are multiple imports and ALL are unused (TS6192 will cover it)
                                total_count > 1 && unused_count == total_count
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    // For variables, check if this is part of a variable declaration where ALL variables are unused.
                    // If so, skip emitting TS6133 here because TS6199 will be emitted for the entire declaration.
                    // Only skip when there are MULTIPLE variables (single unused variables get TS6133).
                    let is_variable = (flags
                        & (symbol_flags::BLOCK_SCOPED_VARIABLE
                            | symbol_flags::FUNCTION_SCOPED_VARIABLE))
                        != 0
                        && !self.is_parameter_declaration(decl_idx);
                    let skip_variable_ts6133 = if is_variable {
                        if let Some(var_decl_idx) = self.find_parent_variable_declaration(decl_idx)
                        {
                            if let Some(&(total_count, unused_count)) =
                                variable_declarations.get(&var_decl_idx)
                            {
                                // Skip TS6133 only if there are multiple variables and ALL are unused (TS6199 will cover it)
                                total_count > 1 && unused_count == total_count
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    // For destructuring patterns, check if this is part of a binding pattern where ALL elements are unused.
                    // If so, skip emitting TS6133 here because TS6198 will be emitted for the entire pattern.
                    // Only skip when there are MULTIPLE elements (single unused elements get TS6133).
                    let skip_destructuring_ts6133 = if is_variable {
                        if let Some(pattern_idx) = self.find_parent_binding_pattern(decl_idx) {
                            if let Some(&(total_count, unused_count)) =
                                destructuring_patterns.get(&pattern_idx)
                            {
                                // Skip TS6133 only if there are multiple elements and ALL are unused (TS6198 will cover it)
                                total_count > 1 && unused_count == total_count
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if !skip_import_ts6133 && !skip_variable_ts6133 && !skip_destructuring_ts6133 {
                        // Check if write-only (assigned but never read)
                        // Destructured variables should NOT get TS6198 - they get TS6133
                        let is_destructured = self.find_parent_binding_pattern(decl_idx).is_some();
                        let is_write_only =
                            !is_destructured && self.ctx.written_symbols.borrow().contains(&sym_id);

                        // TS6196 for classes, interfaces, type aliases, enums ("never used")
                        // TS6198 for write-only variables ("assigned but never used")
                        // TS6133 for variables, functions, imports, class properties ("value never read")
                        // Note: TS6138 ("Property 'x' is declared but its value is never read")
                        // is only for constructor parameter properties, handled in the parameter section below.
                        let is_type_only = (flags & symbol_flags::CLASS) != 0
                            || (flags & symbol_flags::INTERFACE) != 0
                            || (flags & symbol_flags::TYPE_ALIAS) != 0
                            || (flags & symbol_flags::REGULAR_ENUM) != 0
                            || (flags & symbol_flags::CONST_ENUM) != 0;
                        let (msg, code) = if is_type_only {
                            (format!("'{}' is declared but never used.", name), 6196)
                        } else if is_write_only {
                            (
                                format!("'{}' is assigned a value but never used.", name),
                                6198,
                            )
                        } else {
                            (
                                format!("'{}' is declared but its value is never read.", name),
                                6133,
                            )
                        };
                        let start = decl_node.pos;
                        let length = decl_node.end.saturating_sub(decl_node.pos);
                        self.ctx.push_diagnostic(Diagnostic {
                            file: file_name.clone(),
                            start,
                            length,
                            message_text: msg,
                            category: crate::diagnostics::DiagnosticCategory::Error,
                            code,
                            related_information: Vec::new(),
                        });
                    }
                }
            }

            if check_params {
                // Check function parameters (but not catch clause or overload signature params)
                let is_param = (flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0
                    && self.is_parameter_declaration(decl_idx)
                    && !self.is_overload_signature_parameter(decl_idx);

                // Skip `this` parameter — it's a TypeScript type annotation, not an actual parameter
                if is_param && name == "this" {
                    continue;
                }

                // Skip parameters starting with _ (TSC convention for intentionally unused)
                if is_param && name.starts_with('_') {
                    continue;
                }

                if is_param {
                    let msg = format!("'{}' is declared but its value is never read.", name);
                    let start = decl_node.pos;
                    let length = decl_node.end.saturating_sub(decl_node.pos);
                    self.ctx.push_diagnostic(Diagnostic {
                        file: file_name.clone(),
                        start,
                        length,
                        message_text: msg,
                        category: crate::diagnostics::DiagnosticCategory::Error,
                        code: 6133,
                        related_information: Vec::new(),
                    });
                }
            }
        }

        // Emit TS6192 for import declarations where ALL imports are unused.
        // Only emit this when there are MULTIPLE imports (total_count > 1).
        // For single unused imports, TS6133 is emitted above.
        if check_locals {
            for (import_decl_idx, (total_count, unused_count)) in import_declarations {
                // Only emit if there are multiple imports and ALL are unused
                if total_count > 1
                    && unused_count == total_count
                    && let Some(import_decl_node) = self.ctx.arena.get(import_decl_idx)
                {
                    let msg = "All imports in import declaration are unused.".to_string();
                    let start = import_decl_node.pos;
                    let length = import_decl_node.end.saturating_sub(import_decl_node.pos);
                    self.ctx.push_diagnostic(Diagnostic {
                        file: file_name.clone(),
                        start,
                        length,
                        message_text: msg,
                        category: crate::diagnostics::DiagnosticCategory::Error,
                        code: 6192,
                        related_information: Vec::new(),
                    });
                }
            }

            // Emit TS6199 for variable declarations where ALL variables are unused.
            // Only emit this when there are MULTIPLE variables (total_count > 1).
            // For single unused variables, TS6133 is emitted above.
            for (var_decl_idx, (total_count, unused_count)) in variable_declarations {
                // Only emit if there are multiple variables and ALL are unused
                if total_count > 1
                    && unused_count == total_count
                    && let Some(var_decl_node) = self.ctx.arena.get(var_decl_idx)
                {
                    let msg = "All variables are unused.".to_string();
                    let start = var_decl_node.pos;
                    let length = var_decl_node.end.saturating_sub(var_decl_node.pos);
                    self.ctx.push_diagnostic(Diagnostic {
                        file: file_name.clone(),
                        start,
                        length,
                        message_text: msg,
                        category: crate::diagnostics::DiagnosticCategory::Error,
                        code: 6199,
                        related_information: Vec::new(),
                    });
                }
            }

            // Emit TS6198 for destructuring patterns where ALL elements are unused.
            // Only emit this when there are MULTIPLE elements (total_count > 1).
            // For single unused elements, TS6133 is emitted above.
            for (pattern_idx, (total_count, unused_count)) in destructuring_patterns {
                // Only emit if there are multiple elements and ALL are unused
                if total_count > 1
                    && unused_count == total_count
                    && let Some(pattern_node) = self.ctx.arena.get(pattern_idx)
                {
                    let msg = "All destructured elements are unused.".to_string();
                    let start = pattern_node.pos;
                    let length = pattern_node.end.saturating_sub(pattern_node.pos);
                    self.ctx.push_diagnostic(Diagnostic {
                        file: file_name.clone(),
                        start,
                        length,
                        message_text: msg,
                        category: crate::diagnostics::DiagnosticCategory::Error,
                        code: 6198,
                        related_information: Vec::new(),
                    });
                }
            }
        }
    }

    /// Find the parent IMPORT_DECLARATION node for an import symbol's declaration.
    fn find_parent_import_declaration(&self, mut idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        // Walk up the parent chain to find IMPORT_DECLARATION
        for _ in 0..10 {
            // Limit iterations to prevent infinite loops
            if idx.is_none() {
                return None;
            }

            if let Some(node) = self.ctx.arena.get(idx)
                && node.kind == syntax_kind_ext::IMPORT_DECLARATION
            {
                return Some(idx);
            }

            // Move to parent
            idx = self
                .ctx
                .arena
                .get_extended(idx)
                .map(|ext| ext.parent)
                .unwrap_or(NodeIndex::NONE);
        }

        None
    }

    /// Find the parent VARIABLE_DECLARATION node for a variable symbol's declaration.
    /// This returns the VARIABLE_DECLARATION node itself, not the VARIABLE_DECLARATION_LIST.
    fn find_parent_variable_decl_node(&self, mut idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        // Walk up the parent chain to find VARIABLE_DECLARATION
        for _ in 0..10 {
            // Limit iterations to prevent infinite loops
            if idx.is_none() {
                return None;
            }

            if let Some(node) = self.ctx.arena.get(idx)
                && node.kind == syntax_kind_ext::VARIABLE_DECLARATION
            {
                return Some(idx);
            }

            // Move to parent
            idx = self
                .ctx
                .arena
                .get_extended(idx)
                .map(|ext| ext.parent)
                .unwrap_or(NodeIndex::NONE);
        }

        None
    }

    /// Find the parent VARIABLE_DECLARATION_LIST node for a variable symbol's declaration.
    /// This allows us to track all variables declared in a single statement (e.g., `var x, y;`).
    fn find_parent_variable_declaration(&self, mut idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        // Walk up the parent chain to find VARIABLE_DECLARATION_LIST
        for _ in 0..10 {
            // Limit iterations to prevent infinite loops
            if idx.is_none() {
                return None;
            }

            if let Some(node) = self.ctx.arena.get(idx)
                && node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            {
                return Some(idx);
            }

            // Move to parent
            idx = self
                .ctx
                .arena
                .get_extended(idx)
                .map(|ext| ext.parent)
                .unwrap_or(NodeIndex::NONE);
        }

        None
    }

    /// Find the parent BINDING_PATTERN (OBJECT_BINDING_PATTERN or ARRAY_BINDING_PATTERN)
    /// for a binding element declaration. This is used to track TS6198 (all destructured
    /// elements are unused).
    fn find_parent_binding_pattern(&self, mut idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        // Walk up the parent chain to find OBJECT_BINDING_PATTERN or ARRAY_BINDING_PATTERN
        for _ in 0..10 {
            // Limit iterations to prevent infinite loops
            if idx.is_none() {
                return None;
            }

            if let Some(node) = self.ctx.arena.get(idx)
                && (node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                    || node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
            {
                return Some(idx);
            }

            // Move to parent
            idx = self
                .ctx
                .arena
                .get_extended(idx)
                .map(|ext| ext.parent)
                .unwrap_or(NodeIndex::NONE);
        }

        None
    }

    /// Check if a declaration node is a parameter declaration.
    fn is_parameter_declaration(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        node.kind == syntax_kind_ext::PARAMETER
    }

    /// Check if a declaration is a `using` or `await using` variable.
    /// These always have dispose side effects, so TSC never flags them as unused.
    fn is_using_declaration(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::flags::node_flags;
        use tsz_parser::parser::syntax_kind_ext;
        let parent_idx = self
            .ctx
            .arena
            .get_extended(idx)
            .map(|ext| ext.parent)
            .unwrap_or(NodeIndex::NONE);
        if parent_idx.is_none() {
            return false;
        }
        if let Some(parent) = self.ctx.arena.get(parent_idx)
            && parent.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
        {
            let flags = parent.flags as u32;
            if (flags & node_flags::USING) != 0 {
                return true;
            }
        }
        false
    }

    /// Check if a declaration is a catch clause variable.
    fn is_catch_clause_variable(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let parent_idx = self
            .ctx
            .arena
            .get_extended(idx)
            .map(|ext| ext.parent)
            .unwrap_or(NodeIndex::NONE);
        if parent_idx.is_none() {
            return false;
        }
        if let Some(parent) = self.ctx.arena.get(parent_idx)
            && parent.kind == syntax_kind_ext::CATCH_CLAUSE
        {
            return true;
        }
        false
    }

    /// Check if a parameter is in an overload signature (function/method without body).
    /// TSC does not flag parameters in overload signatures as unused.
    fn is_overload_signature_parameter(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        // Walk up from parameter to find containing function/method/constructor
        // Structure: Parameter → SyntaxList/ParameterList → FunctionDecl/MethodDecl/Constructor
        let mut current = idx;
        for _ in 0..5 {
            let parent_idx = self
                .ctx
                .arena
                .get_extended(current)
                .map(|ext| ext.parent)
                .unwrap_or(NodeIndex::NONE);
            if parent_idx.is_none() {
                return false;
            }
            if let Some(parent_node) = self.ctx.arena.get(parent_idx) {
                match parent_node.kind {
                    syntax_kind_ext::FUNCTION_DECLARATION
                    | syntax_kind_ext::ARROW_FUNCTION
                    | syntax_kind_ext::FUNCTION_EXPRESSION => {
                        if let Some(func) = self.ctx.arena.get_function(parent_node) {
                            return func.body.is_none();
                        }
                        return false;
                    }
                    syntax_kind_ext::METHOD_DECLARATION => {
                        if let Some(method) = self.ctx.arena.get_method_decl(parent_node) {
                            return method.body.is_none();
                        }
                        return false;
                    }
                    syntax_kind_ext::CONSTRUCTOR => {
                        if let Some(ctor) = self.ctx.arena.get_constructor(parent_node) {
                            return ctor.body.is_none();
                        }
                        return false;
                    }
                    _ => {
                        current = parent_idx;
                    }
                }
            } else {
                return false;
            }
        }
        false
    }

    // 23. Import and Private Brand Utilities (moved to symbol_resolver.rs)

    // 25. AST Traversal Utilities (11 functions)

    /// Find the enclosing function-like node for a given node.
    ///
    /// Traverses up the AST to find the first parent that is a function-like
    /// construct (function declaration, function expression, arrow function, method, constructor).
    /// Find if there's a constructor implementation after position `start` in members list.
    ///
    /// ## Parameters
    /// - `members`: Slice of member node indices
    /// - `start`: Position to start searching from
    ///
    /// Returns true if a constructor with a body is found, false otherwise.
    pub(crate) fn find_constructor_impl(&self, members: &[NodeIndex], start: usize) -> bool {
        for i in start..members.len() {
            let member_idx = members[i];
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::CONSTRUCTOR {
                if let Some(ctor) = self.ctx.arena.get_constructor(node)
                    && !ctor.body.is_none()
                {
                    return true;
                }
                // Another constructor overload - keep looking
            } else {
                // Non-constructor member - no implementation found
                return false;
            }
        }
        false
    }

    /// Check if there's a method implementation with the given name after position `start`.
    ///
    /// ## Parameters
    /// - `members`: Slice of member node indices
    /// - `start`: Position to start searching from
    /// - `_name`: The method name to search for
    ///
    /// Returns (found: bool, name: Option<String>).
    pub(crate) fn find_method_impl(
        &self,
        members: &[NodeIndex],
        start: usize,
        name: &str,
    ) -> (bool, Option<String>, Option<usize>) {
        for i in start..members.len() {
            let member_idx = members[i];
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::METHOD_DECLARATION {
                if let Some(method) = self.ctx.arena.get_method_decl(node) {
                    let member_name = self.get_method_name_from_node(member_idx);
                    if member_name.as_deref() != Some(name) {
                        if method.body.is_some() {
                            // Different name but has body - wrong-named implementation (TS2389)
                            return (true, member_name, Some(i));
                        }
                        // Different name, no body - no implementation found
                        return (false, None, None);
                    }
                    if !method.body.is_none() {
                        // Found the implementation with matching name
                        return (true, member_name, Some(i));
                    }
                    // Same name but no body - another overload signature, keep looking
                }
            } else {
                // Non-method member encountered - no implementation found
                return (false, None, None);
            }
        }
        (false, None, None)
    }

    /// Find the first return statement with an expression in a function body.
    ///
    /// Used for error reporting position in accessor type checking.
    ///
    /// ## Parameters
    /// - `body_idx`: The function body node index
    ///
    /// Returns Some(NodeIndex) of the return expression if found, None otherwise.
    pub(crate) fn find_return_statement_pos(&self, body_idx: NodeIndex) -> Option<NodeIndex> {
        if body_idx.is_none() {
            return None;
        }

        let body_node = self.ctx.arena.get(body_idx)?;

        if body_node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = self.ctx.arena.get_block(body_node)
        {
            for &stmt_idx in &block.statements.nodes {
                if let Some(stmt_node) = self.ctx.arena.get(stmt_idx)
                    && stmt_node.kind == syntax_kind_ext::RETURN_STATEMENT
                    && let Some(ret) = self.ctx.arena.get_return_statement(stmt_node)
                    && !ret.expression.is_none()
                {
                    return Some(ret.expression);
                }
            }
        }

        None
    }

    /// Find a function implementation with the given name after position `start`.
    ///
    /// Recursively searches through statements to find a matching function implementation.
    /// Handles overload signatures by continuing to search through same-name overloads.
    ///
    /// ## Parameters
    /// - `statements`: Slice of statement node indices
    /// - `start`: Position to start searching from
    /// - `name`: The function name to search for
    ///
    /// Returns (found: bool, name: Option<String>, node: Option<NodeIndex>).
    pub(crate) fn find_function_impl(
        &self,
        statements: &[NodeIndex],
        start: usize,
        name: &str,
    ) -> (bool, Option<String>, Option<NodeIndex>) {
        if start >= statements.len() {
            return (false, None, None);
        }

        let stmt_idx = statements[start];
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return (false, None, None);
        };

        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            && let Some(func) = self.ctx.arena.get_function(node)
        {
            // Check if this is an implementation (has body)
            if !func.body.is_none() {
                // This is an implementation - check if name matches
                let impl_name = self.get_function_name_from_node(stmt_idx);
                return (true, impl_name, Some(stmt_idx));
            } else {
                // Another overload signature without body - need to look further
                // but we should check if this is the same function name
                let overload_name = self.get_function_name_from_node(stmt_idx);
                if overload_name.as_ref() == Some(&name.to_string()) {
                    // Same function, continue looking for implementation
                    return self.find_function_impl(statements, start + 1, name);
                }
            }
        }

        (false, None, None)
    }
}
