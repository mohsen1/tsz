//! Tests for index signature matching in subtype checking.

use super::*;
use crate::TypeInterner;
// =============================================================================
// Index Signature Subtyping Tests
// =============================================================================

#[test]
fn test_string_index_to_string_index() {
    let interner = TypeInterner::new();

    // { [key: string]: number } <: { [key: string]: number }
    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(is_subtype_of(&interner, source, target));
}

#[test]
fn test_string_index_covariant_value() {
    let interner = TypeInterner::new();

    // { [key: string]: "hello" } <: { [key: string]: string }
    let hello = interner.literal_string("hello");

    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: hello,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(is_subtype_of(&interner, source, target));
}

#[test]
fn test_string_index_not_subtype_incompatible_value() {
    let interner = TypeInterner::new();

    // { [key: string]: string } NOT <: { [key: string]: number }
    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(!is_subtype_of(&interner, source, target));
}

#[test]
fn test_object_with_props_to_index_signature() {
    let interner = TypeInterner::new();

    // { foo: number, bar: number } <: { [key: string]: number }
    let source = interner.object(vec![
        PropertyInfo::new(interner.intern_string("foo"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("bar"), TypeId::NUMBER),
    ]);

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(is_subtype_of(&interner, source, target));
}

#[test]
fn test_object_with_incompatible_props_not_subtype() {
    let interner = TypeInterner::new();

    // { foo: string, bar: number } NOT <: { [key: string]: number }
    let source = interner.object(vec![
        PropertyInfo::new(interner.intern_string("foo"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("bar"), TypeId::NUMBER),
    ]);

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(!is_subtype_of(&interner, source, target));
}

#[test]
fn test_index_with_props_to_simple_object() {
    let interner = TypeInterner::new();

    // { [key: string]: number, foo: number } <: { foo: number }
    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("foo"),
            TypeId::NUMBER,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let target = interner.object(vec![PropertyInfo::new(
        interner.intern_string("foo"),
        TypeId::NUMBER,
    )]);

    assert!(is_subtype_of(&interner, source, target));
}

#[test]
fn test_number_index_to_number_index() {
    let interner = TypeInterner::new();

    // { [key: number]: string } <: { [key: number]: string }
    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(is_subtype_of(&interner, source, target));
}

#[test]
fn test_string_and_number_index() {
    let interner = TypeInterner::new();

    // { [key: string]: number, [key: number]: number } <: { [key: string]: number }
    // Number index must be subtype of string index
    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(is_subtype_of(&interner, source, target));
}

#[test]
fn test_index_signature_with_named_property() {
    let interner = TypeInterner::new();

    // { [key: string]: number, length: number } <: { [key: string]: number, length: number }
    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("length"),
            TypeId::NUMBER,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("length"),
            TypeId::NUMBER,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(is_subtype_of(&interner, source, target));
}

#[test]
fn test_index_signature_source_property_mismatch() {
    let interner = TypeInterner::new();

    // { [key: string]: string, foo: number } NOT <: { [key: string]: string }
    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("foo"),
            TypeId::NUMBER,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(!is_subtype_of(&interner, source, target));
}

#[test]
fn test_number_index_signature_source_property_mismatch() {
    let interner = TypeInterner::new();

    // { [key: number]: number, "0": string } NOT <: { [key: number]: number }
    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("0"),
            TypeId::STRING,
        )],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(!is_subtype_of(&interner, source, target));
}

#[test]
fn test_empty_object_to_index_signature() {
    let interner = TypeInterner::new();

    // {} <: { [key: string]: number }
    let source = interner.object(vec![]);

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Empty object satisfies any index signature (no properties to violate it)
    assert!(is_subtype_of(&interner, source, target));
}

// =============================================================================
// classify_element_indexable: Union Preservation Tests
// =============================================================================

