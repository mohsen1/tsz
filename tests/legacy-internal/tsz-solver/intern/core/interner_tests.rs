use super::*;
use crate::caches::db::{TypeDatabase, TypePredicateCache};
use crate::caches::instantiation_cache::{CanonicalSubst, InstantiationCacheKey};
use crate::types::{TypeParamInfo, TypeParamOrigin};

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
fn estimated_size_accounts_for_retained_pure_function_memos() {
    let interner = TypeInterner::new();
    let before = interner.estimated_size_bytes();
    let t_atom = interner.intern_string("T");
    let t_info = TypeParamInfo {
        name: t_atom,
        constraint: None,
        default: None,
        is_const: false,
        origin: TypeParamOrigin::User,
    };
    let proto_key = InstantiationCacheKey::new(TypeId::STRING, CanonicalSubst::empty(), 0, None);

    interner.set_widen_type_memo(TypeId::BOOLEAN_TRUE, TypeId::BOOLEAN);
    interner.set_extract_type_params_memo(TypeId::STRING, vec![t_info].into());
    interner.set_proto_instantiation_memo(proto_key, TypeId::NUMBER);
    interner.set_contravariant_infer_names_memo(TypeId::OBJECT, vec![t_atom].into());
    interner.set_contains_type_by_id_memo(TypeId::STRING, TypeId::NUMBER, false);
    interner.set_prune_union_members_memo(TypeId::BOOLEAN, TypeId::BOOLEAN);

    assert!(
        interner.estimated_size_bytes() > before,
        "retained TypeInterner pure-function memo entries must be visible to residency estimates",
    );
}

#[test]
fn type_predicate_cache_statistics_reports_pure_function_memos() {
    let interner = TypeInterner::new();
    let t_atom = interner.intern_string("T");
    let t_info = TypeParamInfo {
        name: t_atom,
        constraint: None,
        default: None,
        is_const: false,
        origin: TypeParamOrigin::User,
    };
    let proto_key = InstantiationCacheKey::new(TypeId::STRING, CanonicalSubst::empty(), 0, None);

    interner.set_widen_type_memo(TypeId::BOOLEAN_TRUE, TypeId::BOOLEAN);
    interner.set_extract_type_params_memo(TypeId::STRING, vec![t_info].into());
    interner.set_proto_instantiation_memo(proto_key, TypeId::NUMBER);
    interner.set_contravariant_infer_names_memo(TypeId::OBJECT, vec![t_atom].into());

    let stats = interner.type_predicate_cache_statistics();
    assert_eq!(stats.widen_type_cache_entries, 1);
    assert_eq!(stats.extract_type_params_cache_entries, 1);
    assert_eq!(stats.proto_instantiation_cache_entries, 1);
    assert_eq!(stats.contravariant_infer_names_cache_entries, 1);
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
    assert!(
        stats.union_normalize_cache_member_slots >= 2,
        "union normalization memo member slots must be visible to cache residency reports"
    );
}

#[test]
fn union_normalize_cache_bounded_insert_evicts_entries_and_member_slots() {
    let interner = TypeInterner::new();

    interner.insert_union_normalize_cache_with_limits(
        Box::from([TypeId(100), TypeId(101)]),
        TypeId::STRING,
        2,
        4,
    );
    interner.insert_union_normalize_cache_with_limits(
        Box::from([TypeId(102), TypeId(103)]),
        TypeId::NUMBER,
        2,
        4,
    );
    interner.insert_union_normalize_cache_with_limits(
        Box::from([TypeId(104), TypeId(105)]),
        TypeId::BOOLEAN,
        2,
        4,
    );

    let stats = interner.type_predicate_cache_statistics();
    assert!(
        stats.union_normalize_cache_entries <= 2,
        "union normalization memo must evict instead of retaining unbounded exact keys"
    );
    assert!(
        stats.union_normalize_cache_member_slots <= 4,
        "union normalization memo must evict by retained member slots, not only entry count"
    );
}

mod union_preserve_members_tests {
    use super::*;

