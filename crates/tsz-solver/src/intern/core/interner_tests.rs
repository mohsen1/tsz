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
fn type_predicate_cache_statistics_reports_union_normalize_entries() {
    let interner = TypeInterner::new();
    let _ = TypeDatabase::union2(&interner, TypeId::STRING, TypeId::NUMBER);

    let stats = interner.type_predicate_cache_statistics();

    assert!(
        stats.union_normalize_cache_entries > 0,
        "union normalization memo entries must be visible to cache residency reports"
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
        origin: crate::types::TypeParamOrigin::User,
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
        origin: crate::types::TypeParamOrigin::User,
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

/// Interning-identity invariants for the hand-maintained shape `Eq`/`Hash`
/// impls in `types/shape_identity.rs` (#13099). These pin which fields are
/// identity-bearing: cosmetic fields must not split interned ids, while
/// display-preserving fields (index-signature `param_name`, and
/// `declaration_order` under `PRESERVE_DECLARATION_ORDER`) deliberately must.
mod shape_identity {
    use super::*;
    use crate::types::IndexSignature;

    fn named_prop(interner: &TypeInterner, name: &str) -> PropertyInfo {
        PropertyInfo::new(interner.intern_string(name), TypeId::STRING)
    }

    fn string_index_sig(interner: &TypeInterner, param_name: Option<&str>) -> IndexSignature {
        IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: param_name.map(|name| interner.intern_string(name)),
        }
    }

    fn shape_with_string_index(interner: &TypeInterner, param_name: Option<&str>) -> ObjectShape {
        ObjectShape {
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: Some(string_index_sig(interner, param_name)),
            number_index: None,
            symbol: None,
        }
    }

    #[test]
    fn cosmetic_property_fields_do_not_split_interned_ids() {
        let interner = TypeInterner::new();
        let base = named_prop(&interner, "alpha");

        let mut quoted = base.clone();
        quoted.single_quoted_name = true;
        let mut prototype = base.clone();
        prototype.is_class_prototype = true;
        let mut ordered = base.clone();
        ordered.declaration_order = 42;

        let base_id = interner.object(vec![base]);
        assert_eq!(
            interner.object(vec![quoted]),
            base_id,
            "single_quoted_name is cosmetic quote style and must not split interning"
        );
        assert_eq!(
            interner.object(vec![prototype]),
            base_id,
            "is_class_prototype is declaration-site metadata and must not split interning"
        );
        assert_eq!(
            interner.object(vec![ordered]),
            base_id,
            "declaration_order is display-only without PRESERVE_DECLARATION_ORDER"
        );
    }

    #[test]
    fn semantic_property_fields_split_interned_ids() {
        let interner = TypeInterner::new();
        let base = named_prop(&interner, "alpha");
        let mut optional = base.clone();
        optional.optional = true;
        let mut string_named = base.clone();
        string_named.is_string_named = true;

        let base_id = interner.object(vec![base]);
        assert_ne!(
            interner.object(vec![optional]),
            base_id,
            "optional is structural and must split interning"
        );
        assert_ne!(
            interner.object(vec![string_named]),
            base_id,
            "is_string_named distinguishes \"100\" from 100 keys and must split interning"
        );
    }

    #[test]
    fn object_index_signature_param_name_is_display_preserving() {
        let interner = TypeInterner::new();

        // `IndexSignature` itself treats param_name as cosmetic...
        assert_eq!(
            string_index_sig(&interner, Some("key")),
            string_index_sig(&interner, Some("idx")),
            "IndexSignature eq ignores param_name"
        );

        // ...but `ObjectShape` re-adds it via index_signature_display_eq so the
        // printer can reproduce the source parameter name after interning.
        let key_id = interner.object_with_index(shape_with_string_index(&interner, Some("key")));
        let renamed_id =
            interner.object_with_index(shape_with_string_index(&interner, Some("idx")));
        let unnamed_id = interner.object_with_index(shape_with_string_index(&interner, None));
        assert_ne!(
            key_id, renamed_id,
            "different index-signature param_name must intern to distinct object ids"
        );
        assert_ne!(key_id, unnamed_id);
        assert_eq!(
            interner.object_with_index(shape_with_string_index(&interner, Some("key"))),
            key_id,
            "same param_name must re-intern to the same id"
        );
    }

    #[test]
    fn callable_index_signature_param_name_is_display_preserving() {
        let interner = TypeInterner::new();
        let with_name = |param_name: Option<&str>| CallableShape {
            string_index: Some(string_index_sig(&interner, param_name)),
            ..CallableShape::default()
        };

        let key_id = interner.callable(with_name(Some("key")));
        let renamed_id = interner.callable(with_name(Some("idx")));
        assert_ne!(
            key_id, renamed_id,
            "different index-signature param_name must intern to distinct callable ids"
        );
        assert_eq!(
            interner.callable(with_name(Some("key"))),
            key_id,
            "same param_name must re-intern to the same id"
        );
    }

    #[test]
    fn preserve_declaration_order_makes_property_order_identity_bearing() {
        let interner = TypeInterner::new();
        let alpha = named_prop(&interner, "alpha");
        let beta = PropertyInfo::new(interner.intern_string("beta"), TypeId::NUMBER);

        // Without the flag, source order is cosmetic: constructors backfill
        // declaration_order from insertion order, but it stays identity-exempt.
        let ab = interner.object(vec![alpha.clone(), beta.clone()]);
        let ba = interner.object(vec![beta.clone(), alpha.clone()]);
        assert_eq!(
            ab, ba,
            "without PRESERVE_DECLARATION_ORDER, declaration order must not split interning"
        );

        // With the flag, the same properties in a different source order stay
        // distinct so diagnostics can print source/display order after widening.
        let flag = ObjectFlags::PRESERVE_DECLARATION_ORDER;
        let ab_ordered = interner.object_with_flags(vec![alpha.clone(), beta.clone()], flag);
        let ba_ordered = interner.object_with_flags(vec![beta.clone(), alpha.clone()], flag);
        assert_ne!(
            ab_ordered, ba_ordered,
            "PRESERVE_DECLARATION_ORDER makes declaration order identity-bearing"
        );
        assert_eq!(
            interner.object_with_flags(vec![alpha, beta], flag),
            ab_ordered,
            "same declaration order must re-intern to the same id"
        );
    }
}

/// The thread-local string-intern cache must not change `Atom` identity:
/// interning must stay deterministic (same string -> same `Atom`), and a hit
/// must never return the `Atom` of a different string.
mod string_cache {
    use super::*;
    use tsz_common::interner::Atom;

    #[test]
    fn repeated_intern_returns_identical_atom() {
        let interner = TypeInterner::new();
        // First call misses the cache and mints the atom; later calls hit it.
        let a0 = interner.intern_string("length");
        let a1 = interner.intern_string("length");
        let a2 = interner.intern_string("length");
        assert_eq!(a0, a1);
        assert_eq!(a1, a2);
        // The atom round-trips back to the original string.
        assert_eq!(&*interner.resolve_atom_ref(a0), "length");
    }

    #[test]
    fn distinct_strings_get_distinct_atoms_through_the_cache() {
        let interner = TypeInterner::new();
        let names = [
            "T",
            "U",
            "length",
            "value",
            "[Symbol.iterator]",
            "__@iterator",
            "prototype",
            "call",
            "apply",
            "bind",
            "next",
            "done",
        ];
        let mut atoms = Vec::new();
        // Two interleaved passes exercise both the miss (first pass) and the
        // hit (second pass) paths for every name.
        for _ in 0..2 {
            for (i, name) in names.iter().enumerate() {
                let atom = interner.intern_string(name);
                if atoms.len() <= i {
                    atoms.push(atom);
                } else {
                    assert_eq!(atoms[i], atom, "cache changed the atom for {name:?}");
                }
                assert_eq!(&*interner.resolve_atom_ref(atom), *name);
            }
        }
        // All distinct names must have distinct atoms (no collision aliasing).
        for i in 0..atoms.len() {
            for j in (i + 1)..atoms.len() {
                assert_ne!(
                    atoms[i], atoms[j],
                    "distinct names {:?}/{:?} collapsed to one atom",
                    names[i], names[j]
                );
            }
        }
    }

    #[test]
    fn cache_agrees_with_uncached_shard_interner() {
        let interner = TypeInterner::new();
        for name in [
            "T",
            "length",
            "value",
            "averylongpropertynamethatdoesnotfitinline",
        ] {
            // The cached entry point and the raw shard interner must agree.
            let via_cache = interner.intern_string(name);
            let via_shard = interner.string_interner.intern(name);
            assert_eq!(
                via_cache, via_shard,
                "cache disagreed with shard for {name:?}"
            );
        }
    }

    #[test]
    fn empty_string_is_the_none_atom() {
        let interner = TypeInterner::new();
        assert_eq!(interner.intern_string(""), Atom::NONE);
    }

    #[test]
    fn long_strings_bypass_cache_but_stay_deterministic() {
        let interner = TypeInterner::new();
        // Longer than STRING_KEY_INLINE_CAP (23): never cached, always re-interned.
        let long = "ThisIsAVeryLongTypeParameterOrPropertyNameExceedingInlineCap";
        assert!(long.len() > 23);
        let a0 = interner.intern_string(long);
        let a1 = interner.intern_string(long);
        assert_eq!(a0, a1);
        assert_eq!(&*interner.resolve_atom_ref(a0), long);
    }
}
