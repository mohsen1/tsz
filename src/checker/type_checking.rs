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
use crate::parser::NodeIndex;
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
    pub(crate) fn get_const_modifier(&self, modifiers: &Option<crate::parser::NodeList>) -> Option<NodeIndex> {
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
}
