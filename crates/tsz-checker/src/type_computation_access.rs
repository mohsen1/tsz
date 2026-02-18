//! Element Access, Object Literal, and Await Type Computation
//!
//! This module contains type computation methods for `CheckerState` related to:
//! - Element access expressions (arr[i], obj["prop"])
//! - Super keyword type resolution
//! - Object literal type computation with spreads
//! - Await expression type resolution (Promise unwrapping)
//!
//! Split from `type_computation.rs` for maintainability.

use crate::state::{CheckerState, EnumKind};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{TypeId, Visibility};

const MAX_AWAIT_DEPTH: u32 = 10;

impl<'a> CheckerState<'a> {
    /// Get the type of an element access expression (e.g., arr[0], obj["prop"]).
    ///
    /// Handles element access with optional chaining, index signatures,
    /// and nullish coalescing.
    pub(crate) fn get_type_of_element_access(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_solver::operations_property::PropertyAccessResult;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return TypeId::ERROR;
        };

        // In parse-recovery cases like `number[]`, the bracket argument is
        // missing and TS reports parser error TS1011. The expression before
        // `[` is still a primitive type keyword used as a value and should
        // emit TS2693.
        if access.name_or_argument.is_none()
            && let Some(expr_node) = self.ctx.arena.get(access.expression)
        {
            let keyword_name = if expr_node.kind == SyntaxKind::Identifier as u16 {
                self.ctx.arena.get_identifier(expr_node).and_then(|ident| {
                    match ident.escaped_text.as_str() {
                        "number" => Some("number"),
                        "string" => Some("string"),
                        "boolean" => Some("boolean"),
                        "symbol" => Some("symbol"),
                        "void" => Some("void"),
                        "undefined" => Some("undefined"),
                        "null" => Some("null"),
                        "any" => Some("any"),
                        "unknown" => Some("unknown"),
                        "never" => Some("never"),
                        "object" => Some("object"),
                        "bigint" => Some("bigint"),
                        _ => None,
                    }
                })
            } else {
                match expr_node.kind {
                    k if k == SyntaxKind::NumberKeyword as u16 => Some("number"),
                    k if k == SyntaxKind::StringKeyword as u16 => Some("string"),
                    k if k == SyntaxKind::BooleanKeyword as u16 => Some("boolean"),
                    k if k == SyntaxKind::SymbolKeyword as u16 => Some("symbol"),
                    k if k == SyntaxKind::VoidKeyword as u16 => Some("void"),
                    k if k == SyntaxKind::UndefinedKeyword as u16 => Some("undefined"),
                    k if k == SyntaxKind::NullKeyword as u16 => Some("null"),
                    k if k == SyntaxKind::AnyKeyword as u16 => Some("any"),
                    k if k == SyntaxKind::UnknownKeyword as u16 => Some("unknown"),
                    k if k == SyntaxKind::NeverKeyword as u16 => Some("never"),
                    k if k == SyntaxKind::ObjectKeyword as u16 => Some("object"),
                    k if k == SyntaxKind::BigIntKeyword as u16 => Some("bigint"),
                    _ => None,
                }
            };
            if let Some(keyword_name) = keyword_name {
                self.error_type_only_value_at(keyword_name, access.expression);
                return TypeId::ERROR;
            }
        }

        // Get the type of the object
        let object_type = self.get_type_of_node(access.expression);
        let object_type = self.evaluate_application_type(object_type);

        // Handle optional chain continuations: for `o?.b["c"]`, when processing `["c"]`,
        // the object type from `o?.b` includes `undefined`. Strip nullish types when this
        // element access is a continuation of an optional chain.
        let object_type = if !access.question_dot_token
            && crate::optional_chain::is_optional_chain(self.ctx.arena, access.expression)
        {
            let (non_nullish, _) = self.split_nullish_type(object_type);
            non_nullish.unwrap_or(object_type)
        } else {
            object_type
        };

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
            let property_type =
                self.resolve_global_this_property_type(name, access.name_or_argument);
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

