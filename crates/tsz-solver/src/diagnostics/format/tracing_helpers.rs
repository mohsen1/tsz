//! Lightweight type display wrappers for tracing output.
//!
//! These wrappers provide human-readable type summaries in tracing spans and
//! events without allocating unless a subscriber actually formats them. When
//! tracing is disabled (the common case), creating a `TypeDisplay` is just
//! two pointer-sized fields on the stack — zero allocation, zero formatting.
//!
//! # Usage in tracing macros
//!
//! ```rust,ignore
//! use tsz_solver::TypeDisplay;
//!
//! tracing::debug_span!(
//!     "check_subtype",
//!     source = %TypeDisplay::new(db, source_id),
//!     target = %TypeDisplay::new(db, target_id),
//! );
//! ```

use crate::TypeDatabase;
use crate::types::TypeId;
use std::fmt;

/// Renders a `TypeId` as a human-readable string on demand.
///
/// Uses [`super::TypeFormatter`] internally with a shallow depth limit (4)
/// to prevent infinite expansion of recursive types in trace output.
///
/// # Performance
///
/// Formatting only occurs when `Display::fmt` is called — i.e., when a
/// tracing subscriber is active and the log level is enabled. When tracing
/// is off, this struct is never formatted and has zero cost.
pub struct TypeDisplay<'a> {
    db: &'a dyn TypeDatabase,
    id: TypeId,
}

impl<'a> TypeDisplay<'a> {
    /// Create a new display wrapper.
    #[inline]
    pub fn new(db: &'a dyn TypeDatabase, id: TypeId) -> Self {
        Self { db, id }
    }
}

impl fmt::Display for TypeDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Fast path for well-known intrinsic types — avoids TypeFormatter creation.
        let fast = match self.id {
            TypeId::NEVER => Some("never"),
            TypeId::UNKNOWN => Some("unknown"),
            TypeId::ANY => Some("any"),
            TypeId::VOID => Some("void"),
            TypeId::UNDEFINED => Some("undefined"),
            TypeId::NULL => Some("null"),
            TypeId::BOOLEAN => Some("boolean"),
            TypeId::NUMBER => Some("number"),
            TypeId::STRING => Some("string"),
            TypeId::BIGINT => Some("bigint"),
            TypeId::SYMBOL => Some("symbol"),
            TypeId::OBJECT => Some("object"),
            TypeId::FUNCTION => Some("Function"),
            TypeId::ERROR => Some("error"),
            TypeId::BOOLEAN_TRUE => Some("true"),
            TypeId::BOOLEAN_FALSE => Some("false"),
            _ => None,
        };

        if let Some(s) = fast {
            return f.write_str(s);
        }

        // Full formatting with a shallow depth limit for trace-friendly output.
        let mut formatter = super::TypeFormatter::new(self.db);
        formatter.max_depth = 4;
        formatter.max_union_members = 5;
        let s = formatter.format(self.id);
        f.write_str(&s)
    }
}

/// Displays a source→target relation pair for tracing.
///
/// Renders as `"source_type <: target_type"`, useful for relation-checking
/// spans where both sides are relevant.
pub struct RelationDisplay<'a> {
    db: &'a dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
}

impl<'a> RelationDisplay<'a> {
    /// Create a new relation display wrapper.
    #[inline]
    pub fn new(db: &'a dyn TypeDatabase, source: TypeId, target: TypeId) -> Self {
        Self { db, source, target }
    }
}

impl fmt::Display for RelationDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} <: {}",
            TypeDisplay::new(self.db, self.source),
            TypeDisplay::new(self.db, self.target),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intern::TypeInterner;

    #[test]
    fn display_intrinsic_types() {
        let interner = TypeInterner::new();
        let db: &dyn TypeDatabase = &interner;

        assert_eq!(TypeDisplay::new(db, TypeId::STRING).to_string(), "string");
        assert_eq!(TypeDisplay::new(db, TypeId::NUMBER).to_string(), "number");
        assert_eq!(TypeDisplay::new(db, TypeId::BOOLEAN).to_string(), "boolean");
        assert_eq!(TypeDisplay::new(db, TypeId::ANY).to_string(), "any");
        assert_eq!(TypeDisplay::new(db, TypeId::UNKNOWN).to_string(), "unknown");
        assert_eq!(TypeDisplay::new(db, TypeId::NEVER).to_string(), "never");
        assert_eq!(TypeDisplay::new(db, TypeId::VOID).to_string(), "void");
        assert_eq!(
            TypeDisplay::new(db, TypeId::UNDEFINED).to_string(),
            "undefined"
        );
        assert_eq!(TypeDisplay::new(db, TypeId::NULL).to_string(), "null");
        assert_eq!(TypeDisplay::new(db, TypeId::OBJECT).to_string(), "object");
        assert_eq!(TypeDisplay::new(db, TypeId::BIGINT).to_string(), "bigint");
        assert_eq!(TypeDisplay::new(db, TypeId::SYMBOL).to_string(), "symbol");
        assert_eq!(
            TypeDisplay::new(db, TypeId::FUNCTION).to_string(),
            "Function"
        );
        assert_eq!(TypeDisplay::new(db, TypeId::ERROR).to_string(), "error");
        assert_eq!(
            TypeDisplay::new(db, TypeId::BOOLEAN_TRUE).to_string(),
            "true"
        );
        assert_eq!(
            TypeDisplay::new(db, TypeId::BOOLEAN_FALSE).to_string(),
            "false"
        );
    }

    #[test]
    fn display_literal_types() {
        let interner = TypeInterner::new();
        let db: &dyn TypeDatabase = &interner;

        let str_lit = interner.literal_string("hello");
        assert_eq!(TypeDisplay::new(db, str_lit).to_string(), r#""hello""#);

        let num_lit = interner.literal_number(42.0);
        assert_eq!(TypeDisplay::new(db, num_lit).to_string(), "42");
    }

    #[test]
    fn display_union_types() {
        let interner = TypeInterner::new();
        let db: &dyn TypeDatabase = &interner;

        let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let result = TypeDisplay::new(db, union).to_string();
        assert!(
            result.contains("string") && result.contains("number"),
            "union display should contain both members: {result}"
        );
    }

    #[test]
    fn display_none_type() {
        let interner = TypeInterner::new();
        let db: &dyn TypeDatabase = &interner;

        // TypeId::NONE (0) has no type data — should not panic
        let result = TypeDisplay::new(db, TypeId::NONE).to_string();
        assert!(!result.is_empty(), "NONE type should produce some output");
    }

    #[test]
    fn relation_display_format() {
        let interner = TypeInterner::new();
        let db: &dyn TypeDatabase = &interner;

        let display = RelationDisplay::new(db, TypeId::NUMBER, TypeId::STRING);
        assert_eq!(display.to_string(), "number <: string");
    }

    #[test]
    fn display_does_not_panic_on_unknown_type_id() {
        let interner = TypeInterner::new();
        let db: &dyn TypeDatabase = &interner;

        // High TypeId that doesn't exist — should not panic
        let result = TypeDisplay::new(db, TypeId(99999)).to_string();
        assert!(!result.is_empty());
    }
}
