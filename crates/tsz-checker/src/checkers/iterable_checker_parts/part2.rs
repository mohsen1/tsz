impl<'a> CheckerState<'a> {
    /// Emit the appropriate ES5 non-iterable error:
    /// - TS2802 if the type has `[Symbol.iterator]` (iterable but needs downlevelIteration)
    /// - TS2461 if the type is not an array type (when `allows_strings` is false, or for
    ///   spread/destructuring)
    /// - TS2495 if the type is not an array type or a string type (when `allows_strings` is true,
    ///   only used in for-of)
    fn emit_es5_not_iterable_error(
        &mut self,
        resolved_type: TypeId,
        display_type: TypeId,
        error_node: NodeIndex,
        allows_strings: bool,
    ) {
        if let Some((start, end)) = self.get_node_span(error_node) {
            let type_str = self.format_type(display_type);
            if self.is_iterable_type(resolved_type) {
                let message = format_message(
                    diagnostic_messages::TYPE_CAN_ONLY_BE_ITERATED_THROUGH_WHEN_USING_THE_DOWNLEVELITERATION_FLAG_OR_WITH,
                    &[&type_str],
                );
                self.error(
                    start,
                    end.saturating_sub(start),
                    message,
                    diagnostic_codes::TYPE_CAN_ONLY_BE_ITERATED_THROUGH_WHEN_USING_THE_DOWNLEVELITERATION_FLAG_OR_WITH,
                );
            } else if allows_strings {
                let message = format_message(
                    diagnostic_messages::TYPE_IS_NOT_AN_ARRAY_TYPE_OR_A_STRING_TYPE,
                    &[&type_str],
                );
                self.error(
                    start,
                    end.saturating_sub(start),
                    message,
                    diagnostic_codes::TYPE_IS_NOT_AN_ARRAY_TYPE_OR_A_STRING_TYPE,
                );
            } else {
                let message =
                    format_message(diagnostic_messages::TYPE_IS_NOT_AN_ARRAY_TYPE, &[&type_str]);
                self.error(
                    start,
                    end.saturating_sub(start),
                    message,
                    diagnostic_codes::TYPE_IS_NOT_AN_ARRAY_TYPE,
                );
            }
        }
    }

    /// Check if a type is an array or tuple type (for ES5 destructuring).
    fn is_array_or_tuple_type(&self, type_id: TypeId) -> bool {
        if is_array_type(self.ctx.types, type_id) || is_tuple_type(self.ctx.types, type_id) {
            return true;
        }
        // Check unions: all members must be array/tuple
        if let Some(members) = union_members_for_type(self.ctx.types, type_id) {
            return members
                .iter()
                .all(|&member| self.is_array_or_tuple_type(member));
        }
        false
    }

    /// Check if a type contains a string-like constituent (for ES5 for-of error discrimination).
    ///
    /// This mirrors TSC's `hasStringConstituent` check: when a union type contains a string
    /// member alongside non-array types, the error changes from TS2495 to TS2461.
    fn has_string_constituent(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::STRING || is_string_type(self.ctx.types, type_id) {
            return true;
        }
        if is_string_literal_type(self.ctx.types, type_id) {
            return true;
        }
        if let Some(members) = union_members_for_type(self.ctx.types, type_id) {
            return members.iter().any(|&m| self.has_string_constituent(m));
        }
        false
    }

    /// Check if a type is an array, tuple, or string type (for ES5 for-of).
    fn is_array_or_tuple_or_string(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::STRING || is_string_type(self.ctx.types, type_id) {
            return true;
        }
        if is_array_type(self.ctx.types, type_id) || is_tuple_type(self.ctx.types, type_id) {
            return true;
        }
        // String literals count as string types
        if is_string_literal_type(self.ctx.types, type_id) {
            return true;
        }
        // Check unions: all members must be array/tuple/string
        if let Some(members) = union_members_for_type(self.ctx.types, type_id) {
            return members
                .iter()
                .all(|&member| self.is_array_or_tuple_or_string(member));
        }
        false
    }

    /// Check that the iterator's `next()` parameter type is compatible with what
    /// will be sent to it during iteration.
    ///
    /// For for-of, spread, and destructuring, the sent type is always `undefined`.
    /// For `yield*`, the sent type is the containing generator's `TNext`.
    ///
    /// If incompatible, emits:
    /// - TS2763 for for-of
    /// - TS2764 for array spread
    /// - TS2765 for array destructuring
    /// - TS2766 for yield* delegation
    ///
    /// Returns `true` if compatible or if we can't determine (to avoid false positives).
    pub fn check_iterator_next_type_assignability(
        &mut self,
        iterable_type: TypeId,
        sent_type: TypeId,
        error_node: NodeIndex,
        use_kind: IterationUseKind,
    ) -> bool {
        // Skip for types that can't have meaningful next type checks
        if iterable_type == TypeId::ANY
            || iterable_type == TypeId::UNKNOWN
            || iterable_type == TypeId::ERROR
            || iterable_type == TypeId::STRING
        {
            return true;
        }

        // Try to extract TNext from the Generator/AsyncGenerator/Iterator type directly
        let next_type = self.get_generator_next_type_argument(iterable_type);

        let next_type = match next_type {
            Some(t) => t,
            None => return true, // Can't determine - don't emit false positive
        };

        // If either side is any/unknown, or the iterator accepts undefined, avoid
        // a false positive. `yield*` commonly delegates from generators whose
        // containing TNext is explicitly `unknown`.
        if sent_type == TypeId::ANY || sent_type == TypeId::UNKNOWN {
            return true;
        }

        // If TNext is any, unknown, or undefined, the sent type is always compatible
        if next_type == TypeId::ANY
            || next_type == TypeId::UNKNOWN
            || next_type == TypeId::UNDEFINED
            || common::is_type_parameter_like(self.ctx.types, next_type)
            || common::contains_free_type_parameters(self.ctx.types, next_type)
        {
            return true;
        }

        // A generic or inference-bearing TNext cannot be compared reliably from
        // the declaration alone. Defer rather than reporting TS2763-TS2766 false
        // positives before instantiation supplies the concrete sent type.
        if crate::query_boundaries::common::contains_type_parameters(self.ctx.types, next_type)
            || crate::query_boundaries::common::contains_infer_types(self.ctx.types, next_type)
        {
            return true;
        }

        // Check if the sent type is assignable to the iterator's next type.
        if self.call_arg_relation_outcome(sent_type, next_type).related {
            return true;
        }

        // Not assignable - emit the appropriate diagnostic
        let sent_str = self.format_type(sent_type);
        let next_str = self.format_type(next_type);

        let (message_template, code) = match use_kind {
            IterationUseKind::ForOf => (
                diagnostic_messages::CANNOT_ITERATE_VALUE_BECAUSE_THE_NEXT_METHOD_OF_ITS_ITERATOR_EXPECTS_TYPE_BUT_FO,
                diagnostic_codes::CANNOT_ITERATE_VALUE_BECAUSE_THE_NEXT_METHOD_OF_ITS_ITERATOR_EXPECTS_TYPE_BUT_FO,
            ),
            IterationUseKind::Spread => (
                diagnostic_messages::CANNOT_ITERATE_VALUE_BECAUSE_THE_NEXT_METHOD_OF_ITS_ITERATOR_EXPECTS_TYPE_BUT_AR,
                diagnostic_codes::CANNOT_ITERATE_VALUE_BECAUSE_THE_NEXT_METHOD_OF_ITS_ITERATOR_EXPECTS_TYPE_BUT_AR,
            ),
            IterationUseKind::Destructuring => (
                diagnostic_messages::CANNOT_ITERATE_VALUE_BECAUSE_THE_NEXT_METHOD_OF_ITS_ITERATOR_EXPECTS_TYPE_BUT_AR_2,
                diagnostic_codes::CANNOT_ITERATE_VALUE_BECAUSE_THE_NEXT_METHOD_OF_ITS_ITERATOR_EXPECTS_TYPE_BUT_AR_2,
            ),
            IterationUseKind::YieldStar => (
                diagnostic_messages::CANNOT_DELEGATE_ITERATION_TO_VALUE_BECAUSE_THE_NEXT_METHOD_OF_ITS_ITERATOR_EXPEC,
                diagnostic_codes::CANNOT_DELEGATE_ITERATION_TO_VALUE_BECAUSE_THE_NEXT_METHOD_OF_ITS_ITERATOR_EXPEC,
            ),
        };

        let message = format_message(message_template, &[&sent_str, &next_str]);
        if let Some((start, end)) = self.get_node_span(error_node) {
            self.error(start, end.saturating_sub(start), message, code);
        }

        false
    }
}
