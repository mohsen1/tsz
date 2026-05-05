use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn remapped_mapped_element_access_result(
        &mut self,
        raw_object_type: TypeId,
        pre_resolution_object_type: TypeId,
        index_type: TypeId,
    ) -> Option<TypeId> {
        let resolved_pre = self.resolve_lazy_type(pre_resolution_object_type);
        let mapped_access = crate::query_boundaries::common::remapped_mapped_index_access_result(
            self.ctx.types,
            raw_object_type,
            index_type,
        )
        .or_else(|| {
            crate::query_boundaries::common::remapped_mapped_index_access_result(
                self.ctx.types,
                resolved_pre,
                index_type,
            )
        })?;

        use crate::query_boundaries::common::RemappedMappedIndexAccessResult::{Deferred, Known};
        let value_type = match mapped_access {
            Known(value_type) | Deferred(value_type) => value_type,
        };
        Some(value_type)
    }
}
