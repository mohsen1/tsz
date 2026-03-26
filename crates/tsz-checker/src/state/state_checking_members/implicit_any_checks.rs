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
        self.maybe_report_implicit_any_parameter_with_type_hint(
            param,
            has_contextual_type,
            param_index,
            None,
        );
    }

    pub(crate) fn maybe_report_implicit_any_parameter_with_type_hint(
        &mut self,
        param: &tsz_parser::parser::node::ParameterData,
        has_contextual_type: bool,
        param_index: usize,
        implicit_type_hint: Option<&'static str>,
    ) {
        use crate::diagnostics::diagnostic_codes;

        // In tsc, both TS7019 (rest parameter) and TS7006/TS7051 (regular parameter)
        // implicit-any diagnostics are emitted as suggestions (not errors) when
        // noImplicitAny is off. Since we only track errors, gate both behind
        // noImplicitAny to match tsc's error-level behavior.
        if !self.ctx.no_implicit_any() || has_contextual_type {
            return;
        }
        // Skip rest parameters named 'arguments' — tsc emits TS1100 instead of TS7019
        // for `...arguments` because 'arguments' is a reserved identifier in strict mode.
        if param.dot_dot_dot_token
            && let Some(name_node) = self.ctx.arena.get(param.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && ident.escaped_text.as_str() == "arguments"
        {
            return;
        }
        // Skip parameters that have explicit type annotations
        if param.type_annotation.is_some() {
            return;
        }
        // Check if parameter has an initializer — any initializer (including null/undefined)
        // provides a type for the parameter. tsc infers `null` or `undefined` as the type,
        // so these do NOT trigger TS7006.
        if param.initializer.is_some() && implicit_type_hint.is_none() {
            return;
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
        if self.is_parameter_in_jsx_callback_context(param.name) {
            return;
        }

        let reserved_word_param = self.ctx.arena.get(param.name).and_then(|name_node| {
            self.ctx
                .arena
                .get_identifier(name_node)
                .map(|ident| ident.escaped_text.as_str())
        });
        let preserve_on_strict_mode_parse_error = reserved_word_param.is_some_and(|name| {
            crate::state_checking::is_strict_mode_reserved_name(name)
                || crate::state_checking::is_eval_or_arguments(name)
        });

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
        // Check both the parameter name AND the enclosing function/arrow for errors,
        // since parse errors like `(a): => {}` set flags on the parent, not on `a`.
        //
        // EXCEPTION: Rest parameters (dot_dot_dot_token) are NOT suppressed by parse errors.
        // tsc always emits TS7019 for rest parameters even when related parse errors exist
        // (e.g., TS1047 "rest can't be optional" for `...arg?`, TS1014 "rest not last"
        // for `...x, y`). The empty-name check below still catches truly malformed rest params.
        use tsz_parser::parser::node_flags;
        if !param.dot_dot_dot_token {
            if let Some(name_node) = self.ctx.arena.get(param.name) {
                let flags = name_node.flags as u32;
                if ((flags & node_flags::THIS_NODE_HAS_ERROR) != 0
                    || (flags & node_flags::THIS_NODE_OR_ANY_SUB_NODES_HAS_ERROR) != 0)
                    && !preserve_on_strict_mode_parse_error
                {
                    return;
                }
            }
            // Also check parent chain (parameter → function/arrow) for parse errors
            if let Some(ext) = self.ctx.arena.get_extended(param.name) {
                // param.name's parent is ParameterDeclaration; its parent is the function/arrow
                let param_decl = ext.parent;
                if let Some(param_node) = self.ctx.arena.get(param_decl) {
                    let flags = param_node.flags as u32;
                    if (flags & node_flags::THIS_NODE_OR_ANY_SUB_NODES_HAS_ERROR) != 0
                        && !preserve_on_strict_mode_parse_error
                    {
                        return;
                    }
                }
                if let Some(param_ext) = self.ctx.arena.get_extended(param_decl)
                    && let Some(func_node) = self.ctx.arena.get(param_ext.parent)
                {
                    let flags = func_node.flags as u32;
                    if (flags & node_flags::THIS_NODE_OR_ANY_SUB_NODES_HAS_ERROR) != 0
                        && !preserve_on_strict_mode_parse_error
                    {
                        return;
                    }
                }
            }

            // Suppress TS7006 when a scanner-level parse error (e.g. TS1127 invalid character)
            // exists near the parameter. This handles cases like `function f(a,¬) {}`
            // where the sibling token is invalid but the param node itself has no error flag.
            if self.has_syntax_parse_errors()
                && self.node_has_nearby_parse_error(param.name)
                && !preserve_on_strict_mode_parse_error
            {
                return;
            }
        }

        let param_name = self.parameter_name_for_error(param.name);
        // Skip if the parameter name is empty (parse recovery artifact)
        if param_name.is_empty() {
            return;
        }
        let implicit_type = implicit_type_hint.unwrap_or("any");

        // Rest parameters use TS7019, regular parameters use TS7006
        let report_node = self.param_node_for_implicit_any_diagnostic(param);

        // TS7051 only applies to parameters WITHOUT modifiers (public/private/protected/readonly).
        // When a parameter has a modifier, the name is clearly a parameter name, not a type.
        let has_parameter_modifiers = param
            .modifiers
            .as_ref()
            .is_some_and(|m| !m.nodes.is_empty());

        if param.dot_dot_dot_token {
            // TS7019/TS7051 for rest parameters: tsc anchors the span at the `...`
            // token, covering `...name`.  `normalized_anchor_span` would collapse the
            // Parameter node to just the name identifier, so we bypass it and emit with
            // the raw Parameter-node span (which starts at `...`).
            let rest_report_node = self
                .ctx
                .arena
                .get_extended(param.name)
                .map(|ext| ext.parent)
                .unwrap_or(report_node);
            // Get the span from the Parameter node directly (starts at `...`).
            // Use name end as the span end so modifiers/type annotations are excluded.
            let (rest_start, rest_len) = self
                .get_node_span(rest_report_node)
                .and_then(|(param_start, _)| {
                    let name_end = self.ctx.arena.get(param.name)?.end;
                    Some((param_start, name_end.saturating_sub(param_start)))
                })
                .unwrap_or_else(|| self.get_node_span(report_node).unwrap_or((0, 0)));

            if !has_parameter_modifiers && Self::is_type_keyword_name(&param_name) {
                let suggested_name = format!("arg{param_index}");
                let template = tsz_common::diagnostics::get_message_template(
                    diagnostic_codes::PARAMETER_HAS_A_NAME_BUT_NO_TYPE_DID_YOU_MEAN,
                )
                .unwrap_or("");
                let message =
                    crate::diagnostics::format_message(template, &[&suggested_name, &param_name]);
                self.error_at_position(
                    rest_start,
                    rest_len,
                    &message,
                    diagnostic_codes::PARAMETER_HAS_A_NAME_BUT_NO_TYPE_DID_YOU_MEAN,
                );
            } else {
                let template = tsz_common::diagnostics::get_message_template(
                    diagnostic_codes::REST_PARAMETER_IMPLICITLY_HAS_AN_ANY_TYPE,
                )
                .unwrap_or("");
                let message = crate::diagnostics::format_message(template, &[&param_name]);
                self.error_at_position(
                    rest_start,
                    rest_len,
                    &message,
                    diagnostic_codes::REST_PARAMETER_IMPLICITLY_HAS_AN_ANY_TYPE,
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
                        .is_some_and(|c| c.is_ascii_uppercase())
                    || Self::is_non_modifier_reserved_name(&param_name))
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
                    &[&param_name, implicit_type],
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

    /// Check if a parameter name is a strict-mode reserved word that tsc treats
    /// as a potential type annotation (TS7051) rather than a regular parameter name (TS7006).
    /// tsc emits TS7051 for reserved words like `package` that could plausibly be
    /// type names, but NOT for modifier keywords (`public`, `private`, `protected`)
    /// or flow control keywords (`yield`) which are clearly parameter names.
    fn is_non_modifier_reserved_name(name: &str) -> bool {
        matches!(
            name,
            "implements" | "interface" | "let" | "package" | "static"
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

    fn is_parameter_in_jsx_callback_context(&self, name_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = Some(name_idx);
        let mut function_idx = None;
        while let Some(idx) = current {
            let Some(node) = self.ctx.arena.get(idx) else {
                break;
            };
            if matches!(
                node.kind,
                syntax_kind_ext::ARROW_FUNCTION | syntax_kind_ext::FUNCTION_EXPRESSION
            ) {
                function_idx = Some(idx);
                break;
            }
            current = self.ctx.arena.get_extended(idx).map(|ext| ext.parent);
        }

        let Some(function_idx) = function_idx else {
            return false;
        };
        let Some(function_parent) = self
            .ctx
            .arena
            .get_extended(function_idx)
            .map(|ext| ext.parent)
        else {
            return false;
        };
        let Some(parent_node) = self.ctx.arena.get(function_parent) else {
            return false;
        };
        if parent_node.kind != syntax_kind_ext::JSX_EXPRESSION {
            return false;
        }

        let Some(jsx_parent) = self
            .ctx
            .arena
            .get_extended(function_parent)
            .map(|ext| ext.parent)
        else {
            return false;
        };
        let Some(jsx_parent_node) = self.ctx.arena.get(jsx_parent) else {
            return false;
        };

        matches!(
            jsx_parent_node.kind,
            syntax_kind_ext::JSX_ATTRIBUTE
                | syntax_kind_ext::JSX_ELEMENT
                | syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
                | syntax_kind_ext::JSX_FRAGMENT
        )
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
                            // Check if name is a nested pattern - if so, only recurse, don't report
                            // TS7031 for intermediate patterns. tsc only reports for leaf identifiers.
                            let name_is_pattern = self
                                .ctx
                                .arena
                                .get(binding_elem.name)
                                .map(|n| {
                                    n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                        || n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                                })
                                .unwrap_or(false);

                            if name_is_pattern {
                                // Recursively check nested patterns only
                                self.emit_implicit_any_parameter_for_pattern(
                                    binding_elem.name,
                                    is_rest_parameter,
                                );
                            } else {
                                // Leaf binding - report error if no initializer
                                let has_initializer = binding_elem.initializer.is_some();
                                if !has_initializer {
                                    let binding_name =
                                        self.parameter_name_for_error(binding_elem.name);

                                    let implicit_type =
                                        if is_rest_parameter { "any[]" } else { "any" };
                                    self.error_at_node_msg(
                                        binding_elem.name,
                                        diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE,
                                        &[&binding_name, implicit_type],
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
                        // Check if name is a nested pattern - if so, only recurse, don't report
                        // TS7031 for intermediate patterns. tsc only reports for leaf identifiers.
                        let name_is_pattern = self
                            .ctx
                            .arena
                            .get(binding_elem.name)
                            .map(|n| {
                                n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                    || n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                            })
                            .unwrap_or(false);

                        if name_is_pattern {
                            // Recursively check nested patterns only
                            self.emit_implicit_any_parameter_for_pattern(
                                binding_elem.name,
                                is_rest_parameter,
                            );
                        } else {
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
                        }
                    }
                }
            }
        }
    }

    /// Emit TS7031 errors for binding elements in destructuring variable declarations
    /// without type annotations or initializers (`var [a], {b};` under noImplicitAny).
    pub(crate) fn emit_implicit_any_for_var_destructuring(&mut self, pattern_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;
        use tsz_parser::parser::syntax_kind_ext;

        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };

        let pattern_kind = pattern_node.kind;

        if pattern_kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
            if let Some(pattern) = self.ctx.arena.get_binding_pattern(pattern_node) {
                for &element_idx in &pattern.elements.nodes {
                    if let Some(element_node) = self.ctx.arena.get(element_idx) {
                        if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                            continue;
                        }
                        if let Some(binding_elem) = self.ctx.arena.get_binding_element(element_node)
                        {
                            let name_is_pattern = self
                                .ctx
                                .arena
                                .get(binding_elem.name)
                                .map(|n| {
                                    n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                        || n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                                })
                                .unwrap_or(false);

                            if name_is_pattern {
                                self.emit_implicit_any_for_var_destructuring(binding_elem.name);
                            } else if binding_elem.initializer.is_none() {
                                let binding_name = self.parameter_name_for_error(binding_elem.name);
                                self.error_at_node_msg(
                                    binding_elem.name,
                                    diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE,
                                    &[&binding_name, "any"],
                                );
                            }
                        }
                    }
                }
            }
        } else if pattern_kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            && let Some(pattern) = self.ctx.arena.get_binding_pattern(pattern_node)
        {
            for &element_idx in &pattern.elements.nodes {
                if let Some(element_node) = self.ctx.arena.get(element_idx) {
                    if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                        continue;
                    }
                    if let Some(binding_elem) = self.ctx.arena.get_binding_element(element_node) {
                        let name_is_pattern = self
                            .ctx
                            .arena
                            .get(binding_elem.name)
                            .map(|n| {
                                n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                    || n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                            })
                            .unwrap_or(false);

                        if name_is_pattern {
                            self.emit_implicit_any_for_var_destructuring(binding_elem.name);
                        } else if binding_elem.initializer.is_none() {
                            let binding_name = self.parameter_name_for_error(binding_elem.name);
                            self.error_at_node_msg(
                                binding_elem.name,
                                diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE,
                                &[&binding_name, "any"],
                            );
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn ts7019_emitted_with_rest_not_last_parse_error() {
        // tsc emits TS7019 for rest params even when TS1014 (rest not last) is present.
        // TS1014 is a parser error (not in checker diagnostics), but TS7019 must appear.
        let codes = crate::test_utils::check_source_codes("function f(...x, y) { }");
        assert!(
            codes.contains(&7019),
            "Should have TS7019 for rest param even with parse errors, got {codes:?}"
        );
        // TS7006 should also be emitted for the regular parameter `y`
        assert!(
            codes.contains(&7006),
            "Should have TS7006 for regular param y, got {codes:?}"
        );
    }

    #[test]
    fn ts7019_emitted_with_syntax_parse_errors_flag() {
        // When has_syntax_parse_errors is set (as in the CLI driver path),
        // rest params should still get TS7019.
        let source = "function f(...x, y) { }";
        let options = crate::context::CheckerOptions::default();
        let mut parser =
            tsz_parser::parser::ParserState::new("test.ts".to_string(), source.to_string());
        let sf = parser.parse_source_file();
        let mut binder = tsz_binder::BinderState::new();
        binder.bind_source_file(parser.get_arena(), sf);
        let types = crate::query_boundaries::type_construction::TypeInterner::new();
        let mut checker = crate::state::CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            options,
        );
        checker.ctx.set_lib_contexts(Vec::new());
        // Simulate the CLI driver setting has_syntax_parse_errors
        checker.ctx.has_syntax_parse_errors = true;
        checker.check_source_file(sf);
        let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&7019),
            "Should have TS7019 for rest param with has_syntax_parse_errors, got {codes:?}"
        );
    }

    #[test]
    fn ts7019_emitted_with_optional_rest_parse_error() {
        // tsc emits TS7019 for rest params even when TS1047 (rest can't be optional) is present.
        // TS1047 is a parser error (not in checker diagnostics), but TS7019 must appear.
        let codes = crate::test_utils::check_source_codes("(...arg?) => 102;");
        assert!(
            codes.contains(&7019),
            "Should have TS7019 for rest param even with parse errors, got {codes:?}"
        );
    }
}
