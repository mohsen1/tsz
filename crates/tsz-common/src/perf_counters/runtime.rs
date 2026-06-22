/// One process-wide instance. Incremented from any thread, read once at
/// dump time.
pub struct PerfCounters {
    pub enabled: AtomicBool,

    // ─── delegation / cross-arena resolution ─────────────────────────────
    pub delegate_cross_arena_calls: AtomicU64,
    pub delegate_cross_arena_cache_hits_lib: AtomicU64,
    pub delegate_cross_arena_cache_hits_cross_file: AtomicU64,
    pub delegate_cross_arena_misses: AtomicU64,
    /// Of `delegate_cross_arena_misses` (full child-checker work), how many
    /// completed with a sentinel (`ERROR`/`UNKNOWN`) result. Sentinels are
    /// refused by the shared cross-file buckets, so a large count here means
    /// the same failed resolutions are being recomputed per delegation tree.
    pub delegate_cross_arena_full_work_sentinel_results: AtomicU64,
    /// T2.2 cross-file type-parameter memo: hits and misses on the
    /// `extract_type_params_from_decl` slow-path memoization. A hit means
    /// the slow-path's `with_parent_cache_attributed(..., TypeEnvironmentCore)`
    /// was elided.
    pub cross_file_type_params_cache_hits: AtomicU64,
    pub cross_file_type_params_cache_misses: AtomicU64,
    pub delegate_max_recursion_depth: AtomicU64,
    /// `DelegateCrossArenaSymbol` misses classified by how the target arena
    /// was found. This is a subset of `delegate_cross_arena_misses`.
    pub delegate_cross_arena_symbol_miss_by_source:
        [AtomicU64; CROSS_ARENA_SYMBOL_MISS_SOURCE_COUNT],
    /// `DelegateCrossArenaSymbol` misses classified by target symbol kind.
    pub delegate_cross_arena_symbol_miss_by_kind: [AtomicU64; CROSS_ARENA_SYMBOL_MISS_KIND_COUNT],
    pub delegate_cross_arena_symbol_miss_target_declaration_file: AtomicU64,
    pub delegate_cross_arena_symbol_miss_target_source_file: AtomicU64,
    /// Outcome buckets for the no-child alias shortcut attempted before a
    /// `DelegateCrossArenaSymbol` miss constructs a child checker.
    pub delegate_cross_arena_alias_shortcut_outcome:
        [AtomicU64; CROSS_ARENA_ALIAS_SHORTCUT_OUTCOME_COUNT],
    /// Outcome buckets for direct cross-file interface lowering attempts.
    pub direct_cross_file_interface_lowering_outcome:
        [AtomicU64; DIRECT_CROSS_FILE_INTERFACE_LOWERING_OUTCOME_COUNT],
    /// Reason buckets for complex direct cross-file interface declarations.
    pub direct_cross_file_interface_complex_reason:
        [AtomicU64; DIRECT_CROSS_FILE_INTERFACE_COMPLEX_REASON_COUNT],
    /// Outcome buckets for direct actual-lib alias-body attempts.
    pub direct_actual_lib_alias_body_outcome:
        [AtomicU64; DIRECT_ACTUAL_LIB_ALIAS_BODY_OUTCOME_COUNT],
    /// Outcome buckets for direct source-file type-alias lowering attempts.
    pub direct_source_file_type_alias_lowering_outcome:
        [AtomicU64; DIRECT_SOURCE_FILE_TYPE_ALIAS_LOWERING_OUTCOME_COUNT],
    /// Root syntax family for source-file alias bodies rejected by the direct
    /// lowering proof.
    pub direct_source_file_type_alias_body_rejection_kind:
        [AtomicU64; DIRECT_SOURCE_FILE_TYPE_ALIAS_BODY_REJECTION_KIND_COUNT],
    /// Structural subtype for root `TypeReference` alias bodies rejected by
    /// the direct-lowering proof.
    pub direct_source_file_type_alias_type_reference_rejection_kind:
        [AtomicU64; DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_COUNT],
    /// First nested `TypeReference` rejection seen per rejected source-file
    /// alias body. This is one bucket per rejected alias, unlike the all-refs
    /// counter above.
    pub direct_source_file_type_alias_first_type_reference_rejection_kind:
        [AtomicU64; DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_COUNT],
    /// Outcome buckets for direct actual-lib Intl interface attempts.
    pub direct_actual_lib_intl_interface_outcome:
        [AtomicU64; DIRECT_ACTUAL_LIB_INTL_INTERFACE_OUTCOME_COUNT],
    /// Track 7 stable-identity migration counter: times
    /// `TypeEnvironment::resolve_lazy` had to treat a `DefId` value as a raw
    /// `SymbolId` and redirect it to the real `DefId`.
    pub type_environment_raw_symbol_lazy_fallbacks: AtomicU64,
    /// #14344 identity-collision observability: times the
    /// `raw_symbol_fallback_def` `#13862` guard suppressed a *genuine* content
    /// collision — a store-registered `DefId(N)` whose raw value `N`, reread as
    /// a `SymbolId`, resolves to a DIFFERENT def whose canonical decl (name)
    /// differs from `DefId(N)`'s own. This is the `HTMLDivElement(218)` ->
    /// `FileSystemEntry(symbol 218)` class of collision (#13862), NOT mere
    /// raw-`u32` overlap (which is ~100% by construction and uninformative). A
    /// nonzero value is the measurable witness of the non-canonical-identity
    /// root; the migration's md5-stability gate watches it trend to zero.
    pub identity_collision_wrong_decl_suppressed: AtomicU64,
    /// #14344 denominator context: total `symbol_def_index` (`(symbol, file)`
    /// composite-key) resolution attempts, partitioned into hits/misses, so the
    /// wrong-decl collision count above has a population to normalize against
    /// (a raw count is uninterpretable without "out of how many lookups").
    pub symbol_def_index_lookup_hits: AtomicU64,
    /// Companion miss counter for [`Self::symbol_def_index_lookup_hits`].
    pub symbol_def_index_lookup_misses: AtomicU64,
    /// Why each `cached_cross_file_*` reader returned `None`. See
    /// [`CrossFileCacheMissCause`] for the bucket semantics. Sum of
    /// all buckets equals the flat miss count for the four reader
    /// helpers in `crates/tsz-checker/src/context/cross_file_query.rs`.
    pub cross_file_cache_miss_cause: [AtomicU64; CROSS_FILE_CACHE_MISS_CAUSE_COUNT],
    /// Source-file symbol-arena cache eligibility/rejection buckets for
    /// `DelegateCrossArenaSymbol` delegations. This classifies the remaining
    /// post-#6191 symbol-arena residue before we widen any cache keys or direct
    /// lowering paths.
    pub source_file_symbol_arena_cache_eligibility_outcome:
        [AtomicU64; SOURCE_FILE_SYMBOL_ARENA_CACHE_ELIGIBILITY_OUTCOME_COUNT],

