//! Unified relation query entrypoints.
//!
//! This module centralizes common relation checks (assignability, subtype,
//! overlap) behind one API so checker code can call Solver queries instead
//! of wiring checker internals directly to concrete checker engines.

use crate::TypeDatabase;
use crate::caches::db::QueryDatabase;
use crate::classes::inheritance::InheritanceGraph;
use crate::operations::AssignabilityChecker;
use crate::relations::compat::{
    AssignabilityOverrideProvider, CompatChecker, NoopOverrideProvider,
};
use crate::relations::subtype::{AnyPropagationMode, NoopResolver, SubtypeChecker, TypeResolver};
use crate::types::{RelationCacheKey, SymbolRef, TypeId};

/// Relation categories supported by the unified query API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationKind {
    /// TypeScript assignability (Lawyer layer).
    Assignable,
    /// Assignability with bivariant callback parameters.
    AssignableBivariantCallbacks,
    /// Structural subtyping (Judge layer).
    Subtype,
    /// Type overlap check used by TS2367-style diagnostics.
    Overlap,
    /// Type identity used for variable redeclaration compatibility.
    RedeclarationIdentical,
}

/// Policy knobs for relation checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelationPolicy {
    /// Packed relation flags (same layout as `RelationCacheKey.flags`).
    pub flags: u16,
    /// Enables additional strictness in the compatibility layer.
    pub strict_subtype_checking: bool,
    /// Disables `any`-suppression in compatibility fast paths.
    pub strict_any_propagation: bool,
    /// Controls how `SubtypeChecker` treats `any`.
    pub any_propagation_mode: AnyPropagationMode,
    /// Whether recursive relation cycles should be treated as assumed-related.
    pub assume_related_on_cycle: bool,
    /// Skip weak type checks (TS2559) during assignability.
    ///
    /// In tsc, `isTypeAssignableTo` does NOT include the weak type check.
    /// The weak type check is only applied at specific diagnostic sites
    /// (variable declarations, argument passing, return statements).
    /// Flow narrowing guards need pure assignability without weak type
    /// rejection, matching tsc's `isTypeAssignableTo` behavior.
    pub skip_weak_type_checks: bool,
    /// Erase generic type parameters in function subtype checks.
    ///
    /// When true, non-generic functions can match generic targets by erasing
    /// target type parameters to their constraints. Matches tsc's
    /// `eraseGenerics` flag used in the comparable relation.
    pub erase_generics: bool,
}

impl Default for RelationPolicy {
    fn default() -> Self {
        Self {
            flags: RelationCacheKey::FLAG_STRICT_NULL_CHECKS,
            strict_subtype_checking: false,
            strict_any_propagation: false,
            any_propagation_mode: AnyPropagationMode::All,
            assume_related_on_cycle: true,
            skip_weak_type_checks: false,
            erase_generics: true,
        }
    }
}

impl RelationPolicy {
    pub const fn from_flags(flags: u16) -> Self {
        use crate::RelationCacheKey;
        let strict_any = (flags & RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES) != 0;
        // erase_generics defaults to true unless the NO_ERASE_GENERICS flag is set.
        // This preserves backward compatibility while allowing specific paths
        // (implements/extends checking) to disable erasure.
        let erase_generics = (flags & RelationCacheKey::FLAG_NO_ERASE_GENERICS) == 0;
        Self {
            flags,
            strict_subtype_checking: false,
            strict_any_propagation: strict_any,
            any_propagation_mode: if strict_any {
                AnyPropagationMode::TopLevelOnly
            } else {
                AnyPropagationMode::All
            },
            assume_related_on_cycle: true,
            skip_weak_type_checks: false,
            erase_generics,
        }
    }

    pub const fn with_erase_generics(mut self, erase: bool) -> Self {
        self.erase_generics = erase;
        self
    }

    pub const fn with_strict_subtype_checking(mut self, strict: bool) -> Self {
        self.strict_subtype_checking = strict;
        self
    }

    pub const fn with_strict_any_propagation(mut self, strict: bool) -> Self {
        self.strict_any_propagation = strict;
        self
    }

    pub const fn with_any_propagation_mode(mut self, mode: AnyPropagationMode) -> Self {
        self.any_propagation_mode = mode;
        self
    }

    pub const fn with_assume_related_on_cycle(mut self, assume: bool) -> Self {
        self.assume_related_on_cycle = assume;
        self
    }

    pub const fn with_skip_weak_type_checks(mut self, skip: bool) -> Self {
        self.skip_weak_type_checks = skip;
        self
    }
}

/// Optional shared context needed by relation engines.
#[derive(Clone, Copy, Default)]
pub struct RelationContext<'a> {
    pub query_db: Option<&'a dyn QueryDatabase>,
    pub inheritance_graph: Option<&'a InheritanceGraph>,
    pub class_check: Option<&'a dyn Fn(SymbolRef) -> bool>,
}

