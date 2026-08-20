use super::{
    AliasBaseAssignmentCache, AliasPathAssignmentCache, FlowCache, ReferenceMatchCache,
    ReferenceSymbolCache, alias_base_assignment_cache_entries,
    alias_base_assignment_cache_estimated_size_bytes, alias_path_assignment_cache_entries,
    alias_path_assignment_cache_estimated_size_bytes, flow_cache_entries,
    flow_cache_estimated_size_bytes, numeric_atom_cache_entries,
    numeric_atom_cache_estimated_size_bytes, reference_match_cache_entries,
    reference_match_cache_estimated_size_bytes, reference_symbol_cache_entries,
    reference_symbol_cache_estimated_size_bytes, shared_numeric_atom_cache_entries,
    shared_numeric_atom_cache_estimated_size_bytes, switch_reference_cache_entries,
    switch_reference_cache_estimated_size_bytes,
};
use super::{FLOW_STEP_BUDGET_MAX, FLOW_STEP_BUDGET_MIN, FLOW_STEP_BUDGET_SCALE, flow_step_budget};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use tsz_binder::{FlowNodeId, SymbolId};
use tsz_common::interner::Atom;
use tsz_solver::TypeId;

#[test]
fn flow_step_budget_has_minimum_floor() {
    assert_eq!(flow_step_budget(0), FLOW_STEP_BUDGET_MIN);
    assert_eq!(flow_step_budget(1), FLOW_STEP_BUDGET_MIN);
}

#[test]
fn flow_step_budget_scales_with_graph_size() {
    let nodes = FLOW_STEP_BUDGET_MIN / FLOW_STEP_BUDGET_SCALE + 10;
    assert_eq!(flow_step_budget(nodes), nodes * FLOW_STEP_BUDGET_SCALE);
}

#[test]
fn flow_step_budget_has_upper_cap() {
    assert_eq!(flow_step_budget(usize::MAX), FLOW_STEP_BUDGET_MAX);
}

#[test]
fn flow_step_budget_caps_large_graphs() {
    let nodes = FLOW_STEP_BUDGET_MAX;
    assert_eq!(flow_step_budget(nodes), FLOW_STEP_BUDGET_MAX);
}

#[test]
fn flow_step_budget_caps_large_contention_graphs_earlier() {
    // Keep pathological full-suite flow walks bounded under worker contention.
    assert_eq!(flow_step_budget(8_000), FLOW_STEP_BUDGET_MAX);
}

#[test]
fn flow_cache_statistics_report_entries_and_size() {
    let mut cache = FlowCache::default();
    assert_eq!(flow_cache_entries(&cache), 0);
    assert_eq!(flow_cache_estimated_size_bytes(&cache), 0);

    cache.insert((FlowNodeId(1), SymbolId(2), TypeId(3)), TypeId(4));
    cache.insert((FlowNodeId(5), SymbolId(6), TypeId(7)), TypeId(8));

    assert_eq!(flow_cache_entries(&cache), 2);
    assert!(
        flow_cache_estimated_size_bytes(&cache)
            >= 2 * (std::mem::size_of::<(FlowNodeId, SymbolId, TypeId)>()
                + std::mem::size_of::<TypeId>())
    );
}

#[test]
fn reference_match_cache_statistics_report_entries_and_size() {
    let cache = ReferenceMatchCache::default();
    assert_eq!(reference_match_cache_entries(&cache), 0);
    assert_eq!(reference_match_cache_estimated_size_bytes(&cache), 0);

    cache.borrow_mut().insert((1, 2), true);
    cache.borrow_mut().insert((3, 4), false);

    assert_eq!(reference_match_cache_entries(&cache), 2);
    assert!(
        reference_match_cache_estimated_size_bytes(&cache)
            >= 2 * (std::mem::size_of::<(u32, u32)>() + std::mem::size_of::<bool>())
    );
}

