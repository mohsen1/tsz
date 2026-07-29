//! Unit tests for `resolve_call`'s `Application` arm cross-file alias-of-callable
//! recovery (refs #13947).

use super::call_evaluator::{
    AssignabilityChecker, CallEvaluator, CallResult, contextual_signature_test_probe,
};
use crate::TypeInterner;
use crate::def::DefId;
use crate::types::{
    CallSignature, CallableShape, ConstructSignatureOrigin, FunctionShape, ParamInfo, TupleElement,
    TypeId, TypeParamInfo, TypePredicate, TypePredicateTarget,
};

/// Mock checker modelling the resolver-less cross-file case: `evaluate_type`
/// cannot reduce the application (the trait default returns it unchanged), but
/// `expand_type_alias_application` recovers the open body for the tracked
/// application `TypeId` — exactly what the real checker's DefId-keyed expansion
/// does for an imported `type Create<R> = Sig<R>` whose `Sig` base carries no
/// `SymbolId` in the calling file's context.
struct AliasExpandChecker {
    app: TypeId,
    expanded: Option<TypeId>,
}

impl AssignabilityChecker for AliasExpandChecker {
    fn is_assignable_to(&mut self, _source: TypeId, _target: TypeId) -> bool {
        true
    }

    fn expand_type_alias_application(&mut self, type_id: TypeId) -> Option<TypeId> {
        if type_id == self.app {
            self.expanded
        } else {
            None
        }
    }
}

struct IdentityChecker;

impl AssignabilityChecker for IdentityChecker {
    fn is_assignable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        source == target
    }
}

fn construct_signature(
    param_type: TypeId,
    return_type: TypeId,
    has_literal_types: bool,
) -> CallSignature {
    let mut signature = CallSignature::new(
        vec![ParamInfo {
            name: None,
            type_id: param_type,
            optional: false,
            rest: false,
        }],
        return_type,
    );
    signature.has_literal_types = has_literal_types;
    signature
}

fn construct_type(interner: &TypeInterner, signature: CallSignature) -> TypeId {
    interner.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: vec![signature],
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    })
}

#[test]
fn construct_union_first_pass_keeps_unmarked_alias_anchor_provenance() {
    let interner = TypeInterner::new();
    let semantic_literal = interner.literal_string("pick");
    let alias_origin = Some(ConstructSignatureOrigin {
        owner: Some(DefId(11)),
        declaration_file: interner.intern_string("alias.ts"),
        declaration_pos: 0,
        declaration_end: 10,
    });
    let mut alias_signature = construct_signature(semantic_literal, TypeId::NUMBER, false);
    alias_signature.construct_origin = alias_origin;
    let alias_member = vec![alias_signature];
    let mut direct_literal_signature = construct_signature(semantic_literal, TypeId::STRING, true);
    direct_literal_signature.construct_origin = Some(ConstructSignatureOrigin {
        owner: Some(DefId(12)),
        declaration_file: interner.intern_string("literal.ts"),
        declaration_pos: 0,
        declaration_end: 10,
    });
    let direct_literal_member = vec![direct_literal_signature];
    let lists = [alias_member.as_slice(), direct_literal_member.as_slice()];

    let mut checker = IdentityChecker;
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);
    let combined = evaluator.get_union_signatures(&lists);

    assert_eq!(combined.len(), 1);
    assert!(
        !combined[0].has_literal_types,
        "the first-pass union signature clones the unmarked alias anchor; a \
         matching direct-literal declaration must not repaint its provenance"
    );
    assert_eq!(combined[0].construct_origin, alias_origin);
}

#[test]
fn construct_union_fallback_ors_literal_provenance_from_each_member() {
    let interner = TypeInterner::new();
    let overloaded_member = vec![
        construct_signature(TypeId::STRING, TypeId::NUMBER, false),
        construct_signature(TypeId::NUMBER, TypeId::STRING, false),
    ];
    let direct_literal_member = vec![construct_signature(TypeId::BOOLEAN, TypeId::BOOLEAN, true)];
    let lists = [
        overloaded_member.as_slice(),
        direct_literal_member.as_slice(),
    ];

    let mut checker = IdentityChecker;
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);
    let combined = evaluator.get_union_signatures(&lists);

    assert_eq!(
        combined.len(),
        2,
        "incompatible parameters force the one-overloaded-member fallback"
    );
    assert!(
        combined.iter().all(|signature| signature.has_literal_types),
        "fallback combinations OR literal provenance from every member"
    );
}

