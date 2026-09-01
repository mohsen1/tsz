//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/visitors/visitor.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN a4d5d42bbfe65d6ef6acfe513755da57a02dd01d6459a21627afbc015c45fc84 1538 referenced_type_walk_state_names_entered_and_revisit
    #[test]
    fn referenced_type_walk_state_names_entered_and_revisit() {
        let db = TypeInterner::new();
        let type_id = db.object(vec![]);
        let mut visited = FxHashSet::default();

        assert_eq!(
            ReferencedTypeWalkState::enter(&mut visited, type_id),
            ReferencedTypeWalkState::Entered
        );
        assert_eq!(
            ReferencedTypeWalkState::enter(&mut visited, type_id),
            ReferencedTypeWalkState::AlreadyVisited
        );
    }
// TSZ_INLINE_TEST_END a4d5d42bbfe65d6ef6acfe513755da57a02dd01d6459a21627afbc015c45fc84

// TSZ_INLINE_TEST_BEGIN af742ad4d678376d136ecf871fdb945be660ece50f09a5d700d0fe4078e72b8a 1554 walk_referenced_types_visits_shared_child_once
    #[test]
    fn walk_referenced_types_visits_shared_child_once() {
        let db = TypeInterner::new();
        let child = db.object(vec![]);
        let root = db.tuple(vec![TupleElement::fixed(child), TupleElement::fixed(child)]);
        let mut visits = Vec::new();

        walk_referenced_types(&db, root, |type_id| visits.push(type_id));

        assert_eq!(
            visits.iter().filter(|&&type_id| type_id == child).count(),
            1
        );
        assert!(visits.contains(&root));
    }
// TSZ_INLINE_TEST_END af742ad4d678376d136ecf871fdb945be660ece50f09a5d700d0fe4078e72b8a

// TSZ_INLINE_TEST_BEGIN 920b57a7d8a53daaf02970a9b69f21d1d19a4362c530f7c13da24dc3fa4b9834 1578 bare_lazy_yields_its_def
    #[test]
    fn bare_lazy_yields_its_def() {
        let db = TypeInterner::new();
        let lazy = db.lazy(DefId(11));
        assert_eq!(union_of_bare_lazy_def_ids(&db, lazy), Some(vec![DefId(11)]));
    }
// TSZ_INLINE_TEST_END 920b57a7d8a53daaf02970a9b69f21d1d19a4362c530f7c13da24dc3fa4b9834

// TSZ_INLINE_TEST_BEGIN d5379589659b864dd71d71a9d3bacdd561484a2f1fa432458659a035afb379a5 1585 union_of_lazies_and_intrinsics_yields_the_lazy_defs
    #[test]
    fn union_of_lazies_and_intrinsics_yields_the_lazy_defs() {
        let db = TypeInterner::new();
        // `Lazy(11) | Lazy(12) | null` — the `getElementById`-style return shape.
        let u = db.union(vec![db.lazy(DefId(11)), db.lazy(DefId(12)), TypeId::NULL]);
        let got = union_of_bare_lazy_def_ids(&db, u).expect("union of bare lazies is classified");
        assert!(got.contains(&DefId(11)) && got.contains(&DefId(12)) && got.len() == 2);
    }
// TSZ_INLINE_TEST_END d5379589659b864dd71d71a9d3bacdd561484a2f1fa432458659a035afb379a5

// TSZ_INLINE_TEST_BEGIN f4e3df3d8764dc23e9bd431396777f17fc29bed6bdd061ef88ec2dca7f92c164 1594 pure_intrinsic_is_classified_with_no_defs
    #[test]
    fn pure_intrinsic_is_classified_with_no_defs() {
        let db = TypeInterner::new();
        // Deferrable-shape-wise valid, but the caller requires a non-empty set.
        assert_eq!(
            union_of_bare_lazy_def_ids(&db, TypeId::STRING),
            Some(vec![])
        );
    }
// TSZ_INLINE_TEST_END f4e3df3d8764dc23e9bd431396777f17fc29bed6bdd061ef88ec2dca7f92c164

// TSZ_INLINE_TEST_BEGIN bf6cac3c0c74ec7eb9244e0424887d50e7c2432e02d07a5b967d9eea82527fa1 1604 application_is_not_classified
    #[test]
    fn application_is_not_classified() {
        let db = TypeInterner::new();
        // `Lazy(11)<string>` (e.g. `Promise<string>`) is resolution-dependent.
        let app = db.application(db.lazy(DefId(11)), vec![TypeId::STRING]);
        assert_eq!(union_of_bare_lazy_def_ids(&db, app), None);
    }
// TSZ_INLINE_TEST_END bf6cac3c0c74ec7eb9244e0424887d50e7c2432e02d07a5b967d9eea82527fa1

// TSZ_INLINE_TEST_BEGIN 2dad499a85f21ebc7d7e3742d0596e20029bea24fcb9b735d4eba949a862e664 1612 union_containing_a_non_bare_member_is_not_classified
    #[test]
    fn union_containing_a_non_bare_member_is_not_classified() {
        let db = TypeInterner::new();
        let app = db.application(db.lazy(DefId(12)), vec![TypeId::NUMBER]);
        let u = db.union(vec![db.lazy(DefId(11)), app]);
        assert_eq!(union_of_bare_lazy_def_ids(&db, u), None);
    }
// TSZ_INLINE_TEST_END 2dad499a85f21ebc7d7e3742d0596e20029bea24fcb9b735d4eba949a862e664

// TSZ_INLINE_TEST_BEGIN bceae13893308c2cfbc42cde640a6128c0bb5264517326c05d8bbd4a03d10b2f 1620 function_type_is_not_classified
    #[test]
    fn function_type_is_not_classified() {
        let db = TypeInterner::new();
        let func = db.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: db.lazy(DefId(11)),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        assert_eq!(union_of_bare_lazy_def_ids(&db, func), None);
    }
// TSZ_INLINE_TEST_END bceae13893308c2cfbc42cde640a6128c0bb5264517326c05d8bbd4a03d10b2f
