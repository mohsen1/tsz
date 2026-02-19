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

    fn constructor_accessibility_override(&self, _source: TypeId, _target: TypeId) -> Option<bool> {
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
        RelationCacheKey::FLAG_STRICT_NULL_CHECKS | RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES,
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

    // any is NOT identical to non-any types for redeclaration (TS2403).
    // var x: any; var x: string; should error because types differ.
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
    // Same type should be identical
    let string_to_string = query_relation(
        &interner,
        TypeId::STRING,
        TypeId::STRING,
        RelationKind::RedeclarationIdentical,
        policy,
        RelationContext::default(),
    );

    // `any` is NOT identical to `string` for redeclaration at the solver level.
    // The checker handles lib-context `any` suppression separately (TS2403).
    assert!(!any_to_string.is_related());
    assert!(!number_to_string.is_related());
    assert!(
        string_to_string.is_related(),
        "string === string for redeclaration"
    );
}

#[test]
fn query_relation_with_overrides_can_short_circuit_assignability() {
    let interner = TypeInterner::new();
    let resolver = NoopResolver;
    let overrides = AlwaysRejectOverride;
    let policy = RelationPolicy::from_flags(RelationCacheKey::FLAG_STRICT_NULL_CHECKS);

    let result = query_relation_with_overrides(RelationQueryInputs {
        interner: &interner,
        resolver: &resolver,
        source: TypeId::NUMBER,
        target: TypeId::NUMBER,
        kind: RelationKind::Assignable,
        policy,
        context: RelationContext::default(),
        overrides: &overrides,
    });

    assert!(!result.is_related());
}
