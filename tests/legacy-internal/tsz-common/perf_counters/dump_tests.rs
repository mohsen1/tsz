mod dump_tests {
    use super::*;

    #[test]
    fn dump_string_surfaces_enum_backed_counter_families() {
        // Issue #13130 was opened because enum-backed counter families can be
        // added to storage/snapshot state without being surfaced in the text
        // dump. Bump one distinctive bucket per family and lock the formatter
        // surface so future bucket-family additions have to update this test.
        //
        // Scoped: the dump only prints a bucket's label when its count is
        // nonzero (see `dump.rs`), so an unscoped process-wide counter could
        // in principle pass because a concurrently-running sibling test
        // happened to bump the same enum variant, not because this test's own
        // formatter wiring is correct.
        let _scope = ScopedPerfCounters::new();

        let c = counters();
        c.with_parent_cache_by_reason[CheckerCreationReason::ImportType.as_index()]
            .fetch_add(1, Ordering::Relaxed);
        c.delegate_cross_arena_symbol_miss_by_source
            [CrossArenaSymbolMissSource::DeclarationArena.as_index()]
        .fetch_add(1, Ordering::Relaxed);
        c.delegate_cross_arena_symbol_miss_by_kind
            [CrossArenaSymbolMissKind::TypeLiteral.as_index()]
        .fetch_add(1, Ordering::Relaxed);
        c.delegate_cross_arena_alias_shortcut_outcome
            [CrossArenaAliasShortcutOutcome::MissingAliasFile.as_index()]
        .fetch_add(1, Ordering::Relaxed);
        c.direct_cross_file_interface_lowering_outcome
            [DirectCrossFileInterfaceLoweringOutcome::ComplexDeclaration.as_index()]
        .fetch_add(1, Ordering::Relaxed);
        c.direct_cross_file_interface_complex_reason
            [DirectCrossFileInterfaceComplexReason::ComputedName.as_index()]
        .fetch_add(1, Ordering::Relaxed);
        c.direct_actual_lib_alias_body_outcome
            [DirectActualLibAliasBodyOutcome::ResolverNotLazyDef.as_index()]
        .fetch_add(1, Ordering::Relaxed);
        c.direct_source_file_type_alias_lowering_outcome
            [DirectSourceFileTypeAliasLoweringOutcome::SourceFileArenaNotAllowed.as_index()]
        .fetch_add(1, Ordering::Relaxed);
        c.direct_source_file_type_alias_body_rejection_kind
            [DirectSourceFileTypeAliasBodyRejectionKind::TemplateLiteralType.as_index()]
        .fetch_add(1, Ordering::Relaxed);
        c.direct_source_file_type_alias_type_reference_rejection_kind
            [DirectSourceFileTypeAliasTypeReferenceRejectionKind::QualifiedName.as_index()]
        .fetch_add(1, Ordering::Relaxed);
        c.direct_source_file_type_alias_first_type_reference_rejection_kind
            [DirectSourceFileTypeAliasTypeReferenceRejectionKind::LocalNamespaceSymbol.as_index()]
        .fetch_add(1, Ordering::Relaxed);
        c.direct_actual_lib_intl_interface_outcome
            [DirectActualLibIntlInterfaceOutcome::NamespaceSymbolMismatch.as_index()]
        .fetch_add(1, Ordering::Relaxed);
        c.compute_type_of_symbol_source_outcome
            [ComputeTypeOfSymbolSourceOutcome::MissingSymbol.as_index()]
        .fetch_add(1, Ordering::Relaxed);
        c.compute_type_of_symbol_kind_outcome
            [ComputeTypeOfSymbolKindOutcome::ObjectLiteral.as_index()]
        .fetch_add(1, Ordering::Relaxed);
        c.compute_type_of_symbol_interface_fastpath_outcome
            [ComputeTypeOfSymbolInterfaceFastPathOutcome::SkipComputedNameMapAndLocalHeritageMerge
                .as_index()]
        .fetch_add(1, Ordering::Relaxed);
        c.compute_type_of_symbol_interface_callsite_outcome
            [ComputeTypeOfSymbolInterfaceCallsiteOutcome::ParentMissing.as_index()]
        .fetch_add(1, Ordering::Relaxed);
        c.compute_type_of_symbol_interface_simple_object_outcome
            [ComputeTypeOfSymbolInterfaceSimpleObjectOutcome::RejectNonPrimitiveAnnotation
                .as_index()]
        .fetch_add(1, Ordering::Relaxed);
        c.compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kind
            [ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationKind::ImportOrTypeQuery
                .as_index()]
        .fetch_add(1, Ordering::Relaxed);
        c.compute_type_of_symbol_interface_simple_object_type_reference_reject_outcome
            [ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectOutcome::QualifiedNameNotFoundSymbol
                .as_index()]
        .fetch_add(1, Ordering::Relaxed);
        c.compute_type_of_symbol_interface_simple_object_actual_lib_type_reference_outcome
            [ComputeTypeOfSymbolInterfaceSimpleObjectActualLibTypeReferenceOutcome::FileLocalShadow
                .as_index()]
        .fetch_add(1, Ordering::Relaxed);
        c.cross_file_cache_miss_cause[CrossFileCacheMissCause::SentinelErrorUnknown.as_index()]
            .fetch_add(1, Ordering::Relaxed);
        c.source_file_symbol_arena_cache_eligibility_outcome
            [SourceFileSymbolArenaCacheEligibilityOutcome::CacheableDeclarationFile.as_index()]
        .fetch_add(1, Ordering::Relaxed);
        c.eval_termination_guard_fires
            [EvaluationTerminationGuard::SolverStackFrames.as_index()]
        .fetch_add(1, Ordering::Relaxed);

        let dump = PerfCounters::dump_string();
        for needle in [
            "ImportType",
            "declaration_arenas",
            "type_literal",
            "missing_alias_file",
            "complex_declaration",
            "computed_name",
            "resolver_not_lazy_def",
            "source_file_arena_not_allowed",
            "template_literal_type",
            "qualified_name",
            "local_namespace_symbol",
            "namespace_symbol_mismatch",
            "missing_symbol",
            "object_literal",
            "skip_computed_name_map_and_local_heritage_merge",
            "parent_missing",
            "reject_non_primitive_annotation",
            "import_or_type_query",
            "qualified_name_not_found_symbol",
            "file_local_shadow",
            "sentinel_error_unknown",
            "cacheable_declaration_file",
            "solver_stack_frames",
        ] {
            assert!(
                dump.contains(needle),
                "perf counter text dump omitted enum-backed bucket `{needle}`:\n{dump}"
            );
        }
    }

    #[test]
    fn dump_string_surfaces_scalar_snapshot_counter_sections() {
        // Keep scalar snapshot sections from repeating the original #13130
        // drift pattern: a counter can be wired into storage and JSON while
        // remaining invisible to humans reading the text dump.
        let _scope = ScopedPerfCounters::new();

        let c = counters();
        c.relation_limit_cache_hits
            .fetch_add(17, Ordering::Relaxed);
        c.relation_maybe_promotions
            .fetch_add(19, Ordering::Relaxed);
        c.shared_application_eval_cache_hits
            .fetch_add(20, Ordering::Relaxed);
        c.shared_application_eval_cache_misses
            .fetch_add(21, Ordering::Relaxed);
        c.shared_application_eval_cache_inserts
            .fetch_add(22, Ordering::Relaxed);
        c.shared_instantiation_cache_hits
            .fetch_add(24, Ordering::Relaxed);
        c.shared_instantiation_cache_misses
            .fetch_add(25, Ordering::Relaxed);
        c.shared_instantiation_cache_inserts
            .fetch_add(26, Ordering::Relaxed);
        c.eval_evaluator_constructions
            .fetch_add(23, Ordering::Relaxed);
        c.eval_local_memo_hits.fetch_add(29, Ordering::Relaxed);
        c.eval_compute_nodes.fetch_add(31, Ordering::Relaxed);
        c.eval_lost_memo_recomputes
            .fetch_add(37, Ordering::Relaxed);
        c.eval_lost_memo_mismatches
            .fetch_add(41, Ordering::Relaxed);
        c.eval_lost_memo_recomputes_identity
            .fetch_add(43, Ordering::Relaxed);
        c.eval_memo_nested_hits.fetch_add(47, Ordering::Relaxed);
        c.eval_lost_memo_recomputes_plain
            .fetch_add(53, Ordering::Relaxed);
        c.eval_lost_memo_recomputes_authoritative
            .fetch_add(59, Ordering::Relaxed);
        c.eval_lost_memo_recomputes_other
            .fetch_add(61, Ordering::Relaxed);
        c.eval_dropped_memo_entries
            .fetch_add(67, Ordering::Relaxed);
        c.eval_dropped_aux_entries.fetch_add(71, Ordering::Relaxed);

        let dump = PerfCounters::dump_string();
        for needle in [
            "Relation limit-result cache",
            "limit cache hits",
            "maybe promotions",
            "Opt-in shared instantiation caches",
            "application eval shared hits",
            "application eval shared misses",
            "application eval shared inserts",
            "instantiation shared hits",
            "instantiation shared misses",
            "instantiation shared inserts",
            "Evaluator memo lifecycle",
            "constructions",
            "local memo hits",
            "compute nodes",
            "lost recomputes",
            "lost mismatches",
            "identity recomputes",
            "nested memo hits",
            "plain recomputes",
            "authoritative recomputes",
            "other recomputes",
            "dropped memo entries",
            "dropped aux entries",
        ] {
            assert!(
                dump.contains(needle),
                "perf counter text dump omitted scalar counter `{needle}`:\n{dump}"
            );
        }
    }

    #[test]
    fn eval_termination_guard_recorder_targets_named_bucket() {
        // #14346: `record_eval_termination_guard` increments exactly the
        // bucket for the guard that fired. Scoped rather than delta-based:
        // a before/after delta on the process-wide atomics is immune to
        // increments made before the window but not to increments a sibling
        // thread makes concurrently inside it (see #16017's `assignability_
        // failure_memo_tests` false red/green pair for the general failure
        // mode), so a private per-thread counter set is the version of this
        // test that cannot be perturbed by anything but its own call.
        let _scope = ScopedPerfCounters::new();
        let c = counters();
        let load = |g: EvaluationTerminationGuard| {
            c.eval_termination_guard_fires[g.as_index()].load(Ordering::Relaxed)
        };

        let before_fuel = load(EvaluationTerminationGuard::FuelExhausted);
        let before_query = load(EvaluationTerminationGuard::QueryOpBudget);

        record_eval_termination_guard(EvaluationTerminationGuard::FuelExhausted);

        assert_eq!(
            load(EvaluationTerminationGuard::FuelExhausted),
            before_fuel + 1,
            "the fired guard's bucket must increment by exactly one"
        );
        assert_eq!(
            load(EvaluationTerminationGuard::QueryOpBudget),
            before_query,
            "an unrelated guard's bucket must not move"
        );
    }
}
