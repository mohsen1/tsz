mod application_reduction;

mod array_infer;

mod object_infer;

mod phases;

use crate::instantiation::instantiate::{
    TypeSubstitution, instantiate_generic_cached, instantiate_type, instantiate_type_with_infer,
};

use crate::operations::property::PropertyAccessResult;

use crate::relations::subtype::{SubtypeChecker, TypeResolver};

use crate::types::{
    CallSignature, CallableShape, ConditionalType, ObjectShape, ObjectShapeId, ParamInfo,
    PropertyInfo, TupleElement, TypeData, TypeId, TypeParamInfo,
};

use crate::visitor::{callable_shape_id, function_shape_id};

use rustc_hash::{FxHashMap, FxHashSet};

use smallvec::SmallVec;

use tracing::trace;

use tsz_common::interner::Atom;

use super::super::evaluate::TypeEvaluator;

use crate::type_queries::get_application_base;

use phases::TailCallStep;

include!("conditional_parts/part1.rs");
include!("conditional_parts/part2.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intern::TypeInterner;
    use crate::types::TypeId;

    #[test]
    fn test_is_primitive_vs_function_intrinsic() {
        let interner = TypeInterner::new();
        // Primitives should match against TypeId::FUNCTION
        assert!(
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_primitive_vs_function(
                &interner,
                TypeId::STRING,
                TypeId::FUNCTION
            )
        );
        assert!(
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_primitive_vs_function(
                &interner,
                TypeId::NUMBER,
                TypeId::FUNCTION
            )
        );
        assert!(
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_primitive_vs_function(
                &interner,
                TypeId::BOOLEAN,
                TypeId::FUNCTION
            )
        );
        // Non-primitives should not match
        assert!(
            !TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_primitive_vs_function(
                &interner,
                TypeId::OBJECT,
                TypeId::FUNCTION
            )
        );
        assert!(
            !TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_primitive_vs_function(
                &interner,
                TypeId::ANY,
                TypeId::FUNCTION
            )
        );
        // Primitives against non-Function target should not match
        assert!(
            !TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_primitive_vs_function(
                &interner,
                TypeId::STRING,
                TypeId::OBJECT
            )
        );
    }

    #[test]
    fn test_is_primitive_vs_function_structural() {
        let interner = TypeInterner::new();
        // Create an ObjectShape that looks like Function (has apply, call, bind)
        let apply = interner.intern_string("apply");
        let call = interner.intern_string("call");
        let bind = interner.intern_string("bind");
        let function_shape = interner.object(vec![
            crate::types::PropertyInfo {
                name: apply,
                type_id: TypeId::ANY,
                ..Default::default()
            },
            crate::types::PropertyInfo {
                name: call,
                type_id: TypeId::ANY,
                ..Default::default()
            },
            crate::types::PropertyInfo {
                name: bind,
                type_id: TypeId::ANY,
                ..Default::default()
            },
        ]);
        // string vs structural Function → should match
        assert!(
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_primitive_vs_function(
                &interner,
                TypeId::STRING,
                function_shape
            )
        );
        // Non-Function object → should not match
        let non_fn = interner.object(vec![crate::types::PropertyInfo {
            name: apply,
            type_id: TypeId::ANY,
            ..Default::default()
        }]);
        assert!(
            !TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_primitive_vs_function(
                &interner,
                TypeId::STRING,
                non_fn
            )
        );
    }

    /// `Lazy(DefId)` is a reference to a concrete named type (interface, class, type alias).
    /// It must NOT be treated as a generic ref — it is always resolvable and not an
    /// unresolved type parameter.
    #[test]
    fn test_is_generic_ref_lazy_is_not_generic() {
        let interner = TypeInterner::new();
        let lazy_a = interner.lazy(crate::def::DefId(100));
        let lazy_b = interner.lazy(crate::def::DefId(200));
        assert!(
            !TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_generic_ref(
                &interner, lazy_a
            ),
            "Lazy(DefId) should not be a generic ref"
        );
        assert!(
            !TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_generic_ref(
                &interner, lazy_b
            ),
            "Lazy(DefId) with different DefId should not be a generic ref"
        );
    }

    /// `TypeParameter` is a genuine unknown and must still trigger deferral.
    /// Tests two different parameter names to prove name-independence.
    #[test]
    fn test_is_generic_ref_type_parameter_is_generic() {
        let interner = TypeInterner::new();
        let atom_t = interner.intern_string("T");
        let atom_k = interner.intern_string("K");
        let make_tp = |name| {
            interner.type_param(crate::types::TypeParamInfo {
                name,
                constraint: None,
                default: None,
                is_const: false,
            })
        };
        assert!(
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_generic_ref(
                &interner,
                make_tp(atom_t)
            ),
            "TypeParameter T should be a generic ref"
        );
        assert!(
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_generic_ref(
                &interner,
                make_tp(atom_k)
            ),
            "TypeParameter K (renamed) should be a generic ref"
        );
    }

    /// `IndexAccess(Lazy(DefId), string)` — property access on a named interface — must NOT
    /// trigger deferral. This was the root cause of issue #6256 where
    /// `Interface["prop"] extends Record<string, any>` was incorrectly deferred.
    #[test]
    fn test_is_generic_ref_index_access_lazy_is_not_generic() {
        let interner = TypeInterner::new();
        let lazy = interner.lazy(crate::def::DefId(42));
        let idx_access = interner.index_access(lazy, TypeId::STRING);
        assert!(
            !TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_generic_ref(
                &interner, idx_access
            ),
            "IndexAccess(Lazy(DefId), string) should not be a generic ref"
        );
    }

    /// `IndexAccess(TypeParam, K)` must remain a generic ref — `T[K]` is indeterminate
    /// until T and K are substituted.
    #[test]
    fn test_is_generic_ref_index_access_type_param_remains_generic() {
        let interner = TypeInterner::new();
        let atom_m = interner.intern_string("M");
        let tp_m = interner.type_param(crate::types::TypeParamInfo {
            name: atom_m,
            constraint: None,
            default: None,
            is_const: false,
        });
        let idx_access = interner.index_access(tp_m, TypeId::STRING);
        assert!(
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_generic_ref(
                &interner, idx_access
            ),
            "IndexAccess(TypeParam, string) should be a generic ref"
        );
    }

    /// Intrinsic `TypeId`s (like `TypeId::STRING`) are never generic regardless of
    /// what internal data they might map to.
    #[test]
    fn test_is_generic_ref_intrinsics_are_never_generic() {
        let interner = TypeInterner::new();
        for id in [
            TypeId::STRING,
            TypeId::NUMBER,
            TypeId::BOOLEAN,
            TypeId::ANY,
            TypeId::UNKNOWN,
            TypeId::NEVER,
            TypeId::VOID,
            TypeId::UNDEFINED,
            TypeId::NULL,
        ] {
            assert!(
                !TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_generic_ref(
                    &interner, id
                ),
                "intrinsic {id:?} should not be a generic ref"
            );
        }
    }
}
