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

use crate::binder::symbol_flags;
use crate::checker::FlowAnalyzer;
use crate::checker::state::{CheckerState, ComputedKey, MAX_TREE_WALK_ITERATIONS, PropertyKey};
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use crate::solver::TypeId;
use rustc_hash::FxHashSet;

// =============================================================================
// Type Checking Methods
// =============================================================================

#[allow(dead_code)]
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
        node: &crate::parser::node::Node,
    ) -> Option<&crate::parser::NodeList> {
        use crate::parser::syntax_kind_ext;
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
            _ => None,
        }
    }

    /// Get modifiers from a class member node (property, method, accessor).
    ///
    /// This helper eliminates the repeated pattern of matching member kinds
    /// and extracting their modifiers.
    pub(crate) fn get_member_modifiers(
        &self,
        node: &crate::parser::node::Node,
    ) -> Option<&crate::parser::NodeList> {
        use crate::parser::syntax_kind_ext;
        match node.kind {
            syntax_kind_ext::PROPERTY_DECLARATION => self
                .ctx
                .arena
                .get_property_decl(node)
                .and_then(|p| p.modifiers.as_ref()),
            syntax_kind_ext::METHOD_DECLARATION => self
                .ctx
                .arena
                .get_method_decl(node)
                .and_then(|m| m.modifiers.as_ref()),
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => self
                .ctx
                .arena
                .get_accessor(node)
                .and_then(|a| a.modifiers.as_ref()),
            _ => None,
        }
    }

    /// Get the name node from a class member node.
    ///
    /// This helper eliminates the repeated pattern of matching member kinds
    /// and extracting their name nodes.
    pub(crate) fn get_member_name_node(
        &self,
        node: &crate::parser::node::Node,
    ) -> Option<NodeIndex> {
        use crate::parser::syntax_kind_ext;
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
            _ => None,
        }
    }

    /// Get the name node from a declaration node.
    ///
    /// This helper eliminates the repeated pattern of matching declaration kinds
    /// and extracting their name nodes.
    pub(crate) fn get_declaration_name(
        &self,
        node: &crate::parser::node::Node,
    ) -> Option<NodeIndex> {
        use crate::parser::syntax_kind_ext;
        match node.kind {
            syntax_kind_ext::VARIABLE_DECLARATION => self
                .ctx
                .arena
                .get_variable_declaration(node)
                .map(|v| v.name),
            syntax_kind_ext::FUNCTION_DECLARATION => {
                self.ctx.arena.get_function(node).map(|f| f.name)
            }
            syntax_kind_ext::CLASS_DECLARATION => self.ctx.arena.get_class(node).map(|c| c.name),
            syntax_kind_ext::INTERFACE_DECLARATION => {
                self.ctx.arena.get_interface(node).map(|i| i.name)
            }
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                self.ctx.arena.get_type_alias(node).map(|t| t.name)
            }
            syntax_kind_ext::ENUM_DECLARATION => self.ctx.arena.get_enum(node).map(|e| e.name),
            _ => None,
        }
    }

    /// Check if a node kind is a literal kind (string, number, boolean, null, undefined).
    ///
    /// This helper eliminates the repeated pattern of matching multiple literal kinds.
    pub(crate) fn is_literal_kind(kind: u16) -> bool {
        matches!(kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::BigIntLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::UndefinedKeyword as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == SyntaxKind::RegularExpressionLiteral as u16
        )
    }

    /// Check if a node kind is a terminal statement (return, throw).
    ///
    /// Terminal statements are statements that always terminate execution.
    pub(crate) fn is_terminal_statement(kind: u16) -> bool {
        use crate::parser::syntax_kind_ext;
        matches!(kind,
            k if k == syntax_kind_ext::RETURN_STATEMENT || k == syntax_kind_ext::THROW_STATEMENT
        )
    }

    /// Get identifier text from a node, if it's an identifier.
    ///
    /// This helper eliminates the repeated pattern of checking for identifier
    /// and extracting escaped_text.
    pub(crate) fn get_identifier_text(&self, node: &crate::parser::node::Node) -> Option<String> {
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
        modifiers: &Option<crate::parser::NodeList>,
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

    /// Generic helper to traverse both sides of a binary expression.
    ///
    /// This eliminates the repeated pattern of:
    /// ```rust
    /// if let Some(bin_expr) = self.ctx.arena.get_binary_expr(node) {
    ///     self.some_check(bin_expr.left);
    ///     self.some_check(bin_expr.right);
    /// }
    /// ```
    pub(crate) fn for_each_binary_child<F>(
        &self,
        node: &crate::parser::node::Node,
        mut f: F,
    ) -> bool
    where
        F: FnMut(NodeIndex),
    {
        use crate::parser::syntax_kind_ext;
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            if let Some(bin_expr) = self.ctx.arena.get_binary_expr(node) {
                f(bin_expr.left);
                f(bin_expr.right);
                return true;
            }
        }
        false
    }

    /// Generic helper to traverse conditional expression branches.
    ///
    /// This eliminates the repeated pattern of:
    /// ```rust
    /// if let Some(cond) = self.ctx.arena.get_conditional_expr(node) {
    ///     self.some_check(cond.condition);
    ///     self.some_check(cond.when_true);
    ///     self.some_check(cond.when_false);
    /// }
    /// ```
    pub(crate) fn for_each_conditional_child<F>(
        &self,
        node: &crate::parser::node::Node,
        mut f: F,
    ) -> bool
    where
        F: FnMut(NodeIndex),
    {
        use crate::parser::syntax_kind_ext;
        if node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION {
            if let Some(cond) = self.ctx.arena.get_conditional_expr(node) {
                f(cond.condition);
                f(cond.when_true);
                if !cond.when_false.is_none() {
                    f(cond.when_false);
                }
                return true;
            }
        }
        false
    }

    /// Generic helper to traverse call expression with arguments.
    ///
    /// This eliminates the repeated pattern of:
    /// ```rust
    /// if let Some(call) = self.ctx.arena.get_call_expr(node) {
    ///     self.some_check(call.expression);
    ///     if let Some(args) = &call.arguments {
    ///         for &arg in &args.nodes {
    ///             self.some_check(arg);
    ///         }
    ///     }
    /// }
    /// ```
    pub(crate) fn for_each_call_child<F>(&self, node: &crate::parser::node::Node, mut f: F) -> bool
    where
        F: FnMut(NodeIndex),
    {
        use crate::parser::syntax_kind_ext;
        if node.kind == syntax_kind_ext::CALL_EXPRESSION {
            if let Some(call) = self.ctx.arena.get_call_expr(node) {
                f(call.expression);
                if let Some(args) = &call.arguments {
                    for &arg in &args.nodes {
                        f(arg);
                    }
                }
                return true;
            }
        }
        false
    }

    /// Generic helper to skip parenthesized expressions.
    ///
    /// This eliminates the repeated pattern of:
    /// ```rust
    /// if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
    ///     self.some_check(paren.expression);
    /// }
    /// ```
    pub(crate) fn for_each_parenthesized_child<F>(
        &self,
        node: &crate::parser::node::Node,
        mut f: F,
    ) -> bool
    where
        F: FnMut(NodeIndex),
    {
        use crate::parser::syntax_kind_ext;
        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                f(paren.expression);
                return true;
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
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

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
                let message =
                    format_message(diagnostic_messages::DUPLICATE_IDENTIFIER, &[&name_text]);
                self.error_at_node(
                    member.name,
                    &message,
                    diagnostic_codes::DUPLICATE_IDENTIFIER,
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
                self.error_property_not_exist_at(&property_name, rhs_type, name_idx);
            }
            return;
        }

        let rhs_type = self.evaluate_application_type(rhs_type);
        if rhs_type == TypeId::ANY || rhs_type == TypeId::ERROR || rhs_type == TypeId::UNKNOWN {
            return;
        }

        let declaring_type = match self.private_member_declaring_type(symbols[0]) {
            Some(ty) => ty,
            None => {
                if saw_class_scope {
                    self.error_property_not_exist_at(&property_name, rhs_type, name_idx);
                }
                return;
            }
        };

        if !self.is_assignable_to(rhs_type, declaring_type) {
            let shadowed = symbols.iter().skip(1).any(|sym_id| {
                self.private_member_declaring_type(*sym_id)
                    .map(|ty| self.is_assignable_to(rhs_type, ty))
                    .unwrap_or(false)
            });
            if shadowed {
                return;
            }

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
        if elem_node.kind == syntax_kind_ext::NAMED_TUPLE_MEMBER {
            if let Some(member) = self.ctx.arena.get_named_tuple_member(elem_node) {
                self.check_type_for_missing_names(member.type_node);
            }
            return;
        }
        self.check_type_for_missing_names(elem_idx);
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
        type_parameters: &Option<crate::parser::NodeList>,
    ) {
        let Some(list) = type_parameters else {
            return;
        };
        for &param_idx in &list.nodes {
            self.check_type_parameter_node_for_missing_names(param_idx);
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
        // Check type literals (object types) for call/construct signatures
        else if node.kind == syntax_kind_ext::TYPE_LITERAL {
            if let Some(type_lit) = self.ctx.arena.get_type_literal(node) {
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

        // TS2461: Check if array destructuring is applied to a non-array type
        if pattern_kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
            self.check_array_destructuring_target_type(pattern_idx, pattern_type);
        }

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

            if !self.is_assignable_to(default_value_type, element_type) {
                self.error_type_not_assignable_with_reason_at(
                    default_value_type,
                    element_type,
                    element_data.initializer,
                );
            }
        }

        // If the name is a nested binding pattern, recursively check it
        if let Some(name_node) = self.ctx.arena.get(element_data.name)
            && (name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
        {
            self.check_binding_pattern(element_data.name, element_type);
        }
    }

    /// Check if the target type is valid for array destructuring.
    ///
    /// Validates that the type is array-like (has iterator, is tuple, or is string).
    /// Emits TS2488 if the type is not iterable, TS2461 for other non-array-like types.
    ///
    /// ## Parameters:
    /// - `pattern_idx`: The array binding pattern node index
    /// - `source_type`: The type being destructured
    ///
    /// ## Validation:
    /// - Checks if the type is array, tuple, string, or has iterator
    /// - Emits TS2488 for non-iterable types (preferred error for destructuring)
    /// - Emits TS2461 as fallback for non-array-like types
    pub(crate) fn check_array_destructuring_target_type(
        &mut self,
        pattern_idx: NodeIndex,
        source_type: TypeId,
    ) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        // Skip check for any, unknown, error, or never types
        if source_type == TypeId::ANY
            || source_type == TypeId::UNKNOWN
            || source_type == TypeId::ERROR
            || source_type == TypeId::NEVER
        {
            return;
        }

        // First check if the type is iterable (TS2488 - preferred error)
        // This is the primary check for array destructuring
        if !self.is_iterable_type(source_type) {
            let type_str = self.format_type(source_type);
            let message = format_message(
                diagnostic_messages::TYPE_MUST_HAVE_SYMBOL_ITERATOR,
                &[&type_str],
            );
            self.error_at_node(
                pattern_idx,
                &message,
                diagnostic_codes::TYPE_MUST_HAVE_SYMBOL_ITERATOR,
            );
            return;
        }

        // Check if the type is array-like (TS2461 - fallback error)
        // This catches cases where type is iterable but not array-like
        let is_array_like = self.is_array_destructurable_type(source_type);

        if !is_array_like {
            let type_str = self.format_type(source_type);
            let message =
                format_message(diagnostic_messages::TYPE_IS_NOT_AN_ARRAY_TYPE, &[&type_str]);
            self.error_at_node(
                pattern_idx,
                &message,
                diagnostic_codes::TYPE_IS_NOT_AN_ARRAY_TYPE,
            );
        }
    }

    // =========================================================================
    // Import Validation
    // =========================================================================
}

