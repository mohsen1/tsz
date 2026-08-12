//! Generic signature erasure helpers for the N×M overload comparison path.
//!
//! Split out of `functions/mod.rs` to stay under the file-size ratchet; these
//! functions have no dependency on `SubtypeChecker` and are pure functions of
//! an interner plus the signature data passed in.

use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::type_param_info;
use crate::types::{CallSignature, FunctionShape, ParamInfo, TypeId, TypeParamInfo};

/// Build a `TypeSubstitution` that maps each type parameter to its constraint
/// (or `unknown` if unconstrained). This corresponds to tsc's `getCanonicalSignature`
/// behavior — used when generic signatures need to be compared structurally after
/// erasing their type parameter identities.
pub(super) fn erase_type_params_to_constraints(type_params: &[TypeParamInfo]) -> TypeSubstitution {
    let mut sub = TypeSubstitution::for_signature_domain(type_params);
    for tp in type_params {
        sub.insert(tp.name, tp.constraint.unwrap_or(TypeId::UNKNOWN));
    }
    sub
}

/// Build a `TypeSubstitution` erasing to `any` every `TypeParameter`/`Infer`
/// that occurs *free* across `params`/`return_type` and is either this
/// signature's own declared `type_params` or one it merely references by
/// shared identity with the *paired* signature's declared `type_params` --
/// e.g. a contextually-typed function expression's parameter is seeded from a
/// target overload's own type parameter `TypeId` without the expression
/// carrying a `type_params` list of its own (#16952). tsc erases whichever
/// side of a `compareSignaturesRelated` pair is generic regardless of how
/// that genericity was introduced, so this keys off free occurrences rather
/// than the declared quantifier list alone.
///
/// A free occurrence that belongs to neither signature -- an outer/captured
/// type parameter from an enclosing generic function that both signatures
/// merely reference, e.g. `Args` in
/// `f<Args extends unknown[]>(source: (...a: Args) => void, target: (v: Args) => void)`
/// -- is not erasable: it names a single rigid type the caller controls, not
/// a per-signature generic marker, so it must stay opaque for the structural
/// comparison.
fn free_type_params_to_any(
    interner: &dyn crate::construction::TypeDatabase,
    params: &[ParamInfo],
    return_type: TypeId,
    own_type_params: &[TypeParamInfo],
    paired_type_params: &[TypeParamInfo],
) -> Option<TypeSubstitution> {
    // Fast path, and by far the common case: a signature that declares its
    // own type parameters erases exactly those by name, with no need to walk
    // the (potentially large) param/return type graph -- substituting by
    // name already reaches every occurrence of that declared parameter,
    // including any the paired signature happens to share by identity.
    if !own_type_params.is_empty() {
        let mut sub = TypeSubstitution::for_signature_domain(own_type_params);
        for tp in own_type_params {
            sub.insert(tp.name, TypeId::ANY);
        }
        return Some(sub);
    }
    if paired_type_params.is_empty() {
        return None;
    }
    // Narrow, contextual-identity-sharing case (#16952): this signature
    // declares no type parameters of its own, but its body may still
    // reference the *paired* signature's own type parameter by identity
    // (e.g. a contextually-typed function expression whose parameter was
    // seeded directly from a target overload's `T`). Only pay for the free-
    // occurrence walk here, where it is the sole way to detect that sharing.
    let free = crate::visitors::visitor_predicates::free_type_parameter_ids_in(
        interner,
        params
            .iter()
            .map(|p| p.type_id)
            .chain(std::iter::once(return_type)),
    );
    if free.is_empty() {
        return None;
    }
    // Match by `is_same_binder`, not a re-interned `TypeId` from
    // `paired_type_params`: a signature's own declared `TypeParamInfo` can
    // carry slightly different structural metadata than the `TypeParameter`
    // node actually occurring free in its body (the same #14345
    // declaration-scoped-intern gap `own_type_param_identity_ids` above
    // works around), so an exact `TypeId` comparison silently misses genuine
    // same-binder occurrences.
    let mut sub = TypeSubstitution::new();
    for id in free {
        let Some(info) = type_param_info(interner, id) else {
            continue;
        };
        if paired_type_params.iter().any(|tp| tp.is_same_binder(info)) {
            sub.insert(info.name, TypeId::ANY);
        }
    }
    if sub.is_empty() { None } else { Some(sub) }
}

/// Erase a call signature's type parameters to `any`, producing a non-generic
/// `FunctionShape`. Used by the N×M signature comparison path. `paired_type_params`
/// is the other side of this signature pair's own declared type parameters, so a
/// contextually shared identity (no `type_params` of its own but a free reference to
/// the paired signature's `T`) still erases; see [`free_type_params_to_any`].
pub(super) fn erase_call_sig_to_any(
    sig: &CallSignature,
    paired_type_params: &[TypeParamInfo],
    interner: &dyn crate::construction::TypeDatabase,
) -> FunctionShape {
    let Some(sub) = free_type_params_to_any(
        interner,
        &sig.params,
        sig.return_type,
        &sig.type_params,
        paired_type_params,
    ) else {
        return FunctionShape {
            type_params: Vec::new(),
            params: sig.params.clone(),
            this_type: sig.this_type,
            return_type: sig.return_type,
            type_predicate: sig.type_predicate,
            is_constructor: false,
            is_method: sig.is_method,
        };
    };
    let erased_params: Vec<_> = sig
        .params
        .iter()
        .map(|p| ParamInfo {
            suppress_display_optional: false,
            name: p.name,
            type_id: instantiate_type(interner, p.type_id, &sub),
            optional: p.optional,
            rest: p.rest,
        })
        .collect();
    FunctionShape {
        type_params: Vec::new(),
        params: erased_params,
        this_type: sig.this_type,
        return_type: instantiate_type(interner, sig.return_type, &sub),
        type_predicate: sig.type_predicate,
        is_constructor: false,
        is_method: sig.is_method,
    }
}

/// Erase a function shape's type parameters to `any`, producing a non-generic
/// `FunctionShape`. Used by the N×M signature comparison path. See
/// [`erase_call_sig_to_any`] for `paired_type_params`.
pub(super) fn erase_fn_shape_to_any(
    f: &FunctionShape,
    paired_type_params: &[TypeParamInfo],
    interner: &dyn crate::construction::TypeDatabase,
) -> FunctionShape {
    let Some(sub) = free_type_params_to_any(
        interner,
        &f.params,
        f.return_type,
        &f.type_params,
        paired_type_params,
    ) else {
        return f.clone();
    };
    let erased_params: Vec<_> = f
        .params
        .iter()
        .map(|p| ParamInfo {
            suppress_display_optional: false,
            name: p.name,
            type_id: instantiate_type(interner, p.type_id, &sub),
            optional: p.optional,
            rest: p.rest,
        })
        .collect();
    FunctionShape {
        type_params: Vec::new(),
        params: erased_params,
        this_type: f.this_type,
        return_type: instantiate_type(interner, f.return_type, &sub),
        type_predicate: f.type_predicate,
        is_constructor: f.is_constructor,
        is_method: f.is_method,
    }
}
