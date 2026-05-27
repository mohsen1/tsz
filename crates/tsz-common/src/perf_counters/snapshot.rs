/// Stable schema version for `PerfCounterSnapshot`. Bump when the JSON
/// shape changes in a way the bench harness must adapt to.
pub const PERF_COUNTER_SNAPSHOT_SCHEMA_VERSION: u32 = 3;

/// Frozen value-object view of the counter state. Built by
/// [`PerfCounters::snapshot`]; serializable to JSON via serde.
///
/// Buckets that the producer code does not yet write are encoded as
/// [`Option<u64>::None`] (serializing as `null`) and the matching
/// [`WiredCounters`] field is `false`. That distinguishes "not measured"
/// from "measured zero" — without that, a reviewer staring at `0`
/// can't tell whether a counter site needs more wiring or is genuinely
/// idle.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PerfCounterSnapshot {
    pub schema_version: u32,
    /// `enabled_fast()` at snapshot time. When `false`, all counters are
    /// either zero (atomic loads return their initial state) or `null`
    /// (unwired buckets); the dump is included for schema stability so
    /// the bench harness can rely on the same shape every run.
    pub enabled: bool,
    /// Mirrors `PerfDiagnosticsReport.mode`: `"timing"` when counters
    /// are disabled, `"attribution"` when enabled.
    pub mode: &'static str,
    pub wired: WiredCounters,
    pub delegate: DelegateCounters,
    pub checker: CheckerCounters,
    pub identity: IdentityCounters,
    pub overlay: OverlayCounters,
    pub resolver: ResolverCounters,
    pub interner: InternerCounters,
    /// Per-`CheckerCreationReason` breakdown. Always
    /// `CHECKER_CREATION_REASON_COUNT` long; rows for inactive reasons
    /// carry all-zero counts (matching the text dump's filter behavior
    /// would force consumers to handle missing rows; emitting the full
    /// table keeps the JSON shape stable).
    pub by_reason: Vec<ByReasonRow>,
    /// `DelegateCrossArenaSymbol` miss classification.
    ///
    /// JSON counterpart of `dump_cross_arena_symbol_miss_classification`.
    /// Says how each miss reached the fallback child-checker path, so
    /// reviewers picking a T2.2 migration target can see whether
    /// `symbol_arenas` / `declaration_arenas` / `symbol_file_targets`
    /// dominates, and which symbol kinds are walking through the path.
    pub delegate_miss_classification: DelegateMissClassification,
    /// Bounded symbol-level attribution for declaration-file targets that
    /// still construct a `DelegateCrossArenaSymbol` child checker after the
    /// lib/direct lowering fast paths have declined.
    ///
    /// Captures at most `DELEGATE_DECLARATION_FILE_MISS_RESIDUE_LIMIT`
    /// distinct `(name, kind, source, target_file)` rows in perf-counter mode.
    /// This turns the remaining declaration-file residue from an aggregate
    /// count into the exact APIs the next T2.2 PR needs to prove safe.
    pub delegate_declaration_file_miss_residues: Vec<DelegateDeclarationFileMissResidue>,
    /// Bounded symbol-level attribution for source-file targets that still
    /// construct a `DelegateCrossArenaSymbol` child checker.
    ///
    /// Captures at most `DELEGATE_SOURCE_FILE_MISS_RESIDUE_LIMIT` distinct
    /// `(name, kind, source, target_file)` rows in perf-counter mode. This
    /// keeps source-project residue visible after declaration-file fast paths
    /// have removed most lib misses.
    pub delegate_source_file_miss_residues: Vec<DelegateSourceFileMissResidue>,
    /// Outcome buckets for the no-child alias shortcut attempted before
    /// constructing a `DelegateCrossArenaSymbol` child checker.
    ///
    /// JSON counterpart of `dump_cross_arena_alias_shortcut_outcomes`.
    /// Always `CROSS_ARENA_ALIAS_SHORTCUT_OUTCOME_COUNT` long, in
    /// `CROSS_ARENA_ALIAS_SHORTCUT_OUTCOME_NAMES` order. A high
    /// `not_alias` / `missing_module` / `default_import` count says the
    /// shortcut bailed for a structural reason; a high `success` count
    /// says the fast path is paying off.
    pub alias_shortcut_outcomes: Vec<NamedCount>,
    /// How `compute_type_of_symbol` sourced symbol payloads.
    ///
    /// Always `COMPUTE_TYPE_OF_SYMBOL_SOURCE_OUTCOME_COUNT` long, in
    /// `COMPUTE_TYPE_OF_SYMBOL_SOURCE_OUTCOME_NAMES` order.
    pub compute_type_of_symbol_source_outcomes: Vec<NamedCount>,
    /// Coarse symbol-kind buckets lowered by `compute_type_of_symbol`.
    ///
    /// Always `COMPUTE_TYPE_OF_SYMBOL_KIND_OUTCOME_COUNT` long, in
    /// `COMPUTE_TYPE_OF_SYMBOL_KIND_OUTCOME_NAMES` order.
    pub compute_type_of_symbol_kind_outcomes: Vec<NamedCount>,
    /// Interface-branch fast-path combinations observed inside
    /// `compute_type_of_symbol`.
    ///
    /// Always `COMPUTE_TYPE_OF_SYMBOL_INTERFACE_FASTPATH_OUTCOME_COUNT`
    /// long, in
    /// `COMPUTE_TYPE_OF_SYMBOL_INTERFACE_FASTPATH_OUTCOME_NAMES` order.
    pub compute_type_of_symbol_interface_fastpath_outcomes: Vec<NamedCount>,
    /// Call-site parent-kind attribution for interface-symbol calls in
    /// `compute_type_of_symbol`.
    ///
    /// Always `COMPUTE_TYPE_OF_SYMBOL_INTERFACE_CALLSITE_OUTCOME_COUNT`
    /// long, in
    /// `COMPUTE_TYPE_OF_SYMBOL_INTERFACE_CALLSITE_OUTCOME_NAMES` order.
    pub compute_type_of_symbol_interface_callsite_outcomes: Vec<NamedCount>,
    /// Success/reject outcomes for the simple local-interface object shortcut
    /// inside `compute_type_of_symbol`.
    ///
    /// Always `COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_OUTCOME_COUNT`
    /// long, in
    /// `COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_OUTCOME_NAMES` order.
    pub compute_type_of_symbol_interface_simple_object_outcomes: Vec<NamedCount>,
    /// Annotation-kind split for
    /// `compute_type_of_symbol_interface_simple_object_outcomes.reject_non_primitive_annotation`.
    ///
    /// Always
    /// `COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_KIND_COUNT`
    /// long, in
    /// `COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_KIND_NAMES`
    /// order.
    pub compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kinds:
        Vec<NamedCount>,
    /// Bounded source-level attribution for
    /// `compute_type_of_symbol_interface_simple_object_outcomes.reject_non_primitive_annotation`.
    ///
    /// Captures at most
    /// `COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_RESIDUE_LIMIT`
    /// distinct `(kind, interface, property)` rows in perf-counter mode. This
    /// names the sparse non-primitive residue before widening the guarded
    /// shortcut.
    pub compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues:
        Vec<ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationResidue>,
    /// Bounded symbol-level attribution for declaration/provenance guards in
    /// the simple local-interface object shortcut.
    ///
    /// Captures at most
    /// `COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_DECLARATION_PROVENANCE_RESIDUE_LIMIT`
    /// distinct `(outcome, symbol, declaration_count)` rows in perf-counter
    /// mode. This names the sparse `reject_out_of_arena_decl` /
    /// `reject_missing_interface_decl` residue before any behavior change.
    pub compute_type_of_symbol_interface_simple_object_declaration_provenance_residues:
        Vec<ComputeTypeOfSymbolInterfaceSimpleObjectDeclarationProvenanceResidue>,
    /// Attribution split for `type_reference` rows within
    /// `compute_type_of_symbol_interface_simple_object_outcomes.reject_non_primitive_annotation`.
    ///
    /// Always
    /// `COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_OUTCOME_COUNT`
    /// long, in
    /// `COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_OUTCOME_NAMES`
    /// order.
    pub compute_type_of_symbol_interface_simple_object_type_reference_reject_outcomes:
        Vec<NamedCount>,
    /// Bounded name-level attribution for `type_reference` rows within
    /// `compute_type_of_symbol_interface_simple_object_outcomes.reject_non_primitive_annotation`.
    ///
    /// Captures at most
    /// `COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_RESIDUE_LIMIT`
    /// distinct `(name, outcome)` rows in perf-counter mode. This makes the
    /// guarded shortcut's `identifier_not_found_symbol` residue actionable
    /// before relaxing symbol-resolution guards.
    pub compute_type_of_symbol_interface_simple_object_type_reference_reject_residues:
        Vec<ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectResidue>,
    /// Outcome buckets for direct cross-file interface lowering attempts.
    ///
    /// JSON counterpart of
    /// `dump_direct_cross_file_interface_lowering_outcomes`. Always
    /// `DIRECT_CROSS_FILE_INTERFACE_LOWERING_OUTCOME_COUNT` long, in
    /// `DIRECT_CROSS_FILE_INTERFACE_LOWERING_OUTCOME_NAMES` order. The
    /// non-`success` rows show which structural reasons keep the fast
    /// path from firing — the target list for "widen the direct
    /// lowering" follow-ups.
    pub direct_interface_lowering_outcomes: Vec<NamedCount>,
    /// Outcome buckets for direct actual-lib alias-body attempts.
    ///
    /// Always `DIRECT_ACTUAL_LIB_ALIAS_BODY_OUTCOME_COUNT` long, in
    /// `DIRECT_ACTUAL_LIB_ALIAS_BODY_OUTCOME_NAMES` order. These buckets say
    /// whether an actual bundled-lib alias was admitted by the typed body
    /// helper, rejected by the current conservative name gate, or rejected
    /// because the resolver/definition-store proof was incomplete.
    pub direct_actual_lib_alias_body_outcomes: Vec<NamedCount>,
    /// Outcome buckets for direct source-file type-alias lowering attempts.
    ///
    /// Always `DIRECT_SOURCE_FILE_TYPE_ALIAS_LOWERING_OUTCOME_COUNT` long, in
    /// `DIRECT_SOURCE_FILE_TYPE_ALIAS_LOWERING_OUTCOME_NAMES` order. These
    /// buckets split regular source-file aliases by the structural proof that
    /// made the direct path succeed or fall back to child-checker delegation.
    pub direct_source_file_type_alias_lowering_outcomes: Vec<NamedCount>,
    /// Root syntax families for source-file alias bodies rejected by the
    /// direct-lowering proof.
    ///
    /// Always `DIRECT_SOURCE_FILE_TYPE_ALIAS_BODY_REJECTION_KIND_COUNT` long,
    /// in `DIRECT_SOURCE_FILE_TYPE_ALIAS_BODY_REJECTION_KIND_NAMES` order.
    /// These buckets classify the dominant `body_not_direct_lowerable` outcome
    /// without depending on user-chosen alias names.
    pub direct_source_file_type_alias_body_rejection_kinds: Vec<NamedCount>,
    /// Structural sub-buckets for root `TypeReference` source-file alias bodies
    /// rejected by the direct-lowering proof.
    ///
    /// Always `DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_COUNT`
    /// long, in `DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_NAMES`
    /// order. These buckets classify referenced symbol shape and type-argument
    /// shape without recording user-written names.
    pub direct_source_file_type_alias_type_reference_rejection_kinds: Vec<NamedCount>,
    /// First nested `TypeReference` rejection bucket per rejected source-file
    /// alias body.
    ///
    /// Always `DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_COUNT`
    /// long, in `DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_NAMES`
    /// order. Unlike the all-refs counter above, these buckets add up to at
    /// most one count per `body_not_direct_lowerable` alias with a type
    /// reference in its rejected body.
    pub direct_source_file_type_alias_first_type_reference_rejection_kinds: Vec<NamedCount>,
    /// Bounded alias-level attribution for source-file alias bodies rejected
    /// by the direct-lowering proof.
    ///
    /// Captures at most
    /// `DIRECT_SOURCE_FILE_TYPE_ALIAS_BODY_REJECTION_RESIDUE_LIMIT` distinct
    /// `(name, body_kind, first_type_reference_kind, first_type_reference_name,
    /// target_file)` rows in perf-counter mode. This keeps the aggregate
    /// `body_not_direct_lowerable` residue targetable without using names as
    /// compiler policy.
    pub direct_source_file_type_alias_body_rejection_residues:
        Vec<DirectSourceFileTypeAliasBodyRejectionResidue>,
    /// Outcome buckets for direct actual-lib Intl interface attempts.
    ///
    /// Always `DIRECT_ACTUAL_LIB_INTL_INTERFACE_OUTCOME_COUNT` long, in
    /// `DIRECT_ACTUAL_LIB_INTL_INTERFACE_OUTCOME_NAMES` order. This splits
    /// success/fallback reasons for the Intl value-interface lane so
    /// declaration-file miss residues can be traced to a specific gate.
    pub direct_actual_lib_intl_interface_outcomes: Vec<NamedCount>,
    /// Why each `cached_cross_file_*` reader returned `None`.
    ///
    /// Always `CROSS_FILE_CACHE_MISS_CAUSE_COUNT` long, in
    /// `CROSS_FILE_CACHE_MISS_CAUSE_NAMES` order. The 2026-05-11
    /// attribution decision record locked in
    /// `delegate.cache_hits_cross_file = 0`; this array splits that
    /// flat miss number into structural root causes so the next T2.2
    /// architecture PR can target the dominant cause directly.
    ///
    /// Sum of all rows equals the total miss count across the four
    /// reader helpers in
    /// `crates/tsz-checker/src/context/cross_file_query.rs`.
    pub cross_file_cache_miss_causes: Vec<NamedCount>,
    /// Source-file symbol-arena cache eligibility and rejection reasons.
    ///
    /// Always `SOURCE_FILE_SYMBOL_ARENA_CACHE_ELIGIBILITY_OUTCOME_COUNT`
    /// long, in `SOURCE_FILE_SYMBOL_ARENA_CACHE_ELIGIBILITY_OUTCOME_NAMES`
    /// order. This splits the post-#6191 `symbol_arenas` residue into
    /// cacheable first misses versus structural non-cacheable cases.
    pub source_file_symbol_arena_cache_eligibility_outcomes: Vec<NamedCount>,
    /// Top semantic `check_source_file` durations observed in attribution mode.
    ///
    /// This is a bounded list of the slowest files, sorted by descending
    /// elapsed time. It is empty when `TSZ_PERF_COUNTERS` is unset or no file
    /// ran semantic checking.
    pub slow_check_file_timings: Vec<SlowCheckFileTiming>,
}