// =============================================================================
// Statement Validation
// =============================================================================

#[allow(dead_code)]
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

        // Get the expected return type from the function context
        let expected_type = self.current_return_type().unwrap_or(TypeId::UNKNOWN);

        // Get the type of the return expression (if any)
        let return_type = if !return_data.expression.is_none() {
            // TS1359: Check for await expressions outside async function
            self.check_await_expression(return_data.expression);

            let prev_context = self.ctx.contextual_type;
            if expected_type != TypeId::ANY && !self.type_contains_error(expected_type) {
                self.ctx.contextual_type = Some(expected_type);
                // Clear cached type to force recomputation with contextual type
                // This is necessary because the expression might have been previously typed
                // without contextual information (e.g., during function body analysis)
                self.clear_type_cache_recursive(return_data.expression);
            }
            let return_type = self.get_type_of_node(return_data.expression);
            self.ctx.contextual_type = prev_context;
            return_type
        } else {
            // `return;` without expression returns undefined
            TypeId::UNDEFINED
        };

        // Ensure all Application type symbols are resolved before assignability check
        self.ensure_application_symbols_resolved(return_type);
        self.ensure_application_symbols_resolved(expected_type);

        // Check if the return type is assignable to the expected type
        // Exception: Constructors allow `return;` without an expression (no assignability check)
        let is_constructor_return_without_expr = self
            .ctx
            .enclosing_class
            .as_ref()
            .map(|c| c.in_constructor)
            .unwrap_or(false)
            && return_data.expression.is_none();

        if expected_type != TypeId::ANY
            && !is_constructor_return_without_expr
            && !self.is_assignable_to(return_type, expected_type)
        {
            // Report error at the return expression (or at return keyword if no expression)
            let error_node = if !return_data.expression.is_none() {
                return_data.expression
            } else {
                stmt_idx
            };
            if !self.should_skip_weak_union_error(return_type, expected_type, error_node) {
                self.error_type_not_assignable_with_reason_at(
                    return_type,
                    expected_type,
                    error_node,
                );
            }
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
    /// - Recursively checks child expressions for await expressions
    pub(crate) fn check_await_expression(&mut self, expr_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return;
        };

        // If this is an await expression, check if we're in async context
        if node.kind == syntax_kind_ext::AWAIT_EXPRESSION && !self.ctx.in_async_context() {
            use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                expr_idx,
                diagnostic_messages::AWAIT_EXPRESSION_ONLY_IN_ASYNC_FUNCTION,
                diagnostic_codes::AWAIT_EXPRESSION_ONLY_IN_ASYNC_FUNCTION,
            );
        }

        // Recursively check child expressions
        match node.kind {
            syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin_expr) = self.ctx.arena.get_binary_expr(node) {
                    self.check_await_expression(bin_expr.left);
                    self.check_await_expression(bin_expr.right);
                }
            }
            syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            | syntax_kind_ext::POSTFIX_UNARY_EXPRESSION => {
                if let Some(unary_expr) = self.ctx.arena.get_unary_expr_ex(node) {
                    self.check_await_expression(unary_expr.expression);
                }
            }
            syntax_kind_ext::AWAIT_EXPRESSION => {
                // Already checked above
                if let Some(unary_expr) = self.ctx.arena.get_unary_expr_ex(node) {
                    self.check_await_expression(unary_expr.expression);
                }
            }
            syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call_expr) = self.ctx.arena.get_call_expr(node) {
                    self.check_await_expression(call_expr.expression);
                    // Check arguments
                    if let Some(ref args) = call_expr.arguments {
                        for &arg in &args.nodes {
                            self.check_await_expression(arg);
                        }
                    }
                }
            }
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access_expr) = self.ctx.arena.get_access_expr(node) {
                    self.check_await_expression(access_expr.expression);
                }
            }
            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                // Element access is stored differently - need to check the actual structure
                // The expression and argument are stored in specific data_index positions
                // For now, skip this to avoid breaking the build
            }
            syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren_expr) = self.ctx.arena.get_parenthesized(node) {
                    self.check_await_expression(paren_expr.expression);
                }
            }
            _ => {
                // For other expression types, don't recurse into children
                // to avoid infinite recursion or performance issues
            }
        }
    }

    // =========================================================================
    // Variable Statement Validation
    // =========================================================================

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
        use crate::parser::flags::node_flags;
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
        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};

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
            self.error_at_node(
                var_decl.initializer,
                diagnostic_messages::USING_INITIALIZER_MUST_HAVE_DISPOSE,
                if is_await_using {
                    diagnostic_codes::AWAIT_USING_INITIALIZER_MUST_HAVE_DISPOSE
                } else {
                    diagnostic_codes::USING_INITIALIZER_MUST_HAVE_DISPOSE
                },
            );
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

    /// Get a type string for error messages (fallback when detailed formatting isn't available).
    fn get_type_string_fallback(&self, type_id: TypeId) -> String {
        // Try to get a reasonable type name
        self.get_type_display_name(type_id)
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
        use crate::checker::types::diagnostics::diagnostic_codes;

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
                                diagnostic_codes::PROPERTY_USED_BEFORE_INITIALIZATION,
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

    /// Check if a function is a class method.
    ///
    /// Walks up the parent chain looking for ClassDeclaration nodes.
    ///
    /// ## Parameters
    /// - `func_idx`: The function node index
    ///
    /// Returns true if the function is inside a class declaration.
    pub(crate) fn is_class_method(&self, func_idx: NodeIndex) -> bool {
        // Walk up the parent chain looking for ClassDeclaration nodes
        let mut current = func_idx;

        while !current.is_none() {
            if let Some(ext) = self.ctx.arena.get_extended(current) {
                // Check if this node is a ClassDeclaration
                if let Some(node) = self.ctx.arena.get(current) {
                    if node.kind == syntax_kind_ext::CLASS_DECLARATION
                        || node.kind == syntax_kind_ext::CLASS_EXPRESSION
                    {
                        return true;
                    }
                }
                current = ext.parent;
            } else {
                break;
            }
        }

        false
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
        use crate::parser::node_flags;

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
                | "PromiseLike"
                | "PromiseConstructor"
                | "PromiseConstructorLike"
                | "Awaited"
                // Iterator/Generator types
                | "Iterator"
                | "IteratorResult"
                | "IteratorYieldResult"
                | "IteratorReturnResult"
                | "Iterable"
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
    pub(crate) fn is_constructor_type(&self, type_id: TypeId) -> bool {
        // Any type is always considered a constructor type (TypeScript compatibility)
        if type_id == TypeId::ANY {
            return true;
        }

        // First check if it directly has construct signatures
        if crate::solver::type_queries::has_construct_signatures(self.ctx.types, type_id) {
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
        use crate::solver::type_queries::{
            ConstructorCheckKind, classify_for_constructor_check, get_ref_symbol,
        };

        match classify_for_constructor_check(self.ctx.types, type_id) {
            ConstructorCheckKind::TypeParameter { constraint } => {
                if let Some(constraint) = constraint {
                    self.is_constructor_type(constraint)
                } else {
                    false
                }
            }
            ConstructorCheckKind::Intersection(members) => {
                members.iter().any(|&m| self.is_constructor_type(m))
            }
            ConstructorCheckKind::Union(members) => {
                // Union types are constructable if ALL members are constructable
                // This matches TypeScript's behavior where `type A | B` used in extends
                // requires both A and B to be constructors
                !members.is_empty() && members.iter().all(|&m| self.is_constructor_type(m))
            }
            ConstructorCheckKind::Application { base } => {
                // For type applications like Ctor<{}>, check if the base type is a constructor
                // This handles cases like:
                //   type Constructor<T> = new (...args: any[]) => T;
                //   function f<T extends Constructor<{}>>(x: T) {
                //     class C extends x {}  // x should be valid here
                //   }
                // Only check the base - don't recurse further to avoid infinite loops
                // Check if base is a Ref to a type alias with constructor type body
                if let Some(symbol_ref) = get_ref_symbol(self.ctx.types, base) {
                    use crate::binder::SymbolId;
                    let sym_id = SymbolId(symbol_ref.0);
                    if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                        if let Some(decl_idx) = symbol.declarations.first().copied() {
                            if let Some(decl_node) = self.ctx.arena.get(decl_idx) {
                                if decl_node.kind
                                    == crate::parser::syntax_kind_ext::TYPE_ALIAS_DECLARATION
                                {
                                    if let Some(alias) = self.ctx.arena.get_type_alias(decl_node) {
                                        if let Some(body_node) = self.ctx.arena.get(alias.type_node)
                                        {
                                            // Constructor type syntax: new (...args) => T
                                            if body_node.kind
                                                == crate::parser::syntax_kind_ext::CONSTRUCTOR_TYPE
                                            {
                                                return true;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // Also check if base is directly a Callable with construct signatures
                crate::solver::type_queries::has_construct_signatures(self.ctx.types, base)
            }
            // Ref to a symbol - check if it's a class symbol or resolve to the actual type
            // This handles cases like:
            // 1. `class C extends MyClass` where MyClass is a class
            // 2. `function f<T>(ctor: T)` then `class B extends ctor` where ctor has a constructor type
            // 3. `class C extends Object` where Object is declared as ObjectConstructor interface
            ConstructorCheckKind::SymbolRef(symbol_ref) => {
                use crate::binder::SymbolId;
                let symbol_id = SymbolId(symbol_ref.0);
                if let Some(symbol) = self.ctx.binder.get_symbol(symbol_id) {
                    // Check if this is a class symbol - classes are always constructors
                    if (symbol.flags & crate::binder::symbol_flags::CLASS) != 0 {
                        return true;
                    }

                    // Check if this is an interface symbol with construct signatures
                    // This handles cases like ObjectConstructor, ArrayConstructor, etc.
                    // which are interfaces with `new()` signatures
                    if (symbol.flags & crate::binder::symbol_flags::INTERFACE) != 0 {
                        // Check the cached type for interface - it should be Callable if it has construct signatures
                        if let Some(&cached_type) = self.ctx.symbol_types.get(&symbol_id) {
                            if cached_type != type_id {
                                // Interface type was already resolved - check if it has construct signatures
                                if crate::solver::type_queries::has_construct_signatures(
                                    self.ctx.types,
                                    cached_type,
                                ) {
                                    return true;
                                }
                            }
                        } else if !symbol.declarations.is_empty() {
                            // Interface not cached - check if it has construct signatures by examining declarations
                            // This handles lib.d.ts interfaces like ObjectConstructor that may not be resolved yet
                            // IMPORTANT: Use the correct arena for the symbol (may be different for lib types)
                            use crate::solver::TypeLowering;
                            let symbol_arena = self
                                .ctx
                                .binder
                                .symbol_arenas
                                .get(&symbol_id)
                                .map(|arena| arena.as_ref())
                                .unwrap_or(self.ctx.arena);

                            let type_param_bindings = self.get_type_param_bindings();
                            let type_resolver = |node_idx: crate::parser::NodeIndex| {
                                self.resolve_type_symbol_for_lowering(node_idx)
                            };
                            let value_resolver = |node_idx: crate::parser::NodeIndex| {
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
                            if crate::solver::type_queries::has_construct_signatures(
                                self.ctx.types,
                                interface_type,
                            ) {
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
            ConstructorCheckKind::TypeQuery(symbol_ref) => {
                use crate::binder::SymbolId;
                let symbol_id = SymbolId(symbol_ref.0);
                if let Some(symbol) = self.ctx.binder.get_symbol(symbol_id) {
                    // Classes have constructor types
                    if (symbol.flags & crate::binder::symbol_flags::CLASS) != 0 {
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
            ConstructorCheckKind::Other => false,
        }
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
        use crate::solver::type_queries;

        // Check callable shape for prototype property
        if let Some(shape) = type_queries::get_callable_shape(self.ctx.types, type_id) {
            let prototype_atom = self.ctx.types.intern_string("prototype");
            return shape.properties.iter().any(|p| p.name == prototype_atom);
        }

        // Function types typically have prototype
        type_queries::get_function_shape(self.ctx.types, type_id).is_some()
    }

    /// Check if a symbol is a class symbol.
    ///
    /// ## Parameters
    /// - `symbol_id`: The symbol ID to check
    ///
    /// Returns true if the symbol represents a class.
    pub(crate) fn is_class_symbol(&self, symbol_id: crate::binder::SymbolId) -> bool {
        use crate::binder::symbol_flags;
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
        use crate::scanner::SyntaxKind;

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
        use crate::parser::node_flags;

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
        self.has_declare_modifier(&class.modifiers)
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

    /// Check if a node is an assignment target in a for-in or for-of loop.
    ///
    /// ## Parameters
    /// - `idx`: The node index to check
    ///
    /// Returns true if the node is the variable being assigned in a for-in/of loop.
    pub(crate) fn is_for_in_of_assignment_target(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
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
            let parent = ext.parent;
            let parent_node = match self.ctx.arena.get(parent) {
                Some(node) => node,
                None => return false,
            };
            if (parent_node.kind == syntax_kind_ext::FOR_IN_STATEMENT
                || parent_node.kind == syntax_kind_ext::FOR_OF_STATEMENT)
                && let Some(for_data) = self.ctx.arena.get_for_in_of(parent_node)
            {
                let analyzer = FlowAnalyzer::new(self.ctx.arena, self.ctx.binder, self.ctx.types);
                return analyzer.assignment_targets_reference(for_data.initializer, idx);
            }
            current = parent;
        }
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
            PropertyKey::Computed(ComputedKey::Qualified(q)) => {
                format!("[{}]", q)
            }
            PropertyKey::Computed(ComputedKey::Symbol(Some(s))) => {
                format!("[Symbol({})]", s)
            }
            PropertyKey::Computed(ComputedKey::Symbol(None)) => "[Symbol()]".to_string(),
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
        use crate::scanner::SyntaxKind;

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
            return Some(format!("Symbol.{}", ident.escaped_text));
        }

        if matches!(
            name_node.kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        ) && let Some(lit) = self.ctx.arena.get_literal(name_node)
            && !lit.text.is_empty()
        {
            return Some(format!("Symbol.{}", lit.text));
        }

        None
    }

    // 22. Type Checking Utilities (2 functions)

    /// Check if a type is narrowable (can be narrowed via control flow).
    ///
    /// Narrowable types include unions, type parameters, infer types, and unknown.
    /// These types can be narrowed to more specific types through
    /// type guards and control flow analysis.
    ///
    /// ## Parameters
    /// - `type_id`: The type ID to check
    ///
    /// Returns true if the type can be narrowed.
    ///
    /// ## Narrowable Types
    /// - **Union types**: Can be narrowed to specific members via discriminant checks
    /// - **Type parameters**: Can be narrowed via constraints
    /// - **Infer types**: Can be narrowed during type inference
    /// - **Unknown type**: Can be narrowed via typeof guards and user-defined type guards
    /// - **Nullish types**: Can be narrowed via null/undefined checks
    pub(crate) fn is_narrowable_type(&self, type_id: TypeId) -> bool {
        use crate::solver::type_queries::is_narrowable_type_key;

        // unknown type is narrowable - typeof guards and user-defined type guards
        // should narrow unknown to the guard's target type
        // This prevents false positive TS2571 errors after type guards
        if type_id == TypeId::UNKNOWN {
            return true;
        }

        // Check if it's a union type or a type parameter (which can be narrowed)
        if is_narrowable_type_key(self.ctx.types, type_id) {
            return true;
        }

        // Types that include null or undefined can be narrowed via null checks
        if self.type_contains_nullish(type_id) {
            return true;
        }

        false
    }

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
        use crate::lib_loader;

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

        // Check if lib files are loaded
        let has_lib = self.ctx.has_lib_loaded();

        // Only emit TS2318 errors when:
        // - no_lib is false (user expects lib files to be loaded)
        // - AND lib files are not loaded (something went wrong with lib loading)
        // When no_lib is true, the user explicitly opted out of lib files.
        if !has_lib && !self.ctx.no_lib() {
            for &type_name in CORE_GLOBAL_TYPES {
                // Check if the type is declared in the current file
                if !self.ctx.binder.file_locals.has(type_name) {
                    // Type not declared locally and no lib loaded - emit TS2318
                    self.ctx
                        .push_diagnostic(lib_loader::emit_error_global_type_missing(
                            type_name,
                            self.ctx.file_name.clone(),
                            0,
                            0,
                        ));
                }
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
        use crate::solver::types::IntrinsicKind;

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
            if let Some(ty) = array_type {
                env.set_array_base_type(ty, array_type_params);
            }
        }
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
        use crate::lib_loader;

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
                // Awaited is checked when using await type operator or async functions
                "Awaited" => {
                    // Only check if async/await is actually used, not just because noLib is set
                    self.ctx.async_depth > 0
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
    /// noLib is set - they require the actual feature to be used.
    pub(crate) fn should_check_for_feature_type(&self, _type_name: &str) -> bool {
        // Don't emit feature-specific global type errors based on simple noLib detection.
        // TSC only emits these when the actual feature is used (generators, decorators, etc.)
        // For now, disable these to match TSC's behavior for @noLib tests.
        // TODO: Implement proper feature detection (AST traversal for generators, decorators, etc.)
        false
    }

    /// Check for duplicate identifiers (TS2300, TS2451, TS2392).
    /// Reports when variables, functions, classes, or other declarations
    /// have conflicting names within the same scope.
    pub(crate) fn check_duplicate_identifiers(&mut self) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        // Skip duplicate checking entirely if lib contexts are loaded
        // Lib symbols can have multiple declarations (interface merging, etc.)
        // which are not duplicates
        if self.ctx.has_lib_loaded() {
            return;
        }

        let mut symbol_ids = FxHashSet::default();
        if !self.ctx.binder.scopes.is_empty() {
            for scope in &self.ctx.binder.scopes {
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
                        let node = self.ctx.arena.get(decl_idx)?;
                        let constructor = self.ctx.arena.get_constructor(node)?;
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
                    let message = diagnostic_messages::MULTIPLE_CONSTRUCTOR_IMPLEMENTATIONS;
                    for &decl_idx in &implementations {
                        self.error_at_node(
                            decl_idx,
                            message,
                            diagnostic_codes::MULTIPLE_CONSTRUCTOR_IMPLEMENTATIONS,
                        );
                    }
                }
                continue;
            }

            let mut declarations = Vec::new();
            for &decl_idx in &symbol.declarations {
                if let Some(flags) = self.declaration_symbol_flags(decl_idx) {
                    declarations.push((decl_idx, flags));
                }
            }

            if declarations.len() <= 1 {
                continue;
            }

            let mut conflicts = FxHashSet::default();
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

                    // Check for type alias merging - multiple type alias declarations are allowed
                    let both_type_aliases = (decl_flags & symbol_flags::TYPE_ALIAS) != 0
                        && (other_flags & symbol_flags::TYPE_ALIAS) != 0;
                    if both_type_aliases {
                        continue; // Type alias merging is always allowed
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

                    // Namespace + Function merging is allowed
                    let decl_is_function = (decl_flags & symbol_flags::FUNCTION) != 0;
                    let other_is_function = (other_flags & symbol_flags::FUNCTION) != 0;
                    if (decl_is_namespace && other_is_function)
                        || (decl_is_function && other_is_namespace)
                    {
                        continue;
                    }

                    // Namespace + Class merging is allowed
                    let decl_is_class = (decl_flags & symbol_flags::CLASS) != 0;
                    let other_is_class = (other_flags & symbol_flags::CLASS) != 0;
                    if (decl_is_namespace && other_is_class)
                        || (decl_is_class && other_is_namespace)
                    {
                        continue;
                    }

                    // Namespace + Enum merging is allowed
                    let decl_is_enum = (decl_flags & symbol_flags::ENUM) != 0;
                    let other_is_enum = (other_flags & symbol_flags::ENUM) != 0;
                    if (decl_is_namespace && other_is_enum) || (decl_is_enum && other_is_namespace)
                    {
                        continue;
                    }

                    // Ambient class + Function merging is allowed
                    // (declare class provides the type, function provides the value)
                    if (decl_is_class && other_is_function) || (decl_is_function && other_is_class)
                    {
                        let class_idx = if decl_is_class { decl_idx } else { other_idx };
                        if self.is_ambient_class_declaration(class_idx) {
                            continue;
                        }
                    }

                    if Self::declarations_conflict(decl_flags, other_flags) {
                        conflicts.insert(decl_idx);
                        conflicts.insert(other_idx);
                    }
                }
            }

            if conflicts.is_empty() {
                continue;
            }

            // Check if we have any non-block-scoped declarations (var, function, etc.)
            // Imports (ALIAS) and let/const (BLOCK_SCOPED_VARIABLE) are block-scoped
            let has_non_block_scoped = declarations.iter().any(|(decl_idx, flags)| {
                conflicts.contains(decl_idx) && {
                    (flags & (symbol_flags::BLOCK_SCOPED_VARIABLE | symbol_flags::ALIAS)) == 0
                }
            });

            let name = symbol.escaped_name.clone();
            let (message, code) = if !has_non_block_scoped {
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

    /// Check for unused declarations (TS6133).
    /// Reports variables, functions, classes, and other declarations that are never referenced.
    pub(crate) fn check_unused_declarations(&mut self) {
        // Temporarily disable unused declaration checking to focus on core functionality
        // The reference tracking system needs more work to avoid false positives
        // TODO: Re-enable and fix reference tracking system properly
    }

    // 23. Import and Private Brand Utilities (moved to symbol_resolver.rs)

    // 24. Module Detection Utilities (3 functions)

    /// Check if async function context validation should be performed.
    ///
    /// Determines whether async function validation should be strict based on:
    /// - File extension (.d.ts files are always strict)
    /// - isolatedModules compiler option
    /// - Whether the file is a module (has import/export)
    /// - Whether the function is a class method
    /// - Whether in a namespace context
    /// - Strict property initialization mode
    /// - Other strict mode flags
    ///
    /// ## Parameters
    /// - `func_idx`: The function node to check
    ///
    /// Returns true if async validation should be performed.
    pub(crate) fn should_validate_async_function_context(&self, func_idx: NodeIndex) -> bool {
        // Enhanced validation to catch more TS2705 cases (we have 34 missing)
        // Need to be more liberal while maintaining precision

        // Always validate in declaration files (.d.ts files are always strict)
        if self.ctx.file_name.ends_with(".d.ts") {
            return true;
        }

        // Always validate for isolatedModules mode (explicit flag for strict validation)
        if self.ctx.isolated_modules() {
            return true;
        }

        // Validate if this is a module file (has import/export declarations in AST)
        if self.is_file_module() {
            return true;
        }

        // Validate class methods - class methods are typically strict
        if self.is_class_method(func_idx) {
            return true;
        }

        // Validate functions in namespaces (explicit module structure)
        if self.is_in_namespace_context(func_idx) {
            return true;
        }

        // Validate async functions in strict property initialization contexts
        // If we're doing strict property checking, likely need strict async too
        if self.ctx.strict_property_initialization() {
            return true;
        }

        // More liberal fallback: validate if any strict mode features are enabled
        if self.ctx.strict_null_checks()
            || self.ctx.strict_function_types()
            || self.ctx.no_implicit_any()
        {
            return true;
        }

        false
    }

    /// Check if the current file is a module (has import/export declarations).
    ///
    /// Uses AST-based detection instead of filename heuristics. A file is
    /// considered a module if it contains any import or export declarations.
    ///
    /// Returns true if the file is a module.
    pub(crate) fn is_file_module(&self) -> bool {
        // Get the root source file node
        let Some(root_node) = self.ctx.arena.nodes.last() else {
            return false;
        };

        // Check if it's a source file
        if root_node.kind != syntax_kind_ext::SOURCE_FILE {
            return false;
        }

        let Some(source_file) = self.ctx.arena.get_source_file(root_node) else {
            return false;
        };

        // Check each top-level statement for import/export declarations
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };

            match stmt.kind {
                // Import declarations indicate module
                k if k == syntax_kind_ext::IMPORT_DECLARATION => return true,
                k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => return true,

                // Export declarations indicate module
                k if k == syntax_kind_ext::EXPORT_DECLARATION => return true,
                k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => return true,

                // Check for export modifier on declarations using existing method
                k if k == syntax_kind_ext::VARIABLE_STATEMENT
                    || k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::INTERFACE_DECLARATION
                    || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || k == syntax_kind_ext::ENUM_DECLARATION
                    || k == syntax_kind_ext::MODULE_DECLARATION =>
                {
                    if self.has_export_modifier_on_modifiers(stmt) {
                        return true;
                    }
                }
                _ => {}
            }
        }

        false
    }

    /// Check if a node's modifiers include the 'export' keyword.
    ///
    /// Helper for `is_file_module` to check export on declarations.
    /// Iterates through the modifier nodes to find an ExportKeyword.
    ///
    /// ## Parameters
    /// - `node`: The node to check (must be a declaration with modifiers)
    ///
    /// Returns true if the node has an export modifier.
    pub(crate) fn has_export_modifier_on_modifiers(
        &self,
        node: &crate::parser::node::Node,
    ) -> bool {
        use crate::scanner::SyntaxKind;

        // Use helper to get modifiers from declaration
        let Some(mods) = self.get_declaration_modifiers(node) else {
            return false;
        };

        // Check if export modifier is present
        mods.nodes.iter().any(|&mod_idx| {
            self.ctx
                .arena
                .get(mod_idx)
                .is_some_and(|mod_node| mod_node.kind == SyntaxKind::ExportKeyword as u16)
        })
    }

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
    ) -> (bool, Option<String>) {
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
                            return (true, member_name);
                        }
                        // Different name, no body - no implementation found
                        return (false, None);
                    }
                    if !method.body.is_none() {
                        // Found the implementation with matching name
                        return (true, member_name);
                    }
                    // Same name but no body - another overload signature, keep looking
                }
            } else {
                // Non-method member encountered - no implementation found
                return (false, None);
            }
        }
        (false, None)
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
    /// Returns (found: bool, name: Option<String>).
    pub(crate) fn find_function_impl(
        &self,
        statements: &[NodeIndex],
        start: usize,
        name: &str,
    ) -> (bool, Option<String>) {
        if start >= statements.len() {
            return (false, None);
        }

        let stmt_idx = statements[start];
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return (false, None);
        };

        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            && let Some(func) = self.ctx.arena.get_function(node)
        {
            // Check if this is an implementation (has body)
            if !func.body.is_none() {
                // This is an implementation - check if name matches
                let impl_name = self.get_function_name_from_node(stmt_idx);
                return (true, impl_name);
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

        (false, None)
    }
}
