//! Canonical queries for "is this type the global `Object`/`Function` interface".
//!
//! Before this module existed, seven hand-rolled structural sniffs across the
//! solver (relations, evaluation, narrowing, operations) each re-derived the
//! answer from member-name probes ("has `toString` and `hasOwnProperty`", "has
//! `apply`/`call`/`bind`") with diverging property lists and count caps. Those
//! copies were load-order sensitive (they pass differently before vs after lib
//! materialization) and drifted semantically. See issue #13090.
//!
//! # Tiers
//!
//! 1. **Identity** ([`is_global_interface_by_identity`],
//!    [`is_global_interface_by_identity_with_resolver`]): compares against the
//!    boxed-type registry populated by the checker from binder/global builtin
//!    ids when lib is loaded (`register_boxed_types`). This tier is exact:
//!    a user interface that merely *looks like* `Object` never matches.
//!    **When lib is not loaded (`noLib`, or before lib materialization) the
//!    registry is empty and this tier returns `false`.**
//! 2. **Structural shape fallback** ([`matches_global_object_interface_shape`],
//!    [`matches_global_function_interface_shape`]): a single shared copy of
//!    the historical member-name probe. It exists because pre-evaluation can
//!    lower the lib interface to an `ObjectShape` whose `TypeId` differs from
//!    every registered boxed id (cross-arena declaration splitting,
//!    `get_type_of_symbol` vs `resolve_lib_type_by_name` producing distinct
//!    `TypeId`s). Callers should prefer the combined queries below, which try
//!    identity first; the structural tier is a documented compatibility
//!    fallback, not a precise identity test.
//!
//! The combined queries ([`is_global_object_interface`],
//! [`is_global_function_interface`] and their `_with_resolver` variants) are
//! the intended entry points: identity first, then the shared structural
//! fallback.

use crate::TypeId;
use crate::construction::TypeDatabase;
use crate::def::resolver::TypeResolver;
use crate::types::{IntrinsicKind, ObjectShape, ObjectShapeId};
use crate::visitor::{lazy_def_id, object_shape_id, object_with_index_shape_id};

/// The global `Object` interface declares exactly 7 properties
/// (`constructor`, `toString`, `toLocaleString`, `valueOf`, `hasOwnProperty`,
/// `isPrototypeOf`, `propertyIsEnumerable`). A tight cap avoids matching
/// derived lib interfaces like `Boolean` (8 props) or `Number` (~10 props).
const GLOBAL_OBJECT_INTERFACE_MAX_PROPERTIES: usize = 7;

/// The global `Function` interface has ~8 own properties plus ~7 inherited
/// `Object` properties (~15 when flattened). Cap at 20 to avoid false
/// positives on large unrelated interfaces.
const GLOBAL_FUNCTION_INTERFACE_MAX_PROPERTIES: usize = 20;

/// Identity tier: is `type_id` a registered form of the global interface for
/// `kind` (e.g. `IntrinsicKind::Object` / `IntrinsicKind::Function`)?
///
/// Checks the interner-backed boxed-type registry: the registered boxed
/// `TypeId` and `Lazy(DefId)` forms whose `DefId` is registered as boxed.
///
/// Returns `false` when lib is not loaded (the registry is empty), including
/// `noLib` compilations and any query issued before lib materialization. This
/// is intentional: identity for a lib global is undefined until the lib
/// declarations exist.
pub fn is_global_interface_by_identity(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    kind: IntrinsicKind,
) -> bool {
    if db.get_boxed_type(kind) == Some(type_id) {
        return true;
    }
    lazy_def_id(db, type_id).is_some_and(|def_id| db.is_boxed_def_id(def_id, kind))
}

/// Identity tier that also consults a [`TypeResolver`]'s boxed registry.
///
/// The resolver (`TypeEnvironment`) and the interner can hold independently
/// populated boxed registries (boxed types are registered on the interner
/// during lib processing, while a resolver instance may be created later or
/// be a different instance). Checking both keeps the answer stable across
/// call sites that historically consulted only one of the two.
pub fn is_global_interface_by_identity_with_resolver<R: TypeResolver + ?Sized>(
    db: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
    kind: IntrinsicKind,
) -> bool {
    if resolver.is_boxed_type_id(type_id, kind) || resolver.get_boxed_type(kind) == Some(type_id) {
        return true;
    }
    if lazy_def_id(db, type_id).is_some_and(|def_id| resolver.is_boxed_def_id(def_id, kind)) {
        return true;
    }
    is_global_interface_by_identity(db, type_id, kind)
}

