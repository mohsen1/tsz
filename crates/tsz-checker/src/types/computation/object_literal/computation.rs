//! Object literal property iteration and type computation.
//!
//! Contains the main `get_type_of_object_literal_with_request` function that iterates
//! over object literal elements and builds the resulting object type.

use super::super::object_literal_context::ContextualPropertyPresence;
use crate::context::speculation::DiagnosticSpeculationSnapshot;
use crate::context::{PartialObjectLiteralInitializer, TypingRequest};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::computation::ContextualTypeContext;
use tsz_solver::{TypeId, Visibility};

use super::computation_support::SPREAD_DISPLAY_ORDER_OFFSET;

impl<'a> CheckerState<'a> {
    pub(crate) fn get_type_of_object_literal_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use rustc_hash::FxHashMap;
        use tsz_common::interner::Atom;
        use tsz_solver::{IndexSignature, PropertyInfo};
        let mut contextual_type = request.contextual_type;

        // Strip nullish types from contextual type for object literals.
        // When a parameter is optional (e.g., `options?: Opts`), the contextual type
        // includes `undefined`. Since an object literal can never be `undefined` or
        // `null`, using nullish types as contextual type causes incorrect `this` typing
        // (e.g., `this` becomes `undefined` inside method bodies) and breaks ThisType
        // marker extraction from intersection types like `Opts & ThisType<T>`.
        if let Some(ctx) = contextual_type {
            if ctx == TypeId::UNDEFINED || ctx == TypeId::NULL || ctx == TypeId::VOID {
                contextual_type = None;
            } else {
                let (non_nullish, _) = crate::query_boundaries::common::split_nullish_type(
                    self.ctx.types.as_type_database(),
                    ctx,
                );
                if let Some(non_nullish) = non_nullish
                    && non_nullish != ctx
                {
                    contextual_type = Some(non_nullish);
                }
            }
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(obj) = self.ctx.arena.get_literal_expr(node) else {
            return TypeId::ERROR; // Missing object literal data - propagate error
        };

        if let Some(ctx_ty) = contextual_type {
            // Keep the last real contextual object target we saw for this literal.
            // The same node can be recomputed later under TypingRequest::NONE during
            // diagnostic elaboration, and clearing the side table there loses the
            // richer surface we want to report.
            self.ctx
                .object_literal_tracking
                .contextual_targets
                .insert(idx, ctx_ty);
        }

        tracing::trace!(
            idx = idx.0,
            contextual_type = ?contextual_type.map(|t| t.0),
            contextual_type_display = ?contextual_type.map(|t| self.format_type(t)),
            "get_type_of_object_literal: entry"
        );

        // Collect properties from the object literal (later entries override earlier ones)
        let mut properties: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();
        let all_properties_context_sensitive = !obj.elements.nodes.is_empty()
            && obj.elements.nodes.iter().all(|&element_idx| {
                let Some(element) = self.ctx.arena.get(element_idx) else {
                    return false;
                };

                if let Some(prop) = self.ctx.arena.get_property_assignment(element) {
                    return super::super::contextual::is_contextually_sensitive(
                        self,
                        prop.initializer,
                    );
                }

                if element.kind == syntax_kind_ext::METHOD_DECLARATION {
                    return super::super::contextual::is_contextually_sensitive(self, element_idx);
                }

                element.kind == syntax_kind_ext::GET_ACCESSOR
                    || element.kind == syntax_kind_ext::SET_ACCESSOR
            });
        // Track pre-widened (display) types for freshness model.
        // Maps property name → original literal TypeId before widening.
        // Only populated when a property's type was actually widened.
        let mut display_type_overrides: FxHashMap<Atom, TypeId> = FxHashMap::default();
        let mut string_index_types: Vec<TypeId> = Vec::new();
        let mut number_index_types: Vec<TypeId> = Vec::new();
        // Wide `symbol`-typed computed keys (see `symbol_key_routing`).
        let mut symbol_index_types: Vec<TypeId> = Vec::new();
        // Index signatures inherited from spread sources (kept separate because
        // they should only be included when the literal has no explicit properties —
        // tsc drops spread index signatures when explicit properties exist).
        let mut spread_string_index_signatures: Vec<IndexSignature> = Vec::new();
        let mut spread_number_index_signatures: Vec<IndexSignature> = Vec::new();
        let mut has_spread = false;
        let mut has_any_spread = false;
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
        let mut spread_display_order_base = SPREAD_DISPLAY_ORDER_OFFSET;

        let obj_all_method_names = self.object_literal_callable_member_names(&obj.elements.nodes);
        let circular_return_method_sites =
            self.object_literal_circular_return_method_sites(&obj_all_method_names);

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
        // Check for ThisType<T> marker in contextual type (Vue 2 / Options API
        // pattern) after union narrowing so discriminated object literals choose
        // the matching union member's marker.
        let marker_this_type: Option<TypeId> = if let Some(ctx_type) = contextual_type {
            self.contextual_this_type_from_marker(ctx_type)
        } else {
            None
        };

        // Push this type onto stack if found (methods will pick it up)
        if let Some(mut this_type) = marker_this_type {
            // The ThisType<T> marker may contain unresolved type parameters
            // (e.g., `Data & Readonly<Props> & Instance` before inference completes)
            // or unresolved Lazy references to generic interfaces that need their
            // default type arguments applied (e.g., `ThisType<T & Comp>` where
            // `Comp<U = any>` appears as bare `Lazy(DefId)` without an Application
            // wrapper). Evaluate through the type environment to resolve both
            // cases, ensuring property access on `this` inside method bodies
            // works correctly.
            if crate::query_boundaries::common::contains_type_parameters(self.ctx.types, this_type)
                || crate::query_boundaries::common::contains_lazy_or_recursive(
                    self.ctx.types,
                    this_type,
                )
            {
                this_type = self.evaluate_type_with_env(this_type);
            }
            self.ctx.this_type_stack.push(this_type);
        }
        let prototype_owner_this_type = if self.is_js_file() {
            self.js_prototype_owner_expression_for_node(idx)
                .and_then(|owner_expr| self.js_prototype_owner_function_target(owner_expr))
                .and_then(|owner_target| {
                    self.synthesize_js_constructor_instance_type(owner_target, TypeId::ANY, &[])
                })
        } else {
            None
        };
        let contextual_receiver_this_type = prototype_owner_this_type.or_else(|| {
            self.contextual_object_receiver_this_type(contextual_type, marker_this_type)
        });
        let base_request = request.contextual_opt(contextual_type);
        let partial_initializer_stack_index = self
            .object_literal_variable_initializer_symbol(idx)
            .map(|variable_symbol| {
                self.ctx
                    .object_literal_tracking
                    .partial_initializers
                    .push(PartialObjectLiteralInitializer::new(variable_symbol, idx));
                self.ctx.object_literal_tracking.partial_initializers.len() - 1
            });

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
                let name_opt = if computed_key_is_error
                    || self.object_literal_computed_key_is_wide_symbol(prop.name)
                {
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
                    let initializer_is_function_expression = self
                        .ctx
                        .arena
                        .get(prop.initializer)
                        .is_some_and(|init_node| {
                            init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                        });
                    let initializer_is_local_js_prototype_function =
                        initializer_is_function_expression
                            && self.is_js_file()
                            && self
                                .js_prototype_owner_expression_for_node(prop.initializer)
                                .and_then(|owner_expr| {
                                    self.js_prototype_owner_function_target(owner_expr)
                                })
                                .is_some();
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
                    let function_property_lookup_context = if initializer_is_function_like
                        && original_contextual_type.is_some_and(|ctx_type| {
                            self.primitive_union_member_has_property(ctx_type, &name)
                        }) {
                        original_contextual_type.or(contextual_type)
                    } else {
                        contextual_type
                    };
                    let property_context_type = if let Some(ctx_type) =
                        function_property_lookup_context
                    {
                        let lookup_type = self.contextual_lookup_type(ctx_type);
                        let lookup_presence =
                            self.named_contextual_property_presence(lookup_type, &name);
                        let allows_callable_fallback =
                            matches!(lookup_presence, ContextualPropertyPresence::Present);
                        let mut property_context_type =
                            self.contextual_object_property_type_for_lookup(ctx_type, &name);
                        // For optional callable properties (e.g., `set?` in ProxyHandler),
                        // the contextual type includes `undefined` from optionality. When
                        // the property IS present in the literal, that `undefined` is
                        // irrelevant for callable inference — strip it so a generic
                        // wrapper like `deprecate<T extends Function>(fn: T): T` can
                        // infer T as the handler function type rather than falling back
                        // to `Function`.
                        //
                        // Restrict the strip to callable property types so non-callable
                        // optional properties (e.g., `y?: number` in `{ y?: number }`)
                        // keep `undefined` in their contextual type. tsc's
                        // `getTypeOfPropertyOfContextualType` always returns
                        // `T | undefined` for such optional properties, so a generic
                        // call like `match<T>(cb)` used as a value still infers
                        // `T = number | undefined` and TS18048 fires inside the
                        // callback as expected.
                        if let Some(pct) = property_context_type {
                            let stripped = crate::query_boundaries::common::remove_undefined(
                                self.ctx.types,
                                pct,
                            );
                            if stripped != TypeId::UNDEFINED
                                && stripped != pct
                                && self.stripped_property_context_is_callable(stripped)
                            {
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
                                function_property_lookup_context,
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
                    //
                    // Treat `unknown`, `any`, and `never` as "no real context" for
                    // widening purposes. tsc's `isLiteralOfContextualType` returns
                    // false for these types, so property literals widen normally
                    // (e.g., `{ a: 1 } satisfies unknown` produces `{ a: number }`,
                    // not `{ a: 1 }`).
                    let had_object_context = contextual_type.is_some_and(|ct| {
                        !crate::query_boundaries::type_computation::core::is_literal_permissive_object_context(
                            ct,
                        )
                    });
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
                            self.primitive_union_member_has_property(ctx_type, &name)
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
                    if let Some(diag_target) = jsdoc_declared_type
                        .or(property_context_type)
                        .or(resolved_prop_ctx)
                    {
                        self.ctx
                            .object_literal_tracking
                            .property_diag_targets
                            .insert(elem_idx, diag_target);
                    }
                    let property_request = base_request.contextual_opt(
                        self.contextual_type_option_for_call_argument_at(
                            resolved_prop_ctx,
                            prop.initializer,
                            None,
                            None,
                            crate::call_checker::CallableContext::none(),
                        )
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
                    let mut value_type = if prop.initializer == prop.name {
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
                        let mut pushed_prop_fn_this = false;
                        if initializer_is_function_expression
                            && marker_this_type.is_none()
                            && self.current_this_type().is_none()
                        {
                            if let Some(receiver_this_type) = contextual_receiver_this_type {
                                self.ctx.this_type_stack.push(receiver_this_type);
                                pushed_prop_fn_this = true;
                            } else if let Some(ctx_type) = contextual_type {
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
                        if !initializer_is_local_js_prototype_function
                            && self.request_has_concrete_contextual_type(&property_request)
                            && property_request.contextual_type != Some(TypeId::NEVER)
                        {
                            // Only clear within parameter spans where the
                            // refresh actually produced a non-`any` symbol
                            // type. This keeps the genuine contextual-typing
                            // wins (annotated targets, mapped/generic targets)
                            // while preserving TS7006 for IIFE-arg shapes
                            // where the "contextual type" is the function's
                            // own value type and doesn't constrain the param.
                            let spans =
                                self.contextually_typed_param_spans_for_node(prop.initializer);
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
                    if circular_return_method_sites.contains(&elem_idx)
                        && initializer_is_function_expression
                        && jsdoc_declared_type.is_none()
                        && property_request.contextual_type.is_none()
                        && self.ctx.no_implicit_any()
                        && !self.has_syntax_parse_errors()
                        && !self.is_js_file()
                    {
                        use crate::diagnostics::diagnostic_codes;
                        self.error_at_node_msg(
                            prop.name,
                            diagnostic_codes::IMPLICITLY_HAS_RETURN_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_RETURN_TYPE_ANNOTATION,
                            &[&name],
                        );
                        value_type =
                            crate::query_boundaries::assignability::replace_function_return_type(
                                self.ctx.types,
                                value_type,
                                TypeId::ANY,
                            );
                    }

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
                                crate::query_boundaries::common::remove_undefined(
                                    self.ctx.types,
                                    declared_type,
                                );
                            self.check_assignable_or_report_at_exact_anchor(
                                value_type,
                                declared_check_type,
                                prop.initializer,
                                prop.name,
                            );
                        }
                        declared_type
                    } else {
                        let value_has_non_widening_source = self
                            .expression_is_type_assertion(prop.initializer)
                            || self.identifier_refers_to_non_widening_declared_value_type(
                                prop.initializer,
                            )
                            || self
                                .object_literal_property_access_literal_type(prop.initializer)
                                .is_some();
                        // Apply bidirectional type inference - use contextual type to narrow
                        // the value type, except for function-like values with explicit
                        // signature annotations. For those, tsc preserves the explicit
                        // source signature in diagnostics (e.g. lastPropertyInLiteralWins).
                        let value_type = if initializer_is_function_like
                            && self
                                .function_like_has_explicit_signature_annotations(prop.initializer)
                        {
                            value_type
                        } else if value_has_non_widening_source {
                            self.literal_type_from_initializer(prop.initializer)
                                .or_else(|| {
                                    self.object_literal_property_access_literal_type(
                                        prop.initializer,
                                    )
                                })
                                .unwrap_or(value_type)
                        } else {
                            let applied = crate::query_boundaries::common::apply_contextual_type(
                                self.ctx.types,
                                value_type,
                                property_context_type,
                            );
                            let applied = self.reduce_literal_index_access_property_types(applied);
                            self.object_literal_property_access_literal_type(prop.initializer)
                                .unwrap_or(applied)
                        };

                        // Widen literal types for object literal properties.
                        // Object literal properties are mutable by default, so `{ x: "a" }`
                        // produces `{ x: string }`. Only preserve literals when:
                        // - A const assertion is active (`as const`)
                        // - A contextual type narrows the property to a literal
                        // - The value is a type assertion (`as T` / `<T>expr`) or an identifier
                        //   whose declaration is non-widening (const-asserted or literal-annotated).
                        let final_type = if self.should_widen_object_property_literal(
                            value_type,
                            property_context_type,
                            had_object_context,
                            value_has_non_widening_source,
                        ) {
                            self.widen_literal_type(value_type)
                        } else {
                            value_type
                        };

                        let recheck_key_remapped_property = if let Some(ctx_type) = contextual_type
                        {
                            let mut evaluate = |ty| self.evaluate_contextual_type(ty);
                            self.object_literal_property_is_typed_variable_initializer(elem_idx)
                                && crate::query_boundaries::type_origin::originates_from_remapped_mapped_type_with_evaluator(
                                        self.ctx.types,
                                        ctx_type,
                                        &mut evaluate,
                                    )
                        } else {
                            false
                        };
                        let recheck_contextual_property = (self
                            .object_literal_property_has_conditional_mapped_annotation(elem_idx)
                            || self.object_literal_property_has_conditional_annotation(elem_idx)
                            || self.object_literal_property_has_mapped_annotation(elem_idx))
                            && property_context_type.is_some()
                            && original_contextual_type == contextual_type
                            && !self.ctx.arena.get(prop.name).is_some_and(|name| {
                                name.kind
                                    == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME
                            });

                        if (recheck_key_remapped_property || recheck_contextual_property)
                            && let Some(check_target) = property_context_type
                            && value_type != TypeId::ERROR
                            && value_type != TypeId::ANY
                            && check_target != TypeId::ERROR
                            && check_target != TypeId::ANY
                            && !self.is_assignable_to(value_type, check_target)
                        {
                            let _ = self.check_assignable_or_report_at_exact_anchor(
                                value_type,
                                check_target,
                                prop.initializer,
                                prop.name,
                            );
                        }

                        // Freshness model: record the literal property value from
                        // the AST for display in error messages. The canonical
                        // property type may widen for checking, but assignment
                        // diagnostics into literal-sensitive targets still need the
                        // original object-literal surface (`{ c: true }`, not
                        // `{ c: boolean }`). Non-literal-sensitive diagnostic paths
                        // already widen these display properties back to primitives.
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
                    // Determine if this property was declared with a string key
                    // that looks numeric (e.g. "404" vs 404). This affects DTS
                    // emit quoting: `"404": ...` vs `404: ...`.
                    let (is_string_named, is_symbol_named, single_quoted_name) =
                        self.object_literal_member_naming_flags(prop.name);
                    let prop_info = PropertyInfo {
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
                        is_string_named,
                        is_symbol_named,
                        single_quoted_name,
                    };
                    properties.insert(name_atom, prop_info.clone());
                    self.record_partial_object_literal_property(
                        partial_initializer_stack_index,
                        &prop_info,
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
                        let mut handled_by_literal_type = false;
                        if let Some(atom) =
                            crate::query_boundaries::type_computation::access::literal_property_name(
                                self.ctx.types,
                                prop_name_type,
                            )
                        {
                            handled_by_literal_type = true;
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
                        if !handled_by_name
                            && !handled_by_literal_type
                            && let Some(name) =
                                self.simple_computed_call_name_for_duplicates(prop.name)
                        {
                            let atom = self.ctx.types.intern_string(&name);
                            if resolved_computed_name.is_none() {
                                resolved_computed_name = Some(name.clone());
                            }
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
                    let property_request = base_request.contextual_opt(
                        self.contextual_type_option_for_call_argument_at(
                            index_ctx_type,
                            prop.initializer,
                            None,
                            None,
                            crate::call_checker::CallableContext::none(),
                        ),
                    );
                    let mut value_type =
                        self.get_type_of_node_with_request(prop.initializer, &property_request);
                    if self.ctx.in_const_assertion
                        && let Some(literal_type) =
                            self.literal_type_from_initializer(prop.initializer)
                    {
                        value_type = literal_type;
                    }

                    self.route_computed_member_value_to_index_signature(
                        prop_name_type,
                        value_type,
                        &mut number_index_types,
                        &mut string_index_types,
                        &mut symbol_index_types,
                    );
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

                    // Set contextual type for shorthand property value.
                    let had_object_context = contextual_type.is_some_and(|ct| {
                        !crate::query_boundaries::type_computation::core::is_literal_permissive_object_context(
                            ct,
                        )
                    });
                    if let Some(diag_target) = jsdoc_declared_type.or(property_context_type) {
                        self.ctx
                            .object_literal_tracking
                            .property_diag_targets
                            .insert(elem_idx, diag_target);
                    }
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
                            if shorthand.equals_token && !self.ctx.in_destructuring_target {
                                // TS1312: shorthand `{ s = 5 }` in non-destructuring context.
                                // tsc suggests using `:` instead of `=`.
                                let message = "Did you mean to use a ':'? An '=' can only follow a property name when the containing object literal is part of a destructuring pattern.";
                                if shorthand.equals_token_pos > 0 {
                                    self.error_at_position(
                                        shorthand.equals_token_pos,
                                        1,
                                        message,
                                        1312,
                                    );
                                } else {
                                    self.error_at_node(shorthand.name, message, 1312);
                                }
                            } else {
                                // TS18004: Missing value binding for shorthand property name
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
                        self.get_type_of_assignment_target(shorthand_name_idx)
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
                        let value_type = crate::query_boundaries::common::apply_contextual_type(
                            self.ctx.types,
                            value_type,
                            property_context_type,
                        );
                        let shorthand_is_non_widening = shorthand_sym.is_some_and(|sym_id| {
                            self.sym_has_non_widening_declared_value_type(sym_id)
                        });
                        if self.should_widen_object_property_literal(
                            value_type,
                            property_context_type,
                            had_object_context,
                            shorthand_is_non_widening,
                        ) {
                            self.widen_literal_type(value_type)
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
                    let prop_info = PropertyInfo {
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
                        is_string_named: false,
                        is_symbol_named: false,
                        single_quoted_name: false,
                    };
                    properties.insert(name_atom, prop_info.clone());
                    self.record_partial_object_literal_property(
                        partial_initializer_stack_index,
                        &prop_info,
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
                let name_opt = if self.object_literal_computed_key_is_wide_symbol(method.name) {
                    None
                } else {
                    self.get_property_name_resolved(method.name)
                };
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
                    let method_property_context_type = contextual_type.and_then(|ctx_type| {
                        self.contextual_object_property_type_for_lookup(ctx_type, &name)
                    });
                    let method_context_type = self.substitute_contextual_this_type(
                        method_context_type,
                        contextual_receiver_this_type,
                    );
                    let method_property_context_type = self.substitute_contextual_this_type(
                        method_property_context_type,
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
                        // Prefer the method's contextual `this` type (e.g., from an
                        // interface declaration `(this: { options: T }) => R`) over the
                        // outer object's contextual type. This ensures that in Round 2 of
                        // call inference, when type params are instantiated, the method
                        // body sees concrete `this` types (fixing TS2783 for spreads of
                        // `this.options.suggestion` where Options = { suggestion: Foo }).
                        let method_ctx_this = method_request.contextual_type.and_then(|ctx_ty| {
                            let ctx_helper = ContextualTypeContext::with_expected_and_options(
                                self.ctx.types,
                                ctx_ty,
                                self.ctx.compiler_options.no_implicit_any,
                            );
                            ctx_helper.get_this_type()
                        });
                        if let Some(mut method_this) = method_ctx_this {
                            if crate::query_boundaries::common::contains_type_parameters(
                                self.ctx.types,
                                method_this,
                            ) || crate::query_boundaries::common::contains_lazy_or_recursive(
                                self.ctx.types,
                                method_this,
                            ) {
                                method_this = self.evaluate_type_with_env(method_this);
                            }
                            self.ctx.this_type_stack.push(method_this);
                            pushed_contextual_this = true;
                        } else if let Some(receiver_this_type) = contextual_receiver_this_type {
                            self.ctx.this_type_stack.push(receiver_this_type);
                            pushed_contextual_this = true;
                        } else if let Some(ctx_type) = contextual_type {
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
                            let ctx_helper = ContextualTypeContext::with_expected_and_options(
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

                    let method_diag_snap = DiagnosticSpeculationSnapshot::new(&self.ctx);
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
                                .speculative_diagnostics_since(method_diag_snap.snapshot())
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
                        method_diag_snap.rollback(&mut self.ctx.diagnostic_state());
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
                        let rerun_snap = DiagnosticSpeculationSnapshot::new(&self.ctx);
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
                        rerun_snap.rollback_filtered(&mut self.ctx.diagnostic_state(), |diag| {
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

                    if circular_return_method_sites.contains(&elem_idx)
                        && pushed_synthetic_this
                        && jsdoc_declared_type.is_none()
                        && method.type_annotation.is_none()
                        && !has_concrete_method_context
                        && self.ctx.no_implicit_any()
                        && !self.has_syntax_parse_errors()
                        && !self.is_js_file()
                    {
                        use crate::diagnostics::diagnostic_codes;
                        self.error_at_node_msg(
                            method.name,
                            diagnostic_codes::IMPLICITLY_HAS_RETURN_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_RETURN_TYPE_ANNOTATION,
                            &[&name],
                        );
                        method_type =
                            crate::query_boundaries::assignability::replace_function_return_type(
                                self.ctx.types,
                                method_type,
                                TypeId::ANY,
                            );
                    }

                    let method_type = jsdoc_declared_type.unwrap_or_else(|| {
                        if name == "return" || matches!(method_type, TypeId::ANY | TypeId::UNKNOWN)
                        {
                            method_property_context_type
                                .filter(|&context_type| {
                                    !matches!(context_type, TypeId::ANY | TypeId::UNKNOWN)
                                })
                                .unwrap_or(method_type)
                        } else {
                            method_type
                        }
                    });

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
                    let (is_string_named, is_symbol_named, single_quoted_name) =
                        self.object_literal_member_naming_flags(method.name);
                    let prop_info = PropertyInfo {
                        name: name_atom,
                        type_id: method_type,
                        write_type: method_type,
                        // A method shorthand may carry `?` — `{ a?() {} }` —
                        // in which case the inferred property type must be
                        // optional so the .d.ts emits `a?(): void`.
                        optional: method.question_token,
                        readonly: false,
                        is_method: true, // Object literal methods should be bivariant
                        is_class_prototype: false,
                        visibility: Visibility::Public,
                        parent_id: None,
                        declaration_order: order,
                        is_string_named,
                        is_symbol_named,
                        single_quoted_name,
                    };
                    properties.insert(name_atom, prop_info.clone());
                    self.record_partial_object_literal_property(
                        partial_initializer_stack_index,
                        &prop_info,
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

                    self.route_computed_member_value_to_index_signature(
                        prop_name_type,
                        method_type,
                        &mut number_index_types,
                        &mut string_index_types,
                        &mut symbol_index_types,
                    );
                }
            }
            // Accessor: { get foo() {} } or { set foo(v) {} }
            else if self.process_object_literal_accessor_element(
                elem_idx,
                &obj_getter_names,
                contextual_type,
                marker_this_type,
                &mut properties,
                &mut setter_names,
                &mut getter_names,
                &mut explicit_property_names,
                skip_duplicate_check,
                &mut prop_order,
                partial_initializer_stack_index,
                &mut number_index_types,
                &mut string_index_types,
                &mut symbol_index_types,
            ) {
            }
            // Spread assignment: { ...obj }
            else if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                || elem_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT
            {
                if let Some(spread_type) = self.process_object_literal_spread_element(
                    elem_idx,
                    obj.elements.nodes.len(),
                    &base_request,
                    contextual_type,
                    marker_this_type,
                    partial_initializer_stack_index,
                    &mut properties,
                    &mut named_property_nodes,
                    &mut union_spread_branches,
                    &mut spread_string_index_signatures,
                    &mut spread_number_index_signatures,
                    &mut generic_spread_types,
                    &mut has_spread,
                    &mut has_any_spread,
                    &mut has_union_spread,
                    &mut spread_display_order_base,
                ) {
                    return spread_type;
                }
            }

            // Other element types (e.g., unknown AST node kinds) are silently skipped
        }

        // Merge spread-contributed index signatures only when the object literal
        // has no explicit (non-spread) properties. In tsc, `{ ...indexedObj, b: 1 }`
        // drops the index signature, but `{ ...indexedObj }` preserves it.
        let mut string_index_param_name = None;
        let mut number_index_param_name = None;
        if explicit_property_names.is_empty() {
            string_index_param_name = spread_string_index_signatures
                .iter()
                .filter(|idx| idx.key_type != TypeId::SYMBOL)
                .find_map(|idx| idx.param_name);
            number_index_param_name = spread_number_index_signatures
                .iter()
                .find_map(|idx| idx.param_name);
            // `ObjectShape.string_index` is shared between string- and
            // symbol-keyed signatures (discriminated by `key_type`). Route each
            // spread-contributed index into the matching bucket so a spread
            // source with `{ [k: symbol]: V }` does not leak `V` into a
            // string index signature.
            for idx in spread_string_index_signatures {
                if idx.key_type == TypeId::SYMBOL {
                    symbol_index_types.push(idx.value_type);
                } else {
                    string_index_types.push(idx.value_type);
                }
            }
            number_index_types.extend(
                spread_number_index_signatures
                    .into_iter()
                    .map(|idx| idx.value_type),
            );
        }

        let object_type = self.finalize_object_literal_type(
            super::super::object_literal_support::ObjectLiteralFinalizeCtx {
                properties,
                display_type_overrides,
                string_index_types,
                number_index_types,
                symbol_index_types,
                string_index_param_name,
                number_index_param_name,
                has_spread,
                has_any_spread,
                has_union_spread,
                union_spread_branches,
                generic_spread_types,
                all_properties_context_sensitive,
            },
        );

        // Check getter/setter type compatibility for object literal accessors.
        // When a getter has no explicit return type annotation, its type is inferred
        // from the body and must be compatible with the setter's parameter type.
        // This matches tsc behavior for object literals with accessor pairs.
        self.check_object_literal_accessor_type_compatibility(&obj.elements.nodes);

        self.pop_object_literal_contexts(marker_this_type, partial_initializer_stack_index);

        object_type
    }
}