    // --- lib bootstrap ----------------------------------------------------
    pub lib_snapshot_set_load_attempts: AtomicU64,
    pub lib_snapshot_set_load_hits: AtomicU64,
    pub lib_snapshot_set_load_misses: AtomicU64,
    pub lib_snapshot_set_load_files_total: AtomicU64,
    pub lib_snapshot_set_load_elapsed_ns_total: AtomicU64,
    pub lib_snapshot_set_load_elapsed_ns_max: AtomicU64,
    pub checker_lib_clone_calls: AtomicU64,
    pub checker_lib_clone_parallel_calls: AtomicU64,
    pub checker_lib_clone_files_total: AtomicU64,
    pub checker_lib_clone_elapsed_ns_total: AtomicU64,
    pub checker_lib_clone_elapsed_ns_max: AtomicU64,

    // ─── checker construction ────────────────────────────────────────────
    pub checker_state_constructed: AtomicU64,
    pub checker_state_with_parent_cache_constructed: AtomicU64,
    /// Per-`CheckerCreationReason` breakdown of `with_parent_cache` calls.
    /// `with_parent_cache_by_reason[reason as usize]` is the count for that
    /// site. Total equals `checker_state_with_parent_cache_constructed`.
    pub with_parent_cache_by_reason: [AtomicU64; CHECKER_CREATION_REASON_COUNT],

    // ─── checker file-session ────────────────────────────────────────────
    /// Number of times `CheckerContext::reset_for_next_file()` has been
    /// invoked. Zero on the default per-file checker construction path;
    /// nonzero only on a sequential session-reuse path (T2.1.B).
    /// Attribution-mode verification: in a reuse run the counter equals
    /// `(files_checked - 1)` and `checker_state_constructed` falls by the
    /// same amount versus the baseline construction-per-file path.
    pub file_session_resets: AtomicU64,
    /// High-water retained checker-context cache entries observed immediately
    /// before `CheckerContext::reset_for_next_file()` clears file-local state.
    pub file_session_reset_cache_entries_max: AtomicU64,
    /// High-water estimated bytes for the same reset-boundary cache snapshot.
    pub file_session_reset_cache_bytes_max: AtomicU64,
    pub file_session_reset_namespace_member_entries_max: AtomicU64,
    pub file_session_reset_namespace_member_bytes_max: AtomicU64,
    pub file_session_reset_export_equals_entries_max: AtomicU64,
    pub file_session_reset_export_equals_bytes_max: AtomicU64,
    pub file_session_reset_nested_namespace_entries_max: AtomicU64,
    pub file_session_reset_nested_namespace_bytes_max: AtomicU64,
    pub file_session_reset_lowering_entity_name_entries_max: AtomicU64,
    pub file_session_reset_lowering_entity_name_bytes_max: AtomicU64,
    pub file_session_reset_env_eval_entries_max: AtomicU64,
    pub file_session_reset_env_eval_bytes_max: AtomicU64,

    // ─── overlay copy ────────────────────────────────────────────────────
    pub copy_symbol_file_targets_calls: AtomicU64,
    pub copy_symbol_file_targets_entries_total: AtomicU64,
    /// Largest single overlay clone observed across the whole run.
    /// Distinguishes "many medium clones" from "a few catastrophic huge
    /// clones" — both can produce the same `entries_total`, but the fix
    /// shape is different. (Per PR #1630 review.)
    pub copy_symbol_file_targets_entries_max: AtomicU64,
    /// Bucketed histogram of overlay-clone sizes. `len_ge_N` counts the
    /// number of `copy_symbol_file_targets_to` calls where the parent's
    /// overlay had ≥ N entries at copy time. The buckets are nested so
    /// `len_ge_1m ≤ len_ge_100k ≤ len_ge_10k ≤ len_ge_1k ≤ calls`.
    pub copy_symbol_file_targets_len_ge_1k: AtomicU64,
    pub copy_symbol_file_targets_len_ge_10k: AtomicU64,
    pub copy_symbol_file_targets_len_ge_100k: AtomicU64,
    pub copy_symbol_file_targets_len_ge_1m: AtomicU64,
    /// Per-`CheckerCreationReason` breakdown of overlay-copy calls.
    pub overlay_copy_calls_by_reason: [AtomicU64; CHECKER_CREATION_REASON_COUNT],
    /// Per-`CheckerCreationReason` breakdown of overlay entries copied
    /// (sum of `parent.cross_file_symbol_targets.len()` at each call).
    pub overlay_copy_entries_by_reason: [AtomicU64; CHECKER_CREATION_REASON_COUNT],
    /// Per-`CheckerCreationReason` max overlay size observed at call time.
    /// Updated via [`record_max`] so the report shows the worst single
    /// clone per reason, not just the average.
    pub overlay_copy_max_entries_by_reason: [AtomicU64; CHECKER_CREATION_REASON_COUNT],

