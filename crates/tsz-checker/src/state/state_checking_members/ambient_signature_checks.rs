//! Ambient and class member declaration checks (property, method, constructor, accessor).
//!
//! For overload compatibility, signature utilities, and implicit-any return checks,
//! see [`super::overload_compatibility`].

use crate::context::TypingRequest;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{ContextualTypeContext, TypeId};

impl<'a> CheckerState<'a> {
    fn contextual_class_member_type_from_request(
        &mut self,
        request: &TypingRequest,
        member_name: NodeIndex,
    ) -> Option<TypeId> {
        let ctx_type = request.contextual_type?;
        let prop_name = self.get_property_name(member_name)?;
        let resolved_ctx = self.evaluate_type_for_assignability(ctx_type);
        let ctx_helper = ContextualTypeContext::with_expected(self.ctx.types, resolved_ctx);
        ctx_helper
            .get_property_type(&prop_name)
            .filter(|&ty| ty != TypeId::ANY && !self.type_contains_error(ty))
    }

    #[allow(dead_code)]
    pub(crate) fn check_property_declaration(&mut self, member_idx: NodeIndex) {
        self.check_property_declaration_with_request(member_idx, &TypingRequest::NONE);
    }

    pub(crate) fn check_property_declaration_with_request(
        &mut self,
        member_idx: NodeIndex,
        request: &TypingRequest,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let Some(prop) = self.ctx.arena.get_property_decl(node) else {
            return;
        };

        // TS1166: Computed property name in class property declaration must have
        // a simple literal type or a 'unique symbol' type.
        // This check only fires when the expression is NOT an entity name expression
        // (i.e., not a simple identifier or property access chain like a.b.c).
        // Entity name expressions are always allowed regardless of their type.
        //
        // TSC suppresses TS1166 for decorated properties in class expressions when
        // experimentalDecorators is enabled (those get TS1206 instead).
        let suppress_ts1166 = self.ctx.compiler_options.experimental_decorators
            && self.ctx.enclosing_class.as_ref().is_some_and(|c| {
                self.ctx
                    .arena
                    .get(c.class_idx)
                    .is_some_and(|n| n.kind == syntax_kind_ext::CLASS_EXPRESSION)
            })
            && prop.modifiers.as_ref().is_some_and(|mods| {
                mods.nodes.iter().any(|&mod_idx| {
                    self.ctx
                        .arena
                        .get(mod_idx)
                        .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                })
            });
        if !suppress_ts1166 {
            self.check_class_computed_property_name(prop.name);
        }
        self.check_modifier_combinations(&prop.modifiers);

        // TS8009/TS8010: Check for TypeScript-only features in JavaScript files
        let is_js_file = self.is_js_file();
        tracing::debug!(is_js_file, file_name = %self.ctx.file_name, "Checking if JS file for TS8009/TS8010");

        if is_js_file {
            use crate::diagnostics::{diagnostic_messages, format_message};

            // TS8009: Modifiers like 'declare' can only be used in TypeScript files
            if self
                .ctx
                .arena
                .has_modifier(&prop.modifiers, tsz_scanner::SyntaxKind::DeclareKeyword)
            {
                let message = format_message(
                    diagnostic_messages::THE_MODIFIER_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    &["declare"],
                );
                if let Some(declare_idx) = self.get_modifier_index(
                    &prop.modifiers,
                    tsz_scanner::SyntaxKind::DeclareKeyword as u16,
                ) {
                    self.error_at_node(
                        declare_idx,
                        &message,
                        diagnostic_codes::THE_MODIFIER_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    );
                } else {
                    self.error_at_node(
                        member_idx,
                        &message,
                        diagnostic_codes::THE_MODIFIER_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    );
                }
            }

            // TS8010: Type annotations can only be used in TypeScript files
            if prop.type_annotation.is_some() {
                self.error_at_node(
                    prop.type_annotation,
                    diagnostic_messages::TYPE_ANNOTATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    diagnostic_codes::TYPE_ANNOTATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                );
            }
        }

        // TS2314: Check type annotation for generic types used without required type arguments.
        // Class/interface bodies are lowered by TypeLowering which doesn't validate TS2314,
        // so we explicitly walk the type annotation AST to catch missing type arguments.
        if prop.type_annotation.is_some() {
            self.check_nested_type_refs_for_ts2314(prop.type_annotation);
        }

        // Track static property initializer context for TS17011
        let is_static = self.has_static_modifier(&prop.modifiers);
        let prev_static_prop_init = self
            .ctx
            .enclosing_class
            .as_ref()
            .is_some_and(|c| c.in_static_property_initializer);
        if is_static
            && prop.initializer.is_some()
            && let Some(ref mut class_info) = self.ctx.enclosing_class
        {
            class_info.in_static_property_initializer = true;
        }

        // When useDefineForClassFields is true (target >= ES2022), property
        // initializers run in the class body scope, NOT the constructor scope.
        // Constructor parameters are not visible, so we skip TS2301 checks
        // and let normal name resolution handle it (producing TS2304 if needed).
        if !is_static
            && prop.initializer.is_some()
            && !self.ctx.compiler_options.target.supports_es2022()
            && let Some(member_name) = self.get_property_name(prop.name)
        {
            self.check_constructor_param_capture_in_instance_initializer(
                &member_name,
                prop.initializer,
            );
        }

        // TS18045: accessor modifier only allowed when targeting ES2015+
        // Ambient contexts (declare class) are exempt.
        if self.has_accessor_modifier(&prop.modifiers) {
            use crate::context::ScriptTarget;
            let is_es5_or_lower = matches!(
                self.ctx.compiler_options.target,
                ScriptTarget::ES3 | ScriptTarget::ES5
            );
            let in_ambient = self
                .ctx
                .enclosing_class
                .as_ref()
                .is_some_and(|c| c.is_declared);
            if is_es5_or_lower && !in_ambient {
                self.error_at_node(
                    member_idx,
                    "Properties with the 'accessor' modifier are only available when targeting ECMAScript 2015 and higher.",
                    diagnostic_codes::PROPERTIES_WITH_THE_ACCESSOR_MODIFIER_ARE_ONLY_AVAILABLE_WHEN_TARGETING_ECMASCRI,
                );
            }
        }

        // Error 1248: A class member cannot have the 'const' keyword
        if let Some(_const_mod) = self.get_const_modifier(&prop.modifiers) {
            self.error_at_node(
                prop.name,
                "A class member cannot have the 'const' keyword.",
                diagnostic_codes::A_CLASS_MEMBER_CANNOT_HAVE_THE_KEYWORD,
            );
        }

        // TS1255/TS1263/TS1264: Definite assignment assertion checks on class properties
        if prop.exclamation_token {
            let in_ambient = self
                .ctx
                .enclosing_class
                .as_ref()
                .is_some_and(|c| c.is_declared);
            let is_static = self.has_static_modifier(&prop.modifiers);
            let is_abstract = self.has_abstract_modifier(&prop.modifiers);
            let has_declare = self.has_declare_modifier(&prop.modifiers);

            // tsc points TS1255/TS1263/TS1264 at the `!` token itself.
            // For class property names parsed via parse_property_name(), the name
            // node's `end` is one past the `!` (due to end_pos being captured after
            // next_token()). So the `!` is at name_node.end - 1.
            let excl_pos = self
                .ctx
                .arena
                .get(prop.name)
                .map(|n| n.end.saturating_sub(1));

            // TS1255: ! is not permitted on static, abstract, ambient, or declared properties
            if in_ambient || is_static || is_abstract || has_declare {
                if let Some(pos) = excl_pos {
                    self.emit_error_at(
                        pos,
                        1,
                        diagnostic_messages::A_DEFINITE_ASSIGNMENT_ASSERTION_IS_NOT_PERMITTED_IN_THIS_CONTEXT,
                        diagnostic_codes::A_DEFINITE_ASSIGNMENT_ASSERTION_IS_NOT_PERMITTED_IN_THIS_CONTEXT,
                    );
                } else {
                    self.error_at_node(
                        prop.name,
                        diagnostic_messages::A_DEFINITE_ASSIGNMENT_ASSERTION_IS_NOT_PERMITTED_IN_THIS_CONTEXT,
                        diagnostic_codes::A_DEFINITE_ASSIGNMENT_ASSERTION_IS_NOT_PERMITTED_IN_THIS_CONTEXT,
                    );
                }
            }

            // TS1263: ! with initializer is contradictory
            if prop.initializer.is_some() {
                if let Some(pos) = excl_pos {
                    self.emit_error_at(
                        pos,
                        1,
                        diagnostic_messages::DECLARATIONS_WITH_INITIALIZERS_CANNOT_ALSO_HAVE_DEFINITE_ASSIGNMENT_ASSERTIONS,
                        diagnostic_codes::DECLARATIONS_WITH_INITIALIZERS_CANNOT_ALSO_HAVE_DEFINITE_ASSIGNMENT_ASSERTIONS,
                    );
                } else {
                    self.error_at_node(
                        prop.name,
                        diagnostic_messages::DECLARATIONS_WITH_INITIALIZERS_CANNOT_ALSO_HAVE_DEFINITE_ASSIGNMENT_ASSERTIONS,
                        diagnostic_codes::DECLARATIONS_WITH_INITIALIZERS_CANNOT_ALSO_HAVE_DEFINITE_ASSIGNMENT_ASSERTIONS,
                    );
                }
            }

            // TS1264: ! without type annotation is meaningless
            if prop.type_annotation.is_none() {
                if let Some(pos) = excl_pos {
                    self.emit_error_at(
                        pos,
                        1,
                        diagnostic_messages::DECLARATIONS_WITH_DEFINITE_ASSIGNMENT_ASSERTIONS_MUST_ALSO_HAVE_TYPE_ANNOTATIONS,
                        diagnostic_codes::DECLARATIONS_WITH_DEFINITE_ASSIGNMENT_ASSERTIONS_MUST_ALSO_HAVE_TYPE_ANNOTATIONS,
                    );
                } else {
                    self.error_at_node(
                        prop.name,
                        diagnostic_messages::DECLARATIONS_WITH_DEFINITE_ASSIGNMENT_ASSERTIONS_MUST_ALSO_HAVE_TYPE_ANNOTATIONS,
                        diagnostic_codes::DECLARATIONS_WITH_DEFINITE_ASSIGNMENT_ASSERTIONS_MUST_ALSO_HAVE_TYPE_ANNOTATIONS,
                    );
                }
            }
        }

        // TS1039: Initializers are not allowed in ambient contexts.
        // A class property with `declare` modifier or in a `declare class` is ambient.
        if prop.initializer.is_some() && !self.ctx.compiler_options.no_types_and_symbols {
            let has_declare = self.has_declare_modifier(&prop.modifiers);
            let in_declared_class = self
                .ctx
                .enclosing_class
                .as_ref()
                .is_some_and(|c| c.is_declared);
            if has_declare || in_declared_class {
                // tsc short-circuits: when a `declare` property has an ES decorator,
                // checkGrammarModifiers fires TS1206 first and skips checkGrammarProperty
                // (which would emit TS1039). Mirror this by suppressing TS1039 when
                // ES decorators are present on a `declare` property.
                let has_es_decorator_on_declare = has_declare
                    && !self.ctx.compiler_options.experimental_decorators
                    && prop.modifiers.as_ref().is_some_and(|m| {
                        m.nodes.iter().any(|&n| {
                            self.ctx
                                .arena
                                .get(n)
                                .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                        })
                    });
                if !has_es_decorator_on_declare {
                    self.error_at_node(
                        prop.initializer,
                        diagnostic_messages::INITIALIZERS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS,
                        diagnostic_codes::INITIALIZERS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS,
                    );
                }
            }
        }

        // Check for await expressions in the initializer (TS1308)
        if prop.initializer.is_some() {
            self.check_await_expression(prop.initializer);
        }

        let effective_declared_type = self.effective_class_property_declared_type(member_idx, prop);
        let contextual_member_type =
            self.contextual_class_member_type_from_request(request, prop.name);
        let mut inferred_initializer_type = None;

        // If property has a semantic declared type and initializer, check type compatibility.
        if prop.initializer.is_some()
            && let Some(declared_type) = effective_declared_type
        {
            // Check for undefined type names in nested types (e.g., function type parameters).
            // This matches the variable declaration path in check_variable_declaration.
            if !self.is_js_file() && prop.type_annotation.is_some() {
                self.check_type_for_missing_names_skip_top_level_ref(prop.type_annotation);
            }
            let request =
                if declared_type != TypeId::ANY && !self.type_contains_error(declared_type) {
                    // Clear cached type to force recomputation with contextual type.
                    // Function expressions may have been typed without contextual info
                    // during build_type_environment, missing parameter type inference.
                    self.invalidate_initializer_for_context_change(prop.initializer);
                    request.read().contextual(declared_type)
                } else {
                    request.read().contextual_opt(None)
                };
            let init_type = self.get_type_of_node_with_request(prop.initializer, &request);

            if declared_type != TypeId::ANY
                && !self.type_contains_error(declared_type)
                // Use prop.initializer as source_idx for excess-property resolution,
                // and prop.name as diag_idx for TS2322 diagnostic anchoring (tsc
                // points at the property name, not the initializer value).
                && self.check_assignable_or_report_at(
                    init_type,
                    declared_type,
                    prop.initializer,
                    prop.name,
                )
            {
                self.check_object_literal_excess_properties(
                    init_type,
                    declared_type,
                    prop.initializer,
                );
            }
        } else if prop.initializer.is_some() {
            // When a class property has an initializer but no semantic declared type,
            // and the class has a contextual type (e.g., from a function return type),
            // look up the property's expected type from the contextual type and use it
            // as contextual type for the initializer. This enables arrow/function
            // expression initializers to get parameter types from the context.
            //
            // Build-type-environment may have already cached this initializer before
            // class-member `this` context is available, especially for arrow initializers
            // that reference `this`. This path still depends on member-context state
            // that is not fully request-audited yet, so keep the explicit recursive
            // clear here until class-property initializer caching is fully migrated.
            self.clear_type_cache_recursive(prop.initializer);
            let request = if let Some(member_type) = contextual_member_type {
                request.read().contextual(member_type)
            } else {
                request.read().contextual_opt(None)
            };
            let initializer_snap = self.ctx.snapshot_diagnostics();
            let init_type = self.get_type_of_node_with_request(prop.initializer, &request);
            inferred_initializer_type = Some(init_type);

            if self.ctx.no_implicit_any()
                && contextual_member_type.is_none()
                && prop.type_annotation.is_none()
                && self.class_property_initializer_has_non_deferred_circularity(member_idx)
                && let Some(member_name) = self.get_member_name_display_text(prop.name)
            {
                self.suppress_circular_initializer_relation_diagnostics(
                    &initializer_snap,
                    prop.initializer,
                );
                self.error_at_node_msg(
                    prop.name,
                    diagnostic_codes::IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION_AND_IS_REFERE,
                    &[&member_name],
                );
                inferred_initializer_type = Some(TypeId::ANY);
            }
        }

        // Error 2729: Property is used before its initialization
        // Check if initializer references properties declared after this one
        if prop.initializer.is_some() && !self.has_static_modifier(&prop.modifiers) {
            self.check_property_initialization_order(member_idx, prop.initializer);
        }

        // Error 2729: Static property used before its initialization
        // Check if initializer references static properties declared after this one
        if prop.initializer.is_some() && self.has_static_modifier(&prop.modifiers) {
            self.check_static_property_initialization_order(member_idx, prop.initializer);
        }

        // TS7008: Member implicitly has an 'any' type
        // Report this error when noImplicitAny is enabled and the property has no type annotation
        // AND no initializer (if there's an initializer, TypeScript can infer the type)
        // TSC suppresses this for private members in ambient (declare) classes
        let is_private_in_ambient = self
            .ctx
            .enclosing_class
            .as_ref()
            .is_some_and(|c| c.is_declared)
            && (self.has_private_modifier(&prop.modifiers)
                || self.is_private_identifier_name(prop.name));
        let is_static = self.has_static_modifier(&prop.modifiers);
        // tsc suppresses TS7008 for `static prototype` since TS2699 already fires
        let is_static_prototype = is_static
            && self
                .get_member_name_display_text(prop.name)
                .is_some_and(|n| n == "prototype");
        if self.ctx.no_implicit_any()
            && effective_declared_type.is_none()
            && prop.initializer.is_none()
            && prop.type_annotation.is_none()
            && !is_private_in_ambient
            && !is_static_prototype
            // Constructor assignments only apply to instance properties, not static
            && (is_static || !self.property_assigned_in_enclosing_class_constructor(prop.name))
            // TSC also suppresses TS7008 for static properties assigned in class
            // static blocks (e.g., `static { this.x = 1; }`)
            && !(is_static
                && self.property_assigned_in_enclosing_class_static_block(prop.name))
            && let Some(member_name) = self.get_member_name_display_text(prop.name)
        {
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node_msg(
                prop.name,
                diagnostic_codes::MEMBER_IMPLICITLY_HAS_AN_TYPE,
                &[&member_name, "any"],
            );
        }

        // Cache the inferred type for the property node so DeclarationEmitter can use it
        // Get type: either from annotation or inferred from initializer
        let prop_type = if let Some(declared_type) = effective_declared_type {
            declared_type
        } else if let Some(member_type) = contextual_member_type {
            member_type
        } else if let Some(init_type) = inferred_initializer_type {
            init_type
        } else if prop.initializer.is_some() {
            let request = request.read().contextual_opt(None);
            let init_type = self.get_type_of_node_with_request(prop.initializer, &request);
            let init_type =
                if init_type == TypeId::ANY && self.has_accessor_modifier(&prop.modifiers) {
                    self.this_access_name_node(prop.initializer)
                        .and_then(|name_idx| {
                            self.infer_property_type_from_enclosing_class_assignments(
                                name_idx, is_static,
                            )
                        })
                        .unwrap_or(init_type)
                } else {
                    init_type
                };
            // Widen literal types for mutable class properties (tsc behavior).
            // `class Foo { name = "" }` infers `name: string`, not `name: ""`.
            // Readonly properties preserve literal types:
            // `class Foo { readonly tag = "x" }` infers `tag: "x"`.
            let is_readonly = self
                .ctx
                .arena
                .has_modifier(&prop.modifiers, tsz_scanner::SyntaxKind::ReadonlyKeyword);
            if is_readonly {
                init_type
            } else {
                self.widen_literal_type(init_type)
            }
        } else if self.has_accessor_modifier(&prop.modifiers) {
            self.infer_property_type_from_enclosing_class_assignments(prop.name, is_static)
                .unwrap_or(TypeId::ANY)
        } else {
            TypeId::ANY
        };

        self.ctx.node_types.insert(member_idx.0, prop_type);

        if is_static {
            self.check_static_member_for_class_type_param_refs(member_idx);
        }

        // Restore static property initializer context
        if let Some(ref mut class_info) = self.ctx.enclosing_class {
            class_info.in_static_property_initializer = prev_static_prop_init;
        }
    }

