//! Materialize-or-defer apparent-type gateway.
//!
//! A single chokepoint that reduces a checker *decision-site* operand toward a
//! decidable apparent type and reports, in-band, when materialization is
//! blocked. Decision sites (property-access receiver reduction today; TS2344
//! constraint validation and relation member-reads next) route their operand
//! through this gateway instead of calling the raw evaluation entries
//! (`evaluate_type_with_resolution`, `evaluate_type_with_env`, its
//! `_uncached` variant, `evaluate_application_type`,
//! `evaluate_application_type_for_property_access`,
//! `evaluate_type_for_assignability`, and the `evaluate_property_access_receiver_type`
//! composite) directly — the full set the `raw_entry` arch-scan test pins.
//!
//! The pre-gateway hazard (issue #15396): those entries each return the
//! **opaque input** `TypeId` when their own cycle/depth/fuel guard trips, so the
//! same deferred `Lazy`/`Application` is materialized on one path and left
//! opaque on another, and a decision site cannot tell "reduced to this concrete
//! type" from "gave up and handed the operand back" — deciding on the opaque
//! form yields both false positives (fail-closed checks) and false negatives
//! (skipped reductions). The gateway makes the distinction explicit via
//! [`ApparentType`]: [`ApparentType::Decidable`] carries a materialized form,
//! [`ApparentType::Deferred`] carries the best-effort reduction of a form that
//! is still an unmaterialized `Lazy`/`Application`.
//!
//! This first step is behavior-preserving: both property-access receiver
//! branches consume the verdict through [`ApparentType::into_type`], which
//! reproduces the pre-gateway result byte-for-byte. A follow-up migrates the
//! TS2344 / relation-member sites to branch on `Deferred` (and to grow a defer
//! reason where a consumer needs one).
//!
//! Only the gateway calls the raw evaluation entries from a decision-site
//! context; the `apparent_type::raw_entry` arch-scan test pins the residual
//! count of direct raw-entry calls in the property-access decision site
//! shrink-only (interim scaffold until the entries can be visibility-scoped).

use crate::query_boundaries::state::type_environment::{self as query, TypeResolutionKind};
use crate::state::CheckerState;
use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;

/// Verdict of the materialize-or-defer apparent-type gateway.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ApparentType {
    /// Reduced to a decidable (non-deferred) apparent type; safe to judge
    /// structurally.
    Decidable(TypeId),
    /// Materialization is blocked — the reduced form is still a bare
    /// `Lazy`/`Application`. Carries the best-effort reduction so a
    /// not-yet-migrated site keeps byte-parity.
    Deferred(TypeId),
}

impl ApparentType {
    /// The reduced `TypeId` regardless of decidability.
    ///
    /// Byte-parity accessor: a site that has not yet adopted defer semantics
    /// reproduces the exact pre-gateway result, since the raw entries also
    /// returned their best-effort (possibly opaque) reduction.
    pub(crate) const fn into_type(self) -> TypeId {
        match self {
            ApparentType::Decidable(type_id) | ApparentType::Deferred(type_id) => type_id,
        }
    }
}

/// Classify an already-reduced operand as decidable or still-deferred.
///
/// A reduced form that is still a bare `Lazy`/`Application` means the raw entry
/// handed the operand back opaque; anything else is a materialized apparent
/// type. Reuses the solver's `classify_for_type_resolution` trichotomy.
fn classify_apparent_type(db: &dyn TypeDatabase, reduced: TypeId) -> ApparentType {
    match query::classify_for_type_resolution(db, reduced) {
        TypeResolutionKind::Lazy(_) | TypeResolutionKind::Application => {
            ApparentType::Deferred(reduced)
        }
        TypeResolutionKind::Resolved => ApparentType::Decidable(reduced),
    }
}

