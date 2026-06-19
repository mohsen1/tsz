use rustc_hash::FxHashMap;
use std::cell::RefCell;
use tsz_common::interner::Atom;
use tsz_solver::TypeId;
use tsz_solver::def::DefId;

/// File-session caches for lib type and lazy lib-member resolution.
#[derive(Default)]
pub struct LibTypeResolutionCaches {
    /// Cache for `resolve_lib_type_by_name` results.
    /// Keyed by type name and stores both hits (`Some(TypeId)`) and misses (`None`).
    pub types: FxHashMap<String, Option<TypeId>>,

    /// Per-checker cache for lazy single-member lib-interface property reads.
    /// Keyed by `(interface_name, property_name)` atoms and stores both hits and
    /// conservative misses after the existing lazy-member resolver has decided
    /// whether it can lower only the requested member. This keeps repeated DOM
    /// reads from rescanning declaration and heritage lists while preserving the
    /// same full-materialization fallback on cached misses.
    pub lazy_members: RefCell<FxHashMap<(Atom, Atom), Option<TypeId>>>,

    /// Per-checker cache for lazy single-member property reads once a receiver
    /// has already been classified as an eligible bare `Lazy(DefId)`.
    /// Keyed by `(receiver_def_id, property_name)` and stores the same hit/miss
    /// result as `lazy_members`, but lets the receiver hot path avoid mapping
    /// the cached `DefId` back to a symbol name before a repeated lookup.
    pub lazy_member_receiver_properties: RefCell<FxHashMap<(DefId, Atom), Option<TypeId>>>,

    /// Per-checker cache for lazy single-member receiver eligibility.
    /// Keyed by the bare receiver `Lazy(DefId)`. Stores both eligible and
    /// ineligible decisions after the conservative receiver predicate has
    /// inspected symbol provenance, generic parameters, shadowing, global
    /// augmentations, and heritage bases. This keeps repeated DOM/lib property
    /// reads from re-walking the same heritage/augmentation graph before they
    /// can hit the member cache.
    pub lazy_member_receivers: RefCell<FxHashMap<DefId, bool>>,
}
