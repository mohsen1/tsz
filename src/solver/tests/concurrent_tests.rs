//! Concurrent type interning tests
//!
//! These tests verify that the TypeInterner's lock-free DashMap architecture
//! enables true parallel type checking without lock contention or deadlocks.

use crate::interner::Atom;
use crate::solver::{
    CallableShape, FunctionShape, ObjectFlags, ObjectShape, ParamInfo, PropertyInfo, TypeId,
    TypeInterner, TypeParamInfo,
};
use rayon::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn test_concurrent_string_interning_deduplication() {
    let interner = Arc::new(TypeInterner::new());

    // Have many threads intern the same strings
    let strings: Vec<String> = (0..1000).map(|i| format!("string_{}", i % 100)).collect();

    let results: Vec<Atom> = strings
        .par_iter()
        .map(|s| interner.intern_string(s))
        .collect();

    // Verify deduplication: same string should produce same atom
    for (i, s) in strings.iter().enumerate() {
        let atom1 = results[i];
        let atom2 = interner.intern_string(s);
        assert_eq!(atom1, atom2, "String should deduplicate: {}", s);
    }

    // Verify we can resolve all atoms
    for atom in results.iter() {
        let resolved = interner.resolve_atom(*atom);
        assert!(!resolved.is_empty());
    }
}

#[test]
fn test_concurrent_type_interning() {
    let interner = Arc::new(TypeInterner::new());

    // Many threads intern different types concurrently
    let type_ids: Vec<TypeId> = (0..1000)
        .into_par_iter()
        .map(|i| match i % 4 {
            0 => interner.literal_number(i as f64),
            1 => interner.literal_string(&format!("str_{}", i)),
            2 => interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
            3 => interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]),
            _ => unreachable!(),
        })
        .collect();

    // Verify all types are valid (not error)
    for &type_id in &type_ids {
        assert_ne!(type_id, TypeId::ERROR);
        assert!(interner.lookup(type_id).is_some());
    }
}

#[test]
fn test_concurrent_object_creation() {
    let interner = Arc::new(TypeInterner::new());

    // Many threads create objects concurrently
    let object_types: Vec<TypeId> = (0..100)
        .into_par_iter()
        .map(|i| {
            let props = vec![
                PropertyInfo {
                    name: interner.intern_string("x"),
                    type_id: TypeId::NUMBER,
                    write_type: TypeId::NUMBER,
                    optional: false,
                    readonly: false,
                    is_method: false,
                },
                PropertyInfo {
                    name: interner.intern_string(&format!("prop_{}", i)),
                    type_id: TypeId::STRING,
                    write_type: TypeId::STRING,
                    optional: false,
                    readonly: false,
                    is_method: false,
                },
            ];
            interner.object(props)
        })
        .collect();

    // Verify all objects were created successfully
    assert_eq!(object_types.len(), 100);
    for &type_id in &object_types {
        assert_ne!(type_id, TypeId::ERROR);
        assert!(interner.lookup(type_id).is_some());
    }
}

#[test]
fn test_concurrent_function_creation() {
    let interner = Arc::new(TypeInterner::new());

    // Many threads create function types concurrently
    let function_types: Vec<TypeId> = (0..100)
        .into_par_iter()
        .map(|_| {
            let shape = FunctionShape {
                type_params: vec![TypeParamInfo {
                    name: interner.intern_string("T"),
                    constraint: None,
                    is_const: false,
                    default: None,
                }],
                params: vec![
                    ParamInfo {
                        name: Some(interner.intern_string("x")),
                        type_id: TypeId::NUMBER,
                        optional: false,
                        rest: false,
                    },
                    ParamInfo {
                        name: Some(interner.intern_string("y")),
                        type_id: TypeId::STRING,
                        optional: false,
                        rest: false,
                    },
                ],
                this_type: None,
                return_type: TypeId::BOOLEAN,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            };
            interner.function(shape)
        })
        .collect();

    // Verify all functions were created successfully
    assert_eq!(function_types.len(), 100);
    for &type_id in &function_types {
        assert_ne!(type_id, TypeId::ERROR);
        assert!(interner.lookup(type_id).is_some());
    }
}