#[test]
fn reference_symbol_cache_statistics_report_entries_and_size() {
    let cache = ReferenceSymbolCache::default();
    assert_eq!(reference_symbol_cache_entries(&cache), 0);
    assert_eq!(reference_symbol_cache_estimated_size_bytes(&cache), 0);

    cache.borrow_mut().insert(1, Some(SymbolId(2)));
    cache.borrow_mut().insert(3, None);

    assert_eq!(reference_symbol_cache_entries(&cache), 2);
    assert!(
        reference_symbol_cache_estimated_size_bytes(&cache)
            >= 2 * (std::mem::size_of::<u32>() + std::mem::size_of::<Option<SymbolId>>())
    );
}

#[test]
fn switch_reference_cache_statistics_report_entries_and_size() {
    let cache = ReferenceMatchCache::default();
    assert_eq!(switch_reference_cache_entries(&cache), 0);
    assert_eq!(switch_reference_cache_estimated_size_bytes(&cache), 0);

    cache.borrow_mut().insert((1, 2), true);
    cache.borrow_mut().insert((3, 4), false);

    assert_eq!(switch_reference_cache_entries(&cache), 2);
    assert!(
        switch_reference_cache_estimated_size_bytes(&cache)
            >= 2 * (std::mem::size_of::<(u32, u32)>() + std::mem::size_of::<bool>())
    );
}

#[test]
fn numeric_atom_cache_statistics_report_entries_and_size() {
    let cache = RefCell::new(FxHashMap::default());
    assert_eq!(numeric_atom_cache_entries(&cache), 0);
    assert_eq!(numeric_atom_cache_estimated_size_bytes(&cache), 0);

    cache.borrow_mut().insert(1, Atom(2));
    cache.borrow_mut().insert(3, Atom(4));

    assert_eq!(numeric_atom_cache_entries(&cache), 2);
    assert!(
        numeric_atom_cache_estimated_size_bytes(&cache)
            >= 2 * (std::mem::size_of::<u64>() + std::mem::size_of::<Atom>())
    );
}

#[test]
fn shared_numeric_atom_cache_statistics_report_borrowed_entries_and_zero_owned_size() {
    let cache = RefCell::new(FxHashMap::default());
    assert_eq!(shared_numeric_atom_cache_entries(Some(&cache)), 0);
    assert_eq!(
        shared_numeric_atom_cache_estimated_size_bytes(Some(&cache)),
        0
    );
    assert_eq!(shared_numeric_atom_cache_entries(None), 0);
    assert_eq!(shared_numeric_atom_cache_estimated_size_bytes(None), 0);

    cache.borrow_mut().insert(1, Atom(2));

    assert_eq!(shared_numeric_atom_cache_entries(Some(&cache)), 1);
    assert_eq!(
        shared_numeric_atom_cache_estimated_size_bytes(Some(&cache)),
        0
    );
}

#[test]
fn alias_assignment_cache_statistics_report_entries_and_size() {
    let base_cache = AliasBaseAssignmentCache::default();
    assert_eq!(alias_base_assignment_cache_entries(&base_cache), 0);
    assert_eq!(
        alias_base_assignment_cache_estimated_size_bytes(&base_cache),
        0
    );
    base_cache.borrow_mut().insert((1, 2), true);
    assert_eq!(alias_base_assignment_cache_entries(&base_cache), 1);
    assert!(alias_base_assignment_cache_estimated_size_bytes(&base_cache) > 0);

    let path_cache = AliasPathAssignmentCache::default();
    assert_eq!(alias_path_assignment_cache_entries(&path_cache), 0);
    assert_eq!(
        alias_path_assignment_cache_estimated_size_bytes(&path_cache),
        0
    );
    path_cache.borrow_mut().insert((1, 2, 3), false);
    assert_eq!(alias_path_assignment_cache_entries(&path_cache), 1);
    assert!(alias_path_assignment_cache_estimated_size_bytes(&path_cache) > 0);
}
