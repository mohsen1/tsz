use super::*;

impl<'a> TypeInstantiator<'a> {
    /// Propagate display properties from intersection members to the result.
    #[allow(dead_code)]
    pub(super) fn propagate_display_properties_for_intersection(
        &self,
        original_members: &[TypeId],
        result: TypeId,
    ) {
        let display_vec = crate::types::merge_display_properties_for_intersection(
            self.interner,
            original_members,
        );
        if !display_vec.is_empty() {
            self.interner.store_display_properties(result, display_vec);
        }
    }
}
