//! Type Operations Matcher - Pattern Matching on Type Properties
//!
//! This module provides helpers for pattern matching on type property combinations.
//!
//! # Problem This Solves
//!
//! When checking type compatibility, we often need to handle specific combinations:
//!
//! ```ignore
//! // BEFORE: Repetitive boolean combinations
//! let is_callable = is_callable_type(db, source);
//! let is_union = is_union_type(db, source);
//! let is_object = is_object_type(db, source);
//!
//! if is_callable && is_union {
//!     // Handle callable union
//! } else if is_callable && is_object {
//!     // Handle callable with object
//! }
//! ```
//!
//! # Solution: TypeOperationsMatcher (with TypeQueryResult)
//!
//! ```ignore
//! // AFTER: Clean query-based approach
//! let query = TypeQueryBuilder::new(db, source_type).build();
//!
//! if query.is_callable && query.is_union {
//!     // Handle callable union
//! } else if query.is_callable && query.is_object {
//!     // Handle callable with object
//! }
//! ```

use crate::type_query_builder::TypeQueryResult;

/// Result of pattern matching on type operations
#[derive(Debug, Clone, Copy)]
pub enum MatchOutcome {
    /// Pattern matched
    Match,

    /// No pattern matched
    NoMatch,
}

impl MatchOutcome {
    /// Check if a pattern matched
    pub fn is_matched(self) -> bool {
        matches!(self, MatchOutcome::Match)
    }
}

/// Helper functions for pattern matching on type queries
pub struct TypeOperationsMatcher;

impl TypeOperationsMatcher {
    /// Check if type matches callable + union pattern
    pub fn is_callable_and_union(query: &TypeQueryResult) -> bool {
        query.is_callable && query.is_union
    }

    /// Check if type matches callable + object pattern
    pub fn is_callable_and_object(query: &TypeQueryResult) -> bool {
        query.is_callable && query.is_object
    }

    /// Check if type matches callable + intersection pattern
    pub fn is_callable_and_intersection(query: &TypeQueryResult) -> bool {
        query.is_callable && query.is_intersection
    }

    /// Check if type matches union + object pattern
    pub fn is_union_and_object(query: &TypeQueryResult) -> bool {
        query.is_union && query.is_object
    }

    /// Check if type is pure union (not combined with others)
    pub fn is_union_only(query: &TypeQueryResult) -> bool {
        query.is_union && !query.is_object && !query.is_callable
    }

    /// Check if type is pure object (not combined with others)
    pub fn is_object_only(query: &TypeQueryResult) -> bool {
        query.is_object && !query.is_union && !query.is_callable
    }

    /// Check if type is pure callable (not combined with others)
    pub fn is_callable_only(query: &TypeQueryResult) -> bool {
        query.is_callable && !query.is_object && !query.is_union
    }

    /// Check if type is any composite (union or intersection)
    pub fn is_any_composite(query: &TypeQueryResult) -> bool {
        query.is_composite
    }

    /// Check if type is any collection (array or tuple)
    pub fn is_any_collection(query: &TypeQueryResult) -> bool {
        query.is_collection
    }

    /// Check if type is primitive
    pub fn is_any_primitive(query: &TypeQueryResult) -> bool {
        query.is_primitive
    }

    /// Match a query against multiple patterns, returning first match
    ///
    /// Example:
    /// ```ignore
    /// let query = TypeQueryBuilder::new(db, type_id).build();
    /// match TypeOperationsMatcher::match_patterns(&query) {
    ///     MatchOutcome::Match if Self::is_callable_and_union(&query) => { /* ... */ }
    ///     MatchOutcome::Match if Self::is_object_only(&query) => { /* ... */ }
    ///     _ => { /* default */ }
    /// }
    /// ```
    pub fn match_patterns(query: &TypeQueryResult) -> MatchOutcome {
        // Try specific combinations
        if Self::is_callable_and_union(query)
            || Self::is_callable_and_object(query)
            || Self::is_union_and_object(query)
        {
            return MatchOutcome::Match;
        }

        // Try single-property patterns
        if Self::is_union_only(query)
            || Self::is_object_only(query)
            || Self::is_callable_only(query)
            || Self::is_any_composite(query)
            || Self::is_any_collection(query)
            || Self::is_any_primitive(query)
        {
            return MatchOutcome::Match;
        }

        MatchOutcome::NoMatch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_outcome_variants() {
        let matched = MatchOutcome::Match;
        let no_match = MatchOutcome::NoMatch;

        assert!(matched.is_matched());
        assert!(!no_match.is_matched());
    }
}