/// Per-bucket "is this wired up to its producer?" flag. Lets the bench
/// harness emit a clean follow-up list without parsing the whole
/// snapshot.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct WiredCounters {
    pub delegate_cross_arena: bool,
    pub checker_construction: bool,
    pub property_classification: bool,
    pub overlay_copy: bool,
    pub interner_intern_calls: bool,
    pub interner_per_kind: bool,
    pub interner_lock_wait: bool,
    pub resolver_lookup: bool,
    pub resolver_fs_probes: bool,
    pub compute_type_of_symbol: bool,
    pub stable_identity: bool,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct DelegateCounters {
    pub calls: u64,
    pub cache_hits_lib: u64,
    pub cache_hits_cross_file: u64,
    pub misses: u64,
    pub max_recursion_depth: u64,
    /// T2.2 typed-query memo: hits on the cross-file type-parameter cache.
    pub cross_file_type_params_cache_hits: u64,
    /// T2.2 typed-query memo: misses (where the slow path constructed a child checker).
    pub cross_file_type_params_cache_misses: u64,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct CheckerCounters {
    pub state_constructed: u64,
    pub with_parent_cache_constructed: u64,
    /// `CheckerContext::reset_for_next_file()` invocations. Zero on the
    /// default construction-per-file path, nonzero only on a sequential
    /// session-reuse path (T2.1.B). Reuse vs. construct is the comparison
    /// against `state_constructed`.
    pub file_session_resets: u64,
    pub compute_type_of_symbol_calls: u64,
    pub compute_type_of_symbol_cache_hits: u64,
    pub compute_type_of_symbol_interface_simple_object_fastpath_hits: u64,
    pub property_classification_calls: u64,
    pub property_classification_string_fallback_source_lookups: u64,
    pub property_classification_string_fallback_target_names: u64,
    pub property_classification_string_fallback_target_types: u64,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct IdentityCounters {
    /// Raw `SymbolId`-shaped `DefId` redirects inside
    /// `TypeEnvironment::resolve_lazy`.
    pub type_environment_raw_symbol_lazy_fallbacks: u64,
}

/// One `(name, count)` row in a named-counter JSON array.
///
/// Used for the `alias_shortcut_outcomes`,
/// `compute_type_of_symbol_*_outcomes`,
/// `compute_type_of_symbol_interface_fastpath_outcomes`, and
/// `compute_type_of_symbol_interface_callsite_outcomes`,
/// `compute_type_of_symbol_interface_simple_object_outcomes`, and
/// `direct_interface_lowering_outcomes` arrays on
/// [`PerfCounterSnapshot`]. Each array is always emitted at its full
/// declared length, with zero counts for inactive buckets, so the JSON
/// shape stays stable across runs and consumers can index by name
/// without re-parsing the source enum.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct NamedCount {
    /// Stable, human-readable bucket name (from the matching `*_NAMES`
    /// constant in this module).
    pub name: &'static str,
    /// Atomic load at snapshot time. Zero means "this bucket was not
    /// hit", not "the producer is unwired"; per-bucket wiring is
    /// project-wide for these counters.
    pub count: u64,
}

/// `DelegateCrossArenaSymbol` miss classification, as JSON.
///
/// Counterpart of `dump_cross_arena_symbol_miss_classification`'s text
/// dump. Says *why* a delegate path missed both caches and the alias
/// shortcut — i.e. which fast paths the next T2.2 migration could
/// plausibly cover.
///
/// The `by_source` and `by_kind` arrays are always emitted at their
/// full `*_NAMES` length so consumers can index by position. The two
/// scalar totals are the declaration-file vs. source-file split that
/// the text dump prints at the end of the classification block.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DelegateMissClassification {
    /// How the target arena was discovered. Always
    /// `CROSS_ARENA_SYMBOL_MISS_SOURCE_COUNT` long, in
    /// `CROSS_ARENA_SYMBOL_MISS_SOURCE_NAMES` order.
    pub by_source: Vec<NamedCount>,
    /// Coarse symbol-kind bucket for the miss. Always
    /// `CROSS_ARENA_SYMBOL_MISS_KIND_COUNT` long, in
    /// `CROSS_ARENA_SYMBOL_MISS_KIND_NAMES` order.
    pub by_kind: Vec<NamedCount>,
    /// Misses whose target arena's primary source file is a declaration
    /// file (`.d.ts` / `.d.cts` / `.d.mts`).
    pub target_declaration_files: u64,
    /// Misses whose target arena's primary source file is a regular
    /// source file (not a declaration file).
    pub target_source_files: u64,
}

