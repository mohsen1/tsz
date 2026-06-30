use super::TypeId;

/// Which relation is being cached.
///
/// This enum replaces the historical `u8` relation tag (`SUBTYPE=0`,
/// `ASSIGNABLE=1`, `IDENTICAL=2`). Using a real enum prevents accidental
/// collisions with unrelated `u8` values and makes cache partitioning visible
/// at API boundaries.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum RelationCacheKind {
    /// Structural subtyping (Judge layer).
    Subtype,
    /// TypeScript assignability (Lawyer layer).
    Assignable,
    /// Checker-final assignability: the Lawyer relation verdict combined
    /// with the checker's post-relation compatibility gates (iterator
    /// protocol, namespace property mismatch, alias-application argument
    /// rejection, keyof literal membership). Entries under this kind are
    /// written and read only by the `tsz-checker` assignability boundary, so
    /// a cached verdict is authoritative without checker post-processing and
    /// never collides with raw Lawyer-relation entries.
    CheckerAssignable,
    /// Variable-redeclaration identity.
    Identical,
}

bitflags::bitflags! {
    /// Behavior-affecting boolean configuration for a relation check.
    ///
    /// Every bit here corresponds to a compiler option or mode toggle that
    /// can change whether two types are considered related. The cache key
    /// encodes these bits so that results computed under one configuration
    /// never leak into another.
    ///
    /// Bits `0..=8` are preserved from the original packed `u16` layout so
    /// legacy callers (e.g. checker boundary helpers that import the
    /// `FLAG_*` constants) continue to interoperate byte-for-byte. Bits
    /// `9..=15` are new and encode previously-missing Lawyer-layer options
    /// that were silently missing from the cache key.
    #[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
    pub struct RelationFlags: u32 {
        /// `strictNullChecks` compiler option.
        const STRICT_NULL_CHECKS            = 1 << 0;
        /// `strictFunctionTypes` compiler option.
        const STRICT_FUNCTION_TYPES         = 1 << 1;
        /// `exactOptionalPropertyTypes` compiler option.
        const EXACT_OPTIONAL_PROPERTY_TYPES = 1 << 2;
        /// `noUncheckedIndexedAccess` compiler option.
        const NO_UNCHECKED_INDEXED_ACCESS   = 1 << 3;
        /// Disable method bivariance (Sound Mode).
        const DISABLE_METHOD_BIVARIANCE     = 1 << 4;
        /// Allow `any` return type to be compatible with `void` target returns.
        const ALLOW_VOID_RETURN             = 1 << 5;
        /// Treat rest parameters of `any`/`unknown` as bivariant.
        const ALLOW_BIVARIANT_REST          = 1 << 6;
        /// Allow required-parameter count mismatches for bivariant methods.
        const ALLOW_BIVARIANT_PARAM_COUNT   = 1 << 7;
        /// Disable generic type-parameter erasure in function subtype checks.
        const NO_ERASE_GENERICS             = 1 << 8;
        // --- Lawyer-layer options added to close the drift between
        //     `RelationPolicy` and `RelationCacheKey` ---
        /// Additional strictness in the compatibility layer (lib.d.ts).
        const STRICT_SUBTYPE_CHECKING       = 1 << 9;
        /// Strict-any propagation in Sound Mode. When set, `any` does not
        /// silence structural mismatches in the Lawyer layer.
        ///
        /// Must be set explicitly; it is NOT derived from
        /// `STRICT_FUNCTION_TYPES`.
        const STRICT_ANY_PROPAGATION        = 1 << 10;
        /// Skip TS2559 weak-type checks. Matches tsc's `isTypeAssignableTo`.
        const SKIP_WEAK_TYPE_CHECKS         = 1 << 11;
        /// Treat recursive relation cycles as assumed-related. When clear,
        /// cycles resolve to "not related".
        const ASSUME_RELATED_ON_CYCLE       = 1 << 12;
        /// Retry a failed contextual generic-signature inference by comparing
        /// erased signatures. This is a targeted relation mode for interface
        /// property compatibility; ordinary assignment keeps inference failure
        /// definitive so invalid generic assignments still report TS2322.
        const ALLOW_ERASED_GENERIC_SIGNATURE_RETRY = 1 << 13;
        /// We are entering a callback parameter check: the next function
        /// signature comparison is reached from a callable parameter and
        /// must use strict variance (no method-bivariance loosening),
        /// matching tsc's `SignatureCheckMode.Callback` bit. Cache results
        /// computed under this mode separately from non-callback results.
        const IN_CALLBACK_PARAM_CHECK       = 1 << 14;
        /// Strict identity mode for the readonly modifier. See the
        /// `strict_readonly_identity` field on `SubtypeChecker` for the
        /// rationale and toggle site.
        const STRICT_READONLY_IDENTITY      = 1 << 15;
        /// A class-symbol classifier is active for this relation check (the
        /// solver was configured via `with_class_check`).
        ///
        /// The classifier makes a class-flagged symbol that has no resolvable
        /// `DefId` behave nominally - it then needs an explicit declared index
        /// signature (`requires_explicit_declared_index_signature`) and gains
        /// the both-classes nominal-heritage shortcut (`nominal_heritage_subtype`)
        /// - whereas absent the classifier the same shape is judged purely
        /// structurally. Those two verdicts genuinely differ, so they must not
        /// share a cache slot. The classifier itself is a pure function of the
        /// program's binder `CLASS` flags, fixed for the whole compilation, so a
        /// single discriminating bit fully partitions the two regimes and lets
        /// class-context verdicts live in the cross-checker shared cache without
        /// poisoning the class-agnostic regime (issue #13828).
        const CLASS_CHECK_CONTEXT           = 1 << 16;
    }
}

