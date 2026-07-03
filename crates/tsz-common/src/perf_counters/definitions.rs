use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

/// Cache-line-padded `AtomicU64` to avoid false sharing between hot
/// counters that get hammered concurrently. 64 is the typical cache-line
/// size on x86-64 and Apple Silicon's M-series; over-aligning is harmless.
///
/// Defined for use by future PRs. Not adopted in PR #1631 because the
/// [`enabled_fast`] gate already eliminates the false-sharing problem
/// in production builds (where the env var is unset, the counter writes
/// don't fire at all). Inside profiling runs we accept some perturbation;
/// when we want profiler-grade fidelity for the highest-frequency
/// counters we'll switch their fields to `PaddedAtomicU64` and call
/// `field.0.fetch_add(...)` directly. See PR #1630 review issue #2.
#[repr(align(64))]
pub struct PaddedAtomicU64(pub AtomicU64);

impl PaddedAtomicU64 {
    pub const fn new(v: u64) -> Self {
        Self(AtomicU64::new(v))
    }
}

/// Process-wide enabled flag for the perf counters. Initialized exactly
/// once on first observation and read on every counter increment via
/// [`enabled_fast`]; the increment then becomes a single predictable
/// branch that's elided in the disabled case so production builds (where
/// `TSZ_PERF_COUNTERS` is unset) pay only the cost of the load.
///
/// Why this matters: even `AtomicU64::fetch_add(_, Relaxed)` is a
/// read-modify-write on a shared cache line. On the exact codebase where
/// we're trying to measure parallel-work contention, leaving the atomic
/// always-firing creates a synthetic contention point that distorts the
/// numbers we're trying to collect.
static ENABLED_FAST: OnceLock<bool> = OnceLock::new();

/// Test-only override so unit tests that assert on counter deltas can opt
/// into counting without depending on the `TSZ_PERF_COUNTERS` env var (and
/// without being defeated by the `ENABLED_FAST` `OnceLock` having already
/// latched `false`). Only compiled in test/debug builds; production builds
/// keep the single env-gated `OnceLock` read.
#[cfg(any(test, debug_assertions))]
static FORCE_ENABLED_FOR_TESTS: AtomicBool = AtomicBool::new(false);

/// Cheap O(1) gate readable from any hot path. Reads a `OnceLock<bool>`
/// (one branch + one load) instead of going through `counters().enabled`
/// (deref-via-OnceLock + load).
#[inline(always)]
pub fn enabled_fast() -> bool {
    #[cfg(any(test, debug_assertions))]
    if FORCE_ENABLED_FOR_TESTS.load(Ordering::Relaxed) {
        return true;
    }
    *ENABLED_FAST.get_or_init(|| std::env::var_os("TSZ_PERF_COUNTERS").is_some())
}

/// Force the perf-counter gate on for the current process. Test-only; lets a
/// unit test observe `fetch_add` deltas regardless of env or `OnceLock` state.
#[cfg(any(test, debug_assertions))]
pub fn force_enable_perf_counters_for_tests() {
    FORCE_ENABLED_FOR_TESTS.store(true, Ordering::Relaxed);
}

// Declarative manifest for enum-backed counter families. Each entry owns the
// Rust variant, stable numeric index, and dump/JSON display name together so
// adding a bucket does not require editing parallel count/name tables by hand.
macro_rules! perf_counter_enum {
    (
        $(#[$enum_meta:meta])*
        pub enum $enum_name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident = $index:expr => $name:literal,
            )+
        }

        pub const $count_name:ident;
        pub const $names_name:ident;
    ) => {
        $(#[$enum_meta])*
        #[repr(usize)]
        pub enum $enum_name {
            $(
                $(#[$variant_meta])*
                $variant = $index,
            )+
        }

        pub const $count_name: usize = [$($name),+].len();

        pub const $names_name: [&str; $count_name] = [
            $($name,)+
        ];

        impl $enum_name {
            #[inline(always)]
            pub const fn as_index(self) -> usize {
                self as usize
            }

            pub const fn name(self) -> &'static str {
                $names_name[self as usize]
            }
        }
    };
}