#[test]
fn ordinary_constructor_intersection_resolves_as_one_ordered_overload_set() {
    let interner = TypeInterner::new();
    let source_file = interner.intern_string("intersection.ts");
    let mut left = construct_signature(TypeId::STRING, TypeId::NUMBER, false);
    left.construct_origin = Some(ConstructSignatureOrigin {
        owner: Some(DefId(21)),
        declaration_file: source_file,
        declaration_pos: 0,
        declaration_end: 10,
    });
    let mut right = construct_signature(TypeId::STRING, TypeId::BOOLEAN, false);
    right.construct_origin = Some(ConstructSignatureOrigin {
        owner: Some(DefId(22)),
        declaration_file: source_file,
        declaration_pos: 11,
        declaration_end: 20,
    });
    let regular_intersection = interner.intersection2(
        construct_type(&interner, left),
        construct_type(&interner, right),
    );

    let mut checker = IdentityChecker;
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);
    assert!(
        matches!(
            evaluator.resolve_new(regular_intersection, &[TypeId::STRING]),
            CallResult::Success(TypeId::NUMBER)
        ),
        "ordinary constructor intersections preserve distinct-owner candidate order"
    );

    let literal_parameter = interner.literal_string("pick");
    let mut alias_like = construct_signature(literal_parameter, TypeId::NUMBER, false);
    alias_like.construct_origin = Some(ConstructSignatureOrigin {
        owner: Some(DefId(23)),
        declaration_file: source_file,
        declaration_pos: 21,
        declaration_end: 30,
    });
    let mut specialized = construct_signature(literal_parameter, TypeId::BOOLEAN, true);
    specialized.construct_origin = Some(ConstructSignatureOrigin {
        owner: Some(DefId(24)),
        declaration_file: source_file,
        declaration_pos: 31,
        declaration_end: 40,
    });
    let specialized_intersection = interner.intersection2(
        construct_type(&interner, alias_like),
        construct_type(&interner, specialized),
    );
    assert!(
        matches!(
            evaluator.resolve_new(specialized_intersection, &[literal_parameter]),
            CallResult::Success(TypeId::BOOLEAN)
        ),
        "a later literal-specialized owner moves ahead of an earlier regular owner"
    );
}

#[test]
fn true_mixin_constructor_intersection_combines_instance_returns() {
    let interner = TypeInterner::new();
    let left_result = interner.lazy(DefId(31));
    let right_result = interner.lazy(DefId(32));
    let mixin = CallSignature::new(
        vec![ParamInfo {
            name: None,
            type_id: interner.array(TypeId::ANY),
            optional: false,
            rest: true,
        }],
        left_result,
    );
    let ordinary = construct_signature(TypeId::STRING, right_result, false);
    let intersection = interner.intersection2(
        construct_type(&interner, mixin),
        construct_type(&interner, ordinary),
    );
    let expected = interner.intersection2(left_result, right_result);

    let mut checker = IdentityChecker;
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);
    assert!(
        matches!(
            evaluator.resolve_new(intersection, &[TypeId::STRING]),
            CallResult::Success(result) if result == expected
        ),
        "only true rest-any mixin constructors fold their instance into the selected return"
    );
}

