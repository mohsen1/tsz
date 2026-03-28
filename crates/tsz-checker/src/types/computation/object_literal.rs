//! Object literal type computation.
//!
//! Handles typing of object literal expressions including property assignments,
//! shorthand properties, method shorthands, getters/setters, spread properties,
//! duplicate property detection, and contextual type inference.

use super::object_literal_context::ContextualPropertyPresence;
use crate::context::TypingRequest;
use crate::context::speculation::DiagnosticSpeculationGuard;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{TypeId, Visibility};

impl<'a> CheckerState<'a> {
    fn is_object_define_property_descriptor_literal(&self, idx: NodeIndex) -> bool {
        use tsz_scanner::SyntaxKind;

        let Some(parent_idx) = self.ctx.arena.get_extended(idx).map(|ext| ext.parent) else {
            return false;
        };
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        if parent_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }
        let Some(call) = self.ctx.arena.get_call_expr(parent_node) else {
            return false;
        };
        let Some(args) = call.arguments.as_ref() else {
            return false;
        };
        if args.nodes.len() < 3 || args.nodes[2] != idx {
            return false;
        }

        let Some(callee_node) = self.ctx.arena.get(call.expression) else {
            return false;
        };
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(callee_access) = self.ctx.arena.get_access_expr(callee_node) else {
            return false;
        };
        let Some(object_node) = self.ctx.arena.get(callee_access.expression) else {
            return false;
        };
        if object_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(object_ident) = self.ctx.arena.get_identifier(object_node) else {
            return false;
        };
        if object_ident.escaped_text != "Object" {
            return false;
        }
        let Some(name_node) = self.ctx.arena.get(callee_access.name_or_argument) else {
            return false;
        };
        let Some(name_ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        name_ident.escaped_text == "defineProperty"
    }

    fn define_property_descriptor_accessor_type(
        &mut self,
        object_literal_idx: NodeIndex,
        elements: &[NodeIndex],
        method_name: &str,
    ) -> Option<TypeId> {
        if !self.is_object_define_property_descriptor_literal(object_literal_idx) {
            return None;
        }

        for &element_idx in elements {
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };

            if let Some(method) = self.ctx.arena.get_method_decl(element_node)
                && self
                    .get_property_name_resolved(method.name)
                    .is_some_and(|name| name == method_name)
            {
                let method_type = self.get_type_of_function(element_idx);
                return crate::query_boundaries::assignability::get_function_return_type(
                    self.ctx.types,
                    method_type,
                );
            }

            if method_name == "get"
                && element_node.kind == syntax_kind_ext::GET_ACCESSOR
                && let Some(accessor) = self.ctx.arena.get_accessor(element_node)
                && self
                    .get_property_name_resolved(accessor.name)
                    .is_some_and(|name| name == method_name)
            {
                self.get_type_of_function(element_idx);
                return Some(if accessor.type_annotation.is_none() {
                    self.infer_getter_return_type(accessor.body)
                } else {
                    self.get_type_from_type_node(accessor.type_annotation)
                });
            }
        }

