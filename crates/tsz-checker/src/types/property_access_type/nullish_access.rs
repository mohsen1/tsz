//! Diagnostics for property access on possibly nullish receivers.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// Parameters for a property access on a possibly-nullish receiver.
pub(crate) struct NullishAccessSite<'a> {
    /// The node index of the full access expression.
    pub(crate) idx: NodeIndex,
    /// The receiver expression node.
    pub(crate) expression: NodeIndex,
    /// The property name or element-access argument node.
    pub(crate) name_or_argument: NodeIndex,
    /// Whether a `?.` optional-chain token is present.
    pub(crate) question_dot_token: bool,
    /// The resolved property type on the non-nullish slice, if any.
    pub(crate) property_type: Option<TypeId>,
    /// The nullish cause type (`null`, `undefined`, or `null | undefined`).
    pub(crate) cause: TypeId,
    /// The full object type being accessed (including the nullish component).
    pub(crate) object_type_for_access: TypeId,
    /// The property name string used for diagnostics.
    pub(crate) property_name: &'a str,
    /// Whether flow narrowing should be skipped for the result.
    pub(crate) skip_flow_narrowing: bool,
    /// Whether the receiver already has a destructuring-assignment error.
    pub(crate) receiver_has_daa_error: bool,
}

impl<'a> CheckerState<'a> {
    /// Handles the `PossiblyNullOrUndefined` result from property access resolution.
    /// Emits appropriate diagnostics (TS18047/18048/18049/18050) and returns the resolved type.
    pub(crate) fn handle_possibly_null_or_undefined_access(
        &mut self,
        site: NullishAccessSite<'_>,
    ) -> TypeId {
        let NullishAccessSite {
            idx,
            expression,
            name_or_argument,
            question_dot_token,
            property_type,
            cause,
            object_type_for_access,
            property_name,
            skip_flow_narrowing,
            receiver_has_daa_error,
        } = site;
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
            // When the optional-chain receiver has no non-nullish slice
            // (`property_type` is `None`, i.e. the receiver is exactly `null`,
            // `undefined`, or `null | undefined`), tsc reports TS2339 at the
            // property name with the receiver type narrowed to `never` — the
            // chain always short-circuits, so the property access is
            // unreachable. Match that diagnostic; the result type still flows
            // through as `unknown | undefined` to keep downstream typing.
            if property_type.is_none() {
                self.error_property_not_exist_at(property_name, TypeId::NEVER, name_or_argument);
            }
            let base_type = property_type.unwrap_or(TypeId::UNKNOWN);
            return factory.union2(base_type, TypeId::UNDEFINED);
        }

        use crate::diagnostics::diagnostic_codes;

        if cause == TypeId::ERROR || cause == TypeId::ANY || cause == TypeId::UNKNOWN {
            return property_type.unwrap_or(TypeId::ERROR);
        }

        // If the non-nullish receiver still lacks the property, tsc reports the
        // missing property on that receiver type instead of masking it with a
        // possibly-nullish diagnostic. Example: `let s: Symbol = null; s.foo`
        // reports TS2339 for `foo`, while `let s: string | null; s.length`
        // reports TS18047 because `length` exists on `string`.
        let (non_nullish_base, _) = self.split_nullish_type(object_type_for_access);
        let initialized_to_declared_nullish =
            self.explicit_nullish_initializer_matches_annotation(expression, cause);
        if property_type.is_none()
            && let Some(non_nullish_base) = non_nullish_base
            && !matches!(
                non_nullish_base,
                TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR
            )
            && !initialized_to_declared_nullish
            && let PropertyAccessResult::PropertyNotFound { .. } =
                self.resolve_property_access_with_env(non_nullish_base, property_name)
        {
            self.error_property_not_exist_at(
                property_name,
                self.diagnostic_display_type_for_missing_property(
                    non_nullish_base,
                    non_nullish_base,
                ),
                name_or_argument,
            );
            return self.finalize_property_access_result(
                idx,
                TypeId::ERROR,
                skip_flow_narrowing,
                false,
            );
        }