/// Verify that `classify_element_indexable` returns Union for union types,
/// even when one union member is structurally a subtype of another.
///
/// Regression test: `evaluate_type`'s union simplification was collapsing
/// `{ a: number } | { [s: string]: number }` into just the `ObjectWithIndex`
/// member, because the first member is a structural subtype. This broke
/// TS7053 detection which needs per-constituent indexability information.
#[test]
fn test_classify_element_indexable_preserves_union_members() {
    use crate::type_queries::{ElementIndexableKind, classify_element_indexable};

    let interner = TypeInterner::new();

    // Member 1: plain object { a: number } — no index signature
    let member1 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    // Member 2: object with string index { [s: string]: number }
    let member2 = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Create union: member1 | member2
    // Note: member1 is a structural subtype of member2 (every property is covered
    // by the string index signature). evaluate_type would collapse this union.
    let union_type = interner.union(vec![member1, member2]);

    // classify_element_indexable must preserve the Union variant so that
    // is_element_indexable can check each constituent independently.
    let kind = classify_element_indexable(&interner, union_type);
    match kind {
        ElementIndexableKind::Union(members) => {
            assert_eq!(members.len(), 2, "union should have 2 members");
        }
        other => {
            panic!(
                "expected ElementIndexableKind::Union, got {other:?}. \
                 Union was incorrectly collapsed by type evaluation."
            );
        }
    }
}

/// When the target has both string and number index signatures, an object with
/// only string-keyed properties (no numeric properties) should be assignable.
/// The number index is vacuously satisfied because the string index already
/// covers all keys and TypeScript requires `number_index_type <: string_index_type`.
///
/// Regression test for: `{ foo: fn } <: { [x: string]: T; [x: number]: T }`
/// failing with false TS2322/TS2345 when the source has no numeric properties.
#[test]
fn test_object_with_string_props_assignable_to_dual_index_target() {
    let interner = TypeInterner::new();

    // Source: { foo: string }
    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name: interner.intern_string("foo"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        }],
        string_index: None,
        number_index: None,
    });

    // Target: { [x: string]: string; [x: number]: string }
    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(
        is_subtype_of(&interner, source, target),
        "{{ foo: string }} should be assignable to {{ [x: string]: string; [x: number]: string }}"
    );
}

/// Same as above but the source has NO properties at all (empty object).
/// Empty objects should also be assignable to dual index targets.
#[test]
fn test_empty_object_assignable_to_dual_index_target() {
    let interner = TypeInterner::new();

    let source = interner.object(vec![]);

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(
        is_subtype_of(&interner, source, target),
        "Empty object should be assignable to {{ [x: string]: string; [x: number]: string }}"
    );
}

/// Regression for tsc parity: when the target has a string index signature whose
/// value type is `any`, the source need NOT declare a matching string/number
/// index signature -- even when the source is a class/interface (which would
/// normally require an explicit declared index signature).
///
/// This mirrors `indexSignaturesRelatedTo` short-circuit in tsc's checker.ts
/// (around line 24828):
///
///     const related = relation !== strictSubtypeRelation
///         && !sourceIsPrimitive
///         && targetHasStringIndex
///         && targetInfo.type.flags & TypeFlags.Any
///         ? Ternary.True : ...
///
/// Conformance test:
/// `tests/cases/conformance/types/members/objectTypeWithStringAndNumberIndexSignatureToAny.ts`
#[test]
fn test_named_source_assignable_to_string_index_any_target() {
    use tsz_binder::SymbolId;

    let interner = TypeInterner::new();
    let class_symbols = [crate::SymbolRef(7)];
    let is_class = |s: crate::SymbolRef| class_symbols.contains(&s);
    let mut checker = SubtypeChecker::new(&interner).with_class_check(&is_class);

    // Source: a class-like named type with only a number index, e.g. `class NumberTo<any> { [x: number]: any }`.
    let source = interner.object_with_index(ObjectShape {
        symbol: Some(SymbolId(7)),
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
    });

    // Target: anonymous `{ [x: string]: any }`.
    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(
        checker.is_subtype_of(source, target),
        "Named class with only a number index must be assignable to a target whose \
         string index value is `any` (tsc short-circuit)"
    );
}

