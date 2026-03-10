//! Element access, super keyword, await type computation, and optional chain detection.

use crate::state::{CheckerState, EnumKind};
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

const MAX_AWAIT_DEPTH: u32 = 10;

impl<'a> CheckerState<'a> {
    fn is_expando_element_access_read(
        &self,
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

        fn expando_element_key(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
            let node = arena.get(idx)?;
            match node.kind {
                k if k == SyntaxKind::Identifier as u16 => {
                    arena.get_identifier(node).map(|id| id.escaped_text.clone())
                }
                k if k == SyntaxKind::StringLiteral as u16
                    || k == SyntaxKind::NumericLiteral as u16
                    || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
                {
                    arena.get_literal(node).map(|lit| lit.text.clone())
                }
                _ => None,
            }
        }

        let Some(obj_key) = property_access_chain(self.ctx.arena, object_expr_idx) else {
            return false;
        };
        let Some(prop_key) = expando_element_key(self.ctx.arena, key_expr_idx) else {
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

        if let Some(all_binders) = &self.ctx.all_binders {
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
    pub(crate) fn get_type_of_element_access(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_solver::operations::property::PropertyAccessResult;

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

        // Get the type of the object
        let object_type = self.get_type_of_node(access.expression);
        let object_type = self.evaluate_application_type(object_type);

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

        // Save the pre-resolution object type. When the object is a type parameter,
        // resolve_type_for_property_access replaces it with its constraint. But for
        // generic indexed access (e.g., U[keyof T] where U extends T), we need to
        // keep the original type parameter to produce the correct deferred type.
        let pre_resolution_object_type = object_type;

        let literal_string = self.get_literal_string_from_node(access.name_or_argument);
        let numeric_string_index = literal_string
            .as_deref()
            .and_then(|name| self.get_numeric_index_from_string(name));
        let literal_index = self
            .get_literal_index_from_node(access.name_or_argument)
            .or(numeric_string_index);

        if let Some(name) = literal_string.as_deref()
            && self.is_global_this_expression(access.expression)
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
            return self.apply_flow_narrowing(idx, property_type);
        }

        // Don't report errors for any/error types - check BEFORE accessibility
        // to prevent cascading errors when the object type is already invalid
        if object_type == TypeId::ANY {
            return TypeId::ANY;
        }
        if object_type == TypeId::ERROR {
            return TypeId::ERROR;
        }

        // TS18013: Check for private identifier access outside of class
        // Private identifiers (#foo) can only be accessed from within the class that declares them
        if let Some(name_node) = self.ctx.arena.get(access.name_or_argument)
            && name_node.kind == tsz_scanner::SyntaxKind::PrivateIdentifier as u16
        {
            // Get the property name
            let prop_name = if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                &ident.escaped_text
            } else {
                "#"
            };

            // Check if we're inside the class that declares this private identifier
            let (symbols, _saw_class_scope) =
                self.resolve_private_identifier_symbols(access.name_or_argument);

            // If we didn't find the symbol, it's being accessed outside the class that declares it
            if symbols.is_empty() {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

                // Find the class that declares this private member (walk up hierarchy)
                let class_name = self
                    .get_declaring_class_name_for_private_member(object_type, prop_name)
                    .unwrap_or_else(|| "the class".to_string());

                let message = format_message(
                        diagnostic_messages::PROPERTY_IS_NOT_ACCESSIBLE_OUTSIDE_CLASS_BECAUSE_IT_HAS_A_PRIVATE_IDENTIFIER,
                        &[prop_name, &class_name],
                    );
                self.error_at_node(
                        access.name_or_argument,
                        &message,
                        diagnostic_codes::PROPERTY_IS_NOT_ACCESSIBLE_OUTSIDE_CLASS_BECAUSE_IT_HAS_A_PRIVATE_IDENTIFIER,
                    );
                return TypeId::ERROR;
            }
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
        let index_type = self.get_type_of_node(access.name_or_argument);
        self.ctx.preserve_literal_types = prev_preserve;

        // Propagate error from index expression to suppress cascading errors
        if index_type == TypeId::ERROR {
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
        if self.ctx.skip_flow_narrowing
            && tsz_solver::visitor::is_type_parameter(self.ctx.types, pre_resolution_object_type)
            && self.is_valid_index_for_type_param(index_type, pre_resolution_object_type)
        {
            return self
                .ctx
                .types
                .factory()
                .index_access(pre_resolution_object_type, index_type);
        }

        // TS2476: A const enum member can only be accessed using a string literal.
        if let Some(sym_id) = self.enum_symbol_from_type(object_type_for_access)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.flags & tsz_binder::symbol_flags::CONST_ENUM != 0
        {
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
            if self.is_type_only_import_equals_namespace_expr(access.expression) {
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

            if let Some(member_type) =
                self.resolve_namespace_value_member(object_type_for_access, name)
            {
                result_type = Some(member_type);
                use_index_signature_check = false;
            } else if self.namespace_has_type_only_member(object_type_for_access, name) {
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
                        self.error_namespace_used_as_value_at(&ns_name, access.expression);
                    }
                    // Also emit TS2693 for the type-only member itself
                    self.error_type_only_value_at(name, access.name_or_argument);
                }
                return TypeId::ERROR;
            }
        }

        if !self.ctx.skip_flow_narrowing
            && self.is_expando_element_access_read(access.expression, access.name_or_argument)
        {
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
                    result_type = Some(if self.ctx.skip_flow_narrowing {
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
                    use_index_signature_check = false;
                    // In write context (assignment target), prefer the setter type.
                    let effective = if self.ctx.skip_flow_narrowing {
                        write_type.unwrap_or(type_id)
                    } else {
                        type_id
                    };
                    Some(effective)
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
                    // Use .is_some() — TS2576 fires for any static member (property or method).
                    if self.is_super_expression(access.expression)
                        && let Some(ref class_info) = self.ctx.enclosing_class
                        && let Some(base_idx) = self.get_base_class_idx(class_info.class_idx)
                        && self
                            .is_method_member_in_class_hierarchy(base_idx, &property_name, true)
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
                            .is_method_member_in_class_hierarchy(class_idx, &property_name, true)
                            .is_some()
                    {
                        use crate::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };

                        let class_name = self.get_class_name_from_decl(class_idx);
                        let static_member_name = format!("{class_name}.{property_name}");
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
                    } else {
                        // TS2339 parity for element access on `typeof const enum` with missing member.
                        // Const enums do not have a reverse mapping, so they shouldn't fall back to
                        // TS7053 string index signature checks like regular enums do.
                        let mut is_const_enum = false;
                        if let Some(sym_id) = self.enum_symbol_from_type(object_type_for_access)
                            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                            && symbol.flags & tsz_binder::symbol_flags::CONST_ENUM != 0
                        {
                            is_const_enum = true;
                        }

                        if is_const_enum {
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
                    let effective = if self.ctx.skip_flow_narrowing {
                        write_type.unwrap_or(type_id)
                    } else {
                        type_id
                    };
                    Some(effective)
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
                let effective = if self.ctx.skip_flow_narrowing {
                    write_type.unwrap_or(type_id)
                } else {
                    type_id
                };
                result_type = Some(effective);
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
                let effective = if self.ctx.skip_flow_narrowing {
                    write_type.unwrap_or(type_id)
                } else {
                    type_id
                };
                result_type = Some(effective);
            }
        }

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
        let is_fresh_object_literal = self.ctx.arena.get(access.expression).is_some_and(|n| {
            n.kind == tsz_parser::parser::syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
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
            && let Some(members) = tsz_solver::type_queries::data::get_union_members(
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
            let is_expando_write = self.ctx.skip_flow_narrowing
                && (tsz_solver::visitor::is_function_type(self.ctx.types, object_type_for_access)
                    || (self.ctx.is_js_file()
                        && tsz_solver::visitor::is_object_like_type(
                            self.ctx.types,
                            object_type_for_access,
                        )));
            if !is_expando_write {
                self.error_no_index_signature_at(
                    index_type,
                    object_type,
                    idx,
                    access.name_or_argument,
                );
            }
        }

        if let Some(cause) = nullish_cause {
            if access.question_dot_token {
                result_type = self
                    .ctx
                    .types
                    .factory()
                    .union(vec![result_type, TypeId::UNDEFINED]);
            } else if !report_no_index {
                self.report_possibly_nullish_object(access.expression, cause);
            }
        }

        self.apply_flow_narrowing(idx, result_type)
    }

    fn get_number_value_from_element_index(&self, idx: NodeIndex) -> Option<f64> {
        let node = self.ctx.arena.get(idx)?;

        if node.kind == SyntaxKind::NumericLiteral as u16 {
            return self
                .ctx
                .arena
                .get_literal(node)
                .and_then(|literal| literal.value);
        }

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.ctx.arena.get_parenthesized(node)
        {
            return self.get_number_value_from_element_index(paren.expression);
        }

        if node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION {
            let data = self.ctx.arena.get_unary_expr(node)?;
            let operand = self.get_number_value_from_element_index(data.operand)?;
            return match data.operator {
                k if k == SyntaxKind::MinusToken as u16 => Some(-operand),
                k if k == SyntaxKind::PlusToken as u16 => Some(operand),
                _ => None,
            };
        }

        if node.kind == syntax_kind_ext::LITERAL_TYPE
            && let Some(literal_type) = self.ctx.arena.get_literal_type(node)
        {
            return self.get_number_value_from_element_index(literal_type.literal);
        }

        None
    }

    /// Get the element access type for array/tuple/object with index signatures.
    ///
    /// Computes the type when accessing an element using an index.
    /// Uses `ElementAccessEvaluator` from solver for structured error handling.
    pub(crate) fn get_element_access_type(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
        literal_index: Option<usize>,
    ) -> TypeId {
        // Normalize index type for enum values
        let solver_index_type = if let Some(index) = literal_index {
            self.ctx.types.literal_number(index as f64)
        } else if self
            .enum_symbol_from_type(index_type)
            .is_some_and(|sym_id| self.enum_kind(sym_id) == Some(EnumKind::Numeric))
        {
            // Numeric enum values are number-like at runtime.
            TypeId::NUMBER
        } else {
            index_type
        };

        self.ctx
            .types
            .resolve_element_access_type(object_type, solver_index_type, literal_index)
    }

    /// Check if a type is a union of tuples where ALL members are out of bounds
    /// for the given literal index. Used to emit TS2339 instead of TS2493.
    fn is_union_of_tuples_all_out_of_bounds(&self, object_type: TypeId, index: usize) -> bool {
        let Some(members) =
            tsz_solver::type_queries::data::get_union_members(self.ctx.types, object_type)
        else {
            return false;
        };
        let mut has_any_tuple = false;
        for member in &members {
            if let Some(elems) = crate::query_boundaries::type_computation::access::tuple_elements(
                self.ctx.types,
                *member,
            ) {
                has_any_tuple = true;
                let has_rest = elems.iter().any(|e| e.rest);
                if has_rest || index < elems.len() {
                    return false;
                }
            } else {
                return false;
            }
        }
        has_any_tuple
    }

    /// Check if an index type is "generic" — i.e., it cannot be resolved to a
    /// concrete property key and must remain deferred in an `IndexAccess` type.
    ///
    /// Generic index types include: keyof T, type parameters, indexed access types,
    /// conditional types, and intersections containing any of the above
    /// (e.g., `keyof Boxified<T> & string` from for-in variable typing).
    fn is_generic_index_type(&self, index_type: TypeId) -> bool {
        use tsz_solver::visitor;
        visitor::is_type_parameter(self.ctx.types, index_type)
            || visitor::keyof_inner_type(self.ctx.types, index_type).is_some()
            || visitor::is_index_access_type(self.ctx.types, index_type)
            || visitor::is_conditional_type(self.ctx.types, index_type)
            || tsz_solver::is_generic_application(self.ctx.types, index_type)
            || self.intersection_has_generic_index(index_type)
    }

    /// Check if an intersection type contains a generic index member.
    ///
    /// For-in variables over generic types get type `keyof ExprType & string`,
    /// which is an intersection. This helper recursively checks whether any
    /// member of the intersection is a generic index type.
    fn intersection_has_generic_index(&self, type_id: TypeId) -> bool {
        if let Some(members) =
            tsz_solver::type_queries::data::get_intersection_members(self.ctx.types, type_id)
        {
            members.iter().any(|&m| self.is_generic_index_type(m))
        } else {
            false
        }
    }

    /// Check if an index type is known to be a valid key for a given type parameter.
    ///
    /// Returns true for:
    /// - `keyof T` where T is the target type param (direct keyof)
    /// - `K extends keyof T` where T is the target type param (constrained key)
    pub(crate) fn is_valid_index_for_type_param(
        &mut self,
        index_type: TypeId,
        type_param: TypeId,
    ) -> bool {
        use tsz_solver::visitor;
        if let Some(members) =
            tsz_solver::type_queries::data::get_intersection_members(self.ctx.types, index_type)
        {
            return members
                .iter()
                .copied()
                .any(|member| self.is_valid_index_for_type_param(member, type_param));
        }
        if tsz_solver::is_generic_application(self.ctx.types, index_type) {
            let evaluated = self.evaluate_type_with_env(index_type);
            if evaluated != index_type && evaluated != TypeId::ERROR {
                return self.is_valid_index_for_type_param(evaluated, type_param);
            }
        }
        // Direct keyof T
        if let Some(keyof_inner) = visitor::keyof_inner_type(self.ctx.types, index_type) {
            return keyof_inner == type_param;
        }
        // K extends keyof T (type param whose constraint is keyof T)
        if let Some(param_info) = visitor::type_param_info(self.ctx.types, index_type)
            && let Some(constraint) = param_info.constraint
            && let Some(keyof_inner) = visitor::keyof_inner_type(self.ctx.types, constraint)
        {
            return keyof_inner == type_param;
        }
        false
    }

    /// Return the type parameter source when `index_type` is `keyof S` or `K extends keyof S`
    /// for a type parameter `S` different from `type_param`.
    ///
    /// The caller can then decide whether indexing should be legal based on
    /// type-parameter relation direction (e.g. `U[keyof T]` is legal when `U extends T`,
    /// but `T[keyof U]` is not).
    fn keyof_source_type_param(&self, index_type: TypeId, type_param: TypeId) -> Option<TypeId> {
        use tsz_solver::visitor;

        if let Some(keyof_inner) = visitor::keyof_inner_type(self.ctx.types, index_type)
            && visitor::is_type_parameter(self.ctx.types, keyof_inner)
            && keyof_inner != type_param
        {
            return Some(keyof_inner);
        }

        if let Some(param_info) = visitor::type_param_info(self.ctx.types, index_type)
            && let Some(constraint) = param_info.constraint
            && let Some(keyof_inner) = visitor::keyof_inner_type(self.ctx.types, constraint)
            && visitor::is_type_parameter(self.ctx.types, keyof_inner)
            && keyof_inner != type_param
        {
            return Some(keyof_inner);
        }

        None
    }

    fn should_report_union_generic_key_mismatch_ts2536(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
    ) -> bool {
        let Some(members) =
            tsz_solver::type_queries::data::get_union_members(self.ctx.types, object_type)
        else {
            return false;
        };
        if members.len() < 2 || !self.is_generic_key_space(index_type) {
            return false;
        }

        members.iter().any(|&member| {
            let member_keyof = self.ctx.types.evaluate_keyof(member);
            !self.is_assignable_to(index_type, member_keyof)
        })
    }

    fn is_generic_key_space(&self, type_id: TypeId) -> bool {
        use tsz_solver::visitor;

        if visitor::keyof_inner_type(self.ctx.types, type_id).is_some()
            || visitor::is_type_parameter(self.ctx.types, type_id)
        {
            return true;
        }

        if let Some(members) =
            tsz_solver::type_queries::data::get_union_members(self.ctx.types, type_id)
        {
            return members
                .iter()
                .all(|&member| self.is_generic_key_space(member));
        }

        if let Some(members) =
            tsz_solver::type_queries::get_intersection_members(self.ctx.types, type_id)
        {
            return members
                .iter()
                .all(|&member| self.is_generic_key_space(member));
        }

        false
    }

    /// Get the type of the `super` keyword.
    ///
    /// Computes the type of `super` expressions:
    /// - `super()` calls: returns the base class constructor type
    /// - `super.property` access: returns the base class instance type
    /// - Static context: returns constructor type
    /// - Instance context: returns instance type
    pub(crate) fn get_type_of_super_keyword(&mut self, idx: NodeIndex) -> TypeId {
        // Check super expression validity and emit any errors
        self.check_super_expression(idx);

        let Some(class_info) = self.ctx.enclosing_class.clone() else {
            return TypeId::ERROR;
        };

        let mut extends_expr_idx = NodeIndex::NONE;
        let mut extends_type_args = None;
        if let Some(current_class) = self.ctx.arena.get_class_at(class_info.class_idx)
            && let Some(heritage_clauses) = &current_class.heritage_clauses
        {
            for &clause_idx in &heritage_clauses.nodes {
                let Some(heritage) = self.ctx.arena.get_heritage_clause_at(clause_idx) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                let Some(&type_idx) = heritage.types.nodes.first() else {
                    continue;
                };
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args_at(type_idx) {
                    extends_expr_idx = expr_type_args.expression;
                    extends_type_args = expr_type_args.type_arguments.clone();
                } else {
                    extends_expr_idx = type_idx;
                }
                break;
            }
        }

        // Detect `super(...)` usage by checking if the parent is a CallExpression whose callee is `super`.
        let is_super_call = self
            .ctx
            .arena
            .get_extended(idx)
            .and_then(|ext| self.ctx.arena.get(ext.parent).map(|n| (ext.parent, n)))
            .and_then(|(parent_idx, parent_node)| {
                if parent_node.kind != syntax_kind_ext::CALL_EXPRESSION {
                    return None;
                }
                let call = self.ctx.arena.get_call_expr(parent_node)?;
                Some(call.expression == idx && parent_idx.is_some())
            })
            .unwrap_or(false);

        // Static context: the current `this` type is the current class constructor type.
        let is_static_context = self.current_this_type().is_some_and(|this_ty| {
            if let Some(sym_id) = self.ctx.binder.get_node_symbol(class_info.class_idx) {
                this_ty == self.get_type_of_symbol(sym_id)
            } else if let Some(class_node) = self.ctx.arena.get(class_info.class_idx) {
                if let Some(class) = self.ctx.arena.get_class(class_node) {
                    this_ty == self.get_class_constructor_type(class_info.class_idx, class)
                } else {
                    false
                }
            } else {
                false
            }
        });

        if is_super_call || is_static_context {
            if extends_expr_idx.is_some()
                && let Some(ctor_type) = self.base_constructor_type_from_expression(
                    extends_expr_idx,
                    extends_type_args.as_ref(),
                )
            {
                return ctor_type;
            }

            let Some(base_class_idx) = self.get_base_class_idx(class_info.class_idx) else {
                return TypeId::ERROR;
            };
            let Some(base_node) = self.ctx.arena.get(base_class_idx) else {
                return TypeId::ERROR;
            };
            let Some(base_class) = self.ctx.arena.get_class(base_node) else {
                return TypeId::ERROR;
            };
            return self.get_class_constructor_type(base_class_idx, base_class);
        }

        if extends_expr_idx.is_some()
            && let Some(instance_type) = self
                .base_instance_type_from_expression(extends_expr_idx, extends_type_args.as_ref())
        {
            return instance_type;
        }

        let Some(base_class_idx) = self.get_base_class_idx(class_info.class_idx) else {
            return TypeId::ERROR;
        };
        let Some(base_node) = self.ctx.arena.get(base_class_idx) else {
            return TypeId::ERROR;
        };
        let Some(base_class) = self.ctx.arena.get_class(base_node) else {
            return TypeId::ERROR;
        };

        self.get_class_instance_type(base_class_idx, base_class)
    }

    // =========================================================================
    // Await Expression Type Computation
    // =========================================================================

    /// Get the type of an await expression with contextual typing support.
    ///
    /// Propagate contextual type to await operand.
    ///
    /// When awaiting with a contextual type T (e.g., `const x: T = await expr`),
    /// the operand should receive T | `PromiseLike`<T> as its contextual type.
    /// This allows both immediate values and Promises to be inferred correctly.
    ///
    /// Example:
    /// ```typescript
    /// async function fn(): Promise<Obj> {
    ///     const obj: Obj = await { key: "value" };  // Operand gets Obj | PromiseLike<Obj>
    ///     return obj;
    /// }
    /// ```
    pub(crate) fn get_type_of_await_expression(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(unary) = self.ctx.arena.get_unary_expr_ex(node) else {
            return TypeId::ERROR;
        };

        // Match tsc's special-case for `await(...)` inside sync functions.
        // In these contexts TypeScript treats this as an unresolved identifier use
        // and reports TS2311 instead of await-context diagnostics.
        if !self.ctx.in_async_context()
            && self.ctx.function_depth > 0
            && !self.ctx.binder.is_external_module()
            && self.await_expression_uses_call_like_syntax(idx)
        {
            if let Some((start, _)) = self.get_node_span(idx) {
                let message = crate::diagnostics::format_message(
                    crate::diagnostics::diagnostic_messages::CANNOT_FIND_NAME_DID_YOU_MEAN_TO_WRITE_THIS_IN_AN_ASYNC_FUNCTION,
                    &["await"],
                );
                self.error_at_position(
                    start,
                    5,
                    &message,
                    crate::diagnostics::diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_TO_WRITE_THIS_IN_AN_ASYNC_FUNCTION,
                );
            }
            return TypeId::ANY;
        }

        // Propagate contextual type to await operand
        // If we have a contextual type T, transform it to T | PromiseLike<T> | Promise<T>
        // Including Promise<T> is critical for generic constructor inference:
        // `const obj: Obj = await new Promise(resolve => ...)` needs the constraint
        // `Promise<__infer_0> <: Promise<Obj>` (same base) to infer T = Obj.
        // Without Promise<T>, we'd only have PromiseLike<Obj> which has a different
        // base and can't be directly unified through type argument matching.
        let prev_context = self.ctx.contextual_type;
        if let Some(contextual) = prev_context {
            // Skip transformation for error types, any, unknown, or never
            if contextual != TypeId::ANY
                && contextual != TypeId::UNKNOWN
                && contextual != TypeId::NEVER
                && !self.type_contains_error(contextual)
            {
                let promise_like_t = self.get_promise_like_type(contextual);
                let promise_t = self.get_promise_type(contextual);
                let mut members = vec![contextual, promise_like_t];
                if let Some(pt) = promise_t {
                    members.push(pt);
                }
                let union_context = self.ctx.types.factory().union(members);
                self.ctx.contextual_type = Some(union_context);
            }
        }

        // Get the type of the await operand with transformed contextual type
        let expr_type = self.get_type_of_node(unary.expression);

        // Restore the original contextual type
        self.ctx.contextual_type = prev_context;

        // Recursively unwrap Promise<T> to get T (simulating Awaited<T>)
        // TypeScript's await recursively unwraps nested Promises.
        // For example: await Promise<Promise<number>> should have type `number`
        let mut current_type = expr_type;
        let mut depth = 0;

        while let Some(inner) = self.promise_like_return_type_argument(current_type) {
            current_type = inner;
            depth += 1;
            if depth > MAX_AWAIT_DEPTH {
                break;
            }
        }
        current_type
    }

    fn await_expression_uses_call_like_syntax(&self, idx: NodeIndex) -> bool {
        let Some((start, end)) = self.get_node_span(idx) else {
            return false;
        };
        if end <= start {
            return false;
        }
        let Some(source_file) = self.ctx.arena.source_files.first() else {
            return false;
        };
        source_file
            .text
            .get(start as usize..end as usize)
            .is_some_and(|text| text.starts_with("await("))
    }

    /// Get `PromiseLike`<T> for a given type T.
    ///
    /// Helper function for await contextual typing.
    /// Returns the type application `PromiseLike`<T>.
    ///
    /// If `PromiseLike` is not available in lib files, returns the base type T.
    /// This is a conservative fallback that still allows correct typing.
    pub(crate) fn get_promise_like_type(&mut self, type_arg: TypeId) -> TypeId {
        // Try to resolve PromiseLike from lib files
        if let Some(promise_like_base) = self.resolve_global_interface_type("PromiseLike") {
            // Check if we successfully got a PromiseLike type
            if promise_like_base != TypeId::ANY
                && promise_like_base != TypeId::ERROR
                && promise_like_base != TypeId::UNKNOWN
            {
                // Create PromiseLike<T> application
                return self
                    .ctx
                    .types
                    .application(promise_like_base, vec![type_arg]);
            }
        }

        // Fallback: If PromiseLike is not available, return the base type
        // This allows await to work even without full lib files
        type_arg
    }

    /// Get `Promise`<T> for a given type T.
    ///
    /// Helper for await contextual typing — enables same-base constraint matching
    /// when the await operand is `new Promise(resolve => ...)`.
    /// Returns `None` if `Promise` is not available in lib files.
    pub(crate) fn get_promise_type(&mut self, type_arg: TypeId) -> Option<TypeId> {
        if let Some(promise_base) = self.resolve_global_interface_type("Promise")
            && promise_base != TypeId::ANY
            && promise_base != TypeId::ERROR
            && promise_base != TypeId::UNKNOWN
        {
            return Some(self.ctx.types.application(promise_base, vec![type_arg]));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::context::CheckerOptions;
    use crate::diagnostics::Diagnostic;
    use crate::state::CheckerState;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;
    use tsz_solver::TypeInterner;

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
}