        if property_type.is_none()
            && let Some(declared_receiver) =
                self.explicit_variable_annotation_type_for_nullish_receiver(expression)
            && !initialized_to_declared_nullish
        {
            let (declared_non_nullish, declared_nullish) =
                self.split_nullish_type(declared_receiver);
            if declared_nullish.is_none() {
                let lookup_type = declared_non_nullish.unwrap_or(declared_receiver);
                if !matches!(lookup_type, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)
                    && let PropertyAccessResult::PropertyNotFound { .. } =
                        self.resolve_property_access_with_env(lookup_type, property_name)
                {
                    self.error_property_not_exist_at(
                        property_name,
                        self.diagnostic_display_type_for_missing_property(lookup_type, lookup_type),
                        name_or_argument,
                    );
                    return self.finalize_property_access_result(
                        idx,
                        TypeId::ERROR,
                        skip_flow_narrowing,
                        false,
                    );
                }
            }
        }

        let is_type_nullish =
            object_type_for_access == TypeId::NULL || object_type_for_access == TypeId::UNDEFINED;

        if !self.ctx.compiler_options.strict_null_checks && !is_type_nullish {
            return self.finalize_property_access_result(
                idx,
                property_type.unwrap_or(TypeId::ERROR),
                skip_flow_narrowing,
                false,
            );
        }

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

        if !self.ctx.compiler_options.strict_null_checks {
            return self.finalize_property_access_result(
                idx,
                property_type.unwrap_or(TypeId::ERROR),
                skip_flow_narrowing,
                false,
            );
        }

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

    fn explicit_variable_annotation_type_for_nullish_receiver(
        &mut self,
        expression: NodeIndex,
    ) -> Option<TypeId> {
        self.explicit_variable_annotation_and_initializer_for_nullish_receiver(expression)
            .map(|(annotation, _)| annotation)
    }

    fn explicit_nullish_initializer_matches_annotation(
        &mut self,
        expression: NodeIndex,
        cause: TypeId,
    ) -> bool {
        let Some((declared_receiver, initializer)) =
            self.explicit_variable_annotation_and_initializer_for_nullish_receiver(expression)
        else {
            return false;
        };
        if initializer.is_none() {
            return false;
        }
        let Some(initializer_nullish) = self
            .literal_type_from_initializer(initializer)
            .filter(|ty| matches!(*ty, TypeId::NULL | TypeId::UNDEFINED))
        else {
            return false;
        };
        self.type_contains_nullish_kind(cause, initializer_nullish)
            && self.type_contains_nullish_kind(declared_receiver, initializer_nullish)
    }

    fn explicit_variable_annotation_and_initializer_for_nullish_receiver(
        &mut self,
        expression: NodeIndex,
    ) -> Option<(TypeId, NodeIndex)> {
        let expr_node = self.ctx.arena.get(expression)?;
        if expr_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, expression)?;
        let value_decl = self.ctx.binder.get_symbol(sym_id)?.value_declaration;
        let decl_node = self.ctx.arena.get(value_decl)?;
        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        let type_annotation = var_decl.type_annotation;
        let initializer = var_decl.initializer;
        if var_decl.type_annotation.is_some() {
            return Some((self.get_type_from_type_node(type_annotation), initializer));
        }
        self.jsdoc_type_annotation_for_node(value_decl)
            .or_else(|| self.jsdoc_type_annotation_for_node_inference(value_decl))
            .map(|annotation| (annotation, initializer))
    }

    fn type_contains_nullish_kind(&self, type_id: TypeId, nullish: TypeId) -> bool {
        if type_id == nullish {
            return true;
        }
        crate::query_boundaries::common::union_members(self.ctx.types.as_type_database(), type_id)
            .is_some_and(|members| members.contains(&nullish))
    }
}
