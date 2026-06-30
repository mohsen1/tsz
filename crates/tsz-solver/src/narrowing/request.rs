//! Typed entry request for type narrowing.
//!
//! The `NarrowingContext` still owns traversal, recursion guards, and result
//! caching. This module names the request/options stage so cache keys and
//! narrowing configuration stay in one place as the monolithic narrowing
//! engine is staged into explicit pipeline steps.

use crate::narrowing::core::{GuardSense, TypeGuard};
use crate::options::IndexAccessOptions;
use crate::types::TypeId;

/// Options that affect narrowing results.
///
/// Replaces the anonymous packed `u8` produced by `cache_compiler_flags()` with
/// named boolean fields. Any compiler option that changes which type a guard
/// produces must appear here so the narrowing cache key stays accurate without
/// ad-hoc bit-flag maintenance.
///
/// Embeds the shared [`IndexAccessOptions`] newtype, which `EvaluationOptions`
/// also embeds, so the `{no_unchecked_indexed_access,
/// exact_optional_property_types}` pair has a single definition. The derived
/// `Hash` reduces to that newtype's `Hash`, which hashes the two `bool` fields
/// in the same order as before, keeping `NarrowTypeCacheKey` hashing
/// byte-identical.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct NarrowingOptions {
    index_access: IndexAccessOptions,
}

impl NarrowingOptions {
    pub const fn new() -> Self {
        Self {
            index_access: IndexAccessOptions::new(),
        }
    }

    pub const fn with_no_unchecked_indexed_access(mut self, enabled: bool) -> Self {
        self.index_access = self.index_access.with_no_unchecked_indexed_access(enabled);
        self
    }

    pub const fn with_exact_optional_property_types(mut self, enabled: bool) -> Self {
        self.index_access = self
            .index_access
            .with_exact_optional_property_types(enabled);
        self
    }

    pub const fn no_unchecked_indexed_access(self) -> bool {
        self.index_access.no_unchecked_indexed_access()
    }

    pub const fn exact_optional_property_types(self) -> bool {
        self.index_access.exact_optional_property_types()
    }
}

/// Generation-independent key for option-sensitive predicate-guard narrowing.
///
/// Narrowing cache entries are valid only for the resolver generation at which
/// they were computed, but old generations are dead once the resolver advances.
/// Keeping the generation as a separate memo stamp lets the cache retain only
/// recent generations per stable predicate key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct NarrowTypeStableCacheKey {
    source_type: TypeId,
    guard: TypeGuard,
    sense: GuardSense,
    options: NarrowingOptions,
}

/// A normalized request to narrow one type by a guard under caller-supplied
/// inputs.
///
/// Groups `source_type`, `guard`, and `sense` so they travel together and the
/// stable cache key can be built canonically rather than at each call site.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NarrowingRequest {
    source_type: TypeId,
    guard: TypeGuard,
    sense: GuardSense,
}

impl NarrowingRequest {
    pub const fn new(source_type: TypeId, guard: TypeGuard, sense: GuardSense) -> Self {
        Self {
            source_type,
            guard,
            sense,
        }
    }

    pub const fn source_type(&self) -> TypeId {
        self.source_type
    }

    pub const fn guard(&self) -> &TypeGuard {
        &self.guard
    }

    pub const fn sense(&self) -> GuardSense {
        self.sense
    }

    pub(crate) fn stable_cache_key(&self, options: NarrowingOptions) -> NarrowTypeStableCacheKey {
        NarrowTypeStableCacheKey {
            source_type: self.source_type,
            guard: self.guard.clone(),
            sense: self.sense,
            options,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::narrowing::core::{GuardSense, TypeGuard, TypeofKind};
    use crate::types::TypeId;

    #[test]
    fn narrowing_options_default_flags_are_clear() {
        let opts = NarrowingOptions::new();
        assert!(!opts.no_unchecked_indexed_access());
        assert!(!opts.exact_optional_property_types());
    }

    #[test]
    fn narrowing_options_no_unchecked_flag_is_independent() {
        let opts_on = NarrowingOptions::new().with_no_unchecked_indexed_access(true);
        let opts_off = NarrowingOptions::new();
        assert_ne!(
            opts_on, opts_off,
            "no_unchecked_indexed_access flag must distinguish options"
        );
        assert!(!opts_on.exact_optional_property_types());
    }

    #[test]
    fn narrowing_options_exact_optional_flag_is_independent() {
        let opts_on = NarrowingOptions::new().with_exact_optional_property_types(true);
        let opts_off = NarrowingOptions::new();
        assert_ne!(
            opts_on, opts_off,
            "exact_optional_property_types flag must distinguish options"
        );
        assert!(!opts_on.no_unchecked_indexed_access());
    }

    #[test]
    fn narrowing_options_both_flags_independent_of_each_other() {
        let only_unchecked = NarrowingOptions::new().with_no_unchecked_indexed_access(true);
        let only_exact = NarrowingOptions::new().with_exact_optional_property_types(true);
        let both = NarrowingOptions::new()
            .with_no_unchecked_indexed_access(true)
            .with_exact_optional_property_types(true);
        assert_ne!(only_unchecked, only_exact);
        assert_ne!(only_unchecked, both);
        assert_ne!(only_exact, both);
    }

    #[test]
    fn narrowing_request_stable_cache_key_omits_resolver_generation() {
        let guard = TypeGuard::Typeof(TypeofKind::String);
        let req = NarrowingRequest::new(TypeId::NUMBER, guard, GuardSense::Positive);
        let opts = NarrowingOptions::new();
        let key0 = req.stable_cache_key(opts);
        let key1 = req.stable_cache_key(opts);
        assert_eq!(key0, key1, "resolver generation is a memo stamp now");
    }

    #[test]
    fn narrowing_request_cache_key_reflects_options() {
        let guard = TypeGuard::Typeof(TypeofKind::Number);
        let req = NarrowingRequest::new(TypeId::ANY, guard, GuardSense::Negative);
        let opts_default = NarrowingOptions::new();
        let opts_unchecked = NarrowingOptions::new().with_no_unchecked_indexed_access(true);
        let key_default = req.stable_cache_key(opts_default);
        let key_unchecked = req.stable_cache_key(opts_unchecked);
        assert_ne!(
            key_default, key_unchecked,
            "different options must produce different cache keys"
        );
    }

    #[test]
    fn narrowing_request_same_inputs_produce_equal_cache_keys() {
        let make_req =
            || NarrowingRequest::new(TypeId::STRING, TypeGuard::Truthy, GuardSense::Positive);
        let opts = NarrowingOptions::new();
        let k1 = make_req().stable_cache_key(opts);
        let k2 = make_req().stable_cache_key(opts);
        assert_eq!(k1, k2, "equal inputs must produce equal cache keys");
    }
}
