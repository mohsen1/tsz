//! Process-wide performance counters used to drive the perf-architectural
//! plan in `docs/plan/PERFORMANCE_PLAN.md`.
//!
//! Counters are gated by the `TSZ_PERF_COUNTERS` environment variable. When
//! the variable is unset the increments still fire (`AtomicU64::fetch_add`
//! is a single relaxed atomic op, which is well under a nanosecond), so we
//! could in principle just always count, but the env var also gates the
//! more expensive counters (per-shard lock-wait histograms, top-N largest
//! types, recomputation tracking) so production builds stay clean.
//!
//! Output is printed on demand via [`PerfCounters::dump`]. Drivers wire that
//! into `--extendedDiagnostics` (or `--perfCounters`) so a single bench
//! invocation produces both the standard phase timings and the counter dump.
//!
//! Per the architectural plan, this is a plan-changing PR — the data we
//! collect here decides how PRs 2–7 are scoped. Don't ship later PRs without
//! looking at the dump on `large-ts-repo` first.

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

/// Cheap O(1) gate readable from any hot path. Reads a `OnceLock<bool>`
/// (one branch + one load) instead of going through `counters().enabled`
/// (deref-via-OnceLock + load).
#[inline(always)]
pub fn enabled_fast() -> bool {
    *ENABLED_FAST.get_or_init(|| std::env::var_os("TSZ_PERF_COUNTERS").is_some())
}

/// Why a `CheckerState::with_parent_cache` (and the matching
/// `copy_symbol_file_targets_to`) call fired. Each variant pins one specific
/// call site so the counter dump shows attribution: "X of the 17,329
/// constructions came from `delegate_cross_arena_symbol_resolution`,
/// Y came from `jsdoc_type_construction`, ...".
///
/// Adding a new reason: add the variant, update `REASON_NAMES` to keep them
/// aligned, and increase `CHECKER_CREATION_REASON_COUNT`.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum CheckerCreationReason {
    /// `cross_file.rs::delegate_cross_arena_symbol_resolution` — the headline
    /// hot path; deep recursion through cross-file type queries.
    DelegateCrossArenaSymbol = 0,
    /// `cross_file.rs::delegate_cross_arena_class_instance_type`.
    DelegateCrossArenaClass = 1,
    /// `cross_file.rs::delegate_cross_arena_interface_type`.
    DelegateCrossArenaInterface = 2,
    /// Other `cross_file.rs` delegate variants (heritage, etc).
    DelegateCrossArenaOther = 3,
    /// JSDoc namespace-typedef lookups crossing arenas.
    JsDocLookup = 4,
    /// JSDoc type-construction (synthesized object/function shapes).
    JsDocTypeConstruction = 5,
    /// CommonJS `module.exports` / `exports.x` resolution + collection.
    CjsExports = 6,
    /// Cross-file type alias resolution.
    AliasResolution = 7,
    /// `import("…").Foo` indirect import-type resolution.
    ImportType = 8,
    /// Type-environment `core.rs` deep resolution helpers.
    TypeEnvironmentCore = 9,
    /// `types::queries::callable_truthiness` cross-file fall-through.
    CallableTruthiness = 10,
    /// Expando property assignments crossing files.
    ExpandoProperty = 11,
    /// `identifier::resolution` cross-file fallback.
    IdentifierResolution = 12,
    /// Generic call-helpers cross-file resolution (`call_helpers.rs`).
    CallHelpers = 13,
    /// `computed_helpers_binding` deep alias resolution.
    BindingHelpers = 14,
    /// `class_abstract_checker` cross-file abstract-method check.
    ClassAbstract = 15,
    /// Anything not explicitly classified above.
    Other = 16,
}

pub const CHECKER_CREATION_REASON_COUNT: usize = 17;

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

/// Human-readable names, one entry per `CheckerCreationReason` variant.
/// MUST stay in sync with the enum.
pub const REASON_NAMES: [&str; CHECKER_CREATION_REASON_COUNT] = [
    "DelegateCrossArenaSymbol",
    "DelegateCrossArenaClass",
    "DelegateCrossArenaInterface",
    "DelegateCrossArenaOther",
    "JsDocLookup",
    "JsDocTypeConstruction",
    "CjsExports",
    "AliasResolution",
    "ImportType",
    "TypeEnvironmentCore",
    "CallableTruthiness",
    "ExpandoProperty",
    "IdentifierResolution",
    "CallHelpers",
    "BindingHelpers",
    "ClassAbstract",
    "Other",
];

impl CheckerCreationReason {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }
    pub const fn name(self) -> &'static str {
        REASON_NAMES[self as usize]
    }
}

/// How `delegate_cross_arena_symbol_resolution` found the target arena for
/// a cache miss that must construct a child checker.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum CrossArenaSymbolMissSource {
    /// `binder.symbol_arenas` pointed at a non-current arena.
    SymbolArena = 0,
    /// `binder.declaration_arenas` found a non-current declaration arena.
    DeclarationArena = 1,
    /// `cross_file_symbol_targets` resolved the target file index.
    SymbolFileTarget = 2,
    /// Fallback bucket for unexpected delegation shapes.
    Unknown = 3,
}

pub const CROSS_ARENA_SYMBOL_MISS_SOURCE_COUNT: usize = 4;

pub const CROSS_ARENA_SYMBOL_MISS_SOURCE_NAMES: [&str; CROSS_ARENA_SYMBOL_MISS_SOURCE_COUNT] = [
    "symbol_arenas",
    "declaration_arenas",
    "symbol_file_targets",
    "unknown",
];

impl CrossArenaSymbolMissSource {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

/// Coarse symbol-kind bucket for `DelegateCrossArenaSymbol` misses.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum CrossArenaSymbolMissKind {
    TypeAlias = 0,
    Interface = 1,
    Class = 2,
    Function = 3,
    Variable = 4,
    Property = 5,
    Method = 6,
    Accessor = 7,
    Enum = 8,
    Module = 9,
    Alias = 10,
    TypeParameter = 11,
    TypeLiteral = 12,
    Signature = 13,
    Constructor = 14,
    ObjectLiteral = 15,
    Unresolved = 16,
    Other = 17,
}

pub const CROSS_ARENA_SYMBOL_MISS_KIND_COUNT: usize = 18;

pub const CROSS_ARENA_SYMBOL_MISS_KIND_NAMES: [&str; CROSS_ARENA_SYMBOL_MISS_KIND_COUNT] = [
    "type_alias",
    "interface",
    "class",
    "function",
    "variable",
    "property",
    "method",
    "accessor",
    "enum",
    "module",
    "alias",
    "type_parameter",
    "type_literal",
    "signature",
    "constructor",
    "object_literal",
    "unresolved",
    "other",
];

