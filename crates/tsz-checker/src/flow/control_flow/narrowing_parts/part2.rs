impl<'a> FlowAnalyzer<'a> {
    /// For `typeof a.prop === "undefined"`, extract the property path from
    /// the typeof operand relative to `target` and the comparison literal.
    /// Returns (`property_path`, `is_optional_chain`, `typeof_literal_string`) if the typeof operand
    /// is a property access chain rooted at `target`.
    pub(super) fn typeof_discriminant_path(
        &self,
        left: NodeIndex,
        right: NodeIndex,
        target: NodeIndex,
    ) -> Option<(Vec<Atom>, bool, &str)> {
        // Single-segment path only (see `discriminant_property_info`):
        // `typeof s.meta.x === "string"` narrows `s.meta`, never the outer union `s`.
        // Try left = typeof expr, right = string literal
        if let Some(operand) = self.get_typeof_operand(self.skip_parenthesized(left))
            && let Some((path, is_optional)) = self.relative_discriminant_path(operand, target)
            && path.len() == 1
            && let Some(lit) = self.literal_string_from_node(right)
        {
            return Some((path, is_optional, lit));
        }
        // Try right = typeof expr, left = string literal
        if let Some(operand) = self.get_typeof_operand(self.skip_parenthesized(right))
            && let Some((path, is_optional)) = self.relative_discriminant_path(operand, target)
            && path.len() == 1
            && let Some(lit) = self.literal_string_from_node(left)
        {
            return Some((path, is_optional, lit));
        }
        None
    }
}
