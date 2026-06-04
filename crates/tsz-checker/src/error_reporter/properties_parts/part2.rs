impl<'a> CheckerState<'a> {
    /// Report a property not exist error using solver diagnostics with source tracking.
    /// If a similar property name is found on the type, emits TS2551 ("Did you mean?")
    /// instead of TS2339.
    pub fn error_property_not_exist_at(
        &mut self,
        prop_name: &str,
        type_id: TypeId,
        idx: NodeIndex,
    ) {
        // Suppress TS2339 when the type is an internal inference placeholder (__infer_*).
        // These placeholders are created during generic call inference when a type parameter
        // cannot be resolved to a concrete type. Reporting errors on these placeholders
        // produces confusing diagnostics. The actual inference/assignability issue should be
        // reported elsewhere.
        if crate::query_boundaries::common::is_bare_infer_placeholder(self.ctx.types, type_id) {
            return;
        }

        if self.actual_lib_namespace_merged_type_has_property(type_id, prop_name) {
            return;
        }

        // Suppress error if type is ERROR/ANY or an Error type wrapper.
        // This prevents cascading errors when accessing properties on error types.
        // NOTE: We do NOT suppress for UNKNOWN — accessing properties on unknown should error (TS2339).
        // NOTE: We do NOT suppress for NEVER — tsc emits TS2339 for property access on `never`
        // (e.g., after typeof narrowing exhausts all possibilities).
        if type_id == TypeId::ERROR
            || type_id == TypeId::ANY
            || crate::query_boundaries::common::is_error_type(self.ctx.types, type_id)
        {
            return;
        }

        if self.is_global_this_surface_type(type_id)
            && self.ctx.no_implicit_any()
            && !self.is_js_file()
        {
            use crate::diagnostics::{diagnostic_messages, format_message};
            self.error_at_anchor(
                idx,
                DiagnosticAnchorKind::PropertyToken,
                &format_message(
                    diagnostic_messages::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_TYPE_HAS_NO_INDEX_SIGNATURE,
                    &["typeof globalThis"],
                ),
                diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_TYPE_HAS_NO_INDEX_SIGNATURE,
            );
            return;
        }

        // Suppress TS2339 when evaluating a computed property name expression
        // during class instance type building. When a class has a self-referential
        // computed property (e.g., `[rC.x]` inside `declare class RC<T> { x: T;
        // [rC.x]: "b"; }` where `rC: RC<"a">`), the class instance type isn't
        // fully built yet, causing property access on the incomplete type to fail.
        // This is a transient state — the property will be found once the class
        // is fully built. Suppressing here avoids false positives while the
        // computed property name is being evaluated for class member resolution.
        if self.ctx.checking_computed_property_name.is_some()
            && !self.ctx.class_instance_resolution_set.is_empty()
            && crate::query_boundaries::common::application_info(
                self.ctx.types.as_type_database(),
                type_id,
            )
            .is_some()
        {
            return;
        }

        // Suppress TS2339 when the object is a type parameter whose constraint
        // resolved to ERROR or is self-referential/circular.
        //
        // Case 1: constraint == ERROR — the constraint itself already produced a
        // diagnostic (e.g., `typeof a` where `a` is out of scope → TS2552).
        //
        // Case 2: circular constraint — `T extends typeof a` where `a: T` creates
        // a self-referential chain. During two-pass type parameter resolution, the
        // placeholder T (constraint=None) is created first, then the refined T gets
        // constraint=placeholder_T. When a destructured binding `{a}: {a:T}` uses
        // the placeholder T, property access sees constraint=None but the scope has
        // a refined version whose constraint points back to this same placeholder.
        // tsc suppresses TS2339 in this case because the constraint is unresolvable.
        if crate::query_boundaries::state::checking::is_type_parameter_like(self.ctx.types, type_id)
        {
            let constraint = crate::query_boundaries::state::checking::type_parameter_constraint(
                self.ctx.types,
                type_id,
            );
            if constraint.is_some_and(|constraint| {
                constraint == TypeId::ERROR
                    || crate::query_boundaries::common::is_error_type(self.ctx.types, constraint)
            }) {
                return;
            }
            if let Some(name) = crate::query_boundaries::property_access::type_parameter_name(
                self.ctx.types,
                type_id,
            ) {
                let is_self_ref = |c: TypeId| -> bool {
                    crate::query_boundaries::state::checking::is_type_parameter_like(
                        self.ctx.types,
                        c,
                    ) && crate::query_boundaries::property_access::type_parameter_name(
                        self.ctx.types,
                        c,
                    ) == Some(name)
                };
                if constraint.is_some_and(&is_self_ref) {
                    return;
                }
                let name_str = self.ctx.types.resolve_atom(name);
                if let Some(&scope_id) = self.ctx.type_parameter_scope.get(&*name_str)
                    && scope_id != type_id
                {
                    let scope_constraint =
                        crate::query_boundaries::state::checking::type_parameter_constraint(
                            self.ctx.types,
                            scope_id,
                        );
                    if scope_constraint.is_some_and(|constraint| {
                        constraint == TypeId::ERROR
                            || crate::query_boundaries::common::is_error_type(
                                self.ctx.types,
                                constraint,
                            )
                            || is_self_ref(constraint)
                            || constraint == type_id
                    }) {
                        return;
                    }
                }
            } else if constraint.is_none() {
                // Fall back to the display-keyed scope lookup for stale placeholder copies.
                let type_display = self.format_type(type_id);
                if let Some(&scope_id) = self.ctx.type_parameter_scope.get(&type_display)
                    && scope_id != type_id
                {
                    let scope_constraint =
                        crate::query_boundaries::state::checking::type_parameter_constraint(
                            self.ctx.types,
                            scope_id,
                        );
                    if scope_constraint == Some(type_id) {
                        return;
                    }
                }
            }
        }

        // Suppress TS2339 when the file has syntax parse errors.
        // This prevents cascading errors when the parser has already reported syntax issues
        // (e.g., malformed import.defer() without parentheses → TS1005 already emitted).
        if self.has_syntax_parse_errors() {
            return;
        }

        // Suppress TS2339 when the property access is on an expression rooted in an
        // unresolved import (TS2307 was already emitted for the missing module).
        // This prevents cascading errors when a namespace import fails to resolve.
        if let Some(parent) = self.ctx.arena.get_extended(idx)
            && let Some(parent_node) = self.ctx.arena.get(parent.parent)
            && parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        {
            // Get the access expression and check if the base is an unresolved import
            if let Some(access) = self.ctx.arena.get_access_expr(parent_node) {
                // Check if the base expression is an unresolved import
                if self.is_unresolved_import_symbol(access.expression) {
                    return;
                }
                // Check if the base expression type is ERROR (indicating a failed resolution)
                let base_type = self.get_type_of_node(access.expression);
                if base_type == TypeId::ERROR {
                    return;
                }
                // Also check the full chain for unresolved imports
                if self.is_property_access_on_unresolved_import(parent.parent) {
                    return;
                }
            }
        }

        // Checked-JS function declarations support expando writes like
        // `fn.extra = value` without TS2339. Suppress the diagnostic when the
        // property name belongs to a direct write target rooted at a function
        // symbol, even if an intermediate query transiently observed the RHS type.
        if self.is_js_file()
            && self.ctx.compiler_options.check_js
            && let Some(parent) = self.ctx.arena.get_extended(idx)
            && parent.parent.is_some()
            && let Some(parent_node) = self.ctx.arena.get(parent.parent)
            && parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(parent_node)
            && access.name_or_argument == idx
            && self
                .ctx
                .arena
                .get_extended(parent.parent)
                .is_some_and(|prop_ext| {
                    let parent_idx = prop_ext.parent;
                    self.ctx
                        .arena
                        .get(parent_idx)
                        .and_then(|write_parent| {
                            if write_parent.kind == syntax_kind_ext::BINARY_EXPRESSION {
                                let binary = self.ctx.arena.get_binary_expr(write_parent)?;
                                return Some(
                                    binary.left == parent.parent
                                        && self.is_assignment_operator(binary.operator_token),
                                );
                            }
                            if write_parent.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                                || write_parent.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
                            {
                                let unary = self.ctx.arena.get_unary_expr(write_parent)?;
                                return Some(
                                    unary.operator == tsz_scanner::SyntaxKind::PlusPlusToken as u16
                                        || unary.operator
                                            == tsz_scanner::SyntaxKind::MinusMinusToken as u16,
                                );
                            }
                            Some(false)
                        })
                        .unwrap_or(false)
                })
            && let Some(obj_sym) =
                self.resolve_identifier_symbol_without_tracking(access.expression)
            && let Some(symbol) = self
                .get_cross_file_symbol(obj_sym)
                .or_else(|| self.ctx.binder.get_symbol(obj_sym))
            && symbol.has_any_flags(tsz_binder::symbol_flags::FUNCTION)
            && !symbol.has_any_flags(tsz_binder::symbol_flags::CLASS)
        {
            return;
        }

        // In JS/checkJs, `Object.defineProperty(...)` can be handled by the
        // checker’s descriptor-aware paths even when generic member lookup on
        // the global Object value misses. Suppress the fallback TS2339 here so
        // those specialized defineProperty semantics can proceed without the
        // spurious property-not-found diagnostic.
        if self.is_js_file()
            && self.ctx.compiler_options.check_js
            && prop_name == "defineProperty"
            && let Some(parent) = self.ctx.arena.get_extended(idx)
            && parent.parent.is_some()
            && let Some(parent_node) = self.ctx.arena.get(parent.parent)
            && parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(parent_node)
            && access.name_or_argument == idx
            && self.identifier_resolves_to_unshadowed_global(access.expression, "Object")
        {
            return;
        }

        // Suppress cascaded TS2339 from failed generic inference when the receiver
        // remains a union that still contains unresolved type parameters.
        // This keeps follow-on property errors from obscuring the primary root cause
        // (typically assignability/inference diagnostics).
        //
        // Only suppress when a DIRECT union member is a type parameter (e.g. T | Foo).
        // Do NOT suppress when type parameters are deeply nested inside object types
        // (e.g. string | MyInterface where MyInterface has generic base types).
        // The deep nesting case occurs with concrete unions like `string | MyArr`
        // where MyArr extends Array<string> -- the resolved object shape may contain
        // type parameters from the generic base, but the union itself is concrete.
        // NOTE: In tsc 6.0, unconstrained type parameters in unions DO trigger
        // TS2339 when the property doesn't exist on the type parameter member.
        // We no longer suppress TS2339 for unions with type parameters.

        // When a class extends `any`, tsc treats unknown member accesses as `any`
        // and does not emit TS2339. Check this before computing source location
        // to avoid unnecessary work.
        if self.class_extends_any_base(type_id) {
            return;
        }

        // Array-like generic constraints always provide `.length`; if property
        // resolution misses while recursive conditional evaluation is still
        // deferred, avoid emitting a cascaded TS2339.
        if prop_name == "length" && self.property_type_has_array_like_length(type_id) {
            return;
        }

        // Suppress TS2339 for indexed access types on generic conditional/mapped types.
        // For example, `Parameters<DataFirst>["length"]` where `Parameters<T>` is a
        // conditional type. When the type argument is generic, tsc defers the check
        // rather than emitting a false TS2339.
        if crate::query_boundaries::common::is_index_access_type(self.ctx.types, type_id) {
            return;
        }

        // Suppress TS2339 for types that are generic type parameters with conditional
        // type constraints. For example, when accessing a property on a type parameter
        // like `T extends SomeConditionalType`, the property may exist on the resolved
        // conditional type but we can't determine it until the type parameter is
        // instantiated with a concrete type.
        if crate::query_boundaries::state::checking::is_type_parameter_like(self.ctx.types, type_id)
            && crate::query_boundaries::common::type_parameter_has_conditional_constraint(
                self.ctx.types,
                type_id,
            )
        {
            return;
        }

        // Suppress TS2339 for type parameters with generic mapped type constraints.
        // For example, `T extends { [K in keyof U]: V }` where U is another type parameter.
        // The mapped type cannot be fully resolved until U is instantiated.
        if crate::query_boundaries::state::checking::is_type_parameter_like(self.ctx.types, type_id)
            && crate::query_boundaries::common::type_parameter_has_mapped_constraint(
                self.ctx.types,
                type_id,
            )
        {
            return;
        }

        // Suppress TS2339 when the type is an intersection containing type parameters
        // that haven't been resolved yet. This commonly occurs with mixin patterns where
        // the return type is `Constructor<Tagged> & T` - the instance type should have
        // properties from both sides of the intersection, but we may not resolve them
        // properly when T is still generic.
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
        {
            let has_unresolved_type_param = members.iter().any(|&member| {
                crate::query_boundaries::state::checking::is_type_parameter_like(
                    self.ctx.types,
                    member,
                ) || crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    member,
                )
            });
            if has_unresolved_type_param {
                return;
            }
        }

        // Suppress TS2339 for types that contain conditional types which may resolve
        // to have the property once the type parameters are instantiated.
        // For example: `FirstParameter<typeof h>["foo"]` where `FirstParameter<T>` is
        // `T extends (x: infer P) => unknown ? P : unknown`. When the conditional type
        // cannot be resolved (e.g., during generic inference), tsc defers the check
        // rather than emitting a false TS2339.
        if crate::query_boundaries::common::contains_conditional_type(self.ctx.types, type_id) {
            return;
        }

        // Suppress TS2339 for indexed access types on unresolved generic conditional types.
        // For example, `FirstParameter<typeof h>['foo']` where `FirstParameter<T>` is
        // `T extends (x: infer P) => unknown ? P : unknown`. When the conditional type
        // argument is generic and cannot be resolved (e.g., during inference),
        // tsc defers the check rather than emitting a false TS2339.
        // This covers cases where the base type is an indexed access type whose
        // object type is an unresolved conditional type.
        if let Some(indexed_info) =
            crate::query_boundaries::common::get_indexed_access_type(self.ctx.types, type_id)
            && (crate::query_boundaries::common::contains_conditional_type(
                self.ctx.types,
                indexed_info.object_type,
            ) || crate::query_boundaries::common::contains_type_parameters(
                self.ctx.types,
                indexed_info.object_type,
            ))
        {
            return;
        }

        // Suppress TS2339 for types that are the result of inference-based conditional
        // types that haven't been resolved yet. This commonly occurs with patterns like
        // `type X = FirstParameter<typeof h>['foo']` where `h` is a generic function
        // and the conditional type cannot be resolved until inference completes.
        if crate::query_boundaries::common::type_is_conditional_type_result_with_unresolved_inference(
            self.ctx.types,
            type_id,
        )
        {
            return;
        }

        // Suppress TS2339 for type parameters constrained to generic functions.
        // For example, in `const h = f(g)` where `f` and `g` are generic,
        // the inferred type of `h` may contain unresolved type parameters from
        // the conditional type inference that cannot be checked for property access.
        if crate::query_boundaries::state::checking::is_type_parameter_like(self.ctx.types, type_id)
        {
            let constraint =
                crate::query_boundaries::common::type_parameter_constraint(self.ctx.types, type_id);
            if let Some(constraint_type) = constraint {
                // Only suppress if the constraint is unknown (unresolved) or contains
                // conditional types or type parameters.
                if constraint_type == TypeId::UNKNOWN
                    || crate::query_boundaries::common::contains_conditional_type(
                        self.ctx.types,
                        constraint_type,
                    )
                    || crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        constraint_type,
                    )
                {
                    return;
                }
            }
            // Note: We do NOT suppress for unconstrained type parameters with no constraint.
            // These should still report TS2339 for property access failures as tsc does.
        }

        // Suppress TS2339 for types that are intersections involving generic conditional types.
        // For example, `{ foo: T } & (T extends string ? { bar: string } : { baz: number })`
        // where the conditional type part may or may not have the property being accessed.
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
        {
            let has_conditional_or_type_param = members.iter().any(|&member| {
                crate::query_boundaries::common::contains_conditional_type(self.ctx.types, member)
                    || crate::query_boundaries::state::checking::is_type_parameter_like(
                        self.ctx.types,
                        member,
                    )
                    || crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        member,
                    )
            });
            if has_conditional_or_type_param {
                return;
            }
        }

        if self
            .resolve_diagnostic_anchor(idx, DiagnosticAnchorKind::PropertyToken)
            .is_some()
        {
            // TS2550: Check if property exists in a newer lib version before
            // trying spelling suggestions. This matches tsc's priority order.
            if !self.has_syntax_parse_errors()
                && let Some((lib_name, override_type_name)) =
                    self.get_lib_suggestion_for_property_with_node(prop_name, type_id, idx)
            {
                let type_str = if let Some(name) = override_type_name {
                    name.to_string()
                } else {
                    self.property_receiver_display_for_node(type_id, idx)
                };
                let message = format!(
                    "Property '{prop_name}' does not exist on type '{type_str}'. Do you need to change your target library? Try changing the 'lib' compiler option to '{lib_name}' or later."
                );
                self.error_at_anchor(
                    idx,
                    DiagnosticAnchorKind::PropertyToken,
                    &message,
                    diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CH,
                );
                return;
            }

            // On files with syntax parse errors, TypeScript generally avoids TS2551
            // suggestion diagnostics and sticks with TS2339 to reduce cascades.
            let suggestion = if self.has_syntax_parse_errors() {
                None
            } else {
                self.find_similar_property(prop_name, type_id)
            };

            // For namespace types, override the type display to match TSC's
            // `typeof import("module")` format instead of the literal object shape.
            //
            // Exception: when the receiver is a CJS module whose
            // `module.exports = <callable>` produces a merged-callable apparent
            // type, tsc displays the structural form (`{ (): void; blah: any; }`)
            // rather than the namespace alias. The receiver Object cached in
            // `namespace_module_names` may be a stale snapshot from an early-call
            // race in `infer_commonjs_export_rhs_type` (returned UNDEFINED before
            // the function expression was typed). Force a fresh recompute of
            // the file's JS export surface — bypassing the resolution-set guard
            // and the cache — and, when it now yields a different callable
            // type, format that structural shape (with ERROR properties
            // rewritten to ANY for parity with tsc's display policy) instead
            // of the alias.
            //
            // See `compiler/pushTypeGetTypeOfAlias.ts` for the symptom and
            // `memory/project_pushTypeGetTypeOfAlias_modulenamespace_display.md`
            // for the iter-20/22/24/28/30/32 investigation trail.
            if self.ctx.namespace_module_names.contains_key(&type_id) {
                let recomputed_surface_type = {
                    let current_file_idx = self.ctx.current_file_idx;
                    self.ctx.js_export_surface_cache.remove(&current_file_idx);
                    let was_in_resolution = self
                        .ctx
                        .js_export_surface_resolution_set
                        .remove(&current_file_idx);
                    let result = self.js_export_surface_namespace_type(current_file_idx);
                    if was_in_resolution {
                        self.ctx
                            .js_export_surface_resolution_set
                            .insert(current_file_idx);
                    }
                    result
                };
                if let Some(merged_ty) = recomputed_surface_type
                    && merged_ty != type_id
                    && crate::query_boundaries::common::has_call_signatures(
                        self.ctx.types.as_type_database(),
                        merged_ty,
                    )
                {
                    let merged_for_display = self
                        .substitute_error_with_any_in_callable_shape(merged_ty)
                        .unwrap_or(merged_ty);
                    let type_str = self.format_type(merged_for_display);
                    let (code, message) = if let Some(ref suggestion) = suggestion {
                        (
                            diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN,
                            format!(
                                "Property '{prop_name}' does not exist on type '{type_str}'. Did you mean '{suggestion}'?"
                            ),
                        )
                    } else {
                        (
                            diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                            format!("Property '{prop_name}' does not exist on type '{type_str}'."),
                        )
                    };
                    self.error_at_anchor(idx, DiagnosticAnchorKind::PropertyToken, &message, code);
                    return;
                }
            }
            if let Some(module_name) = self.ctx.namespace_module_names.get(&type_id).cloned() {
                if let Some(members) =
                    crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
                    && let Some(display_member) = members.into_iter().find(|&member| {
                        !self.ctx.namespace_module_names.contains_key(&member)
                            && !crate::query_boundaries::js_exports::commonjs_direct_export_supports_named_props(
                                self.ctx.types,
                                member,
                            )
                    })
                {
                    let type_str = self.format_type(display_member);
                    let (code, message) = if let Some(ref suggestion) = suggestion {
                        (
                            diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN,
                            format!(
                                "Property '{prop_name}' does not exist on type '{type_str}'. Did you mean '{suggestion}'?"
                            ),
                        )
                    } else {
                        (
                            diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                            format!("Property '{prop_name}' does not exist on type '{type_str}'."),
                        )
                    };
                    self.error_at_anchor(idx, DiagnosticAnchorKind::PropertyToken, &message, code);
                    return;
                }

                // Normalize module specifier: TSC displays resolved module names
                // without the relative path prefix (e.g., "./b" → "b").
                let display_name = module_name.strip_prefix("./").unwrap_or(&module_name);
                let display_name = strip_property_namespace_module_extension(display_name);
                let type_str = format!("typeof import(\"{display_name}\")");
                let (code, message) = if let Some(ref suggestion) = suggestion {
                    (
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN,
                        format!(
                            "Property '{prop_name}' does not exist on type '{type_str}'. Did you mean '{suggestion}'?"
                        ),
                    )
                } else {
                    (
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                        format!("Property '{prop_name}' does not exist on type '{type_str}'."),
                    )
                };
                self.error_at_anchor(idx, DiagnosticAnchorKind::PropertyToken, &message, code);
                return;
            }

            // For enum container types (e.g., `U8.nonExistent`), tsc displays
            // "typeof EnumName" for the type in the error message.
            if let Some(def_id) =
                crate::query_boundaries::common::enum_def_id(self.ctx.types, type_id)
                && let Some(sym_id) = self.ctx.def_to_symbol_id(def_id)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            {
                let enum_name = &symbol.escaped_name;
                let type_str = format!("typeof {enum_name}");
                let (code, message) = if let Some(ref suggestion) = suggestion {
                    (
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN,
                        format!(
                            "Property '{prop_name}' does not exist on type '{type_str}'. Did you mean '{suggestion}'?"
                        ),
                    )
                } else {
                    (
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                        format!("Property '{prop_name}' does not exist on type '{type_str}'."),
                    )
                };
                self.error_at_anchor(idx, DiagnosticAnchorKind::PropertyToken, &message, code);
                return;
            }

            // For namespace/module value types (e.g., `namespace M { ... }`), tsc displays
            // "typeof NamespaceName" for the type in the error message.
            if let Some(name) = self.get_namespace_typeof_name(type_id) {
                let type_str = format!("typeof {name}");
                let (code, message) = if let Some(ref suggestion) = suggestion {
                    (
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN,
                        format!(
                            "Property '{prop_name}' does not exist on type '{type_str}'. Did you mean '{suggestion}'?"
                        ),
                    )
                } else {
                    (
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                        format!("Property '{prop_name}' does not exist on type '{type_str}'."),
                    )
                };
                self.error_at_anchor(idx, DiagnosticAnchorKind::PropertyToken, &message, code);
                return;
            }

            // TS2812: If the type name matches a known DOM global and the type is
            // structurally empty, suggest including the 'dom' lib option.
            if suggestion.is_none() && self.should_suggest_dom_lib_for_type(type_id) {
                let type_display = self.property_receiver_display_for_node(type_id, idx);
                let message = format!(
                    "Property '{prop_name}' does not exist on type '{type_display}'. Try changing the 'lib' compiler option to include 'dom'."
                );
                self.error_at_anchor(
                    idx,
                    DiagnosticAnchorKind::PropertyToken,
                    &message,
                    diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_TRY_CHANGING_THE_LIB_COMPILER_OPTION_TO_INCLUDE,
                );
                return;
            }

            let type_display = self.property_receiver_display_for_node(type_id, idx);
            let (code, message) = if let Some(ref suggestion) = suggestion {
                (
                    diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN,
                    format!(
                        "Property '{prop_name}' does not exist on type '{type_display}'. Did you mean '{suggestion}'?"
                    ),
                )
            } else {
                (
                    diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                    format!("Property '{prop_name}' does not exist on type '{type_display}'."),
                )
            };
            self.error_at_anchor(idx, DiagnosticAnchorKind::PropertyToken, &message, code);
        }
    }
}