impl CrossArenaSymbolMissKind {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

/// Outcome of the no-child named-alias shortcut attempted before constructing
/// a `DelegateCrossArenaSymbol` child checker.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum CrossArenaAliasShortcutOutcome {
    Success = 0,
    NotAlias = 1,
    MissingSymbol = 2,
    MissingModule = 3,
    MissingImportName = 4,
    NamespaceImport = 5,
    DefaultImport = 6,
    MissingAliasFile = 7,
    MissingTarget = 8,
    SelfTarget = 9,
    MissingTargetSymbol = 10,
    TargetAlias = 11,
    AliasPartner = 12,
    InterfaceValueMerge = 13,
    UnknownResult = 14,
    ErrorResult = 15,
}

pub const CROSS_ARENA_ALIAS_SHORTCUT_OUTCOME_COUNT: usize = 16;

pub const CROSS_ARENA_ALIAS_SHORTCUT_OUTCOME_NAMES: [&str;
    CROSS_ARENA_ALIAS_SHORTCUT_OUTCOME_COUNT] = [
    "success",
    "not_alias",
    "missing_symbol",
    "missing_module",
    "missing_import_name",
    "namespace_import",
    "default_import",
    "missing_alias_file",
    "missing_target",
    "self_target",
    "missing_target_symbol",
    "target_alias",
    "alias_partner",
    "interface_value_merge",
    "unknown_result",
    "error_result",
];