#[test]
fn constrained_and_tuple_rest_constructors_are_not_mixins() {
    let interner = TypeInterner::new();
    let outer_name = interner.intern_string("OuterArgs");
    let constrained_rest = interner.type_param(TypeParamInfo {
        constraint: Some(interner.array(TypeId::ANY)),
        ..TypeParamInfo::simple(outer_name)
    });
    let tuple_rest = interner.tuple(vec![TupleElement::fixed(TypeId::ANY)]);
    let no_infer_rest = interner.no_infer(interner.array(TypeId::ANY));
    let substitution_rest = interner.substitution(constrained_rest, interner.array(TypeId::ANY));

    for rest_type in [
        constrained_rest,
        tuple_rest,
        no_infer_rest,
        substitution_rest,
    ] {
        let first_result = interner.lazy(DefId(41));
        let second_result = interner.lazy(DefId(42));
        let rest_signature = CallSignature::new(
            vec![ParamInfo {
                name: None,
                type_id: rest_type,
                optional: false,
                rest: true,
            }],
            first_result,
        );
        let ordinary = construct_signature(TypeId::STRING, second_result, false);
        let intersection = interner.intersection2(
            construct_type(&interner, rest_signature),
            construct_type(&interner, ordinary),
        );
        let signatures = crate::type_queries::get_construct_signatures(&interner, intersection)
            .expect("both ordinary constructor constituents must remain candidates");

        assert_eq!(signatures.len(), 2);
        assert_eq!(
            signatures
                .iter()
                .map(|signature| signature.return_type)
                .collect::<Vec<_>>(),
            vec![first_result, second_result],
            "a constrained type parameter or tuple rest is not a direct any-array mixin"
        );
    }
}

#[test]
fn generic_constraint_and_default_differences_survive_intersection_dedup() {
    let interner = TypeInterner::new();
    // Keep this projection resolver-independent so the adjacent assertions
    // exercise the immutable construct-signature memo as well as identity.
    let return_type = TypeId::BOOLEAN;
    let make_generic = |name: &str, constraint: TypeId, default: TypeId| {
        let mut signature = CallSignature::new(Vec::new(), return_type);
        signature.type_params.push(TypeParamInfo {
            constraint: Some(constraint),
            default: Some(default),
            ..TypeParamInfo::simple(interner.intern_string(name))
        });
        construct_type(&interner, signature)
    };

    let string_generic = make_generic("Text", TypeId::STRING, TypeId::STRING);
    let number_constraint = make_generic("Count", TypeId::NUMBER, TypeId::STRING);
    let number_default = make_generic("Amount", TypeId::STRING, TypeId::NUMBER);
    let constraint_intersection = interner.intersection2(string_generic, number_constraint);
    let default_intersection = interner.intersection2(string_generic, number_default);

    let cache_entries_before = interner
        .type_predicate_cache_statistics()
        .construct_signatures_cache_entries;
    let constraint_signatures =
        crate::type_queries::get_construct_signatures(&interner, constraint_intersection)
            .expect("construct signatures");
    assert_eq!(
        constraint_signatures.len(),
        2,
        "different generic constraints are distinct constructor candidates"
    );
    let cache_entries_after_first = interner
        .type_predicate_cache_statistics()
        .construct_signatures_cache_entries;
    assert_eq!(cache_entries_after_first, cache_entries_before + 1);
    assert_eq!(
        crate::type_queries::get_construct_signatures(&interner, constraint_intersection)
            .expect("cached construct signatures"),
        constraint_signatures
    );
    assert_eq!(
        interner
            .type_predicate_cache_statistics()
            .construct_signatures_cache_entries,
        cache_entries_after_first,
        "repeated constructor-intersection queries reuse the immutable projection cache"
    );
    assert_eq!(
        crate::type_queries::get_construct_signatures(&interner, default_intersection)
            .expect("construct signatures")
            .len(),
        2,
        "different generic defaults are distinct constructor candidates"
    );

    let renamed_equivalent = make_generic("Renamed", TypeId::STRING, TypeId::STRING);
    let renamed_intersection = interner.intersection2(string_generic, renamed_equivalent);
    assert_eq!(
        crate::type_queries::get_construct_signatures(&interner, renamed_intersection)
            .expect("construct signatures")
            .len(),
        1,
        "alpha-equivalent renamed generic binders deduplicate"
    );

    let mut implicit_unknown_default = CallSignature::new(Vec::new(), return_type);
    implicit_unknown_default
        .type_params
        .push(TypeParamInfo::simple(interner.intern_string("Implicit")));
    let mut explicit_unknown_default = CallSignature::new(Vec::new(), return_type);
    explicit_unknown_default.type_params.push(TypeParamInfo {
        default: Some(TypeId::UNKNOWN),
        ..TypeParamInfo::simple(interner.intern_string("Explicit"))
    });
    let unknown_default_intersection = interner.intersection2(
        construct_type(&interner, implicit_unknown_default),
        construct_type(&interner, explicit_unknown_default),
    );
    assert_eq!(
        crate::type_queries::get_construct_signatures(&interner, unknown_default_intersection)
            .expect("construct signatures")
            .len(),
        1,
        "a missing constraint/default compares as explicit unknown"
    );
}

