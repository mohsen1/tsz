//! Property access type resolution, global augmentation property lookup,
//! and expando function pattern detection.

use crate::query_boundaries::property_access as access_query;
use crate::state::{CheckerState, MAX_INSTANTIATION_DEPTH};
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Get type of property access expression.
    pub(crate) fn get_type_of_property_access(&mut self, idx: NodeIndex) -> TypeId {
        if self.ctx.instantiation_depth.get() >= MAX_INSTANTIATION_DEPTH {
            *self.ctx.depth_exceeded.borrow_mut() = true;
            return TypeId::ERROR; // Max instantiation depth exceeded - propagate error
        }

        self.ctx
            .instantiation_depth
            .set(self.ctx.instantiation_depth.get() + 1);
        let result = self.get_type_of_property_access_inner(idx);
        self.ctx
            .instantiation_depth
            .set(self.ctx.instantiation_depth.get() - 1);
        result
    }

    fn missing_typescript_lib_dom_global_alias(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        let ident = self.ctx.arena.get_identifier(node)?;
        let name = ident.escaped_text.as_str();
        if !matches!(name, "window" | "self") {
            return None;
        }
        if !self.ctx.typescript_dom_replacement_loaded {
            return None;
        }
        match name {
            "window" if !self.ctx.typescript_dom_replacement_has_window => Some(name.to_string()),
            "self" if !self.ctx.typescript_dom_replacement_has_self => Some(name.to_string()),
            _ => None,
        }
    }

    /// Inner implementation of property access type resolution.
    fn get_type_of_property_access_inner(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_solver::operations::property::PropertyAccessResult;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return TypeId::ERROR; // Missing access expression data - propagate error
        };

        // Handle import.meta: emit TS1470 in files that compile to CommonJS output
        if let Some(expr_node) = self.ctx.arena.get(access.expression)
            && expr_node.kind == SyntaxKind::ImportKeyword as u16
        {
            if let Some(name_n) = self.ctx.arena.get(access.name_or_argument)
                && let Some(ident) = self.ctx.arena.get_identifier(name_n)
                && ident.escaped_text == "meta"
            {
                self.check_import_meta_in_cjs(idx);
            }
            // import.meta resolves to the global ImportMeta interface;
            // return any as a safe fallback until we resolve that global.
            return TypeId::ANY;
        }

        let factory = self.ctx.types.factory();

        // Get the property name first (needed for abstract property check regardless of object type)
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            // Preserve diagnostics on the base expression (e.g. TS2304 for `missing.`)
            // even when parser recovery could not build a property name node.
            let _ = self.get_type_of_node(access.expression);
            return TypeId::ERROR;
        };
        let property_name_for_probe = self
            .ctx
            .arena
            .get_identifier(name_node)
            .map(|ident| ident.escaped_text.clone());
        if let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && ident.escaped_text.is_empty()
        {
            // Preserve diagnostics on the base expression when member name is missing.
            let _ = self.get_type_of_node(access.expression);
            return TypeId::ERROR;
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

        // Fast path for enum member value access (`E.Member`).
        // This avoids the general property-access pipeline (accessibility checks,
        // type environment classification, etc.) for a very common hot path.
        if let Some(name_ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &name_ident.escaped_text;
            let is_identifier_base = self
                .ctx
                .arena
                .get(access.expression)
                .is_some_and(|expr_node| expr_node.kind == SyntaxKind::Identifier as u16);
            if is_identifier_base
                && let Some(base_sym_id) = self
                    .ctx
                    .binder
                    .resolve_identifier(self.ctx.arena, access.expression)
                && let Some(base_symbol) = self.ctx.binder.get_symbol(base_sym_id)
                && base_symbol.flags & symbol_flags::ENUM != 0
                && let Some(exports) = base_symbol.exports.as_ref()
                && let Some(member_sym_id) = exports.get(property_name)
            {
                // TS1361/TS1362: Check if the base identifier is a type-only import.
                // resolve_identifier follows aliases, so base_sym_id is the target enum,
                // not the local import binding. Check the local symbol in file_locals.
                if let Some(local_sym_id) = self.resolve_identifier_symbol(access.expression)
                    && self.alias_resolves_to_type_only(local_sym_id)
                {
                    if let Some(base_node) = self.ctx.arena.get(access.expression)
                        && let Some(base_ident) = self.ctx.arena.get_identifier(base_node)
                    {
                        self.error_type_only_value_at(&base_ident.escaped_text, access.expression);
                    }
                    return TypeId::ERROR;
                }
                // TS2450: Check if enum is used before its declaration (TDZ violation).
                // Only non-const enums are flagged (const enums are always hoisted).
                if let Some(base_node) = self.ctx.arena.get(access.expression)
                    && let Some(base_ident) = self.ctx.arena.get_identifier(base_node)
                {
                    let base_name = &base_ident.escaped_text;
                    if self.check_tdz_violation(base_sym_id, access.expression, base_name, true) {
                        return TypeId::ERROR;
                    }
                }

                // Enum members and namespace exports both resolve to the selected member symbol type.
                // Namespace exports may represent functions, variables, etc., each with its own symbol type.
                let member_type = self.get_type_of_symbol(member_sym_id);
                return self.apply_flow_narrowing(idx, member_type);
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
        let skip_result_flow_for_result = !self.ctx.skip_flow_narrowing
            && self.should_skip_property_result_flow_narrowing_for_result(idx);
        let skip_result_flow = !self.ctx.skip_flow_narrowing
            && (skip_result_flow_for_result
                || self.should_skip_property_result_flow_narrowing(idx));
        let skip_optional_base_flow = access.question_dot_token && skip_result_flow_for_result;
        let prev_skip = self.ctx.skip_flow_narrowing;

        let original_object_type = if skip_optional_base_flow {
            self.ctx.skip_flow_narrowing = true;
            let object_type_no_flow = self.get_type_of_node(access.expression);
            self.ctx.skip_flow_narrowing = prev_skip;
            object_type_no_flow
        } else if skip_result_flow {
            self.ctx.skip_flow_narrowing = true;
            let object_type_no_flow = self.get_type_of_node(access.expression);
            self.ctx.skip_flow_narrowing = prev_skip;

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
                object_type_no_flow
            } else {
                self.ctx.skip_flow_narrowing = false;
                let object_type_with_flow = self.get_type_of_node(access.expression);
                self.ctx.skip_flow_narrowing = prev_skip;
                object_type_with_flow
            }
        } else {
            self.ctx.skip_flow_narrowing = false;
            let object_type_with_flow = self.get_type_of_node(access.expression);
            self.ctx.skip_flow_narrowing = prev_skip;
            object_type_with_flow
        };

        // Compute a display type for error messages that preserves literal types.
        // When `get_type_of_node` widens literals (e.g., "" -> string, 42 -> number),
        // tsc still shows the literal type in error messages like TS2339.
        // Try to recover the literal type from the expression node for display purposes.
        let mut display_object_type = if matches!(
            original_object_type,
            TypeId::STRING | TypeId::NUMBER | TypeId::BOOLEAN | TypeId::BIGINT
        ) {
            self.literal_type_from_initializer(access.expression)
                .unwrap_or(original_object_type)
        } else {
            self.enum_member_initializer_display_type(access.expression)
                .unwrap_or(original_object_type)
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

        // Handle optional chain continuations: for `o?.b.c`, when processing `.c`,
        // the object type from `o?.b` includes `undefined` from the optional chain.
        // But `.c` should only be reached when `o` is defined, so we strip nullish
        // types. Only do this when this access is NOT itself an optional chain
        // (`question_dot_token` is false) but is part of one (parent has `?.`).
        object_type = if !access.question_dot_token
            && super::computation::access::is_optional_chain(self.ctx.arena, access.expression)
        {
            let (non_nullish, _) = self.split_nullish_type(object_type);
            non_nullish.unwrap_or(object_type)
        } else {
            object_type
        };

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
            display_object_type = namespace_type;
        }

        // Fast path for optional chaining on non-class receivers when the
        // property resolves successfully without diagnostics.
        //
        // This avoids the full property-access diagnostic pipeline for common
        // patterns like `opts?.timeout` / `opts?.retries` in hot call sites.
        if access.question_dot_token
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && !self.is_super_expression(access.expression)
        {
            let property_name = &ident.escaped_text;
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
                let can_cache_fast = !self.contains_type_parameters_cached(resolved_base);
                let prop_atom = can_cache_fast.then(|| self.ctx.types.intern_string(property_name));

                if can_cache_fast
                    && let Some(prop_atom) = prop_atom
                    && let Some(cached) = self
                        .ctx
                        .narrowing_cache
                        .property_cache
                        .borrow()
                        .get(&(resolved_base, prop_atom))
                        .copied()
                {
                    match cached {
                        Some(type_id) => {
                            let mut result_type = type_id;
                            if base_nullish.is_some()
                                && !tsz_solver::type_contains_undefined(self.ctx.types, result_type)
                            {
                                result_type = factory.union(vec![result_type, TypeId::UNDEFINED]);
                            }
                            return if !self.ctx.skip_flow_narrowing && skip_result_flow_for_result {
                                result_type
                            } else {
                                self.apply_flow_narrowing(idx, result_type)
                            };
                        }
                        None => {
                            // Fall through to full diagnostic path so TS2339 and related
                            // diagnostics are still emitted at this access site.
                        }
                    }
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
                        ..
                    } => {
                        if can_cache_fast {
                            self.ctx.narrowing_cache.property_cache.borrow_mut().insert(
                                (resolved_base, prop_atom.expect("cached atom")),
                                Some(type_id),
                            );
                        }
                        let mut result_type = if self.ctx.skip_flow_narrowing {
                            write_type.unwrap_or(type_id)
                        } else {
                            type_id
                        };
                        if base_nullish.is_some()
                            && !tsz_solver::type_contains_undefined(self.ctx.types, result_type)
                        {
                            result_type = factory.union(vec![result_type, TypeId::UNDEFINED]);
                        }
                        return if !self.ctx.skip_flow_narrowing && skip_result_flow_for_result {
                            result_type
                        } else {
                            self.apply_flow_narrowing(idx, result_type)
                        };
                    }
                    PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                        if can_cache_fast {
                            self.ctx.narrowing_cache.property_cache.borrow_mut().insert(
                                (resolved_base, prop_atom.expect("cached atom")),
                                property_type,
                            );
                        }
                        let mut result_type = property_type.unwrap_or(TypeId::ERROR);
                        if base_nullish.is_some()
                            && !tsz_solver::type_contains_undefined(self.ctx.types, result_type)
                        {
                            result_type = factory.union(vec![result_type, TypeId::UNDEFINED]);
                        }
                        return self.apply_flow_narrowing(idx, result_type);
                    }
                    PropertyAccessResult::PropertyNotFound { .. } => {
                        if can_cache_fast {
                            self.ctx
                                .narrowing_cache
                                .property_cache
                                .borrow_mut()
                                .insert((resolved_base, prop_atom.expect("cached atom")), None);
                        }
                        // Fall through to full diagnostic path.
                    }
                    PropertyAccessResult::IsUnknown => {
                        // Fall through to full diagnostic path.
                    }
                }
            }
        }

        if name_node.kind == SyntaxKind::PrivateIdentifier as u16 {
            return self.get_type_of_private_property_access(
                idx,
                access,
                access.name_or_argument,
                object_type,
            );
        }

        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;
            if self.is_global_this_expression(access.expression) {
                let property_type =
                    self.resolve_global_this_property_type(property_name, access.name_or_argument);
                if property_type == TypeId::ERROR {
                    return TypeId::ERROR;
                }
                return self.apply_flow_narrowing(idx, property_type);
            }
        }

        if object_type == TypeId::ANY
            && self.is_js_file()
            && self
                .ctx
                .arena
                .get_identifier_at(access.expression)
                .is_some_and(|ident| ident.escaped_text == "module")
            && self
                .resolve_identifier_symbol_without_tracking(access.expression)
                .is_none()
            && self
                .ctx
                .arena
                .get_identifier(name_node)
                .is_some_and(|ident| ident.escaped_text == "exports")
        {
            return self.current_file_commonjs_module_exports_namespace_type();
        }

        // Don't report errors for any/error types - check BEFORE accessibility
        // to prevent cascading errors when the object type is already invalid
        if object_type == TypeId::ANY {
            return TypeId::ANY;
        }
        if object_type == TypeId::ERROR {
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }

        // Property access on `never` returns `never` (bottom type propagation).
        // In TypeScript, this is an error: Property 'X' does not exist on type 'never'.
        if object_type == TypeId::NEVER {
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
            return TypeId::NEVER;
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
            if let Some(expr_node) = self.ctx.arena.get(access.expression)
                && let Some(expr_ident) = self.ctx.arena.get_identifier(expr_node)
            {
                let expr_name = &expr_ident.escaped_text;
                // Try file_locals first (fast path for top-level symbols)
                if let Some(sym_id) = self.ctx.binder.file_locals.get(expr_name)
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                {
                    // Check if this is a merged symbol (has both MODULE and value constructor flags)
                    let is_merged = (symbol.flags & symbol_flags::MODULE) != 0
                        && (symbol.flags
                            & (symbol_flags::CLASS
                                | symbol_flags::FUNCTION
                                | symbol_flags::REGULAR_ENUM))
                            != 0;

                    if is_merged
                        && let Some(exports) = symbol.exports.as_ref()
                        && let Some(member_id) = exports.get(property_name)
                    {
                        // For merged symbols, we return the type for any exported member
                        let member_type = self.get_type_of_symbol(member_id);
                        return self.apply_flow_narrowing(idx, member_type);
                    }
                }
            }
        }

        // If it's an identifier, look up the property
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;

            if self.is_type_only_import_equals_namespace_expr(access.expression) {
                let has_scoped_value_or_alias = self
                    .entity_name_text(access.expression)
                    .map(|entity_name| {
                        let lib_binders = self.get_lib_binders();
                        self.ctx
                            .binder
                            .resolve_identifier_with_filter(
                                self.ctx.arena,
                                access.expression,
                                &lib_binders,
                                |sid| {
                                    self.ctx
                                        .binder
                                        .get_symbol_with_libs(sid, &lib_binders)
                                        .is_some_and(|s| {
                                            (s.flags & symbol_flags::VALUE) != 0
                                                || ((s.flags & symbol_flags::ALIAS) != 0
                                                    && !s.is_type_only
                                                    && s.escaped_name == entity_name)
                                        })
                                },
                            )
                            .is_some()
                    })
                    .unwrap_or(false);
                if has_scoped_value_or_alias {
                    // A value-capable alias is visible in scope (e.g. duplicate
                    // `import M = ...` where one target is value-bearing). Defer to
                    // regular member resolution instead of forcing TS2693/TS2708.
                } else {
                    if let Some(ns_name) = self.entity_name_text(access.expression) {
                        self.error_namespace_used_as_value_at(&ns_name, access.expression);
                        if let Some(sym_id) = self.resolve_identifier_symbol(access.expression)
                            && self.alias_resolves_to_type_only(sym_id)
                        {
                            self.error_type_only_value_at(&ns_name, access.expression);
                        }
                    }
                    return TypeId::ERROR;
                }
            }

            let enum_instance_like_access = self
                .is_enum_instance_property_access(object_type, access.expression)
                || access_query::type_parameter_constraint(self.ctx.types, object_type)
                    .is_some_and(|constraint| {
                        access_query::enum_def_id(self.ctx.types, constraint).is_some()
                    });

            if !enum_instance_like_access
                && let Some(member_type) =
                    self.resolve_namespace_value_member(object_type, property_name)
            {
                return self.apply_flow_narrowing(idx, member_type);
            }

            // Fallback for namespace/export member accesses where type-only namespace
            // classification misses the object form but symbol resolution can still
            // identify `A.B` as a concrete exported value member.
            if let Some(member_sym_id) = self.resolve_qualified_symbol(idx)
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
                    && (member_symbol.flags & symbol_flags::VALUE) != 0
                    && !transitively_type_only
                {
                    let parent_sym_id = member_symbol.parent;
                    if let Some(parent_symbol) = self
                        .get_cross_file_symbol(parent_sym_id)
                        .or_else(|| self.ctx.binder.get_symbol(parent_sym_id))
                        && (parent_symbol.flags & (symbol_flags::MODULE | symbol_flags::ENUM)) != 0
                    {
                        let member_type = self.get_type_of_symbol(member_sym_id);
                        if member_type != TypeId::ERROR && member_type != TypeId::UNKNOWN {
                            return self.apply_flow_narrowing(idx, member_type);
                        }
                    }
                }
            }

            if self.namespace_has_type_only_member(object_type, property_name) {
                if self.is_unresolved_import_symbol(access.expression) {
                    return TypeId::ERROR;
                }
                // Don't emit TS2693 in heritage clause context — the heritage
                // checker will emit the appropriate error (e.g., TS2689).
                if self
                    .find_enclosing_heritage_clause(access.name_or_argument)
                    .is_none()
                {
                    // Emit TS2708 for namespace member access (e.g., ns.Interface())
                    // This is "Cannot use namespace as a value"
                    // Get the namespace name from the left side of the access
                    if let Some(ns_name) = self.entity_name_text(access.expression) {
                        self.error_namespace_used_as_value_at(&ns_name, access.expression);
                    }
                    // Also emit TS2693 for the type-only member itself
                    self.error_type_only_value_at(property_name, access.name_or_argument);
                }
                return TypeId::ERROR;
            }
            if self.is_namespace_value_type(object_type) && !enum_instance_like_access {
                if !access.question_dot_token
                    && !property_name.starts_with('#')
                    && !accessibility_error_emitted
                {
                    // Report at the property name node, not the full expression (matches tsc behavior)
                    // Use display_object_type to preserve literal types in error messages
                    self.error_property_not_exist_at(
                        property_name,
                        display_object_type,
                        access.name_or_argument,
                    );
                }
                return TypeId::ERROR;
            }

            let object_type_for_access = if enum_instance_like_access {
                self.apparent_enum_instance_type(object_type)
                    .unwrap_or_else(|| self.resolve_type_for_property_access(object_type))
            } else {
                self.resolve_type_for_property_access(object_type)
            };
            if object_type_for_access == TypeId::ANY {
                return TypeId::ANY;
            }
            if object_type_for_access == TypeId::ERROR {
                return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
            }

            if self.ctx.strict_bind_call_apply()
                && let Some(strict_method_type) = self.strict_bind_call_apply_method_type(
                    object_type_for_access,
                    access.expression,
                    property_name,
                )
            {
                return self.apply_flow_narrowing(idx, strict_method_type);
            }

            // Use the environment-aware resolver so that array methods, boxed
            // primitive types, and other lib-registered types are available.
            let result =
                self.resolve_property_access_with_env(object_type_for_access, property_name);

            match result {
                PropertyAccessResult::Success {
                    type_id: prop_type,
                    write_type,
                    from_index_signature,
                } => {
                    if self.ctx.skip_flow_narrowing
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
                    // When in a write context (assignment target), use the setter
                    // type if the property has divergent getter/setter types.
                    let effective_type = if self.ctx.skip_flow_narrowing {
                        write_type.unwrap_or(prop_type)
                    } else {
                        prop_type
                    };
                    if !self.ctx.skip_flow_narrowing && skip_result_flow_for_result {
                        effective_type
                    } else {
                        self.apply_flow_narrowing(idx, effective_type)
                    }
                }

                PropertyAccessResult::PropertyNotFound { .. } => {
                    if let Some(augmented_type) = self.resolve_array_global_augmentation_property(
                        object_type_for_access,
                        property_name,
                    ) {
                        return self.apply_flow_narrowing(idx, augmented_type);
                    }
                    // Check global interface augmentations for primitive wrappers
                    // and other built-in types (e.g., `interface Boolean { doStuff() }`)
                    if let Some(augmented_type) = self.resolve_general_global_augmentation_property(
                        object_type_for_access,
                        property_name,
                    ) {
                        return self.apply_flow_narrowing(idx, augmented_type);
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
                        return self.apply_flow_narrowing(idx, type_id);
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
                        && self
                            .resolve_identifier_symbol_without_tracking(access.expression)
                            .is_none()
                    {
                        return self.current_file_commonjs_module_exports_namespace_type();
                    }
                    // Check for expando property reads: X.prop where X.prop = value was assigned
                    // Returns `any` type for properties that were assigned via expando pattern.
                    if self.is_expando_property_read(access.expression, property_name) {
                        return TypeId::ANY;
                    }
                    // Check for expando function pattern: func.prop = value
                    // TypeScript allows property assignments to function/class declarations
                    // without emitting TS2339. The assigned properties become part of the
                    // function's type (expando pattern).
                    if self.is_expando_function_assignment(
                        idx,
                        access.expression,
                        object_type_for_access,
                    ) {
                        return TypeId::ANY;
                    }

                    // JavaScript files allow dynamic property assignment on 'this' without errors.
                    // In JS files, accessing a property on 'this' that doesn't exist should not error
                    // and should return 'any' type, matching TypeScript's behavior.
                    let is_this_access =
                        if let Some(obj_node) = self.ctx.arena.get(access.expression) {
                            obj_node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16
                        } else {
                            false
                        };

                    if self.is_js_file() && is_this_access {
                        // Allow dynamic property on `this` in loose JS contexts, but
                        // keep checks when `this` is contextually owned by a class/object
                        // member (checkJs should still enforce member-consistent typing).
                        if self.this_has_contextual_owner(access.expression).is_none() {
                            return TypeId::ANY;
                        }
                    }

                    // TS2576: super.member where `member` exists on the base class static side.
                    // Use .is_some() instead of == Some(true) because TS2576 should fire for
                    // ANY static member (methods, properties, accessors), not just methods.
                    if self.is_super_expression(access.expression)
                        && let Some(ref class_info) = self.ctx.enclosing_class
                        && let Some(base_idx) = self.get_base_class_idx(class_info.class_idx)
                        && self
                            .is_method_member_in_class_hierarchy(base_idx, property_name, true)
                            .is_some()
                    {
                        use crate::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };

                        let base_name = self.get_class_name_from_decl(base_idx);
                        let static_member_name = format!("{base_name}.{property_name}");
                        let object_type_str = self.format_type(display_object_type);
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

                    // TS2576: instance.member where `member` exists on the class static side.
                    // Use .is_some() — TS2576 fires for any static member (property or method).
                    if !self.is_super_expression(access.expression)
                        && let Some((class_idx, is_static_access)) =
                            self.resolve_class_for_access(access.expression, object_type_for_access)
                        && !is_static_access
                        && self
                            .is_method_member_in_class_hierarchy(class_idx, property_name, true)
                            .is_some()
                    {
                        use crate::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };

                        let class_name = self.get_class_name_from_decl(class_idx);
                        let static_member_name = format!("{class_name}.{property_name}");
                        let object_type_str = self.format_type(display_object_type);
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
                    if !property_name.starts_with('#') && !accessibility_error_emitted {
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
                            self.error_property_not_exist_with_apparent_type(
                                property_name,
                                &format!("typeof {}", symbol.escaped_name),
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
                                self.error_property_not_exist_at(
                                    property_name,
                                    display_object_type,
                                    access.name_or_argument,
                                );
                            }
                        } else {
                            self.error_property_not_exist_at(
                                property_name,
                                display_object_type,
                                access.name_or_argument,
                            );
                        }
                    }
                    TypeId::ERROR
                }

                PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type,
                    cause,
                } => {
                    // Check for optional chaining (?.)
                    if access.question_dot_token {
                        // Suppress error, return (property_type | undefined)
                        let base_type = property_type.unwrap_or(TypeId::UNKNOWN);
                        return factory.union(vec![base_type, TypeId::UNDEFINED]);
                    }

                    // Report error based on the cause (TS2531/TS2532/TS2533 or TS18050)
                    // TS18050 is for definitely-nullish values in strict mode
                    // TS2531/2532/2533 are for possibly-nullish values in strict mode
                    use crate::diagnostics::diagnostic_codes;

                    // Suppress cascade errors when cause is ERROR/ANY/UNKNOWN
                    if cause == TypeId::ERROR || cause == TypeId::ANY || cause == TypeId::UNKNOWN {
                        return property_type.unwrap_or(TypeId::ERROR);
                    }

                    // Check if the type is entirely nullish (no non-nullish part in union)
                    let is_type_nullish = object_type_for_access == TypeId::NULL
                        || object_type_for_access == TypeId::UNDEFINED;

                    // For possibly-nullish values in non-strict mode, don't error
                    // But for definitely-nullish values in non-strict mode, fall through to error reporting below
                    if !self.ctx.compiler_options.strict_null_checks && !is_type_nullish {
                        return self
                            .apply_flow_narrowing(idx, property_type.unwrap_or(TypeId::ERROR));
                    }
                    // Check if the expression is a literal null/undefined keyword (not a variable)
                    // TS18050 is only for `null.foo` and `undefined.bar`, not `x.foo` where x: null
                    // TS18050 is emitted even without strictNullChecks, so check first
                    let is_literal_nullish =
                        if let Some(expr_node) = self.ctx.arena.get(access.expression) {
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

                    // When the expression IS a literal null/undefined keyword (e.g., null.foo or undefined.bar),
                    // emit TS18050 "The value 'X' cannot be used here."
                    if is_literal_nullish {
                        let value_name = if cause == TypeId::NULL {
                            "null"
                        } else if cause == TypeId::UNDEFINED {
                            "undefined"
                        } else {
                            "null | undefined"
                        };
                        self.error_at_node_msg(
                            access.expression,
                            diagnostic_codes::THE_VALUE_CANNOT_BE_USED_HERE,
                            &[value_name],
                        );
                        return self
                            .apply_flow_narrowing(idx, property_type.unwrap_or(TypeId::ERROR));
                    }

                    // Without strictNullChecks, null/undefined are in every type's domain,
                    // so TS18047/TS18048/TS18049 are never emitted (matches tsc behavior).
                    // Note: TS18050 for literal null/undefined is handled above.
                    if !self.ctx.compiler_options.strict_null_checks {
                        return self
                            .apply_flow_narrowing(idx, property_type.unwrap_or(TypeId::ERROR));
                    }

                    // Try to get the name of the expression (handles identifiers and property chains like a.b)
                    // Use specific error codes (TS18047/18048/18049) when name is available
                    let name = self.expression_text(access.expression);

                    let (code, message): (u32, String) = if let Some(ref name) = name {
                        // Use specific error codes with the variable name
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
                    } else {
                        // Fall back to generic error codes
                        if cause == TypeId::NULL {
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
                        }
                    };

                    // Report the error on the expression part
                    self.error_at_node(access.expression, &message, code);

                    // Error recovery: return the property type found in valid members
                    self.apply_flow_narrowing(idx, property_type.unwrap_or(TypeId::ERROR))
                }

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

    fn enum_member_initializer_display_type(&mut self, expr_idx: NodeIndex) -> Option<TypeId> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol.declarations.first().copied()?
        };

        let var_decl = self.ctx.arena.get_variable_declaration_at(decl_idx)?;
        if var_decl.initializer.is_none() {
            return None;
        }

        let init_type = self.get_type_of_node(var_decl.initializer);
        self.is_enum_member_type_for_widening(init_type)
            .then_some(init_type)
    }

    /// In `obj.prop === <literal>`/`!==` comparisons, the base object (`obj`) has
    /// already been flow-narrowed before we resolve `prop`. Re-applying flow
    /// narrowing to the property access result is redundant and expensive on large
    /// discriminated unions.
    fn should_skip_property_result_flow_narrowing(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(ext) = self.ctx.arena.get_extended(idx) else {
            return false;
        };
        let parent = ext.parent;
        if parent.is_none() {
            return false;
        }

        let Some(parent_node) = self.ctx.arena.get(parent) else {
            return false;
        };

        // For optional-chain continuations like `obj?.a?.b`, applying flow
        // narrowing to the intermediate `obj?.a` result is redundant because
        // the continuation logic already handles nullish propagation.
        if let Some(access_node) = self.ctx.arena.get(idx)
            && let Some(access) = self.ctx.arena.get_access_expr(access_node)
            && access.question_dot_token
            && matches!(
                parent_node.kind,
                syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    | syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            )
            && let Some(parent_access) = self.ctx.arena.get_access_expr(parent_node)
            && parent_access.expression == idx
        {
            return true;
        }

        if parent_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }
        let Some(binary) = self.ctx.arena.get_binary_expr(parent_node) else {
            return false;
        };

        let is_equality = matches!(
            binary.operator_token,
            k if k == SyntaxKind::EqualsEqualsToken as u16
                || k == SyntaxKind::ExclamationEqualsToken as u16
                || k == SyntaxKind::EqualsEqualsEqualsToken as u16
                || k == SyntaxKind::ExclamationEqualsEqualsToken as u16
        );
        if !is_equality {
            return false;
        }

        let other = if binary.left == idx {
            binary.right
        } else if binary.right == idx {
            binary.left
        } else {
            return false;
        };
        let other = self.ctx.arena.skip_parenthesized(other);
        let Some(other_node) = self.ctx.arena.get(other) else {
            return false;
        };

        matches!(
            other_node.kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::BigIntLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        )
    }

    /// Additional skip conditions for applying flow narrowing to property
    /// access results.
    ///
    /// For `obj?.prop ?? fallback`, flow narrowing the left operand result is
    /// generally redundant and adds overhead in hot optional-chain paths.
    fn should_skip_property_result_flow_narrowing_for_result(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        if self.should_skip_property_result_flow_narrowing(idx) {
            return true;
        }

        let Some(ext) = self.ctx.arena.get_extended(idx) else {
            return false;
        };
        let parent = ext.parent;
        if parent.is_none() {
            return false;
        }

        let Some(parent_node) = self.ctx.arena.get(parent) else {
            return false;
        };
        if parent_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }
        let Some(binary) = self.ctx.arena.get_binary_expr(parent_node) else {
            return false;
        };

        if binary.operator_token != SyntaxKind::QuestionQuestionToken as u16 || binary.left != idx {
            return false;
        }

        let Some(access_node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(access) = self.ctx.arena.get_access_expr(access_node) else {
            return false;
        };
        access.question_dot_token
    }

    fn resolve_array_global_augmentation_property(
        &mut self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        tracing::debug!(
            "resolve_array_global_augmentation_property: property_name = {:?}, object_type = {:?}",
            property_name,
            object_type
        );
        use rustc_hash::FxHashMap;
        use std::sync::Arc;
        use tsz_lowering::TypeLowering;
        use tsz_parser::parser::NodeArena;
        use tsz_parser::parser::node::NodeAccess;
        use tsz_solver::is_compiler_managed_type;
        use tsz_solver::operations::property::PropertyAccessResult;
        let base_type =
            crate::query_boundaries::property_access::unwrap_readonly(self.ctx.types, object_type);

        let element_type = if let Some(elem) =
            crate::query_boundaries::property_access::array_element_type(self.ctx.types, base_type)
        {
            Some(elem)
        } else if let Some(union_ty) =
            crate::query_boundaries::property_access::tuple_element_type_union(
                self.ctx.types,
                base_type,
            )
        {
            Some(union_ty)
        } else {
            crate::query_boundaries::property_access::application_first_arg(
                self.ctx.types,
                base_type,
            )
        };
        let element_type = element_type?;

        let augmentation_decls = self.ctx.binder.global_augmentations.get("Array")?;
        if augmentation_decls.is_empty() {
            return None;
        }

        let all_arenas = self.ctx.all_arenas.clone();
        let all_binders = self.ctx.all_binders.clone();
        let lib_contexts = self.ctx.lib_contexts.clone();
        let binder_for_arena = |arena_ref: &NodeArena| -> Option<&tsz_binder::BinderState> {
            let arenas = all_arenas.as_ref()?;
            let binders = all_binders.as_ref()?;
            let arena_ptr = arena_ref as *const NodeArena;
            for (idx, arena) in arenas.iter().enumerate() {
                if Arc::as_ptr(arena) == arena_ptr {
                    return binders.get(idx).map(Arc::as_ref);
                }
            }
            None
        };

        let resolve_in_scope = |binder: &tsz_binder::BinderState,
                                arena_ref: &NodeArena,
                                node_idx: NodeIndex|
         -> Option<u32> {
            let ident_name = arena_ref.get_identifier_text(node_idx)?;
            let mut scope_id = binder.find_enclosing_scope(arena_ref, node_idx)?;
            while scope_id != tsz_binder::ScopeId::NONE {
                let scope = binder.scopes.get(scope_id.0 as usize)?;
                if let Some(sym_id) = scope.table.get(ident_name) {
                    return Some(sym_id.0);
                }
                scope_id = scope.parent;
            }
            None
        };

        let mut cross_file_groups: FxHashMap<usize, (Arc<NodeArena>, Vec<NodeIndex>)> =
            FxHashMap::default();
        for aug in augmentation_decls {
            if let Some(ref arena) = aug.arena {
                let key = Arc::as_ptr(arena) as usize;
                cross_file_groups
                    .entry(key)
                    .or_insert_with(|| (Arc::clone(arena), Vec::new()))
                    .1
                    .push(aug.node);
            } else {
                let key = self.ctx.arena as *const NodeArena as usize;
                cross_file_groups
                    .entry(key)
                    .or_insert_with(|| (Arc::new(self.ctx.arena.clone()), Vec::new()))
                    .1
                    .push(aug.node);
            }
        }

        let mut found_types = Vec::new();
        for (_, (arena, decls)) in cross_file_groups {
            let decl_binder = binder_for_arena(arena.as_ref()).unwrap_or(self.ctx.binder);
            let resolver = |node_idx: NodeIndex| -> Option<u32> {
                if let Some(sym_id) = decl_binder.get_node_symbol(node_idx) {
                    return Some(sym_id.0);
                }
                if let Some(sym_id) = resolve_in_scope(decl_binder, arena.as_ref(), node_idx) {
                    return Some(sym_id);
                }
                let ident_name = arena.as_ref().get_identifier_text(node_idx)?;
                if is_compiler_managed_type(ident_name) {
                    return None;
                }
                if let Some(found_sym) = decl_binder.file_locals.get(ident_name) {
                    return Some(found_sym.0);
                }
                if let Some(all_binders) = all_binders.as_ref() {
                    for binder in all_binders.iter() {
                        if let Some(found_sym) = binder.file_locals.get(ident_name) {
                            return Some(found_sym.0);
                        }
                    }
                }
                for ctx in &lib_contexts {
                    if let Some(found_sym) = ctx.binder.file_locals.get(ident_name) {
                        return Some(found_sym.0);
                    }
                }
                None
            };
            let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::DefId> {
                if let Some(sym_id) = decl_binder.get_node_symbol(node_idx) {
                    return Some(
                        self.ctx
                            .get_or_create_def_id(tsz_binder::SymbolId(sym_id.0)),
                    );
                }
                if let Some(sym_id) = resolve_in_scope(decl_binder, arena.as_ref(), node_idx) {
                    return Some(self.ctx.get_or_create_def_id(tsz_binder::SymbolId(sym_id)));
                }
                let ident_name = arena.as_ref().get_identifier_text(node_idx)?;
                if is_compiler_managed_type(ident_name) {
                    return None;
                }
                let sym_id = decl_binder.file_locals.get(ident_name).or_else(|| {
                    if let Some(all_binders) = all_binders.as_ref() {
                        for binder in all_binders.iter() {
                            if let Some(found_sym) = binder.file_locals.get(ident_name) {
                                return Some(found_sym);
                            }
                        }
                    }
                    lib_contexts
                        .iter()
                        .find_map(|ctx| ctx.binder.file_locals.get(ident_name))
                })?;
                Some(
                    self.ctx
                        .get_or_create_def_id(tsz_binder::SymbolId(sym_id.0)),
                )
            };

            let decls_with_arenas: Vec<(NodeIndex, &NodeArena)> = decls
                .iter()
                .map(|&decl_idx| (decl_idx, arena.as_ref()))
                .collect();
            let name_resolver = |type_name: &str| -> Option<tsz_solver::DefId> {
                self.resolve_entity_name_text_to_def_id_for_lowering(type_name)
            };
            let lowering = TypeLowering::with_hybrid_resolver(
                arena.as_ref(),
                self.ctx.types,
                &resolver,
                &def_id_resolver,
                &|_| None,
            )
            .with_name_def_id_resolver(&name_resolver);
            let (aug_type, params) =
                lowering.lower_merged_interface_declarations(&decls_with_arenas);
            if aug_type == TypeId::ERROR {
                continue;
            }

            if let PropertyAccessResult::Success { type_id, .. } =
                self.resolve_property_access_with_env(aug_type, property_name)
            {
                found_types.push(type_id);
                continue;
            }

            if !params.is_empty() {
                let mut args = Vec::with_capacity(params.len());
                args.push(element_type);
                for _ in 1..params.len() {
                    args.push(TypeId::ANY);
                }
                let app_type = self.ctx.types.factory().application(aug_type, args);
                if let PropertyAccessResult::Success { type_id, .. } =
                    self.resolve_property_access_with_env(app_type, property_name)
                {
                    found_types.push(type_id);
                }
            }
        }

        if found_types.is_empty() {
            None
        } else {
            Some(tsz_solver::utils::union_or_single(
                self.ctx.types,
                found_types,
            ))
        }
    }

    /// Resolve property from global interface augmentations for primitive wrapper types
    /// and other well-known global interfaces (Boolean, Number, String, `ErrorConstructor`, etc.).
    ///
    /// When a user writes `interface Boolean { doStuff() }` at the top level, this augments
    /// the built-in Boolean interface. Property accesses on `boolean` values should find
    /// these augmented members.
    fn resolve_general_global_augmentation_property(
        &mut self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        // Map the object type to potential global interface names
        let interface_names: &[&str] = if crate::query_boundaries::property_access::is_boolean_type(
            self.ctx.types,
            object_type,
        ) {
            &["Boolean"]
        } else if crate::query_boundaries::property_access::is_number_type(
            self.ctx.types,
            object_type,
        ) {
            &["Number"]
        } else if crate::query_boundaries::property_access::is_string_type(
            self.ctx.types,
            object_type,
        ) {
            &["String"]
        } else if crate::query_boundaries::property_access::is_symbol_type(
            self.ctx.types,
            object_type,
        ) {
            &["Symbol"]
        } else if crate::query_boundaries::property_access::is_bigint_type(
            self.ctx.types,
            object_type,
        ) {
            &["BigInt"]
        } else {
            // For object types, try to find the interface name from the symbol
            // that declared the type (handles ErrorConstructor, RegExp, Date, etc.)
            return self.resolve_object_type_global_augmentation(object_type, property_name);
        };

        for &iface_name in interface_names {
            if let Some(result) =
                self.resolve_augmentation_property_by_name(iface_name, property_name)
            {
                return Some(result);
            }
        }
        None
    }

    /// Try to resolve a property from global augmentations for an object type
    /// by looking up its symbol's name in the augmentation map.
    fn resolve_object_type_global_augmentation(
        &mut self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        // For object types that come from lib declarations (ErrorConstructor, RegExp, etc.),
        // check if the type's symbol name matches any global augmentation.
        let def_id = crate::query_boundaries::property_access::def_id(self.ctx.types, object_type)?;

        // Look up the symbol for this DefId
        let sym_id = self.ctx.def_to_symbol.borrow().get(&def_id).copied()?;
        let lib_binders = self.get_lib_binders();
        let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)?;
        let name = &symbol.escaped_name;

        if self.ctx.binder.global_augmentations.contains_key(name) {
            return self.resolve_augmentation_property_by_name(name, property_name);
        }
        None
    }

    /// Resolve a property from global augmentation declarations for a specific interface name.
    fn resolve_augmentation_property_by_name(
        &mut self,
        interface_name: &str,
        property_name: &str,
    ) -> Option<TypeId> {
        use rustc_hash::FxHashMap;
        use std::sync::Arc;
        use tsz_lowering::TypeLowering;
        use tsz_parser::parser::NodeArena;
        use tsz_parser::parser::node::NodeAccess;
        use tsz_solver::is_compiler_managed_type;
        use tsz_solver::operations::property::PropertyAccessResult;

        let augmentation_decls = self.ctx.binder.global_augmentations.get(interface_name)?;
        if augmentation_decls.is_empty() {
            return None;
        }

        let all_arenas = self.ctx.all_arenas.clone();
        let all_binders = self.ctx.all_binders.clone();
        let lib_contexts = self.ctx.lib_contexts.clone();

        let binder_for_arena = |arena_ref: &NodeArena| -> Option<&tsz_binder::BinderState> {
            let arenas = all_arenas.as_ref()?;
            let binders = all_binders.as_ref()?;
            let arena_ptr = arena_ref as *const NodeArena;
            for (idx, arena) in arenas.iter().enumerate() {
                if Arc::as_ptr(arena) == arena_ptr {
                    return binders.get(idx).map(Arc::as_ref);
                }
            }
            None
        };

        let resolve_in_scope = |binder: &tsz_binder::BinderState,
                                arena_ref: &NodeArena,
                                node_idx: tsz_parser::parser::NodeIndex|
         -> Option<u32> {
            let ident_name = arena_ref.get_identifier_text(node_idx)?;
            let mut scope_id = binder.find_enclosing_scope(arena_ref, node_idx)?;
            while scope_id != tsz_binder::ScopeId::NONE {
                let scope = binder.scopes.get(scope_id.0 as usize)?;
                if let Some(sym_id) = scope.table.get(ident_name) {
                    return Some(sym_id.0);
                }
                scope_id = scope.parent;
            }
            None
        };

        let mut cross_file_groups: FxHashMap<
            usize,
            (Arc<NodeArena>, Vec<tsz_parser::parser::NodeIndex>),
        > = FxHashMap::default();
        for aug in augmentation_decls {
            if let Some(ref arena) = aug.arena {
                let key = Arc::as_ptr(arena) as usize;
                cross_file_groups
                    .entry(key)
                    .or_insert_with(|| (Arc::clone(arena), Vec::new()))
                    .1
                    .push(aug.node);
            } else {
                let key = self.ctx.arena as *const NodeArena as usize;
                cross_file_groups
                    .entry(key)
                    .or_insert_with(|| (Arc::new(self.ctx.arena.clone()), Vec::new()))
                    .1
                    .push(aug.node);
            }
        }

        let mut found_types = Vec::new();
        for (_, (arena, decls)) in cross_file_groups {
            let decl_binder = binder_for_arena(arena.as_ref()).unwrap_or(self.ctx.binder);
            let resolver = |node_idx: tsz_parser::parser::NodeIndex| -> Option<u32> {
                if let Some(sym_id) = decl_binder.get_node_symbol(node_idx) {
                    return Some(sym_id.0);
                }
                if let Some(sym_id) = resolve_in_scope(decl_binder, arena.as_ref(), node_idx) {
                    return Some(sym_id);
                }
                let ident_name = arena.as_ref().get_identifier_text(node_idx)?;
                if is_compiler_managed_type(ident_name) {
                    return None;
                }
                if let Some(found_sym) = decl_binder.file_locals.get(ident_name) {
                    return Some(found_sym.0);
                }
                if let Some(all_binders) = all_binders.as_ref() {
                    for binder in all_binders.iter() {
                        if let Some(found_sym) = binder.file_locals.get(ident_name) {
                            return Some(found_sym.0);
                        }
                    }
                }
                for ctx in &lib_contexts {
                    if let Some(found_sym) = ctx.binder.file_locals.get(ident_name) {
                        return Some(found_sym.0);
                    }
                }
                None
            };
            let def_id_resolver =
                |node_idx: tsz_parser::parser::NodeIndex| -> Option<tsz_solver::DefId> {
                    if let Some(sym_id) = decl_binder.get_node_symbol(node_idx) {
                        return Some(
                            self.ctx
                                .get_or_create_def_id(tsz_binder::SymbolId(sym_id.0)),
                        );
                    }
                    if let Some(sym_id) = resolve_in_scope(decl_binder, arena.as_ref(), node_idx) {
                        return Some(self.ctx.get_or_create_def_id(tsz_binder::SymbolId(sym_id)));
                    }
                    let ident_name = arena.as_ref().get_identifier_text(node_idx)?;
                    if is_compiler_managed_type(ident_name) {
                        return None;
                    }
                    let sym_id = decl_binder.file_locals.get(ident_name).or_else(|| {
                        if let Some(all_binders) = all_binders.as_ref() {
                            for binder in all_binders.iter() {
                                if let Some(found_sym) = binder.file_locals.get(ident_name) {
                                    return Some(found_sym);
                                }
                            }
                        }
                        lib_contexts
                            .iter()
                            .find_map(|ctx| ctx.binder.file_locals.get(ident_name))
                    })?;
                    Some(
                        self.ctx
                            .get_or_create_def_id(tsz_binder::SymbolId(sym_id.0)),
                    )
                };

            let decls_with_arenas: Vec<(tsz_parser::parser::NodeIndex, &NodeArena)> = decls
                .iter()
                .map(|&decl_idx| (decl_idx, arena.as_ref()))
                .collect();
            let name_resolver = |type_name: &str| -> Option<tsz_solver::DefId> {
                self.resolve_entity_name_text_to_def_id_for_lowering(type_name)
            };
            let lowering = TypeLowering::with_hybrid_resolver(
                arena.as_ref(),
                self.ctx.types,
                &resolver,
                &def_id_resolver,
                &|_| None,
            )
            .with_name_def_id_resolver(&name_resolver);
            let (aug_type, _params) =
                lowering.lower_merged_interface_declarations(&decls_with_arenas);
            if aug_type == TypeId::ERROR {
                continue;
            }

            if let PropertyAccessResult::Success { type_id, .. } =
                self.resolve_property_access_with_env(aug_type, property_name)
            {
                found_types.push(type_id);
            }
        }

        if found_types.is_empty() {
            None
        } else {
            Some(tsz_solver::utils::union_or_single(
                self.ctx.types,
                found_types,
            ))
        }
    }

    /// Check if a property access is an expando function assignment pattern.
    ///
    /// TypeScript allows assigning properties to function and class declarations:
    /// ```typescript
    /// function foo() {}
    /// foo.bar = 1;  // OK - expando pattern, no TS2339
    /// ```
    ///
    /// Returns true if:
    /// 1. The property access is the LHS of a `=` assignment
    /// 2. The object expression is an identifier bound to a function or class declaration
    /// 3. The object type is a function type
    fn is_expando_function_assignment(
        &self,
        property_access_idx: NodeIndex,
        object_expr_idx: NodeIndex,
        object_type: TypeId,
    ) -> bool {
        use tsz_solver::visitor::is_function_type;

        // Check if object type is a function type
        if !is_function_type(self.ctx.types, object_type) {
            return false;
        }

        // Check if property access is LHS of a `=` assignment
        let parent_idx = match self.ctx.arena.get_extended(property_access_idx) {
            Some(ext) if ext.parent.is_some() => ext.parent,
            _ => return false,
        };
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        let Some(binary) = self.ctx.arena.get_binary_expr(parent_node) else {
            return false;
        };
        if binary.operator_token != SyntaxKind::EqualsToken as u16
            || binary.left != property_access_idx
        {
            return false;
        }

        // Resolve object symbol for both simple identifiers and qualified chains.
        let sym_id = self
            .resolve_identifier_symbol(object_expr_idx)
            .or_else(|| self.resolve_qualified_symbol(object_expr_idx));

        if let Some(sym_id) = sym_id
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            return (symbol.flags & (symbol_flags::FUNCTION | symbol_flags::CLASS)) != 0;
        }

        // Namespace member fallback: allow expando assignment for function-typed
        // members accessed through namespace/value-module chains (e.g., `app.foo.bar = ...`).
        // Binder tracks these expandos by chain key, so reads can observe them later.
        fn root_identifier(
            arena: &tsz_parser::parser::node::NodeArena,
            idx: NodeIndex,
        ) -> Option<String> {
            let node = arena.get(idx)?;
            if node.kind == SyntaxKind::Identifier as u16 {
                return arena.get_identifier(node).map(|id| id.escaped_text.clone());
            }
            if node.kind == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let access = arena.get_access_expr(node)?;
                return root_identifier(arena, access.expression);
            }
            None
        }

        if let Some(root_name) = root_identifier(self.ctx.arena, object_expr_idx)
            && let Some(root_sym) = self.ctx.binder.file_locals.get(&root_name)
            && let Some(root_symbol) = self.ctx.binder.get_symbol(root_sym)
            && (root_symbol.flags & (symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE))
                != 0
        {
            return true;
        }

        false
    }

    /// Check if a property access reads an expando property assigned via `X.prop = value`.
    ///
    /// Checks the current file's binder first, then all other binders in multi-file
    /// mode (for global-scope cross-file expando access). Also handles import chains
    /// like `a.C1.staticProp` by resolving the object expression to its source symbol
    /// and checking the source file's binder.
    fn is_expando_property_read(&self, object_expr_idx: NodeIndex, property_name: &str) -> bool {
        fn property_access_chain(
            arena: &tsz_parser::parser::node::NodeArena,
            idx: NodeIndex,
        ) -> Option<String> {
            let node = arena.get(idx)?;
            if node.kind == SyntaxKind::Identifier as u16 {
                return arena.get_identifier(node).map(|id| id.escaped_text.clone());
            }
            if node.kind == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let access = arena.get_access_expr(node)?;
                let left = property_access_chain(arena, access.expression)?;
                let right_node = arena.get(access.name_or_argument)?;
                let right = arena.get_identifier(right_node)?.escaped_text.clone();
                return Some(format!("{left}.{right}"));
            }
            None
        }

        let Some(obj_key) = property_access_chain(self.ctx.arena, object_expr_idx) else {
            return false;
        };

        // 1. Check current file's binder
        if self
            .ctx
            .binder
            .expando_properties
            .get(&obj_key)
            .is_some_and(|props| props.contains(property_name))
        {
            return true;
        }

        // 2. Check all other binders (cross-file global scope access)
        if let Some(all_binders) = &self.ctx.all_binders {
            for binder in all_binders.iter() {
                if binder
                    .expando_properties
                    .get(&obj_key)
                    .is_some_and(|props| props.contains(property_name))
                {
                    return true;
                }
            }
        }

        // 3. For qualified access chains like `a.C1` where `a` is an import namespace,
        //    the source file's binder stores the expando under just "C1" (the original
        //    symbol name), not "a.C1". Extract the last segment and check all binders.
        if let Some(last_dot) = obj_key.rfind('.') {
            let last_segment = &obj_key[last_dot + 1..];
            if let Some(all_binders) = &self.ctx.all_binders {
                for binder in all_binders.iter() {
                    if binder
                        .expando_properties
                        .get(last_segment)
                        .is_some_and(|props| props.contains(property_name))
                    {
                        return true;
                    }
                }
            }
        }

        false
    }

    fn union_has_explicit_property_member(&mut self, object_type: TypeId, prop_name: &str) -> bool {
        use tsz_solver::operations::property::PropertyAccessResult;

        let Some(members) =
            crate::query_boundaries::state::checking::union_members(self.ctx.types, object_type)
        else {
            return false;
        };

        members.iter().copied().any(|member| {
            let resolved_member = self.resolve_type_for_property_access(member);
            matches!(
                self.resolve_property_access_with_env(resolved_member, prop_name),
                PropertyAccessResult::Success {
                    from_index_signature: false,
                    ..
                }
            )
        })
    }

    fn type_parameter_constraint_has_explicit_property(
        &mut self,
        object_type: TypeId,
        prop_name: &str,
    ) -> bool {
        use tsz_solver::operations::property::PropertyAccessResult;

        let Some(constraint) = crate::query_boundaries::state::checking::type_parameter_constraint(
            self.ctx.types,
            object_type,
        ) else {
            return false;
        };

        let resolved_constraint = self.resolve_type_for_property_access(constraint);
        matches!(
            self.resolve_property_access_with_env(resolved_constraint, prop_name),
            PropertyAccessResult::Success {
                from_index_signature: false,
                ..
            }
        )
    }

    fn strict_bind_call_apply_method_type(
        &mut self,
        object_type: TypeId,
        object_expr_idx: NodeIndex,
        property_name: &str,
    ) -> Option<TypeId> {
        if property_name != "apply" {
            return None;
        }

        let factory = self.ctx.types.factory();
        let mut candidates = vec![object_type];
        if let Some(sym_id) = self.resolve_identifier_symbol(object_expr_idx) {
            let sym_type = self.get_type_of_symbol(sym_id);
            if sym_type != TypeId::ERROR && !candidates.contains(&sym_type) {
                candidates.push(sym_type);
            }
        }

        let mut resolved_shape = None;
        for candidate in candidates {
            if let Some(shape) =
                crate::query_boundaries::property_access::function_shape(self.ctx.types, candidate)
            {
                resolved_shape = Some((shape.params.clone(), shape.return_type));
                break;
            }
            if let Some(shape) =
                crate::query_boundaries::property_access::callable_shape(self.ctx.types, candidate)
                && let Some(sig) = shape.call_signatures.first()
            {
                resolved_shape = Some((sig.params.clone(), sig.return_type));
                break;
            }
        }

        let (params, return_type) = resolved_shape?;

        let tuple_elements: Vec<tsz_solver::TupleElement> = params
            .iter()
            .map(|param| tsz_solver::TupleElement {
                type_id: param.type_id,
                name: param.name,
                optional: param.optional && !param.rest,
                rest: param.rest,
            })
            .collect();
        let args_tuple = factory.tuple(tuple_elements);
        let method_shape = tsz_solver::FunctionShape {
            params: vec![
                tsz_solver::ParamInfo {
                    name: Some(self.ctx.types.intern_string("thisArg")),
                    type_id: TypeId::ANY,
                    optional: false,
                    rest: false,
                },
                tsz_solver::ParamInfo {
                    name: Some(self.ctx.types.intern_string("args")),
                    type_id: args_tuple,
                    optional: true,
                    rest: false,
                },
            ],
            this_type: None,
            return_type,
            type_params: vec![],
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        };

        Some(factory.function(method_shape))
    }

    /// Emit TS1470 if `import.meta` appears in a file that builds to CommonJS output.
    ///
    /// TSC logic: in Node16/NodeNext module modes, the per-file format determines
    /// whether the file outputs CJS (TS1470). For older module modes (< ES2020,
    /// excluding System), ALL files produce CJS output so TS1470 always fires.
    fn check_import_meta_in_cjs(&mut self, node_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_common::common::ModuleKind;

        let module_kind = self.ctx.compiler_options.module;
        let should_error = if module_kind.is_node_module() {
            // Node16/Node18/Node20/NodeNext: per-file CJS/ESM determination
            let current_file = &self.ctx.file_name;
            let is_commonjs_file = current_file.ends_with(".cts") || current_file.ends_with(".cjs");
            let is_esm_file = current_file.ends_with(".mts") || current_file.ends_with(".mjs");
            if is_commonjs_file {
                true
            } else if is_esm_file {
                false
            } else if let Some(is_esm) = self.ctx.file_is_esm {
                !is_esm
            } else {
                false
            }
        } else if module_kind == ModuleKind::System
            || (module_kind as u32) >= (ModuleKind::ES2020 as u32)
        {
            // System and ES2020+ support import.meta natively
            false
        } else {
            // CommonJS, AMD, UMD, ES2015, None → always CJS output
            true
        };

        if should_error {
            self.error_at_node(
                node_idx,
                diagnostic_messages::THE_IMPORT_META_META_PROPERTY_IS_NOT_ALLOWED_IN_FILES_WHICH_WILL_BUILD_INTO_COMM,
                diagnostic_codes::THE_IMPORT_META_META_PROPERTY_IS_NOT_ALLOWED_IN_FILES_WHICH_WILL_BUILD_INTO_COMM,
            );
        }
    }
}
