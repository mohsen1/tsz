//! Function parameter validation (duplicates, ordering, initializers).

use crate::context::TypingRequest;
use crate::query_boundaries::checkers::parameters as parameter_query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_solver::TypeId;

// Parameter Checking Methods
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
            .filter_map(|sym_id| self.ctx.symbol_types.get(&sym_id))
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
        // TS1359: `await` as a parameter name inside an async function-like.
        // Unlike the strict-mode reserved-name checks below, this is not gated
        // on strict mode, so it must run before the strict-mode early return.
        self.check_await_reserved_parameter_names(params);

        // TS1346/TS1347: `"use strict"` directive with a non-simple parameter
        // list. Also a checkGrammar diagnostic (not gated on strict mode), so it
        // runs before the strict-mode early return.
        self.check_use_strict_non_simple_parameter_list(params, strict_context_node);

        // tsc picks the class-context message from the identifier's own ancestor
        // chain (`getContainingClass`), not from whichever member walk happens to
        // be running: a parameter of a nested function declaration, of a
        // property-initializer arrow, or of a function inside a static block is
        // still "code contained in a class". Callers can only report the ambient
        // `enclosing_class`, which is `None` on every one of those paths, so
        // recover the class context structurally. Being inside a class also
        // *implies* strict mode, which is why this feeds the early return below.
        let use_class_strict_message =
            use_class_strict_message || self.nearest_enclosing_class(strict_context_node).is_some();

        if !use_class_strict_message && !self.is_strict_mode_for_node(strict_context_node) {
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

            let has_recovery_error_after_name =
                (self.ctx.has_parse_errors || !self.ctx.all_parse_error_positions.is_empty())
                    && self.ctx.all_parse_error_positions.iter().any(|&pos| {
                        pos >= name_node.end && pos <= param_node.end.max(name_node.end)
                    });

            // TS1212/TS1213/TS1214: Reserved word used as parameter name in strict mode.
            // Suppress when parser recovery has already reported a syntax error inside
            // this malformed parameter after the apparent name, e.g.
            // `constructor(public @dec p: number)` where tsc reports only TS1005.
            if crate::state_checking::is_strict_mode_reserved_name(&ident.escaped_text)
                && !has_recovery_error_after_name
            {
                if self.rest_parameter_name_is_recovery_artifact(param_idx, param.name) {
                    continue;
                }
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
            // TS1210: In class bodies, using `arguments` as a parameter name is
            // rejected with the class-strict message. `eval` is already handled
            // above via TS1100. Mirrors tsc:
            //
            //   class C { public foo(arguments: any) { } }
            //                       ^^^^^^^^^^
            //   error TS1210: Code contained in a class is evaluated in JavaScript's
            //   strict mode which does not allow this use of 'arguments'.
            //
            // Skip when the parameter has a parameter-property modifier
            // (`public`/`private`/`protected`/`readonly`/`override`) — that form
            // is a shorthand field declaration (e.g. `constructor(public arguments: ASTList)`)
            // and tsc does not emit TS1210 for those. See parserRealSource11.ts.
            if use_class_strict_message
                && ident.escaped_text == "arguments"
                && self
                    .find_first_parameter_property_modifier(&param.modifiers)
                    .is_none()
                // tsc skips this class-auto-strict bind error in JS files.
                // Mirrors the corresponding skip in
                // `emit_eval_or_arguments_strict_mode_error`; without it we
                // emit a spurious TS1210 on `class c { a(arguments) {} }` in
                // `b.js` (see `jsFileCompilationBindStrictModeErrors.ts`).
                && !self.ctx.is_js_file()
            {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
                if let Some((pos, end)) = self.ctx.get_node_span(param.name) {
                    self.ctx.error(
                        pos,
                        end - pos,
                        format_message(
                            diagnostic_messages::CODE_CONTAINED_IN_A_CLASS_IS_EVALUATED_IN_JAVASCRIPTS_STRICT_MODE_WHICH_DOES_NOT,
                            &[&ident.escaped_text],
                        ),
                        diagnostic_codes::CODE_CONTAINED_IN_A_CLASS_IS_EVALUATED_IN_JAVASCRIPTS_STRICT_MODE_WHICH_DOES_NOT,
                    );
                }
            }
        }
    }

    /// TS1359: reject `await` used as a parameter name when the parameter's
    /// immediately enclosing function-like is `async`. tsc parses a function's
    /// parameter list under that function's own Await context, so `await` is not
    /// a legal binding identifier there (see `asyncFunctionDeclaration5` /
    /// `asyncArrowFunction5`).
    ///
    /// This is a grammar check that tsc suppresses program-wide once any file in
    /// the program has a real syntax (parse) error — mirrored here via
    /// `has_parse_errors` (the program-wide `program_has_real_syntax_errors`
    /// flag). The `parser.asyncGenerators.*` suites witness the suppression: a
    /// sibling file's TS1109 drops every TS1359 in the program.
    fn check_await_reserved_parameter_names(&mut self, params: &[NodeIndex]) {
        if self.ctx.has_parse_errors {
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
            if ident.escaped_text != "await" {
                continue;
            }

            if self.parameter_enclosing_function_is_async(param_idx) {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node(
                    param.name,
                    "Identifier expected. 'await' is a reserved word that cannot be used here.",
                    diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE,
                );
            }
        }
    }

    /// Whether the function-like that owns `param_idx` carries the `async`
    /// modifier. Resolves to the first enclosing function-like (arrows included);
    /// a non-async function nested inside an async one — or inside a class static
    /// block — resets the Await context, exactly like tsc, because the walk stops
    /// at that inner function rather than the outer async scope.
    fn parameter_enclosing_function_is_async(&self, param_idx: NodeIndex) -> bool {
        let Some(func_idx) = self.enclosing_function_like_for_parameter(param_idx) else {
            return false;
        };
        let Some(func_node) = self.ctx.arena.get(func_idx) else {
            return false;
        };
        if let Some(func) = self.ctx.arena.get_function(func_node) {
            return func.is_async;
        }
        self.ctx
            .arena
            .get_method_decl(func_node)
            .is_some_and(|method| {
                self.ctx
                    .arena
                    .has_modifier(&method.modifiers, tsz_scanner::SyntaxKind::AsyncKeyword)
            })
    }

    fn rest_parameter_name_is_recovery_artifact(
        &self,
        param_idx: NodeIndex,
        name_idx: NodeIndex,
    ) -> bool {
        let Some(param_node) = self.ctx.arena.get(param_idx) else {
            return false;
        };
        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
            return false;
        };
        if !param.dot_dot_dot_token {
            return false;
        }
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return false;
        };
        self.ctx
            .syntax_parse_error_positions
            .iter()
            .any(|&pos| pos > name_node.end && pos < param_node.end)
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
        // Same structural class-context rule as the value-parameter path above.
        let use_class_strict_message =
            use_class_strict_message || self.nearest_enclosing_class(strict_context_node).is_some();
        if !use_class_strict_message && !self.is_strict_mode_for_node(strict_context_node) {
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

    pub(crate) fn collect_parameter_forward_references_recursive(
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

        // Deferred function/class evaluation does not trigger TS2373. An
        // immediately-invoked GENERATOR is still deferred (calling it does
        // not run the body), and tsc 7.0.2 also exempts async IIFEs
        // (capturedParametersInInitializers1 foo7/foo8 are clean).
        if node.is_function_expression_or_arrow()
            && (!self.ctx.arena.is_immediately_invoked(node_idx)
                || self
                    .ctx
                    .arena
                    .get_function(node)
                    .is_some_and(|func| func.asterisk_token || func.is_async))
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
            // ES2015+ semantics (tsc 7.0.2): the class body defers evaluation
            // EXCEPT the regions evaluated with the class expression itself —
            // heritage expressions, computed member names, and STATIC
            // property initializers (oracle: `static c = x` gets TS2373;
            // an instance `[x] = x` flags only its computed name — the
            // instance initializer runs at construction and stays deferred,
            // like method/accessor/constructor bodies).
            if let Some(class) = self.ctx.arena.get_class(node) {
                if let Some(clauses) = &class.heritage_clauses {
                    for &clause_idx in &clauses.nodes {
                        self.collect_parameter_forward_references_recursive(
                            clause_idx, later_name, refs,
                        );
                    }
                }
                for &member_idx in &class.members.nodes {
                    let Some(member_node) = self.ctx.arena.get(member_idx) else {
                        continue;
                    };
                    for name_child in self.ctx.arena.get_children(member_idx) {
                        if let Some(child) = self.ctx.arena.get(name_child)
                            && child.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                        {
                            self.collect_parameter_forward_references_recursive(
                                name_child, later_name, refs,
                            );
                        }
                    }
                    if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                        && let Some(prop) = self.ctx.arena.get_property_decl(member_node)
                        && prop.initializer.is_some()
                        && self
                            .ctx
                            .arena
                            .has_modifier(&prop.modifiers, tsz_scanner::SyntaxKind::StaticKeyword)
                    {
                        self.collect_parameter_forward_references_recursive(
                            prop.initializer,
                            later_name,
                            refs,
                        );
                    }
                }
            }
            return;
        }

        // Method and accessor declarations (object literals included) defer
        // their bodies like function expressions; only a computed member name
        // evaluates immediately (`{[z]() { return z; }}` flags the name `z`,
        // not the body reference).
        if matches!(
            node.kind,
            k if k == syntax_kind_ext::METHOD_DECLARATION
                || k == syntax_kind_ext::GET_ACCESSOR
                || k == syntax_kind_ext::SET_ACCESSOR
                || k == syntax_kind_ext::FUNCTION_DECLARATION
        ) {
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
                // The renaming is only *unused* when no `typeof` query in the
                // owning signature names the renamed binding; `typeof b` keeps
                // it used and `tsc` reports nothing.
                if !has_body
                    && let Some(prop_node) = self.ctx.arena.get(elem.property_name)
                    && prop_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                    && let Some(name_node) = self.ctx.arena.get(elem.name)
                    && name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                    && !self.ctx.arena.get_identifier_text(elem.name).is_some_and(|n| {
                        crate::types_domain::signature_binding_scope::binding_is_referenced_by_type_query(
                            &self.ctx, elem_idx, n,
                        )
                    })
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

        self.check_this_parameter_placement(parameters, func_idx);

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
                // tsc's `checkGrammarParameterList` returns at the first grammar
                // error found anywhere in the list, so this parameter list draws
                // no further TS1015/TS1016 diagnostics.
                break;
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
                    // tsc reports at most one grammar diagnostic per parameter
                    // list and stops walking; every later required parameter in
                    // this same list would otherwise draw its own TS1016.
                    break;
                }
            }
        }
    }

    /// Report the `this`-parameter placement and container errors for one
    /// signature's parameter list.
    ///
    /// A `this` parameter is legal only as the *first* parameter of a
    /// signature whose container can have one at all. `tsc` decides this in
    /// `checkParameter` from two structural facts and nothing else — the
    /// parameter's index in its own list, and the `SyntaxKind` of the
    /// container — and reports every arm that applies rather than stopping at
    /// the first:
    ///
    /// - not at index 0 -> `TS2680`
    /// - container constructs (`Constructor` / `ConstructSignature` /
    ///   `ConstructorType`) -> `TS2681`
    /// - container is an accessor (`GetAccessor` / `SetAccessor`) -> `TS2784`
    /// - container is an `ArrowFunction` -> `TS2730`, whose `this` is lexical
    ///
    /// The arms are independent: `class C { constructor(x: number, this: C) {} }`
    /// draws `TS2680` *and* `TS2681`, so this cannot be a match over the
    /// container kind.
    pub(crate) fn check_this_parameter_placement(
        &mut self,
        parameters: &tsz_parser::parser::NodeList,
        func_idx: Option<tsz_parser::parser::NodeIndex>,
    ) {
        let container_kind = func_idx
            .and_then(|idx| self.ctx.arena.get(idx))
            .map(|node| node.kind);
        check_this_parameter_placement_in_ctx(&mut self.ctx, parameters, container_kind);
    }

    /// Check if a JSDoc `@param` tag marks a parameter as optional.
    ///
    /// A parameter is JSDoc-optional if:
    /// - `@param {Type} [name]` — bracket syntax
    /// - `@param {Type} [name=default]` — bracket with default
    /// - `@param {Type=} name` — equals suffix on type expression
    ///
    /// Also handles backtick-quoted param names and name-first format.
    pub(crate) fn is_jsdoc_param_optional(jsdoc: &str, param_name: &str) -> bool {
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
    /// This emits TS2371 ("A parameter initializer is only allowed in a
    /// function or constructor implementation.") when a signature has
    /// parameter initializers in either case:
    /// - Ambient/declaration contexts (`declare`)
    /// - Non-implementation signatures (no body), such as overloads and function types
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

            // If parameter has an initializer in an ambient function, emit
            // TS2371. tsc anchors at the PARAMETER, not the initializer and not
            // the name: `getErrorSpanForNode` has no `SyntaxKind.Parameter`
            // case, so the span is the parameter's own. For a plain parameter
            // that is the name (the node starts there), but a parameter
            // property starts at its accessibility modifier — `declare class C
            // { constructor(public c = 10); }` anchors at `public`. Reporting
            // on `param_idx` lets `normalized_anchor_span` narrow to the name
            // only in the modifier-less case.
            let name = param.name;
            if param.initializer.is_some() {
                self.error_at_node(
                    param_idx,
                    "A parameter initializer is only allowed in a function or constructor implementation.",
                    2371, // TS2371
                );
            }

            // Defaults nested inside a destructuring binding pattern
            // (`{ mult = 1 }`, `[a = 1]`, nested) are equally illegal in a
            // body-less signature; recurse so they are reported too.
            crate::types_domain::type_node_helpers::check_binding_pattern_initializers(
                &mut self.ctx,
                name,
            );
        }
    }

    /// - Emits TS2322 when the default value type doesn't match the parameter type
    /// - Checks for undefined identifiers in default expressions (TS2304)
    /// - Checks for self-referential parameter defaults (TS2372:
    ///   "Parameter 'x' cannot reference itself.")
    pub(crate) fn check_parameter_initializers(
        &mut self,
        parameters: &[NodeIndex],
        owner_is_async: bool,
    ) {
        self.check_parameter_downlevel_body_capture(parameters);
        for (param_pos, &param_idx) in parameters.iter().enumerate() {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

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

            // TS1308 (#16072): gate on the owning function's async-ness, not
            // ambient state — the signature checks before the body pushes it.
            let saved_async_depth = self.ctx.enter_function_async_context(owner_is_async);
            self.check_await_expression(param.initializer);
            self.ctx.restore_async_context(saved_async_depth);

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
                        t = parameter_query::optional_parameter_type_with_undefined(
                            self.ctx.types,
                            t,
                        );
                    }
                    Some(t)
                } else {
                    None
                };
                let has_effective_undefined = declared_type.is_some_and(|t| {
                    t != TypeId::ANY
                        && t != TypeId::UNKNOWN
                        && t != TypeId::ERROR
                        && crate::query_boundaries::common::remove_undefined(self.ctx.types, t) != t
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
                    let mut refs =
                        self.collect_parameter_forward_references(param.initializer, &later_name);
                    refs.retain(|&ref_idx| !self.is_property_access_name_position(ref_idx));
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
                    t = parameter_query::optional_parameter_type_with_undefined(self.ctx.types, t);
                }
                Some(t)
            } else if self
                .parameter_initializer_has_explicit_jsdoc_type(param_idx, param.name, param_pos)
            {
                self.parameter_symbol_ids(param_idx, param.name)
                    .into_iter()
                    .flatten()
                    .find_map(|sym_id| self.ctx.symbol_types.get(&sym_id))
                    .filter(|&t| t != TypeId::ANY && t != TypeId::UNKNOWN && t != TypeId::ERROR)
            } else {
                None
            };

            let initializer_is_identifier = self
                .ctx
                .arena
                .get(param.initializer)
                .is_some_and(|node| node.kind == tsz_scanner::SyntaxKind::Identifier as u16);
            let request = match declared_type {
                Some(dt) if dt != TypeId::ANY && !initializer_is_identifier => {
                    TypingRequest::with_contextual_type(dt)
                }
                _ => TypingRequest::NONE,
            };

            // IMPORTANT: Always resolve the initializer expression to check for undefined identifiers (TS2304)
            // This must happen regardless of whether there's a type annotation.
            let init_type = self.get_type_of_node_with_request(param.initializer, &request);

            // Must run after get_type_of_node_with_request so that closures typed via
            // the contextual type above are already in implicit_any_checked_closures.
            self.check_for_nested_function_ts7006(param.initializer);

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

    pub(crate) fn enclosing_function_like_for_parameter(
        &self,
        param_idx: NodeIndex,
    ) -> Option<NodeIndex> {
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

            // Get the parameter type: from type annotation or from cached symbol type.
            // Track whether the type comes from an explicit annotation —
            // TSC only checks binding-element default assignability when there
            // is a declared type, not when the type is inferred from an initializer.
            let has_explicit_type = param.type_annotation.is_some();
            let param_type = if has_explicit_type {
                let t = self.get_type_from_type_node(param.type_annotation);
                if t == TypeId::ANY || t == TypeId::ERROR {
                    continue;
                }
                t
            } else if param.initializer.is_some() {
                let init_type =
                    self.get_type_of_node_with_request(param.initializer, &TypingRequest::NONE);
                if init_type != TypeId::ANY
                    && init_type != TypeId::UNKNOWN
                    && init_type != TypeId::ERROR
                {
                    init_type
                } else {
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
                }
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

            if let Some(name_node) = self.ctx.arena.get(param.name)
                && name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            {
                let is_iterable =
                    self.check_destructuring_iterability(param.name, param_type, param.initializer);
                if !is_iterable {
                    continue;
                }
            }

            // Delegate to check_binding_pattern which handles element type resolution,
            // contextual type for function-like initializers, and assignability checks.
            let request = TypingRequest::with_contextual_type(param_type);
            self.check_binding_pattern_with_request(
                param.name,
                param_type,
                has_explicit_type,
                &request,
            );
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
            // This is reported for function expressions, arrow functions, and
            // function/constructor types in type alias context.
            if param.question_token {
                let parent_kind = self
                    .ctx
                    .arena
                    .get_extended(param_idx)
                    .and_then(|ext| self.ctx.arena.get(ext.parent))
                    .map(|n| n.kind);

                let is_arrow_or_expr = parent_kind.is_some_and(|k| {
                    k == tsz_parser::parser::syntax_kind_ext::ARROW_FUNCTION
                        || k == tsz_parser::parser::syntax_kind_ext::FUNCTION_EXPRESSION
                });

                let is_callable_type = parent_kind.is_some_and(|k| {
                    k == tsz_parser::parser::syntax_kind_ext::FUNCTION_TYPE
                        || k == tsz_parser::parser::syntax_kind_ext::CONSTRUCTOR_TYPE
                });

                // For arrow/function expressions: only emit TS2370 if no type annotation and no initializer
                // For callable types in type aliases: always emit TS2370 for optional rest params
                let should_emit = if is_callable_type {
                    true
                } else if is_arrow_or_expr {
                    // An unannotated rest param on an arrow/function expression is
                    // implicitly `any[]` — a valid array type — so TS2370 only fires
                    // under noImplicitAny (tsc reports the implicit-any/rest grammar
                    // separately otherwise). Gate to match.
                    self.ctx.no_implicit_any()
                        && param.type_annotation.is_none()
                        && param.initializer.is_none()
                } else {
                    false
                };

                if should_emit {
                    // For callable types, report at the parameter position (includes the ...)
                    // to match tsc's error position behavior. Use error_at_position directly
                    // because error_at_node would normalize to the name node.
                    if is_callable_type {
                        if let Some(pn) = self.ctx.arena.get(param_idx) {
                            // Report at the ... token position (param node start)
                            // Use name's length to get correct span length
                            if let Some(name_node) = self.ctx.arena.get(param.name) {
                                let length = name_node.end.saturating_sub(pn.pos);
                                self.error_at_position(
                                    pn.pos,
                                    length,
                                    "A rest parameter must be of an array type.",
                                    diagnostic_codes::A_REST_PARAMETER_MUST_BE_OF_AN_ARRAY_TYPE,
                                );
                            }
                        }
                    } else {
                        self.error_at_node(
                            param.name,
                            "A rest parameter must be of an array type.",
                            diagnostic_codes::A_REST_PARAMETER_MUST_BE_OF_AN_ARRAY_TYPE,
                        );
                    }
                }

                // For callable types, we've handled TS2370; continue to skip the normal array check.
                // For arrow/expr, continue only if we emitted or if there's no type annotation.
                if is_callable_type || (is_arrow_or_expr && param.type_annotation.is_none()) {
                    continue;
                }
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
                if crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    resolved,
                ) {
                    continue;
                }

                // Use is_array_like_type first — it properly resolves type parameter
                // constraints (e.g., `T extends any[]` is recognized as array-like).
                // Fall back to assignability for custom array subclasses (e.g.,
                // `CoolArray<T> extends Array<T>` which is structurally array-like
                // but not recognized by classify_array_like as a raw Array/Tuple).
                let array_check_type = resolved;
                if !self.is_array_like_type(declared_type)
                    && !self.is_array_like_type(array_check_type)
                {
                    let readonly_any_array =
                        parameter_query::readonly_any_array_type(self.ctx.types);

                    if !self
                        .rest_parameter_relation_outcome(declared_type, readonly_any_array)
                        .related
                        && !self
                            .rest_parameter_relation_outcome(array_check_type, readonly_any_array)
                            .related
                    {
                        // tsc anchors TS2370 at the parameter (including the
                        // `...` token) rather than at its type annotation or
                        // name.  Use the param node's start position with the
                        // span up to the end of the name for parity.
                        if let Some(pn) = self.ctx.arena.get(param_idx)
                            && let Some(name_node) = self.ctx.arena.get(param.name)
                        {
                            let length = name_node.end.saturating_sub(pn.pos);
                            self.error_at_position(
                                pn.pos,
                                length,
                                "A rest parameter must be of an array type.",
                                diagnostic_codes::A_REST_PARAMETER_MUST_BE_OF_AN_ARRAY_TYPE,
                            );
                        }
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
                    let readonly_any_array =
                        parameter_query::readonly_any_array_type(self.ctx.types);
                    if !self
                        .rest_parameter_relation_outcome(init_type, readonly_any_array)
                        .related
                    {
                        // Anchor at the parameter start (the `...` token) like
                        // the annotated branches; error_at_node would
                        // normalize to the name node.
                        if let Some(pn) = self.ctx.arena.get(param_idx)
                            && let Some(name_node) = self.ctx.arena.get(param.name)
                        {
                            let length = name_node.end.saturating_sub(pn.pos);
                            self.error_at_position(
                                pn.pos,
                                length,
                                "A rest parameter must be of an array type.",
                                diagnostic_codes::A_REST_PARAMETER_MUST_BE_OF_AN_ARRAY_TYPE,
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Report the `this`-parameter placement and container errors for one
/// signature's parameter list, given only the checker context and the
/// container's `SyntaxKind`.
///
/// A `this` parameter is legal only as the *first* parameter of a signature
/// whose container can have one at all. `tsc` decides this in `checkParameter`
/// from two structural facts and nothing else — the parameter's index in its
/// own list, and the `SyntaxKind` of the container — and reports every arm
/// that applies rather than stopping at the first:
///
/// - not at index 0 -> `TS2680`
/// - container constructs (`Constructor` / `ConstructSignature` /
///   `ConstructorType`) -> `TS2681`
/// - container is an accessor (`GetAccessor` / `SetAccessor`) -> `TS2784`
/// - container is an `ArrowFunction` -> `TS2730`, whose `this` is lexical
///
/// The arms are independent: `class C { constructor(x: number, this: C) {} }`
/// draws `TS2680` *and* `TS2681`, so this cannot be a match over the container
/// kind. `container_kind` is `None` only when the caller has no owning node, in
/// which case the position arm still applies and the container arms cannot.
///
/// Lives at context level rather than on `CheckerState` because the
/// `FunctionType` / `ConstructorType` callers run inside `TypeNodeChecker`,
/// which has the context but not the checker state.
pub(crate) fn check_this_parameter_placement_in_ctx(
    ctx: &mut crate::CheckerContext,
    parameters: &tsz_parser::parser::NodeList,
    container_kind: Option<u16>,
) {
    use crate::diagnostics::{diagnostic_codes, format_message};
    use tsz_common::diagnostics::get_message_template;
    use tsz_parser::parser::syntax_kind_ext;
    use tsz_scanner::SyntaxKind;

    let report = |ctx: &mut crate::CheckerContext, param_idx: NodeIndex, code: u32| {
        let Some(node) = ctx.arena.get(param_idx) else {
            return;
        };
        let (pos, len) = (node.pos, node.end.saturating_sub(node.pos));
        let template = get_message_template(code).unwrap_or_default();
        // `{0}` on TS2680 is the parameter name, always `this` here; tsc
        // renders it from the name rather than hard-coding the word.
        let message = format_message(template, &["this"]);
        ctx.error(pos, len, message, code);
    };

    for (index, &param_idx) in parameters.nodes.iter().enumerate() {
        let this_param_data = ctx
            .arena
            .get(param_idx)
            .and_then(|param_node| ctx.arena.get_parameter(param_node));
        let is_this_param = this_param_data
            .and_then(|param| ctx.arena.get(param.name))
            .is_some_and(|name_node| {
                name_node.kind == SyntaxKind::ThisKeyword as u16
                    || ctx
                        .arena
                        .get_identifier(name_node)
                        .is_some_and(|ident| ident.escaped_text == "this")
            });
        if !is_this_param {
            continue;
        }

        // TS1433 ("Neither decorators nor modifiers may be applied to `this`
        // parameters") is a parser-level grammar error reported whenever a
        // `this` parameter carries decorators or modifiers
        // (`parse_parameter` in `state_statements_class.rs`). tsc does not
        // additionally run the semantic placement/container checks below on
        // that same parameter once the grammar error fires — verified
        // against the pinned oracle across all three container arms
        // (position, constructor, accessor): a decorated `this` parameter
        // reports TS1433 alone, never TS1433 plus TS2680/2681/2784.
        if this_param_data.is_some_and(|param| param.modifiers.is_some()) {
            continue;
        }

        if index != 0 {
            report(
                ctx,
                param_idx,
                diagnostic_codes::A_PARAMETER_MUST_BE_THE_FIRST_PARAMETER,
            );
        }

        let Some(kind) = container_kind else {
            continue;
        };

        // All three of these container kinds construct, so none of them has a
        // meaningful `this` to annotate.
        if kind == syntax_kind_ext::CONSTRUCTOR
            || kind == syntax_kind_ext::CONSTRUCT_SIGNATURE
            || kind == syntax_kind_ext::CONSTRUCTOR_TYPE
        {
            report(
                ctx,
                param_idx,
                diagnostic_codes::A_CONSTRUCTOR_CANNOT_HAVE_A_THIS_PARAMETER,
            );
        }

        if kind == syntax_kind_ext::GET_ACCESSOR || kind == syntax_kind_ext::SET_ACCESSOR {
            report(
                ctx,
                param_idx,
                diagnostic_codes::GET_AND_SET_ACCESSORS_CANNOT_DECLARE_THIS_PARAMETERS,
            );
        }

        // The JS/JSDoc `@this`-tag arm for arrow functions lives in
        // `function_type.rs` and triggers on a tag with no parameter node, so
        // the two cannot both fire for one arrow function.
        if kind == syntax_kind_ext::ARROW_FUNCTION {
            report(
                ctx,
                param_idx,
                diagnostic_codes::AN_ARROW_FUNCTION_CANNOT_HAVE_A_THIS_PARAMETER,
            );
        }
    }
}
