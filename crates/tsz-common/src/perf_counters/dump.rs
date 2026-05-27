impl PerfCounters {
    /// Format the current counter snapshot as a multi-line report. Returns
    /// an empty string when the counters are disabled (so callers can
    /// unconditionally `print!("{}", PerfCounters::dump_string())` without
    /// noisy output in the common case).
    ///
    /// Counters that are NOT yet wired into their producer code (e.g. the
    /// per-kind `interner_*_intern_calls` buckets — the bucket fields are
    /// declared but the actual `tsz-solver` intern sites still need to be
    /// updated) are printed as `n/a` rather than `0`, so a reader doesn't
    /// mistake "not measured" for "didn't happen". A small `wired: false`
    /// table at the bottom of the dump lists which buckets are pending.
    pub fn dump_string() -> String {
        if !enabled_fast() {
            return String::new();
        }
        // Per `PERFORMANCE_PLAN.md` §3: "Text dumping and JSON dumping
        // should format the same snapshot so they cannot drift." Take
        // one snapshot here and format from the resulting value object
        // — same atomic-read pass `write_json_to` uses for the JSON
        // surface. A new counter added to `PerfCounterSnapshot` automatically
        // becomes available to both surfaces; adding a counter only to the
        // dump (or only to the JSON) is no longer possible.
        let snap = Self::snapshot();
        format!(
            "\n=== TSZ_PERF_COUNTERS ===\n\
             Delegation (cross-arena symbol resolution):\n  \
             calls                      {:>12}\n  \
             cache hits (lib)           {:>12}\n  \
             cache hits (cross-file)    {:>12}\n  \
             misses (full work)         {:>12}\n  \
             max recursion depth        {:>12}\n\
             Checker construction:\n  \
             CheckerState::new          {:>12}\n  \
             ::with_parent_cache        {:>12}\n  \
             ::reset_for_next_file      {:>12}\n  \
             copy_symbol_file_targets   {:>12}\n  \
             overlay entries copied     {:>12}\n  \
             overlay entries (max)      {:>12}\n  \
             overlay len ≥ 1k           {:>12}\n  \
             overlay len ≥ 10k          {:>12}\n  \
             overlay len ≥ 100k         {:>12}\n  \
             overlay len ≥ 1M           {:>12}\n\
             compute_type_of_symbol:\n  \
             total calls                {:>12}\n  \
             cache hits                 {:>12}\n  \
             simple-object hits         {:>12}\n\
             property classification:\n  \
             calls                      {:>12}\n  \
             string source lookups      {:>12}\n  \
             string target names        {:>12}\n  \
             string target type entries {:>12}\n\
             TypeInterner:\n  \
             intern calls (total)       {:>12}\n  \
             intern hits                {:>12}\n  \
             intern misses              {:>12}\n  \
             string intern calls        {:>12}\n  \
             type-list intern calls     {:>12}\n  \
             object-shape intern calls  {:>12}\n  \
             function-shape intern calls{:>12}\n  \
             callable-shape intern calls{:>12}\n  \
             application intern calls   {:>12}\n  \
             conditional intern calls   {:>12}\n  \
             mapped intern calls        {:>12}\n\
             Resolver:\n  \
             lookup calls               {:>12}\n  \
             is_file calls              {:>12}\n  \
             is_dir calls               {:>12}\n  \
             read_dir calls             {:>12}\n  \
             read_package_json calls    {:>12}\n  \
             candidate paths total      {:>12}\n\
             Stable identity:\n  \
             raw SymbolRef lazy fallback{:>12}\n",
            snap.delegate.calls,
            snap.delegate.cache_hits_lib,
            snap.delegate.cache_hits_cross_file,
            snap.delegate.misses,
            snap.delegate.max_recursion_depth,
            snap.checker.state_constructed,
            snap.checker.with_parent_cache_constructed,
            snap.checker.file_session_resets,
            snap.overlay.copy_calls,
            snap.overlay.entries_total,
            snap.overlay.entries_max,
            snap.overlay.len_ge_1k,
            snap.overlay.len_ge_10k,
            snap.overlay.len_ge_100k,
            snap.overlay.len_ge_1m,
            snap.checker.compute_type_of_symbol_calls,
            snap.checker.compute_type_of_symbol_cache_hits,
            snap.checker
                .compute_type_of_symbol_interface_simple_object_fastpath_hits,
            snap.checker.property_classification_calls,
            snap.checker
                .property_classification_string_fallback_source_lookups,
            snap.checker
                .property_classification_string_fallback_target_names,
            snap.checker
                .property_classification_string_fallback_target_types,
            snap.interner.intern_calls.unwrap_or(0),
            snap.interner.intern_hits.unwrap_or(0),
            snap.interner.intern_misses.unwrap_or(0),
            snap.interner.string_intern_calls,
            snap.interner.type_list_intern_calls,
            snap.interner.object_shape_intern_calls,
            snap.interner.function_shape_intern_calls,
            snap.interner.callable_shape_intern_calls,
            snap.interner.application_intern_calls,
            snap.interner.conditional_intern_calls,
            snap.interner.mapped_intern_calls,
            snap.resolver.lookup_calls,
            snap.resolver.is_file_calls.unwrap_or(0),
            snap.resolver.is_dir_calls.unwrap_or(0),
            snap.resolver.read_dir_calls.unwrap_or(0),
            snap.resolver.package_json_reads,
            snap.resolver.candidate_paths_total,
            snap.identity.type_environment_raw_symbol_lazy_fallbacks,
        ) + &Self::dump_compute_type_of_symbol_outcomes()
            + &Self::dump_compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues(
                &snap.compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues,
            )
            + &Self::dump_compute_type_of_symbol_interface_simple_object_declaration_provenance_residues(
                &snap.compute_type_of_symbol_interface_simple_object_declaration_provenance_residues,
            )
            + &Self::dump_compute_type_of_symbol_interface_simple_object_type_reference_reject_residues(
                &snap.compute_type_of_symbol_interface_simple_object_type_reference_reject_residues,
            )
            + &Self::dump_cross_arena_symbol_miss_classification()
            + &Self::dump_cross_arena_alias_shortcut_outcomes()
            + &Self::dump_direct_cross_file_interface_lowering_outcomes()
            + &Self::dump_direct_actual_lib_alias_body_outcomes()
            + &Self::dump_direct_source_file_type_alias_lowering_outcomes()
            + &Self::dump_direct_source_file_type_alias_body_rejection_kinds()
            + &Self::dump_direct_source_file_type_alias_type_reference_rejection_kinds()
            + &Self::dump_direct_source_file_type_alias_first_type_reference_rejection_kinds()
            + &Self::dump_direct_source_file_type_alias_body_rejection_residues(
                &snap.direct_source_file_type_alias_body_rejection_residues,
            )
            + &Self::dump_direct_actual_lib_intl_interface_outcomes()
            + &Self::dump_delegate_declaration_file_miss_residues(
                &snap.delegate_declaration_file_miss_residues,
            )
            + &Self::dump_delegate_source_file_miss_residues(
                &snap.delegate_source_file_miss_residues,
            )
            + &Self::dump_source_file_symbol_arena_cache_eligibility_outcomes()
            + &Self::dump_by_reason()
    }

    fn dump_compute_type_of_symbol_outcomes() -> String {
        let c = counters();
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
        let source_total: u64 = c
            .compute_type_of_symbol_source_outcome
            .iter()
            .map(load)
            .sum();
        let kind_total: u64 = c.compute_type_of_symbol_kind_outcome.iter().map(load).sum();
        let interface_fastpath_total: u64 = c
            .compute_type_of_symbol_interface_fastpath_outcome
            .iter()
            .map(load)
            .sum();
        let interface_callsite_total: u64 = c
            .compute_type_of_symbol_interface_callsite_outcome
            .iter()
            .map(load)
            .sum();
        let interface_simple_object_total: u64 = c
            .compute_type_of_symbol_interface_simple_object_outcome
            .iter()
            .map(load)
            .sum();
        let interface_simple_object_non_primitive_annotation_kind_total: u64 = c
            .compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kind
            .iter()
            .map(load)
            .sum();
        let interface_simple_object_type_reference_reject_outcome_total: u64 = c
            .compute_type_of_symbol_interface_simple_object_type_reference_reject_outcome
            .iter()
            .map(load)
            .sum();
        if source_total == 0
            && kind_total == 0
            && interface_fastpath_total == 0
            && interface_callsite_total == 0
            && interface_simple_object_total == 0
            && interface_simple_object_non_primitive_annotation_kind_total == 0
            && interface_simple_object_type_reference_reject_outcome_total == 0
        {
            return String::new();
        }

        let mut out = String::new();
        if source_total > 0 {
            out.push_str("\ncompute_type_of_symbol source outcomes:\n");
            for (idx, name) in COMPUTE_TYPE_OF_SYMBOL_SOURCE_OUTCOME_NAMES
                .iter()
                .enumerate()
            {
                let count = load(&c.compute_type_of_symbol_source_outcome[idx]);
                if count > 0 {
                    out.push_str(&format!("  {name:<28} {count:>12}\n"));
                }
            }
        }
        if kind_total > 0 {
            out.push_str("\ncompute_type_of_symbol kind outcomes:\n");
            for (idx, name) in COMPUTE_TYPE_OF_SYMBOL_KIND_OUTCOME_NAMES.iter().enumerate() {
                let count = load(&c.compute_type_of_symbol_kind_outcome[idx]);
                if count > 0 {
                    out.push_str(&format!("  {name:<28} {count:>12}\n"));
                }
            }
        }
        if interface_fastpath_total > 0 {
            out.push_str("\ncompute_type_of_symbol interface fastpath outcomes:\n");
            for (idx, name) in COMPUTE_TYPE_OF_SYMBOL_INTERFACE_FASTPATH_OUTCOME_NAMES
                .iter()
                .enumerate()
            {
                let count = load(&c.compute_type_of_symbol_interface_fastpath_outcome[idx]);
                if count > 0 {
                    out.push_str(&format!("  {name:<28} {count:>12}\n"));
                }
            }
        }
        if interface_callsite_total > 0 {
            out.push_str("\ncompute_type_of_symbol interface callsite outcomes:\n");
            for (idx, name) in COMPUTE_TYPE_OF_SYMBOL_INTERFACE_CALLSITE_OUTCOME_NAMES
                .iter()
                .enumerate()
            {
                let count = load(&c.compute_type_of_symbol_interface_callsite_outcome[idx]);
                if count > 0 {
                    out.push_str(&format!("  {name:<28} {count:>12}\n"));
                }
            }
        }
        if interface_simple_object_total > 0 {
            out.push_str("\ncompute_type_of_symbol interface simple-object outcomes:\n");
            for (idx, name) in COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_OUTCOME_NAMES
                .iter()
                .enumerate()
            {
                let count = load(&c.compute_type_of_symbol_interface_simple_object_outcome[idx]);
                if count > 0 {
                    out.push_str(&format!("  {name:<28} {count:>12}\n"));
                }
            }
        }
        if interface_simple_object_non_primitive_annotation_kind_total > 0 {
            out.push_str(
                "\ncompute_type_of_symbol interface simple-object non-primitive annotation kinds:\n",
            );
            for (idx, name) in
                COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_KIND_NAMES
                    .iter()
                    .enumerate()
            {
                let count = load(
                    &c.compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kind
                        [idx],
                );
                if count > 0 {
                    out.push_str(&format!("  {name:<28} {count:>12}\n"));
                }
            }
        }
        if interface_simple_object_type_reference_reject_outcome_total > 0 {
            out.push_str(
                "\ncompute_type_of_symbol interface simple-object type-reference reject outcomes:\n",
            );
            for (idx, name) in
                COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_OUTCOME_NAMES
                    .iter()
                    .enumerate()
            {
                let count = load(
                    &c.compute_type_of_symbol_interface_simple_object_type_reference_reject_outcome
                        [idx],
                );
                if count > 0 {
                    out.push_str(&format!("  {name:<28} {count:>12}\n"));
                }
            }
        }
        out
    }

    fn dump_compute_type_of_symbol_interface_simple_object_type_reference_reject_residues(
        rows: &[ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectResidue],
    ) -> String {
        if rows.is_empty() {
            return String::new();
        }

        let mut out = String::from(
            "\ncompute_type_of_symbol interface simple-object type-reference reject residues:\n",
        );
        for row in rows {
            out.push_str(&format!(
                "  {:<32} {:<36} {:>8}\n",
                row.name, row.outcome, row.count,
            ));
        }
        out
    }

    fn dump_compute_type_of_symbol_interface_simple_object_declaration_provenance_residues(
        rows: &[ComputeTypeOfSymbolInterfaceSimpleObjectDeclarationProvenanceResidue],
    ) -> String {
        if rows.is_empty() {
            return String::new();
        }

        let mut out = String::from(
            "\ncompute_type_of_symbol interface simple-object declaration provenance residues:\n",
        );
        for row in rows {
            out.push_str(&format!(
                "  {:<36} {:<32} {:>8} {:>8}\n",
                row.outcome,
                row.symbol.as_deref().unwrap_or("<unknown>"),
                row.declaration_count,
                row.count,
            ));
        }
        out
    }

    fn dump_compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues(
        rows: &[ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationResidue],
    ) -> String {
        if rows.is_empty() {
            return String::new();
        }

        let mut out = String::from(
            "\ncompute_type_of_symbol interface simple-object non-primitive annotation residues:\n",
        );
        for row in rows {
            out.push_str(&format!(
                "  {:<28} {:<32} {:<32} {:>8}\n",
                row.kind,
                row.interface.as_deref().unwrap_or("<unknown>"),
                row.property.as_deref().unwrap_or("<unknown>"),
                row.count,
            ));
        }
        out
    }

    fn dump_cross_arena_symbol_miss_classification() -> String {
        let c = counters();
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
        let source_total: u64 = c
            .delegate_cross_arena_symbol_miss_by_source
            .iter()
            .map(load)
            .sum();
        let kind_total: u64 = c
            .delegate_cross_arena_symbol_miss_by_kind
            .iter()
            .map(load)
            .sum();
        if source_total == 0 && kind_total == 0 {
            return String::new();
        }

        let mut out = String::from("\nDelegateCrossArenaSymbol miss classification:\n");
        out.push_str("  by source:\n");
        for (idx, name) in CROSS_ARENA_SYMBOL_MISS_SOURCE_NAMES.iter().enumerate() {
            let count = load(&c.delegate_cross_arena_symbol_miss_by_source[idx]);
            out.push_str(&format!("  {name:<28} {count:>12}\n"));
        }
        out.push_str("  by kind:\n");
        for (idx, name) in CROSS_ARENA_SYMBOL_MISS_KIND_NAMES.iter().enumerate() {
            let count = load(&c.delegate_cross_arena_symbol_miss_by_kind[idx]);
            if count > 0 {
                out.push_str(&format!("  {name:<28} {count:>12}\n"));
            }
        }
        out.push_str(&format!(
            "  {:<28} {:>12}\n  {:<28} {:>12}\n",
            "target .d.ts/.d.cts/.d.mts",
            load(&c.delegate_cross_arena_symbol_miss_target_declaration_file),
            "target source files",
            load(&c.delegate_cross_arena_symbol_miss_target_source_file),
        ));
        out
    }

    fn dump_delegate_declaration_file_miss_residues(
        rows: &[DelegateDeclarationFileMissResidue],
    ) -> String {
        if rows.is_empty() {
            return String::new();
        }

        let mut out = String::from("\nDelegateCrossArenaSymbol declaration-file miss residues:\n");
        for row in rows {
            let file = row.target_file.as_deref().unwrap_or("<unknown>");
            out.push_str(&format!(
                "  {:<32} {:<12} {:<20} {:>8}  {file}\n",
                row.name, row.kind, row.source, row.count,
            ));
        }
        out
    }

    fn dump_delegate_source_file_miss_residues(rows: &[DelegateSourceFileMissResidue]) -> String {
        if rows.is_empty() {
            return String::new();
        }

        let mut out = String::from("\nDelegateCrossArenaSymbol source-file miss residues:\n");
        for row in rows {
            let file = row.target_file.as_deref().unwrap_or("<unknown>");
            out.push_str(&format!(
                "  {:<32} {:<12} {:<20} {:>8}  {file}\n",
                row.name, row.kind, row.source, row.count,
            ));
        }
        out
    }

    fn dump_cross_arena_alias_shortcut_outcomes() -> String {
        let c = counters();
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
        let total: u64 = c
            .delegate_cross_arena_alias_shortcut_outcome
            .iter()
            .map(load)
            .sum();
        if total == 0 {
            return String::new();
        }

        let mut out = String::from("\nDelegateCrossArenaSymbol alias shortcut outcomes:\n");
        for (idx, name) in CROSS_ARENA_ALIAS_SHORTCUT_OUTCOME_NAMES.iter().enumerate() {
            let count = load(&c.delegate_cross_arena_alias_shortcut_outcome[idx]);
            if count > 0 {
                out.push_str(&format!("  {name:<28} {count:>12}\n"));
            }
        }
        out
    }

    fn dump_direct_cross_file_interface_lowering_outcomes() -> String {
        let c = counters();
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
        let total: u64 = c
            .direct_cross_file_interface_lowering_outcome
            .iter()
            .map(load)
            .sum();
        if total == 0 {
            return String::new();
        }

        let mut out = String::from("\nDirect cross-file interface lowering outcomes:\n");
        for (idx, name) in DIRECT_CROSS_FILE_INTERFACE_LOWERING_OUTCOME_NAMES
            .iter()
            .enumerate()
        {
            let count = load(&c.direct_cross_file_interface_lowering_outcome[idx]);
            if count > 0 {
                out.push_str(&format!("  {name:<28} {count:>12}\n"));
            }
        }
        out
    }

    fn dump_direct_actual_lib_alias_body_outcomes() -> String {
        let c = counters();
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
        let total: u64 = c
            .direct_actual_lib_alias_body_outcome
            .iter()
            .map(load)
            .sum();
        if total == 0 {
            return String::new();
        }

        let mut out = String::from("\nDirect actual-lib alias body outcomes:\n");
        for (idx, name) in DIRECT_ACTUAL_LIB_ALIAS_BODY_OUTCOME_NAMES
            .iter()
            .enumerate()
        {
            let count = load(&c.direct_actual_lib_alias_body_outcome[idx]);
            if count > 0 {
                out.push_str(&format!("  {name:<36} {count:>12}\n"));
            }
        }
        out
    }

    fn dump_direct_source_file_type_alias_lowering_outcomes() -> String {
        let c = counters();
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
        let total: u64 = c
            .direct_source_file_type_alias_lowering_outcome
            .iter()
            .map(load)
            .sum();
        if total == 0 {
            return String::new();
        }

        let mut out = String::from("\nDirect source-file type-alias lowering outcomes:\n");
        for (idx, name) in DIRECT_SOURCE_FILE_TYPE_ALIAS_LOWERING_OUTCOME_NAMES
            .iter()
            .enumerate()
        {
            let count = load(&c.direct_source_file_type_alias_lowering_outcome[idx]);
            if count > 0 {
                out.push_str(&format!("  {name:<36} {count:>12}\n"));
            }
        }
        out
    }

    fn dump_direct_source_file_type_alias_body_rejection_kinds() -> String {
        let c = counters();
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
        let total: u64 = c
            .direct_source_file_type_alias_body_rejection_kind
            .iter()
            .map(load)
            .sum();
        if total == 0 {
            return String::new();
        }

        let mut out = String::from("\nDirect source-file type-alias body rejection kinds:\n");
        for (idx, name) in DIRECT_SOURCE_FILE_TYPE_ALIAS_BODY_REJECTION_KIND_NAMES
            .iter()
            .enumerate()
        {
            let count = load(&c.direct_source_file_type_alias_body_rejection_kind[idx]);
            if count > 0 {
                out.push_str(&format!("  {name:<36} {count:>12}\n"));
            }
        }
        out
    }

    fn dump_direct_source_file_type_alias_type_reference_rejection_kinds() -> String {
        let c = counters();
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
        let total: u64 = c
            .direct_source_file_type_alias_type_reference_rejection_kind
            .iter()
            .map(load)
            .sum();
        if total == 0 {
            return String::new();
        }

        let mut out =
            String::from("\nDirect source-file type-alias type-reference rejection kinds:\n");
        for (idx, name) in DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_NAMES
            .iter()
            .enumerate()
        {
            let count = load(&c.direct_source_file_type_alias_type_reference_rejection_kind[idx]);
            if count > 0 {
                out.push_str(&format!("  {name:<44} {count:>12}\n"));
            }
        }
        out
    }

    fn dump_direct_source_file_type_alias_first_type_reference_rejection_kinds() -> String {
        let c = counters();
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
        let total: u64 = c
            .direct_source_file_type_alias_first_type_reference_rejection_kind
            .iter()
            .map(load)
            .sum();
        if total == 0 {
            return String::new();
        }

        let mut out =
            String::from("\nDirect source-file type-alias first type-reference rejection kinds:\n");
        for (idx, name) in DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_NAMES
            .iter()
            .enumerate()
        {
            let count =
                load(&c.direct_source_file_type_alias_first_type_reference_rejection_kind[idx]);
            if count > 0 {
                out.push_str(&format!("  {name:<44} {count:>12}\n"));
            }
        }
        out
    }

    fn dump_direct_source_file_type_alias_body_rejection_residues(
        rows: &[DirectSourceFileTypeAliasBodyRejectionResidue],
    ) -> String {
        if rows.is_empty() {
            return String::new();
        }

        let mut out = String::from("\ndirect source-file type-alias body rejection residues:\n");
        for row in rows {
            let type_ref_kind = row.first_type_reference_kind.unwrap_or("<none>");
            let type_ref_name = row.first_type_reference_name.as_deref().unwrap_or("<none>");
            let non_lowerable_kind = row
                .first_non_lowerable_type_reference_kind
                .unwrap_or("<none>");
            let non_lowerable_name = row
                .first_non_lowerable_type_reference_name
                .as_deref()
                .unwrap_or("<none>");
            let non_lowerable_leaf_kind = row
                .first_non_lowerable_leaf_type_reference_kind
                .unwrap_or("<none>");
            let non_lowerable_leaf_name = row
                .first_non_lowerable_leaf_type_reference_name
                .as_deref()
                .unwrap_or("<none>");
            let file = row.target_file.as_deref().unwrap_or("<unknown>");
            out.push_str(&format!(
                "  {:<32} {:<28} {:<36} {:<28} {:<36} {:<28} {:<36} {:<28} {:>8}  {file}\n",
                row.name,
                row.body_kind,
                type_ref_kind,
                type_ref_name,
                non_lowerable_kind,
                non_lowerable_name,
                non_lowerable_leaf_kind,
                non_lowerable_leaf_name,
                row.count,
            ));
        }
        out
    }

    fn dump_direct_actual_lib_intl_interface_outcomes() -> String {
        let c = counters();
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
        let total: u64 = c
            .direct_actual_lib_intl_interface_outcome
            .iter()
            .map(load)
            .sum();
        if total == 0 {
            return String::new();
        }

        let mut out = String::from("\nDirect actual-lib Intl interface outcomes:\n");
        for (idx, name) in DIRECT_ACTUAL_LIB_INTL_INTERFACE_OUTCOME_NAMES
            .iter()
            .enumerate()
        {
            let count = load(&c.direct_actual_lib_intl_interface_outcome[idx]);
            if count > 0 {
                out.push_str(&format!("  {name:<36} {count:>12}\n"));
            }
        }
        out
    }

    fn dump_source_file_symbol_arena_cache_eligibility_outcomes() -> String {
        let c = counters();
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
        let total: u64 = c
            .source_file_symbol_arena_cache_eligibility_outcome
            .iter()
            .map(load)
            .sum();
        if total == 0 {
            return String::new();
        }

        let mut out = String::from("\nSource-file symbol-arena cache eligibility outcomes:\n");
        for (idx, name) in SOURCE_FILE_SYMBOL_ARENA_CACHE_ELIGIBILITY_OUTCOME_NAMES
            .iter()
            .enumerate()
        {
            let count = load(&c.source_file_symbol_arena_cache_eligibility_outcome[idx]);
            if count > 0 {
                out.push_str(&format!("  {name:<32} {count:>12}\n"));
            }
        }
        out
    }

    /// Per-reason breakdown of `with_parent_cache` and overlay-copy calls.
    /// Sorted by `with_parent_cache` count descending so the headline
    /// offenders show first. Skips reasons with zero counts.
    fn dump_by_reason() -> String {
        let c = counters();
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
        // Collect (reason_idx, count, overlay_calls, overlay_entries, max_entries).
        let mut rows: Vec<(usize, u64, u64, u64, u64)> = (0..CHECKER_CREATION_REASON_COUNT)
            .map(|i| {
                (
                    i,
                    load(&c.with_parent_cache_by_reason[i]),
                    load(&c.overlay_copy_calls_by_reason[i]),
                    load(&c.overlay_copy_entries_by_reason[i]),
                    load(&c.overlay_copy_max_entries_by_reason[i]),
                )
            })
            .filter(|t| t.1 > 0 || t.2 > 0)
            .collect();
        if rows.is_empty() {
            return String::new();
        }
        rows.sort_by(|a, b| b.1.cmp(&a.1).then(b.3.cmp(&a.3)));
        let total_constructions = load(&c.checker_state_with_parent_cache_constructed).max(1);
        let total_overlay_entries = load(&c.copy_symbol_file_targets_entries_total).max(1);
        let mut out = String::from(
            "\n  with_parent_cache + overlay copies attributed by call site:\n  \
             reason                              cons    %  ovl_calls  ovl_entries          max  ent%\n",
        );
        for (i, cons, ovl_calls, ovl_entries, max_entries) in rows {
            let cons_pct = (cons as f64 / total_constructions as f64) * 100.0;
            let ent_pct = (ovl_entries as f64 / total_overlay_entries as f64) * 100.0;
            let row = format!(
                "  {:<32} {:>10} {:>4.1} {:>10} {:>12} {:>12} {:>5.1}\n",
                REASON_NAMES[i], cons, cons_pct, ovl_calls, ovl_entries, max_entries, ent_pct,
            );
            out.push_str(&row);
        }
        out
    }
}

// ─────────────────────────────────────────────────────────────────────────
//                      JSON snapshot (`PERFORMANCE_PLAN.md` §4.T0.3)
// ─────────────────────────────────────────────────────────────────────────
