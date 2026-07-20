//! Callable-specific conditional relation shortcuts.

use super::*;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Check if this is a primitive type vs `Function` or callable target.
    ///
    /// Primitive types are never subtypes of `Function` in TypeScript. The
    /// structural relation can otherwise autobox them and find accidental
    /// wrapper overlap.
    pub(super) fn is_primitive_vs_function(
        interner: &dyn crate::construction::TypeDatabase,
        check_type: TypeId,
        extends_type: TypeId,
    ) -> bool {
        use crate::types::IntrinsicKind;

        let is_primitive = matches!(
            check_type,
            TypeId::STRING | TypeId::NUMBER | TypeId::BOOLEAN | TypeId::BIGINT | TypeId::SYMBOL
        );
        if !is_primitive {
            return false;
        }
        if extends_type == TypeId::FUNCTION {
            return true;
        }
        if let Some(TypeData::Intrinsic(IntrinsicKind::Function)) = interner.lookup(extends_type) {
            return true;
        }
        crate::type_queries::is_global_function_interface(interner, extends_type)
    }

    pub(super) fn function_intrinsic_extends_callable_target(
        interner: &dyn crate::construction::TypeDatabase,
        check_type: TypeId,
        extends_type: TypeId,
    ) -> bool {
        use crate::types::IntrinsicKind;

        let check_is_function_intrinsic = check_type == TypeId::FUNCTION
            || matches!(
                interner.lookup(check_type),
                Some(TypeData::Intrinsic(IntrinsicKind::Function))
            );
        if !check_is_function_intrinsic {
            return false;
        }
        if function_shape_id(interner, extends_type).is_some() {
            return true;
        }
        callable_shape_id(interner, extends_type)
            .is_some_and(|shape_id| !interner.callable_shape(shape_id).call_signatures.is_empty())
    }
}
