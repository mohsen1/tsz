//! Shared input bundle for partial static-constructor type building.
//!
//! Extracted verbatim from `constructor.rs` to keep that module under its
//! architecture size ratchet; behavior is unchanged. `StaticMemberBuildData`
//! is consumed by `build_partial_static_constructor_type` (in `helpers.rs`)
//! and constructed at the partial-constructor build sites in `constructor.rs`.

use super::member_aggregates::{AccessorAggregate, MethodAggregate};
use rustc_hash::FxHashMap;
use tsz_common::interner::Atom;
use tsz_solver::{CallSignature, IndexSignature, PropertyInfo};

pub(super) struct StaticMemberBuildData<'a> {
    pub(super) current_sym: Option<tsz_binder::SymbolId>,
    pub(super) properties: &'a FxHashMap<Atom, PropertyInfo>,
    pub(super) methods: &'a FxHashMap<Atom, MethodAggregate>,
    pub(super) accessors: &'a FxHashMap<Atom, AccessorAggregate>,
    pub(super) static_string_index: &'a Option<IndexSignature>,
    pub(super) static_number_index: &'a Option<IndexSignature>,
    /// Property being injected mid-pass, before it has a cached type entry.
    pub(super) extra_property: Option<PropertyInfo>,
    pub(super) inherited_static_props: &'a [PropertyInfo],
    pub(super) all_static_member_names: &'a [Atom],
    pub(super) construct_signatures: &'a [CallSignature],
}
