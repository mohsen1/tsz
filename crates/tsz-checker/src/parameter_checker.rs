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
use tsz_parser::parser::node::NodeAccess;
use tsz_solver::TypeId;

// =============================================================================
// Parameter Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    fn is_immediately_invoked_function_like(&self, node_idx: NodeIndex) -> bool {
        let Some(ext) = self.ctx.arena.get_extended(node_idx) else {
            return false;
        };
        let parent_idx = ext.parent;
        if parent_idx.is_none() {
            return false;
        }
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };

        if parent_node.kind == tsz_parser::parser::syntax_kind_ext::CALL_EXPRESSION
            && let Some(call) = self.ctx.arena.get_call_expr(parent_node)
            && call.expression == node_idx
        {
            return true;
        }

        if parent_node.kind == tsz_parser::parser::syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(grand_ext) = self.ctx.arena.get_extended(parent_idx)
        {
            let grand_idx = grand_ext.parent;
            if !grand_idx.is_none()
                && let Some(grand_node) = self.ctx.arena.get(grand_idx)
                && grand_node.kind == tsz_parser::parser::syntax_kind_ext::CALL_EXPRESSION
                && let Some(call) = self.ctx.arena.get_call_expr(grand_node)
                && call.expression == parent_idx
            {
                return true;
            }
        }

        false
    }

    fn collect_parameter_forward_references_recursive(
        &self,
        node_idx: NodeIndex,
        later_name: &str,
        refs: &mut Vec<NodeIndex>,
    ) {
        use tsz_parser::parser::syntax_kind_ext;

        if node_idx.is_none() {
            return;
        }
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        if let Some(ident) = self.ctx.arena.get_identifier(node) {
            if ident.escaped_text == later_name {
                refs.push(node_idx);
            }
            return;
        }

        // Skip type-only references (e.g. typeof z in type position).
        if node.kind == syntax_kind_ext::TYPE_QUERY {
            return;
        }

        // Deferred function/class evaluation does not trigger TS2373.
        if (node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || node.kind == syntax_kind_ext::ARROW_FUNCTION)
            && !self.is_immediately_invoked_function_like(node_idx)
        {
            return;
        }

        // For class expressions:
        // - ES5/ES3 targets downlevel classes, so class body references are
        //   effectively evaluated in the initializer context.
        // - ES2015+ keeps deferred semantics except computed names.
        if node.kind == syntax_kind_ext::CLASS_EXPRESSION
            || node.kind == syntax_kind_ext::CLASS_DECLARATION
        {
            if self.ctx.compiler_options.target.is_es5() {
                for child_idx in self.ctx.arena.get_children(node_idx) {
                    self.collect_parameter_forward_references_recursive(
                        child_idx, later_name, refs,
                    );
                }
                return;
            }
            for child_idx in self.ctx.arena.get_children(node_idx) {
                if let Some(child) = self.ctx.arena.get(child_idx)
                    && child.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                {
                    self.collect_parameter_forward_references_recursive(
                        child_idx, later_name, refs,
                    );
                }
            }
            return;
        }

        for child_idx in self.ctx.arena.get_children(node_idx) {
            self.collect_parameter_forward_references_recursive(child_idx, later_name, refs);
        }
    }

    fn collect_parameter_forward_references(
        &self,
        init_idx: NodeIndex,
        later_name: &str,
    ) -> Vec<NodeIndex> {
        let mut refs = Vec::new();
        self.collect_parameter_forward_references_recursive(init_idx, later_name, &mut refs);
        refs
    }

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
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
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
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

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
                        diagnostic_messages::A_REQUIRED_PARAMETER_CANNOT_FOLLOW_AN_OPTIONAL_PARAMETER,
                        diagnostic_codes::A_REQUIRED_PARAMETER_CANNOT_FOLLOW_AN_OPTIONAL_PARAMETER,
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
        use crate::diagnostics::diagnostic_codes;

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
                    diagnostic_codes::A_PARAMETER_PROPERTY_IS_ONLY_ALLOWED_IN_A_CONSTRUCTOR_IMPLEMENTATION,
                );
            }
        }
    }

    // =========================================================================
    // Parameter Initializers
    // =========================================================================

    /// Check for parameter initializers that are not allowed by signature shape (TS2371).
    ///
    /// Parameter initializers are only valid in function/constructor implementations.
    /// This emits TS2371 when a signature has parameter initializers in either case:
    /// - Ambient/declaration contexts (`declare`)
    /// - Non-implementation signatures (no body), such as overloads and function types
    ///
    /// ## Error TS2371:
    /// "A parameter initializer is only allowed in a function or constructor implementation."
    pub(crate) fn check_non_impl_parameter_initializers(
        &mut self,
        parameters: &[NodeIndex],
        has_declare_modifier: bool,
        has_body: bool,
    ) {
        if has_body && !has_declare_modifier {
            return;
        }

        for &param_idx in parameters {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            // If parameter has an initializer in an ambient function, emit TS2371
            if !param.initializer.is_none() {
                self.error_at_node(
                    param.initializer,
                    "A parameter initializer is only allowed in a function or constructor implementation.",
                    2371, // TS2371
                );
            }
        }
    }

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
        for (param_pos, &param_idx) in parameters.iter().enumerate() {
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
                    use crate::diagnostics::diagnostic_codes;
                    let msg = format!("Parameter '{}' cannot reference itself.", param_name);
                    for ref_node in self_refs {
                        self.error_at_node(
                            ref_node,
                            &msg,
                            diagnostic_codes::PARAMETER_CANNOT_REFERENCE_ITSELF,
                        );
                    }
                }

                // TS2373: parameter default cannot reference later parameters
                for &later_param_idx in parameters.iter().skip(param_pos + 1) {
                    let Some(later_param_node) = self.ctx.arena.get(later_param_idx) else {
                        continue;
                    };
                    let Some(later_param) = self.ctx.arena.get_parameter(later_param_node) else {
                        continue;
                    };
                    let Some(later_name) = self.get_parameter_name(later_param.name) else {
                        continue;
                    };
                    let refs =
                        self.collect_parameter_forward_references(param.initializer, &later_name);
                    if refs.is_empty() {
                        continue;
                    }
                    let msg = format!(
                        "Parameter '{}' cannot reference identifier '{}' declared after it.",
                        param_name, later_name
                    );
                    for ref_node in refs {
                        self.error_at_node(
                            ref_node,
                            &msg,
                            crate::diagnostics::diagnostic_codes::PARAMETER_CANNOT_REFERENCE_IDENTIFIER_DECLARED_AFTER_IT,
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
            if declared_type != TypeId::ANY && !self.type_contains_error(declared_type) {
                let _ = self.check_assignable_or_report(init_type, declared_type, param_idx);
            }
        }
    }

    // =========================================================================
    // Rest Parameter Type Validation
    // =========================================================================

    /// Check that rest parameters have array types (TS2370).
    ///
    /// Rest parameters must be of an array type. This validates that `...rest`
    /// parameters have types like `T[]`, `Array<T>`, `[T, U]`, etc.
    ///
    /// ## Error TS2370:
    /// "A rest parameter must be of an array type."
    pub(crate) fn check_rest_parameter_types(&mut self, parameters: &[NodeIndex]) {
        use crate::diagnostics::diagnostic_codes;

        for &param_idx in parameters {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            // Only check rest parameters (those with ... token)
            if !param.dot_dot_dot_token {
                continue;
            }

            // If there's no type annotation, skip (implicitly any[])
            if param.type_annotation.is_none() {
                continue;
            }

            // Get the declared type
            let declared_type = self.get_type_from_type_node(param.type_annotation);

            // TypeScript accepts `...args: any` as a valid rest parameter type.
            // Also skip unresolved/error types to avoid cascading TS2370 when
            // type resolution itself already failed.
            if declared_type == TypeId::ANY
                || declared_type == TypeId::UNKNOWN
                || declared_type == TypeId::ERROR
            {
                continue;
            }

            // Check if the type is an array type
            // We need to use a Solver query to check this - following architecture rule
            // that Checker never inspects TypeData
            if !self.is_array_like_type(declared_type) {
                self.error_at_node(
                    param.type_annotation,
                    "A rest parameter must be of an array type.",
                    diagnostic_codes::A_REST_PARAMETER_MUST_BE_OF_AN_ARRAY_TYPE,
                );
            }
        }
    }
}
