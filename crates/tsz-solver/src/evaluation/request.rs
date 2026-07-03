//! Typed entry request for type evaluation.
//!
//! The evaluator still owns traversal, recursion guards, and result caching.
//! This module names the request/options stage so cache keys and evaluator
//! configuration stay in one place as the monolithic evaluator is split.

use crate::options::IndexAccessOptions;
use crate::types::TypeId;

/// Cache key for resolver- and option-sensitive type evaluation.
///
/// Both `no_unchecked_indexed_access` and `exact_optional_property_types` are
/// part of the key because both compiler options change evaluation results
/// (indexed access of optional/array members, homomorphic mapped-modifier
/// stripping). A cache key that omitted either would return a result computed
/// under a different option set if the owning interner's options ever change
/// between writes and reads (the explicit cache reset boundary described in
/// issue #10970).
///
/// Resolver-backed fresh evaluators also stamp the type-database identity,
/// resolver identity, and resolver generation because the same numeric
/// `TypeId` can name different arena-local shapes, and resolving the same
/// `Lazy(DefId)` may produce different bodies as checker environments
/// materialize. Plain resolver-free evaluators keep those dimensions `0`,
/// preserving the existing persistent eval-cache key space.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EvaluationCacheKey {
    type_id: TypeId,
    index_access: IndexAccessOptions,
    type_database_identity: usize,
    resolver_identity: usize,
    resolver_generation: u64,
}

impl EvaluationCacheKey {
    pub const fn new(
        type_id: TypeId,
        no_unchecked_indexed_access: bool,
        exact_optional_property_types: bool,
    ) -> Self {
        Self {
            type_id,
            index_access: IndexAccessOptions::new()
                .with_no_unchecked_indexed_access(no_unchecked_indexed_access)
                .with_exact_optional_property_types(exact_optional_property_types),
            type_database_identity: 0,
            resolver_identity: 0,
            resolver_generation: 0,
        }
    }

    pub const fn with_type_database_identity(mut self, type_database_identity: usize) -> Self {
        self.type_database_identity = type_database_identity;
        self
    }

    pub const fn with_resolver_identity(mut self, resolver_identity: usize) -> Self {
        self.resolver_identity = resolver_identity;
        self
    }

    pub const fn with_resolver_generation(mut self, resolver_generation: u64) -> Self {
        self.resolver_generation = resolver_generation;
        self
    }

    pub const fn type_id(self) -> TypeId {
        self.type_id
    }

    pub const fn resolver_generation(self) -> u64 {
        self.resolver_generation
    }

    pub const fn type_database_identity(self) -> usize {
        self.type_database_identity
    }

    pub const fn resolver_identity(self) -> usize {
        self.resolver_identity
    }

    pub const fn no_unchecked_indexed_access(self) -> bool {
        self.index_access.no_unchecked_indexed_access()
    }

    pub const fn exact_optional_property_types(self) -> bool {
        self.index_access.exact_optional_property_types()
    }
}

/// Options that affect type evaluation results.
///
/// Embeds the shared [`IndexAccessOptions`] newtype, which `NarrowingOptions`
/// also embeds, so the `{no_unchecked_indexed_access,
/// exact_optional_property_types}` pair has a single definition threaded into
/// both stages' cache keys.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EvaluationOptions {
    index_access: IndexAccessOptions,
}

impl EvaluationOptions {
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

/// A normalized request to evaluate one type under explicit options and
/// resolver-visible state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvaluationRequest {
    type_id: TypeId,
    options: EvaluationOptions,
    type_database_identity: usize,
    resolver_identity: usize,
    resolver_generation: u64,
}

impl EvaluationRequest {
    pub const fn new(type_id: TypeId) -> Self {
        Self {
            type_id,
            options: EvaluationOptions::new(),
            type_database_identity: 0,
            resolver_identity: 0,
            resolver_generation: 0,
        }
    }

    pub const fn with_options(type_id: TypeId, options: EvaluationOptions) -> Self {
        Self {
            type_id,
            options,
            type_database_identity: 0,
            resolver_identity: 0,
            resolver_generation: 0,
        }
    }

    pub const fn with_type_id(mut self, type_id: TypeId) -> Self {
        self.type_id = type_id;
        self
    }

    pub const fn with_type_database_identity(mut self, type_database_identity: usize) -> Self {
        self.type_database_identity = type_database_identity;
        self
    }

