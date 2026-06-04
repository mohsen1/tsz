use super::{AssignabilityChecker, CallEvaluator};
use crate::types::{TypeData, TypeId};

const SPREAD_ARGUMENT_MARKER_NAME: &str = "__tsz_spread_argument__";

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    pub(crate) fn spread_argument_marker_inner(&self, type_id: TypeId) -> Option<TypeId> {
        let Some(TypeData::Tuple(elems_id)) = self.interner.lookup(type_id) else {
            return None;
        };
        let elems = self.interner.tuple_list(elems_id);
        let [elem] = &*elems else {
            return None;
        };
        if !elem.rest {
            return None;
        }
        let name = elem.name?;
        (self.interner.resolve_atom(name) == SPREAD_ARGUMENT_MARKER_NAME).then_some(elem.type_id)
    }

    pub(super) fn generic_spread_argument_marker_inner(&self, type_id: TypeId) -> Option<TypeId> {
        let Some(TypeData::Tuple(elems_id)) = self.interner.lookup(type_id) else {
            return None;
        };
        let [elem] = &*self.interner.tuple_list(elems_id) else {
            return None;
        };
        (elem.rest
            && elem.name.is_none()
            && matches!(
                self.interner.lookup(elem.type_id),
                Some(TypeData::TypeParameter(_))
            ))
        .then_some(elem.type_id)
    }
}