/// How `any` should be treated at the current depth when caching.
///
/// The `SubtypeChecker` uses `AnyPropagationMode` to decide whether to
/// short-circuit on `any`. Because `TopLevelOnly` behavior depends on the
/// current recursion depth, we encode the *effective* mode in the cache key
/// rather than the configured mode - otherwise a top-level and a nested
/// lookup could share a slot and contaminate one another.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub enum CachedAnyMode {
    /// Propagate `any` at every depth (TypeScript default).
    #[default]
    All,
    /// Configured `TopLevelOnly`, currently at depth 0.
    TopLevelOnlyAtTop,
    /// Configured `TopLevelOnly`, currently nested (depth > 0).
    TopLevelOnlyNested,
    /// Overload-resolution subtype pass: an `any` source is not related to
    /// non-`any`/`unknown` targets at every depth, while an `any` target
    /// still accepts everything. Depth-independent, so it needs no
    /// at-top/nested split. Results computed under this mode must never
    /// share a cache slot with the default assignable relation.
    AnySourceNotRelated,
}

/// Canonical cache-partitioning configuration for relation queries.
///
/// Every behavior-affecting option that can change the outcome of a relation
/// check lives here. Two relation results computed under different
/// `RelationCacheConfig` values are guaranteed to live in different cache
/// slots, which makes it impossible to accidentally share a slot across
/// behavioral boundaries.
///
/// Fields that are strictly diagnostic (they affect error messages but not
/// the boolean relation outcome) must not be added here; they belong on the
/// higher-level `RelationPolicy` instead.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct RelationCacheConfig {
    /// Packed behavior-affecting boolean options.
    pub flags: RelationFlags,
    /// Effective `any`-propagation mode for this lookup.
    pub any_mode: CachedAnyMode,
}

impl RelationCacheConfig {
    /// Construct a cache config with the given flags and any-mode.
    pub(crate) const fn new(flags: RelationFlags, any_mode: CachedAnyMode) -> Self {
        Self { flags, any_mode }
    }

    /// Fluent builder override.
    pub const fn with_any_mode(mut self, any_mode: CachedAnyMode) -> Self {
        self.any_mode = any_mode;
        self
    }
}

/// Cache key for type relation queries (subtype, assignability, identity).
///
/// This key fully represents every behavior-affecting input that can change
/// the outcome of a relation check. Two queries that differ in *any*
/// behavior-affecting configuration will produce different keys, which makes
/// it impossible to accidentally share a cache slot across behavioral
/// boundaries.
///
/// ## Fields
///
/// - `source`: The source type being compared.
/// - `target`: The target type being compared.
/// - `relation`: Which relation is being cached. See [`RelationCacheKind`].
/// - `config`:   The behavior-affecting configuration. See [`RelationCacheConfig`].
///
/// ## Construction
///
/// Prefer the typed [`RelationCacheKey::for_subtype`],
/// [`RelationCacheKey::for_assignability`], and
/// [`RelationCacheKey::for_identical`] builders.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct RelationCacheKey {
    pub source: TypeId,
    pub target: TypeId,
    pub relation: RelationCacheKind,
    pub config: RelationCacheConfig,
    /// Resolved polymorphic-`this` binding under which this verdict holds, or
    /// [`TypeId::NONE`] when the pair does not depend on a `this` binding.
    ///
    /// A pair that carries a polymorphic `this` resolves `ThisType` against the
    /// current receiver, so its relation outcome is valid only under that
    /// receiver. Encoding the resolved binding here lets such verdicts live in
    /// the cross-checker shared cache without poisoning a sibling checker that
    /// compares the same `(source, target)` under a different receiver (issue
    /// #13828). It is [`TypeId::NONE`] for every non-`this` pair, so ordinary
    /// keys are byte-identical to the pre-`this`-context protocol and share
    /// their existing cache slots unchanged.
    pub this_context: TypeId,
}