    pub const fn with_resolver_identity(mut self, resolver_identity: usize) -> Self {
        self.resolver_identity = resolver_identity;
        self
    }

    pub const fn with_resolver_generation(mut self, resolver_generation: u64) -> Self {
        self.resolver_generation = resolver_generation;
        self
    }

    pub const fn with_no_unchecked_indexed_access(mut self, enabled: bool) -> Self {
        self.options = self.options.with_no_unchecked_indexed_access(enabled);
        self
    }

    pub const fn with_exact_optional_property_types(mut self, enabled: bool) -> Self {
        self.options = self.options.with_exact_optional_property_types(enabled);
        self
    }

    pub const fn type_id(self) -> TypeId {
        self.type_id
    }

    pub const fn options(self) -> EvaluationOptions {
        self.options
    }

    pub const fn resolver_generation(self) -> u64 {
        self.resolver_generation
    }

    pub const fn type_database_identity(self) -> usize {
        self.type_database_identity
    }

    pub const fn resolver_identity(self) -> usize {
        self.resolver_identity
    }

    pub const fn no_unchecked_indexed_access(self) -> bool {
        self.options.no_unchecked_indexed_access()
    }

    pub const fn exact_optional_property_types(self) -> bool {
        self.options.exact_optional_property_types()
    }

    pub const fn cache_key(self) -> EvaluationCacheKey {
        EvaluationCacheKey::new(
            self.type_id,
            self.options.no_unchecked_indexed_access(),
            self.options.exact_optional_property_types(),
        )
        .with_type_database_identity(self.type_database_identity)
        .with_resolver_identity(self.resolver_identity)
        .with_resolver_generation(self.resolver_generation)
    }
}

#[cfg(test)]
mod tests {
    use super::{EvaluationCacheKey, EvaluationOptions, EvaluationRequest};
    use crate::construction::TypeInterner;
    use crate::evaluation::evaluate::evaluate_type_with_request;
    use crate::types::{
        MappedModifier, MappedType, PropertyInfo, TypeData, TypeId, TypeParamInfo, TypeParamOrigin,
    };

    #[test]
    fn default_request_cache_key_disables_no_unchecked_indexed_access() {
        let request = EvaluationRequest::new(TypeId::STRING);

        assert_eq!(request.type_id(), TypeId::STRING);
        assert_eq!(request.resolver_generation(), 0);
        assert_eq!(request.type_database_identity(), 0);
        assert_eq!(request.resolver_identity(), 0);
        assert!(!request.no_unchecked_indexed_access());
        assert_eq!(
            request.cache_key(),
            EvaluationCacheKey::new(TypeId::STRING, false, false)
        );
        assert_eq!(request.cache_key().type_id(), TypeId::STRING);
        assert_eq!(request.cache_key().resolver_generation(), 0);
        assert_eq!(request.cache_key().type_database_identity(), 0);
        assert_eq!(request.cache_key().resolver_identity(), 0);
        assert!(!request.cache_key().no_unchecked_indexed_access());
    }

    #[test]
    fn request_cache_key_tracks_no_unchecked_indexed_access() {
        let request = EvaluationRequest::with_options(
            TypeId::NUMBER,
            EvaluationOptions::new().with_no_unchecked_indexed_access(true),
        );

        assert!(request.no_unchecked_indexed_access());
        assert_eq!(
            request.cache_key(),
            EvaluationCacheKey::new(TypeId::NUMBER, true, false)
        );
        assert_eq!(
            request.with_type_id(TypeId::BOOLEAN).cache_key(),
            EvaluationCacheKey::new(TypeId::BOOLEAN, true, false)
        );
    }

    #[test]
    fn request_cache_key_tracks_exact_optional_property_types() {
        let request = EvaluationRequest::with_options(
            TypeId::NUMBER,
            EvaluationOptions::new().with_exact_optional_property_types(true),
        );

        assert!(request.exact_optional_property_types());
        assert!(!request.no_unchecked_indexed_access());
        assert!(request.cache_key().exact_optional_property_types());
        assert!(!request.cache_key().no_unchecked_indexed_access());
        assert_eq!(
            request.cache_key(),
            EvaluationCacheKey::new(TypeId::NUMBER, false, true)
        );
        // The two option flags are independent discriminants: flipping one must
        // not collide with the other being set.
        assert_ne!(
            EvaluationCacheKey::new(TypeId::NUMBER, true, false),
            EvaluationCacheKey::new(TypeId::NUMBER, false, true)
        );
    }

