//! JSX iterator protocol artifact handling for intrinsic props.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::checkers_domain::jsx) fn jsx_required_props_are_only_iterator_protocol_artifacts(
        &mut self,
        props_type: TypeId,
    ) -> bool {
        let Some(shape) = self.get_normalized_jsx_required_props_shape(props_type) else {
            return false;
        };

        let mut saw_required = false;
        shape
            .properties
            .iter()
            .filter(|prop| !prop.optional)
            .all(|prop| {
                saw_required = true;
                matches!(
                    self.ctx.types.resolve_atom_ref(prop.name).as_ref(),
                    "[Symbol.iterator]" | "__@iterator" | "next"
                )
            })
            && saw_required
    }
}
