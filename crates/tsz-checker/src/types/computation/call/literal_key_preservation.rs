use crate::query_boundaries::common::TypeSubstitution;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::{FunctionShape, TypeId};

impl<'a> CheckerState<'a> {
    pub(super) fn direct_round1_literal_preservation_type_params(
        &mut self,
        shape: &FunctionShape,
        args: &[NodeIndex],
        round1_arg_types: &[TypeId],
        sensitive_args: &[bool],
    ) -> TypeSubstitution {
        let mut preserved = self.direct_round1_literal_conflict_type_params(
            shape,
            args,
            round1_arg_types,
            sensitive_args,
        );
        let index_key_type_params = self.direct_round1_literal_index_key_type_params(
            shape,
            args,
            round1_arg_types,
            sensitive_args,
        );
        for (&name, &type_id) in index_key_type_params.map() {
            preserved.insert(name, type_id);
        }
        preserved
    }
}
