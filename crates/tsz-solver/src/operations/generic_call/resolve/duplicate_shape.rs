use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::types::{PropertyInfo, TypeData, TypeId};
use rustc_hash::FxHashMap;

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    pub(super) fn duplicate_single_arg_application_value_shape(
        &self,
        arg_type: TypeId,
    ) -> Option<TypeId> {
        let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) =
            self.interner.lookup(arg_type)
        else {
            return None;
        };
        let shape = self.interner.object_shape(shape_id);
        if shape.properties.len() < 2 {
            return None;
        }

        let mut keys_by_prop = Vec::with_capacity(shape.properties.len());
        let mut counts: FxHashMap<(TypeId, TypeId), usize> = FxHashMap::default();
        for prop in shape.properties.iter() {
            let Some(alias) = self.interner.get_display_alias(prop.type_id) else {
                keys_by_prop.push(None);
                continue;
            };
            let Some(TypeData::Application(app_id)) = self.interner.lookup(alias) else {
                keys_by_prop.push(None);
                continue;
            };
            let app = self.interner.type_application(app_id);
            let Some(&arg) = app.args.first() else {
                keys_by_prop.push(None);
                continue;
            };
            if app.args.len() != 1
                || crate::visitor::literal_string(self.interner.as_type_database(), arg).is_none()
            {
                keys_by_prop.push(None);
                continue;
            }
            let key = (app.base, arg);
            *counts.entry(key).or_default() += 1;
            keys_by_prop.push(Some(key));
        }

        if !counts.values().any(|&count| count > 1) {
            return None;
        }

        let properties = shape
            .properties
            .iter()
            .zip(keys_by_prop)
            .map(|(prop, key)| {
                let is_duplicate =
                    key.is_some_and(|key| counts.get(&key).copied().unwrap_or(0) > 1);
                let type_id = if is_duplicate {
                    TypeId::NEVER
                } else {
                    TypeId::ANY
                };
                PropertyInfo {
                    name: prop.name,
                    type_id,
                    write_type: type_id,
                    optional: prop.optional,
                    readonly: prop.readonly,
                    is_method: prop.is_method,
                    is_class_prototype: prop.is_class_prototype,
                    visibility: prop.visibility,
                    parent_id: prop.parent_id,
                    declaration_order: prop.declaration_order,
                    is_string_named: prop.is_string_named,
                    is_symbol_named: prop.is_symbol_named,
                    single_quoted_name: prop.single_quoted_name,
                    non_widening: false,
                }
            })
            .collect();

        Some(self.interner.object(properties))
    }
}
