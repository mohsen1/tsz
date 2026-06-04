use crate::inference::infer::{InferenceContext, InferenceVar};

use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};

use crate::operations::{AssignabilityChecker, CallEvaluator};

use crate::types::{FunctionShape, ObjectFlags, ParamInfo, TypeData, TypeId, TypePredicate};

use rustc_hash::{FxHashMap, FxHashSet};

pub(super) fn is_bare_foreign_type_param(
    interner: &dyn crate::construction::TypeDatabase,
    ty: TypeId,
    local_type_params: &FxHashSet<tsz_common::Atom>,
    local_placeholders: &[tsz_common::Atom],
) -> bool {
    if ty.is_intrinsic() {
        return false;
    }
    match interner.lookup(ty) {
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
            !local_type_params.contains(&info.name) && !local_placeholders.contains(&info.name)
        }
        _ => false,
    }
}

pub(super) fn is_substantive_inference_candidate(
    interner: &dyn crate::construction::TypeDatabase,
    ty: TypeId,
    local_type_params: &FxHashSet<tsz_common::Atom>,
    local_placeholders: &[tsz_common::Atom],
) -> bool {
    !ty.is_any_unknown_or_error()
        && !is_bare_foreign_type_param(interner, ty, local_type_params, local_placeholders)
        && !crate::visitor::contains_type_parameters(interner, ty)
        && !crate::type_queries::contains_infer_types_db(interner, ty)
}

include!("inference_helpers_parts/part1.rs");
include!("inference_helpers_parts/part2.rs");
