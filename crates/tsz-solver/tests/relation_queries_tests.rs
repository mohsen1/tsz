use super::*;
use crate::construction::TypeInterner;
use crate::{FunctionShape, ParamInfo, PropertyInfo};

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
    let strict_policy = RelationPolicy::from_relation_flags(RelationFlags::STRICT_NULL_CHECKS);
    let non_strict_policy = RelationPolicy::unflagged_compatibility();

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

    let policy = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_NULL_CHECKS | RelationFlags::STRICT_FUNCTION_TYPES,
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
    let policy = RelationPolicy::from_relation_flags(RelationFlags::STRICT_NULL_CHECKS);

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
    let policy = RelationPolicy::from_relation_flags(RelationFlags::STRICT_NULL_CHECKS);

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
    let policy = RelationPolicy::from_relation_flags(RelationFlags::STRICT_NULL_CHECKS);

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
fn single_pass_analysis_matches_decision_and_explains_structural_failure() {
    // `animal` (only `name`) is not assignable to `dog` (`name` + `breed`).
    // The single-pass helper must report `related == false` AND surface a
    // structured reason from the SAME checker pass, with a decision identical
    // to the canonical boolean query.
    let interner = TypeInterner::new();
    let resolver = NoopResolver;
    let overrides = NoopOverrideProvider;
    let (animal, dog) = make_animal_dog(&interner);
    let policy = RelationPolicy::from_relation_flags(RelationFlags::STRICT_NULL_CHECKS);

    let inputs = || RelationQueryInputs {
        interner: &interner,
        resolver: &resolver,
        source: animal,
        target: dog,
        kind: RelationKind::Assignable,
        policy,
        context: RelationContext::default(),
        overrides: &overrides,
    };

    let outcome = query_assignability_with_failure_analysis(inputs());
    let decision = query_relation_with_overrides(inputs());

    assert_eq!(
        outcome.result.related, decision.related,
        "single-pass decision must match the canonical assignability query"
    );
    assert!(!outcome.result.related);

    let analysis = outcome
        .analysis
        .expect("a failed relation must carry structured analysis");
    assert!(
        analysis.failure_reason.is_some(),
        "missing-property failure must surface a reason from the same pass"
    );
}

#[test]
fn single_pass_analysis_is_absent_when_relation_holds() {
    // `dog` (name + breed) IS assignable to `animal` (name): the helper must
    // report `related == true` and carry no failure analysis.
    let interner = TypeInterner::new();
    let resolver = NoopResolver;
    let overrides = NoopOverrideProvider;
    let (animal, dog) = make_animal_dog(&interner);
    let policy = RelationPolicy::from_relation_flags(RelationFlags::STRICT_NULL_CHECKS);

    let outcome = query_assignability_with_failure_analysis(RelationQueryInputs {
        interner: &interner,
        resolver: &resolver,
        source: dog,
        target: animal,
        kind: RelationKind::Assignable,
        policy,
        context: RelationContext::default(),
        overrides: &overrides,
    });

    assert!(outcome.result.related);
    assert!(
        outcome.analysis.is_none(),
        "a holding relation must not carry failure analysis"
    );
}

#[test]
fn single_pass_decision_honors_overrides_and_records_analysis() {
    // An override that forces non-assignability for a structurally-assignable
    // pair (`number -> number`) drives BOTH the decision and the analysis
    // through the same configured checker. Previously the decision observed the
    // override while a separate reason pass did not, leaving `related == false`
    // with no analysis record. The single-pass helper keeps them coupled.
    let interner = TypeInterner::new();
    let resolver = NoopResolver;
    let overrides = AlwaysRejectOverride;
    let policy = RelationPolicy::from_relation_flags(RelationFlags::STRICT_NULL_CHECKS);

    let inputs = || RelationQueryInputs {
        interner: &interner,
        resolver: &resolver,
        source: TypeId::NUMBER,
        target: TypeId::NUMBER,
        kind: RelationKind::Assignable,
        policy,
        context: RelationContext::default(),
        overrides: &overrides,
    };

    let outcome = query_assignability_with_failure_analysis(inputs());
    let decision = query_relation_with_overrides(inputs());

    assert_eq!(
        outcome.result.related, decision.related,
        "single-pass decision must match the canonical override-aware query"
    );
    assert!(!outcome.result.related, "override forces non-assignability");
    assert!(
        outcome.analysis.is_some(),
        "a non-related outcome must always carry an analysis record"
    );
}

