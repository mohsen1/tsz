//! Union reduction microbenchmarks.
//! Tests performance of `reduce_union_subtypes` for large unions of tuples/arrays.
//!
//! The `union_mixed_kind_*` group targets the large-ts-repo / ts-toolbelt
//! `reduce_union_subtypes` quadratic sweep over a wide concrete union whose
//! members span several disjoint structural kinds (objects with unique
//! discriminants, distinct numeric/string literals, a widened primitive). That
//! shape misses discriminant partitioning (no property covers half the members)
//! and lands on the raw O(N²) pairwise `is_subtype_shallow` loop, where most
//! pairs are cross-kind (object-vs-literal, primitive-vs-object) and provably
//! cannot relate. The structural-bucket skip should drop
//! `union_subtype_reduction_shallow_checks` toward the count of same-kind pairs
//! while leaving the union result byte-identical.
//!
//! The `union_inert_keyof_*` group targets the complementary inert-deferred
//! lift: a wide union of distinct unevaluated `keyof <unique literal>` members.
//! The shallow engine can only relate such members by identity, so the whole
//! band is lifted out of the sweep in O(N) and contributes **zero** pairwise
//! `is_subtype_shallow` calls (vs N·(N−1) before the lift was widened past
//! `Conditional`/`IndexAccess`).

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use tsz_solver::construction::TypeInterner;
use tsz_solver::{PropertyInfo, TupleElement, TypeId};

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

/// Build a wide mixed-kind union member set of size ~`count`: objects with a
/// unique single property each (so no discriminant covers half the members and
/// partitioning is skipped), interleaved with distinct number and string
/// literals. No widened `string`/`number` primitive is included, so the
/// cross-domain literals are NOT absorbed in the pre-sweep pass and all members
/// survive into the O(N²) reduction loop. This is the large-row "mixed
/// object/primitive/literal" shape where most pairs are cross-kind
/// (object-vs-literal, number-literal-vs-string-literal) and provably disjoint.
fn create_mixed_kind_members(interner: &TypeInterner, count: usize) -> Vec<TypeId> {
    let mut members = Vec::with_capacity(count);
    for i in 0..count {
        match i % 4 {
            // Object with a unique property name + literal value type. No single
            // property name is shared across members, so the discriminant
            // partition pass returns None and the union lands on the quadratic
            // path. Objects only ever relate to other objects in the shallow
            // engine.
            0 => {
                let prop_name = interner.intern_string(&format!("p{i}"));
                let val = interner.literal_number((1000 + i) as f64);
                let obj = interner.object(vec![PropertyInfo::new(prop_name, val)]);
                members.push(obj);
            }
            1 => {
                let prop_name = interner.intern_string(&format!("q{i}"));
                let val = interner.literal_string(&format!("v{i}"));
                let obj = interner.object(vec![PropertyInfo::new(prop_name, val)]);
                members.push(obj);
            }
            // Distinct number literal: survives (no widened `number` peer), only
            // relates to a same-domain primitive or template, neither present.
            2 => members.push(interner.literal_number((7_000_000 + i) as f64)),
            // Distinct string literal: survives (no widened `string` peer).
            _ => members.push(interner.literal_string(&format!("s{i}"))),
        }
    }
    members
}

fn bench_mixed_kind(c: &mut Criterion, count: usize) {
    c.bench_function(&format!("union_mixed_kind_{count}"), |b| {
        b.iter_with_setup(
            || {
                // Fresh interner per iteration so the union-normalize memo never
                // hides the reduction work across iterations.
                let interner = TypeInterner::new();
                let members = create_mixed_kind_members(&interner, count);
                (interner, members)
            },
            |(interner, members)| black_box(interner.union(black_box(members))),
        );
    });
}

fn bench_union_mixed_kind_80(c: &mut Criterion) {
    bench_mixed_kind(c, 80);
}
fn bench_union_mixed_kind_200(c: &mut Criterion) {
    bench_mixed_kind(c, 200);
}
fn bench_union_mixed_kind_400(c: &mut Criterion) {
    bench_mixed_kind(c, 400);
}

/// Build a wide union of `count` distinct, fully inert `keyof <unique literal>`
/// members. The shallow subtype engine can only relate two such members by
/// identity (already deduped), so the entire band is inert: the widened
/// inert-deferred lift partitions it out of the pairwise sweep in O(N) and the
/// reduction contributes zero `is_subtype_shallow` calls. Before the lift was
/// widened past `Conditional`/`IndexAccess`, this shape drove the full N·(N−1)
/// sweep (all `false`).
fn create_inert_keyof_members(interner: &TypeInterner, count: usize) -> Vec<TypeId> {
    (0..count)
        .map(|i| interner.keyof(interner.literal_number(i as f64)))
        .collect()
}

fn bench_inert_keyof(c: &mut Criterion, count: usize) {
    c.bench_function(&format!("union_inert_keyof_{count}"), |b| {
        b.iter_with_setup(
            || {
                let interner = TypeInterner::new();
                let members = create_inert_keyof_members(&interner, count);
                (interner, members)
            },
            |(interner, members)| black_box(interner.union(black_box(members))),
        );
    });
}

fn bench_union_inert_keyof_256(c: &mut Criterion) {
    bench_inert_keyof(c, 256);
}
fn bench_union_inert_keyof_512(c: &mut Criterion) {
    bench_inert_keyof(c, 512);
}
fn bench_union_inert_keyof_1000(c: &mut Criterion) {
    bench_inert_keyof(c, 1000);
}

criterion_group!(
    benches,
    bench_union_512_tuples,
    bench_union_512_arrays,
    bench_incremental_union_512,
    bench_union_100_tuples,
    bench_union_512_identical,
    bench_union_mixed_kind_80,
    bench_union_mixed_kind_200,
    bench_union_mixed_kind_400,
    bench_union_inert_keyof_256,
    bench_union_inert_keyof_512,
    bench_union_inert_keyof_1000,
);
criterion_main!(benches);
