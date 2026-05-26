mod json_tests {
    use super::*;

    #[test]
    fn schema_version_is_two() {
        // Bumping schema_version is a breaking change for the bench harness;
        // make the intent explicit.
        assert_eq!(PERF_COUNTER_SNAPSHOT_SCHEMA_VERSION, 2);
    }

    #[test]
    fn snapshot_serializes_with_expected_top_level_keys() {
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        for key in [
            "schema_version",
            "enabled",
            "mode",
            "wired",
            "delegate",
            "checker",
            "identity",
            "overlay",
            "resolver",
            "interner",
            "by_reason",
            "delegate_miss_classification",
            "delegate_declaration_file_miss_residues",
            "delegate_source_file_miss_residues",
            "alias_shortcut_outcomes",
            "compute_type_of_symbol_source_outcomes",
            "compute_type_of_symbol_kind_outcomes",
            "compute_type_of_symbol_interface_fastpath_outcomes",
            "compute_type_of_symbol_interface_callsite_outcomes",
            "compute_type_of_symbol_interface_simple_object_outcomes",
            "compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kinds",
            "compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues",
            "compute_type_of_symbol_interface_simple_object_declaration_provenance_residues",
            "compute_type_of_symbol_interface_simple_object_type_reference_reject_outcomes",
            "compute_type_of_symbol_interface_simple_object_type_reference_reject_residues",
            "direct_interface_lowering_outcomes",
            "direct_actual_lib_alias_body_outcomes",
            "direct_source_file_type_alias_lowering_outcomes",
            "direct_source_file_type_alias_body_rejection_kinds",
            "direct_source_file_type_alias_type_reference_rejection_kinds",
            "direct_source_file_type_alias_first_type_reference_rejection_kinds",
            "direct_source_file_type_alias_body_rejection_residues",
            "direct_actual_lib_intl_interface_outcomes",
            "cross_file_cache_miss_causes",
            "source_file_symbol_arena_cache_eligibility_outcomes",
        ] {
            assert!(json.get(key).is_some(), "missing top-level key: {key}");
        }
        assert_eq!(json["schema_version"], 2);
    }

    #[test]
    fn by_reason_array_has_one_row_per_reason_with_stable_field_shape() {
        // The T2.2 migration order (`PERFORMANCE_PLAN.md` §7) needs
        // per-`CheckerCreationReason` data to pick the next target.
        // `dump_string` exposes that breakdown as text; this snapshot
        // field exposes it as JSON. Lock both invariants:
        //   1. exactly `CHECKER_CREATION_REASON_COUNT` rows, in declaration order
        //      (so consumers can index by `REASON_NAMES`).
        //   2. each row has the documented field set; no rename, add, or
        //      remove can slip in without flipping this test.
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["by_reason"].as_array().expect("by_reason is array");
        assert_eq!(
            rows.len(),
            CHECKER_CREATION_REASON_COUNT,
            "by_reason length must match REASON_NAMES so consumers can index by reason"
        );
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(
                row["reason"], REASON_NAMES[i],
                "by_reason[{i}] is out of declaration order"
            );
            let obj = row.as_object().expect("row is object");
            let actual: std::collections::BTreeSet<&str> = obj.keys().map(String::as_str).collect();
            let expected: std::collections::BTreeSet<&str> = [
                "reason",
                "with_parent_cache_constructed",
                "overlay_copy_calls",
                "overlay_copy_entries",
                "overlay_copy_max_entries",
            ]
            .into_iter()
            .collect();
            assert_eq!(
                actual, expected,
                "by_reason row {i} (`{}`) drifted from the field lock",
                REASON_NAMES[i]
            );
        }
    }

    #[test]
    fn lock_wait_histogram_serialization_matches_feature_gate() {
        // The plan requires `null` for unwired buckets so `0` is unambiguous.
        // The lock-wait histogram is the only counter whose wiring is a
        // compile-time gate (`perf-counters-timing`) rather than a runtime
        // env var: builds with the feature off must serialize the histogram
        // as `null` and `wired.interner_lock_wait = false`; builds with the
        // feature on must serialize an array of bucket counts and
        // `interner_lock_wait = true`.
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        if cfg!(feature = "perf-counters-timing") {
            assert!(
                json["interner"]["lock_wait_histogram_ns"].is_array(),
                "histogram must be an array when feature is on, got: {}",
                json["interner"]["lock_wait_histogram_ns"]
            );
            assert_eq!(json["wired"]["interner_lock_wait"], true);
        } else {
            assert_eq!(
                json["interner"]["lock_wait_histogram_ns"],
                serde_json::Value::Null
            );
            assert_eq!(json["wired"]["interner_lock_wait"], false);
        }
    }

    #[test]
    fn wired_resolver_fs_probe_buckets_serialize_as_numbers() {
        // T0.3 follow-up: resolver `is_file`/`is_dir`/`read_dir` are wired
        // through `count_is_file`/`count_is_dir`/`count_read_dir` thin
        // wrappers in `crates/tsz-cli/src/driver/resolution.rs`. They
        // must serialize as numbers (zero is fine in this test process)
        // and the wired flag must agree.
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        assert!(
            json["resolver"]["is_file_calls"].is_number(),
            "is_file_calls should be a number once wired, got: {}",
            json["resolver"]["is_file_calls"]
        );
        assert!(json["resolver"]["is_dir_calls"].is_number());
        assert!(json["resolver"]["read_dir_calls"].is_number());
        assert_eq!(json["wired"]["resolver_fs_probes"], true);
    }

    #[test]
    fn wired_intern_call_buckets_serialize_as_numbers() {
        // T0.3 follow-up: intern_calls/hits/misses are now wired at the
        // solver intern site. They must surface as numbers (zero is fine
        // when the test process has not interned any user types) and the
        // wired flag must agree.
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        assert!(
            json["interner"]["intern_calls"].is_number(),
            "intern_calls should be a number once wired, got: {}",
            json["interner"]["intern_calls"]
        );
        assert!(json["interner"]["intern_hits"].is_number());
        assert!(json["interner"]["intern_misses"].is_number());
        assert_eq!(json["wired"]["interner_intern_calls"], true);
    }

    #[test]
    fn file_session_resets_serializes_as_number() {
        // The T2.1 file-session reset counter rides inside the existing
        // `checker_construction` wired group, so adding it must not
        // require a new `wired` flag — but it must surface as a number
        // (not `null`) so attribution tooling can compare it against
        // `state_constructed` to detect reuse-vs-construct directly.
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        assert!(
            json["checker"]["file_session_resets"].is_number(),
            "file_session_resets should serialize as a number, got: {}",
            json["checker"]["file_session_resets"]
        );
        assert_eq!(json["wired"]["checker_construction"], true);
    }

    #[test]
    fn wired_keys_match_snapshot_struct_fields() {
        // If a future PR adds a wired flag, it must also surface in the
        // top-level snapshot, and vice versa. This keeps the schema and
        // the wired map honest.
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let wired = json["wired"].as_object().expect("wired is an object");
        // Cross-check: keys are stable across runs.
        let expected_keys: std::collections::BTreeSet<&str> = [
            "delegate_cross_arena",
            "checker_construction",
            "property_classification",
            "overlay_copy",
            "interner_intern_calls",
            "interner_per_kind",
            "interner_lock_wait",
            "resolver_lookup",
            "resolver_fs_probes",
            "compute_type_of_symbol",
            "stable_identity",
        ]
        .into_iter()
        .collect();
        let actual_keys: std::collections::BTreeSet<&str> =
            wired.keys().map(String::as_str).collect();
        assert_eq!(actual_keys, expected_keys);
    }

    /// Lock the field shape of each top-level snapshot section so an
    /// accidental rename, addition, or removal is caught at test time
    /// instead of by a downstream bench harness parsing the JSON.
    /// `interner` is excluded because that section's field set is in
    /// flight (e.g. #5128 adds `callable_shape_intern_calls`); the
    /// invariant for it is owned by the JSON round-trip test plus the
    /// counter-specific `wired_*_serialize_as_numbers` cases.
    fn assert_section_keys(json: &serde_json::Value, section: &str, expected: &[&str]) {
        let obj = json[section]
            .as_object()
            .unwrap_or_else(|| panic!("section `{section}` is not a JSON object"));
        let actual: std::collections::BTreeSet<&str> = obj.keys().map(String::as_str).collect();
        let expected: std::collections::BTreeSet<&str> = expected.iter().copied().collect();
        assert_eq!(
            actual, expected,
            "section `{section}` field set drifted from the lock"
        );
    }

    #[test]
    fn delegate_section_field_shape() {
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        assert_section_keys(
            &json,
            "delegate",
            &[
                "calls",
                "cache_hits_lib",
                "cache_hits_cross_file",
                "misses",
                "max_recursion_depth",
                "cross_file_type_params_cache_hits",
                "cross_file_type_params_cache_misses",
            ],
        );
    }

    #[test]
    fn checker_section_field_shape() {
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        assert_section_keys(
            &json,
            "checker",
            &[
                "state_constructed",
                "with_parent_cache_constructed",
                "file_session_resets",
                "compute_type_of_symbol_calls",
                "compute_type_of_symbol_cache_hits",
                "compute_type_of_symbol_interface_simple_object_fastpath_hits",
                "property_classification_calls",
                "property_classification_string_fallback_source_lookups",
                "property_classification_string_fallback_target_names",
                "property_classification_string_fallback_target_types",
            ],
        );
    }

    #[test]
    fn identity_section_field_shape() {
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        assert_section_keys(
            &json,
            "identity",
            &["type_environment_raw_symbol_lazy_fallbacks"],
        );
        assert_eq!(json["wired"]["stable_identity"], true);
    }

    #[test]
    fn overlay_section_field_shape() {
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        assert_section_keys(
            &json,
            "overlay",
            &[
                "copy_calls",
                "entries_total",
                "entries_max",
                "len_ge_1k",
                "len_ge_10k",
                "len_ge_100k",
                "len_ge_1m",
            ],
        );
    }

    #[test]
    fn resolver_section_field_shape() {
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        assert_section_keys(
            &json,
            "resolver",
            &[
                "lookup_calls",
                "is_file_calls",
                "is_dir_calls",
                "read_dir_calls",
                "package_json_reads",
                "candidate_paths_total",
            ],
        );
    }

    #[test]
    fn delegate_miss_classification_field_shape() {
        // Lock the top-level field set of `delegate_miss_classification`
        // so a later rename / addition / removal is caught here instead
        // of by the bench harness silently swallowing a missing key.
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let obj = json["delegate_miss_classification"]
            .as_object()
            .expect("delegate_miss_classification is an object");
        let actual: std::collections::BTreeSet<&str> = obj.keys().map(String::as_str).collect();
        let expected: std::collections::BTreeSet<&str> = [
            "by_source",
            "by_kind",
            "target_declaration_files",
            "target_source_files",
        ]
        .into_iter()
        .collect();
        assert_eq!(
            actual, expected,
            "`delegate_miss_classification` field set drifted from the lock"
        );
    }

    #[test]
    fn delegate_miss_classification_by_source_locks_to_names_array() {
        // Each row in `by_source` is keyed by index against
        // `CROSS_ARENA_SYMBOL_MISS_SOURCE_NAMES`. A future PR that adds
        // a variant must extend both the enum and the names array; this
        // test would surface a length mismatch immediately.
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["delegate_miss_classification"]["by_source"]
            .as_array()
            .expect("by_source is array");
        assert_eq!(
            rows.len(),
            CROSS_ARENA_SYMBOL_MISS_SOURCE_COUNT,
            "by_source length must match CROSS_ARENA_SYMBOL_MISS_SOURCE_NAMES",
        );
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(
                row["name"], CROSS_ARENA_SYMBOL_MISS_SOURCE_NAMES[i],
                "by_source[{i}] is out of declaration order",
            );
            assert!(
                row["count"].is_u64(),
                "by_source[{i}].count should be a number",
            );
            let obj = row.as_object().expect("row is object");
            let actual: std::collections::BTreeSet<&str> = obj.keys().map(String::as_str).collect();
            let expected: std::collections::BTreeSet<&str> =
                ["name", "count"].into_iter().collect();
            assert_eq!(actual, expected, "by_source[{i}] field shape drifted");
        }
    }

    #[test]
    fn delegate_miss_classification_by_kind_locks_to_names_array() {
        // Mirror of the `by_source` invariant for the symbol-kind bucket.
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["delegate_miss_classification"]["by_kind"]
            .as_array()
            .expect("by_kind is array");
        assert_eq!(
            rows.len(),
            CROSS_ARENA_SYMBOL_MISS_KIND_COUNT,
            "by_kind length must match CROSS_ARENA_SYMBOL_MISS_KIND_NAMES",
        );
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(
                row["name"], CROSS_ARENA_SYMBOL_MISS_KIND_NAMES[i],
                "by_kind[{i}] is out of declaration order",
            );
            assert!(
                row["count"].is_u64(),
                "by_kind[{i}].count should be a number"
            );
        }
    }

    #[test]
    fn delegate_declaration_file_miss_residues_lock_field_shape() {
        let unique_name = format!("__test_decl_residue_{}__", std::process::id());
        {
            let mut rows = delegate_declaration_file_miss_residues()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            rows.push(DelegateDeclarationFileMissResidue {
                name: unique_name.clone(),
                kind: "interface",
                source: "symbol_arenas",
                target_file: Some("lib.test.d.ts".to_string()),
                count: 7,
            });
        }

        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["delegate_declaration_file_miss_residues"]
            .as_array()
            .expect("delegate_declaration_file_miss_residues is array");
        let row = rows
            .iter()
            .find(|row| row["name"] == unique_name)
            .expect("test residue row is present");
        let obj = row.as_object().expect("row is object");
        let actual: std::collections::BTreeSet<&str> = obj.keys().map(String::as_str).collect();
        let expected: std::collections::BTreeSet<&str> =
            ["name", "kind", "source", "target_file", "count"]
                .into_iter()
                .collect();
        assert_eq!(
            actual, expected,
            "delegate_declaration_file_miss_residues row field shape drifted",
        );
        assert_eq!(row["kind"], "interface");
        assert_eq!(row["source"], "symbol_arenas");
        assert_eq!(row["target_file"], "lib.test.d.ts");
        assert_eq!(row["count"], 7);
    }

    #[test]
    fn delegate_source_file_miss_residues_lock_field_shape() {
        let unique_name = format!("__test_source_residue_{}__", std::process::id());
        {
            let mut rows = delegate_source_file_miss_residues()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            rows.push(DelegateSourceFileMissResidue {
                name: unique_name.clone(),
                kind: "type_alias",
                source: "symbol_arenas",
                target_file: Some("mapped-types.ts".to_string()),
                count: 11,
            });
        }

        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["delegate_source_file_miss_residues"]
            .as_array()
            .expect("delegate_source_file_miss_residues is array");
        let row = rows
            .iter()
            .find(|row| row["name"] == unique_name)
            .expect("test residue row is present");
        let obj = row.as_object().expect("row is object");
        let actual: std::collections::BTreeSet<&str> = obj.keys().map(String::as_str).collect();
        let expected: std::collections::BTreeSet<&str> =
            ["name", "kind", "source", "target_file", "count"]
                .into_iter()
                .collect();
        assert_eq!(
            actual, expected,
            "delegate_source_file_miss_residues row field shape drifted",
        );
        assert_eq!(row["kind"], "type_alias");
        assert_eq!(row["source"], "symbol_arenas");
        assert_eq!(row["target_file"], "mapped-types.ts");
        assert_eq!(row["count"], 11);
    }

    #[test]
    fn direct_source_file_type_alias_body_rejection_residues_lock_field_shape() {
        let unique_name = format!("__test_source_alias_body_residue_{}__", std::process::id());
        {
            let mut rows = direct_source_file_type_alias_body_rejection_residues()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            rows.push(DirectSourceFileTypeAliasBodyRejectionResidue {
                name: unique_name.clone(),
                body_kind: "mapped_type",
                first_type_reference_kind: Some("local_type_parameter"),
                first_type_reference_name: Some("T".to_string()),
                first_non_lowerable_type_reference_kind: Some("unresolved_identifier"),
                first_non_lowerable_type_reference_name: Some("Missing".to_string()),
                first_non_lowerable_leaf_type_reference_kind: Some("local_alias_symbol"),
                first_non_lowerable_leaf_type_reference_name: Some("Leaf".to_string()),
                target_file: Some("mapped-types.ts".to_string()),
                count: 13,
            });
        }

        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["direct_source_file_type_alias_body_rejection_residues"]
            .as_array()
            .expect("direct_source_file_type_alias_body_rejection_residues is array");
        let row = rows
            .iter()
            .find(|row| row["name"] == unique_name)
            .expect("test residue row is present");
        let obj = row.as_object().expect("row is object");
        let actual: std::collections::BTreeSet<&str> = obj.keys().map(String::as_str).collect();
        let expected: std::collections::BTreeSet<&str> = [
            "name",
            "body_kind",
            "first_type_reference_kind",
            "first_type_reference_name",
            "first_non_lowerable_type_reference_kind",
            "first_non_lowerable_type_reference_name",
            "first_non_lowerable_leaf_type_reference_kind",
            "first_non_lowerable_leaf_type_reference_name",
            "target_file",
            "count",
        ]
        .into_iter()
        .collect();
        assert_eq!(
            actual, expected,
            "direct_source_file_type_alias_body_rejection_residues row field shape drifted",
        );
        assert_eq!(row["body_kind"], "mapped_type");
        assert_eq!(row["first_type_reference_kind"], "local_type_parameter");
        assert_eq!(row["first_type_reference_name"], "T");
        assert_eq!(
            row["first_non_lowerable_type_reference_kind"],
            "unresolved_identifier",
        );
        assert_eq!(row["first_non_lowerable_type_reference_name"], "Missing");
        assert_eq!(
            row["first_non_lowerable_leaf_type_reference_kind"],
            "local_alias_symbol",
        );
        assert_eq!(row["first_non_lowerable_leaf_type_reference_name"], "Leaf");
        assert_eq!(row["target_file"], "mapped-types.ts");
        assert_eq!(row["count"], 13);
    }

    #[test]
    fn alias_shortcut_outcomes_locks_to_names_array() {
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["alias_shortcut_outcomes"]
            .as_array()
            .expect("alias_shortcut_outcomes is array");
        assert_eq!(
            rows.len(),
            CROSS_ARENA_ALIAS_SHORTCUT_OUTCOME_COUNT,
            "alias_shortcut_outcomes length must match CROSS_ARENA_ALIAS_SHORTCUT_OUTCOME_NAMES",
        );
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(
                row["name"], CROSS_ARENA_ALIAS_SHORTCUT_OUTCOME_NAMES[i],
                "alias_shortcut_outcomes[{i}] is out of declaration order",
            );
            assert!(
                row["count"].is_u64(),
                "alias_shortcut_outcomes[{i}].count should be a number",
            );
        }
    }

    #[test]
    fn compute_type_of_symbol_source_outcomes_locks_to_names_array() {
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["compute_type_of_symbol_source_outcomes"]
            .as_array()
            .expect("compute_type_of_symbol_source_outcomes is array");
        assert_eq!(
            rows.len(),
            COMPUTE_TYPE_OF_SYMBOL_SOURCE_OUTCOME_COUNT,
            "compute_type_of_symbol_source_outcomes length must match \
             COMPUTE_TYPE_OF_SYMBOL_SOURCE_OUTCOME_NAMES",
        );
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(
                row["name"], COMPUTE_TYPE_OF_SYMBOL_SOURCE_OUTCOME_NAMES[i],
                "compute_type_of_symbol_source_outcomes[{i}] is out of declaration order",
            );
            assert!(
                row["count"].is_u64(),
                "compute_type_of_symbol_source_outcomes[{i}].count should be a number",
            );
        }
    }

    #[test]
    fn compute_type_of_symbol_kind_outcomes_locks_to_names_array() {
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["compute_type_of_symbol_kind_outcomes"]
            .as_array()
            .expect("compute_type_of_symbol_kind_outcomes is array");
        assert_eq!(
            rows.len(),
            COMPUTE_TYPE_OF_SYMBOL_KIND_OUTCOME_COUNT,
            "compute_type_of_symbol_kind_outcomes length must match \
             COMPUTE_TYPE_OF_SYMBOL_KIND_OUTCOME_NAMES",
        );
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(
                row["name"], COMPUTE_TYPE_OF_SYMBOL_KIND_OUTCOME_NAMES[i],
                "compute_type_of_symbol_kind_outcomes[{i}] is out of declaration order",
            );
            assert!(
                row["count"].is_u64(),
                "compute_type_of_symbol_kind_outcomes[{i}].count should be a number",
            );
        }
    }

    #[test]
    fn compute_type_of_symbol_interface_fastpath_outcomes_locks_to_names_array() {
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["compute_type_of_symbol_interface_fastpath_outcomes"]
            .as_array()
            .expect("compute_type_of_symbol_interface_fastpath_outcomes is array");
        assert_eq!(
            rows.len(),
            COMPUTE_TYPE_OF_SYMBOL_INTERFACE_FASTPATH_OUTCOME_COUNT,
            "compute_type_of_symbol_interface_fastpath_outcomes length must match \
             COMPUTE_TYPE_OF_SYMBOL_INTERFACE_FASTPATH_OUTCOME_NAMES",
        );
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(
                row["name"], COMPUTE_TYPE_OF_SYMBOL_INTERFACE_FASTPATH_OUTCOME_NAMES[i],
                "compute_type_of_symbol_interface_fastpath_outcomes[{i}] is out of declaration order",
            );
            assert!(
                row["count"].is_u64(),
                "compute_type_of_symbol_interface_fastpath_outcomes[{i}].count should be a number",
            );
        }
    }

    #[test]
    fn compute_type_of_symbol_interface_callsite_outcomes_locks_to_names_array() {
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["compute_type_of_symbol_interface_callsite_outcomes"]
            .as_array()
            .expect("compute_type_of_symbol_interface_callsite_outcomes is array");
        assert_eq!(
            rows.len(),
            COMPUTE_TYPE_OF_SYMBOL_INTERFACE_CALLSITE_OUTCOME_COUNT,
            "compute_type_of_symbol_interface_callsite_outcomes length must match \
             COMPUTE_TYPE_OF_SYMBOL_INTERFACE_CALLSITE_OUTCOME_NAMES",
        );
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(
                row["name"], COMPUTE_TYPE_OF_SYMBOL_INTERFACE_CALLSITE_OUTCOME_NAMES[i],
                "compute_type_of_symbol_interface_callsite_outcomes[{i}] is out of declaration order",
            );
            assert!(
                row["count"].is_u64(),
                "compute_type_of_symbol_interface_callsite_outcomes[{i}].count should be a number",
            );
        }
    }

    #[test]
    fn compute_type_of_symbol_interface_simple_object_outcomes_locks_to_names_array() {
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["compute_type_of_symbol_interface_simple_object_outcomes"]
            .as_array()
            .expect("compute_type_of_symbol_interface_simple_object_outcomes is array");
        assert_eq!(
            rows.len(),
            COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_OUTCOME_COUNT,
            "compute_type_of_symbol_interface_simple_object_outcomes length must match \
             COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_OUTCOME_NAMES",
        );
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(
                row["name"], COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_OUTCOME_NAMES[i],
                "compute_type_of_symbol_interface_simple_object_outcomes[{i}] is out of declaration order",
            );
            assert!(
                row["count"].is_u64(),
                "compute_type_of_symbol_interface_simple_object_outcomes[{i}].count should be a number",
            );
        }
    }

    #[test]
    fn compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kinds_locks_to_names_array(
    ) {
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kinds"]
            .as_array()
            .expect("compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kinds is array");
        assert_eq!(
            rows.len(),
            COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_KIND_COUNT,
            "compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kinds length must match \
             COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_KIND_NAMES",
        );
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(
                row["name"],
                COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_KIND_NAMES
                    [i],
                "compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kinds[{i}] is out of declaration order",
            );
            assert!(
                row["count"].is_u64(),
                "compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kinds[{i}].count should be a number",
            );
        }
    }

    #[test]
    fn compute_type_of_symbol_interface_simple_object_type_reference_reject_outcomes_locks_to_names_array(
    ) {
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["compute_type_of_symbol_interface_simple_object_type_reference_reject_outcomes"]
            .as_array()
            .expect(
                "compute_type_of_symbol_interface_simple_object_type_reference_reject_outcomes is array",
            );
        assert_eq!(
            rows.len(),
            COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_OUTCOME_COUNT,
            "compute_type_of_symbol_interface_simple_object_type_reference_reject_outcomes length must match \
             COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_OUTCOME_NAMES",
        );
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(
                row["name"],
                COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_OUTCOME_NAMES
                    [i],
                "compute_type_of_symbol_interface_simple_object_type_reference_reject_outcomes[{i}] is out of declaration order",
            );
            assert!(
                row["count"].is_u64(),
                "compute_type_of_symbol_interface_simple_object_type_reference_reject_outcomes[{i}].count should be a number",
            );
        }
    }

    #[test]
    fn compute_type_of_symbol_interface_simple_object_type_reference_reject_residues_lock_field_shape(
    ) {
        let unique_name = format!(
            "__test_simple_object_type_ref_residue_{}__",
            std::process::id()
        );
        {
            let mut rows =
                compute_type_of_symbol_interface_simple_object_type_reference_reject_residues()
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            rows.push(
                ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectResidue {
                    name: unique_name.clone(),
                    outcome: "identifier_not_found_symbol",
                    count: 11,
                },
            );
        }

        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json
            ["compute_type_of_symbol_interface_simple_object_type_reference_reject_residues"]
            .as_array()
            .expect(
                "compute_type_of_symbol_interface_simple_object_type_reference_reject_residues is array",
            );
        let row = rows
            .iter()
            .find(|row| row["name"] == unique_name)
            .expect("test residue row is present");
        let obj = row.as_object().expect("row is object");
        let actual: std::collections::BTreeSet<&str> = obj.keys().map(String::as_str).collect();
        let expected: std::collections::BTreeSet<&str> =
            ["name", "outcome", "count"].into_iter().collect();
        assert_eq!(
            actual, expected,
            "compute_type_of_symbol_interface_simple_object_type_reference_reject_residues row field shape drifted",
        );
        assert_eq!(row["outcome"], "identifier_not_found_symbol");
        assert_eq!(row["count"], 11);
    }

    #[test]
    fn compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues_lock_field_shape(
    ) {
        let unique_interface = format!(
            "__test_simple_object_non_primitive_interface_{}__",
            std::process::id()
        );
        let unique_property = format!(
            "__test_simple_object_non_primitive_property_{}__",
            std::process::id()
        );
        {
            let mut rows =
                compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues()
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            rows.push(
                ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationResidue {
                    kind: "union_or_intersection",
                    interface: Some(unique_interface.clone()),
                    property: Some(unique_property.clone()),
                    count: 7,
                },
            );
        }

        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json
            ["compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues"]
            .as_array()
            .expect(
                "compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues is array",
            );
        let row = rows
            .iter()
            .find(|row| row["interface"] == unique_interface)
            .expect("test residue row is present");
        let obj = row.as_object().expect("row is object");
        let actual: std::collections::BTreeSet<&str> = obj.keys().map(String::as_str).collect();
        let expected: std::collections::BTreeSet<&str> = ["kind", "interface", "property", "count"]
            .into_iter()
            .collect();
        assert_eq!(
            actual, expected,
            "compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues row field shape drifted",
        );
        assert_eq!(row["kind"], "union_or_intersection");
        assert_eq!(row["property"], unique_property);
        assert_eq!(row["count"], 7);
    }

    #[test]
    fn compute_type_of_symbol_interface_simple_object_declaration_provenance_residues_lock_field_shape(
    ) {
        let unique_symbol = format!(
            "__test_simple_object_declaration_provenance_{}__",
            std::process::id()
        );
        {
            let mut rows =
                compute_type_of_symbol_interface_simple_object_declaration_provenance_residues()
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            rows.push(
                ComputeTypeOfSymbolInterfaceSimpleObjectDeclarationProvenanceResidue {
                    outcome: "reject_out_of_arena_decl",
                    symbol: Some(unique_symbol.clone()),
                    declaration_count: 3,
                    count: 5,
                },
            );
        }

        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json
            ["compute_type_of_symbol_interface_simple_object_declaration_provenance_residues"]
            .as_array()
            .expect(
                "compute_type_of_symbol_interface_simple_object_declaration_provenance_residues is array",
            );
        let row = rows
            .iter()
            .find(|row| row["symbol"] == unique_symbol)
            .expect("test residue row is present");
        let obj = row.as_object().expect("row is object");
        let actual: std::collections::BTreeSet<&str> = obj.keys().map(String::as_str).collect();
        let expected: std::collections::BTreeSet<&str> =
            ["outcome", "symbol", "declaration_count", "count"]
                .into_iter()
                .collect();
        assert_eq!(
            actual, expected,
            "compute_type_of_symbol_interface_simple_object_declaration_provenance_residues row field shape drifted",
        );
        assert_eq!(row["outcome"], "reject_out_of_arena_decl");
        assert_eq!(row["declaration_count"], 3);
        assert_eq!(row["count"], 5);
    }

    #[test]
    fn direct_interface_lowering_outcomes_locks_to_names_array() {
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["direct_interface_lowering_outcomes"]
            .as_array()
            .expect("direct_interface_lowering_outcomes is array");
        assert_eq!(
            rows.len(),
            DIRECT_CROSS_FILE_INTERFACE_LOWERING_OUTCOME_COUNT,
            "direct_interface_lowering_outcomes length must match \
             DIRECT_CROSS_FILE_INTERFACE_LOWERING_OUTCOME_NAMES",
        );
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(
                row["name"], DIRECT_CROSS_FILE_INTERFACE_LOWERING_OUTCOME_NAMES[i],
                "direct_interface_lowering_outcomes[{i}] is out of declaration order",
            );
            assert!(
                row["count"].is_u64(),
                "direct_interface_lowering_outcomes[{i}].count should be a number",
            );
        }
    }

    #[test]
    fn direct_actual_lib_alias_body_outcomes_locks_to_names_array() {
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["direct_actual_lib_alias_body_outcomes"]
            .as_array()
            .expect("direct_actual_lib_alias_body_outcomes is array");
        assert_eq!(
            rows.len(),
            DIRECT_ACTUAL_LIB_ALIAS_BODY_OUTCOME_COUNT,
            "direct_actual_lib_alias_body_outcomes length must match \
             DIRECT_ACTUAL_LIB_ALIAS_BODY_OUTCOME_NAMES",
        );
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(
                row["name"], DIRECT_ACTUAL_LIB_ALIAS_BODY_OUTCOME_NAMES[i],
                "direct_actual_lib_alias_body_outcomes[{i}] is out of declaration order",
            );
            assert!(
                row["count"].is_u64(),
                "direct_actual_lib_alias_body_outcomes[{i}].count should be a number",
            );
        }
    }

    #[test]
    fn direct_actual_lib_intl_interface_outcomes_locks_to_names_array() {
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["direct_actual_lib_intl_interface_outcomes"]
            .as_array()
            .expect("direct_actual_lib_intl_interface_outcomes is array");
        assert_eq!(
            rows.len(),
            DIRECT_ACTUAL_LIB_INTL_INTERFACE_OUTCOME_COUNT,
            "direct_actual_lib_intl_interface_outcomes length must match \
             DIRECT_ACTUAL_LIB_INTL_INTERFACE_OUTCOME_NAMES",
        );
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(
                row["name"], DIRECT_ACTUAL_LIB_INTL_INTERFACE_OUTCOME_NAMES[i],
                "direct_actual_lib_intl_interface_outcomes[{i}] is out of declaration order",
            );
            assert!(
                row["count"].is_u64(),
                "direct_actual_lib_intl_interface_outcomes[{i}].count should be a number",
            );
        }
    }

    #[test]
    fn cross_file_cache_miss_causes_locks_to_names_array() {
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["cross_file_cache_miss_causes"]
            .as_array()
            .expect("cross_file_cache_miss_causes is array");
        assert_eq!(
            rows.len(),
            CROSS_FILE_CACHE_MISS_CAUSE_COUNT,
            "cross_file_cache_miss_causes length must match CROSS_FILE_CACHE_MISS_CAUSE_NAMES",
        );
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(
                row["name"], CROSS_FILE_CACHE_MISS_CAUSE_NAMES[i],
                "cross_file_cache_miss_causes[{i}] is out of declaration order",
            );
            assert!(
                row["count"].is_u64(),
                "cross_file_cache_miss_causes[{i}].count should be a number",
            );
            let obj = row.as_object().expect("row is object");
            let actual: std::collections::BTreeSet<&str> = obj.keys().map(String::as_str).collect();
            let expected: std::collections::BTreeSet<&str> =
                ["name", "count"].into_iter().collect();
            assert_eq!(
                actual, expected,
                "cross_file_cache_miss_causes[{i}] field shape drifted",
            );
        }
    }

    #[test]
    fn source_file_symbol_arena_cache_eligibility_locks_to_names_array() {
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["source_file_symbol_arena_cache_eligibility_outcomes"]
            .as_array()
            .expect("source_file_symbol_arena_cache_eligibility_outcomes is array");
        assert_eq!(
            rows.len(),
            SOURCE_FILE_SYMBOL_ARENA_CACHE_ELIGIBILITY_OUTCOME_COUNT,
            "source_file_symbol_arena_cache_eligibility_outcomes length must match \
             SOURCE_FILE_SYMBOL_ARENA_CACHE_ELIGIBILITY_OUTCOME_NAMES",
        );
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(
                row["name"], SOURCE_FILE_SYMBOL_ARENA_CACHE_ELIGIBILITY_OUTCOME_NAMES[i],
                "source_file_symbol_arena_cache_eligibility_outcomes[{i}] is out of declaration order",
            );
            assert!(
                row["count"].is_u64(),
                "source_file_symbol_arena_cache_eligibility_outcomes[{i}].count should be a number",
            );
            let obj = row.as_object().expect("row is object");
            let actual: std::collections::BTreeSet<&str> = obj.keys().map(String::as_str).collect();
            let expected: std::collections::BTreeSet<&str> =
                ["name", "count"].into_iter().collect();
            assert_eq!(
                actual, expected,
                "source_file_symbol_arena_cache_eligibility_outcomes[{i}] field shape drifted",
            );
        }
    }

    #[test]
    fn cross_file_cache_miss_cause_atomic_propagates_into_snapshot() {
        // Mirrors `classification_arrays_propagate_atomic_state_into_snapshot`:
        // drive the underlying atomic directly to prove the snapshot reads
        // it back at the right index. `record_cross_file_cache_miss_cause`
        // short-circuits on `enabled_fast() == false`, which is the default
        // in `cargo nextest`, so the helper is unsuitable here.
        let c = counters();

        let gate_idx = CrossFileCacheMissCause::GateOff.as_index();
        let bucket_idx = CrossFileCacheMissCause::BucketEmpty.as_index();
        let sentinel_idx = CrossFileCacheMissCause::SentinelErrorUnknown.as_index();
        let not_interned_idx = CrossFileCacheMissCause::TypeIdNotInterned.as_index();

        let before_gate = c.cross_file_cache_miss_cause[gate_idx].load(Ordering::Relaxed);
        let before_bucket = c.cross_file_cache_miss_cause[bucket_idx].load(Ordering::Relaxed);
        let before_sentinel = c.cross_file_cache_miss_cause[sentinel_idx].load(Ordering::Relaxed);
        let before_not_interned =
            c.cross_file_cache_miss_cause[not_interned_idx].load(Ordering::Relaxed);

        c.cross_file_cache_miss_cause[gate_idx].fetch_add(1, Ordering::Relaxed);
        c.cross_file_cache_miss_cause[bucket_idx].fetch_add(2, Ordering::Relaxed);
        c.cross_file_cache_miss_cause[sentinel_idx].fetch_add(3, Ordering::Relaxed);
        c.cross_file_cache_miss_cause[not_interned_idx].fetch_add(4, Ordering::Relaxed);

        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["cross_file_cache_miss_causes"]
            .as_array()
            .expect("cross_file_cache_miss_causes is array");

        let read = |idx: usize| rows[idx]["count"].as_u64().unwrap_or(0);

        assert_eq!(rows[gate_idx]["name"], "gate_off");
        assert!(
            read(gate_idx) > before_gate,
            "gate_off bump not visible (before={before_gate}, after={})",
            read(gate_idx),
        );

        assert_eq!(rows[bucket_idx]["name"], "bucket_empty");
        assert!(
            read(bucket_idx) >= before_bucket.saturating_add(2),
            "bucket_empty bump not visible (before={before_bucket}, after={})",
            read(bucket_idx),
        );

        assert_eq!(rows[sentinel_idx]["name"], "sentinel_error_unknown");
        assert!(
            read(sentinel_idx) >= before_sentinel.saturating_add(3),
            "sentinel_error_unknown bump not visible (before={before_sentinel}, after={})",
            read(sentinel_idx),
        );

        assert_eq!(rows[not_interned_idx]["name"], "type_id_not_interned");
        assert!(
            read(not_interned_idx) >= before_not_interned.saturating_add(4),
            "type_id_not_interned bump not visible (before={before_not_interned}, after={})",
            read(not_interned_idx),
        );
    }

    #[test]
    fn source_file_symbol_arena_cache_eligibility_atomic_propagates_into_snapshot() {
        // The public recorder is gated on `TSZ_PERF_COUNTERS`; drive the
        // atomics directly so this unit test is independent of process env.
        let c = counters();

        let cacheable_idx = SourceFileSymbolArenaCacheEligibilityOutcome::Cacheable.as_index();
        let variable_idx =
            SourceFileSymbolArenaCacheEligibilityOutcome::NotClassOrInterface.as_index();
        let mismatch_idx =
            SourceFileSymbolArenaCacheEligibilityOutcome::DeclarationArenaMismatch.as_index();

        let before_cacheable = c.source_file_symbol_arena_cache_eligibility_outcome[cacheable_idx]
            .load(Ordering::Relaxed);
        let before_variable = c.source_file_symbol_arena_cache_eligibility_outcome[variable_idx]
            .load(Ordering::Relaxed);
        let before_mismatch = c.source_file_symbol_arena_cache_eligibility_outcome[mismatch_idx]
            .load(Ordering::Relaxed);

        c.source_file_symbol_arena_cache_eligibility_outcome[cacheable_idx]
            .fetch_add(1, Ordering::Relaxed);
        c.source_file_symbol_arena_cache_eligibility_outcome[variable_idx]
            .fetch_add(2, Ordering::Relaxed);
        c.source_file_symbol_arena_cache_eligibility_outcome[mismatch_idx]
            .fetch_add(3, Ordering::Relaxed);

        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["source_file_symbol_arena_cache_eligibility_outcomes"]
            .as_array()
            .expect("source_file_symbol_arena_cache_eligibility_outcomes is array");
        let read = |idx: usize| rows[idx]["count"].as_u64().unwrap_or(0);

        assert_eq!(rows[cacheable_idx]["name"], "cacheable");
        assert!(
            read(cacheable_idx) > before_cacheable,
            "cacheable bump not visible (before={before_cacheable}, after={})",
            read(cacheable_idx),
        );

        assert_eq!(rows[variable_idx]["name"], "not_class_or_interface");
        assert!(
            read(variable_idx) >= before_variable.saturating_add(2),
            "not_class_or_interface bump not visible (before={before_variable}, after={})",
            read(variable_idx),
        );

        assert_eq!(rows[mismatch_idx]["name"], "declaration_arena_mismatch");
        assert!(
            read(mismatch_idx) >= before_mismatch.saturating_add(3),
            "declaration_arena_mismatch bump not visible (before={before_mismatch}, after={})",
            read(mismatch_idx),
        );
    }

    #[test]
    fn direct_source_file_type_alias_lowering_atomic_propagates_into_snapshot() {
        // The public recorder is gated on `TSZ_PERF_COUNTERS`; drive the
        // atomics directly so this unit test is independent of process env.
        let c = counters();

        let success_idx = DirectSourceFileTypeAliasLoweringOutcome::Success.as_index();
        let body_idx = DirectSourceFileTypeAliasLoweringOutcome::BodyNotDirectLowerable.as_index();
        let query_idx =
            DirectSourceFileTypeAliasLoweringOutcome::TypeQueryOrSelfReference.as_index();

        let before_success =
            c.direct_source_file_type_alias_lowering_outcome[success_idx].load(Ordering::Relaxed);
        let before_body =
            c.direct_source_file_type_alias_lowering_outcome[body_idx].load(Ordering::Relaxed);
        let before_query =
            c.direct_source_file_type_alias_lowering_outcome[query_idx].load(Ordering::Relaxed);

        c.direct_source_file_type_alias_lowering_outcome[success_idx]
            .fetch_add(1, Ordering::Relaxed);
        c.direct_source_file_type_alias_lowering_outcome[body_idx].fetch_add(2, Ordering::Relaxed);
        c.direct_source_file_type_alias_lowering_outcome[query_idx].fetch_add(3, Ordering::Relaxed);

        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["direct_source_file_type_alias_lowering_outcomes"]
            .as_array()
            .expect("direct_source_file_type_alias_lowering_outcomes is array");
        let read = |idx: usize| rows[idx]["count"].as_u64().unwrap_or(0);

        assert_eq!(rows[success_idx]["name"], "success");
        assert!(
            read(success_idx) > before_success,
            "success bump not visible (before={before_success}, after={})",
            read(success_idx),
        );

        assert_eq!(rows[body_idx]["name"], "body_not_direct_lowerable");
        assert!(
            read(body_idx) >= before_body.saturating_add(2),
            "body_not_direct_lowerable bump not visible (before={before_body}, after={})",
            read(body_idx),
        );

        assert_eq!(rows[query_idx]["name"], "type_query_or_self_reference");
        assert!(
            read(query_idx) >= before_query.saturating_add(3),
            "type_query_or_self_reference bump not visible (before={before_query}, after={})",
            read(query_idx),
        );
    }

    #[test]
    fn direct_source_file_type_alias_body_rejection_kind_atomic_propagates_into_snapshot() {
        // The public recorder is gated on `TSZ_PERF_COUNTERS`; drive the
        // atomics directly so this unit test is independent of process env.
        let c = counters();

        let type_ref_idx = DirectSourceFileTypeAliasBodyRejectionKind::TypeReference.as_index();
        let conditional_idx =
            DirectSourceFileTypeAliasBodyRejectionKind::ConditionalType.as_index();
        let mapped_idx = DirectSourceFileTypeAliasBodyRejectionKind::MappedType.as_index();

        let before_type_ref = c.direct_source_file_type_alias_body_rejection_kind[type_ref_idx]
            .load(Ordering::Relaxed);
        let before_conditional = c.direct_source_file_type_alias_body_rejection_kind
            [conditional_idx]
            .load(Ordering::Relaxed);
        let before_mapped =
            c.direct_source_file_type_alias_body_rejection_kind[mapped_idx].load(Ordering::Relaxed);

        c.direct_source_file_type_alias_body_rejection_kind[type_ref_idx]
            .fetch_add(1, Ordering::Relaxed);
        c.direct_source_file_type_alias_body_rejection_kind[conditional_idx]
            .fetch_add(2, Ordering::Relaxed);
        c.direct_source_file_type_alias_body_rejection_kind[mapped_idx]
            .fetch_add(3, Ordering::Relaxed);

        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["direct_source_file_type_alias_body_rejection_kinds"]
            .as_array()
            .expect("direct_source_file_type_alias_body_rejection_kinds is array");
        let read = |idx: usize| rows[idx]["count"].as_u64().unwrap_or(0);

        assert_eq!(rows[type_ref_idx]["name"], "type_reference");
        assert!(
            read(type_ref_idx) > before_type_ref,
            "type_reference bump not visible (before={before_type_ref}, after={})",
            read(type_ref_idx),
        );

        assert_eq!(rows[conditional_idx]["name"], "conditional_type");
        assert!(
            read(conditional_idx) >= before_conditional.saturating_add(2),
            "conditional_type bump not visible (before={before_conditional}, after={})",
            read(conditional_idx),
        );

        assert_eq!(rows[mapped_idx]["name"], "mapped_type");
        assert!(
            read(mapped_idx) >= before_mapped.saturating_add(3),
            "mapped_type bump not visible (before={before_mapped}, after={})",
            read(mapped_idx),
        );
    }

    #[test]
    fn direct_source_file_type_alias_type_reference_rejection_kind_atomic_propagates_into_snapshot()
    {
        // The public recorder is gated on `TSZ_PERF_COUNTERS`; drive the
        // atomics directly so this unit test is independent of process env.
        let c = counters();

        let alias_with_args_idx =
            DirectSourceFileTypeAliasTypeReferenceRejectionKind::LocalTypeAliasWithArguments
                .as_index();
        let interface_no_args_idx =
            DirectSourceFileTypeAliasTypeReferenceRejectionKind::LocalInterfaceNoArguments
                .as_index();
        let alias_symbol_idx =
            DirectSourceFileTypeAliasTypeReferenceRejectionKind::LocalAliasSymbol.as_index();
        let unresolved_idx =
            DirectSourceFileTypeAliasTypeReferenceRejectionKind::UnresolvedIdentifier.as_index();

        let before_alias_with_args = c.direct_source_file_type_alias_type_reference_rejection_kind
            [alias_with_args_idx]
            .load(Ordering::Relaxed);
        let before_interface_no_args = c
            .direct_source_file_type_alias_type_reference_rejection_kind[interface_no_args_idx]
            .load(Ordering::Relaxed);
        let before_alias_symbol = c.direct_source_file_type_alias_type_reference_rejection_kind
            [alias_symbol_idx]
            .load(Ordering::Relaxed);
        let before_unresolved = c.direct_source_file_type_alias_type_reference_rejection_kind
            [unresolved_idx]
            .load(Ordering::Relaxed);

        c.direct_source_file_type_alias_type_reference_rejection_kind[alias_with_args_idx]
            .fetch_add(1, Ordering::Relaxed);
        c.direct_source_file_type_alias_type_reference_rejection_kind[interface_no_args_idx]
            .fetch_add(2, Ordering::Relaxed);
        c.direct_source_file_type_alias_type_reference_rejection_kind[alias_symbol_idx]
            .fetch_add(4, Ordering::Relaxed);
        c.direct_source_file_type_alias_type_reference_rejection_kind[unresolved_idx]
            .fetch_add(3, Ordering::Relaxed);

        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["direct_source_file_type_alias_type_reference_rejection_kinds"]
            .as_array()
            .expect("direct_source_file_type_alias_type_reference_rejection_kinds is array");
        let read = |idx: usize| rows[idx]["count"].as_u64().unwrap_or(0);

        assert_eq!(
            rows[alias_with_args_idx]["name"],
            "local_type_alias_with_arguments"
        );
        assert!(
            read(alias_with_args_idx) > before_alias_with_args,
            "local_type_alias_with_arguments bump not visible (before={before_alias_with_args}, after={})",
            read(alias_with_args_idx),
        );

        assert_eq!(
            rows[interface_no_args_idx]["name"],
            "local_interface_no_arguments"
        );
        assert!(
            read(interface_no_args_idx) >= before_interface_no_args.saturating_add(2),
            "local_interface_no_arguments bump not visible (before={before_interface_no_args}, after={})",
            read(interface_no_args_idx),
        );

        assert_eq!(rows[alias_symbol_idx]["name"], "local_alias_symbol");
        assert!(
            read(alias_symbol_idx) >= before_alias_symbol.saturating_add(4),
            "local_alias_symbol bump not visible (before={before_alias_symbol}, after={})",
            read(alias_symbol_idx),
        );

        assert_eq!(rows[unresolved_idx]["name"], "unresolved_identifier");
        assert!(
            read(unresolved_idx) >= before_unresolved.saturating_add(3),
            "unresolved_identifier bump not visible (before={before_unresolved}, after={})",
            read(unresolved_idx),
        );
    }

    #[test]
    fn direct_source_file_type_alias_first_type_reference_rejection_snapshot() {
        // The public recorder is gated on `TSZ_PERF_COUNTERS`; drive the
        // atomics directly so this unit test is independent of process env.
        let c = counters();

        let alias_with_args_idx =
            DirectSourceFileTypeAliasTypeReferenceRejectionKind::LocalTypeAliasWithArguments
                .as_index();
        let unresolved_idx =
            DirectSourceFileTypeAliasTypeReferenceRejectionKind::UnresolvedIdentifier.as_index();

        let before_alias_with_args = c
            .direct_source_file_type_alias_first_type_reference_rejection_kind[alias_with_args_idx]
            .load(Ordering::Relaxed);
        let before_unresolved = c.direct_source_file_type_alias_first_type_reference_rejection_kind
            [unresolved_idx]
            .load(Ordering::Relaxed);

        c.direct_source_file_type_alias_first_type_reference_rejection_kind[alias_with_args_idx]
            .fetch_add(1, Ordering::Relaxed);
        c.direct_source_file_type_alias_first_type_reference_rejection_kind[unresolved_idx]
            .fetch_add(2, Ordering::Relaxed);

        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let rows = json["direct_source_file_type_alias_first_type_reference_rejection_kinds"]
            .as_array()
            .expect("direct_source_file_type_alias_first_type_reference_rejection_kinds is array");
        let read = |idx: usize| rows[idx]["count"].as_u64().unwrap_or(0);

        assert_eq!(
            rows[alias_with_args_idx]["name"],
            "local_type_alias_with_arguments"
        );
        assert!(
            read(alias_with_args_idx) > before_alias_with_args,
            "local_type_alias_with_arguments first-bump not visible (before={before_alias_with_args}, after={})",
            read(alias_with_args_idx),
        );

        assert_eq!(rows[unresolved_idx]["name"], "unresolved_identifier");
        assert!(
            read(unresolved_idx) >= before_unresolved.saturating_add(2),
            "unresolved_identifier first-bump not visible (before={before_unresolved}, after={})",
            read(unresolved_idx),
        );
    }

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

        c.delegate_cross_arena_symbol_miss_by_source[source_idx].fetch_add(1, Ordering::Relaxed);
        c.delegate_cross_arena_symbol_miss_by_kind[kind_idx].fetch_add(1, Ordering::Relaxed);
        c.delegate_cross_arena_symbol_miss_target_declaration_file
            .fetch_add(1, Ordering::Relaxed);
        c.delegate_cross_arena_alias_shortcut_outcome[aso_idx].fetch_add(1, Ordering::Relaxed);
        c.source_file_symbol_arena_cache_eligibility_outcome[sfsa_idx]
            .fetch_add(1, Ordering::Relaxed);
        c.direct_cross_file_interface_lowering_outcome[dilo_idx].fetch_add(1, Ordering::Relaxed);
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

    #[test]
    fn write_json_to_writes_valid_json_with_atomic_rename() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("tsz-perf-counter-snap-{}.json", std::process::id()));
        // Clean up beforehand if a stale file is sitting around.
        let _ = std::fs::remove_file(&path);
        PerfCounters::write_json_to(&path).expect("write succeeds");
        let raw = std::fs::read_to_string(&path).expect("read back");
        // Round-trip through serde to confirm structure.
        let value: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
        assert_eq!(value["schema_version"], 2);
        assert!(value["wired"].is_object());
        // The atomic-rename `.json.tmp` should not be left behind.
        let tmp = path.with_extension("json.tmp");
        assert!(!tmp.exists(), "tmp file leaked: {tmp:?}");
        let _ = std::fs::remove_file(&path);
    }
}