#[test]
fn redeclaration_identity_evaluates_keyof_to_literal_union() {
    // Regression test: `var v: "a" | "b"; var v: keyof { a: number, b: string }`
    // should NOT produce TS2403 because `keyof { a: number, b: string }` evaluates
    // to `"a" | "b"`. The normalization step in the compat checker must evaluate
    // KeyOf types before comparing for redeclaration identity.
    let interner = TypeInterner::new();
    let policy = RelationPolicy::from_relation_flags(RelationFlags::STRICT_NULL_CHECKS);

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

#[test]
fn redeclaration_identity_union_vs_nonunion_not_identical() {
    // C | D is NOT identical to C even when D is a subtype of C.
    // This matches tsc's isTypeIdenticalTo semantics for TS2403.
    let interner = TypeInterner::new();
    let policy = RelationPolicy::unflagged_compatibility();

    let name = interner.intern_string("name");
    let foo = interner.intern_string("foo");

    // C = { name: string }
    let c = interner.object(vec![PropertyInfo::new(name, TypeId::STRING)]);
    // D = { name: string, foo: string } (D <: C)
    let d = interner.object(vec![
        PropertyInfo::new(name, TypeId::STRING),
        PropertyInfo::new(foo, TypeId::STRING),
    ]);

    // C | D (preserved as union via literal-only reduction)
    let c_or_d = interner.union_literal_reduce(vec![c, d]);

    // C | D should NOT be identical to C for redeclaration
    let result = query_relation(
        &interner,
        c,
        c_or_d,
        RelationKind::RedeclarationIdentical,
        policy,
        RelationContext::default(),
    );
    assert!(
        !result.is_related(),
        "C should NOT be identical to C | D for redeclaration (union vs non-union mismatch)"
    );

    // Also test the reverse direction
    let result_rev = query_relation(
        &interner,
        c_or_d,
        c,
        RelationKind::RedeclarationIdentical,
        policy,
        RelationContext::default(),
    );
    assert!(
        !result_rev.is_related(),
        "C | D should NOT be identical to C for redeclaration (union vs non-union mismatch)"
    );
}

#[test]
fn redeclaration_identity_same_union_is_identical() {
    // C | D should be identical to C | D for redeclaration.
    let interner = TypeInterner::new();
    let policy = RelationPolicy::unflagged_compatibility();

    let name = interner.intern_string("name");
    let foo = interner.intern_string("foo");

    let c = interner.object(vec![PropertyInfo::new(name, TypeId::STRING)]);
    let d = interner.object(vec![
        PropertyInfo::new(name, TypeId::STRING),
        PropertyInfo::new(foo, TypeId::STRING),
    ]);

    let c_or_d_1 = interner.union_literal_reduce(vec![c, d]);
    let c_or_d_2 = interner.union_literal_reduce(vec![c, d]);

    // Same union should be identical (physically same TypeId due to interning)
    let result = query_relation(
        &interner,
        c_or_d_1,
        c_or_d_2,
        RelationKind::RedeclarationIdentical,
        policy,
        RelationContext::default(),
    );
    assert!(
        result.is_related(),
        "C | D should be identical to C | D for redeclaration"
    );
}

/// Compile-time tripwire for the relation cache-key contract (#8207).
///
/// `RelationPolicy` is the single canonical cache-key input for relation
/// queries: every behavior-affecting field must be projected into
/// `RelationCacheConfig` by `RelationPolicy::cache_config`.
///
/// The destructuring below is intentionally exhaustive (no `..` rest
/// pattern), so adding a field to `RelationPolicy` fails compilation here.
/// When that happens, the new field must:
///
/// (a) be mapped into `RelationCacheConfig` / the relation cache key, or be
///     explicitly documented as cache-neutral (diagnostic-only) on the
///     struct doc comment; and
/// (b) get a cache-partition test (see `relation_cache_config_tests` and the
///     per-field assertions below) before this pattern is extended with the
///     new field.
///
/// This test lives next to `RelationPolicy` (instead of in
/// `relation_cache_config_tests`) because the exhaustive pattern needs
/// visibility of the private `flags` field.
#[test]
fn relation_policy_fields_are_exhaustively_partitioned_in_cache_keys() {
    use crate::relation_cache_config_tests::{
        assert_assignability_partitions, assert_subtype_partitions,
    };

    let policy = RelationPolicy::default();
    let RelationPolicy {
        flags,
        any_propagation_mode,
        assume_related_on_cycle,
        assume_related_on_depth,
        skip_weak_type_checks,
        erase_generics,
    } = policy;

    // `flags`: toggle a representative typed bit relative to the current
    // default; the full per-bit matrix lives in
    // `relation_cache_config_tests::each_relation_flag_bit_produces_a_distinct_key`.
    assert_subtype_partitions(
        "flags",
        RelationPolicy::from_relation_flags(flags),
        RelationPolicy::from_relation_flags(
            flags.symmetric_difference(RelationFlags::STRICT_NULL_CHECKS),
        ),
    );
    // Exhaustive over `AnyPropagationMode` as well: a new variant must pick a
    // distinct `CachedAnyMode` projection in `RelationPolicy::cache_config`.
    let other_any_mode = match any_propagation_mode {
        AnyPropagationMode::All => AnyPropagationMode::TopLevelOnly,
        AnyPropagationMode::TopLevelOnly
        | AnyPropagationMode::AnySourceNotRelated
        | AnyPropagationMode::IdenticalOnly => AnyPropagationMode::All,
    };
    assert_subtype_partitions(
        "any_propagation_mode",
        policy.with_any_propagation_mode(other_any_mode),
        policy,
    );
    assert_subtype_partitions(
        "assume_related_on_cycle",
        policy.with_assume_related_on_cycle(!assume_related_on_cycle),
        policy,
    );
    assert_subtype_partitions(
        "assume_related_on_depth",
        policy.with_assume_related_on_depth(!assume_related_on_depth),
        policy,
    );
    assert_assignability_partitions(
        "skip_weak_type_checks",
        policy.with_skip_weak_type_checks(!skip_weak_type_checks),
        policy,
    );
    assert_subtype_partitions(
        "erase_generics",
        policy.with_erase_generics(!erase_generics),
        policy,
    );
}