    // ─── solver relation limit-result cache (issue #13241) ──────────────
    /// Times a budget-conditional `LimitTrue` relation entry short-circuited
    /// a subtype query (the recorded fuel band covered the query's remaining
    /// budget). Each hit avoids re-burning a full limit-hit relation chain.
    pub relation_limit_cache_hits: AtomicU64,
    /// Maybe-stack keys promoted into the relation cache at outermost
    /// relation success (tsc `maybeKeys` promotion parity): cycle-derived
    /// keys promoted to definitive `true` plus fuel-derived keys promoted to
    /// band-conditional `LimitTrue` entries.
    pub relation_maybe_promotions: AtomicU64,

    // ─── opt-in shared instantiation/application caches (#13240) ─────────
    pub shared_application_eval_cache_hits: AtomicU64,
    pub shared_application_eval_cache_misses: AtomicU64,
    pub shared_application_eval_cache_inserts: AtomicU64,
    pub shared_application_eval_cache_bypasses: AtomicU64,
    pub shared_instantiation_cache_hits: AtomicU64,
    pub shared_instantiation_cache_misses: AtomicU64,
    pub shared_instantiation_cache_inserts: AtomicU64,
    pub shared_instantiation_cache_bypasses: AtomicU64,

    // ─── relation failure-reason single pass (issue #13243) ─────────────
    /// Failure-reason walks executed after a failing reason-collecting
    /// assignability relation (`is_weak_union_violation` plus
    /// `explain_failure` on the configured `CompatChecker`). Each walk
    /// re-traverses the failing relation graph, so on diagnostic-heavy code
    /// this is the duplicated cost the single-pass campaign removes.
    pub relation_failure_reason_walks: AtomicU64,
    /// Failing relation analyses served from the checker's stamp-guarded
    /// failure-analysis memo instead of re-running the relation engine plus
    /// the failure-reason walk.
    pub relation_failure_memo_hits: AtomicU64,
    /// Weak-type/weak-union probes (`violates_weak_union` /
    /// `violates_weak_type`) executed while collecting a failure reason. The
    /// boolean (`weak_union_violation`) and the failure reason previously each
    /// ran these probes; the single-pass `analyze_weak_and_explain` runs them
    /// once and feeds both, so on weak/union-target-heavy failures this counter
    /// halves (issue #13243).
    pub relation_weak_violation_probes: AtomicU64,

    // ─── solver concrete materialization (issue #13242) ─────────────────
    pub union_subtype_reduction_calls: AtomicU64,
    pub union_subtype_reduction_members_total: AtomicU64,
    pub union_subtype_reduction_members_max: AtomicU64,
    pub union_subtype_reduction_pairwise_budget_total: AtomicU64,
    pub union_subtype_reduction_shallow_checks: AtomicU64,
    pub property_instantiation_walks: AtomicU64,
    pub property_instantiation_properties_total: AtomicU64,
    pub property_instantiation_properties_max: AtomicU64,
    pub property_instantiation_changed: AtomicU64,

    // ─── solver evaluator memo lifecycle (issue #13097) ──────────────────
    /// `TypeEvaluator` constructions (each is a fresh per-run memo set).
    pub eval_evaluator_constructions: AtomicU64,
    /// Hits on an evaluator's own per-run `TypeId -> TypeId` memo.
    pub eval_local_memo_hits: AtomicU64,
    /// Nodes actually computed (`evaluate_guarded_inner` entries past every
    /// memo/cache layer).
    pub eval_compute_nodes: AtomicU64,
    /// Clean (untainted) computes whose `(TypeId, options)` key was already
    /// computed clean — with the same result — by an *earlier* evaluator in
    /// the same file scope. Each one is work a per-file shared memo would
    /// have skipped; the fresh-evaluator pattern discarded it instead.
    pub eval_lost_memo_recomputes: AtomicU64,
    /// Same-key clean computes whose result *differed* from the earlier
    /// evaluator's. Evidence that naive within-file result sharing across
    /// evaluator contexts would change behavior (resolver/registration
    /// dependence); these must stay per-run.
    pub eval_lost_memo_mismatches: AtomicU64,
    /// `lookup_eval_memo` hits served at nested evaluate nodes.
    pub eval_memo_nested_hits: AtomicU64,
    /// Lost recomputes performed by a plain memo-reading evaluator.
    pub eval_lost_memo_recomputes_plain: AtomicU64,
    /// Lost recomputes performed by the checker's authoritative
    /// (closed-eval-writing, resolver-backed) evaluator.
    pub eval_lost_memo_recomputes_authoritative: AtomicU64,
    /// Lost recomputes performed by other contexts (resolver-backed
    /// non-authoritative, or mode-flagged plain evaluators).
    pub eval_lost_memo_recomputes_other: AtomicU64,
    /// Subset of `eval_lost_memo_recomputes` whose result equals its input
    /// (`eval(T) == T`): identity walks the per-file drain deliberately
    /// skips, so they are re-walked by every evaluator that meets them.
    pub eval_lost_memo_recomputes_identity: AtomicU64,
    /// Per-run memo entries still resident when an evaluator was dropped
    /// (i.e. not drained into a longer-lived cache).
    pub eval_dropped_memo_entries: AtomicU64,
    /// Auxiliary memo entries (conditional-subtype + contains-infer) dropped
    /// with their evaluator; these tables are never drained anywhere.
    pub eval_dropped_aux_entries: AtomicU64,
    /// Which guard cut a `TypeEvaluator::evaluate` walk short, bucketed by
    /// [`EvaluationTerminationGuard`] (#14346). The firing-order signal the
    /// issue flags: which bound a runaway recursive walk hits first. Always
    /// `EVALUATION_TERMINATION_GUARD_COUNT` long, in
    /// `EVALUATION_TERMINATION_GUARD_NAMES` order.
    pub eval_termination_guard_fires: [AtomicU64; EVALUATION_TERMINATION_GUARD_COUNT],

