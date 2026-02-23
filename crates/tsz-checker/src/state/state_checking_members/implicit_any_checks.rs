//! Implicit `any` parameter diagnostic checks (TS7006, TS7019, TS7051).
//!
//! Detects parameters that implicitly have type `any` under `--noImplicitAny`
//! and emits the appropriate diagnostic for regular params, rest params, and
//! destructuring patterns.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    pub(crate) fn maybe_report_implicit_any_parameter(
        &mut self,
        param: &tsz_parser::parser::node::ParameterData,
        has_contextual_type: bool,
        param_index: usize,
    ) {
        use crate::diagnostics::diagnostic_codes;

        if !self.ctx.no_implicit_any() || has_contextual_type {
            return;
        }
        // Skip parameters that have explicit type annotations
        if param.type_annotation.is_some() {
            return;
        }
        // Check if parameter has an initializer
        if param.initializer.is_some() {
            // TypeScript infers type from initializer, EXCEPT for null and undefined
            // Parameters initialized with null/undefined still trigger TS7006
            use tsz_scanner::SyntaxKind;
            let initializer_is_null_or_undefined =
                if let Some(init_node) = self.ctx.arena.get(param.initializer) {
                    init_node.kind == SyntaxKind::NullKeyword as u16
                        || init_node.kind == SyntaxKind::UndefinedKeyword as u16
                } else {
                    false
                };

            // Skip only if initializer is NOT null or undefined
            if !initializer_is_null_or_undefined {
                return;
            }
            // Otherwise continue to emit TS7006 for null/undefined initializers
        }
        if self.is_this_parameter_name(param.name) {
            return;
        }
        if self.is_parameter_in_promise_executor(param.name) {
            return;
        }
        if self.is_parameter_in_iife(param.name) {
            return;
        }

        // Enhanced destructuring parameter detection
        // Check if the parameter name is a destructuring pattern (object/array binding)
        if let Some(name_node) = self.ctx.arena.get(param.name) {
            use tsz_parser::parser::syntax_kind_ext;

            let kind = name_node.kind;

            // Direct destructuring patterns
            if kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            {
                // For destructuring parameters, recursively check nested binding elements
                self.emit_implicit_any_parameter_for_pattern(param.name, param.dot_dot_dot_token);
                return;
            }
        }

        // Skip TS7006 for parameters on nodes with parse errors.
        // This prevents cascading "implicitly has any type" errors on malformed AST nodes.
        // The parse error itself should already be emitted (e.g., TS1005, TS2390).
        use tsz_parser::parser::node_flags;
        if let Some(name_node) = self.ctx.arena.get(param.name) {
            let flags = name_node.flags as u32;
            if (flags & node_flags::THIS_NODE_HAS_ERROR) != 0
                || (flags & node_flags::THIS_NODE_OR_ANY_SUB_NODES_HAS_ERROR) != 0
            {
                return;
            }
        }

        let param_name = self.parameter_name_for_error(param.name);
        // Skip if the parameter name is empty (parse recovery artifact)
        if param_name.is_empty() {
            return;
        }

        // Rest parameters use TS7019, regular parameters use TS7006
        let report_node = self.param_node_for_implicit_any_diagnostic(param);
        let rest_report_node = if param.dot_dot_dot_token {
            // TS7019 points at the `...` token span, not the parameter name.
            self.ctx
                .arena
                .get_extended(param.name)
                .map(|ext| ext.parent)
                .unwrap_or(report_node)
        } else {
            report_node
        };

        // TS7051 only applies to parameters WITHOUT modifiers (public/private/protected/readonly).
        // When a parameter has a modifier, the name is clearly a parameter name, not a type.
        let has_parameter_modifiers = param
            .modifiers
            .as_ref()
            .is_some_and(|m| !m.nodes.is_empty());

        if param.dot_dot_dot_token {
            // TS7051: Check if rest parameter name looks like a type keyword
            // e.g., `m(...string)` where `string` is likely meant as `...args: string[]`
            if !has_parameter_modifiers && Self::is_type_keyword_name(&param_name) {
                let suggested_name = format!("arg{param_index}");
                self.error_at_node_msg(
                    rest_report_node,
                    diagnostic_codes::PARAMETER_HAS_A_NAME_BUT_NO_TYPE_DID_YOU_MEAN,
                    &[&suggested_name, &param_name],
                );
            } else {
                self.error_at_node_msg(
                    rest_report_node,
                    diagnostic_codes::REST_PARAMETER_IMPLICITLY_HAS_AN_ANY_TYPE,
                    &[&param_name],
                );
            }
        } else {
            // TS7051: Detect parameters whose name looks like a type keyword or type name
            // e.g., `(string, number)` where the user likely meant `(arg0: string, arg1: number)`
            // TypeScript emits TS7051 for type keyword names and uppercase-starting names
            // (which conventionally refer to classes/interfaces).
            // Only when the parameter has NO modifiers (public A is clearly a parameter name).
            if !has_parameter_modifiers
                && (Self::is_type_keyword_name(&param_name)
                    || param_name
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_uppercase()))
            {
                let suggested_name = format!("arg{param_index}");
                self.error_at_node_msg(
                    report_node,
                    diagnostic_codes::PARAMETER_HAS_A_NAME_BUT_NO_TYPE_DID_YOU_MEAN,
                    &[&suggested_name, &param_name],
                );
            } else {
                self.error_at_node_msg(
                    report_node,
                    diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE,
                    &[&param_name, "any"],
                );
            }
        }
    }

    /// Check if a parameter name is a TypeScript type keyword.
    /// These keywords when used as parameter names strongly suggest the user
    /// intended them as type annotations, not parameter names.
    fn is_type_keyword_name(name: &str) -> bool {
        matches!(
            name,
            "string"
                | "number"
                | "boolean"
                | "symbol"
                | "void"
                | "object"
                | "undefined"
                | "bigint"
                | "never"
                | "any"
                | "unknown"
        )
    }

    fn param_node_for_implicit_any_diagnostic(
        &self,
        param: &tsz_parser::parser::node::ParameterData,
    ) -> NodeIndex {
        let Some(modifiers) = param.modifiers.as_ref() else {
            return param.name;
        };
        use tsz_scanner::SyntaxKind;
        for &mod_idx in &modifiers.nodes {
            let Some(mod_node) = self.ctx.arena.get(mod_idx) else {
                continue;
            };
            if mod_node.kind == SyntaxKind::PublicKeyword as u16
                || mod_node.kind == SyntaxKind::PrivateKeyword as u16
                || mod_node.kind == SyntaxKind::ProtectedKeyword as u16
                || mod_node.kind == SyntaxKind::ReadonlyKeyword as u16
            {
                return mod_idx;
            }
        }
        param.name
    }

    /// Emit TS7006 errors for nested binding elements in destructuring parameters.
    /// TypeScript reports implicit 'any' for individual bindings in patterns like:
    ///   function foo({ x, y }: any) {}  // no error on x, y with type annotation
    ///   function bar({ x, y }) {}        // errors on x and y
    pub(crate) fn emit_implicit_any_parameter_for_pattern(
        &mut self,
        pattern_idx: NodeIndex,
        is_rest_parameter: bool,
    ) {
        use crate::diagnostics::diagnostic_codes;
        use tsz_parser::parser::syntax_kind_ext;

        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };

        let pattern_kind = pattern_node.kind;

        // Handle object binding patterns: { x, y, z }
        if pattern_kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
            if let Some(pattern) = self.ctx.arena.get_binding_pattern(pattern_node) {
                for &element_idx in &pattern.elements.nodes {
                    if let Some(element_node) = self.ctx.arena.get(element_idx) {
                        // Skip omitted expressions
                        if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                            continue;
                        }

                        if let Some(binding_elem) = self.ctx.arena.get_binding_element(element_node)
                        {
                            // Check if this binding element has an initializer
                            let has_initializer = binding_elem.initializer.is_some();

                            // If no initializer, report error for implicit any
                            if !has_initializer {
                                // Get the property name (could be identifier or string literal)
                                let binding_name = if binding_elem.property_name.is_some() {
                                    self.parameter_name_for_error(binding_elem.property_name)
                                } else {
                                    self.parameter_name_for_error(binding_elem.name)
                                };

                                let implicit_type = if is_rest_parameter { "any[]" } else { "any" };
                                self.error_at_node_msg(
                                    binding_elem.name,
                                    diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE,
                                    &[&binding_name, implicit_type],
                                );
                            }

                            // Recursively check nested patterns
                            if let Some(name_node) = self.ctx.arena.get(binding_elem.name) {
                                let name_kind = name_node.kind;
                                if name_kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                    || name_kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                                {
                                    self.emit_implicit_any_parameter_for_pattern(
                                        binding_elem.name,
                                        is_rest_parameter,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        // Handle array binding patterns: [ x, y, z ]
        else if pattern_kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            && let Some(pattern) = self.ctx.arena.get_binding_pattern(pattern_node)
        {
            for &element_idx in &pattern.elements.nodes {
                if let Some(element_node) = self.ctx.arena.get(element_idx) {
                    let element_kind = element_node.kind;

                    // Skip omitted expressions (holes in array patterns)
                    if element_kind == syntax_kind_ext::OMITTED_EXPRESSION {
                        continue;
                    }

                    // Check if this element is a binding element with initializer
                    if let Some(binding_elem) = self.ctx.arena.get_binding_element(element_node) {
                        let has_initializer = binding_elem.initializer.is_some();

                        if !has_initializer {
                            let binding_name = self.parameter_name_for_error(binding_elem.name);

                            let implicit_type = if is_rest_parameter { "any[]" } else { "any" };
                            self.error_at_node_msg(
                                binding_elem.name,
                                diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE,
                                &[&binding_name, implicit_type],
                            );
                        }

                        // Recursively check nested patterns
                        if let Some(name_node) = self.ctx.arena.get(binding_elem.name) {
                            let name_kind = name_node.kind;
                            if name_kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                || name_kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                            {
                                self.emit_implicit_any_parameter_for_pattern(
                                    binding_elem.name,
                                    is_rest_parameter,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
