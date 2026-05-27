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
        /// Anything not explicitly classified above.
        Other = 16 => "Other",
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

/// Outcome of the direct source-file type-alias lowering shortcut attempted
/// before a `DelegateCrossArenaSymbol` miss constructs a child checker.
///
/// This classifies the regular source-file alias lane separately from
/// declaration-file and actual-lib shortcuts. The buckets identify which
/// structural proof failed, so performance work can decide whether to widen a
/// guard, cache a result, or leave the alias on the exact child-checker path.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum DirectSourceFileTypeAliasLoweringOutcome {
    Success = 0,
    MissingTargetFile = 1,
    MissingArenaOrBinder = 2,
    SourceFileArenaNotAllowed = 3,
    MissingSymbol = 4,
    NotTypeAlias = 5,
    DisallowedMergeFlags = 6,
    MultipleDeclarations = 7,
    NameMismatch = 8,
    MissingTypeAliasNode = 9,
    BodyNotDirectLowerable = 10,
    TypeQueryOrSelfReference = 11,
    UnknownOrError = 12,
}

pub const DIRECT_SOURCE_FILE_TYPE_ALIAS_LOWERING_OUTCOME_COUNT: usize = 13;

pub const DIRECT_SOURCE_FILE_TYPE_ALIAS_LOWERING_OUTCOME_NAMES: [&str;
    DIRECT_SOURCE_FILE_TYPE_ALIAS_LOWERING_OUTCOME_COUNT] = [
    "success",
    "missing_target_file",
    "missing_arena_or_binder",
    "source_file_arena_not_allowed",
    "missing_symbol",
    "not_type_alias",
    "disallowed_merge_flags",
    "multiple_declarations",
    "name_mismatch",
    "missing_type_alias_node",
    "body_not_direct_lowerable",
    "type_query_or_self_reference",
    "unknown_or_error",
];

impl DirectSourceFileTypeAliasLoweringOutcome {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

/// Root syntax family for source-file type-alias bodies rejected by the direct
/// lowering shortcut.
///
/// These buckets are intentionally coarse. They classify the structural
/// operation that needs a proof before the `body_not_direct_lowerable` gate can
/// be safely widened, without naming user aliases or benchmark files.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum DirectSourceFileTypeAliasBodyRejectionKind {
    TypeReference = 0,
    ConditionalType = 1,
    TypeOperator = 2,
    IndexedAccessType = 3,
    MappedType = 4,
    TypeLiteral = 5,
    TemplateLiteralType = 6,
    UnionOrIntersectionType = 7,
    ArrayOrTupleType = 8,
    WrappedType = 9,
    InferType = 10,
    Other = 11,
}

pub const DIRECT_SOURCE_FILE_TYPE_ALIAS_BODY_REJECTION_KIND_COUNT: usize = 12;

pub const DIRECT_SOURCE_FILE_TYPE_ALIAS_BODY_REJECTION_KIND_NAMES: [&str;
    DIRECT_SOURCE_FILE_TYPE_ALIAS_BODY_REJECTION_KIND_COUNT] = [
    "type_reference",
    "conditional_type",
    "type_operator",
    "indexed_access_type",
    "mapped_type",
    "type_literal",
    "template_literal_type",
    "union_or_intersection_type",
    "array_or_tuple_type",
    "wrapped_type",
    "infer_type",
    "other",
];

impl DirectSourceFileTypeAliasBodyRejectionKind {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

/// Structural bucket for root `TypeReference` alias bodies rejected by the
/// source-file direct-lowering proof.
///
/// This intentionally records symbol shape and type-argument shape, not the
/// user-written type name. The goal is to decide whether the next safe widening
/// target is alias applications, interface refs, unresolved names, or a parser
/// shape such as qualified names.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum DirectSourceFileTypeAliasTypeReferenceRejectionKind {
    OwnTypeParamWithTypeArguments = 0,
    BuiltinArrayWrongArity = 1,
    BuiltinArrayNonDirectArgument = 2,
    LocalTypeAliasNoArguments = 3,
    LocalTypeAliasWithArguments = 4,
    LocalInterfaceNoArguments = 5,
    LocalInterfaceWithArguments = 6,
    LocalTypeParameter = 7,
    LocalAliasSymbol = 8,
    LocalNamespaceSymbol = 9,
    LocalValueSymbol = 10,
    LocalTypeLiteralSymbol = 11,
    LocalTransientSymbol = 12,
    LocalOtherSymbol = 13,
    UnresolvedIdentifier = 14,
    QualifiedName = 15,
    Other = 16,
}

pub const DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_COUNT: usize = 17;

pub const DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_NAMES: [&str;
    DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_COUNT] = [
    "own_type_param_with_type_arguments",
    "builtin_array_wrong_arity",
    "builtin_array_non_direct_argument",
    "local_type_alias_no_arguments",
    "local_type_alias_with_arguments",
    "local_interface_no_arguments",
    "local_interface_with_arguments",
    "local_type_parameter",
    "local_alias_symbol",
    "local_namespace_symbol",
    "local_value_symbol",
    "local_type_literal_symbol",
    "local_transient_symbol",
    "local_other_symbol",
    "unresolved_identifier",
    "qualified_name",
    "other",
];

impl DirectSourceFileTypeAliasTypeReferenceRejectionKind {
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
    CacheableDeclarationFile = 13,
}

pub const SOURCE_FILE_SYMBOL_ARENA_CACHE_ELIGIBILITY_OUTCOME_COUNT: usize = 14;

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
    "cacheable_declaration_file",
];

impl SourceFileSymbolArenaCacheEligibilityOutcome {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

pub const DELEGATE_DECLARATION_FILE_MISS_RESIDUE_LIMIT: usize = 128;
pub const DELEGATE_SOURCE_FILE_MISS_RESIDUE_LIMIT: usize = 128;
pub const DIRECT_SOURCE_FILE_TYPE_ALIAS_BODY_REJECTION_RESIDUE_LIMIT: usize = 128;
pub const SLOW_CHECK_FILE_TIMING_LIMIT: usize = 32;
pub const SLOW_CHECK_STATEMENT_TIMING_LIMIT: usize = 64;

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