    // ─── interner ────────────────────────────────────────────────────────
    pub interner_intern_calls: AtomicU64,
    pub interner_intern_hits: AtomicU64,
    pub interner_intern_misses: AtomicU64,
    pub interner_string_intern_calls: AtomicU64,
    /// `intern_string` calls served from the thread-local string cache
    /// (no shard `RwLock` / `ShardedInterner::intern`). The cache hit rate is
    /// `interner_string_intern_cache_hits / interner_string_intern_calls`.
    pub interner_string_intern_cache_hits: AtomicU64,
    pub interner_type_list_intern_calls: AtomicU64,
    pub interner_object_shape_intern_calls: AtomicU64,
    pub interner_function_shape_intern_calls: AtomicU64,
    pub interner_callable_shape_intern_calls: AtomicU64,
    pub interner_application_intern_calls: AtomicU64,
    pub interner_conditional_intern_calls: AtomicU64,
    pub interner_mapped_intern_calls: AtomicU64,
    /// Lock-wait histogram. Each call to [`time_shard_write`] adds one
    /// observation to the bucket whose upper bound first exceeds the
    /// elapsed nanoseconds. Only populated when the
    /// `perf-counters-timing` cargo feature is enabled — otherwise
    /// `time_shard_write` compiles to a direct call of its closure
    /// and the histogram stays at all-zero.
    pub interner_lock_wait_histogram_ns: [AtomicU64; LOCK_WAIT_BUCKET_COUNT],

    // ─── interner locality (issue #13246, Tier-1 memory-locality tax) ────
    // The fixed-size TLS direct-mapped caches in
    // `crates/tsz-solver/src/intern/core/interner/cache.rs` thrash once a
    // file's live working set exceeds their slot count, falling back to the
    // sharded `RwLock<Vec<TypeData>>` cold path. These counters quantify
    // that thrash directly so the `O(files^1.7)` per-file check-time slope
    // can be attributed to (or ruled out as) cache-locality decay.
    //
    // Hit-rate denominators:
    //   intern: `interner_intern_calls`
    //   lookup: `interner_lookup_calls`
    /// `lookup()` entries that reached the TLS/shard probe (intrinsics and
    /// error ids short-circuit before this and are not counted).
    pub interner_lookup_calls: AtomicU64,
    /// `lookup()` calls served from the TLS direct-mapped lookup cache.
    pub interner_lookup_tls_hits: AtomicU64,
    /// `lookup()` calls that missed the TLS cache and fell through to the
    /// cold sharded `RwLock<Vec<TypeData>>` (the 15-25 ns/lookup path).
    pub interner_lookup_cold_vec_fallbacks: AtomicU64,
    /// TLS lookup-cache inserts that overwrote a live entry whose tag
    /// belonged to a *different* `TypeId` (a direct-mapped collision). A
    /// rising eviction rate is the working-set-exceeds-cache thrash signal.
    pub interner_lookup_tls_evictions: AtomicU64,
    /// `intern()` calls served from the TLS direct-mapped intern cache.
    /// Subset of `interner_intern_hits`; isolates TLS hits from intrinsic
    /// and shard-read hits, which the aggregate `interner_intern_hits`
    /// lumps together.
    pub interner_intern_tls_hits: AtomicU64,
    /// `intern()` calls that missed the TLS intern cache and ran the
    /// `DashMap`/shard slow path (`intern_slow`).
    pub interner_intern_cold_fallbacks: AtomicU64,
    /// TLS intern-cache inserts that overwrote a live entry for a different
    /// hash (direct-mapped collision). The intern-side thrash signal.
    pub interner_intern_tls_evictions: AtomicU64,
    /// Per-file distinct-`TypeId` working-set high-water mark across the
    /// run (max over files of distinct ids touched by `lookup`/`intern`).
    /// When this exceeds `LOOKUP_CACHE_SIZE` (1024) the TLS cache cannot
    /// hold a file's live set and thrash is structurally forced.
    pub interner_working_set_distinct_max: AtomicU64,
    /// Number of files whose distinct working set exceeded the TLS lookup
    /// cache slot count. The over-capacity file fraction along the scale
    /// ladder is the locality-decay signal.
    pub interner_working_set_files_over_cache: AtomicU64,
    /// Files sampled for the working-set metric (denominator for the
    /// over-cache fraction).
    pub interner_working_set_files_sampled: AtomicU64,
    /// Sum of per-file distinct working sets (for a mean alongside the max).
    pub interner_working_set_distinct_total: AtomicU64,
    /// Probe accounting (`TSZ_PROMOTE_FIRST`, opt-in, default OFF): times a
    /// `lookup`/`intern` was served from the promoted/global hot tier before
    /// the per-instance TLS cache was consulted. Measurement-only; zero when
    /// the probe is off. See [`promote_first_enabled`].
    pub interner_promote_tier_hits: AtomicU64,
    /// Probe accounting: times the promoted tier was consulted but missed
    /// (the id/key was not in the stable hot set), falling through to the
    /// normal TLS + shard path. Sum with `interner_promote_tier_hits` is the
    /// number of probe consultations.
    pub interner_promote_tier_misses: AtomicU64,