impl CrossArenaAliasShortcutOutcome {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum DirectCrossFileInterfaceLoweringOutcome {
    Success = 0,
    RejectedNonDirectArena = 1,
    MissingSymbol = 2,
    NotInterface = 3,
    DisallowedMergeFlags = 4,
    MissingDeclarations = 5,
    ComplexDeclaration = 6,
    UnknownOrError = 7,
}

pub const DIRECT_CROSS_FILE_INTERFACE_LOWERING_OUTCOME_COUNT: usize = 8;

pub const DIRECT_CROSS_FILE_INTERFACE_LOWERING_OUTCOME_NAMES: [&str;
    DIRECT_CROSS_FILE_INTERFACE_LOWERING_OUTCOME_COUNT] = [
    "success",
    "rejected_non_direct_arena",
    "missing_symbol",
    "not_interface",
    "disallowed_merge_flags",
    "missing_declarations",
    "complex_declaration",
    "unknown_or_error",
];

impl DirectCrossFileInterfaceLoweringOutcome {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

/// How `compute_type_of_symbol` found the symbol payload for a call.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum ComputeTypeOfSymbolSourceOutcome {
    GlobalSymbol = 0,
    CrossFileSymbol = 1,
    MissingSymbol = 2,
}

pub const COMPUTE_TYPE_OF_SYMBOL_SOURCE_OUTCOME_COUNT: usize = 3;

pub const COMPUTE_TYPE_OF_SYMBOL_SOURCE_OUTCOME_NAMES: [&str;
    COMPUTE_TYPE_OF_SYMBOL_SOURCE_OUTCOME_COUNT] =
    ["global_symbol", "cross_file_symbol", "missing_symbol"];

impl ComputeTypeOfSymbolSourceOutcome {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

/// Coarse symbol-kind bucket for `compute_type_of_symbol` calls.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum ComputeTypeOfSymbolKindOutcome {
    Alias = 0,
    TypeAlias = 1,
    Interface = 2,
    Class = 3,
    Function = 4,
    Variable = 5,
    Module = 6,
    Property = 7,
    Method = 8,
    Accessor = 9,
    Enum = 10,
    TypeParameter = 11,
    TypeLiteral = 12,
    ObjectLiteral = 13,
    Signature = 14,
    Other = 15,
}

pub const COMPUTE_TYPE_OF_SYMBOL_KIND_OUTCOME_COUNT: usize = 16;

pub const COMPUTE_TYPE_OF_SYMBOL_KIND_OUTCOME_NAMES: [&str;
    COMPUTE_TYPE_OF_SYMBOL_KIND_OUTCOME_COUNT] = [
    "alias",
    "type_alias",
    "interface",
    "class",
    "function",
    "variable",
    "module",
    "property",
    "method",
    "accessor",
    "enum",
    "type_parameter",
    "type_literal",
    "object_literal",
    "signature",
    "other",
];

impl ComputeTypeOfSymbolKindOutcome {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

/// Fast-path combination used for an interface symbol in
/// `compute_type_of_symbol`.
///
/// The three skip gates are:
/// - computed-name precompute map
/// - member type-param prewarm scan
/// - local heritage merge
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum ComputeTypeOfSymbolInterfaceFastPathOutcome {
    FullPath = 0,
    SkipComputedNameMap = 1,
    SkipPrewarm = 2,
    SkipLocalHeritageMerge = 3,
    SkipComputedNameMapAndPrewarm = 4,
    SkipComputedNameMapAndLocalHeritageMerge = 5,
    SkipPrewarmAndLocalHeritageMerge = 6,
    SkipAllThree = 7,
}

pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_FASTPATH_OUTCOME_COUNT: usize = 8;

pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_FASTPATH_OUTCOME_NAMES: [&str;
    COMPUTE_TYPE_OF_SYMBOL_INTERFACE_FASTPATH_OUTCOME_COUNT] = [
    "full_path",
    "skip_computed_name_map",
    "skip_prewarm",
    "skip_local_heritage_merge",
    "skip_computed_name_map_and_prewarm",
    "skip_computed_name_map_and_local_heritage_merge",
    "skip_prewarm_and_local_heritage_merge",
    "skip_all_three",
];

impl ComputeTypeOfSymbolInterfaceFastPathOutcome {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }

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

/// Call-site parent classification for interface-symbol calls in
/// `compute_type_of_symbol`.
///
/// Uses the caller frame from `symbol_resolution_stack`:
/// - `root`: no parent symbol in the current resolution chain
/// - `parent_*`: parent symbol kind bucket
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum ComputeTypeOfSymbolInterfaceCallsiteOutcome {
    Root = 0,
    ParentInterface = 1,
    ParentTypeAlias = 2,
    ParentAlias = 3,
    ParentOther = 4,
    ParentMissing = 5,
}

pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_CALLSITE_OUTCOME_COUNT: usize = 6;

pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_CALLSITE_OUTCOME_NAMES: [&str;
    COMPUTE_TYPE_OF_SYMBOL_INTERFACE_CALLSITE_OUTCOME_COUNT] = [
    "root",
    "parent_interface",
    "parent_type_alias",
    "parent_alias",
    "parent_other",
    "parent_missing",
];

impl ComputeTypeOfSymbolInterfaceCallsiteOutcome {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

/// Outcome of the actual-lib alias-body helper inside the direct
/// `DelegateCrossArenaSymbol` path. This is intentionally separate from the
/// older source-file alias shortcut counters: it classifies bundled-lib aliases
/// by why the typed alias-body proof did or did not admit them.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum DirectActualLibAliasBodyOutcome {
    Success = 0,
    NameNotAdmitted = 1,
    NotTypeAlias = 2,
    ValueMerge = 3,
    UnprovenActualLibDeclarations = 4,
    MissingResolverType = 5,
    ResolverNotLazyDef = 6,
    MissingDefinition = 7,
    NonTypeAliasDefinition = 8,
    MissingBody = 9,
    GenericAlias = 10,
}

pub const DIRECT_ACTUAL_LIB_ALIAS_BODY_OUTCOME_COUNT: usize = 11;

pub const DIRECT_ACTUAL_LIB_ALIAS_BODY_OUTCOME_NAMES: [&str;
    DIRECT_ACTUAL_LIB_ALIAS_BODY_OUTCOME_COUNT] = [
    "success",
    "name_not_admitted",
    "not_type_alias",
    "value_merge",
    "unproven_actual_lib_declarations",
    "missing_resolver_type",
    "resolver_not_lazy_def",
    "missing_definition",
    "non_type_alias_definition",
    "missing_body",
    "generic_alias",
];

impl DirectActualLibAliasBodyOutcome {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

/// Outcome buckets for direct actual-lib Intl interface attempts in
/// `direct_actual_lib_symbol_type`.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum DirectActualLibIntlInterfaceOutcome {
    SuccessByName = 0,
    SuccessNamespaceExport = 1,
    ValueInterfaceNotAdmitted = 2,
    DeclarationNotProven = 3,
    IntlNameNotAdmitted = 4,
    MissingNamespaceExport = 5,
    NamespaceSymbolMismatch = 6,
    MissingNamespaceInterfaceType = 7,
    UnknownOrError = 8,
}

pub const DIRECT_ACTUAL_LIB_INTL_INTERFACE_OUTCOME_COUNT: usize = 9;

pub const DIRECT_ACTUAL_LIB_INTL_INTERFACE_OUTCOME_NAMES: [&str;
    DIRECT_ACTUAL_LIB_INTL_INTERFACE_OUTCOME_COUNT] = [
    "success_by_name",
    "success_namespace_export",
    "value_interface_not_admitted",
    "declaration_not_proven",
    "intl_name_not_admitted",
    "missing_namespace_export",
    "namespace_symbol_mismatch",
    "missing_namespace_interface_type",
    "unknown_or_error",
];

impl DirectActualLibIntlInterfaceOutcome {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

/// Outcome buckets for the simple local-interface object shortcut in
/// `compute_type_of_symbol`.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum ComputeTypeOfSymbolInterfaceSimpleObjectOutcome {
    Success = 0,
    RejectOutOfArenaDecl = 1,
    RejectCrossFileSameIndex = 2,
    RejectDeclarationCount = 3,
    RejectMissingInterfaceDecl = 4,
    RejectTypeParameters = 5,
    RejectHeritageExtends = 6,
    RejectNonPropertyMember = 7,
    RejectComputedName = 8,
    RejectUnresolvedPropertyName = 9,
    RejectNonPrimitiveAnnotation = 10,
}

pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_OUTCOME_COUNT: usize = 11;

pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_OUTCOME_NAMES: [&str;
    COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_OUTCOME_COUNT] = [
    "success",
    "reject_out_of_arena_decl",
    "reject_cross_file_same_index",
    "reject_declaration_count",
    "reject_missing_interface_decl",
    "reject_type_parameters",
    "reject_heritage_extends",
    "reject_non_property_member",
    "reject_computed_name",
    "reject_unresolved_property_name",
    "reject_non_primitive_annotation",
];

impl ComputeTypeOfSymbolInterfaceSimpleObjectOutcome {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

/// Annotation-kind buckets for `RejectNonPrimitiveAnnotation` outcomes in the
/// simple local-interface object shortcut.
///
/// These buckets preserve behavioral parity (the shortcut still rejects all
/// non-primitive annotation nodes) while making the reject residue actionable
/// for conformance-proven guard relaxation.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationKind {
    TypeReference = 0,
    UnionOrIntersection = 1,
    TypeLiteral = 2,
    ArrayOrTuple = 3,
    FunctionOrConstructor = 4,
    ConditionalOrInfer = 5,
    IndexedOrMapped = 6,
    ImportOrTypeQuery = 7,
    LiteralOrTemplateLiteral = 8,
    OperatorOrParenthesized = 9,
    OptionalRestOrThis = 10,
    Other = 11,
}

pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_KIND_COUNT:
    usize = 12;

pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_KIND_NAMES:
    [&str; COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_KIND_COUNT] = [
    "type_reference",
    "union_or_intersection",
    "type_literal",
    "array_or_tuple",
    "function_or_constructor",
    "conditional_or_infer",
    "indexed_or_mapped",
    "import_or_type_query",
    "literal_or_template_literal",
    "operator_or_parenthesized",
    "optional_rest_or_this",
    "other",
];

impl ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationKind {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

/// Attribution split for `type_reference` rows inside
/// `RejectNonPrimitiveAnnotation` of the simple local-interface object
/// shortcut.
///
/// This keeps runtime behavior unchanged (the shortcut still rejects all
/// non-primitive annotations) while exposing why `type_reference` rows are
/// rejected.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectOutcome {
    IdentifierResolvableSymbol = 0,
    IdentifierValueOnlySymbol = 1,
    IdentifierNotFoundSymbol = 2,
    IdentifierCompilerManagedType = 3,
    QualifiedNameResolvableSymbol = 4,
    QualifiedNameValueOnlySymbol = 5,
    QualifiedNameNotFoundSymbol = 6,
    OtherTypeNameSyntax = 7,
    MalformedTypeReference = 8,
}

pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_OUTCOME_COUNT:
    usize = 9;

pub const COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_OUTCOME_NAMES:
    [&str; COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_OUTCOME_COUNT] = [
    "identifier_resolvable_symbol",
    "identifier_value_only_symbol",
    "identifier_not_found_symbol",
    "identifier_compiler_managed_type",
    "qualified_name_resolvable_symbol",
    "qualified_name_value_only_symbol",
    "qualified_name_not_found_symbol",
    "other_type_name_syntax",
    "malformed_type_reference",
];

impl ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectOutcome {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

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
/// `as_index` matches the `*_NAMES` array ordering; new variants
/// MUST append, never re-order.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum CrossFileCacheMissCause {
    GateOff = 0,
    BucketEmpty = 1,
    SentinelErrorUnknown = 2,
    TypeIdNotInterned = 3,
}

pub const CROSS_FILE_CACHE_MISS_CAUSE_COUNT: usize = 4;

pub const CROSS_FILE_CACHE_MISS_CAUSE_NAMES: [&str; CROSS_FILE_CACHE_MISS_CAUSE_COUNT] = [
    "gate_off",
    "bucket_empty",
    "sentinel_error_unknown",
    "type_id_not_interned",
];

impl CrossFileCacheMissCause {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

/// Why a `DelegateCrossArenaSymbol` symbol-arena delegation did or did not
/// become eligible for the source-file symbol-arena cache.
///
/// This is the next-level split after `delegate_miss_classification.by_source`
/// says `symbol_arenas` dominates. It distinguishes cacheable first misses
/// (`cacheable`, which may still appear as `cross_file_cache_miss_causes.bucket_empty`)
/// from the structural reasons a symbol-arena delegation never reaches that
/// cache at all.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum SourceFileSymbolArenaCacheEligibilityOutcome {
    Cacheable = 0,
    CrossFileTarget = 1,
    NonSymbolArena = 2,
    ModuleAugmentation = 3,
    MissingDelegateArena = 4,
    CurrentArena = 5,
    MissingSourceFile = 6,
    TargetDeclarationFile = 7,
    MissingSymbol = 8,
    NotClassOrInterface = 9,
    MultipleDeclarations = 10,
    DeclarationArenaMismatch = 11,
    MissingFileIndex = 12,
}

pub const SOURCE_FILE_SYMBOL_ARENA_CACHE_ELIGIBILITY_OUTCOME_COUNT: usize = 13;

pub const SOURCE_FILE_SYMBOL_ARENA_CACHE_ELIGIBILITY_OUTCOME_NAMES: [&str;
    SOURCE_FILE_SYMBOL_ARENA_CACHE_ELIGIBILITY_OUTCOME_COUNT] = [
    "cacheable",
    "cross_file_target",
    "non_symbol_arena",
    "module_augmentation",
    "missing_delegate_arena",
    "current_arena",
    "missing_source_file",
    "target_declaration_file",
    "missing_symbol",
    "not_class_or_interface",
    "multiple_declarations",
    "declaration_arena_mismatch",
    "missing_file_index",
];

impl SourceFileSymbolArenaCacheEligibilityOutcome {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

pub const DELEGATE_DECLARATION_FILE_MISS_RESIDUE_LIMIT: usize = 128;

#[derive(Debug, Clone, serde::Serialize)]
pub struct DelegateDeclarationFileMissResidue {
    pub name: String,
    pub kind: &'static str,
    pub source: &'static str,
    pub target_file: Option<String>,
    pub count: u64,
}

static DELEGATE_DECLARATION_FILE_MISS_RESIDUES: OnceLock<
    Mutex<Vec<DelegateDeclarationFileMissResidue>>,
> = OnceLock::new();

fn delegate_declaration_file_miss_residues()
-> &'static Mutex<Vec<DelegateDeclarationFileMissResidue>> {
    DELEGATE_DECLARATION_FILE_MISS_RESIDUES.get_or_init(|| Mutex::new(Vec::new()))
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

fn compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues()
-> &'static Mutex<Vec<ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationResidue>> {
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

fn compute_type_of_symbol_interface_simple_object_declaration_provenance_residues()
-> &'static Mutex<Vec<ComputeTypeOfSymbolInterfaceSimpleObjectDeclarationProvenanceResidue>> {
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

fn compute_type_of_symbol_interface_simple_object_type_reference_reject_residues()
-> &'static Mutex<Vec<ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectResidue>> {
    COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_RESIDUES
        .get_or_init(|| Mutex::new(Vec::new()))
}

/// One process-wide instance. Incremented from any thread, read once at
/// dump time.
pub struct PerfCounters {
    pub enabled: AtomicBool,

