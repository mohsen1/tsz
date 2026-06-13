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
    outcome: ComputeTypeOfSymbolInterfaceFastPathOutcome,
) {
    if !enabled_fast() {
        return;
    }
    counters().compute_type_of_symbol_interface_fastpath_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

/// Record call-site parent-kind attribution for interface calls in
/// `compute_type_of_symbol`.
#[inline]
pub fn record_compute_type_of_symbol_interface_callsite_outcome(
    outcome: ComputeTypeOfSymbolInterfaceCallsiteOutcome,
) {
    if !enabled_fast() {
        return;
    }
    counters().compute_type_of_symbol_interface_callsite_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

/// Record success/reject outcomes for the simple local-interface object
/// shortcut in `compute_type_of_symbol`.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_outcome(
    outcome: ComputeTypeOfSymbolInterfaceSimpleObjectOutcome,
) {
    if !enabled_fast() {
        return;
    }
    counters().compute_type_of_symbol_interface_simple_object_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

/// Record annotation-kind attribution for
/// `RejectNonPrimitiveAnnotation` outcomes in the simple local-interface
/// object shortcut.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kind(
    kind: ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationKind,
) {
    if !enabled_fast() {
        return;
    }
    counters().compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kind
        [kind.as_index()]
    .fetch_add(1, Ordering::Relaxed);
}

/// Record bounded source-level residue for non-primitive annotations rejected
/// by the simple local-interface object shortcut.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residue(
    kind: ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationKind,
    interface: Option<&str>,
    property: Option<&str>,
) {
    if !enabled_fast() {
        return;
    }

    let kind_name =
        COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_KIND_NAMES
            [kind.as_index()];
    let mut rows =
        compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(row) = rows.iter_mut().find(|row| {
        row.kind == kind_name
            && row.interface.as_deref() == interface
            && row.property.as_deref() == property
    }) {
        row.count += 1;
        return;
    }

    if rows.len()
        < COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_RESIDUE_LIMIT
    {
        rows.push(
            ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationResidue {
                kind: kind_name,
                interface: interface.map(str::to_owned),
                property: property.map(str::to_owned),
                count: 1,
            },
        );
    } else if let Some(row) = rows
        .iter_mut()
        .find(|row| row.interface.as_deref() == Some("__truncated__"))
    {
        row.count += 1;
    } else {
        rows.push(
            ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationResidue {
                kind: "overflow",
                interface: Some("__truncated__".to_string()),
                property: None,
                count: 1,
            },
        );
    }
}

/// Record bounded symbol-level residue for declaration/provenance guards
/// rejected by the simple local-interface object shortcut.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_declaration_provenance_residue(
    outcome: ComputeTypeOfSymbolInterfaceSimpleObjectOutcome,
    symbol: Option<&str>,
    declaration_count: usize,
) {
    if !enabled_fast() {
        return;
    }

    let outcome_name =
        COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_OUTCOME_NAMES[outcome.as_index()];
    let declaration_count = declaration_count as u64;
    let mut rows = compute_type_of_symbol_interface_simple_object_declaration_provenance_residues()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(row) = rows.iter_mut().find(|row| {
        row.outcome == outcome_name
            && row.symbol.as_deref() == symbol
            && row.declaration_count == declaration_count
    }) {
        row.count += 1;
        return;
    }

    if rows.len()
        < COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_DECLARATION_PROVENANCE_RESIDUE_LIMIT
    {
        rows.push(
            ComputeTypeOfSymbolInterfaceSimpleObjectDeclarationProvenanceResidue {
                outcome: outcome_name,
                symbol: symbol.map(str::to_owned),
                declaration_count,
                count: 1,
            },
        );
    } else if let Some(row) = rows
        .iter_mut()
        .find(|row| row.symbol.as_deref() == Some("__truncated__"))
    {
        row.count += 1;
    } else {
        rows.push(
            ComputeTypeOfSymbolInterfaceSimpleObjectDeclarationProvenanceResidue {
                outcome: "overflow",
                symbol: Some("__truncated__".to_string()),
                declaration_count: 0,
                count: 1,
            },
        );
    }
}

/// Record attribution for why a `type_reference` annotation was still rejected
/// by the simple local-interface object shortcut.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_type_reference_reject_outcome(
    outcome: ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectOutcome,
) {
    if !enabled_fast() {
        return;
    }
    counters().compute_type_of_symbol_interface_simple_object_type_reference_reject_outcome
        [outcome.as_index()]
    .fetch_add(1, Ordering::Relaxed);
}

/// Record bounded name-level residue for type-reference annotations rejected by
/// the simple local-interface object shortcut.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_type_reference_reject_residue(
    outcome: ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectOutcome,
    name: &str,
) {
    if !enabled_fast() {
        return;
    }

    let outcome_name =
        COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_OUTCOME_NAMES
            [outcome.as_index()];
    let mut rows = compute_type_of_symbol_interface_simple_object_type_reference_reject_residues()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(row) = rows
        .iter_mut()
        .find(|row| row.name == name && row.outcome == outcome_name)
    {
        row.count += 1;
        return;
    }

    if rows.len()
        < COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_RESIDUE_LIMIT
    {
        rows.push(
            ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectResidue {
                name: name.to_owned(),
                outcome: outcome_name,
                count: 1,
            },
        );
    } else if let Some(row) = rows.iter_mut().find(|row| row.name == "__truncated__") {
        row.count += 1;
    } else {
        rows.push(
            ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectResidue {
                name: "__truncated__".to_string(),
                outcome: "overflow",
                count: 1,
            },
        );
    }
}

/// Record why the actual-lib lazy-ref lowering helper accepted or rejected a
/// property-signature type reference inside the simple local-interface shortcut.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_actual_lib_type_reference_outcome(
    outcome: ComputeTypeOfSymbolInterfaceSimpleObjectActualLibTypeReferenceOutcome,
) {
    if !enabled_fast() {
        return;
    }
    counters()
        .compute_type_of_symbol_interface_simple_object_actual_lib_type_reference_outcome
        [outcome.as_index()]
    .fetch_add(1, Ordering::Relaxed);
}

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
