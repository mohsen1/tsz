//! Unified relation query entrypoints.
//!
//! This module centralizes common relation checks (assignability, subtype,
//! overlap) behind one API so checker code can call Solver queries instead
//! of wiring checker internals directly to concrete checker engines.

use crate::TypeDatabase;
use crate::compat::{AssignabilityOverrideProvider, CompatChecker, NoopOverrideProvider};
use crate::db::QueryDatabase;
use crate::inheritance::InheritanceGraph;
use crate::operations::AssignabilityChecker;
use crate::subtype::{AnyPropagationMode, NoopResolver, SubtypeChecker, TypeResolver};
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
}

impl Default for RelationPolicy {
    fn default() -> Self {
        Self {
            flags: RelationCacheKey::FLAG_STRICT_NULL_CHECKS,
            strict_subtype_checking: false,
            strict_any_propagation: false,
            any_propagation_mode: AnyPropagationMode::All,
        }
    }
}

impl RelationPolicy {
    pub const fn from_flags(flags: u16) -> Self {
        use crate::RelationCacheKey;
        let strict_any = (flags & RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES) != 0;
        Self {
            flags,
            strict_subtype_checking: false,
            strict_any_propagation: strict_any,
            any_propagation_mode: if strict_any {
                AnyPropagationMode::TopLevelOnly
            } else {
                AnyPropagationMode::All
            },
        }
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
        .with_any_propagation_mode(policy.any_propagation_mode);
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

#[cfg(test)]
#[path = "../tests/relation_queries_tests.rs"]
mod tests;
