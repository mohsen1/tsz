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

    // In tsc, `var x: any; var x: string;` DOES produce TS2403.
    // `any` is only compatible with `any` for redeclaration.
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

#[test]
fn assignability_failure_analysis_helper_reports_reason() {
    let interner = TypeInterner::new();
    let resolver = NoopResolver;

    let analysis = analyze_assignability_failure_with_resolver(
        &interner,
        &resolver,
        TypeId::NUMBER,
        TypeId::STRING,
        |_| {},
    );

    assert!(!analysis.weak_union_violation);
    assert!(
        analysis.failure_reason.is_some(),
        "expected failure reason for number -> string assignability mismatch"
    );
}

#[test]
fn redeclaration_identity_evaluates_keyof_to_literal_union() {
    // Regression test: `var v: "a" | "b"; var v: keyof { a: number, b: string }`
    // should NOT produce TS2403 because `keyof { a: number, b: string }` evaluates
    // to `"a" | "b"`. The normalization step in the compat checker must evaluate
    // KeyOf types before comparing for redeclaration identity.
    let interner = TypeInterner::new();
    let policy = RelationPolicy::from_flags(RelationCacheKey::FLAG_STRICT_NULL_CHECKS);

    let a_atom = interner.intern_string("a");
    let b_atom = interner.intern_string("b");

    // Build the object type { a: number, b: string }
    let obj = interner.object(vec![
        PropertyInfo::new(a_atom, TypeId::NUMBER),
        PropertyInfo::new(b_atom, TypeId::STRING),
    ]);

    // Build keyof { a: number, b: string } — should evaluate to "a" | "b"
    let keyof_obj = interner.keyof(obj);

    // Build "a" | "b" as a union of string literals
    let lit_a = interner.literal_string_atom(a_atom);
    let lit_b = interner.literal_string_atom(b_atom);
    let union_ab = interner.union(vec![lit_a, lit_b]);

    // These must be identical for redeclaration purposes
    let result = query_relation(
        &interner,
        keyof_obj,
        union_ab,
        RelationKind::RedeclarationIdentical,
        policy,
        RelationContext::default(),
    );
    assert!(
        result.is_related(),
        "keyof {{a: number, b: string}} should be identical to \"a\" | \"b\" for redeclaration"
    );

    // And in the reverse direction
    let result_rev = query_relation(
        &interner,
        union_ab,
        keyof_obj,
        RelationKind::RedeclarationIdentical,
        policy,
        RelationContext::default(),
    );
    assert!(
        result_rev.is_related(),
        "\"a\" | \"b\" should be identical to keyof {{a: number, b: string}} for redeclaration"
    );
}
