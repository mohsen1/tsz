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
             full-work sentinel results {:>12}\n  \
             max recursion depth        {:>12}\n  \
             xfile type-params hits     {:>12}\n  \
             xfile type-params misses   {:>12}\n\
             Checker construction:\n  \
             CheckerState::new          {:>12}\n  \
             ::with_parent_cache        {:>12}\n  \
             ::reset_for_next_file      {:>12}\n  \
             reset cache entries max    {:>12}\n  \
             reset cache bytes max      {:>12}\n  \
             reset ns-member entries    {:>12}\n  \
             reset export= entries      {:>12}\n  \
             reset nested-ns entries    {:>12}\n  \
             reset lowering entries     {:>12}\n  \
             reset env-eval entries     {:>12}\n  \
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
             string intern cache hits   {:>12}\n  \
             type-list intern calls     {:>12}\n  \
             object-shape intern calls  {:>12}\n  \
             function-shape intern calls{:>12}\n  \
             callable-shape intern calls{:>12}\n  \
             application intern calls   {:>12}\n  \
             conditional intern calls   {:>12}\n  \
             mapped intern calls        {:>12}\n\
             TypeInterner locality (#13246):\n  \
             lookup calls               {:>12}\n  \
             lookup TLS hits            {:>12}\n  \
             lookup cold-Vec fallbacks  {:>12}\n  \
             lookup TLS evictions       {:>12}\n  \
             intern TLS hits            {:>12}\n  \
             intern cold fallbacks      {:>12}\n  \
             intern TLS evictions       {:>12}\n  \
             working-set distinct max   {:>12}\n  \
             working-set distinct total {:>12}\n  \
             working-set files sampled  {:>12}\n  \
             working-set files >cache   {:>12}\n  \
             promote-tier hits          {:>12}\n  \
             promote-tier misses        {:>12}\n\
             Solver materialization:\n  \
             union subtype reductions   {:>12}\n  \
             union reduction members    {:>12}\n  \
             union reduction max members{:>12}\n  \
             union pairwise budget      {:>12}\n  \
             union shallow checks       {:>12}\n  \
             property walks             {:>12}\n  \
             property entries walked    {:>12}\n  \
             property walk max entries  {:>12}\n  \
             property walks changed     {:>12}\n\
             Resolver:\n  \
             lookup calls               {:>12}\n  \
             is_file calls              {:>12}\n  \
             is_dir calls               {:>12}\n  \
             read_dir calls             {:>12}\n  \
             read_package_json calls    {:>12}\n  \
             candidate paths total      {:>12}\n\
             Stable identity:\n  \
             raw SymbolRef lazy fallback{:>12}\n  \
             wrong-decl collisions      {:>12}\n  \
             symbol_def_index hits      {:>12}\n  \
             symbol_def_index misses    {:>12}\n",
            snap.delegate.calls,
            snap.delegate.cache_hits_lib,
            snap.delegate.cache_hits_cross_file,
            snap.delegate.misses,
            snap.delegate.full_work_sentinel_results,
            snap.delegate.max_recursion_depth,
            snap.delegate.cross_file_type_params_cache_hits,
            snap.delegate.cross_file_type_params_cache_misses,
            snap.checker.state_constructed,
            snap.checker.with_parent_cache_constructed,
            snap.checker.file_session_resets,
            snap.checker.file_session_reset_cache_entries_max,
            snap.checker.file_session_reset_cache_bytes_max,
            snap.checker.file_session_reset_namespace_member_entries_max,
            snap.checker.file_session_reset_export_equals_entries_max,
            snap.checker.file_session_reset_nested_namespace_entries_max,
            snap.checker.file_session_reset_lowering_entity_name_entries_max,
            snap.checker.file_session_reset_env_eval_entries_max,
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
            snap.interner.string_intern_cache_hits,
            snap.interner.type_list_intern_calls,
            snap.interner.object_shape_intern_calls,
            snap.interner.function_shape_intern_calls,
            snap.interner.callable_shape_intern_calls,
            snap.interner.application_intern_calls,
            snap.interner.conditional_intern_calls,
            snap.interner.mapped_intern_calls,
            snap.interner.lookup_calls,
            snap.interner.lookup_tls_hits,
            snap.interner.lookup_cold_vec_fallbacks,
            snap.interner.lookup_tls_evictions,
            snap.interner.intern_tls_hits,
            snap.interner.intern_cold_fallbacks,
            snap.interner.intern_tls_evictions,
            snap.interner.working_set_distinct_max,
            snap.interner.working_set_distinct_total,
            snap.interner.working_set_files_sampled,
            snap.interner.working_set_files_over_cache,
            snap.interner.promote_tier_hits,
            snap.interner.promote_tier_misses,
            snap.solver_materialization
                .union_subtype_reduction_calls,
            snap.solver_materialization
                .union_subtype_reduction_members_total,
            snap.solver_materialization
                .union_subtype_reduction_members_max,
            snap.solver_materialization
                .union_subtype_reduction_pairwise_budget_total,
            snap.solver_materialization
                .union_subtype_reduction_shallow_checks,
            snap.solver_materialization.property_instantiation_walks,
            snap.solver_materialization
                .property_instantiation_properties_total,
            snap.solver_materialization
                .property_instantiation_properties_max,
            snap.solver_materialization.property_instantiation_changed,
            snap.resolver.lookup_calls,
            snap.resolver.is_file_calls.unwrap_or(0),
            snap.resolver.is_dir_calls.unwrap_or(0),
            snap.resolver.read_dir_calls.unwrap_or(0),
            snap.resolver.package_json_reads,
            snap.resolver.candidate_paths_total,
            snap.identity.type_environment_raw_symbol_lazy_fallbacks,
            snap.identity.identity_collision_wrong_decl_suppressed,
            snap.identity.symbol_def_index_lookup_hits,
            snap.identity.symbol_def_index_lookup_misses,
        ) + &Self::dump_compute_type_of_symbol_outcomes()
            + &Self::dump_shared_instantiation_cache(&snap)
            + &Self::dump_relation_limit_cache(&snap)
            + &Self::dump_evaluator_memo(&snap)
            + &Self::dump_compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues(
                &snap.compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues,
            )
            + &Self::dump_compute_type_of_symbol_interface_simple_object_declaration_provenance_residues(
                &snap.compute_type_of_symbol_interface_simple_object_declaration_provenance_residues,
            )
            + &Self::dump_compute_type_of_symbol_interface_simple_object_type_reference_reject_residues(
                &snap.compute_type_of_symbol_interface_simple_object_type_reference_reject_residues,
            )
            + &Self::dump_cross_file_cache_miss_causes(&snap.cross_file_cache_miss_causes)
            + &Self::dump_cross_arena_symbol_miss_classification()
            + &Self::dump_cross_arena_alias_shortcut_outcomes()
            + &Self::dump_direct_cross_file_interface_lowering_outcomes()
            + &Self::dump_direct_cross_file_interface_complex_reasons()
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
            + &Self::dump_lib_bootstrap(&snap)
            + &Self::dump_source_file_symbol_arena_cache_eligibility_outcomes()
            + &Self::dump_slow_check_timings(&snap)
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
        let interface_simple_object_actual_lib_type_reference_outcome_total: u64 = c
            .compute_type_of_symbol_interface_simple_object_actual_lib_type_reference_outcome
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
            && interface_simple_object_actual_lib_type_reference_outcome_total == 0
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
        if interface_simple_object_actual_lib_type_reference_outcome_total > 0 {
            out.push_str(
                "\ncompute_type_of_symbol interface simple-object actual-lib type-reference outcomes:\n",
            );
            for (idx, name) in
                COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_ACTUAL_LIB_TYPE_REFERENCE_OUTCOME_NAMES
                    .iter()
                    .enumerate()
            {
                let count = load(
                    &c.compute_type_of_symbol_interface_simple_object_actual_lib_type_reference_outcome
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

    /// Why canonical cross-file query bucket reads returned `None` (see
    /// `CrossFileCacheMissCause`). `bucket_empty` is the expected first-miss
    /// case; a large `sentinel_error_unknown` count means completed
    /// `ERROR`/`UNKNOWN` answers are being recomputed instead of replayed.
    fn dump_cross_file_cache_miss_causes(causes: &[NamedCount]) -> String {
        let total: u64 = causes.iter().map(|c| c.count).sum();
        if total == 0 {
            return String::new();
        }
        let mut out = String::from("\nCross-file query bucket miss causes:\n");
        for cause in causes {
            if cause.count > 0 {
                out.push_str(&format!("  {:<28} {:>12}\n", cause.name, cause.count));
            }
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

    fn dump_direct_cross_file_interface_complex_reasons() -> String {
        let c = counters();
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
        let total: u64 = c
            .direct_cross_file_interface_complex_reason
            .iter()
            .map(load)
            .sum();
        if total == 0 {
            return String::new();
        }

        let mut out = String::from("\nDirect cross-file interface complex reasons:\n");
        for (idx, name) in DIRECT_CROSS_FILE_INTERFACE_COMPLEX_REASON_NAMES
            .iter()
            .enumerate()
        {
            let count = load(&c.direct_cross_file_interface_complex_reason[idx]);
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

    fn dump_lib_bootstrap(snap: &PerfCounterSnapshot) -> String {
        let counters = snap.lib_bootstrap;
        if counters.snapshot_set_load_attempts == 0 && counters.checker_lib_clone_calls == 0 {
            return String::new();
        }

        let mut out = String::from("\nLib bootstrap attribution:\n");
        if counters.snapshot_set_load_attempts > 0 {
            out.push_str(&format!(
                "  snapshot set load  attempts={} hits={} misses={} files={} total_ms={:.2} max_ms={:.2}\n",
                counters.snapshot_set_load_attempts,
                counters.snapshot_set_load_hits,
                counters.snapshot_set_load_misses,
                counters.snapshot_set_load_files_total,
                counters.snapshot_set_load_elapsed_ms_total,
                counters.snapshot_set_load_elapsed_ms_max,
            ));
        }
        if counters.checker_lib_clone_calls > 0 {
            out.push_str(&format!(
                "  checker lib clone  calls={} parallel_calls={} files={} total_ms={:.2} max_ms={:.2}\n",
                counters.checker_lib_clone_calls,
                counters.checker_lib_clone_parallel_calls,
                counters.checker_lib_clone_files_total,
                counters.checker_lib_clone_elapsed_ms_total,
                counters.checker_lib_clone_elapsed_ms_max,
            ));
        }
        out
    }

    fn dump_shared_instantiation_cache(snap: &PerfCounterSnapshot) -> String {
        let counters = &snap.shared_instantiation_cache;
        if counters.application_eval_shared_hits == 0
            && counters.application_eval_shared_misses == 0
            && counters.application_eval_shared_inserts == 0
            && counters.application_eval_shared_bypasses == 0
            && counters.instantiation_shared_hits == 0
            && counters.instantiation_shared_misses == 0
            && counters.instantiation_shared_inserts == 0
            && counters.instantiation_shared_bypasses == 0
        {
            return String::new();
        }
        format!(
            "\nOpt-in shared instantiation caches:\n  \
             application eval shared hits     {:>12}\n  \
             application eval shared misses   {:>12}\n  \
             application eval shared inserts  {:>12}\n  \
             application eval shared bypasses {:>12}\n  \
             instantiation shared hits        {:>12}\n  \
             instantiation shared misses      {:>12}\n  \
             instantiation shared inserts     {:>12}\n  \
             instantiation shared bypasses    {:>12}\n",
            counters.application_eval_shared_hits,
            counters.application_eval_shared_misses,
            counters.application_eval_shared_inserts,
            counters.application_eval_shared_bypasses,
            counters.instantiation_shared_hits,
            counters.instantiation_shared_misses,
            counters.instantiation_shared_inserts,
            counters.instantiation_shared_bypasses,
        )
    }

    fn dump_relation_limit_cache(snap: &PerfCounterSnapshot) -> String {
        let counters = &snap.relation_limit_cache;
        if counters.limit_cache_hits == 0 && counters.maybe_promotions == 0 {
            return String::new();
        }
        format!(
            "\nRelation limit-result cache:\n  \
             limit cache hits          {:>12}\n  \
             maybe promotions          {:>12}\n",
            counters.limit_cache_hits, counters.maybe_promotions,
        )
    }

    fn dump_evaluator_memo(snap: &PerfCounterSnapshot) -> String {
        let counters = &snap.evaluator_memo;
        let termination_total: u64 = counters
            .termination_guard_fires
            .iter()
            .map(|g| g.count)
            .sum();
        if counters.constructions == 0
            && counters.local_memo_hits == 0
            && counters.compute_nodes == 0
            && counters.lost_memo_recomputes == 0
            && counters.lost_memo_mismatches == 0
            && counters.lost_memo_recomputes_identity == 0
            && counters.memo_nested_hits == 0
            && counters.lost_memo_recomputes_plain == 0
            && counters.lost_memo_recomputes_authoritative == 0
            && counters.lost_memo_recomputes_other == 0
            && counters.dropped_memo_entries == 0
            && counters.dropped_aux_entries == 0
            && termination_total == 0
        {
            return String::new();
        }
        let mut out = format!(
            "\nEvaluator memo lifecycle:\n  \
             constructions             {:>12}\n  \
             local memo hits           {:>12}\n  \
             compute nodes             {:>12}\n  \
             lost recomputes           {:>12}\n  \
             lost mismatches           {:>12}\n  \
             identity recomputes       {:>12}\n  \
             nested memo hits          {:>12}\n  \
             plain recomputes          {:>12}\n  \
             authoritative recomputes  {:>12}\n  \
             other recomputes          {:>12}\n  \
             dropped memo entries      {:>12}\n  \
             dropped aux entries       {:>12}\n",
            counters.constructions,
            counters.local_memo_hits,
            counters.compute_nodes,
            counters.lost_memo_recomputes,
            counters.lost_memo_mismatches,
            counters.lost_memo_recomputes_identity,
            counters.memo_nested_hits,
            counters.lost_memo_recomputes_plain,
            counters.lost_memo_recomputes_authoritative,
            counters.lost_memo_recomputes_other,
            counters.dropped_memo_entries,
            counters.dropped_aux_entries,
        );
        // #14346: which guard cut a walk short, and how often. The
        // firing-order signal — a nonzero bucket fingerprints which bound a
        // runaway recursive walk hits first.
        if termination_total > 0 {
            out.push_str("  termination guard fires:\n");
            for guard in &counters.termination_guard_fires {
                if guard.count > 0 {
                    out.push_str(&format!("    {:<26} {:>12}\n", guard.name, guard.count));
                }
            }
        }
        out
    }

    fn dump_slow_check_timings(snap: &PerfCounterSnapshot) -> String {
        if snap.slow_check_file_timings.is_empty()
            && snap.slow_check_statement_timings.is_empty()
            && snap.slow_type_alias_check_timings.is_empty()
        {
            return String::new();
        }

        let mut out = String::new();
        if !snap.slow_check_file_timings.is_empty() {
            out.push_str("\nSlowest semantic check files:\n");
            for row in snap.slow_check_file_timings.iter().take(10) {
                out.push_str(&format!(
                    "  {:>8.2} ms  diags={:>4}  {}\n",
                    row.elapsed_ms, row.diagnostics, row.file
                ));
            }
        }
        if !snap.slow_check_statement_timings.is_empty() {
            out.push_str("\nSlowest semantic check statements:\n");
            for row in snap.slow_check_statement_timings.iter().take(10) {
                out.push_str(&format!(
                    "  {:>8.2} ms  kind={:>4}  span={:>8}..{:<8}  {}\n",
                    row.elapsed_ms, row.kind, row.pos, row.end, row.file
                ));
            }
        }
        if !snap.slow_type_alias_check_timings.is_empty() {
            out.push_str("\nSlowest type alias check phases:\n");
            for row in snap.slow_type_alias_check_timings.iter().take(10) {
                out.push_str(&format!(
                    "  {:>8.2} ms  phase={:<24} span={:>8}..{:<8}  {}  {}\n",
                    row.elapsed_ms, row.phase, row.pos, row.end, row.name, row.file
                ));
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