    /// `union_preserve_members` skips structural subtype reduction but must still
    /// apply the universal top/bottom sentinel absorptions (`error` > `any` >
    /// `unknown`), since `unknown | T` is `unknown`, `any | T` is `any`, and
    /// `error | T` is `error` for every `T`. The flow-narrowing logical-condition
    /// combiner relied on this: the false branch of
    /// `typeof x === "object" && x !== null` over an `unknown` produced a stray
    /// `unknown | null`, which then mis-narrowed under a later `typeof` guard.
    #[test]
    fn union_preserve_members_absorbs_top_and_bottom_sentinels() {
        let interner = TypeInterner::new();

        assert_eq!(
            interner.union_preserve_members(vec![TypeId::UNKNOWN, TypeId::NULL]),
            TypeId::UNKNOWN,
            "unknown | null must absorb to unknown"
        );
        assert_eq!(
            interner.union_preserve_members(vec![TypeId::STRING, TypeId::UNKNOWN, TypeId::NUMBER]),
            TypeId::UNKNOWN,
            "any member set containing unknown collapses to unknown"
        );

        // `any | T` is `any`; `any` outranks `unknown`.
        assert_eq!(
            interner.union_preserve_members(vec![TypeId::ANY, TypeId::STRING]),
            TypeId::ANY,
            "any | string must absorb to any"
        );
        assert_eq!(
            interner.union_preserve_members(vec![TypeId::UNKNOWN, TypeId::ANY]),
            TypeId::ANY,
            "any outranks unknown"
        );

        // `error | T` is `error`; `error` outranks everything.
        assert_eq!(
            interner.union_preserve_members(vec![TypeId::ERROR, TypeId::ANY]),
            TypeId::ERROR,
            "error outranks any"
        );

        // A sentinel nested inside a member union is also caught (members are
        // flattened before the scan).
        let nested = interner.union_preserve_members(vec![TypeId::STRING, TypeId::UNKNOWN]);
        assert_eq!(
            interner.union_preserve_members(vec![TypeId::NUMBER, nested]),
            TypeId::UNKNOWN,
            "unknown nested inside a member union still absorbs"
        );

        // Non-sentinel members keep their structure (no subtype reduction).
        assert_eq!(
            interner.union_preserve_members(vec![TypeId::STRING, TypeId::NUMBER]),
            interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
            "ordinary members are unaffected"
        );
    }

    /// `union(Vec)` and `union_from_slice(&[..])` both delegate to the same
    /// `union_from_iter` normalizer, so they must return a byte-identical
    /// `TypeId` for the same member sequence. The hot evaluation/instantiation/
    /// inference/widening/narrowing paths rely on this equivalence to construct
    /// unions from a borrowed slice (`union_from_slice`) instead of cloning the
    /// member vector into `union` — a pure allocation cut with no result change.
    /// This pins the contract so a future divergence in either constructor is
    /// caught here rather than as a silent diagnostic drift downstream.
    #[test]
    fn union_from_slice_matches_owned_union() {
        let interner = TypeInterner::new();
        let cases: &[Vec<TypeId>] = &[
            vec![TypeId::STRING, TypeId::NUMBER],
            // Order that requires sorting/normalization.
            vec![TypeId::NUMBER, TypeId::STRING, TypeId::BOOLEAN],
            // Duplicates that must dedup identically.
            vec![TypeId::STRING, TypeId::STRING, TypeId::NUMBER],
            // Sentinel absorption (`any | T` == `any`).
            vec![TypeId::ANY, TypeId::STRING],
            // Single-member and empty edge cases.
            vec![TypeId::STRING],
            vec![],
        ];
        for members in cases {
            assert_eq!(
                interner.union(members.clone()),
                interner.union_from_slice(members),
                "union(Vec) and union_from_slice(&[..]) must agree for {members:?}",
            );
        }
    }
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
            symbol_index: None,
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

/// Non-strict (`strictNullChecks` off) `null`/`undefined` absorption at the
/// union-construction seam (#16580). tsc's `addTypeToUnion` never adds a nullable
/// constituent to a union in non-strict mode; it survives only when the set would
/// otherwise be empty (an all-nullish union). The interner must apply this
/// uniformly across every union constructor so a member set keeps one canonical
/// identity per program.
mod nonstrict_nullish_union_tests {
    use super::*;

