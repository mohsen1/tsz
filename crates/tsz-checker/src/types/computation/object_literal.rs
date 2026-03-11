//! Object literal type computation.
//!
//! Handles typing of object literal expressions including property assignments,
//! shorthand properties, method shorthands, getters/setters, spread properties,
//! duplicate property detection, and contextual type inference.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{CallSignature, CallableShape, TypeId, Visibility};

impl<'a> CheckerState<'a> {
    pub(crate) fn contextual_object_literal_property_type(
        &mut self,
        contextual_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        let union_member_property_type = |this: &mut Self,
                                          union_type: TypeId,
                                          property_name: &str|
         -> Option<TypeId> {
            let members = tsz_solver::type_queries::get_union_members(this.ctx.types, union_type)
                .or_else(|| {
                match crate::query_boundaries::assignability::classify_for_excess_properties(
                    this.ctx.types,
                    union_type,
                ) {
                    crate::query_boundaries::assignability::ExcessPropertiesKind::Union(
                        members,
                    ) => Some(members),
                    _ => None,
                }
            })?;
            let mut property_types = Vec::new();

            for &member in &members {
                let resolved_member = this.resolve_type_for_property_access(member);
                let evaluated_member = this.evaluate_type_with_env(member);
                let evaluated_member_for_property_access =
                    this.resolve_type_for_property_access(evaluated_member);
                let evaluated_member_for_property_access =
                    this.resolve_lazy_type(evaluated_member_for_property_access);
                let evaluated_member_for_property_access =
                    this.evaluate_application_type(evaluated_member_for_property_access);
                let mut property_type = this
                    .ctx
                    .types
                    .contextual_property_type(member, property_name);

                // When the property type is `any`, it may come from an index signature
                // in an intersection with unresolved Lazy members (e.g.,
                // `Lazy(Interface) & { [k: string]: any }`). Try the resolved paths
                // which can evaluate Lazy types to get the specific property type.
                if (property_type.is_none() || property_type == Some(tsz_solver::TypeId::ANY))
                    && let Some(pt) = this
                        .ctx
                        .types
                        .contextual_property_type(resolved_member, property_name)
                    && (pt != tsz_solver::TypeId::ANY || property_type.is_none())
                {
                    property_type = Some(pt);
                }

                if (property_type.is_none() || property_type == Some(tsz_solver::TypeId::ANY))
                    && let Some(pt) = this.ctx.types.contextual_property_type(
                        evaluated_member_for_property_access,
                        property_name,
                    )
                    && (pt != tsz_solver::TypeId::ANY || property_type.is_none())
                {
                    property_type = Some(pt);
                }

                let mut alternate_member_for_property_access = None;
                if property_type.is_none() || property_type == Some(tsz_solver::TypeId::ANY) {
                    use tsz_solver::TypeEvaluator;

                    let mut evaluator = TypeEvaluator::with_resolver(this.ctx.types, &this.ctx);
                    let alternate_member = evaluator.evaluate(member);
                    let alternate_member = this.resolve_type_for_property_access(alternate_member);
                    let alternate_member = this.resolve_lazy_type(alternate_member);
                    let alternate_member = this.evaluate_application_type(alternate_member);
                    alternate_member_for_property_access = Some(alternate_member);
                    property_type = this
                        .ctx
                        .types
                        .contextual_property_type(alternate_member, property_name);
                }

                let property_type = property_type;
                if property_type.is_none() {
                    tracing::trace!(
                        union_type = union_type.0,
                        union_type_str = %this.format_type(union_type),
                        property_name,
                        member = member.0,
                        member_str = %this.format_type(member),
                        resolved_member = resolved_member.0,
                        resolved_member_str = %this.format_type(resolved_member),
                        evaluated_member = evaluated_member.0,
                        evaluated_member_str = %this.format_type(evaluated_member),
                        evaluated_member_for_property_access = evaluated_member_for_property_access.0,
                        evaluated_member_for_property_access_str = %this.format_type(evaluated_member_for_property_access),
                        alternate_member_for_property_access = alternate_member_for_property_access.map(|id| id.0),
                        alternate_member_for_property_access_str = alternate_member_for_property_access
                            .map(|id| this.format_type(id))
                            .unwrap_or_default(),
                        "contextual_object_literal_property_type: union-member miss"
                    );
                }
                if let Some(property_type) = property_type {
                    property_types.push(property_type);
                }
            }

            if property_types.is_empty() {
                None
            } else {
                Some(
                    this.ctx
                        .types
                        .factory()
                        .union_preserve_members(property_types),
                )
            }
        };
        let original_contextual_type = contextual_type;
        let mut best_property_type = None;
        let env_property_type = if matches!(
            self.resolve_property_access_with_env(original_contextual_type, property_name),
            tsz_solver::operations::property::PropertyAccessResult::Success { .. }
        ) {
            match self.resolve_property_access_with_env(original_contextual_type, property_name) {
                tsz_solver::operations::property::PropertyAccessResult::Success {
                    type_id, ..
                } => Some(type_id),
                _ => None,
            }
        } else {
            None
        };
        if let Some(property_type) = self
            .ctx
            .types
            .contextual_property_type(original_contextual_type, property_name)
        {
            // When the property type is `any`, it may come from an index signature
            // in a distributed intersection. Don't return eagerly — fall through
            // to resolved paths which can extract the specific property type.
            if property_type != tsz_solver::TypeId::ANY {
                tracing::trace!(
                    contextual_type = original_contextual_type.0,
                    property_name,
                    property_type = property_type.0,
                    "contextual_object_literal_property_type: pre-eval extracted"
                );
                best_property_type = self.prefer_more_specific_contextual_property_type(
                    best_property_type,
                    property_type,
                );
            }

            if let Some(env_property_type) = env_property_type {
                best_property_type = self.prefer_more_specific_contextual_property_type(
                    best_property_type,
                    env_property_type,
                );
            }
        }

        if let Some(property_type) =
            union_member_property_type(self, original_contextual_type, property_name)
        {
            tracing::trace!(
                contextual_type = original_contextual_type.0,
                property_name,
                property_type = property_type.0,
                "contextual_object_literal_property_type: union-member extracted"
            );
            best_property_type = self
                .prefer_more_specific_contextual_property_type(best_property_type, property_type);
        }

        let resolved_original_contextual_type =
            self.resolve_type_for_property_access(original_contextual_type);
        if resolved_original_contextual_type != original_contextual_type
            && let Some(property_type) = self
                .ctx
                .types
                .contextual_property_type(resolved_original_contextual_type, property_name)
        {
            tracing::trace!(
                original_contextual_type = original_contextual_type.0,
                resolved_original_contextual_type = resolved_original_contextual_type.0,
                property_name,
                property_type = property_type.0,
                "contextual_object_literal_property_type: resolved-original extracted"
            );
            best_property_type = self
                .prefer_more_specific_contextual_property_type(best_property_type, property_type);
        }

        if resolved_original_contextual_type != original_contextual_type
            && let Some(property_type) =
                union_member_property_type(self, resolved_original_contextual_type, property_name)
        {
            tracing::trace!(
                original_contextual_type = original_contextual_type.0,
                resolved_original_contextual_type = resolved_original_contextual_type.0,
                property_name,
                property_type = property_type.0,
                "contextual_object_literal_property_type: resolved-union-member extracted"
            );
            best_property_type = self
                .prefer_more_specific_contextual_property_type(best_property_type, property_type);
        }

        // Cache the expensive contextual type resolution chain.
        // The same contextual type is resolved for each property of an object literal,
        // so caching saves O(properties-1) full resolution chains per literal.
        let contextual_type = if let Some(&cached) = self
            .ctx
            .narrowing_cache
            .contextual_resolve_cache
            .borrow()
            .get(&original_contextual_type)
        {
            cached
        } else {
            let ct = self.evaluate_contextual_type(contextual_type);
            let ct = self.evaluate_type_with_env(ct);
            let ct = self.resolve_type_for_property_access(ct);
            let ct = self.resolve_lazy_type(ct);
            let ct = self.evaluate_application_type(ct);
            self.ctx
                .narrowing_cache
                .contextual_resolve_cache
                .borrow_mut()
                .insert(original_contextual_type, ct);
            ct
        };

        if contextual_type == TypeId::UNKNOWN {
            return Some(best_property_type.unwrap_or(TypeId::UNKNOWN));
        }

        if let Some(property_type) = self
            .ctx
            .types
            .contextual_property_type(contextual_type, property_name)
        {
            tracing::trace!(
                contextual_type = contextual_type.0,
                property_name,
                property_type = property_type.0,
                "contextual_object_literal_property_type: extracted"
            );
            best_property_type = self
                .prefer_more_specific_contextual_property_type(best_property_type, property_type);
        }

        if let Some(type_id) = env_property_type {
            tracing::trace!(
                contextual_type = contextual_type.0,
                property_name,
                property_type = type_id.0,
                "contextual_object_literal_property_type: env property access extracted"
            );
            best_property_type =
                self.prefer_more_specific_contextual_property_type(best_property_type, type_id);
        }

        let alternate_contextual_type = {
            use tsz_solver::TypeEvaluator;

            let mut evaluator = TypeEvaluator::with_resolver(self.ctx.types, &self.ctx);
            evaluator.evaluate(original_contextual_type)
        };
        if alternate_contextual_type != contextual_type {
            let alternate_contextual_type =
                self.resolve_type_for_property_access(alternate_contextual_type);
            let alternate_contextual_type = self.resolve_lazy_type(alternate_contextual_type);
            let alternate_contextual_type =
                self.evaluate_application_type(alternate_contextual_type);
            if let Some(property_type) = self
                .ctx
                .types
                .contextual_property_type(alternate_contextual_type, property_name)
            {
                tracing::trace!(
                    original_contextual_type = original_contextual_type.0,
                    alternate_contextual_type = alternate_contextual_type.0,
                    property_name,
                    property_type = property_type.0,
                    "contextual_object_literal_property_type: alternate extracted"
                );
                best_property_type = self.prefer_more_specific_contextual_property_type(
                    best_property_type,
                    property_type,
                );
            }
        }

        if let Some(property_type) = best_property_type {
            return Some(self.sanitize_contextual_property_type(property_type));
        }

        // If contextual extraction fails but the parent context is generic/deferred,
        // preserve an `unknown` contextual slot to prevent false implicit-any
        // diagnostics during higher-order inference rounds.
        if tsz_solver::type_queries::contains_type_parameters_db(self.ctx.types, contextual_type) {
            tracing::trace!(
                contextual_type = contextual_type.0,
                property_name,
                "contextual_object_literal_property_type: deferred unknown"
            );
            return Some(TypeId::UNKNOWN);
        }

        tracing::trace!(
            original_contextual_type = original_contextual_type.0,
            original_contextual_type_str = %self.format_type(original_contextual_type),
            contextual_type = contextual_type.0,
            contextual_type_str = %self.format_type(contextual_type),
            property_name,
            "contextual_object_literal_property_type: no property type"
        );
        None
    }

