//! Regression tests: a homomorphic mapped type `{ [K in keyof T]: ... }`
//! instantiated over a `readonly` array/tuple source must copy the source's
//! `readonly` modifier onto its result (issue #9651).
//!
//! Structural rule under test: when a homomorphic mapped type has no explicit
//! `readonly` modifier and its source resolves to a `readonly` array/tuple,
//! the instantiated result is `readonly`; `+readonly` always adds it,
//! `-readonly` always strips it, and a mutable source stays mutable.

use super::*;
use crate::construction::TypeInterner;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::types::{MappedModifier, TypeData};

/// Build `{ <modifier> [ITER in keyof T]: TEMPLATE }` where `TEMPLATE` is the
/// identity `T[ITER]` (when `wrap` is false) or the element-wrapping `[T[ITER]]`
/// (when `wrap` is true), then instantiate it with `T := source`.
fn instantiate_homomorphic(
    interner: &TypeInterner,
    iter_name: &str,
    readonly_modifier: Option<MappedModifier>,
    wrap: bool,
    source: TypeId,
) -> TypeId {
    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let keyof_t = interner.keyof(t_type);

    let iter_param = TypeParamInfo {
        name: interner.intern_string(iter_name),
        constraint: Some(keyof_t),
        default: None,
        is_const: false,
    };
    let iter_type = interner.intern(TypeData::TypeParameter(iter_param));
    let index_access = interner.index_access(t_type, iter_type);
    let template = if wrap {
        interner.tuple(vec![TupleElement {
            type_id: index_access,
            name: None,
            optional: false,
            rest: false,
        }])
    } else {
        index_access
    };

    let mapped = interner.mapped(MappedType {
        type_param: iter_param,
        constraint: keyof_t,
        name_type: None,
        template,
        readonly_modifier,
        optional_modifier: None,
    });

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, source);
    instantiate_type(interner, mapped, &subst)
}

fn is_readonly(interner: &TypeInterner, type_id: TypeId) -> bool {
    matches!(interner.lookup(type_id), Some(TypeData::ReadonlyType(_)))
}

fn lit_tuple(interner: &TypeInterner, values: &[f64]) -> Vec<TupleElement> {
    values
        .iter()
        .map(|v| TupleElement {
            type_id: interner.literal_number(*v),
            name: None,
            optional: false,
            rest: false,
        })
        .collect()
}

#[test]
fn readonly_tuple_identity_preserves_readonly() {
    let interner = TypeInterner::new();
    let source = interner.readonly_tuple(lit_tuple(&interner, &[1.0, 2.0]));
    let result = instantiate_homomorphic(&interner, "K", None, false, source);
    assert!(
        is_readonly(&interner, result),
        "homomorphic identity over `readonly [1, 2]` must stay readonly"
    );
}

#[test]
fn readonly_tuple_element_wrap_preserves_readonly() {
    let interner = TypeInterner::new();
    let source = interner.readonly_tuple(lit_tuple(&interner, &[1.0, 2.0]));
    let result = instantiate_homomorphic(&interner, "K", None, true, source);
    assert!(
        is_readonly(&interner, result),
        "homomorphic `[T[K]]` over `readonly [1, 2]` must stay readonly"
    );
}

#[test]
fn readonly_array_preserves_readonly() {
    let interner = TypeInterner::new();
    let source = interner.readonly_array(TypeId::NUMBER);
    let result = instantiate_homomorphic(&interner, "K", None, false, source);
    assert!(
        is_readonly(&interner, result),
        "homomorphic identity over `readonly number[]` must stay readonly"
    );
}

#[test]
fn renamed_iteration_variable_still_preserves_readonly() {
    // Uses `P` instead of `K`: the fix must be structural, not keyed to a name.
    let interner = TypeInterner::new();
    let source = interner.readonly_tuple(lit_tuple(&interner, &[1.0, 2.0]));
    let result = instantiate_homomorphic(&interner, "P", None, false, source);
    assert!(
        is_readonly(&interner, result),
        "renaming the iteration variable must not change readonly preservation"
    );
}

#[test]
fn mutable_tuple_stays_mutable() {
    let interner = TypeInterner::new();
    let source = interner.tuple(lit_tuple(&interner, &[1.0, 2.0]));
    let result = instantiate_homomorphic(&interner, "K", None, false, source);
    assert!(
        !is_readonly(&interner, result),
        "homomorphic identity over a mutable tuple must not add readonly"
    );
}

#[test]
fn mutable_array_stays_mutable() {
    let interner = TypeInterner::new();
    let source = interner.array(TypeId::NUMBER);
    let result = instantiate_homomorphic(&interner, "K", None, false, source);
    assert!(
        !is_readonly(&interner, result),
        "homomorphic identity over a mutable array must not add readonly"
    );
}

#[test]
fn explicit_add_readonly_over_mutable_tuple_adds_readonly() {
    let interner = TypeInterner::new();
    let source = interner.tuple(lit_tuple(&interner, &[1.0, 2.0]));
    let result = instantiate_homomorphic(&interner, "K", Some(MappedModifier::Add), false, source);
    assert!(
        is_readonly(&interner, result),
        "`+readonly` must add readonly even for a mutable source"
    );
}

#[test]
fn explicit_remove_readonly_over_readonly_tuple_strips_readonly() {
    let interner = TypeInterner::new();
    let source = interner.readonly_tuple(lit_tuple(&interner, &[1.0, 2.0]));
    let result =
        instantiate_homomorphic(&interner, "K", Some(MappedModifier::Remove), false, source);
    assert!(
        !is_readonly(&interner, result),
        "`-readonly` must strip readonly even for a readonly source"
    );
}

#[test]
fn explicit_remove_readonly_over_readonly_array_strips_readonly() {
    let interner = TypeInterner::new();
    let source = interner.readonly_array(TypeId::NUMBER);
    let result =
        instantiate_homomorphic(&interner, "K", Some(MappedModifier::Remove), false, source);
    assert!(
        !is_readonly(&interner, result),
        "`-readonly` must strip readonly from a readonly array source"
    );
}
