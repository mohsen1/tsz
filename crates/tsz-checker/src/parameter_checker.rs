//! Parameter Checking Module
//!
//! This module contains methods for validating function parameters.
//! It handles:
//! - Duplicate parameter names (TS2300)
//! - Parameter ordering (required after optional, TS1016)
//! - Parameter properties (TS2374)
//! - Parameter initializers and self-references (TS2322, TS2372)
//!
//! This module extends CheckerState with parameter-related methods as part of
//! the Phase 2 architecture refactoring (task 2.3 - file splitting).

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

// =============================================================================
// Parameter Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Duplicate Parameter Detection
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
    pub(crate) fn check_duplicate_parameters(&mut self, parameters: &tsz_parser::parser::NodeList) {
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

    /// Recursively collect parameter names and check for duplicates.
    ///
    /// This helper function handles the recursive nature of parameter names,
    /// which can be simple identifiers or complex binding patterns.
    fn collect_and_check_parameter_names(
        &mut self,
        name_idx: NodeIndex,
        seen: &mut rustc_hash::FxHashSet<String>,
    ) {
        use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_scanner::SyntaxKind;

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
            k if k == tsz_parser::parser::syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                if let Some(pattern) = self.ctx.arena.get_binding_pattern(node) {
                    for &elem_idx in &pattern.elements.nodes {
                        self.collect_and_check_binding_element(elem_idx, seen);
                    }
                }
            }
            // Array Binding Pattern: [a, b]
            k if k == tsz_parser::parser::syntax_kind_ext::ARRAY_BINDING_PATTERN => {
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
        if node.kind == tsz_parser::parser::syntax_kind_ext::OMITTED_EXPRESSION {
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

    // =========================================================================
    // Parameter Ordering
    // =========================================================================

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
    pub(crate) fn check_parameter_ordering(&mut self, parameters: &tsz_parser::parser::NodeList) {
        use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};

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

            // Only `?` token marks a parameter as "optional" for the seen_optional flag.
            // Parameters with initializers don't set seen_optional.
            if param.question_token {
                seen_optional = true;
            } else if seen_optional {
                // A parameter is "required" only if it has neither `?` nor an initializer.
                // Parameters with initializers (e.g., `options = {}`) are effectively optional
                // and don't trigger TS1016 even after `?` parameters.
                let has_initializer = !param.initializer.is_none();
                if !has_initializer {
                    self.error_at_node(
                        param.name,
                        diagnostic_messages::REQUIRED_PARAMETER_AFTER_OPTIONAL,
                        diagnostic_codes::REQUIRED_PARAMETER_AFTER_OPTIONAL,
                    );
                }
            }
        }
    }

    // =========================================================================
    // Parameter Properties
    // =========================================================================

    /// Check for parameter properties in function signatures (TS2374).
    ///
    /// Parameter properties (e.g., `constructor(public x: number)`) are only
    /// allowed in constructor implementations, not in function signatures.
    ///
    /// ## Error TS2374:
    /// "A parameter property is only allowed in a constructor implementation."
    pub(crate) fn check_parameter_properties(&mut self, parameters: &[NodeIndex]) {
        use crate::types::diagnostics::diagnostic_codes;

        for &param_idx in parameters {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            // If the parameter has parameter property modifiers (public/private/protected/readonly),
            // it's a parameter property which is only allowed in constructors.
            // Decorators on parameters are NOT parameter properties.
            if self.has_parameter_property_modifier(&param.modifiers) {
                self.error_at_node(
                    param_idx,
                    "A parameter property is only allowed in a constructor implementation.",
                    diagnostic_codes::PARAMETER_PROPERTY_NOT_ALLOWED,
                );
            }
        }
    }

    // =========================================================================
    // Parameter Initializers
    // =========================================================================

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
            // e.g., function f(x = x) { }, function f(x = x + 1) { }, or
            //        function f(b = b.toString()) { }
            // TSC emits one TS2372 error per self-referencing identifier in the
            // initializer expression tree (recursively, but stopping at scope
            // boundaries like function expressions, arrow functions, and class
            // expressions).
            if let Some(param_name) = self.get_parameter_name(param.name) {
                let self_refs = self.collect_self_references(param.initializer, &param_name);
                if !self_refs.is_empty() {
                    use crate::types::diagnostics::diagnostic_codes;
                    let msg = format!("Parameter '{}' cannot reference itself.", param_name);
                    for ref_node in self_refs {
                        self.error_at_node(
                            ref_node,
                            &msg,
                            diagnostic_codes::PARAMETER_CANNOT_REFERENCE_ITSELF,
                        );
                    }
                }
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
}
