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
    pub(crate) fn evaluate_awaited_application_for_assignability(
        &mut self,
        type_id: TypeId,
    ) -> TypeId {
        self.evaluate_awaited_application_for_assignability_inner(type_id, 0)
    }

    pub(super) fn evaluate_awaited_application_for_assignability_inner(
        &mut self,
        type_id: TypeId,
        depth: u8,
    ) -> TypeId {
        if depth > 8 {
            return type_id;
        }
        if self.awaited_application_arg(type_id).is_none() {
            if let Some(elem) =
                crate::query_boundaries::common::array_element_type(self.ctx.types, type_id)
            {
                let evaluated_elem =
                    self.evaluate_awaited_application_for_assignability_inner(elem, depth + 1);
                if evaluated_elem != elem {
                    return self.ctx.types.factory().array(evaluated_elem);
                }
            }
            if let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, type_id)
            {
                let raw_awaited_distribution = members
                    .iter()
                    .copied()
                    .any(|member| self.is_raw_awaited_conditional_for_assignability(member));
                let mut changed = false;
                let evaluated_members: Vec<_> = members
                    .into_iter()
                    .map(|member| {
                        let mut evaluated = self
                            .evaluate_awaited_application_for_assignability_inner(
                                member,
                                depth + 1,
                            );
                        if raw_awaited_distribution
                            && let Some(awaited) = self
                                .unwrap_promise_type(evaluated)
                                .or_else(|| self.extract_awaited_type_from_thenable(evaluated))
                        {
                            evaluated = self.evaluate_awaited_application_for_assignability_inner(
                                awaited,
                                depth + 1,
                            );
                        }
                        changed |= evaluated != member;
                        evaluated
                    })
                    .collect();
                if changed {
                    return self.ctx.types.factory().union(evaluated_members);
                }
            }
            if let Some(elems) =
                crate::query_boundaries::common::tuple_elements(self.ctx.types, type_id)
            {
                let mut changed = false;
                let evaluated_elems: Vec<_> = elems
                    .into_iter()
                    .map(|mut elem| {
                        let evaluated = self.evaluate_awaited_application_for_assignability_inner(
                            elem.type_id,
                            depth + 1,
                        );
                        changed |= evaluated != elem.type_id;
                        elem.type_id = evaluated;
                        elem
                    })
                    .collect();
                if changed {
                    return self.ctx.types.factory().tuple(evaluated_elems);
                }
            }
            if let Some((base, args)) =
                crate::query_boundaries::common::application_info(self.ctx.types, type_id)
            {
                let mut changed = false;
                let evaluated_args: Vec<_> = args
                    .iter()
                    .copied()
                    .map(|arg| {
                        let evaluated = self
                            .evaluate_awaited_application_for_assignability_inner(arg, depth + 1);
                        changed |= evaluated != arg;
                        evaluated
                    })
                    .collect();
                if changed {
                    return self.ctx.types.factory().application(base, evaluated_args);
                }
            }
            if let Some(evaluated) =
                self.evaluate_awaited_object_properties_for_assignability(type_id, depth)
            {
                return evaluated;
            }
            if let Some(evaluated) =
                self.evaluate_raw_awaited_conditional_for_assignability(type_id, depth)
            {
                return evaluated;
            }
            return type_id;
        }

        if self.awaited_application_arg_from_type(type_id).is_some() {
            let evaluated = self.evaluate_application_type(type_id);
            if evaluated != type_id {
                return self
                    .evaluate_awaited_application_for_assignability_inner(evaluated, depth + 1);
            }
        }

        let Some(arg) = self.awaited_application_arg(type_id) else {
            return type_id;
        };
        let arg = self.evaluate_type_for_assignability(arg);

        if let Some(members) = crate::query_boundaries::common::union_members(self.ctx.types, arg) {
            let awaited_members = members
                .into_iter()
                .map(|member| {
                    if let Some(awaited) = self
                        .unwrap_promise_type(member)
                        .or_else(|| self.extract_awaited_type_from_thenable(member))
                    {
                        self.evaluate_awaited_application_for_assignability_inner(
                            awaited,
                            depth + 1,
                        )
                    } else {
                        member
                    }
                })
                .collect();
            return self.ctx.types.factory().union(awaited_members);
        }

        if let Some(awaited) = self
            .unwrap_promise_type(arg)
            .or_else(|| self.extract_awaited_type_from_thenable(arg))
        {
            return self.evaluate_awaited_application_for_assignability_inner(awaited, depth + 1);
        }

        // Awaited<T> is transparent for non-thenables. If the conditional
        // evaluator preserved the raw alias application, keep assignability in
        // step with tsc's getAwaitedType without incorrectly treating
        // Awaited<Promise<T>> as Promise<T>.
        arg
    }

    fn evaluate_raw_awaited_conditional_for_assignability(
        &mut self,
        type_id: TypeId,
        depth: u8,
    ) -> Option<TypeId> {
        let cond_id =
            crate::query_boundaries::common::get_conditional_type_id(self.ctx.types, type_id)?;
        let cond = self.ctx.types.conditional_type(cond_id);
        // Awaited<T> expands to `T extends thenable ? ... : T`. After
        // distribution over a union, assignability can see the raw conditional
        // branches instead of the `Awaited<T>` application. Only fold that
        // canonical false-branch shape; other conditional aliases must stay
        // deferred.
        if !self.is_raw_awaited_conditional_for_assignability(type_id) {
            return None;
        }

        let check_type = self.evaluate_type_for_assignability(cond.check_type);
        if let Some(awaited) = self
            .unwrap_promise_type(check_type)
            .or_else(|| self.extract_awaited_type_from_thenable(check_type))
        {
            return Some(
                self.evaluate_awaited_application_for_assignability_inner(awaited, depth + 1),
            );
        }

        if !crate::query_boundaries::common::has_property_by_str(self.ctx.types, check_type, "then")
        {
            return Some(
                self.evaluate_awaited_application_for_assignability_inner(
                    cond.false_type,
                    depth + 1,
                ),
            );
        }

        None
    }

    fn is_raw_awaited_conditional_for_assignability(&mut self, type_id: TypeId) -> bool {
        let Some(cond_id) =
            crate::query_boundaries::common::get_conditional_type_id(self.ctx.types, type_id)
        else {
            return false;
        };
        let cond = self.ctx.types.conditional_type(cond_id);
        if cond.false_type != cond.check_type {
            return false;
        }

        let extends_type = self.evaluate_type_for_assignability(cond.extends_type);
        crate::query_boundaries::common::has_property_by_str(self.ctx.types, extends_type, "then")
    }
}
