use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn index_sig_optional_type(&mut self, base_type: TypeId, optional: bool) -> TypeId {
        if optional && self.ctx.strict_null_checks() && !self.ctx.exact_optional_property_types() {
            self.ctx.types.union2(base_type, TypeId::UNDEFINED)
        } else {
            base_type
        }
    }
}