impl RelationCacheKey {
    // Legacy `FLAG_*` constants preserved as `u16` so that callers still on
    // the packed protocol (checker `RelationFlags` boundary, tests, docs)
    // continue to compile unchanged. All new code should use the typed
    // `RelationFlags` bitflags via the typed builders instead.
    pub const FLAG_STRICT_NULL_CHECKS: u16 = RelationFlags::STRICT_NULL_CHECKS.bits() as u16;
    pub const FLAG_STRICT_FUNCTION_TYPES: u16 = RelationFlags::STRICT_FUNCTION_TYPES.bits() as u16;
    pub const FLAG_EXACT_OPTIONAL_PROPERTY_TYPES: u16 =
        RelationFlags::EXACT_OPTIONAL_PROPERTY_TYPES.bits() as u16;
    pub const FLAG_NO_UNCHECKED_INDEXED_ACCESS: u16 =
        RelationFlags::NO_UNCHECKED_INDEXED_ACCESS.bits() as u16;
    pub const FLAG_DISABLE_METHOD_BIVARIANCE: u16 =
        RelationFlags::DISABLE_METHOD_BIVARIANCE.bits() as u16;
    pub const FLAG_ALLOW_VOID_RETURN: u16 = RelationFlags::ALLOW_VOID_RETURN.bits() as u16;
    pub const FLAG_ALLOW_BIVARIANT_REST: u16 = RelationFlags::ALLOW_BIVARIANT_REST.bits() as u16;
    pub const FLAG_ALLOW_BIVARIANT_PARAM_COUNT: u16 =
        RelationFlags::ALLOW_BIVARIANT_PARAM_COUNT.bits() as u16;
    /// Disable generic type parameter erasure in function subtype checks.
    /// When set, non-generic functions are NOT assignable to generic functions,
    /// matching tsc's `eraseGenerics=false` behavior for implements/extends checks.
    pub const FLAG_NO_ERASE_GENERICS: u16 = RelationFlags::NO_ERASE_GENERICS.bits() as u16;
    /// Allow a failed contextual generic-signature inference to retry with
    /// erased signatures. Used for interface property compatibility, not
    /// ordinary assignment.
    pub const FLAG_ALLOW_ERASED_GENERIC_SIGNATURE_RETRY: u16 =
        RelationFlags::ALLOW_ERASED_GENERIC_SIGNATURE_RETRY.bits() as u16;

    /// Typed builder for subtype cache entries.
    pub const fn for_subtype(source: TypeId, target: TypeId, config: RelationCacheConfig) -> Self {
        Self {
            source,
            target,
            relation: RelationCacheKind::Subtype,
            config,
            this_context: TypeId::NONE,
        }
    }

    /// Typed builder for assignability cache entries.
    pub const fn for_assignability(
        source: TypeId,
        target: TypeId,
        config: RelationCacheConfig,
    ) -> Self {
        Self {
            source,
            target,
            relation: RelationCacheKind::Assignable,
            config,
            this_context: TypeId::NONE,
        }
    }

    /// Typed builder for checker-final assignability cache entries.
    ///
    /// See [`RelationCacheKind::CheckerAssignable`]: only the `tsz-checker`
    /// assignability boundary constructs these keys, so checker-final
    /// verdicts never share a slot with raw Lawyer-relation entries.
    pub const fn for_checker_assignability(
        source: TypeId,
        target: TypeId,
        config: RelationCacheConfig,
    ) -> Self {
        Self {
            source,
            target,
            relation: RelationCacheKind::CheckerAssignable,
            config,
            this_context: TypeId::NONE,
        }
    }

    /// Typed builder for redeclaration-identity cache entries.
    pub const fn for_identical(
        source: TypeId,
        target: TypeId,
        config: RelationCacheConfig,
    ) -> Self {
        Self {
            source,
            target,
            relation: RelationCacheKind::Identical,
            config,
            this_context: TypeId::NONE,
        }
    }

    /// Return this key discriminated by a resolved polymorphic-`this` binding.
    ///
    /// Passing [`TypeId::NONE`] (the default) is a no-op, leaving the key
    /// byte-identical to the undiscriminated form. See [`Self::this_context`].
    #[must_use]
    pub const fn with_this_context(mut self, this_context: TypeId) -> Self {
        self.this_context = this_context;
        self
    }
}

/// Stored outcome of a relation query in the cross-checker relation caches.
///
/// `True` / `False` are definitive, budget-independent verdicts. `LimitTrue`
/// records a `tsc` `Ternary.Maybe`-style assumed-related verdict produced when
/// a relation chain exhausted its global fuel budget. It is honest only for a
/// later query whose *remaining* fuel budget at lookup time is at most
/// `fuel_band` (the budget the recorded run started with): a query holding a
/// larger budget could complete the comparison honestly and must recompute
/// instead of reusing the truncated verdict (fuel-band cache honesty).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RelationCacheValue {
    /// Definitively related.
    True,
    /// Definitively not related.
    False,
    /// Assumed related because the computation hit the global fuel limit
    /// while `fuel_band` units of budget were available at its entry.
    LimitTrue {
        /// Remaining global subtype fuel at the recorded run's entry.
        fuel_band: u32,
    },
}

impl RelationCacheValue {
    /// Wrap a definitive boolean verdict.
    #[must_use]
    pub const fn from_bool(related: bool) -> Self {
        if related { Self::True } else { Self::False }
    }

    /// The definitive verdict, or `None` for budget-conditional entries.
    #[must_use]
    pub const fn as_definitive(self) -> Option<bool> {
        match self {
            Self::True => Some(true),
            Self::False => Some(false),
            Self::LimitTrue { .. } => None,
        }
    }
}
