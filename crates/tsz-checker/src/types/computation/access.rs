//! Element access and optional chain detection.
//!
//! Super keyword computation lives in `access_super`.
//! Await expression computation lives in `access_await`.
//! Index type helpers live in `access_helpers`.

use crate::context::TypingRequest;
use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// Checks if a node is an optional chain expression (`?.`).
///
/// Handles property access (`o?.b`), element access (`o?.[0]`), and
/// call expressions (`o?.b()` / `o.b?.()`).
pub(crate) fn is_optional_chain(arena: &NodeArena, idx: NodeIndex) -> bool {
    let Some(node) = arena.get(idx) else {
        return false;
    };

    match node.kind {
        k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
        {
            if let Some(access) = arena.get_access_expr(node) {
                access.question_dot_token
            } else {
                false
            }
        }
        k if k == syntax_kind_ext::CALL_EXPRESSION => {
            // Check if this call is part of an optional chain.
            // A call can be optional in two ways:
            // 1. The callee itself is optional: `o?.b()` -> callee `o?.b` has question_dot_token
            // 2. The call has an optional token: `o.b?.()` -> call node has OPTIONAL_CHAIN flag
            if (node.flags as u32) & tsz_parser::parser::node_flags::OPTIONAL_CHAIN != 0 {
                return true;
            }
            if let Some(call) = arena.get_call_expr(node) {
                is_optional_chain(arena, call.expression)
            } else {
                false
            }
        }
        _ => false,
    }
}