    fn nonstrict() -> TypeInterner {
        let interner = TypeInterner::new();
        interner.set_strict_null_checks(false);
        interner
    }

    fn union_members(interner: &TypeInterner, id: TypeId) -> Option<Vec<TypeId>> {
        match interner.lookup(id)? {
            TypeData::Union(list_id) => Some(interner.type_list(list_id).to_vec()),
            _ => None,
        }
    }

    #[test]
    fn nonstrict_drops_nullish_when_a_non_nullish_sibling_is_present() {
        let interner = nonstrict();
        assert_eq!(
            interner.union(vec![TypeId::NUMBER, TypeId::NULL]),
            TypeId::NUMBER,
            "number | null -> number"
        );
        assert_eq!(
            interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]),
            TypeId::NUMBER,
            "number | undefined -> number"
        );
        assert_eq!(
            interner.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]),
            TypeId::STRING,
            "string | null | undefined -> string"
        );
    }

    #[test]
    fn nonstrict_keeps_void_and_never_semantics() {
        let interner = nonstrict();
        // `void` is not `TypeFlags.Nullable`: it stays in the union.
        let with_void = interner.union(vec![TypeId::NUMBER, TypeId::VOID]);
        let members = union_members(&interner, with_void).expect("number | void stays a union");
        assert!(
            members.contains(&TypeId::VOID) && members.contains(&TypeId::NUMBER),
            "void must survive non-strict union construction: {members:?}"
        );
        // `never` is stripped and never counts as the non-nullish sibling, so a
        // nullish + never set is all-nullish and keeps its nullish member.
        assert_eq!(
            interner.union(vec![TypeId::NULL, TypeId::NEVER]),
            TypeId::NULL,
            "null | never -> null (never stripped, null kept)"
        );
        assert_eq!(
            interner.union(vec![TypeId::NUMBER, TypeId::NULL, TypeId::NEVER]),
            TypeId::NUMBER,
            "number | null | never -> number"
        );
    }

    #[test]
    fn nonstrict_all_nullish_collapses_to_scalar_null() {
        let interner = nonstrict();
        // tsc's `addTypeToUnion` exclusion is unconditional, so an all-nullish set
        // leaves `typeSet` empty and `getUnionType` yields the scalar survivor —
        // `null` preferred over `undefined` on presence, not written position, and
        // never a surviving `Union` node or `never` (#16657).
        assert_eq!(
            interner.union(vec![TypeId::NULL, TypeId::UNDEFINED]),
            TypeId::NULL,
            "null | undefined -> scalar null"
        );
        // Order must not matter: `null` wins on presence, not position.
        assert_eq!(
            interner.union(vec![TypeId::UNDEFINED, TypeId::NULL]),
            TypeId::NULL,
            "undefined | null -> scalar null"
        );
        // Discriminating negative: with no `null` in the set, `undefined` is the
        // survivor — this is not "rewrite every nullish to null". `never` is the
        // union identity and must not keep the set from collapsing to the scalar.
        assert_eq!(
            interner.union(vec![TypeId::UNDEFINED, TypeId::NEVER]),
            TypeId::UNDEFINED,
            "undefined | never -> scalar undefined (never stripped, no null present)"
        );
        // `void` is not `TypeFlags.Nullable`, so it is a genuine non-nullish sibling
        // that absorbs the dropped `undefined`: `undefined | void` reduces to the
        // scalar `void`, matching tsc's unconditional nullish exclusion — this path
        // is the sibling-present drop, not the all-nullish collapse.
        assert_eq!(
            interner.union(vec![TypeId::UNDEFINED, TypeId::VOID]),
            TypeId::VOID,
            "undefined | void -> void (undefined dropped, void survives)"
        );
        // An all-nullish set must never collapse to `never` (#16580 row a6).
        assert_ne!(
            interner.union(vec![TypeId::NULL, TypeId::UNDEFINED]),
            TypeId::NEVER,
            "null | undefined must not become never"
        );
    }

    #[test]
    fn nonstrict_all_nullish_collapse_is_uniform_across_every_seam() {
        let interner = nonstrict();
        // The collapse lives in the shared construction primitive, so every union
        // constructor reaches the same scalar `null` for an all-nullish set
        // (one-universe invariant), just as the sibling-present drop does.
        assert_eq!(
            interner.union(vec![TypeId::NULL, TypeId::UNDEFINED]),
            TypeId::NULL
        );
        assert_eq!(
            interner.union_from_slice(&[TypeId::NULL, TypeId::UNDEFINED]),
            TypeId::NULL
        );
        assert_eq!(
            interner.union2(TypeId::NULL, TypeId::UNDEFINED),
            TypeId::NULL
        );
        assert_eq!(
            interner.union2(TypeId::UNDEFINED, TypeId::NULL),
            TypeId::NULL
        );
        assert_eq!(
            interner.union3(TypeId::NULL, TypeId::UNDEFINED, TypeId::NEVER),
            TypeId::NULL
        );
        // The all-nullish collapse replaces the buffer with the lone survivor, so
        // it holds on `union_from_sorted_vec` regardless of the input member order.
        assert_eq!(
            interner.union_from_sorted_vec(vec![TypeId::NULL, TypeId::UNDEFINED]),
            TypeId::NULL
        );
        assert_eq!(
            interner.union_preserve_members(vec![TypeId::NULL, TypeId::UNDEFINED]),
            TypeId::NULL
        );
        assert_eq!(
            interner.union_literal_reduce(vec![TypeId::NULL, TypeId::UNDEFINED]),
            TypeId::NULL
        );
    }

    #[test]
    fn nonstrict_reduction_is_uniform_across_every_seam() {
        let interner = nonstrict();
        // Every union constructor must reach the same canonical identity for the
        // same member set (one-universe invariant).
        assert_eq!(
            interner.union(vec![TypeId::NUMBER, TypeId::NULL]),
            TypeId::NUMBER
        );
        assert_eq!(
            interner.union_from_slice(&[TypeId::NUMBER, TypeId::NULL]),
            TypeId::NUMBER
        );
        assert_eq!(
            interner.union2(TypeId::NUMBER, TypeId::NULL),
            TypeId::NUMBER
        );
        assert_eq!(
            interner.union2(TypeId::NULL, TypeId::NUMBER),
            TypeId::NUMBER
        );
        assert_eq!(
            interner.union3(TypeId::NUMBER, TypeId::NULL, TypeId::UNDEFINED),
            TypeId::NUMBER
        );
        // `union_from_sorted_vec` requires a pre-sorted list: NUMBER(9) < NULL(15).
        assert_eq!(
            interner.union_from_sorted_vec(vec![TypeId::NUMBER, TypeId::NULL]),
            TypeId::NUMBER
        );
        assert_eq!(
            interner.union_preserve_members(vec![TypeId::NUMBER, TypeId::NULL]),
            TypeId::NUMBER
        );
        assert_eq!(
            interner.union_literal_reduce(vec![TypeId::NUMBER, TypeId::NULL]),
            TypeId::NUMBER
        );
    }

    #[test]
    fn strict_mode_keeps_every_nullish_member() {
        // The default interner is strict: the reduction is a no-op, so all the
        // seams retain `null`/`undefined` exactly as before.
        let interner = TypeInterner::new();
        for id in [
            interner.union(vec![TypeId::NUMBER, TypeId::NULL]),
            interner.union2(TypeId::NUMBER, TypeId::NULL),
            interner.union_preserve_members(vec![TypeId::NUMBER, TypeId::NULL]),
            interner.union_from_sorted_vec(vec![TypeId::NUMBER, TypeId::NULL]),
            interner.union_literal_reduce(vec![TypeId::NUMBER, TypeId::NULL]),
        ] {
            let members = union_members(&interner, id)
                .unwrap_or_else(|| panic!("strict number | null must stay a union: {id:?}"));
            assert!(
                members.contains(&TypeId::NULL) && members.contains(&TypeId::NUMBER),
                "strict mode must keep null: {members:?}"
            );
        }
    }
}
