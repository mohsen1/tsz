//! Small structural probes shared by the `implements` member checker.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    /// True when the class's computed instance type exposes `member_name`.
    ///
    /// A member can be absent from the class AST body and inheritance chain yet
    /// still present on the instance type — e.g. added by a `declare module`
    /// augmentation or declaration merging (`class X implements X {}`). The
    /// missing-member check consults this before reporting a member missing.
    pub(super) fn class_instance_type_has_member(
        &mut self,
        class_idx: NodeIndex,
        class_data: &tsz_parser::parser::node::ClassData,
        member_name: &str,
    ) -> bool {
        let inst = self.get_class_instance_type(class_idx, class_data);
        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, inst)
        else {
            return false;
        };
        let member_atom = self.ctx.types.intern_string(member_name);
        shape.properties.iter().any(|p| p.name == member_atom)
    }
}