#[test]
fn constructor_intersection_identity_matches_predicate_and_this_rules() {
    let interner = TypeInterner::new();
    let predicate = TypePredicate {
        asserts: true,
        target: TypePredicateTarget::This,
        type_id: Some(TypeId::STRING),
        parameter_index: None,
    };
    let mut predicate_left = CallSignature::new(Vec::new(), TypeId::NUMBER);
    predicate_left.type_predicate = Some(predicate);
    let mut predicate_right = CallSignature::new(Vec::new(), TypeId::BOOLEAN);
    predicate_right.type_predicate = Some(predicate);
    let predicate_intersection = interner.intersection2(
        construct_type(&interner, predicate_left),
        construct_type(&interner, predicate_right),
    );
    assert_eq!(
        crate::type_queries::get_construct_signatures(&interner, predicate_intersection)
            .expect("construct signatures")
            .len(),
        1,
        "matching predicates, rather than raw returns, determine signature identity"
    );

    let no_this = CallSignature::new(Vec::new(), TypeId::STRING);
    let mut explicit_this = no_this.clone();
    explicit_this.this_type = Some(TypeId::NUMBER);
    let one_sided_this = interner.intersection2(
        construct_type(&interner, no_this),
        construct_type(&interner, explicit_this),
    );
    assert_eq!(
        crate::type_queries::get_construct_signatures(&interner, one_sided_this)
            .expect("construct signatures")
            .len(),
        1,
        "tsc compares this types only when both signatures declare one"
    );

    let mut string_this = CallSignature::new(Vec::new(), TypeId::STRING);
    string_this.this_type = Some(TypeId::STRING);
    let mut number_this = string_this.clone();
    number_this.this_type = Some(TypeId::NUMBER);
    let conflicting_this = interner.intersection2(
        construct_type(&interner, string_this),
        construct_type(&interner, number_this),
    );
    assert_eq!(
        crate::type_queries::get_construct_signatures(&interner, conflicting_this)
            .expect("construct signatures")
            .len(),
        2,
        "two explicit, distinct this types keep separate candidates"
    );
}

/// A generic application `Base<string>` whose base is a bare `Lazy(DefId)` that
/// the (default) evaluator cannot resolve — the resolver-less call-target shape.
fn resolver_less_application(interner: &TypeInterner) -> TypeId {
    let base = interner.lazy(DefId(1));
    interner.application(base, vec![TypeId::STRING])
}

#[test]
fn application_call_target_falls_back_to_alias_expansion() {
    let interner = TypeInterner::new();
    let app = resolver_less_application(&interner);
    // The alias body is a callable `() => number`.
    let callable = interner.function(FunctionShape::new(vec![], TypeId::NUMBER));

    let mut checker = AliasExpandChecker {
        app,
        expanded: Some(callable),
    };
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);
    let result = evaluator.resolve_call(app, &[]);

    assert!(
        matches!(result, CallResult::Success(ret) if ret == TypeId::NUMBER),
        "a resolver-less alias-of-callable application used as a call target must \
         resolve through expand_type_alias_application rather than collapsing to \
         NotCallable, got {result:?}"
    );
}

