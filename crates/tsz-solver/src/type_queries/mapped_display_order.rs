//! Display-order helpers for mapped-type public surfaces.

use crate::construction::TypeDatabase;
use crate::types::{PropertyInfo, TypeData, TypeId};
use rustc_hash::FxHashMap;
use tsz_common::Atom;

pub fn collect_homomorphic_source_property_infos(
    db: &dyn TypeDatabase,
    source: TypeId,
) -> Vec<PropertyInfo> {
    collect_homomorphic_source_property_infos_with_evaluator(db, source, &mut |type_id| {
        crate::evaluation::evaluate::evaluate_type(db, type_id)
    })
}

pub fn collect_homomorphic_source_property_infos_with_evaluator<F>(
    db: &dyn TypeDatabase,
    source: TypeId,
    evaluate: &mut F,
) -> Vec<PropertyInfo>
where
    F: FnMut(TypeId) -> TypeId,
{
    fn sort_by_display_or_declaration_order(
        db: &dyn TypeDatabase,
        source: TypeId,
        props: &[PropertyInfo],
    ) -> Vec<PropertyInfo> {
        let mut ordered = props.to_vec();
        if let Some(display_props) = db.get_display_properties(source) {
            let mut display_props = display_props.as_ref().clone();
            if display_props.iter().any(|prop| prop.declaration_order > 0) {
                display_props.sort_by_key(|prop| prop.declaration_order);
            }
            let order_map: FxHashMap<Atom, usize> = display_props
                .iter()
                .enumerate()
                .map(|(idx, prop)| (prop.name, idx))
                .collect();
            ordered.sort_by_key(|prop| order_map.get(&prop.name).copied().unwrap_or(usize::MAX));
        } else if ordered.iter().any(|prop| prop.declaration_order > 0) {
            ordered.sort_by_key(|prop| prop.declaration_order);
        }
        ordered
    }

    fn display_props_if_richer(
        db: &dyn TypeDatabase,
        source: TypeId,
        raw_prop_count: usize,
    ) -> Option<Vec<PropertyInfo>> {
        let display_props = db.get_display_properties(source)?;
        if display_props.len() <= raw_prop_count {
            return None;
        }
        let mut display_props = display_props.as_ref().clone();
        if display_props.iter().any(|prop| prop.declaration_order > 0) {
            display_props.sort_by_key(|prop| prop.declaration_order);
        }
        Some(display_props)
    }

    fn collect_array_property_infos(
        db: &dyn TypeDatabase,
        element_type: TypeId,
        evaluate: &mut impl FnMut(TypeId) -> TypeId,
    ) -> Vec<PropertyInfo> {
        let Some(array_base) = db
            .get_array_display_base_type()
            .or_else(|| db.get_array_base_type())
        else {
            return Vec::new();
        };
        let mut base_props =
            collect_homomorphic_source_property_infos_with_evaluator(db, array_base, evaluate);
        let Some(array_param) = db.get_array_base_type_params().first() else {
            sort_array_homomorphic_source_properties(db, &mut base_props);
            return base_props;
        };
        let mut subst = crate::instantiation::instantiate::TypeSubstitution::new();
        subst.insert(array_param.name, element_type);
        let this_type = db.array(element_type);
        let mut props: Vec<_> = base_props
            .into_iter()
            .map(|mut prop| {
                prop.type_id = evaluate(crate::instantiation::instantiate::instantiate_type(
                    db,
                    prop.type_id,
                    &subst,
                ));
                prop.write_type = evaluate(crate::instantiation::instantiate::instantiate_type(
                    db,
                    prop.write_type,
                    &subst,
                ));
                if crate::contains_this_type(db, prop.type_id) {
                    prop.type_id = crate::instantiation::instantiate::substitute_this_type_cached(
                        db,
                        None,
                        prop.type_id,
                        this_type,
                    );
                }
                if crate::contains_this_type(db, prop.write_type) {
                    prop.write_type =
                        crate::instantiation::instantiate::substitute_this_type_cached(
                            db,
                            None,
                            prop.write_type,
                            this_type,
                        );
                }
                prop
            })
            .collect();
        sort_array_homomorphic_source_properties(db, &mut props);
        props
    }

    if !source.is_intrinsic()
        && let Some(TypeData::Array(element_type)) = db.lookup(source)
    {
        return collect_array_property_infos(db, element_type, evaluate);
    }

    let evaluated = evaluate(source);
    match db.lookup(evaluated) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            display_props_if_richer(db, evaluated, shape.properties.len()).unwrap_or_else(|| {
                sort_by_display_or_declaration_order(db, evaluated, &shape.properties)
            })
        }
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            display_props_if_richer(db, evaluated, shape.properties.len()).unwrap_or_else(|| {
                sort_by_display_or_declaration_order(db, evaluated, &shape.properties)
            })
        }
        Some(TypeData::Array(element_type)) => {
            collect_array_property_infos(db, element_type, evaluate)
        }
        _ => Vec::new(),
    }
}

