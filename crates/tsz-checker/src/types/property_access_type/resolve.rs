//! Core property access resolution logic.
//!
//! Contains the main `get_type_of_property_access_inner` function that handles
//! all property access type resolution including optional chaining, enum/namespace
//! fast paths, class member access, and diagnostic emission.
use crate::context::TypingRequest;
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
        let optional_property_chain_cache_key =
            self.optional_property_chain_cache_key(idx, request);
        if let Some(key) = optional_property_chain_cache_key.as_ref()
            && let Some(&cached) = self
                .ctx
                .flow_shared
                .narrowing_cache
                .optional_property_chain_cache
                .borrow()
                .get(key)
        {
            return cached;
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
                && self.ctx.function_depth == 0
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
                .contains(&base_ident.escaped_text)
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
                let evaluated_no_flow = self.evaluate_application_type(object_type_no_flow);
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
        // Evaluate Application types to resolve generic type aliases/interfaces.
        // But preserve original for error messages to maintain nominal identity (e.g., D<string>).
        //
        // For `obj?.prop ?? fallback`, defer this work: the optional-chain fast path
        // below will resolve property access through `resolve_type_for_property_access`,
        // and eagerly evaluating applications here is redundant on hot paths.
        // Keep the wider env evaluation on the imported builder callback case
        // this PR targets; applying it to ordinary alias property reads
        // over-normalizes flow-narrowed unions.
        let is_builder_select_access = self
            .ctx
            .arena
            .get_identifier(name_node)
            .is_some_and(|prop_ident| prop_ident.escaped_text == "select");
        let receiver_fallback_def = crate::query_boundaries::common::get_application_lazy_def_id(
            self.ctx.types,
            original_object_type,
        )
        .or_else(|| {
            crate::query_boundaries::common::lazy_def_id(self.ctx.types, original_object_type)
        });
        let receiver_needs_env_fallback = is_builder_select_access
            && (receiver_fallback_def
                .and_then(|def_id| self.ctx.def_to_symbol_id_with_fallback(def_id))
                .is_some_and(|sym_id| !self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id))
                || crate::query_boundaries::common::is_conditional_type(
                    self.ctx.types,
                    original_object_type,
                )
                || crate::query_boundaries::common::index_access_types(
                    self.ctx.types,
                    original_object_type,
                )
                .is_some());
        let mut object_type = if access.question_dot_token && skip_optional_base_flow {
            original_object_type
        } else if receiver_needs_env_fallback {
            self.evaluate_property_access_receiver_type(original_object_type)
        } else {
            self.evaluate_application_type(original_object_type)
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
                        object_type = self.evaluate_application_type(sym_type);
                    }
                }
            } else if self.ctx.arena.get_access_expr(expr_node).is_some() {
                let inner_type = self.get_type_of_property_access_with_request(
                    access.expression,
                    &TypingRequest::NONE,
                );
                if inner_type != TypeId::UNKNOWN && inner_type != TypeId::ERROR {
                    object_type = self.evaluate_application_type(inner_type);
                }
            }
        }

        // Handle optional chain continuations: for `o?.b.c`, when processing `.c`,
        // the object type from `o?.b` includes `undefined` from the optional chain.
        // But `.c` should only be reached when `o` is defined, so we strip nullish
        // types. Only do this when this access is NOT itself an optional chain
        // (`question_dot_token` is false) but is part of one (parent has `?.`).
        object_type = if !access.question_dot_token
            && crate::types_domain::computation::access::is_optional_chain(
                self.ctx.arena,
                access.expression,
            ) {
            let (non_nullish, _) = self.split_nullish_type(object_type);
            non_nullish.unwrap_or(object_type)
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
        let (receiver_start, receiver_end) = self
            .ctx
            .arena
            .get(access.expression)
            .map(|node| (node.pos, node.end))
            .unwrap_or((u32::MAX, u32::MAX));
        // A receiver "has a DAA error" when:
        //   1. The receiver expression node itself was flagged with TS2454, or
        //   2. The property-access node was flagged, or
        //   3. Any TS2454 diagnostic falls within the receiver's [pos, end) span.
        //
        // Case (3) covers composite receivers like `get(foo)` where the
        // identifier `foo` is a sub-expression of the receiver (not the
        // receiver itself) and was the DAA-flagged node. tsc suppresses
        // TS18047/TS18048/TS18049 (and the legacy TS2531/TS2532/TS2533) on
        // property access whenever the receiver expression contains a
        // definite-assignment failure, because the cascade is meaningless
        // once we already reported that the underlying variable has no value.
        let receiver_has_daa_error = self.ctx.daa_error_nodes.contains(&access.expression.0)
            || self.ctx.daa_error_nodes.contains(&idx.0)
            || self.ctx.diagnostics.iter().any(|diag| {
                diag.code == 2454 && diag.start >= receiver_start && diag.start < receiver_end
            });
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
            self.literal_type_from_initializer(access.expression)
                .unwrap_or(original_object_type)
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

        // When `this` has been deliberately typed as `any` (e.g. TS2683 was
        // emitted because the `this` expression is in a nested regular
        // function without its own `this` binding), don't override back to
        // the enclosing class type — property access on `any` must succeed
        // without a TS2339 cascade.
        let this_has_own_fresh_binding = self
            .ctx
            .arena
            .get(access.expression)
            .is_some_and(|node| node.kind == SyntaxKind::ThisKeyword as u16)
            && self.is_this_in_nested_function_without_own_this_binding(access.expression);
        if self
            .ctx
            .arena
            .get(access.expression)
            .is_some_and(|node| node.kind == SyntaxKind::ThisKeyword as u16)
            && !this_has_own_fresh_binding
            && object_type != TypeId::ANY
            && let Some(class_info) = self.ctx.enclosing_class.as_ref()
            && crate::query_boundaries::common::object_shape_for_type(self.ctx.types, object_type)
                .is_none()
        {
            // In static context, `this` refers to the constructor type (typeof ClassName).
            // In instance context, `this` refers to the instance type (ClassName).
            let is_static_context = self.is_in_static_class_member_context(idx);
            let class_this_type = if is_static_context {
                // Get the constructor type for static context
                let class_idx = class_info.class_idx;
                self.ctx
                    .arena
                    .get(class_idx)
                    .and_then(|node| self.ctx.arena.get_class(node))
                    .map(|class| self.get_class_constructor_type(class_idx, class))
            } else {
                // Use cached instance type for instance context
                class_info.cached_instance_this_type
            };
            if let Some(class_this_type) = class_this_type
                && crate::query_boundaries::common::object_shape_for_type(
                    self.ctx.types,
                    class_this_type,
                )
                .is_some()
            {
                // When `this` has been narrowed by flow analysis (e.g., via a
                // `this is DatafulFoo<T>` type predicate), the narrowed type is
                // an intersection that lacks a direct object shape. Do NOT
                // override it with the class instance type — that would discard
                // the narrowing and cause false TS2532/TS2339 diagnostics on
                // properties that differ between the original class and the
                // predicate target interface.
                let was_narrowed_by_flow =
                    object_type != class_this_type && original_object_type != class_this_type;
                if !was_narrowed_by_flow {
                    object_type = class_this_type;
                    display_object_type = class_this_type;
                }
            }
        }

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
            && self
                .resolve_js_export_surface(self.ctx.current_file_idx)
                .direct_export_type
                .is_some_and(|direct_export_type| {
                    !crate::query_boundaries::js_exports::commonjs_direct_export_supports_named_props(
                        self.ctx.types,
                        direct_export_type,
                    )
                });

        let is_this_access = self.js_object_expr_is_this_or_alias(access.expression);
        let static_member_name = self
            .ctx
            .arena
            .get_identifier(name_node)
            .map(|ident| ident.escaped_text.clone())
            .or_else(|| self.current_file_commonjs_static_member_name(access.name_or_argument));

        if self.is_js_file()
            && is_this_access
            && !self.property_access_is_direct_write_target(idx)
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
            && let Some(node) = self.ctx.arena.get(idx)
            && let Some(prior_type) =
                self.current_file_commonjs_prior_named_export_type(member_name, node.pos)
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
                if !suppress_for_jsdoc_type_decl {
                    use crate::diagnostics::format_message;
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.error_at_node(
                        access.name_or_argument,
                        &format_message(
                            diagnostic_messages::PROPERTY_IS_USED_BEFORE_BEING_ASSIGNED,
                            &[property_name],
                        ),
                        diagnostic_codes::PROPERTY_IS_USED_BEFORE_BEING_ASSIGNED,
                    );
                } else if let Some(declared_type) = self
                    .enclosing_expression_statement(idx)
                    .and_then(|stmt_idx| self.js_statement_declared_type(stmt_idx))
                {
                    self.check_jsdoc_prototype_type_decl_constructor_assignment(
                        access.expression,
                        property_name,
                        declared_type,
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
            return self.current_file_commonjs_module_exports_namespace_type();
        }

        if skip_flow_narrowing
            && self.is_js_file()
            && self.property_access_is_direct_write_target(idx)
            && self.current_file_commonjs_exports_target_is_unshadowed(access.expression)
        {
            let surface = self.resolve_js_export_surface(self.ctx.current_file_idx);
            let can_add_named_props = surface.direct_export_type.is_none_or(|direct_export_type| {
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

        if skip_flow_narrowing
            && self.is_js_file()
            && self.property_access_is_direct_write_target(idx)
            && let Some(base_export_name) =
                self.current_file_commonjs_export_member_name(access.expression)
        {
            let surface = self.resolve_js_export_surface(self.ctx.current_file_idx);
            if let Some(base_type) = surface.lookup_named_export(&base_export_name, self.ctx.types)
                && (crate::query_boundaries::common::is_object_like_type(self.ctx.types, base_type)
                    || crate::query_boundaries::common::callable_shape_for_type(
                        self.ctx.types,
                        base_type,
                    )
                    .is_some())
            {
                return TypeId::ANY;
            }
        }

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