    /// Check a method declaration.
    #[allow(dead_code)]
    pub(crate) fn check_method_declaration(&mut self, member_idx: NodeIndex) {
        self.check_method_declaration_with_request(member_idx, &TypingRequest::NONE);
    }

    pub(crate) fn check_method_declaration_with_request(
        &mut self,
        member_idx: NodeIndex,
        request: &TypingRequest,
    ) {
        use crate::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let Some(method) = self.ctx.arena.get_method_decl(node) else {
            return;
        };

        // Error 1248: A class member cannot have the 'const' keyword
        if let Some(_const_mod) = self.get_const_modifier(&method.modifiers) {
            self.error_at_node(
                method.name,
                "A class member cannot have the 'const' keyword.",
                diagnostic_codes::A_CLASS_MEMBER_CANNOT_HAVE_THE_KEYWORD,
            );
        }

        // Error 1183: An implementation cannot be declared in ambient contexts
        // Check if we're in a declared class and the method has a body,
        // OR if the method itself has a `declare` modifier and a body.
        // TSC anchors the error at the body node (the `{`), not the whole member.
        if method.body.is_some() {
            let in_declared_class = self
                .ctx
                .enclosing_class
                .as_ref()
                .is_some_and(|c| c.is_declared);
            let method_has_declare = self.has_declare_modifier(&method.modifiers);
            if in_declared_class || method_has_declare {
                self.error_at_node(
                    method.body,
                    "An implementation cannot be declared in ambient contexts.",
                    diagnostic_codes::AN_IMPLEMENTATION_CANNOT_BE_DECLARED_IN_AMBIENT_CONTEXTS,
                );
            }
        }

        // TS2394: Check overload compatibility for method declarations with a body.
        if method.body.is_some() {
            self.check_overload_compatibility(member_idx);
        }

        // Error 1245: Method '{0}' cannot have an implementation because it is marked abstract.
        // TSC anchors this error at the method name, not the whole member node.
        if method.body.is_some() && self.has_abstract_modifier(&method.modifiers) {
            let name_text = self
                .get_property_name(method.name)
                .unwrap_or_else(|| "unknown".to_string());
            self.error_at_node(
                method.name,
                &format!("Method '{name_text}' cannot have an implementation because it is marked abstract."),
                diagnostic_codes::METHOD_CANNOT_HAVE_AN_IMPLEMENTATION_BECAUSE_IT_IS_MARKED_ABSTRACT,
            );
        }

        // TS1221 / TS1222
        // TSC anchors these errors at the `*` asterisk token, not the whole method node.
        if method.asterisk_token {
            let in_declared_class = self
                .ctx
                .enclosing_class
                .as_ref()
                .is_some_and(|c| c.is_declared);
            let method_has_declare = self.has_declare_modifier(&method.modifiers);
            let is_ambient = in_declared_class
                || method_has_declare
                || self.ctx.is_declaration_file()
                || self.is_ambient_declaration(member_idx);

            if is_ambient {
                self.emit_generator_error_at_asterisk(
                    method.name,
                    member_idx,
                    "Generators are not allowed in an ambient context.",
                    diagnostic_codes::GENERATORS_ARE_NOT_ALLOWED_IN_AN_AMBIENT_CONTEXT,
                );
            } else if method.body.is_none() {
                self.emit_generator_error_at_asterisk(
                    method.name,
                    member_idx,
                    "An overload signature cannot be declared as a generator.",
                    diagnostic_codes::AN_OVERLOAD_SIGNATURE_CANNOT_BE_DECLARED_AS_A_GENERATOR,
                );
            }
        }

        // Push type parameters (like <U> in `fn<U>(id: U)`) before checking types
        let (_type_params, type_param_updates) = self.push_type_parameters(&method.type_parameters);

        self.check_modifier_combinations(&method.modifiers);

        // Check for unused type parameters (TS6133)
        self.check_unused_type_params(&method.type_parameters, member_idx);

        // Extract parameter types from contextual type (for object literal methods)
        // This enables shorthand method parameter type inference
        let mut param_types: Vec<Option<TypeId>> = Vec::new();
        let contextual_method_type =
            self.contextual_class_member_type_from_request(request, method.name);
        if let Some(ctx_type) = contextual_method_type {
            let ctx_helper = ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                ctx_type,
                self.ctx.compiler_options.no_implicit_any,
            );

            for (i, &param_idx) in method.parameters.nodes.iter().enumerate() {
                if let Some(param_node) = self.ctx.arena.get(param_idx)
                    && let Some(param) = self.ctx.arena.get_parameter(param_node)
                {
                    let type_id = if param.type_annotation.is_some() {
                        // Use explicit type annotation if present
                        Some(self.get_type_from_type_node(param.type_annotation))
                    } else {
                        // Infer from contextual type
                        ctx_helper.get_parameter_type(i)
                    };
                    param_types.push(type_id);
                }
            }
        }

