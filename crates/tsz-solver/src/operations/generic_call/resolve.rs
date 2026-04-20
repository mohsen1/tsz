//! Core generic call resolution (`resolve_generic_call_inner`).

use crate::contains_type_by_id;
use crate::inference::infer::{InferenceContext, InferenceError, InferenceVar};
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::widening;
use crate::operations::{AssignabilityChecker, CallEvaluator, CallResult};
use crate::types::{
    FunctionShape, ParamInfo, TupleElement, TypeData, TypeId, TypeParamInfo, TypePredicate,
};
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::{debug, trace};

use super::{
    constraint_is_primitive_type, instantiate_call_type, type_implies_literals_deep,
    type_references_placeholder, unique_placeholder_name,
};

mod finalize;
mod rounds;
mod setup;
mod state;

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    pub(super) fn resolve_generic_call_inner(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> CallResult {
        state::ResolveGenericCallState::new(self, func, arg_types).run()
    }
}
