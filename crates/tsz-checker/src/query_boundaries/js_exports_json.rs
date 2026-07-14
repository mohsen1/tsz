//! JSON-module type construction for the JS export surface, split out of
//! `js_exports.rs`.
//!
//! Owns the `json_module_*` / `json_array_*` / `json_object_type` synthesis
//! plus the ESM/`CommonJS` JSON namespace shapers used when a `.json` module
//! is imported.

use rustc_hash::FxHashSet;
use serde_json::Value as JsonValue;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::{PropertyInfo, TypeId};

use crate::query_boundaries::js_exports::public_export_property;

pub(crate) fn json_module_union(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.union(members)
}

pub(crate) fn json_module_array_type(db: &dyn TypeDatabase, element_type: TypeId) -> TypeId {
    db.array(element_type)
}

pub(crate) fn json_module_object_type(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
) -> TypeId {
    db.object(properties)
}

pub(crate) fn json_module_object_property(
    db: &dyn TypeDatabase,
    name: &str,
    type_id: TypeId,
    declaration_order: u32,
) -> PropertyInfo {
    public_export_property(db, name, type_id, false, declaration_order)
}

pub(crate) fn json_module_missing_property(
    db: &dyn TypeDatabase,
    name: &str,
    declaration_order: u32,
) -> PropertyInfo {
    public_export_property(db, name, TypeId::UNDEFINED, true, declaration_order)
}

pub(crate) fn json_module_value_type(db: &dyn TypeDatabase, value: &JsonValue) -> TypeId {
    match value {
        JsonValue::Null => TypeId::NULL,
        JsonValue::Bool(_) => TypeId::BOOLEAN,
        JsonValue::Number(_) => TypeId::NUMBER,
        JsonValue::String(_) => TypeId::STRING,
        JsonValue::Array(elements) => json_array_type(db, elements),
        JsonValue::Object(entries) => json_object_type(db, entries, None),
    }
}

fn json_array_type(db: &dyn TypeDatabase, elements: &[JsonValue]) -> TypeId {
    let object_property_order = json_array_object_property_order(elements);
    let element_types: Vec<TypeId> = elements
        .iter()
        .map(|element| match element {
            JsonValue::Object(entries) if !object_property_order.is_empty() => {
                json_object_type(db, entries, Some(&object_property_order))
            }
            _ => json_module_value_type(db, element),
        })
        .collect();
    let element_type = match element_types.as_slice() {
        [] => TypeId::NEVER,
        [single] => *single,
        _ => json_module_union(db, element_types),
    };
    json_module_array_type(db, element_type)
}

fn json_array_object_property_order(elements: &[JsonValue]) -> Vec<String> {
    let mut names = Vec::new();
    for element in elements {
        let JsonValue::Object(entries) = element else {
            continue;
        };
        for name in entries.keys() {
            if !names.iter().any(|existing| existing == name) {
                names.push(name.clone());
            }
        }
    }
    names
}

fn json_object_type(
    db: &dyn TypeDatabase,
    entries: &serde_json::Map<String, JsonValue>,
    complete_property_order: Option<&[String]>,
) -> TypeId {
    let mut props = Vec::with_capacity(entries.len());
    let mut present_names = FxHashSet::default();
    for (declaration_order, (name, entry_value)) in entries.iter().enumerate() {
        let prop_type = json_module_value_type(db, entry_value);
        present_names.insert(name.as_str());
        props.push(json_module_object_property(
            db,
            name,
            prop_type,
            (declaration_order + 1) as u32,
        ));
    }

    if let Some(all_names) = complete_property_order {
        let mut declaration_order = props.len() as u32 + 1;
        for name in all_names {
            if present_names.contains(name.as_str()) {
                continue;
            }
            props.push(json_module_missing_property(db, name, declaration_order));
            declaration_order += 1;
        }
    }

    json_module_object_type(db, props)
}

pub(crate) fn json_esm_namespace_type(db: &dyn TypeDatabase, json_type: TypeId) -> TypeId {
    db.object(vec![public_export_property(
        db, "default", json_type, false, 0,
    )])
}

pub(crate) fn commonjs_json_namespace_type(db: &dyn TypeDatabase, json_type: TypeId) -> TypeId {
    let default_atom = db.intern_string("default");
    let Some(shape) = crate::query_boundaries::common::object_shape_for_type(db, json_type) else {
        return json_type;
    };
    if !shape
        .properties
        .iter()
        .any(|property| property.name == default_atom)
    {
        return json_type;
    }

    let mut properties = shape.properties.clone();
    for property in &mut properties {
        if property.name == default_atom {
            property.type_id = json_type;
            property.write_type = json_type;
            property.optional = false;
            property.readonly = false;
            property.declaration_order = 0;
        }
    }
    db.object(properties)
}
