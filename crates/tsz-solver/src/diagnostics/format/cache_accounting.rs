use super::{TypeFormatter, TypeFormatterCacheStatistics};
use crate::def::DefId;
use crate::types::TypeId;
use std::mem::size_of;
use std::sync::Arc;
use tsz_common::interner::Atom;

impl<'a> TypeFormatter<'a> {
    /// Return cache entry and residency accounting for this formatter.
    pub fn cache_statistics(&self) -> TypeFormatterCacheStatistics {
        let application_reduction_cache_entries = self.application_reduction_cache.borrow().len();
        let recursive_alias_base_cache_entries = self.recursive_alias_base_cache.borrow().len();
        TypeFormatterCacheStatistics {
            atom_cache_entries: self.atom_cache.len(),
            application_reduction_cache_entries,
            recursive_alias_base_cache_entries,
            estimated_size_bytes: self.estimated_size_bytes(),
        }
    }

    /// Estimate memory retained by this operation-local formatter.
    pub fn estimated_size_bytes(&self) -> usize {
        size_of::<Self>()
            + self.atom_cache.capacity() * size_of::<(Atom, Arc<str>)>()
            + self.display_alias_visiting.capacity() * size_of::<TypeId>()
            + self.format_visiting.capacity() * size_of::<TypeId>()
            + self.skip_type_alias_def_ids.capacity() * size_of::<DefId>()
            + self.skipped_type_alias_expansion_visiting.capacity() * size_of::<DefId>()
            + self.application_reduction_cache.borrow().capacity()
                * size_of::<(TypeId, Option<TypeId>)>()
            + self.recursive_alias_base_cache.borrow().capacity() * size_of::<(TypeId, bool)>()
    }
}
