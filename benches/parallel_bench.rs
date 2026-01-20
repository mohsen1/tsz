//! Parallel Type Checking Benchmarks
//!
//! This benchmark suite tests the scaling of parallel type checking
//! with the lock-free TypeInterner architecture.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use rayon::prelude::*;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use std::time::Duration;
use wasm::solver::{TypeId, TypeInterner};

/// Benchmark type interning under concurrent load
fn bench_concurrent_interning(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_interning");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));

    for thread_count in [1, 2, 4, 8, 16].iter() {
        group.bench_with_input(
            BenchmarkId::new("threads", thread_count),
            thread_count,
            |b, &thread_count| {
                // Use a thread pool for consistent thread counts
                let pool = rayon::ThreadPoolBuilder::new()
                    .num_threads(thread_count)
                    .build()
                    .unwrap();

                b.iter(|| {
                    let interner = Arc::new(TypeInterner::new());
                    pool.scope(|_s| {
                        // Each thread interns many types concurrently
                        (0..10000).into_par_iter().for_each(|i| {
                            let _ = interner.intern_string(&format!("property_{}", i % 1000));
                            let _ = interner.literal_number(i as f64);
                            let _ = interner.union(vec![
                                TypeId::STRING,
                                TypeId::NUMBER,
                                interner.literal_number((i % 10) as f64),
                            ]);
                        });
                    });
                });
            },
        );
    }
    group.finish();
}

/// Benchmark object type creation under concurrent load
fn bench_concurrent_objects(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_objects");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));

    for thread_count in [1, 2, 4, 8].iter() {
        group.bench_with_input(
            BenchmarkId::new("object_creation", thread_count),
            thread_count,
            |b, &thread_count| {
                let pool = rayon::ThreadPoolBuilder::new()
                    .num_threads(thread_count)
                    .build()
                    .unwrap();

                b.iter(|| {
                    let interner = Arc::new(TypeInterner::new());
                    pool.scope(|_s| {
                        (0..1000).into_par_iter().for_each(|i| {
                            use wasm::solver::{PropertyInfo, TypeParamInfo};
                            let name = interner.intern_string(&format!("prop_{}", i % 100));
                            let _ = interner.object(vec![PropertyInfo {
                                name,
                                type_id: TypeId::STRING,
                                write_type: TypeId::STRING,
                                optional: false,
                                readonly: false,
                                is_method: false,
                            }]);
                        });
                    });
                });
            },
        );
    }
    group.finish();
}

/// Test scaling efficiency - measure throughput per thread
fn bench_scaling_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("scaling_efficiency");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(2));

    // Baseline: single-threaded performance
    let baseline_ops = {
        let interner = TypeInterner::new();
        let start = std::time::Instant::now();
        for i in 0..100000 {
            let _ = interner.intern_string(&format!("key_{}", i));
            let _ = interner.literal_number(i as f64);
        }
        start.elapsed().as_nanos() as f64
    };

    for thread_count in [2, 4, 8].iter() {
        group.bench_with_input(
            BenchmarkId::new("speedup", thread_count),
            thread_count,
            |b, &thread_count| {
                let pool = rayon::ThreadPoolBuilder::new()
                    .num_threads(thread_count)
                    .build()
                    .unwrap();

                b.iter(|| {
                    let interner = Arc::new(TypeInterner::new());
                    pool.scope(|_s| {
                        (0..100000).into_par_iter().for_each(|i| {
                            let _ = interner.intern_string(&format!("key_{}", i));
                            let _ = interner.literal_number(i as f64);
                        });
                    });
                });
            },
        );
    }
    group.finish();
}

/// Verify no contention: measure operations per second
fn bench_contention_check(c: &mut Criterion) {
    let mut group = c.benchmark_group("contention_check");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(2));

    // This test creates high contention by all threads accessing the same keys
    group.bench_function("high_contention", |b| {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(8)
            .build()
            .unwrap();

        b.iter(|| {
            let interner = Arc::new(TypeInterner::new());
            pool.scope(|_s| {
                // All threads intern the same 100 strings repeatedly
                (0..10000).into_par_iter().for_each(|i| {
                    let key = i % 100;
                    let _ = interner.intern_string(&format!("shared_key_{}", key));
                    let _ = interner.literal_number(key as f64);
                });
            });
        });
    });

    // This test has low contention by using different keys per thread
    group.bench_function("low_contention", |b| {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(8)
            .build()
            .unwrap();

        b.iter(|| {
            let interner = Arc::new(TypeInterner::new());
            pool.scope(|_s| {
                (0..10000).into_par_iter().for_each(|i| {
                    // Different keys per thread due to rayon's work distribution
                    let _ = interner.intern_string(&format!("unique_key_{}", i));
                    let _ = interner.literal_number(i as f64);
                });
            });
        });
    });

    group.finish();
}

/// Benchmark property lookup with caching
fn bench_property_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("property_lookup");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(2));

    group.bench_function("small_object", |b| {
        use wasm::solver::{ObjectShape, PropertyInfo};
        let interner = TypeInterner::new();

        // Create a small object (under cache threshold)
        let props = (0..5)
            .map(|i| PropertyInfo {
                name: interner.intern_string(&format!("prop_{}", i)),
                type_id: TypeId::STRING,
                write_type: TypeId::STRING,
                optional: false,
                readonly: false,
                is_method: false,
            })
            .collect::<Vec<_>>();

        let shape = ObjectShape {
            properties: props,
            string_index: None,
            number_index: None,
        };
        let shape_id = interner.intern_object_shape(shape.clone());

        b.iter(|| {
            let _ = interner.object_property_index(shape_id, interner.intern_string("prop_2"));
        });
    });

    group.bench_function("large_object_cached", |b| {
        use wasm::solver::{ObjectShape, PropertyInfo};
        let interner = TypeInterner::new();

        // Create a large object (above cache threshold)
        let props = (0..30)
            .map(|i| PropertyInfo {
                name: interner.intern_string(&format!("prop_{}", i)),
                type_id: TypeId::STRING,
                write_type: TypeId::STRING,
                optional: false,
                readonly: false,
                is_method: false,
            })
            .collect::<Vec<_>>();

        let shape = ObjectShape {
            properties: props,
            string_index: None,
            number_index: None,
        };
        let shape_id = interner.intern_object_shape(shape.clone());

        b.iter(|| {
            let _ = interner.object_property_index(shape_id, interner.intern_string("prop_15"));
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_concurrent_interning,
    bench_concurrent_objects,
    bench_scaling_efficiency,
    bench_contention_check,
    bench_property_lookup
);
criterion_main!(benches);
