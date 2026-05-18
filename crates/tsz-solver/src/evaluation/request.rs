//! Typed entry request for type evaluation.
//!
//! The evaluator still owns traversal, recursion guards, and result caching.
//! This module names the request/options stage so cache keys and evaluator
//! configuration stay in one place as the monolithic evaluator is split.

use crate::types::TypeId;

/// Cache key for option-sensitive type evaluation.
pub type EvaluationCacheKey = (TypeId, bool);

/// Options that affect type evaluation results.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EvaluationOptions {
    no_unchecked_indexed_access: bool,
}

impl EvaluationOptions {
    pub const fn new() -> Self {
        Self {
            no_unchecked_indexed_access: false,
        }
    }

    pub const fn with_no_unchecked_indexed_access(mut self, enabled: bool) -> Self {
        self.no_unchecked_indexed_access = enabled;
        self
    }

    pub const fn no_unchecked_indexed_access(self) -> bool {
        self.no_unchecked_indexed_access
    }
}

/// A normalized request to evaluate one type under explicit options.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvaluationRequest {
    type_id: TypeId,
    options: EvaluationOptions,
}

impl EvaluationRequest {
    pub const fn new(type_id: TypeId) -> Self {
        Self {
            type_id,
            options: EvaluationOptions::new(),
        }
    }

    pub const fn with_options(type_id: TypeId, options: EvaluationOptions) -> Self {
        Self { type_id, options }
    }

    pub const fn with_type_id(mut self, type_id: TypeId) -> Self {
        self.type_id = type_id;
        self
    }

    pub const fn with_no_unchecked_indexed_access(mut self, enabled: bool) -> Self {
        self.options = self.options.with_no_unchecked_indexed_access(enabled);
        self
    }

    pub const fn type_id(self) -> TypeId {
        self.type_id
    }

    pub const fn options(self) -> EvaluationOptions {
        self.options
    }

    pub const fn no_unchecked_indexed_access(self) -> bool {
        self.options.no_unchecked_indexed_access()
    }

    pub const fn cache_key(self) -> EvaluationCacheKey {
        (self.type_id, self.options.no_unchecked_indexed_access())
    }
}

#[cfg(test)]
mod tests {
    use super::{EvaluationOptions, EvaluationRequest};
    use crate::TypeInterner;
    use crate::evaluation::evaluate::evaluate_type_with_request;
    use crate::types::TypeId;

    #[test]
    fn default_request_cache_key_disables_no_unchecked_indexed_access() {
        let request = EvaluationRequest::new(TypeId::STRING);

        assert_eq!(request.type_id(), TypeId::STRING);
        assert!(!request.no_unchecked_indexed_access());
        assert_eq!(request.cache_key(), (TypeId::STRING, false));
    }

    #[test]
    fn request_cache_key_tracks_no_unchecked_indexed_access() {
        let request = EvaluationRequest::with_options(
            TypeId::NUMBER,
            EvaluationOptions::new().with_no_unchecked_indexed_access(true),
        );

        assert!(request.no_unchecked_indexed_access());
        assert_eq!(request.cache_key(), (TypeId::NUMBER, true));
        assert_eq!(
            request.with_type_id(TypeId::BOOLEAN).cache_key(),
            (TypeId::BOOLEAN, true)
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
}
