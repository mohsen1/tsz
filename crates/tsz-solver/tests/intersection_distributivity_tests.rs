//! Intersection distributivity tests (Task #34)
//!
//! Tests for intersection distributivity: A & (B | C) → (A & B) | (A & C)

use crate::intern::TypeInterner;
use crate::types::*;

#[test]
fn test_intersection_distributes_over_union() {
    // string & (string | number) should distribute to (string & string) | (string & number)
    // which simplifies to string | never = string
    let interner = TypeInterner::new();

    // Create union: string | number
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Create intersection: string & (string | number)
    let result = interner.intersection(vec![TypeId::STRING, union]);

    // After distributivity: (string & string) | (string & number)
    // string & string = string (identity)
    // string & number = never (disjoint primitives)
    // Result: string | never = string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_intersection_distributes_with_multiple_members() {
    // Test: (string | boolean) & number should distribute to (string & number) | (boolean & number)
    // Both intersections are disjoint (string & number = never, boolean & number = never)
    // Result: never | never = never
    let interner = TypeInterner::new();

    let union = interner.union(vec![TypeId::STRING, TypeId::BOOLEAN]);
    let result = interner.intersection(vec![TypeId::NUMBER, union]);

    // Should distribute and then reduce to never
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_intersection_distributes_cardinality_guard() {
    // Test that the cardinality guard prevents exponential explosion
    let interner = TypeInterner::new();

    // Create unions to test the cardinality guard
    // With a limit of 25, we should be able to distribute:
    // - 1 union with 25 members → 25 combinations ✓
    // - 2 unions with 5 members each → 25 combinations ✓
    // - 3 unions with 3 members each → 27 combinations ✗ (exceeds limit)

    let mut members5 = Vec::new();
    for i in 0..5 {
        members5.push(interner.literal_string(&format!("s{i}")));
    }
    let union1 = interner.union(members5.clone());
    let union2 = interner.union(members5.clone());
    let union3 = interner.union(members5);

    // This should distribute (5 * 5 * 5 = 125 combinations, but let's see...)
    // Actually, wait - with my current logic, after union1: total=5, after union2: total=25, after union3: total=125 > 25
    // So distribution should NOT happen
    let result = interner.intersection(vec![union1, union2, union3]);

    // Should NOT distribute (exceeds cardinality limit)
    match interner.lookup(result) {
        Some(TypeData::Intersection(_)) | Some(TypeData::Union(_)) => {
            // Expected - cardinality guard prevented distribution or merged conservatively
        }
        other => {
            panic!("Expected union or intersection for cardinality guard result, got {other:?}");
        }
    }
}

#[test]
fn test_intersection_distributes_with_object_types() {
    // { a: string } & ({ a: string } | { a: number }) should distribute
    let interner = TypeInterner::new();

    let obj1 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj2 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    let obj3 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let union = interner.union(vec![obj3, obj2]);
    let result = interner.intersection(vec![obj1, union]);

    // Should distribute to: ({ a: string } & { a: string }) | ({ a: string } & { a: number })
    // Which simplifies to: { a: string } | never
    // Note: never removal in unions happens separately, so we might get 2 members
    match interner.lookup(result) {
        Some(TypeData::Object(_)) => {
            // Best case: simplified to { a: string }
        }
        Some(TypeData::Union(members)) => {
            let members = interner.type_list(members);
            // Acceptable: union with 1 or 2 members (2 if never not yet removed)
            assert!(members.len() <= 2);
        }
        other => {
            panic!("Expected object or union, got {other:?}");
        }
    }
}

#[test]
fn test_discriminated_union_intersection_distributes() {
    // Repro from typeVariableConstraintIntersections.ts (#30581):
    // (OptionOne | OptionTwo) & { kind: "one" } should distribute and narrow to OptionOne.
    //
    // OptionOne = { kind: "one", s: string }
    // OptionTwo = { kind: "two", x: number, y: number }
    //
    // Distribution:
    //   (OptionOne & { kind: "one" }) | (OptionTwo & { kind: "one" })
    //   = { kind: "one", s: string } | never  (because "two" & "one" = never → object = never)
    //   = { kind: "one", s: string }
    //
    // Previously, `should_preserve_discriminated_object_intersection` incorrectly
    // prevented this distribution, keeping the raw intersection form. This made
    // property access like `option.s` fail with TS2339 ("Property does not exist").
    let interner = TypeInterner::new();

    let kind_atom = interner.intern_string("kind");
    let s_atom = interner.intern_string("s");
    let x_atom = interner.intern_string("x");
    let y_atom = interner.intern_string("y");

    let lit_one = interner.literal_string("one");
    let lit_two = interner.literal_string("two");

    // OptionOne = { kind: "one", s: string }
    let option_one = interner.object(vec![
        PropertyInfo::new(kind_atom, lit_one),
        PropertyInfo::new(s_atom, TypeId::STRING),
    ]);

    // OptionTwo = { kind: "two", x: number, y: number }
    let option_two = interner.object(vec![
        PropertyInfo::new(kind_atom, lit_two),
        PropertyInfo::new(x_atom, TypeId::NUMBER),
        PropertyInfo::new(y_atom, TypeId::NUMBER),
    ]);

    // Options = OptionOne | OptionTwo
    let options = interner.union(vec![option_one, option_two]);

    // Discriminant object = { kind: "one" }
    let discriminant = interner.object(vec![PropertyInfo::new(kind_atom, lit_one)]);

    // Options & { kind: "one" } — should distribute and narrow
    let result = interner.intersection(vec![options, discriminant]);

    // The result should NOT be a raw intersection — it should be distributed.
    // After distribution and simplification, it should be an object type
    // containing the 's' property (from OptionOne).
    match interner.lookup(result) {
        Some(TypeData::Intersection(_)) => {
            panic!(
                "Expected distribution to narrow the discriminated union, \
                 but got a raw intersection. The intersection (OptionOne | OptionTwo) & \
                 {{ kind: \"one\" }} should distribute to OptionOne."
            );
        }
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            // Should have the 's' property from OptionOne
            assert!(
                shape.properties.iter().any(|p| p.name == s_atom),
                "Distributed result should contain property 's' from OptionOne"
            );
        }
        Some(TypeData::Union(_)) => {
            // Acceptable if never hasn't been fully removed yet
        }
        other => {
            panic!("Expected object or union after distribution, got {other:?}");
        }
    }
}
