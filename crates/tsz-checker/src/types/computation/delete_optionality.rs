//! TS2790: `delete` operand optionality checking.
//!
//! In `strictNullChecks`, `delete obj.prop` is only legal when the property
//! is optional (or, with `exactOptionalPropertyTypes` disabled, when its
//! declared type includes `undefined`). `tsc` resolves the deleted
//! property's *declared* symbol and checks its declared type — flow
//! narrowing of the receiver is irrelevant. tsz's truthiness/`in` narrowing
//! intersects the receiver with a synthetic required slot, so a
//! declared-optional property can look required at the delete site; the
//! check therefore re-validates against the un-narrowed (write-context)
//! receiver before reporting.

use crate::context::TypingRequest;
use crate::query_boundaries::common::PropertyAccessResult;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Report TS2790 when the operand of a `delete` expression is a
    /// non-optional property whose declared type does not include
    /// `undefined`.
    ///
    /// `operand_type` is the (possibly narrowed) type already computed for
    /// the operand expression; `operand_idx` is the property/element access
    /// node (parentheses already skipped by the caller).
    pub(crate) fn check_delete_operand_optionality(
        &mut self,
        operand_idx: NodeIndex,
        operand_type: TypeId,
    ) {
        let Some(operand_node) = self.ctx.arena.get(operand_idx) else {
            return;
        };
        if operand_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && operand_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return;
        }
        let Some(access) = self.ctx.arena.get_access_expr(operand_node) else {
            return;
        };

        let prop_name = self
            .ctx
            .arena
            .get_identifier_at(access.name_or_argument)
            .map(|ident| ident.escaped_text.clone())
            .or_else(|| self.get_literal_string_from_node(access.name_or_argument))
            .or_else(|| {
                self.get_literal_index_from_node(access.name_or_argument)
                    .map(|idx| idx.to_string())
            });
        let Some(prop_name) = prop_name else {
            return;
        };

        let mut object_type = self.get_type_of_node(access.expression);
        let uses_optional_chain_base = access.question_dot_token
            || crate::computation::access::is_optional_chain(self.ctx.arena, access.expression);
        if uses_optional_chain_base {
            let (non_nullish, _) = self.split_nullish_type(object_type);
            if let Some(non_nullish) = non_nullish {
                object_type = non_nullish;
            }
        }

        if object_type == TypeId::ANY
            || object_type == TypeId::UNKNOWN
            || object_type == TypeId::ERROR
            || object_type == TypeId::NEVER
        {
            return;
        }

        let property_result = self.resolve_property_access_with_env(object_type, &prop_name);
        let (prop_type, from_idx_sig) = match property_result {
            PropertyAccessResult::Success {
                type_id,
                from_index_signature,
                ..
            } => {
                let prop_type = if uses_optional_chain_base
                    || operand_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                {
                    type_id
                } else {
                    operand_type
                };
                (prop_type, from_index_signature)
            }
            _ => (operand_type, false),
        };

        if prop_type == TypeId::ANY
            || prop_type == TypeId::UNKNOWN
            || prop_type == TypeId::NEVER
            || prop_type == TypeId::ERROR
        {
            return;
        }

        let is_mapped =
            crate::query_boundaries::common::is_mapped_type(self.ctx.types, object_type);
        if from_idx_sig || is_mapped {
            return;
        }

        let is_optional = self.is_property_optional(object_type, &prop_name);
        let type_includes_undefined =
            crate::query_boundaries::class_type::type_includes_undefined(self.ctx.types, prop_type);
        if is_optional || type_includes_undefined {
            return;
        }

        // The narrowed receiver says "required"; re-check against the
        // declared (un-narrowed, write-context) receiver. tsc keys TS2790 off
        // the declared property symbol, so truthiness/`in` narrowing that
        // promoted the slot to required must not produce the error.
        let declared_object = self
            .get_type_of_node_with_request(access.expression, &TypingRequest::for_write_context());
        let declared_object = self.evaluate_type_with_resolution(declared_object);
        let declared_deletable = declared_object != object_type
            && (self.is_property_optional(declared_object, &prop_name)
                || matches!(
                    self.resolve_property_access_with_env(declared_object, &prop_name),
                    PropertyAccessResult::Success { type_id, .. }
                        if crate::query_boundaries::class_type::type_includes_undefined(
                            self.ctx.types,
                            type_id,
                        )
                ));
        if declared_deletable {
            return;
        }

        self.error_at_node(
            operand_idx,
            crate::diagnostics::diagnostic_messages::THE_OPERAND_OF_A_DELETE_OPERATOR_MUST_BE_OPTIONAL,
            crate::diagnostics::diagnostic_codes::THE_OPERAND_OF_A_DELETE_OPERATOR_MUST_BE_OPTIONAL,
        );
    }
}
