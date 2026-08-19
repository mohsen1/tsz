//! Per-definition flag sets shared across checkers.
//!
//! Groups the poison / circular / publish-isolation / alias-body flag sets that
//! were previously fused as six loose `DashSet` fields on [`DefinitionStore`].
//! Each set's invalidation ownership now lives behind named queries here, so
//! body publication and `clear` no longer reach into raw sets and the
//! relationships between the sets (e.g. "computed unless directly named",
//! "publish-once freeze wins over deferred-publish") are stated in one place.
//!
//! [`DefinitionStore`]: super::DefinitionStore

use super::{DefDashMap, DefDashSet, DefId};
use crate::types::TypeId;

/// Cross-checker flag sets keyed by definition or alias-body identity.
///
/// All sets are first-wins / monotonic within a store generation; none is
/// cleared by [`DefinitionStore::clear`] except the two alias-body sets (see
/// [`DefStateFlags::clear_alias_bodies`]), matching the historical behavior.
///
/// [`DefinitionStore`]: super::DefinitionStore
/// [`DefinitionStore::clear`]: super::DefinitionStore::clear
#[derive(Debug, Default)]
pub(crate) struct DefStateFlags {
    /// Body `TypeId`s produced by type-level computation (intersection
    /// reduction, conditional evaluation) that must not drive alias-name
    /// display.
    computed_alias_bodies: DefDashSet<TypeId>,

    /// Body `TypeId`s that are the constructive body of at least one
    /// *non-computed* alias, so they keep their alias name even when a computed
    /// alias resolves to the same interned shape ("direct wins").
    directly_named_alias_bodies: DefDashSet<TypeId>,

    /// Type-alias `DefId`s whose instantiation is unconditionally infinite
    /// (TS2589); every later application resolves to the error type.
    depth_poisoned_defs: DefDashSet<DefId>,

    /// Defs whose shared-store body is frozen after finalized materialization:
    /// later different-body publications are dropped.
    publish_once_defs: DefDashSet<DefId>,

    /// Defs whose pre-finalize different-body overwrites are deferred (dropped)
    /// until the finalize entry point replaces the first published form.
    deferred_publish_defs: DefDashSet<DefId>,

    /// Defs detected as participating in a circular type alias cycle.
    circular_def_ids: DefDashSet<DefId>,

    /// Non-generic type-alias `DefId`s whose declared body is a tuple built by
    /// flattening a fixed-tuple spread (`type T = [...[a, b], c]`). `tsc`
    /// attaches no `aliasSymbol` to the freshly-spread tuple, so diagnostics
    /// render the structural form (`[a, b, c]`) rather than the alias name.
    /// Keyed per def (not per body `TypeId`) because the flattened tuple
    /// interns to the same shape as a directly-written `type T = [a, b, c]`,
    /// which `tsc` *does* display by name.
    tuple_spread_flattened_alias_defs: DefDashSet<DefId>,

    /// Non-generic type-alias `DefId`s whose declared body is a *bare*
    /// (argument-free) type reference resolving to a non-generic interface or
    /// class declaration, mapped to that declaration's `DefId`. `tsc` attaches
    /// no `aliasSymbol` to the declaration's shared nominal type, so
    /// diagnostics render the declaration's own name (`type IA = Iface`
    /// renders `Iface`). Keyed per alias def because the resolved body may
    /// flatten to the declaration's structural shape, which no longer records
    /// which reference produced it.
    bare_nominal_ref_alias_defs: DefDashMap<DefId, DefId>,
}

impl DefStateFlags {
    /// Flag a def as depth-poisoned. Returns `true` if this was a new entry
    /// (the caller bumps the store generation only then).
    pub(crate) fn mark_depth_poisoned(&self, id: DefId) -> bool {
        self.depth_poisoned_defs.insert(id)
    }

    /// Whether the given def was flagged via [`Self::mark_depth_poisoned`].
    #[inline]
    pub(crate) fn is_depth_poisoned(&self, id: DefId) -> bool {
        self.depth_poisoned_defs.contains(&id)
    }