perf_counter_enum! {
    /// Why a `CheckerState::with_parent_cache` (and the matching
    /// `copy_symbol_file_targets_to`) call fired. Each variant pins one specific
    /// call site so the counter dump shows attribution: "X of the 17,329
    /// constructions came from `delegate_cross_arena_symbol_resolution`,
    /// Y came from `jsdoc_type_construction`, ...".
    ///
    /// Adding a new reason: add one manifest entry below. The enum variant, stable
    /// display name, count, and `REASON_NAMES` order are generated together.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum CheckerCreationReason {
        /// `cross_file.rs::delegate_cross_arena_symbol_resolution` — the headline
        /// hot path; deep recursion through cross-file type queries.
        DelegateCrossArenaSymbol = 0 => "DelegateCrossArenaSymbol",
        /// `cross_file.rs::delegate_cross_arena_class_instance_type`.
        DelegateCrossArenaClass = 1 => "DelegateCrossArenaClass",
        /// `cross_file.rs::delegate_cross_arena_interface_type`.
        DelegateCrossArenaInterface = 2 => "DelegateCrossArenaInterface",
        /// Other `cross_file.rs` delegate variants (heritage, etc).
        DelegateCrossArenaOther = 3 => "DelegateCrossArenaOther",
        /// JSDoc namespace-typedef lookups crossing arenas.
        JsDocLookup = 4 => "JsDocLookup",
        /// JSDoc type-construction (synthesized object/function shapes).
        JsDocTypeConstruction = 5 => "JsDocTypeConstruction",
        /// CommonJS `module.exports` / `exports.x` resolution + collection.
        CjsExports = 6 => "CjsExports",
        /// Cross-file type alias resolution.
        AliasResolution = 7 => "AliasResolution",
        /// `import("…").Foo` indirect import-type resolution.
        ImportType = 8 => "ImportType",
        /// Type-environment `core.rs` deep resolution helpers.
        TypeEnvironmentCore = 9 => "TypeEnvironmentCore",
        /// `types::queries::callable_truthiness` cross-file fall-through.
        CallableTruthiness = 10 => "CallableTruthiness",
        /// Expando property assignments crossing files.
        ExpandoProperty = 11 => "ExpandoProperty",
        /// `identifier::resolution` cross-file fallback.
        IdentifierResolution = 12 => "IdentifierResolution",
        /// Generic call-helpers cross-file resolution (`call_helpers.rs`).
        CallHelpers = 13 => "CallHelpers",
        /// `computed_helpers_binding` deep alias resolution.
        BindingHelpers = 14 => "BindingHelpers",
        /// `class_abstract_checker` cross-file abstract-method check.
        ClassAbstract = 15 => "ClassAbstract",
        /// Module augmentation export value recovery in a delegate checker.
        ModuleAugmentationValue = 16 => "ModuleAugmentationValue",
        /// Anything not explicitly classified above.
        Other = 17 => "Other",
    }

    pub const CHECKER_CREATION_REASON_COUNT;
    pub const REASON_NAMES;
}

/// Number of log-spaced buckets in the interner lock-wait histogram.
/// See `LOCK_WAIT_BUCKET_UPPER_BOUNDS_NS` for the bucket boundaries.
pub const LOCK_WAIT_BUCKET_COUNT: usize = 8;

/// Upper bounds of the lock-wait histogram buckets, in nanoseconds. An
/// observation `ns` lands in the lowest-index bucket where
/// `ns < bucket_upper_bound`. The boundaries are log-spaced over the
/// 100ns…100ms range, with a final overflow bucket (`u64::MAX`) for
/// outliers. Plan §4.T0.3 notes that interner contention at the cliff
/// is the signal we need; a coarse log-bucketed histogram is enough
/// to distinguish "tail-bound" from "broadly slow" without paying for
/// per-shard or fine-grained quantile machinery.
pub const LOCK_WAIT_BUCKET_UPPER_BOUNDS_NS: [u64; LOCK_WAIT_BUCKET_COUNT] = [
    100,         // < 100 ns
    1_000,       // < 1 µs
    10_000,      // < 10 µs
    100_000,     // < 100 µs
    1_000_000,   // < 1 ms
    10_000_000,  // < 10 ms
    100_000_000, // < 100 ms
    u64::MAX,    // overflow
];

perf_counter_enum! {
    /// How `delegate_cross_arena_symbol_resolution` found the target arena for
    /// a cache miss that must construct a child checker.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum CrossArenaSymbolMissSource {
        /// `binder.symbol_arenas` pointed at a non-current arena.
        SymbolArena = 0 => "symbol_arenas",
        /// `binder.declaration_arenas` found a non-current declaration arena.
        DeclarationArena = 1 => "declaration_arenas",
        /// `cross_file_symbol_targets` resolved the target file index.
        SymbolFileTarget = 2 => "symbol_file_targets",
        /// Fallback bucket for unexpected delegation shapes.
        Unknown = 3 => "unknown",
    }

    pub const CROSS_ARENA_SYMBOL_MISS_SOURCE_COUNT;
    pub const CROSS_ARENA_SYMBOL_MISS_SOURCE_NAMES;
}

perf_counter_enum! {
    /// Coarse symbol-kind bucket for `DelegateCrossArenaSymbol` misses.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum CrossArenaSymbolMissKind {
        TypeAlias = 0 => "type_alias",
        Interface = 1 => "interface",
        Class = 2 => "class",
        Function = 3 => "function",
        Variable = 4 => "variable",
        Property = 5 => "property",
        Method = 6 => "method",
        Accessor = 7 => "accessor",
        Enum = 8 => "enum",
        Module = 9 => "module",
        Alias = 10 => "alias",
        TypeParameter = 11 => "type_parameter",
        TypeLiteral = 12 => "type_literal",
        Signature = 13 => "signature",
        Constructor = 14 => "constructor",
        ObjectLiteral = 15 => "object_literal",
        Unresolved = 16 => "unresolved",
        Other = 17 => "other",
    }

    pub const CROSS_ARENA_SYMBOL_MISS_KIND_COUNT;
    pub const CROSS_ARENA_SYMBOL_MISS_KIND_NAMES;
}

