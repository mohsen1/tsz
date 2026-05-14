//! Advanced Type Node Handlers
//!
//! This module contains handlers for advanced/derived type constructs:
//! - Type operators (readonly, keyof, unique)
//! - Indexed access types (T[K], Person["name"])
//! - Type queries (typeof X)
//! - Mapped types ({ [P in K]: T })

mod enum_indexed_access;
mod indexed_access_fast_path;

use super::type_node::TypeNodeChecker;
use super::type_node_helpers::{
    get_string_literal_from_type_index, is_type_query_in_non_flow_sensitive_signature_parameter,
    is_typeof_global_this_type_node,
};
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;
use tsz_solver::{ObjectShape, PropertyInfo, TypeId};

impl<'a, 'ctx> TypeNodeChecker<'a, 'ctx> {
    // =========================================================================
    // Type Operators
    // =========================================================================

    /// Get type from a type operator node (readonly T[], readonly [T, U], unique symbol).
    ///
    /// Handles type modifiers like:
    /// - `readonly T[]` - Creates `ReadonlyType` wrapper
    /// - `unique symbol` - Special marker for unique symbols
    pub(super) fn get_type_from_type_operator(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_scanner::SyntaxKind;
        let factory = self.ctx.types.factory();

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        if let Some(type_op) = self.ctx.arena.get_type_operator(node) {
            let operator = type_op.operator;
            let inner_type = self.check(type_op.type_node);

            // Handle readonly operator
            if operator == SyntaxKind::ReadonlyKeyword as u16 {
                // TS1354: 'readonly' type modifier is only permitted on array and tuple literal types.
                if let Some(operand_node) = self.ctx.arena.get(type_op.type_node) {
                    let operand_kind = operand_node.kind;
                    if operand_kind != syntax_kind_ext::ARRAY_TYPE
                        && operand_kind != syntax_kind_ext::TUPLE_TYPE
                    {
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                        self.ctx.error(
                            node.pos,
                            node.end.saturating_sub(node.pos),
                            diagnostic_messages::READONLY_TYPE_MODIFIER_IS_ONLY_PERMITTED_ON_ARRAY_AND_TUPLE_LITERAL_TYPES.to_string(),
                            diagnostic_codes::READONLY_TYPE_MODIFIER_IS_ONLY_PERMITTED_ON_ARRAY_AND_TUPLE_LITERAL_TYPES,
                        );
                    }
                }
                return factory.readonly_type(inner_type);
            }

            // Handle keyof operator
            if operator == SyntaxKind::KeyOfKeyword as u16 {
                return factory.keyof(inner_type);
            }

            // Handle unique operator
            if operator == SyntaxKind::UniqueKeyword as u16 {
                // unique is handled differently - it's a type modifier for symbols
                // For now, just return the inner type
                return inner_type;
            }

            // Unknown operator - return inner type
            inner_type
        } else {
            TypeId::ERROR
        }
    }

    // =========================================================================
    // Indexed Access Types
    // =========================================================================

    /// Handle indexed access type nodes (e.g., `Person["name"]`, `T[K]`).
    pub(super) fn get_type_from_indexed_access_type(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };
        let factory = self.ctx.types.factory();

        if let Some(indexed_access) = self.ctx.arena.get_indexed_access_type(node) {
            if let Some(fast_result) = self.try_fast_alias_union_literal_index_access(
                indexed_access.object_type,
                indexed_access.index_type,
            ) {
                return fast_result;
            }

            let object_type = self.check(indexed_access.object_type);
            let index_type = self.check(indexed_access.index_type);

            // TS2538: Check if the index type is valid (string, number, symbol, or literal thereof)
            if let Some(invalid_member) = self.get_invalid_index_type_member(index_type) {
                let (diag_pos, diag_len) =
                    self.indexed_access_index_diagnostic_span(node, indexed_access.index_type);
                for member in self.invalid_index_type_diagnostic_members(index_type, invalid_member)
                {
                    let mut formatter = self.ctx.create_type_formatter();
                    let index_type_str = formatter.format(member);
                    let message = crate::diagnostics::format_message(
                        crate::diagnostics::diagnostic_messages::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                        &[&index_type_str],
                    );
                    self.ctx.error(diag_pos, diag_len, message, 2538);
                }
            }

            if let Some(inode) = self.ctx.arena.get(indexed_access.index_type)
                && let Some(index_value) = self
                    .get_number_value_from_type_node(indexed_access.index_type)
                    .or_else(|| {
                        crate::query_boundaries::common::number_literal_value(
                            self.ctx.types,
                            index_type,
                        )
                    })
                && index_value.is_finite()
                && index_value.fract() == 0.0
                && index_value < 0.0
            {
                let object_for_tuple_check = self.resolve_object_for_tuple_check(object_type);
                if crate::query_boundaries::common::is_tuple_type(
                    self.ctx.types,
                    object_for_tuple_check,
                ) {
                    let message = crate::diagnostics::diagnostic_messages::
                        A_TUPLE_TYPE_CANNOT_BE_INDEXED_WITH_A_NEGATIVE_VALUE
                        .to_string();
                    self.ctx.error(
                        inode.pos,
                        inode.end - inode.pos,
                        message,
                        crate::diagnostics::diagnostic_codes::A_TUPLE_TYPE_CANNOT_BE_INDEXED_WITH_A_NEGATIVE_VALUE,
                    );
                    return TypeId::ERROR;
                }
            }

            let object_is_type_query_node = self
                .ctx
                .arena
                .get(indexed_access.object_type)
                .is_some_and(|node| node.kind == syntax_kind_ext::TYPE_QUERY);
            let indexed_type = factory.index_access(object_type, index_type);
            let evaluated_indexed_type = if object_is_type_query_node
                || !crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    indexed_type,
                ) {
                Some(
                    crate::query_boundaries::state::type_environment::evaluate_type_with_cache(
                        self.ctx.types,
                        &*self.ctx,
                        indexed_type,
                        std::iter::empty(),
                        false,
                        self.ctx.is_declaration_file() || self.ctx.emit_declarations(),
                    ),
                )
            } else {
                None
            };
            if evaluated_indexed_type
                .as_ref()
                .is_some_and(|result| result.depth_exceeded)
            {
                if let Some(object_node) = self.ctx.arena.get(indexed_access.object_type) {
                    self.ctx.error(
                        object_node.pos,
                        object_node.end - object_node.pos,
                        crate::diagnostics::diagnostic_messages::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE
                            .to_string(),
                        crate::diagnostics::diagnostic_codes::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
                    );
                }
                return TypeId::ERROR;
            }

