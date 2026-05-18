//! Typed recovery sentinels for checker `TypeId::ANY` fallbacks.
//!
//! Bare `TypeId::ANY` returned from a recovery path is indistinguishable
//! from a user-written `: any` annotation. [`CheckerContext::recover_any`]
//! is the single named entry-point that records the (node, reason) pair in
//! a per-checker [`RecoverySites`] registry and emits a structured
//! `tracing::debug!` event, so relation/diagnostic paths can later
//! distinguish a *recovered* ANY from a declared `any` without inspecting
//! printed type strings.
//!
//! Add a new recovery family by adding a variant to [`RecoveryReason`] and
//! routing the inline `TypeId::ANY` fallback through
//! `ctx.recover_any(node, RecoveryReason::…)`. The closed enum keeps the
//! set of recovery sites auditable.

use crate::context::CheckerContext;
use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

/// Why the checker fell back to [`TypeId::ANY`] at a node.
///
/// Variants name a *family* of fallbacks, not a single test case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecoveryReason {
    /// `this` is referenced inside a class or object-literal member, but the
    /// enclosing class binder has not been resolved yet (e.g., evaluation
    /// reentry before binder finalization). TS2683 is suppressed because the
    /// receiver is contextually typed; we return `any` to avoid poisoning
    /// downstream property lookups.
    ThisUnresolvedClassOrObjectLiteralMember,
    /// A `new C(...)` or `extends C` lookup could not resolve the target's
    /// constructor signature. Returning `any` suppresses the cascading TS2571
    /// chain that would otherwise fire on every downstream member access.
    ClassConstructorTargetUnresolved,
    /// `yield` was used outside any generator function. The parser already
    /// emitted TS1163; we return `any` so the surrounding expression checker
    /// does not double-report.
    YieldOutsideGenerator,
    /// `yield <expr>` whose enclosing generator has neither an annotated
    /// next-type nor a contextually-known next-type. The yield expression's
    /// observable value is implicitly `any`; TS7057 is emitted separately
    /// when `noImplicitAny` is enabled.
    YieldExpressionNoGeneratorContext,
}

impl RecoveryReason {
    /// Stable `site` label used by the structured trace emitted from
    /// [`CheckerContext::recover_any`]. Centralizing the label here keeps
    /// trace filters (e.g. `TSZ_LOG=tsz_checker::recovery=trace`) working
    /// across migrations and prevents per-call-site label drift.
    pub const fn trace_site(self) -> &'static str {
        match self {
            Self::ThisUnresolvedClassOrObjectLiteralMember => {
                "dispatch::this_unresolved_class_or_object_literal_member"
            }
            Self::ClassConstructorTargetUnresolved => {
                "dispatch::class_constructor_target_unresolved"
            }
            Self::YieldOutsideGenerator => "dispatch_yield::yield_outside_generator",
            Self::YieldExpressionNoGeneratorContext => {
                "dispatch_yield::yield_result_no_generator_context"
            }
        }
    }

    /// Human-readable description used as the `tracing` message body.
    pub const fn description(self) -> &'static str {
        match self {
            Self::ThisUnresolvedClassOrObjectLiteralMember => {
                "TypeId::ANY recovery: unresolved enclosing this scope"
            }
            Self::ClassConstructorTargetUnresolved => {
                "TypeId::ANY recovery: cascading TS2571 suppression after unresolved class target"
            }
            Self::YieldOutsideGenerator => {
                "TypeId::ANY recovery: yield outside generator (TS1163 already from parser)"
            }
            Self::YieldExpressionNoGeneratorContext => {
                "TypeId::ANY recovery: yield expression with no generator next-type context"
            }
        }
    }
}

/// Per-checker registry of recovery fallback sites.
///
/// Records the *last* reason per node. A node produces at most one
/// ANY-yielding evaluation per check, and re-evaluation must agree on the
/// reason (otherwise the recovery sites have diverged, which is itself a
/// checker bug).
#[derive(Default, Debug)]
pub struct RecoverySites {
    sites: FxHashMap<NodeIndex, RecoveryReason>,
}

impl RecoverySites {
    pub(crate) fn record(&mut self, node: NodeIndex, reason: RecoveryReason) {
        self.sites.insert(node, reason);
    }

    pub(crate) fn get(&self, node: NodeIndex) -> Option<RecoveryReason> {
        self.sites.get(&node).copied()
    }

    pub(crate) fn len(&self) -> usize {
        self.sites.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.sites.is_empty()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (NodeIndex, RecoveryReason)> + '_ {
        self.sites.iter().map(|(node, reason)| (*node, *reason))
    }
}