perf_counter_enum! {
    /// Outcome of the no-child named-alias shortcut attempted before
    /// constructing a `DelegateCrossArenaSymbol` child checker.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum CrossArenaAliasShortcutOutcome {
        Success = 0 => "success",
        NotAlias = 1 => "not_alias",
        MissingSymbol = 2 => "missing_symbol",
        MissingModule = 3 => "missing_module",
        MissingImportName = 4 => "missing_import_name",
        NamespaceImport = 5 => "namespace_import",
        DefaultImport = 6 => "default_import",
        MissingAliasFile = 7 => "missing_alias_file",
        MissingTarget = 8 => "missing_target",
        SelfTarget = 9 => "self_target",
        MissingTargetSymbol = 10 => "missing_target_symbol",
        TargetAlias = 11 => "target_alias",
        AliasPartner = 12 => "alias_partner",
        InterfaceValueMerge = 13 => "interface_value_merge",
        UnknownResult = 14 => "unknown_result",
        ErrorResult = 15 => "error_result",
    }

    pub const CROSS_ARENA_ALIAS_SHORTCUT_OUTCOME_COUNT;
    pub const CROSS_ARENA_ALIAS_SHORTCUT_OUTCOME_NAMES;
}

perf_counter_enum! {
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum DirectCrossFileInterfaceLoweringOutcome {
        Success = 0 => "success",
        RejectedNonDirectArena = 1 => "rejected_non_direct_arena",
        MissingSymbol = 2 => "missing_symbol",
        NotInterface = 3 => "not_interface",
        DisallowedMergeFlags = 4 => "disallowed_merge_flags",
        MissingDeclarations = 5 => "missing_declarations",
        ComplexDeclaration = 6 => "complex_declaration",
        UnknownOrError = 7 => "unknown_or_error",
    }

    pub const DIRECT_CROSS_FILE_INTERFACE_LOWERING_OUTCOME_COUNT;
    pub const DIRECT_CROSS_FILE_INTERFACE_LOWERING_OUTCOME_NAMES;
}

perf_counter_enum! {
    /// Structural reason recorded alongside
    /// `DirectCrossFileInterfaceLoweringOutcome::ComplexDeclaration`.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum DirectCrossFileInterfaceComplexReason {
        Heritage = 0 => "heritage",
        ComputedName = 1 => "computed_name",
        HeritageAndComputedName = 2 => "heritage_and_computed_name",
        SourceFileShape = 3 => "source_file_shape",
    }

    pub const DIRECT_CROSS_FILE_INTERFACE_COMPLEX_REASON_COUNT;
    pub const DIRECT_CROSS_FILE_INTERFACE_COMPLEX_REASON_NAMES;
}

perf_counter_enum! {
    /// How `compute_type_of_symbol` found the symbol payload for a call.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum ComputeTypeOfSymbolSourceOutcome {
        GlobalSymbol = 0 => "global_symbol",
        CrossFileSymbol = 1 => "cross_file_symbol",
        MissingSymbol = 2 => "missing_symbol",
    }

    pub const COMPUTE_TYPE_OF_SYMBOL_SOURCE_OUTCOME_COUNT;
    pub const COMPUTE_TYPE_OF_SYMBOL_SOURCE_OUTCOME_NAMES;
}

perf_counter_enum! {
    /// Which guard cut a `TypeEvaluator::evaluate` walk short (#14346). One
    /// variant per bail outcome in the evaluator's guard prologue; the counter
    /// dump shows the firing-order signal the issue flags — which bound a
    /// runaway recursive walk actually hits first. Mirrors the typed
    /// `crate::evaluation::result::TerminationKind` channel on the solver side;
    /// measurement only, never fed back into evaluation.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum EvaluationTerminationGuard {
        /// Per-evaluator recursion-depth guard already exceeded on re-entry.
        DepthExceeded = 0 => "depth_exceeded",
        /// Process-wide evaluation fuel counter exhausted.
        FuelExhausted = 1 => "fuel_exhausted",
        /// Shared cross-operation solver-stack-frame breaker bailed.
        SolverStackFrames = 2 => "solver_stack_frames",
        /// Cross-evaluator global-depth limit (`MAX_GLOBAL_EVAL_DEPTH`) hit.
        CrossEvalCycle = 3 => "cross_eval_cycle",
        /// Per-query operation budget ran out.
        QueryOpBudget = 4 => "query_op_budget",
    }

    pub const EVALUATION_TERMINATION_GUARD_COUNT;
    pub const EVALUATION_TERMINATION_GUARD_NAMES;
}

perf_counter_enum! {
    /// Coarse symbol-kind bucket for `compute_type_of_symbol` calls.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum ComputeTypeOfSymbolKindOutcome {
        Alias = 0 => "alias",
        TypeAlias = 1 => "type_alias",
        Interface = 2 => "interface",
        Class = 3 => "class",
        Function = 4 => "function",
        Variable = 5 => "variable",
        Module = 6 => "module",
        Property = 7 => "property",
        Method = 8 => "method",
        Accessor = 9 => "accessor",
        Enum = 10 => "enum",
        TypeParameter = 11 => "type_parameter",
        TypeLiteral = 12 => "type_literal",
        ObjectLiteral = 13 => "object_literal",
        Signature = 14 => "signature",
        Other = 15 => "other",
    }

    pub const COMPUTE_TYPE_OF_SYMBOL_KIND_OUTCOME_COUNT;
    pub const COMPUTE_TYPE_OF_SYMBOL_KIND_OUTCOME_NAMES;
}

