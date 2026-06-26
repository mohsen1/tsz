//! Regression tests: a homomorphic mapped type `{ [K in keyof T]: ... }` must
//! preserve the source **index signature**'s `readonly` modifier, and a bare
//! numeric-index object must not be reshaped into an array (issue #10822).
//!
//! Structural rule under test: when a homomorphic mapped type has no explicit
//! `readonly` directive, the `readonly` flag of the source's `string_index` /
//! `number_index` slot propagates onto the result's index signature — exactly
//! as it does for named properties. `+readonly` always adds it, `-readonly`
//! always strips it. A readonly numeric index signature alone does NOT make a
//! type an array, so `{ readonly [k: number]: V }` maps to an object with a
//! readonly numeric index signature, never to a `readonly V[]`.

use crate::evaluation::evaluate::evaluate_type;
use crate::intern::TypeInterner;
use crate::types::{
    IndexSignature, MappedModifier, MappedType, ObjectFlags, ObjectShape, TypeData, TypeId,
    TypeParamInfo,
};

/// Build a single-slot object with an index signature on `key_type`.
fn index_object(
    interner: &TypeInterner,
    key_type: TypeId,
    value_type: TypeId,
    readonly: bool,
) -> TypeId {
    let index = IndexSignature {
        key_type,
        value_type,
        readonly,
        param_name: None,
    };
    let (string_index, number_index) = if key_type == TypeId::NUMBER {
        (None, Some(index))
    } else {
        (Some(index), None)
    };
    interner.object_with_index(ObjectShape {
        base_types: Vec::new(),
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index,
        number_index,
        symbol_index: None,
        symbol: None,
    })
}

/// Evaluate `{ <readonly_modifier> [K in keyof source]: source[K] }`.
fn eval_identity_mapped(
    interner: &TypeInterner,
    source: TypeId,
    readonly_modifier: Option<MappedModifier>,
) -> TypeId {
    let keyof_source = interner.keyof(source);
    let type_param_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info));
    let template = interner.index_access(source, type_param);
    let mapped = MappedType {
        type_param: type_param_info,
        constraint: keyof_source,
        name_type: None,
        template,
        optional_modifier: None,
        readonly_modifier,
    };
    evaluate_type(interner, interner.mapped(mapped))
}

/// Pull the string index signature out of an object/object-with-index result.
fn string_index_of(interner: &TypeInterner, result: TypeId) -> IndexSignature {
    match interner.lookup(result) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => interner
            .object_shape(shape_id)
            .string_index
            .expect("result should carry a string index signature"),
        other => panic!("expected object with string index, got {other:?}"),
    }
}

#[test]
fn homomorphic_mapped_preserves_readonly_string_index() {
    // type S = { readonly [k: string]: number };
    // type R = { [K in keyof S]: S[K] };  // R must keep the readonly index.
    let interner = TypeInterner::new();
    let source = index_object(&interner, TypeId::STRING, TypeId::NUMBER, true);
    let result = eval_identity_mapped(&interner, source, None);
    assert!(
        string_index_of(&interner, result).readonly,
        "homomorphic identity map must preserve the source readonly string index signature"
    );
}

#[test]
fn homomorphic_mapped_keeps_writable_string_index_writable() {
    // A mutable source index signature must stay mutable through the map.
    let interner = TypeInterner::new();
    let source = index_object(&interner, TypeId::STRING, TypeId::NUMBER, false);
    let result = eval_identity_mapped(&interner, source, None);
    assert!(
        !string_index_of(&interner, result).readonly,
        "mapping a writable index signature must not synthesize a readonly one"
    );
}

#[test]
fn add_readonly_modifier_sets_string_index_readonly() {
    // `{ readonly [K in keyof S]: S[K] }` adds readonly to a writable source.
    let interner = TypeInterner::new();
    let source = index_object(&interner, TypeId::STRING, TypeId::NUMBER, false);
    let result = eval_identity_mapped(&interner, source, Some(MappedModifier::Add));
    assert!(
        string_index_of(&interner, result).readonly,
        "+readonly must add the readonly modifier to the result index signature"
    );
}

#[test]
fn remove_readonly_modifier_clears_string_index_readonly() {
    // `{ -readonly [K in keyof S]: S[K] }` strips readonly from a readonly source.
    let interner = TypeInterner::new();
    let source = index_object(&interner, TypeId::STRING, TypeId::NUMBER, true);
    let result = eval_identity_mapped(&interner, source, Some(MappedModifier::Remove));
    assert!(
        !string_index_of(&interner, result).readonly,
        "-readonly must strip the readonly modifier from the result index signature"
    );
}

#[test]
fn bare_readonly_numeric_index_object_is_not_reshaped_to_array() {
    // type S = { readonly [k: number]: string };
    // type R = { [K in keyof S]: S[K] };
    // A readonly numeric index signature alone is NOT an array: the result must
    // stay an object carrying a readonly numeric index signature, not become a
    // `readonly string[]`.
    let interner = TypeInterner::new();
    let source = index_object(&interner, TypeId::NUMBER, TypeId::STRING, true);
    let result = eval_identity_mapped(&interner, source, None);
    match interner.lookup(result) {
        Some(TypeData::ObjectWithIndex(shape_id)) => {
            let number_index = interner
                .object_shape(shape_id)
                .number_index
                .expect("result should keep a numeric index signature");
            assert!(
                number_index.readonly,
                "homomorphic map must preserve the source readonly numeric index signature"
            );
        }
        Some(TypeData::Array(_)) => {
            panic!("bare numeric-index object must not be reshaped into an array")
        }
        other => panic!("expected object with numeric index signature, got {other:?}"),
    }
}
