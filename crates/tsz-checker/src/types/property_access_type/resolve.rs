//! Core property access resolution logic.
//!
//! Contains the main `get_type_of_property_access_inner` function that handles
//! all property access type resolution including optional chaining, enum/namespace
//! fast paths, class member access, and diagnostic emission.
use crate::context::TypingRequest;
use crate::query_boundaries::common::TypeResolver;
use crate::query_boundaries::property_access as access_query;
use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Inner implementation of property access type resolution.
    pub(crate) fn get_type_of_property_access_inner(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        use crate::query_boundaries::common::PropertyAccessResult;
        let skip_flow_narrowing = request.flow.skip_flow_narrowing();
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };
        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return TypeId::ERROR; // Missing access expression data - propagate error
        };
        // Handle import.meta module-compat diagnostic (TS1343 for <ES2020, TS1470 for Node CJS output)
        if let Some(result) =
            self.try_resolve_import_meta_access(idx, access.expression, access.name_or_argument)
        {
            return result;
        }

        // Get the property name first (needed for abstract property check regardless of object type)
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            // Preserve diagnostics on the base expression (e.g. TS2304 for `missing.`)
            // even when parser recovery could not build a property name node.
            let _ = self.get_type_of_node(access.expression);
            return TypeId::ERROR;
        };
        // Parser recovery placeholders for missing member names — emitted by
        // helpers like `create_missing_expression`. Use the canonical
        // `is_missing_recovery_identifier` helper rather than the weaker
        // `escaped_text.is_empty()` shorthand so we only short-circuit on
        // true placeholders, not on a hypothetical real empty-named ident.
        if self
            .ctx
            .arena
            .is_missing_recovery_identifier(access.name_or_argument)
        {
            // Preserve diagnostics on the base expression when member name is missing.
            let _ = self.get_type_of_node(access.expression);
            return TypeId::ERROR;
        }

        // The ANY short-circuit exists so an invalid write target can't cascade
        // into assignability diagnostics (TS2322 etc.) — it does not apply to a
        // genuine read of this node. Compound assignment and increment/decrement
        // read the target's value before writing it (`get_type_of_node`, default
        // Read flow), and that read must see the target's real type — including
        // any `undefined` introduced by an optional chain's own short-circuit
        // marker — so the existing nullish-operand checks can fire on it, the
        // same way tsc reports `TS18048` on a read-before-write target.
        if skip_flow_narrowing && self.optional_chain_invalid_assignment_target_context(idx) {
            let read_request = request.read().normal_origin().contextual_opt(None);
            let receiver_type =
                self.get_type_of_node_with_request(access.expression, &read_request);
            self.report_write_target_chain_nullish_receiver(
                access.expression,
                access.question_dot_token,
                receiver_type,
            );
            return TypeId::ANY;
        }

        let optional_property_chain_cache_key =
            self.optional_property_chain_cache_key(idx, request);
        let optional_property_chain_cache_generation = TypeResolver::resolver_generation(&self.ctx);
        if let Some(key) = optional_property_chain_cache_key.as_ref() {
            let cached = self
                .ctx
                .flow_shared
                .narrowing_cache
                .optional_property_chain_cache
                .borrow()
                .get(key, optional_property_chain_cache_generation);
            if let Some(cached) = cached {
                // The cache is keyed by root type and path, not node: replay
                // the marker bit for this node so chain continuations still
                // know whether the result's `undefined` is chain-introduced.
                self.set_optional_chain_marker_only(idx, cached.undefined_is_marker_only);
                return cached.type_id;
            }
        }

        if let Some(type_id) = self.partial_object_literal_initializer_property_type(
            access.expression,
            access.name_or_argument,
        ) {
            return type_id;
        }

        if let Some(literal_type) = self.const_array_to_enum_member_literal_type_query(idx) {
            return literal_type;
        }
        if let Some(literal_type) = self
            .imported_array_to_enum_member_literal_type(access.expression, access.name_or_argument)
        {
            return literal_type;
        }

        if self.is_js_file()
            && self.ctx.compiler_options.check_js
            && self.property_access_is_direct_write_target(idx)
        {
            let write_base_type = self.get_type_of_write_target_base_expression(access.expression);
            if self.is_expando_function_assignment(idx, access.expression, write_base_type) {
                return TypeId::ANY;
            }
        }

        if let Some(missing_global) =
            self.missing_typescript_lib_dom_global_alias(access.expression)
        {
            self.error_at_node_msg(
                access.expression,
                crate::diagnostics::diagnostic_codes::CANNOT_FIND_NAME,
                &[&missing_global],
            );
            return TypeId::ERROR;
        }

        // Property access is a value context. If the base identifier resolves to a
        // type-only import/export chain, stop before member lookup so we don't emit
        // a follow-on TS2339 after the primary TS1361/TS1362 wrong-meaning error.
        if let Some(base_node) = self.ctx.arena.get(access.expression)
            && base_node.kind == SyntaxKind::Identifier as u16
            && let Some(base_ident) = self.ctx.arena.get_identifier(base_node)
            && let Some(base_sym_id) =
                self.resolve_identifier_symbol(access.expression)
                    .or_else(|| {
                        self.ctx
                            .binder
                            .resolve_identifier(self.ctx.arena, access.expression)
                    })
            && self.alias_resolves_to_type_only(base_sym_id)
            && !self.source_file_has_value_import_binding_named(
                access.expression,
                &base_ident.escaped_text,
            )
            && self
                .local_current_file_value_symbol_named(&base_ident.escaped_text)
                .is_none()
        {
            if self.is_heritage_type_only_context(access.expression)
                || self.is_in_ambient_computed_property_context()
                || self.is_in_type_query_context(access.expression)
            {
                return TypeId::ERROR;
            }
            self.report_wrong_meaning_diagnostic(
                &base_ident.escaped_text,
                access.expression,
                crate::query_boundaries::name_resolution::NameLookupKind::Type,
            );
            return TypeId::ERROR;
        }

        if self.ctx.checking_computed_property_name.is_some()
            && let Some(base_ident) = self.ctx.arena.get_identifier_at(access.expression)
            && base_ident.escaped_text == "Symbol"
            && let Some(prop_ident) = self.ctx.arena.get_identifier(name_node)
        {
            let symbol_value_type = self.type_of_value_symbol_by_name("Symbol");
            if symbol_value_type != TypeId::UNKNOWN && symbol_value_type != TypeId::ERROR {
                match self
                    .resolve_property_access_with_env(symbol_value_type, &prop_ident.escaped_text)
                {
                    PropertyAccessResult::Success { type_id, .. }
                    | PropertyAccessResult::PossiblyNullOrUndefined {
                        property_type: Some(type_id),
                        ..
                    } => return type_id,
                    _ => {}
                }
            }
        }

        // Check for abstract property access in constructor BEFORE evaluating types (error 2715)
        // This must happen even when `this` has type ANY
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;

            if self.is_this_expression(access.expression)
                && let Some(ref class_info) = self.ctx.enclosing_class.clone()
                && self.ctx.directly_in_class_member_body()
                && (class_info.in_constructor || self.is_in_instance_property_initializer(idx))
                && let Some(declaring_class_name) =
                    self.find_abstract_property_declaring_class(class_info.class_idx, property_name)
            {
                self.error_abstract_property_in_constructor(
                    property_name,
                    &declaring_class_name,
                    access.name_or_argument,
                );
            }
        }

        // Once the base expression is known to be a type-only import/export chain,
        // property access is not a valid value operation. Preserve the TS1361/TS1362
        // diagnostic on the base identifier and stop before member lookup adds a
        // spurious downstream TS2339.
        if let Some(local_sym_id) = self.resolve_identifier_symbol(access.expression)
            && self.alias_resolves_to_type_only(local_sym_id)
            && let Some(base_node) = self.ctx.arena.get(access.expression)
            && let Some(base_ident) = self.ctx.arena.get_identifier(base_node)
            && !self.source_file_has_value_import_binding_named(
                access.expression,
                &base_ident.escaped_text,
            )
            && self
                .local_current_file_value_symbol_named(&base_ident.escaped_text)
                .is_none()
        {
            self.report_wrong_meaning_diagnostic(
                &base_ident.escaped_text,
                access.expression,
                crate::query_boundaries::name_resolution::NameLookupKind::Type,
            );
            return TypeId::ERROR;
        }

        // Fast path for enum/namespace member value access (`E.Member` or `Ns.Member`).
        if let Some(result) = self.try_resolve_enum_namespace_member_access(
            idx,
            access.expression,
            access.name_or_argument,
            name_node,
            skip_flow_narrowing,
        ) {
            return result;
        }

        if let Some(base_ident) = self.ctx.arena.get_identifier_at(access.expression)
            && let Some(prop_ident) = self.ctx.arena.get_identifier(name_node)
            && self
                .ctx
                .import_conflict_names
                .contains(base_ident.escaped_text.as_str())
            && let Some(namespace_sym_id) = self
                .ctx
                .binder
                .get_symbols()
                .find_all_by_name(&base_ident.escaped_text)
                .iter()
                .copied()
                .find(|&candidate_id| {
                    self.ctx
                        .binder
                        .get_symbol(candidate_id)
                        .is_some_and(|candidate| {
                            candidate.has_any_flags(symbol_flags::MODULE)
                                && candidate.declarations.iter().copied().any(|decl_idx| {
                                    self.ctx.arena.get(decl_idx).is_some_and(|node| {
                                        node.kind == syntax_kind_ext::MODULE_DECLARATION
                                    })
                                })
                        })
                })
        {
            let namespace_type = self.get_type_of_symbol(namespace_sym_id);
            match self.resolve_property_access_with_env(namespace_type, &prop_ident.escaped_text) {
                PropertyAccessResult::Success { type_id, .. }
                | PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type: Some(type_id),
                    ..
                } => {
                    return self.finalize_property_access_result(
                        idx,
                        type_id,
                        skip_flow_narrowing,
                        false,
                    );
                }
                _ => {}
            }
        }

        // Get the type of the object.
        // When checking assignment targets (skip_flow_narrowing=true), we still need
        // narrowing on the object expression. E.g., for `target.info.a_count = 3` inside
        // `if (target instanceof A2)`, `target` must narrow to A2 so we can resolve `info`.
        // Only the final property access result should skip narrowing.
        //
        // Hot path optimization: in literal equality comparisons (`obj.prop === "x"`),
        // probing the property on the non-flow object type is often enough. If the
        // property is found without flow narrowing, keep that cheaper object type and
        // avoid an additional flow walk on the object expression.
        // `should_skip_property_result_flow_narrowing_for_result` internally calls
        // `should_skip_property_result_flow_narrowing` and returns true whenever that
        // base check succeeds. So skip_result_flow_for_result is always a superset of
        // the base check, eliminating the need for a separate call.
        let skip_result_flow_for_result =
            !skip_flow_narrowing && self.should_skip_property_result_flow_narrowing_for_result(idx);
        let skip_result_flow = skip_result_flow_for_result;
        let skip_optional_base_flow = access.question_dot_token && skip_result_flow_for_result;

        let (original_object_type, write_presence_only) = if skip_flow_narrowing {
            let object_type_no_flow =
                self.get_type_of_write_target_base_expression(access.expression);

            let preserve_non_js_write_base = self.is_js_file()
                && self.ctx.compiler_options.check_js
                && self
                    .ctx
                    .arena
                    .get(access.expression)
                    .is_some_and(|expr_node| expr_node.kind == SyntaxKind::Identifier as u16)
                && self
                    .ctx
                    .arena
                    .get_identifier_at(access.expression)
                    .is_some_and(|ident| {
                        self.cross_file_global_value_type_by_name(&ident.escaped_text, false)
                            .is_some_and(|preferred_type| {
                                preferred_type != TypeId::ANY
                                    && preferred_type != TypeId::UNKNOWN
                                    && preferred_type != TypeId::ERROR
                                    && !crate::query_boundaries::common::is_function_type(
                                        self.ctx.types,
                                        preferred_type,
                                    )
                            })
                    });

            let property_name_for_probe = self.ctx.arena.get_identifier(name_node);
            self.write_receiver_type_for_property_access(
                idx,
                access.expression,
                property_name_for_probe.map(|ident| ident.escaped_text.as_str()),
                object_type_no_flow,
                preserve_non_js_write_base,
            )
        } else if skip_optional_base_flow {
            (
                self.get_type_of_write_target_base_expression(access.expression),
                false,
            )
        } else if skip_result_flow {
            let object_type_no_flow =
                self.get_type_of_write_target_base_expression(access.expression);

            let property_name_for_probe = self
                .ctx
                .arena
                .get_identifier(name_node)
                .map(|ident| ident.escaped_text.clone());
            let can_use_no_flow = if let Some(property_name) = property_name_for_probe.as_deref() {
                let evaluated_no_flow = self
                    .apparent_type_of_receiver_light(object_type_no_flow)
                    .into_type();
                let resolved_no_flow = self.resolve_type_for_property_access(evaluated_no_flow);
                !matches!(
                    self.resolve_property_access_with_env(resolved_no_flow, property_name),
                    PropertyAccessResult::PropertyNotFound { .. }
                        | PropertyAccessResult::IsUnknown
                        | PropertyAccessResult::PossiblyNullOrUndefined { .. }
                )
            } else {
                false
            };

            if can_use_no_flow {
                (object_type_no_flow, false)
            } else {
                (
                    self.get_type_of_node_with_request(access.expression, &TypingRequest::NONE),
                    false,
                )
            }
        } else {
            (
                self.get_type_of_node_with_request(access.expression, &TypingRequest::NONE),
                false,
            )
        };
        // Evaluate Application types to resolve generic type aliases/interfaces,
        // preserving the original for error messages (nominal identity, e.g.
        // `D<string>`). A non-lib generic *interface* application receiver —
        // including one reached through a barrel re-export — is materialized
        // through the env evaluator so its type arguments substitute into the
        // members before lookup; see `receiver_needs_env_materialization`.
        let receiver_needs_env_fallback =
            self.receiver_needs_env_materialization(original_object_type, access.name_or_argument);
        let mut object_type = if access.question_dot_token && skip_optional_base_flow {
            original_object_type
        } else if receiver_needs_env_fallback {
            self.apparent_type_of_receiver_env(original_object_type)
                .into_type()
        } else if let Some(recovered) =
            self.recover_arena_collided_application_for_property_access(original_object_type)
        {
            // Recover a cross-arena lib interface receiver (e.g. `g: Generator<Y,
            // R>`) whose type-parameter push collided with the current file arena;
            // see `recover_arena_collided_application_for_property_access`. Returns
            // `None` (and falls through) for every well-formed receiver.
            recovered
        } else if let Some(materialized) =
            self.materialize_alias_wrapped_interface_receiver(original_object_type)
        {
            // Recover a `type L<T> = Box<T>`-style receiver that forwards concrete
            // arguments into a cross-file generic interface reached through a
            // barrel re-export; the shared evaluator drops the interface's
            // parameter substitution. Returns `None` for every other shape.
            materialized
        } else {
            self.apparent_type_of_receiver_light(original_object_type)
                .into_type()
        };
        let receiver_has_jsdoc_type_annotation = if self.ctx.is_js_file()
            && self.ctx.should_resolve_jsdoc()
            && let Some(sym_id) = self.resolve_identifier_symbol_without_tracking(access.expression)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.value_declaration.is_some()
            && (symbol.flags
                & (symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::BLOCK_SCOPED_VARIABLE))
                != 0
        {
            self.jsdoc_type_annotation_for_node(symbol.value_declaration)
                .is_some()
        } else {
            false
        };

        // Override object_type with the global value type for an unshadowed known
        // global (e.g. `let location = shape.location; location.x` keeps the local
        // type, but bare `location.x` uses the DOM `Location` global). Preserves an
        // already-eligible bare `Lazy` receiver so the lazy single-member fast path
        // stays engaged for `document.title` etc. See `global_value_type_override`.
        if let Some(ident) = self.ctx.arena.get_identifier_at(access.expression)
            && let Some(value_type) =
                self.global_value_type_override(&ident.escaped_text, object_type, access.expression)
        {
            object_type = value_type;
        }

        if self.ctx.is_js_file()
            && self.ctx.should_resolve_jsdoc()
            && let Some(ident) = self.ctx.arena.get_identifier_at(access.expression)
            && let Some(sym_id) = self.resolve_identifier_symbol_without_tracking(access.expression)
            && !receiver_has_jsdoc_type_annotation
            && !self.is_require_call_bound_identifier(access.expression)
            && let Some(preferred_type) =
                self.preferred_non_js_cross_file_global_value_type(&ident.escaped_text, sym_id)
        {
            object_type = preferred_type;
        }

        // When the object type is `unknown` but the expression is an identifier or
        // property access whose type was not fully resolved (lazy type alias evaluation),
        // re-resolve to trigger deferred Application type expansion. This handles
        // cases where variables declared with generic type alias annotations (e.g.,
        // `type P = Proxy<string>; const ps: P`) or mapped types with Application
        // templates (e.g., `Proxify<Shape>`) have not been fully evaluated when
        // the first property access occurs.
        if object_type == TypeId::UNKNOWN
            && let Some(expr_node) = self.ctx.arena.get(access.expression)
        {
            if expr_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
                if let Some(sym_id) = self.resolve_identifier_symbol(access.expression) {
                    let sym_type = self.get_type_of_symbol(sym_id);
                    if sym_type != TypeId::UNKNOWN && sym_type != TypeId::ERROR {
                        object_type = self.apparent_type_of_receiver_light(sym_type).into_type();
                    }
                }
            } else if self.ctx.arena.get_access_expr(expr_node).is_some() {
                let inner_type = self.get_type_of_property_access_with_request(
                    access.expression,
                    &TypingRequest::NONE,
                );
                if inner_type != TypeId::UNKNOWN && inner_type != TypeId::ERROR {
                    object_type = self.apparent_type_of_receiver_light(inner_type).into_type();
                }
            }
        }

        // Handle optional chain continuations: for `o?.b.c`, when processing `.c`,
        // the object type from `o?.b` includes `undefined` from the optional chain.
        // Remove only that chain-introduced marker (tsc's
        // `removeOptionalTypeMarker`): when `b` itself is optional, its own
        // `undefined` survives and the normal possibly-nullish path reports
        // TS18048 exactly like tsc. Only do this when this access is NOT itself
        // an optional chain (`question_dot_token` is false) but is part of one
        // (parent has `?.`).
        object_type = if !access.question_dot_token
            && crate::types_domain::computation::access::is_optional_chain(
                self.ctx.arena,
                access.expression,
            ) {
            let non_optional = self.remove_optional_chain_marker(access.expression, object_type);
            // Honor guards (`if (o?.f) o?.f.g`) before diagnosing: tsc's
            // receiver is the flow-narrowed reference.
            self.flow_narrow_optional_chain_remainder(access.expression, non_optional)
        } else {
            object_type
        };
        if let Some(receiver_ident) = self.ctx.arena.get_identifier_at(access.expression)
            && let Some(prop_ident) = self.ctx.arena.get_identifier(name_node)
            && matches!(
                self.resolve_property_access_with_env(object_type, &prop_ident.escaped_text),
                PropertyAccessResult::PropertyNotFound { .. } | PropertyAccessResult::IsUnknown
            )
            && let Some(value_sym_id) =
                self.local_current_file_value_symbol_named(&receiver_ident.escaped_text)
            && let Some(value_symbol) = self.ctx.binder.get_symbol(value_sym_id)
            && value_symbol.value_declaration.is_some()
            && let Some(value_node) = self.ctx.arena.get(value_symbol.value_declaration)
            && let Some(var_decl) = self.ctx.arena.get_variable_declaration(value_node)
            && var_decl.initializer.is_some()
            && let Some(literal_type) = self.literal_type_from_initializer(var_decl.initializer)
            && !matches!(
                self.resolve_property_access_with_env(literal_type, &prop_ident.escaped_text),
                PropertyAccessResult::PropertyNotFound { .. } | PropertyAccessResult::IsUnknown
            )
        {
            object_type = literal_type;
        }
        let receiver_has_daa_error =
            self.receiver_has_definite_assignment_error(idx, access.expression);
        if !skip_flow_narrowing
            // When TS2454 already forced the receiver read back to its declared type,
            // keep property access on that declared type so member lookup and call
            // contextual typing still work. Only the second property-read flow pass
            // must be skipped, otherwise we reapply narrowing and lose tsc-aligned
            // downstream behavior.
            && !receiver_has_daa_error
            && self.ctx.arena.get(access.expression).is_some_and(|expr| {
                matches!(
                    expr.kind,
                    k if k == SyntaxKind::Identifier as u16
                        || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                )
            })
            && let Some(flow_node) = self.flow_node_for_reference_usage(idx)
        {
            // For identifier expressions, get_type_of_node_with_request() already
            // applied flow narrowing to compute original_object_type. When the
            // property-read flow node is identical to the expression's own flow
            // node, re-narrowing the already-narrowed type would produce wrong
            // results (double-narrowing through instanceof conditions). Only
            // apply additional narrowing when the property access has a distinct
            // flow node that may carry extra narrowing information.
            if !self.is_redundant_receiver_narrow(access.expression, flow_node) {
                object_type = self.flow_analyzer_for_property_reads().get_flow_type(
                    access.expression,
                    object_type,
                    flow_node,
                );
            }
        }
        let mut commonjs_namespace_override: Option<TypeId> = None;
        if object_type == TypeId::ANY
            && self.is_js_file()
            && !self.current_source_file_has_esm_syntax()
            && self
                .ctx
                .arena
                .get_identifier_at(access.expression)
                .is_some_and(|ident| ident.escaped_text == "exports")
            && self.is_unshadowed_commonjs_exports_identifier(access.expression)
        {
            let namespace_type = self.current_file_commonjs_namespace_type();
            object_type = namespace_type;
            commonjs_namespace_override = Some(namespace_type);
        }

        if let Some(type_id) = self.try_resolve_optional_property_chain_fast_path(
            idx,
            access.expression,
            access.name_or_argument,
            name_node,
            super::optional_fast_path::OptionalPropertyChainFastPathRequest {
                object_type,
                original_object_type,
                question_dot_token: access.question_dot_token,
                skip_flow_narrowing,
                skip_result_flow_for_result,
                write_presence_only,
                optional_property_chain_cache_key: optional_property_chain_cache_key.as_ref(),
            },
        ) {
            return type_id;
        }

        // Deferred display_object_type computation: now that the optional-chain
        // fast path has been exhausted, compute the proper display type for error
        // messages. This preserves literal types that get_type_of_node widens.
        let mut display_object_type = if let Some(ns_type) = commonjs_namespace_override {
            ns_type
        } else if matches!(
            original_object_type,
            TypeId::STRING | TypeId::NUMBER | TypeId::BOOLEAN | TypeId::BIGINT
        ) {
            self.expression_display_type_preferring_literal(access.expression, original_object_type)
        } else {
            self.enum_member_initializer_display_type(access.expression)
                .unwrap_or(original_object_type)
        };

        // Override display type with the global value type for an unshadowed known
        // global, preserving an already-eligible bare `Lazy` display type (it
        // renders identically by name, e.g. `Document`, without forcing a full
        // structural materialization). See `global_value_type_override`.
        if let Some(ident) = self.ctx.arena.get_identifier_at(access.expression)
            && let Some(value_type) = self.global_value_type_override(
                &ident.escaped_text,
                display_object_type,
                access.expression,
            )
        {
            display_object_type = value_type;
        }

        if self.ctx.is_js_file()
            && self.ctx.should_resolve_jsdoc()
            && let Some(ident) = self.ctx.arena.get_identifier_at(access.expression)
            && let Some(sym_id) = self.resolve_identifier_symbol_without_tracking(access.expression)
            && !receiver_has_jsdoc_type_annotation
            && !self.is_require_call_bound_identifier(access.expression)
            && let Some(preferred_type) =
                self.preferred_non_js_cross_file_global_value_type(&ident.escaped_text, sym_id)
        {
            display_object_type = preferred_type;
        }

        // For IndexAccess types (e.g., Entries[EntryId]), resolve to the base
        // constraint for display purposes. tsc shows the apparent type in error
        // messages (e.g., 'NumClass<number> | StrClass<string>'), not the raw
        // indexed access type (e.g., 'Entries[EntryId]').
        if crate::query_boundaries::common::is_index_access_type(
            self.ctx.types,
            display_object_type,
        ) {
            let resolved = self.resolve_index_access_base_constraint(display_object_type);
            if resolved != display_object_type {
                display_object_type = resolved;
            }
        }

        // When the receiver is a bare `this` whose flow type lost its object
        // shape, fall back to the enclosing class's instance/constructor type
        // (see `apply_this_receiver_class_type_override`).
        (object_type, display_object_type) = self.apply_this_receiver_class_type_override(
            idx,
            access.expression,
            object_type,
            original_object_type,
            display_object_type,
        );

        if name_node.kind == SyntaxKind::PrivateIdentifier as u16 {
            return self.get_type_of_private_property_access(
                idx,
                access,
                access.name_or_argument,
                object_type,
                skip_flow_narrowing,
            );
        }

        let commonjs_named_props_disallowed = self.is_js_file()
            && self.is_current_file_commonjs_export_base(access.expression)
            && {
                let surface = self.resolve_js_export_surface(self.ctx.current_file_idx);
                // TS7: `module.exports = X` mixed with sibling property exports
                // keeps the module type as exactly `X` (the siblings are illegal,
                // TS2309, not expando members). Resolve `module.exports.p` /
                // `exports.p` against `X` so missing members surface TS2339
                // instead of being answered from the sibling assignments.
                surface.suppresses_expando_merge()
                    || surface.direct_export_type.is_some_and(|direct_export_type| {
                        !crate::query_boundaries::js_exports::commonjs_direct_export_supports_named_props(
                            self.ctx.types,
                            direct_export_type,
                        )
                    })
            };

        let is_this_access = self.js_object_expr_is_this_or_alias(access.expression);
        let static_member_name = self
            .ctx
            .arena
            .get_identifier(name_node)
            .map(|ident| ident.escaped_text.to_string())
            .or_else(|| self.current_file_commonjs_static_member_name(access.name_or_argument));

        if self.is_js_file()
            && is_this_access
            && !self.property_access_is_direct_write_target(idx)
            && self.this_property_assignment_receiver_is_class_instance(access.expression)
            && let Some(member_name) = static_member_name.as_deref()
            && let Some(prior_type) = self.prior_js_this_property_assignment_type(idx, member_name)
        {
            return prior_type;
        }

        if self.is_js_file()
            && !self.property_access_is_direct_write_target(idx)
            && !commonjs_named_props_disallowed
            && self.current_file_commonjs_exports_target_is_unshadowed(access.expression)
            && let Some(member_name) = static_member_name.as_deref()
            && let Some(prior_type) = self.current_file_commonjs_export_property_read_type(
                idx,
                access.expression,
                access.name_or_argument,
                member_name,
            )
        {
            return prior_type;
        }

        let mut js_expando_before_assignment = false;
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;
            if self.is_js_file()
                && self.property_access_is_direct_write_target(idx)
                && let Some(prototype_node) = self.ctx.arena.get(access.expression)
                && prototype_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && let Some(prototype_access) = self.ctx.arena.get_access_expr(prototype_node)
                && self
                    .ctx
                    .arena
                    .get_identifier_at(prototype_access.name_or_argument)
                    .is_some_and(|prototype_ident| prototype_ident.escaped_text == "prototype")
                && let Some(read_pos) = self.ctx.arena.pos_at(idx)
                && self
                    .prior_js_prototype_object_literal_declares_property(
                        prototype_access.expression,
                        property_name,
                        read_pos,
                    )
                    .is_some_and(|declares| !declares)
                // Every function's prototype closes from a non-empty object
                // literal, regardless of `isJSConstructor` evidence (JSDoc
                // `@constructor` / `this.x =` assignments) — oracle-verified
                // (tsconfig-sentinel, typescript@7.0.2): a plain
                // `function F() {}` behaves identically to a real JS
                // constructor here. The only gate tsc applies is
                // `noImplicitAny`: the write is `TS2339` when it is on, and
                // silently accepted (the JS open-container leniency) when it
                // is off. #17226 gap 2.
                && self.ctx.no_implicit_any()
            {
                let type_display = self
                    .prior_js_prototype_object_literal_assignment_display(
                        prototype_access.expression,
                        read_pos,
                    )
                    .unwrap_or_else(|| self.format_type(display_object_type));
                self.error_property_not_exist_with_apparent_type(property_name, &type_display, idx);
            }
            if !commonjs_named_props_disallowed {
                js_expando_before_assignment = self.expando_property_read_before_assignment(
                    idx,
                    access.expression,
                    property_name,
                );
            }
            if js_expando_before_assignment {
                // Suppress TS2565 when a leading JSDoc `@type` annotation
                // declares a function constructor prototype property's type:
                //   function C() { this.x = false; }
                //   /** @type {number} */
                //   C.prototype.x;
                // ES class receivers still report TS2565.
                let suppress_for_jsdoc_type_decl = self.is_js_file()
                    && self.ctx.compiler_options.check_js
                    && self.expando_receiver_is_function_constructor(access.expression)
                    && self
                        .enclosing_expression_statement(idx)
                        .and_then(|stmt_idx| self.js_statement_declared_type(stmt_idx))
                        .is_some();
                // A function/class declaration's expando properties are
                // ordered; a plain object's are not. `exports`/
                // `module.exports` is the same unless this property's own
                // assignment is non-aliasable (see `commonjs_export_property_is_ordered`).
                let receiver_is_ordered = self
                    .expando_root_has_ordered_declarations(access.expression)
                    || self.commonjs_export_property_is_ordered(access.expression, property_name);
                if !suppress_for_jsdoc_type_decl && receiver_is_ordered {
                    self.report_property_used_before_assigned(
                        access.name_or_argument,
                        property_name,
                    );
                }
            }
            if let Some(result) = self.try_resolve_global_this_property_access(
                idx,
                access.expression,
                access.name_or_argument,
                property_name,
                skip_flow_narrowing,
            ) {
                return result;
            }
        }

        if self.is_js_file()
            && self
                .ctx
                .arena
                .get_identifier_at(access.expression)
                .is_some_and(|ident| ident.escaped_text == "module")
            && self.current_file_commonjs_module_identifier_is_unshadowed(access.expression)
            && self
                .ctx
                .arena
                .get_identifier(name_node)
                .is_some_and(|ident| ident.escaped_text == "exports")
        {
            return self.current_file_commonjs_namespace_type();
        }

        if skip_flow_narrowing
            && self.is_js_file()
            && self.property_access_is_direct_write_target(idx)
            && self.current_file_commonjs_exports_target_is_unshadowed(access.expression)
        {
            let surface = self.resolve_js_export_surface(self.ctx.current_file_idx);
            // TS7: under merge suppression the sibling `module.exports.p = ...`
            // writes are not exports; resolve the write target against the
            // direct-export type so a missing member surfaces TS2339.
            let can_add_named_props = !surface.suppresses_expando_merge()
                && surface.direct_export_type.is_none_or(|direct_export_type| {
                    crate::query_boundaries::js_exports::commonjs_direct_export_supports_named_props(
                        self.ctx.types,
                        direct_export_type,
                    )
                });
            if can_add_named_props {
                if self
                    .current_file_commonjs_direct_write_rhs(idx)
                    .is_some_and(|rhs| self.current_file_commonjs_write_rhs_is_undefined_like(rhs))
                    && let Some(export_name) = static_member_name.as_deref()
                    && let Some(node) = self.ctx.arena.get(idx)
                    && let Some(export_type) = self
                        .current_file_commonjs_late_bound_named_export_type(export_name, node.pos)
                {
                    return export_type;
                }
                if let Some(export_name) = static_member_name.as_deref()
                    && let Some(export_type) =
                        surface.lookup_named_export(export_name, self.ctx.types)
                {
                    return export_type;
                }
                return TypeId::ANY;
            }
        }

        // A direct CommonJS export member (`module.exports.b` in
        // `module.exports.b.cat = …`) never hosts FURTHER nested expando
        // growth, regardless of its own RHS shape (object-like or callable)
        // — oracle-verified (`typescript@7.0.2`): `module.exports.b =
        // function b() {}; module.exports.b.cat = "cat";` reports `TS2339`
        // on `.cat`. `typescript@6.0.2` (this block's prior target) granted
        // it unconditionally as `any` for any object-like/callable base
        // export member; the repo's oracle moved to 7.0.2, which tightened
        // it (`scripts/conformance/typescript-versions.json`). Removed
        // rather than narrowed — no base-type shape makes this write legal
        // under the current oracle.

        if self.report_namespace_value_access_for_type_only_import_equals_expr(access.expression) {
            return TypeId::ERROR;
        }

        if self.report_declared_intersection_access_on_invalid_receiver(
            object_type,
            access.expression,
            access.name_or_argument,
            access.name_or_argument,
        ) {
            return TypeId::ERROR;
        }

        // Don't report errors for any/error types - check BEFORE accessibility
        // to prevent cascading errors when the object type is already invalid
        if object_type == TypeId::ANY {
            return TypeId::ANY;
        }
        if object_type == TypeId::ERROR {
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }

        // `getReducedApparentType`: an object intersection with a disjoint
        // discriminant property (e.g. `{ kind: "a" } & { kind: "b" }`) reduces
        // to `never`. The interner applies this reduction only while every
        // member is already concrete; a member reached through a generic
        // application (`WithKind<"a"> & WithKind<"b">`) or an alias stays
        // deferred at intern time, so the conflict is invisible until the
        // members are evaluated. The lighter `evaluate_application_type`
        // receiver path never re-materializes those members, so detect the
        // reduction here by resolving the receiver for property access (which
        // evaluates each member and re-interns, collapsing to `never` on a
        // disjoint discriminant) and fold the receiver to `never` so the
        // shared never-access machinery below reports TS2339 — matching tsc.
        if crate::query_boundaries::common::is_intersection_type(self.ctx.types, object_type)
            && !access_query::contains_never_type(self.ctx.types, object_type)
            && self.resolve_type_for_property_access(object_type) == TypeId::NEVER
        {
            object_type = TypeId::NEVER;
        }

        // Property access on `never` emits TS2339 and returns an any-like
        // fallback. This preserves tsc's follow-on TS2322 when the failed
        // access is assigned to `never`.
        // In TypeScript, `never` has no properties — accessing any property is an error.
        // Also handle intersections that contain `never`.
        if object_type == TypeId::NEVER
            || access_query::contains_never_type(self.ctx.types, object_type)
        {
            let Some(receiver) = self.declared_intersection_receiver_for_never_access(
                access.expression,
                access.name_or_argument,
                access.name_or_argument,
            ) else {
                return TypeId::ANY;
            };
            object_type = receiver;
        }

        // Enforce private/protected access modifiers when possible.
        // Note: we do NOT return ERROR on failure — the diagnostic is already emitted,
        // and tsc continues resolving the property type so that subsequent expressions
        // on the same line are still checked (e.g., `new A().priv + new A().prot`).
        // When accessibility fails, we suppress subsequent TS2339/TS2551 "not found"
        // errors, since the property *does* exist — it's just not accessible.
        let mut accessibility_error_emitted = false;
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;
            let accessible = self.check_property_accessibility(
                access.expression,
                property_name,
                access.name_or_argument,
                object_type,
            );
            if !accessible {
                accessibility_error_emitted = true;
            }
        }

        // Check for merged class/enum/function + namespace symbols
        // When a class/enum/function merges with a namespace (same name), the symbol has both
        // value constructor flags and MODULE flags. We need to check the symbol's exports.
        // This handles value access like `Foo.value` when Foo is both a class and namespace.
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;

            // For value access to merged symbols, check the exports directly
            // This is needed because the type system doesn't track which symbol a Callable came from
            let base_expr = self.ctx.arena.skip_parenthesized(access.expression);
            if let Some(expr_node) = self.ctx.arena.get(base_expr)
                && let Some(expr_ident) = self.ctx.arena.get_identifier(expr_node)
            {
                let expr_name = &expr_ident.escaped_text;
                // Try file_locals first (fast path for top-level symbols)
                if let Some(sym_id) = self.ctx.binder.file_locals.get(expr_name)
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                {
                    // Check if this is a merged symbol (has both MODULE and value constructor flags)
                    let is_merged = symbol.has_any_flags(symbol_flags::MODULE)
                        && symbol.has_any_flags(
                            symbol_flags::CLASS
                                | symbol_flags::FUNCTION
                                | symbol_flags::REGULAR_ENUM,
                        );

                    if is_merged
                        && let Some(exports) = symbol.exports.as_ref()
                        && let Some(member_id) = exports.get(property_name)
                    {
                        // For merged symbols, we return the type for any exported member
                        let member_type = self.get_type_of_symbol(member_id);
                        return self.finalize_property_access_result(
                            idx,
                            member_type,
                            skip_flow_narrowing,
                            false,
                        );
                    }
                }
            }
        }

        self.resolve_identifier_property_access(
            idx,
            access,
            name_node,
            super::identifier_resolution::IdentifierPropertyAccessRequest {
                object_type,
                original_object_type,
                display_object_type,
                skip_flow_narrowing,
                skip_result_flow_for_result,
                write_presence_only,
                receiver_has_daa_error,
                accessibility_error_emitted,
                commonjs_named_props_disallowed,
                is_this_access,
                js_expando_before_assignment,
            },
        )
    }
}
