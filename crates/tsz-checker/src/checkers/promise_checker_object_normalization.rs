use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn evaluate_awaited_object_properties_for_assignability(
        &mut self,
        type_id: TypeId,
        depth: u8,
    ) -> Option<TypeId> {
        let shape_id = crate::query_boundaries::common::object_shape_id(self.ctx.types, type_id)?;
        let shape = self.ctx.types.object_shape(shape_id);
        let mut changed = false;
        let evaluated_properties: Vec<_> = shape
            .properties
            .iter()
            .map(|prop| {
                let evaluated_type = self
                    .evaluate_awaited_application_for_assignability_inner(prop.type_id, depth + 1);
                let evaluated_write = self.evaluate_awaited_application_for_assignability_inner(
                    prop.write_type,
                    depth + 1,
                );
                changed |= evaluated_type != prop.type_id || evaluated_write != prop.write_type;
                tsz_solver::PropertyInfo {
                    type_id: evaluated_type,
                    write_type: evaluated_write,
                    ..*prop
                }
            })
            .collect();
        let evaluated_string_index = shape.string_index.map(|mut index| {
            let evaluated = self
                .evaluate_awaited_application_for_assignability_inner(index.value_type, depth + 1);
            changed |= evaluated != index.value_type;
            index.value_type = evaluated;
            index
        });
        let evaluated_number_index = shape.number_index.map(|mut index| {
            let evaluated = self
                .evaluate_awaited_application_for_assignability_inner(index.value_type, depth + 1);
            changed |= evaluated != index.value_type;
            index.value_type = evaluated;
            index
        });

        changed.then(|| {
            self.ctx
                .types
                .factory()
                .object_with_index(tsz_solver::ObjectShape {
                    properties: evaluated_properties,
                    string_index: evaluated_string_index,
                    number_index: evaluated_number_index,
                    ..(*shape).clone()
                })
        })
    }
}
