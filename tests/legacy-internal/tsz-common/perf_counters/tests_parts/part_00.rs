    #[test]
    fn classification_arrays_propagate_atomic_state_into_snapshot() {
        // The producer helpers (`record_cross_arena_*`) short-circuit on
        // `enabled_fast() == false`, so we cannot rely on them in a test
        // process where `TSZ_PERF_COUNTERS` is unset. Instead drive the
        // underlying atomics directly to prove the snapshot reads them
        // back at the right indices — the same atomic-bump the producer
        // would do under the gate.
        //
        // Use `fetch_add(1)` rather than overwriting so this test stays
        // resilient to other tests that may also touch the global
        // atomics. Capture the pre-bump counts and assert the post-bump
        // snapshot reflects the delta.
        let c = counters();

        let source_idx = CrossArenaSymbolMissSource::SymbolArena.as_index();
        let kind_idx = CrossArenaSymbolMissKind::Class.as_index();
        let aso_idx = CrossArenaAliasShortcutOutcome::Success.as_index();
        let sfsa_idx = SourceFileSymbolArenaCacheEligibilityOutcome::Cacheable.as_index();
        let dilo_idx = DirectCrossFileInterfaceLoweringOutcome::Success.as_index();
        let dicr_idx = DirectCrossFileInterfaceComplexReason::Heritage.as_index();
        let dalabo_idx = DirectActualLibAliasBodyOutcome::Success.as_index();
        let daliio_idx = DirectActualLibIntlInterfaceOutcome::SuccessByName.as_index();
        let ctos_source_idx = ComputeTypeOfSymbolSourceOutcome::GlobalSymbol.as_index();
        let ctos_kind_idx = ComputeTypeOfSymbolKindOutcome::Interface.as_index();
        let ctos_fastpath_idx =
            ComputeTypeOfSymbolInterfaceFastPathOutcome::SkipAllThree.as_index();
        let ctos_callsite_idx = ComputeTypeOfSymbolInterfaceCallsiteOutcome::Root.as_index();
        let ctos_simple_object_outcome_idx =
            ComputeTypeOfSymbolInterfaceSimpleObjectOutcome::Success.as_index();
        let ctos_simple_object_non_primitive_annotation_kind_idx =
            ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationKind::TypeReference
                .as_index();
        let ctos_simple_object_type_reference_reject_outcome_idx =
            ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectOutcome::IdentifierNotFoundSymbol
                .as_index();
        let ctos_simple_object_actual_lib_type_reference_outcome_idx =
            ComputeTypeOfSymbolInterfaceSimpleObjectActualLibTypeReferenceOutcome::Success
                .as_index();

        let before_source =
            c.delegate_cross_arena_symbol_miss_by_source[source_idx].load(Ordering::Relaxed);
        let before_kind =
            c.delegate_cross_arena_symbol_miss_by_kind[kind_idx].load(Ordering::Relaxed);
        let before_decl_file = c
            .delegate_cross_arena_symbol_miss_target_declaration_file
            .load(Ordering::Relaxed);
        let before_aso =
            c.delegate_cross_arena_alias_shortcut_outcome[aso_idx].load(Ordering::Relaxed);
        let before_sfsa =
            c.source_file_symbol_arena_cache_eligibility_outcome[sfsa_idx].load(Ordering::Relaxed);
        let before_dilo =
            c.direct_cross_file_interface_lowering_outcome[dilo_idx].load(Ordering::Relaxed);
        let before_dicr =
            c.direct_cross_file_interface_complex_reason[dicr_idx].load(Ordering::Relaxed);
        let before_dalabo =
            c.direct_actual_lib_alias_body_outcome[dalabo_idx].load(Ordering::Relaxed);
        let before_daliio =
            c.direct_actual_lib_intl_interface_outcome[daliio_idx].load(Ordering::Relaxed);
        let before_ctos_source =
            c.compute_type_of_symbol_source_outcome[ctos_source_idx].load(Ordering::Relaxed);
        let before_ctos_kind =
            c.compute_type_of_symbol_kind_outcome[ctos_kind_idx].load(Ordering::Relaxed);
        let before_ctos_fastpath = c.compute_type_of_symbol_interface_fastpath_outcome
            [ctos_fastpath_idx]
            .load(Ordering::Relaxed);
        let before_ctos_callsite = c.compute_type_of_symbol_interface_callsite_outcome
            [ctos_callsite_idx]
            .load(Ordering::Relaxed);
        let before_ctos_simple_object_outcome = c
            .compute_type_of_symbol_interface_simple_object_outcome[ctos_simple_object_outcome_idx]
            .load(Ordering::Relaxed);
        let before_ctos_simple_object_hits = c
            .compute_type_of_symbol_interface_simple_object_fastpath_hits
            .load(Ordering::Relaxed);
        let before_property_classification_calls =
            c.property_classification_calls.load(Ordering::Relaxed);
        let before_property_classification_source_lookups = c
            .property_classification_string_fallback_source_lookups
            .load(Ordering::Relaxed);
        let before_property_classification_target_names = c
            .property_classification_string_fallback_target_names
            .load(Ordering::Relaxed);
        let before_property_classification_target_types = c
            .property_classification_string_fallback_target_types
            .load(Ordering::Relaxed);
        let before_ctos_simple_object_non_primitive_annotation_kind = c
            .compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kind
            [ctos_simple_object_non_primitive_annotation_kind_idx]
            .load(Ordering::Relaxed);
        let before_ctos_simple_object_type_reference_reject_outcome = c
            .compute_type_of_symbol_interface_simple_object_type_reference_reject_outcome
            [ctos_simple_object_type_reference_reject_outcome_idx]
            .load(Ordering::Relaxed);
        let before_ctos_simple_object_actual_lib_type_reference_outcome = c
            .compute_type_of_symbol_interface_simple_object_actual_lib_type_reference_outcome
            [ctos_simple_object_actual_lib_type_reference_outcome_idx]
            .load(Ordering::Relaxed);

        c.delegate_cross_arena_symbol_miss_by_source[source_idx].fetch_add(1, Ordering::Relaxed);
        c.delegate_cross_arena_symbol_miss_by_kind[kind_idx].fetch_add(1, Ordering::Relaxed);
        c.delegate_cross_arena_symbol_miss_target_declaration_file
            .fetch_add(1, Ordering::Relaxed);
        c.delegate_cross_arena_alias_shortcut_outcome[aso_idx].fetch_add(1, Ordering::Relaxed);
        c.source_file_symbol_arena_cache_eligibility_outcome[sfsa_idx]
            .fetch_add(1, Ordering::Relaxed);
        c.direct_cross_file_interface_lowering_outcome[dilo_idx].fetch_add(1, Ordering::Relaxed);
        c.direct_cross_file_interface_complex_reason[dicr_idx].fetch_add(1, Ordering::Relaxed);
        c.direct_actual_lib_alias_body_outcome[dalabo_idx].fetch_add(1, Ordering::Relaxed);
        c.direct_actual_lib_intl_interface_outcome[daliio_idx].fetch_add(1, Ordering::Relaxed);
        c.compute_type_of_symbol_source_outcome[ctos_source_idx].fetch_add(1, Ordering::Relaxed);
        c.compute_type_of_symbol_kind_outcome[ctos_kind_idx].fetch_add(1, Ordering::Relaxed);
        c.compute_type_of_symbol_interface_fastpath_outcome[ctos_fastpath_idx]
            .fetch_add(1, Ordering::Relaxed);
        c.compute_type_of_symbol_interface_callsite_outcome[ctos_callsite_idx]
            .fetch_add(1, Ordering::Relaxed);
        c.compute_type_of_symbol_interface_simple_object_outcome[ctos_simple_object_outcome_idx]
            .fetch_add(1, Ordering::Relaxed);
        c.compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kind
            [ctos_simple_object_non_primitive_annotation_kind_idx]
            .fetch_add(1, Ordering::Relaxed);
        c.compute_type_of_symbol_interface_simple_object_type_reference_reject_outcome
            [ctos_simple_object_type_reference_reject_outcome_idx]
            .fetch_add(1, Ordering::Relaxed);
        c.compute_type_of_symbol_interface_simple_object_actual_lib_type_reference_outcome
            [ctos_simple_object_actual_lib_type_reference_outcome_idx]
            .fetch_add(1, Ordering::Relaxed);
        c.compute_type_of_symbol_interface_simple_object_fastpath_hits
            .fetch_add(1, Ordering::Relaxed);
        c.property_classification_calls
            .fetch_add(1, Ordering::Relaxed);
        c.property_classification_string_fallback_source_lookups
            .fetch_add(1, Ordering::Relaxed);
        c.property_classification_string_fallback_target_names
            .fetch_add(1, Ordering::Relaxed);
        c.property_classification_string_fallback_target_types
            .fetch_add(1, Ordering::Relaxed);

        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");

        let by_source = json["delegate_miss_classification"]["by_source"]
            .as_array()
            .expect("by_source is array");
        let symbol_arena_row = &by_source[source_idx];
        assert_eq!(symbol_arena_row["name"], "symbol_arenas");
        assert!(
            symbol_arena_row["count"].as_u64().unwrap_or(0) > before_source,
            "by_source[symbol_arenas] did not reflect the bump",
        );

        let by_kind = json["delegate_miss_classification"]["by_kind"]
            .as_array()
            .expect("by_kind is array");
        let class_row = &by_kind[kind_idx];
        assert_eq!(class_row["name"], "class");
        assert!(
            class_row["count"].as_u64().unwrap_or(0) > before_kind,
            "by_kind[class] did not reflect the bump",
        );

        assert!(
            json["delegate_miss_classification"]["target_declaration_files"]
                .as_u64()
                .unwrap_or(0)
                > before_decl_file,
            "target_declaration_files did not reflect the bump",
        );

        let aso = json["alias_shortcut_outcomes"]
            .as_array()
            .expect("alias_shortcut_outcomes is array");
        let success_row = &aso[aso_idx];
        assert_eq!(success_row["name"], "success");
        assert!(
            success_row["count"].as_u64().unwrap_or(0) > before_aso,
            "alias_shortcut_outcomes[success] did not reflect the bump",
        );

        let sfsa = json["source_file_symbol_arena_cache_eligibility_outcomes"]
            .as_array()
            .expect("source_file_symbol_arena_cache_eligibility_outcomes is array");
        let cacheable_row = &sfsa[sfsa_idx];
        assert_eq!(cacheable_row["name"], "cacheable");
        assert!(
            cacheable_row["count"].as_u64().unwrap_or(0) > before_sfsa,
            "source_file_symbol_arena_cache_eligibility_outcomes[cacheable] did not reflect the bump",
        );

        let dilo = json["direct_interface_lowering_outcomes"]
            .as_array()
            .expect("direct_interface_lowering_outcomes is array");
        let dilo_row = &dilo[dilo_idx];
        assert_eq!(dilo_row["name"], "success");
        assert!(
            dilo_row["count"].as_u64().unwrap_or(0) > before_dilo,
            "direct_interface_lowering_outcomes[success] did not reflect the bump",
        );
        let dicr = json["direct_interface_complex_reasons"]
            .as_array()
            .expect("direct_interface_complex_reasons is array");
        let dicr_row = &dicr[dicr_idx];
        assert_eq!(dicr_row["name"], "heritage");
        assert!(
            dicr_row["count"].as_u64().unwrap_or(0) > before_dicr,
            "direct_interface_complex_reasons[heritage] did not reflect the bump",
        );

        let dalabo = json["direct_actual_lib_alias_body_outcomes"]
            .as_array()
            .expect("direct_actual_lib_alias_body_outcomes is array");
        let dalabo_row = &dalabo[dalabo_idx];
        assert_eq!(dalabo_row["name"], "success");
        assert!(
            dalabo_row["count"].as_u64().unwrap_or(0) > before_dalabo,
            "direct_actual_lib_alias_body_outcomes[success] did not reflect the bump",
        );

        let daliio = json["direct_actual_lib_intl_interface_outcomes"]
            .as_array()
            .expect("direct_actual_lib_intl_interface_outcomes is array");
        let daliio_row = &daliio[daliio_idx];
        assert_eq!(daliio_row["name"], "success_by_name");
        assert!(
            daliio_row["count"].as_u64().unwrap_or(0) > before_daliio,
            "direct_actual_lib_intl_interface_outcomes[success_by_name] did not reflect the bump",
        );

        let ctos_source = json["compute_type_of_symbol_source_outcomes"]
            .as_array()
            .expect("compute_type_of_symbol_source_outcomes is array");
        let ctos_source_row = &ctos_source[ctos_source_idx];
        assert_eq!(ctos_source_row["name"], "global_symbol");
        assert!(
            ctos_source_row["count"].as_u64().unwrap_or(0) > before_ctos_source,
            "compute_type_of_symbol_source_outcomes[global_symbol] did not reflect the bump",
        );

        let ctos_kind = json["compute_type_of_symbol_kind_outcomes"]
            .as_array()
            .expect("compute_type_of_symbol_kind_outcomes is array");
        let ctos_kind_row = &ctos_kind[ctos_kind_idx];
        assert_eq!(ctos_kind_row["name"], "interface");
        assert!(
            ctos_kind_row["count"].as_u64().unwrap_or(0) > before_ctos_kind,
            "compute_type_of_symbol_kind_outcomes[interface] did not reflect the bump",
        );

        let ctos_fastpath = json["compute_type_of_symbol_interface_fastpath_outcomes"]
            .as_array()
            .expect("compute_type_of_symbol_interface_fastpath_outcomes is array");
        let ctos_fastpath_row = &ctos_fastpath[ctos_fastpath_idx];
        assert_eq!(ctos_fastpath_row["name"], "skip_all_three");
        assert!(
            ctos_fastpath_row["count"].as_u64().unwrap_or(0) > before_ctos_fastpath,
            "compute_type_of_symbol_interface_fastpath_outcomes[skip_all_three] did not reflect the bump",
        );

        let ctos_callsite = json["compute_type_of_symbol_interface_callsite_outcomes"]
            .as_array()
            .expect("compute_type_of_symbol_interface_callsite_outcomes is array");
        let ctos_callsite_row = &ctos_callsite[ctos_callsite_idx];
        assert_eq!(ctos_callsite_row["name"], "root");
        assert!(
            ctos_callsite_row["count"].as_u64().unwrap_or(0) > before_ctos_callsite,
            "compute_type_of_symbol_interface_callsite_outcomes[root] did not reflect the bump",
        );

        let ctos_simple_object = json["compute_type_of_symbol_interface_simple_object_outcomes"]
            .as_array()
            .expect("compute_type_of_symbol_interface_simple_object_outcomes is array");
        let ctos_simple_object_row = &ctos_simple_object[ctos_simple_object_outcome_idx];
        assert_eq!(ctos_simple_object_row["name"], "success");
        assert!(
            ctos_simple_object_row["count"].as_u64().unwrap_or(0)
                > before_ctos_simple_object_outcome,
            "compute_type_of_symbol_interface_simple_object_outcomes[success] did not reflect the bump",
        );

        let ctos_simple_object_non_primitive_annotation_kinds =
            json["compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kinds"]
                .as_array()
                .expect(
                    "compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kinds is array",
                );
        let ctos_simple_object_non_primitive_annotation_kind_row =
            &ctos_simple_object_non_primitive_annotation_kinds
                [ctos_simple_object_non_primitive_annotation_kind_idx];
        assert_eq!(
            ctos_simple_object_non_primitive_annotation_kind_row["name"],
            "type_reference"
        );
        assert!(
            ctos_simple_object_non_primitive_annotation_kind_row["count"]
                .as_u64()
                .unwrap_or(0)
                > before_ctos_simple_object_non_primitive_annotation_kind,
            "compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kinds[type_reference] did not reflect the bump",
        );

        let ctos_simple_object_type_reference_reject_outcomes = json
            ["compute_type_of_symbol_interface_simple_object_type_reference_reject_outcomes"]
            .as_array()
            .expect(
                "compute_type_of_symbol_interface_simple_object_type_reference_reject_outcomes is array",
            );
        let ctos_simple_object_type_reference_reject_outcome_row =
            &ctos_simple_object_type_reference_reject_outcomes
                [ctos_simple_object_type_reference_reject_outcome_idx];
        assert_eq!(
            ctos_simple_object_type_reference_reject_outcome_row["name"],
            "identifier_not_found_symbol"
        );
        assert!(
            ctos_simple_object_type_reference_reject_outcome_row["count"]
                .as_u64()
                .unwrap_or(0)
                > before_ctos_simple_object_type_reference_reject_outcome,
            "compute_type_of_symbol_interface_simple_object_type_reference_reject_outcomes[identifier_not_found_symbol] did not reflect the bump",
        );

        let ctos_simple_object_actual_lib_type_reference_outcomes = json
            ["compute_type_of_symbol_interface_simple_object_actual_lib_type_reference_outcomes"]
            .as_array()
            .expect(
                "compute_type_of_symbol_interface_simple_object_actual_lib_type_reference_outcomes is array",
            );
        let ctos_simple_object_actual_lib_type_reference_outcome_row =
            &ctos_simple_object_actual_lib_type_reference_outcomes
                [ctos_simple_object_actual_lib_type_reference_outcome_idx];
        assert_eq!(
            ctos_simple_object_actual_lib_type_reference_outcome_row["name"],
            "success"
        );
        assert!(
            ctos_simple_object_actual_lib_type_reference_outcome_row["count"]
                .as_u64()
                .unwrap_or(0)
                > before_ctos_simple_object_actual_lib_type_reference_outcome,
            "compute_type_of_symbol_interface_simple_object_actual_lib_type_reference_outcomes[success] did not reflect the bump",
        );

        assert!(
            json["checker"]["compute_type_of_symbol_interface_simple_object_fastpath_hits"]
                .as_u64()
                .unwrap_or(0)
                > before_ctos_simple_object_hits,
            "checker.compute_type_of_symbol_interface_simple_object_fastpath_hits did not reflect the bump",
        );
        assert!(
            json["checker"]["property_classification_calls"]
                .as_u64()
                .unwrap_or(0)
                > before_property_classification_calls,
            "checker.property_classification_calls did not reflect the bump",
        );
        assert!(
            json["checker"]["property_classification_string_fallback_source_lookups"]
                .as_u64()
                .unwrap_or(0)
                > before_property_classification_source_lookups,
            "checker.property_classification_string_fallback_source_lookups did not reflect the bump",
        );
        assert!(
            json["checker"]["property_classification_string_fallback_target_names"]
                .as_u64()
                .unwrap_or(0)
                > before_property_classification_target_names,
            "checker.property_classification_string_fallback_target_names did not reflect the bump",
        );
        assert!(
            json["checker"]["property_classification_string_fallback_target_types"]
                .as_u64()
                .unwrap_or(0)
                > before_property_classification_target_types,
            "checker.property_classification_string_fallback_target_types did not reflect the bump",
        );
    }