    // ─── compute_type_of_symbol ──────────────────────────────────────────
    pub compute_type_of_symbol_calls: AtomicU64,
    pub compute_type_of_symbol_cache_hits: AtomicU64,
    pub compute_type_of_symbol_interface_simple_object_fastpath_hits: AtomicU64,
    pub compute_type_of_symbol_source_outcome:
        [AtomicU64; COMPUTE_TYPE_OF_SYMBOL_SOURCE_OUTCOME_COUNT],
    pub compute_type_of_symbol_kind_outcome: [AtomicU64; COMPUTE_TYPE_OF_SYMBOL_KIND_OUTCOME_COUNT],
    pub compute_type_of_symbol_interface_fastpath_outcome:
        [AtomicU64; COMPUTE_TYPE_OF_SYMBOL_INTERFACE_FASTPATH_OUTCOME_COUNT],
    pub compute_type_of_symbol_interface_callsite_outcome:
        [AtomicU64; COMPUTE_TYPE_OF_SYMBOL_INTERFACE_CALLSITE_OUTCOME_COUNT],
    pub compute_type_of_symbol_interface_simple_object_outcome:
        [AtomicU64; COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_OUTCOME_COUNT],
    pub compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kind: [AtomicU64;
        COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_KIND_COUNT],
    pub compute_type_of_symbol_interface_simple_object_type_reference_reject_outcome: [AtomicU64;
        COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_OUTCOME_COUNT],
    pub compute_type_of_symbol_interface_simple_object_actual_lib_type_reference_outcome:
        [AtomicU64;
            COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_ACTUAL_LIB_TYPE_REFERENCE_OUTCOME_COUNT],
    pub property_classification_calls: AtomicU64,
    pub property_classification_string_fallback_source_lookups: AtomicU64,
    pub property_classification_string_fallback_target_names: AtomicU64,
    pub property_classification_string_fallback_target_types: AtomicU64,

    // ─── resolver / VFS ──────────────────────────────────────────────────
    pub resolver_lookup_calls: AtomicU64,
    pub resolver_is_file_calls: AtomicU64,
    pub resolver_is_dir_calls: AtomicU64,
    pub resolver_read_dir_calls: AtomicU64,
    pub resolver_read_package_json_calls: AtomicU64,
    pub resolver_candidate_paths_total: AtomicU64,
}

static COUNTERS: OnceLock<PerfCounters> = OnceLock::new();