    // ─── delegation / cross-arena resolution ─────────────────────────────
    pub delegate_cross_arena_calls: AtomicU64,
    pub delegate_cross_arena_cache_hits_lib: AtomicU64,
    pub delegate_cross_arena_cache_hits_cross_file: AtomicU64,
    pub delegate_cross_arena_misses: AtomicU64,
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
    /// Outcome buckets for direct actual-lib alias-body attempts.
    pub direct_actual_lib_alias_body_outcome:
        [AtomicU64; DIRECT_ACTUAL_LIB_ALIAS_BODY_OUTCOME_COUNT],
    /// Outcome buckets for direct actual-lib Intl interface attempts.
    pub direct_actual_lib_intl_interface_outcome:
        [AtomicU64; DIRECT_ACTUAL_LIB_INTL_INTERFACE_OUTCOME_COUNT],
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

    // ─── interner ────────────────────────────────────────────────────────
    pub interner_intern_calls: AtomicU64,
    pub interner_intern_hits: AtomicU64,
    pub interner_intern_misses: AtomicU64,
    pub interner_string_intern_calls: AtomicU64,
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
            direct_actual_lib_alias_body_outcome: [const { AtomicU64::new(0) };
                DIRECT_ACTUAL_LIB_ALIAS_BODY_OUTCOME_COUNT],
            direct_actual_lib_intl_interface_outcome: [const { AtomicU64::new(0) };
                DIRECT_ACTUAL_LIB_INTL_INTERFACE_OUTCOME_COUNT],
            cross_file_cache_miss_cause: [const { AtomicU64::new(0) };
                CROSS_FILE_CACHE_MISS_CAUSE_COUNT],
            source_file_symbol_arena_cache_eligibility_outcome: [const { AtomicU64::new(0) };
                SOURCE_FILE_SYMBOL_ARENA_CACHE_ELIGIBILITY_OUTCOME_COUNT],
            checker_state_constructed: AtomicU64::new(0),
            checker_state_with_parent_cache_constructed: AtomicU64::new(0),
            with_parent_cache_by_reason: [const { AtomicU64::new(0) };
                CHECKER_CREATION_REASON_COUNT],
            file_session_resets: AtomicU64::new(0),
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
            interner_intern_calls: AtomicU64::new(0),
            interner_intern_hits: AtomicU64::new(0),
            interner_intern_misses: AtomicU64::new(0),
            interner_string_intern_calls: AtomicU64::new(0),
            interner_type_list_intern_calls: AtomicU64::new(0),
            interner_object_shape_intern_calls: AtomicU64::new(0),
            interner_function_shape_intern_calls: AtomicU64::new(0),
            interner_callable_shape_intern_calls: AtomicU64::new(0),
            interner_application_intern_calls: AtomicU64::new(0),
            interner_conditional_intern_calls: AtomicU64::new(0),
            interner_mapped_intern_calls: AtomicU64::new(0),
            interner_lock_wait_histogram_ns: [const { AtomicU64::new(0) }; LOCK_WAIT_BUCKET_COUNT],
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

/// Record a single lock-wait observation into the histogram. Buckets are
/// log-spaced over 100 ns…100 ms with a final overflow bucket; see
/// [`LOCK_WAIT_BUCKET_UPPER_BOUNDS_NS`]. Gated behind the
/// `perf-counters-timing` feature: when the feature is off this function
/// is not compiled at all (the `cfg` excludes the entire item), and the
/// only call site lives inside the feature-on variant of
/// [`time_shard_write`], which is replaced with a no-op stub that calls
/// `f()` directly.
#[cfg(feature = "perf-counters-timing")]
#[inline]
fn record_lock_wait_ns(ns: u64) {
    if !enabled_fast() {
        return;
    }
    let buckets = &counters().interner_lock_wait_histogram_ns;
    for (i, &upper) in LOCK_WAIT_BUCKET_UPPER_BOUNDS_NS.iter().enumerate() {
        if ns < upper {
            buckets[i].fetch_add(1, Ordering::Relaxed);
            return;
        }
    }
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

/// Record a `CheckerState::with_parent_cache` construction with attribution.
/// Bumps both the global counter and the per-reason bucket so PR #1631's
/// dump shows where the 17,329 constructions on subset3 come from.
///
/// Gate once at the top: when counters are disabled the helper returns
/// without paying the `counters()` `OnceLock` deref. When enabled the
/// two atomic increments are direct `fetch_add` calls (no per-call
/// `enabled_fast()` re-check via `inc()`).
#[inline]
pub fn record_with_parent_cache(reason: CheckerCreationReason) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.checker_state_with_parent_cache_constructed
        .fetch_add(1, Ordering::Relaxed);
    c.with_parent_cache_by_reason[reason.as_index()].fetch_add(1, Ordering::Relaxed);
}

/// Record an overlay copy with attribution: count + entries-copied +
/// global max + size-bucket histogram + per-reason max. The histogram
/// tells us whether `entries_total = 12.8B` is "many medium clones" or
/// "a few catastrophic huge clones" — both produce the same total but
/// imply very different fixes (per PR #1630 review).
///
/// Caller passes the parent overlay's len so we can attribute without
/// holding a borrow across the copy.
///
/// Gate once at the top: when counters are disabled the helper returns
/// without paying the `counters()` `OnceLock` deref. When enabled the
/// 10+ atomic operations are direct `fetch_add`/`compare_exchange`
/// calls instead of routing each through `inc()`/`add()`/`record_max()`
/// (which each re-check `enabled_fast()`).
#[inline]
pub fn record_overlay_copy(reason: CheckerCreationReason, entries: u64) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.copy_symbol_file_targets_calls
        .fetch_add(1, Ordering::Relaxed);
    c.copy_symbol_file_targets_entries_total
        .fetch_add(entries, Ordering::Relaxed);
    record_max_inner(&c.copy_symbol_file_targets_entries_max, entries);
    if entries >= 1_000 {
        c.copy_symbol_file_targets_len_ge_1k
            .fetch_add(1, Ordering::Relaxed);
    }
    if entries >= 10_000 {
        c.copy_symbol_file_targets_len_ge_10k
            .fetch_add(1, Ordering::Relaxed);
    }
    if entries >= 100_000 {
        c.copy_symbol_file_targets_len_ge_100k
            .fetch_add(1, Ordering::Relaxed);
    }
    if entries >= 1_000_000 {
        c.copy_symbol_file_targets_len_ge_1m
            .fetch_add(1, Ordering::Relaxed);
    }
    c.overlay_copy_calls_by_reason[reason.as_index()].fetch_add(1, Ordering::Relaxed);
    c.overlay_copy_entries_by_reason[reason.as_index()].fetch_add(entries, Ordering::Relaxed);
    record_max_inner(
        &c.overlay_copy_max_entries_by_reason[reason.as_index()],
        entries,
    );
}

/// `record_max` without the gate check — called from helpers that
/// already gated at the top. Keeps the CAS-loop semantics of the public
/// `record_max` while avoiding a redundant `enabled_fast()` read.
#[inline]
fn record_max_inner(counter: &AtomicU64, value: u64) {
    let mut current = counter.load(Ordering::Relaxed);
    while value > current {
        match counter.compare_exchange_weak(current, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(observed) => current = observed,
        }
    }
}

/// Record one cross-arena symbol miss with source/kind/target attribution.
///
/// Gate once at the top: when counters are disabled the helper returns
/// without paying the `counters()` `OnceLock` deref. When enabled the
/// three atomic increments are direct `fetch_add` calls (no per-call
/// `enabled_fast()` re-check via `inc()`).
#[inline]
pub fn record_cross_arena_symbol_miss(
    source: CrossArenaSymbolMissSource,
    kind: CrossArenaSymbolMissKind,
    target_is_declaration_file: bool,
) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.delegate_cross_arena_symbol_miss_by_source[source.as_index()].fetch_add(1, Ordering::Relaxed);
    c.delegate_cross_arena_symbol_miss_by_kind[kind.as_index()].fetch_add(1, Ordering::Relaxed);
    if target_is_declaration_file {
        c.delegate_cross_arena_symbol_miss_target_declaration_file
            .fetch_add(1, Ordering::Relaxed);
    } else {
        c.delegate_cross_arena_symbol_miss_target_source_file
            .fetch_add(1, Ordering::Relaxed);
    }
}

#[inline]
pub fn record_cross_arena_declaration_file_miss_residue(
    source: CrossArenaSymbolMissSource,
    kind: CrossArenaSymbolMissKind,
    name: &str,
    target_file: Option<&str>,
) {
    if !enabled_fast() {
        return;
    }

    let source_name = CROSS_ARENA_SYMBOL_MISS_SOURCE_NAMES[source.as_index()];
    let kind_name = CROSS_ARENA_SYMBOL_MISS_KIND_NAMES[kind.as_index()];
    let target_file = target_file.map(|file| {
        std::path::Path::new(file)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(file)
            .to_owned()
    });
    let mut rows = delegate_declaration_file_miss_residues()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(row) = rows.iter_mut().find(|row| {
        row.name == name
            && row.kind == kind_name
            && row.source == source_name
            && row.target_file == target_file
    }) {
        row.count += 1;
        return;
    }

    if rows.len() < DELEGATE_DECLARATION_FILE_MISS_RESIDUE_LIMIT {
        rows.push(DelegateDeclarationFileMissResidue {
            name: name.to_owned(),
            kind: kind_name,
            source: source_name,
            target_file,
            count: 1,
        });
    } else if let Some(row) = rows.iter_mut().find(|row| row.name == "__truncated__") {
        row.count += 1;
    } else {
        rows.push(DelegateDeclarationFileMissResidue {
            name: "__truncated__".to_string(),
            kind: "overflow",
            source: "overflow",
            target_file: None,
            count: 1,
        });
    }
}

#[inline]
pub fn record_cross_arena_alias_shortcut_outcome(outcome: CrossArenaAliasShortcutOutcome) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.delegate_cross_arena_alias_shortcut_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

/// Classify a `cached_cross_file_*` miss. Called by the four reader
/// helpers in `crates/tsz-checker/src/context/cross_file_query.rs`
/// at each early-return point. See [`CrossFileCacheMissCause`].
#[inline]
pub fn record_cross_file_cache_miss_cause(cause: CrossFileCacheMissCause) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.cross_file_cache_miss_cause[cause.as_index()].fetch_add(1, Ordering::Relaxed);
}

