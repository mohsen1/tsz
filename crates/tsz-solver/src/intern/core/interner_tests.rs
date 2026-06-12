use super::*;
use crate::caches::db::{TypeDatabase, TypePredicateCache};

#[test]
fn interned_type_limit_fallback_poison_returns_error() {
    let interner = TypeInterner::new();
    interner
        .alloc_counter
        .store((MAX_INTERNED_TYPES + 1) as u32, Ordering::Relaxed);

    assert!(interner.interned_type_limit_exceeded());
    assert_eq!(
        interner.interned_type_limit_context(),
        InternedTypeLimitContext {
            current_count: MAX_INTERNED_TYPES + 1,
            max_interned_types: MAX_INTERNED_TYPES,
            fallback_type: TypeId::ERROR,
        }
    );
    assert_eq!(interner.poison_due_to_interned_type_limit(), TypeId::ERROR);
    assert!(interner.poisoned.load(Ordering::Relaxed));
}

#[test]
fn poisoned_interner_keeps_existing_types_readable_and_rejects_new_ones() {
    let interner = TypeInterner::new();

    // Intern a couple of structurally distinct types before the limit hits.
    let pre_limit_union = TypeDatabase::union2(&interner, TypeId::STRING, TypeId::NUMBER);
    let pre_limit_data = interner
        .lookup(pre_limit_union)
        .expect("freshly interned type must be readable");

    // Simulate crossing the type-count limit.
    interner
        .alloc_counter
        .store((MAX_INTERNED_TYPES + 1) as u32, Ordering::Relaxed);
    assert_eq!(interner.poison_due_to_interned_type_limit(), TypeId::ERROR);
    assert!(interner.poisoned.load(Ordering::Relaxed));

    // Already-interned ids stay readable: graceful degradation must not
    // collapse previously computed program types (or the shared cross-file
    // caches holding their ids) into opaque misses.
    assert_eq!(interner.lookup(pre_limit_union), Some(pre_limit_data));

    // Re-interning an existing key resolves to the existing id.
    let reinterned = TypeDatabase::union2(&interner, TypeId::STRING, TypeId::NUMBER);
    assert_eq!(reinterned, pre_limit_union);

    // Brand-new structural keys degrade to ERROR.
    let fresh = TypeDatabase::union2(&interner, TypeId::BOOLEAN, TypeId::NUMBER);
    assert_eq!(fresh, TypeId::ERROR);
}

#[test]
fn interned_type_limit_boundary_is_strictly_greater_than_limit() {
    assert!(!TypeInterner::interned_type_limit_exceeded_for_count(
        MAX_INTERNED_TYPES
    ));
    assert!(TypeInterner::interned_type_limit_exceeded_for_count(
        MAX_INTERNED_TYPES + 1
    ));
}

#[test]
fn estimated_size_accounts_for_retained_predicate_caches() {
    let interner = TypeInterner::new();
    let before = interner.estimated_size_bytes();

    interner.set_contains_this_type_cache(TypeId::NUMBER, true);
    interner.set_contains_infer_types_cache(TypeId::STRING, false);
    interner.set_contains_type_query_cache(TypeId::BOOLEAN, true);

    assert!(
        interner.estimated_size_bytes() > before,
        "retained TypeInterner predicate cache entries must be visible to residency estimates",
    );
}

#[test]
fn boxed_def_id_registration_is_idempotent() {
    let interner = TypeInterner::new();
    let def_id = DefId(7);

    interner.register_boxed_def_id(IntrinsicKind::Function, def_id);
    interner.register_boxed_def_id(IntrinsicKind::Function, def_id);

    assert!(interner.is_boxed_def_id(def_id, IntrinsicKind::Function));
    assert_eq!(
        interner
            .boxed_def_ids
            .get(&IntrinsicKind::Function)
            .map(|entry| entry.len()),
        Some(1)
    );
}

#[test]
fn all_type_parameter_intersections_preserve_ordered_members() {
    let interner = TypeInterner::new();
    let alpha = interner.type_param(TypeParamInfo::simple(interner.intern_string("Alpha")));
    let beta = interner.type_param(TypeParamInfo::simple(interner.intern_string("Beta")));
    let gamma = interner.type_param(TypeParamInfo::simple(interner.intern_string("Gamma")));

    let result = interner.intersection(vec![alpha, beta, gamma, beta]);
    let Some(TypeData::Intersection(list_id)) = interner.lookup(result) else {
        panic!("expected all-type-parameter intersection, got {result:?}");
    };

    assert_eq!(&*interner.type_list(list_id), &[alpha, beta, gamma]);
}

#[test]
fn same_name_type_parameter_intersection_collapses_to_constrained_member() {
    let interner = TypeInterner::new();
    let name = interner.intern_string("T");
    let unconstrained = interner.type_param(TypeParamInfo::simple(name));
    let constrained = interner.type_param(TypeParamInfo {
        name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    });

    assert_eq!(
        interner.intersection(vec![unconstrained, constrained]),
        constrained
    );
}