            // TS2493/TS2339: Check positive out-of-bounds index on tuple/union-of-tuples
            if let Some(inode) = self.ctx.arena.get(indexed_access.index_type)
                && let Some(index_value) = self
                    .get_number_value_from_type_node(indexed_access.index_type)
                    .or_else(|| {
                        crate::query_boundaries::common::number_literal_value(
                            self.ctx.types,
                            index_type,
                        )
                    })
                && index_value.is_finite()
                && index_value.fract() == 0.0
                && index_value >= 0.0
            {
                let index = index_value as usize;
                let object_for_tuple_check = self.resolve_object_for_tuple_check(object_type);
                // Single tuple out of bounds → TS2493
                if let Some(tuple_elements) =
                    crate::query_boundaries::type_computation::access::tuple_elements(
                        self.ctx.types,
                        object_for_tuple_check,
                    )
                {
                    let has_rest = tuple_elements.iter().any(|e| e.rest);
                    if !has_rest && index >= tuple_elements.len() {
                        let mut formatter = self.ctx.create_type_formatter();
                        let tuple_type_str = formatter.format(object_for_tuple_check);
                        let message = format!(
                            "Tuple type '{}' of length '{}' has no element at index '{}'.",
                            tuple_type_str,
                            tuple_elements.len(),
                            index,
                        );
                        self.ctx.error(
                            inode.pos,
                            inode.end - inode.pos,
                            message,
                            crate::diagnostics::diagnostic_codes::TUPLE_TYPE_OF_LENGTH_HAS_NO_ELEMENT_AT_INDEX,
                        );
                    }
                }
                // Union of tuples all out of bounds → TS2339
                // But suppress if object type is ANY/ERROR/conditional/generic (circular reference implicit any)
                else if {
                    let object_is_deferred_or_generic = object_type == TypeId::ANY
                        || object_type == TypeId::ERROR
                        || crate::query_boundaries::common::is_error_type(
                            self.ctx.types,
                            object_type,
                        )
                        || crate::query_boundaries::common::is_conditional_type(
                            self.ctx.types,
                            object_type,
                        )
                        || crate::query_boundaries::common::is_conditional_type(
                            self.ctx.types,
                            object_for_tuple_check,
                        )
                        || crate::query_boundaries::common::is_generic_application(
                            self.ctx.types,
                            object_type,
                        )
                        || crate::query_boundaries::common::is_generic_application(
                            self.ctx.types,
                            object_for_tuple_check,
                        )
                        || crate::query_boundaries::common::contains_type_parameters(
                            self.ctx.types,
                            object_for_tuple_check,
                        )
                        || crate::query_boundaries::common::lazy_def_id(
                            self.ctx.types,
                            object_type,
                        )
                        .and_then(|def_id| self.ctx.definition_store.get_body(def_id))
                        .is_some_and(|body| {
                            crate::query_boundaries::common::is_conditional_type(
                                self.ctx.types,
                                body,
                            ) || crate::query_boundaries::common::is_generic_application(
                                self.ctx.types,
                                body,
                            ) || crate::query_boundaries::common::is_index_access_type(
                                self.ctx.types,
                                body,
                            ) || crate::query_boundaries::common::contains_type_parameters(
                                self.ctx.types,
                                body,
                            )
                        });
                    !object_is_deferred_or_generic
                } && let Some(members) = crate::query_boundaries::common::union_members(
                    self.ctx.types,
                    object_for_tuple_check,
                ) {
                    let all_out_of_bounds = !members.is_empty()
                        && members.iter().all(|&m| {
                            if let Some(elems) =
                                crate::query_boundaries::type_computation::access::tuple_elements(
                                    self.ctx.types,
                                    m,
                                )
                            {
                                let has_rest = elems.iter().any(|e| e.rest);
                                !has_rest && index >= elems.len()
                            } else {
                                false
                            }
                        });
                    if all_out_of_bounds {
                        let mut formatter = self.ctx.create_type_formatter();
                        let type_str = formatter.format(object_type);
                        let message = crate::diagnostics::format_message(
                            crate::diagnostics::diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                            &[&index.to_string(), &type_str],
                        );
                        self.ctx.error(
                            inode.pos,
                            inode.end - inode.pos,
                            message,
                            crate::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                        );
                    }
                }
            }

            // Special case: `(typeof globalThis)['key']` — typeof globalThis
            // resolves to ANY in lowering (no synthetic globalThis type), so the
            // solver would not emit any error. But tsc treats typeof globalThis
            // as a specific type whose properties are the globally-visible
            // `var`/`function`/`namespace` bindings. Reject when the key is:
            //   - a block-scoped variable (let/const) — block-scoped bindings
            //     are NOT properties of typeof globalThis;
            //   - a name not bound in the file's global locals at all (e.g.
            //     the key is a quoted ambient-module name like `"mod"`).
            if object_type == TypeId::ANY
                && is_typeof_global_this_type_node(self.ctx.arena, indexed_access.object_type)
            {
                // In type position, the index is a LiteralType wrapping a string literal
                if let Some(key) =
                    get_string_literal_from_type_index(self.ctx.arena, indexed_access.index_type)
                {
                    // Self-reference: `(typeof globalThis)["globalThis"]` is
                    // a valid self-reference to the globalThis type itself.
                    if key.as_str() == "globalThis" {
                        return object_type;
                    }
                    let not_in_locals = self.ctx.binder.file_locals.get(key.as_str()).is_none();
                    let is_block_scoped = self
                        .ctx
                        .binder
                        .file_locals
                        .get(key.as_str())
                        .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                        .is_some_and(|symbol| {
                            symbol.has_any_flags(tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE)
                                && !symbol.has_any_flags(
                                    tsz_binder::symbol_flags::FUNCTION_SCOPED_VARIABLE,
                                )
                        });
                    if not_in_locals || is_block_scoped {
                        if let Some(idx_node) = self.ctx.arena.get(indexed_access.index_type) {
                            let message = crate::diagnostics::format_message(
                                crate::diagnostics::diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                                &[key.as_str(), "typeof globalThis"],
                            );
                            self.ctx.error(
                                idx_node.pos,
                                idx_node.end - idx_node.pos,
                                message,
                                crate::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                            );
                        }
                        return TypeId::ERROR;
                    }
                }
            }

            // TS2339: Check if the property exists on the object type for string literal index access
            // This handles cases like `Color["Red"]` where "Red" is not a property of the Color type
            if let Some(key) =
                get_string_literal_from_type_index(self.ctx.arena, indexed_access.index_type)
            {
                // Skip for type parameters, generic types, and deferred types - let the
                // property access validation at the actual access site handle those cases
                let resolved_object = self.resolve_object_for_tuple_check(object_type);
                let is_type_param = crate::query_boundaries::common::is_type_parameter_like(
                    self.ctx.types,
                    resolved_object,
                );

                // Suppress TS2339 when the object type is ANY or ERROR - this prevents
                // cascading errors when a variable has implicit any due to circular reference
                // (TS7022/TS7024 already reported for the circularity)
                let is_error_or_any = object_type == TypeId::ANY
                    || object_type == TypeId::ERROR
                    || crate::query_boundaries::common::is_error_type(self.ctx.types, object_type);

                // Suppress TS2339 for generic application types (e.g., Options<State, Actions>)
                // where the type arguments are type parameters. When the object type is generic,
                // we can't determine if the property exists until the type is instantiated.
                // Also suppress when the object is a union containing Application members
                // (e.g., AnyConfig = ExtensionConfig<any> | NodeConfig<any> | MarkConfig<any>)
                // since the solver may not resolve properties on generic interface instantiations.
                let is_generic_application =
                    crate::query_boundaries::common::is_generic_application_with_type_params(
                        self.ctx.types,
                        resolved_object,
                    ) || crate::query_boundaries::common::is_generic_application(
                        self.ctx.types,
                        object_type,
                    ) || crate::query_boundaries::common::is_generic_application(
                        self.ctx.types,
                        resolved_object,
                    ) || self.union_contains_application(resolved_object);

                let alias_body_is_deferred =
                    crate::query_boundaries::common::lazy_def_id(self.ctx.types, object_type)
                        .and_then(|def_id| self.ctx.definition_store.get_body(def_id))
                        .is_some_and(|body| {
                            crate::query_boundaries::common::is_conditional_type(
                                self.ctx.types,
                                body,
                            ) || crate::query_boundaries::common::is_generic_application(
                                self.ctx.types,
                                body,
                            ) || crate::query_boundaries::common::is_index_access_type(
                                self.ctx.types,
                                body,
                            ) || crate::query_boundaries::common::contains_type_parameters(
                                self.ctx.types,
                                body,
                            )
                        });

                // Suppress TS2339 when the index type itself contains type parameters.
                // This handles cases like `Options<State, Actions>[Key]` where Key is a type parameter.
                let index_has_type_params =
                    crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        index_type,
                    );

                // Suppress TS2339 when the object type is a Lazy type that may resolve to a generic type.
                // This handles cases where the interface reference needs to be resolved first.
                let is_lazy_with_potential_generic =
                    crate::query_boundaries::common::lazy_def_id(self.ctx.types, resolved_object)
                        .is_some()
                        && crate::query_boundaries::common::contains_type_parameters(
                            self.ctx.types,
                            object_type,
                        );