    /// Whether any def is depth-poisoned (cheap gate for hot paths).
    #[inline]
    pub(crate) fn has_any_depth_poisoned(&self) -> bool {
        !self.depth_poisoned_defs.is_empty()
    }

    /// Freeze a def's body against later different-body publications.
    pub(crate) fn mark_publish_once(&self, id: DefId) {
        self.publish_once_defs.insert(id);
    }

    /// Whether `id`'s body is frozen by a publish-once marker.
    #[inline]
    pub(crate) fn is_publish_once_frozen(&self, id: DefId) -> bool {
        !self.publish_once_defs.is_empty() && self.publish_once_defs.contains(&id)
    }

    /// Mark a def for deferred publication (pre-finalize overwrites dropped).
    pub(crate) fn mark_deferred_publish(&self, id: DefId) {
        self.deferred_publish_defs.insert(id);
    }

    /// Whether `id` is marked for deferred publication.
    #[inline]
    pub(crate) fn is_deferred_publish(&self, id: DefId) -> bool {
        !self.deferred_publish_defs.is_empty() && self.deferred_publish_defs.contains(&id)
    }

    /// Record `id` as participating in a circular type alias cycle.
    pub(crate) fn mark_circular(&self, id: DefId) {
        self.circular_def_ids.insert(id);
    }

    /// Whether `id` was marked circular by any checker.
    #[inline]
    pub(crate) fn is_circular(&self, id: DefId) -> bool {
        self.circular_def_ids.contains(&id)
    }

    /// Mark an alias body as computed (skipped for alias-name display).
    pub(crate) fn mark_body_computed(&self, body: TypeId) {
        self.computed_alias_bodies.insert(body);
    }

    /// Record `body` as the body of a directly-written alias ("direct wins").
    pub(crate) fn mark_body_directly_named(&self, body: TypeId) {
        self.directly_named_alias_bodies.insert(body);
    }

    /// Whether `body` should be displayed structurally because it was produced
    /// by a reducing operator and is not also a directly-written alias body.
    #[inline]
    pub(crate) fn is_computed_body(&self, body: TypeId) -> bool {
        self.computed_alias_bodies.contains(&body)
            && !self.directly_named_alias_bodies.contains(&body)
    }

    /// Flag a non-generic type alias whose tuple body was spread-flattened, so
    /// diagnostics render the structural tuple instead of the alias name.
    pub(crate) fn mark_tuple_spread_flattened_alias(&self, id: DefId) {
        self.tuple_spread_flattened_alias_defs.insert(id);
    }

    /// Whether `id` was flagged via [`Self::mark_tuple_spread_flattened_alias`].
    #[inline]
    pub(crate) fn is_tuple_spread_flattened_alias(&self, id: DefId) -> bool {
        !self.tuple_spread_flattened_alias_defs.is_empty()
            && self.tuple_spread_flattened_alias_defs.contains(&id)
    }

    /// Record a non-generic alias whose declared body is a bare reference to
    /// the non-generic interface/class declaration `target`.
    pub(crate) fn record_bare_nominal_ref_alias(&self, alias: DefId, target: DefId) {
        self.bare_nominal_ref_alias_defs.insert(alias, target);
    }

    /// The interface/class declaration recorded for `alias` via
    /// [`Self::record_bare_nominal_ref_alias`], if any.
    #[inline]
    pub(crate) fn bare_nominal_ref_alias_target(&self, alias: DefId) -> Option<DefId> {
        if self.bare_nominal_ref_alias_defs.is_empty() {
            return None;
        }
        self.bare_nominal_ref_alias_defs.get(&alias).map(|r| *r)
    }

    /// Reset the alias-body flag sets. The poison / publish / circular sets are
    /// intentionally retained, matching the historical [`DefinitionStore::clear`].
    ///
    /// [`DefinitionStore::clear`]: super::DefinitionStore::clear
    pub(crate) fn clear_alias_bodies(&self) {
        self.computed_alias_bodies.clear();
        self.directly_named_alias_bodies.clear();
        self.tuple_spread_flattened_alias_defs.clear();
        self.bare_nominal_ref_alias_defs.clear();
    }
}
