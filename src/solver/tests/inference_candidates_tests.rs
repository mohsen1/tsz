use super::*;

#[test]
fn test_infer_candidates_disjoint_primitives_union() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name);
    ctx.add_candidate(var, TypeId::NUMBER, InferencePriority::Argument);
    ctx.add_candidate(var, TypeId::STRING, InferencePriority::Argument);

    let result = ctx.resolve_with_constraints(var).unwrap();
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    assert_eq!(result, expected);
}

#[test]
fn test_infer_candidates_literal_widening_number() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name);
    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    ctx.add_candidate(var, one, InferencePriority::Argument);
    ctx.add_candidate(var, two, InferencePriority::Argument);

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
    }]);
    let dog = interner.object(vec![
        PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: breed,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let var = ctx.fresh_type_param(t_name);
    ctx.add_candidate(var, dog, InferencePriority::Argument);
    ctx.add_candidate(var, animal, InferencePriority::Argument);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, animal);
}