pub fn sort_homomorphic_source_properties_for_display(
    db: &dyn TypeDatabase,
    source: TypeId,
    resolved_source: TypeId,
    props: &mut [PropertyInfo],
) {
    if super::mapped_declaration_surface::sort_number_wrapper_properties_for_display(
        db,
        source,
        resolved_source,
        props,
    ) {
        return;
    }

    if is_array_display_source(db, source, resolved_source) {
        sort_array_homomorphic_source_properties(db, props);
        return;
    }

    let display_props = db
        .get_display_properties(source)
        .or_else(|| db.get_display_properties(resolved_source));
    if let Some(display_props) = display_props {
        let mut display_props = display_props.as_ref().clone();
        if display_props.iter().any(|prop| prop.declaration_order != 0) {
            display_props.sort_by_key(|prop| prop.declaration_order);
        }
        let order_map: FxHashMap<Atom, usize> = display_props
            .iter()
            .enumerate()
            .map(|(idx, prop)| (prop.name, idx))
            .collect();
        props.sort_by_key(|prop| order_map.get(&prop.name).copied().unwrap_or(usize::MAX));
    } else if props.iter().any(|prop| prop.declaration_order != 0) {
        props.sort_by_key(|prop| prop.declaration_order);
    }
}

fn is_array_display_source(db: &dyn TypeDatabase, source: TypeId, resolved_source: TypeId) -> bool {
    is_array_type_id(db, source)
        || is_array_type_id(db, resolved_source)
        || db
            .get_display_alias(source)
            .is_some_and(|alias| is_array_type_id(db, alias))
        || db
            .get_display_alias(resolved_source)
            .is_some_and(|alias| is_array_type_id(db, alias))
}

fn is_array_type_id(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeData::Array(_)))
}

pub(crate) fn sort_array_homomorphic_source_properties(
    db: &dyn TypeDatabase,
    props: &mut [PropertyInfo],
) {
    props.sort_by(|a, b| {
        match (
            array_property_rank(db, a.name),
            array_property_rank(db, b.name),
        ) {
            (Some(a_rank), Some(b_rank)) => a_rank.cmp(&b_rank),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => {
                if a.declaration_order > 0 && b.declaration_order > 0 {
                    a.declaration_order.cmp(&b.declaration_order)
                } else {
                    std::cmp::Ordering::Equal
                }
            }
        }
    });
    for (index, prop) in props.iter_mut().enumerate() {
        prop.declaration_order = (index + 1) as u32;
    }
}

fn array_property_rank(db: &dyn TypeDatabase, name: Atom) -> Option<usize> {
    [
        "length",
        "toString",
        "toLocaleString",
        "pop",
        "push",
        "concat",
        "join",
        "reverse",
        "shift",
        "slice",
        "sort",
        "splice",
        "unshift",
        "indexOf",
        "lastIndexOf",
        "every",
        "some",
        "forEach",
        "map",
        "filter",
        "reduce",
        "reduceRight",
        "find",
        "findIndex",
        "fill",
        "copyWithin",
        "entries",
        "keys",
        "values",
        "includes",
        "flatMap",
        "flat",
        "[Symbol.iterator]",
        "[Symbol.unscopables]",
    ]
    .iter()
    .position(|candidate| db.resolve_atom_ref(name).as_ref() == *candidate)
}