    #[test]
    fn request_cache_key_tracks_resolver_generation() {
        let request = EvaluationRequest::new(TypeId::STRING).with_resolver_generation(7);

        assert_eq!(request.resolver_generation(), 7);
        assert_eq!(
            request.cache_key(),
            EvaluationCacheKey::new(TypeId::STRING, false, false).with_resolver_generation(7)
        );
        assert_ne!(
            request.cache_key(),
            EvaluationCacheKey::new(TypeId::STRING, false, false)
        );
        assert_eq!(
            request.with_type_id(TypeId::NUMBER).cache_key(),
            EvaluationCacheKey::new(TypeId::NUMBER, false, false).with_resolver_generation(7)
        );
    }

    #[test]
    fn request_cache_key_tracks_arena_and_resolver_identity() {
        let request = EvaluationRequest::new(TypeId::STRING)
            .with_type_database_identity(11)
            .with_resolver_identity(22)
            .with_resolver_generation(7);

        assert_eq!(request.type_database_identity(), 11);
        assert_eq!(request.resolver_identity(), 22);
        assert_eq!(request.cache_key().type_database_identity(), 11);
        assert_eq!(request.cache_key().resolver_identity(), 22);
        assert_eq!(
            request.cache_key(),
            EvaluationCacheKey::new(TypeId::STRING, false, false)
                .with_type_database_identity(11)
                .with_resolver_identity(22)
                .with_resolver_generation(7)
        );
        assert_ne!(
            request.cache_key(),
            EvaluationCacheKey::new(TypeId::STRING, false, false)
                .with_type_database_identity(12)
                .with_resolver_identity(22)
                .with_resolver_generation(7)
        );
        assert_ne!(
            request.cache_key(),
            EvaluationCacheKey::new(TypeId::STRING, false, false)
                .with_type_database_identity(11)
                .with_resolver_identity(23)
                .with_resolver_generation(7)
        );
    }

    #[test]
    fn request_routes_no_unchecked_indexed_access_option() {
        let interner = TypeInterner::new();
        let array = interner.array(TypeId::STRING);
        let indexed = interner.index_access(array, TypeId::NUMBER);

        let default_result = evaluate_type_with_request(&interner, EvaluationRequest::new(indexed));
        assert_eq!(default_result, TypeId::STRING);

        let no_unchecked_result = evaluate_type_with_request(
            &interner,
            EvaluationRequest::new(indexed).with_no_unchecked_indexed_access(true),
        );
        let expected = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
        assert_eq!(no_unchecked_result, expected);
    }

    #[test]
    fn request_routes_exact_optional_property_types_option() {
        let interner = TypeInterner::new();
        let prop = interner.intern_string("value");
        let key_name = interner.intern_string("K");
        let key_param_info = TypeParamInfo {
            name: key_name,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::User,
        };
        let key_param = interner.type_param(key_param_info);
        let number_or_undefined = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
        let source = interner.object(vec![PropertyInfo::opt(prop, number_or_undefined)]);
        let mapped = interner.mapped(MappedType {
            type_param: key_param_info,
            constraint: interner.keyof(source),
            name_type: None,
            template: interner.index_access(source, key_param),
            optional_modifier: Some(MappedModifier::Remove),
            readonly_modifier: None,
        });

        let legacy_result = evaluate_type_with_request(&interner, EvaluationRequest::new(mapped));
        let exact_result = evaluate_type_with_request(
            &interner,
            EvaluationRequest::new(mapped).with_exact_optional_property_types(true),
        );

        assert_eq!(
            mapped_property_type(&interner, legacy_result, prop),
            TypeId::NUMBER,
            "legacy optional mode strips top-level undefined when -? removes optionality"
        );
        assert_eq!(
            mapped_property_type(&interner, exact_result, prop),
            number_or_undefined,
            "exact optional mode preserves explicit undefined under -?"
        );
    }

    fn mapped_property_type(
        interner: &TypeInterner,
        object: TypeId,
        prop: tsz_common::Atom,
    ) -> TypeId {
        let Some(TypeData::Object(shape_id)) = interner.lookup(object) else {
            panic!("expected evaluated mapped type to produce an object");
        };
        interner
            .object_shape(shape_id)
            .properties
            .iter()
            .find(|property| property.name == prop)
            .map(|property| property.type_id)
            .expect("mapped object should contain requested property")
    }
}