impl PerfCounters {
    const fn new_zero() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            delegate_cross_arena_calls: AtomicU64::new(0),
            delegate_cross_arena_cache_hits_lib: AtomicU64::new(0),
            delegate_cross_arena_cache_hits_cross_file: AtomicU64::new(0),
            delegate_cross_arena_misses: AtomicU64::new(0),
            delegate_cross_arena_full_work_sentinel_results: AtomicU64::new(0),
            cross_file_type_params_cache_hits: AtomicU64::new(0),
            cross_file_type_params_cache_misses: AtomicU64::new(0),
            delegate_max_recursion_depth: AtomicU64::new(0),
            delegate_cross_arena_symbol_miss_by_source: [const { AtomicU64::new(0) };
                CROSS_ARENA_SYMBOL_MISS_SOURCE_COUNT],
            delegate_cross_arena_symbol_miss_by_kind: [const { AtomicU64::new(0) };
                CROSS_ARENA_SYMBOL_MISS_KIND_COUNT],
            delegate_cross_arena_symbol_miss_target_declaration_file: AtomicU64::new(0),
            delegate_cross_arena_symbol_miss_target_source_file: AtomicU64::new(0),
            delegate_cross_arena_alias_shortcut_outcome: [const { AtomicU64::new(0) };
                CROSS_ARENA_ALIAS_SHORTCUT_OUTCOME_COUNT],
            direct_cross_file_interface_lowering_outcome: [const { AtomicU64::new(0) };
                DIRECT_CROSS_FILE_INTERFACE_LOWERING_OUTCOME_COUNT],
            direct_cross_file_interface_complex_reason: [const { AtomicU64::new(0) };
                DIRECT_CROSS_FILE_INTERFACE_COMPLEX_REASON_COUNT],
            direct_actual_lib_alias_body_outcome: [const { AtomicU64::new(0) };
                DIRECT_ACTUAL_LIB_ALIAS_BODY_OUTCOME_COUNT],
            direct_source_file_type_alias_lowering_outcome: [const { AtomicU64::new(0) };
                DIRECT_SOURCE_FILE_TYPE_ALIAS_LOWERING_OUTCOME_COUNT],
            direct_source_file_type_alias_body_rejection_kind: [const { AtomicU64::new(0) };
                DIRECT_SOURCE_FILE_TYPE_ALIAS_BODY_REJECTION_KIND_COUNT],
            direct_source_file_type_alias_type_reference_rejection_kind: [const { AtomicU64::new(0) };
                DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_COUNT],
            direct_source_file_type_alias_first_type_reference_rejection_kind: [const {
                AtomicU64::new(0)
            };
                DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_COUNT],
            direct_actual_lib_intl_interface_outcome: [const { AtomicU64::new(0) };
                DIRECT_ACTUAL_LIB_INTL_INTERFACE_OUTCOME_COUNT],
            type_environment_raw_symbol_lazy_fallbacks: AtomicU64::new(0),
            identity_collision_wrong_decl_suppressed: AtomicU64::new(0),
            symbol_def_index_lookup_hits: AtomicU64::new(0),
            symbol_def_index_lookup_misses: AtomicU64::new(0),
            cross_file_cache_miss_cause: [const { AtomicU64::new(0) };
                CROSS_FILE_CACHE_MISS_CAUSE_COUNT],
            source_file_symbol_arena_cache_eligibility_outcome: [const { AtomicU64::new(0) };
                SOURCE_FILE_SYMBOL_ARENA_CACHE_ELIGIBILITY_OUTCOME_COUNT],
            lib_snapshot_set_load_attempts: AtomicU64::new(0),
            lib_snapshot_set_load_hits: AtomicU64::new(0),
            lib_snapshot_set_load_misses: AtomicU64::new(0),
            lib_snapshot_set_load_files_total: AtomicU64::new(0),
            lib_snapshot_set_load_elapsed_ns_total: AtomicU64::new(0),
            lib_snapshot_set_load_elapsed_ns_max: AtomicU64::new(0),
            checker_lib_clone_calls: AtomicU64::new(0),
            checker_lib_clone_parallel_calls: AtomicU64::new(0),
            checker_lib_clone_files_total: AtomicU64::new(0),
            checker_lib_clone_elapsed_ns_total: AtomicU64::new(0),
            checker_lib_clone_elapsed_ns_max: AtomicU64::new(0),
            checker_state_constructed: AtomicU64::new(0),
            checker_state_with_parent_cache_constructed: AtomicU64::new(0),
            with_parent_cache_by_reason: [const { AtomicU64::new(0) };
                CHECKER_CREATION_REASON_COUNT],
            file_session_resets: AtomicU64::new(0),
            file_session_reset_cache_entries_max: AtomicU64::new(0),
            file_session_reset_cache_bytes_max: AtomicU64::new(0),
            file_session_reset_namespace_member_entries_max: AtomicU64::new(0),
            file_session_reset_namespace_member_bytes_max: AtomicU64::new(0),
            file_session_reset_export_equals_entries_max: AtomicU64::new(0),
            file_session_reset_export_equals_bytes_max: AtomicU64::new(0),
            file_session_reset_nested_namespace_entries_max: AtomicU64::new(0),
            file_session_reset_nested_namespace_bytes_max: AtomicU64::new(0),
            file_session_reset_lowering_entity_name_entries_max: AtomicU64::new(0),
            file_session_reset_lowering_entity_name_bytes_max: AtomicU64::new(0),
            file_session_reset_env_eval_entries_max: AtomicU64::new(0),
            file_session_reset_env_eval_bytes_max: AtomicU64::new(0),
            copy_symbol_file_targets_calls: AtomicU64::new(0),
            copy_symbol_file_targets_entries_total: AtomicU64::new(0),
            copy_symbol_file_targets_entries_max: AtomicU64::new(0),
            copy_symbol_file_targets_len_ge_1k: AtomicU64::new(0),
            copy_symbol_file_targets_len_ge_10k: AtomicU64::new(0),
            copy_symbol_file_targets_len_ge_100k: AtomicU64::new(0),
            copy_symbol_file_targets_len_ge_1m: AtomicU64::new(0),
            overlay_copy_calls_by_reason: [const { AtomicU64::new(0) };
                CHECKER_CREATION_REASON_COUNT],
            overlay_copy_entries_by_reason: [const { AtomicU64::new(0) };
                CHECKER_CREATION_REASON_COUNT],
            overlay_copy_max_entries_by_reason: [const { AtomicU64::new(0) };
                CHECKER_CREATION_REASON_COUNT],
            relation_limit_cache_hits: AtomicU64::new(0),
            relation_maybe_promotions: AtomicU64::new(0),
            shared_application_eval_cache_hits: AtomicU64::new(0),
            shared_application_eval_cache_misses: AtomicU64::new(0),
            shared_application_eval_cache_inserts: AtomicU64::new(0),
            shared_application_eval_cache_bypasses: AtomicU64::new(0),
            shared_instantiation_cache_hits: AtomicU64::new(0),
            shared_instantiation_cache_misses: AtomicU64::new(0),
            shared_instantiation_cache_inserts: AtomicU64::new(0),
            shared_instantiation_cache_bypasses: AtomicU64::new(0),
            relation_failure_reason_walks: AtomicU64::new(0),
            relation_failure_memo_hits: AtomicU64::new(0),
            relation_weak_violation_probes: AtomicU64::new(0),
            union_subtype_reduction_calls: AtomicU64::new(0),
            union_subtype_reduction_members_total: AtomicU64::new(0),
            union_subtype_reduction_members_max: AtomicU64::new(0),
            union_subtype_reduction_pairwise_budget_total: AtomicU64::new(0),
            union_subtype_reduction_shallow_checks: AtomicU64::new(0),
            property_instantiation_walks: AtomicU64::new(0),
            property_instantiation_properties_total: AtomicU64::new(0),
            property_instantiation_properties_max: AtomicU64::new(0),
            property_instantiation_changed: AtomicU64::new(0),
            eval_evaluator_constructions: AtomicU64::new(0),
            eval_local_memo_hits: AtomicU64::new(0),
            eval_compute_nodes: AtomicU64::new(0),
            eval_lost_memo_recomputes: AtomicU64::new(0),
            eval_lost_memo_mismatches: AtomicU64::new(0),
            eval_lost_memo_recomputes_identity: AtomicU64::new(0),
            eval_memo_nested_hits: AtomicU64::new(0),
            eval_lost_memo_recomputes_plain: AtomicU64::new(0),
            eval_lost_memo_recomputes_authoritative: AtomicU64::new(0),
            eval_lost_memo_recomputes_other: AtomicU64::new(0),
            eval_dropped_memo_entries: AtomicU64::new(0),
            eval_dropped_aux_entries: AtomicU64::new(0),
            eval_termination_guard_fires: [const { AtomicU64::new(0) };
                EVALUATION_TERMINATION_GUARD_COUNT],
            interner_intern_calls: AtomicU64::new(0),
            interner_intern_hits: AtomicU64::new(0),
            interner_intern_misses: AtomicU64::new(0),
            interner_string_intern_calls: AtomicU64::new(0),
            interner_string_intern_cache_hits: AtomicU64::new(0),
            interner_type_list_intern_calls: AtomicU64::new(0),
            interner_object_shape_intern_calls: AtomicU64::new(0),
            interner_function_shape_intern_calls: AtomicU64::new(0),
            interner_callable_shape_intern_calls: AtomicU64::new(0),
            interner_application_intern_calls: AtomicU64::new(0),
            interner_conditional_intern_calls: AtomicU64::new(0),
            interner_mapped_intern_calls: AtomicU64::new(0),
            interner_lock_wait_histogram_ns: [const { AtomicU64::new(0) }; LOCK_WAIT_BUCKET_COUNT],
            interner_lookup_calls: AtomicU64::new(0),
            interner_lookup_tls_hits: AtomicU64::new(0),
            interner_lookup_cold_vec_fallbacks: AtomicU64::new(0),
            interner_lookup_tls_evictions: AtomicU64::new(0),
            interner_intern_tls_hits: AtomicU64::new(0),
            interner_intern_cold_fallbacks: AtomicU64::new(0),
            interner_intern_tls_evictions: AtomicU64::new(0),
            interner_working_set_distinct_max: AtomicU64::new(0),
            interner_working_set_files_over_cache: AtomicU64::new(0),
            interner_working_set_files_sampled: AtomicU64::new(0),
            interner_working_set_distinct_total: AtomicU64::new(0),
            interner_promote_tier_hits: AtomicU64::new(0),
            interner_promote_tier_misses: AtomicU64::new(0),
            compute_type_of_symbol_calls: AtomicU64::new(0),
            compute_type_of_symbol_cache_hits: AtomicU64::new(0),
            compute_type_of_symbol_interface_simple_object_fastpath_hits: AtomicU64::new(0),
            compute_type_of_symbol_source_outcome: [const { AtomicU64::new(0) };
                COMPUTE_TYPE_OF_SYMBOL_SOURCE_OUTCOME_COUNT],
            compute_type_of_symbol_kind_outcome: [const { AtomicU64::new(0) };
                COMPUTE_TYPE_OF_SYMBOL_KIND_OUTCOME_COUNT],
            compute_type_of_symbol_interface_fastpath_outcome: [const { AtomicU64::new(0) };
                COMPUTE_TYPE_OF_SYMBOL_INTERFACE_FASTPATH_OUTCOME_COUNT],
            compute_type_of_symbol_interface_callsite_outcome: [const { AtomicU64::new(0) };
                COMPUTE_TYPE_OF_SYMBOL_INTERFACE_CALLSITE_OUTCOME_COUNT],
            compute_type_of_symbol_interface_simple_object_outcome: [const { AtomicU64::new(0) };
                COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_OUTCOME_COUNT],
            compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kind: [const {
                AtomicU64::new(0)
            };
                COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_KIND_COUNT],
            compute_type_of_symbol_interface_simple_object_type_reference_reject_outcome: [const {
                AtomicU64::new(0)
            };
                COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_OUTCOME_COUNT],
            compute_type_of_symbol_interface_simple_object_actual_lib_type_reference_outcome:
                [const { AtomicU64::new(0) };
                    COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_ACTUAL_LIB_TYPE_REFERENCE_OUTCOME_COUNT],
            property_classification_calls: AtomicU64::new(0),
            property_classification_string_fallback_source_lookups: AtomicU64::new(0),
            property_classification_string_fallback_target_names: AtomicU64::new(0),
            property_classification_string_fallback_target_types: AtomicU64::new(0),
            resolver_lookup_calls: AtomicU64::new(0),
            resolver_is_file_calls: AtomicU64::new(0),
            resolver_is_dir_calls: AtomicU64::new(0),
            resolver_read_dir_calls: AtomicU64::new(0),
            resolver_read_package_json_calls: AtomicU64::new(0),
            resolver_candidate_paths_total: AtomicU64::new(0),
        }
    }
}

