//! Core property access resolution logic.
//!
//! Contains the main `get_type_of_property_access_inner` function that handles
//! all property access type resolution including optional chaining, enum/namespace
//! fast paths, class member access, and diagnostic emission.

use crate::classes_domain::class_summary::ClassMemberKind;
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

        // Handle import.meta: emit TS1470 in files that compile to CommonJS output
        if let Some(result) =
            self.try_resolve_import_meta_access(idx, access.expression, access.name_or_argument)
        {
            return result;
        }

        let factory = self.ctx.types.factory();

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
                    PropertyAccessResult::PropertyNotFound { .. } | PropertyAccessResult::IsUnknown
                )
            } else {
                false
            };

            if can_use_no_flow || preserve_non_js_write_base {
                let read_object_type =
                    self.get_type_of_node_with_request(access.expression, &TypingRequest::NONE);
                if let Some(property_name) = property_name_for_probe.as_deref() {
                    let evaluated_read = self.evaluate_application_type(read_object_type);
                    let resolved_read = self.resolve_type_for_property_access(evaluated_read);
                    if self.union_write_requires_existing_named_member(resolved_read, property_name)
                    {
                        (read_object_type, false)
                    } else {
                        let read_has_property = !matches!(
                            self.resolve_property_access_with_env(resolved_read, property_name),
                            PropertyAccessResult::PropertyNotFound { .. }
                                | PropertyAccessResult::IsUnknown
                        );
                        (object_type_no_flow, !read_has_property)
                    }
                } else {
                    (object_type_no_flow, false)
                }
            } else {
                (
                    self.get_type_of_node_with_request(access.expression, &TypingRequest::NONE),
                    false,
                )
            }
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

        let effective_write_result = |type_id: TypeId, write_type: Option<TypeId>| -> TypeId {
            if skip_flow_narrowing {
                if write_presence_only {
                    TypeId::ANY
                } else {
                    write_type.unwrap_or(type_id)
                }
            } else {
                type_id
            }
        };

        // Evaluate Application types to resolve generic type aliases/interfaces.
        // But preserve original for error messages to maintain nominal identity (e.g., D<string>).
        //
        // For `obj?.prop ?? fallback`, defer this work: the optional-chain fast path
        // below will resolve property access through `resolve_type_for_property_access`,
        // and eagerly evaluating applications here is redundant on hot paths.
        let mut object_type = if access.question_dot_token && skip_optional_base_flow {
            original_object_type
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

        // Override object_type with the global value type only when the identifier
        // actually resolves to a global, not when a local variable shadows the global.
        // E.g., `let location = shape.location; location.x` should use the local's type,
        // not the DOM `Location` global type.
        if let Some(ident) = self.ctx.arena.get_identifier_at(access.expression)
            && self.is_known_global_value_name(&ident.escaped_text)
        {
            // Check if there's a local binding shadowing the global
            let is_local_shadow = self
                .resolve_identifier_symbol_without_tracking(access.expression)
                .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                .is_some_and(|symbol| {
                    // Local declarations shadow global value names. This includes
                    // variables, classes, and functions — e.g., a file-local
                    // `export declare class Promise<R>` must shadow the global
                    // `Promise` so that its custom static members are visible.
                    (symbol.flags
                        & (symbol_flags::FUNCTION_SCOPED_VARIABLE
                            | symbol_flags::BLOCK_SCOPED_VARIABLE
                            | symbol_flags::PROPERTY
                            | symbol_flags::CLASS
                            | symbol_flags::FUNCTION))
                        != 0
                });

            if !is_local_shadow {
                let value_type = self.type_of_value_symbol_by_name(&ident.escaped_text);
                if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                    object_type = value_type;
                }
            }
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
            && self
                .ctx
                .arena
                .get_identifier_at(access.expression)
                .is_some_and(|ident| ident.escaped_text == "exports")
            && self
                .resolve_identifier_symbol_without_tracking(access.expression)
                .is_none()
        {
            let namespace_type = self.current_file_commonjs_namespace_type();
            object_type = namespace_type;
            commonjs_namespace_override = Some(namespace_type);
        }

        // Fast path for optional chaining on non-class receivers when the
        // property resolves successfully without diagnostics.
        //
        // This avoids the full property-access diagnostic pipeline for common
        // patterns like `opts?.timeout` / `opts?.retries` in hot call sites.
        if access.question_dot_token
            && !self
                .ctx
                .compiler_options
                .no_property_access_from_index_signature
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && !self.is_super_expression(access.expression)
        {
            let property_name = &ident.escaped_text;

            // TOP-LEVEL CACHE: check the dedicated optional_chain_cache first.
            // This is keyed by (object_type_with_nullish, prop_atom) and stores
            // the FINAL result including undefined union. On cache hit, we skip
            // split_nullish, resolve_type, contains_type_params, property lookup,
            // and union2 — eliminating 4+ RefCell borrows and HashMap lookups.
            // Only used when flow narrowing is skipped (skip_result_flow_for_result),
            // which guarantees the result is context-independent.
            if skip_result_flow_for_result {
                let oc_atom = if ident.atom != tsz_common::interner::Atom::none() {
                    ident.atom
                } else {
                    self.ctx.types.intern_string(property_name)
                };
                if let Some(&cached) = self
                    .ctx
                    .narrowing_cache
                    .optional_chain_cache
                    .borrow()
                    .get(&(object_type, oc_atom))
                {
                    return cached;
                }
            }

            let (non_nullish_base, base_nullish) = self.split_nullish_type(object_type);
            let Some(non_nullish_base) = non_nullish_base else {
                return TypeId::UNDEFINED;
            };

            // Keep class/private/protected semantics on the full path.
            if self
                .resolve_class_for_access(access.expression, non_nullish_base)
                .is_none()
            {
                let resolved_base = self.resolve_type_for_property_access(non_nullish_base);
                // PERF: Reuse the pre-interned atom from the identifier when available,
                // avoiding a DashMap lookup in intern_string on every property access.
                let prop_atom = if ident.atom != tsz_common::interner::Atom::none() {
                    ident.atom
                } else {
                    self.ctx.types.intern_string(property_name)
                };

                // property_cache stores Option<TypeId>: Some(id) = resolved type,
                // None = property not found (fall through for TS2339 diagnostics).
                let cached_property_type = self
                    .ctx
                    .narrowing_cache
                    .property_cache
                    .borrow()
                    .get(&(resolved_base, prop_atom))
                    .copied();
                if let Some(Some(type_id)) = cached_property_type {
                    let mut result_type = self.refine_expando_property_read_type(
                        idx,
                        access.expression,
                        property_name,
                        type_id,
                    );
                    if base_nullish.is_some()
                        && !crate::query_boundaries::common::type_contains_undefined(
                            self.ctx.types,
                            result_type,
                        )
                    {
                        result_type = factory.union2(result_type, TypeId::UNDEFINED);
                    }
                    // Store in optional_chain_cache for instant hits next time.
                    if skip_result_flow_for_result {
                        self.ctx
                            .narrowing_cache
                            .optional_chain_cache
                            .borrow_mut()
                            .insert((object_type, prop_atom), result_type);
                    }
                    return self.finalize_property_access_result(
                        idx,
                        result_type,
                        skip_flow_narrowing,
                        skip_result_flow_for_result,
                    );
                }

                let fast_result = self.ctx.types.resolve_property_access_with_options(
                    resolved_base,
                    property_name,
                    self.ctx.compiler_options.no_unchecked_indexed_access,
                );
                let result = self.resolve_property_access_with_env_post_query(
                    resolved_base,
                    property_name,
                    fast_result,
                );
                match result {
                    PropertyAccessResult::Success {
                        type_id,
                        write_type,
                        from_index_signature,
                    } => {
                        let generic_mapped_missing_named_property = from_index_signature
                            && self.generic_mapped_receiver_lacks_property_access_name(
                                original_object_type,
                                property_name,
                            );
                        if from_index_signature
                            && self
                                .ctx
                                .compiler_options
                                .no_property_access_from_index_signature
                            && !self
                                .union_has_explicit_property_member(resolved_base, property_name)
                        {
                            // Preserve the optional-chain fast path for regular
                            // property reads, but fall back to the full path when
                            // TS4111 must be reported.
                        } else if generic_mapped_missing_named_property {
                            // Generic mapped receivers like
                            // `Record<keyof T | "x", V>` can surface a broad index
                            // signature in the fast solver path even when a specific
                            // named property is not guaranteed for every instantiation.
                            // Fall through so the full path can emit TS2339/TS2551.
                        } else {
                            let refined_type_id = self.refine_expando_property_read_type(
                                idx,
                                access.expression,
                                property_name,
                                type_id,
                            );
                            self.ctx
                                .narrowing_cache
                                .property_cache
                                .borrow_mut()
                                .insert((resolved_base, prop_atom), Some(refined_type_id));
                            let mut result_type =
                                effective_write_result(refined_type_id, write_type);
                            if base_nullish.is_some()
                                && !crate::query_boundaries::common::type_contains_undefined(
                                    self.ctx.types,
                                    result_type,
                                )
                            {
                                result_type = factory.union2(result_type, TypeId::UNDEFINED);
                            }
                            return self.finalize_property_access_result(
                                idx,
                                result_type,
                                skip_flow_narrowing,
                                skip_result_flow_for_result,
                            );
                        }
                    }
                    PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                        self.ctx
                            .narrowing_cache
                            .property_cache
                            .borrow_mut()
                            .insert((resolved_base, prop_atom), property_type);
                        let mut result_type = property_type.unwrap_or(TypeId::ERROR);
                        if base_nullish.is_some()
                            && !crate::query_boundaries::common::type_contains_undefined(
                                self.ctx.types,
                                result_type,
                            )
                        {
                            result_type = factory.union2(result_type, TypeId::UNDEFINED);
                        }
                        return self.finalize_property_access_result(
                            idx,
                            result_type,
                            skip_flow_narrowing,
                            false,
                        );
                    }
                    PropertyAccessResult::PropertyNotFound { .. } => {
                        self.ctx
                            .narrowing_cache
                            .property_cache
                            .borrow_mut()
                            .insert((resolved_base, prop_atom), None);
                        // Fall through to full diagnostic path.
                    }
                    PropertyAccessResult::IsUnknown => {
                        // Fall through to full diagnostic path.
                    }
                }
            }
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

        // Override display type with global value type only when the identifier
        // actually resolves to a global, not when a local variable shadows it.
        if let Some(ident) = self.ctx.arena.get_identifier_at(access.expression)
            && self.is_known_global_value_name(&ident.escaped_text)
        {
            let is_local_shadow = self
                .resolve_identifier_symbol_without_tracking(access.expression)
                .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                .is_some_and(|symbol| {
                    (symbol.flags
                        & (symbol_flags::FUNCTION_SCOPED_VARIABLE
                            | symbol_flags::BLOCK_SCOPED_VARIABLE
                            | symbol_flags::PROPERTY))
                        != 0
                });

            if !is_local_shadow {
                let value_type = self.type_of_value_symbol_by_name(&ident.escaped_text);
                if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                    display_object_type = value_type;
                }
            }
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
                let type_display = if let Some(obj_lit_idx) = self
                    .prior_js_prototype_object_literal_assignment_node(
                        prototype_access.expression,
                        read_pos,
                    ) {
                    let obj_lit_type = self.get_type_of_node(obj_lit_idx);
                    self.format_type(obj_lit_type)
                } else {
                    self.format_type(display_object_type)
                };
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

        // Don't report errors for any/error types - check BEFORE accessibility
        // to prevent cascading errors when the object type is already invalid
        if object_type == TypeId::ANY {
            return TypeId::ANY;
        }
        if object_type == TypeId::ERROR {
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }

        // Property access on `never` emits TS2339 and returns `error` type.
        // In TypeScript, `never` has no properties — accessing any property is an error.
        // Returning `error` (not `never`) matches tsc behavior: when a property doesn't
        // exist, tsc returns `errorType` which suppresses cascading diagnostics (e.g.
        // TS2322 on `ab.y = 'hello'` when `ab: never`).
        // Also handle intersections that contain `never` (e.g., when mixin classes have
        // conflicting private members that reduce the intersection to `never`).
        if object_type == TypeId::NEVER
            || access_query::contains_never_type(self.ctx.types, object_type)
        {
            if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                let property_name = &ident.escaped_text;
                if !property_name.starts_with('#') {
                    // Report at the property name node, not the full expression (matches tsc behavior)
                    self.error_property_not_exist_at(
                        property_name,
                        TypeId::NEVER,
                        access.name_or_argument,
                    );
                }
            }
            return TypeId::ERROR;
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

        // If it's an identifier, look up the property
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;

            if self
                .report_namespace_value_access_for_type_only_import_equals_expr(access.expression)
            {
                return TypeId::ERROR;
            }

            if let Some(base_sym_id) = self.resolve_identifier_symbol(access.expression)
                && let Some(base_symbol) = self.ctx.binder.get_symbol(base_sym_id)
                && base_symbol.has_any_flags(symbol_flags::ALIAS)
                && base_symbol.import_module.is_some()
                && base_symbol
                    .import_name
                    .as_ref()
                    .is_none_or(|name| name == "*")
            {
                if let Some(member_type) =
                    self.resolve_namespace_value_member_from_symbol(base_sym_id, property_name)
                {
                    return self.finalize_property_access_result(
                        idx,
                        member_type,
                        skip_flow_narrowing,
                        false,
                    );
                }

                if self.is_in_type_only_position(idx)
                    && let Some(member_sym_id) =
                        base_symbol
                            .import_module
                            .as_deref()
                            .and_then(|module_specifier| {
                                self.resolve_effective_module_exports_from_file(
                                    module_specifier,
                                    Some(base_symbol.decl_file_idx as usize),
                                )
                                .and_then(|exports| exports.get(property_name))
                            })
                {
                    let member_type = self.get_type_of_symbol(member_sym_id);
                    if member_type != TypeId::ERROR && member_type != TypeId::UNKNOWN {
                        return self.finalize_property_access_result(
                            idx,
                            member_type,
                            skip_flow_narrowing,
                            false,
                        );
                    }
                }
            }

            let enum_instance_like_access = self
                .is_enum_instance_property_access(object_type, access.expression)
                || access_query::type_parameter_constraint(self.ctx.types, object_type)
                    .is_some_and(|constraint| {
                        access_query::enum_def_id(self.ctx.types, constraint).is_some()
                    });
            let hidden_qualified_namespace_member_apparent_type = self
                .qualified_namespace_member_hidden_on_exported_surface(
                    idx,
                    access.expression,
                    property_name,
                );
            let hidden_qualified_namespace_member =
                hidden_qualified_namespace_member_apparent_type.is_some();

            if !skip_flow_narrowing
                && !enum_instance_like_access
                && !hidden_qualified_namespace_member
                && let Some(obj_node) = self.ctx.arena.get(access.expression)
                && let Some(obj_ident) = self.ctx.arena.get_identifier(obj_node)
                && let Some(member_type) =
                    self.resolve_umd_global_member_by_name(&obj_ident.escaped_text, property_name)
            {
                if let Some(umd_sym_id) =
                    self.resolve_umd_global_symbol_by_name(&obj_ident.escaped_text)
                {
                    let is_pure_umd_alias = self
                        .get_cross_file_symbol(umd_sym_id)
                        .or_else(|| self.ctx.binder.get_symbol(umd_sym_id))
                        .is_some_and(|symbol| {
                            symbol.is_umd_export
                                && (symbol.flags & tsz_binder::symbol_flags::VALUE) == 0
                        });
                    if is_pure_umd_alias
                        && self.current_file_is_module_for_umd_global_access()
                        && !self.ctx.compiler_options.allow_umd_global_access
                        && !self.has_non_umd_global_value(&obj_ident.escaped_text)
                    {
                        use crate::diagnostics::diagnostic_codes;
                        self.error_at_node_msg(
                            access.expression,
                            diagnostic_codes::REFERS_TO_A_UMD_GLOBAL_BUT_THE_CURRENT_FILE_IS_A_MODULE_CONSIDER_ADDING_AN_IMPOR,
                            &[&obj_ident.escaped_text],
                        );
                    }
                }
                return self.finalize_property_access_result(
                    idx,
                    member_type,
                    skip_flow_narrowing,
                    false,
                );
            }

            if !skip_flow_narrowing
                && !enum_instance_like_access
                && !hidden_qualified_namespace_member
                && let Some(member_type) =
                    self.resolve_shadowed_global_value_member(access.expression, property_name)
            {
                return self.finalize_property_access_result(
                    idx,
                    member_type,
                    skip_flow_narrowing,
                    false,
                );
            }

            // Fallback for namespace/export member accesses where type-only namespace
            // classification misses the object form but symbol resolution can still
            // identify `A.B` as a concrete exported value member.
            if !hidden_qualified_namespace_member
                && let Some(member_sym_id) = self.resolve_qualified_symbol(idx)
                && let Some(member_symbol) = self
                    .get_cross_file_symbol(member_sym_id)
                    .or_else(|| self.ctx.binder.get_symbol(member_sym_id))
            {
                // Skip type-only members (e.g., `export type { A }`, interfaces).
                // These should not be resolved as values; let the code fall
                // through to TS2693 "type only" or TS2339 "property doesn't exist" handling.
                let transitively_type_only = self
                    .is_namespace_member_transitively_type_only(access.expression, property_name);
                if !member_symbol.is_type_only
                    && !self.symbol_member_is_type_only(member_sym_id, Some(property_name))
                    && member_symbol.has_any_flags(symbol_flags::VALUE)
                    && !transitively_type_only
                    // For merged symbols (e.g., namespace + interface), verify that the VALUE
                    // part is actually exported. If only the TYPE part is exported, the value
                    // is not accessible at runtime.
                    && self.symbol_has_exported_value_declaration(member_sym_id)
                {
                    let parent_sym_id = member_symbol.parent;
                    if let Some(parent_symbol) = self
                        .get_cross_file_symbol(parent_sym_id)
                        .or_else(|| self.ctx.binder.get_symbol(parent_sym_id))
                        && parent_symbol.has_any_flags(symbol_flags::MODULE | symbol_flags::ENUM)
                    {
                        // If the member is an enum (not an enum member), return
                        // the enum object type so property access on enum members
                        // (e.g., M3.Color.Blue) resolves correctly.
                        let member_type = if member_symbol.has_any_flags(symbol_flags::ENUM)
                            && !member_symbol.has_any_flags(symbol_flags::ENUM_MEMBER)
                        {
                            self.enum_object_type(member_sym_id)
                                .unwrap_or_else(|| self.get_type_of_symbol(member_sym_id))
                        } else if member_symbol.has_any_flags(symbol_flags::INTERFACE)
                            && member_symbol.has_any_flags(symbol_flags::VALUE)
                        {
                            // When a namespace member is both an interface and a value
                            // (e.g., `interface NumberFormat` + `var NumberFormat: { new(): ... }`
                            // in namespace Intl), resolve the value declaration's type so
                            // construct signatures are available for `new NS.Member()`.
                            // This mirrors the merged-symbol resolution in get_type_of_identifier.
                            let value_decl = member_symbol.value_declaration;
                            let declarations = member_symbol.declarations.clone();
                            let preferred = self
                                .preferred_value_declaration(
                                    member_sym_id,
                                    value_decl,
                                    &declarations,
                                )
                                .unwrap_or(value_decl);
                            let mut val_type =
                                self.type_of_value_declaration_for_symbol(member_sym_id, preferred);
                            if val_type == TypeId::UNKNOWN || val_type == TypeId::ERROR {
                                for &decl_idx in &declarations {
                                    if decl_idx == preferred {
                                        continue;
                                    }
                                    let candidate = self.type_of_value_declaration_for_symbol(
                                        member_sym_id,
                                        decl_idx,
                                    );
                                    if candidate != TypeId::UNKNOWN && candidate != TypeId::ERROR {
                                        val_type = candidate;
                                        break;
                                    }
                                }
                            }
                            if val_type != TypeId::UNKNOWN && val_type != TypeId::ERROR {
                                val_type
                            } else {
                                self.get_type_of_symbol(member_sym_id)
                            }
                        } else {
                            // For merged interface+variable symbols (e.g.,
                            // `interface Foo` + `var Foo: FooConstructor`), prefer the
                            // variable's type in value position so construct signatures
                            // are visible to `new` expressions.
                            self.merged_value_type_for_symbol_if_available(member_sym_id)
                                .unwrap_or_else(|| self.get_type_of_symbol(member_sym_id))
                        };
                        if member_type != TypeId::ERROR && member_type != TypeId::UNKNOWN {
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

            if self.namespace_has_type_only_member(object_type, property_name) {
                if self.is_js_file()
                    && self.ctx.compiler_options.check_js
                    && let Some(ns_name) = self.entity_name_text(access.expression)
                    && let Some(member_sym_id) =
                        self.resolve_namespace_member_from_all_binders(&ns_name, property_name)
                {
                    if !self.symbol_member_is_type_only(member_sym_id, Some(property_name)) {
                        let value_type = self.get_type_of_symbol(member_sym_id);
                        if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                            return value_type;
                        }
                    }

                    if let Some(member_symbol) = self
                        .ctx
                        .binder
                        .get_symbol(member_sym_id)
                        .or_else(|| self.get_cross_file_symbol(member_sym_id))
                    {
                        let checked_js_decl = if member_symbol.value_declaration.is_some() {
                            self.checked_js_constructor_value_declaration(
                                member_sym_id,
                                member_symbol.value_declaration,
                                &member_symbol.declarations,
                            )
                        } else {
                            member_symbol
                                .declarations
                                .iter()
                                .copied()
                                .find(|&decl_idx| {
                                    self.declaration_is_checked_js_constructor_value_declaration(
                                        member_sym_id,
                                        decl_idx,
                                    )
                                })
                        };
                        if let Some(checked_js_decl) = checked_js_decl {
                            let value_type = self.type_of_value_declaration_for_symbol(
                                member_sym_id,
                                checked_js_decl,
                            );
                            if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                                return value_type;
                            }
                        }
                    }
                }
                // Suppress TS2339/TS2693 when base expression is a property access on an unresolved import
                // TS2307 was already emitted for the missing module, so we shouldn't
                // emit additional errors about properties not existing on the import.
                if self.is_property_access_on_unresolved_import(access.expression) {
                    return TypeId::ERROR;
                }
                // Don't emit TS2693 in heritage clause context — the heritage
                // checker will emit the appropriate error (e.g., TS2689).
                // Also suppress in JS/checkJs when the access sits on an
                // assignment LHS chain (e.g., `ns.Interface = function() {}`
                // or `ns.Interface.prototype.fn = ...`). tsc treats these as
                // prototype-property-assignment merges and does not emit TS2708.
                if self
                    .find_enclosing_heritage_clause(access.name_or_argument)
                    .is_none()
                    && !(self.is_js_file()
                        && self.ctx.compiler_options.check_js
                        && self.property_access_is_write_target_or_base(idx))
                {
                    // Emit TS2708 for namespace member access (e.g., ns.Interface())
                    // This is "Cannot use namespace as a value"
                    // Get the namespace name from the left side of the access
                    if let Some(ns_name) = self.entity_name_text(access.expression) {
                        self.report_wrong_meaning_diagnostic(
                            &ns_name,
                            access.expression,
                            crate::query_boundaries::name_resolution::NameLookupKind::Namespace,
                        );
                    }
                    // tsc does NOT emit TS2693 for the type-only member
                    // when TS2708 was already emitted for the namespace.
                }
                return TypeId::ERROR;
            }
            if let Some(display_type) = hidden_qualified_namespace_member_apparent_type.as_deref() {
                if !access.question_dot_token
                    && !property_name.starts_with('#')
                    && !accessibility_error_emitted
                {
                    self.error_property_not_exist_with_apparent_type(
                        property_name,
                        display_type,
                        access.name_or_argument,
                    );
                }
                return TypeId::ERROR;
            }
            if self.is_namespace_value_type(object_type) && !enum_instance_like_access {
                let hidden_qualified_namespace_member_apparent_type = self
                    .qualified_namespace_member_hidden_on_exported_surface(
                        idx,
                        access.expression,
                        property_name,
                    );
                let hidden_qualified_namespace_member =
                    hidden_qualified_namespace_member_apparent_type.is_some();
                if !hidden_qualified_namespace_member {
                    let namespace_object_type = self.resolve_type_for_property_access(object_type);
                    if let Some(member_type) =
                        self.resolve_namespace_value_member(namespace_object_type, property_name)
                    {
                        return self.finalize_property_access_result(
                            idx,
                            member_type,
                            skip_flow_narrowing,
                            false,
                        );
                    }
                }

                // When the object type is a TypeQuery (typeof M) for a namespace,
                // try to resolve the property from the namespace symbol's exports.
                // This handles `var m: typeof M; m.Point` where `m` is a variable
                // typed as `typeof Namespace`.
                if let Some(ns_member_type) =
                    self.resolve_namespace_typeof_member(object_type, property_name)
                {
                    return self.finalize_property_access_result(
                        idx,
                        ns_member_type,
                        skip_flow_narrowing,
                        false,
                    );
                }
                if self.is_js_file()
                    && property_name == "prototype"
                    && self.property_access_is_direct_write_target(idx)
                    && !self
                        .resolve_identifier_symbol(access.expression)
                        .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                        .is_some_and(|sym| {
                            sym.has_any_flags(symbol_flags::ALIAS) && sym.import_module.is_some()
                        })
                {
                    return TypeId::ANY;
                }
                if self.find_enclosing_computed_property(idx).is_some()
                    && self.get_symbol_property_name_from_expr(idx).is_some()
                {
                    return TypeId::SYMBOL;
                }
                if !access.question_dot_token
                    && !property_name.starts_with('#')
                    && !accessibility_error_emitted
                    && !self.is_property_access_on_unresolved_import(access.expression)
                {
                    // Check if the base expression is an uninstantiated namespace.
                    // tsc emits TS2708 "Cannot use namespace 'X' as a value" on the
                    // namespace identifier, not TS2339 on the property.
                    if let Some(ns_name) = self.uninstantiated_namespace_name(access.expression) {
                        self.report_wrong_meaning_diagnostic(
                            &ns_name,
                            access.expression,
                            crate::query_boundaries::name_resolution::NameLookupKind::Namespace,
                        );
                    } else {
                        self.error_property_not_exist_at(
                            property_name,
                            display_object_type,
                            access.name_or_argument,
                        );
                    }
                }
                return TypeId::ERROR;
            }

            let external_prototype_owner_instance_type = self
                .find_enclosing_non_arrow_function(access.expression)
                .and_then(|func_idx| self.js_prototype_owner_expression_for_node(func_idx))
                .and_then(|owner_expr| {
                    // Only for external/imported prototype owners. Local function/class
                    // owners are handled by regular JS prototype-this logic.
                    if self
                        .js_prototype_owner_function_target(owner_expr)
                        .is_some()
                    {
                        return None;
                    }
                    let owner_type = self.get_type_of_node(owner_expr);
                    if owner_type == TypeId::ANY
                        || owner_type == TypeId::UNKNOWN
                        || owner_type == TypeId::ERROR
                    {
                        return None;
                    }
                    let owner_type_for_access = self.resolve_type_for_property_access(owner_type);
                    match self.resolve_property_access_with_env(owner_type_for_access, "prototype")
                    {
                        PropertyAccessResult::Success { type_id, .. }
                        | PropertyAccessResult::PossiblyNullOrUndefined {
                            property_type: Some(type_id),
                            ..
                        } => Some(type_id),
                        _ => None,
                    }
                });

            let mut object_type_for_access = if enum_instance_like_access {
                self.apparent_enum_instance_type(object_type)
                    .unwrap_or_else(|| self.resolve_type_for_property_access(object_type))
            } else {
                self.resolve_type_for_property_access(object_type)
            };
            if object_type_for_access == TypeId::ANY
                && is_this_access
                && let Some(owner_instance_type) = external_prototype_owner_instance_type
            {
                object_type_for_access = owner_instance_type;
            }
            if object_type_for_access == TypeId::ANY {
                return TypeId::ANY;
            }
            if object_type_for_access == TypeId::ERROR {
                return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
            }

            // In write context (skip_flow_narrowing), skip this shortcut:
            // resolve_namespace_value_member returns the symbol's read type, which
            // doesn't account for divergent getter/setter types. The full property
            // access path below correctly uses write_type for setter parameters.
            //
            // Do this after resolving the base type for property access so cross-file
            // enum/namespace objects (e.g. imported class statics initialized to enums)
            // classify the same way as local ones.
            if !skip_flow_narrowing
                && !enum_instance_like_access
                && !hidden_qualified_namespace_member
                && let Some(member_type) =
                    self.resolve_namespace_value_member(object_type_for_access, property_name)
            {
                return self.finalize_property_access_result(
                    idx,
                    member_type,
                    skip_flow_narrowing,
                    false,
                );
            }

            if self.ctx.strict_bind_call_apply()
                && let Some(strict_method_type) = self.strict_bind_call_apply_method_type(
                    object_type_for_access,
                    access.expression,
                    property_name,
                )
            {
                return self.finalize_property_access_result(
                    idx,
                    strict_method_type,
                    skip_flow_narrowing,
                    false,
                );
            }

            if let Some(iterator_method_type) =
                self.synthesized_array_iterator_method_type(object_type_for_access, property_name)
            {
                return self.finalize_property_access_result(
                    idx,
                    iterator_method_type,
                    skip_flow_narrowing,
                    false,
                );
            }

            if self.is_super_expression(access.expression)
                && let Some((class_idx, is_static_access)) =
                    self.resolve_class_for_access(access.expression, object_type_for_access)
                && !is_static_access
                && matches!(
                    self.summarize_class_chain(class_idx)
                        .member_kind(property_name, false, true),
                    Some(ClassMemberKind::FieldLike)
                )
            {
                return TypeId::ANY;
            }

            // Use the environment-aware resolver so that array methods, boxed
            // primitive types, and other lib-registered types are available.
            let mut result =
                self.resolve_property_access_with_env(object_type_for_access, property_name);
            // Flow predicate narrowing can produce unions/intersections like
            // `C2 | (C2 & C1)` or `(D1 & C2) | (D1 & C1)`. Looking up properties
            // directly on those unevaluated shells may fall back to a bare `any`.
            // Retry on the evaluated receiver to recover the concrete property type.
            if matches!(
                result,
                PropertyAccessResult::Success {
                    type_id: TypeId::ANY,
                    from_index_signature: false,
                    ..
                }
            ) && !crate::query_boundaries::state::checking::is_type_parameter_like(
                self.ctx.types,
                object_type_for_access,
            ) {
                let evaluated_receiver = self.evaluate_type_with_env(object_type_for_access);
                if evaluated_receiver != object_type_for_access
                    && evaluated_receiver != TypeId::ANY
                    && evaluated_receiver != TypeId::ERROR
                {
                    let retry =
                        self.resolve_property_access_with_env(evaluated_receiver, property_name);
                    let retry_improved = match retry {
                        PropertyAccessResult::Success {
                            type_id,
                            from_index_signature,
                            ..
                        } => type_id != TypeId::ANY || from_index_signature,
                        _ => true,
                    };
                    if retry_improved {
                        object_type_for_access = evaluated_receiver;
                        result = retry;
                    }
                }
            }
            match result {
                PropertyAccessResult::Success {
                    type_id: mut prop_type,
                    write_type,
                    from_index_signature,
                } => {
                    if property_name == "exports"
                        && prop_type == TypeId::ANY
                        && self.is_js_file()
                        && let Some(obj_node) = self.ctx.arena.get(access.expression)
                        && let Some(ident) = self.ctx.arena.get_identifier(obj_node)
                        && ident.escaped_text == "module"
                        && self.current_file_commonjs_module_identifier_is_unshadowed(
                            access.expression,
                        )
                    {
                        return self.current_file_commonjs_module_exports_namespace_type();
                    }

                    // A bare type-parameter receiver can fall back to `any` here
                    // when the constraint only exposes the property on some union
                    // members. Preserve TS2339 for direct reads like `value.foo`
                    // but avoid firing after control-flow has already refined the
                    // receiver to a narrower view.
                    if !skip_flow_narrowing
                        && !from_index_signature
                        && prop_type == TypeId::ANY
                        && object_type == object_type_for_access
                        && object_type_for_access == original_object_type
                        && crate::query_boundaries::state::checking::is_type_parameter_like(
                            self.ctx.types,
                            object_type_for_access,
                        )
                        && !self.type_parameter_constraint_has_explicit_property(
                            object_type_for_access,
                            property_name,
                        )
                    {
                        // Suppress TS2339 for index access types on type parameters.
                        // When accessing properties on types like T[keyof T], we cannot
                        // determine what properties exist until T is instantiated.
                        if !crate::query_boundaries::common::is_index_access_type(
                            self.ctx.types,
                            object_type_for_access,
                        ) {
                            self.error_property_not_exist_at(
                                property_name,
                                object_type_for_access,
                                access.name_or_argument,
                            );
                        }
                        return TypeId::ERROR;
                    }

                    // Substitute polymorphic `this` type with the receiver type.
                    // E.g., for `class C<T> { x = this; }`, accessing `c.x` where
                    // `c: C<string>` should yield `C<string>`, not raw `ThisType`.
                    //
                    // `super.method` is special: the property lookup happens on the
                    // base instance type, but polymorphic `this` in the base member
                    // should still bind to the current derived receiver. Without
                    // this, `super.compare(other)` inside `Dog.compare(other: this)`
                    // sees the base signature as `(other: Animal) => boolean`
                    // instead of `(other: Dog) => boolean`, which diverges from tsc.
                    let this_substitution_target = if self.is_super_expression(access.expression) {
                        self.current_this_type().unwrap_or(original_object_type)
                    } else {
                        original_object_type
                    };
                    //
                    // Skip substitution when prop_type IS the receiver type. This
                    // prevents creating a new TypeId when accessing properties like
                    // `self2: D` where D is the current class instance type. Without
                    // this guard, `this.self2` would return D_subst (a new TypeId)
                    // instead of D, causing assignment mismatches in polymorphic
                    // `this` checks (e.g., `this.self = this.self2` would fail
                    // because D_subst != D even though they're semantically equal).
                    if crate::query_boundaries::common::contains_this_type(
                        self.ctx.types,
                        prop_type,
                    ) && prop_type != this_substitution_target
                    {
                        prop_type = crate::query_boundaries::common::substitute_this_type(
                            self.ctx.types,
                            prop_type,
                            this_substitution_target,
                        );
                    } else {
                        // When a method returns `this` on an intersection member,
                        // the solver's object visitor eagerly binds `this` to the
                        // structural (flattened) object — so `contains_this_type`
                        // above returns false. Re-resolve with `this` binding
                        // deferred to recover raw `ThisType`, then substitute with
                        // the nominal receiver (e.g., Thing5 instead of {a,b,c}).
                        let raw = crate::query_boundaries::property_access::resolve_property_access_raw_this(
                            self.ctx.types,
                            object_type_for_access,
                            property_name,
                        );
                        if let PropertyAccessResult::Success {
                            type_id: raw_type, ..
                        } = raw
                            && crate::query_boundaries::common::contains_this_type(
                                self.ctx.types,
                                raw_type,
                            )
                        {
                            prop_type = crate::query_boundaries::common::substitute_this_type(
                                self.ctx.types,
                                raw_type,
                                this_substitution_target,
                            );
                        }
                    }

                    if skip_flow_narrowing
                        && from_index_signature
                        && crate::query_boundaries::state::checking::is_type_parameter_like(
                            self.ctx.types,
                            object_type,
                        )
                        && !self.type_parameter_constraint_has_explicit_property(
                            object_type,
                            property_name,
                        )
                    {
                        self.error_property_not_exist_at(
                            property_name,
                            object_type,
                            access.name_or_argument,
                        );
                        return TypeId::ERROR;
                    }

                    if skip_flow_narrowing
                        && from_index_signature
                        && self.generic_mapped_receiver_lacks_property_access_name(
                            original_object_type,
                            property_name,
                        )
                    {
                        self.error_property_not_exist_at(
                            property_name,
                            original_object_type,
                            access.name_or_argument,
                        );
                        return TypeId::ERROR;
                    }

                    let union_has_explicit_member = from_index_signature
                        && self.union_has_explicit_property_member(
                            object_type_for_access,
                            property_name,
                        );
                    // Check for error 4111: property access from index signature
                    if from_index_signature
                        && self
                            .ctx
                            .compiler_options
                            .no_property_access_from_index_signature
                        && !union_has_explicit_member
                    {
                        use crate::diagnostics::diagnostic_codes;
                        self.error_at_node(
                            access.name_or_argument,
                            &format!(
                                "Property '{property_name}' comes from an index signature, so it must be accessed with ['{property_name}']."
                            ),
                            diagnostic_codes::PROPERTY_COMES_FROM_AN_INDEX_SIGNATURE_SO_IT_MUST_BE_ACCESSED_WITH,
                        );
                    }
                    if skip_flow_narrowing
                        && self.union_write_requires_existing_named_member(
                            object_type_for_access,
                            property_name,
                        )
                    {
                        self.error_property_not_exist_at(
                            property_name,
                            object_type_for_access,
                            access.name_or_argument,
                        );
                        return TypeId::ERROR;
                    }
                    // When in a write context (assignment target), use the setter
                    // type if the property has divergent getter/setter types.
                    let effective_type = effective_write_result(prop_type, write_type);
                    self.finalize_property_access_result(
                        idx,
                        effective_type,
                        skip_flow_narrowing,
                        skip_result_flow_for_result,
                    )
                }

                PropertyAccessResult::PropertyNotFound { .. } => {
                    if self.is_stale_unconstrained_type_parameter(object_type_for_access) {
                        // Stale type parameter from two-pass resolution.
                        // The updated version in scope has a constraint (likely ERROR),
                        // so suppress the cascading TS2339.
                        return TypeId::ERROR;
                    }

                    // Special case: unconstrained type parameters should emit TS2339
                    // because they have no properties by definition.
                    if crate::query_boundaries::state::checking::is_type_parameter_like(
                        self.ctx.types,
                        object_type_for_access,
                    ) && crate::query_boundaries::common::type_parameter_constraint(
                        self.ctx.types,
                        object_type_for_access,
                    )
                    .is_none()
                    {
                        // Genuinely unconstrained type parameter - emit TS2339
                        if !property_name.starts_with('#') && !accessibility_error_emitted {
                            self.error_property_not_exist_at(
                                property_name,
                                object_type_for_access,
                                access.name_or_argument,
                            );
                        }
                        return TypeId::ERROR;
                    }

                    // For JS files with checkJs enabled, when accessing properties on
                    // new expression results that don't exist, emit TS2339 instead of
                    // falling through to expando/any fallbacks. This ensures proper
                    // error reporting for imported class instances like `new A().foo`.
                    if self.is_js_file()
                        && self.ctx.compiler_options.check_js
                        && !skip_flow_narrowing
                        && !accessibility_error_emitted
                        && !property_name.starts_with('#')
                    {
                        // Check if the object expression is a new expression
                        let is_new_expression = self
                            .ctx
                            .arena
                            .get(access.expression)
                            .is_some_and(|n| n.kind == syntax_kind_ext::NEW_EXPRESSION);

                        if is_new_expression {
                            self.error_property_not_exist_at(
                                property_name,
                                object_type_for_access,
                                access.name_or_argument,
                            );
                            return TypeId::ERROR;
                        }
                    }

                    let resolved_class_access =
                        self.resolve_class_for_access(access.expression, object_type_for_access);
                    let class_chain_summary = resolved_class_access
                        .map(|(class_idx, _)| self.summarize_class_chain(class_idx));
                    let static_this_member_context = is_this_access
                        && (self
                            .find_enclosing_static_block(access.expression)
                            .is_some()
                            || self
                                .find_enclosing_function(access.expression)
                                .map(|func_idx| {
                                    let mut member_idx = func_idx;
                                    if let Some(func_node) = self.ctx.arena.get(func_idx)
                                        && (func_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                                            || func_node.kind
                                                == syntax_kind_ext::FUNCTION_EXPRESSION)
                                        && let Some(ext) = self.ctx.arena.get_extended(func_idx)
                                        && let Some(parent_node) = self.ctx.arena.get(ext.parent)
                                        && (parent_node.kind == syntax_kind_ext::METHOD_DECLARATION
                                            || parent_node.kind == syntax_kind_ext::GET_ACCESSOR
                                            || parent_node.kind == syntax_kind_ext::SET_ACCESSOR)
                                    {
                                        member_idx = ext.parent;
                                    }
                                    self.class_member_is_static(member_idx)
                                })
                                .unwrap_or(false));

                    if !access.question_dot_token
                        && static_this_member_context
                        && let Some(class_idx) = self.nearest_enclosing_class(access.expression)
                    {
                        let summary = self.summarize_class_chain(class_idx);
                        if let Some(member_info) = summary.lookup(property_name, true, true) {
                            return self.finalize_property_access_result(
                                idx,
                                effective_write_result(
                                    member_info.type_id,
                                    Some(member_info.type_id),
                                ),
                                skip_flow_narrowing,
                                false,
                            );
                        }
                        if summary.lookup(property_name, false, true).is_some() {
                            self.error_property_not_exist_at(
                                property_name,
                                object_type_for_access,
                                access.name_or_argument,
                            );
                            return TypeId::ERROR;
                        }
                    }

                    if !access.question_dot_token
                        && is_this_access
                        && let Some((_, is_static_access)) = resolved_class_access
                        && is_static_access
                        && let Some(summary) = class_chain_summary.as_ref()
                        && summary.lookup(property_name, true, true).is_none()
                        && summary.lookup(property_name, false, true).is_some()
                    {
                        self.error_property_not_exist_at(
                            property_name,
                            object_type_for_access,
                            access.name_or_argument,
                        );
                        return TypeId::ERROR;
                    }

                    if let Some(augmented_type) = self.resolve_array_global_augmentation_property(
                        object_type_for_access,
                        property_name,
                    ) {
                        return self.finalize_property_access_result(
                            idx,
                            augmented_type,
                            skip_flow_narrowing,
                            false,
                        );
                    }
                    // Check global interface augmentations for primitive wrappers
                    // and other built-in types (e.g., `interface Boolean { doStuff() }`)
                    if let Some(augmented_type) = self.resolve_general_global_augmentation_property(
                        object_type_for_access,
                        property_name,
                    ) {
                        return self.finalize_property_access_result(
                            idx,
                            augmented_type,
                            skip_flow_narrowing,
                            false,
                        );
                    }
                    // Check module augmentations (declare module "X" { interface Y { ... } })
                    // for properties added by cross-file augmentation declarations.
                    if let Some(augmented_type) = self
                        .resolve_module_augmentation_property(object_type_for_access, property_name)
                    {
                        return self.finalize_property_access_result(
                            idx,
                            augmented_type,
                            skip_flow_narrowing,
                            false,
                        );
                    }
                    // For callable/function types, check the Function interface
                    // for augmented members (e.g., declare global { interface Function { ... } })
                    if crate::query_boundaries::property_access::is_function_type(
                        self.ctx.types,
                        object_type_for_access,
                    ) && let Some(func_iface) = self.resolve_lib_type_by_name("Function")
                        && let PropertyAccessResult::Success { type_id, .. } =
                            self.resolve_property_access_with_env(func_iface, property_name)
                    {
                        return self.finalize_property_access_result(
                            idx,
                            type_id,
                            skip_flow_narrowing,
                            false,
                        );
                    }
                    if let Some((class_idx, is_static_access)) = resolved_class_access
                        && !is_static_access
                        && let Some(interface_type) = self
                            .recover_property_from_implemented_interfaces(class_idx, property_name)
                    {
                        return self.finalize_property_access_result(
                            idx,
                            interface_type,
                            skip_flow_narrowing,
                            false,
                        );
                    }
                    // Check for optional chaining (?.) - suppress TS2339 error when using optional chaining
                    if access.question_dot_token {
                        // With optional chaining, missing property results in undefined
                        return TypeId::UNDEFINED;
                    }
                    // In JS checkJs mode, unresolved CommonJS `module.exports` accesses
                    // should use the current file's export surface instead of `any`.
                    if property_name == "exports"
                        && self.is_js_file()
                        && let Some(obj_node) = self.ctx.arena.get(access.expression)
                        && let Some(ident) = self.ctx.arena.get_identifier(obj_node)
                        && ident.escaped_text == "module"
                        && self.current_file_commonjs_module_identifier_is_unshadowed(
                            access.expression,
                        )
                    {
                        return self.current_file_commonjs_module_exports_namespace_type();
                    }
                    if self.is_js_file()
                        && self.ctx.compiler_options.check_js
                        && let Some(jsdoc_type) = self
                            .enclosing_expression_statement(idx)
                            .and_then(|stmt_idx| self.js_statement_declared_type(stmt_idx))
                            .or_else(|| self.jsdoc_type_annotation_for_node_direct(idx))
                            .or_else(|| {
                                self.jsdoc_type_annotation_for_node_direct(access.expression)
                            })
                            .or_else(|| {
                                let root = self.expression_root(idx);
                                (root != idx)
                                    .then(|| self.jsdoc_type_annotation_for_node_direct(root))?
                            })
                    {
                        return jsdoc_type;
                    }
                    let skip_js_write_assigned_value_fallback = skip_flow_narrowing
                        && self.property_access_is_direct_write_target(idx)
                        && self.is_js_file()
                        && self.ctx.compiler_options.check_js
                        && self
                            .ctx
                            .arena
                            .get(access.expression)
                            .is_some_and(|expr_node| {
                                expr_node.kind == SyntaxKind::Identifier as u16
                            })
                        && self
                            .ctx
                            .arena
                            .get_identifier_at(access.expression)
                            .is_some_and(|ident| {
                                self.cross_file_global_value_type_by_name(
                                    &ident.escaped_text,
                                    false,
                                )
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
                    if self.is_js_file()
                        && self.ctx.compiler_options.check_js
                        && !skip_js_write_assigned_value_fallback
                        && let Some(expr_text) = self.expression_text(idx)
                        && let Some(jsdoc_type) = if skip_flow_narrowing
                            && self.property_access_is_direct_write_target(idx)
                        {
                            self.resolve_jsdoc_assigned_value_type_for_write(&expr_text)
                        } else {
                            self.resolve_jsdoc_assigned_value_type(&expr_text)
                        }
                    {
                        return jsdoc_type;
                    }
                    if js_expando_before_assignment {
                        return TypeId::ANY;
                    }
                    // Check for expando property reads: X.prop where X.prop = value was assigned
                    // Recover the assigned value type when we can, then fall back to `any`.
                    if !skip_flow_narrowing
                        && !commonjs_named_props_disallowed
                        && self.is_expando_property_read(access.expression, property_name)
                    {
                        if let Some(expando_type) =
                            self.expando_property_read_type(idx, access.expression, property_name)
                        {
                            return expando_type;
                        }
                        return TypeId::ANY;
                    }
                    // Check for expando function pattern: func.prop = value
                    // TypeScript allows property assignments to function/class declarations
                    // without emitting TS2339. The assigned properties become part of the
                    // function's type (expando pattern).
                    let static_class_this_write = is_this_access
                        && resolved_class_access
                            .is_some_and(|(_, is_static_access)| is_static_access);
                    if !commonjs_named_props_disallowed
                        && !static_class_this_write
                        && self.is_expando_function_assignment(
                            idx,
                            access.expression,
                            object_type_for_access,
                        )
                    {
                        return TypeId::ANY;
                    }
                    if self.is_js_expando_object_assignment(
                        idx,
                        access.expression,
                        object_type_for_access,
                        property_name,
                    ) {
                        return TypeId::ANY;
                    }

                    // JavaScript files allow dynamic property assignment on 'this' without errors.
                    // In JS files, accessing a property on 'this' that doesn't exist should not error
                    // and should return 'any' type, matching TypeScript's behavior.
                    let has_explicit_this_context = is_this_access
                        && self
                            .current_this_type()
                            .is_some_and(|ty| ty != TypeId::ANY && ty != TypeId::UNKNOWN);
                    // When `this` type comes from a ThisType<T> marker (e.g., Vue 2
                    // Options API pattern), property access on unresolved type parameters
                    // should not emit TS2339. The type parameters will be inferred from the
                    // object literal, creating a circular dependency that tsc handles by
                    // deferring the check.
                    // Also handle intersections containing type parameters (e.g.,
                    // `Data & Readonly<Props> & Instance` from
                    // `ThisType<Data & Readonly<Props> & Instance>` before inference).
                    // Only suppress when `this` doesn't have an explicit type context
                    // to ensure we still emit TS2339 for regular object literal methods.
                    let this_owner_is_object_literal = self
                        .this_has_contextual_owner(access.expression)
                        .and_then(|owner_idx| self.ctx.arena.get(owner_idx))
                        .is_some_and(|owner_node| {
                            owner_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        });
                    let this_prototype_owner_expr = self
                        .find_enclosing_non_arrow_function(access.expression)
                        .and_then(|func_idx| self.js_prototype_owner_expression_for_node(func_idx));
                    let this_owner_is_js_prototype_method =
                        this_prototype_owner_expr.is_some_and(|owner_expr| {
                            self.js_prototype_owner_function_target(owner_expr)
                                .is_some()
                        });
                    let this_owner_is_external_js_prototype_method = this_prototype_owner_expr
                        .is_some_and(|owner_expr| {
                            self.js_prototype_owner_function_target(owner_expr)
                                .is_none()
                        });
                    if is_this_access
                        && this_owner_is_object_literal
                        && !has_explicit_this_context
                        && self.ctx.this_type_stack.last().is_some_and(|&top| {
                            access_query::is_this_type(self.ctx.types, top)
                                || crate::query_boundaries::common::contains_type_parameters(
                                    self.ctx.types,
                                    top,
                                )
                                || crate::query_boundaries::common::contains_type_parameters(
                                    self.ctx.types,
                                    top,
                                )
                        })
                    {
                        return TypeId::ANY;
                    }

                    if self.is_js_file()
                        && is_this_access
                        && this_owner_is_js_prototype_method
                        && self.property_access_is_direct_write_target(idx)
                    {
                        return TypeId::ANY;
                    }

                    if self.is_js_file()
                        && is_this_access
                        && skip_flow_narrowing
                        && self.property_access_is_direct_write_target(idx)
                        && !this_owner_is_external_js_prototype_method
                    {
                        let object_literal_owned_this = self
                            .this_has_contextual_owner(access.expression)
                            .and_then(|owner_idx| self.ctx.arena.get(owner_idx))
                            .is_some_and(|owner_node| {
                                owner_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                            });
                        let prototype_object_literal_expando_write = object_literal_owned_this
                            && self.is_js_prototype_object_literal_expando_write(
                                access.expression,
                                property_name,
                            );
                        if !object_literal_owned_this || prototype_object_literal_expando_write {
                            return TypeId::ANY;
                        }
                    }

                    if self.is_js_file() && is_this_access && !has_explicit_this_context {
                        // Allow dynamic property on `this` in loose JS contexts, but
                        // keep checks when `this` is contextually owned by a class/object
                        // member (checkJs should still enforce member-consistent typing).
                        if self.this_has_contextual_owner(access.expression).is_none() {
                            return TypeId::ANY;
                        }
                        if self.is_jsdoc_annotated_this_member_declaration(idx) {
                            return TypeId::ANY;
                        }
                    }

                    if self.is_js_file()
                        && property_name == "prototype"
                        && self.property_access_is_direct_write_target(idx)
                        && !self
                            .resolve_identifier_symbol(access.expression)
                            .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                            .is_some_and(|sym| {
                                sym.has_any_flags(symbol_flags::ALIAS)
                                    && sym.import_module.is_some()
                            })
                    {
                        return TypeId::ANY;
                    }

                    if self.is_js_file()
                        && self.is_super_expression(access.expression)
                        && let Some((_, is_static_access)) = resolved_class_access
                        && is_static_access
                        && matches!(
                            class_chain_summary
                                .as_ref()
                                .and_then(|summary| summary.member_kind(property_name, true, true)),
                            Some(ClassMemberKind::FieldLike)
                        )
                    {
                        return TypeId::ANY;
                    }

                    // TSC does not emit TS2576 for `super.member` access. When accessing a
                    // property through `super`, TypeScript suppresses "did you mean to access
                    // the static member?" errors entirely. The TS2576 check only applies to
                    // regular instance access (e.g., `instance.y` where `y` is static), not
                    // super access. See: superAccess2.ts — `super.y()` in instance method and
                    // `super.x()` in static method produce no TS2576 errors in tsc.

                    if let Some((_, is_static_access)) = resolved_class_access
                        && is_static_access
                        && let Some(member_info) = class_chain_summary
                            .as_ref()
                            .and_then(|summary| summary.lookup(property_name, true, true))
                    {
                        return self.finalize_property_access_result(
                            idx,
                            effective_write_result(member_info.type_id, Some(member_info.type_id)),
                            skip_flow_narrowing,
                            false,
                        );
                    }

                    // TS2576: instance.member where `member` exists on the class static side.
                    // Route this through the shared class summary so inherited
                    // static fields/accessors don't force another class walk.
                    if !self.is_super_expression(access.expression)
                        && let Some((_, is_static_access)) = resolved_class_access
                        && !is_static_access
                        && class_chain_summary
                            .as_ref()
                            .and_then(|summary| summary.lookup(property_name, true, true))
                            .is_some()
                    {
                        use crate::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };

                        let object_type_str =
                            self.format_type_for_assignability_message(display_object_type);
                        let static_member_name = format!("{object_type_str}.{property_name}");
                        let message = format_message(
                            diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN_TO_ACCESS_THE_STATIC_MEMBER_INSTEAD,
                            &[property_name, &object_type_str, &static_member_name],
                        );
                        // Report at the property name node, not the full expression (matches tsc behavior)
                        self.error_at_node(
                            access.name_or_argument,
                            &message,
                            diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN_TO_ACCESS_THE_STATIC_MEMBER_INSTEAD,
                        );
                        return TypeId::ERROR;
                    }

                    // Don't emit TS2339 for private fields (starting with #) - they're handled elsewhere.
                    // Also suppress when accessibility check already emitted TS2341/TS2445
                    // (property exists but is private/protected — not truly "not found").
                    // TSC also suppresses property-not-found errors for `super.member` access:
                    // when a property is not found on the super type, TypeScript does not report
                    // TS2339. For example, `super.x()` in a static method (where `x` is an
                    // instance method) and `super.y()` in an instance method (where `y` is a
                    // static method) produce no TS2339 errors in tsc (see superAccess2.ts).
                    // Also suppress TS2339 when base expression is a property access on an unresolved import
                    // (TS2307 was already emitted for the missing module).
                    // Suppress TS2339 when evaluating a computed property name
                    // inside a class that is currently being constructed. The
                    // property lookup may fail because the class instance type
                    // hasn't been fully registered yet (circular reference).
                    // tsc handles this gracefully and does not emit TS2339.
                    let in_circular_computed_property =
                        self.ctx.checking_computed_property_name.is_some()
                            && !self.ctx.class_instance_resolution_set.is_empty();
                    if !property_name.starts_with('#')
                        && !accessibility_error_emitted
                        && !self.is_super_expression(access.expression)
                        && !self.is_property_access_on_unresolved_import(access.expression)
                        && !in_circular_computed_property
                    {
                        if self.is_js_file()
                            && self.is_current_file_commonjs_export_base(access.expression)
                        {
                            let export_namespace_type =
                                self.current_file_commonjs_module_exports_namespace_type();
                            display_object_type = export_namespace_type;
                            if let PropertyAccessResult::Success {
                                type_id,
                                write_type,
                                ..
                            } = self.resolve_property_access_with_env(
                                export_namespace_type,
                                property_name,
                            ) {
                                return self.finalize_property_access_result(
                                    idx,
                                    effective_write_result(type_id, write_type),
                                    skip_flow_narrowing,
                                    false,
                                );
                            }
                        }
                        // Property access expressions are VALUE context - always emit TS2339.
                        // TS2694 (namespace has no exported member) is for TYPE context only,
                        // which is handled separately in type name resolution.
                        // Use display_object_type to preserve literal types in error messages
                        // while maintaining nominal identity (e.g., D<string>)
                        // Report at the property name node, not the full expression (matches tsc behavior)
                        if let Some(sym_id) = self.resolve_qualified_symbol(access.expression)
                            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                            && symbol.has_any_flags(tsz_binder::symbol_flags::ENUM)
                            && !symbol.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER)
                        {
                            self.error_property_not_exist_on_enum(
                                property_name,
                                &symbol.escaped_name.to_string(),
                                display_object_type,
                                access.name_or_argument,
                            );
                            return TypeId::ERROR;
                        }

                        if enum_instance_like_access {
                            let enum_display: Option<String> =
                                access_query::type_parameter_constraint(
                                    self.ctx.types,
                                    display_object_type,
                                )
                                .filter(|constraint| {
                                    access_query::enum_def_id(self.ctx.types, *constraint).is_some()
                                })
                                .map(|constraint| {
                                    self.format_type_for_assignability_message(constraint)
                                })
                                .or_else(|| {
                                    access_query::enum_def_id(self.ctx.types, display_object_type)
                                        .map(|_| {
                                            self.format_type_for_assignability_message(
                                                display_object_type,
                                            )
                                        })
                                });
                            if let Some(enum_display) = enum_display {
                                self.error_property_not_exist_with_apparent_type(
                                    property_name,
                                    &enum_display,
                                    access.name_or_argument,
                                );
                            } else {
                                // Suppress TS2339 for bare type parameters,
                                // for index access types (like T[keyof T]), or for
                                // unknown/error types that result from unresolved generics.
                                // Use is_type_parameter_like which correctly handles
                                // mapped types (their iteration variable is not "unresolved").
                                let should_suppress_inner =
                                    crate::query_boundaries::common::is_type_parameter_like(
                                        self.ctx.types,
                                        display_object_type,
                                    ) || crate::query_boundaries::common::is_index_access_type(
                                        self.ctx.types,
                                        display_object_type,
                                    ) || display_object_type == TypeId::UNKNOWN
                                        || display_object_type == TypeId::ERROR;
                                if !should_suppress_inner {
                                    self.error_property_not_exist_at(
                                        property_name,
                                        display_object_type,
                                        access.name_or_argument,
                                    );
                                }
                            }
                        } else {
                            // Suppress TS2339 for bare type parameters,
                            // for index access types (like T[keyof T]), or for
                            // unknown/error types that result from unresolved generics.
                            // Use is_type_parameter_like which correctly handles
                            // mapped types (their iteration variable is not "unresolved").
                            let should_suppress =
                                crate::query_boundaries::common::is_type_parameter_like(
                                    self.ctx.types,
                                    display_object_type,
                                ) || crate::query_boundaries::common::is_index_access_type(
                                    self.ctx.types,
                                    display_object_type,
                                ) || display_object_type == TypeId::UNKNOWN
                                    || display_object_type == TypeId::ERROR;
                            if !should_suppress {
                                self.error_property_not_exist_at(
                                    property_name,
                                    display_object_type,
                                    access.name_or_argument,
                                );
                            }
                        }
                    }
                    if receiver_has_daa_error {
                        return self.finalize_property_access_result(
                            idx,
                            TypeId::ERROR,
                            skip_flow_narrowing,
                            false,
                        );
                    }
                    TypeId::ERROR
                }

                PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type,
                    cause,
                } => self.handle_possibly_null_or_undefined_access(
                    idx,
                    access.expression,
                    access.name_or_argument,
                    access.question_dot_token,
                    property_type,
                    cause,
                    object_type_for_access,
                    property_name,
                    skip_flow_narrowing,
                    receiver_has_daa_error,
                ),

                PropertyAccessResult::IsUnknown => {
                    // TS18046: 'x' is of type 'unknown'.
                    // Without strictNullChecks, unknown is treated like any (no error).
                    if self.error_is_of_type_unknown(access.expression) {
                        TypeId::ERROR
                    } else {
                        TypeId::ANY
                    }
                }
            }
        } else {
            TypeId::ANY
        }
    }

    /// Handles import.meta property access.
    /// Returns Some(type) if this is an import.meta access, None otherwise.
    fn try_resolve_import_meta_access(
        &mut self,
        idx: NodeIndex,
        expression: NodeIndex,
        name_or_argument: NodeIndex,
    ) -> Option<TypeId> {
        let expr_node = self.ctx.arena.get(expression)?;
        if expr_node.kind != SyntaxKind::ImportKeyword as u16 {
            return None;
        }

        let is_meta = self
            .ctx
            .arena
            .get(name_or_argument)
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .is_some_and(|ident| ident.escaped_text == "meta");

        if is_meta {
            self.check_import_meta_in_cjs(idx);
            // import.meta resolves to the global `ImportMeta` interface
            // (declared in lib.es2020.full.d.ts). Returning that type
            // enables TS2339 on unknown properties (`import.meta.blah`)
            // and merges `declare global { interface ImportMeta { ... } }`
            // augmentations through lib-heritage merging.
            if let Some(import_meta_ty) = self.resolve_lib_type_by_name("ImportMeta") {
                return Some(import_meta_ty);
            }
        }
        // Fallback (ImportMeta not in lib scope, or non-`meta` meta-property
        // like `import.metal`): return ANY so downstream access doesn't
        // cascade misleading TS2339s. A separate grammar check is expected
        // to emit TS17012 for the invalid meta-property name.
        Some(TypeId::ANY)
    }

    /// Fast path for enum/namespace member value access (`E.Member` or `Ns.Member`).
    /// Returns Some(type) if this is an enum/namespace member access that can be resolved
    /// directly, None otherwise (fall through to general property-access pipeline).
    fn try_resolve_enum_namespace_member_access(
        &mut self,
        idx: NodeIndex,
        expression: NodeIndex,
        name_or_argument: NodeIndex,
        name_node: &tsz_parser::parser::node::Node,
        skip_flow_narrowing: bool,
    ) -> Option<TypeId> {
        let name_ident = self.ctx.arena.get_identifier(name_node)?;
        let property_name = &name_ident.escaped_text;

        let is_identifier_base = self
            .ctx
            .arena
            .get(expression)
            .is_some_and(|expr_node| expr_node.kind == SyntaxKind::Identifier as u16);

        if !is_identifier_base {
            return None;
        }

        let base_sym_id = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, expression)?;
        let base_symbol = self.ctx.binder.get_symbol(base_sym_id)?;

        // When the binder resolves an import to an intermediate alias (e.g.,
        // re-exported enums: `export { E } from './source'`), follow the
        // alias chain to find the actual enum/namespace symbol.
        //
        // For merged alias + namespace (`import { E } from './e'; namespace E { ... }`),
        // the base symbol carries both ALIAS and VALUE_MODULE flags. Prefer the
        // base symbol's own exports first, then fall back to the alias target's
        // exports so that enum members from the aliased source remain reachable.
        let (resolved_sym_id, resolved_flags) = if base_symbol.has_any_flags(symbol_flags::ALIAS)
            && !base_symbol.has_any_flags(symbol_flags::ENUM | symbol_flags::VALUE_MODULE)
        {
            let mut visited = crate::symbols_domain::alias_cycle::AliasCycleTracker::new();
            if let Some(target_id) = self.resolve_alias_symbol(base_sym_id, &mut visited) {
                let target_flags = self
                    .get_cross_file_symbol(target_id)
                    .or_else(|| self.ctx.binder.get_symbol(target_id))
                    .map_or(0, |s| s.flags);
                (target_id, target_flags)
            } else {
                (base_sym_id, base_symbol.flags)
            }
        } else {
            (base_sym_id, base_symbol.flags)
        };

        if resolved_flags & (symbol_flags::ENUM | symbol_flags::VALUE_MODULE) == 0 {
            return None;
        }

        // Extract data from resolved symbol before taking mutable borrows below.
        // If the member is missing from the resolved symbol's own exports and the
        // original base symbol is a merged alias + namespace, follow the alias to
        // consult the aliased target's exports (const-enum members accessible via
        // a re-exported + locally-merged namespace).
        let base_has_alias = base_symbol.has_any_flags(symbol_flags::ALIAS);
        let (member_sym_id, resolved_value_decl, resolved_first_decl, resolved_is_ambient) = {
            let resolved_symbol = self
                .get_cross_file_symbol(resolved_sym_id)
                .or_else(|| self.ctx.binder.get_symbol(resolved_sym_id))?;
            let own_member = resolved_symbol
                .exports
                .as_ref()
                .and_then(|e| e.get(property_name));
            let value_decl = resolved_symbol.value_declaration;
            let first_decl = resolved_symbol.declarations.first().copied();
            let is_ambient = self.is_const_enum_ambient(resolved_symbol);
            (own_member, value_decl, first_decl, is_ambient)
        };
        let (member_sym_id, resolved_flags, resolved_is_ambient) = if let Some(id) = member_sym_id {
            (id, resolved_flags, resolved_is_ambient)
        } else if base_has_alias && resolved_sym_id == base_sym_id {
            // Merged alias + namespace: the namespace's own exports don't have
            // this member. Follow the alias to the aliased target.
            let mut visited = crate::symbols_domain::alias_cycle::AliasCycleTracker::new();
            let alias_target = self.resolve_alias_symbol(base_sym_id, &mut visited)?;
            let (alias_member, alias_flags, alias_is_ambient) = {
                let alias_sym = self
                    .get_cross_file_symbol(alias_target)
                    .or_else(|| self.ctx.binder.get_symbol(alias_target))?;
                let id = alias_sym.exports.as_ref()?.get(property_name)?;
                (id, alias_sym.flags, self.is_const_enum_ambient(alias_sym))
            };
            if alias_flags & (symbol_flags::ENUM | symbol_flags::VALUE_MODULE) == 0 {
                return None;
            }
            (alias_member, alias_flags, alias_is_ambient)
        } else {
            return None;
        };

        // For namespace members, only use the fast path when the export has
        // value semantics (VARIABLE, CLASS, FUNCTION, etc.) or is an alias
        // (export import). Type-only exports (interfaces, type aliases) must go
        // through the general property-access path so that TS2708/TS2693
        // diagnostics are properly emitted.
        let member_has_value_semantics = self
            .ctx
            .binder
            .get_symbol(member_sym_id)
            .is_some_and(|s| s.flags & (symbol_flags::VALUE | symbol_flags::ALIAS) != 0);
        if !member_has_value_semantics {
            return None;
        }

        // For merged symbols (e.g., namespace + interface), verify that the VALUE
        // part is actually exported. If only the TYPE part is exported, the value
        // is not accessible and we should fall through to emit TS2339.
        if !self.symbol_has_exported_value_declaration(member_sym_id) {
            return None;
        }

        let is_enum = resolved_flags & symbol_flags::ENUM != 0;

        // TS1361/TS1362: Check if the base identifier is a type-only import.
        if let Some(local_sym_id) = self.resolve_identifier_symbol(expression)
            && self.alias_resolves_to_type_only(local_sym_id)
            && let Some(base_node) = self.ctx.arena.get(expression)
            && let Some(base_ident) = self.ctx.arena.get_identifier(base_node)
            && !self
                .source_file_has_value_import_binding_named(expression, &base_ident.escaped_text)
        {
            self.report_wrong_meaning_diagnostic(
                &base_ident.escaped_text,
                expression,
                crate::query_boundaries::name_resolution::NameLookupKind::Type,
            );
            return Some(TypeId::ERROR);
        }

        if is_enum {
            // TS2450: Check if enum is used before its declaration (TDZ violation).
            if let Some(base_node) = self.ctx.arena.get(expression)
                && let Some(base_ident) = self.ctx.arena.get_identifier(base_node)
            {
                let base_name = &base_ident.escaped_text;
                if self.check_tdz_violation(base_sym_id, expression, base_name, true) {
                    return Some(TypeId::ERROR);
                }
            }

            // TS2748: Cannot access ambient const enums when isolatedModules is enabled.
            if self.ctx.isolated_modules()
                && resolved_flags & symbol_flags::CONST_ENUM != 0
                && resolved_is_ambient
                && !self.is_in_type_only_position(idx)
            {
                let option_name = if self.ctx.compiler_options.verbatim_module_syntax {
                    "verbatimModuleSyntax"
                } else {
                    "isolatedModules"
                };
                let msg = crate::diagnostics::format_message(
                    crate::diagnostics::diagnostic_messages::CANNOT_ACCESS_AMBIENT_CONST_ENUMS_WHEN_IS_ENABLED,
                    &[option_name],
                );
                self.error_at_node(
                    idx,
                    &msg,
                    crate::diagnostics::diagnostic_codes::CANNOT_ACCESS_AMBIENT_CONST_ENUMS_WHEN_IS_ENABLED,
                );
            }
        }

        // TS2729 for namespace member access in static property initializers.
        // Methods are hoisted and don't need initialization, so skip them.
        let member_is_method = self
            .get_cross_file_symbol(member_sym_id)
            .or_else(|| self.ctx.binder.get_symbol(member_sym_id))
            .is_some_and(|s| s.has_any_flags(symbol_flags::METHOD));
        if resolved_flags & symbol_flags::VALUE_MODULE != 0
            && !member_is_method
            && self.is_in_static_property_initializer_ast_context(expression)
            && self.find_enclosing_computed_property(expression).is_none()
        {
            let decl_idx = if resolved_value_decl.is_some() {
                resolved_value_decl
            } else if let Some(first_decl) = resolved_first_decl {
                first_decl
            } else {
                NodeIndex::NONE
            };
            if decl_idx.is_some()
                && let Some(usage_node) = self.ctx.arena.get(expression)
                && let Some(decl_node) = self.ctx.arena.get(decl_idx)
                && usage_node.pos < decl_node.pos
            {
                self.error_at_node(
                    name_or_argument,
                    &format!(
                        "Property '{}' is used before its initialization.",
                        name_ident.escaped_text
                    ),
                    tsz_common::diagnostics::diagnostic_codes::PROPERTY_IS_USED_BEFORE_ITS_INITIALIZATION,
                );
            }
        }

        // Resolve the member type.
        let member_sym = self
            .get_cross_file_symbol(member_sym_id)
            .or_else(|| self.ctx.binder.get_symbol(member_sym_id));
        let member_type = if let Some(member_sym) = member_sym
            && member_sym.has_any_flags(symbol_flags::INTERFACE)
            && member_sym.has_any_flags(symbol_flags::VARIABLE)
            && member_sym.value_declaration.is_some()
        {
            self.type_of_value_declaration_for_symbol(member_sym_id, member_sym.value_declaration)
        } else {
            self.get_type_of_symbol(member_sym_id)
        };

        Some(self.finalize_property_access_result(idx, member_type, skip_flow_narrowing, false))
    }

    /// Handles the `PossiblyNullOrUndefined` result from property access resolution.
    /// Emits appropriate diagnostics (TS18047/18048/18049/18050) and returns the resolved type.
    #[allow(clippy::too_many_arguments)]
    fn handle_possibly_null_or_undefined_access(
        &mut self,
        idx: NodeIndex,
        expression: NodeIndex,
        name_or_argument: NodeIndex,
        question_dot_token: bool,
        property_type: Option<TypeId>,
        cause: TypeId,
        object_type_for_access: TypeId,
        property_name: &str,
        skip_flow_narrowing: bool,
        receiver_has_daa_error: bool,
    ) -> TypeId {
        use crate::query_boundaries::common::PropertyAccessResult;

        let factory = self.ctx.types.factory();

        if receiver_has_daa_error {
            return self.finalize_property_access_result(
                idx,
                property_type.unwrap_or(TypeId::ERROR),
                skip_flow_narrowing,
                false,
            );
        }

        // Check for optional chaining (?.)
        if question_dot_token {
            if self
                .ctx
                .compiler_options
                .no_property_access_from_index_signature
                && let (Some(non_nullish_base), _) = self.split_nullish_type(object_type_for_access)
                && let PropertyAccessResult::Success {
                    from_index_signature,
                    ..
                } = self.resolve_property_access_with_env(non_nullish_base, property_name)
                && from_index_signature
                && !self.union_has_explicit_property_member(non_nullish_base, property_name)
            {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node(
                    name_or_argument,
                    &format!(
                        "Property '{property_name}' comes from an index signature, so it must be accessed with ['{property_name}']."
                    ),
                    diagnostic_codes::PROPERTY_COMES_FROM_AN_INDEX_SIGNATURE_SO_IT_MUST_BE_ACCESSED_WITH,
                );
            }
            // Suppress error, return (property_type | undefined)
            let base_type = property_type.unwrap_or(TypeId::UNKNOWN);
            return factory.union2(base_type, TypeId::UNDEFINED);
        }

        // Report error based on the cause (TS2531/TS2532/TS2533 or TS18050)
        use crate::diagnostics::diagnostic_codes;

        // Suppress cascade errors when cause is ERROR/ANY/UNKNOWN
        if cause == TypeId::ERROR || cause == TypeId::ANY || cause == TypeId::UNKNOWN {
            return property_type.unwrap_or(TypeId::ERROR);
        }

        // Check if the type is entirely nullish (no non-nullish part in union)
        let is_type_nullish =
            object_type_for_access == TypeId::NULL || object_type_for_access == TypeId::UNDEFINED;

        // For possibly-nullish values in non-strict mode, don't error
        if !self.ctx.compiler_options.strict_null_checks && !is_type_nullish {
            return self.finalize_property_access_result(
                idx,
                property_type.unwrap_or(TypeId::ERROR),
                skip_flow_narrowing,
                false,
            );
        }

        // Check if the expression is a literal null/undefined keyword
        let is_literal_nullish = if let Some(expr_node) = self.ctx.arena.get(expression) {
            expr_node.kind == SyntaxKind::NullKeyword as u16
                || (expr_node.kind == SyntaxKind::Identifier as u16
                    && self
                        .ctx
                        .arena
                        .get_identifier(expr_node)
                        .is_some_and(|ident| ident.escaped_text == "undefined"))
        } else {
            false
        };

        // When the expression IS a literal null/undefined keyword, emit TS18050
        if is_literal_nullish {
            let value_name = if cause == TypeId::NULL {
                "null"
            } else if cause == TypeId::UNDEFINED {
                "undefined"
            } else {
                "null | undefined"
            };
            self.error_at_node_msg(
                expression,
                diagnostic_codes::THE_VALUE_CANNOT_BE_USED_HERE,
                &[value_name],
            );
            return self.finalize_property_access_result(
                idx,
                property_type.unwrap_or(TypeId::ERROR),
                skip_flow_narrowing,
                false,
            );
        }

        // Without strictNullChecks, TS18047/TS18048/TS18049 are never emitted
        if !self.ctx.compiler_options.strict_null_checks {
            return self.finalize_property_access_result(
                idx,
                property_type.unwrap_or(TypeId::ERROR),
                skip_flow_narrowing,
                false,
            );
        }

        // When TS2454 has already been emitted, suppress TS18047/18048/18049
        if self.ctx.daa_error_nodes.contains(&expression.0)
            || self.ctx.daa_error_nodes.contains(&idx.0)
        {
            return self.finalize_property_access_result(
                idx,
                property_type.unwrap_or(TypeId::ERROR),
                skip_flow_narrowing,
                false,
            );
        }

        // Named "'this' is possibly 'undefined'." (TS18048) only when `this`
        // is explicitly typed; implicit `this: undefined` uses TS2532.
        let name = self.expression_text(expression).or_else(|| {
            (self.is_this_expression(expression)
                && (self.enclosing_function_has_explicit_this_parameter(expression)
                    || self.enclosing_function_has_contextual_this_type(expression)))
            .then(|| "this".to_string())
        });

        let (code, message): (u32, String) = if let Some(ref name) = name {
            if cause == TypeId::NULL {
                (
                    diagnostic_codes::IS_POSSIBLY_NULL,
                    format!("'{name}' is possibly 'null'."),
                )
            } else if cause == TypeId::UNDEFINED {
                (
                    diagnostic_codes::IS_POSSIBLY_UNDEFINED,
                    format!("'{name}' is possibly 'undefined'."),
                )
            } else {
                (
                    diagnostic_codes::IS_POSSIBLY_NULL_OR_UNDEFINED,
                    format!("'{name}' is possibly 'null' or 'undefined'."),
                )
            }
        } else if cause == TypeId::NULL {
            (
                diagnostic_codes::OBJECT_IS_POSSIBLY_NULL,
                "Object is possibly 'null'.".to_string(),
            )
        } else if cause == TypeId::UNDEFINED {
            (
                diagnostic_codes::OBJECT_IS_POSSIBLY_UNDEFINED,
                "Object is possibly 'undefined'.".to_string(),
            )
        } else {
            (
                diagnostic_codes::OBJECT_IS_POSSIBLY_NULL_OR_UNDEFINED,
                "Object is possibly 'null' or 'undefined'.".to_string(),
            )
        };

        self.error_at_node(expression, &message, code);

        self.finalize_property_access_result(
            idx,
            property_type.unwrap_or(TypeId::ERROR),
            skip_flow_narrowing,
            false,
        )
    }

    /// Handles property access on globalThis or Window-like expressions.
    /// Returns Some(type) if this is a globalThis/Window access, None otherwise.
    fn try_resolve_global_this_property_access(
        &mut self,
        idx: NodeIndex,
        expression: NodeIndex,
        name_or_argument: NodeIndex,
        property_name: &str,
        skip_flow_narrowing: bool,
    ) -> Option<TypeId> {
        let is_this_global = self.is_this_resolving_to_global(expression);
        if !self.is_global_this_like_expression(expression) && !is_this_global {
            return None;
        }

        let base_display = if self.is_global_this_expression(expression) || is_this_global {
            "typeof globalThis"
        } else {
            "Window & typeof globalThis"
        };
        let allow_unknown_property_fallback =
            self.is_global_this_expression(expression) || is_this_global;
        let property_type = self.resolve_global_this_property_type(
            property_name,
            name_or_argument,
            allow_unknown_property_fallback,
            base_display,
        );

        if property_type == TypeId::ERROR {
            return Some(TypeId::ERROR);
        }

        // TS7017: When noImplicitAny is enabled and `this` resolves to typeof globalThis
        // and the property is not found, emit the index signature error.
        if is_this_global
            && property_type == TypeId::ANY
            && self.ctx.no_implicit_any()
            && !self.is_js_file()
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            self.error_at_node(
                name_or_argument,
                &format_message(
                    diagnostic_messages::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_TYPE_HAS_NO_INDEX_SIGNATURE,
                    &["typeof globalThis"],
                ),
                diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_TYPE_HAS_NO_INDEX_SIGNATURE,
            );
        }

        Some(self.finalize_property_access_result(idx, property_type, skip_flow_narrowing, false))
    }
}
