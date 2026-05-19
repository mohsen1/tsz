use std::mem;

use tsz_parser::parser::NodeIndex;

use super::{CheckerContext, CrossFileTypeParamsCache};

/// Residency statistics for [`CrossFileTypeParamsCache`].
///
/// Owner: `ProgramContext`/`CheckerContext`; invalidated at the project-run or
/// project-version boundary by dropping the shared cache.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CrossFileTypeParamsCacheStatistics {
    /// Number of declaration entries keyed by `(target_file_idx, decl_idx)`.
    pub entries: usize,
    /// Total `TypeParamInfo` values stored across all entries.
    pub type_param_entries: usize,
    estimated_size_bytes: usize,
}

impl CrossFileTypeParamsCacheStatistics {
    /// Estimated heap bytes owned by the cache entries.
    #[must_use]
    pub const fn estimated_size_bytes(self) -> usize {
        self.estimated_size_bytes
    }
}

/// Return entry and size accounting for a shared cross-file type-params cache.
#[must_use]
pub fn cross_file_type_params_cache_statistics(
    cache: &CrossFileTypeParamsCache,
) -> CrossFileTypeParamsCacheStatistics {
    let type_param_entries: usize = cache.iter().map(|entry| entry.value().len()).sum();
    let entries = cache.len();
    let estimated_size_bytes = entries
        .saturating_mul(
            mem::size_of::<(u32, NodeIndex)>() + mem::size_of::<Vec<tsz_solver::TypeParamInfo>>(),
        )
        .saturating_add(
            type_param_entries.saturating_mul(mem::size_of::<tsz_solver::TypeParamInfo>()),
        );

    CrossFileTypeParamsCacheStatistics {
        entries,
        type_param_entries,
        estimated_size_bytes,
    }
}

impl<'a> CheckerContext<'a> {
    /// Return statistics for the optional shared cross-file type-params cache.
    #[must_use]
    pub fn cross_file_type_params_cache_statistics(
        &self,
    ) -> Option<CrossFileTypeParamsCacheStatistics> {
        self.cross_file_type_params_cache
            .as_ref()
            .map(cross_file_type_params_cache_statistics)
    }
}
