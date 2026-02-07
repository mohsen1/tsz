//! Solver microbenchmarks (subtype, evaluate, infer).

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use tsz::interner::Atom;
use tsz::solver::{
    CompatChecker, ConditionalType, FunctionShape, ObjectShapeId, ParamInfo, PropertyInfo, TypeId,
    TypeInterner, TypeKey, TypeParamInfo, Visibility, evaluate_type, infer_generic_function,
    is_subtype_of,
};

fn build_subtype_fixtures(interner: &TypeInterner) -> (TypeId, TypeId, TypeId) {
    let name_x = interner.intern_string("x");
    let name_y = interner.intern_string("y");
    let name_z = interner.intern_string("z");

    let source = interner.object(vec![
        PropertyInfo {
            name: name_x,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        },
        PropertyInfo {
            name: name_y,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        },
    ]);

    let mismatch = interner.object(vec![PropertyInfo {
        name: name_x,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    let extra_required = interner.object(vec![
        PropertyInfo {
            name: name_x,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        },
        PropertyInfo {
            name: name_z,
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        },
    ]);

    let match_type = interner.object(vec![
        PropertyInfo {
            name: name_x,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        },
        PropertyInfo {
            name: name_y,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: true,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        },
    ]);

    let union_match = interner.union(vec![mismatch, extra_required, match_type]);
    let union_miss = interner.union(vec![mismatch, extra_required]);

    (source, union_match, union_miss)
}

fn build_conditional_type(interner: &TypeInterner) -> TypeId {
    let check = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    let conditional = ConditionalType {
        check_type: check,
        extends_type: TypeId::NUMBER,
        true_type: TypeId::STRING,
        false_type: TypeId::BOOLEAN,
        is_distributive: true,
    };
    interner.conditional(conditional)
}

fn build_infer_fixture(interner: &TypeInterner) -> (FunctionShape, [TypeId; 1]) {
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let u_param = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: Some(TypeId::STRING),
        is_const: false,
    };
    let t_type = interner.intern(TypeKey::TypeParameter(t_param.clone()));
    let u_type = interner.intern(TypeKey::TypeParameter(u_param.clone()));
    let array_t = interner.array(t_type);

    let func = FunctionShape {
        type_params: vec![t_param, u_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("items")),
            type_id: array_t,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: u_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let arg = interner.array(TypeId::NUMBER);
    (func, [arg])
}

fn build_property_lookup_fixture(interner: &TypeInterner) -> (ObjectShapeId, Atom, Atom) {
    let mut props = Vec::with_capacity(64);
    for i in 0..64 {
        let name = interner.intern_string(&format!("prop{}", i));
        props.push(PropertyInfo {
            name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        });
    }

    let obj = interner.object(props);
    let shape_id = match interner.lookup(obj) {
        Some(TypeKey::Object(shape_id)) => shape_id,
        other => panic!("expected object shape, got {:?}", other),
    };

    let hit = interner.intern_string("prop32");
    let miss = interner.intern_string("missing");
    (shape_id, hit, miss)
}

fn build_normalization_fixture(interner: &TypeInterner) -> (Vec<TypeId>, Vec<TypeId>) {
    let mut members = Vec::with_capacity(64);
    for i in 0..64 {
        members.push(interner.literal_number(i as f64));
    }

    let inner_union = interner.union(members[..32].to_vec());
    let mut union_members = Vec::with_capacity(68);
    union_members.push(inner_union);
    union_members.extend_from_slice(&members[32..]);
    union_members.extend_from_slice(&members[..4]);

    let inner_intersection = interner.intersection(members[..32].to_vec());
    let mut intersection_members = Vec::with_capacity(68);
    intersection_members.push(inner_intersection);
    intersection_members.extend_from_slice(&members[32..]);
    intersection_members.extend_from_slice(&members[..4]);

    (union_members, intersection_members)
}

fn bench_subtype(c: &mut Criterion) {
    let interner = TypeInterner::new();
    let (source, union_match, union_miss) = build_subtype_fixtures(&interner);

    c.bench_function("subtype_object_union_match", |b| {
        b.iter(|| black_box(is_subtype_of(&interner, source, union_match)))
    });

    c.bench_function("subtype_object_union_miss", |b| {
        b.iter(|| black_box(is_subtype_of(&interner, source, union_miss)))
    });
}

fn bench_evaluate(c: &mut Criterion) {
    let interner = TypeInterner::new();
    let conditional = build_conditional_type(&interner);

    c.bench_function("evaluate_conditional_distributive", |b| {
        b.iter(|| black_box(evaluate_type(&interner, conditional)))
    });
}

fn bench_infer(c: &mut Criterion) {
    let interner = TypeInterner::new();
    let (func, arg_types) = build_infer_fixture(&interner);

    c.bench_function("infer_generic_default_param", |b| {
        b.iter(|| {
            let mut checker = CompatChecker::new(&interner);
            let result = infer_generic_function(&interner, &mut checker, &func, &arg_types);
            black_box(result)
        })
    });
}

fn bench_property_lookup(c: &mut Criterion) {
    let interner = TypeInterner::new();
    let (shape_id, hit, miss) = build_property_lookup_fixture(&interner);

    let _ = interner.object_property_index(shape_id, hit);
    let _ = interner.object_property_index(shape_id, miss);

    c.bench_function("property_lookup_cached_hit", |b| {
        b.iter(|| black_box(interner.object_property_index(shape_id, hit)))
    });

    c.bench_function("property_lookup_cached_miss", |b| {
        b.iter(|| black_box(interner.object_property_index(shape_id, miss)))
    });
}

fn bench_normalization(c: &mut Criterion) {
    let interner = TypeInterner::new();
    let (union_members, intersection_members) = build_normalization_fixture(&interner);

    c.bench_function("union_normalize_nested", |b| {
        b.iter(|| {
            let members = union_members.clone();
            black_box(interner.union(members))
        })
    });

    c.bench_function("intersection_normalize_nested", |b| {
        b.iter(|| {
            let members = intersection_members.clone();
            black_box(interner.intersection(members))
        })
    });
}

criterion_group!(
    solver_benches,
    bench_subtype,
    bench_evaluate,
    bench_infer,
    bench_property_lookup,
    bench_normalization
);
criterion_main!(solver_benches);
