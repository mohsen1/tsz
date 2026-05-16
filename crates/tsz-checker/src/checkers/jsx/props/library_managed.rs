//! JSX `LibraryManagedAttributes` props helpers.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn jsx_props_type_is_library_managed_attributes_application(
        &mut self,
        type_id: TypeId,
    ) -> bool {
        let Some((base, _args)) =
            crate::query_boundaries::state::type_environment::application_info(
                self.ctx.types,
                type_id,
            )
        else {
            return false;
        };
        let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(base) else {
            return false;
        };
        self.get_symbol_globally(sym_id)
            .is_some_and(|symbol| symbol.escaped_name == "LibraryManagedAttributes")
    }
}
