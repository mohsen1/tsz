//! Union reduction microbenchmarks.
//! Tests performance of reduce_union_subtypes for large unions of tuples/arrays.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use wasm::solver::{TupleElement, TypeId, TypeInterner};

/// Create N distinct tuple types like [Lit0, Lit1], [Lit2, Lit3], etc.
/// This simulates enumLiteralsSubtypeReduction.ts which has 512 return types.
fn create_distinct_tuples(interner: &TypeInterner, count: usize) -> Vec<TypeId> {
    let mut tuples = Vec::with_capacity(count);
    for i in 0..count {
        // Create two distinct number literals for each tuple
        let lit1 = interner.literal_number((i * 2) as f64);
        let lit2 = interner.literal_number((i * 2 + 1) as f64);

        // Create tuple [lit1, lit2]
        let tuple = interner.tuple(vec![
            TupleElement {
                type_id: lit1,
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: lit2,
                name: None,
                optional: false,
                rest: false,
            },
        ]);
        tuples.push(tuple);
    }
    tuples
}

/// Create N distinct array types like Array<Lit0 | Lit1>, Array<Lit2 | Lit3>, etc.
fn create_distinct_arrays(interner: &TypeInterner, count: usize) -> Vec<TypeId> {
    let mut arrays = Vec::with_capacity(count);
    for i in 0..count {
        let lit1 = interner.literal_number((i * 2) as f64);
        let lit2 = interner.literal_number((i * 2 + 1) as f64);
        let element_type = interner.union2(lit1, lit2);
        let array = interner.array(element_type);
        arrays.push(array);
    }
    arrays
}

/// Benchmark creating a union of 512 distinct tuples (simulates enumLiteralsSubtypeReduction.ts)
fn bench_union_512_tuples(c: &mut Criterion) {
    let interner = TypeInterner::new();
    let tuples = create_distinct_tuples(&interner, 512);

    c.bench_function("union_512_tuples", |b| {
        b.iter(|| {
            // Clone types to measure fresh union creation each time
            black_box(interner.union(tuples.clone()))
        })
    });
}

/// Benchmark creating a union of 512 distinct arrays
fn bench_union_512_arrays(c: &mut Criterion) {
    let interner = TypeInterner::new();
    let arrays = create_distinct_arrays(&interner, 512);

    c.bench_function("union_512_arrays", |b| {
        b.iter(|| black_box(interner.union(arrays.clone())))
    });
}

/// Benchmark incremental union building (the anti-pattern)
/// This simulates calling union2 in a loop 512 times
fn bench_incremental_union_512(c: &mut Criterion) {
    let interner = TypeInterner::new();
    let tuples = create_distinct_tuples(&interner, 512);

    c.bench_function("incremental_union_512", |b| {
        b.iter(|| {
            let mut result = TypeId::NEVER;
            for &tuple in &tuples {
                result = interner.union2(result, tuple);
            }
            black_box(result)
        })
    });
}

/// Benchmark union of 100 tuples (smaller scale)
fn bench_union_100_tuples(c: &mut Criterion) {
    let interner = TypeInterner::new();
    let tuples = create_distinct_tuples(&interner, 100);

    c.bench_function("union_100_tuples", |b| {
        b.iter(|| black_box(interner.union(tuples.clone())))
    });
}

/// Benchmark union of identical types (should be fast due to dedup)
fn bench_union_512_identical(c: &mut Criterion) {
    let interner = TypeInterner::new();
    let tuple = create_distinct_tuples(&interner, 1)[0];
    let tuples: Vec<TypeId> = (0..512).map(|_| tuple).collect();

    c.bench_function("union_512_identical", |b| {
        b.iter(|| black_box(interner.union(tuples.clone())))
    });
}

criterion_group!(
    benches,
    bench_union_512_tuples,
    bench_union_512_arrays,
    bench_incremental_union_512,
    bench_union_100_tuples,
    bench_union_512_identical,
);
criterion_main!(benches);