/// Shape-level structural fallback for the global `Object` interface.
///
/// `true` when the shape has at most
/// [`GLOBAL_OBJECT_INTERFACE_MAX_PROPERTIES`] properties and declares
/// `constructor`, `hasOwnProperty`, `isPrototypeOf`, and
/// `propertyIsEnumerable`. This is the single shared copy of the historical
/// sniff; prefer [`is_global_object_interface`] which tries identity first.
pub fn object_shape_matches_global_object_interface(
    db: &dyn TypeDatabase,
    shape: &ObjectShape,
) -> bool {
    if shape.properties.len() > GLOBAL_OBJECT_INTERFACE_MAX_PROPERTIES {
        return false;
    }
    let constructor = db.intern_string("constructor");
    let has_own = db.intern_string("hasOwnProperty");
    let is_proto = db.intern_string("isPrototypeOf");
    let prop_is_enum = db.intern_string("propertyIsEnumerable");
    shape.properties.iter().any(|p| p.name == constructor)
        && shape.properties.iter().any(|p| p.name == has_own)
        && shape.properties.iter().any(|p| p.name == is_proto)
        && shape.properties.iter().any(|p| p.name == prop_is_enum)
}

fn object_like_shape_id(db: &dyn TypeDatabase, type_id: TypeId) -> Option<ObjectShapeId> {
    if type_id.is_intrinsic() {
        return None;
    }
    object_shape_id(db, type_id).or_else(|| object_with_index_shape_id(db, type_id))
}

/// Structural fallback for the global `Object` interface on a `TypeId`.
///
/// See [`object_shape_matches_global_object_interface`]. Returns `false` for
/// intrinsics and non-object types.
pub fn matches_global_object_interface_shape(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    object_like_shape_id(db, type_id).is_some_and(|shape_id| {
        let shape = db.object_shape(shape_id);
        object_shape_matches_global_object_interface(db, &shape)
    })
}

/// Structural fallback for the global `Function` interface on a `TypeId`.
///
/// `true` when the type is an object shape with at most
/// [`GLOBAL_FUNCTION_INTERFACE_MAX_PROPERTIES`] properties declaring `apply`,
/// `call`, and `bind`. This is the single shared copy of the historical
/// sniff; prefer [`is_global_function_interface`] which tries identity first.
pub fn matches_global_function_interface_shape(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    object_like_shape_id(db, type_id).is_some_and(|shape_id| {
        let shape = db.object_shape(shape_id);
        if shape.properties.len() > GLOBAL_FUNCTION_INTERFACE_MAX_PROPERTIES {
            return false;
        }
        let apply = db.intern_string("apply");
        let call = db.intern_string("call");
        let bind = db.intern_string("bind");
        shape.properties.iter().any(|p| p.name == apply)
            && shape.properties.iter().any(|p| p.name == call)
            && shape.properties.iter().any(|p| p.name == bind)
    })
}

/// Is `type_id` the global `Object` interface? Identity first, then the
/// shared structural fallback.
pub fn is_global_object_interface(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_global_interface_by_identity(db, type_id, IntrinsicKind::Object)
        || matches_global_object_interface_shape(db, type_id)
}

/// Is `type_id` the global `Function` interface? Identity first, then the
/// shared structural fallback.
pub fn is_global_function_interface(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_global_interface_by_identity(db, type_id, IntrinsicKind::Function)
        || matches_global_function_interface_shape(db, type_id)
}

/// Resolver-aware variant of [`is_global_object_interface`].
pub fn is_global_object_interface_with_resolver<R: TypeResolver + ?Sized>(
    db: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
) -> bool {
    is_global_interface_by_identity_with_resolver(db, resolver, type_id, IntrinsicKind::Object)
        || matches_global_object_interface_shape(db, type_id)
}

/// Resolver-aware variant of [`is_global_function_interface`].
pub fn is_global_function_interface_with_resolver<R: TypeResolver + ?Sized>(
    db: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
) -> bool {
    is_global_interface_by_identity_with_resolver(db, resolver, type_id, IntrinsicKind::Function)
        || matches_global_function_interface_shape(db, type_id)
}
