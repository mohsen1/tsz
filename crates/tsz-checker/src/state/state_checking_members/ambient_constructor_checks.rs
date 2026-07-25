//! Constructor declaration checks, split out of `ambient_signature_checks.rs`
//! to keep that file under the 2000-line architecture limit.

use crate::context::TypingRequest;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Check a constructor declaration.
    #[allow(dead_code)]
    pub(crate) fn check_constructor_declaration(&mut self, member_idx: NodeIndex) {
        self.check_constructor_declaration_with_request(member_idx, &TypingRequest::NONE);
    }

    pub(crate) fn check_constructor_declaration_with_request(
        &mut self,
        member_idx: NodeIndex,
        request: &TypingRequest,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let Some(ctor) = self.ctx.arena.get_constructor(node) else {
            return;
        };

        // Error 1089: 'async' modifier cannot appear on a constructor declaration.
        if let Some(async_mod_idx) = self.find_async_modifier(&ctor.modifiers) {
            self.error_at_node_msg(
                async_mod_idx,
                diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION,
                &["async"],
            );
        }

        // Error 1089: 'override' modifier cannot appear on a constructor declaration.
        if let Some(override_mod_idx) = self.find_override_modifier(&ctor.modifiers) {
            self.error_at_node_msg(
                override_mod_idx,
                diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION,
                &["override"],
            );
        }

        // Error 1242: 'abstract' modifier can only appear on a class, method, or property declaration.
        // Constructors cannot be abstract. TSC anchors the error at the 'abstract' keyword.
        if let Some(abstract_mod) = self
            .ctx
            .arena
            .find_modifier(&ctor.modifiers, tsz_scanner::SyntaxKind::AbstractKeyword)
        {
            self.error_at_node(
                abstract_mod,
                "'abstract' modifier can only appear on a class, method, or property declaration.",
                diagnostic_codes::ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION,
            );
        }

        // Error 1183: An implementation cannot be declared in ambient contexts
        // Check if we're in a declared class and the constructor has a body.
        // TSC anchors the error at the body node (the `{`).
        if ctor.body.is_some()
            && let Some(ref class_info) = self.ctx.enclosing_class
            && class_info.is_declared
        {
            self.error_at_node(
                ctor.body,
                "An implementation cannot be declared in ambient contexts.",
                diagnostic_codes::AN_IMPLEMENTATION_CANNOT_BE_DECLARED_IN_AMBIENT_CONTEXTS,
            );
        }

        // TS2394: Check overload compatibility for constructors with a body.
        if ctor.body.is_some() {
            self.check_overload_compatibility(member_idx);
        }

        // Check for parameter properties in constructor overload signatures (error 2369)
        // Parameter properties are only allowed in constructor implementations (with body).
        // This applies to both regular constructors and ambient (declare class) constructors.
        if ctor.body.is_none() {
            self.check_parameter_properties(&ctor.parameters.nodes);
        }
        // TS1294: erasableSyntaxOnly — parameter properties are not erasable.
        if self.ctx.compiler_options.erasable_syntax_only {
            for &param_idx in &ctor.parameters.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                if let Some(modifier_idx) =
                    self.find_first_parameter_property_modifier(&param.modifiers)
                    && let Some(mod_node) = self.ctx.arena.get(modifier_idx)
                {
                    self.ctx.error(
                            mod_node.pos,
                            mod_node.end - mod_node.pos,
                            diagnostic_messages::THIS_SYNTAX_IS_NOT_ALLOWED_WHEN_ERASABLESYNTAXONLY_IS_ENABLED
                                .to_string(),
                            diagnostic_codes::THIS_SYNTAX_IS_NOT_ALLOWED_WHEN_ERASABLESYNTAXONLY_IS_ENABLED,
                        );
                }
            }
        }

        // TS1187: Parameter properties cannot use binding patterns in constructors.
        // TS1317: A parameter property cannot be declared using a rest parameter.
        for &param_idx in &ctor.parameters.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            if !self.has_parameter_property_modifier(&param.modifiers) {
                continue;
            }
            // TS1317: rest parameter with property modifier
            if param.dot_dot_dot_token {
                let error_node = self
                    .find_first_parameter_property_modifier(&param.modifiers)
                    .unwrap_or(param_idx);
                self.error_at_node(
                    error_node,
                    diagnostic_messages::A_PARAMETER_PROPERTY_CANNOT_BE_DECLARED_USING_A_REST_PARAMETER,
                    diagnostic_codes::A_PARAMETER_PROPERTY_CANNOT_BE_DECLARED_USING_A_REST_PARAMETER,
                );
            }
            let name_idx = param.name;
            {
                if let Some(name_node) = self.ctx.arena.get(name_idx)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    && ident.escaped_text == "constructor"
                {
                    self.error_at_node(
                                name_idx,
                                diagnostic_messages::CONSTRUCTOR_CANNOT_BE_USED_AS_A_PARAMETER_PROPERTY_NAME,
                                diagnostic_codes::CONSTRUCTOR_CANNOT_BE_USED_AS_A_PARAMETER_PROPERTY_NAME,
                            );
                }
            }

            let Some(name_node) = self.ctx.arena.get(param.name) else {
                continue;
            };
            if name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            {
                // Report at the accessibility modifier (public/private/protected/readonly)
                // to match tsc's diagnostic location, not at the binding pattern.
                let error_node = param
                    .modifiers
                    .as_ref()
                    .and_then(|mods| mods.nodes.first().copied())
                    .unwrap_or(param_idx);
                self.error_at_node(
                    error_node,
                    diagnostic_messages::A_PARAMETER_PROPERTY_MAY_NOT_BE_DECLARED_USING_A_BINDING_PATTERN,
                    diagnostic_codes::A_PARAMETER_PROPERTY_MAY_NOT_BE_DECLARED_USING_A_BINDING_PATTERN,
                );
            }
        }

        // Check parameter type annotations for parameter properties in function types
        // TSC suppresses TS7006 for private constructors in ambient (declare) classes
        let skip_implicit_any_ctor = self
            .ctx
            .enclosing_class
            .as_ref()
            .is_some_and(|c| c.is_declared)
            && self.has_private_modifier(&ctor.modifiers);
        // Get constructor-level JSDoc for @param type checking
        let ctor_jsdoc = self.get_jsdoc_for_function(member_idx);
        for (pi, &param_idx) in ctor.parameters.nodes.iter().enumerate() {
            if let Some(param_node) = self.ctx.arena.get(param_idx)
                && let Some(param) = self.ctx.arena.get_parameter(param_node)
            {
                if param.type_annotation.is_some() {
                    self.check_type_for_parameter_properties(param.type_annotation);
                }
                if !skip_implicit_any_ctor {
                    let has_jsdoc = self.param_has_inline_jsdoc_type(param_idx)
                        || if let Some(ref jsdoc) = ctor_jsdoc {
                            let pname = self.parameter_name_for_error(param.name);
                            Self::jsdoc_has_param_type(jsdoc, &pname)
                        } else {
                            false
                        };
                    self.maybe_report_implicit_any_parameter(param, has_jsdoc, pi);
                }
            }
        }

        // Constructors don't have explicit return types, but they implicitly return the class instance type
        // Get the class instance type to validate constructor return expressions (TS2322)

        self.cache_parameter_types(&ctor.parameters.nodes, None);

        // Check for duplicate parameter names (TS2300)
        self.check_duplicate_parameters(&ctor.parameters, ctor.body.is_some());

        // TS1210/TS1213: Check constructor parameter names in class strict mode.
        // Classes are implicitly strict mode.
        if self
            .ctx
            .enclosing_class
            .as_ref()
            .is_none_or(|c| !c.is_declared)
        {
            self.check_strict_mode_reserved_parameter_names(
                &ctor.parameters.nodes,
                member_idx,
                self.ctx.enclosing_class.is_some(),
            );
        }
        for &param_idx in &ctor.parameters.nodes {
            if let Some(param_node) = self.ctx.arena.get(param_idx)
                && let Some(param) = self.ctx.arena.get_parameter(param_node)
                && let Some(name_node) = self.ctx.arena.get(param.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                && ident.escaped_text == "static"
                && ident.original_text.is_none()
            {
                self.ctx.error(
                            param_node.pos,
                            param_node.end - param_node.pos,
                            diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO
                                .replace("{0}", "static"),
                            diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO,
                        );
            }
        }

        // Check for required parameters following optional parameters (TS1016)
        self.check_parameter_ordering(&ctor.parameters, Some(member_idx));
        self.check_binding_pattern_optionality(
            &ctor.parameters.nodes,
            ctor.body.is_some(),
            Some(member_idx),
        );

        // Check that rest parameters have array types (TS2370)
        self.check_rest_parameter_types(&ctor.parameters.nodes);

        // Check that parameter default values are assignable to declared types (TS2322)
        self.check_parameter_initializers(&ctor.parameters.nodes);
        self.check_non_impl_parameter_initializers(
            &ctor.parameters.nodes,
            self.has_declare_modifier(&ctor.modifiers),
            ctor.body.is_some(),
        );

        // Check binding-element property/index lookups in destructuring parameters
        // (e.g., `constructor([{ x1, x2 }, y]: [ObjType1, number])` emits TS2339 for
        // properties not on `ObjType1`). Mirrors the call in `check_function_decl`.
        // This must run for constructors too — otherwise destructuring parameter
        // patterns silently skip nested property-existence checks.
        self.check_parameter_binding_pattern_defaults(&ctor.parameters.nodes);

        // Set in_constructor flag for abstract property checks (error 2715)
        if let Some(ref mut class_info) = self.ctx.enclosing_class {
            class_info.in_constructor = true;
            class_info.has_super_call_in_current_constructor = false;
        }

        // Check constructor body
        if ctor.body.is_some() {
            // Get class instance type for constructor return expression validation
            let instance_type = if let Some(ref class_info) = self.ctx.enclosing_class {
                let class_node = self.ctx.arena.get(class_info.class_idx);
                if let Some(class) = class_node.and_then(|n| self.ctx.arena.get_class(n)) {
                    self.get_class_instance_type(class_info.class_idx, class)
                } else {
                    TypeId::ANY
                }
            } else {
                TypeId::ANY
            };

            // Set expected return type to class instance type
            self.push_return_type(instance_type);
            let body_request = request.read().contextual_opt(None);
            self.clear_type_cache_recursive(ctor.body);
            // Re-cache parameter types after clearing: clear_type_cache_recursive
            // removes symbol_types for all VARIABLE_DECLARATION nodes in the body.
            // When a `var` re-declares a constructor parameter (sharing the same
            // SymbolId), the parameter type cached earlier gets erased. Re-caching
            // ensures the parameter type is available for initializer evaluation.
            self.cache_parameter_types(&ctor.parameters.nodes, None);
            self.check_statement_with_request(ctor.body, &body_request);
            self.pop_return_type();

            // TS2377: Constructors for derived classes must contain a super() call.
            let requires_super = self
                .ctx
                .enclosing_class
                .as_ref()
                .and_then(|info| self.ctx.arena.get(info.class_idx))
                .and_then(|class_node| self.ctx.arena.get_class(class_node))
                .is_some_and(|class| self.class_requires_super_call(class));
            let has_super_call = self
                .ctx
                .enclosing_class
                .as_ref()
                .is_some_and(|info| info.has_super_call_in_current_constructor);

            if requires_super && !has_super_call {
                self.error_at_node(
                    member_idx,
                    diagnostic_messages::CONSTRUCTORS_FOR_DERIVED_CLASSES_MUST_CONTAIN_A_SUPER_CALL,
                    diagnostic_codes::CONSTRUCTORS_FOR_DERIVED_CLASSES_MUST_CONTAIN_A_SUPER_CALL,
                );
            }
        }

        // Reset in_constructor flag
        if let Some(ref mut class_info) = self.ctx.enclosing_class {
            class_info.in_constructor = false;
        }

        // Check overload compatibility for constructor implementations
        if ctor.body.is_some() {
            self.check_overload_modifier_consistency(member_idx);
            self.check_overload_compatibility(member_idx);
        }

        // TS1092: @template on constructors is illegal in JS files
        // TS1093: @return/@returns type annotation on constructors is illegal in JS files
        self.check_jsdoc_constructor_tags(member_idx);
    }

    /// Check JSDoc `@template` and `@return`/`@returns` tags on constructor
    /// declarations in JS files (TS1092, TS1093).
    ///
    /// tsc reports:
    /// - TS1092 "Type parameters cannot appear on a constructor declaration."
    ///   at the position of the first type parameter name in `@template T`
    /// - TS1093 "Type annotation cannot appear on a constructor declaration."
    ///   at the position of the `{` in `@return {Type}`
    fn check_jsdoc_constructor_tags(&mut self, member_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        // Find the JSDoc comment for this constructor.
        // We need the raw comment text from the source (not the processed JSDoc content)
        // so we can compute accurate source positions.
        let Some((_jsdoc_content, comment_pos)) =
            self.try_leading_jsdoc_with_pos(comments, node.pos, source_text)
        else {
            return;
        };

        // Get the raw comment text from the source to compute positions accurately.
        let comment_end = node.pos as usize;
        let raw_comment = &source_text[comment_pos as usize..comment_end.min(source_text.len())];

        // TS1092: Check for @template tag on constructor
        if let Some(template_offset) = Self::jsdoc_tag_offset(raw_comment, "template") {
            // tsc anchors at the `@` of the `@template` tag itself, not at what
            // follows it. Skipping to the text after the tag also mis-landed on
            // `{` for a typed form like `@template {string} T`, which is wrong
            // under either rule.
            let abs_pos = comment_pos + template_offset as u32;
            self.ctx.error(
                abs_pos,
                0,
                diagnostic_messages::TYPE_PARAMETERS_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION
                    .to_string(),
                diagnostic_codes::TYPE_PARAMETERS_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION,
            );
        }

        // A preceding @callback creates a nested function type whose return tag
        // belongs to that callback. A plain @typedef does not consume later
        // constructor-level return tags.
        let callback_scope_start = Self::jsdoc_tag_offset(raw_comment, "callback");

        for tag in ["returns", "return"] {
            if let Some(tag_offset) = Self::jsdoc_tag_offset(raw_comment, tag) {
                if callback_scope_start.is_some_and(|scope_start| tag_offset > scope_start) {
                    continue;
                }
                let tag_len = tag.len() + 1;
                let rest = &raw_comment[tag_offset + tag_len..];
                let trimmed = rest.trim_start();
                if trimmed.starts_with('{') {
                    let ws_len = rest.len() - trimmed.len();
                    let error_offset = tag_offset + tag_len + ws_len + 1;
                    let abs_pos = comment_pos + error_offset as u32;
                    self.ctx.error(
                        abs_pos,
                        0,
                        diagnostic_messages::TYPE_ANNOTATION_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION
                            .to_string(),
                        diagnostic_codes::TYPE_ANNOTATION_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION,
                    );
                    break; // Only report once
                }
            }
        }
    }
}
