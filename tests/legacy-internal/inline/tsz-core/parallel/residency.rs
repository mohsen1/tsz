//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-core/src/parallel/residency.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 6dc0f34e70f689829780ee256873c975e150e3be98ef370299bd367bea850ee2 409 low_pressure_below_watermark
    #[test]
    fn low_pressure_below_watermark() {
        let budget = ResidencyBudget {
            low_watermark_bytes: 1000,
            high_watermark_bytes: 2000,
        };
        let stats = MergedProgramResidencyStats {
            file_count: 1,
            bound_file_arena_count: 1,
            unique_arena_count: 1,
            symbol_arena_count: 0,
            declaration_arena_bucket_count: 0,
            declaration_arena_mapping_count: 0,
            has_skeleton_index: false,
            skeleton_merge_candidate_count: 0,
            skeleton_total_symbol_count: 0,
            skeleton_estimated_size_bytes: 0,
            pre_merge_bind_total_bytes: 300,
            total_bound_file_bytes: 200,
            unique_arena_estimated_bytes: 0,
            has_dep_graph: false,
            dep_graph_edge_count: 0,
            dep_graph_root_count: 0,
            dep_graph_is_acyclic: true,
            dep_graph_cycle_count: 0,
            dep_graph_unresolved_count: 0,
        };
        assert_eq!(budget.assess(&stats), MemoryPressure::Low);
    }
// TSZ_INLINE_TEST_END 6dc0f34e70f689829780ee256873c975e150e3be98ef370299bd367bea850ee2

// TSZ_INLINE_TEST_BEGIN dfa7d36932b06cb226587c6193421f4f3451b165bbf61ff7f79a3cefb830afcc 439 high_pressure_above_watermark
    #[test]
    fn high_pressure_above_watermark() {
        let budget = ResidencyBudget {
            low_watermark_bytes: 1000,
            high_watermark_bytes: 2000,
        };
        let stats = MergedProgramResidencyStats {
            file_count: 100,
            bound_file_arena_count: 100,
            unique_arena_count: 50,
            symbol_arena_count: 100,
            declaration_arena_bucket_count: 50,
            declaration_arena_mapping_count: 200,
            has_skeleton_index: true,
            skeleton_merge_candidate_count: 10,
            skeleton_total_symbol_count: 500,
            skeleton_estimated_size_bytes: 50,
            pre_merge_bind_total_bytes: 1500,
            // Retained state (total_bound_file_bytes + unique_arena_estimated_bytes)
            // must exceed the 2000-byte high watermark; pre_merge_bind_total_bytes
            // is excluded from the residency formula.
            total_bound_file_bytes: 2500,
            unique_arena_estimated_bytes: 0,
            has_dep_graph: false,
            dep_graph_edge_count: 0,
            dep_graph_root_count: 0,
            dep_graph_is_acyclic: true,
            dep_graph_cycle_count: 0,
            dep_graph_unresolved_count: 0,
        };
        assert_eq!(budget.assess(&stats), MemoryPressure::High);
    }
// TSZ_INLINE_TEST_END dfa7d36932b06cb226587c6193421f4f3451b165bbf61ff7f79a3cefb830afcc

// TSZ_INLINE_TEST_BEGIN 02002f44056261b8712d8bbdf79087df7f5a05bd5e19fd6148a782506b7df79f 472 eviction_savings_estimates_freed_bytes
    #[test]
    fn eviction_savings_estimates_freed_bytes() {
        let stats = MergedProgramResidencyStats {
            file_count: 10,
            bound_file_arena_count: 10,
            unique_arena_count: 5,
            symbol_arena_count: 10,
            declaration_arena_bucket_count: 5,
            declaration_arena_mapping_count: 20,
            has_skeleton_index: true,
            skeleton_merge_candidate_count: 3,
            skeleton_total_symbol_count: 50,
            skeleton_estimated_size_bytes: 1000,
            pre_merge_bind_total_bytes: 50_000,
            total_bound_file_bytes: 20_000,
            unique_arena_estimated_bytes: 0,
            has_dep_graph: false,
            dep_graph_edge_count: 0,
            dep_graph_root_count: 0,
            dep_graph_is_acyclic: true,
            dep_graph_cycle_count: 0,
            dep_graph_unresolved_count: 0,
        };
        // Savings = pre_merge - skeleton = 50000 - 1000 = 49000
        assert_eq!(ResidencyBudget::eviction_savings(&stats), 49_000);
    }
// TSZ_INLINE_TEST_END 02002f44056261b8712d8bbdf79087df7f5a05bd5e19fd6148a782506b7df79f

// TSZ_INLINE_TEST_BEGIN 0dd8c3c5ca2794be7875c8b4bb9a84f08c29062873c85f3ca300de9738ef3138 499 no_eviction_without_skeleton
    #[test]
    fn no_eviction_without_skeleton() {
        let stats = MergedProgramResidencyStats {
            file_count: 10,
            bound_file_arena_count: 10,
            unique_arena_count: 5,
            symbol_arena_count: 10,
            declaration_arena_bucket_count: 5,
            declaration_arena_mapping_count: 20,
            has_skeleton_index: false,
            skeleton_merge_candidate_count: 0,
            skeleton_total_symbol_count: 0,
            skeleton_estimated_size_bytes: 0,
            pre_merge_bind_total_bytes: 50_000,
            total_bound_file_bytes: 20_000,
            unique_arena_estimated_bytes: 0,
            has_dep_graph: false,
            dep_graph_edge_count: 0,
            dep_graph_root_count: 0,
            dep_graph_is_acyclic: true,
            dep_graph_cycle_count: 0,
            dep_graph_unresolved_count: 0,
        };
        assert_eq!(ResidencyBudget::eviction_savings(&stats), 0);
    }
// TSZ_INLINE_TEST_END 0dd8c3c5ca2794be7875c8b4bb9a84f08c29062873c85f3ca300de9738ef3138