/// Classify whether a source-file symbol-arena delegation is eligible for the
/// post-#6191 cache. Called before the cache lookup so non-cacheable residue is
/// visible in attribution JSON instead of hiding behind the flat miss count.
#[inline]
pub fn record_source_file_symbol_arena_cache_eligibility_outcome(
    outcome: SourceFileSymbolArenaCacheEligibilityOutcome,
) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.source_file_symbol_arena_cache_eligibility_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a cross-arena delegate invocation that has no cache fast path —
/// i.e., every call is a miss. Increments both `delegate_cross_arena_calls`
/// and `delegate_cross_arena_misses` with a single `counters()` lookup.
///
/// The hand-rolled call-site pattern this helper replaces was:
///
/// ```rust,ignore
/// if tsz_common::perf_counters::enabled_fast() {
///     tsz_common::perf_counters::inc(
///         &tsz_common::perf_counters::counters().delegate_cross_arena_calls,
///     );
///     tsz_common::perf_counters::inc(
///         &tsz_common::perf_counters::counters().delegate_cross_arena_misses,
///     );
/// }
/// ```
///
/// — which pays two `counters()` `OnceLock` derefs per increment pair.
/// This helper folds them into one. Callers that have a cache fast path
/// (e.g. lib-delegation hit) should keep using `inc(&perf.delegate_cross_arena_calls)`
/// directly and only call this when the miss is unconditional.
#[inline]
pub fn record_delegate_cross_arena_miss() {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.delegate_cross_arena_calls.fetch_add(1, Ordering::Relaxed);
    c.delegate_cross_arena_misses
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a cross-file cache hit during cross-arena delegation. Used at
/// three sites in `state/type_analysis/cross_file.rs` where the
/// `cached_cross_file_*_type` fast path returns before the slow
/// child-checker construction would fire.
///
/// Mirrors [`record_delegate_cross_arena_miss`]: gate once, look up
/// `counters()` once, increment both the aggregate call counter and the named
/// per-outcome counter directly. These cross-file fast paths return before the
/// slow child-checker miss path can call [`record_delegate_cross_arena_miss`],
/// so the hit helper owns the aggregate `delegate_cross_arena_calls` bump.
#[inline]
pub fn record_delegate_cross_arena_cache_hit_cross_file() {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.delegate_cross_arena_calls.fetch_add(1, Ordering::Relaxed);
    c.delegate_cross_arena_cache_hits_cross_file
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a hit on the cross-file type-parameter extraction cache. Mirrors
/// [`record_delegate_cross_arena_miss`]: gate-once and one `counters()`
/// lookup, then increment exactly the per-outcome counter that names this
/// branch of the cache.
#[inline]
pub fn record_cross_file_type_params_cache_hit() {
    if !enabled_fast() {
        return;
    }
    counters()
        .cross_file_type_params_cache_hits
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a miss on the cross-file type-parameter extraction cache. Counted
/// when the slow path runs to build a child checker, regardless of whether
/// the slow path ultimately returns `Some(_)` — see the call sites in
/// `state/type_environment/core.rs` for the rationale (counting only on
/// `Some(_)` undercounts misses when the slow path runs but extraction fails,
/// distorting attribution for Tier-2 decision-making).
#[inline]
pub fn record_cross_file_type_params_cache_miss() {
    if !enabled_fast() {
        return;
    }
    counters()
        .cross_file_type_params_cache_misses
        .fetch_add(1, Ordering::Relaxed);
}

/// Record an entry to `get_type_of_symbol`'s computation path. Sits on a
/// multi-million-call hot path (`state/type_analysis/computed/mod.rs`),
/// so gating once before the `counters()` `OnceLock` deref is the load-
/// bearing optimization — disabled builds pay one branch and one
/// relaxed atomic load on `ENABLED_FAST`, not a deref.
#[inline]
pub fn record_compute_type_of_symbol_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .compute_type_of_symbol_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a cache hit on `get_type_of_symbol`'s `symbol_types` lookup.
/// Used at two sites in `state/type_analysis/core.rs` (the provisional-
/// type and the standard cached-type branches of the recursion guard).
/// Compared against [`record_compute_type_of_symbol_call`] in attribution
/// mode to characterize recomputation pressure.
#[inline]
pub fn record_compute_type_of_symbol_cache_hit() {
    if !enabled_fast() {
        return;
    }
    counters()
        .compute_type_of_symbol_cache_hits
        .fetch_add(1, Ordering::Relaxed);
}

/// Record use of the simple local-interface object shortcut inside
/// `compute_type_of_symbol`.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_fastpath_hit() {
    if !enabled_fast() {
        return;
    }
    counters()
        .compute_type_of_symbol_interface_simple_object_fastpath_hits
        .fetch_add(1, Ordering::Relaxed);
}

/// Record how `compute_type_of_symbol` sourced the symbol payload.
#[inline]
pub fn record_compute_type_of_symbol_source_outcome(outcome: ComputeTypeOfSymbolSourceOutcome) {
    if !enabled_fast() {
        return;
    }
    counters().compute_type_of_symbol_source_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

/// Record the coarse symbol-kind bucket lowered by `compute_type_of_symbol`.
#[inline]
pub fn record_compute_type_of_symbol_kind_outcome(outcome: ComputeTypeOfSymbolKindOutcome) {
    if !enabled_fast() {
        return;
    }
    counters().compute_type_of_symbol_kind_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

/// Record which interface fast-path combination ran inside
/// `compute_type_of_symbol`.
#[inline]
pub fn record_compute_type_of_symbol_interface_fastpath_outcome(
    outcome: ComputeTypeOfSymbolInterfaceFastPathOutcome,
) {
    if !enabled_fast() {
        return;
    }
    counters().compute_type_of_symbol_interface_fastpath_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

/// Record call-site parent-kind attribution for interface calls in
/// `compute_type_of_symbol`.
#[inline]
pub fn record_compute_type_of_symbol_interface_callsite_outcome(
    outcome: ComputeTypeOfSymbolInterfaceCallsiteOutcome,
) {
    if !enabled_fast() {
        return;
    }
    counters().compute_type_of_symbol_interface_callsite_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

/// Record success/reject outcomes for the simple local-interface object
/// shortcut in `compute_type_of_symbol`.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_outcome(
    outcome: ComputeTypeOfSymbolInterfaceSimpleObjectOutcome,
) {
    if !enabled_fast() {
        return;
    }
    counters().compute_type_of_symbol_interface_simple_object_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

/// Record annotation-kind attribution for
/// `RejectNonPrimitiveAnnotation` outcomes in the simple local-interface
/// object shortcut.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kind(
    kind: ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationKind,
) {
    if !enabled_fast() {
        return;
    }
    counters().compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kind
        [kind.as_index()]
    .fetch_add(1, Ordering::Relaxed);
}

/// Record bounded source-level residue for non-primitive annotations rejected
/// by the simple local-interface object shortcut.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residue(
    kind: ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationKind,
    interface: Option<&str>,
    property: Option<&str>,
) {
    if !enabled_fast() {
        return;
    }

    let kind_name =
        COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_KIND_NAMES
            [kind.as_index()];
    let mut rows =
        compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(row) = rows.iter_mut().find(|row| {
        row.kind == kind_name
            && row.interface.as_deref() == interface
            && row.property.as_deref() == property
    }) {
        row.count += 1;
        return;
    }

    if rows.len()
        < COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_RESIDUE_LIMIT
    {
        rows.push(
            ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationResidue {
                kind: kind_name,
                interface: interface.map(str::to_owned),
                property: property.map(str::to_owned),
                count: 1,
            },
        );
    } else if let Some(row) = rows
        .iter_mut()
        .find(|row| row.interface.as_deref() == Some("__truncated__"))
    {
        row.count += 1;
    } else {
        rows.push(
            ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationResidue {
                kind: "overflow",
                interface: Some("__truncated__".to_string()),
                property: None,
                count: 1,
            },
        );
    }
}

/// Record bounded symbol-level residue for declaration/provenance guards
/// rejected by the simple local-interface object shortcut.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_declaration_provenance_residue(
    outcome: ComputeTypeOfSymbolInterfaceSimpleObjectOutcome,
    symbol: Option<&str>,
    declaration_count: usize,
) {
    if !enabled_fast() {
        return;
    }

    let outcome_name =
        COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_OUTCOME_NAMES[outcome.as_index()];
    let declaration_count = declaration_count as u64;
    let mut rows = compute_type_of_symbol_interface_simple_object_declaration_provenance_residues()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(row) = rows.iter_mut().find(|row| {
        row.outcome == outcome_name
            && row.symbol.as_deref() == symbol
            && row.declaration_count == declaration_count
    }) {
        row.count += 1;
        return;
    }

    if rows.len()
        < COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_DECLARATION_PROVENANCE_RESIDUE_LIMIT
    {
        rows.push(
            ComputeTypeOfSymbolInterfaceSimpleObjectDeclarationProvenanceResidue {
                outcome: outcome_name,
                symbol: symbol.map(str::to_owned),
                declaration_count,
                count: 1,
            },
        );
    } else if let Some(row) = rows
        .iter_mut()
        .find(|row| row.symbol.as_deref() == Some("__truncated__"))
    {
        row.count += 1;
    } else {
        rows.push(
            ComputeTypeOfSymbolInterfaceSimpleObjectDeclarationProvenanceResidue {
                outcome: "overflow",
                symbol: Some("__truncated__".to_string()),
                declaration_count: 0,
                count: 1,
            },
        );
    }
}

/// Record attribution for why a `type_reference` annotation was still rejected
/// by the simple local-interface object shortcut.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_type_reference_reject_outcome(
    outcome: ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectOutcome,
) {
    if !enabled_fast() {
        return;
    }
    counters().compute_type_of_symbol_interface_simple_object_type_reference_reject_outcome
        [outcome.as_index()]
    .fetch_add(1, Ordering::Relaxed);
}

/// Record bounded name-level residue for type-reference annotations rejected by
/// the simple local-interface object shortcut.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_type_reference_reject_residue(
    outcome: ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectOutcome,
    name: &str,
) {
    if !enabled_fast() {
        return;
    }

    let outcome_name =
        COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_OUTCOME_NAMES
            [outcome.as_index()];
    let mut rows = compute_type_of_symbol_interface_simple_object_type_reference_reject_residues()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(row) = rows
        .iter_mut()
        .find(|row| row.name == name && row.outcome == outcome_name)
    {
        row.count += 1;
        return;
    }

    if rows.len()
        < COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_RESIDUE_LIMIT
    {
        rows.push(
            ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectResidue {
                name: name.to_owned(),
                outcome: outcome_name,
                count: 1,
            },
        );
    } else if let Some(row) = rows.iter_mut().find(|row| row.name == "__truncated__") {
        row.count += 1;
    } else {
        rows.push(
            ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectResidue {
                name: "__truncated__".to_string(),
                outcome: "overflow",
                count: 1,
            },
        );
    }
}

pub fn record_property_classification_call() {
    inc(&counters().property_classification_calls);
}

pub fn record_property_classification_string_fallback_source_lookup() {
    inc(&counters().property_classification_string_fallback_source_lookups);
}

pub fn record_property_classification_string_fallback_target_name() {
    inc(&counters().property_classification_string_fallback_target_names);
}

pub fn record_property_classification_string_fallback_target_type() {
    inc(&counters().property_classification_string_fallback_target_types);
}

/// Record a `TypeInterner::intern_string` call. Mirrors the existing
/// `record_compute_type_of_symbol_*` shape: gate once, one `counters()`
/// lookup, increment exactly the named field.
///
/// `intern_string` is a fundamental hot path — every property name,
/// every string literal, every diagnostic message tag eventually flows
/// through it. The wrapper keeps that path cheap when counters are
/// disabled (one `OnceLock<bool>` read, no `OnceLock<PerfCounters>`
/// deref) without spreading the gate/deref pair across every interner
/// entry point.
#[inline]
pub fn record_interner_string_intern_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_string_intern_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a `TypeInterner::intern_type_list` call (covers both the
/// owning `Vec` entry point and the borrowed-slice entry point).
#[inline]
pub fn record_interner_type_list_intern_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_type_list_intern_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a `TypeInterner::intern_object_shape` call.
#[inline]
pub fn record_interner_object_shape_intern_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_object_shape_intern_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a `TypeInterner::intern_function_shape` call.
#[inline]
pub fn record_interner_function_shape_intern_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_function_shape_intern_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a `TypeInterner::intern_conditional_type` call.
#[inline]
pub fn record_interner_conditional_intern_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_conditional_intern_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a `TypeInterner::intern_mapped_type` call.
#[inline]
pub fn record_interner_mapped_intern_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_mapped_intern_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a `TypeInterner::intern_callable_shape` call. Mirrors the
/// sibling `record_interner_function_shape_intern_call` shape — gate
/// once, one `counters()` lookup, increment the named field.
#[inline]
pub fn record_interner_callable_shape_intern_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_callable_shape_intern_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a `TypeInterner::intern_application` call.
#[inline]
pub fn record_interner_application_intern_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_application_intern_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one `ModuleResolver::lookup()` call from
/// `crates/tsz-cli/src/driver/sources.rs` — the entry point for per-import
/// module resolution. Sibling to the fs-probe `record_resolver_*`
/// helpers but lives in a different file (sources.rs vs resolution.rs)
/// because resolution caching happens at the lookup level, above the
/// individual fs-probe wrappers.
#[inline]
pub fn record_resolver_lookup_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .resolver_lookup_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one `Path::is_file()` probe from the resolver fast path.
/// Used by the `count_is_file` wrapper in `crates/tsz-cli/src/driver/resolution.rs`,
/// which bundles the syscall and the counter in one place. Gate once,
/// deref `counters()` once, increment.
#[inline]
pub fn record_resolver_is_file() {
    if !enabled_fast() {
        return;
    }
    counters()
        .resolver_is_file_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one `Path::is_dir()` probe from the resolver fast path.
/// Sibling to [`record_resolver_is_file`].
#[inline]
pub fn record_resolver_is_dir() {
    if !enabled_fast() {
        return;
    }
    counters()
        .resolver_is_dir_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one `std::fs::read_dir()` call from the resolver. Sibling to
/// [`record_resolver_is_file`]. The cost of the syscall itself dwarfs
/// the counter overhead — this helper is only structural cleanup.
#[inline]
pub fn record_resolver_read_dir() {
    if !enabled_fast() {
        return;
    }
    counters()
        .resolver_read_dir_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one candidate path examined during module resolution
/// (path-mapping virtual roots and suffix-extension expansion).
/// Lifted into a helper so the two emit sites in
/// `crates/tsz-cli/src/driver/resolution.rs` don't re-pay the `counters()`
/// `OnceLock` deref.
#[inline]
pub fn record_resolver_candidate_path() {
    if !enabled_fast() {
        return;
    }
    counters()
        .resolver_candidate_paths_total
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one uncached `package.json` read. Sits inside the resolver's
/// `read_package_json_uncached`, which `large-ts-repo` profiles flag
/// as the dominant resolver work — keeping the gate cheap matters even
/// though the surrounding `read_to_string` is several orders of
/// magnitude more expensive.
#[inline]
pub fn record_resolver_read_package_json() {
    if !enabled_fast() {
        return;
    }
    counters()
        .resolver_read_package_json_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a root `CheckerState` construction. Called from each of the
/// nine `CheckerState::new` / `with_*` constructors in
/// `crates/tsz-checker/src/state/state.rs`. Sibling to the other `record_*`
/// helpers — gate once, look up `counters()` once, increment.
#[inline]
pub fn record_checker_state_constructed() {
    if !enabled_fast() {
        return;
    }
    counters()
        .checker_state_constructed
        .fetch_add(1, Ordering::Relaxed);
}

/// Record an invocation of `CheckerContext::reset_for_next_file()`. Bumps
/// only on the sequential session-reuse path (T2.1.B). Sibling to the
/// other `record_*` helpers — gate once, look up `counters()` once,
/// increment. Compared against `checker_state_constructed` in
/// attribution mode to detect reuse-vs-construct directly.
#[inline]
pub fn record_file_session_reset() {
    if !enabled_fast() {
        return;
    }
    counters()
        .file_session_resets
        .fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_direct_cross_file_interface_lowering_outcome(
    outcome: DirectCrossFileInterfaceLoweringOutcome,
) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.direct_cross_file_interface_lowering_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_direct_actual_lib_alias_body_outcome(outcome: DirectActualLibAliasBodyOutcome) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.direct_actual_lib_alias_body_outcome[outcome.as_index()].fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_direct_actual_lib_intl_interface_outcome(
    outcome: DirectActualLibIntlInterfaceOutcome,
) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.direct_actual_lib_intl_interface_outcome[outcome.as_index()].fetch_add(1, Ordering::Relaxed);
}

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
             max recursion depth        {:>12}\n\
             Checker construction:\n  \
             CheckerState::new          {:>12}\n  \
             ::with_parent_cache        {:>12}\n  \
             ::reset_for_next_file      {:>12}\n  \
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
             type-list intern calls     {:>12}\n  \
             object-shape intern calls  {:>12}\n  \
             function-shape intern calls{:>12}\n  \
             callable-shape intern calls{:>12}\n  \
             application intern calls   {:>12}\n  \
             conditional intern calls   {:>12}\n  \
             mapped intern calls        {:>12}\n\
             Resolver:\n  \
             lookup calls               {:>12}\n  \
             is_file calls              {:>12}\n  \
             is_dir calls               {:>12}\n  \
             read_dir calls             {:>12}\n  \
             read_package_json calls    {:>12}\n  \
             candidate paths total      {:>12}\n",
            snap.delegate.calls,
            snap.delegate.cache_hits_lib,
            snap.delegate.cache_hits_cross_file,
            snap.delegate.misses,
            snap.delegate.max_recursion_depth,
            snap.checker.state_constructed,
            snap.checker.with_parent_cache_constructed,
            snap.checker.file_session_resets,
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
            snap.interner.type_list_intern_calls,
            snap.interner.object_shape_intern_calls,
            snap.interner.function_shape_intern_calls,
            snap.interner.callable_shape_intern_calls,
            snap.interner.application_intern_calls,
            snap.interner.conditional_intern_calls,
            snap.interner.mapped_intern_calls,
            snap.resolver.lookup_calls,
            snap.resolver.is_file_calls.unwrap_or(0),
            snap.resolver.is_dir_calls.unwrap_or(0),
            snap.resolver.read_dir_calls.unwrap_or(0),
            snap.resolver.package_json_reads,
            snap.resolver.candidate_paths_total,
        ) + &Self::dump_compute_type_of_symbol_outcomes()
            + &Self::dump_compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues(
                &snap.compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues,
            )
            + &Self::dump_compute_type_of_symbol_interface_simple_object_declaration_provenance_residues(
                &snap.compute_type_of_symbol_interface_simple_object_declaration_provenance_residues,
            )
            + &Self::dump_compute_type_of_symbol_interface_simple_object_type_reference_reject_residues(
                &snap.compute_type_of_symbol_interface_simple_object_type_reference_reject_residues,
            )
            + &Self::dump_cross_arena_symbol_miss_classification()
            + &Self::dump_cross_arena_alias_shortcut_outcomes()
            + &Self::dump_direct_cross_file_interface_lowering_outcomes()
            + &Self::dump_direct_actual_lib_alias_body_outcomes()
            + &Self::dump_direct_actual_lib_intl_interface_outcomes()
            + &Self::dump_delegate_declaration_file_miss_residues(
                &snap.delegate_declaration_file_miss_residues,
            )
            + &Self::dump_source_file_symbol_arena_cache_eligibility_outcomes()
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
        if source_total == 0
            && kind_total == 0
            && interface_fastpath_total == 0
            && interface_callsite_total == 0
            && interface_simple_object_total == 0
            && interface_simple_object_non_primitive_annotation_kind_total == 0
            && interface_simple_object_type_reference_reject_outcome_total == 0
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

/// Stable schema version for `PerfCounterSnapshot`. Bump when the JSON
/// shape changes in a way the bench harness must adapt to.
pub const PERF_COUNTER_SNAPSHOT_SCHEMA_VERSION: u32 = 2;

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

    fn snapshot_compute_type_of_symbol_interface_simple_object_type_reference_reject_residues()
    -> Vec<ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectResidue> {
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

    fn snapshot_compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues()
    -> Vec<ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationResidue> {
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

    fn snapshot_compute_type_of_symbol_interface_simple_object_declaration_provenance_residues()
    -> Vec<ComputeTypeOfSymbolInterfaceSimpleObjectDeclarationProvenanceResidue> {
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

#[cfg(test)]
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
            "overlay",
            "resolver",
            "interner",
            "by_reason",
            "delegate_miss_classification",
            "delegate_declaration_file_miss_residues",
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
    fn compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kinds_locks_to_names_array()
     {
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
    fn compute_type_of_symbol_interface_simple_object_type_reference_reject_outcomes_locks_to_names_array()
     {
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
    fn compute_type_of_symbol_interface_simple_object_type_reference_reject_residues_lock_field_shape()
     {
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
    fn compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues_lock_field_shape()
     {
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
    fn compute_type_of_symbol_interface_simple_object_declaration_provenance_residues_lock_field_shape()
     {
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
