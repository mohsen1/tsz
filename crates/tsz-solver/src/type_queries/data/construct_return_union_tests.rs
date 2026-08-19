//! Unit tests for [`get_construct_return_type_union`] (via the public
//! [`construct_return_type_for_type`] entry point).
//!
//! These pin `tsc`'s `getConstructorsForTypeArguments` rule with zero supplied
//! type arguments: a construct signature contributes to the instance type only
//! when it is applicable with no explicit type arguments (`minTypeArgumentCount`
//! is 0). A `MapConstructor`-shaped value — a non-generic `new (): T` overload
//! plus a generic `new <K, V>(): U` overload — must resolve to `T` alone, never
//! the spurious `T | U` union that leaks the generic overload's free type
//! parameters (the #15248 `class DraftMap extends Map` false-positive TS2416).

use crate::intern::TypeInterner;
use crate::type_queries::construct_return_type_for_type;
use crate::types::{CallSignature, CallableShape, TypeId, TypeParamInfo};

/// Build a construct signature with the given type parameters and return type
/// (no value parameters — this suite exercises only the type-argument arity).
fn construct_sig(type_params: Vec<TypeParamInfo>, return_type: TypeId) -> CallSignature {
    CallSignature {
        type_params,
        params: vec![],
        this_type: None,
        return_type,
        type_predicate: None,
        is_method: false,
        declaration_group: 0,
    }
}

/// Intern a constructor value from a set of construct signatures.
fn constructor_value(interner: &TypeInterner, construct_signatures: Vec<CallSignature>) -> TypeId {
    interner.callable(CallableShape {
        call_signatures: vec![],
        construct_signatures,
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    })
}

#[test]
fn drops_uninstantiated_generic_signature() {
    let interner = TypeInterner::new();

    // A `MapConstructor`-shaped value: a non-generic `new (): string` overload
    // alongside a generic `new <Key, Val>(): Key` overload whose type parameters
    // are required (no defaults). With zero explicit type arguments only the
    // non-generic overload is applicable, so the generic overload's free type
    // parameter must not leak into the instance type as a spurious union member.
    let key = interner.intern_string("Key");
    let val = interner.intern_string("Val");
    let key_param_ty = interner.type_param(TypeParamInfo::simple(key));
    let callable = constructor_value(
        &interner,
        vec![
            construct_sig(vec![], TypeId::STRING),
            construct_sig(
                vec![TypeParamInfo::simple(key), TypeParamInfo::simple(val)],
                key_param_ty,
            ),
        ],
    );

    assert_eq!(
        construct_return_type_for_type(&interner, callable),
        Some(TypeId::STRING),
        "the uninstantiated generic construct signature must be filtered out, \
         not unioned into the instance type"
    );
}

#[test]
fn keeps_fully_defaulted_generic_signature() {
    let interner = TypeInterner::new();

    // `new <Elem = number>(): number` — the sole type parameter has a default, so
    // its `minTypeArgumentCount` is 0 and the signature is applicable with zero
    // explicit type arguments (matching `tsc`'s `getConstructorsForTypeArguments`).
    let elem = interner.intern_string("Elem");
    let elem_param = TypeParamInfo {
        default: Some(TypeId::NUMBER),
        ..TypeParamInfo::simple(elem)
    };
    let callable = constructor_value(
        &interner,
        vec![construct_sig(vec![elem_param], TypeId::NUMBER)],
    );

    assert_eq!(
        construct_return_type_for_type(&interner, callable),
        Some(TypeId::NUMBER)
    );
}

#[test]
fn falls_back_when_every_signature_is_generic() {
    let interner = TypeInterner::new();

    // Only a required-type-parameter construct signature exists. With no
    // applicable (zero-min-type-arg) overload, the helper still surfaces an
    // instance type rather than collapsing to `None`.
    let item = interner.intern_string("Item");
    let item_param_ty = interner.type_param(TypeParamInfo::simple(item));
    let callable = constructor_value(
        &interner,
        vec![construct_sig(
            vec![TypeParamInfo::simple(item)],
            item_param_ty,
        )],
    );

    assert_eq!(
        construct_return_type_for_type(&interner, callable),
        Some(item_param_ty)
    );
}