        None
    }

    fn define_property_descriptor_setter_context_type(
        &mut self,
        object_literal_idx: NodeIndex,
        elements: &[NodeIndex],
    ) -> Option<TypeId> {
        let getter_type =
            self.define_property_descriptor_accessor_type(object_literal_idx, elements, "get")?;
        Some(
            self.ctx
                .types
                .factory()
                .function(tsz_solver::FunctionShape::new(
                    vec![tsz_solver::ParamInfo::unnamed(getter_type)],
                    TypeId::VOID,
                )),
        )
    }

    /// Get the type of an object literal expression.
    ///
    /// Computes the type of object literals like `{ x: 1, y: 2 }` or `{ foo, bar }`.
    /// Handles:
    /// - Property assignments: `{ x: value }`
    /// - Shorthand properties: `{ x }`
    /// - Method shorthands: `{ foo() {} }`
    /// - Getters/setters: `{ get foo() {}, set foo(v) {} }`
    /// - Spread properties: `{ ...obj }`
    /// - Duplicate property detection
    /// - Contextual type inference
    /// - Implicit any reporting (TS7008)
    #[allow(dead_code)]
    pub(crate) fn get_type_of_object_literal(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_object_literal_with_request(idx, &TypingRequest::NONE)
    }

    pub(crate) fn get_type_of_object_literal_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use rustc_hash::FxHashMap;
        use tsz_common::interner::Atom;
        use tsz_solver::PropertyInfo;
        let mut contextual_type = request.contextual_type;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(obj) = self.ctx.arena.get_literal_expr(node) else {
            return TypeId::ERROR; // Missing object literal data - propagate error
        };

        tracing::trace!(
            idx = idx.0,
            contextual_type = ?contextual_type.map(|t| t.0),
            contextual_type_display = ?contextual_type.map(|t| self.format_type(t)),
            "get_type_of_object_literal: entry"
        );

        // Collect properties from the object literal (later entries override earlier ones)
        let mut properties: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();
        // Track pre-widened (display) types for freshness model.
        // Maps property name → original literal TypeId before widening.
        // Only populated when a property's type was actually widened.
        let mut display_type_overrides: FxHashMap<Atom, TypeId> = FxHashMap::default();
        let mut string_index_types: Vec<TypeId> = Vec::new();
        let mut number_index_types: Vec<TypeId> = Vec::new();
        // Index signatures inherited from spread sources (kept separate because
        // they should only be included when the literal has no explicit properties —
        // tsc drops spread index signatures when explicit properties exist).
        let mut spread_string_index_types: Vec<TypeId> = Vec::new();
        let mut spread_number_index_types: Vec<TypeId> = Vec::new();
        let mut has_spread = false;
        let mut has_union_spread = false;
        let mut union_spread_branches: Vec<FxHashMap<Atom, PropertyInfo>> = Vec::new();
        // Track type-parameter-containing spread types for intersection creation.
        // When a type parameter (or type containing type parameters) is spread
        // alongside other properties, we create an intersection of the type parameter
        // with the explicit properties, preserving generic identity for instantiation.
        let mut generic_spread_types: Vec<TypeId> = Vec::new();
        // Track getter/setter names to allow getter+setter pairs with the same name
        let mut getter_names: rustc_hash::FxHashSet<Atom> = rustc_hash::FxHashSet::default();
        let mut setter_names: rustc_hash::FxHashSet<Atom> = rustc_hash::FxHashSet::default();
        let mut explicit_property_names: rustc_hash::FxHashSet<Atom> =
            rustc_hash::FxHashSet::default();
        // Track which named properties came from explicit assignments (not spreads)
        // so we can emit TS2783 when a later spread overwrites them.
        // Maps property name atom -> (node_idx for error, property display name)
        let mut named_property_nodes: FxHashMap<Atom, (NodeIndex, String)> = FxHashMap::default();

        // Skip duplicate property checks for destructuring assignment targets.
        // `({ x, y: y1, "y": y1 } = obj)` is valid - same property extracted twice.
        let skip_duplicate_check = self.ctx.in_destructuring_target;
        let mut prop_order: u32 = 1;

        // Check for ThisType<T> marker in contextual type (Vue 2 / Options API pattern)
        // We need to extract this BEFORE the for loop so it's available for the pop at the end
        let marker_this_type: Option<TypeId> = if let Some(ctx_type) = contextual_type {
            use tsz_solver::ContextualTypeContext;
            let ctx_helper = ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                ctx_type,
                self.ctx.compiler_options.no_implicit_any,
            );
            let env = self.ctx.type_env.borrow();
            ctx_helper.get_this_type_from_marker_with_resolver(&*env)
        } else {
            None
        };

        // Push this type onto stack if found (methods will pick it up)
        if let Some(mut this_type) = marker_this_type {
            // The ThisType<T> marker may contain unresolved type parameters
            // (e.g., `Data & Readonly<Props> & Instance` before inference completes).
            // Evaluate through the type environment to resolve them to their
            // inferred concrete types. Without this, property access on `this`
            // inside method bodies would fail to find properties of the inferred
            // type, producing false TS2322 errors.
            if tsz_solver::type_queries::contains_type_parameters_db(self.ctx.types, this_type) {
                this_type = self.evaluate_type_with_env(this_type);
            }
            self.ctx.this_type_stack.push(this_type);
        }

        // Pre-scan: collect ALL method names from the object literal so that
        // the synthetic `this` type includes placeholders for all methods,
        // enabling mutually-recursive methods to resolve `this.otherMethod`.
        // Maps method name atom → element node index so we can extract annotated
        // parameter/return types when building placeholders for not-yet-processed methods.
        let obj_all_method_names: rustc_hash::FxHashMap<Atom, (NodeIndex, u32)> = obj
            .elements
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(pos, &elem_idx)| {
                let elem_node = self.ctx.arena.get(elem_idx)?;
                let method = self.ctx.arena.get_method_decl(elem_node)?;
                let name = self.get_property_name(method.name)?;
                Some((
                    self.ctx.types.intern_string(&name),
                    (elem_idx, (pos + 1) as u32),
                ))
            })
            .collect();

        // Pre-scan: collect getter property names so setter TS7006 checks can
        // detect paired getters regardless of declaration order.
        let obj_getter_names: rustc_hash::FxHashSet<String> = obj
            .elements
            .nodes
            .iter()
            .filter_map(|&elem_idx| {
                let elem_node = self.ctx.arena.get(elem_idx)?;
                if elem_node.kind != syntax_kind_ext::GET_ACCESSOR {
                    return None;
                }
                let accessor = self.ctx.arena.get_accessor(elem_node)?;
                self.get_property_name_resolved(accessor.name)
            })
            .collect();

        // Pre-scan: narrow union contextual type via discriminant properties.
        // When the contextual type is a union (e.g. `A | B`) and the object literal
        // has literal-valued properties that discriminate the union, narrow to the
        // matching member(s) so other properties get precise contextual types.
        // Save original for TS7006 checks (must use pre-narrowed union to detect
        // primitive members like `string` in `string | FullRule`).
        let original_contextual_type = contextual_type;
        if let Some(ctx_type) = contextual_type {
            let narrowed = self.narrow_contextual_union_via_object_literal_discriminants(
                ctx_type,
                &obj.elements.nodes,
            );
            if narrowed != ctx_type {
                contextual_type = Some(narrowed);
            }
        }
        let contextual_receiver_this_type =
            self.contextual_object_receiver_this_type(contextual_type, marker_this_type);
        let base_request = request.contextual_opt(contextual_type);

        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Property assignment: { x: value }
            if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                if let Some(prop_name_node) = self.ctx.arena.get(prop.name)
                    && prop_name_node.kind
                        == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME
                {
                    // Always run TS2464 validation for computed property names, even when
                    // the name can be resolved to a literal atom.
                    self.check_computed_property_name(prop.name);
                }

                // When the computed key expression has error type (e.g., [Symbol.nonsense]),
                // treat the property as unnamed to avoid cascading errors. tsc drops
                // error-typed computed property keys from the object literal type.
                let computed_key_is_error = self.ctx.arena.get(prop.name).is_some_and(|n| {
                    n.kind == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME
                }) && self
                    .ctx
                    .arena
                    .get(prop.name)
                    .and_then(|n| self.ctx.arena.get_computed_property(n))
                    .is_some_and(|computed| {
                        self.get_type_of_node(computed.expression) == TypeId::ERROR
                    });
                let name_opt = if computed_key_is_error {
                    None
                } else {
                    self.get_property_name_resolved(prop.name)
                };
                if let Some(name) = name_opt.clone() {
                    let initializer_is_function_like = self
                        .ctx
                        .arena
                        .get(prop.initializer)
                        .is_some_and(|init_node| {
                            matches!(
                                init_node.kind,
                                syntax_kind_ext::ARROW_FUNCTION
                                    | syntax_kind_ext::FUNCTION_EXPRESSION
                            )
                        });
                    // JSDoc @type on object literal properties acts as the declared
                    // type for the property. When present:
                    // - The property type in the resulting object is the @type type
                    // - The initializer is checked for assignability against it
                    // - The @type type is used as contextual type so literals are preserved
                    // This matches tsc behavior for JS files with checkJs/ts-check.
                    let jsdoc_declared_type = self.jsdoc_type_annotation_for_node_direct(elem_idx);
                    let jsdoc_callable_context_type = initializer_is_function_like
                        .then(|| self.jsdoc_callable_type_annotation_for_node_direct(elem_idx))
                        .flatten();
                    // Get contextual type for this property.
                    // For mapped/conditional/application types that contain Lazy references
                    // (e.g. { [K in keyof Props]: Props[K] } after generic inference),
                    // evaluate them with the full resolver first so the solver can
                    // extract property types from the resulting concrete object type.
                    let property_context_type = if let Some(ctx_type) = contextual_type {
                        let lookup_type = self.contextual_lookup_type(ctx_type);
                        let lookup_presence =
                            self.named_contextual_property_presence(lookup_type, &name);
                        let allows_callable_fallback =
                            matches!(lookup_presence, ContextualPropertyPresence::Present);
                        let mut property_context_type =
                            self.contextual_object_property_type_for_lookup(ctx_type, &name);
                        // For optional properties (e.g., `set?` in ProxyHandler), the
                        // contextual type includes `undefined` from optionality. When
                        // the property IS present in the literal, that `undefined` is
                        // irrelevant — the property is being assigned, not read. Strip
                        // it so the callable type flows through for generic inference
                        // (e.g., `deprecate<T extends Function>(fn: T): T` should
                        // infer T as the handler type, not fall back to `Function`).
                        if let Some(pct) = property_context_type {
                            let stripped = tsz_solver::remove_undefined(self.ctx.types, pct);
                            if stripped != TypeId::UNDEFINED && stripped != pct {
                                property_context_type = Some(stripped);
                            }
                        }
                        if initializer_is_function_like
                            && property_context_type.is_none()
                            && !allows_callable_fallback
                        {
                            property_context_type = Some(TypeId::NEVER);
                        }
                        let needs_callable_fallback = property_context_type.is_none()
                            || matches!(property_context_type, Some(TypeId::ANY | TypeId::UNKNOWN));
                        if allows_callable_fallback
                            && needs_callable_fallback
                            && initializer_is_function_like
                        {
                            self.contextual_callable_property_fallback_for_lookup(
                                ctx_type,
                                property_context_type,
                            )
                        } else {
                            property_context_type
                        }
                    } else {
                        None
                    };
                    let initializer_context_type =
                        if let Some(jsdoc_callable_context_type) = jsdoc_callable_context_type {
                            Some(jsdoc_callable_context_type)
                        } else if jsdoc_declared_type.is_none() {
                            self.function_initializer_context_type(
                                contextual_type,
                                &name,
                                property_context_type,
                                prop.initializer,
                            )
                        } else if initializer_is_function_like {
                            None
                        } else {
                            jsdoc_declared_type
                        };
                    // Set contextual type for property value.
                    // When a JSDoc @type is present, use it as the contextual type
                    // so that literal values like `"a"` preserve their literal type
                    // (e.g., `@type {"a"}` + `a: "a"` should not widen to `string`).
                    let had_object_context = contextual_type.is_some();
                    // When the outer contextual type is a union with a non-nullish
                    // non-object member (e.g. `string | FullRule`), tsc does not
                    // provide a contextual type for function-like property initializers.
                    // `function_initializer_context_type` returns `None` to signal this,
                    // but the `property_context_type` fallback would re-introduce the
                    // contextual type (suppressing the intended TS7006 on the parameter).
                    // Skip the fallback in that case.
                    let suppress_function_ctx = jsdoc_declared_type.is_none()
                        && initializer_context_type.is_none()
                        && initializer_is_function_like
                        && original_contextual_type.is_some_and(|ctx_type| {
                            self.contextual_type_has_primitive_union_member(ctx_type)
                        });
                    let resolved_prop_ctx = self.substitute_contextual_this_type(
                        jsdoc_declared_type.or(initializer_context_type).or(
                            if suppress_function_ctx {
                                None
                            } else {
                                property_context_type
                            },
                        ),
                        contextual_receiver_this_type,
                    );
                    let property_request = base_request.contextual_opt(
                        self.contextual_type_option_for_expression(resolved_prop_ctx)
                            .or_else(|| {
                                // When the outer contextual type is UNKNOWN (e.g., from a
                                // generic JSX component's spread attribute), preserve UNKNOWN
                                // as the contextual type for function-like initializers. This
                                // prevents false TS7006 emissions on callback parameters
                                // inside object literals spread into generic JSX components.
                                if contextual_type == Some(TypeId::UNKNOWN)
                                    && let Some(init_node) = self.ctx.arena.get(prop.initializer)
                                    && matches!(
                                        init_node.kind,
                                        syntax_kind_ext::ARROW_FUNCTION
                                            | syntax_kind_ext::FUNCTION_EXPRESSION
                                    )
                                {
                                    return Some(TypeId::UNKNOWN);
                                }
                                None
                            }),
                    );
                    // When the parser can't parse a value expression (e.g. `{ a: return; }`),
                    // it uses the property NAME node as the fallback initializer for error
                    // recovery (prop.initializer == prop.name). Skip type-checking in that
                    // case to prevent a spurious TS2304 for the property name identifier.
                    let value_type = if prop.initializer == prop.name {
                        TypeId::ANY
                    } else if self.ctx.in_destructuring_target {
                        self.destructuring_target_type_from_initializer(prop.initializer)
                    } else {
                        if initializer_is_function_like
                            && property_request.contextual_type == Some(TypeId::NEVER)
                            && property_request.is_empty()
                        {
                            self.ctx
                                .implicit_any_contextual_closures
                                .remove(&prop.initializer);
                            self.ctx
                                .implicit_any_checked_closures
                                .remove(&prop.initializer);
                            self.invalidate_initializer_for_context_change(prop.initializer);
                        }
                        // For function expression property initializers (not arrow functions),
                        // push a synthetic `this` type so that `this` inside the function body
                        // resolves to the object literal's type rather than `any`.
                        // Arrow functions inherit `this` from the enclosing scope, so they
                        // must NOT get a synthetic `this` push.
                        let initializer_is_function_expression = self
                            .ctx
                            .arena
                            .get(prop.initializer)
                            .is_some_and(|init_node| {
                                init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                            });
                        let mut pushed_prop_fn_this = false;
                        if initializer_is_function_expression
                            && marker_this_type.is_none()
                            && self.current_this_type().is_none()
                        {
                            if let Some(ctx_type) = contextual_type {
                                let ctx_type = self.evaluate_contextual_type(ctx_type);
                                self.ctx.this_type_stack.push(ctx_type);
                                pushed_prop_fn_this = true;
                            } else {
                                let synthetic_this_type = self
                                    .build_object_literal_fn_property_synthetic_this_type(
                                        &properties,
                                        &obj_all_method_names,
                                        &name,
                                    );
                                self.ctx.this_type_stack.push(synthetic_this_type);
                                pushed_prop_fn_this = true;
                            }
                        }

                        let pre_refresh_snap = self.ctx.snapshot_diagnostics();
                        let value_type =
                            self.get_type_of_node_with_request(prop.initializer, &property_request);
                        if initializer_is_function_like
                            && property_request.contextual_type == Some(TypeId::NEVER)
                        {
                            self.ctx
                                .implicit_any_contextual_closures
                                .remove(&prop.initializer);
                            self.ctx
                                .implicit_any_checked_closures
                                .remove(&prop.initializer);
                        }
                        if self.request_has_concrete_contextual_type(&property_request)
                            && property_request.contextual_type != Some(TypeId::NEVER)
                        {
                            let spans = self.function_like_param_spans_for_node(prop.initializer);
                            self.clear_stale_function_like_implicit_any_diagnostics(
                                &spans,
                                &pre_refresh_snap,
                            );
                        }

                        if pushed_prop_fn_this {
                            self.ctx.this_type_stack.pop();
                        }

                        value_type
                    };

                    // TS2779: The left-hand side of an assignment expression may not be
                    // an optional property access. Applies to destructuring targets like
                    // `{ a: obj?.a } = source` where obj?.a is the assignment target.
                    if self.ctx.in_destructuring_target
                        && prop.initializer != prop.name
                        && self.is_optional_chain_access(prop.initializer)
                    {
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                        self.error_at_node(
                            prop.initializer,
                            diagnostic_messages::THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_A,
                            diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_A,
                        );
                    }

                    // When a JSDoc @type annotation is present, check assignability
                    // of the initializer against the declared type, and use the
                    // declared type as the property type (not the initializer type).
                    let value_type = if let Some(declared_type) = jsdoc_declared_type {
                        // Check initializer assignability against @type (TS2322)
                        if prop.initializer != prop.name
                            && !self.is_assignable_to(value_type, declared_type)
                        {
                            let declared_check_type =
                                tsz_solver::remove_undefined(self.ctx.types, declared_type);
                            self.check_assignable_or_report_at_exact_anchor(
                                value_type,
                                declared_check_type,
                                prop.initializer,
                                prop.name,
                            );
                        }
                        declared_type
                    } else {
                        // Apply bidirectional type inference - use contextual type to narrow the value type
                        let value_type = tsz_solver::apply_contextual_type(
                            self.ctx.types,
                            value_type,
                            property_context_type,
                        );

                        // Widen literal types for object literal properties.
                        // Object literal properties are mutable by default, so `{ x: "a" }`
                        // produces `{ x: string }`. Only preserve literals when:
                        // - A const assertion is active (`as const`)
                        // - A contextual type narrows the property to a literal
                        // - The value has a type assertion (`as T` or `<T>expr`):
                        //   tsc creates non-widening literal types from type assertions,
                        //   so `{ value: 0 as 0 }` produces `{ value: 0 }`, not `{ value: number }`.
                        let value_has_type_assertion =
                            self.ctx.arena.get(prop.initializer).is_some_and(|n| {
                                n.kind == syntax_kind_ext::AS_EXPRESSION
                                    || n.kind == syntax_kind_ext::TYPE_ASSERTION
                            });
                        let final_type = if !self.ctx.in_const_assertion
                            && !self.ctx.preserve_literal_types
                            && property_context_type.is_none()
                            && !had_object_context
                            && !value_has_type_assertion
                        {
                            self.widen_literal_type(value_type)
                        } else {
                            value_type
                        };

                        // Freshness model: always record literal property values
                        // from the AST for display in error messages. Store even
                        // when lit_type == final_type — inference-time widening
                        // may change the property type later, and we need the
                        // original literal for error display.
                        if prop.initializer != prop.name
                            && let Some(lit_type) =
                                self.literal_type_from_initializer(prop.initializer)
                        {
                            let name_atom = self.ctx.types.intern_string(&name);
                            display_type_overrides.insert(name_atom, lit_type);
                        }

                        final_type
                    };

                    // Note: TS7008 is NOT emitted for object literal properties.
                    // tsc only emits TS7008 for class properties, property signatures,
                    // auto-accessors, and binary expressions.
                    // However, TS7018 IS emitted for object literal properties when
                    // noImplicitAny is on and the property implicitly has 'any' type.
                    // This happens when:
                    // - The value is `null` or `undefined` with strictNullChecks off (widens to any)
                    // - The value has type `any` without a contextual/declared type
                    // tsc suppresses TS7018 when the object literal has ANY contextual
                    // type (e.g. from a type assertion, parameter type, variable
                    // declaration), even if the specific property doesn't exist in that
                    // contextual type. The contextual type signals developer intent.
                    // tsc also suppresses TS7018 for object literals used as parameter
                    // default values (e.g. `function f({b} = { b: null })`), because
                    // the implicit-any is reported via TS7006/TS7031 on the binding
                    // elements instead.
                    let is_parameter_default = self.ctx.arena.node_info(idx).is_some_and(|info| {
                        self.ctx.arena.get(info.parent).is_some_and(|parent_node| {
                            parent_node.kind == syntax_kind_ext::PARAMETER
                        })
                    });
                    if self.ctx.no_implicit_any()
                        && !self.ctx.in_destructuring_target
                        && !is_parameter_default
                        && jsdoc_declared_type.is_none()
                        && contextual_type.is_none()
                        && prop.initializer != prop.name
                    {
                        // TS7018 only fires for IMPLICIT any — when null/undefined
                        // widens to any without strictNullChecks. When the initializer
                        // expression evaluates to explicit `any` (from an `any` variable,
                        // function returning `any`, etc.), tsc does NOT emit TS7018.
                        let is_implicit_any = !self.ctx.strict_null_checks()
                            && (value_type == TypeId::NULL || value_type == TypeId::UNDEFINED);
                        if is_implicit_any {
                            use crate::diagnostics::diagnostic_codes;
                            self.error_at_node_msg(
                                prop.name,
                                diagnostic_codes::OBJECT_LITERALS_PROPERTY_IMPLICITLY_HAS_AN_TYPE,
                                &[&name, "any"],
                            );
                        }
                    }

                    let name_atom = self.ctx.types.intern_string(&name);

                    // Check for duplicate property (skip in destructuring targets).
                    // TS1117: duplicate properties are an error in object literals.
                    if !skip_duplicate_check
                        && explicit_property_names.contains(&name_atom)
                        && !self.ctx.has_parse_errors
                        && (!self.is_js_file() || self.ctx.js_strict_mode_diagnostics_enabled())
                    {
                        let message = format_message(
                            diagnostic_messages::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                            &[&name],
                        );
                        self.error_at_node(
                            prop.name,
                            &message,
                            diagnostic_codes::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                        );
                    }
                    explicit_property_names.insert(name_atom);

                    // Track this named property for TS2783 spread-overwrite checking
                    named_property_nodes.insert(name_atom, (prop.name, name.clone()));

                    // In destructuring assignment targets, a property with a default
                    // value (e.g. `{ a: target = default } = source`) makes the property
                    // optional in the source type.  The parser represents
                    // `target = default` as a BinaryExpression with EqualsToken.
                    let is_optional_destructuring = self.ctx.in_destructuring_target
                        && self
                            .ctx
                            .arena
                            .get(prop.initializer)
                            .is_some_and(|init_node| {
                                init_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                                    && self.ctx.arena.get_binary_expr(init_node).is_some_and(
                                        |bin| {
                                            bin.operator_token
                                                == tsz_scanner::SyntaxKind::EqualsToken as u16
                                        },
                                    )
                            });

                    let order = prop_order;
                    prop_order += 1;
                    properties.insert(
                        name_atom,
                        PropertyInfo {
                            name: name_atom,
                            type_id: value_type,
                            write_type: value_type,
                            optional: is_optional_destructuring,
                            readonly: false,
                            is_method: false,
                            is_class_prototype: false,
                            visibility: Visibility::Public,
                            parent_id: None,
                            declaration_order: order,
                        },
                    );
                } else {
                    // Computed property name that can't be statically resolved (e.g., { [expr]: value })
                    // Still type-check the computed expression and the value to catch errors like TS2304.
                    // For contextual typing, use the index signature type from the contextual type.
                    // E.g., `var o: { [s: string]: (x: string) => number } = { ["" + 0](y) { ... } }`
                    // should contextually type `y` as `string` from the string index signature.
                    self.check_computed_property_name(prop.name);

                    let mut prop_name_type = TypeId::ANY;
                    let mut resolved_computed_name = None;
                    if let Some(prop_name_node) = self.ctx.arena.get(prop.name)
                        && prop_name_node.kind
                            == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME
                        && let Some(computed) = self.ctx.arena.get_computed_property(prop_name_node)
                    {
                        prop_name_type = self.get_type_of_node(computed.expression);
                        resolved_computed_name = self.get_property_name_resolved(prop.name);
                        let literal_computed_name =
                            crate::types_domain::queries::core::get_literal_property_name(
                                self.ctx.arena,
                                computed.expression,
                            );
                        let handled_by_name =
                            literal_computed_name.is_some() || resolved_computed_name.is_some();
                        if let Some(name) =
                            literal_computed_name.or_else(|| resolved_computed_name.clone())
                        {
                            let atom = self.ctx.types.intern_string(&name);
                            if !skip_duplicate_check
                                && explicit_property_names.contains(&atom)
                                && !self.ctx.has_parse_errors
                                && (!self.is_js_file()
                                    || self.ctx.js_strict_mode_diagnostics_enabled())
                            {
                                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                                let message = crate::diagnostics::format_message(
                                    diagnostic_messages::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                                    &[&name],
                                );
                                self.error_at_node(
                                    prop.name,
                                    &message,
                                    diagnostic_codes::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                                );
                            }
                            explicit_property_names.insert(atom);
                        }
                        if let Some(atom) =
                            crate::query_boundaries::type_computation::access::literal_property_name(
                                self.ctx.types,
                                prop_name_type,
                            )
                        {
                            if resolved_computed_name.is_none() {
                                resolved_computed_name =
                                    Some(self.ctx.types.resolve_atom(atom).to_string());
                            }
                            if !handled_by_name {
                                if !skip_duplicate_check
                                    && explicit_property_names.contains(&atom)
                                    && !self.ctx.has_parse_errors
                                    && (!self.is_js_file()
                                        || self.ctx.js_strict_mode_diagnostics_enabled())
                                {
                                    let name = self.ctx.types.resolve_atom(atom).to_string();
                                    use crate::diagnostics::{
                                        diagnostic_codes, diagnostic_messages,
                                    };
                                    let message = crate::diagnostics::format_message(
                                                diagnostic_messages::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                                                &[&name],
                                            );
                                    self.error_at_node(
                                                prop.name,
                                                &message,
                                                diagnostic_codes::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                                            );
                                }
                                explicit_property_names.insert(atom);
                            }
                        }
                    }
                    let index_ctx_type = if let Some(ctx_type) = contextual_type {
                        let property_context_type = self.contextual_object_literal_property_type(
                            ctx_type,
                            resolved_computed_name.as_deref().unwrap_or("__@computed"),
                        );
                        let needs_callable_fallback = property_context_type.is_none()
                            || matches!(property_context_type, Some(TypeId::ANY | TypeId::UNKNOWN));
                        if needs_callable_fallback
                            && let Some(init_node) = self.ctx.arena.get(prop.initializer)
                            && matches!(
                                init_node.kind,
                                syntax_kind_ext::ARROW_FUNCTION
                                    | syntax_kind_ext::FUNCTION_EXPRESSION
                            )
                        {
                            self.contextual_callable_property_fallback_type(
                                ctx_type,
                                property_context_type,
                            )
                        } else {
                            property_context_type
                        }
                    } else {
                        None
                    };
                    let property_request = base_request
                        .contextual_opt(self.contextual_type_option_for_expression(index_ctx_type));
                    let value_type =
                        self.get_type_of_node_with_request(prop.initializer, &property_request);

                    if self.is_assignable_to(prop_name_type, TypeId::NUMBER) {
                        number_index_types.push(value_type);
                    } else if self.is_assignable_to(prop_name_type, TypeId::STRING)
                        || self.is_assignable_to(prop_name_type, TypeId::ANY)
                    {
                        string_index_types.push(value_type);
                    }
                }
            }
            // Shorthand property: { x } - identifier is both name and value
            else if elem_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                if let Some(shorthand) = self.ctx.arena.get_shorthand_property(elem_node)
                    && let Some(name_node) = self.ctx.arena.get(shorthand.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                {
                    // TS1255: Definite assignment assertion '!' is not permitted in object literals
                    if shorthand.exclamation_token_pos > 0 {
                        self.error(
                            shorthand.exclamation_token_pos,
                            1,
                            "A definite assignment assertion '!' is not permitted in this context.".to_string(),
                            tsz_common::diagnostics::diagnostic_codes::A_DEFINITE_ASSIGNMENT_ASSERTION_IS_NOT_PERMITTED_IN_THIS_CONTEXT,
                        );
                    }

                    let name = ident.escaped_text.clone();
                    let shorthand_name_idx = shorthand.name;

                    // Get contextual type for this property
                    let property_context_type = if let Some(ctx_type) = contextual_type {
                        self.contextual_object_property_type_for_lookup(ctx_type, &name)
                    } else {
                        None
                    };
                    let jsdoc_declared_type = self.jsdoc_type_annotation_for_node_direct(elem_idx);

                    // Set contextual type for shorthand property value
                    let had_object_context = contextual_type.is_some();
                    let shorthand_request =
                        base_request.contextual_opt(self.contextual_type_option_for_expression(
                            jsdoc_declared_type.or(property_context_type),
                        ));
                    let shorthand_sym = if self.ctx.in_destructuring_target {
                        self.ctx
                            .binder
                            .resolve_identifier(self.ctx.arena, shorthand_name_idx)
                    } else {
                        self.resolve_identifier_symbol(shorthand_name_idx)
                    };

                    let value_type = if shorthand_sym.is_none() {
                        // Don't emit TS18004 for strict reserved words that require `:` syntax.
                        // Example: `{ class }` — parser already emits TS1005 "':' expected".
                        // Checker should not also emit TS18004 (cascading error).
                        //
                        // Only suppress for ECMAScript reserved words that ALWAYS require `:`
                        // in object literals. Be conservative — when in doubt, emit TS18004.
                        let is_strict_reserved = matches!(
                            name.as_str(),
                            "break"
                                | "case"
                                | "catch"
                                | "class"
                                | "const"
                                | "continue"
                                | "debugger"
                                | "default"
                                | "delete"
                                | "do"
                                | "else"
                                | "enum"
                                | "export"
                                | "extends"
                                | "finally"
                                | "for"
                                | "function"
                                | "if"
                                | "import"
                                | "in"
                                | "instanceof"
                                | "new"
                                | "return"
                                | "super"
                                | "switch"
                                | "throw"
                                | "try"
                                | "var"
                                | "void"
                                | "while"
                                | "with"
                        );

                        // Also suppress TS18004 for obviously invalid names that
                        // are parser-recovery artifacts (single punctuation characters
                        // like `:`, `,`, `;` that became shorthand properties during
                        // error recovery).
                        let is_obviously_invalid_name = name.len() == 1
                            && name
                                .chars()
                                .next()
                                .is_some_and(|c| !c.is_alphanumeric() && c != '_' && c != '$');

                        if !is_strict_reserved
                            && !is_obviously_invalid_name
                            && !self.ctx.has_parse_errors
                        {
                            // TS18004: Missing value binding for shorthand property name
                            // Example: `({ arguments })` inside arrow function where `arguments`
                            // is not in scope as a value.
                            let message = format_message(
                                diagnostic_messages::NO_VALUE_EXISTS_IN_SCOPE_FOR_THE_SHORTHAND_PROPERTY_EITHER_DECLARE_ONE_OR_PROVID,
                                &[&name],
                            );
                            self.error_at_node(
                                elem_idx,
                                &message,
                                diagnostic_codes::NO_VALUE_EXISTS_IN_SCOPE_FOR_THE_SHORTHAND_PROPERTY_EITHER_DECLARE_ONE_OR_PROVID,
                            );
                        }

                        // In destructuring assignment targets, unresolved shorthand names
                        // are already invalid (TS18004). Don't synthesize a required
                        // object property from this invalid entry; doing so can produce
                        // follow-on missing-property errors (e.g. TS2741) that tsc omits.
                        if self.ctx.in_destructuring_target {
                            continue;
                        }
                        TypeId::ANY
                    } else if self.ctx.in_destructuring_target {
                        let target_type = self.get_type_of_assignment_target(shorthand_name_idx);
                        if shorthand.equals_token {
                            self.check_destructuring_default_initializer(
                                shorthand.object_assignment_initializer,
                                target_type,
                                elem_idx,
                            );
                        }
                        target_type
                    } else {
                        // Use shorthand_name_idx (the identifier) so that get_type_of_identifier
                        // is invoked, which calls check_flow_usage and can emit TS2454
                        // if the variable is used before assignment.
                        // Using elem_idx (SHORTHAND_PROPERTY_ASSIGNMENT) would return TypeId::ERROR
                        // since that node kind has no dispatch handler, silently suppressing TS2454.
                        self.get_type_of_node_with_request(shorthand_name_idx, &shorthand_request)
                    };

                    let value_type = if let Some(declared_type) = jsdoc_declared_type {
                        let has_uninitialized_value_decl = shorthand_sym.is_some_and(|sym_id| {
                            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                                return false;
                            };
                            let declaration_lacks_initializer = |decl_id| {
                                let Some(mut decl_node) = self.ctx.arena.get(decl_id) else {
                                    return false;
                                };
                                if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
                                    decl_node = self
                                        .ctx
                                        .arena
                                        .node_info(decl_id)
                                        .and_then(|info| self.ctx.arena.get(info.parent))
                                        .unwrap_or(decl_node);
                                }
                                self.ctx
                                    .arena
                                    .get_variable_declaration(decl_node)
                                    .is_some_and(|var_data| var_data.initializer.is_none())
                            };

                            declaration_lacks_initializer(symbol.value_declaration)
                                || symbol
                                    .declarations
                                    .iter()
                                    .copied()
                                    .any(declaration_lacks_initializer)
                        });
                        let check_value_type = shorthand_sym
                            .filter(|&sym_id| {
                                !self.is_definitely_assigned_at_with_symbol(
                                    shorthand_name_idx,
                                    Some(sym_id),
                                ) || has_uninitialized_value_decl
                            })
                            .map(|_| TypeId::UNDEFINED)
                            .unwrap_or(value_type);
                        if !self.is_assignable_to(check_value_type, declared_type) {
                            self.error_type_not_assignable_at_with_anchor(
                                check_value_type,
                                declared_type,
                                elem_idx,
                            );
                        }
                        declared_type
                    } else {
                        // Apply bidirectional type inference and widen (same as named properties)
                        let value_type = tsz_solver::apply_contextual_type(
                            self.ctx.types,
                            value_type,
                            property_context_type,
                        );
                        if !self.ctx.in_const_assertion
                            && !self.ctx.preserve_literal_types
                            && property_context_type.is_none()
                            && !had_object_context
                        {
                            let widened = self.widen_literal_type(value_type);
                            if widened != value_type {
                                let name_atom = self.ctx.types.intern_string(&name);
                                display_type_overrides.insert(name_atom, value_type);
                            }
                            widened
                        } else {
                            value_type
                        }
                    };

                    // Note: TS7008 is NOT emitted for object literal properties.
                    // tsc only emits TS7008 for class properties, property signatures,
                    // auto-accessors, and binary expressions.

                    let name_atom = self.ctx.types.intern_string(&name);

                    // Check for duplicate property (skip in destructuring targets)
                    // TS1117: duplicate properties are an error in object literals.
                    if !skip_duplicate_check
                        && explicit_property_names.contains(&name_atom)
                        && !self.ctx.has_parse_errors
                        && (!self.is_js_file() || self.ctx.js_strict_mode_diagnostics_enabled())
                    {
                        let message = format_message(
                            diagnostic_messages::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                            &[&name],
                        );
                        self.error_at_node(
                            elem_idx,
                            &message,
                            diagnostic_codes::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                        );
                    }
                    explicit_property_names.insert(name_atom);

                    // Track this shorthand property for TS2783 spread-overwrite checking
                    named_property_nodes.insert(name_atom, (elem_idx, name.clone()));

                    // In destructuring assignment targets, a shorthand with a default
                    // value (e.g. `{ x = 0 } = source`) makes the property optional
                    // in the source type.
                    let is_optional_shorthand =
                        self.ctx.in_destructuring_target && shorthand.equals_token;

                    let order = prop_order;
                    prop_order += 1;
                    properties.insert(
                        name_atom,
                        PropertyInfo {
                            name: name_atom,
                            type_id: value_type,
                            write_type: value_type,
                            optional: is_optional_shorthand,
                            readonly: false,
                            is_method: false,
                            is_class_prototype: false,
                            visibility: Visibility::Public,
                            parent_id: None,
                            declaration_order: order,
                        },
                    );
                } else if let Some(shorthand) = self.ctx.arena.get_shorthand_property(elem_node) {
                    self.check_computed_property_name(shorthand.name);
                }
            }
            // Method shorthand: { foo() {} }
            else if let Some(method) = self.ctx.arena.get_method_decl(elem_node) {
                // Always type-check computed property name expressions for methods,
                // even when the identifier can be resolved as a literal name.
                // E.g., `{ [e]() {} }` needs TS2304 for undeclared `e`.
                // We call get_type_of_node directly (not check_computed_property_name)
                // to avoid triggering TS2467 for type parameters in nested object literals.
                if let Some(prop_name_node) = self.ctx.arena.get(method.name)
                    && prop_name_node.kind
                        == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME
                    && let Some(computed) = self.ctx.arena.get_computed_property(prop_name_node)
                {
                    self.get_type_of_node(computed.expression);
                }
                let name_opt = self.get_property_name_resolved(method.name);
                if let Some(name) = name_opt.clone() {
                    // Set contextual type for method
                    let jsdoc_declared_type = self.jsdoc_type_annotation_for_node_direct(elem_idx);
                    let jsdoc_method_context_type =
                        self.jsdoc_callable_type_annotation_for_node_direct(elem_idx);
                    let define_property_context_type = (name == "set")
                        .then(|| {
                            self.define_property_descriptor_setter_context_type(
                                idx,
                                &obj.elements.nodes,
                            )
                        })
                        .flatten();
                    let method_context_type = contextual_type.and_then(|ctx_type| {
                        self.contextual_method_context_type_for_lookup(ctx_type, &name)
                    });
                    let method_context_type = self.substitute_contextual_this_type(
                        method_context_type,
                        contextual_receiver_this_type,
                    );
                    let method_request = base_request.contextual_opt(
                        self.contextual_type_option_for_expression(
                            jsdoc_method_context_type
                                .or(method_context_type)
                                .or(define_property_context_type),
                        ),
                    );

                    // If no explicit ThisType marker exists, use the object literal's
                    // contextual type as `this` inside method bodies.
                    let mut pushed_contextual_this = false;
                    let mut pushed_synthetic_this = false;
                    if marker_this_type.is_none() && self.current_this_type().is_none() {
                        if let Some(ctx_type) = contextual_type {
                            let ctx_type = self.evaluate_contextual_type(ctx_type);
                            self.ctx.this_type_stack.push(ctx_type);
                            pushed_contextual_this = true;
                        } else {
                            let synthetic_this_type = self
                                .build_object_literal_method_synthetic_this_type(
                                    &properties,
                                    &obj_all_method_names,
                                    elem_idx,
                                    &name,
                                    None,
                                );
                            self.ctx.this_type_stack.push(synthetic_this_type);
                            pushed_synthetic_this = true;
                        }
                    }

                    let contextual_method_param_types =
                        method_request.contextual_type.map(|ctx_ty| {
                            let ctx_helper =
                                tsz_solver::ContextualTypeContext::with_expected_and_options(
                                    self.ctx.types,
                                    ctx_ty,
                                    self.ctx.compiler_options.no_implicit_any,
                                );
                            let this_atom = self.ctx.types.intern_string("this");
                            let mut contextual_index = 0usize;
                            method
                                .parameters
                                .nodes
                                .iter()
                                .map(|&param_idx| {
                                    let param = self.ctx.arena.get_parameter_at(param_idx)?;
                                    let is_this_param = self
                                        .ctx
                                        .arena
                                        .get(param.name)
                                        .and_then(|name_node| {
                                            self.ctx.arena.get_identifier(name_node)
                                        })
                                        .is_some_and(|ident| ident.atom == this_atom);
                                    let contextual_param_type = if is_this_param {
                                        ctx_helper
                                            .get_this_type()
                                            .or_else(|| ctx_helper.get_this_type_from_marker())
                                    } else if param.dot_dot_dot_token {
                                        ctx_helper.get_rest_parameter_type(contextual_index)
                                    } else {
                                        ctx_helper.get_parameter_type(contextual_index)
                                    };
                                    if !is_this_param {
                                        contextual_index += 1;
                                    }
                                    contextual_param_type
                                })
                                .collect::<Vec<_>>()
                        });
                    self.cache_parameter_types(
                        &method.parameters.nodes,
                        contextual_method_param_types.as_deref(),
                    );
                    if let Some(contextual_types) = contextual_method_param_types.as_ref() {
                        for (&param_idx, contextual_type) in method
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

                    let method_diag_guard = DiagnosticSpeculationGuard::new(&self.ctx);
                    let pre_refresh_snap = self.ctx.snapshot_diagnostics();
                    let mut method_type = self.get_type_of_function_impl(elem_idx, &method_request);
                    let has_concrete_method_context =
                        self.request_has_concrete_contextual_type(&method_request);
                    if has_concrete_method_context {
                        let spans = self.function_like_param_spans_for_node(elem_idx);
                        self.clear_stale_function_like_implicit_any_diagnostics(
                            &spans,
                            &pre_refresh_snap,
                        );
                    }

                    let this_property_accesses =
                        self.collect_return_expression_this_property_accesses(method.body);
                    let method_return_this_circularity = pushed_synthetic_this
                        && jsdoc_declared_type.is_none()
                        && method.type_annotation.is_none()
                        && !has_concrete_method_context
                        && !this_property_accesses.is_empty()
                        && self.ctx.arena.get(method.body).is_some_and(|body_node| {
                            self.ctx
                                .speculative_diagnostics_since(method_diag_guard.snapshot())
                                .iter()
                                .any(|diag| {
                                    diag.start >= body_node.pos
                                        && diag.start < body_node.end
                                        && matches!(diag.code, 2339 | 2464)
                                })
                        });

                    if pushed_contextual_this || pushed_synthetic_this {
                        self.ctx.this_type_stack.pop();
                    }

                    if method_return_this_circularity {
                        method_diag_guard.rollback(&mut self.ctx);
                        self.invalidate_expression_for_contextual_retry(elem_idx);
                        let refined_method_type = crate::query_boundaries::assignability::get_function_return_type(
                            self.ctx.types,
                            method_type,
                        )
                        .map(|return_type| {
                            let refined_return_type = if matches!(return_type, TypeId::ERROR | TypeId::VOID) {
                                TypeId::ANY
                            } else {
                                crate::query_boundaries::common::widen_type(
                                    self.ctx.types,
                                    return_type,
                                )
                            };
                            crate::query_boundaries::assignability::replace_function_return_type(
                                self.ctx.types,
                                method_type,
                                refined_return_type,
                            )
                        })
                        .unwrap_or(method_type);
                        let refined_this_type = self
                            .build_object_literal_method_synthetic_this_type(
                                &properties,
                                &obj_all_method_names,
                                elem_idx,
                                &name,
                                Some(refined_method_type),
                            );
                        let refined_this_display = Self::widen_primitive_literal_type_display(
                            &self.format_type(crate::query_boundaries::common::widen_type(
                                self.ctx.types,
                                crate::query_boundaries::common::widen_freshness(
                                    self.ctx.types,
                                    refined_this_type,
                                ),
                            )),
                        );
                        self.ctx.this_type_stack.push(refined_this_type);
                        let rerun_guard = DiagnosticSpeculationGuard::new(&self.ctx);
                        let rerun_pre_refresh_snap = self.ctx.snapshot_diagnostics();
                        let _ = self.get_type_of_function_impl(elem_idx, &method_request);
                        if has_concrete_method_context {
                            let spans = self.function_like_param_spans_for_node(elem_idx);
                            self.clear_stale_function_like_implicit_any_diagnostics(
                                &spans,
                                &rerun_pre_refresh_snap,
                            );
                        }
                        self.ctx.this_type_stack.pop();
                        let this_property_positions: std::collections::HashSet<u32> =
                            this_property_accesses
                                .iter()
                                .filter_map(|(idx, _)| {
                                    self.ctx.arena.get(*idx).map(|node| node.pos)
                                })
                                .collect();
                        rerun_guard.rollback_filtered(&mut self.ctx, |diag| {
                            let is_replaced_this_property_error =
                                diag.code == 2339 && this_property_positions.contains(&diag.start);
                            !is_replaced_this_property_error
                        });
                        for (property_idx, property_name) in &this_property_accesses {
                            self.error_property_not_exist_with_apparent_type(
                                property_name,
                                &refined_this_display,
                                *property_idx,
                            );
                        }
                        use crate::diagnostics::diagnostic_codes;
                        self.error_at_node_msg(
                            method.name,
                            diagnostic_codes::IMPLICITLY_HAS_RETURN_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_RETURN_TYPE_ANNOTATION,
                            &[&name],
                        );
                        method_type = refined_method_type;
                    }

                    let method_type = jsdoc_declared_type.unwrap_or(method_type);

                    let name_atom = self.ctx.types.intern_string(&name);

                    // Check for duplicate property (skip in destructuring targets and computed names)
                    // TS1117: duplicate properties are an error in object literals.
                    if !skip_duplicate_check
                        && explicit_property_names.contains(&name_atom)
                        && !self.ctx.has_parse_errors
                        && (!self.is_js_file() || self.ctx.js_strict_mode_diagnostics_enabled())
                    {
                        let message = format_message(
                            diagnostic_messages::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                            &[&name],
                        );
                        self.error_at_node(
                            method.name,
                            &message,
                            diagnostic_codes::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                        );
                    }
                    explicit_property_names.insert(name_atom);

                    let order = prop_order;
                    prop_order += 1;
                    properties.insert(
                        name_atom,
                        PropertyInfo {
                            name: name_atom,
                            type_id: method_type,
                            write_type: method_type,
                            optional: false,
                            readonly: false,
                            is_method: true, // Object literal methods should be bivariant
                            is_class_prototype: false,
                            visibility: Visibility::Public,
                            parent_id: None,
                            declaration_order: order,
                        },
                    );
                } else {
                    // Computed method name - still type-check the expression and function body.
                    // For contextual typing, use the index signature type from the contextual type.
                    // E.g., `var o: { [s: string]: (x: string) => number } = { ["" + 0](y) { ... } }`
                    // should contextually type `y` as `string` from the string index signature.
                    self.check_computed_property_name(method.name);

                    let mut prop_name_type = TypeId::ANY;
                    let mut resolved_computed_name = None;
                    if let Some(prop_name_node) = self.ctx.arena.get(method.name)
                        && prop_name_node.kind
                            == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME
                        && let Some(computed) = self.ctx.arena.get_computed_property(prop_name_node)
                    {
                        prop_name_type = self.get_type_of_node(computed.expression);
                        resolved_computed_name = self.get_property_name_resolved(method.name);
                        if let Some(atom) =
                            crate::query_boundaries::type_computation::access::literal_property_name(
                                self.ctx.types,
                                prop_name_type,
                            )
                        {
                            if resolved_computed_name.is_none() {
                                resolved_computed_name =
                                    Some(self.ctx.types.resolve_atom(atom).to_string());
                            }
                            if !skip_duplicate_check
                                && explicit_property_names.contains(&atom)
                                && !self.ctx.has_parse_errors
                                && (!self.is_js_file()
                                    || self.ctx.js_strict_mode_diagnostics_enabled())
                            {
                                let name = self.ctx.types.resolve_atom(atom).to_string();
                                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                                let message = crate::diagnostics::format_message(
                                            diagnostic_messages::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                                            &[&name],
                                        );
                                self.error_at_node(
                                            method.name,
                                            &message,
                                            diagnostic_codes::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                                        );
                            }
                            explicit_property_names.insert(atom);
                        }
                    }
                    let computed_context_type = contextual_type.and_then(|ctx_type| {
                        let property_context_type = self
                            .contextual_object_property_type_for_lookup(
                                ctx_type,
                                resolved_computed_name.as_deref().unwrap_or("__@computed"),
                            );
                        if matches!(property_context_type, Some(TypeId::ANY | TypeId::UNKNOWN)) {
                            self.contextual_callable_property_fallback_for_lookup(
                                ctx_type,
                                property_context_type,
                            )
                        } else {
                            property_context_type.or_else(|| {
                                self.contextual_callable_property_fallback_for_lookup(
                                    ctx_type, None,
                                )
                            })
                        }
                    });
                    let computed_context_type = self.substitute_contextual_this_type(
                        computed_context_type,
                        contextual_receiver_this_type,
                    );
                    let method_request = base_request.contextual_opt(
                        self.contextual_type_option_for_expression(computed_context_type),
                    );
                    let pre_refresh_snap = self.ctx.snapshot_diagnostics();
                    let method_type = self.get_type_of_function_impl(elem_idx, &method_request);
                    if self.request_has_concrete_contextual_type(&method_request) {
                        let spans = self.function_like_param_spans_for_node(elem_idx);
                        self.clear_stale_function_like_implicit_any_diagnostics(
                            &spans,
                            &pre_refresh_snap,
                        );
                    }

                    if self.is_assignable_to(prop_name_type, TypeId::NUMBER) {
                        number_index_types.push(method_type);
                    } else if self.is_assignable_to(prop_name_type, TypeId::STRING)
                        || self.is_assignable_to(prop_name_type, TypeId::ANY)
                    {
                        string_index_types.push(method_type);
                    }
                }
            }
            // Accessor: { get foo() {} } or { set foo(v) {} }
            else if let Some(accessor) = self.ctx.arena.get_accessor(elem_node) {
                // Always type-check computed property name expressions for accessors,
                // even when the identifier can be resolved as a literal name.
                // E.g., `{ get [e]() {} }` needs TS2304 for undeclared `e`.
                // We call get_type_of_node directly (not check_computed_property_name)
                // to avoid triggering TS2467 for type parameters in nested object literals.
                if let Some(prop_name_node) = self.ctx.arena.get(accessor.name)
                    && prop_name_node.kind
                        == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME
                    && let Some(computed) = self.ctx.arena.get_computed_property(prop_name_node)
                {
                    self.get_type_of_node(computed.expression);
                }
                // Missing body for accessors in object literals is a grammar error.
                // tsc does NOT emit TS1005 here; it defers to TS2378/TS1049
                // ("A 'get' accessor must have a body"). We skip TS1005 to avoid
                // false positives that incorrectly suppress TS5107 deprecation
                // warnings in the driver's grammar-error priority logic.

                // For setters, check implicit any on parameters (error 7006) and on
                // the property name itself (error 7032).
                // When a paired getter exists, the setter parameter type is inferred
                // from the getter return type (contextually typed, suppress TS7006/7032).
                if elem_node.kind == syntax_kind_ext::SET_ACCESSOR {
                    let name_opt = self.get_property_name(accessor.name).or_else(|| {
                        let prop_name_type = self.get_type_of_node(accessor.name);
                        crate::query_boundaries::type_computation::access::literal_property_name(
                            self.ctx.types,
                            prop_name_type,
                        )
                        .map(|atom| self.ctx.types.resolve_atom(atom))
                    });
                    let has_paired_getter = name_opt
                        .as_ref()
                        .is_some_and(|name| obj_getter_names.contains(name));
                    // Check if accessor JSDoc has @param type annotations
                    let accessor_jsdoc = self.get_jsdoc_for_function(elem_idx);
                    let mut first_param_lacks_annotation = false;
                    for (pi, &param_idx) in accessor.parameters.nodes.iter().enumerate() {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        {
                            let has_jsdoc = has_paired_getter
                                || self.param_has_inline_jsdoc_type(param_idx)
                                || if let Some(ref jsdoc) = accessor_jsdoc {
                                    let pname = self.parameter_name_for_error(param.name);
                                    Self::jsdoc_has_param_type(jsdoc, &pname)
                                } else {
                                    false
                                };
                            if param.type_annotation.is_none() && !has_jsdoc {
                                first_param_lacks_annotation = true;
                            }
                            self.maybe_report_implicit_any_parameter(param, has_jsdoc, pi);
                        }
                    }
                    // TS7032: emit on property name when the setter has no parameter type
                    // annotation and no paired getter (TSC checks this at accessor symbol
                    // resolution time; we emit it here during object literal checking).
                    if first_param_lacks_annotation
                        && !has_paired_getter
                        && self.ctx.no_implicit_any()
                        && let Some(prop_name) = name_opt.as_deref()
                    {
                        use crate::diagnostics::diagnostic_codes;
                        self.error_at_node_msg(
                            accessor.name,
                            diagnostic_codes::PROPERTY_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_ITS_SET_ACCESSOR_LACKS_A_PARAMETER_TYPE,
                            &[prop_name],
                        );
                    }
                }

                let name_opt = self.get_property_name_resolved(accessor.name);
                if let Some(name) = name_opt.clone() {
                    // For non-contextual object literals, TypeScript treats `this` inside
                    // accessors as the object literal under construction. Provide a
                    // lightweight synthetic receiver so property access checks (TS2339)
                    // run during accessor body checking.
                    let mut pushed_synthetic_this = false;
                    if marker_this_type.is_none() {
                        let mut this_props: Vec<PropertyInfo> =
                            properties.values().cloned().collect();
                        let name_atom = self.ctx.types.intern_string(&name);
                        if !this_props.iter().any(|p| p.name == name_atom) {
                            // Getter-only accessors are readonly in the object type
                            let is_getter_only = elem_node.kind == syntax_kind_ext::GET_ACCESSOR
                                && !setter_names.contains(&name_atom);
                            this_props.push(PropertyInfo {
                                name: name_atom,
                                type_id: TypeId::ANY,
                                write_type: TypeId::ANY,
                                optional: false,
                                readonly: is_getter_only,
                                is_method: false,
                                is_class_prototype: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                                declaration_order: 0,
                            });
                        }
                        self.ctx
                            .this_type_stack
                            .push(self.ctx.types.factory().object(this_props));
                        pushed_synthetic_this = true;
                    }

                    // For getter, infer return type; for setter, use the parameter type
                    let accessor_type = if elem_node.kind == syntax_kind_ext::GET_ACCESSOR {
                        // Check getter body/parameters via function checking, but object
                        // property read type is the getter's return type (not a function type).
                        self.get_type_of_function(elem_idx);
                        if accessor.type_annotation.is_none() {
                            // When a contextual type exists (e.g., `T extends { [k: string]: Types }`),
                            // pass the contextual property type as the return context so that
                            // literal types from `as const` are preserved instead of widened.
                            // Without this, `get x() { return 'boolean' as const }` widens
                            // to `string` because infer_getter_return_type passes None.
                            let return_context = contextual_type.and_then(|ctx| {
                                self.contextual_object_property_type_for_lookup(ctx, &name)
                            });
                            self.infer_return_type_from_body(
                                tsz_parser::parser::NodeIndex::NONE,
                                accessor.body,
                                return_context,
                            )
                        } else {
                            self.get_type_from_type_node(accessor.type_annotation)
                        }
                    } else {
                        // Setter: type-check the function body to track variable usage
                        // (especially for noUnusedParameters/noUnusedLocals checking),
                        // but use the parameter type annotation for the property type
                        self.get_type_of_function(elem_idx);

                        // Extract setter write type from first parameter.
                        // When no type annotation, fall back to the paired getter's
                        // return type (mirroring tsc's inference behavior).
                        accessor
                            .parameters
                            .nodes
                            .first()
                            .and_then(|&param_idx| {
                                let param = self.ctx.arena.get_parameter_at(param_idx)?;
                                if param.type_annotation.is_none() {
                                    None
                                } else {
                                    Some(self.get_type_from_type_node(param.type_annotation))
                                }
                            })
                            .or_else(|| {
                                // No annotation — infer from paired getter's type
                                let setter_name = name_opt.clone()?;
                                let name_atom = self.ctx.types.intern_string(&setter_name);
                                properties.get(&name_atom).map(|p| p.type_id)
                            })
                            .unwrap_or(TypeId::ANY)
                    };

                    if pushed_synthetic_this {
                        self.ctx.this_type_stack.pop();
                    }

                    if elem_node.kind == syntax_kind_ext::GET_ACCESSOR {
                        if accessor.type_annotation.is_none() {
                            use crate::diagnostics::diagnostic_codes;
                            let self_refs =
                                self.collect_property_name_references(accessor.body, &name);
                            if !self_refs.is_empty() {
                                self.error_at_node_msg(
                                    accessor.name,
                                    diagnostic_codes::IMPLICITLY_HAS_RETURN_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_RETURN_TYPE_ANNOTATION,
                                    &[&name],
                                );
                            }
                        }

                        self.maybe_report_implicit_any_return(
                            Some(name.clone()),
                            Some(accessor.name),
                            accessor_type,
                            accessor.type_annotation.is_some(),
                            false,
                            elem_idx,
                        );
                    }

                    // TS2378: A 'get' accessor must return a value.
                    // Check if the getter has a body but no return statement with a value.
                    if elem_node.kind == syntax_kind_ext::GET_ACCESSOR && accessor.body.is_some() {
                        let has_return = self.body_has_return_with_value(accessor.body);
                        let falls_through = self.function_body_falls_through(accessor.body);

                        if !has_return && falls_through {
                            use crate::diagnostics::diagnostic_codes;
                            self.error_at_node(
                                accessor.name,
                                "A 'get' accessor must return a value.",
                                diagnostic_codes::A_GET_ACCESSOR_MUST_RETURN_A_VALUE,
                            );
                        }
                    }
                    let name_atom = self.ctx.types.intern_string(&name);

                    // Check for duplicate property - but allow getter+setter pairs
                    // A getter and setter with the same name is valid, not a duplicate
                    let is_getter = elem_node.kind == syntax_kind_ext::GET_ACCESSOR;
                    let is_complementary_pair = if is_getter {
                        setter_names.contains(&name_atom) && !getter_names.contains(&name_atom)
                    } else {
                        getter_names.contains(&name_atom) && !setter_names.contains(&name_atom)
                    };
                    // Duplicate properties are an error in object literals.
                    // TS1118 for duplicate get/set accessors, TS1117 for other duplicates.
                    // Skip for computed property names — tsc only checks static names.
                    if !skip_duplicate_check
                        && explicit_property_names.contains(&name_atom)
                        && !is_complementary_pair
                        && !self.ctx.has_parse_errors
                        && (!self.is_js_file() || self.ctx.js_strict_mode_diagnostics_enabled())
                    {
                        let is_duplicate_accessor = (is_getter
                            && getter_names.contains(&name_atom))
                            || (!is_getter && setter_names.contains(&name_atom));
                        if is_duplicate_accessor {
                            self.error_at_node(
                                accessor.name,
                                "An object literal cannot have multiple get/set accessors with the same name.",
                                diagnostic_codes::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_GET_SET_ACCESSORS_WITH_THE_SAME_NAME,
                            );
                        } else {
                            let message = format_message(
                                diagnostic_messages::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                                &[&name],
                            );
                            self.error_at_node(
                                accessor.name,
                                &message,
                                diagnostic_codes::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                            );
                        }
                    }
                    explicit_property_names.insert(name_atom);

                    if is_getter {
                        getter_names.insert(name_atom);
                    } else {
                        setter_names.insert(name_atom);
                    }

                    // Merge getter/setter into a single property with separate
                    // read (type_id) and write (write_type) types.
                    if let Some(existing) = properties.get(&name_atom) {
                        let existing_order = existing.declaration_order;
                        let (read_type, write_type) = if is_getter {
                            // Getter arriving after setter
                            (accessor_type, existing.write_type)
                        } else {
                            // Setter arriving after getter
                            (existing.type_id, accessor_type)
                        };
                        // Both getter and setter exist → not readonly
                        properties.insert(
                            name_atom,
                            PropertyInfo {
                                name: name_atom,
                                type_id: read_type,
                                write_type,
                                optional: false,
                                readonly: false,
                                is_method: false,
                                is_class_prototype: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                                declaration_order: existing_order,
                            },
                        );
                    } else {
                        // Single accessor so far: getter-only is readonly.
                        // Set-only: read type is `undefined`.
                        let readonly = is_getter;
                        let (read_type, write_type) = if is_getter {
                            (accessor_type, accessor_type)
                        } else {
                            (TypeId::UNDEFINED, accessor_type)
                        };
                        let order = prop_order;
                        prop_order += 1;
                        properties.insert(
                            name_atom,
                            PropertyInfo {
                                name: name_atom,
                                type_id: read_type,
                                write_type,
                                optional: false,
                                readonly,
                                is_method: false,
                                is_class_prototype: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                                declaration_order: order,
                            },
                        );
                    }
                } else {
                    // Computed accessor name - still type-check the expression and body
                    self.check_computed_property_name(accessor.name);

                    let mut prop_name_type = TypeId::ANY;
                    if let Some(prop_name_node) = self.ctx.arena.get(accessor.name)
                        && prop_name_node.kind
                            == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME
                        && let Some(computed) = self.ctx.arena.get_computed_property(prop_name_node)
                    {
                        prop_name_type = self.get_type_of_node(computed.expression);
                        if let Some(atom) =
                            crate::query_boundaries::type_computation::access::literal_property_name(
                                self.ctx.types,
                                prop_name_type,
                            )
                        {
                            let is_getter =
                                elem_node.kind == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR;
                            let is_complementary_pair = if is_getter {
                                setter_names.contains(&atom) && !getter_names.contains(&atom)
                            } else {
                                getter_names.contains(&atom) && !setter_names.contains(&atom)
                            };
                            if !skip_duplicate_check
                                && explicit_property_names.contains(&atom)
                                && !is_complementary_pair
                                && !self.ctx.has_parse_errors
                                && (!self.is_js_file()
                                    || self.ctx.js_strict_mode_diagnostics_enabled())
                            {
                                let name = self.ctx.types.resolve_atom(atom).to_string();
                                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                                let message = crate::diagnostics::format_message(
                                            diagnostic_messages::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                                            &[&name],
                                        );
                                self.error_at_node(
                                            accessor.name,
                                            &message,
                                            diagnostic_codes::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                                        );
                            }
                            explicit_property_names.insert(atom);
                        }
                    }

                    let accessor_type = if elem_node.kind == syntax_kind_ext::GET_ACCESSOR {
                        self.get_type_of_function(elem_idx);

                        // TS2378: A 'get' accessor must return a value.
                        if accessor.body.is_some() {
                            let has_return = self.body_has_return_with_value(accessor.body);
                            let falls_through = self.function_body_falls_through(accessor.body);
                            if !has_return && falls_through {
                                use crate::diagnostics::diagnostic_codes;
                                self.error_at_node(
                                    accessor.name,
                                    "A 'get' accessor must return a value.",
                                    diagnostic_codes::A_GET_ACCESSOR_MUST_RETURN_A_VALUE,
                                );
                            }
                        }

                        if accessor.type_annotation.is_none() {
                            self.infer_getter_return_type(accessor.body)
                        } else {
                            self.get_type_from_type_node(accessor.type_annotation)
                        }
                    } else {
                        self.get_type_of_function(elem_idx);
                        accessor
                            .parameters
                            .nodes
                            .first()
                            .and_then(|&param_idx| {
                                let param = self.ctx.arena.get_parameter_at(param_idx)?;
                                if param.type_annotation.is_none() {
                                    None
                                } else {
                                    Some(self.get_type_from_type_node(param.type_annotation))
                                }
                            })
                            .unwrap_or(TypeId::ANY)
                    };

                    if self.is_assignable_to(prop_name_type, TypeId::NUMBER) {
                        number_index_types.push(accessor_type);
                    } else if self.is_assignable_to(prop_name_type, TypeId::STRING)
                        || self.is_assignable_to(prop_name_type, TypeId::ANY)
                    {
                        string_index_types.push(accessor_type);
                    }
                }
            }
            // Spread assignment: { ...obj }
            else if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                || elem_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT
            {
                has_spread = true;
                let spread_expr = self
                    .ctx
                    .arena
                    .get_spread(elem_node)
                    .map(|spread| spread.expression)
                    .or_else(|| {
                        self.ctx
                            .arena
                            .get_unary_expr_ex(elem_node)
                            .map(|unary| unary.expression)
                    });
                if let Some(spread_expr) = spread_expr {
                    let mut invalid_rest_target = false;
                    if self.ctx.in_destructuring_target {
                        // TS2701: The target of an object rest assignment must be
                        // a variable or a property access.
                        // E.g. `{ ...expr + expr } = source` is invalid.
                        if !self.is_valid_rest_assignment_target(spread_expr) {
                            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                            self.error_at_node(
                                spread_expr,
                                diagnostic_messages::THE_TARGET_OF_AN_OBJECT_REST_ASSIGNMENT_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS,
                                diagnostic_codes::THE_TARGET_OF_AN_OBJECT_REST_ASSIGNMENT_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS,
                            );
                            invalid_rest_target = true;
                        }
                        // TS2778: The target of an object rest assignment may not be
                        // an optional property access. E.g. `{ ...obj?.a } = source`
                        else if self.is_optional_chain_access(spread_expr) {
                            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                            self.error_at_node(
                                spread_expr,
                                diagnostic_messages::THE_TARGET_OF_AN_OBJECT_REST_ASSIGNMENT_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS,
                                diagnostic_codes::THE_TARGET_OF_AN_OBJECT_REST_ASSIGNMENT_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS,
                            );
                        }
                    }
                    // Clear contextual type for call-like spread expressions.
                    // The outer contextual type (e.g., from a destructuring pattern)
                    // should not propagate into call expression return types —
                    // otherwise IIFEs in spreads get false contextual return types,
                    // producing spurious TS2741/TS2322 errors.
                    // But direct object literals in spreads (e.g., `{ ...{ a: "a" } }`)
                    // SHOULD keep the contextual type so literals stay narrow.
                    let unwrapped_spread = self
                        .ctx
                        .arena
                        .skip_parenthesized_and_assertions(spread_expr);
                    let spread_is_call_like =
                        self.ctx.arena.get(unwrapped_spread).is_some_and(|node| {
                            node.kind == syntax_kind_ext::CALL_EXPRESSION
                                || node.kind == syntax_kind_ext::NEW_EXPRESSION
                                || node.kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
                        });
                    let spread_request = if spread_is_call_like {
                        base_request.contextual_opt(None)
                    } else {
                        base_request
                    };
                    let spread_type =
                        self.get_type_of_node_with_request(spread_expr, &spread_request);
                    // TS2698: Spread types may only be created from object types.
                    // Skip when TS2701 was already emitted (invalid rest target) —
                    // the expression isn't a valid spread source because it's not
                    // even a valid assignment target.
                    let resolved_spread = self.resolve_type_for_property_access(spread_type);
                    let resolved_spread = self.resolve_lazy_type(resolved_spread);
                    let is_valid_spread = if invalid_rest_target {
                        true // suppress TS2698 when TS2701 already reported
                    } else {
                        crate::query_boundaries::type_computation::access::is_valid_spread_type(
                            self.ctx.types,
                            resolved_spread,
                        )
                    };
                    if !is_valid_spread {
                        self.report_spread_not_object_type(elem_idx);
                    }

                    // Short-circuit: when the object literal is a single spread
                    // of a type parameter (e.g., `{ ...item }` where `item: T`),
                    // preserve the type parameter as the result type. Expanding
                    // to the constraint's properties would lose generic type
                    // information, causing false TS2322 errors like
                    // `Type '{ name: string }' is not assignable to type 'T'`.
                    // Only when the spread is valid (no TS2698) — invalid spreads
                    // like `T extends undefined` must not short-circuit.
                    if is_valid_spread
                        && obj.elements.nodes.len() == 1
                        && properties.is_empty()
                        && (tsz_solver::type_param_info(self.ctx.types, spread_type).is_some()
                            || tsz_solver::type_queries::contains_type_parameters_db(
                                self.ctx.types,
                                spread_type,
                            ))
                    {
                        // Pop this type from stack if we pushed it earlier
                        if marker_this_type.is_some() {
                            self.ctx.this_type_stack.pop();
                        }
                        return spread_type;
                    }

                    // Check if the spread type is a union — if so, distribute
                    // the spread over each union member: { ...A|B } → { ...A } | { ...B }
                    let union_members_opt = tsz_solver::type_queries::get_union_members(
                        self.ctx.types,
                        resolved_spread,
                    );

                    // Guard against exponential blowup: if the cross-product
                    // of branches would exceed a limit, skip distribution.
                    let branch_count = if union_spread_branches.is_empty() {
                        1
                    } else {
                        union_spread_branches.len()
                    };
                    let union_members_opt = union_members_opt.filter(|members| {
                        // Only distribute when all members are object-like (not
                        // false/null/undefined). Spreading primitives just
                        // contributes {} which isn't useful to distribute.
                        let all_object_like = members.iter().all(|m| {
                            !self
                                .ctx
                                .types
                                .collect_object_spread_properties(*m)
                                .is_empty()
                        });
                        all_object_like && branch_count.saturating_mul(members.len()) <= 16
                    });

                    if let Some(members) = union_members_opt {
                        // TS2783: Check if any earlier named properties will be
                        // overwritten by required properties from this union spread.
                        // A property triggers TS2783 when it is required (non-optional)
                        // in ALL non-nullish members of the union.
                        if self.ctx.strict_null_checks() {
                            let non_nullish_members: Vec<TypeId> = members
                                .iter()
                                .copied()
                                .filter(|m| !m.is_nullable())
                                .collect();
                            if !non_nullish_members.is_empty() {
                                // Collect properties per member
                                let all_member_props: Vec<Vec<_>> = non_nullish_members
                                    .iter()
                                    .map(|m| self.collect_object_spread_properties(*m))
                                    .collect();
                                // Find properties that are required in ALL members
                                if let Some(first) = all_member_props.first() {
                                    for prop in first {
                                        if prop.optional {
                                            continue;
                                        }
                                        let in_all =
                                            all_member_props[1..].iter().all(|member_props| {
                                                member_props
                                                    .iter()
                                                    .any(|p| p.name == prop.name && !p.optional)
                                            });
                                        if in_all
                                            && let Some((prop_node, prop_name)) =
                                                named_property_nodes.get(&prop.name)
                                        {
                                            let message = format_message(
                                                    diagnostic_messages::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                                                    &[prop_name],
                                                );
                                            self.error_at_node(
                                                    *prop_node,
                                                    &message,
                                                    diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                                                );
                                        }
                                    }
                                }
                            }
                        }

                        // Union spread distribution: fork current property set
                        // into N branches, one per union member.
                        has_union_spread = true;
                        let mut new_branches: Vec<FxHashMap<Atom, PropertyInfo>> = Vec::new();

                        // Collect properties from each union member for TS2783
                        // and branching.
                        let all_member_props: Vec<Vec<PropertyInfo>> = members
                            .iter()
                            .map(|m| self.collect_object_spread_properties(*m))
                            .collect();

                        // TS2783: When a property is required (non-optional)
                        // in ALL members of the union spread, it will always
                        // overwrite any earlier named property.
                        if self.ctx.strict_null_checks() && !named_property_nodes.is_empty() {
                            // Find property names that are required in every member.
                            let mut always_required: FxHashMap<Atom, bool> = FxHashMap::default();
                            for (i, member_props) in all_member_props.iter().enumerate() {
                                if i == 0 {
                                    for prop in member_props {
                                        always_required.insert(prop.name, !prop.optional);
                                    }
                                } else {
                                    // Remove names not present in this member
                                    always_required.retain(|name, required| {
                                        if let Some(prop) =
                                            member_props.iter().find(|p| p.name == *name)
                                        {
                                            if prop.optional {
                                                *required = false;
                                            }
                                            true
                                        } else {
                                            false
                                        }
                                    });
                                }
                            }
                            for (name, required) in &always_required {
                                if *required
                                    && let Some((prop_node, prop_name)) =
                                        named_property_nodes.get(name)
                                {
                                    let message = format_message(
                                            diagnostic_messages::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                                            &[prop_name],
                                        );
                                    self.error_at_node(
                                            *prop_node,
                                            &message,
                                            diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                                        );
                                }
                            }
                            // Clear named-property tracking for overwritten props
                            for name in always_required.keys() {
                                named_property_nodes.remove(name);
                            }
                        }

                        for member_props in all_member_props {
                            if union_spread_branches.is_empty() {
                                // First union spread: fork from the main properties
                                let mut branch = properties.clone();
                                for prop in member_props {
                                    branch.insert(prop.name, prop);
                                }
                                new_branches.push(branch);
                            } else {
                                // Subsequent union spread: cross-product with existing branches
                                for existing in &union_spread_branches {
                                    let mut branch = existing.clone();
                                    for prop in &member_props {
                                        branch.insert(prop.name, prop.clone());
                                    }
                                    new_branches.push(branch);
                                }
                            }
                        }
                        union_spread_branches = new_branches;
                        // Clear main properties so post-union properties
                        // don't include pre-union ones when applied at the end
                        properties.clear();
                    } else {
                        // When the spread type is/contains a type parameter,
                        // track it for intersection creation at the end.
                        // This preserves generic identity so that return types
                        // of generic functions are properly instantiated at
                        // call sites. Without this, spreading a type parameter
                        // resolves to constraint properties, losing the generic
                        // information and causing false TS2741/TS2322 errors.
                        let is_generic_spread = is_valid_spread
                            && (tsz_solver::type_param_info(self.ctx.types, spread_type).is_some()
                                || tsz_solver::type_queries::contains_type_parameters_db(
                                    self.ctx.types,
                                    spread_type,
                                ));

                        if is_generic_spread {
                            generic_spread_types.push(spread_type);
                        }

                        let spread_props = self.collect_object_spread_properties(spread_type);
                        let resolved_spread = self.resolve_lazy_type(spread_type);
                        let resolved_spread = self.evaluate_type_with_env(resolved_spread);
                        let resolved_spread =
                            self.resolve_type_for_property_access(resolved_spread);
                        let idx_resolver = tsz_solver::IndexSignatureResolver::new(self.ctx.types);
                        if let Some(value_type) = idx_resolver.resolve_string_index(resolved_spread)
                        {
                            spread_string_index_types.push(value_type);
                        }
                        if let Some(value_type) = idx_resolver.resolve_number_index(resolved_spread)
                        {
                            spread_number_index_types.push(value_type);
                        }

                        // Propagate index signatures from spread source.
                        // When spreading an object with index signatures (e.g.,
                        // `{ ...roindex }` where `roindex: { readonly [x: string]: number }`),
                        // the result should inherit the index signatures (with readonly removed).
                        // These are collected separately and only included in the final type
                        // when the literal has no explicit (non-spread) properties, matching tsc.
                        {
                            use tsz_solver::IndexSignatureResolver;
                            let resolver = IndexSignatureResolver::new(self.ctx.types);
                            if let Some(string_index_value) =
                                resolver.resolve_string_index(resolved_spread)
                            {
                                spread_string_index_types.push(string_index_value);
                            }
                            if let Some(number_index_value) =
                                resolver.resolve_number_index(resolved_spread)
                            {
                                spread_number_index_types.push(number_index_value);
                            }
                        }

                        // TS2783: Check if any earlier named properties will be
                        // overwritten by required properties from this spread.
                        // Only when strict null checks are enabled.
                        // Skip for generic spreads — the constraint properties
                        // are approximations and may include properties that
                        // aren't actually present in the concrete type.
                        if self.ctx.strict_null_checks() && !is_generic_spread {
                            for sp in &spread_props {
                                if !sp.optional
                                    && let Some((prop_node, prop_name)) =
                                        named_property_nodes.get(&sp.name)
                                {
                                    let message = format_message(
                                        diagnostic_messages::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                                        &[prop_name],
                                    );
                                    self.error_at_node(
                                        *prop_node,
                                        &message,
                                        diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                                    );
                                }
                            }
                        }

                        // After TS2783 check, clear the named-property tracking
                        // for properties that the spread overwrites (so only the
                        // first occurrence can trigger the diagnostic, not later
                        // spreads which are spread-vs-spread and exempt).
                        for prop in &spread_props {
                            named_property_nodes.remove(&prop.name);
                        }

                        for prop in &spread_props {
                            properties.insert(prop.name, prop.clone());
                        }

                        // Also apply non-union spread to any existing union branches
                        for branch in &mut union_spread_branches {
                            for prop in &spread_props {
                                branch.insert(prop.name, prop.clone());
                            }
                        }
                    }
                }
            }
            // Other element types (e.g., unknown AST node kinds) are silently skipped
        }

        // Merge spread-contributed index signatures only when the object literal
        // has no explicit (non-spread) properties. In tsc, `{ ...indexedObj, b: 1 }`
        // drops the index signature, but `{ ...indexedObj }` preserves it.
        if explicit_property_names.is_empty() {
            string_index_types.extend(spread_string_index_types);
            number_index_types.extend(spread_number_index_types);
        }

        let object_type = self.finalize_object_literal_type(
            properties,
            display_type_overrides,
            string_index_types,
            number_index_types,
            has_spread,
            has_union_spread,
            union_spread_branches,
            generic_spread_types,
        );

        // Pop this type from stack if we pushed it earlier
        if marker_this_type.is_some() {
            self.ctx.this_type_stack.pop();
        }

        object_type
    }
}
