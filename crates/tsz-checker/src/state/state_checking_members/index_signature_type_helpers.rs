use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Returns `base_type | undefined` for optional props under strict-null-checks, unless
    /// `exactOptionalPropertyTypes` is on — EOP means `p?: T` is exactly `T`, not `T | undefined`.
    ///
    /// This mirrors `tsc`'s `getNonMissingTypeOfSymbol`: an optional property's type is read with
    /// the synthetic "missing" marker stripped, so under `exactOptionalPropertyTypes` a `p?: T`
    /// property contributes exactly `T` to the index-signature (TS2411) check. Any *explicit*
    /// `undefined` written in the annotation (`p?: T | undefined`) is preserved, so it still
    /// participates in the check.
    pub(crate) fn index_sig_optional_type(&self, base_type: TypeId, optional: bool) -> TypeId {
        if optional && self.ctx.strict_null_checks() && !self.ctx.exact_optional_property_types() {
            self.ctx.types.union2(base_type, TypeId::UNDEFINED)
        } else {
            base_type
        }
    }
}
