impl<'a> CheckerState<'a> {
    fn type_node_contains_kind(&self, root: NodeIndex, kind: u16) -> bool {
        let mut stack = vec![root];
        while let Some(idx) = stack.pop() {
            if self
                .ctx
                .arena
                .get(idx)
                .is_some_and(|node| node.kind == kind)
            {
                return true;
            }
            stack.extend(self.ctx.arena.get_children(idx));
        }
        false
    }
}
