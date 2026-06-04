impl<'a> CheckerState<'a> {
    /// Check a binary operation with `IndexAccess` operands is valid through assignability.
    pub(super) fn resolve_indexed_access_binary_op(
        &mut self,
        left: TypeId,
        right: TypeId,
        op: &str,
    ) -> bool {
        let left_is_index_access =
            crate::query_boundaries::common::is_index_access_type(self.ctx.types, left);
        let right_is_index_access =
            crate::query_boundaries::common::is_index_access_type(self.ctx.types, right);

        if !left_is_index_access && !right_is_index_access {
            return false;
        }

        match op {
            "+" | "-" | "*" | "/" | "%" | "**" => {
                let left_ok = crate::query_boundaries::type_computation::core::is_arithmetic_operand(
                    self.ctx.types,
                    left,
                )
                    || left_is_index_access
                        && self
                            .binary_arithmetic_number_relation_outcome(left, TypeId::NUMBER)
                            .related;
                let right_ok =
                    crate::query_boundaries::type_computation::core::is_arithmetic_operand(
                        self.ctx.types,
                        right,
                    ) || right_is_index_access
                        && self
                            .binary_arithmetic_number_relation_outcome(right, TypeId::NUMBER)
                            .related;
                left_ok && right_ok
            }
            _ => false,
        }
    }
}