impl<'a> CheckerState<'a> {
    fn expando_element_key_name(&mut self, key_expr_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(key_expr_idx)?;
        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                let ident = self.ctx.arena.get_identifier(node)?;
                let name = &ident.escaped_text;

                // Resolve through the binder the same way detect_expando_assignment
                // does, so the key matches what was stored at bind time.
                let binder_sym = self
                    .ctx
                    .binder
                    .get_node_symbol(key_expr_idx)
                    .or_else(|| {
                        self.ctx
                            .binder
                            .resolve_identifier(self.ctx.arena, key_expr_idx)
                    })
                    .or_else(|| self.ctx.binder.file_locals.get(name));
                if let Some(sym_id) = binder_sym
                    && let Some(key) = self.resolved_const_expando_key_from_binder(sym_id, 0)
                {
                    return Some(key);
                }

                // Fallback: resolve through the type system for non-binder cases.
                let prev = self.ctx.preserve_literal_types;
                self.ctx.preserve_literal_types = true;
                let key_type = self.get_type_of_node(key_expr_idx);
                self.ctx.preserve_literal_types = prev;

                if let Some(lit) = tsz_solver::visitor::literal_value(self.ctx.types, key_type) {
                    return Some(match lit {
                        tsz_solver::LiteralValue::String(s) => self.ctx.types.resolve_atom(s),
                        tsz_solver::LiteralValue::Number(n) => n.0.to_string(),
                        tsz_solver::LiteralValue::Boolean(b) => b.to_string(),
                        tsz_solver::LiteralValue::BigInt(b) => self.ctx.types.resolve_atom(b),
                    });
                }

                if let Some(sym_ref) =
                    tsz_solver::visitor::unique_symbol_ref(self.ctx.types, key_type)
                {
                    return Some(format!("__unique_{}", sym_ref.0));
                }

                Some(name.clone())
            }
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                self.ctx.arena.get_literal(node).map(|lit| lit.text.clone())
            }
            _ => None,
        }
    }

    fn is_expando_element_access_read(
        &mut self,
        object_expr_idx: NodeIndex,
        key_expr_idx: NodeIndex,
    ) -> bool {
        fn property_access_chain(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
            let node = arena.get(idx)?;
            if node.kind == SyntaxKind::Identifier as u16 {
                return arena.get_identifier(node).map(|id| id.escaped_text.clone());
            }
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
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
        let Some(prop_key) = self.expando_element_key_name(key_expr_idx) else {
            return false;
        };

        if self
            .ctx
            .binder
            .expando_properties
            .get(&obj_key)
            .is_some_and(|props| props.contains(&prop_key))
        {
            return true;
        }

        // Use global expando index for O(1) lookup instead of O(N) binder scan
        if let Some(expando_idx) = &self.ctx.global_expando_index {
            if expando_idx
                .get(&obj_key)
                .is_some_and(|props| props.contains(&prop_key))
            {
                return true;
            }
        } else if let Some(all_binders) = &self.ctx.all_binders {
            for binder in all_binders.iter() {
                if binder
                    .expando_properties
                    .get(&obj_key)
                    .is_some_and(|props| props.contains(&prop_key))
                {
                    return true;
                }
            }
        }

        false
    }

    /// Get the type of an element access expression (e.g., arr[0], obj["prop"]).
    ///
    /// Handles element access with optional chaining, index signatures,
    /// and nullish coalescing.
    #[allow(dead_code)]
    pub(crate) fn get_type_of_element_access(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_element_access_with_request(idx, &TypingRequest::NONE)
    }

    pub(crate) fn get_type_of_element_access_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        use crate::query_boundaries::common::PropertyAccessResult;
        let skip_flow_narrowing = request.flow.skip_flow_narrowing();
        let read_request = request.read().normal_origin().contextual_opt(None);

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return TypeId::ERROR;
        };

        // In parse-recovery cases like `number[]`, the bracket argument is
        // missing and the parser already reports TS1011. Don't additionally
        // emit TS2693 here — the parse error is sufficient and tsc doesn't
        // emit TS2693 in this case. Just return ERROR to prevent cascading.
        if access.name_or_argument.is_none() {
            return TypeId::ERROR;
        }

        let literal_string = self.get_literal_string_from_node(access.name_or_argument);
        let numeric_string_index = literal_string
            .as_deref()
            .and_then(|name| self.get_numeric_index_from_string(name));
        let literal_index = self
            .get_literal_index_from_node(access.name_or_argument)
            .or(numeric_string_index);

        // Get the type of the object. In write context, prefer the receiver's
        // declared type when it already has the indexed member, otherwise fall
        // back to the flow-narrowed receiver so subtype-based writes still work.
        let (object_type, write_presence_only) = if skip_flow_narrowing {
            let object_type_no_flow =
                self.get_type_of_write_target_base_expression(access.expression);
            let evaluated_no_flow = self.evaluate_application_type(object_type_no_flow);
            let resolved_no_flow = self.resolve_type_for_property_access(evaluated_no_flow);
            let can_use_no_flow = if let Some(name) = literal_string.as_deref() {
                !matches!(
                    self.resolve_property_access_with_env(resolved_no_flow, name),
                    PropertyAccessResult::PropertyNotFound { .. } | PropertyAccessResult::IsUnknown
                )
            } else if literal_index.is_some() {
                self.get_element_access_type(resolved_no_flow, TypeId::NUMBER, literal_index)
                    != TypeId::ERROR
            } else {
                false
            };
            let chosen = if can_use_no_flow {
                let read_object_type =
                    self.get_type_of_node_with_request(access.expression, &read_request);
                if let Some(name) = literal_string.as_deref() {
                    let evaluated_read = self.evaluate_application_type(read_object_type);
                    let resolved_read = self.resolve_type_for_property_access(evaluated_read);
                    if self.union_write_requires_existing_named_member(resolved_read, name) {
                        (read_object_type, false)
                    } else {
                        let read_has_property = !matches!(
                            self.resolve_property_access_with_env(resolved_read, name),
                            PropertyAccessResult::PropertyNotFound { .. }
                                | PropertyAccessResult::IsUnknown
                        );
                        (object_type_no_flow, !read_has_property)
                    }
                } else if literal_index.is_some() {
                    let evaluated_read = self.evaluate_application_type(read_object_type);
                    let resolved_read = self.resolve_type_for_property_access(evaluated_read);
                    let read_has_property =
                        self.get_element_access_type(resolved_read, TypeId::NUMBER, literal_index)
                            != TypeId::ERROR;
                    (object_type_no_flow, !read_has_property)
                } else {
                    (object_type_no_flow, false)
                }
            } else {
                (
                    self.get_type_of_node_with_request(access.expression, &read_request),
                    false,
                )
            };
            (self.evaluate_application_type(chosen.0), chosen.1)
        } else {
            let object_type = self.get_type_of_node_with_request(access.expression, &read_request);
            (self.evaluate_application_type(object_type), false)
        };

        // Handle optional chain continuations: for `o?.b["c"]`, when processing `["c"]`,
        // the object type from `o?.b` includes `undefined`. Strip nullish types when this
        // element access is a continuation of an optional chain.
        let object_type =
            if !access.question_dot_token && is_optional_chain(self.ctx.arena, access.expression) {
                let (non_nullish, _) = self.split_nullish_type(object_type);
                non_nullish.unwrap_or(object_type)
            } else {
                object_type
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

        // Save the pre-resolution object type. When the object is a type parameter,
        // resolve_type_for_property_access replaces it with its constraint. But for
        // generic indexed access (e.g., U[keyof T] where U extends T), we need to
        // keep the original type parameter to produce the correct deferred type.
        //
        // Instance `this[K]` writes in class members need the same preservation:
        // the expression `this` evaluates to the concrete class instance type, but
        // generic writes like `this[key] = value` should still target deferred
        // `this[K]` so the polymorphic `this` relationship survives assignability.
        let pre_resolution_object_type = if self.is_this_expression(access.expression)
            && self.ctx.enclosing_class.is_some()
            && !self.is_this_in_nested_function_inside_class(idx)
            && !self.is_this_in_static_class_member(idx)
        {
            self.ctx.types.this_type()
        } else {
            object_type
        };

        let is_this_global = self.is_this_resolving_to_global(access.expression);
        if let Some(name) = literal_string.as_deref()
            && (self.is_global_this_like_expression(access.expression) || is_this_global)
        {
            // For element access (globalThis['y']), tsc reports TS2339 at the full
            // expression span. For property access (globalThis.y), at the property name.
            let error_node = if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
                idx
            } else {
                access.name_or_argument
            };
            let property_type = self.resolve_global_this_property_type(name, error_node);
            if property_type == TypeId::ERROR {
                return TypeId::ERROR;
            }
            // TS7053: When noImplicitAny is enabled and `this` (resolving to typeof
            // globalThis) is used with bracket access and the property is not found,
            // emit the can't-index diagnostic. Only for `this` — the `globalThis`
            // identifier path may return ANY for unresolved properties that exist
            // in lib declarations.
            if is_this_global
                && property_type == TypeId::ANY
                && self.ctx.no_implicit_any()
                && !self.is_js_file()
                && node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
                let index_str = format!("\"{name}\"");
                self.error_at_node(
                    idx,
                    &format_message(
                        diagnostic_messages::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN,
                        &[&index_str, "typeof globalThis"],
                    ),
                    diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN,
                );
            }
            return if skip_flow_narrowing {
                property_type
            } else {
                self.apply_flow_narrowing(idx, property_type)
            };
        }

        if self.report_namespace_value_access_for_type_only_import_equals_expr(access.expression) {
            return TypeId::ERROR;
        }

        // Don't report errors for any/error types - check BEFORE accessibility
        // to prevent cascading errors when the object type is already invalid.
        // Exception: top-level JS `this[expr] = value` should still report TS7053
        // when the key is not a simple expando-trackable declaration form.
        if object_type == TypeId::ANY
            && skip_flow_narrowing
            && self.ctx.is_js_file()
            && is_this_global
            && node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            && self.ctx.no_implicit_any()
            && self
                .expando_element_key_name(access.name_or_argument)
                .is_none()
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

            let prev_preserve = self.ctx.preserve_literal_types;
            self.ctx.preserve_literal_types = true;
            let index_type =
                self.get_type_of_node_with_request(access.name_or_argument, &read_request);
            self.ctx.preserve_literal_types = prev_preserve;

            self.error_at_node(
                idx,
                &format_message(
                    diagnostic_messages::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN,
                    &[&self.format_type(index_type), "typeof globalThis"],
                ),
                diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN,
            );
            return TypeId::ANY;
        }

        if object_type == TypeId::ANY {
            return TypeId::ANY;
        }
        if object_type == TypeId::ERROR {
            return TypeId::ERROR;
        }

        let object_type = self.resolve_type_for_property_access(object_type);
        if object_type == TypeId::ANY {
            return TypeId::ANY;
        }
        if object_type == TypeId::ERROR {
            return TypeId::ERROR;
        }
        // Element access on `never` returns `never` (bottom type propagation).
        if object_type == TypeId::NEVER {
            return TypeId::NEVER;
        }

        let (object_type_for_access, nullish_cause) = self.split_nullish_type(object_type);
        let Some(object_type_for_access) = object_type_for_access else {
            if access.question_dot_token {
                return TypeId::UNDEFINED;
            }
            if let Some(cause) = nullish_cause {
                // Type is entirely nullish - emit TS18050 "The value X cannot be used here"
                self.report_nullish_object(access.expression, cause, true);
            }
            return TypeId::ERROR;
        };

        // Type is possibly nullish (e.g., Foo | undefined) - emit TS18048/TS2532
        // unless optional chaining is used
        if let Some(cause) = nullish_cause
            && !access.question_dot_token
        {
            self.report_nullish_object(access.expression, cause, false);
        }

        let prev_preserve = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;
        let index_type = self.get_type_of_node_with_request(access.name_or_argument, &read_request);
        self.ctx.preserve_literal_types = prev_preserve;

        // Preserve the write target when the index expression already errored.
        if index_type == TypeId::ERROR {
            if skip_flow_narrowing
                && let Some(recovered_type) = self
                    .recover_assignment_target_type_for_errored_element_index(
                        object_type_for_access,
                        access.name_or_argument,
                    )
            {
                return recovered_type;
            }
            return TypeId::ERROR;
        }

        // TS2538: Type cannot be used as an index type
        // Resolve Lazy types (interfaces, classes, type aliases) before checking
        // indexability. Lazy types remain as TypeData::Lazy(DefId) in the solver's
        // type interner, but they may resolve to object types which are invalid
        // index types. Without resolution, `obj[x]` where `x: SomeInterface`
        // would silently skip the TS2538 check.
        let resolved_index = self.resolve_lazy_type(index_type);
        if let Some(invalid_member) = self.type_get_invalid_index_type_member(resolved_index) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let index_type_str = self.format_type(invalid_member);
            let message = format_message(
                diagnostic_messages::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                &[&index_type_str],
            );
            self.error_at_node(
                access.name_or_argument,
                &message,
                diagnostic_codes::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
            );
            return TypeId::ERROR;
        }

        // In write context, preserve `T[keyof T]` / `T[K]` on generic receivers
        // before resolving through the receiver's constraint. Otherwise the
        // write target collapses to the constraint's index-signature value type
        // (e.g. `number`) and incorrectly accepts writes that should produce
        // generic TS2322 errors.
        let is_generic_receiver =
            tsz_solver::visitor::is_type_parameter(self.ctx.types, pre_resolution_object_type)
                || tsz_solver::visitor::is_this_type(self.ctx.types, pre_resolution_object_type);
        if skip_flow_narrowing
            && is_generic_receiver
            && self.is_valid_index_for_type_param(index_type, pre_resolution_object_type)
        {
            return self
                .ctx
                .types
                .factory()
                .index_access(pre_resolution_object_type, index_type);
        }

        // TS2476: A const enum member can only be accessed using a string literal.
        let const_enum_sym = self
            .resolve_identifier_symbol(access.expression)
            .map(|sym_id| {
                self.resolve_alias_symbol(sym_id, &mut Vec::new())
                    .unwrap_or(sym_id)
            })
            .or_else(|| {
                self.resolve_qualified_symbol(access.expression)
                    .map(|sym_id| {
                        self.resolve_alias_symbol(sym_id, &mut Vec::new())
                            .unwrap_or(sym_id)
                    })
            })
            .filter(|&sym_id| self.is_const_enum_symbol(sym_id))
            .or_else(|| {
                self.enum_symbol_from_type(object_type_for_access)
                    .filter(|&sym_id| self.is_const_enum_symbol(sym_id))
            });

        if const_enum_sym.is_some() {
            let arg_is_string_literal =
                self.ctx
                    .arena
                    .get(access.name_or_argument)
                    .is_some_and(|arg_node| {
                        arg_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
                            || arg_node.kind
                                == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16
                    });
            if !arg_is_string_literal {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    access.name_or_argument,
                    diagnostic_messages::A_CONST_ENUM_MEMBER_CAN_ONLY_BE_ACCESSED_USING_A_STRING_LITERAL,
                    diagnostic_codes::A_CONST_ENUM_MEMBER_CAN_ONLY_BE_ACCESSED_USING_A_STRING_LITERAL,
                );
                return TypeId::ERROR;
            }
        }

        if let Some(index_value) = self
            .get_number_value_from_element_index(access.name_or_argument)
            .or_else(|| {
                tsz_solver::type_queries::get_number_literal_value(self.ctx.types, index_type)
            })
            && index_value.is_finite()
            && index_value.fract() == 0.0
            && index_value < 0.0
        {
            let object_for_tuple_check = {
                let unwrapped = crate::query_boundaries::common::unwrap_readonly(
                    self.ctx.types,
                    object_type_for_access,
                );
                self.resolve_lazy_type(unwrapped)
            };
            let object_for_tuple_check = crate::query_boundaries::common::unwrap_readonly(
                self.ctx.types,
                object_for_tuple_check,
            );
            if tsz_solver::type_queries::is_tuple_type(self.ctx.types, object_for_tuple_check) {
                self.error_at_node(
                    access.name_or_argument,
                    crate::diagnostics::diagnostic_messages::A_TUPLE_TYPE_CANNOT_BE_INDEXED_WITH_A_NEGATIVE_VALUE,
                    crate::diagnostics::diagnostic_codes::A_TUPLE_TYPE_CANNOT_BE_INDEXED_WITH_A_NEGATIVE_VALUE,
                );
                return TypeId::ERROR;
            }
        }

        let literal_string_is_none = literal_string.is_none();

        let mut result_type = None;
        let mut report_no_index = false;
        let mut use_index_signature_check = true;

        if let Some(name) = literal_string.as_deref() {
            if self
                .report_namespace_value_access_for_type_only_import_equals_expr(access.expression)
            {
                return TypeId::ERROR;
            }

            // For merged class/function/enum + namespace symbols, literal element
            // access should see exported namespace members just like property access.
            // In write context (skip_flow_narrowing), skip these shortcuts:
            // they return the symbol's read type, which doesn't account for
            // divergent getter/setter types. The full property access path
            // below correctly uses write_type for setter parameters.
            if !skip_flow_narrowing {
                if let Some(expr_node) = self.ctx.arena.get(access.expression)
                    && let Some(expr_ident) = self.ctx.arena.get_identifier(expr_node)
                {
                    let expr_name = &expr_ident.escaped_text;
                    if let Some(sym_id) = self.ctx.binder.file_locals.get(expr_name)
                        && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                    {
                        let is_merged = (symbol.flags & symbol_flags::MODULE) != 0
                            && (symbol.flags
                                & (symbol_flags::CLASS
                                    | symbol_flags::FUNCTION
                                    | symbol_flags::REGULAR_ENUM))
                                != 0;

                        if is_merged
                            && let Some(exports) = symbol.exports.as_ref()
                            && let Some(member_id) = exports.get(name)
                        {
                            result_type = Some(self.get_type_of_symbol(member_id));
                            use_index_signature_check = false;
                        }
                    }
                }

                if let Some(member_type) =
                    self.resolve_namespace_value_member(object_type_for_access, name)
                {
                    result_type = Some(member_type);
                    use_index_signature_check = false;
                }
            }

            if result_type.is_none()
                && self.namespace_has_type_only_member(object_type_for_access, name)
            {
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
                    if let Some(ns_name) = self.entity_name_text(access.expression) {
                        self.report_wrong_meaning_diagnostic(
                            &ns_name,
                            access.expression,
                            crate::query_boundaries::name_resolution::NameLookupKind::Namespace,
                        );
                    }
                    // Also emit TS2693 for the type-only member itself
                    self.report_wrong_meaning_diagnostic(
                        name,
                        access.name_or_argument,
                        crate::query_boundaries::name_resolution::NameLookupKind::Type,
                    );
                }
                return TypeId::ERROR;
            }
        }

        if !skip_flow_narrowing
            && self.is_expando_element_access_read(access.expression, access.name_or_argument)
        {
            if let Some(prop_name) = self.expando_element_key_name(access.name_or_argument)
                && let Some(expando_type) =
                    self.expando_property_read_type(idx, access.expression, &prop_name)
            {
                return expando_type;
            }
            return TypeId::ANY;
        }
        if self.is_jsdoc_annotated_this_member_declaration(idx) {
            return TypeId::ANY;
        }
        let union_keys = self.get_literal_key_union_from_type(index_type);
        // Track missing literal keys for TS2339 reporting (instead of TS7053).
        let mut missing_literal_keys: Vec<String> = Vec::new();
        if result_type.is_none()
            && literal_index.is_none()
            && let Some((string_keys, number_keys)) = union_keys
        {
            let total_keys = string_keys.len() + number_keys.len();
            // Non-integer numeric literals (e.g., 1.1, -1) should be resolved as
            // property names, not via index signatures. Skip this block when all
            // keys are non-indexable numbers so the property name handler below
            // can process them (e.g., c[1.1] where C has property `1.1: string`).
            let all_non_indexable_numbers = string_keys.is_empty()
                && !number_keys.is_empty()
                && number_keys
                    .iter()
                    .all(|&n| self.get_numeric_index_from_number(n).is_none());
            if (total_keys > 1 || literal_string_is_none) && !all_non_indexable_numbers {
                let mut types = Vec::new();
                let mut string_keys_ok = true;
                let mut number_keys_ok = true;

                if !string_keys.is_empty() {
                    let keys_result = self.get_element_access_type_for_literal_keys(
                        object_type_for_access,
                        &string_keys,
                        skip_flow_narrowing,
                    );
                    if let Some(result) = keys_result.result_type {
                        types.push(result);
                    }
                    if !keys_result.missing_keys.is_empty() {
                        string_keys_ok = false;
                        if keys_result.result_type.is_none() {
                            // ALL keys missing — fall through to normal TS7053 /
                            // expando suppression path (don't emit per-key TS2339).
                            report_no_index = true;
                        } else {
                            // SOME keys found, some missing — emit TS2339 per
                            // missing key below (e.g., 'z' in 'a' | 'b' | 'z').
                            missing_literal_keys = keys_result.missing_keys;
                        }
                    }
                }

                if !number_keys.is_empty() {
                    match self.get_element_access_type_for_literal_number_keys(
                        object_type_for_access,
                        &number_keys,
                        skip_flow_narrowing,
                    ) {
                        Some(result) => types.push(result),
                        None => {
                            number_keys_ok = false;
                            report_no_index = true;
                        }
                    }
                }

                // Suppress index signature checks when literal keys were
                // resolved (fully or partially). When all keys resolve, there
                // is no error. When some keys are missing, we emit TS2339 per
                // missing key below — the generic TS7053 check must not run.
                if (string_keys_ok && number_keys_ok) || !missing_literal_keys.is_empty() {
                    use_index_signature_check = false;
                }

                if report_no_index {
                    result_type = Some(TypeId::ANY);
                } else if !types.is_empty() {
                    // In write context, intersect the results from string and number
                    // keys — the assigned value must satisfy all possible key types.
                    result_type = Some(if skip_flow_narrowing {
                        let intersection =
                            tsz_solver::utils::intersection_or_single(self.ctx.types, types);
                        self.evaluate_type_with_env(intersection)
                    } else {
                        tsz_solver::utils::union_or_single(self.ctx.types, types)
                    });
                }
            }
        }

        if result_type.is_none()
            && let Some(property_name) = self.get_literal_string_from_node(access.name_or_argument)
            && numeric_string_index.is_none()
        {
            // Resolve type references (Ref, TypeQuery, etc.) before property access lookup
            let resolved_type = self.resolve_type_for_property_access(object_type_for_access);
            let result = self.resolve_property_access_with_env(resolved_type, &property_name);
            result_type = match result {
                PropertyAccessResult::Success {
                    type_id,
                    write_type,
                    ..
                } => {
                    if skip_flow_narrowing
                        && self.union_write_requires_existing_named_member(
                            resolved_type,
                            &property_name,
                        )
                    {
                        None
                    } else {
                        use_index_signature_check = false;
                        // In write context (assignment target), prefer the setter type.
                        Some(effective_write_result(type_id, write_type))
                    }
                }
                PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                    use_index_signature_check = false;
                    // Use ERROR instead of UNKNOWN to prevent TS2571 errors
                    Some(property_type.unwrap_or(TypeId::ERROR))
                }
                PropertyAccessResult::IsUnknown => {
                    use_index_signature_check = false;
                    // TS18046: 'x' is of type 'unknown'.
                    // Without strictNullChecks, unknown is treated like any.
                    if self.error_is_of_type_unknown(access.expression) {
                        Some(TypeId::ERROR)
                    } else {
                        Some(TypeId::ANY)
                    }
                }
                PropertyAccessResult::PropertyNotFound { .. } => {
                    // TS2576 parity for element access on instance/super with a static member name.
                    // Use the shared class summary so inherited static fields/accessors
                    // don't rewalk the base chain at each access.
                    if self.is_super_expression(access.expression)
                        && let Some(ref class_info) = self.ctx.enclosing_class
                        && let Some(base_idx) = self.get_base_class_idx(class_info.class_idx)
                        && self
                            .summarize_class_chain(base_idx)
                            .lookup(&property_name, true, true)
                            .is_some()
                    {
                        use crate::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };

                        let base_name = self.get_class_name_from_decl(base_idx);
                        let static_member_name = format!("{base_name}.{property_name}");
                        let object_type_str = self.format_type(object_type);
                        let message = format_message(
                            diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN_TO_ACCESS_THE_STATIC_MEMBER_INSTEAD,
                            &[&property_name, &object_type_str, &static_member_name],
                        );
                        self.error_at_node(
                            access.name_or_argument,
                            &message,
                            diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN_TO_ACCESS_THE_STATIC_MEMBER_INSTEAD,
                        );
                        use_index_signature_check = false;
                        Some(TypeId::ERROR)
                    } else if !self.is_super_expression(access.expression)
                        && let Some((class_idx, is_static_access)) =
                            self.resolve_class_for_access(access.expression, object_type_for_access)
                        && !is_static_access
                        && self
                            .summarize_class_chain(class_idx)
                            .lookup(&property_name, true, true)
                            .is_some()
                    {
                        use crate::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };

                        let object_type_str =
                            self.format_type_for_assignability_message(object_type);
                        let static_member_name = format!("{object_type_str}.{property_name}");
                        let message = format_message(
                            diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN_TO_ACCESS_THE_STATIC_MEMBER_INSTEAD,
                            &[&property_name, &object_type_str, &static_member_name],
                        );
                        self.error_at_node(
                            access.name_or_argument,
                            &message,
                            diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN_TO_ACCESS_THE_STATIC_MEMBER_INSTEAD,
                        );
                        use_index_signature_check = false;
                        Some(TypeId::ERROR)
                    } else {
                        // TS2339 parity for element access on `typeof const enum` with a missing
                        // string-literal member. Const enums do not have reverse mappings, so they
                        // should not fall back to TS7053 string-index diagnostics.
                        if const_enum_sym.is_some() {
                            self.error_property_not_exist_at(
                                &property_name,
                                object_type_for_access,
                                access.name_or_argument,
                            );
                            use_index_signature_check = false;
                            Some(TypeId::ERROR)
                        } else {
                            // CRITICAL FIX: Don't immediately return ANY when property is not found.
                            // Let it fall through to check for index signatures below.
                            // This allows map["foo"] to work when map has [key: string]: boolean
                            None
                        }
                    }
                }
            };
        }

        if result_type.is_none()
            && let Some(index) = literal_index
            && !self.is_array_like_type(object_type_for_access)
        {
            let property_name = index.to_string();
            let resolved_type = self.resolve_type_for_property_access(object_type_for_access);
            let result = self.resolve_property_access_with_env(resolved_type, &property_name);
            result_type = match result {
                PropertyAccessResult::Success {
                    type_id,
                    write_type,
                    ..
                } => {
                    use_index_signature_check = false;
                    // In write context (assignment target), prefer the setter type.
                    Some(effective_write_result(type_id, write_type))
                }
                PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                    use_index_signature_check = false;
                    Some(property_type.unwrap_or(TypeId::ERROR))
                }
                PropertyAccessResult::IsUnknown => {
                    use_index_signature_check = false;
                    // TS18046: 'x' is of type 'unknown'.
                    // Without strictNullChecks, unknown is treated like any.
                    if self.error_is_of_type_unknown(access.expression) {
                        Some(TypeId::ERROR)
                    } else {
                        Some(TypeId::ANY)
                    }
                }
                PropertyAccessResult::PropertyNotFound { .. } => None,
            };
        }

        // Handle non-integer numeric literals (e.g., c[1.1], c[-1]) as property name access.
        // Integer literals are handled above via literal_index. Non-integer numeric literals
        // aren't covered by get_literal_string_from_node or get_literal_index_from_node,
        // so we need to try property access using their text representation.
        if result_type.is_none()
            && literal_index.is_none()
            && literal_string_is_none
            && let Some(node) = self.ctx.arena.get(access.name_or_argument)
            && node.kind == SyntaxKind::NumericLiteral as u16
            && let Some(lit) = self.ctx.arena.get_literal(node)
        {
            let property_name = &lit.text;
            let resolved_type = self.resolve_type_for_property_access(object_type_for_access);
            let result = self.resolve_property_access_with_env(resolved_type, property_name);
            if let PropertyAccessResult::Success {
                type_id,
                write_type,
                ..
            } = result
            {
                use_index_signature_check = false;
                result_type = Some(effective_write_result(type_id, write_type));
            }
        }

        // Handle unique symbol index access on concrete (non-type-parameter) objects.
        // Unique symbols resolve to internal property names like "__unique_N" and need
        // write_type propagation for getter/setter divergence (e.g., `foo[k] = value`
        // where `k` is a unique symbol with a setter type different from the getter).
        if result_type.is_none()
            && let Some(sym_ref) =
                tsz_solver::visitor::unique_symbol_ref(self.ctx.types, index_type)
            && !tsz_solver::visitor::is_type_parameter(self.ctx.types, pre_resolution_object_type)
        {
            let property_name = format!("__unique_{}", sym_ref.0);
            let resolved_type = self.resolve_type_for_property_access(object_type_for_access);
            let result = self.resolve_property_access_with_env(resolved_type, &property_name);
            if let PropertyAccessResult::Success {
                type_id,
                write_type,
                ..
            } = result
            {
                use_index_signature_check = false;
                result_type = Some(effective_write_result(type_id, write_type));
            }

            // Fallback: well-known symbols (Symbol.hasInstance, Symbol.iterator, etc.)
            // are stored as "[Symbol.xxx]" in class/interface types, not "__unique_N".
            // When the __unique_N lookup fails, try the [Symbol.xxx] format.
            if result_type.is_none() {
                let sym_id = tsz_binder::SymbolId(sym_ref.0);
                if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                    let sym_name = &symbol.escaped_name;
                    // Check if the parent is the Symbol global constructor
                    if symbol.parent.is_some()
                        && let Some(parent_sym) = self.ctx.binder.get_symbol(symbol.parent)
                        && parent_sym.escaped_name == "Symbol"
                    {
                        let well_known_name = format!("[Symbol.{sym_name}]");
                        let result =
                            self.resolve_property_access_with_env(resolved_type, &well_known_name);
                        if let PropertyAccessResult::Success {
                            type_id,
                            write_type,
                            ..
                        } = result
                        {
                            use_index_signature_check = false;
                            result_type = Some(effective_write_result(type_id, write_type));
                        }
                    }
                }
            }
        }

        // Handle `symbol` (primitive) index on types with late-bound (computed) members.
        // When a class declares `[expr]()` where `expr` has type `symbol`, the member
        // is late-bound and not stored as a named property. tsc resolves `obj[expr]` to
        // the computed member's type; we conservatively return `any` to avoid false
        // positives like TS2722 ("Cannot invoke possibly undefined").
        if result_type.is_none()
            && index_type == TypeId::SYMBOL
            && tsz_solver::visitor::has_late_bound_members(self.ctx.types, object_type_for_access)
        {
            result_type = Some(TypeId::ANY);
            use_index_signature_check = false;
        }

        // MAPPED TYPE GENERIC INDEXED ACCESS
        // When the pre-resolution object type is (or resolves to) a mapped type and the
        // index is a generic type parameter, produce an IndexAccess(Mapped, T) and let
        // the solver's evaluator handle template substitution via
        // try_mapped_type_param_substitution. This avoids the eager mapped-type expansion
        // in resolve_type_for_property_access which destroys the template relationship
        // needed for generic indexed access (e.g., `handlers[key]` where `handlers` has
        // type `{ [T in keyof M]?: (p: T) => void }` and `key: K extends keyof M`).
        if result_type.is_none()
            && tsz_solver::visitor::is_type_parameter(self.ctx.types, index_type)
        {
            let resolved_pre = self.resolve_lazy_type(pre_resolution_object_type);
            if tsz_solver::mapped_type_id(self.ctx.types, resolved_pre).is_some() {
                let index_access = self
                    .ctx
                    .types
                    .factory()
                    .index_access(resolved_pre, index_type);
                let evaluated = self.evaluate_type_with_env(index_access);
                if evaluated != index_access && evaluated != TypeId::ERROR {
                    result_type = Some(evaluated);
                    use_index_signature_check = false;
                }
            }
        }

        let used_generic_element_resolution = result_type.is_none();
        let mut result_type = result_type.unwrap_or_else(|| {
            if tsz_solver::visitor::is_type_parameter(self.ctx.types, pre_resolution_object_type)
                && self.is_generic_index_type(index_type)
            {
                // When indexing a type parameter T with keys from a different type
                // parameter (e.g., `keyof U` where `U extends T`), tsc emits TS2536.
                // We should not defer this case to IndexAccess(T, ...).
                if let Some(key_source) =
                    self.keyof_source_type_param(index_type, pre_resolution_object_type)
                    && !self.is_assignable_to(pre_resolution_object_type, key_source)
                {
                    use crate::diagnostics::{
                        diagnostic_codes, diagnostic_messages, format_message,
                    };
                    let index_type_str = self.format_type(index_type);
                    let object_type_str = self.format_type(pre_resolution_object_type);
                    let message = format_message(
                        diagnostic_messages::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
                        &[&index_type_str, &object_type_str],
                    );
                    self.error_at_node(
                        access.expression,
                        &message,
                        diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
                    );
                    return TypeId::ERROR;
                }

                // Case 1: U resolved to a DIFFERENT type parameter T (its constraint).
                // Produce a deferred IndexAccess(U, index) to preserve the distinction
                // between U[K] and T[K] for assignability.
                // Exception: when constraint is concrete (e.g., Record<K, number>),
                // let normal resolution proceed so T[K] resolves to number.
                if pre_resolution_object_type != object_type_for_access
                    && tsz_solver::visitor::is_type_parameter(
                        self.ctx.types,
                        object_type_for_access,
                    )
                {
                    return self
                        .ctx
                        .types
                        .factory()
                        .index_access(pre_resolution_object_type, index_type);
                }
                // Case 2: T resolved to itself (unconstrained type param) and the
                // index is known to be a valid key for T. Produce deferred IndexAccess
                // since the solver's is_indexable rejects bare type parameters.
                // Valid indices for T: keyof T (directly), or K extends keyof T.
                if pre_resolution_object_type == object_type_for_access
                    && self.is_valid_index_for_type_param(index_type, pre_resolution_object_type)
                {
                    return self
                        .ctx
                        .types
                        .factory()
                        .index_access(pre_resolution_object_type, index_type);
                }
                // Case 3: Type param resolved to a concrete constraint (e.g.,
                // T extends object → object). The index is generic (e.g.,
                // keyof Boxified<T> & string from a for-in). Produce deferred
                // IndexAccess(T, index) to preserve the generic relationship
                // and prevent false TS7053 on the constraint type.
                if pre_resolution_object_type != object_type_for_access {
                    return self
                        .ctx
                        .types
                        .factory()
                        .index_access(pre_resolution_object_type, index_type);
                }
            }
            // Case 4: Type param with unique symbol index. Unique symbols are concrete
            // (not generic), but when the object is a type parameter, the result type
            // depends on the specific T at instantiation time. Produce a deferred
            // IndexAccess(T, UniqueSymbol) to match tsc behavior (e.g., T[typeof fooProp]).
            if tsz_solver::visitor::is_type_parameter(self.ctx.types, pre_resolution_object_type)
                && tsz_solver::visitor::unique_symbol_ref(self.ctx.types, index_type).is_some()
                && pre_resolution_object_type != object_type_for_access
            {
                return self
                    .ctx
                    .types
                    .factory()
                    .index_access(pre_resolution_object_type, index_type);
            }
            self.get_element_access_type(object_type_for_access, index_type, literal_index)
        });

        if used_generic_element_resolution
            && literal_index.is_none()
            && self.ctx.no_unchecked_indexed_access()
            && !skip_flow_narrowing
            && result_type != TypeId::ERROR
            && result_type != TypeId::ANY
            && result_type != TypeId::UNKNOWN
            && result_type != TypeId::NEVER
            && self.split_nullish_type(result_type).1.is_none()
        {
            result_type = self
                .ctx
                .types
                .factory()
                .union2(result_type, TypeId::UNDEFINED);
        }

        if result_type == TypeId::ERROR
            && let Some(index) = literal_index
        {
            if let Some(tuple_elements) =
                crate::query_boundaries::type_computation::access::tuple_elements(
                    self.ctx.types,
                    object_type_for_access,
                )
            {
                // Single tuple: emit TS2493
                let has_rest_tail = tuple_elements.last().is_some_and(|element| element.rest);
                if !has_rest_tail && index >= tuple_elements.len() {
                    let tuple_type_str = self.format_type(object_type_for_access);
                    self.error_at_node(
                        access.name_or_argument,
                        &format!(
                            "Tuple type '{}' of length '{}' has no element at index '{}'.",
                            tuple_type_str,
                            tuple_elements.len(),
                            index
                        ),
                        crate::diagnostics::diagnostic_codes::TUPLE_TYPE_OF_LENGTH_HAS_NO_ELEMENT_AT_INDEX,
                    );
                    // tsc treats the type of an out-of-bounds tuple access as `undefined`,
                    // not an error type. This prevents cascading errors (e.g., TS2403 for
                    // subsequent variable declarations should still fire against `undefined`).
                    result_type = TypeId::UNDEFINED;
                }
            } else if self.is_union_of_tuples_all_out_of_bounds(object_type_for_access, index) {
                // Union of tuples where ALL members are out of bounds: emit TS2339
                let type_str = self.format_type(object_type);
                self.error_at_node(
                    access.name_or_argument,
                    &format!("Property '{index}' does not exist on type '{type_str}'.",),
                    crate::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                );
            }
        }

        // Fresh object literal implicit index signature: tsc allows indexing a
        // directly-written object literal with `string` (or `number`) even when
        // the type has no explicit index signature. The solver already computes
        // the union of all property types as the result, so we only need to
        // suppress the checker's independent TS7053 check.
        //
        // IMPORTANT: Only suppress when the object literal has at least one
        // property. An empty `{}` has no properties to form an implicit index
        // signature, so `{}["hi"]` and `{}[10]` should still report diagnostics
        // (TS2339 for literal keys, TS7053 for non-literal keys).
        let is_fresh_object_literal = self.ctx.arena.get(access.expression).is_some_and(|n| {
            n.kind == tsz_parser::parser::syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                && self
                    .ctx
                    .arena
                    .get_literal_expr(n)
                    .is_some_and(|lit| !lit.elements.nodes.is_empty())
        });

        if use_index_signature_check
            && self
                .should_report_union_generic_key_mismatch_ts2536(object_type_for_access, index_type)
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let index_type_str = self.format_type(index_type);
            let object_type_str = self.format_type(object_type);
            let message = format_message(
                diagnostic_messages::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
                &[&index_type_str, &object_type_str],
            );
            self.error_at_node(
                access.expression,
                &message,
                diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
            );
            return TypeId::ERROR;
        }

        if use_index_signature_check
            && !is_fresh_object_literal
            && self.should_report_no_index_signature(
                object_type_for_access,
                index_type,
                literal_index,
            )
        {
            report_no_index = true;
        }

        // For unique symbol indices on union types, check that ALL members
        // support the symbol property. The solver's index access evaluator
        // silently drops UNDEFINED results from union members, which is correct
        // for string/number indices (covered by index signatures) but wrong for
        // unique symbols that can't fall through to index signatures.
        if !report_no_index
            && use_index_signature_check
            && tsz_solver::visitor::unique_symbol_ref(self.ctx.types, index_type).is_some()
            && crate::query_boundaries::common::union_members(
                self.ctx.types,
                object_type_for_access,
            )
            .is_none()
        {
            let member_result = self.ctx.types.resolve_element_access_type(
                object_type_for_access,
                index_type,
                None,
            );
            if member_result == TypeId::ERROR || member_result == TypeId::UNDEFINED {
                report_no_index = true;
            }
        }

        if !report_no_index
            && use_index_signature_check
            && tsz_solver::visitor::unique_symbol_ref(self.ctx.types, index_type).is_some()
            && let Some(members) = crate::query_boundaries::common::union_members(
                self.ctx.types,
                object_type_for_access,
            )
        {
            for member in &members {
                let member_result = self
                    .ctx
                    .types
                    .resolve_element_access_type(*member, index_type, None);
                if member_result == TypeId::ERROR || member_result == TypeId::UNDEFINED {
                    report_no_index = true;
                    break;
                }
            }
        }

        if !report_no_index
            && use_index_signature_check
            && self.union_has_missing_concrete_element_access(
                object_type_for_access,
                index_type,
                literal_index,
            )
        {
            report_no_index = true;
        }

        // When we have specific missing literal keys from a union, emit TS2339
        // per-key instead of TS7053 for the whole union. tsc identifies the
        // specific non-existent key(s) from the union and reports TS2339.
        // tsc spans the error over the entire element access expression node,
        // not just the argument.
        if !missing_literal_keys.is_empty() {
            for key in &missing_literal_keys {
                self.error_property_not_exist_at(key, object_type_for_access, idx);
            }
        }

        if report_no_index {
            // Suppress TS7053 for expando bracket assignments on function types.
            // When `func["prop"] = value` and the object is callable, tsc does not emit
            // TS7053 — it treats this as a valid JS-style property expansion.
            // We detect write context via `skip_flow_narrowing` which is set by
            // `get_type_of_assignment_target`.
            let is_namespace_object = self
                .ctx
                .namespace_module_names
                .contains_key(&object_type_for_access);
            let is_js_expando_object_write = self.ctx.is_js_file()
                && tsz_solver::visitor::is_object_like_type(self.ctx.types, object_type_for_access)
                // JS expando-style element writes only suppress TS7053 when the key is a
                // simple literal/identifier shape the binder/checker can track. Arbitrary
                // computed expressions like `this["a" + "b"] = 0` should still report.
                && self.expando_element_key_name(access.name_or_argument).is_some();
            let is_expando_write = skip_flow_narrowing
                && !is_namespace_object
                && (tsz_solver::visitor::is_function_type(self.ctx.types, object_type_for_access)
                    || is_js_expando_object_write);
            // Suppress TS7053 for expando reads with unique symbol keys on function
            // types. When `func[symKey]` where symKey is a const Symbol() variable
            // and `func[symKey] = value` was assigned as an expando property, tsc
            // does not emit TS7053 on the read side either.
            // We check: (a) read context, (b) function type, (c) unique symbol index,
            // (d) the object has ANY unique-symbol expando properties recorded by the
            // binder. This avoids depending on exact SymbolId matching (which can
            // fail due to lib-merge rewriting the binder's symbol arena).
            let is_expando_symbol_read = !skip_flow_narrowing
                && !is_namespace_object
                && tsz_solver::visitor::is_function_type(self.ctx.types, object_type_for_access)
                && tsz_solver::visitor::unique_symbol_ref(self.ctx.types, index_type).is_some()
                && self.object_has_unique_symbol_expandos(access.expression);
            if !is_expando_write && !is_expando_symbol_read {
                self.error_no_index_signature_at(
                    index_type,
                    object_type_for_access,
                    idx,
                    access.name_or_argument,
                );
                if skip_flow_narrowing {
                    return TypeId::ERROR;
                }
            }
        }

        if let Some(cause) = nullish_cause {
            if access.question_dot_token {
                result_type = self
                    .ctx
                    .types
                    .factory()
                    .union2(result_type, TypeId::UNDEFINED);
            } else if !report_no_index {
                self.report_possibly_nullish_object(access.expression, cause);
            }
        }

        let result_type = if skip_flow_narrowing {
            result_type
        } else {
            self.apply_flow_narrowing(idx, result_type)
        };
        self.instantiate_callable_result_from_request(idx, result_type, request)
    }
}