/// Get the process-wide counters. The first call also reads `TSZ_PERF_COUNTERS`
/// to set the `enabled` flag.
pub fn counters() -> &'static PerfCounters {
    COUNTERS.get_or_init(|| {
        let c = PerfCounters::new_zero();
        if std::env::var_os("TSZ_PERF_COUNTERS").is_some() {
            c.enabled.store(true, Ordering::Relaxed);
        }
        c
    })
}

/// Increment a counter when counters are enabled. The branch is the
/// only cost in the disabled case, which keeps production builds clean
/// without adding shared-cache-line traffic. See [`ENABLED_FAST`].
#[inline(always)]
pub fn inc(counter: &AtomicU64) {
    if enabled_fast() {
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

/// Add `n` to a counter when counters are enabled.
#[inline(always)]
pub fn add(counter: &AtomicU64, n: u64) {
    if enabled_fast() {
        counter.fetch_add(n, Ordering::Relaxed);
    }
}

/// Set the maximum-seen value for a counter, racy but good enough for
/// "max recursion depth" / "largest overlay clone" style reporting.
/// Gated by [`enabled_fast`] for the same contention-avoidance reason.
#[inline]
pub fn record_max(counter: &AtomicU64, value: u64) {
    if !enabled_fast() {
        return;
    }
    let mut current = counter.load(Ordering::Relaxed);
    while value > current {
        match counter.compare_exchange_weak(current, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(observed) => current = observed,
        }
    }
}



/// RAII guard that tracks recursion depth into
/// `delegate_cross_arena_symbol_resolution`. Each `enter_delegate()` increments
/// a thread-local counter and updates `delegate_max_recursion_depth` to the
/// running peak; the guard's `Drop` impl decrements when the call returns.
/// The whole thing short-circuits when counters are disabled, so timing builds
/// pay one branch per call.
pub struct DelegateDepthGuard(());

thread_local! {
    static DELEGATE_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

#[inline]
pub fn enter_delegate() -> DelegateDepthGuard {
    if !enabled_fast() {
        return DelegateDepthGuard(());
    }
    DELEGATE_DEPTH.with(|d| {
        let next = d.get().saturating_add(1);
        d.set(next);
        record_max(&counters().delegate_max_recursion_depth, u64::from(next));
    });
    DelegateDepthGuard(())
}

impl Drop for DelegateDepthGuard {
    fn drop(&mut self) {
        if !enabled_fast() {
            return;
        }
        DELEGATE_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// Returns true when `TSZ_PERF_COUNTERS` is set. Use this to gate the
/// expensive bookkeeping; the simple `inc` calls are always cheap enough
/// that gating them is more expensive than just doing them.
pub fn enabled() -> bool {
    counters().enabled.load(Ordering::Relaxed)
}


/// Time a contended write inside the type-interner. The closure runs in
/// both modes; the cost of the timing infrastructure is the
/// difference between the two `cfg`-gated implementations:
///
/// - **`perf-counters-timing` ON**: `Instant::now()` brackets the closure;
///   the elapsed nanos land in the lock-wait histogram (gated on
///   `enabled_fast()`, so timing-mode runs that don't enable counters
///   still pay only the gate load + closure call).
/// - **`perf-counters-timing` OFF (default)**: the wrapper compiles to a
///   direct call of `f()`. Zero `Instant::now()` calls, zero atomic
///   loads, zero histogram accesses. Default release builds do not pay
///   the timing cost the plan §4.T0.3 explicitly forbids.
///
/// `_shard_idx` is reserved for a future per-shard breakdown; today
/// every shard's observations land in the same global histogram.
#[cfg(feature = "perf-counters-timing")]
#[inline]
pub fn time_shard_write<R>(_shard_idx: u32, f: impl FnOnce() -> R) -> R {
    if !enabled_fast() {
        return f();
    }
    // `web_time::Instant` is the WASM-safe drop-in for `std::time::Instant`;
    // tsz-common is compiled for wasm32 and the arch guard bans the std
    // type even on cfg-gated paths. See `scripts/arch/arch_guard.py`.
    let start = web_time::Instant::now();
    let result = f();
    record_lock_wait_ns(start.elapsed().as_nanos() as u64);
    result
}

#[cfg(not(feature = "perf-counters-timing"))]
#[inline(always)]
pub fn time_shard_write<R>(_shard_idx: u32, f: impl FnOnce() -> R) -> R {
    f()
}

/// Whether the lock-wait histogram is *physically wired* (the
/// `perf-counters-timing` cfg feature is on). Independent of
/// `enabled_fast()`: a build with the feature on but the env var off
/// still has the histogram fields and serializes them as zeroes; a
/// build with the feature off keeps the histogram fields (so the
/// `PerfCounters` layout is feature-stable) but compiles out the
/// timing + recording logic and serializes the histogram as `null` via
/// [`PerfCounterSnapshot`].
#[inline(always)]
pub const fn lock_wait_histogram_wired() -> bool {
    cfg!(feature = "perf-counters-timing")
}
