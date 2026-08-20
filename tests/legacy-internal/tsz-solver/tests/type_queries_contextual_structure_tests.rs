use super::*;
use crate::type_queries::evaluate_contextual_structure_with;

#[test]
fn evaluate_contextual_structure_distributes_over_union_members() {
    let db = TypeInterner::new();
    let lazy = db.lazy(DefId(42));
    let root = db.union(vec![TypeId::STRING, lazy]);

    let mut evaluated_leaf_count = 0usize;
    let evaluated = evaluate_contextual_structure_with(&db, root, &mut |leaf| {
        evaluated_leaf_count += 1;
        if leaf == lazy { TypeId::NUMBER } else { leaf }
    });

    assert_eq!(
        evaluated_leaf_count, 1,
        "only lazy leaf should be evaluated"
    );
    let members = crate::type_queries::get_union_members(&db, evaluated).expect("union expected");
    assert!(members.contains(&TypeId::STRING));
    assert!(members.contains(&TypeId::NUMBER));
}

#[test]
fn evaluate_contextual_structure_keeps_non_contextual_types_unchanged() {
    let db = TypeInterner::new();
    let root = db.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let mut evaluated_leaf_count = 0usize;
    let evaluated = evaluate_contextual_structure_with(&db, root, &mut |leaf| {
        evaluated_leaf_count += 1;
        leaf
    });

    assert_eq!(evaluated, root);
    assert_eq!(
        evaluated_leaf_count, 0,
        "no contextual leaf should be evaluated"
    );
}

#[test]
fn evaluate_contextual_structure_recurses_through_nested_unions_and_intersections() {
    let db = TypeInterner::new();
    let lazy = db.lazy(DefId(77));
    let nested = db.intersection(vec![lazy, TypeId::BOOLEAN]);
    let root = db.union(vec![TypeId::STRING, nested]);

    let mut evaluated_leaf_count = 0usize;
    let evaluated = evaluate_contextual_structure_with(&db, root, &mut |leaf| {
        evaluated_leaf_count += 1;
        if leaf == lazy { TypeId::NUMBER } else { leaf }
    });

    assert_eq!(
        evaluated_leaf_count, 1,
        "only nested lazy leaf should be evaluated"
    );
    assert_ne!(
        evaluated, root,
        "nested contextual leaves should trigger reconstructed outer structure"
    );
}
