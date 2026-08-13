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
use tsz_common::Atom;

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

/// Shape-level structural fallback for the global `Function` interface.
///
/// `true` when the shape has at most
/// [`GLOBAL_FUNCTION_INTERFACE_MAX_PROPERTIES`] properties and declares
/// `apply`, `call`, and `bind`. This is the single shared copy of the
/// historical sniff; prefer [`is_global_function_interface`] which tries
/// identity first.
///
/// When the real boxed `Function` interface is registered on `db` (true for
/// every compilation with a lib loaded), a shape must also declare no
/// property outside the boxed interface's own surface. Without that check, a
/// user interface that extends `Function` and adds a genuine data member
/// (`interface Foo extends Function { x: number }`) still carries
/// `apply`/`call`/`bind` from its heritage and fits comfortably under the
/// property-count cap, so the old apply/call/bind-only probe misidentified it
/// as `Function` itself — silencing the member check callers like
/// `core_dispatch`'s function-value compatibility bridge run for a target
/// they believe declares nothing beyond `Function`'s surface. `noLib`
/// compilations (no boxed `Function` to compare against) keep the historical
/// minimum-surface probe, since there is no ground truth to check against.
pub fn object_shape_matches_global_function_interface(
    db: &dyn TypeDatabase,
    shape: &ObjectShape,
) -> bool {
    if shape.properties.len() > GLOBAL_FUNCTION_INTERFACE_MAX_PROPERTIES {
        return false;
    }
    let apply = db.intern_string("apply");
    let call = db.intern_string("call");
    let bind = db.intern_string("bind");
    let has_minimum_surface = shape.properties.iter().any(|p| p.name == apply)
        && shape.properties.iter().any(|p| p.name == call)
        && shape.properties.iter().any(|p| p.name == bind);
    if !has_minimum_surface {
        return false;
    }
    match global_function_interface_own_property_names(db) {
        Some(reference_names) => shape
            .properties
            .iter()
            .all(|p| reference_names.contains(&p.name)),
        None => true,
    }
}

/// The boxed global `Function` interface's own declared property names, or
/// `None` when no boxed `Function` is registered (`noLib`) or its shape is
/// not (yet) an object shape.
fn global_function_interface_own_property_names(db: &dyn TypeDatabase) -> Option<Vec<Atom>> {
    let boxed = db.get_boxed_type(IntrinsicKind::Function)?;
    let shape_id = object_like_shape_id(db, boxed)?;
    let shape = db.object_shape(shape_id);
    Some(shape.properties.iter().map(|p| p.name).collect())
}