    fn prefer_more_specific_contextual_property_type(
        &self,
        current: Option<TypeId>,
        candidate: TypeId,
    ) -> Option<TypeId> {
        let Some(current) = current else {
            return Some(candidate);
        };

        if current == candidate {
            return Some(current);
        }

        if matches!(current, TypeId::ANY | TypeId::UNKNOWN)
            && !matches!(candidate, TypeId::ANY | TypeId::UNKNOWN)
        {
            return Some(candidate);
        }
        if matches!(candidate, TypeId::ANY | TypeId::UNKNOWN)
            && !matches!(current, TypeId::ANY | TypeId::UNKNOWN)
        {
            return Some(current);
        }

        let current_eval = tsz_solver::evaluate_type(self.ctx.types, current);
        let candidate_eval = tsz_solver::evaluate_type(self.ctx.types, candidate);
        let candidate_narrower =
            tsz_solver::is_subtype_of(self.ctx.types, candidate_eval, current_eval);
        let current_narrower =
            tsz_solver::is_subtype_of(self.ctx.types, current_eval, candidate_eval);

        if candidate_narrower && !current_narrower {
            Some(candidate)
        } else {
            Some(current)
        }
    }

    fn sanitize_contextual_property_type(&self, property_type: TypeId) -> TypeId {
        if property_type == TypeId::ERROR
            || tsz_solver::type_queries::contains_error_type_db(self.ctx.types, property_type)
        {
            return TypeId::UNKNOWN;
        }
        if let Some(tsz_solver::TypeData::TypeParameter(info) | tsz_solver::TypeData::Infer(info)) =
            self.ctx.types.lookup(property_type)
            && let Some(default) = info.default
        {
            return default;
        }
        property_type
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
    pub(crate) fn get_type_of_object_literal(&mut self, idx: NodeIndex) -> TypeId {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use rustc_hash::FxHashMap;
        use tsz_common::interner::Atom;
        use tsz_solver::PropertyInfo;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(obj) = self.ctx.arena.get_literal_expr(node) else {
            return TypeId::ERROR; // Missing object literal data - propagate error
        };

        // Collect properties from the object literal (later entries override earlier ones)
        let mut properties: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();
        let mut string_index_types: Vec<TypeId> = Vec::new();
        let mut number_index_types: Vec<TypeId> = Vec::new();
        let mut has_spread = false;
        let mut has_union_spread = false;
        let mut union_spread_branches: Vec<FxHashMap<Atom, PropertyInfo>> = Vec::new();
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
        let marker_this_type: Option<TypeId> = if let Some(ctx_type) = self.ctx.contextual_type {
            use tsz_solver::ContextualTypeContext;
            let ctx_helper = ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                ctx_type,
                self.ctx.compiler_options.no_implicit_any,
            );
            ctx_helper.get_this_type_from_marker()
        } else {
            None
        };

        // Push this type onto stack if found (methods will pick it up)
        if let Some(this_type) = marker_this_type {
            self.ctx.this_type_stack.push(this_type);
        }

        // Pre-scan: collect ALL method names from the object literal so that
        // the synthetic `this` type includes placeholders for all methods,
        // enabling mutually-recursive methods to resolve `this.otherMethod`.
        let obj_all_method_names: rustc_hash::FxHashSet<Atom> = obj
            .elements
            .nodes
            .iter()
            .filter_map(|&elem_idx| {
                let elem_node = self.ctx.arena.get(elem_idx)?;
                let method = self.ctx.arena.get_method_decl(elem_node)?;
                let name = self.get_property_name(method.name)?;
                Some(self.ctx.types.intern_string(&name))
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

        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Property assignment: { x: value }
            if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                let is_computed_name = self
                    .ctx
                    .arena
                    .get(prop.name)
                    .is_some_and(|prop_name_node| {
                        prop_name_node.kind
                            == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME
                    });
                if let Some(prop_name_node) = self.ctx.arena.get(prop.name)
                    && prop_name_node.kind
                        == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME
                {
                    // Always run TS2464 validation for computed property names, even when
                    // the name can be resolved to a literal atom.
                    self.check_computed_property_name(prop.name);
                }

                let name_opt = self.get_property_name_resolved(prop.name);
                if !is_computed_name && let Some(name) = name_opt.clone() {
                    // Get contextual type for this property.
                    // For mapped/conditional/application types that contain Lazy references
                    // (e.g. { [K in keyof Props]: Props[K] } after generic inference),
                    // evaluate them with the full resolver first so the solver can
                    // extract property types from the resulting concrete object type.
                    let property_context_type = if let Some(ctx_type) = self.ctx.contextual_type {
                        self.contextual_object_literal_property_type(ctx_type, &name)
                    } else {
                        None
                    };

                    // JSDoc @type on object literal properties acts as the declared
                    // type for the property. When present:
                    // - The property type in the resulting object is the @type type
                    // - The initializer is checked for assignability against it
                    // - The @type type is used as contextual type so literals are preserved
                    // This matches tsc behavior for JS files with checkJs/ts-check.
                    let jsdoc_declared_type = self.jsdoc_type_annotation_for_node_direct(elem_idx);

                    // Set contextual type for property value.
                    // When a JSDoc @type is present, use it as the contextual type
                    // so that literal values like `"a"` preserve their literal type
                    // (e.g., `@type {"a"}` + `a: "a"` should not widen to `string`).
                    let prev_context = self.ctx.contextual_type;
                    let had_object_context = prev_context.is_some();
                    self.ctx.contextual_type = self.contextual_type_option_for_expression(
                        jsdoc_declared_type.or(property_context_type),
                    );

                    // When the parser can't parse a value expression (e.g. `{ a: return; }`),
                    // it uses the property NAME node as the fallback initializer for error
                    // recovery (prop.initializer == prop.name). Skip type-checking in that
                    // case to prevent a spurious TS2304 for the property name identifier.
                    let value_type = if prop.initializer == prop.name {
                        TypeId::ANY
                    } else {
                        self.get_type_of_node(prop.initializer)
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

                    // In destructuring assignment targets with defaults
                    // (e.g. `{ a: target = default } = source`), the property type
                    // for the target object should be the target variable's type,
                    // not the assignment expression's type.  The assignment
                    // expression `target = default` returns the default's type
                    // which may differ from the target's declared type.
                    let value_type = if self.ctx.in_destructuring_target
                        && let Some(init_node) = self.ctx.arena.get(prop.initializer)
                        && init_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                        && let Some(bin) = self.ctx.arena.get_binary_expr(init_node)
                        && bin.operator_token == tsz_scanner::SyntaxKind::EqualsToken as u16
                    {
                        // Use the target (LHS) type as the property type
                        self.get_type_of_assignment_target(bin.left)
                    } else {
                        value_type
                    };

                    // Restore context
                    self.ctx.contextual_type = prev_context;

                    // When a JSDoc @type annotation is present, check assignability
                    // of the initializer against the declared type, and use the
                    // declared type as the property type (not the initializer type).
                    let value_type = if let Some(declared_type) = jsdoc_declared_type {
                        // Check initializer assignability against @type (TS2322)
                        if prop.initializer != prop.name {
                            self.check_assignable_or_report_at(
                                value_type,
                                declared_type,
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

                        // Widen literal types for object literal properties (tsc behavior).
                        // Object literal properties are mutable by default, so `{ x: "a" }`
                        // produces `{ x: string }`.  Only preserve literals when:
                        // - A const assertion is active (`as const`)
                        // - A contextual type narrows the property to a literal
                        if !self.ctx.in_const_assertion
                            && !self.ctx.preserve_literal_types
                            && property_context_type.is_none()
                            && !had_object_context
                        {
                            self.widen_literal_type(value_type)
                        } else {
                            value_type
                        }
                    };

                    // Note: TS7008 is NOT emitted for object literal properties.
                    // tsc only emits TS7008 for class properties, property signatures,
                    // auto-accessors, and binary expressions.

                    let name_atom = self.ctx.types.intern_string(&name);

                    // Check for duplicate property (skip in destructuring targets).
                    // TS1117: duplicate properties are an error in object literals.
                    // Skip for computed property names — tsc only checks static names.
                    // Computed names like [Symbol.xxx] or [variable] may legitimately
                    // appear multiple times (e.g., value + getter/setter for same symbol).
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
                                            prop.name,
                                            &message,
                                            diagnostic_codes::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                                        );
                            }
                            explicit_property_names.insert(atom);
                        }
                    }
                    let index_ctx_type = if let Some(ctx_type) = self.ctx.contextual_type {
                        self.contextual_object_literal_property_type(
                            ctx_type,
                            resolved_computed_name.as_deref().unwrap_or("__@computed"),
                        )
                    } else {
                        None
                    };
                    let prev_context = self.ctx.contextual_type;
                    self.ctx.contextual_type =
                        self.contextual_type_option_for_expression(index_ctx_type);
                    let value_type = self.get_type_of_node(prop.initializer);
                    self.ctx.contextual_type = prev_context;

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
                    let name = ident.escaped_text.clone();
                    let shorthand_name_idx = shorthand.name;

                    // Get contextual type for this property
                    let property_context_type = if let Some(ctx_type) = self.ctx.contextual_type {
                        self.contextual_object_literal_property_type(ctx_type, &name)
                    } else {
                        None
                    };
                    let jsdoc_declared_type = self.jsdoc_type_annotation_for_node_direct(elem_idx);

                    // Set contextual type for shorthand property value
                    let prev_context = self.ctx.contextual_type;
                    let had_object_context = prev_context.is_some();
                    self.ctx.contextual_type = self.contextual_type_option_for_expression(
                        jsdoc_declared_type.or(property_context_type),
                    );

                    let value_type = if self.resolve_identifier_symbol(shorthand_name_idx).is_none()
                    {
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
                    } else {
                        // Use shorthand_name_idx (the identifier) so that get_type_of_identifier
                        // is invoked, which calls check_flow_usage and can emit TS2454
                        // if the variable is used before assignment.
                        // Using elem_idx (SHORTHAND_PROPERTY_ASSIGNMENT) would return TypeId::ERROR
                        // since that node kind has no dispatch handler, silently suppressing TS2454.
                        self.get_type_of_node(shorthand_name_idx)
                    };

                    // Restore context
                    self.ctx.contextual_type = prev_context;

                    let value_type = if let Some(declared_type) = jsdoc_declared_type {
                        self.check_assignable_or_report_at(
                            value_type,
                            declared_type,
                            shorthand_name_idx,
                            elem_idx,
                        );
                        declared_type
                    } else {
                        // Apply bidirectional type inference - use contextual type to narrow the value type
                        let value_type = tsz_solver::apply_contextual_type(
                            self.ctx.types,
                            value_type,
                            property_context_type,
                        );

                        // Widen literal types for shorthand properties (same as named properties)
                        if !self.ctx.in_const_assertion
                            && !self.ctx.preserve_literal_types
                            && property_context_type.is_none()
                            && !had_object_context
                        {
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
                    let prev_context = self.ctx.contextual_type;
                    let jsdoc_declared_type = self.jsdoc_type_annotation_for_node_direct(elem_idx);
                    if let Some(ctx_type) = prev_context {
                        let method_context_type =
                            self.contextual_object_literal_property_type(ctx_type, &name);
                        self.ctx.contextual_type =
                            self.contextual_type_option_for_expression(
                                jsdoc_declared_type.or(method_context_type),
                            );
                    } else if jsdoc_declared_type.is_some() {
                        self.ctx.contextual_type =
                            self.contextual_type_option_for_expression(jsdoc_declared_type);
                    }

                    // If no explicit ThisType marker exists, use the object literal's
                    // contextual type as `this` inside method bodies.
                    let mut pushed_contextual_this = false;
                    let mut pushed_synthetic_this = false;
                    if marker_this_type.is_none() && self.current_this_type().is_none() {
                        if let Some(ctx_type) = prev_context {
                            let ctx_type = self.evaluate_contextual_type(ctx_type);
                            self.ctx.this_type_stack.push(ctx_type);
                            pushed_contextual_this = true;
                        } else {
                            // For non-contextual object literals, model `this` as the
                            // object-under-construction so assignments like `this.a = ...`
                            // in method `a()` validate against the method property type.
                            // Include placeholders for ALL methods (not just the current
                            // one) so mutually-recursive methods can resolve `this.other()`.
                            let mut this_props: Vec<PropertyInfo> =
                                properties.values().cloned().collect();
                            for &method_name_atom in &obj_all_method_names {
                                if !this_props.iter().any(|p| p.name == method_name_atom) {
                                    let placeholder_method_type =
                                        self.ctx.types.factory().callable(CallableShape {
                                            call_signatures: vec![CallSignature {
                                                type_params: Vec::new(),
                                                params: vec![tsz_solver::ParamInfo {
                                                    name: None,
                                                    type_id: TypeId::ANY,
                                                    optional: false,
                                                    rest: true,
                                                }],
                                                this_type: None,
                                                return_type: TypeId::ANY,
                                                type_predicate: None,
                                                is_method: true,
                                            }],
                                            construct_signatures: Vec::new(),
                                            properties: Vec::new(),
                                            string_index: None,
                                            number_index: None,
                                            symbol: None,
                                            is_abstract: false,
                                        });
                                    this_props.push(PropertyInfo {
                                        name: method_name_atom,
                                        type_id: placeholder_method_type,
                                        write_type: placeholder_method_type,
                                        optional: false,
                                        readonly: false,
                                        is_method: true,
                                        is_class_prototype: false,
                                        visibility: Visibility::Public,
                                        parent_id: None,
                                        declaration_order: 0,
                                    });
                                }
                            }
                            self.ctx
                                .this_type_stack
                                .push(self.ctx.types.factory().object(this_props));
                            pushed_synthetic_this = true;
                        }
                    }

                    let method_type = self.get_type_of_function(elem_idx);

                    if pushed_contextual_this || pushed_synthetic_this {
                        self.ctx.this_type_stack.pop();
                    }

                    // Restore context
                    self.ctx.contextual_type = prev_context;
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
                    let prev_context = self.ctx.contextual_type;
                    if let Some(ctx_type) = prev_context {
                        let computed_context_type = self.contextual_object_literal_property_type(
                            ctx_type,
                            resolved_computed_name.as_deref().unwrap_or("__@computed"),
                        );
                        self.ctx.contextual_type =
                            self.contextual_type_option_for_expression(computed_context_type);
                    }
                    let method_type = self.get_type_of_function(elem_idx);
                    self.ctx.contextual_type = prev_context;

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
                            self.infer_getter_return_type(accessor.body)
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
                    // TS2778: The target of an object rest assignment may not be
                    // an optional property access. E.g. `{ ...obj?.a } = source`
                    if self.ctx.in_destructuring_target
                        && self.is_optional_chain_access(spread_expr)
                    {
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                        self.error_at_node(
                            spread_expr,
                            diagnostic_messages::THE_TARGET_OF_AN_OBJECT_REST_ASSIGNMENT_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS,
                            diagnostic_codes::THE_TARGET_OF_AN_OBJECT_REST_ASSIGNMENT_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS,
                        );
                    }
                    let spread_type = self.get_type_of_node(spread_expr);
                    // TS2698: Spread types may only be created from object types
                    let resolved_spread = self.resolve_type_for_property_access(spread_type);
                    let resolved_spread = self.resolve_lazy_type(resolved_spread);
                    let is_valid_spread =
                        crate::query_boundaries::type_computation::access::is_valid_spread_type(
                            self.ctx.types,
                            resolved_spread,
                        );
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
                        // Union spread distribution: fork current property set
                        // into N branches, one per union member.
                        has_union_spread = true;
                        let mut new_branches: Vec<FxHashMap<Atom, PropertyInfo>> = Vec::new();

                        for member in &members {
                            let member_props = self.collect_object_spread_properties(*member);
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
                        let spread_props = self.collect_object_spread_properties(spread_type);

                        // TS2783: Check if any earlier named properties will be
                        // overwritten by required properties from this spread.
                        // Only when strict null checks are enabled.
                        if self.ctx.strict_null_checks() {
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

        // Union spread distribution: if we encountered union spreads, produce a
        // union of object types (one per combination of union members).
        let object_type = if has_union_spread && !union_spread_branches.is_empty() {
            // Apply any non-spread properties that were added after the union
            // spread(s) to each branch. Properties in `properties` override
            // branch properties (later properties win in object literals).
            let mut post_spread_props: Vec<PropertyInfo> = properties.into_values().collect();
            post_spread_props.sort_by_key(|p| p.declaration_order);

            let mut union_members: Vec<TypeId> = Vec::new();
            for mut branch in union_spread_branches {
                for prop in &post_spread_props {
                    branch.insert(prop.name, prop.clone());
                }
                let mut branch_props: Vec<PropertyInfo> = branch.into_values().collect();
                branch_props.sort_by_key(|p| p.declaration_order);
                let obj = self.ctx.types.factory().object(branch_props);
                union_members.push(obj);
            }
            self.ctx.types.factory().union(union_members)
        } else {
            let mut properties: Vec<PropertyInfo> = properties.into_values().collect();
            properties.sort_by_key(|p| p.declaration_order);
            // Object literals with spreads are not fresh (no excess property checking)

            if string_index_types.is_empty() && number_index_types.is_empty() {
                if has_spread {
                    self.ctx.types.factory().object(properties)
                } else {
                    self.ctx.types.factory().object_fresh(properties)
                }
            } else {
                use tsz_solver::{IndexSignature, ObjectFlags, ObjectShape};
                if !string_index_types.is_empty() {
                    // A computed string-key member makes the object open to arbitrary
                    // string property access. Match tsc by widening the string index
                    // over all string-named members, not just the computed one.
                    string_index_types.extend(properties.iter().map(|prop| prop.type_id));
                }

                let string_index = if !string_index_types.is_empty() {
                    Some(IndexSignature {
                        key_type: TypeId::STRING,
                        value_type: self.ctx.types.factory().union(string_index_types),
                        readonly: false,
                        param_name: None,
                    })
                } else {
                    None
                };

                let number_index = if !number_index_types.is_empty() {
                    Some(IndexSignature {
                        key_type: TypeId::NUMBER,
                        value_type: self.ctx.types.factory().union(number_index_types),
                        readonly: false,
                        param_name: None,
                    })
                } else {
                    None
                };

                let flags = if has_spread {
                    ObjectFlags::empty()
                } else {
                    ObjectFlags::FRESH_LITERAL
                };

                self.ctx.types.factory().object_with_index(ObjectShape {
                    flags,
                    properties,
                    string_index,
                    number_index,
                    symbol: None,
                })
            }
        };

        // NOTE: Freshness is now tracked on the TypeId via ObjectFlags.
        // This fixes the "Zombie Freshness" bug by distinguishing fresh vs
        // non-fresh object types at interning time.

        // Pop this type from stack if we pushed it earlier
        if marker_this_type.is_some() {
            self.ctx.this_type_stack.pop();
        }

        object_type
    }

    /// Collect properties from a spread expression in an object literal.
    ///
    /// Given the type of the spread expression, extracts all properties that would
    /// be spread into the object literal.
    pub(crate) fn collect_object_spread_properties(
        &mut self,
        type_id: TypeId,
    ) -> Vec<tsz_solver::PropertyInfo> {
        let resolved = self.resolve_type_for_property_access(type_id);
        let resolved = self.resolve_lazy_type(resolved);
        self.ctx.types.collect_object_spread_properties(resolved)
    }
}
