//! Checker-side relation policy compatibility edges.

use tsz_solver::relations::relation_queries::RelationPolicy;

/// Decode checker-owned packed relation flags into the solver policy type.
///
/// New checker relation paths should keep this conversion at the query-boundary
/// edge and pass typed [`RelationPolicy`] values inward.
pub(crate) const fn from_checker_flags_u16(flags: u16) -> RelationPolicy {
    RelationPolicy::from_flags(flags)
}
