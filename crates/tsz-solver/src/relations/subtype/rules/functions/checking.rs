use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};

use crate::type_param_info;

use crate::types::{
    CallSignature, CallableShape, CallableShapeId, FunctionShape, FunctionShapeId, ObjectFlags,
    ObjectShape, ParamInfo, PropertyInfo, TupleElement, TypeData, TypeId, TypeParamInfo,
    Visibility,
};

use crate::visitor::callable_shape_id;

use super::super::super::{SubtypeChecker, SubtypeResult, TypeResolver};

use super::erase_type_params_to_constraints;

mod overloads;

mod params;

type HoistedTypeParams = (Vec<TypeParamInfo>, Vec<(TypeId, TypeId)>);

/// Result of comparing one source/target type-parameter constraint pair while
/// relating two generic signatures of the same arity (see
/// [`SubtypeChecker::classify_generic_tp_constraint`]).
struct GenericTpConstraintRelation {
    /// The source bound is strictly narrower than the target bound, so the source
    /// type parameter cannot be freely alpha-renamed onto the target marker.
    source_is_stricter: bool,
    /// The source constraint merely wraps the target's recursive constraint in
    /// extra application layers (e.g. `Array<Array<T>>` vs `Array<T>`); the extra
    /// wrapping is not treated as genuinely stricter. Only computed (and only
    /// meaningful) when `source_is_stricter` is set.
    wraps_recursive: bool,
    /// The two constraints are mutually assignable. Only computed (and only
    /// meaningful) when the caller requests the bidirectional check via
    /// `need_bidirectional`, i.e. for mapped/indexed contexts; otherwise `false`.
    constraints_mutually_assignable: bool,
}

include!("checking_parts/part1.rs");
include!("checking_parts/part2.rs");
