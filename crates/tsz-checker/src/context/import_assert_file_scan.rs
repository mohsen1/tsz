//! TS2880 file-wide dynamic-import suppression cache accessors. See
//! `type_position_deprecated_import_assert_files` on `CheckerContext` and
//! `declarations/dynamic_import_checker.rs` for the consumer.

use super::CheckerContext;

impl CheckerContext<'_> {
    /// Cached result of the per-file syntactic pre-scan, if already computed
    /// for `file_idx` this check pass.
    pub(crate) fn cached_file_has_type_position_deprecated_import_assert(
        &self,
        file_idx: usize,
    ) -> Option<bool> {
        self.type_position_deprecated_import_assert_files
            .get(&file_idx)
            .copied()
    }

    /// Records the pre-scan result for `file_idx`.
    pub(crate) fn set_file_has_type_position_deprecated_import_assert(
        &mut self,
        file_idx: usize,
        value: bool,
    ) {
        self.type_position_deprecated_import_assert_files
            .insert(file_idx, value);
    }
}