                // Try to get the class name from the type
                let class_name = self
                    .get_class_name_from_type(object_type)
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

        if let Some(name) = literal_string.as_deref() {
            if !self.check_property_accessibility(
                access.expression,
                name,
                access.name_or_argument,
                object_type,
            ) {
                return TypeId::ERROR;
            }
        } else if let Some(index) = literal_index {
            let name = index.to_string();
            if !self.check_property_accessibility(
                access.expression,
                &name,
                access.name_or_argument,
                object_type,
            ) {
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

        let index_type = self.get_type_of_node(access.name_or_argument);

        // Propagate error from index expression to suppress cascading errors
        if index_type == TypeId::ERROR {
            return TypeId::ERROR;
        }

        // TS2538: Type cannot be used as an index type
        if self.type_is_invalid_index_type(index_type) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let index_type_str = self.format_type(index_type);
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

        if result_type.is_none()
            && literal_index.is_none()
            && let Some((string_keys, number_keys)) =
                self.get_literal_key_union_from_type(index_type)
        {
            let total_keys = string_keys.len() + number_keys.len();
            if total_keys > 1 || literal_string_is_none {
                if !string_keys.is_empty() && number_keys.is_empty() {
                    use_index_signature_check = false;
                }

                let mut types = Vec::new();
                if !string_keys.is_empty() {
                    match self.get_element_access_type_for_literal_keys(
                        object_type_for_access,
                        &string_keys,
                    ) {
                        Some(result) => types.push(result),
                        None => report_no_index = true,
                    }
                }

                if !number_keys.is_empty() {
                    match self.get_element_access_type_for_literal_number_keys(
                        object_type_for_access,
                        &number_keys,
                    ) {
                        Some(result) => types.push(result),
                        None => report_no_index = true,
                    }
                }

                if report_no_index {
                    result_type = Some(TypeId::ANY);
                } else if !types.is_empty() {
                    result_type = Some(tsz_solver::utils::union_or_single(self.ctx.types, types));
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
                    // TS2339: Property does not exist on type 'unknown'
                    // Use the same error as TypeScript for property access on unknown
                    self.error_property_not_exist_at(
                        &property_name,
                        object_type_for_access,
                        access.name_or_argument,
                    );
                    Some(TypeId::ERROR)
                }
                PropertyAccessResult::PropertyNotFound { .. } => {
                    // TS2576 parity for element access on instance/super with a static member name.
                    if self.is_super_expression(access.expression)
                        && let Some(ref class_info) = self.ctx.enclosing_class
                        && let Some(base_idx) = self.get_base_class_idx(class_info.class_idx)
                        && self.is_method_member_in_class_hierarchy(base_idx, &property_name, true)
                            == Some(true)
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
                    } else if self.is_super_expression(access.expression)
                        && let Some(ref class_info) = self.ctx.enclosing_class
                        && let Some(base_idx) = self.get_base_class_idx(class_info.class_idx)
                        && self.is_method_member_in_class_hierarchy(base_idx, &property_name, true)
                            == Some(false)
                    {
                        self.error_property_not_exist_at(
                            &property_name,
                            object_type_for_access,
                            access.name_or_argument,
                        );
                        use_index_signature_check = false;
                        Some(TypeId::ERROR)
                    } else if !self.is_super_expression(access.expression)
                        && let Some((class_idx, is_static_access)) =
                            self.resolve_class_for_access(access.expression, object_type_for_access)
                        && !is_static_access
                        && self.is_method_member_in_class_hierarchy(class_idx, &property_name, true)
                            == Some(true)
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
                    } else if !self.is_super_expression(access.expression)
                        && let Some((class_idx, is_static_access)) =
                            self.resolve_class_for_access(access.expression, object_type_for_access)
                        && !is_static_access
                        && self.is_method_member_in_class_hierarchy(class_idx, &property_name, true)
                            == Some(false)
                    {
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
                    self.error_property_not_exist_at(
                        &property_name,
                        object_type_for_access,
                        access.name_or_argument,
                    );
                    Some(TypeId::ERROR)
                }
                PropertyAccessResult::PropertyNotFound { .. } => None,
            };
        }

        let mut result_type = result_type.unwrap_or_else(|| {
            self.get_element_access_type(object_type_for_access, index_type, literal_index)
        });

        if result_type == TypeId::ERROR
            && let Some(index) = literal_index
            && let Some(tuple_elements) =
                tsz_solver::type_queries::get_tuple_elements(self.ctx.types, object_type_for_access)
        {
            let has_rest_tail = tuple_elements.last().is_some_and(|element| element.rest);
            if !has_rest_tail && index >= tuple_elements.len() {
                self.error_at_node(
                    access.name_or_argument,
                    &format!(
                        "Tuple type of length '{}' has no element at index '{}'.",
                        tuple_elements.len(),
                        index
                    ),
                    crate::diagnostics::diagnostic_codes::TUPLE_TYPE_OF_LENGTH_HAS_NO_ELEMENT_AT_INDEX,
                );
            }
        }

        if use_index_signature_check
            && self.should_report_no_index_signature(
                object_type_for_access,
                index_type,
                literal_index,
            )
        {
            report_no_index = true;
        }

        if report_no_index {
            // Suppress TS7053 for expando bracket assignments on function types.
            // When `func["prop"] = value` and the object is callable, tsc does not emit
            // TS7053 — it treats this as a valid JS-style property expansion.
            // We detect write context via `skip_flow_narrowing` which is set by
            // `get_type_of_assignment_target`.
            let is_expando_function_write = self.ctx.skip_flow_narrowing
                && tsz_solver::visitor::is_function_type(self.ctx.types, object_type_for_access);
            if !is_expando_function_write {
                self.error_no_index_signature_at(index_type, object_type, access.name_or_argument);
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
            if !extends_expr_idx.is_none()
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

        if !extends_expr_idx.is_none()
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

    /// Get the type of a node with a fallback.
    ///
    /// Returns the computed type, or the fallback if the computed type is ERROR.
    pub fn get_type_of_node_or(&mut self, idx: NodeIndex, fallback: TypeId) -> TypeId {
        let ty = self.get_type_of_node(idx);
        if ty == TypeId::ERROR { fallback } else { ty }
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
        let mut has_spread = false;
        // Track getter/setter names to allow getter+setter pairs with the same name
        let mut getter_names: rustc_hash::FxHashSet<Atom> = rustc_hash::FxHashSet::default();
        let mut setter_names: rustc_hash::FxHashSet<Atom> = rustc_hash::FxHashSet::default();
        // Track which named properties came from explicit assignments (not spreads)
        // so we can emit TS2783 when a later spread overwrites them.
        // Maps property name atom -> (node_idx for error, property display name)
        let mut named_property_nodes: FxHashMap<Atom, (NodeIndex, String)> = FxHashMap::default();

        // Skip duplicate property checks for destructuring assignment targets.
        // `({ x, y: y1, "y": y1 } = obj)` is valid - same property extracted twice.
        let skip_duplicate_check = self.ctx.in_destructuring_target;

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
                self.get_property_name(accessor.name)
            })
            .collect();

        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Property assignment: { x: value }
            if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                if let Some(name) = self.get_property_name(prop.name) {
                    // Get contextual type for this property.
                    // For mapped/conditional/application types that contain Lazy references
                    // (e.g. { [K in keyof Props]: Props[K] } after generic inference),
                    // evaluate them with the full resolver first so the solver can
                    // extract property types from the resulting concrete object type.
                    let property_context_type = if let Some(ctx_type) = self.ctx.contextual_type {
                        let ctx_type = self.evaluate_contextual_type(ctx_type);
                        self.ctx.types.contextual_property_type(ctx_type, &name)
                    } else {
                        None
                    };

                    // Set contextual type for property value
                    let prev_context = self.ctx.contextual_type;
                    self.ctx.contextual_type = property_context_type;

                    // When the parser can't parse a value expression (e.g. `{ a: return; }`),
                    // it uses the property NAME node as the fallback initializer for error
                    // recovery (prop.initializer == prop.name). Skip type-checking in that
                    // case to prevent a spurious TS2304 for the property name identifier.
                    let value_type = if prop.initializer == prop.name {
                        TypeId::ANY
                    } else {
                        self.get_type_of_node(prop.initializer)
                    };

                    // Restore context
                    self.ctx.contextual_type = prev_context;

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
                    let value_type =
                        if !self.ctx.in_const_assertion && property_context_type.is_none() {
                            self.widen_literal_type(value_type)
                        } else {
                            value_type
                        };

                    // TS7008: Member implicitly has an 'any' type
                    // Report this error when noImplicitAny is enabled, the object literal has a contextual type,
                    // and the property value type is 'any'
                    if self.ctx.no_implicit_any()
                        && prev_context.is_some()
                        && value_type == TypeId::ANY
                    {
                        let message = format_message(
                            diagnostic_messages::MEMBER_IMPLICITLY_HAS_AN_TYPE,
                            &[&name, "any"],
                        );
                        self.error_at_node(
                            prop.name,
                            &message,
                            diagnostic_codes::MEMBER_IMPLICITLY_HAS_AN_TYPE,
                        );
                    }

                    let name_atom = self.ctx.types.intern_string(&name);

                    // Check for duplicate property (skip in destructuring targets)
                    // TS1117: TypeScript always checks for duplicate properties regardless of target
                    if !skip_duplicate_check && properties.contains_key(&name_atom) {
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

                    // Track this named property for TS2783 spread-overwrite checking
                    named_property_nodes.insert(name_atom, (prop.name, name.clone()));

                    properties.insert(
                        name_atom,
                        PropertyInfo {
                            name: name_atom,
                            type_id: value_type,
                            write_type: value_type,
                            optional: false,
                            readonly: false,
                            is_method: false,
                            visibility: Visibility::Public,
                            parent_id: None,
                        },
                    );
                } else {
                    // Computed property name that can't be statically resolved (e.g., { [expr]: value })
                    // Still type-check the computed expression and the value to catch errors like TS2304.
                    // For contextual typing, use the index signature type from the contextual type.
                    // E.g., `var o: { [s: string]: (x: string) => number } = { ["" + 0](y) { ... } }`
                    // should contextually type `y` as `string` from the string index signature.
                    self.check_computed_property_name(prop.name);
                    let index_ctx_type = if let Some(ctx_type) = self.ctx.contextual_type {
                        let ctx_type = self.evaluate_contextual_type(ctx_type);
                        // Use a synthetic name that won't match any named property,
                        // causing contextual_property_type to fall back to the index signature.
                        self.ctx
                            .types
                            .contextual_property_type(ctx_type, "__@computed")
                    } else {
                        None
                    };
                    let prev_context = self.ctx.contextual_type;
                    self.ctx.contextual_type = index_ctx_type;
                    self.get_type_of_node(prop.initializer);
                    self.ctx.contextual_type = prev_context;
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
                        let ctx_type = self.evaluate_contextual_type(ctx_type);
                        self.ctx.types.contextual_property_type(ctx_type, &name)
                    } else {
                        None
                    };

                    // Set contextual type for shorthand property value
                    let prev_context = self.ctx.contextual_type;
                    self.ctx.contextual_type = property_context_type;

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

                        if !is_strict_reserved && !is_obviously_invalid_name {
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
                        self.get_type_of_node(elem_idx)
                    };

                    // Restore context
                    self.ctx.contextual_type = prev_context;

                    // Apply bidirectional type inference - use contextual type to narrow the value type
                    let value_type = tsz_solver::apply_contextual_type(
                        self.ctx.types,
                        value_type,
                        property_context_type,
                    );

                    // Widen literal types for shorthand properties (same as named properties)
                    let value_type =
                        if !self.ctx.in_const_assertion && property_context_type.is_none() {
                            self.widen_literal_type(value_type)
                        } else {
                            value_type
                        };

                    // TS7008: Member implicitly has an 'any' type
                    // Report this error when noImplicitAny is enabled, the object literal has a contextual type,
                    // and the shorthand property value type is 'any'
                    if self.ctx.no_implicit_any()
                        && prev_context.is_some()
                        && value_type == TypeId::ANY
                    {
                        let message = format_message(
                            diagnostic_messages::MEMBER_IMPLICITLY_HAS_AN_TYPE,
                            &[&name, "any"],
                        );
                        self.error_at_node(
                            elem_idx,
                            &message,
                            diagnostic_codes::MEMBER_IMPLICITLY_HAS_AN_TYPE,
                        );
                    }

                    let name_atom = self.ctx.types.intern_string(&name);

                    // Check for duplicate property (skip in destructuring targets)
                    // TS1117: tsc only emits this for ES5 and earlier targets.
                    if !skip_duplicate_check
                        && properties.contains_key(&name_atom)
                        && (self.ctx.compiler_options.target as u32)
                            < (tsz_common::common::ScriptTarget::ES2015 as u32)
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

                    // Track this shorthand property for TS2783 spread-overwrite checking
                    named_property_nodes.insert(name_atom, (elem_idx, name.clone()));

                    properties.insert(
                        name_atom,
                        PropertyInfo {
                            name: name_atom,
                            type_id: value_type,
                            write_type: value_type,
                            optional: false,
                            readonly: false,
                            is_method: false,
                            visibility: Visibility::Public,
                            parent_id: None,
                        },
                    );
                } else if let Some(shorthand) = self.ctx.arena.get_shorthand_property(elem_node) {
                    self.check_computed_property_name(shorthand.name);
                }
            }
            // Method shorthand: { foo() {} }
            else if let Some(method) = self.ctx.arena.get_method_decl(elem_node) {
                if let Some(name) = self.get_property_name(method.name) {
                    // Set contextual type for method
                    let prev_context = self.ctx.contextual_type;
                    if let Some(ctx_type) = prev_context {
                        let ctx_type = self.evaluate_contextual_type(ctx_type);
                        self.ctx.contextual_type =
                            self.ctx.types.contextual_property_type(ctx_type, &name);
                    }

                    // If no explicit ThisType marker exists, use the object literal's
                    // contextual type as `this` inside method bodies.
                    let mut pushed_contextual_this = false;
                    if marker_this_type.is_none()
                        && self.current_this_type().is_none()
                        && let Some(ctx_type) = prev_context
                    {
                        let ctx_type = self.evaluate_contextual_type(ctx_type);
                        self.ctx.this_type_stack.push(ctx_type);
                        pushed_contextual_this = true;
                    }

                    let method_type = self.get_type_of_function(elem_idx);

                    if pushed_contextual_this {
                        self.ctx.this_type_stack.pop();
                    }

                    // Restore context
                    self.ctx.contextual_type = prev_context;

                    let name_atom = self.ctx.types.intern_string(&name);

                    // Check for duplicate property (skip in destructuring targets)
                    // TS1117: tsc only emits this for ES5 and earlier targets.
                    if !skip_duplicate_check
                        && properties.contains_key(&name_atom)
                        && (self.ctx.compiler_options.target as u32)
                            < (tsz_common::common::ScriptTarget::ES2015 as u32)
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

                    properties.insert(
                        name_atom,
                        PropertyInfo {
                            name: name_atom,
                            type_id: method_type,
                            write_type: method_type,
                            optional: false,
                            readonly: false,
                            is_method: true, // Object literal methods should be bivariant
                            visibility: Visibility::Public,
                            parent_id: None,
                        },
                    );
                } else {
                    // Computed method name - still type-check the expression and function body.
                    // For contextual typing, use the index signature type from the contextual type.
                    // E.g., `var o: { [s: string]: (x: string) => number } = { ["" + 0](y) { ... } }`
                    // should contextually type `y` as `string` from the string index signature.
                    self.check_computed_property_name(method.name);
                    let prev_context = self.ctx.contextual_type;
                    if let Some(ctx_type) = prev_context {
                        let ctx_type = self.evaluate_contextual_type(ctx_type);
                        self.ctx.contextual_type = self
                            .ctx
                            .types
                            .contextual_property_type(ctx_type, "__@computed");
                    }
                    self.get_type_of_function(elem_idx);
                    self.ctx.contextual_type = prev_context;
                }
            }
            // Accessor: { get foo() {} } or { set foo(v) {} }
            else if let Some(accessor) = self.ctx.arena.get_accessor(elem_node) {
                // Check for missing body - error 1005 at end of accessor
                if accessor.body.is_none() {
                    use crate::diagnostics::diagnostic_codes;
                    // Report at accessor.end - 1 (pointing to the closing paren)
                    let end_pos = elem_node.end.saturating_sub(1);
                    self.error_at_position(end_pos, 1, "'{' expected.", diagnostic_codes::EXPECTED);
                }

                // For setters, check implicit any on parameters (error 7006) and on
                // the property name itself (error 7032).
                // When a paired getter exists, the setter parameter type is inferred
                // from the getter return type (contextually typed, suppress TS7006/7032).
                if elem_node.kind == syntax_kind_ext::SET_ACCESSOR {
                    let has_paired_getter = self
                        .get_property_name(accessor.name)
                        .is_some_and(|name| obj_getter_names.contains(&name));
                    // Check if accessor JSDoc has @param type annotations
                    let accessor_jsdoc = self.get_jsdoc_for_function(elem_idx);
                    let mut first_param_lacks_annotation = false;
                    for &param_idx in &accessor.parameters.nodes {
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
                            self.maybe_report_implicit_any_parameter(param, has_jsdoc);
                        }
                    }
                    // TS7032: emit on property name when the setter has no parameter type
                    // annotation and no paired getter (TSC checks this at accessor symbol
                    // resolution time; we emit it here during object literal checking).
                    if first_param_lacks_annotation
                        && !has_paired_getter
                        && self.ctx.no_implicit_any()
                        && let Some(prop_name) = self.get_property_name(accessor.name).as_deref()
                    {
                        use crate::diagnostics::diagnostic_codes;
                        self.error_at_node_msg(
                            accessor.name,
                            diagnostic_codes::PROPERTY_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_ITS_SET_ACCESSOR_LACKS_A_PARAMETER_TYPE,
                            &[prop_name],
                        );
                    }
                }

                if let Some(name) = self.get_property_name(accessor.name) {
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
                            this_props.push(PropertyInfo {
                                name: name_atom,
                                type_id: TypeId::ANY,
                                write_type: TypeId::ANY,
                                optional: false,
                                readonly: false,
                                is_method: false,
                                visibility: Visibility::Public,
                                parent_id: None,
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
                                let setter_name = self.get_property_name(accessor.name)?;
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
                            !accessor.type_annotation.is_none(),
                            false,
                            elem_idx,
                        );
                    }

                    // TS2378: A 'get' accessor must return a value.
                    // Check if the getter has a body but no return statement with a value.
                    if elem_node.kind == syntax_kind_ext::GET_ACCESSOR && !accessor.body.is_none() {
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
                    // TS1117: tsc only emits this for ES5 and earlier targets.
                    if !skip_duplicate_check
                        && properties.contains_key(&name_atom)
                        && !is_complementary_pair
                        && (self.ctx.compiler_options.target as u32)
                            < (tsz_common::common::ScriptTarget::ES2015 as u32)
                    {
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

                    if is_getter {
                        getter_names.insert(name_atom);
                    } else {
                        setter_names.insert(name_atom);
                    }

                    // Merge getter/setter into a single property with separate
                    // read (type_id) and write (write_type) types.
                    if let Some(existing) = properties.get(&name_atom) {
                        let (read_type, write_type) = if is_getter {
                            // Getter arriving after setter
                            (accessor_type, existing.write_type)
                        } else {
                            // Setter arriving after getter
                            (existing.type_id, accessor_type)
                        };
                        properties.insert(
                            name_atom,
                            PropertyInfo {
                                name: name_atom,
                                type_id: read_type,
                                write_type,
                                optional: false,
                                readonly: false,
                                is_method: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                            },
                        );
                    } else {
                        properties.insert(
                            name_atom,
                            PropertyInfo {
                                name: name_atom,
                                type_id: accessor_type,
                                write_type: accessor_type,
                                optional: false,
                                readonly: false,
                                is_method: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                            },
                        );
                    }
                } else {
                    // Computed accessor name - still type-check the expression and body
                    self.check_computed_property_name(accessor.name);
                    if elem_node.kind == syntax_kind_ext::GET_ACCESSOR {
                        self.get_type_of_function(elem_idx);

                        // TS2378: A 'get' accessor must return a value.
                        if !accessor.body.is_none() {
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
                    let spread_type = self.get_type_of_node(spread_expr);
                    // TS2698: Spread types may only be created from object types
                    let resolved_spread = self.resolve_type_for_property_access(spread_type);
                    let resolved_spread = self.resolve_lazy_type(resolved_spread);
                    if !tsz_solver::type_queries::is_valid_spread_type(
                        self.ctx.types,
                        resolved_spread,
                    ) {
                        self.report_spread_not_object_type(elem_idx);
                    }
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

                    for prop in spread_props {
                        properties.insert(prop.name, prop);
                    }
                }
            }
            // Other element types (e.g., unknown AST node kinds) are silently skipped
        }

        let properties: Vec<PropertyInfo> = properties.into_values().collect();
        // Object literals with spreads are not fresh (no excess property checking)
        let object_type = if has_spread {
            self.ctx.types.factory().object(properties)
        } else {
            self.ctx.types.factory().object_fresh(properties)
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

    // =========================================================================
    // Await Expression Type Computation
    // =========================================================================

    /// Get the type of an await expression with contextual typing support.
    ///
    /// Phase 6 Task 3: Propagate contextual type to await operand.
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

        // Phase 6 Task 3: Propagate contextual type to await operand
        // If we have a contextual type T, transform it to T | PromiseLike<T>
        let prev_context = self.ctx.contextual_type;
        if let Some(contextual) = prev_context {
            // Skip transformation for error types, any, unknown, or never
            if contextual != TypeId::ANY
                && contextual != TypeId::UNKNOWN
                && contextual != TypeId::NEVER
                && !self.type_contains_error(contextual)
            {
                // Create PromiseLike<T> type
                let promise_like_t = self.get_promise_like_type(contextual);
                // Create union: T | PromiseLike<T>
                let union_context = self
                    .ctx
                    .types
                    .factory()
                    .union(vec![contextual, promise_like_t]);
                // Set the union as the contextual type for the operand
                self.ctx.contextual_type = Some(union_context);
            }
        }

        // Get the type of the await operand with transformed contextual type
        let expr_type = self.get_type_of_node(unary.expression);

        // Restore the original contextual type
        self.ctx.contextual_type = prev_context;

        // Phase 6 Task 3: Recursively unwrap Promise<T> to get T (simulating Awaited<T>)
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
    fn get_promise_like_type(&mut self, type_arg: TypeId) -> TypeId {
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
}