#[test]
fn application_call_target_without_expansion_stays_not_callable() {
    let interner = TypeInterner::new();
    let app = resolver_less_application(&interner);

    // The checker cannot expand the application (genuinely opaque / non-callable):
    // the fallback must not invent callability.
    let mut checker = AliasExpandChecker {
        app,
        expanded: None,
    };
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);
    let result = evaluator.resolve_call(app, &[]);

    assert!(
        matches!(result, CallResult::NotCallable { .. }),
        "an application the checker cannot expand stays NotCallable, got {result:?}"
    );
}

#[test]
fn application_call_target_prefers_evaluation_when_available() {
    let interner = TypeInterner::new();
    let app = resolver_less_application(&interner);
    // If expansion yields the same type (no progress), the call stays NotCallable
    // rather than recursing forever.
    let mut checker = AliasExpandChecker {
        app,
        expanded: Some(app),
    };
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);
    let result = evaluator.resolve_call(app, &[]);

    assert!(
        matches!(result, CallResult::NotCallable { .. }),
        "a non-progressing expansion must not loop or fabricate callability, got {result:?}"
    );
}

#[test]
fn contextual_signature_memoizes_shared_union_intersection_constituents_per_walk() {
    let interner = TypeInterner::new();
    let param = interner.intern_string("value");
    let callable = |return_type| {
        interner.function(FunctionShape::new(
            vec![ParamInfo::required(param, TypeId::STRING)],
            return_type,
        ))
    };
    let shared = callable(TypeId::STRING);
    let left_only = callable(TypeId::NUMBER);
    let right_only = callable(TypeId::BOOLEAN);

    let left = interner.intersect_types_raw2(shared, left_only);
    let right = interner.intersect_types_raw2(shared, right_only);
    let contextual = interner.union_literal_reduce(vec![left, right]);

    let (signature, visits) = contextual_signature_test_probe::with_recorded_visits(|| {
        CallEvaluator::<AliasExpandChecker>::get_contextual_signature_cached(&interner, contextual)
    });

    assert!(
        signature.is_some(),
        "shared callable constituents in union/intersection contextual types \
         should still produce a contextual signature"
    );
    assert_eq!(
        visits.iter().filter(|&&visited| visited == shared).count(),
        1,
        "a shared callable constituent should be walked once per contextual \
         signature extraction, not once per DAG path"
    );
}

fn contextual_signature_in_flight_cycle() -> (TypeInterner, TypeId) {
    for raw_app in TypeId::FIRST_USER..TypeId::FIRST_USER + 512 {
        let interner = TypeInterner::new();
        let future_app = TypeId(raw_app);

        let wrapper = interner.no_infer(future_app);
        let base = interner.lazy(DefId(wrapper.0));
        let app = interner.application(base, vec![TypeId::STRING]);

        if app == future_app {
            assert_eq!(
                crate::evaluation::evaluate::evaluate_type(&interner, wrapper),
                app
            );
            return (interner, app);
        }
    }

    panic!("could not construct a stable contextual-signature cycle");
}

#[test]
fn contextual_signature_cycle_truncation_none_is_not_memoized() {
    let (interner, cyclic_app) = contextual_signature_in_flight_cycle();
    let param = interner.intern_string("value");
    let callable = |return_type| {
        interner.function(FunctionShape::new(
            vec![ParamInfo::required(param, TypeId::STRING)],
            return_type,
        ))
    };

    let left = interner.intersect_types_raw2(cyclic_app, callable(TypeId::NUMBER));
    let right = interner.intersect_types_raw2(cyclic_app, callable(TypeId::BOOLEAN));
    let contextual = interner.union_preserve_members(vec![left, right]);

    let (signature, visits) = contextual_signature_test_probe::with_recorded_visits(|| {
        CallEvaluator::<AliasExpandChecker>::get_contextual_signature_cached(&interner, contextual)
    });

    assert!(
        signature.is_some(),
        "a cycle-truncated application must not hide callable constituents"
    );
    assert_eq!(
        visits
            .iter()
            .filter(|&&visited| visited == cyclic_app)
            .count(),
        2,
        "cycle-truncated None for an in-flight contextual application must not \
         be memoized"
    );
}
