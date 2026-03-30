//! Function parameter validation (duplicates, ordering, initializers).

use crate::context::TypingRequest;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_solver::TypeId;

// =============================================================================
// Parameter Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    fn parameter_pattern_has_concrete_type(
        &self,
        param_idx: NodeIndex,
        param: &tsz_parser::parser::node::ParameterData,
    ) -> bool {
        if param.type_annotation.is_some() {
            return true;
        }

        self.parameter_symbol_ids(param_idx, param.name)
            .into_iter()
            .flatten()
            .filter_map(|sym_id| self.ctx.symbol_types.get(&sym_id).copied())
            .any(|ty| ty != TypeId::ANY && ty != TypeId::UNKNOWN && ty != TypeId::ERROR)
    }

    fn collect_parameter_pattern_leaf_bindings(
        &self,
        pattern_idx: NodeIndex,
        out: &mut Vec<(NodeIndex, String, NodeIndex)>,
    ) {
        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };
        let Some(pattern) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        for &element_idx in &pattern.elements.nodes {
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            let Some(binding_elem) = self.ctx.arena.get_binding_element(element_node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(binding_elem.name) else {
                continue;
            };

            if name_node.kind == tsz_parser::parser::syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name_node.kind == tsz_parser::parser::syntax_kind_ext::ARRAY_BINDING_PATTERN
            {
                self.collect_parameter_pattern_leaf_bindings(binding_elem.name, out);
                continue;
            }

            out.push((
                binding_elem.name,
                self.parameter_name_for_error(binding_elem.name),
                binding_elem.initializer,
            ));
        }
    }

    fn emit_circular_implicit_any_for_parameter_pattern(&mut self, pattern_idx: NodeIndex) {
        let mut leaf_bindings = Vec::new();
        self.collect_parameter_pattern_leaf_bindings(pattern_idx, &mut leaf_bindings);

        for &(name_idx, ref name, initializer_idx) in &leaf_bindings {
            let self_referential_default = initializer_idx.is_some()
                && self.initializer_has_non_deferred_self_reference_by_name(initializer_idx, name);
            let captured_by_sibling_default = initializer_idx.is_none()
                && leaf_bindings
                    .iter()
                    .any(|&(other_name_idx, _, other_initializer_idx)| {
                        other_name_idx != name_idx
                            && other_initializer_idx.is_some()
                            && self.initializer_has_non_deferred_self_reference_by_name(
                                other_initializer_idx,
                                name,
                            )
                    });

            if self_referential_default || captured_by_sibling_default {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node_msg(
                    name_idx,
                    diagnostic_codes::IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION_AND_IS_REFERE,
                    &[name],
                );
            }
        }
    }

    pub(crate) fn check_strict_mode_reserved_parameter_names(
        &mut self,
        params: &[NodeIndex],
        strict_context_node: NodeIndex,
        use_class_strict_message: bool,
    ) {
        if !self.is_strict_mode_for_node(strict_context_node) {
            return;
        }

        for &param_idx in params {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            let Some(name_node) = self.ctx.arena.get(param.name) else {
                continue;
            };
            let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                continue;
            };

            // TS1212/TS1213/TS1214: Reserved word used as parameter name in strict mode
            if crate::state_checking::is_strict_mode_reserved_name(&ident.escaped_text) {
                self.emit_strict_mode_reserved_word_error(
                    param.name,
                    &ident.escaped_text,
                    use_class_strict_message,
                );
            }
            // TS1100: `eval` or `arguments` used as parameter name in strict mode.
            // In class contexts (`use_class_strict_message=true`), `arguments` is
            // reported as TS1210 instead, so only emit TS1100 for `eval` there.
            if crate::state_checking::is_eval_or_arguments(&ident.escaped_text)
                && (!use_class_strict_message || ident.escaped_text == "eval")
            {
                self.emit_eval_or_arguments_strict_mode_error(param.name, &ident.escaped_text);
            }
        }
    }

    /// Check type parameter names for strict-mode reserved words (TS1212/TS1213/TS1214).
    /// In strict mode, using a reserved word like `implements`, `interface`, `let`, etc.
    /// as a type parameter name is an error.
    pub(crate) fn check_strict_mode_reserved_type_parameter_names(
        &mut self,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
        strict_context_node: NodeIndex,
        use_class_strict_message: bool,
    ) {
        let Some(type_params) = type_parameters else {
            return;
        };
        if !self.is_strict_mode_for_node(strict_context_node) {
            return;
        }

        for &param_idx in &type_params.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(type_param) = self.ctx.arena.get_type_parameter(param_node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(type_param.name) else {
                continue;
            };
            let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                continue;
            };

            if crate::state_checking::is_strict_mode_reserved_name(&ident.escaped_text) {
                self.emit_strict_mode_reserved_word_error(
                    type_param.name,
                    &ident.escaped_text,
                    use_class_strict_message,
                );
            }
        }
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
            && !self.ctx.arena.is_immediately_invoked(node_idx)
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
    pub(crate) fn check_duplicate_parameters(
        &mut self,
        parameters: &tsz_parser::parser::NodeList,
        has_body: bool,
    ) {
        let mut seen_names = rustc_hash::FxHashMap::default();

        for &param_idx in &parameters.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            // Parameters can be identifiers or binding patterns
            if let Some(param) = self.ctx.arena.get_parameter(param_node) {
                self.collect_and_check_parameter_names(param.name, &mut seen_names, has_body);
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
        seen: &mut rustc_hash::FxHashMap<String, NodeIndex>,
        has_body: bool,
    ) {
        use crate::diagnostics::{diagnostic_messages, format_message};
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(name_idx) else {
            return;
        };

        match node.kind {
            // Simple Identifier: parameter name
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(name) = self.node_text(name_idx) {
                    let name_str = name;
                    match seen.entry(name_str.clone()) {
                        std::collections::hash_map::Entry::Occupied(entry) => {
                            let msg = format_message(
                                diagnostic_messages::DUPLICATE_IDENTIFIER,
                                &[&name_str],
                            );
                            let code = crate::diagnostics::diagnostic_codes::DUPLICATE_IDENTIFIER;
                            // Report on the first occurrence (only once)
                            let first_idx = *entry.get();
                            if first_idx != NodeIndex::NONE {
                                self.error_at_node(first_idx, &msg, code);
                                // Mark as already reported
                                *entry.into_mut() = NodeIndex::NONE;
                            }
                            // Report on this (duplicate) occurrence
                            self.error_at_node(name_idx, &msg, code);
                        }
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            entry.insert(name_idx);
                        }
                    }
                }
            }
            // Object Binding Pattern: { a, b: c }
            k if k == tsz_parser::parser::syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                if let Some(pattern) = self.ctx.arena.get_binding_pattern(node) {
                    for &elem_idx in &pattern.elements.nodes {
                        self.collect_and_check_binding_element(elem_idx, seen, has_body);
                    }
                }
            }
            // Array Binding Pattern: [a, b]
            k if k == tsz_parser::parser::syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                if let Some(pattern) = self.ctx.arena.get_binding_pattern(node) {
                    for &elem_idx in &pattern.elements.nodes {
                        self.collect_and_check_binding_element(elem_idx, seen, has_body);
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
        seen: &mut rustc_hash::FxHashMap<String, NodeIndex>,
        has_body: bool,
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

            if elem.property_name.is_some() {
                self.check_computed_property_name(elem.property_name);

                // TS2842: 'b' is an unused renaming of 'a'. Did you intend to use it as a type annotation?
                // This is emitted when both property_name and name are identifiers, and there's no body.
                if !has_body
                    && let Some(prop_node) = self.ctx.arena.get(elem.property_name)
                    && prop_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                    && let Some(name_node) = self.ctx.arena.get(elem.name)
                    && name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                {
                    let prop_name_str = self
                        .node_text(elem.property_name)
                        .unwrap_or_default()
                        .trim_end_matches(":")
                        .trim()
                        .to_string();
                    let name_str = self.node_text(elem.name).unwrap_or_default();
                    self.error_at_node_msg(
                                        elem.name,
                                        crate::diagnostics::diagnostic_codes::IS_AN_UNUSED_RENAMING_OF_DID_YOU_INTEND_TO_USE_IT_AS_A_TYPE_ANNOTATION,
                                        &[&name_str, &prop_name_str],
                                    );
                }
            }
            // Recurse on the name (which can be an identifier or another pattern)
            self.collect_and_check_parameter_names(elem.name, seen, has_body);
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
    /// - In JS files, JSDoc `@param {Type} [name]` or `@param {Type=} name` also marks optional
    /// - Rest parameters end the check (don't count as optional/required)
    ///
    /// ## Error TS1016:
    /// "A required parameter cannot follow an optional parameter."
    pub(crate) fn check_parameter_ordering(
        &mut self,
        parameters: &tsz_parser::parser::NodeList,
        func_idx: Option<tsz_parser::parser::NodeIndex>,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        // In JS files, get JSDoc to detect optional params via bracket/type= syntax
        let jsdoc = if self.is_js_file() {
            func_idx.and_then(|idx| self.get_jsdoc_for_function(idx))
        } else {
            None
        };

        let mut seen_optional = false;

        for &param_idx in &parameters.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            // TS1015: Parameter cannot have question mark and initializer.
            // This is a grammar check (in tsc it lives in the checker, not the parser).
            // Suppress when the file has syntax parse errors — tsc skips grammar checks
            // on subtrees from parser-recovery artifacts (e.g. broken arrow functions).
            if param.question_token
                && param.initializer.is_some()
                && !self.has_syntax_parse_errors()
            {
                self.error_at_node(
                    param.name,
                    diagnostic_messages::PARAMETER_CANNOT_HAVE_QUESTION_MARK_AND_INITIALIZER,
                    diagnostic_codes::PARAMETER_CANNOT_HAVE_QUESTION_MARK_AND_INITIALIZER,
                );
            }

            // Rest parameter ends the check - rest params don't count as optional/required in this context
            if param.dot_dot_dot_token {
                break;
            }

            // Check if this parameter is optional via `?` token or JSDoc annotations
            let is_optional = param.question_token
                || (jsdoc.is_some() && {
                    if let Some(name) = self.get_parameter_name(param.name) {
                        Self::is_jsdoc_param_optional(
                            jsdoc.as_deref().expect("guarded by jsdoc.is_some()"),
                            &name,
                        )
                    } else {
                        false
                    }
                });

            if is_optional {
                seen_optional = true;
            } else if seen_optional {
                // A parameter is "required" only if it has neither `?` nor an initializer.
                // Parameters with initializers (e.g., `options = {}`) are effectively optional
                // and don't trigger TS1016 even after `?` parameters.
                let has_initializer = param.initializer.is_some();
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

    /// Check if a JSDoc `@param` tag marks a parameter as optional.
    ///
    /// A parameter is JSDoc-optional if:
    /// - `@param {Type} [name]` — bracket syntax
    /// - `@param {Type} [name=default]` — bracket with default
    /// - `@param {Type=} name` — equals suffix on type expression
    ///
    /// Also handles backtick-quoted param names and name-first format.
    fn is_jsdoc_param_optional(jsdoc: &str, param_name: &str) -> bool {
        for chunk in jsdoc.split_inclusive('\n') {
            let trimmed = chunk
                .trim_end_matches('\n')
                .trim()
                .trim_start_matches('*')
                .trim();

            let effective = Self::skip_backtick_quoted(trimmed);

            if let Some(rest) = effective.strip_prefix("@param") {
                let rest = rest.trim();
                if rest.starts_with('{') {
                    // Format: @param {type} name
                    if let Some(close) = rest.find('}') {
                        let type_expr = &rest[1..close];
                        let after = rest[close + 1..].trim();
                        let name_token = after.split_whitespace().next().unwrap_or("");
                        // Strip backticks from name
                        let name_token = name_token.trim_matches('`');
                        // [name] or [name=default] means optional
                        let is_bracket_optional = name_token.starts_with('[');
                        let bare_name = name_token.trim_start_matches('[');
                        let bare_name = bare_name.split('=').next().unwrap_or(bare_name);
                        let bare_name = bare_name.trim_end_matches(']');
                        // {Type=} means optional
                        let is_type_optional = type_expr.ends_with('=');
                        if bare_name == param_name && (is_bracket_optional || is_type_optional) {
                            return true;
                        }
                    }
                } else {
                    // Format: @param name {type} or @param `name` {type}
                    let name_token = rest.split_whitespace().next().unwrap_or("");
                    let bare_name = name_token.trim_matches('`');
                    if bare_name == param_name {
                        // Check if there's a type with = suffix after the name
                        let after_name = rest[name_token.len()..].trim();
                        if after_name.starts_with('{')
                            && let Some(close) = after_name.find('}')
                        {
                            let type_expr = &after_name[1..close];
                            if type_expr.ends_with('=') {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    pub(crate) fn check_binding_pattern_optionality(
        &mut self,
        parameters: &[NodeIndex],
        has_body: bool,
        func_idx: Option<NodeIndex>,
    ) {
        if !has_body {
            return;
        }

        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_parser::parser::syntax_kind_ext::{ARRAY_BINDING_PATTERN, OBJECT_BINDING_PATTERN};

        for &param_idx in parameters {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            if param.initializer.is_none()
                && self.parameter_has_optional_binding_pattern_marker(param_idx, param, func_idx)
            {
                let Some(name_node) = self.ctx.arena.get(param.name) else {
                    continue;
                };

                if name_node.kind == OBJECT_BINDING_PATTERN
                    || name_node.kind == ARRAY_BINDING_PATTERN
                {
                    self.error_at_node(
                        param_idx,
                        diagnostic_messages::A_BINDING_PATTERN_PARAMETER_CANNOT_BE_OPTIONAL_IN_AN_IMPLEMENTATION_SIGNATURE,
                        diagnostic_codes::A_BINDING_PATTERN_PARAMETER_CANNOT_BE_OPTIONAL_IN_AN_IMPLEMENTATION_SIGNATURE,
                    );
                }
            }
        }
    }

    fn parameter_has_optional_binding_pattern_marker(
        &self,
        param_idx: NodeIndex,
        param: &tsz_parser::parser::node::ParameterData,
        func_idx: Option<NodeIndex>,
    ) -> bool {
        param.question_token
            || func_idx
                .or_else(|| self.enclosing_function_like_for_parameter(param_idx))
                .is_some_and(|idx| self.jsdoc_marks_parameter_optional(idx, param_idx, param.name))
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
            // tsc reports the error at the modifier keyword, not the parameter name.
            if let Some(modifier_idx) =
                self.find_first_parameter_property_modifier(&param.modifiers)
            {
                self.error_at_node(
                    modifier_idx,
                    "A parameter property is only allowed in a constructor implementation.",
                    diagnostic_codes::A_PARAMETER_PROPERTY_IS_ONLY_ALLOWED_IN_A_CONSTRUCTOR_IMPLEMENTATION,
                );
            }
        }
    }

    /// Find the first parameter property modifier in a modifier list.
    /// Returns the `NodeIndex` of the first public/private/protected/readonly/override keyword.
    pub(crate) fn find_first_parameter_property_modifier(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> Option<NodeIndex> {
        use tsz_scanner::SyntaxKind;
        let arena = self.ctx.arena;
        arena
            .find_modifier(modifiers, SyntaxKind::PublicKeyword)
            .or_else(|| arena.find_modifier(modifiers, SyntaxKind::PrivateKeyword))
            .or_else(|| arena.find_modifier(modifiers, SyntaxKind::ProtectedKeyword))
            .or_else(|| arena.find_modifier(modifiers, SyntaxKind::ReadonlyKeyword))
            .or_else(|| arena.find_modifier(modifiers, SyntaxKind::OverrideKeyword))
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
            // TSC anchors the error at the parameter name, not the initializer.
            if param.initializer.is_some() {
                self.error_at_node(
                    param.name,
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
        let factory = self.ctx.types.factory();
        for (param_pos, &param_idx) in parameters.iter().enumerate() {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            // Check for TS7006 in nested function expressions within the default value
            if param.initializer.is_some() {
                self.check_for_nested_function_ts7006(param.initializer);
            }

            if self.ctx.no_implicit_any()
                && !self.ctx.has_real_syntax_errors
                && !self.parameter_pattern_has_concrete_type(param_idx, param)
                && let Some(name_node) = self.ctx.arena.get(param.name)
                && (name_node.kind == tsz_parser::parser::syntax_kind_ext::OBJECT_BINDING_PATTERN
                    || name_node.kind == tsz_parser::parser::syntax_kind_ext::ARRAY_BINDING_PATTERN)
            {
                self.emit_circular_implicit_any_for_parameter_pattern(param.name);
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
                    let msg = format!("Parameter '{param_name}' cannot reference itself.");
                    for &ref_node in &self_refs {
                        self.error_at_node(
                            ref_node,
                            &msg,
                            diagnostic_codes::PARAMETER_CANNOT_REFERENCE_ITSELF,
                        );
                    }
                }

                if !self_refs.is_empty()
                    && self.ctx.no_implicit_any()
                    && !self.ctx.has_real_syntax_errors
                    && param.type_annotation.is_none()
                {
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node_msg(
                        param.name,
                        diagnostic_codes::IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION_AND_IS_REFERE,
                        &[&param_name],
                    );
                }

                // TS2502: When a typed parameter's effective type includes
                // `undefined`, the optionality-removal path reads the parameter's
                // own type while checking its default. A self-referential default
                // therefore becomes circular even when the annotation text itself
                // is not a `typeof` query.
                let declared_type = if param.type_annotation.is_some() {
                    let mut t = self.get_type_from_type_node(param.type_annotation);
                    if param.question_token
                        && self.ctx.strict_null_checks()
                        && t != TypeId::ANY
                        && t != TypeId::UNKNOWN
                        && t != TypeId::ERROR
                    {
                        t = factory.union2(t, TypeId::UNDEFINED);
                    }
                    Some(t)
                } else {
                    None
                };
                let has_effective_undefined = declared_type.is_some_and(|t| {
                    t != TypeId::ANY
                        && t != TypeId::UNKNOWN
                        && t != TypeId::ERROR
                        && tsz_solver::remove_undefined(self.ctx.types, t) != t
                });
                if !self_refs.is_empty() && has_effective_undefined {
                    self.error_at_node(
                        param.name,
                        &format!(
                            "'{param_name}' is referenced directly or indirectly in its own type annotation."
                        ),
                        2502,
                    );
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
                        "Parameter '{param_name}' cannot reference identifier '{later_name}' declared after it."
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

            // Get the declared parameter type (if annotated) and use it as
            // contextual type so that literal initializers keep their narrow types.
            // E.g., `function f(p: 1 = 1)` — without contextual typing, `1` widens
            // to `number` and fails assignability. With it, `1` stays as literal `1`.
            let declared_type = if param.type_annotation.is_some() {
                let mut t = self.get_type_from_type_node(param.type_annotation);
                if param.question_token
                    && self.ctx.strict_null_checks()
                    && t != TypeId::ANY
                    && t != TypeId::UNKNOWN
                    && t != TypeId::ERROR
                {
                    t = factory.union2(t, TypeId::UNDEFINED);
                }
                Some(t)
            } else if self
                .parameter_initializer_has_explicit_jsdoc_type(param_idx, param.name, param_pos)
            {
                self.parameter_symbol_ids(param_idx, param.name)
                    .into_iter()
                    .flatten()
                    .find_map(|sym_id| self.ctx.symbol_types.get(&sym_id).copied())
                    .filter(|&t| t != TypeId::ANY && t != TypeId::UNKNOWN && t != TypeId::ERROR)
            } else {
                None
            };

            let request = match declared_type {
                Some(dt) if dt != TypeId::ANY => TypingRequest::with_contextual_type(dt),
                _ => TypingRequest::NONE,
            };

            // IMPORTANT: Always resolve the initializer expression to check for undefined identifiers (TS2304)
            // This must happen regardless of whether there's a type annotation.
            let init_type = self.get_type_of_node_with_request(param.initializer, &request);

            // Only check type assignability if there's a type annotation
            let Some(declared_type) = declared_type else {
                continue;
            };

            // Check if the initializer type is assignable to the declared type
            if declared_type != TypeId::ANY && !self.type_contains_error(declared_type) {
                let _ = self.check_assignable_or_report(init_type, declared_type, param_idx);
            }
        }
    }

    fn parameter_initializer_has_explicit_jsdoc_type(
        &mut self,
        param_idx: NodeIndex,
        param_name: NodeIndex,
        param_pos: usize,
    ) -> bool {
        if !self.is_js_file() || self.param_has_inline_jsdoc_type(param_idx) {
            return self.is_js_file() && self.param_has_inline_jsdoc_type(param_idx);
        }

        let Some(func_idx) = self.enclosing_function_like_for_parameter(param_idx) else {
            return false;
        };
        let Some(jsdoc) = self.get_jsdoc_for_function(func_idx) else {
            return false;
        };

        let jsdoc_param_names: Vec<String> = Self::extract_jsdoc_param_names(&jsdoc)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        let pname = self.effective_jsdoc_param_name(param_name, &jsdoc_param_names, param_pos);
        if Self::jsdoc_has_param_type(&jsdoc, &pname)
            || Self::jsdoc_type_tag_declares_callable(&jsdoc)
        {
            return true;
        }

        if self.ctx.arena.get(param_name).is_some_and(|node| {
            node.kind == tsz_parser::parser::syntax_kind_ext::OBJECT_BINDING_PATTERN
                || node.kind == tsz_parser::parser::syntax_kind_ext::ARRAY_BINDING_PATTERN
        }) && Self::jsdoc_has_type_annotations(&jsdoc)
        {
            return true;
        }

        self.jsdoc_callable_type_annotation_for_function(func_idx)
            .is_some()
    }

    fn enclosing_function_like_for_parameter(&self, param_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = param_idx;
        for _ in 0..8 {
            let parent = self.ctx.arena.get_extended(current)?.parent;
            if parent.is_none() {
                return None;
            }
            let parent_node = self.ctx.arena.get(parent)?;
            if matches!(
                parent_node.kind,
                tsz_parser::parser::syntax_kind_ext::FUNCTION_DECLARATION
                    | tsz_parser::parser::syntax_kind_ext::FUNCTION_EXPRESSION
                    | tsz_parser::parser::syntax_kind_ext::ARROW_FUNCTION
                    | tsz_parser::parser::syntax_kind_ext::METHOD_DECLARATION
                    | tsz_parser::parser::syntax_kind_ext::CONSTRUCTOR
                    | tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR
                    | tsz_parser::parser::syntax_kind_ext::SET_ACCESSOR
            ) {
                return Some(parent);
            }
            current = parent;
        }
        None
    }

    // =========================================================================
    // Binding Pattern Default Value Validation for Parameters
    // =========================================================================

    /// Check that default values in destructuring parameter patterns are assignable
    /// to the declared property types.
    ///
    /// For `function f({ show: showRename = v => v }: Show)`, the default value
    /// `v => v` must be checked against `Show.show`'s type `(x: number) => string`.
    /// This is analogous to `check_binding_pattern` for variable declarations, but
    /// for function parameters.
    ///
    /// ## Error TS2322:
    /// "Type X is not assignable to type Y."
    pub(crate) fn check_parameter_binding_pattern_defaults(&mut self, parameters: &[NodeIndex]) {
        use tsz_parser::parser::syntax_kind_ext;

        for &param_idx in parameters {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            // Only process binding patterns (destructuring)
            let Some(name_node) = self.ctx.arena.get(param.name) else {
                continue;
            };
            if name_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN
                && name_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN
            {
                continue;
            }

            // TS2463 owns optional binding-pattern parameters in implementation
            // signatures. Once that grammar error is reported, do not also run the
            // binding-pattern property/default checker and emit cascaded TS2339.
            if param.initializer.is_none()
                && self.parameter_has_optional_binding_pattern_marker(param_idx, param, None)
            {
                continue;
            }

            // Get the parameter type: from type annotation or from cached symbol type
            let param_type = if param.type_annotation.is_some() {
                let t = self.get_type_from_type_node(param.type_annotation);
                if t == TypeId::ANY || t == TypeId::ERROR {
                    continue;
                }
                t
            } else {
                // Try to get cached type from symbol
                let Some(sym_id) = self
                    .parameter_symbol_ids(param_idx, param.name)
                    .into_iter()
                    .flatten()
                    .next()
                else {
                    continue;
                };
                let t = self.get_type_of_symbol(sym_id);
                if t == TypeId::ANY || t == TypeId::UNKNOWN || t == TypeId::ERROR {
                    continue;
                }
                t
            };

            // Delegate to check_binding_pattern which handles element type resolution,
            // contextual type for function-like initializers, and assignability checks.
            let request = TypingRequest::with_contextual_type(param_type);
            self.check_binding_pattern_with_request(param.name, param_type, true, &request);
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

            // In malformed signatures like `(...arg?) => {}`, TypeScript still
            // reports TS2370 in addition to TS1047/TS7019.
            // However, this is only reported for function expressions and arrow functions,
            // not for methods or function declarations.
            if param.question_token
                && param.type_annotation.is_none()
                && param.initializer.is_none()
            {
                let is_arrow_or_expr = if let Some(ext) = self.ctx.arena.get_extended(param_idx)
                    && let Some(parent_node) = self.ctx.arena.get(ext.parent)
                {
                    parent_node.kind == tsz_parser::parser::syntax_kind_ext::ARROW_FUNCTION
                        || parent_node.kind
                            == tsz_parser::parser::syntax_kind_ext::FUNCTION_EXPRESSION
                } else {
                    false
                };

                if is_arrow_or_expr {
                    self.error_at_node(
                        param.name,
                        "A rest parameter must be of an array type.",
                        diagnostic_codes::A_REST_PARAMETER_MUST_BE_OF_AN_ARRAY_TYPE,
                    );
                }
                continue;
            }

            if param.type_annotation.is_some() {
                // Has explicit type annotation — check the declared type
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

                // For deferred generic types (Application/Conditional containing
                // type parameters), skip the array-like check. These can't be fully
                // resolved at declaration time and tsc defers the check. Examples:
                //   ...args: ConstructorParameters<Ctor>
                //   ...args: ArgMap[K]
                let resolved = self.evaluate_type_with_resolution(declared_type);
                if tsz_solver::visitor::contains_type_parameters(self.ctx.types, resolved) {
                    continue;
                }

                // Use is_array_like_type first — it properly resolves type parameter
                // constraints (e.g., `T extends any[]` is recognized as array-like).
                // Fall back to assignability for custom array subclasses (e.g.,
                // `CoolArray<T> extends Array<T>` which is structurally array-like
                // but not recognized by classify_array_like as a raw Array/Tuple).
                if !self.is_array_like_type(declared_type) {
                    let factory = self.ctx.types.factory();
                    let any_array = factory.array(TypeId::ANY);
                    let readonly_any_array = factory.readonly_type(any_array);

                    if !self.is_assignable_to(declared_type, readonly_any_array) {
                        self.error_at_node(
                            param.type_annotation,
                            "A rest parameter must be of an array type.",
                            diagnostic_codes::A_REST_PARAMETER_MUST_BE_OF_AN_ARRAY_TYPE,
                        );
                    }
                }
            } else if param.initializer.is_some() {
                // No type annotation, but has initializer (e.g., `...bar = 0`).
                // Infer the type from the initializer.
                let init_type = self.get_type_of_node(param.initializer);
                if init_type != TypeId::ANY
                    && init_type != TypeId::UNKNOWN
                    && init_type != TypeId::ERROR
                    && !self.is_array_like_type(init_type)
                {
                    let factory = self.ctx.types.factory();
                    let any_array = factory.array(TypeId::ANY);
                    let readonly_any_array = factory.readonly_type(any_array);
                    if !self.is_assignable_to(init_type, readonly_any_array) {
                        self.error_at_node(
                            param_idx,
                            "A rest parameter must be of an array type.",
                            diagnostic_codes::A_REST_PARAMETER_MUST_BE_OF_AN_ARRAY_TYPE,
                        );
                    }
                }
            }
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod binding_pattern_defaults_tests {
    use crate::test_utils::{check_js_source_diagnostics, check_source_codes};

    /// Positive test: arrow function default correctly typed via contextual type.
    /// `v => v.toString()` returns string, matching `(x: number) => string`.
    #[test]
    fn arrow_default_matching_signature_no_error() {
        let codes = check_source_codes(
            "interface Show { show: (x: number) => string; }
             function f({ show = v => v.toString() }: Show) {}",
        );
        assert!(
            !codes.contains(&2322),
            "Should not emit TS2322 for matching arrow default: {codes:?}"
        );
    }

    /// Positive test: renamed property with arrow default, correct return type.
    #[test]
    fn renamed_property_arrow_default_no_error() {
        let codes = check_source_codes(
            r#"interface Show { show: (x: number) => string; }
               function f2({ "show": showRename = v => v.toString() }: Show) {}"#,
        );
        assert!(
            !codes.contains(&2322),
            "Should not emit TS2322 for matching renamed arrow default: {codes:?}"
        );
    }

    /// Positive test: string literal default matches union type.
    #[test]
    fn string_literal_default_matches_union_no_error() {
        let codes = check_source_codes(
            r#"interface StringUnion { prop: "foo" | "bar"; }
               function h({ prop = "foo" }: StringUnion) {}"#,
        );
        assert!(
            !codes.contains(&2322),
            "Should not emit TS2322 for matching string literal default: {codes:?}"
        );
    }

    /// Positive test: tuple default matches tuple type.
    #[test]
    fn tuple_default_matches_tuple_type_no_error() {
        let codes = check_source_codes(
            "interface Tuples { prop: [string, number]; }
             function g({ prop = [\"hello\", 1234] }: Tuples) {}",
        );
        assert!(
            !codes.contains(&2322),
            "Should not emit TS2322 for matching tuple default: {codes:?}"
        );
    }

    /// Optional property default — `check_binding_element` validates when
    /// element type includes undefined.
    #[test]
    fn optional_property_default_assignable_no_error() {
        let codes = check_source_codes(
            "interface Opts { name?: string; }
             function f({ name = \"default\" }: Opts) {}",
        );
        assert!(
            !codes.contains(&2322),
            "Should not emit TS2322 for assignable optional property default: {codes:?}"
        );
    }

    /// The `check_parameter_binding_pattern_defaults` infrastructure is called
    /// for function declarations with binding pattern parameters.
    #[test]
    fn parameter_binding_check_called_for_function_decl() {
        // This should not panic or crash — verifies the call path works.
        let codes = check_source_codes(
            "interface Config { debug?: boolean; }
             function init({ debug = false }: Config) {}",
        );
        assert!(
            !codes.contains(&2322),
            "Should not emit TS2322 for boolean default: {codes:?}"
        );
    }

    /// Nested object binding pattern with defaults.
    #[test]
    fn nested_object_binding_no_error_when_matching() {
        let codes = check_source_codes(
            "interface Show { show: (x: number) => string; }
             interface Nested { nested: Show }
             function ff({ nested = { show: v => v.toString() } }: Nested) {}",
        );
        assert!(
            !codes.contains(&2322),
            "Should not emit TS2322 for matching nested default: {codes:?}"
        );
    }

    #[test]
    fn optional_binding_pattern_parameter_reports_ts2463_without_ts2339() {
        let codes = check_source_codes(
            "function f({ x }?: { x: number }) {
                 return x;
             }",
        );
        assert!(
            codes.contains(&2463),
            "Expected TS2463 for optional binding-pattern parameter, got: {codes:?}"
        );
        assert!(
            !codes.contains(&2339),
            "Optional binding-pattern parameter should not cascade into TS2339: {codes:?}"
        );
    }

    #[test]
    fn arrow_optional_binding_pattern_parameter_reports_ts2463_without_ts2339() {
        let codes = check_source_codes("const f = ({ x }?: { x: number }) => x;");
        assert!(
            codes.contains(&2463),
            "Expected TS2463 for arrow optional binding-pattern parameter, got: {codes:?}"
        );
        assert!(
            !codes.contains(&2339),
            "Arrow optional binding-pattern parameter should not cascade into TS2339: {codes:?}"
        );
    }

    #[test]
    fn typed_binding_pattern_parameter_default_object_literal_suppresses_ts2339() {
        let codes = check_source_codes(
            "function f({ x }: { x?: number } = {}) {
                 return x;
             }",
        );
        assert!(
            !codes.contains(&2339),
            "Typed parameter default object literal should not trigger TS2339: {codes:?}"
        );
    }

    #[test]
    fn jsdoc_optional_binding_pattern_parameter_reports_ts2463_without_ts2339() {
        let diagnostics = check_js_source_diagnostics(
            "/**
              * @typedef Foo
              * @property {string} a
              */
             /**
              * @param {Foo} [options]
              */
             function f({ a = \"a\" }) {}",
        );
        let codes: Vec<u32> = diagnostics.iter().map(|diag| diag.code).collect();
        assert!(
            codes.contains(&2463),
            "Expected TS2463 for JSDoc-optional binding-pattern parameter, got: {codes:?}"
        );
        assert!(
            !codes.contains(&2339),
            "JSDoc-optional binding-pattern parameter should not cascade into TS2339: {codes:?}"
        );
    }
}

#[cfg(test)]
mod jsdoc_optional_param_tests {
    use crate::state::CheckerState;

    // Note: is_jsdoc_param_optional processes raw JSDoc comment text which
    // includes the `/** */` delimiters. Lines are split by '\n' and each line
    // is trimmed then stripped of leading '*'. For single-line JSDoc like
    // `/** @param ... */`, the leading `/**` starts with `/` so the `*` strip
    // doesn't reach the content. Use multiline format in tests to match real usage.

    #[test]
    fn bracket_syntax_marks_optional() {
        let jsdoc = "/**\n * @param {number} [x]\n */";
        assert!(CheckerState::is_jsdoc_param_optional(jsdoc, "x"));
    }

    #[test]
    fn bracket_with_default_marks_optional() {
        let jsdoc = "/**\n * @param {number} [x=0]\n */";
        assert!(CheckerState::is_jsdoc_param_optional(jsdoc, "x"));
    }

    #[test]
    fn type_equals_suffix_marks_optional() {
        let jsdoc = "/**\n * @param {number=} x\n */";
        assert!(CheckerState::is_jsdoc_param_optional(jsdoc, "x"));
    }

    #[test]
    fn plain_param_not_optional() {
        let jsdoc = "/**\n * @param {number} x\n */";
        assert!(!CheckerState::is_jsdoc_param_optional(jsdoc, "x"));
    }

    #[test]
    fn backtick_quoted_name_with_type_equals() {
        let jsdoc = "/**\n * @param {number=} `x`\n */";
        assert!(CheckerState::is_jsdoc_param_optional(jsdoc, "x"));
    }

    #[test]
    fn name_first_format_with_type_equals() {
        let jsdoc = "/**\n * @param x {number=}\n */";
        assert!(CheckerState::is_jsdoc_param_optional(jsdoc, "x"));
    }

    #[test]
    fn wrong_name_not_matched() {
        let jsdoc = "/**\n * @param {number} [y]\n */";
        assert!(!CheckerState::is_jsdoc_param_optional(jsdoc, "x"));
    }

    #[test]
    fn multiline_jsdoc_finds_correct_param() {
        let jsdoc = "/**\n * @param {number} a\n * @param {string} [b]\n */";
        assert!(!CheckerState::is_jsdoc_param_optional(jsdoc, "a"));
        assert!(CheckerState::is_jsdoc_param_optional(jsdoc, "b"));
    }
}

#[cfg(test)]
mod jsdoc_diagnostic_integration_tests {
    use crate::test_utils::check_js_source_diagnostics;

    /// TS1016: required param after JSDoc optional bracket param.
    #[test]
    fn ts1016_jsdoc_optional_bracket_then_required() {
        let diags = check_js_source_diagnostics(
            "/**\n * @param {number} [x]\n * @param {number} y\n */\nfunction f(x, y) {}",
        );
        // y is required after optional x — should NOT emit TS1016 since y is also required
        // Actually, x is optional (bracket), y is required after optional → TS1016 on y
        assert!(
            diags.iter().any(|d| d.code == 1016),
            "Expected TS1016 for required param after JSDoc optional: {diags:?}"
        );
    }

    /// No TS1016 when all params are required.
    #[test]
    fn no_ts1016_when_all_required() {
        let diags = check_js_source_diagnostics(
            "/**\n * @param {number} x\n * @param {number} y\n */\nfunction f(x, y) {}",
        );
        assert!(
            !diags.iter().any(|d| d.code == 1016),
            "Should not emit TS1016 when all params are required: {diags:?}"
        );
    }
}