#[test]
fn same_name_type_parameter_replacement_dedups_non_adjacent_members() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let unconstrained_t = interner.type_param(TypeParamInfo::simple(t_name));
    let constrained_t = interner.type_param(TypeParamInfo {
        name: t_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    });
    let u = interner.type_param(TypeParamInfo::simple(u_name));

    let result = interner.intersection(vec![unconstrained_t, u, constrained_t]);
    let Some(TypeData::Intersection(list_id)) = interner.lookup(result) else {
        panic!("expected all-type-parameter intersection, got {result:?}");
    };

    assert_eq!(&*interner.type_list(list_id), &[constrained_t, u]);
}

/// Append-protocol regression tests for #13046: ids are allocated by
/// `fetch_add` before the storage lock is taken, so writers can reach the
/// lock out of id order. The old while-`push` backfill let an
/// earlier-arriving higher id claim a later-arriving lower id's slot,
/// silently corrupting `get(id)` for the rest of the session.
mod append_protocol {
    use super::storage::write_id_slot;
    use super::*;
    use std::sync::Barrier;
    use std::thread;

    const THREADS: usize = 8;
    const PER_THREAD: u32 = 2_000;

    #[test]
    fn write_id_slot_round_trips_out_of_order_arrivals() {
        let mut vec: Vec<u32> = Vec::new();
        // A higher id reaches the lock first; lower ids arrive later and
        // must still land in their own slots.
        write_id_slot(&mut vec, 5, 50, || u32::MAX);
        write_id_slot(&mut vec, 1, 10, || u32::MAX);
        write_id_slot(&mut vec, 3, 30, || u32::MAX);
        assert_eq!(vec.len(), 6);
        assert_eq!((vec[1], vec[3], vec[5]), (10, 30, 50));
        // Unwritten gaps hold the placeholder, never another id's data.
        assert_eq!((vec[0], vec[2], vec[4]), (u32::MAX, u32::MAX, u32::MAX));
    }

    #[test]
    fn slice_interner_ids_round_trip_under_concurrent_interning() {
        let interner = ConcurrentSliceInterner::<u32>::new();
        let barrier = Barrier::new(THREADS);
        thread::scope(|s| {
            let handles: Vec<_> = (0..THREADS as u32)
                .map(|t| {
                    let (interner, barrier) = (&interner, &barrier);
                    s.spawn(move || {
                        barrier.wait();
                        (0..PER_THREAD)
                            .map(|i| (interner.intern(&[t, i]), [t, i]))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            for handle in handles {
                for (id, slice) in handle.join().expect("intern thread panicked") {
                    let stored = interner.get(id).expect("interned id must resolve");
                    assert_eq!(
                        &*stored, &slice,
                        "id {id} resolved to another writer's slice"
                    );
                }
            }
        });
    }

    #[test]
    fn value_interner_ids_round_trip_under_concurrent_interning() {
        let interner = ConcurrentValueInterner::<u64>::new();
        let barrier = Barrier::new(THREADS);
        thread::scope(|s| {
            let handles: Vec<_> = (0..THREADS as u64)
                .map(|t| {
                    let (interner, barrier) = (&interner, &barrier);
                    s.spawn(move || {
                        barrier.wait();
                        (0..u64::from(PER_THREAD))
                            .map(|i| {
                                let value = (t << 32) | i;
                                (interner.intern(value), value)
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            for handle in handles {
                for (id, value) in handle.join().expect("intern thread panicked") {
                    assert_eq!(
                        interner.get_copy(id),
                        Some(value),
                        "id {id} resolved to another writer's value"
                    );
                }
            }
        });
    }

    /// Threads race on the same value first (losers leak their allocated
    /// ids), then intern unique values; leaked ids must not shift later
    /// slots out of alignment.
    #[test]
    fn value_interner_duplicate_race_keeps_later_ids_aligned() {
        let interner = ConcurrentValueInterner::<u64>::new();
        let barrier = Barrier::new(THREADS);
        thread::scope(|s| {
            let handles: Vec<_> = (0..THREADS as u64)
                .map(|t| {
                    let (interner, barrier) = (&interner, &barrier);
                    s.spawn(move || {
                        barrier.wait();
                        let shared = interner.intern(u64::MAX);
                        let unique = (t + 1) * 10_000;
                        (shared, interner.intern(unique), unique)
                    })
                })
                .collect();
            for handle in handles {
                let (shared, unique_id, unique) = handle.join().expect("intern thread panicked");
                assert_eq!(interner.get_copy(shared), Some(u64::MAX));
                assert_eq!(interner.get_copy(unique_id), Some(unique));
            }
        });
    }

    #[test]
    fn type_interner_lookup_round_trips_under_concurrent_interning() {
        let interner = TypeInterner::new();
        let barrier = Barrier::new(THREADS);
        thread::scope(|s| {
            let handles: Vec<_> = (0..THREADS as u32)
                .map(|t| {
                    let (interner, barrier) = (&interner, &barrier);
                    s.spawn(move || {
                        barrier.wait();
                        (0..PER_THREAD)
                            .map(|i| {
                                let key = TypeData::Array(TypeId(
                                    TypeId::FIRST_USER + t * PER_THREAD + i,
                                ));
                                (interner.intern(key), key)
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            for handle in handles {
                for (id, key) in handle.join().expect("intern thread panicked") {
                    assert_eq!(
                        interner.lookup(id),
                        Some(key),
                        "id {id:?} resolved to another writer's TypeData"
                    );
                    assert_eq!(interner.intern(key), id, "re-intern must be stable");
                }
            }
        });
    }
}
