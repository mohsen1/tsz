use super::TypeFormatter;
use crate::types::{TypeData, TypeId};

impl<'a> TypeFormatter<'a> {
    fn is_empty_object_type(&self, ty: TypeId) -> bool {
        matches!(
            self.interner.lookup(ty),
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id))
                if {
                    let shape = self.interner.object_shape(shape_id);
                    shape.properties.is_empty()
                        && shape.string_index.is_none()
                        && shape.number_index.is_none()
                }
        )
    }

    pub(super) fn simplify_application_arg_for_display(&self, arg: TypeId) -> TypeId {
        let arg = self.resolve_concrete_index_access_for_display(arg);
        let Some(TypeData::Intersection(list_id)) = self.interner.lookup(arg) else {
            return arg;
        };
        let members = self.interner.type_list(list_id);
        if members.len() < 2
            || !members
                .iter()
                .any(|&member| self.is_empty_object_type(member))
        {
            return arg;
        }
        let retained = members
            .iter()
            .copied()
            .filter(|&member| !self.is_empty_object_type(member))
            .collect::<Vec<_>>();
        if retained.is_empty() {
            arg
        } else if retained.len() == 1 {
            retained[0]
        } else {
            self.interner.intersection(retained)
        }
    }

    pub(super) fn simplify_mapped_application_arg_for_display(&self, arg: TypeId) -> TypeId {
        let arg = self.simplify_application_arg_for_display(arg);
        let Some(TypeData::Lazy(def_id)) = self.interner.lookup(arg) else {
            return arg;
        };
        let Some(def_store) = self.def_store else {
            return arg;
        };
        let Some(def) = def_store.get(def_id) else {
            return arg;
        };
        if def.kind != crate::def::DefKind::TypeAlias {
            return arg;
        }
        let Some(body) = def.body else {
            return arg;
        };
        if !matches!(
            self.interner.lookup(body),
            Some(
                TypeData::Application(_)
                    | TypeData::Conditional(_)
                    | TypeData::IndexAccess(_, _)
                    | TypeData::Mapped(_)
            )
        ) {
            return arg;
        }
        let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, body);
        if evaluated == TypeId::ERROR {
            arg
        } else {
            evaluated
        }
    }
}