/// One row in the per-`CheckerCreationReason` JSON breakdown.
///
/// Counterpart of one row in `dump_by_reason`'s text dump, lifted into
/// machine-readable form so the bench harness and offline analysis tools
/// (`scripts/conformance/query-conformance.py`-style readers) can pick
/// the next T2.2 migration target from data instead of `dump_string`
/// parsing.
///
/// Reason names match `REASON_NAMES`. A future-added variant lands as a
/// new row automatically — the array is always `CHECKER_CREATION_REASON_COUNT`
/// long, so consumers don't need to special-case unknown reasons.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ByReasonRow {
    /// Stable, human-readable name (from `REASON_NAMES`).
    pub reason: &'static str,
    /// `with_parent_cache` constructions attributed to this reason.
    /// Sums to `checker.with_parent_cache_constructed` across all rows.
    pub with_parent_cache_constructed: u64,
    /// `copy_symbol_file_targets` invocations attributed to this reason.
    /// Sums to `overlay.copy_calls` across all rows.
    pub overlay_copy_calls: u64,
    /// Cumulative entries copied across all overlay copies for this reason.
    /// Sums to `overlay.entries_total` across all rows.
    pub overlay_copy_entries: u64,
    /// High-water mark of the per-overlay-copy entries count for this reason.
    /// NOT a sum — this is `max` across calls. Useful for spotting one
    /// pathological copy hiding inside an otherwise reasonable bucket.
    pub overlay_copy_max_entries: u64,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct OverlayCounters {
    pub copy_calls: u64,
    pub entries_total: u64,
    pub entries_max: u64,
    pub len_ge_1k: u64,
    pub len_ge_10k: u64,
    pub len_ge_100k: u64,
    pub len_ge_1m: u64,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ResolverCounters {
    pub lookup_calls: u64,
    /// Filesystem probe counts. `None` until a counting filesystem
    /// wrapper lands (`PERFORMANCE_PLAN.md` §5).
    pub is_file_calls: Option<u64>,
    pub is_dir_calls: Option<u64>,
    pub read_dir_calls: Option<u64>,
    pub package_json_reads: u64,
    pub candidate_paths_total: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct InternerCounters {
    /// Total `intern` calls across kinds. `None` until the solver intern
    /// site is updated to fan into a single counter.
    pub intern_calls: Option<u64>,
    pub intern_hits: Option<u64>,
    pub intern_misses: Option<u64>,
    pub string_intern_calls: u64,
    pub type_list_intern_calls: u64,
    pub object_shape_intern_calls: u64,
    pub function_shape_intern_calls: u64,
    pub callable_shape_intern_calls: u64,
    pub application_intern_calls: u64,
    pub conditional_intern_calls: u64,
    pub mapped_intern_calls: u64,
    /// Lock-wait histogram. `None` because the timing path is gated on
    /// the `perf-counters-timing` feature (`PERFORMANCE_PLAN.md` §4.T0.3).
    pub lock_wait_histogram_ns: Option<Vec<u64>>,
}

impl PerfCounters {
    /// Load every atomic into a [`PerfCounterSnapshot`] in a single pass.
    /// Cheap (one relaxed load per counter); both `dump_string` and
    /// `write_json_to` should eventually share this path so they cannot
    /// drift.
    pub fn snapshot() -> PerfCounterSnapshot {
        let c = counters();
        let load = |a: &std::sync::atomic::AtomicU64| a.load(std::sync::atomic::Ordering::Relaxed);
        let enabled = enabled_fast();
        PerfCounterSnapshot {
            schema_version: PERF_COUNTER_SNAPSHOT_SCHEMA_VERSION,
            enabled,
            mode: if enabled { "attribution" } else { "timing" },
            wired: WiredCounters {
                delegate_cross_arena: true,
                checker_construction: true,
                property_classification: true,
                overlay_copy: true,
                interner_intern_calls: true,
                interner_per_kind: true,
                interner_lock_wait: lock_wait_histogram_wired(),
                resolver_lookup: true,
                resolver_fs_probes: true,
                compute_type_of_symbol: true,
                stable_identity: true,
            },
            delegate: DelegateCounters {
                calls: load(&c.delegate_cross_arena_calls),
                cache_hits_lib: load(&c.delegate_cross_arena_cache_hits_lib),
                cache_hits_cross_file: load(&c.delegate_cross_arena_cache_hits_cross_file),
                misses: load(&c.delegate_cross_arena_misses),
                max_recursion_depth: load(&c.delegate_max_recursion_depth),
                cross_file_type_params_cache_hits: load(&c.cross_file_type_params_cache_hits),
                cross_file_type_params_cache_misses: load(&c.cross_file_type_params_cache_misses),
            },
            checker: CheckerCounters {
                state_constructed: load(&c.checker_state_constructed),
                with_parent_cache_constructed: load(&c.checker_state_with_parent_cache_constructed),
                file_session_resets: load(&c.file_session_resets),
                compute_type_of_symbol_calls: load(&c.compute_type_of_symbol_calls),
                compute_type_of_symbol_cache_hits: load(&c.compute_type_of_symbol_cache_hits),
                compute_type_of_symbol_interface_simple_object_fastpath_hits: load(
                    &c.compute_type_of_symbol_interface_simple_object_fastpath_hits,
                ),
                property_classification_calls: load(&c.property_classification_calls),
                property_classification_string_fallback_source_lookups: load(
                    &c.property_classification_string_fallback_source_lookups,
                ),
                property_classification_string_fallback_target_names: load(
                    &c.property_classification_string_fallback_target_names,
                ),
                property_classification_string_fallback_target_types: load(
                    &c.property_classification_string_fallback_target_types,
                ),
            },
            identity: IdentityCounters {
                type_environment_raw_symbol_lazy_fallbacks: load(
                    &c.type_environment_raw_symbol_lazy_fallbacks,
                ),
            },
            overlay: OverlayCounters {
                copy_calls: load(&c.copy_symbol_file_targets_calls),
                entries_total: load(&c.copy_symbol_file_targets_entries_total),
                entries_max: load(&c.copy_symbol_file_targets_entries_max),
                len_ge_1k: load(&c.copy_symbol_file_targets_len_ge_1k),
                len_ge_10k: load(&c.copy_symbol_file_targets_len_ge_10k),
                len_ge_100k: load(&c.copy_symbol_file_targets_len_ge_100k),
                len_ge_1m: load(&c.copy_symbol_file_targets_len_ge_1m),
            },
            resolver: ResolverCounters {
                lookup_calls: load(&c.resolver_lookup_calls),
                is_file_calls: Some(load(&c.resolver_is_file_calls)),
                is_dir_calls: Some(load(&c.resolver_is_dir_calls)),
                read_dir_calls: Some(load(&c.resolver_read_dir_calls)),
                package_json_reads: load(&c.resolver_read_package_json_calls),
                candidate_paths_total: load(&c.resolver_candidate_paths_total),
            },
            interner: InternerCounters {
                intern_calls: Some(load(&c.interner_intern_calls)),
                intern_hits: Some(load(&c.interner_intern_hits)),
                intern_misses: Some(load(&c.interner_intern_misses)),
                string_intern_calls: load(&c.interner_string_intern_calls),
                type_list_intern_calls: load(&c.interner_type_list_intern_calls),
                object_shape_intern_calls: load(&c.interner_object_shape_intern_calls),
                function_shape_intern_calls: load(&c.interner_function_shape_intern_calls),
                callable_shape_intern_calls: load(&c.interner_callable_shape_intern_calls),
                application_intern_calls: load(&c.interner_application_intern_calls),
                conditional_intern_calls: load(&c.interner_conditional_intern_calls),
                mapped_intern_calls: load(&c.interner_mapped_intern_calls),
                // Lock-wait histogram surfaces only in builds where the
                // `perf-counters-timing` feature is on; otherwise the
                // wrapper is a no-op and the buckets stay all-zero, so
                // emitting `null` keeps "wired vs. zero" unambiguous in
                // the JSON output (matching the plan §4.T0.3 contract).
                lock_wait_histogram_ns: if lock_wait_histogram_wired() {
                    Some(c.interner_lock_wait_histogram_ns.iter().map(load).collect())
                } else {
                    None
                },
            },
            by_reason: (0..CHECKER_CREATION_REASON_COUNT)
                .map(|i| ByReasonRow {
                    reason: REASON_NAMES[i],
                    with_parent_cache_constructed: load(&c.with_parent_cache_by_reason[i]),
                    overlay_copy_calls: load(&c.overlay_copy_calls_by_reason[i]),
                    overlay_copy_entries: load(&c.overlay_copy_entries_by_reason[i]),
                    overlay_copy_max_entries: load(&c.overlay_copy_max_entries_by_reason[i]),
                })
                .collect(),
            delegate_miss_classification: DelegateMissClassification {
                by_source: (0..CROSS_ARENA_SYMBOL_MISS_SOURCE_COUNT)
                    .map(|i| NamedCount {
                        name: CROSS_ARENA_SYMBOL_MISS_SOURCE_NAMES[i],
                        count: load(&c.delegate_cross_arena_symbol_miss_by_source[i]),
                    })
                    .collect(),
                by_kind: (0..CROSS_ARENA_SYMBOL_MISS_KIND_COUNT)
                    .map(|i| NamedCount {
                        name: CROSS_ARENA_SYMBOL_MISS_KIND_NAMES[i],
                        count: load(&c.delegate_cross_arena_symbol_miss_by_kind[i]),
                    })
                    .collect(),
                target_declaration_files: load(
                    &c.delegate_cross_arena_symbol_miss_target_declaration_file,
                ),
                target_source_files: load(&c.delegate_cross_arena_symbol_miss_target_source_file),
            },
            delegate_declaration_file_miss_residues:
                Self::snapshot_delegate_declaration_file_miss_residues(),
            delegate_source_file_miss_residues: Self::snapshot_delegate_source_file_miss_residues(),
            alias_shortcut_outcomes: (0..CROSS_ARENA_ALIAS_SHORTCUT_OUTCOME_COUNT)
                .map(|i| NamedCount {
                    name: CROSS_ARENA_ALIAS_SHORTCUT_OUTCOME_NAMES[i],
                    count: load(&c.delegate_cross_arena_alias_shortcut_outcome[i]),
                })
                .collect(),
            compute_type_of_symbol_source_outcomes: (0
                ..COMPUTE_TYPE_OF_SYMBOL_SOURCE_OUTCOME_COUNT)
                .map(|i| NamedCount {
                    name: COMPUTE_TYPE_OF_SYMBOL_SOURCE_OUTCOME_NAMES[i],
                    count: load(&c.compute_type_of_symbol_source_outcome[i]),
                })
                .collect(),
            compute_type_of_symbol_kind_outcomes: (0..COMPUTE_TYPE_OF_SYMBOL_KIND_OUTCOME_COUNT)
                .map(|i| NamedCount {
                    name: COMPUTE_TYPE_OF_SYMBOL_KIND_OUTCOME_NAMES[i],
                    count: load(&c.compute_type_of_symbol_kind_outcome[i]),
                })
                .collect(),
            compute_type_of_symbol_interface_fastpath_outcomes: (0
                ..COMPUTE_TYPE_OF_SYMBOL_INTERFACE_FASTPATH_OUTCOME_COUNT)
                .map(|i| NamedCount {
                    name: COMPUTE_TYPE_OF_SYMBOL_INTERFACE_FASTPATH_OUTCOME_NAMES[i],
                    count: load(&c.compute_type_of_symbol_interface_fastpath_outcome[i]),
                })
                .collect(),
            compute_type_of_symbol_interface_callsite_outcomes: (0
                ..COMPUTE_TYPE_OF_SYMBOL_INTERFACE_CALLSITE_OUTCOME_COUNT)
                .map(|i| NamedCount {
                    name: COMPUTE_TYPE_OF_SYMBOL_INTERFACE_CALLSITE_OUTCOME_NAMES[i],
                    count: load(&c.compute_type_of_symbol_interface_callsite_outcome[i]),
                })
                .collect(),
            compute_type_of_symbol_interface_simple_object_outcomes: (0
                ..COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_OUTCOME_COUNT)
                .map(|i| NamedCount {
                    name: COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_OUTCOME_NAMES[i],
                    count: load(&c.compute_type_of_symbol_interface_simple_object_outcome[i]),
                })
                .collect(),
            compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kinds: (0
                ..COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_KIND_COUNT)
                .map(|i| NamedCount {
                    name: COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_KIND_NAMES[i],
                    count: load(
                        &c.compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kind
                            [i],
                    ),
                })
                .collect(),
            compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues:
                Self::snapshot_compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues(),
            compute_type_of_symbol_interface_simple_object_declaration_provenance_residues:
                Self::snapshot_compute_type_of_symbol_interface_simple_object_declaration_provenance_residues(),
            compute_type_of_symbol_interface_simple_object_type_reference_reject_outcomes: (0
                ..COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_OUTCOME_COUNT)
                .map(|i| NamedCount {
                    name: COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_OUTCOME_NAMES[i],
                    count: load(
                        &c.compute_type_of_symbol_interface_simple_object_type_reference_reject_outcome
                            [i],
                    ),
                })
                .collect(),
            compute_type_of_symbol_interface_simple_object_type_reference_reject_residues:
                Self::snapshot_compute_type_of_symbol_interface_simple_object_type_reference_reject_residues(),
            direct_interface_lowering_outcomes: (0
                ..DIRECT_CROSS_FILE_INTERFACE_LOWERING_OUTCOME_COUNT)
                .map(|i| NamedCount {
                    name: DIRECT_CROSS_FILE_INTERFACE_LOWERING_OUTCOME_NAMES[i],
                    count: load(&c.direct_cross_file_interface_lowering_outcome[i]),
                })
                .collect(),
            direct_actual_lib_alias_body_outcomes: (0..DIRECT_ACTUAL_LIB_ALIAS_BODY_OUTCOME_COUNT)
                .map(|i| NamedCount {
                    name: DIRECT_ACTUAL_LIB_ALIAS_BODY_OUTCOME_NAMES[i],
                    count: load(&c.direct_actual_lib_alias_body_outcome[i]),
                })
                .collect(),
            direct_source_file_type_alias_lowering_outcomes: (0
                ..DIRECT_SOURCE_FILE_TYPE_ALIAS_LOWERING_OUTCOME_COUNT)
                .map(|i| NamedCount {
                    name: DIRECT_SOURCE_FILE_TYPE_ALIAS_LOWERING_OUTCOME_NAMES[i],
                    count: load(&c.direct_source_file_type_alias_lowering_outcome[i]),
                })
                .collect(),
            direct_source_file_type_alias_body_rejection_kinds: (0
                ..DIRECT_SOURCE_FILE_TYPE_ALIAS_BODY_REJECTION_KIND_COUNT)
                .map(|i| NamedCount {
                    name: DIRECT_SOURCE_FILE_TYPE_ALIAS_BODY_REJECTION_KIND_NAMES[i],
                    count: load(&c.direct_source_file_type_alias_body_rejection_kind[i]),
                })
                .collect(),
            direct_source_file_type_alias_type_reference_rejection_kinds: (0
                ..DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_COUNT)
                .map(|i| NamedCount {
                    name: DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_NAMES[i],
                    count: load(
                        &c.direct_source_file_type_alias_type_reference_rejection_kind[i],
                    ),
                })
                .collect(),
            direct_source_file_type_alias_first_type_reference_rejection_kinds: (0
                ..DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_COUNT)
                .map(|i| NamedCount {
                    name: DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_NAMES[i],
                    count: load(
                        &c.direct_source_file_type_alias_first_type_reference_rejection_kind[i],
                    ),
                })
                .collect(),
            direct_source_file_type_alias_body_rejection_residues:
                Self::snapshot_direct_source_file_type_alias_body_rejection_residues(),
            direct_actual_lib_intl_interface_outcomes: (0
                ..DIRECT_ACTUAL_LIB_INTL_INTERFACE_OUTCOME_COUNT)
                .map(|i| NamedCount {
                    name: DIRECT_ACTUAL_LIB_INTL_INTERFACE_OUTCOME_NAMES[i],
                    count: load(&c.direct_actual_lib_intl_interface_outcome[i]),
                })
                .collect(),
            cross_file_cache_miss_causes: (0..CROSS_FILE_CACHE_MISS_CAUSE_COUNT)
                .map(|i| NamedCount {
                    name: CROSS_FILE_CACHE_MISS_CAUSE_NAMES[i],
                    count: load(&c.cross_file_cache_miss_cause[i]),
                })
                .collect(),
            source_file_symbol_arena_cache_eligibility_outcomes: (0
                ..SOURCE_FILE_SYMBOL_ARENA_CACHE_ELIGIBILITY_OUTCOME_COUNT)
                .map(|i| NamedCount {
                    name: SOURCE_FILE_SYMBOL_ARENA_CACHE_ELIGIBILITY_OUTCOME_NAMES[i],
                    count: load(&c.source_file_symbol_arena_cache_eligibility_outcome[i]),
                })
                .collect(),
            slow_check_file_timings: Self::snapshot_slow_check_file_timings(),
        }
    }

    fn snapshot_delegate_declaration_file_miss_residues() -> Vec<DelegateDeclarationFileMissResidue>
    {
        let mut rows = delegate_declaration_file_miss_residues()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        rows.sort_by(|a, b| {
            b.count.cmp(&a.count).then_with(|| {
                a.name
                    .cmp(&b.name)
                    .then_with(|| a.kind.cmp(b.kind))
                    .then_with(|| a.source.cmp(b.source))
                    .then_with(|| a.target_file.cmp(&b.target_file))
            })
        });
        rows
    }

    fn snapshot_delegate_source_file_miss_residues() -> Vec<DelegateSourceFileMissResidue> {
        let mut rows = delegate_source_file_miss_residues()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        rows.sort_by(|a, b| {
            b.count.cmp(&a.count).then_with(|| {
                a.name
                    .cmp(&b.name)
                    .then_with(|| a.kind.cmp(b.kind))
                    .then_with(|| a.source.cmp(b.source))
                    .then_with(|| a.target_file.cmp(&b.target_file))
            })
        });
        rows
    }

    fn snapshot_direct_source_file_type_alias_body_rejection_residues(
    ) -> Vec<DirectSourceFileTypeAliasBodyRejectionResidue> {
        let mut rows = direct_source_file_type_alias_body_rejection_residues()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        rows.sort_by(|a, b| {
            b.count.cmp(&a.count).then_with(|| {
                a.name
                    .cmp(&b.name)
                    .then_with(|| a.body_kind.cmp(b.body_kind))
                    .then_with(|| {
                        a.first_type_reference_kind
                            .cmp(&b.first_type_reference_kind)
                    })
                    .then_with(|| {
                        a.first_type_reference_name
                            .cmp(&b.first_type_reference_name)
                    })
                    .then_with(|| {
                        a.first_non_lowerable_type_reference_kind
                            .cmp(&b.first_non_lowerable_type_reference_kind)
                    })
                    .then_with(|| {
                        a.first_non_lowerable_type_reference_name
                            .cmp(&b.first_non_lowerable_type_reference_name)
                    })
                    .then_with(|| {
                        a.first_non_lowerable_leaf_type_reference_kind
                            .cmp(&b.first_non_lowerable_leaf_type_reference_kind)
                    })
                    .then_with(|| {
                        a.first_non_lowerable_leaf_type_reference_name
                            .cmp(&b.first_non_lowerable_leaf_type_reference_name)
                    })
                    .then_with(|| a.target_file.cmp(&b.target_file))
            })
        });
        rows
    }

    fn snapshot_compute_type_of_symbol_interface_simple_object_type_reference_reject_residues(
    ) -> Vec<ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectResidue> {
        let mut rows =
            compute_type_of_symbol_interface_simple_object_type_reference_reject_residues()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
        rows.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.outcome.cmp(b.outcome))
        });
        rows
    }

    fn snapshot_compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues(
    ) -> Vec<ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationResidue> {
        let mut rows =
            compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
        rows.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| a.kind.cmp(b.kind))
                .then_with(|| a.interface.cmp(&b.interface))
                .then_with(|| a.property.cmp(&b.property))
        });
        rows
    }

    fn snapshot_compute_type_of_symbol_interface_simple_object_declaration_provenance_residues(
    ) -> Vec<ComputeTypeOfSymbolInterfaceSimpleObjectDeclarationProvenanceResidue> {
        let mut rows =
            compute_type_of_symbol_interface_simple_object_declaration_provenance_residues()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
        rows.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| a.outcome.cmp(b.outcome))
                .then_with(|| a.symbol.cmp(&b.symbol))
                .then_with(|| a.declaration_count.cmp(&b.declaration_count))
        });
        rows
    }

    fn snapshot_slow_check_file_timings() -> Vec<SlowCheckFileTiming> {
        let mut rows = slow_check_file_timings()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        rows.sort_by(|a, b| {
            b.elapsed_ms
                .total_cmp(&a.elapsed_ms)
                .then_with(|| a.file.cmp(&b.file))
        });
        rows.truncate(SLOW_CHECK_FILE_TIMING_LIMIT);
        rows
    }

    /// Serialize a [`PerfCounterSnapshot`] to `path` using an atomic
    /// rename so a partial write can't poison the bench harness's `jq`
    /// consumer.
    pub fn write_json_to(path: &std::path::Path) -> std::io::Result<()> {
        let snap = Self::snapshot();
        let json = serde_json::to_string_pretty(&snap)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}