/// Result of a relation check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelationResult {
    pub kind: RelationKind,
    pub related: bool,
    pub depth_exceeded: bool,
}

impl RelationResult {
    #[inline]
    pub const fn is_related(self) -> bool {
        self.related
    }
}

/// Structured failure details for assignability diagnostics.
#[derive(Debug, Clone)]
pub struct AssignabilityFailureAnalysis {
    pub weak_union_violation: bool,
    pub failure_reason: Option<crate::SubtypeFailureReason>,
}

/// Analyze assignability failure details using a configured compat checker.
pub fn analyze_assignability_failure_with_resolver<'a, R: TypeResolver, F>(
    interner: &'a dyn TypeDatabase,
    resolver: &'a R,
    source: TypeId,
    target: TypeId,
    configure: F,
) -> AssignabilityFailureAnalysis
where
    F: FnOnce(&mut CompatChecker<'a, R>),
{
    let mut checker = CompatChecker::with_resolver(interner, resolver);
    configure(&mut checker);
    AssignabilityFailureAnalysis {
        weak_union_violation: checker.is_weak_union_violation(source, target),
        failure_reason: checker.explain_failure(source, target),
    }
}

/// Query a relation using a no-op resolver and no overrides.
pub fn query_relation(
    interner: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
    kind: RelationKind,
    policy: RelationPolicy,
    context: RelationContext<'_>,
) -> RelationResult {
    let resolver = NoopResolver;
    query_relation_with_resolver(interner, &resolver, source, target, kind, policy, context)
}

/// Query a relation using a custom resolver and no checker overrides.
pub fn query_relation_with_resolver<'a, R: TypeResolver>(
    interner: &'a dyn TypeDatabase,
    resolver: &'a R,
    source: TypeId,
    target: TypeId,
    kind: RelationKind,
    policy: RelationPolicy,
    context: RelationContext<'a>,
) -> RelationResult {
    let overrides = NoopOverrideProvider;
    query_relation_with_overrides(RelationQueryInputs {
        interner,
        resolver,
        source,
        target,
        kind,
        policy,
        context,
        overrides: &overrides,
    })
}

/// Query a relation using a custom resolver and checker-provided overrides.
pub fn query_relation_with_overrides<
    'a,
    R: TypeResolver,
    P: AssignabilityOverrideProvider + ?Sized,
>(
    RelationQueryInputs {
        interner,
        resolver,
        source,
        target,
        kind,
        policy,
        context,
        overrides,
    }: RelationQueryInputs<'a, R, P>,
) -> RelationResult {
    let (related, depth_exceeded) = match kind {
        RelationKind::Assignable => {
            let mut checker = configured_compat_checker(interner, resolver, policy, context);
            (
                checker.is_assignable_with_overrides(source, target, overrides),
                false,
            )
        }
        RelationKind::AssignableBivariantCallbacks => {
            let mut checker = configured_compat_checker(interner, resolver, policy, context);
            let _ = overrides;
            (
                checker.is_assignable_to_bivariant_callback(source, target),
                false,
            )
        }
        RelationKind::Subtype => {
            let mut checker = configured_subtype_checker(interner, resolver, policy, context);
            let related = checker.is_subtype_of(source, target);
            (related, checker.depth_exceeded())
        }
        RelationKind::Overlap => {
            let checker = configured_subtype_checker(interner, resolver, policy, context);
            (checker.are_types_overlapping(source, target), false)
        }
        RelationKind::RedeclarationIdentical => {
            let mut checker = configured_compat_checker(interner, resolver, policy, context);
            (
                checker.are_types_identical_for_redeclaration(source, target),
                false,
            )
        }
    };

    RelationResult {
        kind,
        related,
        depth_exceeded,
    }
}

/// Bundled inputs for relation queries.
pub struct RelationQueryInputs<'a, R: TypeResolver, P: AssignabilityOverrideProvider + ?Sized> {
    pub interner: &'a dyn TypeDatabase,
    pub resolver: &'a R,
    pub source: TypeId,
    pub target: TypeId,
    pub kind: RelationKind,
    pub policy: RelationPolicy,
    pub context: RelationContext<'a>,
    pub overrides: &'a P,
}

fn configured_compat_checker<'a, R: TypeResolver>(
    interner: &'a dyn TypeDatabase,
    resolver: &'a R,
    policy: RelationPolicy,
    context: RelationContext<'a>,
) -> CompatChecker<'a, R> {
    let mut checker = CompatChecker::with_resolver(interner, resolver);
    checker.apply_flags(policy.flags);
    checker.set_inheritance_graph(context.inheritance_graph);
    checker.set_strict_subtype_checking(policy.strict_subtype_checking);
    checker.set_strict_any_propagation(policy.strict_any_propagation);
    checker.set_assume_related_on_cycle(policy.assume_related_on_cycle);
    checker.set_skip_weak_type_checks(policy.skip_weak_type_checks);
    checker.set_erase_generics(policy.erase_generics);
    if let Some(query_db) = context.query_db {
        checker.set_query_db(query_db);
    }
    checker
}

