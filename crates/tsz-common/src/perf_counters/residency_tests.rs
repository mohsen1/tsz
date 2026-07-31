mod residency_tests {
    use super::*;

    /// Recording all three residency categories (merged program, shared
    /// query cache, type caches) with counters force-enabled must surface a
    /// `Some(residency)` snapshot with the recorded values and a coherent
    /// tracked total.
    ///
    /// Uses `ScopedPerfCounters` rather than relying on process ownership:
    /// the residency gauges are last-write-wins `store`s (a program's byte
    /// estimate, not an accumulating count), so under a shared-process
    /// runner a sibling test recording a different program's residency
    /// concurrently would silently overwrite the values asserted below. The
    /// scope gives this thread a private gauge set for its duration.
    #[test]
    fn residency_record_propagates_into_snapshot() {
        let _scope = ScopedPerfCounters::new();

        record_merged_program_residency(&MergedProgramResidencyRecord {
            ast_unique_arena_count: 12,
            ast_unique_arena_bytes_est: 1_000,
            bound_file_count: 3,
            bound_file_state_bytes_est: 200,
            lib_binder_count: 9,
            lib_binder_symbol_bytes_est: 30,
            program_symbol_state_bytes_est: 40,
            definition_store_bytes_est: 50,
            type_interner_bytes_est: 60,
            skeleton_index_bytes_est: 7,
            pre_merge_bind_total_bytes_est: 999,
            retained_file_state_bytes_est: 1_200,
            retained_file_state_pressure: ResidencyPressureLevel::Medium,
        });
        record_shared_query_cache_residency(21, 80);
        record_type_cache_residency(2, 90);

        let snap = PerfCounters::snapshot();
        let residency = snap.residency.expect("residency recorded => Some");
        assert_eq!(residency.ast_unique_arena_count, 12);
        assert_eq!(residency.ast_unique_arena_bytes_est, 1_000);
        assert_eq!(residency.bound_file_count, 3);
        assert_eq!(residency.bound_file_state_bytes_est, 200);
        assert_eq!(residency.lib_binder_count, 9);
        assert_eq!(residency.lib_binder_symbol_bytes_est, 30);
        assert_eq!(residency.program_symbol_state_bytes_est, 40);
        assert_eq!(residency.definition_store_bytes_est, 50);
        assert_eq!(residency.type_interner_bytes_est, 60);
        assert_eq!(residency.skeleton_index_bytes_est, 7);
        assert_eq!(residency.pre_merge_bind_total_bytes_est, 999);
        assert_eq!(residency.retained_file_state_bytes_est, 1_200);
        assert_eq!(
            residency.retained_file_state_pressure,
            ResidencyPressureLevel::Medium
        );
        assert_eq!(residency.shared_query_cache_entries, 21);
        assert_eq!(residency.shared_query_cache_bytes_est, 80);
        assert_eq!(residency.type_cache_count, 2);
        assert_eq!(residency.type_cache_bytes_est, 90);
        // Total excludes the transient pre-merge bytes.
        assert_eq!(
            residency.tracked_total_bytes_est,
            1_000 + 200 + 30 + 40 + 50 + 60 + 7 + 80 + 90
        );

        // JSON shape: residency must serialize as an object with the
        // estimate-labeled field names the bench harness reads.
        let json = serde_json::to_value(&snap).expect("serializes");
        assert_eq!(json["residency"]["ast_unique_arena_bytes_est"], 1_000);
        assert_eq!(json["residency"]["type_interner_bytes_est"], 60);
        assert_eq!(json["residency"]["retained_file_state_bytes_est"], 1_200);
        assert_eq!(json["residency"]["retained_file_state_pressure"], "medium");
        assert_eq!(json["residency"]["tracked_total_bytes_est"], 1_557);
    }

    /// Without any record call the snapshot must say "not measured"
    /// (`residency: null`), not "measured zero". The recording helpers are
    /// gated on `enabled_fast()`, which latches `false` in this process
    /// because `TSZ_PERF_COUNTERS` is unset for unit tests, so the calls
    /// below must leave the gauges unrecorded.
    #[test]
    fn residency_absent_when_disabled() {
        // Deliberately do NOT force-enable counters here.
        if enabled_fast() {
            // Defensive: if the suite ever runs with TSZ_PERF_COUNTERS set,
            // this test's premise does not hold; skip rather than mis-assert.
            return;
        }
        record_shared_query_cache_residency(123, 456);
        record_type_cache_residency(1, 2);
        record_merged_program_residency(&MergedProgramResidencyRecord {
            ast_unique_arena_count: 1,
            ast_unique_arena_bytes_est: 1,
            ..Default::default()
        });

        let snap = PerfCounters::snapshot();
        assert!(
            snap.residency.is_none(),
            "disabled counters must not record residency"
        );
        let json = serde_json::to_value(&snap).expect("serializes");
        assert!(json["residency"].is_null());
    }
}
