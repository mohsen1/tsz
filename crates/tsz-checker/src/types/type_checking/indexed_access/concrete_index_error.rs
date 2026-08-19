//! Concrete indexed-access diagnostic helpers.
//!
//! Kept out of `indexed_access.rs` so concrete TS2339/TS2537/TS2538 message
//! selection can grow without pushing the parent checker file over the LOC cap.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    pub(super) fn try_emit_concrete_index_access_error(
        &mut self,
        error_anchor: NodeIndex,
        object_type: TypeId,
        index_type: TypeId,
        object_is_type_parameter_ref: bool,
    ) -> bool {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        if object_type == TypeId::ERROR || index_type == TypeId::ERROR {
            return false;
        }

        let concrete_object_type =
            if crate::query_boundaries::common::is_generic_application(self.ctx.types, object_type)
            {
                let evaluated = self.evaluate_type_with_env(object_type);
                if evaluated != TypeId::ERROR
                    && !crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        evaluated,
                    )
                {
                    evaluated
                } else {
                    object_type
                }
            } else {
                object_type
            };
        let object_shape = crate::query_boundaries::common::object_shape_for_type(
            self.ctx.types,
            concrete_object_type,
        );
        let object_has_shape = object_shape.is_some();
        let object_is_array_like =
            crate::query_boundaries::common::is_array_type(self.ctx.types, concrete_object_type)
                || crate::query_boundaries::common::tuple_elements(
                    self.ctx.types,
                    concrete_object_type,
                )
                .is_some();

        if crate::query_boundaries::common::contains_type_parameters(
            self.ctx.types,
            concrete_object_type,
        ) || crate::query_boundaries::common::is_type_parameter_like(
            self.ctx.types,
            concrete_object_type,
        ) || crate::query_boundaries::common::is_index_access_type(
            self.ctx.types,
            concrete_object_type,
        ) || crate::query_boundaries::common::is_conditional_type(
            self.ctx.types,
            concrete_object_type,
        ) || (crate::query_boundaries::common::is_primitive_type(
            self.ctx.types,
            concrete_object_type,
        ) && !crate::query_boundaries::dispatch::is_object_like_type(
            self.ctx.types,
            concrete_object_type,
        )) {
            return false;
        }

        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, index_type)
        {
            if members.len() == 2
                && members.contains(&TypeId::STRING)
                && members.contains(&TypeId::NUMBER)
                && !self.is_element_indexable(concrete_object_type, false, true)
            {
                let object_type_str = self.format_type(object_type);
                for index_kind in ["number", "string"] {
                    let message = format_message(
                        diagnostic_messages::TYPE_HAS_NO_MATCHING_INDEX_SIGNATURE_FOR_TYPE,
                        &[&object_type_str, index_kind],
                    );
                    self.error_at_index_type_span(
                        error_anchor,
                        &message,
                        diagnostic_codes::TYPE_HAS_NO_MATCHING_INDEX_SIGNATURE_FOR_TYPE,
                    );
                }
                return true;
            }
            let mut emitted_any = false;
            for &member in members.iter() {
                if member == TypeId::BOOLEAN {
                    for boolean_member in ["false", "true"] {
                        self.emit_index_type_not_usable(error_anchor, boolean_member);
                    }
                    emitted_any = true;
                    continue;
                }

                emitted_any |= self.try_emit_concrete_index_access_error(
                    error_anchor,
                    concrete_object_type,
                    member,
                    object_is_type_parameter_ref,
                );
            }
            return emitted_any;
        }

        if index_type == TypeId::ANY {
            if self.is_element_indexable_by_any_key(concrete_object_type) {
                return false;
            }
            self.emit_index_type_not_usable(error_anchor, "any");
            return true;
        }

        if let Some(invalid_member) = crate::query_boundaries::common::get_invalid_index_type_member(
            self.ctx.types,
            index_type,
        ) {
            let index_type_str = self.format_type(invalid_member);
            self.emit_index_type_not_usable(error_anchor, &index_type_str);
            return true;
        }

        if let Some(prop_atom) =
            crate::query_boundaries::common::string_literal_value(self.ctx.types, index_type)
        {
            let property_name = self.ctx.types.resolve_atom(prop_atom);
            if self
                .union_restricted_literal_property_is_missing(&property_name, concrete_object_type)
            {
                // Suppress TS2339 for types containing type parameters or deferred types.
                let should_suppress = crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    concrete_object_type,
                ) || crate::query_boundaries::common::is_index_access_type(
                    self.ctx.types,
                    concrete_object_type,
                ) || crate::query_boundaries::common::is_conditional_type(
                    self.ctx.types,
                    concrete_object_type,
                ) || concrete_object_type == TypeId::UNKNOWN
                    || concrete_object_type == TypeId::ERROR
                    || crate::query_boundaries::diagnostics::contains_index_access_type(
                        self.ctx.types,
                        concrete_object_type,
                    );
                if !should_suppress {
                    let object_type_str = self.format_type(object_type);
                    let message = format_message(
                        diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                        &[property_name.as_str(), &object_type_str],
                    );
                    self.error_at_node(
                        error_anchor,
                        &message,
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                    );
                }
                return true;
            }
            if self.get_numeric_index_from_string(&property_name).is_some()
                && self.is_element_indexable(concrete_object_type, false, true)
            {
                return false;
            }
            if !matches!(
                self.resolve_property_access_with_env(concrete_object_type, &property_name),
                tsz_solver::operations::property::PropertyAccessResult::Success { .. }
            ) && self.get_index_key_kind(index_type) == Some((true, false))
                && !self.is_element_indexable(concrete_object_type, true, false)
                && !object_is_type_parameter_ref
                && (object_has_shape || object_is_array_like)
            {
                let object_type_str = self.format_type(object_type);
                let message = format_message(
                    diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                    &[property_name.as_str(), &object_type_str],
                );
                self.error_at_node(
                    error_anchor,
                    &message,
                    diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                );
                return true;
            }
        }

        if let Some((wants_string, wants_number)) = self.get_index_key_kind(index_type)
            && !self.is_element_indexable(concrete_object_type, wants_string, wants_number)
        {
            let is_literal_index =
                crate::query_boundaries::common::string_literal_value(self.ctx.types, index_type)
                    .is_some()
                    || crate::query_boundaries::common::number_literal_value(
                        self.ctx.types,
                        index_type,
                    )
                    .is_some();
            if is_literal_index {
                return false;
            }
            if !object_has_shape && !object_is_array_like {
                return false;
            }
            let object_type_str = self.format_type(object_type);
            if wants_string {
                let message = format_message(
                    diagnostic_messages::TYPE_HAS_NO_MATCHING_INDEX_SIGNATURE_FOR_TYPE,
                    &[&object_type_str, "string"],
                );
                self.error_at_index_type_span(
                    error_anchor,
                    &message,
                    diagnostic_codes::TYPE_HAS_NO_MATCHING_INDEX_SIGNATURE_FOR_TYPE,
                );
            }
            if wants_number {
                let message = format_message(
                    diagnostic_messages::TYPE_HAS_NO_MATCHING_INDEX_SIGNATURE_FOR_TYPE,
                    &[&object_type_str, "number"],
                );
                self.error_at_index_type_span(
                    error_anchor,
                    &message,
                    diagnostic_codes::TYPE_HAS_NO_MATCHING_INDEX_SIGNATURE_FOR_TYPE,
                );
            }
            return wants_string || wants_number;
        }

        // tsc's `getPropertyTypeForIndexType` final message selection: a *concrete*
        // object indexed by a valid index *kind* that is neither a string/number
        // signature kind (TS2537, emitted above) nor a literal key (TS2339, emitted
        // above / by the caller) reports TS2538 "Type 'X' cannot be used as an index
        // type". The only kind that reaches here is the `symbol` / `unique symbol`
        // (ESSymbolLike) family: a matching symbol index would have been accepted by
        // the key-space check before this path runs, so reaching here means the
        // concrete object has no symbol index for the key. tsc reserves TS2536
        // ("cannot be used to index type") for generic / type-parameter object
        // types, which the generic guard above already returns `false` for; without
        // this branch a concrete `symbol` index fell through to the caller's terminal
        // TS2536, diverging from tsc on the code (#14230 family).
        if (object_has_shape || object_is_array_like)
            && !object_is_type_parameter_ref
            && crate::query_boundaries::common::is_symbol_or_unique_symbol(
                self.ctx.types,
                index_type,
            )
        {
            // A well-known-symbol-keyed member (`[Symbol.iterator]`) is a *named*
            // member, not a symbol index signature, so the symbol-index key-space
            // check above does not accept it and the access falls through here.
            // `tsc` never reports TS2538 for such a key: when the object declares
            // the member the access is valid (the value-position
            // `i[Symbol.iterator]` resolves it), and when it does not the missing
            // *named* key is a TS2339 ("property does not exist"), not TS2538.
            // Either way the caller's resolver-aware key path (which recovers the
            // canonical `[Symbol.xxx]` name and emits TS2339 only on a genuine
            // miss) owns the outcome, so defer to it rather than emitting a
            // spurious TS2538 here.
            if let Some(sym) = crate::query_boundaries::type_construction::unique_symbol_ref(
                self.ctx.types,
                index_type,
            ) && self.ctx.well_known_symbol_name_for_ref(sym).is_some()
            {
                return false;
            }
            let index_type_str = self.format_type(index_type);
            self.emit_index_type_not_usable(error_anchor, &index_type_str);
            return true;
        }

        false
    }

    /// Emit TS2538 ("Type 'X' cannot be used as an index type") anchored at the
    /// index node. `index_type_display` is the already-formatted index type (e.g.
    /// `"any"`, `"symbol"`, or a `format_type` result) so callers control how the
    /// offending member is rendered.
    fn emit_index_type_not_usable(&mut self, error_anchor: NodeIndex, index_type_display: &str) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        let message = format_message(
            diagnostic_messages::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
            &[index_type_display],
        );
        self.error_at_index_type_span(
            error_anchor,
            &message,
            diagnostic_codes::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
        );
    }
}
