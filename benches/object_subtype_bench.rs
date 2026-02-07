//! Object subtype microbenchmarks.
//!
//! Focus: property lookup cost in check_object_subtype.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use tsz::solver::types::{PropertyInfo, TypeId, Visibility};
use tsz::solver::{SubtypeChecker, TypeInterner};

fn make_object(interner: &TypeInterner, count: usize) -> TypeId {
    let mut props = Vec::with_capacity(count);
    for i in 0..count {
        let name = interner.intern_string(&format!("p{}", i));
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

    interner.object(props)
}

fn bench_object_subtype(c: &mut Criterion) {
    let interner = TypeInterner::new();

    // Source has more properties than target to simulate width subtyping.
    let source_id = make_object(&interner, 500);
    let target_id = make_object(&interner, 250);

    c.bench_function("object_is_subtype_500_vs_250", |b| {
        b.iter(|| {
            let mut checker = SubtypeChecker::new(&interner);
            let result = checker.is_subtype_of(source_id, target_id);
            black_box(result)
        })
    });
}

criterion_group!(benches, bench_object_subtype);
criterion_main!(benches);