impl<'a> CheckerContext<'a> {
    /// Record `node` as a recovery site with `reason` and return
    /// [`TypeId::ANY`]. Prefer this over inline
    /// `tracing::debug!(...); TypeId::ANY` blocks so the set of recovery
    /// reasons stays a closed typed set. The returned `TypeId` is
    /// intentionally `TypeId::ANY` to preserve existing any-propagation
    /// semantics.
    pub fn recover_any(&self, node: NodeIndex, reason: RecoveryReason) -> TypeId {
        tracing::debug!(
            site = reason.trace_site(),
            reason = ?reason,
            idx = node.0,
            "{}",
            reason.description()
        );
        self.recovery_sites.borrow_mut().record(node, reason);
        TypeId::ANY
    }

    /// Snapshot of every recorded `(node, reason)` pair. Used by audit
    /// tooling and tests; not on any hot path.
    pub fn recovery_sites_snapshot(&self) -> Vec<(NodeIndex, RecoveryReason)> {
        self.recovery_sites.borrow().iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(n: u32) -> NodeIndex {
        NodeIndex(n)
    }

    #[test]
    fn empty_registry_has_no_recovery_sites() {
        let sites = RecoverySites::default();
        assert!(sites.is_empty());
        assert_eq!(sites.len(), 0);
        assert!(sites.get(node(0)).is_none());
        assert!(sites.get(node(42)).is_none());
    }

    #[test]
    fn record_then_get_returns_reason() {
        let mut sites = RecoverySites::default();
        sites.record(node(17), RecoveryReason::YieldOutsideGenerator);
        assert_eq!(
            sites.get(node(17)),
            Some(RecoveryReason::YieldOutsideGenerator)
        );
        assert_eq!(sites.len(), 1);
        assert!(!sites.is_empty());
    }

    #[test]
    fn record_distinguishes_each_recovery_family() {
        let mut sites = RecoverySites::default();
        sites.record(
            node(1),
            RecoveryReason::ThisUnresolvedClassOrObjectLiteralMember,
        );
        sites.record(node(2), RecoveryReason::ClassConstructorTargetUnresolved);
        sites.record(node(3), RecoveryReason::YieldOutsideGenerator);
        sites.record(node(4), RecoveryReason::YieldExpressionNoGeneratorContext);

        assert_eq!(sites.len(), 4);
        assert_eq!(
            sites.get(node(1)),
            Some(RecoveryReason::ThisUnresolvedClassOrObjectLiteralMember)
        );
        assert_eq!(
            sites.get(node(2)),
            Some(RecoveryReason::ClassConstructorTargetUnresolved)
        );
        assert_eq!(
            sites.get(node(3)),
            Some(RecoveryReason::YieldOutsideGenerator)
        );
        assert_eq!(
            sites.get(node(4)),
            Some(RecoveryReason::YieldExpressionNoGeneratorContext)
        );
    }

    #[test]
    fn get_returns_none_for_unrecorded_node() {
        // Models "real declared `any`": a node that legitimately produced
        // TypeId::ANY through type evaluation rather than recovery is NOT
        // in the registry.
        let mut sites = RecoverySites::default();
        sites.record(
            node(7),
            RecoveryReason::ThisUnresolvedClassOrObjectLiteralMember,
        );
        assert!(sites.get(node(0)).is_none());
        assert!(sites.get(node(7)).is_some());
        assert!(sites.get(node(8)).is_none());
    }

    #[test]
    fn re_recording_same_node_with_same_reason_is_idempotent() {
        let mut sites = RecoverySites::default();
        sites.record(node(5), RecoveryReason::YieldOutsideGenerator);
        sites.record(node(5), RecoveryReason::YieldOutsideGenerator);
        assert_eq!(sites.len(), 1);
        assert_eq!(
            sites.get(node(5)),
            Some(RecoveryReason::YieldOutsideGenerator)
        );
    }

    #[test]
    fn trace_site_labels_are_distinct_per_family() {
        // Trace filters rely on these labels being unique per family;
        // guard against copy-paste collisions between sites.
        let labels = [
            RecoveryReason::ThisUnresolvedClassOrObjectLiteralMember.trace_site(),
            RecoveryReason::ClassConstructorTargetUnresolved.trace_site(),
            RecoveryReason::YieldOutsideGenerator.trace_site(),
            RecoveryReason::YieldExpressionNoGeneratorContext.trace_site(),
        ];
        for i in 0..labels.len() {
            for j in (i + 1)..labels.len() {
                assert_ne!(labels[i], labels[j], "duplicate trace_site label");
            }
        }
    }

    #[test]
    fn iter_yields_all_recorded_sites() {
        let mut sites = RecoverySites::default();
        sites.record(node(10), RecoveryReason::YieldOutsideGenerator);
        sites.record(node(11), RecoveryReason::ClassConstructorTargetUnresolved);
        let collected: Vec<_> = sites.iter().collect();
        assert_eq!(collected.len(), 2);
        assert!(collected.contains(&(node(10), RecoveryReason::YieldOutsideGenerator)));
        assert!(collected.contains(&(node(11), RecoveryReason::ClassConstructorTargetUnresolved)));
    }
}
