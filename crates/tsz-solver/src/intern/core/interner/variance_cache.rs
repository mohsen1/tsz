//! Universe-shared variance-cache data and accessors.

use super::TypeInterner;
use crate::def::DefId;
use crate::types::Variance;
use std::sync::Arc;

/// A declared-variance result plus its unresolved-definition fingerprint.
pub type SharedDefVariance = (Arc<[Variance]>, Arc<[DefId]>);

impl TypeInterner {
    /// Read a universe-shared declared-variance mask.
    #[inline]
    pub fn shared_def_variance(&self, def_id: DefId) -> Option<SharedDefVariance> {
        self.def_variance_masks
            .get(&def_id)
            .map(|entry| entry.value().clone())
    }

    /// Store a canonical universe-shared declared-variance mask.
    #[inline]
    pub fn insert_shared_def_variance(
        &self,
        def_id: DefId,
        mask: Arc<[Variance]>,
        gaps: Arc<[DefId]>,
    ) {
        self.def_variance_masks
            .entry(def_id)
            .or_insert((mask, gaps));
    }
}
