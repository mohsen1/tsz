use crate::query_boundaries::common;
use crate::state::CheckerState;
use tsz_solver::TypeId;
use tsz_solver::type_handles::PropertyInfo;

impl<'a> CheckerState<'a> {
    pub(super) fn source_is_iterable_like_for_substitution(&mut self, source: TypeId) -> bool {
        let evaluated = self.evaluate_type_with_env(source);
        if self.is_iterable_like_for_substitution(evaluated) {
            return true;
        }
        if common::application_info(self.ctx.types, evaluated).is_none() {
            return false;
        }
        let resolved = self.evaluate_application_type(evaluated);
        if resolved == evaluated {
            return false;
        }
        let resolved = self.evaluate_type_with_env(resolved);
        self.is_iterable_like_for_substitution(resolved)
    }

    fn is_iterable_like_for_substitution(&self, type_id: TypeId) -> bool {
        if let Some(shape_id) = common::object_shape_id(self.ctx.types, type_id)
            .or_else(|| common::object_with_index_shape_id(self.ctx.types, type_id))
        {
            let shape = self.ctx.types.object_shape(shape_id);
            return shape.number_index.is_some()
                || self.has_iterator_property_for_substitution(&shape.properties);
        }
        if let Some(shape_id) = common::callable_shape_id(self.ctx.types, type_id) {
            let shape = self.ctx.types.callable_shape(shape_id);
            return shape.number_index.is_some()
                || self.has_iterator_property_for_substitution(&shape.properties);
        }
        false
    }

    fn has_iterator_property_for_substitution(&self, props: &[PropertyInfo]) -> bool {
        props.iter().any(|prop| {
            let name = self.ctx.types.resolve_atom(prop.name);
            name == "__@iterator" || name == "[Symbol.iterator]"
        })
    }
}
