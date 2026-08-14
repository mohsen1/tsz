//! Ambient and class member declaration checks (property, method, constructor, accessor).
//!
//! For overload compatibility, signature utilities, and implicit-any return checks,
//! see [`super::overload_compatibility`].

use crate::context::{TypingRequest, speculation::DiagnosticSpeculationSnapshot};
use crate::query_boundaries::common::ContextualTypeContext;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    #[expect(dead_code)]
    pub(crate) fn check_property_declaration(&mut self, member_idx: NodeIndex) {
        self.check_property_declaration_with_request(member_idx, &TypingRequest::NONE);
    }

    /// Whether `member_idx` is a *redeclaration* of an earlier same-named
    /// property (with the same static-ness) in the enclosing class.
    ///
    /// `tsc` computes a member's implicit-`any` (`TS7008`) once per member
    /// *symbol*, anchored at the symbol's first declaration. A later
    /// declaration that shares the name only receives the duplicate-identifier
    /// (`TS2300`) and initialization/subsequent-type (`TS2564`/`TS2717`)
    /// diagnostics, never a second `TS7008` — even when that later declaration
    /// itself lacks a type annotation and initializer. Static and instance
    /// members occupy separate namespaces, so a `static x` does not shadow an
    /// instance `x` (each keeps its own `TS7008`).
    fn member_redeclares_earlier_property(&self, member_idx: NodeIndex, is_static: bool) -> bool {
        let Some(class_info) = self.ctx.enclosing_class.as_ref() else {
            return false;
        };
        let Some(pos) = class_info
            .member_nodes
            .iter()
            .position(|&idx| idx == member_idx)
        else {
            return false;
        };
        let earlier_members: Vec<NodeIndex> = class_info.member_nodes[..pos].to_vec();
        let Some(name) = self.get_member_name(member_idx) else {
            return false;
        };
        earlier_members.iter().any(|&earlier| {
            self.ctx
                .arena
                .get(earlier)
                .is_some_and(|n| n.kind == syntax_kind_ext::PROPERTY_DECLARATION)
                && self.is_static_property(earlier) == is_static
                && self.get_member_name(earlier).as_deref() == Some(name.as_str())
        })
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
            {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.check_computed_property_requires_literal(
                    prop.name,
                    diagnostic_messages::A_COMPUTED_PROPERTY_NAME_IN_A_CLASS_PROPERTY_DECLARATION_MUST_HAVE_A_SIMPLE_LITE,
                    diagnostic_codes::A_COMPUTED_PROPERTY_NAME_IN_A_CLASS_PROPERTY_DECLARATION_MUST_HAVE_A_SIMPLE_LITE,
                );
            }
        }
        // TS1539: a bigint literal class property name (`123n = 1`).
        self.check_bigint_literal_property_name(prop.name);
        self.check_modifier_combinations(&prop.modifiers, prop.name, node.kind);

        // TS8009/TS8010: Check for TypeScript-only features in JavaScript files
        let is_js_file = self.is_js_file();
        tracing::debug!(is_js_file, file_name = %self.ctx.file_name, "Checking if JS file for TS8009/TS8010");

        if is_js_file {
            use crate::diagnostics::{diagnostic_messages, format_message};

            // TS8009: Modifiers like 'declare' can only be used in TypeScript files
            if self.ctx.arena.is_declare(&prop.modifiers) {
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

        // noImplicitAny: check function-type annotation parameters for TS7006/TS7019.
        // Ambient class properties (no initializer) skip the normal `check_type_for_missing_names`
        // path, so we explicitly walk the type annotation here.
        // Example: `public pub_f10: (x) => string;` — tsc emits TS7006 for `x`.
        if prop.type_annotation.is_some() && self.ctx.no_implicit_any() {
            self.check_type_annotation_for_implicit_any_params(prop.type_annotation);
        }

        if prop.type_annotation.is_some()
            && let Some(class_info) = self.ctx.enclosing_class.as_ref()
            && let Some(property_name) =
                crate::types_domain::queries::core::get_literal_property_name(
                    self.ctx.arena,
                    prop.name,
                )
            && self.indexed_access_references_owner_property(
                prop.type_annotation,
                &class_info.name,
                &property_name,
            )
        {
            let message = format!(
                "'{property_name}' is referenced directly or indirectly in its own type annotation."
            );
            self.error_at_node(prop.name, &message, 2502);
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

            // tsc points TS1255/TS1263/TS1264 at the `!` token itself, which
            // immediately follows the property name, i.e. at `name_node.end`
            // (length 1) — the same anchor the variable-declaration arm uses.
            let excl_pos = self.ctx.arena.get(prop.name).map(|n| n.end);

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
            // Only emit when there is no initializer — if an initializer is present,
            // TS1263 already fires and tsc suppresses TS1264 in that case.
            if prop.type_annotation.is_none() && prop.initializer.is_none() {
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
        if prop.initializer.is_some() {
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
                // A duplicate `declare` fires TS1030 in the parser; tsc's
                // `checkGrammarModifiers` `return`s there and never runs
                // `checkGrammarProperty`, so the ambient-initializer check below
                // is suppressed for such a member (matches tsc's single grammar
                // diagnostic on `declare declare x = 1`).
                let has_duplicate_declare = self.has_duplicate_declare_modifier(&prop.modifiers);
                if !has_es_decorator_on_declare && !has_duplicate_declare {
                    // A `readonly` class property (incl. `static readonly`) behaves
                    // like a `const` in an ambient context: a string/numeric/negated-
                    // numeric literal initializer is accepted (it is preserved), while
                    // a non-literal initializer is TS1254. A non-readonly property — or
                    // a readonly property carrying an explicit type annotation — is
                    // TS1039. Mirrors the ambient *variable* path in `statement_checks`.
                    let is_readonly = self
                        .ctx
                        .arena
                        .has_modifier(&prop.modifiers, tsz_scanner::SyntaxKind::ReadonlyKeyword);
                    if is_readonly && prop.type_annotation.is_none() {
                        if !self.is_valid_const_initializer(prop.initializer) {
                            self.error_at_node(
                                prop.initializer,
                                diagnostic_messages::A_CONST_INITIALIZER_IN_AN_AMBIENT_CONTEXT_MUST_BE_A_STRING_OR_NUMERIC_LITERAL_OR,
                                diagnostic_codes::A_CONST_INITIALIZER_IN_AN_AMBIENT_CONTEXT_MUST_BE_A_STRING_OR_NUMERIC_LITERAL_OR,
                            );
                        }
                    } else {
                        self.error_at_node(
                            prop.initializer,
                            diagnostic_messages::INITIALIZERS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS,
                            diagnostic_codes::INITIALIZERS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS,
                        );
                    }
                }
            }
        }

        // Check for await expressions in the initializer (TS1308)
        if prop.initializer.is_some() {
            self.check_await_expression(prop.initializer);
        }

        // Use the relation-shape declared type here so a fresh-symbol
        // initializer can flow into a `static readonly: unique symbol`.
        let effective_declared_type = self.class_property_relation_declared_type(member_idx, prop);
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

            // Match tsc TS2322 on `null = () => Unresolved`: allow the check
            // through a nested-error target only for nullish initializers.
            let init_is_nullish = init_type == TypeId::NULL || init_type == TypeId::UNDEFINED;
            let nested_err =
                declared_type != TypeId::ERROR && self.type_contains_error(declared_type);
            let relation_target = self.class_property_init_relation_target(prop, declared_type);
            if declared_type != TypeId::ANY
                && declared_type != TypeId::ERROR
                && (!nested_err || init_is_nullish)
                && self.check_assignable_or_report_at(
                    init_type,
                    relation_target,
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
            let initializer_snap = DiagnosticSpeculationSnapshot::new(&self.ctx);
            let init_type = self.get_type_of_node_with_request(prop.initializer, &request);
            self.check_direct_class_expression_initializer(prop.initializer, &request);
            inferred_initializer_type = Some(init_type);

            if self.ctx.no_implicit_any()
                && contextual_member_type.is_none()
                && prop.type_annotation.is_none()
                && self.class_property_initializer_has_non_deferred_circularity(member_idx)
                && let Some(member_name) = self.get_member_name_display_text(prop.name)
            {
                self.suppress_circular_initializer_relation_diagnostics(
                    initializer_snap,
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
        // TSC suppresses this for private members in ambient (declare) classes,
        // and independently for a private/private-identifier member that
        // carries its own (grammatically illegal here) `declare` modifier —
        // see `member_own_declare_hides_from_ambient_surface`.
        let is_private_in_ambient = (self
            .ctx
            .enclosing_class
            .as_ref()
            .is_some_and(|c| c.is_declared)
            && (self.has_private_modifier(&prop.modifiers)
                || self.is_private_identifier_name(prop.name)))
            || self.member_own_declare_hides_from_ambient_surface(&prop.modifiers, prop.name);
        let is_static = self.has_static_modifier(&prop.modifiers);
        // tsc suppresses TS7008 for `static prototype` since TS2699 already fires
        let is_static_prototype = is_static
            && self
                .get_member_name_display_text(prop.name)
                .is_some_and(|n| n == "prototype");
        // Check if property is abstract - abstract properties should emit TS7008
        // even if assigned in constructor (since the assignment is an error - TS2715)
        let is_abstract = self.has_abstract_modifier(&prop.modifiers);

        // Infer the type of an un-annotated, un-initialized instance property
        // from the constructor's `this.<name> = ...` assignments (tsc's
        // control-flow property inference). The result both supplies the cached
        // type below (for the declaration emitter) and governs TS7008: tsc
        // suppresses the implicit-any error exactly when this inference yields a
        // concrete (non-`any`) type — including conditionally-assigned fields
        // (`x: number | undefined`) — and keeps it when every assignment only
        // produces `null`/`undefined` (the flow type widens back to `any`).
        let ctor_flow_type = if !is_static
            && !is_abstract
            && effective_declared_type.is_none()
            && contextual_member_type.is_none()
            && inferred_initializer_type.is_none()
            && prop.initializer.is_none()
            && prop.type_annotation.is_none()
            && !self.has_accessor_modifier(&prop.modifiers)
        {
            self.infer_property_type_from_enclosing_constructor_flow(prop.name)
        } else {
            None
        };

        if self.ctx.no_implicit_any()
            && effective_declared_type.is_none()
            && prop.initializer.is_none()
            && prop.type_annotation.is_none()
            && !is_private_in_ambient
            && !is_static_prototype
            // Constructor-flow inference only applies to instance properties. A
            // concrete inferred type suppresses the implicit-any error; so does
            // *any* constructor assignment to the property even when inference
            // yields no concrete type (accessor auto-properties, values that
            // widen to `any`, or `null`/`undefined`-only assignments), matching
            // tsc, which takes the property's type from constructor flow in all
            // of those cases.
            && (is_static
                || is_abstract
                || (ctor_flow_type.is_none()
                    && !self.property_assigned_in_enclosing_class_constructor(prop.name)))
            // TSC also suppresses TS7008 for static properties assigned in class
            // static blocks (e.g., `static { this.x = 1; }`)
            && !(is_static
                && self.property_assigned_in_enclosing_class_static_block(prop.name))
            // A redeclaration of an earlier same-named member is not the
            // symbol's primary declaration; tsc emits the implicit-any member
            // error once, on the first declaration (the redeclaration still
            // gets TS2300/TS2564/TS2717).
            && !self.member_redeclares_earlier_property(member_idx, is_static)
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
        } else if !is_static {
            // Un-annotated, un-initialized instance property: reuse the
            // constructor-flow inference computed above so the cached type (used
            // by the declaration emitter) matches the class instance type.
            ctor_flow_type.unwrap_or(TypeId::ANY)
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
    #[expect(dead_code)]
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

        // TS1165: Computed property name in an ambient context must refer to
        // an expression whose type is a literal type or a 'unique symbol'
        // type. `is_declared` already folds in every ambient spelling
        // (`declare class`, `declare abstract class`, a class nested in
        // `declare namespace`/`declare module`, and an implicitly-ambient
        // `.d.ts` file), so no separate `is_declaration_file()` check is
        // needed here. This is the method-signature sibling of TS1166 (class
        // property declarations, checked unconditionally regardless of
        // ambient-ness in `check_property_declaration_with_request`), TS1169
        // (interfaces), and TS1170 (type literals) — verified against the
        // pinned `typescript@7.0.2` oracle that accessors are exempt from
        // this arm entirely, so it applies to method declarations only.
        let in_declared_class = self
            .ctx
            .enclosing_class
            .as_ref()
            .is_some_and(|c| c.is_declared);
        if in_declared_class {
            use crate::diagnostics::diagnostic_messages;
            self.check_computed_property_requires_literal(
                method.name,
                diagnostic_messages::A_COMPUTED_PROPERTY_NAME_IN_AN_AMBIENT_CONTEXT_MUST_REFER_TO_AN_EXPRESSION_WHOSE,
                diagnostic_codes::A_COMPUTED_PROPERTY_NAME_IN_AN_AMBIENT_CONTEXT_MUST_REFER_TO_AN_EXPRESSION_WHOSE,
            );
        }

        // Error 1183: An implementation cannot be declared in ambient contexts
        // Check if we're in a declared class and the method has a body,
        // OR if the method itself has a `declare` modifier and a body.
        // TSC anchors the error at the body node (the `{`), not the whole member.
        if method.body.is_some() {
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

        // TS1168: a computed method name in a *concrete* (non-ambient) class
        // must be a literal or `unique symbol`-typed expression when the
        // method has no body — either a genuine overload signature ahead of
        // its implementation, or a standalone `abstract` method, both of
        // which are bodyless. tsc reports this per bodyless declaration, not
        // once per overload group: an implementation with a body (even one
        // sharing the same bad computed name) never takes it, only the
        // signature(s) that precede it. TS1165 is this same grammar rule's
        // ambient-context sibling (`declare class`/`.d.ts`); the two are
        // mutually exclusive on the same `is_ambient` computation used above
        // for TS1221/TS1222, so this arm only ever fires where TS1165 does
        // not. Accessors are a different function (`check_accessor_...`) and
        // are not affected: an `abstract get`/`set` with a bad computed name
        // stays clean under tsc's own grammar, unlike an `abstract` method.
        if method.body.is_none() {
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

            if !is_ambient {
                use crate::diagnostics::diagnostic_messages;
                self.check_computed_property_requires_literal(
                    method.name,
                    diagnostic_messages::A_COMPUTED_PROPERTY_NAME_IN_A_METHOD_OVERLOAD_MUST_REFER_TO_AN_EXPRESSION_WHOSE,
                    diagnostic_codes::A_COMPUTED_PROPERTY_NAME_IN_A_METHOD_OVERLOAD_MUST_REFER_TO_AN_EXPRESSION_WHOSE,
                );
            }
        }

        // Keep syntax and declaration-stamped JSDoc binders in scope while checking.
        let (type_params, type_param_updates) = self.push_type_parameters(&method.type_parameters);
        let method_jsdoc = self.get_jsdoc_for_function(member_idx);
        let jsdoc_type_param_updates = if type_params.is_empty()
            && let Some(jsdoc) = method_jsdoc.as_deref()
        {
            self.push_jsdoc_template_type_parameters_for_owner(member_idx, jsdoc)
                .1
        } else {
            Vec::new()
        };

        self.check_modifier_combinations(&method.modifiers, method.name, node.kind);

        // Check for unused type parameters (TS6133)
        self.check_unused_type_params(&method.type_parameters, member_idx);

        // Extract parameter types from contextual type (for object literal methods)
        // This enables shorthand method parameter type inference
        let mut param_types: Vec<Option<TypeId>> = Vec::new();
        let contextual_method_type =
            self.contextual_class_member_type_from_request(request, method.name);
        let prototype_owner_this_type = if self.is_js_file() {
            self.js_prototype_owner_expression_for_node(member_idx)
                .and_then(|owner_expr| self.js_prototype_owner_function_target(owner_expr))
                .and_then(|owner_target| {
                    self.synthesize_js_constructor_instance_type(owner_target, TypeId::ANY, &[])
                })
        } else {
            None
        };
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
        let contextual_this_type = contextual_method_type.and_then(|ctx_type| {
            let ctx_helper = ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                ctx_type,
                self.ctx.compiler_options.no_implicit_any,
            );
            ctx_helper.get_this_type()
        });
        let explicit_this_type = self
            .get_explicit_this_type_annotation(&method.parameters.nodes)
            .map(|ann_idx| self.get_type_from_type_node(ann_idx));
        let implicit_this_type = explicit_this_type
            .or(prototype_owner_this_type)
            .or(contextual_this_type);
        let mut pushed_this_type = false;
        if let Some(this_type) = implicit_this_type {
            self.ctx.this_type_stack.push(this_type);
            self.ctx.function_owned_this_stack.push(member_idx);
            pushed_this_type = true;
        }

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
            // A method's *type* parameters carry the same reserved-word grammar as
            // its value parameters — `class C { m[ yield ]() {} }` is TS1213 in tsc.
            // Only free functions and the class/interface heads reached this check
            // before, so class members were the hole.
            self.check_strict_mode_reserved_type_parameter_names(
                &method.type_parameters,
                member_idx,
                self.ctx.enclosing_class.is_some(),
            );
        }

        // Check for required parameters following optional parameters (TS1016)
        self.check_parameter_ordering(&method.parameters, Some(member_idx));
        self.check_binding_pattern_optionality(
            &method.parameters.nodes,
            method.body.is_some(),
            Some(member_idx),
        );

        // Check that rest parameters have array types (TS2370)
        self.check_rest_parameter_types(&method.parameters.nodes);

        // Check that parameter default values are assignable to declared types
        // (TS2322). is_async is needed here for TS1308 (#16072); computed
        // early since the signature is checked before the body's own async
        // context is pushed (see the later "async modifier" block below).
        let is_async = self.has_async_modifier(&method.modifiers);
        self.check_parameter_initializers(&method.parameters.nodes, is_async);
        self.check_non_impl_parameter_initializers(
            &method.parameters.nodes,
            self.has_declare_modifier(&method.modifiers),
            method.body.is_some(),
        );

        // Check for parameter properties (error 2369)
        // Parameter properties are only allowed in constructors, not in methods
        self.check_parameter_properties(&method.parameters.nodes);

        // Check parameter type annotations for parameter properties in function types
        // TSC suppresses the noImplicitAny member family for members that are not
        // part of an ambient declaration's observable surface, and independently
        // for a private/private-identifier method that carries its own
        // (grammatically illegal here) `declare` modifier — see
        // `member_own_declare_hides_from_ambient_surface`. Accessors do not get
        // this second suppression (oracle-confirmed); only this method call site
        // ORs it in.
        let skip_implicit_any = self
            .member_hidden_from_ambient_declaration_surface(&method.modifiers, method.name)
            || self.member_own_declare_hides_from_ambient_surface(&method.modifiers, method.name);
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

        // is_async computed above, before parameter checking.
        let is_generator = method.asterisk_token;

        // TS1064/TS1055/TS2705: parity with fn declarations (issue #4762).
        self.check_async_return_type_is_promise(
            has_type_annotation,
            is_async,
            is_generator,
            return_type,
            method.type_annotation,
        );

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
                        .lib_promise_type_ref()
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

            // Missing-Promise diagnostics for async methods are owned by
            // `get_type_of_function`, which runs for every method body and calls
            // `check_async_promise_constructor_availability` (TS2468/TS2705) and
            // `check_async_return_type_is_promise` (TS1064/TS1055). Those helpers
            // already model `tsc`'s position/target rules for async methods, so
            // there is intentionally no additional Promise-availability check
            // anchored here.

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
                let method_name = self.member_name_for_diagnostic(method.name);
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

            // When the return type is inferred from the body, avoid
            // re-checking return statements against that inferred type.
            // Feeding the inferred type back as contextual typing can
            // create circular false positives in generic calls.
            let body_return_type = if has_type_annotation {
                effective_return_type
            } else {
                TypeId::ANY
            };

            self.push_return_type(body_return_type);

            // For generator functions, push the contextual yield type so that
            // yield expressions can contextually type their operand.
            let contextual_yield_type = if is_generator && has_type_annotation {
                self.get_generator_yield_type_argument(return_type)
            } else {
                None
            };
            self.ctx.push_yield_type(contextual_yield_type);

            // Scope the async context to THIS method body. A non-async
            // function nested inside an async one must not inherit the flag.
            let saved_async_depth = self.ctx.enter_function_async_context(is_async);

            let body_request = request.read().contextual_opt(None);
            self.clear_type_cache_recursive(method.body);
            let saved_member_body_depth = self.ctx.enter_class_member_body();
            self.check_statement_with_request(method.body, &body_request);
            self.ctx.exit_class_member_body(saved_member_body_depth);

            self.ctx.restore_async_context(saved_async_depth);

            // A generator's effective TS7030 return type is its `TReturn` (tsc's
            // `unwrapReturnType`). Extract it once and reuse for the
            // `check_return_type` sentinel below and the per-bare-return type,
            // since the extraction is not idempotent (evaluating a
            // `Generator<Y, R, N>` can expand it structurally and drop the
            // wrapper). An unannotated generator's `return_type` already holds
            // its inferred `TReturn`, so extraction returns `None` there.
            let generator_return_completeness = if is_generator {
                self.generator_return_type_for_implicit_return_check(return_type)
            } else {
                None
            };
            let mut check_return_type = if is_generator {
                generator_return_completeness.unwrap_or(TypeId::UNKNOWN)
            } else {
                self.return_type_for_implicit_return_check(return_type, is_async, false)
            };
            if is_async
                && check_return_type == return_type
                && has_type_annotation
                && self.return_type_annotation_is_exactly_promise(method.type_annotation)
            {
                check_return_type = TypeId::VOID;
            }
            let requires_return = self.requires_return_value(check_return_type);
            let has_return = self.body_has_return_with_value(method.body);
            let falls_through = self.function_body_falls_through(method.body);

            if has_type_annotation
                && requires_return
                && falls_through
                && (!has_return || self.ctx.strict_null_checks())
            {
                if !has_return {
                    self.error_at_node(
                        method.type_annotation,
                        "A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.",
                        diagnostic_codes::A_FUNCTION_WHOSE_DECLARED_TYPE_IS_NEITHER_UNDEFINED_VOID_NOR_ANY_MUST_RETURN_A_V,
                    );
                } else {
                    // TS2366 (has explicit return, falls through). The branch gate
                    // above guarantees strictNullChecks here: in non-strict mode
                    // `undefined` is assignable to every type, so tsc's guard
                    // `strictNullChecks && !isTypeAssignableTo(undefinedType, type)`
                    // short-circuits to false (checker.ts checkAllCodePaths... :39580).
                    // Excluding the has-return non-strict case from the gate lets
                    // control fall through to the TS7030 noImplicitReturns check.
                    use crate::diagnostics::diagnostic_messages;
                    self.error_at_node(
                        method.type_annotation,
                        diagnostic_messages::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                        diagnostic_codes::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                    );
                }
            } else if has_type_annotation
                && check_return_type == TypeId::UNKNOWN
                && !has_return
                && falls_through
                && !is_generator
            {
                // TS2355 for `method(): unknown {}` (empty body): see the note
                // on the same pattern in check_function_return_paths.
                self.error_at_node(
                    method.type_annotation,
                    "A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.",
                    diagnostic_codes::A_FUNCTION_WHOSE_DECLARED_TYPE_IS_NEITHER_UNDEFINED_VOID_NOR_ANY_MUST_RETURN_A_V,
                );
            } else if self.ctx.no_implicit_returns()
                && has_return
                && falls_through
                && !self.should_skip_no_implicit_return_check(
                    check_return_type,
                    has_type_annotation,
                    is_generator,
                )
            {
                // TS7030: noImplicitReturns - not all code paths return a value
                // TSC points TS7030 to: return type annotation > method name > node itself
                use crate::diagnostics::diagnostic_messages;
                let error_node = if method.type_annotation.is_some() {
                    method.type_annotation
                } else if method.name.is_some() {
                    method.name
                } else {
                    member_idx
                };
                self.error_at_node(
                    error_node,
                    diagnostic_messages::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                    diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                );
            }

            // TS7030 for each bare `return;`, independent of the fall-off-the-end
            // check above (both can fire in one method). For a generator the
            // check type is its `TReturn` (tsc's `unwrapReturnType`); the shared
            // `check_return_type` re-unwraps an already-unwrapped inferred
            // `TReturn` and falls back to `unknown`, firing spuriously (#17444).
            let bare_return_check_type = if is_generator {
                self.generator_bare_return_check_type(
                    generator_return_completeness,
                    return_type,
                    has_type_annotation,
                )
            } else {
                check_return_type
            };
            self.report_no_implicit_return_bare_returns(
                method.body,
                bare_return_check_type,
                has_type_annotation,
                is_generator,
            );

            self.ctx.pop_yield_type();
            self.pop_return_type();
        } else {
            // Abstract method or method overload signature
            // Report TS7010 for abstract methods without return type annotation
            // Async methods infer Promise<void>, not 'any', so they should NOT trigger TS7010
            // Private members in ambient classes are excluded (not visible in .d.ts)
            if !is_async && !skip_implicit_any {
                let method_name = self.member_name_for_diagnostic(method.name);
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

        if pushed_this_type {
            self.ctx.this_type_stack.pop();
            self.ctx.function_owned_this_stack.pop();
        }

        self.pop_type_parameters(jsdoc_type_param_updates);
        self.pop_type_parameters(type_param_updates);
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
    #[expect(dead_code)]
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

        self.check_modifier_combinations(&accessor.modifiers, accessor.name, node.kind);

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

        // Error 1318: An abstract accessor cannot have an implementation.
        // Abstract accessors must not have a body. tsc's `checkGrammarAccessor`
        // reports this through `grammarErrorOnNode(node.name, …)`, so it anchors
        // at the accessor name (`get aa`'s `aa`), not the whole member node's
        // leading `abstract` modifier.
        if accessor.body.is_some() && self.has_abstract_modifier(&accessor.modifiers) {
            self.error_at_node(
                accessor.name,
                "An abstract accessor cannot have an implementation.",
                diagnostic_codes::AN_ABSTRACT_ACCESSOR_CANNOT_HAVE_AN_IMPLEMENTATION,
            );
        }

        // TS1005: a non-ambient, non-abstract accessor must have a `{` brace body.
        // tsc's `checkGrammarAccessor` reports this at CHECK time
        // (`grammarErrorAtPos(accessor, accessor.end - 1, 1, …)`), so — unlike a
        // parse-time error — it coexists with the class's semantic diagnostics
        // instead of tripping the program-wide `has_parse_errors` suppression.
        // The parser deliberately defers the body-less accessor here (see
        // `parse_accessor_body`); this is that deferred check.
        //
        // tsc's `checkGrammarModifiers(node) || checkGrammarAccessor(node)`
        // OR-chain means a modifier already invalid on this accessor —
        // `readonly` (TS1024), `in`/`out` (TS1274), or a duplicate `accessor`
        // (TS1275) — reports its own error and short-circuits the missing-body
        // check entirely, rather than piling a second diagnostic on the same
        // malformed member (#17062). `declare` (TS1031) already gets this for
        // free because a `declare`-modified node reads as ambient regardless
        // of placement validity (`is_in_ambient_context`).
        let accessor_end = node.end;
        let accessor_body_missing = accessor.body.is_none();
        let accessor_is_abstract = self.has_abstract_modifier(&accessor.modifiers);
        let accessor_has_modifier_invalid_on_accessor =
            accessor.modifiers.as_ref().is_some_and(|mods| {
                mods.nodes.iter().any(|&mod_idx| {
                    self.ctx.arena.get(mod_idx).is_some_and(|mod_node| {
                        matches!(
                            tsz_scanner::SyntaxKind::try_from_u16(mod_node.kind),
                            Some(
                                tsz_scanner::SyntaxKind::ReadonlyKeyword
                                    | tsz_scanner::SyntaxKind::InKeyword
                                    | tsz_scanner::SyntaxKind::OutKeyword
                                    | tsz_scanner::SyntaxKind::AccessorKeyword
                            )
                        )
                    })
                })
            });
        if accessor_body_missing
            && !accessor_is_abstract
            && !accessor_has_modifier_invalid_on_accessor
            && !self.ctx.is_declaration_file()
            && !self.ctx.arena.is_in_ambient_context(member_idx)
        {
            self.error(
                accessor_end.saturating_sub(1),
                1,
                "'{' expected.".to_string(),
                diagnostic_codes::EXPECTED,
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

        // A `set` accessor's parameters follow the same parameter-name grammar as
        // every other function-like: `"use strict"` / non-simple-parameter-list
        // (TS1346/TS1347), strict-mode reserved words (TS1212/TS1213/TS1214) and
        // `eval`/`arguments` (TS1100/TS1210/TS1215). Set-accessor parameters route
        // through this accessor path rather than the shared per-function-like param
        // check, so wire the whole check in here — not just the `"use strict"` half.
        // `check_strict_mode_reserved_parameter_names` calls
        // `check_use_strict_non_simple_parameter_list` itself, so it subsumes the
        // previous call rather than duplicating it.
        if node.kind == syntax_kind_ext::SET_ACCESSOR {
            self.check_strict_mode_reserved_parameter_names(
                &accessor.parameters.nodes,
                member_idx,
                self.ctx.enclosing_class.is_some(),
            );
        }

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

        // Check that parameter default values are assignable to declared types
        // (TS2322). Accessors can never carry the `async` modifier.
        self.check_parameter_initializers(&accessor.parameters.nodes, false);

        // Check for parameter properties (error 2369)
        // Parameter properties are only allowed in constructors, not in accessors
        self.check_parameter_properties(&accessor.parameters.nodes);

        // TSC suppresses the noImplicitAny member family (TS7006/TS7010/TS7033) for
        // accessors that are not part of an ambient declaration's observable surface.
        let skip_implicit_any_accessor =
            self.member_hidden_from_ambient_declaration_surface(&accessor.modifiers, accessor.name);

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
            let paired_getter_supplies_type =
                self.paired_getter_supplies_property_type(accessor) || skip_implicit_any_accessor;
            self.check_setter_parameter_grammar(member_idx);
            self.check_setter_parameter(
                &accessor.parameters.nodes,
                has_paired_getter || skip_implicit_any_accessor,
                paired_getter_supplies_type,
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
                    let accessor_name = self.member_name_for_diagnostic(accessor.name);
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
            // Exception: an unannotated getter paired with a setter whose parameter
            // IS annotated gets that annotation as its contextual return type (tsc's
            // `isGetAccessorWithAnnotatedSetAccessor` -> `getContextualReturnType`).
            // That type comes from a sibling declaration, not the getter's own
            // inferred type, so it is not self-referential and is safe to check
            // return statements against like a real annotation.
            let paired_setter_type = if is_getter && !has_type_annotation {
                self.contextual_getter_return_type_for_class_accessor(accessor)
            } else {
                None
            };
            let effective_return_type = if has_type_annotation {
                return_type
            } else if let Some(paired_setter_type) = paired_setter_type {
                paired_setter_type
            } else {
                TypeId::ANY
            };
            self.push_return_type(effective_return_type);

            let body_request = request.read().contextual_opt(None);
            self.clear_type_cache_recursive(accessor.body);
            let saved_member_body_depth = self.ctx.enter_class_member_body();
            self.check_statement_with_request(accessor.body, &body_request);
            self.ctx.exit_class_member_body(saved_member_body_depth);
            if is_getter {
                // Check if this is an async getter
                let is_async = self.has_async_modifier(&accessor.modifiers);
                // An unannotated getter paired with an annotated setter is checked
                // for completeness (TS2355/TS2366) against the setter's inherited
                // type exactly as if it were the getter's own annotation — tsc's
                // `isGetAccessorWithAnnotatedSetAccessor` treats that inherited
                // type as the getter's effective declared type for every purpose,
                // not just contextual typing of its `return` statements.
                let has_declared_or_inherited_return_type =
                    has_type_annotation || paired_setter_type.is_some();
                let completeness_return_type = if has_type_annotation {
                    return_type
                } else if let Some(paired_setter_type) = paired_setter_type {
                    paired_setter_type
                } else {
                    return_type
                };
                // For async getters, extract the inner type from Promise<T>
                let mut check_return_type = self.return_type_for_implicit_return_check(
                    completeness_return_type,
                    is_async,
                    false, // getters cannot be generators
                );
                if is_async
                    && check_return_type == completeness_return_type
                    && has_type_annotation
                    && self.return_type_annotation_is_exactly_promise(accessor.type_annotation)
                {
                    check_return_type = TypeId::VOID;
                }
                let requires_return = self.requires_return_value(check_return_type);
                let has_return = self.body_has_return_with_value(accessor.body);
                let falls_through = self.function_body_falls_through(accessor.body);

                // TS2378: A 'get' accessor must return a value (regardless of type annotation)
                // Get accessors ALWAYS require a return value, even without type annotation.
                // tsc computes this from binder-set HasImplicitReturn/HasExplicitReturn flags
                // in `checkAccessorDeclarationDiagnostics`, entirely independently of the
                // code-path completeness check below (`checkAllCodePathsInNonVoidFunctionReturnOrThrow`).
                // The two are separate passes and are not mutually exclusive.
                if !has_return && falls_through {
                    self.error_at_node(
                        accessor.name,
                        "A 'get' accessor must return a value.",
                        diagnostic_codes::A_GET_ACCESSOR_MUST_RETURN_A_VALUE,
                    );
                }
                if has_declared_or_inherited_return_type && requires_return && falls_through {
                    // A getter that inherits its effective type from a paired
                    // setter has no annotation node of its own to anchor on;
                    // tsc blames the getter's name instead (matching TS7033 /
                    // "must return a value" above).
                    let error_node = if has_type_annotation {
                        accessor.type_annotation
                    } else {
                        accessor.name
                    };
                    if !has_return {
                        // TS2355: no return statements at all.
                        self.error_at_node(
                            error_node,
                            "A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.",
                            diagnostic_codes::A_FUNCTION_WHOSE_DECLARED_TYPE_IS_NEITHER_UNDEFINED_VOID_NOR_ANY_MUST_RETURN_A_V,
                        );
                    } else {
                        // TS2366: some return statements exist, but not every path returns.
                        use crate::diagnostics::diagnostic_messages;
                        self.error_at_node(
                            error_node,
                            diagnostic_messages::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                            diagnostic_codes::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                        );
                    }
                } else if self.ctx.no_implicit_returns()
                    && has_return
                    && falls_through
                    && !self.should_skip_no_implicit_return_check(
                        check_return_type,
                        has_type_annotation,
                        false, // accessors cannot be generators
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
                        member_idx
                    };
                    self.error_at_node(
                        error_node,
                        diagnostic_messages::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                        diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                    );
                }

                // TS7030 for each bare `return;`, independent of the
                // fall-off-the-end check above (both can fire in one getter).
                self.report_no_implicit_return_bare_returns(
                    accessor.body,
                    check_return_type,
                    has_type_annotation,
                    false, // accessors cannot be generators
                );
            }

            self.pop_return_type();
        } else if is_getter && !has_type_annotation && !skip_implicit_any_accessor {
            // TS7033. A bodyless get accessor has no body to infer a return type from,
            // so an unannotated one resolves to `any` and tsc reports it — in every
            // container a bodyless getter can legally appear (`declare class`, an
            // `abstract` getter, a `.d.ts` class) and in parse-error recovery for a
            // plain class too, where tsc emits it alongside the syntax error.
            //
            // This is the accessor analogue of the bodyless-method TS7010 arm above,
            // and it carries one exemption that arm does not need: a get/set pair
            // shares a single property type, and when it is missing tsc blames the
            // *setter* (TS7032 on the setter name), never the getter. So ANY paired
            // setter takes the getter out of TS7033 — not just an annotated one.
            // An annotated setter supplies the type outright
            // (tsc's `isGetAccessorWithAnnotatedSetAccessor`); an unannotated one
            // becomes the blame site itself and is reported there by
            // `check_setter_parameter`. Either way the getter stays clean.
            // The member is named through `member_name_for_diagnostic`, which
            // picks the renderer by the name node's kind so a computed name
            // keeps its brackets whether or not `get_property_name` can resolve
            // it to a key, and a string-literal name keeps its source quote
            // character. Deliberately not `property_name_for_error`: its
            // further fallback to a raw source-text slice would also fire on a
            // genuinely malformed computed name (`get [](); `, a parse error),
            // where tsc reports only the syntax error and stays silent on
            // `TS7033`.
            if self.ctx.no_implicit_any()
                && !self.is_js_file()
                && self.paired_setter_member_for_getter(accessor).is_none()
                && let Some(accessor_name) = self.member_name_for_diagnostic(accessor.name)
            {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node_msg(
                    accessor.name,
                    diagnostic_codes::PROPERTY_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_ITS_GET_ACCESSOR_LACKS_A_RETURN_TYPE_AN,
                    &[&accessor_name],
                );
            }
        }

        if self.has_static_modifier(&accessor.modifiers) {
            self.check_static_member_for_class_type_param_refs(member_idx);
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