#[cfg(test)]
mod tests {
    use crate::context::CheckerOptions;
    use crate::diagnostics::Diagnostic;
    use crate::query_boundaries::type_construction::TypeInterner;
    use crate::state::CheckerState;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;

    fn check_source_with_default_libs(source: &str) -> Vec<Diagnostic> {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let source_file = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), source_file);

        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            CheckerOptions::default(),
        );
        checker.check_source_file(source_file);

        checker.ctx.diagnostics.clone()
    }

    fn has_code(diags: &[Diagnostic], code: u32) -> bool {
        diags.iter().any(|d| d.code == code)
    }

    /// Filter out TS2318 ("Cannot find global type") which fires when lib files aren't loaded.
    fn semantic_errors(diags: &[Diagnostic]) -> Vec<u32> {
        diags
            .iter()
            .filter(|d| d.code != 2318)
            .map(|d| d.code)
            .collect()
    }

    /// Minimal Promise/PromiseLike type definitions for tests.
    const PROMISE_LIB: &str = r#"
interface PromiseLike<T> {
    then<TResult1 = T, TResult2 = never>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | null,
        onrejected?: ((reason: any) => TResult2 | PromiseLike<TResult2>) | null
    ): PromiseLike<TResult1 | TResult2>;
}
interface Promise<T> {
    then<TResult1 = T, TResult2 = never>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | null,
        onrejected?: ((reason: any) => TResult2 | PromiseLike<TResult2>) | null
    ): Promise<TResult1 | TResult2>;
}
interface PromiseConstructor {
    new <T>(executor: (resolve: (value: T | PromiseLike<T>) => void, reject: (reason?: any) => void) => void): Promise<T>;
}
declare var Promise: PromiseConstructor;
"#;

    #[test]
    fn contextual_type_through_new_promise_variable_decl() {
        // `const p: Promise<string> = new Promise(resolve => resolve("hello"))` should
        // infer T = string from the contextual type, producing no errors.
        let source = format!(
            r#"{PROMISE_LIB}
const p: Promise<string> = new Promise(resolve => resolve("hello"));"#
        );
        let diags = check_source_with_default_libs(&source);
        let errors = semantic_errors(&diags);
        assert!(
            errors.is_empty(),
            "Expected no semantic errors for contextually typed new Promise, got: {errors:?}"
        );
    }

    #[test]
    fn contextual_type_through_await_new_promise() {
        // `const s: string = await new Promise(resolve => resolve("ok"))` should
        // infer T = string via the await contextual type union.
        let source = format!(
            r#"{PROMISE_LIB}
async function f() {{ const s: string = await new Promise(resolve => resolve("ok")); }}"#
        );
        let diags = check_source_with_default_libs(&source);
        let errors = semantic_errors(&diags);
        assert!(
            errors.is_empty(),
            "Expected no semantic errors for await new Promise with contextual type, got: {errors:?}"
        );
    }

    #[test]
    fn contextual_type_async_return_new_promise() {
        // Note: the full async return + new Promise fix requires real lib files because
        // resolve_global_interface_type("Promise") doesn't find local declarations.
        // This test verifies the code doesn't crash; the full fix is validated by
        // the contextuallyTypeAsyncFunctionReturnType conformance test.
        let source = format!(
            r#"{PROMISE_LIB}
interface Obj {{ key: "value"; }}
async function f(): Promise<Obj> {{
    return new Promise(resolve => {{
        resolve({{ key: "value" }});
    }});
}}"#
        );
        let diags = check_source_with_default_libs(&source);
        // Without real lib files, global Promise resolution fails and inference
        // falls back to unknown, producing TS2322/TS2345. This is expected.
        // The important thing is no crash and the code path executes.
        let _ = semantic_errors(&diags);
    }

    #[test]
    fn tuple_expression_negative_index_emits_t2514() {
        // `as const` makes the literal a readonly tuple — without it, `["a", 1]`
        // is inferred as `(string | number)[]` (an array) and TS2514 is not expected.
        let diags = check_source_with_default_libs(
            r#"
const tuple = ["a", 1] as const;
const bad = tuple[-1];
"#,
        );

        assert!(
            has_code(&diags, 2514),
            "Expected TS2514 for tuple expression negative index, got: {:?}",
            diags.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn private_name_access_unknown_reports_18046() {
        let diags = check_source_with_default_libs(
            r#"
class A {
    #foo = true;
    static #baz = 10;
    static #m() {}
    method(thing: unknown) {
        thing.#foo;
        thing.#m();
        thing.#baz;
        thing.#bar;
        thing.#foo();
    }
}
"#,
        );
        let errors = semantic_errors(&diags);
        assert_eq!(
            errors.iter().filter(|code| **code == 18046).count(),
            5,
            "Expected 5 TS18046 diagnostics for private access on unknown, got: {errors:?}"
        );
        assert_eq!(
            errors.iter().filter(|code| **code == 2339).count(),
            1,
            "Expected one TS2339 diagnostic for undeclared private name, got: {errors:?}"
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == 2339 && d.message_text.contains("#bar")),
            "Expected the TS2339 diagnostic to mention '#bar': {diags:?}"
        );
    }

    #[test]
    fn private_name_access_never_reports_2339() {
        let diags = check_source_with_default_libs(
            r#"
class A {
    #foo = true;
    static #baz = 10;
    static #m() {}
    method(thing: never) {
        thing.#foo;
        thing.#m();
        thing.#baz;
        thing.#bar;
        thing.#foo();
    }
}
"#,
        );
        let errors = semantic_errors(&diags);
        assert_eq!(
            errors.iter().filter(|code| **code == 2339).count(),
            5,
            "Expected 5 TS2339 diagnostics for private access on never, got: {errors:?}"
        );
        assert!(
            errors.iter().all(|code| *code == 2339),
            "Expected only TS2339 diagnostics, got: {errors:?}"
        );
    }

    #[test]
    fn inherited_static_member_element_access_emits_ts2576() {
        let diags = check_source_with_default_libs(
            r#"
class Base {
    static count = 1;
    static get size() {
        return 2;
    }
}
class Derived extends Base {}
const value = new Derived();
value["count"];
value["size"];
"#,
        );

        let errors = semantic_errors(&diags);
        assert_eq!(
            errors.iter().filter(|code| **code == 2576).count(),
            2,
            "Expected TS2576 for inherited static field and accessor element access, got: {errors:?}"
        );
    }
}