                // Suppress TS2339 for conditional types (e.g., Parameters<T>) that may not be
                // fully resolvable when T has circular reference
                let is_conditional = crate::query_boundaries::common::is_conditional_type(
                    self.ctx.types,
                    object_type,
                );

                // Suppress TS2339 for indexed access types (e.g., T[keyof T]) where the
                // result type cannot be determined until the type parameter is instantiated.
                let is_index_access = crate::query_boundaries::common::is_index_access_type(
                    self.ctx.types,
                    object_type,
                ) || crate::query_boundaries::common::is_index_access_type(
                    self.ctx.types,
                    resolved_object,
                );

                // Suppress TS2339 when the object type contains unresolved type parameters.
                // E.g., `Cond<T[K]>["foo"]` where T and K are generic.
                let object_has_type_params =
                    crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        object_type,
                    );

                if !is_type_param
                    && !is_error_or_any
                    && !is_generic_application
                    && !alias_body_is_deferred
                    && !index_has_type_params
                    && !is_lazy_with_potential_generic
                    && !is_conditional
                    && !is_index_access
                    && !object_has_type_params
                {
                    let prop_result =
                        crate::query_boundaries::property_access::resolve_property_access(
                            self.ctx.types,
                            resolved_object,
                            &key,
                        );

                    // If property not found and no index signature exists, emit TS2339
                    use crate::query_boundaries::common::PropertyAccessResult;
                    if matches!(prop_result, PropertyAccessResult::PropertyNotFound { .. }) {
                        // Check if there's an index signature that allows this key
                        let has_index_sig = crate::query_boundaries::common::object_shape_for_type(
                            self.ctx.types,
                            resolved_object,
                        )
                        .is_some_and(|shape| {
                            shape.string_index.is_some()
                                || (shape.number_index.is_some() && key.parse::<f64>().is_ok())
                        });

                        if !has_index_sig
                            && let Some(idx_node) = self.ctx.arena.get(indexed_access.index_type)
                        {
                            // When the receiver is a type alias whose body resolves
                            // to an Enum (e.g. `type C1 = Color`), tsc displays the
                            // underlying enum's nominal name in TS2339 messages.
                            // The default formatter would follow the Lazy(DefId) to
                            // the alias name, producing `'C1'` instead of `'Color'`.
                            let alias_enum_name = crate::query_boundaries::common::lazy_def_id(
                                self.ctx.types,
                                object_type,
                            )
                            .and_then(|def_id| self.ctx.definition_store.get(def_id))
                            .filter(|def| def.kind == tsz_solver::def::DefKind::TypeAlias)
                            .and_then(|_| {
                                crate::query_boundaries::common::enum_def_id(
                                    self.ctx.types,
                                    resolved_object,
                                )
                            })
                            .and_then(|enum_def_id| self.ctx.def_to_symbol_id(enum_def_id))
                            .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                            .map(|symbol| symbol.escaped_name.to_string());
                            let type_str = alias_enum_name.unwrap_or_else(|| {
                                let mut formatter = self.ctx.create_type_formatter();
                                formatter.format(object_type).into_owned()
                            });
                            let message = crate::diagnostics::format_message(
                                crate::diagnostics::diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                                &[&key, &type_str],
                            );
                            self.ctx.error(
                                idx_node.pos,
                                idx_node.end - idx_node.pos,
                                message,
                                crate::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                            );
                        }
                    }
                }
            }

            if let Some(evaluated_result) = evaluated_indexed_type {
                let evaluated = evaluated_result.result;
                if evaluated != TypeId::ERROR
                    && evaluated != indexed_type
                    && let Some(parent_enum_type) =
                        self.full_enum_member_union_parent_type(evaluated)
                {
                    return parent_enum_type;
                }
                if evaluated != TypeId::ERROR
                    && evaluated != indexed_type
                    && object_is_type_query_node
                {
                    return evaluated;
                }
            }

            indexed_type
        } else {
            TypeId::ERROR
        }
    }

    /// Check if a type is a union containing Application (generic instantiation) members.
    /// Used to suppress TS2339 when property existence can't be verified on unresolved
    /// generic interface instantiations (e.g., `ExtensionConfig<any> | NodeConfig<any>`).
    fn union_contains_application(&self, type_id: TypeId) -> bool {
        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, type_id)
        {
            members.iter().any(|&m| {
                crate::query_boundaries::common::is_generic_application(self.ctx.types, m)
            })
        } else {
            false
        }
    }

    /// Resolve object type for tuple-related checks (unwrap readonly, follow Lazy).
    fn resolve_object_for_tuple_check(&self, object_type: TypeId) -> TypeId {
        let unwrapped =
            crate::query_boundaries::common::unwrap_readonly(self.ctx.types, object_type);
        if let Some(def_id) =
            crate::query_boundaries::common::lazy_def_id(self.ctx.types, unwrapped)
        {
            let resolved = self
                .ctx
                .type_env
                .try_borrow()
                .ok()
                .and_then(|env| {
                    // For classes, check class_instance_types first (instance type for
                    // type position), then fall back to get_def (constructor/body type).
                    // This matches TypeEnvironment::resolve_lazy behavior and ensures
                    // indexed access types like C["x"] resolve class instance properties.
                    env.get_class_instance_type(def_id)
                        .or_else(|| env.get_def(def_id))
                })
                .or_else(|| self.ctx.definition_store.get_body(def_id))
                .unwrap_or(unwrapped);
            crate::query_boundaries::common::unwrap_readonly(self.ctx.types, resolved)
        } else {
            unwrapped
        }
    }

    fn get_number_value_from_type_node(&self, idx: NodeIndex) -> Option<f64> {
        let node = self.ctx.arena.get(idx)?;

        if node.kind == syntax_kind_ext::LITERAL_TYPE {
            let data = self.ctx.arena.get_literal_type(node)?;
            return self.get_number_value_from_type_node(data.literal);
        }

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
            return self.get_number_value_from_type_node(paren.expression);
        }

        if node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION {
            let data = self.ctx.arena.get_unary_expr(node)?;
            let operand = self.get_number_value_from_type_node(data.operand)?;
            return match data.operator {
                k if k == SyntaxKind::MinusToken as u16 => Some(-operand),
                k if k == SyntaxKind::PlusToken as u16 => Some(operand),
                _ => None,
            };
        }

        None
    }

    /// Get the specific type that makes this type invalid as an index type (TS2538).
    fn get_invalid_index_type_member(&self, type_id: TypeId) -> Option<TypeId> {
        crate::query_boundaries::common::get_invalid_index_type_member(self.ctx.types, type_id)
    }

    fn invalid_index_type_diagnostic_members(
        &self,
        index_type: TypeId,
        invalid_member: TypeId,
    ) -> Vec<TypeId> {
        if invalid_member == TypeId::BOOLEAN
            && crate::query_boundaries::common::union_members(self.ctx.types, index_type).is_some()
        {
            vec![TypeId::BOOLEAN_FALSE, TypeId::BOOLEAN_TRUE]
        } else {
            vec![invalid_member]
        }
    }

    fn indexed_access_index_diagnostic_span(
        &self,
        indexed_access_node: &Node,
        index_type_idx: NodeIndex,
    ) -> (u32, u32) {
        // The index type node's own AST span anchors the diagnostic. The
        // fallback handles the trailing-`]` case (e.g. `any[[]]` where the
        // index type's own text is `[]` and we want to anchor at the opening
        // `[`). No textual outer-bracket search — that would mis-resolve
        // when the object side uses `[]` array notation, e.g.
        // `string[][boolean]` whose first `[` belongs to the array, or
        // `any[[]]` whose last `[` belongs to the inner tuple.
        self.index_type_node_fallback_span(index_type_idx)
            .unwrap_or((
                indexed_access_node.pos,
                indexed_access_node.end - indexed_access_node.pos,
            ))
    }

    fn index_type_node_fallback_span(&self, index_type_idx: NodeIndex) -> Option<(u32, u32)> {
        let node = self.ctx.arena.get(index_type_idx)?;
        let source_file = self.ctx.arena.source_files.first()?;
        let source = source_file.text.as_ref();
        let start = node.pos as usize;
        let end = node.end as usize;
        let text = source.get(start..end)?;
        if let Some(index_text) = text.trim().strip_suffix(']').map(str::trim_end)
            && !index_text.is_empty()
        {
            let leading_ws = text.len() - text.trim_start().len();
            return Some(((start + leading_ws) as u32, index_text.len() as u32));
        }

        Some((node.pos, node.end.saturating_sub(node.pos)))
    }

    // =========================================================================
    // Type Query (typeof)
    // =========================================================================

    fn apply_instantiation_expression_type_arguments(
        &mut self,
        expr_type: TypeId,
        type_arguments: &NodeList,
    ) -> TypeId {
        if self
            .instantiation_expression_applicability_error_type(
                expr_type,
                type_arguments.nodes.len(),
            )
            .is_some()
        {
            return TypeId::ERROR;
        }

        let type_args: Vec<TypeId> = type_arguments
            .nodes
            .iter()
            .map(|&arg_idx| self.check(arg_idx))
            .collect();
        if type_args.is_empty() {
            return expr_type;
        }

        let application = self.ctx.types.application(expr_type, type_args);
        let evaluated = crate::query_boundaries::state::type_environment::evaluate_type_with_cache(
            self.ctx.types,
            &*self.ctx,
            application,
            std::iter::empty(),
            false,
            self.ctx.is_declaration_file() || self.ctx.emit_declarations(),
        )
        .result;
        if evaluated != TypeId::ERROR && evaluated != application {
            evaluated
        } else {
            application
        }
    }

    fn instantiation_expression_applicability_error_type(
        &self,
        expr_type: TypeId,
        type_argument_count: usize,
    ) -> Option<TypeId> {
        if expr_type == TypeId::ERROR || expr_type == TypeId::ANY {
            return None;
        }

        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, expr_type)
        {
            let mut invalid = Vec::new();
            let mut saw_applicable = false;
            let mut saw_signature = false;
            for member in members.iter().copied() {
                let has_applicable =
                    self.instantiation_type_has_applicable_signature(member, type_argument_count);
                saw_applicable |= has_applicable;
                let has_signature = self.instantiation_type_has_signature(member);
                saw_signature |= has_signature;
                if !has_applicable && has_signature {
                    invalid.push(member);
                }
            }
            if saw_applicable && invalid.is_empty() {
                return None;
            }
            if saw_applicable {
                return if invalid.len() == 1 {
                    invalid.first().copied()
                } else {
                    Some(self.ctx.types.union(invalid))
                };
            }
            return if !saw_signature || invalid.len() == members.len() {
                Some(expr_type)
            } else if invalid.len() == 1 {
                invalid.first().copied()
            } else {
                Some(self.ctx.types.union(invalid))
            };
        }

        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, expr_type)
        {
            if members.iter().copied().any(|member| {
                self.instantiation_type_has_applicable_signature(member, type_argument_count)
            }) {
                return None;
            }
            return Some(expr_type);
        }

        if self.instantiation_type_has_applicable_signature(expr_type, type_argument_count) {
            None
        } else {
            Some(expr_type)
        }
    }

    fn instantiation_type_has_applicable_signature(
        &self,
        type_id: TypeId,
        type_argument_count: usize,
    ) -> bool {
        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
            && shape.type_params.len() == type_argument_count
        {
            return true;
        }
        if let Some(sigs) =
            crate::query_boundaries::common::call_signatures_for_type(self.ctx.types, type_id)
            && sigs
                .iter()
                .any(|sig| sig.type_params.len() == type_argument_count)
        {
            return true;
        }
        if let Some(sigs) =
            crate::query_boundaries::common::construct_signatures_for_type(self.ctx.types, type_id)
            && sigs
                .iter()
                .any(|sig| sig.type_params.len() == type_argument_count)
        {
            return true;
        }
        false
    }

    fn instantiation_type_has_signature(&self, type_id: TypeId) -> bool {
        if crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
            .is_some()
        {
            return true;
        }
        if let Some(sigs) =
            crate::query_boundaries::common::call_signatures_for_type(self.ctx.types, type_id)
            && !sigs.is_empty()
        {
            return true;
        }
        if let Some(sigs) =
            crate::query_boundaries::common::construct_signatures_for_type(self.ctx.types, type_id)
            && !sigs.is_empty()
        {
            return true;
        }
        false
    }

    /// Get type from a type query node (typeof X).
    ///
    /// Creates a `TypeQuery` type that captures the type of a value.
    pub(crate) fn get_type_from_type_query(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_lowering::TypeLowering;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(type_query) = self.ctx.arena.get_type_query(node) else {
            return TypeId::ERROR;
        };

        // Capture type argument node indices early (before borrows prevent access).
        // When present, the base type will be wrapped in Application(base, args)
        // so that constraint checking (TS2344) sees the instantiated type rather
        // than the raw function type. This matches tsc behavior: `typeof fn<Args>`
        // produces an instantiation expression type, not the original function type.
        let type_arguments = type_query.type_arguments.clone();
        let use_flow_sensitive_query =
            !is_type_query_in_non_flow_sensitive_signature_parameter(self.ctx.arena, idx);

        // `default` is a reserved keyword and cannot be used as an identifier in
        // expression position. `typeof default` must always report TS2304 even when
        // the file has an `export default` declaration, because the default-export
        // binding is not a locally-visible value name. This check must come BEFORE
        // the node_types cache lookup, which may have a cached type for the `default`
        // identifier node from a prior expression-space visit.
        if let Some(expr_node) = self.ctx.arena.get(type_query.expr_name)
            && expr_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
            && self
                .ctx
                .arena
                .get_identifier(expr_node)
                .is_some_and(|id| id.escaped_text == "default")
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let msg = format_message(diagnostic_messages::CANNOT_FIND_NAME, &["default"]);
            self.ctx.error(
                expr_node.pos,
                expr_node.end - expr_node.pos,
                msg,
                diagnostic_codes::CANNOT_FIND_NAME,
            );
            return TypeId::ERROR;
        }

        // Type parameter constraints cannot reference function parameters of the
        // same function via `typeof`. Emit TS2304/TS2552 instead of silently resolving.
        // This check MUST come before the node_types cache lookup, because
        // destructured parameter bindings (e.g., `{a}` in `({a}: {a:T})`) may
        // have their type cached during binding pattern processing. Without this
        // priority, `typeof a` would return the cached type instead of ERROR,
        // causing the type parameter constraint to be self-referential instead
        // of ERROR, which then leads to cascading TS2339 diagnostics.
        if let Some(expr_node) = self.ctx.arena.get(type_query.expr_name)
            && expr_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
            && let Some(ident) = self.ctx.arena.get_identifier(expr_node)
            && self
                .ctx
                .type_param_constraint_excluded_params
                .contains(ident.escaped_text.as_str())
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let msg = format_message(
                diagnostic_messages::CANNOT_FIND_NAME,
                &[&ident.escaped_text],
            );
            self.ctx.error(
                expr_node.pos,
                expr_node.end - expr_node.pos,
                msg,
                diagnostic_codes::CANNOT_FIND_NAME,
            );
            return TypeId::ERROR;
        }

        let name_opt = if let Some(expr_node) = self.ctx.arena.get(type_query.expr_name) {
            if expr_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
                self.ctx
                    .arena
                    .get_identifier(expr_node)
                    .map(|id| id.escaped_text.as_str())
            } else {
                None
            }
        } else {
            None
        };

        if name_opt == Some("default") {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let msg = format_message(diagnostic_messages::CANNOT_FIND_NAME, &["default"]);
            let expr_node = self
                .ctx
                .arena
                .get(type_query.expr_name)
                .expect("type_query.expr_name node exists");
            self.ctx.error(
                expr_node.pos,
                expr_node.end - expr_node.pos,
                msg,
                diagnostic_codes::CANNOT_FIND_NAME,
            );
            return TypeId::ERROR;
        }

        // Check typeof_param_scope — resolves `typeof paramName` in return type
        // annotations where the parameter isn't a file-level binding.
        if let Some(expr_node) = self.ctx.arena.get(type_query.expr_name)
            && expr_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
            && let Some(ident) = self.ctx.arena.get_identifier(expr_node)
            && let Some(&param_type) = self.ctx.typeof_param_scope.get(ident.escaped_text.as_str())
        {
            if let Some(type_arguments) = &type_arguments {
                return self
                    .apply_instantiation_expression_type_arguments(param_type, type_arguments);
            }
            return param_type;
        }

        if let Some(object_type) = self.const_array_to_enum_object_type_query(type_query.expr_name)
        {
            if let Some(type_arguments) = &type_arguments {
                return self
                    .apply_instantiation_expression_type_arguments(object_type, type_arguments);
            }
            return object_type;
        }

        if let Some(literal_type) =
            self.const_object_member_literal_type_query(type_query.expr_name)
        {
            if let Some(type_arguments) = &type_arguments {
                return self
                    .apply_instantiation_expression_type_arguments(literal_type, type_arguments);
            }
            return literal_type;
        }

        if let Some(property_type) = self.value_property_type_query(type_query.expr_name) {
            if let Some(type_arguments) = &type_arguments {
                return self
                    .apply_instantiation_expression_type_arguments(property_type, type_arguments);
            }
            return property_type;
        }

        if use_flow_sensitive_query
            && let Some(&expr_type) = self.ctx.node_types.get(&type_query.expr_name.0)
            && expr_type != TypeId::ERROR
        {
            if let Some(type_arguments) = &type_arguments {
                return self
                    .apply_instantiation_expression_type_arguments(expr_type, type_arguments);
            }
            return expr_type;
        }

        if let Some(sym_id) = self.resolve_type_query_symbol(type_query.expr_name) {
            let (sym_flags, type_only_name) =
                self.ctx
                    .binder
                    .get_symbol(sym_id)
                    .map_or((0 /* no symbol */, None), |s| {
                        let has_value = s.has_any_flags(tsz_binder::symbol_flags::VALUE);
                        let is_type_only =
                            s.has_any_flags(tsz_binder::symbol_flags::TYPE) && !has_value;
                        (s.flags, is_type_only.then(|| s.escaped_name.clone()))
                    });
            if let Some(escaped_name) = type_only_name {
                self.emit_type_query_type_only_error(&escaped_name, type_query.expr_name);
                return TypeId::ERROR;
            }

            if sym_flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0
                && sym_flags & tsz_binder::symbol_flags::VALUE != 0
            {
                if let Some(&val_type) = self.ctx.merged_value_types.get(&sym_id) {
                    if let Some(type_arguments) = &type_arguments {
                        return self.apply_instantiation_expression_type_arguments(
                            val_type,
                            type_arguments,
                        );
                    }
                    return val_type;
                }

                if let Some(ann_idx) = self.ctx.binder.get_symbol(sym_id).and_then(|symbol| {
                    let mut decl = symbol.value_declaration;
                    let decl_node = self.ctx.arena.get(decl)?;
                    if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
                        decl = self.ctx.arena.get_extended(decl)?.parent;
                    }
                    let decl_node = self.ctx.arena.get(decl)?;
                    if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                        return None;
                    }
                    let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
                    var_decl
                        .type_annotation
                        .is_some()
                        .then_some(var_decl.type_annotation)
                }) {
                    let ann_type = self.check(ann_idx);
                    if ann_type != TypeId::ERROR && ann_type != TypeId::ANY {
                        if let Some(type_arguments) = &type_arguments {
                            return self.apply_instantiation_expression_type_arguments(
                                ann_type,
                                type_arguments,
                            );
                        }
                        return ann_type;
                    }
                }

                if let Some(val_type) = self.compute_safe_merged_value_type_for_type_query(sym_id) {
                    self.ctx.merged_value_types.insert(sym_id, val_type);
                    if let Some(type_arguments) = &type_arguments {
                        return self.apply_instantiation_expression_type_arguments(
                            val_type,
                            type_arguments,
                        );
                    }
                    return val_type;
                }
            }

            let mut declared_type: Option<TypeId> =
                if sym_flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0
                    && self.ctx.symbol_resolution_set.contains(&sym_id)
                {
                    None
                } else {
                    self.ctx
                        .symbol_types
                        .get(&sym_id)
                        .copied()
                        .filter(|&t| t != TypeId::ANY && t != TypeId::ERROR)
                };

            if declared_type.is_none() {
                let type_ann_idx = self.ctx.binder.get_symbol(sym_id).and_then(|symbol| {
                    let decl = symbol.value_declaration;
                    if decl.is_none() {
                        return None;
                    }
                    let decl_node = self.ctx.arena.get(decl)?;
                    if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
                        if var_decl.type_annotation.is_some() {
                            return Some(var_decl.type_annotation);
                        }
                    } else if decl_node.kind == syntax_kind_ext::PARAMETER {
                        let param = self.ctx.arena.get_parameter(decl_node)?;
                        if param.type_annotation.is_some() {
                            return Some(param.type_annotation);
                        }
                    } else if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                        && let Some(ext) = self.ctx.arena.get_extended(decl)
                        && ext.parent.is_some()
                        && let Some(parent_node) = self.ctx.arena.get(ext.parent)
                        && parent_node.kind == syntax_kind_ext::PARAMETER
                    {
                        let param = self.ctx.arena.get_parameter(parent_node)?;
                        if param.name == decl && param.type_annotation.is_some() {
                            return Some(param.type_annotation);
                        }
                    }
                    None
                });
                if let Some(ann_idx) = type_ann_idx {
                    let resolved = self.check(ann_idx);
                    if resolved != TypeId::ANY && resolved != TypeId::ERROR {
                        declared_type = Some(resolved);
                    }
                }
            }

            if let Some(declared_type) = declared_type
                && declared_type != TypeId::ANY
                && declared_type != TypeId::ERROR
            {
                if !use_flow_sensitive_query {
                    if let Some(type_arguments) = &type_arguments {
                        return self.apply_instantiation_expression_type_arguments(
                            declared_type,
                            type_arguments,
                        );
                    }
                    return declared_type;
                }

                // Find a flow node at or above the expression name for narrowing.
                let flow_node = self
                    .ctx
                    .binder
                    .get_node_flow(type_query.expr_name)
                    .or_else(|| {
                        // Walk up parents to find a flow node (type position nodes
                        // often don't have direct flow links).
                        let mut current = self
                            .ctx
                            .arena
                            .get_extended(type_query.expr_name)
                            .map(|ext| ext.parent);
                        while let Some(parent) = current {
                            if parent.is_none() {
                                break;
                            }
                            if let Some(flow) = self.ctx.binder.get_node_flow(parent) {
                                return Some(flow);
                            }
                            current = self.ctx.arena.parent_of(parent);
                        }
                        None
                    });

                if let Some(flow_node) = flow_node {
                    let analyzer = crate::FlowAnalyzer::with_node_types(
                        self.ctx.arena,
                        self.ctx.binder,
                        self.ctx.types,
                        &self.ctx.node_types,
                    )
                    .with_flow_cache(&self.ctx.flow_analysis_cache)
                    .with_switch_reference_cache(&self.ctx.flow_switch_reference_cache)
                    .with_numeric_atom_cache(&self.ctx.flow_numeric_atom_cache)
                    .with_reference_match_cache(&self.ctx.flow_reference_match_cache)
                    .with_type_environment(&self.ctx.type_environment)
                    .with_narrowing_cache(&self.ctx.narrowing_cache)
                    .with_call_type_predicates(&self.ctx.call_type_predicates)
                    .with_flow_buffers(
                        &self.ctx.flow_worklist,
                        &self.ctx.flow_in_worklist,
                        &self.ctx.flow_visited,
                        &self.ctx.flow_results,
                    )
                    .with_symbol_last_assignment_pos(&self.ctx.symbol_last_assignment_pos)
                    .with_destructured_bindings(&self.ctx.destructured_bindings);

                    let narrowed =
                        analyzer.get_flow_type(type_query.expr_name, declared_type, flow_node);
                    if narrowed != TypeId::ERROR {
                        if let Some(type_arguments) = &type_arguments {
                            return self.apply_instantiation_expression_type_arguments(
                                narrowed,
                                type_arguments,
                            );
                        }
                        return narrowed;
                    }
                }
            }

            let factory = self.ctx.types.factory();
            let base = factory.type_query(tsz_solver::SymbolRef(sym_id.0));
            if let Some(type_arguments) = &type_arguments {
                return self.apply_instantiation_expression_type_arguments(base, type_arguments);
            }
            return base;
        }

        // For qualified/generic typeof expressions (typeof A.B, typeof A<B>),
        // check if the root identifier exists. If not, emit TS2304.
        if name_opt.is_none() {
            use tsz_parser::parser::syntax_kind_ext;
            let mut root_idx = type_query.expr_name;
            while let Some(node) = self.ctx.arena.get(root_idx) {
                if node.kind == syntax_kind_ext::QUALIFIED_NAME
                    && let Some(qn) = self.ctx.arena.get_qualified_name(node)
                {
                    root_idx = qn.left;
                    continue;
                }
                break;
            }
            if let Some(root_node) = self.ctx.arena.get(root_idx)
                && root_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                && let Some(root_ident) = self.ctx.arena.get_identifier(root_node)
            {
                let root_name = root_ident.escaped_text.as_str();
                let is_global_name = matches!(
                    root_name,
                    "undefined" | "NaN" | "Infinity" | "globalThis" | "arguments"
                );
                if !is_global_name
                    && self
                        .ctx
                        .binder
                        .resolve_identifier(self.ctx.arena, root_idx)
                        .is_none()
                    && !self.ctx.typeof_param_scope.contains_key(root_name)
                {
                    use crate::diagnostics::{
                        diagnostic_codes, diagnostic_messages, format_message,
                    };
                    let msg = format_message(diagnostic_messages::CANNOT_FIND_NAME, &[root_name]);
                    self.ctx.error(
                        root_node.pos,
                        root_node.end - root_node.pos,
                        msg,
                        diagnostic_codes::CANNOT_FIND_NAME,
                    );
                    return TypeId::ERROR;
                }
            }
        }

        // For simple identifiers, try full scope resolution (including function params,
        // local variables, etc.) before falling back to lowering.
        if let Some(name) = name_opt {
            if let Some(sym_id) = self
                .ctx
                .binder
                .resolve_identifier(self.ctx.arena, type_query.expr_name)
            {
                // TS2693: typeof requires a value binding (same check as above).
                if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                    let flags = symbol.flags;
                    let has_value = flags & tsz_binder::symbol_flags::VALUE != 0;
                    let is_type_only = (flags & tsz_binder::symbol_flags::TYPE != 0) && !has_value;
                    if is_type_only {
                        self.emit_type_query_type_only_error(name, type_query.expr_name);
                        return TypeId::ERROR;
                    }
                }
                if !use_flow_sensitive_query
                    && let Some(declared_type) = self.declared_type_for_type_query_symbol(sym_id)
                {
                    if let Some(type_arguments) = &type_arguments {
                        return self.apply_instantiation_expression_type_arguments(
                            declared_type,
                            type_arguments,
                        );
                    }
                    return declared_type;
                }
                let factory = self.ctx.types.factory();
                let base = factory.type_query(tsz_solver::SymbolRef(sym_id.0));
                if let Some(type_arguments) = &type_arguments {
                    return self
                        .apply_instantiation_expression_type_arguments(base, type_arguments);
                }
                return base;
            }
            // Skip TS2304 for well-known globals that may not be in local binder scope
            // but are valid in typeof position (undefined, NaN, Infinity, globalThis, etc.)
            let is_global_name = matches!(
                name,
                "undefined" | "NaN" | "Infinity" | "globalThis" | "arguments"
            );
            if name == "globalThis" {
                return self.get_global_this_type(type_query.expr_name);
            } else if is_global_name {
                // Fall through to TypeLowering
            } else {
                // Name not found in any scope — emit TS2304
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
                let msg = format_message(diagnostic_messages::CANNOT_FIND_NAME, &[name]);
                if let Some(expr_node) = self.ctx.arena.get(type_query.expr_name) {
                    self.ctx.error(
                        expr_node.pos,
                        expr_node.end - expr_node.pos,
                        msg,
                        diagnostic_codes::CANNOT_FIND_NAME,
                    );
                }
                return TypeId::ERROR;
            }
        }

        // Fall back to TypeLowering with proper value resolvers
        let value_resolver = |node_idx: NodeIndex| -> Option<u32> {
            let ident = self.ctx.arena.get_identifier_at(node_idx)?;
            let name = ident.escaped_text.as_str();
            if name == "default" {
                return None;
            }
            let sym_id = self.ctx.binder.file_locals.get(name)?;
            Some(sym_id.0)
        };
        let type_resolver = |_node_idx: NodeIndex| -> Option<u32> { None };
        let lowering = TypeLowering::with_resolvers(
            self.ctx.arena,
            self.ctx.types,
            &type_resolver,
            &value_resolver,
        );

        lowering.lower_type(idx)
    }

    pub(crate) fn const_object_member_literal_type_query(
        &self,
        expr_name: NodeIndex,
    ) -> Option<TypeId> {
        let expr_name = self.ctx.arena.skip_parenthesized_and_assertions(expr_name);
        let node = self.ctx.arena.get(expr_name)?;

        let (base, property_name_node) = if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        {
            let access = self.ctx.arena.get_access_expr(node)?;
            if access.question_dot_token {
                return None;
            }
            (access.expression, access.name_or_argument)
        } else if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qualified = self.ctx.arena.get_qualified_name(node)?;
            (qualified.left, qualified.right)
        } else {
            return None;
        };

        let property_name = self.property_name_text(property_name_node)?;
        let base = self.ctx.arena.skip_parenthesized_and_assertions(base);
        let base_node = self.ctx.arena.get(base)?;
        if base_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self.ctx.binder.resolve_identifier(self.ctx.arena, base)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE) {
            return None;
        }

        let mut decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol.primary_declaration()?
        };
        let mut decl_node = self.ctx.arena.get(decl_idx)?;
        if decl_node.kind == SyntaxKind::Identifier as u16 {
            decl_idx = self.ctx.arena.get_extended(decl_idx)?.parent;
            decl_node = self.ctx.arena.get(decl_idx)?;
        }
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION
            || !self.ctx.arena.is_const_variable_declaration(decl_idx)
        {
            return None;
        }

        let decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        let assertion_expr = self.ctx.arena.skip_parenthesized(decl.initializer);
        let initializer_is_const_assertion = self
            .ctx
            .arena
            .get(assertion_expr)
            .and_then(|node| self.ctx.arena.get_type_assertion(node))
            .and_then(|assertion| self.ctx.arena.get(assertion.type_node))
            .is_some_and(|type_node| type_node.kind == SyntaxKind::ConstKeyword as u16);
        let initializer = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(decl.initializer);
        let init_node = self.ctx.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return self.array_to_enum_member_literal_type(initializer, &property_name);
        }

        let literal = self.ctx.arena.get_literal_expr(init_node)?;
        for &element in &literal.elements.nodes {
            let element_node = self.ctx.arena.get(element)?;
            if element_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                let prop = self.ctx.arena.get_property_assignment(element_node)?;
                if self.property_name_text(prop.name).as_deref() == Some(property_name.as_str()) {
                    let member_type =
                        self.literal_type_from_const_member_initializer(prop.initializer)?;
                    return Some(if initializer_is_const_assertion {
                        member_type
                    } else {
                        crate::query_boundaries::common::widen_literal_type(
                            self.ctx.types,
                            member_type,
                        )
                    });
                }
            } else if element_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                let prop = self.ctx.arena.get_shorthand_property(element_node)?;
                if self.property_name_text(prop.name).as_deref() == Some(property_name.as_str()) {
                    let member_type = self.literal_type_from_const_member_initializer(prop.name)?;
                    return Some(if initializer_is_const_assertion {
                        member_type
                    } else {
                        crate::query_boundaries::common::widen_literal_type(
                            self.ctx.types,
                            member_type,
                        )
                    });
                }
            }
        }

        None
    }

    pub(crate) fn const_array_to_enum_object_type_query(
        &self,
        expr_name: NodeIndex,
    ) -> Option<TypeId> {
        let expr_name = self.ctx.arena.skip_parenthesized_and_assertions(expr_name);
        let node = self.ctx.arena.get(expr_name)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, expr_name)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE) {
            return None;
        }

        let mut decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol.primary_declaration()?
        };
        let mut decl_node = self.ctx.arena.get(decl_idx)?;
        if decl_node.kind == SyntaxKind::Identifier as u16 {
            decl_idx = self.ctx.arena.get_extended(decl_idx)?.parent;
            decl_node = self.ctx.arena.get(decl_idx)?;
        }
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION
            || !self.ctx.arena.is_const_variable_declaration(decl_idx)
        {
            return None;
        }

        let decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        let literal_names = self.array_to_enum_literal_names(decl.initializer)?;
        if literal_names.is_empty() {
            return None;
        }

        let props = literal_names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let literal_type = self.ctx.types.literal_string(name);
                tsz_solver::PropertyInfo {
                    name: self.ctx.types.intern_string(name),
                    type_id: literal_type,
                    write_type: literal_type,
                    optional: false,
                    readonly: true,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: tsz_common::Visibility::Public,
                    parent_id: None,
                    declaration_order: index as u32,
                    is_string_named: false,
                    is_symbol_named: false,
                    single_quoted_name: false,
                }
            })
            .collect();

        Some(self.ctx.types.factory().object(props))
    }

    fn array_to_enum_member_literal_type(
        &self,
        initializer: NodeIndex,
        property_name: &str,
    ) -> Option<TypeId> {
        self.array_to_enum_literal_names(initializer)?
            .into_iter()
            .find(|name| name == property_name)
            .map(|name| self.ctx.types.literal_string(&name))
    }

    fn array_to_enum_literal_names(&self, initializer: NodeIndex) -> Option<Vec<String>> {
        let initializer = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(initializer);
        let node = self.ctx.arena.get(initializer)?;
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.ctx.arena.get_call_expr(node)?;
        if !self.call_expression_is_array_to_enum(call.expression) {
            return None;
        }

        let first_arg = call.arguments.as_ref()?.nodes.first().copied()?;
        let arg = self.ctx.arena.skip_parenthesized_and_assertions(first_arg);
        let arg_node = self.ctx.arena.get(arg)?;
        if arg_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }

        let array = self.ctx.arena.get_literal_expr(arg_node)?;
        let mut names = Vec::new();
        for &element in &array.elements.nodes {
            let element = self.ctx.arena.skip_parenthesized_and_assertions(element);
            let element_node = self.ctx.arena.get(element)?;
            if (element_node.kind == SyntaxKind::StringLiteral as u16
                || element_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16)
                && let Some(lit) = self.ctx.arena.get_literal(element_node)
            {
                names.push(lit.text.clone());
            }
        }

        Some(names)
    }

    fn call_expression_is_array_to_enum(&self, callee: NodeIndex) -> bool {
        let callee = self.ctx.arena.skip_parenthesized_and_assertions(callee);
        let Some(node) = self.ctx.arena.get(callee) else {
            return false;
        };

        if let Some(ident) = self.ctx.arena.get_identifier(node) {
            return ident.escaped_text == "arrayToEnum"
                && self.array_to_enum_callee_returns_identity_mapped_type(callee);
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(node)
            && !access.question_dot_token
            && let Some(name_node) = self.ctx.arena.get(access.name_or_argument)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            return ident.escaped_text == "arrayToEnum";
        }

        false
    }

    fn array_to_enum_callee_returns_identity_mapped_type(&self, callee: NodeIndex) -> bool {
        let Some(sym_id) = self.resolve_array_to_enum_callee_symbol(callee) else {
            return false;
        };
        let arena = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .map(|file_idx| self.ctx.get_arena_for_file(file_idx as u32))
            .unwrap_or(self.ctx.arena);
        let Some(symbol) = self.array_to_enum_cross_file_symbol(sym_id) else {
            return false;
        };
        let mut decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            match symbol.primary_declaration() {
                Some(decl) => decl,
                None => return false,
            }
        };
        let mut decl_node = match arena.get(decl_idx) {
            Some(node) => node,
            None => return false,
        };
        if decl_node.kind == SyntaxKind::Identifier as u16 {
            let Some(parent) = arena.get_extended(decl_idx).map(|ext| ext.parent) else {
                return false;
            };
            decl_idx = parent;
            let Some(parent_node) = arena.get(decl_idx) else {
                return false;
            };
            decl_node = parent_node;
        }

        let return_type = if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let Some(decl) = arena.get_variable_declaration(decl_node) else {
                return false;
            };
            let Some(init_node) = arena.get(decl.initializer) else {
                return false;
            };
            let Some(func) = arena.get_function(init_node) else {
                return false;
            };
            func.type_annotation
        } else {
            let Some(func) = arena.get_function(decl_node) else {
                return false;
            };
            func.type_annotation
        };

        self.type_node_is_identity_mapped_type_in_arena(arena, return_type)
    }

    fn resolve_array_to_enum_callee_symbol(
        &self,
        callee: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        let node = self.ctx.arena.get(callee)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return self.ctx.binder.resolve_identifier(self.ctx.arena, callee);
        }
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.ctx.arena.get_access_expr(node)?;
        let left_sym = self.resolve_array_to_enum_callee_symbol(access.expression)?;
        let left_symbol = self.array_to_enum_cross_file_symbol(left_sym)?;
        let right_name = self
            .ctx
            .arena
            .get_identifier_at(access.name_or_argument)
            .map(|ident| ident.escaped_text.as_str())?;
        left_symbol
            .exports
            .as_ref()
            .and_then(|exports| exports.get(right_name))
            .or_else(|| {
                left_symbol
                    .members
                    .as_ref()
                    .and_then(|members| members.get(right_name))
            })
    }

    fn type_node_is_identity_mapped_type(&self, type_node: NodeIndex) -> bool {
        self.type_node_is_identity_mapped_type_in_arena(self.ctx.arena, type_node)
    }

    fn array_to_enum_cross_file_symbol(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<&tsz_binder::Symbol> {
        if let Some(file_idx) = self.ctx.resolve_symbol_file_index(sym_id)
            && let Some(binder) = self.ctx.get_binder_for_file(file_idx)
            && let Some(symbol) = binder.get_symbol(sym_id)
        {
            return Some(symbol);
        }
        self.ctx.binder.get_symbol(sym_id)
    }

    fn type_node_is_identity_mapped_type_in_arena(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        type_node: NodeIndex,
    ) -> bool {
        let Some(node) = arena.get(type_node) else {
            return false;
        };
        if node.kind != syntax_kind_ext::MAPPED_TYPE {
            return false;
        }
        let Some(mapped) = arena.get_mapped_type(node) else {
            return false;
        };
        let Some(param) = arena
            .get(mapped.type_parameter)
            .and_then(|node| arena.get_type_parameter(node))
        else {
            return false;
        };
        let Some(param_name) = arena.get_identifier_at(param.name) else {
            return false;
        };
        arena
            .get_identifier_at(mapped.type_node)
            .is_some_and(|name| name.escaped_text == param_name.escaped_text)
    }

    fn declared_type_for_type_query_symbol(
        &mut self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<TypeId> {
        if let Some(type_id) = self
            .ctx
            .symbol_types
            .get(&sym_id)
            .copied()
            .filter(|&t| t != TypeId::ANY && t != TypeId::ERROR)
        {
            return Some(type_id);
        }

        let decl = self.ctx.binder.get_symbol(sym_id)?.value_declaration;
        if decl.is_none() {
            return None;
        }
        let decl_node = self.ctx.arena.get(decl)?;
        let type_ann = if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
            var_decl
                .type_annotation
                .is_some()
                .then_some(var_decl.type_annotation)
        } else if decl_node.kind == syntax_kind_ext::PARAMETER {
            let param = self.ctx.arena.get_parameter(decl_node)?;
            param
                .type_annotation
                .is_some()
                .then_some(param.type_annotation)
        } else if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let parent = self.ctx.arena.get_extended(decl)?.parent;
            let parent_node = self.ctx.arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::PARAMETER {
                let param = self.ctx.arena.get_parameter(parent_node)?;
                (param.name == decl && param.type_annotation.is_some())
                    .then_some(param.type_annotation)
            } else {
                None
            }
        } else {
            None
        }?;

        Some(self.check(type_ann)).filter(|&t| t != TypeId::ANY && t != TypeId::ERROR)
    }

    fn get_global_this_type(&mut self, _error_node: NodeIndex) -> TypeId {
        let mut names = rustc_hash::FxHashSet::default();
        for (name, _) in self.ctx.binder.file_locals.iter() {
            names.insert(name.clone());
        }
        for lib_ctx in self.ctx.lib_contexts.iter() {
            for (name, _) in lib_ctx.binder.file_locals.iter() {
                names.insert(name.clone());
            }
        }
        names.insert("globalThis".to_string());

        let mut properties = Vec::new();
        for name in names {
            let type_id = if name == "globalThis" {
                TypeId::UNKNOWN
            } else {
                let Some(sym_id) = self.global_this_surface_symbol(&name) else {
                    continue;
                };
                self.ctx
                    .symbol_types
                    .get(&sym_id)
                    .copied()
                    .filter(|&type_id| type_id != TypeId::ERROR)
                    .or_else(|| self.declared_type_for_type_query_symbol(sym_id))
                    .unwrap_or_else(|| {
                        self.ctx
                            .types
                            .factory()
                            .type_query(tsz_solver::SymbolRef(sym_id.0))
                    })
            };

            let prop_name = self.ctx.types.intern_string(&name);
            let mut prop = PropertyInfo::new(prop_name, type_id);
            prop.write_type = type_id;
            prop.readonly = name == "globalThis";
            prop.parent_id = self.global_this_surface_symbol(&name);
            prop.declaration_order = properties.len() as u32;
            properties.push(prop);
        }

        self.ctx.types.factory().object_with_index(ObjectShape {
            properties,
            ..ObjectShape::default()
        })
    }

    fn global_this_surface_symbol(&self, name: &str) -> Option<tsz_binder::SymbolId> {
        use tsz_binder::symbol_flags;

        if let Some(sym_id) = self.ctx.binder.file_locals.get(name)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.has_any_flags(symbol_flags::VALUE)
            && (!symbol.has_any_flags(symbol_flags::BLOCK_SCOPED_VARIABLE)
                || symbol.has_any_flags(symbol_flags::FUNCTION_SCOPED_VARIABLE))
        {
            return Some(sym_id);
        }

        for lib_ctx in self.ctx.lib_contexts.iter() {
            if let Some(sym_id) = lib_ctx.binder.file_locals.get(name)
                && let Some(symbol) = lib_ctx.binder.get_symbol(sym_id)
                && symbol.has_any_flags(symbol_flags::VALUE)
                && (!symbol.has_any_flags(symbol_flags::BLOCK_SCOPED_VARIABLE)
                    || symbol.has_any_flags(symbol_flags::FUNCTION_SCOPED_VARIABLE))
            {
                return Some(sym_id);
            }
        }

        None
    }

    /// Emit TS2693 for a type-only symbol used in a typeof type query.
    fn emit_type_query_type_only_error(&mut self, name: &str, expr_name: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        let msg = format_message(
            diagnostic_messages::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
            &[name],
        );
        if let Some(expr_node) = self.ctx.arena.get(expr_name) {
            self.ctx.error(
                expr_node.pos,
                expr_node.end - expr_node.pos,
                msg,
                diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
            );
        }
    }

    /// Resolve the symbol for a type query expression name.
    ///
    /// Handles both simple identifiers and qualified names (e.g., `M.F2`).
    /// For qualified names, walks through namespace exports to find the member.
    fn resolve_type_query_symbol(&self, expr_name: NodeIndex) -> Option<tsz_binder::SymbolId> {
        use tsz_parser::parser::syntax_kind_ext;

        let node = self.ctx.arena.get(expr_name)?;

        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let ident = self.ctx.arena.get_identifier(node)?;
            let name = ident.escaped_text.as_str();
            if name == "default" {
                return None;
            }
            let sym_id = self.ctx.binder.file_locals.get(name)?;
            return Some(sym_id);
        }

        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.ctx.arena.get_qualified_name(node)?;
            // Recursively resolve the left side
            let left_sym = self.resolve_type_query_symbol(qn.left)?;

            // Get the right name
            let right_node = self.ctx.arena.get(qn.right)?;
            let right_ident = self.ctx.arena.get_identifier(right_node)?;
            let right_name = right_ident.escaped_text.as_str();

            // Look through binder + libs for the left symbol's exports
            let lib_binders: Vec<std::sync::Arc<tsz_binder::BinderState>> = self
                .ctx
                .lib_contexts
                .iter()
                .map(|lc| std::sync::Arc::clone(&lc.binder))
                .collect();
            let left_symbol = self
                .ctx
                .binder
                .get_symbol_with_libs(left_sym, &lib_binders)?;

            if let Some(exports) = left_symbol.exports.as_ref()
                && let Some(member_sym) = exports.get(right_name)
            {
                return Some(member_sym);
            }
        }

        None
    }

    // =========================================================================
    // Mapped Types
    // =========================================================================

    /// Check a mapped type ({ [P in K]: T }).
    ///
    /// This function validates the mapped type and emits TS7039 if the type expression
    /// after the colon is missing (e.g., `{[P in "bar"]}` instead of `{[P in "bar"]: string}`).
    ///
    /// Note: TS2322 constraint validation (key type must be assignable to
    /// `string | number | symbol`) is handled by `CheckerState::check_mapped_type_constraint`
    /// in `check_type_node`, which covers both top-level and conditional-nested mapped types.
    pub(super) fn get_type_from_mapped_type(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_parser::parser::NodeIndex as ParserNodeIndex;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(data) = self.ctx.arena.get_mapped_type(node) else {
            return TypeId::ERROR;
        };

        // TS7039: Mapped object type implicitly has an 'any' template type.
        // This error occurs when the type expression after the colon is missing.
        // Example: type Foo = {[P in "bar"]};  // Missing ": T" after "bar"]
        if data.type_node == ParserNodeIndex::NONE {
            let message = "Mapped object type implicitly has an 'any' template type.";
            self.ctx
                .error(node.pos, node.end - node.pos, message.to_string(), 7039);
            return TypeId::ANY;
        }

        // Delegate to TypeLowering with extended resolvers (enum flags + lib search)
        self.lower_with_resolvers(idx, true, false)
    }
}