/// When the target is a NAMED interface with both string and number indexes
/// that map to `any`, a source class/interface with NO index signatures (only
/// properties) should be assignable. Mirrors tsc behavior for
/// `interface StringAndNumberTo<any> extends StringTo<any>, NumberTo<any> {}`
/// where `Obj <: StringAndNumberTo<any>` succeeds.
///
/// The `target.symbol.is_some()` gate distinguishes a NAMED interface target
/// (where the indexes belong to a single declared interface) from the merged
/// intersection synthetic shape produced by our interner for
/// `StringTo<any> & NumberTo<any>` (where `target.symbol = None`). tsc keeps
/// intersection members separate; we eagerly merge them. The next test locks
/// in the merged-intersection rejection.
#[test]
fn test_named_source_with_props_assignable_to_dual_any_index_named_target() {
    use tsz_binder::SymbolId;

    let interner = TypeInterner::new();
    let class_symbols = [crate::SymbolRef(8)];
    let is_class = |s: crate::SymbolRef| class_symbols.contains(&s);
    let mut checker = SubtypeChecker::new(&interner).with_class_check(&is_class);

    let source = interner.object_with_flags_and_symbol(
        vec![
            PropertyInfo::new(interner.intern_string("hello"), TypeId::STRING),
            PropertyInfo::new(interner.intern_string("world"), TypeId::NUMBER),
        ],
        ObjectFlags::empty(),
        Some(SymbolId(8)),
    );

    // Target is a NAMED interface like `StringAndNumberTo<any>`.
    let target = interner.object_with_index(ObjectShape {
        symbol: Some(SymbolId(80)),
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(
        checker.is_subtype_of(source, target),
        "Named class with only properties must be assignable to a NAMED dual-index \
         target where both index values are `any` (tsc short-circuit)"
    );
}

/// Anonymous synthetic targets (e.g. shape produced by interner-side intersection
/// merging of `StringTo<any> & NumberTo<any>`) must NOT trigger the number-index
/// short-circuit. The shape has `target.symbol == None` and its two indexes
/// originated from distinct intersection members. tsc would relate the source
/// against each intersection member independently, and the `NumberTo<any>`
/// member alone (no string index) rejects a class/interface source without a
/// number index.
#[test]
fn test_anonymous_dual_any_index_target_still_rejects_named_source() {
    use tsz_binder::SymbolId;

    let interner = TypeInterner::new();
    let class_symbols = [crate::SymbolRef(81)];
    let is_class = |s: crate::SymbolRef| class_symbols.contains(&s);
    let mut checker = SubtypeChecker::new(&interner).with_class_check(&is_class);

    let source = interner.object_with_flags_and_symbol(
        vec![
            PropertyInfo::new(interner.intern_string("hello"), TypeId::STRING),
            PropertyInfo::new(interner.intern_string("world"), TypeId::NUMBER),
        ],
        ObjectFlags::empty(),
        Some(SymbolId(81)),
    );

    // Anonymous merged-intersection shape (target.symbol = None).
    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(
        !checker.is_subtype_of(source, target),
        "Named class without numeric members must NOT satisfy an anonymous synthetic \
         dual-index `any` target (preserves tsc's per-member intersection behavior)"
    );
}

/// The "string-index-any" short-circuit must NOT apply when the target lacks a
/// string index. A number-only `{ [n: number]: any }` target still requires the
/// source to provide a numeric-compatible index signature or properties. A
/// named class/interface source with NO numeric members must continue to be
/// rejected.
#[test]
fn test_named_source_still_rejected_by_number_only_any_target() {
    use tsz_binder::SymbolId;

    let interner = TypeInterner::new();
    let class_symbols = [crate::SymbolRef(9)];
    let is_class = |s: crate::SymbolRef| class_symbols.contains(&s);
    let mut checker = SubtypeChecker::new(&interner).with_class_check(&is_class);

    let source = interner.object_with_flags_and_symbol(
        vec![
            PropertyInfo::new(interner.intern_string("hello"), TypeId::STRING),
            PropertyInfo::new(interner.intern_string("world"), TypeId::NUMBER),
        ],
        ObjectFlags::empty(),
        Some(SymbolId(9)),
    );

    // Target: `{ [n: number]: any }` -- number index only, NO string index.
    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(
        !checker.is_subtype_of(source, target),
        "Named class without numeric members must NOT satisfy a number-only `any` index \
         target (the short-circuit only applies when target also has a string index)"
    );
}
