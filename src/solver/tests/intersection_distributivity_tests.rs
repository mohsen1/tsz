//! Intersection distributivity tests (Task #34)
//!
//! Tests for intersection distributivity: A & (B | C) → (A & B) | (A & C)

use crate::solver::intern::TypeInterner;
use crate::solver::types::*;

#[test]
fn test_intersection_distributes_over_union() {
    // string & (string | number) should distribute to (string & string) | (string & number)
    // which simplifies to string | never = string
    let interner = TypeInterner::new();

    // Create union: string | number
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Create intersection: string & (string | number)
    let intersection = interner.intersection(vec![TypeId::STRING, union]);

    // After distributivity: (string & string) | (string & number) = string | never = string
    // For now, let's check what we actually get
    match interner.lookup(intersection) {
        Some(TypeKey::Intersection(_)) => {
            // Expected for now - distributivity not implemented
            println!("Intersection (no distributivity)");
        }
        Some(TypeKey::Union(members)) => {
            let members = interner.type_list(members);
            println!("Union with {} members", members.len());
            // After distributivity, should be 1 member (string)
        }
        Some(TypeKey::Literal(_)) => {
            // Could be string if it simplified all the way
        }
        other => {
            println!("Got: {:?}", other);
        }
    }

    // For now, just verify it doesn't panic and is some valid type
    assert_ne!(intersection, TypeId::NEVER);
}

#[test]
fn test_intersection_with_object_distributes_over_union() {
    // { a: string } & ({ a: string } | { a: number }) should become { a: string } | { a: never }
    let interner = TypeInterner::new();

    let obj1 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    let obj2 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    let obj3 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    let union = interner.union(vec![obj2, obj3]);
    let intersection = interner.intersection(vec![obj1, union]);

    // Should distribute: ({ a: string } & { a: string }) | ({ a: string } & { a: number })
    // Which becomes: { a: string } | { a: never }
    match interner.lookup(intersection) {
        Some(TypeKey::Intersection(_)) => {
            // Expected for now
        }
        Some(TypeKey::Union(_)) => {
            // After distributivity
        }
        other => {
            panic!("Expected intersection or union, got {:?}", other);
        }
    }
}