perf_counter_enum! {
    /// Fast-path combination used for an interface symbol in
    /// `compute_type_of_symbol`.
    ///
    /// The three skip gates are:
    /// - computed-name precompute map
    /// - member type-param prewarm scan
    /// - local heritage merge
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum ComputeTypeOfSymbolInterfaceFastPathOutcome {
        FullPath = 0 => "full_path",
        SkipComputedNameMap = 1 => "skip_computed_name_map",
        SkipPrewarm = 2 => "skip_prewarm",
        SkipLocalHeritageMerge = 3 => "skip_local_heritage_merge",
        SkipComputedNameMapAndPrewarm = 4 => "skip_computed_name_map_and_prewarm",
        SkipComputedNameMapAndLocalHeritageMerge = 5 => "skip_computed_name_map_and_local_heritage_merge",
        SkipPrewarmAndLocalHeritageMerge = 6 => "skip_prewarm_and_local_heritage_merge",
        SkipAllThree = 7 => "skip_all_three",
    }

    pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_FASTPATH_OUTCOME_COUNT;
    pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_FASTPATH_OUTCOME_NAMES;
}

impl ComputeTypeOfSymbolInterfaceFastPathOutcome {
    #[inline(always)]
    pub const fn from_skips(
        skip_computed_name_map: bool,
        skip_prewarm: bool,
        skip_local_heritage_merge: bool,
    ) -> Self {
        match (
            skip_computed_name_map,
            skip_prewarm,
            skip_local_heritage_merge,
        ) {
            (false, false, false) => Self::FullPath,
            (true, false, false) => Self::SkipComputedNameMap,
            (false, true, false) => Self::SkipPrewarm,
            (false, false, true) => Self::SkipLocalHeritageMerge,
            (true, true, false) => Self::SkipComputedNameMapAndPrewarm,
            (true, false, true) => Self::SkipComputedNameMapAndLocalHeritageMerge,
            (false, true, true) => Self::SkipPrewarmAndLocalHeritageMerge,
            (true, true, true) => Self::SkipAllThree,
        }
    }
}

perf_counter_enum! {
    /// Call-site parent classification for interface-symbol calls in
    /// `compute_type_of_symbol`.
    ///
    /// Uses the caller frame from `symbol_resolution_stack`:
    /// - `root`: no parent symbol in the current resolution chain
    /// - `parent_*`: parent symbol kind bucket
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum ComputeTypeOfSymbolInterfaceCallsiteOutcome {
        Root = 0 => "root",
        ParentInterface = 1 => "parent_interface",
        ParentTypeAlias = 2 => "parent_type_alias",
        ParentAlias = 3 => "parent_alias",
        ParentOther = 4 => "parent_other",
        ParentMissing = 5 => "parent_missing",
    }

    pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_CALLSITE_OUTCOME_COUNT;
    pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_CALLSITE_OUTCOME_NAMES;
}

perf_counter_enum! {
    /// Outcome of the actual-lib alias-body helper inside the direct
    /// `DelegateCrossArenaSymbol` path. This is intentionally separate from
    /// the older source-file alias shortcut counters: it classifies bundled-lib
    /// aliases by why the typed alias-body proof did or did not admit them.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum DirectActualLibAliasBodyOutcome {
        Success = 0 => "success",
        NameNotAdmitted = 1 => "name_not_admitted",
        NotTypeAlias = 2 => "not_type_alias",
        ValueMerge = 3 => "value_merge",
        UnprovenActualLibDeclarations = 4 => "unproven_actual_lib_declarations",
        MissingResolverType = 5 => "missing_resolver_type",
        ResolverNotLazyDef = 6 => "resolver_not_lazy_def",
        MissingDefinition = 7 => "missing_definition",
        NonTypeAliasDefinition = 8 => "non_type_alias_definition",
        MissingBody = 9 => "missing_body",
        GenericAlias = 10 => "generic_alias",
    }

    pub const DIRECT_ACTUAL_LIB_ALIAS_BODY_OUTCOME_COUNT;
    pub const DIRECT_ACTUAL_LIB_ALIAS_BODY_OUTCOME_NAMES;
}

perf_counter_enum! {
    /// Outcome of the direct source-file type-alias lowering shortcut attempted
    /// before a `DelegateCrossArenaSymbol` miss constructs a child checker.
    ///
    /// This classifies the regular source-file alias lane separately from
    /// declaration-file and actual-lib shortcuts. The buckets identify which
    /// structural proof failed, so performance work can decide whether to widen
    /// a guard, cache a result, or leave the alias on the exact child-checker
    /// path.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum DirectSourceFileTypeAliasLoweringOutcome {
        Success = 0 => "success",
        MissingTargetFile = 1 => "missing_target_file",
        MissingArenaOrBinder = 2 => "missing_arena_or_binder",
        SourceFileArenaNotAllowed = 3 => "source_file_arena_not_allowed",
        MissingSymbol = 4 => "missing_symbol",
        NotTypeAlias = 5 => "not_type_alias",
        DisallowedMergeFlags = 6 => "disallowed_merge_flags",
        MultipleDeclarations = 7 => "multiple_declarations",
        NameMismatch = 8 => "name_mismatch",
        MissingTypeAliasNode = 9 => "missing_type_alias_node",
        BodyNotDirectLowerable = 10 => "body_not_direct_lowerable",
        TypeQueryOrSelfReference = 11 => "type_query_or_self_reference",
        UnknownOrError = 12 => "unknown_or_error",
    }

    pub const DIRECT_SOURCE_FILE_TYPE_ALIAS_LOWERING_OUTCOME_COUNT;
    pub const DIRECT_SOURCE_FILE_TYPE_ALIAS_LOWERING_OUTCOME_NAMES;
}

