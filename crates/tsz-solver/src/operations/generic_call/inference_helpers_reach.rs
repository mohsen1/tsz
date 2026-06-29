//! Round-1 reachability analysis for generic-call inference.
//!
//! Walks an argument type and its target parameter type in parallel to decide
//! which inference placeholder vars round-1 *direct argument* inference actually
//! constrains. Split out of `inference_helpers` to keep both files under the
//! source-line ceiling.

use crate::inference::infer::InferenceVar;
use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::types::{TypeData, TypeId};
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Round1ReachVisitState {
    Entered,
    AlreadyVisited,
}

impl Round1ReachVisitState {
    fn record(
        arg_type: TypeId,
        target_type: TypeId,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
    ) -> Self {
        if visited.insert((arg_type, target_type)) {
            Self::Entered
        } else {
            Self::AlreadyVisited
        }
    }
}

impl<C: AssignabilityChecker> CallEvaluator<'_, C> {
    /// Collect the inference placeholder vars that round-1 *direct argument*
    /// inference will actually constrain for `arg_type` against `target_type`.
    ///
    /// Unlike [`Self::collect_placeholder_vars_in_type`], which returns every
    /// placeholder structurally present in the parameter type, this walks the
    /// argument and parameter in parallel so placeholders reachable only through
    /// parameter members the argument never supplies (e.g. an omitted optional
    /// property's callback parameter) are not counted as covered. Over-counting
    /// them wrongly marked a return-type var as "already covered" and skipped
    /// contextual-return seeding, leaving that type parameter at `unknown`
    /// (see #14171, #14822).
    ///
    /// Shapes this walker does not decompose precisely (mismatched constructs,
    /// index-signature objects, etc.) fall back to the structural
    /// over-approximation so existing behaviour is preserved.
    pub(super) fn collect_round1_reachable_placeholder_vars(
        &self,
        arg_type: TypeId,
        target_type: TypeId,
        var_map: &FxHashMap<TypeId, InferenceVar>,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
        out: &mut FxHashSet<InferenceVar>,
    ) {
        if var_map.is_empty() {
            return;
        }
        match Round1ReachVisitState::record(arg_type, target_type, visited) {
            Round1ReachVisitState::Entered => {}
            Round1ReachVisitState::AlreadyVisited => return,
        }

        // The target is itself a placeholder: the argument constrains it directly.
        if let Some(&var) = var_map.get(&target_type) {
            out.insert(var);
            return;
        }

        // Targets with no placeholder vars contribute nothing.
        if target_type.is_intrinsic() {
            return;
        }

        let db = self.interner.as_type_database();

        // Plain object / object: only properties the argument actually supplies
        // can carry inference into the parameter's placeholder vars. Parameter-
        // only members (omitted optionals) are left for the contextual return
        // type to seed. Index-signature objects fall through to the
        // over-approximation below.
        if let (Some(TypeData::Object(arg_shape_id)), Some(TypeData::Object(target_shape_id))) = (
            self.interner.lookup(arg_type),
            self.interner.lookup(target_type),
        ) {
            let arg_shape = self.interner.object_shape(arg_shape_id);
            let target_shape = self.interner.object_shape(target_shape_id);
            for target_prop in &target_shape.properties {
                if let Some(arg_prop) = arg_shape
                    .properties
                    .iter()
                    .find(|p| p.name == target_prop.name)
                {
                    self.collect_round1_reachable_placeholder_vars(
                        arg_prop.type_id,
                        target_prop.type_id,
                        var_map,
                        visited,
                        out,
                    );
                }
            }
            return;
        }

        // Array element / array element.
        if let (Some(arg_elem), Some(target_elem)) = (
            crate::type_queries::get_array_element_type(db, arg_type),
            crate::type_queries::get_array_element_type(db, target_type),
        ) {
            self.collect_round1_reachable_placeholder_vars(
                arg_elem,
                target_elem,
                var_map,
                visited,
                out,
            );
            return;
        }

        // Tuple / tuple: align element types positionally.
        if let (Some(TypeData::Tuple(arg_list)), Some(TypeData::Tuple(target_list))) = (
            self.interner.lookup(arg_type),
            self.interner.lookup(target_type),
        ) {
            let arg_elems = self.interner.tuple_list(arg_list);
            let target_elems = self.interner.tuple_list(target_list);
            for (arg_elem, target_elem) in arg_elems.iter().zip(target_elems.iter()) {
                self.collect_round1_reachable_placeholder_vars(
                    arg_elem.type_id,
                    target_elem.type_id,
                    var_map,
                    visited,
                    out,
                );
            }
            return;
        }

        // Same-base application: align type arguments positionally.
        if let (Some((arg_base, arg_args)), Some((target_base, target_args))) = (
            crate::type_queries::get_application_info(db, arg_type),
            crate::type_queries::get_application_info(db, target_type),
        ) && arg_base == target_base
            && arg_args.len() == target_args.len()
        {
            for (arg_arg, target_arg) in arg_args.iter().zip(target_args.iter()) {
                self.collect_round1_reachable_placeholder_vars(
                    *arg_arg,
                    *target_arg,
                    var_map,
                    visited,
                    out,
                );
            }
            return;
        }

        // Optional members surface as `T | undefined` / `T | null`. Recurse
        // through the non-nullish members so a decomposable shape behind an
        // optional property (for example a callback type on an optional
        // constructor option) is handled precisely instead of falling back to
        // the structural over-approximation, which marks every placeholder in
        // the union regardless of variance. Restricted to nullish-stripping so
        // genuine multi-member unions keep their conservative behaviour.
        if let Some(TypeData::Union(list)) = self.interner.lookup(target_type) {
            let members = self.interner.type_list(list);
            let nullish_count = members
                .iter()
                .filter(|&&m| matches!(m, TypeId::UNDEFINED | TypeId::NULL))
                .count();
            // Handle only when the union has both nullish and non-nullish members
            // (an optional property); genuine multi-member unions keep their
            // conservative over-approximation below.
            if nullish_count > 0 && nullish_count < members.len() {
                for &member in members
                    .iter()
                    .filter(|&&m| !matches!(m, TypeId::UNDEFINED | TypeId::NULL))
                {
                    self.collect_round1_reachable_placeholder_vars(
                        arg_type, member, var_map, visited, out,
                    );
                }
                return;
            }
        }

        // Function / function: a callback argument seeds return-type vars through
        // its RETURN position (covariant) and through a PARAMETER position only
        // when the argument supplies a concrete parameter type. A context-
        // sensitive callback whose parameter was left to contextual typing
        // surfaces here as an `unknown`/`any` parameter; counting the placeholder
        // behind it as "covered" would suppress contextual-return seeding and
        // leave the type parameter at `unknown` — e.g.
        // `new Box({ schema: null, refiner(value) {} })` returned where
        // `Box<_, T>` is expected (#14822). Without decomposing the function we
        // fall back to the structural over-approximation below, which marks every
        // placeholder in `(value: T) => boolean` regardless of variance.
        if let (Some(arg_fn), Some(target_fn)) = (
            crate::type_queries::get_function_shape(db, arg_type),
            crate::type_queries::get_function_shape(db, target_type),
        ) {
            self.collect_round1_reachable_placeholder_vars(
                arg_fn.return_type,
                target_fn.return_type,
                var_map,
                visited,
                out,
            );
            for (arg_param, target_param) in arg_fn.params.iter().zip(target_fn.params.iter()) {
                if matches!(
                    arg_param.type_id,
                    TypeId::UNKNOWN | TypeId::ANY | TypeId::ERROR
                ) {
                    continue;
                }
                self.collect_round1_reachable_placeholder_vars(
                    arg_param.type_id,
                    target_param.type_id,
                    var_map,
                    visited,
                    out,
                );
            }
            return;
        }

        // Shapes we do not decompose precisely fall back to the structural
        // over-approximation, preserving prior behaviour.
        out.extend(self.collect_placeholder_vars_in_type(
            target_type,
            var_map,
            &mut FxHashMap::default(),
            &mut FxHashSet::default(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::Round1ReachVisitState;
    use crate::types::TypeId;
    use rustc_hash::FxHashSet;

    #[test]
    fn round1_reach_visit_state_enters_new_pair() {
        let mut visited = FxHashSet::default();

        let state = Round1ReachVisitState::record(TypeId::STRING, TypeId::NUMBER, &mut visited);

        assert_eq!(state, Round1ReachVisitState::Entered);
        assert!(visited.contains(&(TypeId::STRING, TypeId::NUMBER)));
    }

    #[test]
    fn round1_reach_visit_state_detects_reentry() {
        let mut visited = FxHashSet::default();

        assert_eq!(
            Round1ReachVisitState::record(TypeId::STRING, TypeId::NUMBER, &mut visited),
            Round1ReachVisitState::Entered
        );
        assert_eq!(
            Round1ReachVisitState::record(TypeId::STRING, TypeId::NUMBER, &mut visited),
            Round1ReachVisitState::AlreadyVisited
        );
    }

    #[test]
    fn round1_reach_visit_state_distinguishes_target_pairs() {
        let mut visited = FxHashSet::default();

        assert_eq!(
            Round1ReachVisitState::record(TypeId::STRING, TypeId::NUMBER, &mut visited),
            Round1ReachVisitState::Entered
        );
        assert_eq!(
            Round1ReachVisitState::record(TypeId::STRING, TypeId::BOOLEAN, &mut visited),
            Round1ReachVisitState::Entered
        );
    }
}
