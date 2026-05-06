use super::{NodeIndex, Printer};

impl<'a> Printer<'a> {
    pub(super) fn emit_rest_parameter_spread_prefix(
        &mut self,
        param_pos: u32,
        param_name: NodeIndex,
    ) {
        self.write("...");
        if let Some(name_node) = self.arena.get(param_name) {
            self.emit_comments_after_dot_dot_dot(param_pos, name_node.pos, false);
        }
    }
}