perf_counter_enum! {
    /// Root syntax family for source-file type-alias bodies rejected by the
    /// direct lowering shortcut.
    ///
    /// These buckets are intentionally coarse. They classify the structural
    /// operation that needs a proof before the `body_not_direct_lowerable` gate
    /// can be safely widened, without naming user aliases or benchmark files.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum DirectSourceFileTypeAliasBodyRejectionKind {
        TypeReference = 0 => "type_reference",
        ConditionalType = 1 => "conditional_type",
        TypeOperator = 2 => "type_operator",
        IndexedAccessType = 3 => "indexed_access_type",
        MappedType = 4 => "mapped_type",
        TypeLiteral = 5 => "type_literal",
        TemplateLiteralType = 6 => "template_literal_type",
        UnionOrIntersectionType = 7 => "union_or_intersection_type",
        ArrayOrTupleType = 8 => "array_or_tuple_type",
        WrappedType = 9 => "wrapped_type",
        InferType = 10 => "infer_type",
        Other = 11 => "other",
    }

    pub const DIRECT_SOURCE_FILE_TYPE_ALIAS_BODY_REJECTION_KIND_COUNT;
    pub const DIRECT_SOURCE_FILE_TYPE_ALIAS_BODY_REJECTION_KIND_NAMES;
}

perf_counter_enum! {
    /// Structural bucket for root `TypeReference` alias bodies rejected by the
    /// source-file direct-lowering proof.
    ///
    /// This intentionally records symbol shape and type-argument shape, not the
    /// user-written type name. The goal is to decide whether the next safe
    /// widening target is alias applications, interface refs, unresolved names,
    /// or a parser shape such as qualified names.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum DirectSourceFileTypeAliasTypeReferenceRejectionKind {
        OwnTypeParamWithTypeArguments = 0 => "own_type_param_with_type_arguments",
        BuiltinArrayWrongArity = 1 => "builtin_array_wrong_arity",
        BuiltinArrayNonDirectArgument = 2 => "builtin_array_non_direct_argument",
        LocalTypeAliasNoArguments = 3 => "local_type_alias_no_arguments",
        LocalTypeAliasWithArguments = 4 => "local_type_alias_with_arguments",
        LocalInterfaceNoArguments = 5 => "local_interface_no_arguments",
        LocalInterfaceWithArguments = 6 => "local_interface_with_arguments",
        LocalTypeParameter = 7 => "local_type_parameter",
        LocalAliasSymbol = 8 => "local_alias_symbol",
        LocalNamespaceSymbol = 9 => "local_namespace_symbol",
        LocalValueSymbol = 10 => "local_value_symbol",
        LocalTypeLiteralSymbol = 11 => "local_type_literal_symbol",
        LocalTransientSymbol = 12 => "local_transient_symbol",
        LocalOtherSymbol = 13 => "local_other_symbol",
        UnresolvedIdentifier = 14 => "unresolved_identifier",
        QualifiedName = 15 => "qualified_name",
        Other = 16 => "other",
    }

    pub const DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_COUNT;
    pub const DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_NAMES;
}

perf_counter_enum! {
    /// Outcome buckets for direct actual-lib Intl interface attempts in
    /// `direct_actual_lib_symbol_type`.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum DirectActualLibIntlInterfaceOutcome {
        SuccessByName = 0 => "success_by_name",
        SuccessNamespaceExport = 1 => "success_namespace_export",
        ValueInterfaceNotAdmitted = 2 => "value_interface_not_admitted",
        DeclarationNotProven = 3 => "declaration_not_proven",
        IntlNameNotAdmitted = 4 => "intl_name_not_admitted",
        MissingNamespaceExport = 5 => "missing_namespace_export",
        NamespaceSymbolMismatch = 6 => "namespace_symbol_mismatch",
        MissingNamespaceInterfaceType = 7 => "missing_namespace_interface_type",
        UnknownOrError = 8 => "unknown_or_error",
    }

    pub const DIRECT_ACTUAL_LIB_INTL_INTERFACE_OUTCOME_COUNT;
    pub const DIRECT_ACTUAL_LIB_INTL_INTERFACE_OUTCOME_NAMES;
}

perf_counter_enum! {
    /// Outcome buckets for the simple local-interface object shortcut in
    /// `compute_type_of_symbol`.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum ComputeTypeOfSymbolInterfaceSimpleObjectOutcome {
        Success = 0 => "success",
        RejectOutOfArenaDecl = 1 => "reject_out_of_arena_decl",
        RejectCrossFileSameIndex = 2 => "reject_cross_file_same_index",
        RejectDeclarationCount = 3 => "reject_declaration_count",
        RejectMissingInterfaceDecl = 4 => "reject_missing_interface_decl",
        RejectTypeParameters = 5 => "reject_type_parameters",
        RejectHeritageExtends = 6 => "reject_heritage_extends",
        RejectNonPropertyMember = 7 => "reject_non_property_member",
        RejectComputedName = 8 => "reject_computed_name",
        RejectUnresolvedPropertyName = 9 => "reject_unresolved_property_name",
        RejectNonPrimitiveAnnotation = 10 => "reject_non_primitive_annotation",
    }

    pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_OUTCOME_COUNT;
    pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_OUTCOME_NAMES;
}

