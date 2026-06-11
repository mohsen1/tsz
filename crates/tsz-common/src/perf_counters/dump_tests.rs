mod dump_tests {
    use super::*;

    #[test]
    fn dump_string_surfaces_enum_backed_counter_families() {
        // Issue #13130 was opened because enum-backed counter families can be
        // added to storage/snapshot state without being surfaced in the text
        // dump. Bump one distinctive bucket per family and lock the formatter
        // surface so future bucket-family additions have to update this test.
        force_enable_perf_counters_for_tests();

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
        ] {
            assert!(
                dump.contains(needle),
                "perf counter text dump omitted enum-backed bucket `{needle}`:\n{dump}"
            );
        }
    }
}
