//! Unified assignability rules for the `Object`/`{}`/`object` trifecta.
//!
//! TypeScript has three distinct "object-like" super-types with different
//! assignability rules. This module encodes the full matrix in one place so
//! every relation site uses the same decision table.
//!
//! ## The Trifecta Matrix
//!
//! | Source type   | `object` | `{}`  | `Object` |
//! |---------------|----------|-------|----------|
//! | `string`      | ✗        | ✓     | ✓        |
//! | `number`      | ✗        | ✓     | ✓        |
//! | `boolean`     | ✗        | ✓     | ✓        |
//! | `bigint`      | ✗        | ✓     | ✓        |
//! | `symbol`      | ✗        | ✓     | ✓        |
//! | `null`        | ✗        | ✗     | ✗        |
//! | `undefined`   | ✗        | ✗     | ✗        |
//! | `void`        | ✗        | ✗     | ✗        |
//! | `unknown`     | ✗        | ✗     | ✗        |
//! | `never`       | ✓        | ✓     | ✓        |
//! | `object`      | ✓        | ✓     | ✓        |
//! | `Function`    | ✓        | ✓     | ✓        |
//! | `any`         | *        | ✓     | ✓        |
//! | object lit    | ✓        | ✓     | ✓        |
//!
//! (*) `any <: object` is `true` in permissive `any`-propagation mode and `false`
//! in strict mode. The caller (`is_object_keyword_type`) resolves this.
//!
//! ## Key Distinctions
//!
//! - **`object`** (lowercase keyword) is the non-primitive type. It rejects all
//!   five primitive widening types (`string`, `number`, `boolean`, `bigint`,
//!   `symbol`) and all nullish types (`null`, `undefined`, `void`).
//!
//! - **`{}`** (empty object type) rejects `null` and `undefined` (under strict
//!   null checks) but accepts everything else — including primitives that
//!   auto-box to their wrapper interfaces and thus can satisfy a type with zero
//!   property requirements.  The structural subtype checker handles `{}` via the
//!   apparent-primitive-shape path in `core.rs`; there is no dedicated early
//!   exit for `{}`.
//!
//! - **`Object`** (global interface from `lib.d.ts`) accepts all non-nullish
//!   values.  Like `{}` it accepts primitives but also accepts `void`-returning
//!   contexts where `{}` would not.  The key difference from `{}` is that
//!   `Object` carries method members (`toString`, `valueOf`, etc.) that the
//!   structural path must satisfy — those members are provided by the apparent
//!   primitive shape, so primitives still pass.

use crate::types::IntrinsicKind;

/// Identifies which of the three object super-types is being checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IntrinsicObjectKind {
    /// The `object` keyword — non-primitive, non-nullish types only.
    ObjectKeyword,
    /// The `{}` empty-object type — all non-null/undefined types.
    ///
    /// This variant is provided for completeness so `intrinsic_vs_object_super`
    /// encodes the full three-column matrix. The structural path in `core.rs`
    /// handles `{}` targets without calling this helper at runtime.
    #[allow(dead_code)]
    EmptyObject,
    /// The global `Object` interface from `lib.d.ts` — all non-null/undefined.
    GlobalObject,
}

/// Decide whether a source intrinsic is assignable to one of the three
/// object super-types.
///
/// Returns `Some(true)` when the source is always compatible, `Some(false)`
/// when it is always incompatible, and `None` when the answer depends on
/// context that is not encoded in `IntrinsicKind` alone (currently only
/// `any`, whose behaviour depends on `AnyPropagationMode`).
///
/// The caller is responsible for handling the `None` case — typically by
/// checking `AnyPropagationMode`.
pub(crate) const fn intrinsic_vs_object_super(
    source: IntrinsicKind,
    target: IntrinsicObjectKind,
) -> Option<bool> {
    match source {
        // never, object, and Function are all non-primitive non-nullish → always compatible.
        IntrinsicKind::Never | IntrinsicKind::Object | IntrinsicKind::Function => Some(true),

        // Nullish and unknown are rejected by all three super-types.
        // null/undefined/void are nullish; unknown might be null/undefined at runtime.
        IntrinsicKind::Null
        | IntrinsicKind::Undefined
        | IntrinsicKind::Void
        | IntrinsicKind::Unknown => Some(false),

        // Primitives: the trifecta diverges here.
        IntrinsicKind::String
        | IntrinsicKind::Number
        | IntrinsicKind::Boolean
        | IntrinsicKind::Bigint
        | IntrinsicKind::Symbol => match target {
            // `object` keyword rejects all primitives.
            IntrinsicObjectKind::ObjectKeyword => Some(false),
            // `{}` and `Object` accept primitives (they auto-box to wrapper interfaces).
            IntrinsicObjectKind::EmptyObject | IntrinsicObjectKind::GlobalObject => Some(true),
        },

        // `any` is context-dependent: compatible in permissive mode, not in strict.
        // The caller resolves this via AnyPropagationMode.
        IntrinsicKind::Any => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify the full 13-row × 3-column matrix of `intrinsic_vs_object_super`.
    #[test]
    fn test_intrinsic_object_matrix() {
        use IntrinsicKind as K;
        use IntrinsicObjectKind as T;

        // never → true for all three
        assert_eq!(
            intrinsic_vs_object_super(K::Never, T::ObjectKeyword),
            Some(true)
        );
        assert_eq!(
            intrinsic_vs_object_super(K::Never, T::EmptyObject),
            Some(true)
        );
        assert_eq!(
            intrinsic_vs_object_super(K::Never, T::GlobalObject),
            Some(true)
        );

        // nullish / void / unknown → false for all three
        for nullish in [K::Null, K::Undefined, K::Void, K::Unknown] {
            assert_eq!(
                intrinsic_vs_object_super(nullish, T::ObjectKeyword),
                Some(false),
                "expected {nullish:?} <: object = false"
            );
            assert_eq!(
                intrinsic_vs_object_super(nullish, T::EmptyObject),
                Some(false),
                "expected {nullish:?} <: {{}} = false"
            );
            assert_eq!(
                intrinsic_vs_object_super(nullish, T::GlobalObject),
                Some(false),
                "expected {nullish:?} <: Object = false"
            );
        }

        // primitives → false for object, true for {} and Object
        for prim in [K::String, K::Number, K::Boolean, K::Bigint, K::Symbol] {
            assert_eq!(
                intrinsic_vs_object_super(prim, T::ObjectKeyword),
                Some(false),
                "expected {prim:?} <: object = false"
            );
            assert_eq!(
                intrinsic_vs_object_super(prim, T::EmptyObject),
                Some(true),
                "expected {prim:?} <: {{}} = true"
            );
            assert_eq!(
                intrinsic_vs_object_super(prim, T::GlobalObject),
                Some(true),
                "expected {prim:?} <: Object = true"
            );
        }

        // object/Function → true for all three
        for non_prim in [K::Object, K::Function] {
            assert_eq!(
                intrinsic_vs_object_super(non_prim, T::ObjectKeyword),
                Some(true)
            );
            assert_eq!(
                intrinsic_vs_object_super(non_prim, T::EmptyObject),
                Some(true)
            );
            assert_eq!(
                intrinsic_vs_object_super(non_prim, T::GlobalObject),
                Some(true)
            );
        }

        // any → None for all three (context-dependent)
        assert_eq!(intrinsic_vs_object_super(K::Any, T::ObjectKeyword), None);
        assert_eq!(intrinsic_vs_object_super(K::Any, T::EmptyObject), None);
        assert_eq!(intrinsic_vs_object_super(K::Any, T::GlobalObject), None);
    }
}