perf_counter_enum! {
    /// Annotation-kind buckets for `RejectNonPrimitiveAnnotation` outcomes in
    /// the simple local-interface object shortcut.
    ///
    /// These buckets preserve behavioral parity (the shortcut still rejects all
    /// non-primitive annotation nodes) while making the reject residue
    /// actionable for conformance-proven guard relaxation.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationKind {
        TypeReference = 0 => "type_reference",
        UnionOrIntersection = 1 => "union_or_intersection",
        TypeLiteral = 2 => "type_literal",
        ArrayOrTuple = 3 => "array_or_tuple",
        FunctionOrConstructor = 4 => "function_or_constructor",
        ConditionalOrInfer = 5 => "conditional_or_infer",
        IndexedOrMapped = 6 => "indexed_or_mapped",
        ImportOrTypeQuery = 7 => "import_or_type_query",
        LiteralOrTemplateLiteral = 8 => "literal_or_template_literal",
        OperatorOrParenthesized = 9 => "operator_or_parenthesized",
        OptionalRestOrThis = 10 => "optional_rest_or_this",
        Other = 11 => "other",
    }

    pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_KIND_COUNT;
    pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_KIND_NAMES;
}

perf_counter_enum! {
    /// Attribution split for `type_reference` rows inside
    /// `RejectNonPrimitiveAnnotation` of the simple local-interface object
    /// shortcut.
    ///
    /// This keeps runtime behavior unchanged (the shortcut still rejects all
    /// non-primitive annotations) while exposing why `type_reference` rows are
    /// rejected.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectOutcome {
        IdentifierResolvableSymbol = 0 => "identifier_resolvable_symbol",
        IdentifierValueOnlySymbol = 1 => "identifier_value_only_symbol",
        IdentifierNotFoundSymbol = 2 => "identifier_not_found_symbol",
        IdentifierCompilerManagedType = 3 => "identifier_compiler_managed_type",
        QualifiedNameResolvableSymbol = 4 => "qualified_name_resolvable_symbol",
        QualifiedNameValueOnlySymbol = 5 => "qualified_name_value_only_symbol",
        QualifiedNameNotFoundSymbol = 6 => "qualified_name_not_found_symbol",
        OtherTypeNameSyntax = 7 => "other_type_name_syntax",
        MalformedTypeReference = 8 => "malformed_type_reference",
    }

    pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_OUTCOME_COUNT;
    pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_OUTCOME_NAMES;
}

perf_counter_enum! {
    /// Fine-grained outcome buckets for
    /// `try_lower_simple_actual_lib_type_reference`.
    ///
    /// The broader `type_reference` reject counters say whether a reference was
    /// syntactically resolvable. These buckets say why the actual-lib lazy-ref
    /// lowering helper did or did not accept it.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum ComputeTypeOfSymbolInterfaceSimpleObjectActualLibTypeReferenceOutcome {
        Success = 0 => "success",
        Disabled = 1 => "disabled",
        NotTypeReference = 2 => "not_type_reference",
        HasTypeArguments = 3 => "has_type_arguments",
        NonIdentifierName = 4 => "non_identifier_name",
        CompilerManagedType = 5 => "compiler_managed_type",
        FileLocalShadow = 6 => "file_local_shadow",
        SymbolNotType = 7 => "symbol_not_type",
        NotActualLibSymbol = 8 => "not_actual_lib_symbol",
        GenericSymbol = 9 => "generic_symbol",
    }

    pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_ACTUAL_LIB_TYPE_REFERENCE_OUTCOME_COUNT;
    pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_ACTUAL_LIB_TYPE_REFERENCE_OUTCOME_NAMES;
}

perf_counter_enum! {
    /// Why a cross-file cache reader (`cached_cross_file_*` in
    /// `crates/tsz-checker/src/context/cross_file_query.rs`) returned `None`.
    ///
    /// The 2026-05-11 attribution decision record locked in
    /// `delegate.cache_hits_cross_file = 0` on the cliff (1107 calls,
    /// 0 hits on `monorepo-006`). The flat miss counter does not say
    /// **why** each miss happens. Splitting the cause buckets lets the
    /// next T2.2 architecture PR target the dominant root cause directly
    /// instead of guessing between the gate state, the cache-key
    /// collision, and `TypeId` namespacing.
    ///
    /// The four root causes the buckets distinguish:
    ///
    /// - **`GateOff`** — `CheckerContext::share_owner_symbol_type_results`
    ///   is `false`. The reader short-circuits before touching the
    ///   `DefinitionStore`. A high count here means the gate is wrong
    ///   for the workload, not that the cache is empty.
    /// - **`BucketEmpty`** — the `DefinitionStore` lookup returned `None`
    ///   for the composite key. Either no writer has run yet, or the
    ///   writer and reader disagree on the key shape (e.g. caller's
    ///   `SymbolId` vs. owner's `SymbolId`).
    /// - **`SentinelErrorUnknown`** — the bucket has an entry but the
    ///   cached `TypeId` is `TypeId::ERROR` or `TypeId::UNKNOWN`. The
    ///   reader treats those as "not a real answer" so the call re-runs
    ///   the slow path.
    /// - **`TypeIdNotInterned`** — the cached non-intrinsic `TypeId` is
    ///   not interned in the reader's `TypeInterner`. This happens when
    ///   a child checker allocated the `TypeId` and the parent's
    ///   interner doesn't share it. The cache entry is stale.
    ///
    /// New variants must append, never re-order.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum CrossFileCacheMissCause {
        GateOff = 0 => "gate_off",
        BucketEmpty = 1 => "bucket_empty",
        SentinelErrorUnknown = 2 => "sentinel_error_unknown",
        TypeIdNotInterned = 3 => "type_id_not_interned",
    }

    pub const CROSS_FILE_CACHE_MISS_CAUSE_COUNT;
    pub const CROSS_FILE_CACHE_MISS_CAUSE_NAMES;
}

