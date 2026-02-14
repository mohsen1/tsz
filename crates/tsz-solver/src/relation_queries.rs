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
    /// Controls how SubtypeChecker treats `any`.
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
    pub fn from_flags(flags: u16) -> Self {
        Self {
            flags,
            ..Self::default()
        }
    }

    pub fn with_strict_subtype_checking(mut self, strict: bool) -> Self {
        self.strict_subtype_checking = strict;
        self
    }

    pub fn with_strict_any_propagation(mut self, strict: bool) -> Self {
        self.strict_any_propagation = strict;
        self
    }

    pub fn with_any_propagation_mode(mut self, mode: AnyPropagationMode) -> Self {
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
    pub fn is_related(self) -> bool {
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
    query_relation_with_overrides(
        interner, resolver, source, target, kind, policy, context, &overrides,
    )
}

/// Query a relation using a custom resolver and checker-provided overrides.
pub fn query_relation_with_overrides<'a, R: TypeResolver, P: AssignabilityOverrideProvider>(
    interner: &'a dyn TypeDatabase,
    resolver: &'a R,
    source: TypeId,
    target: TypeId,
    kind: RelationKind,
    policy: RelationPolicy,
    context: RelationContext<'a>,
    overrides: &P,
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
mod tests {
    use super::*;
    use crate::{FunctionShape, ParamInfo, PropertyInfo, TypeInterner};

    fn make_animal_dog(interner: &TypeInterner) -> (TypeId, TypeId) {
        let name = interner.intern_string("name");
        let breed = interner.intern_string("breed");

        let animal = interner.object(vec![PropertyInfo::new(name, TypeId::STRING)]);
        let dog = interner.object(vec![
            PropertyInfo::new(name, TypeId::STRING),
            PropertyInfo::new(breed, TypeId::STRING),
        ]);

        (animal, dog)
    }

    struct AlwaysRejectOverride;

    impl AssignabilityOverrideProvider for AlwaysRejectOverride {
        fn enum_assignability_override(&self, _source: TypeId, _target: TypeId) -> Option<bool> {
            Some(false)
        }

        fn abstract_constructor_assignability_override(
            &self,
            _source: TypeId,
            _target: TypeId,
        ) -> Option<bool> {
            None
        }

        fn constructor_accessibility_override(
            &self,
            _source: TypeId,
            _target: TypeId,
        ) -> Option<bool> {
            None
        }
    }

    #[test]
    fn query_relation_assignable_respects_strict_null_flags() {
        let interner = TypeInterner::new();
        let strict_policy = RelationPolicy::from_flags(RelationCacheKey::FLAG_STRICT_NULL_CHECKS);
        let non_strict_policy = RelationPolicy::from_flags(0);

        let strict_result = query_relation(
            &interner,
            TypeId::NULL,
            TypeId::NUMBER,
            RelationKind::Assignable,
            strict_policy,
            RelationContext::default(),
        );
        let non_strict_result = query_relation(
            &interner,
            TypeId::NULL,
            TypeId::NUMBER,
            RelationKind::Assignable,
            non_strict_policy,
            RelationContext::default(),
        );

        assert!(!strict_result.is_related());
        assert!(non_strict_result.is_related());
    }

    #[test]
    fn query_relation_bivariant_callback_mode_relaxes_function_parameter_variance() {
        let interner = TypeInterner::new();
        let (animal, dog) = make_animal_dog(&interner);

        let fn_dog = interner.function(FunctionShape {
            params: vec![ParamInfo::unnamed(dog)],
            this_type: None,
            return_type: TypeId::VOID,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let fn_animal = interner.function(FunctionShape {
            params: vec![ParamInfo::unnamed(animal)],
            this_type: None,
            return_type: TypeId::VOID,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        let policy = RelationPolicy::from_flags(
            RelationCacheKey::FLAG_STRICT_NULL_CHECKS
                | RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES,
        );

        let strict_result = query_relation(
            &interner,
            fn_dog,
            fn_animal,
            RelationKind::Assignable,
            policy,
            RelationContext::default(),
        );
        let bivariant_result = query_relation(
            &interner,
            fn_dog,
            fn_animal,
            RelationKind::AssignableBivariantCallbacks,
            policy,
            RelationContext::default(),
        );

        assert!(!strict_result.is_related());
        assert!(bivariant_result.is_related());
    }

    #[test]
    fn query_relation_subtype_and_overlap_work() {
        let interner = TypeInterner::new();
        let policy = RelationPolicy::from_flags(RelationCacheKey::FLAG_STRICT_NULL_CHECKS);

        let subtype_result = query_relation(
            &interner,
            TypeId::NUMBER,
            TypeId::ANY,
            RelationKind::Subtype,
            policy,
            RelationContext::default(),
        );
        let no_overlap = query_relation(
            &interner,
            TypeId::STRING,
            TypeId::NUMBER,
            RelationKind::Overlap,
            policy,
            RelationContext::default(),
        );
        let overlap = query_relation(
            &interner,
            TypeId::STRING,
            TypeId::STRING,
            RelationKind::Overlap,
            policy,
            RelationContext::default(),
        );

        assert!(subtype_result.is_related());
        assert!(!no_overlap.is_related());
        assert!(overlap.is_related());
    }

    #[test]
    fn query_relation_redeclaration_identity_uses_compat_identity_rules() {
        let interner = TypeInterner::new();
        let policy = RelationPolicy::from_flags(RelationCacheKey::FLAG_STRICT_NULL_CHECKS);

        let any_to_string = query_relation(
            &interner,
            TypeId::ANY,
            TypeId::STRING,
            RelationKind::RedeclarationIdentical,
            policy,
            RelationContext::default(),
        );
        let number_to_string = query_relation(
            &interner,
            TypeId::NUMBER,
            TypeId::STRING,
            RelationKind::RedeclarationIdentical,
            policy,
            RelationContext::default(),
        );

        assert!(any_to_string.is_related());
        assert!(!number_to_string.is_related());
    }

    #[test]
    fn query_relation_with_overrides_can_short_circuit_assignability() {
        let interner = TypeInterner::new();
        let resolver = NoopResolver;
        let overrides = AlwaysRejectOverride;
        let policy = RelationPolicy::from_flags(RelationCacheKey::FLAG_STRICT_NULL_CHECKS);

        let result = query_relation_with_overrides(
            &interner,
            &resolver,
            TypeId::NUMBER,
            TypeId::NUMBER,
            RelationKind::Assignable,
            policy,
            RelationContext::default(),
            &overrides,
        );

        assert!(!result.is_related());
    }
}
