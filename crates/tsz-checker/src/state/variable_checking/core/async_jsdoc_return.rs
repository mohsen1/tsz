impl<'a> CheckerState<'a> {
    /// Check whether an async function initializer's type differs from the
    /// declared JSDoc type only because of Promise wrapping on the return type.
    /// When that is the case, tsc reports TS1064 (async return type must be
    /// Promise) but suppresses the assignment-level TS2322.
    pub(crate) fn async_function_jsdoc_return_type_suppression(
        &mut self,
        init_type: TypeId,
        declared_type: TypeId,
    ) -> bool {
        let init_return =
            crate::query_boundaries::common::return_type_for_type(self.ctx.types, init_type);
        let declared_return =
            crate::query_boundaries::common::return_type_for_type(self.ctx.types, declared_type);
        let (Some(init_ret), Some(decl_ret)) = (init_return, declared_return) else {
            return false;
        };
        // Check if the init return type is Promise<T> where T is assignable
        // to the declared return type.
        if let Some(unwrapped) = self.unwrap_promise_type(init_ret) {
            self.assign_relation_outcome(unwrapped, decl_ret).related
        } else {
            false
        }
    }
}
