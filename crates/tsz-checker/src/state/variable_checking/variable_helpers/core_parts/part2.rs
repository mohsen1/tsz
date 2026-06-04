impl<'a> CheckerState<'a> {
    /// Detect `expr as const` / `<const>expr` const assertions structurally:
    /// either the type-node is the bare `const` keyword (newer parser), or it
    /// is a `TypeReference` to an identifier named `const` with no type args
    /// (legacy form). Mirrors the detection in `dispatch.rs` that toggles
    /// `in_const_assertion`.
    fn is_const_assertion_node(&self, node_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let Some(n) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if n.kind != syntax_kind_ext::AS_EXPRESSION && n.kind != syntax_kind_ext::TYPE_ASSERTION {
            return false;
        }
        let Some(assertion) = self.ctx.arena.get_type_assertion(n) else {
            return false;
        };
        self.is_const_assertion_type_node(assertion.type_node)
    }
}
