/// Record an entry to `get_type_of_symbol`'s computation path. Sits on a
/// multi-million-call hot path (`state/type_analysis/computed/mod.rs`),
/// so gating once before the `counters()` `OnceLock` deref is the load-
/// bearing optimization — disabled builds pay one branch and one
/// relaxed atomic load on `ENABLED_FAST`, not a deref.
#[inline]
pub fn record_compute_type_of_symbol_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .compute_type_of_symbol_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a cache hit on `get_type_of_symbol`'s `symbol_types` lookup.
/// Used at two sites in `state/type_analysis/core.rs` (the provisional-
/// type and the standard cached-type branches of the recursion guard).
/// Compared against [`record_compute_type_of_symbol_call`] in attribution
/// mode to characterize recomputation pressure.
#[inline]
pub fn record_compute_type_of_symbol_cache_hit() {
    if !enabled_fast() {
        return;
    }
    counters()
        .compute_type_of_symbol_cache_hits
        .fetch_add(1, Ordering::Relaxed);
}

/// Record use of the simple local-interface object shortcut inside
/// `compute_type_of_symbol`.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_fastpath_hit() {
    if !enabled_fast() {
        return;
    }
    counters()
        .compute_type_of_symbol_interface_simple_object_fastpath_hits
        .fetch_add(1, Ordering::Relaxed);
}

/// Record how `compute_type_of_symbol` sourced the symbol payload.
#[inline]
pub fn record_compute_type_of_symbol_source_outcome(outcome: ComputeTypeOfSymbolSourceOutcome) {
    if !enabled_fast() {
        return;
    }
    counters().compute_type_of_symbol_source_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

/// Record the coarse symbol-kind bucket lowered by `compute_type_of_symbol`.
#[inline]
pub fn record_compute_type_of_symbol_kind_outcome(outcome: ComputeTypeOfSymbolKindOutcome) {
    if !enabled_fast() {
        return;
    }
    counters().compute_type_of_symbol_kind_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

/// Record which interface fast-path combination ran inside
/// `compute_type_of_symbol`.
#[inline]
pub fn record_compute_type_of_symbol_interface_fastpath_outcome(

/// Record call-site parent-kind attribution for interface calls in
/// `compute_type_of_symbol`.
#[inline]
pub fn record_compute_type_of_symbol_interface_callsite_outcome(

/// Record success/reject outcomes for the simple local-interface object
/// shortcut in `compute_type_of_symbol`.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_outcome(

/// Record annotation-kind attribution for
/// `RejectNonPrimitiveAnnotation` outcomes in the simple local-interface
/// object shortcut.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kind(

/// Record bounded source-level residue for non-primitive annotations rejected
/// by the simple local-interface object shortcut.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residue(

/// Record bounded symbol-level residue for declaration/provenance guards
/// rejected by the simple local-interface object shortcut.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_declaration_provenance_residue(

/// Record attribution for why a `type_reference` annotation was still rejected
/// by the simple local-interface object shortcut.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_type_reference_reject_outcome(

/// Record bounded name-level residue for type-reference annotations rejected by
/// the simple local-interface object shortcut.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_type_reference_reject_residue(

/// Record why the actual-lib lazy-ref lowering helper accepted or rejected a
/// property-signature type reference inside the simple local-interface shortcut.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_actual_lib_type_reference_outcome(

pub fn record_property_classification_call() {
    inc(&counters().property_classification_calls);
}

pub fn record_property_classification_string_fallback_source_lookup() {
    inc(&counters().property_classification_string_fallback_source_lookups);
}

pub fn record_property_classification_string_fallback_target_name() {
    inc(&counters().property_classification_string_fallback_target_names);
}

pub fn record_property_classification_string_fallback_target_type() {
    inc(&counters().property_classification_string_fallback_target_types);
}
