//! Property Access Type Resolution
//!
//! Split from `function_type.rs` to keep file sizes manageable.
//! Contains property access type resolution, global augmentation
//! property lookup, and expando function pattern detection.

use crate::state::{CheckerState, MAX_INSTANTIATION_DEPTH};
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Get type of property access expression.
    pub(crate) fn get_type_of_property_access(&mut self, idx: NodeIndex) -> TypeId {
        if *self.ctx.instantiation_depth.borrow() >= MAX_INSTANTIATION_DEPTH {
            return TypeId::ERROR; // Max instantiation depth exceeded - propagate error
        }

        *self.ctx.instantiation_depth.borrow_mut() += 1;
        let result = self.get_type_of_property_access_inner(idx);
        *self.ctx.instantiation_depth.borrow_mut() -= 1;
        result
    }

    /// Inner implementation of property access type resolution.
    fn get_type_of_property_access_inner(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_solver::operations_property::PropertyAccessResult;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return TypeId::ERROR; // Missing access expression data - propagate error
        };
        let factory = self.ctx.types.factory();

        // Get the property name first (needed for abstract property check regardless of object type)
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            // Preserve diagnostics on the base expression (e.g. TS2304 for `missing.`)
            // even when parser recovery could not build a property name node.
            let _ = self.get_type_of_node(access.expression);
            return TypeId::ERROR;
        };
        if let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && ident.escaped_text.is_empty()
        {
            // Preserve diagnostics on the base expression when member name is missing.
            let _ = self.get_type_of_node(access.expression);
            return TypeId::ERROR;
        }

        // Check for abstract property access in constructor BEFORE evaluating types (error 2715)
        // This must happen even when `this` has type ANY
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;

            if self.is_this_expression(access.expression)
                && let Some(ref class_info) = self.ctx.enclosing_class.clone()
                && class_info.in_constructor
                && self.ctx.function_depth == 0  // Skip inside nested functions/arrow functions
                && self.is_abstract_member(&class_info.member_nodes, property_name)
            {
                self.error_abstract_property_in_constructor(
                    property_name,
                    &class_info.name,
                    access.name_or_argument,
                );
            }
        }

        // Fast path for enum member value access (`E.Member`).
        // This avoids the general property-access pipeline (accessibility checks,
        // type environment classification, etc.) for a very common hot path.
        if let Some(name_ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &name_ident.escaped_text;
            if let Some(base_sym_id) = self.resolve_identifier_symbol(access.expression)
                && let Some(base_symbol) = self.ctx.binder.get_symbol(base_sym_id)
                && base_symbol.flags & symbol_flags::ENUM != 0
                && let Some(exports) = base_symbol.exports.as_ref()
                && let Some(member_sym_id) = exports.get(property_name)
            {
                // TS2450: Check if enum is used before its declaration (TDZ violation).
                // Only non-const enums are flagged (const enums are always hoisted).
                if let Some(base_node) = self.ctx.arena.get(access.expression)
                    && let Some(base_ident) = self.ctx.arena.get_identifier(base_node)
                {
                    let base_name = &base_ident.escaped_text;
                    if self.check_tdz_violation(base_sym_id, access.expression, base_name) {
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
        let prev_skip = self.ctx.skip_flow_narrowing;
        self.ctx.skip_flow_narrowing = false;
        let original_object_type = self.get_type_of_node(access.expression);
        self.ctx.skip_flow_narrowing = prev_skip;

        // Evaluate Application types to resolve generic type aliases/interfaces
        // But preserve original for error messages to maintain nominal identity (e.g., D<string>)
        let object_type = self.evaluate_application_type(original_object_type);

        // Handle optional chain continuations: for `o?.b.c`, when processing `.c`,
        // the object type from `o?.b` includes `undefined` from the optional chain.
        // But `.c` should only be reached when `o` is defined, so we strip nullish
        // types. Only do this when this access is NOT itself an optional chain
        // (`question_dot_token` is false) but is part of one (parent has `?.`).
        let object_type = if !access.question_dot_token
            && crate::optional_chain::is_optional_chain(self.ctx.arena, access.expression)
        {
            let (non_nullish, _) = self.split_nullish_type(object_type);
            non_nullish.unwrap_or(object_type)
        } else {
            object_type
        };

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
                    self.error_property_not_exist_at(property_name, TypeId::NEVER, idx);
                }
            }
            return TypeId::NEVER;
        }

        // Enforce private/protected access modifiers when possible
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;
            if !self.check_property_accessibility(
                access.expression,
                property_name,
                access.name_or_argument,
                object_type,
            ) {
                return TypeId::ERROR;
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
            if self.is_namespace_value_type(object_type)
                && !self.is_enum_instance_property_access(object_type, access.expression)
            {
                if !access.question_dot_token && !property_name.starts_with('#') {
                    self.error_property_not_exist_at(property_name, original_object_type, idx);
                }
                return TypeId::ERROR;
            }

            let object_type_for_access = self.resolve_type_for_property_access(object_type);
            if object_type_for_access == TypeId::ANY {
                return TypeId::ANY;
            }
            if object_type_for_access == TypeId::ERROR {
                return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
            }

            if self.ctx.strict_bind_call_apply()
                && let Some(strict_method_type) =
                    self.strict_bind_call_apply_method_type(object_type_for_access, property_name)
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
                    // Check for error 4111: property access from index signature
                    if from_index_signature
                        && self
                            .ctx
                            .compiler_options
                            .no_property_access_from_index_signature
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
                    if !self.ctx.skip_flow_narrowing
                        && self.should_skip_property_result_flow_narrowing(idx)
                    {
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
                    if tsz_solver::type_queries::is_function_type(
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
                    // In JS checkJs mode, CommonJS `module.exports` accesses are valid.
                    if property_name == "exports"
                        && (self.ctx.file_name.ends_with(".js")
                            || self.ctx.file_name.ends_with(".jsx"))
                        && let Some(obj_node) = self.ctx.arena.get(access.expression)
                        && let Some(ident) = self.ctx.arena.get_identifier(obj_node)
                        && ident.escaped_text == "module"
                    {
                        return TypeId::ANY;
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
                    let is_js_file =
                        self.ctx.file_name.ends_with(".js") || self.ctx.file_name.ends_with(".jsx");
                    let is_this_access =
                        if let Some(obj_node) = self.ctx.arena.get(access.expression) {
                            obj_node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16
                        } else {
                            false
                        };

                    if is_js_file && is_this_access {
                        // Allow dynamic property on 'this' in JavaScript files
                        return TypeId::ANY;
                    }

                    // TS2576: super.member where `member` exists on the base class static side.
                    if self.is_super_expression(access.expression)
                        && let Some(ref class_info) = self.ctx.enclosing_class
                        && let Some(base_idx) = self.get_base_class_idx(class_info.class_idx)
                        && self.is_method_member_in_class_hierarchy(base_idx, property_name, true)
                            == Some(true)
                    {
                        use crate::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };

                        let base_name = self.get_class_name_from_decl(base_idx);
                        let static_member_name = format!("{base_name}.{property_name}");
                        let object_type_str = self.format_type(original_object_type);
                        let message = format_message(
                            diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN_TO_ACCESS_THE_STATIC_MEMBER_INSTEAD,
                            &[property_name, &object_type_str, &static_member_name],
                        );
                        self.error_at_node(
                            idx,
                            &message,
                            diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN_TO_ACCESS_THE_STATIC_MEMBER_INSTEAD,
                        );
                        return TypeId::ERROR;
                    }

                    // TS2576: instance.member where `member` exists on the class static side.
                    if !self.is_super_expression(access.expression)
                        && let Some((class_idx, is_static_access)) =
                            self.resolve_class_for_access(access.expression, object_type_for_access)
                        && !is_static_access
                        && self.is_method_member_in_class_hierarchy(class_idx, property_name, true)
                            == Some(true)
                    {
                        use crate::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };

                        let class_name = self.get_class_name_from_decl(class_idx);
                        let static_member_name = format!("{class_name}.{property_name}");
                        let object_type_str = self.format_type(original_object_type);
                        let message = format_message(
                            diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN_TO_ACCESS_THE_STATIC_MEMBER_INSTEAD,
                            &[property_name, &object_type_str, &static_member_name],
                        );
                        self.error_at_node(
                            idx,
                            &message,
                            diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN_TO_ACCESS_THE_STATIC_MEMBER_INSTEAD,
                        );
                        return TypeId::ERROR;
                    }

                    // Don't emit TS2339 for private fields (starting with #) - they're handled elsewhere
                    if !property_name.starts_with('#') {
                        // Property access expressions are VALUE context - always emit TS2339.
                        // TS2694 (namespace has no exported member) is for TYPE context only,
                        // which is handled separately in type name resolution.
                        // Use original_object_type to preserve nominal identity (e.g., D<string>)
                        self.error_property_not_exist_at(property_name, original_object_type, idx);
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
                    // TS2339: Property does not exist on type 'unknown'
                    // Use the same error as TypeScript for property access on unknown
                    self.error_property_not_exist_at(property_name, object_type_for_access, idx);
                    TypeId::ERROR
                }
            }
        } else {
            TypeId::ANY
        }
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
        let other = self.skip_parenthesized_expression(other);
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

    fn resolve_array_global_augmentation_property(
        &mut self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        use rustc_hash::FxHashMap;
        use std::sync::Arc;
        use tsz_lowering::TypeLowering;
        use tsz_parser::parser::NodeArena;
        use tsz_parser::parser::node::NodeAccess;
        use tsz_solver::is_compiler_managed_type;
        use tsz_solver::operations_property::PropertyAccessResult;
        use tsz_solver::type_queries::{
            get_array_element_type, get_tuple_elements, get_type_application, unwrap_readonly,
        };

        let base_type = unwrap_readonly(self.ctx.types, object_type);

        let element_type = if let Some(elem) = get_array_element_type(self.ctx.types, base_type) {
            Some(elem)
        } else if let Some(elems) = get_tuple_elements(self.ctx.types, base_type) {
            let mut members = Vec::new();
            for elem in elems {
                let mut ty = if elem.rest {
                    get_array_element_type(self.ctx.types, elem.type_id).unwrap_or(elem.type_id)
                } else {
                    elem.type_id
                };
                if elem.optional {
                    ty = self.ctx.types.factory().union(vec![ty, TypeId::UNDEFINED]);
                }
                members.push(ty);
            }
            Some(self.ctx.types.factory().union(members))
        } else if let Some(app) = get_type_application(self.ctx.types, base_type) {
            app.args.first().copied()
        } else {
            None
        }?;

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
            let lowering = TypeLowering::with_hybrid_resolver(
                arena.as_ref(),
                self.ctx.types,
                &resolver,
                &def_id_resolver,
                &|_| None,
            );
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
        use tsz_solver::type_queries;

        // Map the object type to potential global interface names
        let interface_names: &[&str] = if type_queries::is_boolean_type(self.ctx.types, object_type)
        {
            &["Boolean"]
        } else if type_queries::is_number_type(self.ctx.types, object_type) {
            &["Number"]
        } else if type_queries::is_string_type(self.ctx.types, object_type) {
            &["String"]
        } else if type_queries::is_symbol_type(self.ctx.types, object_type) {
            &["Symbol"]
        } else if type_queries::is_bigint_type(self.ctx.types, object_type) {
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
        let def_id = tsz_solver::type_queries_extended::get_def_id(self.ctx.types, object_type)?;

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
        use tsz_solver::operations_property::PropertyAccessResult;

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
            let lowering = TypeLowering::with_hybrid_resolver(
                arena.as_ref(),
                self.ctx.types,
                &resolver,
                &def_id_resolver,
                &|_| None,
            );
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
            Some(ext) if !ext.parent.is_none() => ext.parent,
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

        // Check if the object expression is an identifier bound to a function/class declaration
        let Some(expr_node) = self.ctx.arena.get(object_expr_idx) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(expr_node) else {
            return false;
        };

        // Look up the symbol - try file_locals first, then full scope resolution
        let sym_id = self
            .ctx
            .binder
            .file_locals
            .get(&ident.escaped_text)
            .or_else(|| self.resolve_identifier_symbol(object_expr_idx));

        if let Some(sym_id) = sym_id
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            return (symbol.flags & (symbol_flags::FUNCTION | symbol_flags::CLASS)) != 0;
        }

        false
    }

    /// Check if a property access reads an expando property assigned via `X.prop = value`.
    fn is_expando_property_read(&self, object_expr_idx: NodeIndex, property_name: &str) -> bool {
        let Some(expr_node) = self.ctx.arena.get(object_expr_idx) else {
            return false;
        };
        if expr_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(ident) = self.ctx.arena.get_identifier(expr_node) else {
            return false;
        };
        self.ctx
            .binder
            .expando_properties
            .get(&ident.escaped_text)
            .is_some_and(|props| props.contains(property_name))
    }

    fn strict_bind_call_apply_method_type(
        &self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        if property_name != "apply" {
            return None;
        }

        let factory = self.ctx.types.factory();
        use tsz_solver::type_queries::{get_callable_shape, get_function_shape};

        let (params, return_type) =
            if let Some(shape) = get_function_shape(self.ctx.types, object_type) {
                (shape.params.clone(), shape.return_type)
            } else if let Some(shape) = get_callable_shape(self.ctx.types, object_type) {
                let sig = shape.call_signatures.first()?;
                (sig.params.clone(), sig.return_type)
            } else {
                return None;
            };

        let tuple_elements: Vec<tsz_solver::TupleElement> = params
            .iter()
            .map(|param| tsz_solver::TupleElement {
                type_id: param.type_id,
                name: param.name,
                optional: param.optional,
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
}