fn configured_subtype_checker<'a, R: TypeResolver>(
    interner: &'a dyn TypeDatabase,
    resolver: &'a R,
    policy: RelationPolicy,
    context: RelationContext<'a>,
) -> SubtypeChecker<'a, R> {
    let mut checker = SubtypeChecker::with_resolver(interner, resolver)
        .apply_flags(policy.flags)
        .with_any_propagation_mode(policy.any_propagation_mode)
        .with_assume_related_on_cycle(policy.assume_related_on_cycle);
    if let Some(query_db) = context.query_db {
        checker = checker.with_query_db(query_db);
    }
    if let Some(inheritance_graph) = context.inheritance_graph {
        checker = checker.with_inheritance_graph(inheritance_graph);
    }
    if let Some(class_check) = context.class_check {
        checker = checker.with_class_check(class_check);
    }
    checker
}

/// Variance-aware Application-to-Application assignability check.
///
/// When both source and target are type applications with the same base
/// (e.g., `Covariant<A>` vs `Covariant<B>`), computes variance for each
/// type parameter and checks arguments accordingly. This avoids structural
/// expansion which would lose variance information.
///
/// Returns `Some(true/false)` if variance check is conclusive,
/// `None` if the types are not suitable for variance-based checking
/// (different bases, non-Application types, unknown variance).
pub fn check_application_variance<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    query_db: Option<&dyn QueryDatabase>,
    source: TypeId,
    target: TypeId,
    policy: RelationPolicy,
    context: RelationContext<'_>,
) -> Option<bool> {
    use crate::types::TypeData;
    use crate::visitor::lazy_def_id;

    let (s_app_id, t_app_id) = match (db.lookup(source), db.lookup(target)) {
        (Some(TypeData::Application(s)), Some(TypeData::Application(t))) => (s, t),
        _ => return None,
    };

    let s_app = db.type_application(s_app_id);
    let t_app = db.type_application(t_app_id);

    // Only for same-base applications with matching arg counts
    if s_app.base != t_app.base || s_app.args.len() != t_app.args.len() {
        return None;
    }

    let def_id = lazy_def_id(db, s_app.base)?;

    let variances = query_db
        .and_then(|qdb| QueryDatabase::get_type_param_variance(qdb, def_id))
        .or_else(|| {
            crate::relations::variance::compute_type_param_variances_with_resolver(
                db, resolver, def_id,
            )
        });

    let variances = variances?;
    if variances.len() != s_app.args.len() {
        return None;
    }

    // If all parameters are independent (no variance info), we can't make any
    // conclusion from variance alone — fall through to structural checking.
    if variances.iter().all(|v| v.is_empty()) {
        return None;
    }

    // Clone args to avoid borrow conflicts
    let s_args: Vec<TypeId> = s_app.args.to_vec();
    let t_args: Vec<TypeId> = t_app.args.to_vec();

    // Set up a compat checker for the argument checks
    let mut checker = configured_compat_checker(db, resolver, policy, context);
    if let Some(qdb) = query_db {
        checker.set_query_db(qdb);
    }

    let needs_structural_fallback = variances.iter().any(|v| v.needs_structural_fallback());
    let mut all_ok = true;
    let mut any_checked = false;
    for (i, variance) in variances.iter().enumerate() {
        let s_arg = s_args[i];
        let t_arg = t_args[i];

        if variance.is_invariant() {
            any_checked = true;
            if !checker.is_assignable(s_arg, t_arg) || !checker.is_assignable(t_arg, s_arg) {
                all_ok = false;
                break;
            }
        } else if variance.is_covariant() {
            any_checked = true;
            if !checker.is_assignable(s_arg, t_arg) {
                all_ok = false;
                break;
            }
        } else if variance.is_contravariant() {
            any_checked = true;
            if !checker.is_assignable(t_arg, s_arg) {
                all_ok = false;
                break;
            }
        }
        // Independent: no check needed
    }

    // If we didn't actually check any parameter (all independent), fall through
    if !any_checked {
        return None;
    }

    if all_ok {
        // When any type parameter's variance is marked as needing structural fallback
        // (due to mapped type modifiers like -?/+?), don't trust the variance shortcut.
        // Fall through to structural comparison. This handles cases like
        // Required<{a?}> vs Required<{b?}> where args are mutually assignable
        // but the mapped type results are structurally incompatible.
        if needs_structural_fallback {
            return None;
        }
        return Some(true);
    }

    // Variance failures are definitive even with structural fallback.
    // The structural fallback flag means "don't trust True because mapped
    // modifiers could change the structure". But a False result (type args
    // are incompatible) is trustworthy: if the type arguments fail the
    // invariant/covariant/contravariant check, the generic types cannot be
    // compatible regardless of how modifiers transform the structure.
    Some(false)
}

#[cfg(test)]
#[path = "../../tests/relation_queries_tests.rs"]
mod tests;