perf_counter_enum! {
    /// Why a `DelegateCrossArenaSymbol` symbol-arena delegation did or did not
    /// become eligible for the source-file symbol-arena cache.
    ///
    /// This is the next-level split after
    /// `delegate_miss_classification.by_source` says `symbol_arenas`
    /// dominates. It distinguishes cacheable first misses (`cacheable`, which
    /// may still appear as `cross_file_cache_miss_causes.bucket_empty`) from
    /// the structural reasons a symbol-arena delegation never reaches that
    /// cache at all.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum SourceFileSymbolArenaCacheEligibilityOutcome {
        Cacheable = 0 => "cacheable",
        CrossFileTarget = 1 => "cross_file_target",
        NonSymbolArena = 2 => "non_symbol_arena",
        ModuleAugmentation = 3 => "module_augmentation",
        MissingDelegateArena = 4 => "missing_delegate_arena",
        CurrentArena = 5 => "current_arena",
        MissingSourceFile = 6 => "missing_source_file",
        TargetDeclarationFile = 7 => "target_declaration_file",
        MissingSymbol = 8 => "missing_symbol",
        NotClassOrInterface = 9 => "not_class_or_interface",
        MultipleDeclarations = 10 => "multiple_declarations",
        DeclarationArenaMismatch = 11 => "declaration_arena_mismatch",
        MissingFileIndex = 12 => "missing_file_index",
        CacheableDeclarationFile = 13 => "cacheable_declaration_file",
    }

    pub const SOURCE_FILE_SYMBOL_ARENA_CACHE_ELIGIBILITY_OUTCOME_COUNT;
    pub const SOURCE_FILE_SYMBOL_ARENA_CACHE_ELIGIBILITY_OUTCOME_NAMES;
}

pub const DELEGATE_DECLARATION_FILE_MISS_RESIDUE_LIMIT: usize = 128;
pub const DELEGATE_SOURCE_FILE_MISS_RESIDUE_LIMIT: usize = 128;
pub const DIRECT_SOURCE_FILE_TYPE_ALIAS_BODY_REJECTION_RESIDUE_LIMIT: usize = 128;
pub const SLOW_CHECK_FILE_TIMING_LIMIT: usize = 32;
pub const SLOW_CHECK_STATEMENT_TIMING_LIMIT: usize = 64;
pub const SLOW_TYPE_ALIAS_CHECK_TIMING_LIMIT: usize = 64;

