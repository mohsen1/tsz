//! `Atom`-keyed property-access resolution for [`QueryCache`].
//!
//! Split out of `query_cache.rs` to keep that shard under the 2000-line
//! file-size cap. This is a child module of `query_cache`, so it keeps
//! access to the cache's private fields.

use super::*;

impl QueryCache<'_> {
    /// `Atom`-keyed property-access resolution shared by the `&str` and `Atom`
    /// `QueryDatabase` entry points. The cache key is already `Atom`-based, so
    /// callers holding an `Atom` skip the property-name re-hash entirely.
    pub(super) fn property_access_atom_with_options(
        &self,
        object_type: TypeId,
        prop_atom: Atom,
        no_unchecked_indexed_access: bool,
    ) -> PropertyAccessResult {
        // QueryCache doesn't have full TypeResolver capability, so use
        // PropertyAccessEvaluator with the current QueryDatabase.
        let exact_optional_property_types =
            crate::caches::db::TypeCompilerOptions::exact_optional_property_types(self);
        let key = (
            object_type,
            prop_atom,
            no_unchecked_indexed_access,
            exact_optional_property_types,
        );
        if let Some(result) = self.check_property_cache(key) {
            return result;
        }

        let mut evaluator = crate::operations::property::PropertyAccessEvaluator::new(self);
        evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
        evaluator.set_exact_optional_property_types(exact_optional_property_types);
        let result = evaluator.resolve_property_access_atom(object_type, prop_atom);
        if evaluator.property_result_cacheable() {
            self.insert_property_cache(key, result);
        }
        result
    }
}