        let has_type_annotation = method.type_annotation.is_some();
        let mut return_type = if has_type_annotation {
            self.get_type_from_type_node(method.type_annotation)
        } else if let Some(ctx_type) = contextual_method_type {
            let ctx_helper = ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                ctx_type,
                self.ctx.compiler_options.no_implicit_any,
            );
            ctx_helper.get_return_type().unwrap_or(TypeId::ANY)
        } else {
            TypeId::ANY
        };

        // Cache parameter types for use in method body
        // If we have contextual types, use them; otherwise fall back to type annotations or UNKNOWN
        if param_types.is_empty() {
            self.cache_parameter_types(&method.parameters.nodes, None);
        } else {
            self.cache_parameter_types(&method.parameters.nodes, Some(&param_types));
        }

        // Check for duplicate parameter names (TS2300)
        self.check_duplicate_parameters(&method.parameters, method.body.is_some());

        // TS1210: Check for reserved names in class method parameter lists (strict mode)
        if self
            .ctx
            .enclosing_class
            .as_ref()
            .is_none_or(|c| !c.is_declared)
        {
            self.check_strict_mode_reserved_parameter_names(
                &method.parameters.nodes,
                member_idx,
                self.ctx.enclosing_class.is_some(),
            );
        }

        // Check for required parameters following optional parameters (TS1016)
        self.check_parameter_ordering(&method.parameters, Some(member_idx));
        self.check_binding_pattern_optionality(&method.parameters.nodes, method.body.is_some());

        // Check that rest parameters have array types (TS2370)
        self.check_rest_parameter_types(&method.parameters.nodes);

        // Check that parameter default values are assignable to declared types (TS2322)
        self.check_parameter_initializers(&method.parameters.nodes);
        self.check_non_impl_parameter_initializers(
            &method.parameters.nodes,
            self.has_declare_modifier(&method.modifiers),
            method.body.is_some(),
        );

        // Check for parameter properties (error 2369)
        // Parameter properties are only allowed in constructors, not in methods
        self.check_parameter_properties(&method.parameters.nodes);

        // Check parameter type annotations for parameter properties in function types
        // TSC suppresses TS7006 for private members in ambient (declare) classes
        // since private members are excluded from .d.ts output.
        let skip_implicit_any = self
            .ctx
            .enclosing_class
            .as_ref()
            .is_some_and(|c| c.is_declared)
            && self.has_private_modifier(&method.modifiers);
        // Get method-level JSDoc for @param type checking
        let method_jsdoc = self.get_jsdoc_for_function(member_idx);
        // Pre-extract ordered @param names for positional matching with binding patterns
        let jsdoc_param_names: Vec<String> = method_jsdoc
            .as_ref()
            .map(|jsdoc| {
                Self::extract_jsdoc_param_names(jsdoc)
                    .into_iter()
                    .map(|(name, _)| name)
                    .collect()
            })
            .unwrap_or_default();
        for (pi, &param_idx) in method.parameters.nodes.iter().enumerate() {
            if let Some(param_node) = self.ctx.arena.get(param_idx)
                && let Some(param) = self.ctx.arena.get_parameter(param_node)
            {
                if param.type_annotation.is_some() {
                    self.check_type_for_parameter_properties(param.type_annotation);
                }
                if !skip_implicit_any {
                    let has_jsdoc = self.param_has_inline_jsdoc_type(param_idx)
                        || if let Some(ref jsdoc) = method_jsdoc {
                            let pname =
                                self.effective_jsdoc_param_name(param.name, &jsdoc_param_names, pi);
                            Self::jsdoc_has_param_type(jsdoc, &pname)
                        } else {
                            false
                        };
                    self.maybe_report_implicit_any_parameter(param, has_jsdoc, pi);
                }
            }
        }

        // Check return type annotation for parameter properties in function types
        if method.type_annotation.is_some() {
            self.check_type_for_parameter_properties(method.type_annotation);
        }

        // Check for async modifier (needed for both abstract and concrete methods)
        let is_async = self.has_async_modifier(&method.modifiers);
        let is_generator = method.asterisk_token;

        // Check method body
        if method.body.is_some() {
            if !has_type_annotation {
                return_type = self.infer_return_type_from_body(member_idx, method.body, None);

                // Async methods implicitly return Promise<T>. Wrap the inferred
                // return type so the DTS emitter can emit `a(): Promise<void>`
                // instead of `a(): void`. Uses the same wrapping logic as
                // get_type_of_function (function_type.rs lines 1896-1919).
                if is_async && !is_generator {
                    if let Some(inner) = self.unwrap_promise_type(return_type) {
                        return_type = inner;
                    }
                    let promise_base = self
                        .ctx
                        .binder
                        .file_locals
                        .get("Promise")
                        .map(|sym_id| self.ctx.create_lazy_type_ref(sym_id))
                        .or_else(|| {
                            let lib_binders = self.get_lib_binders();
                            self.ctx
                                .binder
                                .get_global_type_with_libs("Promise", &lib_binders)
                                .map(|sym_id| self.ctx.create_lazy_type_ref(sym_id))
                        })
                        .unwrap_or(TypeId::PROMISE_BASE);
                    return_type = self
                        .ctx
                        .types
                        .factory()
                        .application(promise_base, vec![return_type]);
                }

                // Cache the inferred return type so the declaration emitter can look it up
                self.ctx.node_types.insert(member_idx.0, return_type);
            }

            // TS2697: Check if async method has access to Promise type
            // DISABLED: Causes too many false positives
            // TODO: Investigate lib loading for Promise detection
            // if is_async && !is_generator && !self.is_promise_global_available() {
            //     use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            //     self.error_at_node(
            //         method.name,
            //         diagnostic_messages::ASYNC_FUNCTION_MUST_RETURN_PROMISE,
            //         diagnostic_codes::ASYNC_FUNCTION_MUST_RETURN_PROMISE,
            //     );
            // }

            // TS7011 (implicit any return) is only emitted for ambient methods,
            // matching TypeScript's behavior
            // Async methods infer Promise<void>, not 'any', so they should NOT trigger TS7011
            let is_ambient_class = self
                .ctx
                .enclosing_class
                .as_ref()
                .is_some_and(|c| c.is_declared);
            let is_ambient_file = self.ctx.is_declaration_file();

            if (is_ambient_class || is_ambient_file) && !is_async && !skip_implicit_any {
                let method_name = self.get_property_name(method.name);
                self.maybe_report_implicit_any_return(
                    method_name,
                    Some(method.name),
                    return_type,
                    has_type_annotation,
                    false,
                    member_idx,
                );
            }

            // For async functions, unwrap Promise<T> to T for return type checking
            // The function body should return T, which gets auto-wrapped in Promise
            let effective_return_type = if is_generator && has_type_annotation {
                // Ensure the annotated return type is actually compatible with the Generator protocol.
                let generator_base = if is_async {
                    self.resolve_lib_type_by_name("AsyncGenerator")
                        .unwrap_or(TypeId::ERROR)
                } else {
                    self.resolve_lib_type_by_name("Generator")
                        .unwrap_or(TypeId::ERROR)
                };
                if generator_base != TypeId::ERROR {
                    // Only report if the return type is NOT a generator-like type
                    // (e.g., Iterable, Iterator, Generator, etc.). If it IS generator-like,
                    // the type is inherently compatible and doesn't need a structural check.
                    if self
                        .get_generator_return_type_argument(return_type)
                        .is_none()
                    {
                        let any_gen = self.ctx.types.factory().application(
                            generator_base,
                            vec![TypeId::ANY, TypeId::ANY, TypeId::UNKNOWN],
                        );
                        self.check_assignable_or_report(
                            any_gen,
                            return_type,
                            method.type_annotation,
                        );
                    }
                }

                self.get_generator_return_type_argument(return_type)
                    .unwrap_or(return_type)
            } else if is_async && !is_generator {
                self.unwrap_promise_type(return_type).unwrap_or(return_type)
            } else {
                return_type
            };

            self.push_return_type(effective_return_type);

            // For generator functions, push the contextual yield type so that
            // yield expressions can contextually type their operand.
            let contextual_yield_type = if is_generator && has_type_annotation {
                self.get_generator_yield_type_argument(return_type)
            } else {
                None
            };
            self.ctx.push_yield_type(contextual_yield_type);

            // Enter async context for await expression checking
            if is_async {
                self.ctx.enter_async_context();
            }

            let body_request = request.read().contextual_opt(None);
            self.clear_type_cache_recursive(method.body);
            self.check_statement_with_request(method.body, &body_request);

            // Exit async context
            if is_async {
                self.ctx.exit_async_context();
            }

            let mut check_return_type =
                self.return_type_for_implicit_return_check(return_type, is_async, is_generator);
            if is_async
                && check_return_type == return_type
                && has_type_annotation
                && self.return_type_annotation_looks_like_promise(method.type_annotation)
            {
                check_return_type = TypeId::VOID;
            }
            let requires_return = self.requires_return_value(check_return_type);
            let has_return = self.body_has_return_with_value(method.body);
            let falls_through = self.function_body_falls_through(method.body);

            if has_type_annotation && requires_return && falls_through {
                if !has_return {
                    self.error_at_node(
                        method.type_annotation,
                        "A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.",
                        diagnostic_codes::A_FUNCTION_WHOSE_DECLARED_TYPE_IS_NEITHER_UNDEFINED_VOID_NOR_ANY_MUST_RETURN_A_V,
                    );
                } else {
                    // TS2366: always emit when return type doesn't include undefined.
                    // tsc emits this regardless of strictNullChecks.
                    use crate::diagnostics::diagnostic_messages;
                    self.error_at_node(
                        method.type_annotation,
                        diagnostic_messages::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                        diagnostic_codes::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                    );
                }
            } else if self.ctx.no_implicit_returns()
                && has_return
                && falls_through
                && !self
                    .should_skip_no_implicit_return_check(check_return_type, has_type_annotation)
            {
                // TS7030: noImplicitReturns - not all code paths return a value
                // TSC points TS7030 to: return type annotation > method name > node itself
                use crate::diagnostics::diagnostic_messages;
                let error_node = if method.type_annotation.is_some() {
                    method.type_annotation
                } else if method.name.is_some() {
                    method.name
                } else {
                    method.body
                };
                self.error_at_node(
                    error_node,
                    diagnostic_messages::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                    diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                );
            }

            self.ctx.pop_yield_type();
            self.pop_return_type();
        } else {
            // Abstract method or method overload signature
            // Report TS7010 for abstract methods without return type annotation
            // Async methods infer Promise<void>, not 'any', so they should NOT trigger TS7010
            // Private members in ambient classes are excluded (not visible in .d.ts)
            if !is_async && !skip_implicit_any {
                let method_name = self.get_property_name(method.name);
                self.maybe_report_implicit_any_return(
                    method_name,
                    Some(method.name),
                    return_type,
                    has_type_annotation,
                    false,
                    member_idx,
                );
            }
        }

        // Check overload compatibility for method implementations
        if method.body.is_some() {
            self.check_overload_modifier_consistency(member_idx);
            self.check_overload_compatibility(member_idx);
            self.check_overload_modifier_agreement(member_idx);
        }

        if self.has_static_modifier(&method.modifiers) {
            self.check_static_member_for_class_type_param_refs(member_idx);
        }

        self.pop_type_parameters(type_param_updates);
    }

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
                self.error_at_node(
                    param_idx,
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
                && let Some(name_text) = self.node_text(param.name)
                && name_text == "static"
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
        self.check_binding_pattern_optionality(&ctor.parameters.nodes, ctor.body.is_some());

        // Check that rest parameters have array types (TS2370)
        self.check_rest_parameter_types(&ctor.parameters.nodes);

        // Check that parameter default values are assignable to declared types (TS2322)
        self.check_parameter_initializers(&ctor.parameters.nodes);
        self.check_non_impl_parameter_initializers(
            &ctor.parameters.nodes,
            self.has_declare_modifier(&ctor.modifiers),
            ctor.body.is_some(),
        );

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
        if let Some(template_offset) = raw_comment.find("@template") {
            let rest = &raw_comment[template_offset + "@template".len()..];
            let trimmed = rest.trim_start();
            // tsc points at the first type parameter name after @template
            let ws_len = rest.len() - trimmed.len();
            let error_offset = template_offset + "@template".len() + ws_len;
            let abs_pos = comment_pos + error_offset as u32;
            self.ctx.error(
                abs_pos,
                0,
                diagnostic_messages::TYPE_PARAMETERS_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION
                    .to_string(),
                diagnostic_codes::TYPE_PARAMETERS_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION,
            );
        }

        // TS1093: Check for @return/@returns tag with type annotation on constructor.
        // We must skip @returns tags that appear after @callback or @typedef, since
        // those tags create nested type definitions whose @returns belong to the
        // callback/typedef, not to the constructor itself.
        let nested_scope_start = ["@callback", "@typedef"]
            .iter()
            .filter_map(|t| raw_comment.find(t))
            .min();

        for tag in ["@returns", "@return"] {
            if let Some(tag_offset) = raw_comment.find(tag) {
                // Skip if this @returns belongs to a @callback/@typedef block
                if let Some(scope_start) = nested_scope_start
                    && tag_offset > scope_start
                {
                    continue;
                }
                let rest = &raw_comment[tag_offset + tag.len()..];
                let trimmed = rest.trim_start();
                if trimmed.starts_with('{') {
                    // tsc points one past the `{` of the type annotation
                    let ws_len = rest.len() - trimmed.len();
                    let error_offset = tag_offset + tag.len() + ws_len + 1;
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

    fn is_accessor_circular_reference(
        &self,
        type_node_idx: NodeIndex,
        accessor_name_idx: NodeIndex,
        _accessor_decl_idx: NodeIndex,
    ) -> bool {
        let Some(type_node) = self.ctx.arena.get(type_node_idx) else {
            return false;
        };

        // Check for `typeof this.prop` or `typeof ClassName.prop`
        if type_node.kind == syntax_kind_ext::TYPE_QUERY {
            let Some(query) = self.ctx.arena.get_type_query(type_node) else {
                return false;
            };
            let Some(expr_node) = self.ctx.arena.get(query.expr_name) else {
                return false;
            };

            // Case 1: `typeof this.prop` (PropertyAccessExpression)
            if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let Some(access) = self.ctx.arena.get_access_expr(expr_node) else {
                    return false;
                };

                // Check left side is `this`
                let is_this = self
                    .ctx
                    .arena
                    .get(access.expression)
                    .is_some_and(|n| n.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16);

                // Check left side is the class name (for static members)
                let is_class_name = !is_this
                    && self.ctx.enclosing_class.as_ref().is_some_and(|c| {
                        if let Some(id_node) = self.ctx.arena.get(access.expression)
                            && let Some(ident) = self.ctx.arena.get_identifier(id_node)
                        {
                            ident.escaped_text == c.name
                        } else {
                            false
                        }
                    });

                if is_this || is_class_name {
                    // Check property name matches accessor name
                    let prop_name = self
                        .ctx
                        .arena
                        .get_identifier_at(access.name_or_argument)
                        .map(|id| id.escaped_text.as_str());
                    let accessor_name = self.get_property_name(accessor_name_idx);

                    if let (Some(prop), Some(acc)) = (prop_name, accessor_name) {
                        return prop == acc;
                    }
                }
            }
            // Case 2: `typeof this.prop` where parser produces QualifiedName
            else if expr_node.kind == syntax_kind_ext::QUALIFIED_NAME {
                let Some(qn) = self.ctx.arena.get_qualified_name(expr_node) else {
                    return false;
                };

                // Check if left is `this`
                let is_this = self.ctx.arena.get(qn.left).is_some_and(|n| {
                    if n.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16 {
                        return true;
                    }
                    if let Some(ident) = self.ctx.arena.get_identifier(n) {
                        return ident.escaped_text == "this";
                    }
                    false
                });

                // Check left side is the class name (for static members)
                let is_class_name = !is_this
                    && self.ctx.enclosing_class.as_ref().is_some_and(|c| {
                        if let Some(id_node) = self.ctx.arena.get(qn.left)
                            && let Some(ident) = self.ctx.arena.get_identifier(id_node)
                        {
                            ident.escaped_text == c.name
                        } else {
                            false
                        }
                    });

                if is_this || is_class_name {
                    // Check property name matches accessor name
                    let prop_name = self
                        .ctx
                        .arena
                        .get_identifier_at(qn.right)
                        .map(|id| id.escaped_text.as_str());
                    let accessor_name = self.get_property_name(accessor_name_idx);

                    if let (Some(prop), Some(acc)) = (prop_name, accessor_name) {
                        return prop == acc;
                    }
                }
            }
        }

        false
    }

    /// Check an accessor declaration (getter/setter).
    #[allow(dead_code)]
    pub(crate) fn check_accessor_declaration(&mut self, member_idx: NodeIndex) {
        self.check_accessor_declaration_with_request(member_idx, &TypingRequest::NONE);
    }

    pub(crate) fn check_accessor_declaration_with_request(
        &mut self,
        member_idx: NodeIndex,
        request: &TypingRequest,
    ) {
        use crate::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let Some(accessor) = self.ctx.arena.get_accessor(node) else {
            return;
        };

        self.check_modifier_combinations(&accessor.modifiers);

        // Error 1183: An implementation cannot be declared in ambient contexts
        // Check if we're in a declared class and the accessor has a body.
        // TSC anchors the error at the body node (the `{`).
        if accessor.body.is_some()
            && let Some(ref class_info) = self.ctx.enclosing_class
            && class_info.is_declared
        {
            self.error_at_node(
                accessor.body,
                "An implementation cannot be declared in ambient contexts.",
                diagnostic_codes::AN_IMPLEMENTATION_CANNOT_BE_DECLARED_IN_AMBIENT_CONTEXTS,
            );
        }

        // Error 1318: An abstract accessor cannot have an implementation
        // Abstract accessors must not have a body
        if accessor.body.is_some() && self.has_abstract_modifier(&accessor.modifiers) {
            self.error_at_node(
                member_idx,
                "An abstract accessor cannot have an implementation.",
                diagnostic_codes::METHOD_CANNOT_HAVE_AN_IMPLEMENTATION_BECAUSE_IT_IS_MARKED_ABSTRACT,
            );
        }

        let is_getter = node.kind == syntax_kind_ext::GET_ACCESSOR;

        // TS2808: A get accessor must be at least as accessible as the setter
        if is_getter {
            self.check_getter_setter_accessibility(accessor);
        }

        let has_type_annotation = is_getter && accessor.type_annotation.is_some();
        let mut return_type = if is_getter {
            if has_type_annotation {
                // Check for TS2502 using AST inspection first
                if self.is_accessor_circular_reference(
                    accessor.type_annotation,
                    accessor.name,
                    member_idx,
                ) {
                    let name = self
                        .get_property_name(accessor.name)
                        .unwrap_or_else(|| "unknown".to_string());
                    let message = format!(
                        "'{name}' is referenced directly or indirectly in its own type annotation."
                    );
                    self.error_at_node(accessor.name, &message, 2502);
                    // Use ANY to prevent further errors
                    TypeId::ANY
                } else {
                    self.get_type_from_type_node(accessor.type_annotation)
                }
            } else {
                TypeId::VOID // Default to void for getters without type annotation
            }
        } else {
            TypeId::VOID
        };

        let contextual_setter_param_types = if node.kind == syntax_kind_ext::SET_ACCESSOR {
            self.contextual_setter_parameter_types_for_class_accessor(accessor)
        } else {
            None
        };
        self.cache_parameter_types(
            &accessor.parameters.nodes,
            contextual_setter_param_types.as_deref(),
        );
        if let Some(contextual_types) = contextual_setter_param_types.as_ref() {
            for (&param_idx, contextual_type) in accessor
                .parameters
                .nodes
                .iter()
                .zip(contextual_types.iter().copied())
            {
                let Some(contextual_type) = contextual_type else {
                    continue;
                };
                self.ctx.node_types.insert(param_idx.0, contextual_type);
                if let Some(param) = self.ctx.arena.get_parameter_at(param_idx) {
                    self.ctx.node_types.insert(param.name.0, contextual_type);
                }
            }
        }

        // Check that parameter default values are assignable to declared types (TS2322)
        self.check_parameter_initializers(&accessor.parameters.nodes);

        // Check for parameter properties (error 2369)
        // Parameter properties are only allowed in constructors, not in accessors
        self.check_parameter_properties(&accessor.parameters.nodes);

        // TSC suppresses TS7006/TS7010 for private accessors in ambient (declare) classes
        let skip_implicit_any_accessor = self
            .ctx
            .enclosing_class
            .as_ref()
            .is_some_and(|c| c.is_declared)
            && self.has_private_modifier(&accessor.modifiers);

        // Check getter parameters for TS7006 here.
        // Setter parameters are checked in check_setter_parameter() below, which also
        // validates other setter constraints (no initializer, no rest parameter).
        if is_getter && !skip_implicit_any_accessor {
            for (pi, &param_idx) in accessor.parameters.nodes.iter().enumerate() {
                if let Some(param_node) = self.ctx.arena.get(param_idx)
                    && let Some(param) = self.ctx.arena.get_parameter(param_node)
                {
                    let has_jsdoc = self.param_has_inline_jsdoc_type(param_idx);
                    self.maybe_report_implicit_any_parameter(param, has_jsdoc, pi);
                }
            }
        }

        // For setters, check parameter constraints (1052, 1053)
        if node.kind == syntax_kind_ext::SET_ACCESSOR {
            // TS2808: A get accessor must be at least as accessible as the setter
            // tsc emits this on BOTH the getter and setter declarations.
            self.check_setter_getter_accessibility(accessor);

            // Check if a paired getter exists — if so, setter parameter type is
            // inferred from the getter return type (contextually typed, no TS7006)
            let has_paired_getter = self.setter_has_paired_getter(member_idx, accessor);
            // Get accessor-level JSDoc to suppress TS7006 for @param annotations
            let accessor_jsdoc = self.get_jsdoc_for_function(member_idx);
            let accessor_name = if accessor.name.is_some() {
                Some(accessor.name)
            } else {
                None
            };
            self.check_setter_parameter(
                &accessor.parameters.nodes,
                has_paired_getter || skip_implicit_any_accessor,
                accessor_jsdoc.as_deref(),
                accessor_name,
            );
        }

        // Check accessor body
        if accessor.body.is_some() {
            if is_getter && !has_type_annotation {
                // Use full body-based inference for getter checking so nested returns
                // and implicit fallthrough are represented (e.g. `T | void`), which
                // aligns noImplicitReturns diagnostics with TSC behavior.
                return_type = self.infer_return_type_from_body(member_idx, accessor.body, None);
                // Cache the inferred return type so the declaration emitter can look it up
                self.ctx.node_types.insert(member_idx.0, return_type);
            }

            // TS7010 (implicit any return) is only emitted for ambient accessors,
            // matching TypeScript's behavior
            // Async getters infer Promise<void>, not 'any', so they should NOT trigger TS7010
            if is_getter {
                let is_ambient_class = self
                    .ctx
                    .enclosing_class
                    .as_ref()
                    .is_some_and(|c| c.is_declared);
                let is_ambient_file = self.ctx.is_declaration_file();
                let is_async = self.has_async_modifier(&accessor.modifiers);

                if (is_ambient_class || is_ambient_file) && !is_async && !skip_implicit_any_accessor
                {
                    let accessor_name = self.get_property_name(accessor.name);
                    self.maybe_report_implicit_any_return(
                        accessor_name,
                        Some(accessor.name),
                        return_type,
                        has_type_annotation,
                        false,
                        member_idx,
                    );
                }
            }

            // When the return type was purely inferred from the body (no annotation),
            // push ANY so check_return_statement skips the circular assignability check.
            let effective_return_type = if has_type_annotation {
                return_type
            } else {
                TypeId::ANY
            };
            self.push_return_type(effective_return_type);

            let body_request = request.read().contextual_opt(None);
            self.clear_type_cache_recursive(accessor.body);
            self.check_statement_with_request(accessor.body, &body_request);
            if is_getter {
                // Check if this is an async getter
                let is_async = self.has_async_modifier(&accessor.modifiers);
                // For async getters, extract the inner type from Promise<T>
                let mut check_return_type = self.return_type_for_implicit_return_check(
                    return_type,
                    is_async,
                    false, // getters cannot be generators
                );
                if is_async
                    && check_return_type == return_type
                    && has_type_annotation
                    && self.return_type_annotation_looks_like_promise(accessor.type_annotation)
                {
                    check_return_type = TypeId::VOID;
                }
                let requires_return = self.requires_return_value(check_return_type);
                let has_return = self.body_has_return_with_value(accessor.body);
                let falls_through = self.function_body_falls_through(accessor.body);

                // TS2378: A 'get' accessor must return a value (regardless of type annotation)
                // Get accessors ALWAYS require a return value, even without type annotation
                if !has_return && falls_through {
                    // Use TS2378 for getters without return statements
                    self.error_at_node(
                        accessor.name,
                        "A 'get' accessor must return a value.",
                        diagnostic_codes::A_GET_ACCESSOR_MUST_RETURN_A_VALUE,
                    );
                } else if has_type_annotation && requires_return && falls_through {
                    // TS2366: always emit when return type doesn't include undefined
                    use crate::diagnostics::diagnostic_messages;
                    self.error_at_node(
                        accessor.type_annotation,
                        diagnostic_messages::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                        diagnostic_codes::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                    );
                } else if self.ctx.no_implicit_returns()
                    && has_return
                    && falls_through
                    && !self.should_skip_no_implicit_return_check(
                        check_return_type,
                        has_type_annotation,
                    )
                {
                    // TS7030: noImplicitReturns - not all code paths return a value
                    // TSC points TS7030 to: return type annotation > accessor name > node itself
                    use crate::diagnostics::diagnostic_messages;
                    let error_node = if accessor.type_annotation.is_some() {
                        accessor.type_annotation
                    } else if accessor.name.is_some() {
                        accessor.name
                    } else {
                        accessor.body
                    };
                    self.error_at_node(
                        error_node,
                        diagnostic_messages::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                        diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                    );
                }
            }

            self.pop_return_type();
        }

        if self.has_static_modifier(&accessor.modifiers) {
            self.check_static_member_for_class_type_param_refs(member_idx);
        }
    }

    /// Check if a setter has a paired getter with the same name in the class.
    ///
    /// TSC infers setter parameter types from the getter return type, so a setter
    /// with a paired getter has contextually typed parameters (no TS7006).
    fn setter_has_paired_getter(
        &self,
        _setter_idx: NodeIndex,
        setter_accessor: &tsz_parser::parser::node::AccessorData,
    ) -> bool {
        self.paired_getter_member_for_setter(setter_accessor)
            .is_some()
    }

    /// TS2808: Check that a getter is at least as accessible as its paired setter.
    ///
    /// Accessibility ordering: public(3) > protected(2) > private(1).
    /// If the getter is less accessible than the setter, emit TS2808.
    fn check_getter_setter_accessibility(
        &mut self,
        getter: &tsz_parser::parser::node::AccessorData,
    ) {
        let Some(ref class_info) = self.ctx.enclosing_class else {
            return;
        };

        let getter_name = match self.get_property_name(getter.name) {
            Some(n) => n,
            None => return,
        };

        // Find the paired setter
        let member_nodes = class_info.member_nodes.clone();
        for &member_idx in &member_nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::SET_ACCESSOR {
                continue;
            }
            let Some(setter) = self.ctx.arena.get_accessor(member_node) else {
                continue;
            };
            let Some(setter_name) = self.get_property_name(setter.name) else {
                continue;
            };
            if setter_name != getter_name {
                continue;
            }

            // Found paired setter — compare accessibility
            let getter_level = self.accessibility_level(&getter.modifiers);
            let setter_level = self.accessibility_level(&setter.modifiers);

            if getter_level < setter_level {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    getter.name,
                    diagnostic_messages::A_GET_ACCESSOR_MUST_BE_AT_LEAST_AS_ACCESSIBLE_AS_THE_SETTER,
                    diagnostic_codes::A_GET_ACCESSOR_MUST_BE_AT_LEAST_AS_ACCESSIBLE_AS_THE_SETTER,
                );
            }
            return;
        }
    }

    /// Get the accessibility level for comparing getter/setter pairs.
    /// Returns: 3 for public (default), 2 for protected, 1 for private.
    fn accessibility_level(&self, modifiers: &Option<tsz_parser::parser::NodeList>) -> u8 {
        if self.has_private_modifier(modifiers) {
            1
        } else if self.has_protected_modifier(modifiers) {
            2
        } else {
            3 // public (explicit or implicit)
        }
    }

    /// TS2808 (setter side): Check that the paired getter is at least as accessible.
    ///
    /// tsc emits TS2808 on both the getter and setter, so this method emits on the setter.
    fn check_setter_getter_accessibility(
        &mut self,
        setter: &tsz_parser::parser::node::AccessorData,
    ) {
        let Some(ref class_info) = self.ctx.enclosing_class else {
            return;
        };

        let setter_name = match self.get_property_name(setter.name) {
            Some(n) => n,
            None => return,
        };

        // Find the paired getter
        let member_nodes = class_info.member_nodes.clone();
        for &member_idx in &member_nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::GET_ACCESSOR {
                continue;
            }
            let Some(getter) = self.ctx.arena.get_accessor(member_node) else {
                continue;
            };
            let Some(getter_name) = self.get_property_name(getter.name) else {
                continue;
            };
            if getter_name != setter_name {
                continue;
            }

            // Found paired getter — compare accessibility
            let getter_level = self.accessibility_level(&getter.modifiers);
            let setter_level = self.accessibility_level(&setter.modifiers);

            if getter_level < setter_level {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    setter.name,
                    diagnostic_messages::A_GET_ACCESSOR_MUST_BE_AT_LEAST_AS_ACCESSIBLE_AS_THE_SETTER,
                    diagnostic_codes::A_GET_ACCESSOR_MUST_BE_AT_LEAST_AS_ACCESSIBLE_AS_THE_SETTER,
                );
            }
            return;
        }
    }

    /// Resolve the symbol of a computed property name's inner expression.
    /// Returns the SymbolId if the name is a computed property with an identifier
    /// that resolves to a known symbol.
    pub(crate) fn resolve_computed_name_symbol(
        &self,
        name_idx: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        let name_node = self.ctx.arena.get(name_idx)?;
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }
        let computed = self.ctx.arena.get_computed_property(name_node)?;
        self.ctx
            .binder
            .resolve_identifier(self.ctx.arena, computed.expression)
    }
}
