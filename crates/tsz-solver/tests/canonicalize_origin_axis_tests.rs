//! Canonical type-parameter identity must not fragment on the internal
//! [`TypeParamOrigin`](crate::types::TypeParamOrigin) discriminant — the
//! `origin` axis of the #13609 identity-fragmentation family.
//!
//! `origin` is a purely internal tsz classification (source-written generic vs
//! inference placeholder) that `tsc` never compares for type identity
//! (`compareTypeParametersIdentical` compares constraints only). Its inference
//! variants carry program-unique `id`s, and `TypeParamInfo` derives `Eq`/`Hash`
//! over `origin`, so two otherwise-identical *bound* signatures whose parameters
//! are inference placeholders minted at different ids fragmented into distinct
//! canonical `TypeId`s and missed the relation's reflexive short-circuit.
//!
//! These tests pin the fix: at a *bound* binding site (function / call
//! signature / mapped) the canonical form erases `origin` (alongside the name),
//! while a *free* reference — where an inference placeholder's `id` is its real
//! identity — keeps it.

use super::*;
use crate::intern::TypeInterner;
use crate::relations::subtype::TypeEnvironment;
use crate::types::{TypeData, TypeParamInfo, TypeParamOrigin};

/// `<T extends X>(x: T) => T` where the bound type parameter is a *source*
/// generic vs an *inference-source placeholder* must canonicalize to the SAME
/// identity: `origin` is identity-irrelevant for a positional (bound) parameter.
#[test]
fn bound_type_param_origin_is_alpha_equivalent_function() {
    use crate::types::{FunctionShape, ParamInfo};
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    // `<name>(x: name) => name` with the given origin/constraint.
    let make = |name: &str, origin: TypeParamOrigin, constraint: Option<TypeId>| {
        let info = TypeParamInfo {
            name: interner.intern_string(name),
            constraint,
            default: None,
            is_const: false,
            origin,
        };
        let pref = interner.type_param(info);
        interner.function(FunctionShape {
            type_params: vec![info],
            params: vec![ParamInfo {
                suppress_display_optional: false,
                name: Some(interner.intern_string("x")),
                type_id: pref,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: pref,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    // Vary the binder name as well so the assertion checks structure, not a
    // hard-coded spelling.
    let user = make("T", TypeParamOrigin::User, Some(TypeId::STRING));
    let placeholder = make(
        "Other",
        TypeParamOrigin::InferSource {
            id: 7,
            origin_name: Some(interner.intern_string("T")),
        },
        Some(TypeId::STRING),
    );

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(user),
        c2.canonicalize(placeholder),
        "a bound type parameter's origin discriminant is identity-irrelevant; \
         a source generic and an inference placeholder with the same constraint \
         are alpha-equivalent"
    );
}

/// Two *bound* parameters that are both inference placeholders but minted at
/// different program-unique ids — the higher-order re-generalized return-type
/// form — must collapse to one canonical identity (the ids are internal, never
/// part of type identity).
#[test]
fn bound_type_param_distinct_placeholder_ids_collapse() {
    use crate::types::{FunctionShape, ParamInfo};
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let make = |id: u64| {
        let info = TypeParamInfo {
            name: interner.intern_string("__infer_src"),
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::InferSource {
                id,
                origin_name: None,
            },
        };
        let pref = interner.type_param(info);
        interner.function(FunctionShape {
            type_params: vec![info],
            params: vec![ParamInfo {
                suppress_display_optional: false,
                name: Some(interner.intern_string("a")),
                type_id: pref,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: pref,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(make(1)),
        c2.canonicalize(make(99)),
        "bound inference placeholders differing only in their program-unique id \
         must share one canonical form"
    );
}

/// Negative control: erasing `origin` must NOT erase the constraint. Two bound
/// parameters with identical origin treatment but different constraints stay
/// distinct.
#[test]
fn bound_type_param_origin_drop_preserves_constraint_distinction() {
    use crate::types::{FunctionShape, ParamInfo};
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let make = |constraint: TypeId| {
        let info = TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: Some(constraint),
            default: None,
            is_const: false,
            origin: TypeParamOrigin::InferSource {
                id: 3,
                origin_name: None,
            },
        };
        let pref = interner.type_param(info);
        interner.function(FunctionShape {
            type_params: vec![info],
            params: vec![ParamInfo {
                suppress_display_optional: false,
                name: Some(interner.intern_string("x")),
                type_id: pref,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: pref,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_ne!(
        c1.canonicalize(make(TypeId::STRING)),
        c2.canonicalize(make(TypeId::NUMBER)),
        "dropping origin must not collapse parameters with different constraints"
    );
}

/// Call-signature path (`canonicalize_signature`, shared by `Callable`): the
/// bound `origin` axis collapses there too.
#[test]
fn bound_type_param_origin_alpha_equivalent_call_signature() {
    use crate::types::{CallSignature, CallableShape, ParamInfo};
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let make = |name: &str, origin: TypeParamOrigin| {
        let info = TypeParamInfo {
            name: interner.intern_string(name),
            constraint: None,
            default: None,
            is_const: false,
            origin,
        };
        let pref = interner.type_param(info);
        let sig = CallSignature {
            type_params: vec![info],
            params: vec![ParamInfo {
                suppress_display_optional: false,
                name: Some(interner.intern_string("x")),
                type_id: pref,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: pref,
            type_predicate: None,
            is_method: false,
        };
        interner.callable(CallableShape {
            call_signatures: vec![sig],
            construct_signatures: vec![],
            properties: vec![],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        })
    };

    let user = make("T", TypeParamOrigin::User);
    let placeholder = make("Q", TypeParamOrigin::InferPlaceholder { id: 42 });

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(user),
        c2.canonicalize(placeholder),
        "generic call signatures differing only in bound type-parameter origin \
         are alpha-equivalent"
    );
}

/// A mapped type's bound iteration variable also lives at a binding site, so its
/// `origin` is identity-irrelevant: `{ [K in T]: K }` over a source `T` and over
/// an inference-placeholder `T` (same constraint) canonicalize identically.
#[test]
fn bound_type_param_origin_alpha_equivalent_mapped() {
    use crate::types::MappedType;
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let make = |name: &str, origin: TypeParamOrigin| {
        let info = TypeParamInfo {
            name: interner.intern_string(name),
            constraint: None,
            default: None,
            is_const: false,
            origin,
        };
        let kref = interner.type_param(info);
        interner.mapped(MappedType {
            type_param: info,
            constraint: TypeId::STRING,
            template: kref,
            name_type: None,
            readonly_modifier: None,
            optional_modifier: None,
        })
    };

    let user = make("K", TypeParamOrigin::User);
    let placeholder = make("P", TypeParamOrigin::InferPlaceholder { id: 5 });

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(user),
        c2.canonicalize(placeholder),
        "a mapped type's bound iteration variable origin is identity-irrelevant"
    );
}

/// A *free* type-parameter reference is different: an inference placeholder's
/// program-unique `id` IS its identity, so `canonical_type_param` keeps `origin`.
/// Two free references sharing a name but carrying distinct placeholder ids must
/// stay distinct — the fix must not over-widen free references.
#[test]
fn free_inference_placeholder_ids_stay_distinct() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let name = interner.intern_string("__infer");
    let make = |id: u64| {
        interner.type_param(TypeParamInfo {
            name,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::InferPlaceholder { id },
        })
    };

    // No surrounding binding scope: these are free references.
    let a = make(1);
    let b = make(2);

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    let ca = c1.canonicalize(a);
    let cb = c2.canonicalize(b);

    // Both remain free `TypeParameter` references (not rewritten to a bound index).
    assert!(
        matches!(interner.lookup(ca), Some(TypeData::TypeParameter(_))),
        "a free reference stays a free TypeParameter"
    );
    assert_ne!(
        ca, cb,
        "free inference placeholders with distinct ids must keep distinct identity \
         (origin id is load-bearing for a free reference)"
    );
}