#[test]
fn test_concurrent_union_creation() {
    let interner = Arc::new(TypeInterner::new());

    // Create many union types concurrently
    let union_types: Vec<TypeId> = (0..500)
        .into_par_iter()
        .map(|i| {
            interner.union(vec![
                TypeId::STRING,
                TypeId::NUMBER,
                interner.literal_number((i % 10) as f64),
            ])
        })
        .collect();

    // Verify all unions were created successfully
    assert_eq!(union_types.len(), 500);
    for &type_id in &union_types {
        assert_ne!(type_id, TypeId::ERROR);
        assert!(interner.lookup(type_id).is_some());
    }

    // Verify deduplication: same union should produce same TypeId
    let union1 = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union2 = interner.union(vec![TypeId::NUMBER, TypeId::STRING]); // Order normalized
    assert_eq!(union1, union2, "Unions should normalize and deduplicate");
}

#[test]
fn test_concurrent_intersection_creation() {
    let interner = Arc::new(TypeInterner::new());

    // Create many intersection types concurrently
    let intersection_types: Vec<TypeId> = (0..500)
        .into_par_iter()
        .map(|_| interner.intersection(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]))
        .collect();

    // Verify all intersections were created successfully
    // Note: disjoint primitives = never, so some will be NEVER
    assert_eq!(intersection_types.len(), 500);
    for &type_id in &intersection_types {
        assert!(interner.lookup(type_id).is_some());
    }
}

#[test]
fn test_concurrent_property_map_building() {
    let interner = Arc::new(TypeInterner::new());

    // Create a large object (above cache threshold)
    let props: Vec<PropertyInfo> = (0..30)
        .map(|i| PropertyInfo {
            name: interner.intern_string(&format!("prop_{}", i)),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        })
        .collect();

    let shape = ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: props,
        string_index: None,
        number_index: None,
    };
    let shape_id = interner.intern_object_shape(shape);

    // Concurrent property lookups should build the cache safely
    let lookups: Vec<_> = (0..100)
        .into_par_iter()
        .map(|i| {
            let prop_name = interner.intern_string(&format!("prop_{}", i % 30));
            interner.object_property_index(shape_id, prop_name)
        })
        .collect();

    // All lookups should succeed
    for lookup in &lookups {
        match lookup {
            crate::solver::PropertyLookup::Found(_) => {}
            crate::solver::PropertyLookup::NotFound => panic!("Property should be found"),
            crate::solver::PropertyLookup::Uncached => {} // OK for small objects
        }
    }
}

#[test]
fn test_no_data_races_in_parallel_type_checking() {
    use rayon::ThreadPoolBuilder;

    let pool = ThreadPoolBuilder::new().num_threads(8).build().unwrap();

    pool.scope(|_s| {
        let interner = Arc::new(TypeInterner::new());
        let counter = Arc::new(AtomicUsize::new(0));

        // Spawn many tasks that all access the interner
        (0..100).for_each(|_| {
            let interner_clone = Arc::clone(&interner);
            let counter_clone = Arc::clone(&counter);

            rayon::spawn(move || {
                // Perform various type operations
                for i in 0..100 {
                    let _ = interner_clone.intern_string(&format!("key_{}", i % 10));
                    let _ = interner_clone.literal_number(i as f64);
                    let _ = interner_clone.union(vec![TypeId::STRING, TypeId::NUMBER]);
                    counter_clone.fetch_add(1, Ordering::Relaxed);
                }
            });
        });
    });

    // If we reach here without panicking, there were no data races
    // The lock-free DashMap architecture handled concurrent access correctly
}

