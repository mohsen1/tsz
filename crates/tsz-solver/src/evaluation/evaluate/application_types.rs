//! Application evaluation coordination types.

use crate::types::{MappedType, TypeId, TypeParamInfo};

/// Snapshot of resolver/interner state needed for an `Application(base, args)`
/// evaluation. Built once by `TypeEvaluator::application_evaluation_context`
/// so the rest of `evaluate_application` operates on a typed bundle rather
/// than recomputing the same facts at multiple call sites.
pub(super) struct ApplicationEvalContext {
    /// Formal type parameters declared on the `DefId` resolved from
    /// `app.base`, when the resolver exposes them. `None` triggers the
    /// lite-resolver fallback that extracts parameters from the resolved
    /// body's structure.
    pub(super) type_params: Option<Vec<TypeParamInfo>>,
    /// The resolved body of the `DefId`, when known.
    pub(super) resolved: Option<TypeId>,
    /// Set when `app.base` resolves to a `DefKind::TypeAlias` (vs a class
    /// or interface). Drives display-alias storage policy.
    pub(super) is_type_alias_def: bool,
    /// Whether display-alias bookkeeping should prefer the `Application`
    /// form. True only for non-conditional type-alias applications.
    pub(super) prefer_application_display_alias: bool,
    /// Set when `app.base` is a `TypeQuery` (i.e. `typeof ClassName<T>`).
    /// For `TypeQuery`-based applications the caller wants the constructor
    /// type, not the instance type, so `extract_class_instance_body` must
    /// be skipped.
    pub(super) base_is_type_query: bool,
}

/// Common opening preamble for the homomorphic-mapped shortcuts:
/// `try_homomorphic_mapped_passthrough` and `try_distribute_mapped_union_arg`
/// both require `body == { [P in keyof T]: ... }` with `T` resolvable in
/// `type_params`. Sharing the destructure protects against drift between
/// the two call sites and avoids re-evaluating the same argument twice.
pub(super) struct HomomorphicMappedArg {
    pub(super) mapped: MappedType,
    pub(super) source: TypeId,
    pub(super) tp: TypeParamInfo,
    pub(super) idx: usize,
    pub(super) resolved_arg: TypeId,
}

/// Distinguishes shortcut paths in `evaluate_application` (cache hits,
/// homomorphic passthrough, mapped-union distribution) from the full
/// instantiation path.
///
/// Shortcut paths historically returned via early `decrement_def_depth` +
/// `return`, which leaves `self.apparent_conditional_branch == None` for
/// the outer caller. The full path restores the outer caller's apparent
/// branch and runs display-alias bookkeeping. The orchestrator uses this
/// outcome to apply the right cleanup without losing the historical
/// invariant.
pub(super) enum ApplicationEvalOutcome {
    /// Cache hit or body-aware shortcut. Outer caller's apparent branch
    /// is NOT restored.
    ShortCircuit(TypeId),
    /// Result computed via the full instantiation pipeline. Apparent
    /// branch is restored and display-alias bookkeeping runs.
    Computed(TypeId),
}
