use super::*;

impl<'a, 'ctx> TypeNodeChecker<'a, 'ctx> {
    // =========================================================================
    // Indexed Access Types
    // =========================================================================

    /// Handle indexed access type nodes (e.g., `Person["name"]`, `T[K]`).
    pub(in crate::types_domain) fn get_type_from_indexed_access_type(
        &mut self,
        idx: NodeIndex,
    ) -> TypeId {
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
                    // Check property existence against the materialized object so
                    // unevaluated mapped-alias intersection members contribute their
                    // keys (see `evaluate_type_for_property_check`).
                    let property_object = self.evaluate_type_for_property_check(resolved_object);
                    let prop_result =
                        crate::query_boundaries::property_access::resolve_property_access(
                            self.ctx.types,
                            property_object,
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
                // Returning `evaluated` for the bare `typeof X[K]` shape
                // (but not for `(typeof X)[K]`) gives an alias body a
                // concrete display in diagnostics. The shortcut is only
                // sound when the typeof receiver materialized into a
                // concrete type; if that failed, eager evaluation collapses
                // `IndexAccess(typeof x, K)` to `undefined`.
                if evaluated != TypeId::ERROR
                    && evaluated != TypeId::UNDEFINED
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

    /// Materialize an intersection receiver before a property-existence check.
    ///
    /// An intersection member that is an unevaluated mapped-type alias
    /// application (e.g. `Record<K, V>`, a user alias `M<...>`, or a wrapper
    /// around one) is opaque to the bare property-access resolver, which cannot
    /// expand a checker-owned `DefId`. Evaluating through the `TypeEnvironment`
    /// merges the members into one object shape, as `keyof` and assignability
    /// already do, so every member's keys are visible. Non-intersections and
    /// types that fail to evaluate are returned unchanged.
    fn evaluate_type_for_property_check(&self, object_type: TypeId) -> TypeId {
        if !crate::query_boundaries::common::is_intersection_type(self.ctx.types, object_type) {
            return object_type;
        }

        let evaluated = crate::query_boundaries::state::type_environment::evaluate_type_with_cache(
            self.ctx.types,
            &*self.ctx,
            object_type,
            std::iter::empty(),
            false,
            self.ctx.is_declaration_file() || self.ctx.emit_declarations(),
        )
        .result;

        if evaluated == TypeId::ERROR {
            object_type
        } else {
            evaluated
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
}