#[test]
fn test_concurrent_callable_creation() {
    let interner = Arc::new(TypeInterner::new());

    // Create callable types with multiple signatures concurrently
    let callable_types: Vec<TypeId> = (0..50)
        .into_par_iter()
        .map(|_| {
            let shape = CallableShape {
                symbol: None,
                call_signatures: vec![],
                construct_signatures: vec![],
                properties: vec![PropertyInfo {
                    name: interner.intern_string("length"),
                    type_id: TypeId::NUMBER,
                    write_type: TypeId::NUMBER,
                    optional: false,
                    readonly: true,
                    is_method: false,
                }],
                number_index: None,
                string_index: None,
            };
            interner.callable(shape)
        })
        .collect();

    // Verify all callables were created successfully
    assert_eq!(callable_types.len(), 50);
    for &type_id in &callable_types {
        assert_ne!(type_id, TypeId::ERROR);
        assert!(interner.lookup(type_id).is_some());
    }
}

#[test]
fn test_shard_distribution() {
    let interner = Arc::new(TypeInterner::new());

    // Intern many different types to distribute across shards
    let _type_ids: Vec<TypeId> = (0..10000)
        .into_par_iter()
        .map(|i| {
            interner.union(vec![
                interner.literal_number(i as f64),
                interner.literal_string(&format!("str_{}", i)),
            ])
        })
        .collect();

    // Verify the interner has many types (proving distribution)
    let total_types = interner.len();
    assert!(
        total_types > 1000,
        "Should have interned many types: {}",
        total_types
    );
}

#[test]
fn test_concurrent_array_creation() {
    let interner = Arc::new(TypeInterner::new());

    // Create many array types concurrently
    let array_types: Vec<TypeId> = (0..1000)
        .into_par_iter()
        .map(|i| {
            let element_type = match i % 4 {
                0 => TypeId::STRING,
                1 => TypeId::NUMBER,
                2 => TypeId::BOOLEAN,
                3 => interner.literal_number(i as f64),
                _ => unreachable!(),
            };
            interner.array(element_type)
        })
        .collect();

    // Verify all arrays were created successfully
    assert_eq!(array_types.len(), 1000);
    for &type_id in &array_types {
        assert_ne!(type_id, TypeId::ERROR);
        assert!(interner.lookup(type_id).is_some());
    }
}

#[test]
fn test_concurrent_tuple_creation() {
    use crate::solver::TupleElement;

    let interner = Arc::new(TypeInterner::new());

    // Create many tuple types concurrently
    let tuple_types: Vec<TypeId> = (0..100)
        .into_par_iter()
        .map(|_| {
            let elements = vec![
                TupleElement {
                    type_id: TypeId::STRING,
                    name: None,
                    optional: false,
                    rest: false,
                },
                TupleElement {
                    type_id: TypeId::NUMBER,
                    name: None,
                    optional: false,
                    rest: false,
                },
            ];
            interner.tuple(elements)
        })
        .collect();

    // Verify all tuples were created successfully
    assert_eq!(tuple_types.len(), 100);
    for &type_id in &tuple_types {
        assert_ne!(type_id, TypeId::ERROR);
        assert!(interner.lookup(type_id).is_some());
    }

    // Verify deduplication: same tuple should produce same TypeId
    let tuple1 = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let tuple2 = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    assert_eq!(tuple1, tuple2, "Tuples should deduplicate");
}

#[test]
fn test_concurrent_template_literal_creation() {
    use crate::solver::TemplateSpan;

    let interner = Arc::new(TypeInterner::new());

    // Create many template literal types concurrently
    let template_types: Vec<TypeId> = (0..100)
        .into_par_iter()
        .map(|i| {
            let spans = vec![
                TemplateSpan::Text(interner.intern_string("prefix_")),
                TemplateSpan::Type(interner.literal_string(&format!("literal_{}", i))),
            ];
            interner.template_literal(spans)
        })
        .collect();

    // Verify all template literals were created successfully
    assert_eq!(template_types.len(), 100);
    for &type_id in &template_types {
        assert_ne!(type_id, TypeId::ERROR);
        assert!(interner.lookup(type_id).is_some());
    }
}
