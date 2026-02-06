// NOTE: These tests use the old candidate-based inference API which has been
// replaced with unification-based inference. They need to be rewritten to test
// the new system. Skipping for now.

use super::*;

#[test]
#[ignore = "Tests use deprecated add_candidate / resolve_with_constraints API"]
fn test_infer_candidates_disjoint_primitives_union() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name, false);
    ctx.add_candidate(var, TypeId::NUMBER, InferencePriority::NakedTypeVariable);
    ctx.add_candidate(var, TypeId::STRING, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var).unwrap();
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    assert_eq!(result, expected);
}

#[test]
fn test_infer_candidates_literal_widening_number() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name, false);
    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    ctx.add_candidate(var, one, InferencePriority::NakedTypeVariable);
    ctx.add_candidate(var, two, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_infer_candidates_common_supertype() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let name = interner.intern_string("name");
    let breed = interner.intern_string("breed");
    let animal = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);
    let dog = interner.object(vec![
        PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        },
        PropertyInfo {
            name: breed,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        },
    ]);

    let var = ctx.fresh_type_param(t_name, false);
    ctx.add_candidate(var, dog, InferencePriority::NakedTypeVariable);
    ctx.add_candidate(var, animal, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, animal);
}

#[test]
fn test_infer_candidates_priority_argument_over_return() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name, false);
    ctx.add_candidate(var, TypeId::NUMBER, InferencePriority::ReturnType);
    ctx.add_candidate(var, TypeId::STRING, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_infer_candidates_priority_literal_over_argument() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name, false);
    let arg_lit = interner.literal_string("arg");
    let lit = interner.literal_string("lit");
    ctx.add_candidate(var, arg_lit, InferencePriority::NakedTypeVariable);
    ctx.add_candidate(var, lit, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lit);
}

#[test]
fn test_infer_candidates_literal_priority_single() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name, false);
    let lit = interner.literal_number(3.0);
    ctx.add_candidate(var, lit, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lit);
}

#[test]
fn test_infer_candidates_widening_string_literals() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name, false);
    let a = interner.literal_string("a");
    let b = interner.literal_string("b");
    ctx.add_candidate(var, a, InferencePriority::NakedTypeVariable);
    ctx.add_candidate(var, b, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_infer_candidates_widening_boolean_literals() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name, false);
    let t = interner.literal_boolean(true);
    let f = interner.literal_boolean(false);
    ctx.add_candidate(var, t, InferencePriority::NakedTypeVariable);
    ctx.add_candidate(var, f, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, TypeId::BOOLEAN);
}

#[test]
fn test_infer_candidates_upper_bound_filters_any() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name, false);
    ctx.add_upper_bound(var, TypeId::STRING);
    ctx.add_candidate(var, TypeId::ANY, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_infer_candidates_upper_bound_keeps_literal() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name, false);
    let lit = interner.literal_string("hello");
    ctx.add_upper_bound(var, TypeId::STRING);
    ctx.add_candidate(var, lit, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lit);
}

#[test]
fn test_infer_candidates_upper_bound_intersection() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name, false);
    let upper = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_upper_bound(var, TypeId::STRING);
    ctx.add_upper_bound(var, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, upper);
}

#[test]
fn test_infer_candidates_bounds_violation() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name, false);
    ctx.add_upper_bound(var, TypeId::STRING);
    ctx.add_candidate(var, TypeId::NUMBER, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation { .. })
    ));
}

#[test]
fn test_infer_candidates_filters_by_max_priority() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name, false);
    ctx.add_candidate(var, TypeId::STRING, InferencePriority::ReturnType);
    ctx.add_candidate(var, TypeId::NUMBER, InferencePriority::ReturnType);
    ctx.add_candidate(var, TypeId::NUMBER, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_infer_candidates_return_type_union() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name, false);
    ctx.add_candidate(var, TypeId::STRING, InferencePriority::ReturnType);
    ctx.add_candidate(var, TypeId::NUMBER, InferencePriority::ReturnType);

    let result = ctx.resolve_with_constraints(var).unwrap();
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}
