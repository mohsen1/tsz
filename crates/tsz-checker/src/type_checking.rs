//! Type checking validation: utility methods, AST traversal helpers,
//! member/declaration/private identifier/parameter property validation,
//! destructuring, import/return/await/variable/using declaration validation.

use crate::state::CheckerState;
use rustc_hash::FxHashSet;
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
    /// and extracting their modifiers. Used in `has_export_modifier` and similar functions.
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
    /// and extracting `escaped_text`.
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
            .and_then(|node| self.get_identifier_text(node))
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

    /// Check a class member name for computed property validation and
    /// constructor-name restrictions (TS1341, TS1368).
    ///
    /// This dispatches to `check_computed_property_name` for properties,
    /// methods, and accessors that use computed names, and also checks
    /// that "constructor" is not used as an accessor or generator name.
    pub(crate) fn check_class_member_name(&mut self, member_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let kind = node.kind;

        // Use helper to get member name node
        if let Some(name_idx) = self.get_member_name_node(node) {
            self.check_computed_property_name(name_idx);

            // Check constructor-name restrictions for class members
            if let Some(name_text) = self.get_identifier_text_from_idx(name_idx)
                && name_text == "constructor"
            {
                // TS1341: Class constructor may not be an accessor
                if kind == syntax_kind_ext::GET_ACCESSOR || kind == syntax_kind_ext::SET_ACCESSOR {
                    self.error_at_node(
                        name_idx,
                        diagnostic_messages::CLASS_CONSTRUCTOR_MAY_NOT_BE_AN_ACCESSOR,
                        diagnostic_codes::CLASS_CONSTRUCTOR_MAY_NOT_BE_AN_ACCESSOR,
                    );
                }

                // TS1368: Class constructor may not be a generator
                if kind == syntax_kind_ext::METHOD_DECLARATION {
                    let node = self.ctx.arena.get(member_idx);
                    if let Some(method) = node.and_then(|n| self.ctx.arena.get_method_decl(n))
                        && method.asterisk_token
                    {
                        self.error_at_node(
                            name_idx,
                            diagnostic_messages::CLASS_CONSTRUCTOR_MAY_NOT_BE_A_GENERATOR,
                            diagnostic_codes::CLASS_CONSTRUCTOR_MAY_NOT_BE_A_GENERATOR,
                        );
                    }
                }
            }
        }
    }

    /// Check for duplicate enum member names.
    ///
    /// This function validates that all enum members have unique names.
    /// If duplicates are found, it emits TS2308 errors for each duplicate.
    ///
    /// ## Duplicate Detection:
    /// - Collects all member names into a `HashSet`
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

            self.check_computed_property_name(member.name);

            // Get the member name
            let Some(name_node) = self.ctx.arena.get(member.name) else {
                continue;
            };

            // TS1164: Computed property names are not allowed in enums.
            // Emitted here (checker grammar check) rather than in the parser to avoid
            // position-based dedup conflicts with TS1357 (missing comma between members).
            if name_node.kind == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                self.error_at_node(
                    member.name,
                    "Computed property names are not allowed in enums.",
                    diagnostic_codes::COMPUTED_PROPERTY_NAMES_ARE_NOT_ALLOWED_IN_ENUMS,
                );
            }

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
    // ## Parameters:
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
                    .is_some_and(|ty| self.is_assignable_to(evaluated_rhs_type, ty))
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
    // ## Parameters:
    /// - `param_idx`: The parameter node index to check
    pub(crate) fn check_parameter_type_for_missing_names(&mut self, param_idx: NodeIndex) {
        let Some(param_node) = self.ctx.arena.get(param_idx) else {
            return;
        };
        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
            return;
        };
        if param.type_annotation.is_some() {
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
        if param.constraint.is_some() {
            self.check_type_for_missing_names(param.constraint);
        }

        // Check default type
        if param.default.is_some() {
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
                self.check_strict_mode_reserved_parameter_names(
                    &func_type.parameters.nodes,
                    type_idx,
                    self.ctx.enclosing_class.is_some(),
                );
                self.check_parameter_properties(&func_type.parameters.nodes);
                for &param_idx in &func_type.parameters.nodes {
                    if let Some(param_node) = self.ctx.arena.get(param_idx)
                        && let Some(param) = self.ctx.arena.get_parameter(param_node)
                    {
                        if param.type_annotation.is_some() {
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
                    // TS1170: Computed property in type literal must have literal/unique symbol type
                    if let Some(member_node) = self.ctx.arena.get(member_idx)
                        && let Some(sig) = self.ctx.arena.get_signature(member_node)
                    {
                        self.check_type_literal_computed_property_name(sig.name);
                    }
                }
                // TS2411: Check that properties are assignable to index signature types.
                // Type literals don't inherit, so we pass ERROR as the "parent type"
                // and rely on direct member scanning inside the method.
                self.check_index_signature_compatibility(&type_lit.members.nodes, TypeId::ERROR);
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
            && pred.type_node.is_some()
        {
            self.check_type_for_parameter_properties(pred.type_node);
        }
    }

    /// Check for duplicate property names in type literals (TS2300).
    /// e.g. `{ a: string; a: number; }` has duplicate property `a`.
    ///
    /// Method signatures (overloads) with the same name are allowed — only
    /// property signatures are checked for duplicates.
    pub(crate) fn check_type_literal_duplicate_properties(&mut self, members: &[NodeIndex]) {
        use crate::diagnostics::diagnostic_codes;
        use tsz_parser::parser::syntax_kind_ext::PROPERTY_SIGNATURE;

        let mut seen: rustc_hash::FxHashMap<String, NodeIndex> = rustc_hash::FxHashMap::default();

        for &member_idx in members {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            // Only check property signatures for duplicates.
            // Method signatures with the same name are valid overloads.
            if member_node.kind != PROPERTY_SIGNATURE {
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
                    &format!("Duplicate identifier '{name}'."),
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
                        &format!("Duplicate identifier '{name}'."),
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
    pub(crate) fn check_binding_pattern(
        &mut self,
        pattern_idx: NodeIndex,
        pattern_type: TypeId,
        check_default_assignability: bool,
    ) {
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

        let elements_len = pattern_data.elements.nodes.len();
        for (i, &element_idx) in pattern_data.elements.nodes.iter().enumerate() {
            if i < elements_len - 1
                && let Some(element_node) = self.ctx.arena.get(element_idx)
                && let Some(element_data) = self.ctx.arena.get_binding_element(element_node)
                && element_data.dot_dot_dot_token
            {
                use tsz_common::diagnostics::diagnostic_codes;
                self.error_at_node_msg(
                    element_idx,
                    diagnostic_codes::A_REST_ELEMENT_MUST_BE_LAST_IN_A_DESTRUCTURING_PATTERN,
                    &[],
                );
            }

            self.check_binding_element(
                element_idx,
                pattern_kind,
                i,
                pattern_type,
                check_default_assignability,
            );
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
        check_default_assignability: bool,
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
        if element_data.property_name.is_some() {
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
        // TypeScript only checks default value assignability in function parameter
        // destructuring, not in variable declaration destructuring.
        if check_default_assignability
            && element_data.initializer.is_some()
            && element_type != TypeId::ANY
            // For object binding patterns, a default initializer is only reachable when
            // the property can be missing/undefined. Skip assignability checks for required
            // properties to match TypeScript's control-flow behavior.
            && (pattern_kind != syntax_kind_ext::OBJECT_BINDING_PATTERN
                || tsz_solver::type_queries::type_includes_undefined(self.ctx.types, element_type))
        {
            // Set contextual type when the initializer is a function expression or arrow
            // so that parameter types can be inferred from the expected element type.
            // Only do this for function-like initializers to avoid changing how non-function
            // defaults (object literals, primitives) are typed.
            let prev_context = self.ctx.contextual_type;
            if let Some(init_node) = self.ctx.arena.get(element_data.initializer) {
                let k = init_node.kind;
                if k == syntax_kind_ext::ARROW_FUNCTION || k == syntax_kind_ext::FUNCTION_EXPRESSION
                {
                    self.ctx.contextual_type = Some(element_type);
                }
            }
            let default_value_type = self.get_type_of_node(element_data.initializer);
            self.ctx.contextual_type = prev_context;
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
            self.check_binding_pattern(
                element_data.name,
                element_type,
                check_default_assignability,
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
        // In .d.ts files, TS1036 is emitted instead of TS1108.
        // Like TSC's grammarErrorOnFirstToken, suppress grammar errors when parse
        // errors are present — TSC checks hasParseDiagnostics(sourceFile) before
        // emitting TS1108 and other grammar errors.
        if self.current_return_type().is_none() {
            if !self.ctx.is_in_ambient_declaration_file && !self.has_syntax_parse_errors() {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node(
                    stmt_idx,
                    "A 'return' statement can only be used within a function body.",
                    diagnostic_codes::A_RETURN_STATEMENT_CAN_ONLY_BE_USED_WITHIN_A_FUNCTION_BODY,
                );
            }
            return;
        }

        // TS2408: Setters cannot return a value.
        if return_data.expression.is_some() {
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
        let return_type = if return_data.expression.is_some() {
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
        let is_in_constructor = self
            .ctx
            .enclosing_class
            .as_ref()
            .is_some_and(|c| c.in_constructor);

        let error_node = if return_data.expression.is_some() {
            return_data.expression
        } else {
            stmt_idx
        };

        // In constructors, bare `return;` (without expression) is always allowed — TSC
        // doesn't check assignability for void returns in constructors.
        let skip_assignability = is_in_constructor && return_data.expression.is_none();

        if !skip_assignability
            && expected_type != TypeId::ANY
            && !self.type_contains_error(expected_type)
            && !self.check_assignable_or_report(return_type, expected_type, error_node)
        {
            // TS2409: In constructors, also emit the constructor-specific diagnostic
            // alongside the TS2322 already emitted by check_assignable_or_report.
            if is_in_constructor {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    error_node,
                    diagnostic_messages::RETURN_TYPE_OF_CONSTRUCTOR_SIGNATURE_MUST_BE_ASSIGNABLE_TO_THE_INSTANCE_TYPE_OF,
                    diagnostic_codes::RETURN_TYPE_OF_CONSTRUCTOR_SIGNATURE_MUST_BE_ASSIGNABLE_TO_THE_INSTANCE_TYPE_OF,
                );
            }
        }

        if expected_type != TypeId::ANY
            && expected_type != TypeId::UNKNOWN
            && !self.type_contains_error(expected_type)
            && return_data.expression.is_some()
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
    /// - module is ES2022, `ESNext`, System, Node16, `NodeNext`, or Preserve
    /// - target is ES2017 or higher
    const fn supports_top_level_await(&self) -> bool {
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
                        if bin_expr.right.is_some() {
                            stack.push(bin_expr.right);
                        }
                        if bin_expr.left.is_some() {
                            stack.push(bin_expr.left);
                        }
                    }
                }
                syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                | syntax_kind_ext::POSTFIX_UNARY_EXPRESSION => {
                    if let Some(unary_expr) = self.ctx.arena.get_unary_expr_ex(node)
                        && unary_expr.expression.is_some()
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
                        && unary_expr.expression.is_some()
                    {
                        stack.push(unary_expr.expression);
                    }
                }
                syntax_kind_ext::CALL_EXPRESSION => {
                    if let Some(call_expr) = self.ctx.arena.get_call_expr(node) {
                        // Check arguments (push in reverse order for correct traversal)
                        if let Some(ref args) = call_expr.arguments {
                            for &arg in args.nodes.iter().rev() {
                                if arg.is_some() {
                                    stack.push(arg);
                                }
                            }
                        }
                        if call_expr.expression.is_some() {
                            stack.push(call_expr.expression);
                        }
                    }
                }
                syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                    if let Some(access_expr) = self.ctx.arena.get_access_expr(node)
                        && access_expr.expression.is_some()
                    {
                        stack.push(access_expr.expression);
                    }
                }
                syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    if let Some(paren_expr) = self.ctx.arena.get_parenthesized(node)
                        && paren_expr.expression.is_some()
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
                // TS1431: Only emit when the file is NOT a module (no imports/exports).
                // If the file is a module, top-level for-await is potentially valid
                // (just needs the right module/target settings).
                if !self.ctx.binder.is_external_module() {
                    self.error_at_node(
                        stmt_idx,
                        diagnostic_messages::FOR_AWAIT_LOOPS_ARE_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_FILE_WHEN_THAT_FILE_IS_A,
                        diagnostic_codes::FOR_AWAIT_LOOPS_ARE_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_FILE_WHEN_THAT_FILE_IS_A,
                    );
                }

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

        // Check if this is a using/await using declaration list.
        // Only check the USING bit (bit 2) — AWAIT_USING (6) = CONST (2) | USING (4),
        // so checking just the USING bit correctly matches both using and await using
        // but not const.
        use tsz_parser::parser::flags::node_flags;
        let flags_u32 = node.flags as u32;
        let is_using = (flags_u32 & node_flags::USING) != 0;
        let is_await_using = flags_u32 == node_flags::AWAIT_USING;

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
}

impl<'a> CheckerState<'a> {
    /// Check a type alias declaration.
    pub(crate) fn check_type_alias_declaration(&mut self, node_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };
        let Some(alias) = self.ctx.arena.get_type_alias(node) else {
            return;
        };

        // Check type parameters
        if let Some(_params) = &alias.type_parameters {
            // self.check_type_parameters(params);
        }

        // Check the type node
        self.check_type_node(alias.type_node);
    }

    /// Check a type node for validity (recursive).
    pub(crate) fn check_type_node(&mut self, node_idx: NodeIndex) {
        if node_idx == NodeIndex::NONE {
            return;
        }
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                self.check_indexed_access_type(node_idx);
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                    for &child in &composite.types.nodes {
                        self.check_type_node(child);
                    }
                }
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(arr) = self.ctx.arena.get_array_type(node) {
                    self.check_type_node(arr.element_type);
                }
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(lit) = self.ctx.arena.get_type_literal(node) {
                    for &_member in &lit.members.nodes {
                        // self.check_type_element(member);
                    }
                }
            }
            _ => {}
        }
    }

    /// Check an indexed access type (T[K]).
    pub(crate) fn check_indexed_access_type(&mut self, node_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };
        let Some(data) = self.ctx.arena.get_indexed_access_type(node) else {
            return;
        };

        let object_type = self.get_type_from_type_node(data.object_type);
        let index_type = self.get_type_from_type_node(data.index_type);
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_solver::type_queries::{LiteralValueKind, classify_for_literal_value};

        // Resolve index text first to avoid borrow conflicts
        let prop_name = match classify_for_literal_value(self.ctx.types, index_type) {
            LiteralValueKind::String(atom) => Some(self.ctx.types.resolve_atom(atom)),
            LiteralValueKind::Number(val) => Some(val.to_string()),
            _ => None,
        };

        if let Some(name) = prop_name {
            let result = self.get_object_property_type(object_type, &name);

            if result.is_none() || result == Some(TypeId::ERROR) {
                if object_type == TypeId::ERROR || index_type == TypeId::ERROR {
                    return;
                }

                let obj_type_str = self.format_type(object_type);

                // TS2339: Property does not exist
                let message = format_message(
                    diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                    &[&name, &obj_type_str],
                );
                let error_node = data.index_type;
                self.error_at_node(
                    error_node,
                    &message,
                    diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                );

                // TS2536: Type cannot be used to index type
                // Note: TypeScript emits this when the index type is not compatible with the index signature.
                // Since property access failed, it implies index signature also failed (or did not exist).
                // We construct the index type string (e.g. "0.0") for the message.
                // For literal types, this is the literal text.
                let index_type_str = format!("\"{name}\""); // Quote literal string
                let message_2536 = format_message(
                    diagnostic_messages::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
                    &[&index_type_str, &obj_type_str],
                );
                self.error_at_node(
                    error_node,
                    &message_2536,
                    diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
                );
            }
        }
    }
}