/// Structural fallback for the global `Function` interface on a `TypeId`.
///
/// See [`object_shape_matches_global_function_interface`]. Returns `false` for
/// intrinsics and non-object types.
pub fn matches_global_function_interface_shape(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    object_like_shape_id(db, type_id).is_some_and(|shape_id| {
        let shape = db.object_shape(shape_id);
        object_shape_matches_global_function_interface(db, &shape)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caches::db::QueryDatabase;
    use crate::construction::TypeInterner;
    use crate::def::DefId;
    use crate::def::resolver::TypeEnvironment;
    use crate::types::PropertyInfo;

    /// Intern an object type whose property list matches the global `Object`
    /// interface from lib.es5.d.ts (7 members), under arbitrary value types.
    fn object_like_shape(interner: &TypeInterner) -> TypeId {
        interner.object(
            [
                "constructor",
                "toString",
                "toLocaleString",
                "valueOf",
                "hasOwnProperty",
                "isPrototypeOf",
                "propertyIsEnumerable",
            ]
            .iter()
            .map(|name| PropertyInfo::new(interner.intern_string(name), TypeId::ANY))
            .collect(),
        )
    }

    fn function_like_shape(interner: &TypeInterner, with_bind: bool) -> TypeId {
        let mut names = vec!["apply", "call", "length", "arguments", "caller", "name"];
        if with_bind {
            names.push("bind");
        }
        interner.object(
            names
                .iter()
                .map(|name| PropertyInfo::new(interner.intern_string(name), TypeId::ANY))
                .collect(),
        )
    }

    /// Lib-not-loaded ordering / `noLib`: with empty boxed registries the
    /// identity tier must answer `false` for everything, including types that
    /// are structurally indistinguishable from the lib interface.
    #[test]
    fn identity_is_false_when_lib_not_loaded() {
        let interner = TypeInterner::new();
        let object_like = object_like_shape(&interner);
        let lazy = interner.lazy(DefId(7));
        for candidate in [object_like, lazy, TypeId::OBJECT, TypeId::FUNCTION] {
            assert!(!is_global_interface_by_identity(
                &interner,
                candidate,
                IntrinsicKind::Object
            ));
            assert!(!is_global_interface_by_identity(
                &interner,
                candidate,
                IntrinsicKind::Function
            ));
        }
        let env = TypeEnvironment::new();
        assert!(!is_global_interface_by_identity_with_resolver(
            &interner,
            &env,
            object_like,
            IntrinsicKind::Object
        ));
    }

    /// A user interface that is structurally identical to `Object` (renamed,
    /// e.g. `interface MyObjectLike { constructor: ...; hasOwnProperty: ...;
    /// ... }`) must NOT match the identity tier; only the registered boxed
    /// type does. The structural fallback intentionally still matches it —
    /// that is the documented compatibility hazard the identity tier exists
    /// to replace.
    #[test]
    fn renamed_object_shaped_interface_is_not_identity_matched() {
        let interner = TypeInterner::new();
        let user_iface = object_like_shape(&interner);
        // Register a DIFFERENT type as the real boxed Object.
        let real_object = interner.object(vec![PropertyInfo::new(
            interner.intern_string("toString"),
            TypeId::ANY,
        )]);
        interner.register_boxed_type(IntrinsicKind::Object, real_object);

        assert!(!is_global_interface_by_identity(
            &interner,
            user_iface,
            IntrinsicKind::Object
        ));
        assert!(is_global_interface_by_identity(
            &interner,
            real_object,
            IntrinsicKind::Object
        ));
        // Documented fallback hazard: the shared structural matcher still
        // accepts the impostor shape.
        assert!(matches_global_object_interface_shape(&interner, user_iface));
        assert!(is_global_object_interface(&interner, user_iface));
    }

    /// `Lazy(DefId)` forms registered as boxed def ids match the identity
    /// tier through both the interner and a resolver registry.
    #[test]
    fn lazy_def_id_identity_matches_after_registration() {
        let interner = TypeInterner::new();
        let def_id = DefId(42);
        let lazy = interner.lazy(def_id);
        let other_lazy = interner.lazy(DefId(43));

        let mut env = TypeEnvironment::new();
        env.register_boxed_def_id(IntrinsicKind::Function, def_id);
        assert!(is_global_interface_by_identity_with_resolver(
            &interner,
            &env,
            lazy,
            IntrinsicKind::Function
        ));
        assert!(!is_global_interface_by_identity_with_resolver(
            &interner,
            &env,
            other_lazy,
            IntrinsicKind::Function
        ));
        // Interner-registry tier (resolver may be a different instance).
        interner.register_boxed_def_id(IntrinsicKind::Object, def_id);
        assert!(is_global_interface_by_identity(
            &interner,
            lazy,
            IntrinsicKind::Object
        ));
        assert!(!is_global_interface_by_identity(
            &interner,
            other_lazy,
            IntrinsicKind::Object
        ));
    }

    /// Structural Function fallback: requires all of `apply`/`call`/`bind`
    /// and rejects shapes above the property-count cap.
    #[test]
    fn function_structural_fallback_requires_apply_call_bind_and_cap() {
        let interner = TypeInterner::new();
        let with_bind = function_like_shape(&interner, true);
        let without_bind = function_like_shape(&interner, false);
        assert!(matches_global_function_interface_shape(
            &interner, with_bind
        ));
        assert!(!matches_global_function_interface_shape(
            &interner,
            without_bind
        ));

        // 21 properties incl. apply/call/bind: over the cap, must not match.
        let mut props: Vec<PropertyInfo> = (0..18)
            .map(|i| PropertyInfo::new(interner.intern_string(&format!("p{i}")), TypeId::ANY))
            .collect();
        for name in ["apply", "call", "bind"] {
            props.push(PropertyInfo::new(interner.intern_string(name), TypeId::ANY));
        }
        let oversized = interner.object(props);
        assert!(!matches_global_function_interface_shape(
            &interner, oversized
        ));
        // Intrinsics never match the structural tier.
        assert!(!matches_global_function_interface_shape(
            &interner,
            TypeId::FUNCTION
        ));
    }

    /// Structural Object fallback requires `propertyIsEnumerable` (the
    /// unified, strictest historical probe) and respects the 7-property cap.
    #[test]
    fn object_structural_fallback_requires_all_probe_members() {
        let interner = TypeInterner::new();
        let full = object_like_shape(&interner);
        assert!(matches_global_object_interface_shape(&interner, full));

        let missing_enumerable = interner.object(
            ["constructor", "toString", "hasOwnProperty", "isPrototypeOf"]
                .iter()
                .map(|name| PropertyInfo::new(interner.intern_string(name), TypeId::ANY))
                .collect(),
        );
        assert!(!matches_global_object_interface_shape(
            &interner,
            missing_enumerable
        ));

        // 8 properties: over the cap (derived interfaces like Boolean).
        let mut props: Vec<PropertyInfo> = [
            "constructor",
            "toString",
            "toLocaleString",
            "valueOf",
            "hasOwnProperty",
            "isPrototypeOf",
            "propertyIsEnumerable",
        ]
        .iter()
        .map(|name| PropertyInfo::new(interner.intern_string(name), TypeId::ANY))
        .collect();
        props.push(PropertyInfo::new(
            interner.intern_string("extra"),
            TypeId::ANY,
        ));
        let oversized = interner.object(props);
        assert!(!matches_global_object_interface_shape(&interner, oversized));
    }
}