#[derive(Debug, Clone, serde::Serialize)]
pub struct DelegateDeclarationFileMissResidue {
    pub name: String,
    pub kind: &'static str,
    pub source: &'static str,
    pub target_file: Option<String>,
    pub count: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DelegateSourceFileMissResidue {
    pub name: String,
    pub kind: &'static str,
    pub source: &'static str,
    pub target_file: Option<String>,
    pub count: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DirectSourceFileTypeAliasBodyRejectionResidue {
    pub name: String,
    pub body_kind: &'static str,
    pub first_type_reference_kind: Option<&'static str>,
    pub first_type_reference_name: Option<String>,
    pub first_non_lowerable_type_reference_kind: Option<&'static str>,
    pub first_non_lowerable_type_reference_name: Option<String>,
    pub first_non_lowerable_leaf_type_reference_kind: Option<&'static str>,
    pub first_non_lowerable_leaf_type_reference_name: Option<String>,
    pub target_file: Option<String>,
    pub count: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SlowCheckFileTiming {
    pub file: String,
    pub elapsed_ms: f64,
    pub diagnostics: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SlowCheckStatementTiming {
    pub file: String,
    pub kind: u16,
    pub pos: u32,
    pub end: u32,
    pub elapsed_ms: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SlowTypeAliasCheckTiming {
    pub file: String,
    pub name: String,
    pub phase: &'static str,
    pub pos: u32,
    pub end: u32,
    pub elapsed_ms: f64,
}

#[derive(Debug, Copy, Clone)]
pub struct DirectSourceFileTypeAliasBodyRejectionResidueInput<'a> {
    pub name: &'a str,
    pub body_kind: DirectSourceFileTypeAliasBodyRejectionKind,
    pub first_type_reference_kind: Option<DirectSourceFileTypeAliasTypeReferenceRejectionKind>,
    pub first_type_reference_name: Option<&'a str>,
    pub first_non_lowerable_type_reference_kind:
        Option<DirectSourceFileTypeAliasTypeReferenceRejectionKind>,
    pub first_non_lowerable_type_reference_name: Option<&'a str>,
    pub first_non_lowerable_leaf_type_reference_kind:
        Option<DirectSourceFileTypeAliasTypeReferenceRejectionKind>,
    pub first_non_lowerable_leaf_type_reference_name: Option<&'a str>,
    pub target_file: Option<&'a str>,
}

static DELEGATE_DECLARATION_FILE_MISS_RESIDUES: OnceLock<
    Mutex<Vec<DelegateDeclarationFileMissResidue>>,
> = OnceLock::new();
static DELEGATE_SOURCE_FILE_MISS_RESIDUES: OnceLock<Mutex<Vec<DelegateSourceFileMissResidue>>> =
    OnceLock::new();
static DIRECT_SOURCE_FILE_TYPE_ALIAS_BODY_REJECTION_RESIDUES: OnceLock<
    Mutex<Vec<DirectSourceFileTypeAliasBodyRejectionResidue>>,
> = OnceLock::new();
static SLOW_CHECK_FILE_TIMINGS: OnceLock<Mutex<Vec<SlowCheckFileTiming>>> = OnceLock::new();
static SLOW_CHECK_STATEMENT_TIMINGS: OnceLock<Mutex<Vec<SlowCheckStatementTiming>>> =
    OnceLock::new();
static SLOW_TYPE_ALIAS_CHECK_TIMINGS: OnceLock<Mutex<Vec<SlowTypeAliasCheckTiming>>> =
    OnceLock::new();

fn delegate_declaration_file_miss_residues(
) -> &'static Mutex<Vec<DelegateDeclarationFileMissResidue>> {
    DELEGATE_DECLARATION_FILE_MISS_RESIDUES.get_or_init(|| Mutex::new(Vec::new()))
}

fn delegate_source_file_miss_residues() -> &'static Mutex<Vec<DelegateSourceFileMissResidue>> {
    DELEGATE_SOURCE_FILE_MISS_RESIDUES.get_or_init(|| Mutex::new(Vec::new()))
}

fn direct_source_file_type_alias_body_rejection_residues(
) -> &'static Mutex<Vec<DirectSourceFileTypeAliasBodyRejectionResidue>> {
    DIRECT_SOURCE_FILE_TYPE_ALIAS_BODY_REJECTION_RESIDUES.get_or_init(|| Mutex::new(Vec::new()))
}

fn slow_check_file_timings() -> &'static Mutex<Vec<SlowCheckFileTiming>> {
    SLOW_CHECK_FILE_TIMINGS.get_or_init(|| Mutex::new(Vec::new()))
}

fn slow_check_statement_timings() -> &'static Mutex<Vec<SlowCheckStatementTiming>> {
    SLOW_CHECK_STATEMENT_TIMINGS.get_or_init(|| Mutex::new(Vec::new()))
}

fn slow_type_alias_check_timings() -> &'static Mutex<Vec<SlowTypeAliasCheckTiming>> {
    SLOW_TYPE_ALIAS_CHECK_TIMINGS.get_or_init(|| Mutex::new(Vec::new()))
}

pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_RESIDUE_LIMIT:
    usize = 128;

pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_RESIDUE_LIMIT:
    usize = 128;

pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_DECLARATION_PROVENANCE_RESIDUE_LIMIT:
    usize = 128;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationResidue {
    pub kind: &'static str,
    pub interface: Option<String>,
    pub property: Option<String>,
    pub count: u64,
}

static COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_RESIDUES: OnceLock<
    Mutex<Vec<ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationResidue>>,
> = OnceLock::new();

fn compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues(
) -> &'static Mutex<Vec<ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationResidue>> {
    COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_RESIDUES
        .get_or_init(|| Mutex::new(Vec::new()))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ComputeTypeOfSymbolInterfaceSimpleObjectDeclarationProvenanceResidue {
    pub outcome: &'static str,
    pub symbol: Option<String>,
    pub declaration_count: u64,
    pub count: u64,
}

static COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_DECLARATION_PROVENANCE_RESIDUES: OnceLock<
    Mutex<Vec<ComputeTypeOfSymbolInterfaceSimpleObjectDeclarationProvenanceResidue>>,
> = OnceLock::new();

fn compute_type_of_symbol_interface_simple_object_declaration_provenance_residues(
) -> &'static Mutex<Vec<ComputeTypeOfSymbolInterfaceSimpleObjectDeclarationProvenanceResidue>> {
    COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_DECLARATION_PROVENANCE_RESIDUES
        .get_or_init(|| Mutex::new(Vec::new()))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectResidue {
    pub name: String,
    pub outcome: &'static str,
    pub count: u64,
}

static COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_RESIDUES: OnceLock<
    Mutex<Vec<ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectResidue>>,
> = OnceLock::new();

fn compute_type_of_symbol_interface_simple_object_type_reference_reject_residues(
) -> &'static Mutex<Vec<ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectResidue>> {
    COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_RESIDUES
        .get_or_init(|| Mutex::new(Vec::new()))
}