impl CheckerState<'_> {
    /// Reduce a property-access receiver through the environment materializer to
    /// a materialize-or-defer verdict.
    ///
    /// Wraps `evaluate_property_access_receiver_type` (env evaluator, then the
    /// lighter application evaluator on no progress). Used for receivers that
    /// [`receiver_needs_env_materialization`](CheckerState::receiver_needs_env_materialization)
    /// flags — non-lib generic interface applications and the legacy builder
    /// `.select` access — whose type arguments must substitute into the members
    /// before lookup.
    pub(crate) fn apparent_type_of_receiver_env(&mut self, receiver: TypeId) -> ApparentType {
        let reduced = self.evaluate_property_access_receiver_type(receiver);
        classify_apparent_type(self.ctx.types, reduced)
    }

    /// Reduce a property-access receiver through the lighter application
    /// evaluator to a materialize-or-defer verdict.
    ///
    /// Wraps `evaluate_application_type` — the default receiver reduction when a
    /// receiver needs neither env materialization nor arena-collided /
    /// alias-wrapped interface recovery.
    pub(crate) fn apparent_type_of_receiver_light(&mut self, receiver: TypeId) -> ApparentType {
        let reduced = self.evaluate_application_type(receiver);
        classify_apparent_type(self.ctx.types, reduced)
    }
}

#[cfg(test)]
mod raw_entry {
    //! Arch scan (issue #15396): pin the count of direct raw evaluation-entry
    //! calls in the property-access receiver decision site shrink-only.
    //!
    //! Only [`apparent_type`](super) is allowed to call the raw entries from a
    //! decision-site context. Before this gateway, `resolve.rs` called
    //! `evaluate_property_access_receiver_type` and `evaluate_application_type`
    //! directly to reduce the receiver; those two are now routed through the
    //! gateway. This test fails if a new raw-entry call is added to the decision
    //! site (ratchet up) so migrations of the remaining sites can only shrink
    //! the baseline. Interim scaffold: substring-based, until the raw entries
    //! can be visibility-scoped to this module.

    /// Source of the property-access receiver decision site.
    const RESOLVE_SRC: &str = include_str!("../types/property_access_type/resolve.rs");

    /// Raw evaluation entries a decision site must not call directly.
    const RAW_ENTRIES: &[&str] = &[
        "self.evaluate_property_access_receiver_type(",
        "self.evaluate_application_type(",
        "self.evaluate_application_type_for_property_access(",
        "self.evaluate_type_with_env(",
        "self.evaluate_type_with_env_uncached(",
        "self.evaluate_type_with_resolution(",
        "self.evaluate_type_for_assignability(",
    ];

    /// Residual direct raw-entry calls left in `resolve.rs` after routing the
    /// receiver reduction through the gateway. Now zero: every property-access
    /// receiver reduction — the primary env/light branches and the three
    /// `UNKNOWN`-recovery / no-flow-probe re-reductions — routes through
    /// [`apparent_type`](super). Shrink-only: never raise it.
    const BASELINE: usize = 0;

    fn raw_entry_call_count(src: &str) -> usize {
        RAW_ENTRIES
            .iter()
            .map(|needle| src.matches(needle).count())
            .sum()
    }

    #[test]
    fn property_access_decision_site_stays_shrink_only() {
        let count = raw_entry_call_count(RESOLVE_SRC);
        // Exact match, not `<=`: `BASELINE` is `0` (the site is fully behind the
        // gateway), so a `count <= BASELINE` would be an always-true comparison
        // against `usize::MIN` (`clippy::absurd_extreme_comparisons`). Any raw
        // evaluate_* call added to the decision site makes `count > BASELINE` and
        // fails; migrating one out (only possible if `BASELINE` is later raised)
        // makes `count < BASELINE` and must be paired with lowering `BASELINE`.
        assert_eq!(
            count, BASELINE,
            "property-access decision site changed its raw evaluation-entry call \
             count ({count} vs baseline {BASELINE}); route new receiver reductions \
             through query_boundaries::apparent_type instead of calling a raw \
             evaluate_* entry directly. If you migrated a call *out*, lower BASELINE \
             to {count}.",
        );
    }

    #[test]
    fn receiver_reduction_uses_the_gateway() {
        assert!(
            RESOLVE_SRC.contains("apparent_type_of_receiver_env(")
                && RESOLVE_SRC.contains("apparent_type_of_receiver_light("),
            "property-access receiver reduction should route through the \
             apparent_type gateway",
        );
    }
}
