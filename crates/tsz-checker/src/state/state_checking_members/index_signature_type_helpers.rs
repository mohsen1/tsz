use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Returns `base_type | undefined` for optional props under strict-null-checks, unless
    /// `exactOptionalPropertyTypes` is on — EOP means `p?: T` is exactly `T`, not `T | undefined`.
    pub(crate) fn index_sig_optional_type(&self, base_type: TypeId, optional: bool) -> TypeId {
        if optional && self.ctx.strict_null_checks() && !self.ctx.exact_optional_property_types() {
            self.ctx.types.union2(base_type, TypeId::UNDEFINED)
        } else {
            base_type
        }
    }
}
