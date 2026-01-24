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

use crate::checker::state::{CheckerState, MemberAccessLevel};
use crate::parser::node::ImportDeclData;
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use crate::solver::TypeId;

// =============================================================================
// Type Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Assignment and Expression Checking
    // =========================================================================

    /// Check an assignment expression, applying contextual typing to the RHS.
    ///
    /// This function validates that the right-hand side of an assignment is
    /// assignable to the left-hand side target type.
    ///
    /// ## Contextual Typing:
    /// - The LHS type is used as contextual type for the RHS expression
    /// - This enables better type inference for object literals, etc.
    ///
    /// ## Validation:
    /// - Checks constructor accessibility (if applicable)
    /// - Validates that RHS is assignable to LHS
    /// - Checks for excess properties in object literals
    /// - Validates readonly assignments
    pub(crate) fn check_assignment_expression(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
        expr_idx: NodeIndex,
    ) -> TypeId {
        let left_target = self.get_type_of_assignment_target(left_idx);
        let left_type = self.resolve_type_query_type(left_target);

        let prev_context = self.ctx.contextual_type;
        if left_type != TypeId::ANY && !self.type_contains_error(left_type) {
            self.ctx.contextual_type = Some(left_type);
        }

        let right_raw = self.get_type_of_node(right_idx);
        let right_type = self.resolve_type_query_type(right_raw);

        self.ctx.contextual_type = prev_context;

        self.ensure_application_symbols_resolved(right_type);
        self.ensure_application_symbols_resolved(left_type);

        self.check_readonly_assignment(left_idx, expr_idx);

        if left_type != TypeId::ANY {
            if let Some((source_level, target_level)) =
                self.constructor_accessibility_mismatch_for_assignment(left_idx, right_idx)
            {
                self.error_constructor_accessibility_not_assignable(
                    right_type,
                    left_type,
                    source_level,
                    target_level,
                    right_idx,
                );
            } else if !self.is_assignable_to(right_type, left_type)
                && !self.should_skip_weak_union_error(right_type, left_type, right_idx)
            {
                self.error_type_not_assignable_with_reason_at(right_type, left_type, right_idx);
            }

            if left_type != TypeId::UNKNOWN
                && let Some(right_node) = self.ctx.arena.get(right_idx)
                && right_node.kind == crate::parser::syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            {
                self.check_object_literal_excess_properties(right_type, left_type, right_idx);
            }
        }

        right_type
    }

    /// Check a compound assignment expression (+=, &&=, ??=, etc.).
    ///
    /// Compound assignments have special type computation rules:
    /// - Logical assignments (&&=, ||=, ??=) assign the RHS type
    /// - Other compound assignments assign the computed result type
    ///
    /// ## Type Computation:
    /// - Numeric operators (+, -, *, /, %) compute number type
    /// - Bitwise operators compute number type
    /// - Logical operators return RHS type
    pub(crate) fn check_compound_assignment_expression(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
        operator: u16,
        expr_idx: NodeIndex,
    ) -> TypeId {
        let left_target = self.get_type_of_assignment_target(left_idx);
        let left_type = self.resolve_type_query_type(left_target);

        let prev_context = self.ctx.contextual_type;
        if left_type != TypeId::ANY && !self.type_contains_error(left_type) {
            self.ctx.contextual_type = Some(left_type);
        }

        let right_raw = self.get_type_of_node(right_idx);
        let right_type = self.resolve_type_query_type(right_raw);

        self.ctx.contextual_type = prev_context;

        self.ensure_application_symbols_resolved(right_type);
        self.ensure_application_symbols_resolved(left_type);

        self.check_readonly_assignment(left_idx, expr_idx);

        let result_type = self.compound_assignment_result_type(left_type, right_type, operator);
        let is_logical_assignment = matches!(
            operator,
            k if k == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                || k == SyntaxKind::BarBarEqualsToken as u16
                || k == SyntaxKind::QuestionQuestionEqualsToken as u16
        );
        let assigned_type = if is_logical_assignment {
            right_type
        } else {
            result_type
        };

        if left_type != TypeId::ANY {
            if let Some((source_level, target_level)) =
                self.constructor_accessibility_mismatch_for_assignment(left_idx, right_idx)
            {
                self.error_constructor_accessibility_not_assignable(
                    assigned_type,
                    left_type,
                    source_level,
                    target_level,
                    right_idx,
                );
            } else if !self.is_assignable_to(assigned_type, left_type)
                && !self.should_skip_weak_union_error(right_type, left_type, right_idx)
            {
                self.error_type_not_assignable_with_reason_at(assigned_type, left_type, right_idx);
            }

            if left_type != TypeId::UNKNOWN
                && let Some(right_node) = self.ctx.arena.get(right_idx)
                && right_node.kind == crate::parser::syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            {
                self.check_object_literal_excess_properties(right_type, left_type, right_idx);
            }
        }

        result_type
    }

    /// Compute the result type of a compound assignment operator.
    ///
    /// This function determines what type a compound assignment expression
    /// produces based on the operator and operand types.
    fn compound_assignment_result_type(
        &self,
        left_type: TypeId,
        right_type: TypeId,
        operator: u16,
    ) -> TypeId {
        use crate::solver::{BinaryOpEvaluator, BinaryOpResult};

        let evaluator = BinaryOpEvaluator::new(self.ctx.types);
        let op_str = match operator {
            k if k == SyntaxKind::PlusEqualsToken as u16 => Some("+"),
            k if k == SyntaxKind::MinusEqualsToken as u16 => Some("-"),
            k if k == SyntaxKind::AsteriskEqualsToken as u16 => Some("*"),
            k if k == SyntaxKind::AsteriskAsteriskEqualsToken as u16 => Some("*"),
            k if k == SyntaxKind::SlashEqualsToken as u16 => Some("/"),
            k if k == SyntaxKind::PercentEqualsToken as u16 => Some("%"),
            k if k == SyntaxKind::AmpersandAmpersandEqualsToken as u16 => Some("&&"),
            k if k == SyntaxKind::BarBarEqualsToken as u16 => Some("||"),
            _ => None,
        };

        if let Some(op) = op_str {
            return match evaluator.evaluate(left_type, right_type, op) {
                BinaryOpResult::Success(result) => result,
                // Return ANY instead of UNKNOWN for type errors to prevent cascading errors
                BinaryOpResult::TypeError { .. } => TypeId::ANY,
            };
        }

        if operator == SyntaxKind::QuestionQuestionEqualsToken as u16 {
            return self.ctx.types.union2(left_type, right_type);
        }

        if matches!(
            operator,
            k if k == SyntaxKind::AmpersandEqualsToken as u16
                || k == SyntaxKind::BarEqualsToken as u16
                || k == SyntaxKind::CaretEqualsToken as u16
                || k == SyntaxKind::LessThanLessThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
        ) {
            return TypeId::NUMBER;
        }

        // Return ANY for unknown binary operand types to prevent cascading errors
        TypeId::ANY
    }

    // =========================================================================
    // Member and Declaration Validation
    // =========================================================================

    /// Check a computed property name for type errors.
    ///
    /// This function validates that the expression used for a computed
    /// property name is well-formed. It computes the type of the expression
    /// to ensure any type errors are reported.
    pub(crate) fn check_computed_property_name(&mut self, name_idx: NodeIndex) {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return;
        };

        if name_node.kind != crate::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return;
        }

        let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
            return;
        };

        let _ = self.get_type_of_node(computed.expression);
    }

    /// Check a class member name for computed property validation.
    ///
    /// This dispatches to check_computed_property_name for properties,
    /// methods, and accessors that use computed names.
    pub(crate) fn check_class_member_name(&mut self, member_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        match node.kind {
            k if k == crate::parser::syntax_kind_ext::PROPERTY_DECLARATION => {
                if let Some(prop) = self.ctx.arena.get_property_decl(node) {
                    self.check_computed_property_name(prop.name);
                }
            }
            k if k == crate::parser::syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.ctx.arena.get_method_decl(node) {
                    self.check_computed_property_name(method.name);
                }
            }
            k if k == crate::parser::syntax_kind_ext::GET_ACCESSOR
                || k == crate::parser::syntax_kind_ext::SET_ACCESSOR =>
            {
                if let Some(accessor) = self.ctx.arena.get_accessor(node) {
                    self.check_computed_property_name(accessor.name);
                }
            }
            _ => {}
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
    // Parameter Validation
    // =========================================================================

    /// Check for duplicate parameter names (TS2394).
    ///
    /// This function validates that all parameters in a function signature
    /// have unique names. It handles both simple identifiers and binding patterns.
    ///
    /// ## Duplicate Detection:
    /// - Collects all parameter names recursively
    /// - Handles object destructuring: { a, b }
    /// - Handles array destructuring: [x, y]
    /// - Emits TS2304 for each duplicate name
    pub(crate) fn check_duplicate_parameters(&mut self, parameters: &crate::parser::NodeList) {
        let mut seen_names = rustc_hash::FxHashSet::default();
        for &param_idx in &parameters.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            // Parameters can be identifiers or binding patterns
            if let Some(param) = self.ctx.arena.get_parameter(param_node) {
                self.collect_and_check_parameter_names(param.name, &mut seen_names);
            }
        }
    }

    /// Check for required parameters following optional parameters (TS1016).
    ///
    /// This function validates parameter ordering to ensure that required
    /// parameters don't appear after optional parameters.
    ///
    /// ## Parameter Ordering Rules:
    /// - Required parameters must come before optional parameters
    /// - A parameter is optional if it has `?` or an initializer
    /// - Rest parameters end the check (don't count as optional/required)
    ///
    /// ## Error TS1016:
    /// "A required parameter cannot follow an optional parameter."
    pub(crate) fn check_parameter_ordering(&mut self, parameters: &crate::parser::NodeList) {
        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};

        let mut seen_optional = false;

        for &param_idx in &parameters.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            // Rest parameter ends the check - rest params don't count as optional/required in this context
            if param.dot_dot_dot_token {
                break;
            }

            // A parameter is optional if it has a question token or an initializer
            let is_optional = param.question_token || !param.initializer.is_none();

            if is_optional {
                seen_optional = true;
            } else if seen_optional {
                // Required parameter after optional - emit TS1016
                // Report on the parameter name for better error highlighting
                self.error_at_node(
                    param.name,
                    diagnostic_messages::REQUIRED_PARAMETER_AFTER_OPTIONAL,
                    diagnostic_codes::REQUIRED_PARAMETER_AFTER_OPTIONAL,
                );
            }
        }
    }

    /// Recursively collect parameter names and check for duplicates.
    ///
    /// This helper function handles the recursive nature of parameter names,
    /// which can be simple identifiers or complex binding patterns.
    fn collect_and_check_parameter_names(
        &mut self,
        name_idx: NodeIndex,
        seen: &mut rustc_hash::FxHashSet<String>,
    ) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };
        use crate::scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(name_idx) else {
            return;
        };

        match node.kind {
            // Simple Identifier: parameter name
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(name) = self.node_text(name_idx) {
                    let name_str = name.to_string();
                    if !seen.insert(name_str.clone()) {
                        self.error_at_node(
                            name_idx,
                            &format_message(
                                diagnostic_messages::DUPLICATE_IDENTIFIER,
                                &[&name_str],
                            ),
                            diagnostic_codes::DUPLICATE_IDENTIFIER,
                        );
                    }
                }
            }
            // Object Binding Pattern: { a, b: c }
            k if k == crate::parser::syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                if let Some(pattern) = self.ctx.arena.get_binding_pattern(node) {
                    for &elem_idx in &pattern.elements.nodes {
                        self.collect_and_check_binding_element(elem_idx, seen);
                    }
                }
            }
            // Array Binding Pattern: [a, b]
            k if k == crate::parser::syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                if let Some(pattern) = self.ctx.arena.get_binding_pattern(node) {
                    for &elem_idx in &pattern.elements.nodes {
                        self.collect_and_check_binding_element(elem_idx, seen);
                    }
                }
            }
            _ => {}
        }
    }

    /// Check a binding element for duplicate names.
    ///
    /// This helper validates destructuring parameters with computed property names.
    fn collect_and_check_binding_element(
        &mut self,
        elem_idx: NodeIndex,
        seen: &mut rustc_hash::FxHashSet<String>,
    ) {
        if elem_idx.is_none() {
            return;
        }
        let Some(node) = self.ctx.arena.get(elem_idx) else {
            return;
        };

        // Handle holes in array destructuring: [a, , b]
        if node.kind == crate::parser::syntax_kind_ext::OMITTED_EXPRESSION {
            return;
        }

        if let Some(elem) = self.ctx.arena.get_binding_element(node) {
            // Check computed property name expression for unresolved identifiers (TS2304)
            // e.g., in `{[z]: x}` where `z` is undefined
            if !elem.property_name.is_none() {
                self.check_computed_property_name(elem.property_name);
            }
            // Recurse on the name (which can be an identifier or another pattern)
            self.collect_and_check_parameter_names(elem.name, seen);
        }
    }

    /// Check for parameter properties in function signatures (TS2374).
    ///
    /// Parameter properties (e.g., `constructor(public x: number)`) are only
    /// allowed in constructor implementations, not in function signatures.
    ///
    /// ## Error TS2374:
    /// "A parameter property is only allowed in a constructor implementation."
    pub(crate) fn check_parameter_properties(&mut self, parameters: &[NodeIndex]) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        for &param_idx in parameters {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            // If the parameter has modifiers, it's a parameter property
            // which is only allowed in constructors
            if param.modifiers.is_some() {
                self.error_at_node(
                    param_idx,
                    "A parameter property is only allowed in a constructor implementation.",
                    diagnostic_codes::PARAMETER_PROPERTY_NOT_ALLOWED,
                );
            }
        }
    }

    /// Check that parameter default values are assignable to declared parameter types.
    ///
    /// This function validates parameter initializers against their type annotations:
    /// - Emits TS2322 when the default value type doesn't match the parameter type
    /// - Checks for undefined identifiers in default expressions (TS2304)
    /// - Checks for self-referential parameter defaults (TS2372)
    ///
    /// ## Error TS2322:
    /// "Type X is not assignable to type Y."
    ///
    /// ## Error TS2372:
    /// "Parameter 'x' cannot reference itself."
    pub(crate) fn check_parameter_initializers(&mut self, parameters: &[NodeIndex]) {
        for &param_idx in parameters {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            // Check for TS7006 in nested function expressions within the default value
            if !param.initializer.is_none() {
                self.check_for_nested_function_ts7006(param.initializer);
            }

            // Skip if there's no initializer
            if param.initializer.is_none() {
                continue;
            }

            // TS2372: Check if the initializer references the parameter itself
            // e.g., function f(x = x) { } or function f(await = await) { }
            if let Some(param_name) = self.get_parameter_name(param.name)
                && self.initializer_references_name(param.initializer, &param_name)
            {
                use crate::checker::types::diagnostics::diagnostic_codes;
                self.error_at_node(
                    param.initializer,
                    &format!("Parameter '{}' cannot reference itself.", param_name),
                    diagnostic_codes::PARAMETER_CANNOT_REFERENCE_ITSELF,
                );
            }

            // IMPORTANT: Always resolve the initializer expression to check for undefined identifiers (TS2304)
            // This must happen regardless of whether there's a type annotation.
            let init_type = self.get_type_of_node(param.initializer);

            // Only check type assignability if there's a type annotation
            if param.type_annotation.is_none() {
                continue;
            }

            // Get the declared parameter type
            let declared_type = self.get_type_from_type_node(param.type_annotation);

            // Check if the initializer type is assignable to the declared type
            if declared_type != TypeId::ANY
                && !self.type_contains_error(declared_type)
                && !self.is_assignable_to(init_type, declared_type)
            {
                self.error_type_not_assignable_with_reason_at(init_type, declared_type, param_idx);
            }
        }
    }

    // =========================================================================
    // Accessibility and Member Checking
    // =========================================================================

    /// Check property accessibility for a property access expression.
    ///
    /// This function validates that a property access is allowed based on
    /// the access modifiers (private, protected, public) and the class hierarchy.
    ///
    /// ## Accessibility Rules:
    /// - **Private**: Only accessible within the declaring class
    /// - **Protected**: Accessible within declaring class and its subclasses
    /// - **Public**: Accessible from anywhere (default)
    ///
    /// ## Returns:
    /// - `true` if access is allowed
    /// - `false` if access is denied (error emitted)
    ///
    /// ## Error Codes:
    /// - TS2341: "Property '{}' is private and only accessible within class '{}'."
    /// - TS2445: "Property '{}' is protected and only accessible within class '{}' and its subclasses."
    pub(crate) fn check_property_accessibility(
        &mut self,
        object_expr: NodeIndex,
        property_name: &str,
        error_node: NodeIndex,
        object_type: TypeId,
    ) -> bool {
        use crate::checker::types::diagnostics::diagnostic_codes;

        let Some((class_idx, is_static)) = self.resolve_class_for_access(object_expr, object_type)
        else {
            return true;
        };
        let Some(access_info) = self.find_member_access_info(class_idx, property_name, is_static)
        else {
            return true;
        };

        let current_class_idx = self.ctx.enclosing_class.as_ref().map(|info| info.class_idx);
        let allowed = match access_info.level {
            MemberAccessLevel::Private => {
                current_class_idx == Some(access_info.declaring_class_idx)
            }
            MemberAccessLevel::Protected => match current_class_idx {
                None => false,
                Some(current_class_idx) => {
                    if current_class_idx == access_info.declaring_class_idx {
                        true
                    } else if !self
                        .is_class_derived_from(current_class_idx, access_info.declaring_class_idx)
                    {
                        false
                    } else {
                        let receiver_class_idx =
                            self.resolve_receiver_class_for_access(object_expr, object_type);
                        receiver_class_idx
                            .map(|receiver| {
                                receiver == current_class_idx
                                    || self.is_class_derived_from(receiver, current_class_idx)
                            })
                            .unwrap_or(false)
                    }
                }
            },
        };

        if allowed {
            return true;
        }

        match access_info.level {
            MemberAccessLevel::Private => {
                let message = format!(
                    "Property '{}' is private and only accessible within class '{}'.",
                    property_name, access_info.declaring_class_name
                );
                self.error_at_node(error_node, &message, diagnostic_codes::PROPERTY_IS_PRIVATE);
            }
            MemberAccessLevel::Protected => {
                let message = format!(
                    "Property '{}' is protected and only accessible within class '{}' and its subclasses.",
                    property_name, access_info.declaring_class_name
                );
                self.error_at_node(
                    error_node,
                    &message,
                    diagnostic_codes::PROPERTY_IS_PROTECTED,
                );
            }
        }

        false
    }

    /// Get the const modifier node from a list of modifiers, if present.
    ///
    /// Returns the NodeIndex of the const modifier for error reporting.
    /// Used to validate that readonly properties cannot have initializers.
    pub(crate) fn get_const_modifier(
        &self,
        modifiers: &Option<crate::parser::NodeList>,
    ) -> Option<NodeIndex> {
        use crate::scanner::SyntaxKind;
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx)
                    && mod_node.kind == SyntaxKind::ConstKeyword as u16
                {
                    return Some(mod_idx);
                }
            }
        }
        None
    }

    // =========================================================================
    // Accessor and Constructor Validation
    // =========================================================================

    /// Check setter parameter constraints (TS1052, TS1053, TS7006).
    ///
    /// This function validates that setter parameters comply with TypeScript rules:
    /// - TS1052: Setter parameters cannot have initializers
    /// - TS1053: Setter cannot have rest parameters
    /// - TS7006: Parameters without type annotations are implicitly 'any'
    ///
    /// ## Error Messages:
    /// - TS1052: "A 'set' accessor parameter cannot have an initializer."
    /// - TS1053: "A 'set' accessor cannot have rest parameter."
    pub(crate) fn check_setter_parameter(&mut self, parameters: &[NodeIndex]) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        for &param_idx in parameters {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            // Check for initializer (error 1052)
            if !param.initializer.is_none() {
                self.error_at_node(
                    param.name,
                    "A 'set' accessor parameter cannot have an initializer.",
                    diagnostic_codes::SETTER_PARAMETER_CANNOT_HAVE_INITIALIZER,
                );
            }

            // Check for rest parameter (error 1053)
            if param.dot_dot_dot_token {
                self.error_at_node(
                    param_idx,
                    "A 'set' accessor cannot have rest parameter.",
                    diagnostic_codes::SETTER_CANNOT_HAVE_REST_PARAMETER,
                );
            }

            // Check for implicit any (error 7006)
            // Setter parameters without type annotation implicitly have 'any' type
            self.maybe_report_implicit_any_parameter(param, false);
        }
    }

    // =========================================================================
    // Module and Import Validation
    // =========================================================================

    /// Check dynamic import module specifier for unresolved modules.
    ///
    /// Validates that the module specifier in a dynamic import() call
    /// can be resolved. Emits TS2307 if the module cannot be found.
    ///
    /// ## Parameters:
    /// - `call`: The call expression node for the import() call
    ///
    /// ## Validation:
    /// - Only checks string literal specifiers (dynamic specifiers cannot be statically checked)
    /// - Checks if module exists in resolved_modules, module_exports, shorthand_ambient_modules, or declared_modules
    /// - Emits TS2307 for unresolved module specifiers
    pub(crate) fn check_dynamic_import_module_specifier(
        &mut self,
        call: &crate::parser::node::CallExprData,
    ) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        if !self.ctx.report_unresolved_imports {
            return;
        }

        // Get the first argument (module specifier)
        let args = call
            .arguments
            .as_ref()
            .map(|a| &a.nodes)
            .map(|n| n.as_slice())
            .unwrap_or(&[]);

        if args.is_empty() {
            return; // No argument - will be caught by argument count check
        }

        let arg_idx = args[0];
        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
            return;
        };

        // Only check string literal module specifiers
        // Dynamic specifiers (variables, template literals) cannot be statically checked
        let Some(literal) = self.ctx.arena.get_literal(arg_node) else {
            return;
        };

        let module_name = &literal.text;

        // Check if the module was resolved by the CLI driver (multi-file mode)
        if let Some(ref resolved) = self.ctx.resolved_modules
            && resolved.contains(module_name)
        {
            return; // Module exists
        }

        // Check if the module exists in the module_exports map (cross-file module resolution)
        if self.ctx.binder.module_exports.contains_key(module_name) {
            return; // Module exists
        }

        // Check if this is a shorthand ambient module (declare module "foo")
        if self
            .ctx
            .binder
            .shorthand_ambient_modules
            .contains(module_name)
        {
            return; // Ambient module exists
        }

        // Check declared modules (regular ambient modules with body)
        if self.ctx.binder.declared_modules.contains(module_name) {
            return; // Declared module exists
        }

        // Module not found - emit TS2307
        let message = format_message(diagnostic_messages::CANNOT_FIND_MODULE, &[module_name]);
        self.error_at_node(arg_idx, &message, diagnostic_codes::CANNOT_FIND_MODULE);
    }

    /// Check export declaration module specifier for unresolved modules.
    ///
    /// Validates that the module specifier in an export ... from "module" statement
    /// can be resolved. Emits TS2307 if the module cannot be found.
    ///
    /// ## Parameters:
    /// - `stmt_idx`: The export declaration statement node
    ///
    /// ## Validation:
    /// - Checks if module exists in resolved_modules, module_exports, shorthand_ambient_modules, or declared_modules
    /// - Emits TS2307 for unresolved module specifiers
    pub(crate) fn check_export_module_specifier(&mut self, stmt_idx: NodeIndex) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        if !self.ctx.report_unresolved_imports {
            return;
        }

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(export_decl) = self.ctx.arena.get_export_decl(node) else {
            return;
        };

        // Get module specifier string
        let Some(spec_node) = self.ctx.arena.get(export_decl.module_specifier) else {
            return;
        };

        let Some(literal) = self.ctx.arena.get_literal(spec_node) else {
            return;
        };

        let module_name = &literal.text;

        // Check if the module was resolved by the CLI driver (multi-file mode)
        if let Some(ref resolved) = self.ctx.resolved_modules
            && resolved.contains(module_name)
        {
            return;
        }

        // Check if the module exists in the module_exports map (cross-file module resolution)
        if self.ctx.binder.module_exports.contains_key(module_name) {
            return;
        }

        // Skip TS2307 for ambient module declarations
        if self
            .ctx
            .binder
            .shorthand_ambient_modules
            .contains(module_name)
        {
            return;
        }

        if self.ctx.binder.declared_modules.contains(module_name) {
            return;
        }

        // Emit TS2307 for unresolved export module specifiers
        let message = format_message(diagnostic_messages::CANNOT_FIND_MODULE, &[module_name]);
        self.error_at_node(
            export_decl.module_specifier,
            &message,
            diagnostic_codes::CANNOT_FIND_MODULE,
        );
    }

    // =========================================================================
    // Accessor Validation
    // =========================================================================

    /// Check that accessor pairs (get/set) have consistent abstract modifiers.
    ///
    /// Validates that if a getter and setter for the same property both exist,
    /// they must both be abstract or both be non-abstract.
    /// Emits TS1044 on mismatched accessor abstract modifiers.
    ///
    /// ## Parameters:
    /// - `members`: Slice of class member node indices to check
    ///
    /// ## Validation:
    /// - Collects all getters and setters by property name
    /// - Checks for abstract/non-abstract mismatches
    /// - Reports TS1044 on both accessors if mismatch found
    pub(crate) fn check_accessor_abstract_consistency(&mut self, members: &[NodeIndex]) {
        use crate::checker::types::diagnostics::diagnostic_codes;
        use std::collections::HashMap;

        // Collect getters and setters by name
        #[derive(Default)]
        struct AccessorPair {
            getter: Option<(NodeIndex, bool)>, // (node_idx, is_abstract)
            setter: Option<(NodeIndex, bool)>,
        }

        let mut accessors: HashMap<String, AccessorPair> = HashMap::new();

        for &member_idx in members {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if (node.kind == syntax_kind_ext::GET_ACCESSOR
                || node.kind == syntax_kind_ext::SET_ACCESSOR)
                && let Some(accessor) = self.ctx.arena.get_accessor(node)
            {
                let is_abstract = self.has_abstract_modifier(&accessor.modifiers);

                // Get accessor name
                if let Some(name) = self.get_property_name(accessor.name) {
                    let pair = accessors.entry(name).or_default();
                    if node.kind == syntax_kind_ext::GET_ACCESSOR {
                        pair.getter = Some((member_idx, is_abstract));
                    } else {
                        pair.setter = Some((member_idx, is_abstract));
                    }
                }
            }
        }

        // Check for abstract mismatch
        for (_, pair) in accessors {
            if let (Some((getter_idx, getter_abstract)), Some((setter_idx, setter_abstract))) =
                (pair.getter, pair.setter)
                && getter_abstract != setter_abstract
            {
                // Report error on both accessors
                self.error_at_node(
                    getter_idx,
                    "Accessors must both be abstract or non-abstract.",
                    diagnostic_codes::ACCESSORS_MUST_BOTH_BE_ABSTRACT_OR_NOT,
                );
                self.error_at_node(
                    setter_idx,
                    "Accessors must both be abstract or non-abstract.",
                    diagnostic_codes::ACCESSORS_MUST_BOTH_BE_ABSTRACT_OR_NOT,
                );
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
    fn check_binding_element(
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
    /// Emits TS2461 if the type is not array-like.
    ///
    /// ## Parameters:
    /// - `pattern_idx`: The array binding pattern node index
    /// - `source_type`: The type being destructured
    ///
    /// ## Validation:
    /// - Checks if the type is array, tuple, string, or has iterator
    /// - Emits TS2461 for non-array-like types
    fn check_array_destructuring_target_type(
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

        // Check if the type is array-like (array, tuple, string, or has iterator)
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

    /// Check imported members for existence in module exports.
    ///
    /// Validates that named imports (e.g., `import { a, b } from "module"`)
    /// actually exist in the target module's exports. Emits TS2305 for
    /// missing exports.
    ///
    /// ## Parameters:
    /// - `import`: The import declaration data
    /// - `module_name`: The name of the module being imported from
    ///
    /// ## Validation:
    /// - Checks each named import against the module's exports table
    /// - Emits TS2305 for imports that don't exist in the module
    /// - Skips namespace imports and default imports (handled differently)
    pub(crate) fn check_imported_members(&mut self, import: &ImportDeclData, module_name: &str) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        // Get the import clause
        let clause_node = match self.ctx.arena.get(import.import_clause) {
            Some(node) => node,
            None => return,
        };

        let clause = match self.ctx.arena.get_import_clause(clause_node) {
            Some(c) => c,
            None => return,
        };

        // Get named_bindings (NamedImports or NamespaceImport)
        let bindings_node = match self.ctx.arena.get(clause.named_bindings) {
            Some(node) => node,
            None => return,
        };

        // Check if this is NamedImports (import { a, b })
        if bindings_node.kind == crate::parser::syntax_kind_ext::NAMED_IMPORTS {
            let named_imports = match self.ctx.arena.get_named_imports(bindings_node) {
                Some(ni) => ni,
                None => return,
            };

            // Get the module's exports table
            let exports_table = match self.ctx.binder.module_exports.get(module_name) {
                Some(table) => table,
                None => return,
            };

            // Check each import specifier
            for element_idx in &named_imports.elements.nodes {
                let element_node = match self.ctx.arena.get(*element_idx) {
                    Some(node) => node,
                    None => continue,
                };

                let specifier = match self.ctx.arena.get_specifier(element_node) {
                    Some(s) => s,
                    None => continue,
                };

                // Get the name being imported (property_name if present, otherwise name)
                let name_idx = if specifier.property_name.is_none() {
                    specifier.name
                } else {
                    specifier.property_name
                };

                let name_node = match self.ctx.arena.get(name_idx) {
                    Some(node) => node,
                    None => continue,
                };

                let identifier = match self.ctx.arena.get_identifier(name_node) {
                    Some(id) => id,
                    None => continue,
                };

                let import_name = &identifier.escaped_text;

                // Check if this import exists in the module's exports
                if !exports_table.has(import_name) {
                    // Emit TS2305: Module has no exported member
                    let message = format_message(
                        diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER,
                        &[module_name, import_name],
                    );
                    self.error_at_node(
                        specifier.name,
                        &message,
                        diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER,
                    );
                }
            }
        }
        // Note: Namespace imports (import * as ns) don't need individual checks
        // Default imports don't need checks here (they're handled differently)
    }

    // =========================================================================
    // Module Validation
    // =========================================================================

    /// Check a module body for statements and function implementations.
    ///
    /// Validates that module blocks contain valid statements and checks for
    /// function overload implementations.
    ///
    /// ## Parameters:
    /// - `body_idx`: The module body node index to check
    ///
    /// ## Validation:
    /// - Checks statements in module blocks
    /// - Validates function overload implementations
    /// - Handles nested namespace declarations
    pub(crate) fn check_module_body(&mut self, body_idx: NodeIndex) {
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };

        // Module body can be a MODULE_BLOCK or another MODULE_DECLARATION (for nested namespaces)
        if body_node.kind == syntax_kind_ext::MODULE_BLOCK {
            if let Some(block) = self.ctx.arena.get_module_block(body_node)
                && let Some(ref statements) = block.statements
            {
                // Check statements
                for &stmt_idx in &statements.nodes {
                    self.check_statement(stmt_idx);
                }
                // Check for function overload implementations
                self.check_function_implementations(&statements.nodes);
            }
        } else if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            // Nested namespace - recurse
            self.check_statement(body_idx);
        }
    }

    /// Check for export assignment conflicts with other exported elements.
    ///
    /// Validates that `export = X` is not used when there are also other
    /// exported elements (error 2309). Also checks that the exported
    /// expression exists (error 2304).
    ///
    /// ## Parameters:
    /// - `statements`: Slice of statement node indices to check
    ///
    /// ## Validation:
    /// - Checks for export assignment with other exports (TS2309)
    /// - Validates exported expression exists (TS2304)
    pub(crate) fn check_export_assignment(&mut self, statements: &[NodeIndex]) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        let mut export_assignment_idx: Option<NodeIndex> = None;
        let mut has_other_exports = false;

        for &stmt_idx in statements {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };

            match node.kind {
                syntax_kind_ext::EXPORT_ASSIGNMENT => {
                    export_assignment_idx = Some(stmt_idx);

                    // Check that the exported expression exists
                    if let Some(export_data) = self.ctx.arena.get_export_assignment(node) {
                        // Get the type of the expression (this will report 2304 if not found)
                        self.get_type_of_node(export_data.expression);
                    }
                }
                syntax_kind_ext::EXPORT_DECLARATION => {
                    // export { ... } or export * from '...'
                    has_other_exports = true;
                }
                _ => {
                    // Check for export modifiers on declarations
                    // (export class X, export function f, export const x, etc.)
                    if self.has_export_modifier(stmt_idx) {
                        has_other_exports = true;
                    }
                }
            }
        }

        // Report error 2309 if there's an export assignment AND other exports
        if let Some(export_idx) = export_assignment_idx
            && has_other_exports
        {
            self.error_at_node(
                export_idx,
                "An export assignment cannot be used in a module with other exported elements.",
                diagnostic_codes::EXPORT_ASSIGNMENT_WITH_OTHER_EXPORTS,
            );
        }
    }

    /// Check if a statement has an export modifier.
    ///
    /// ## Parameters:
    /// - `stmt_idx`: The statement node index to check
    ///
    /// Returns true if the statement has an export modifier.
    fn has_export_modifier(&self, stmt_idx: NodeIndex) -> bool {
        use crate::scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        // Check different declaration types for export modifier
        let modifiers = match node.kind {
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
            _ => None,
        };

        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx)
                    && mod_node.kind == SyntaxKind::ExportKeyword as u16
                {
                    return true;
                }
            }
        }

        false
    }

    // =========================================================================
    // Import Declaration Validation
    // =========================================================================

    /// Check an import equals declaration for ESM compatibility and unresolved modules.
    ///
    /// Validates `import x = require()` style imports, emitting TS1202 when used
    /// in ES modules and TS2307 when the module cannot be found.
    ///
    /// ## Parameters:
    /// - `stmt_idx`: The import equals declaration statement node
    ///
    /// ## Validation:
    /// - Emits TS1202 for import assignments in ES modules
    /// - Emits TS2307 for unresolved module specifiers
    pub(crate) fn check_import_equals_declaration(&mut self, stmt_idx: NodeIndex) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };
        let Some(import) = self.ctx.arena.get_import_decl(node) else {
            return;
        };

        // Get the module reference node
        let Some(ref_node) = self.ctx.arena.get(import.module_specifier) else {
            return;
        };

        // Check if this is an external module reference (require with string literal)
        // Internal namespace imports (identifiers/qualified names) don't need module resolution
        if ref_node.kind != SyntaxKind::StringLiteral as u16 {
            return;
        }

        // TS1202: Import assignment cannot be used when targeting ECMAScript modules.
        // This error is emitted when using `import x = require("y")` in a file that
        // has other ES module syntax (import/export).
        if self.ctx.binder.is_external_module() {
            self.error_at_node(
                stmt_idx,
                "Import assignment cannot be used when targeting ECMAScript modules. Consider using 'import * as ns from \"mod\"', 'import {a} from \"mod\"', 'import d from \"mod\"', or another module format instead.",
                diagnostic_codes::IMPORT_ASSIGNMENT_CANNOT_BE_USED_WITH_ESM,
            );
        }

        // TS2307: Cannot find module - check if the module can be resolved
        if !self.ctx.report_unresolved_imports {
            return;
        }

        let Some(literal) = self.ctx.arena.get_literal(ref_node) else {
            return;
        };
        let module_name = &literal.text;

        // Check if the module was resolved by the CLI driver (multi-file mode)
        if let Some(ref resolved) = self.ctx.resolved_modules
            && resolved.contains(module_name)
        {
            return; // Module exists
        }

        // Check if the module exists in the module_exports map (cross-file module resolution)
        if self.ctx.binder.module_exports.contains_key(module_name) {
            return; // Module exists
        }

        // Check if this is a shorthand ambient module (declare module "foo")
        if self
            .ctx
            .binder
            .shorthand_ambient_modules
            .contains(module_name)
        {
            return; // Ambient module exists
        }

        // Check declared modules (regular ambient modules with body)
        if self.ctx.binder.declared_modules.contains(module_name) {
            return; // Declared module exists
        }

        // Module not found - emit TS2307
        let message = format_message(diagnostic_messages::CANNOT_FIND_MODULE, &[module_name]);
        self.error_at_node(
            import.module_specifier,
            &message,
            diagnostic_codes::CANNOT_FIND_MODULE,
        );
    }

    /// Check an import declaration for unresolved modules and missing exports.
    ///
    /// Validates import declarations (e.g., `import { x } from "mod"`), emitting:
    /// - TS2307 when the module cannot be resolved
    /// - TS2305 when a module exists but doesn't export a specific member
    ///
    /// ## Parameters:
    /// - `stmt_idx`: The import declaration statement node
    ///
    /// ## Validation:
    /// - Checks if module specifier can be resolved
    /// - Validates that imported members exist in module exports
    pub(crate) fn check_import_declaration(&mut self, stmt_idx: NodeIndex) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        if !self.ctx.report_unresolved_imports {
            return;
        }

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(import) = self.ctx.arena.get_import_decl(node) else {
            return;
        };

        // Get module specifier string
        let Some(spec_node) = self.ctx.arena.get(import.module_specifier) else {
            return;
        };

        let Some(literal) = self.ctx.arena.get_literal(spec_node) else {
            return;
        };

        let module_name = &literal.text;

        // Check if the module was resolved by the CLI driver (multi-file mode)
        if let Some(ref resolved) = self.ctx.resolved_modules
            && resolved.contains(module_name)
        {
            // Module exists, check if individual imports are exported
            self.check_imported_members(import, module_name);
            return;
        }

        // Check if the module exists in the module_exports map (cross-file module resolution)
        // This enables resolving imports from other files in the same compilation
        if self.ctx.binder.module_exports.contains_key(module_name) {
            // Module exists, check if individual imports are exported
            self.check_imported_members(import, module_name);
            return;
        }

        // Skip TS2307 for ambient module declarations.
        // Both shorthand ambient modules (`declare module "foo"`) and regular ambient modules
        // with body (`declare module "foo" { ... }`) provide type information for imports.
        if self
            .ctx
            .binder
            .shorthand_ambient_modules
            .contains(module_name)
        {
            return; // Shorthand ambient module - imports typed as `any`
        }

        if self.ctx.binder.declared_modules.contains(module_name) {
            return; // Regular ambient module declaration
        }

        // In single-file mode, any external import is considered unresolved.
        // This is correct because WASM checker operates on individual files
        // without access to the module graph (aside from ambient module declarations).
        let message = format_message(diagnostic_messages::CANNOT_FIND_MODULE, &[module_name]);
        self.error_at_node(
            import.module_specifier,
            &message,
            diagnostic_codes::CANNOT_FIND_MODULE,
        );
    }
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

        // Get the expected return type from the function context
        let expected_type = self.current_return_type().unwrap_or(TypeId::UNKNOWN);

        // Get the type of the return expression (if any)
        let return_type = if !return_data.expression.is_none() {
            // TS1359: Check for await expressions outside async function
            self.check_await_expression(return_data.expression);

            let prev_context = self.ctx.contextual_type;
            if expected_type != TypeId::ANY && !self.type_contains_error(expected_type) {
                self.ctx.contextual_type = Some(expected_type);
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

        // VariableDeclarationList uses the same VariableData structure
        if let Some(var_list) = self.ctx.arena.get_variable(node) {
            // Now these are actual VariableDeclaration nodes
            for &decl_idx in &var_list.declarations.nodes {
                self.check_variable_declaration(decl_idx);
            }
        }
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
    /// - `idx`: The node index to start from
    ///
    /// Returns true if super is in a nested function inside a constructor.
    fn is_super_in_nested_function(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        let mut function_depth = 0;

        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            // Count function/arrow function boundaries
            if parent_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || parent_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || parent_node.kind == syntax_kind_ext::ARROW_FUNCTION
            {
                function_depth += 1;
            }

            // Check if we've reached the constructor
            if parent_node.kind == syntax_kind_ext::CONSTRUCTOR {
                // If function_depth > 0, super is in a nested function
                return function_depth > 0;
            }

            // Check if we've left the class
            if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                break;
            }

            current = parent_idx;
        }

        false
    }

    /// Check if a node is inside a class method body (non-static).
    ///
    /// Walks up the AST to determine if the node is within a non-static
    /// method declaration.
    ///
    /// ## Parameters:
    /// - `idx`: The node index to start from
    ///
    /// Returns true if inside a non-static method body.
    fn is_in_class_method_body(&self, idx: NodeIndex) -> bool {
        let mut current = idx;

        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            if parent_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                if let Some(method) = self.ctx.arena.get_method_decl(parent_node) {
                    // Check if it's not static
                    return !self.has_static_modifier(&method.modifiers);
                }
            }

            // Check if we've left the class
            if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                break;
            }

            current = parent_idx;
        }

        false
    }

    /// Check if a node is inside a class accessor body (getter/setter).
    ///
    /// Walks up the AST to determine if the node is within a
    /// getter or setter accessor.
    ///
    /// ## Parameters:
    /// - `idx`: The node index to start from
    ///
    /// Returns true if inside an accessor body.
    fn is_in_class_accessor_body(&self, idx: NodeIndex) -> bool {
        let mut current = idx;

        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            if parent_node.kind == syntax_kind_ext::GET_ACCESSOR
                || parent_node.kind == syntax_kind_ext::SET_ACCESSOR
            {
                return true;
            }

            // Check if we've left the class
            if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                break;
            }

            current = parent_idx;
        }

        false
    }

    /// Check a super expression for proper usage.
    ///
    /// Validates that super expressions are used correctly:
    /// - TS17011: super cannot be in static property initializers
    /// - TS2335: super can only be used in derived classes
    /// - TS2337: super() calls must be in constructors
    /// - TS2336: super property access must be in valid contexts
    ///
    /// ## Parameters:
    /// - `idx`: The super expression node index to check
    pub(crate) fn check_super_expression(&mut self, idx: NodeIndex) {
        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};

        // Check TS17011: super in static property initializer
        if let Some(ref class_info) = self.ctx.enclosing_class {
            if class_info.in_static_property_initializer {
                self.error_at_node(
                    idx,
                    diagnostic_messages::SUPER_IN_STATIC_PROPERTY_INITIALIZER,
                    diagnostic_codes::SUPER_IN_STATIC_PROPERTY_INITIALIZER,
                );
                return;
            }
        }

        // Check if we're in a class context at all
        let Some(class_info) = self.ctx.enclosing_class.clone() else {
            // TS2335: super outside of class (no enclosing class)
            self.error_at_node(
                idx,
                diagnostic_messages::SUPER_ONLY_IN_DERIVED_CLASS,
                diagnostic_codes::SUPER_ONLY_IN_DERIVED_CLASS,
            );
            return;
        };

        // Check if the class has a base class (is a derived class)
        let has_base_class = self.get_base_class_idx(class_info.class_idx).is_some();

        // Detect if this is a super() call or super property access
        let parent_info = self
            .ctx
            .arena
            .get_extended(idx)
            .and_then(|ext| self.ctx.arena.get(ext.parent).map(|n| (ext.parent, n)));

        let is_super_call = parent_info
            .as_ref()
            .map(|(_, parent_node)| {
                if parent_node.kind != syntax_kind_ext::CALL_EXPRESSION {
                    return false;
                }
                let Some(call) = self.ctx.arena.get_call_expr(parent_node) else {
                    return false;
                };
                call.expression == idx
            })
            .unwrap_or(false);

        let is_super_property_access = parent_info
            .as_ref()
            .map(|(_, parent_node)| {
                parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || parent_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            })
            .unwrap_or(false);

        // TS2335: super can only be referenced in a derived class
        if !has_base_class {
            if is_super_call {
                // TS2337: Super calls are not permitted outside constructors
                // But if there's no base class, it's really TS2335
                self.error_at_node(
                    idx,
                    diagnostic_messages::SUPER_ONLY_IN_DERIVED_CLASS,
                    diagnostic_codes::SUPER_ONLY_IN_DERIVED_CLASS,
                );
            } else {
                self.error_at_node(
                    idx,
                    diagnostic_messages::SUPER_ONLY_IN_DERIVED_CLASS,
                    diagnostic_codes::SUPER_ONLY_IN_DERIVED_CLASS,
                );
            }
            return;
        }

        // TS2337: Super calls are not permitted outside constructors
        if is_super_call {
            if !class_info.in_constructor {
                self.error_at_node(
                    idx,
                    diagnostic_messages::SUPER_CALL_NOT_IN_CONSTRUCTOR,
                    diagnostic_codes::SUPER_CALL_NOT_IN_CONSTRUCTOR,
                );
                return;
            }

            // Check for nested function inside constructor
            if self.is_super_in_nested_function(idx) {
                self.error_at_node(
                    idx,
                    diagnostic_messages::SUPER_CALL_NOT_IN_CONSTRUCTOR,
                    diagnostic_codes::SUPER_CALL_NOT_IN_CONSTRUCTOR,
                );
                return;
            }
        }

        // TS2336: super property access must be in constructor, method, or accessor
        if is_super_property_access {
            // Check if we're in a valid context for super property access
            let in_valid_context = class_info.in_constructor
                || self.is_in_class_method_body(idx)
                || self.is_in_class_accessor_body(idx);

            if !in_valid_context {
                self.error_at_node(
                    idx,
                    diagnostic_messages::SUPER_PROPERTY_ACCESS_INVALID_CONTEXT,
                    diagnostic_codes::SUPER_PROPERTY_ACCESS_INVALID_CONTEXT,
                );
            }
        }
    }

    // 16. Unreachable Code Detection (8 functions)

    /// Check if execution can fall through the end of a block of statements.
    ///
    /// A block falls through if all statements in it fall through.
    /// This is used to detect unreachable code and validate return statements.
    ///
    /// ## Parameters
    /// - `statements`: The list of statement node indices to check
    ///
    /// Returns true if execution can continue after the block, false if it always exits.
    pub(crate) fn block_falls_through(&mut self, statements: &[NodeIndex]) -> bool {
        for &stmt_idx in statements {
            if !self.statement_falls_through(stmt_idx) {
                return false;
            }
        }
        true
    }

    /// Check for unreachable code after return/throw statements in a block.
    ///
    /// Emits TS7027 for any statements that come after a return or throw,
    /// or after expressions of type 'never'.
    ///
    /// ## Parameters
    /// - `statements`: The list of statement node indices to check
    pub(crate) fn check_unreachable_code_in_block(&mut self, statements: &[NodeIndex]) {
        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};

        let mut unreachable = false;
        for &stmt_idx in statements {
            if unreachable {
                // This statement is unreachable
                self.error_at_node(
                    stmt_idx,
                    diagnostic_messages::UNREACHABLE_CODE_DETECTED,
                    diagnostic_codes::UNREACHABLE_CODE_DETECTED,
                );
            } else {
                // Check if this statement makes subsequent statements unreachable
                let Some(node) = self.ctx.arena.get(stmt_idx) else {
                    continue;
                };
                match node.kind {
                    syntax_kind_ext::RETURN_STATEMENT | syntax_kind_ext::THROW_STATEMENT => {
                        unreachable = true;
                    }
                    syntax_kind_ext::EXPRESSION_STATEMENT => {
                        // Check if the expression is of type 'never' (e.g., throw(), assertNever())
                        let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) else {
                            continue;
                        };
                        let expr_type = self.get_type_of_node(expr_stmt.expression);
                        if expr_type.is_never() {
                            unreachable = true;
                        }
                    }
                    syntax_kind_ext::VARIABLE_STATEMENT => {
                        // Check if any variable has a 'never' initializer
                        let Some(var_stmt) = self.ctx.arena.get_variable(node) else {
                            continue;
                        };
                        for &decl_idx in &var_stmt.declarations.nodes {
                            let Some(list_node) = self.ctx.arena.get(decl_idx) else {
                                continue;
                            };
                            let Some(var_list) = self.ctx.arena.get_variable(list_node) else {
                                continue;
                            };
                            for &list_decl_idx in &var_list.declarations.nodes {
                                let Some(list_decl_node) =
                                    self.ctx.arena.get(list_decl_idx)
                                else {
                                    continue;
                                };
                                let Some(decl) =
                                    self.ctx.arena.get_variable_declaration(list_decl_node)
                                else {
                                    continue;
                                };
                                if decl.initializer.is_none() {
                                    continue;
                                }
                                let init_type = self.get_type_of_node(decl.initializer);
                                if init_type.is_never() {
                                    unreachable = true;
                                    break;
                                }
                            }
                            if unreachable {
                                break;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// Check if execution can fall through a statement.
    ///
    /// Determines whether a statement always exits (return, throw, etc.)
    /// or if execution can continue to the next statement.
    ///
    /// ## Parameters
    /// - `stmt_idx`: The statement node index to check
    ///
    /// Returns true if execution can continue after the statement.
    fn statement_falls_through(&mut self, stmt_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return true;
        };

        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT | syntax_kind_ext::THROW_STATEMENT => false,
            syntax_kind_ext::BLOCK => self
                .ctx
                .arena
                .get_block(node)
                .map(|block| self.block_falls_through(&block.statements.nodes))
                .unwrap_or(true),
            syntax_kind_ext::EXPRESSION_STATEMENT => {
                let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) else {
                    return true;
                };
                let expr_type = self.get_type_of_node(expr_stmt.expression);
                !expr_type.is_never()
            }
            syntax_kind_ext::VARIABLE_STATEMENT => {
                let Some(var_stmt) = self.ctx.arena.get_variable(node) else {
                    return true;
                };
                for &decl_idx in &var_stmt.declarations.nodes {
                    let Some(list_node) = self.ctx.arena.get(decl_idx) else {
                        continue;
                    };
                    let Some(var_list) = self.ctx.arena.get_variable(list_node) else {
                        continue;
                    };
                    for &list_decl_idx in &var_list.declarations.nodes {
                        let Some(list_decl_node) = self.ctx.arena.get(list_decl_idx) else {
                            continue;
                        };
                        let Some(decl) =
                            self.ctx.arena.get_variable_declaration(list_decl_node)
                        else {
                            continue;
                        };
                        if decl.initializer.is_none() {
                            continue;
                        }
                        let init_type = self.get_type_of_node(decl.initializer);
                        if init_type.is_never() {
                            return false;
                        }
                    }
                }
                true
            }
            syntax_kind_ext::IF_STATEMENT => {
                let Some(if_data) = self.ctx.arena.get_if_statement(node) else {
                    return true;
                };
                let then_falls = self.statement_falls_through(if_data.then_statement);
                if if_data.else_statement.is_none() {
                    return true;
                }
                let else_falls = self.statement_falls_through(if_data.else_statement);
                then_falls || else_falls
            }
            syntax_kind_ext::SWITCH_STATEMENT => self.switch_falls_through(stmt_idx),
            syntax_kind_ext::TRY_STATEMENT => self.try_falls_through(stmt_idx),
            syntax_kind_ext::CATCH_CLAUSE => self
                .ctx
                .arena
                .get_catch_clause(node)
                .map(|catch_data| self.statement_falls_through(catch_data.block))
                .unwrap_or(true),
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => self.loop_falls_through(node),
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => true,
            syntax_kind_ext::LABELED_STATEMENT => self
                .ctx
                .arena
                .get_labeled_statement(node)
                .map(|labeled| self.statement_falls_through(labeled.statement))
                .unwrap_or(true),
            _ => true,
        }
    }

    /// Check if a switch statement falls through.
    ///
    /// A switch falls through if:
    /// - Any case block falls through, OR
    /// - There is no default clause
    ///
    /// ## Parameters
    /// - `switch_idx`: The switch statement node index
    ///
    /// Returns true if execution can continue after the switch.
    fn switch_falls_through(&mut self, switch_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(switch_idx) else {
            return true;
        };
        let Some(switch_data) = self.ctx.arena.get_switch(node) else {
            return true;
        };
        let Some(case_block_node) = self.ctx.arena.get(switch_data.case_block) else {
            return true;
        };
        let Some(case_block) = self.ctx.arena.get_block(case_block_node) else {
            return true;
        };

        let mut has_default = false;
        for &clause_idx in &case_block.statements.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            if clause_node.kind == syntax_kind_ext::DEFAULT_CLAUSE {
                has_default = true;
            }
            let Some(clause) = self.ctx.arena.get_case_clause(clause_node) else {
                continue;
            };
            if self.block_falls_through(&clause.statements.nodes) {
                return true;
            }
        }

        !has_default
    }

    /// Check if a try statement falls through.
    ///
    /// A try statement falls through if:
    /// - The try block falls through and there's no catch, OR
    /// - The try block falls through and the catch falls through, OR
    /// - The finally block falls through (if present)
    ///
    /// ## Parameters
    /// - `try_idx`: The try statement node index
    ///
    /// Returns true if execution can continue after the try statement.
    fn try_falls_through(&mut self, try_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(try_idx) else {
            return true;
        };
        let Some(try_data) = self.ctx.arena.get_try(node) else {
            return true;
        };

        let try_falls = self.statement_falls_through(try_data.try_block);
        let catch_falls = if !try_data.catch_clause.is_none() {
            self.statement_falls_through(try_data.catch_clause)
        } else {
            false
        };

        if !try_data.finally_block.is_none() {
            let finally_falls = self.statement_falls_through(try_data.finally_block);
            if !finally_falls {
                return false;
            }
        }

        try_falls || catch_falls
    }

    /// Check if a loop statement falls through.
    ///
    /// A loop does not fall through if:
    /// - The condition is always true (or missing), AND
    /// - There is no break statement in the loop body
    ///
    /// ## Parameters
    /// - `node`: The loop node (as a reference)
    ///
    /// Returns true if execution can continue after the loop.
    fn loop_falls_through(&mut self, node: &crate::parser::node::Node) -> bool {
        let Some(loop_data) = self.ctx.arena.get_loop(node) else {
            return true;
        };

        let condition_always_true = if loop_data.condition.is_none() {
            true
        } else {
            self.is_true_condition(loop_data.condition)
        };

        if condition_always_true && !self.contains_break_statement(loop_data.statement) {
            return false;
        }

        true
    }

    /// Check if a condition is always true.
    ///
    /// Currently only checks for literal `true`. Could be enhanced
    /// to evaluate constant expressions.
    ///
    /// ## Parameters
    /// - `condition_idx`: The condition expression node index
    ///
    /// Returns true if the condition is always true.
    fn is_true_condition(&self, condition_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(condition_idx) else {
            return false;
        };
        node.kind == SyntaxKind::TrueKeyword as u16
    }

    /// Check if a statement contains a break statement.
    ///
    /// Recursively searches for break statements, but doesn't recurse into:
    /// - Switch statements (breaks target the switch, not outer loop)
    /// - Nested loops (breaks target the inner loop, not outer)
    ///
    /// ## Parameters
    /// - `stmt_idx`: The statement node index to search
    ///
    /// Returns true if a break statement is found.
    fn contains_break_statement(&self, stmt_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        match node.kind {
            syntax_kind_ext::BREAK_STATEMENT => true,
            syntax_kind_ext::BLOCK => self
                .ctx
                .arena
                .get_block(node)
                .map(|block| {
                    block
                        .statements
                        .nodes
                        .iter()
                        .any(|&stmt| self.contains_break_statement(stmt))
                })
                .unwrap_or(false),
            syntax_kind_ext::IF_STATEMENT => self
                .ctx
                .arena
                .get_if_statement(node)
                .map(|if_data| {
                    self.contains_break_statement(if_data.then_statement)
                        || (!if_data.else_statement.is_none()
                            && self.contains_break_statement(if_data.else_statement))
                })
                .unwrap_or(false),
            // Don't recurse into switch statements - breaks inside target the switch, not outer loop
            syntax_kind_ext::SWITCH_STATEMENT => false,
            syntax_kind_ext::TRY_STATEMENT => self
                .ctx
                .arena
                .get_try(node)
                .map(|try_data| {
                    self.contains_break_statement(try_data.try_block)
                        || (!try_data.catch_clause.is_none()
                            && self.contains_break_statement(try_data.catch_clause))
                        || (!try_data.finally_block.is_none()
                            && self.contains_break_statement(try_data.finally_block))
                })
                .unwrap_or(false),
            // Don't recurse into nested loops - breaks inside target the nested loop, not outer loop
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT
            | syntax_kind_ext::FOR_IN_STATEMENT
            | syntax_kind_ext::FOR_OF_STATEMENT => false,
            syntax_kind_ext::LABELED_STATEMENT => self
                .ctx
                .arena
                .get_labeled_statement(node)
                .map(|labeled| self.contains_break_statement(labeled.statement))
                .unwrap_or(false),
            _ => false,
        }
    }
}

