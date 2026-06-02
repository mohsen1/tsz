use super::*;
use tsz_solver::construction::TypeInterner;
use tsz_solver::def::DefId;
use tsz_solver::{IndexSignature, MappedModifier, MappedType, TypeParamInfo};

#[test]
fn target_property_index_uses_first_atom_match() {
    let db = TypeInterner::new();
    let name = db.intern_string("renamed");
    let mut index = TargetPropertyIndex::default();

    index.insert(&PropertyInfo::new(name, TypeId::STRING));
    index.insert(&PropertyInfo::new(name, TypeId::NUMBER));

    let source = PropertyInfo::new(name, TypeId::BOOLEAN);
    assert_eq!(index.matching_type_for(&db, &source), Some(TypeId::STRING));
}

#[test]
fn target_property_index_keeps_string_fallback() {
    let db = TypeInterner::new();
    let name = db.intern_string("fallbackName");
    let mut index = TargetPropertyIndex::default();

    index.fallback_order.push((name, TypeId::NUMBER));

    assert_eq!(
        index.matching_type_by_resolved_name(&db, name),
        Some(TypeId::NUMBER)
    );
}

#[test]
fn symbol_named_source_property_is_accepted_by_property_key_index_signature() {
    let db = TypeInterner::new();
    let property_key = db.union3(TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL);
    let target = db.object_with_index(ObjectShape {
        string_index: Some(IndexSignature {
            key_type: property_key,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        ..ObjectShape::default()
    });
    let mut source_prop = PropertyInfo::new(db.intern_string("[Symbol.iterator]"), TypeId::STRING);
    source_prop.is_symbol_named = true;
    let source = db.object(vec![source_prop]);

    let classification =
        classify_object_properties(&db, source, target).expect("object classification");

    assert!(classification.excess_properties.is_empty());
}

#[test]
fn symbol_named_source_property_is_excess_for_plain_string_index_signature() {
    let db = TypeInterner::new();
    let target = db.object_with_index(ObjectShape {
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        ..ObjectShape::default()
    });
    let mut source_prop = PropertyInfo::new(db.intern_string("[Symbol.iterator]"), TypeId::STRING);
    source_prop.is_symbol_named = true;
    let source = db.object(vec![source_prop]);

    let classification =
        classify_object_properties(&db, source, target).expect("object classification");

    assert_eq!(classification.excess_properties.len(), 1);
}

#[test]
fn optional_mapped_implicit_undefined_is_structural_across_param_names() {
    let db = TypeInterner::new();

    for name in ["K", "Prop"] {
        let mapped = db.mapped(MappedType {
            type_param: TypeParamInfo::simple(db.intern_string(name)),
            constraint: TypeId::STRING,
            template: TypeId::NUMBER,
            name_type: None,
            readonly_modifier: None,
            optional_modifier: Some(MappedModifier::Add),
        });

        assert!(optional_mapped_type_adds_implicit_undefined(
            &db, &db, mapped
        ));
    }
}

#[test]
fn optional_mapped_implicit_undefined_rejects_existing_undefined_template() {
    let db = TypeInterner::new();
    let template = db.union2(TypeId::NUMBER, TypeId::UNDEFINED);
    let mapped = db.mapped(MappedType {
        type_param: TypeParamInfo::simple(db.intern_string("K")),
        constraint: TypeId::STRING,
        template,
        name_type: None,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Add),
    });

    assert!(!optional_mapped_type_adds_implicit_undefined(
        &db, &db, mapped
    ));
}

#[test]
fn optional_mapped_implicit_undefined_respects_display_alias_surface() {
    let db = TypeInterner::new();
    let mapped = db.mapped(MappedType {
        type_param: TypeParamInfo::simple(db.intern_string("K")),
        constraint: TypeId::STRING,
        template: TypeId::NUMBER,
        name_type: None,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Add),
    });
    let alias = db.application(db.lazy(DefId(1)), vec![TypeId::STRING]);
    db.store_display_alias(mapped, alias);

    assert!(!optional_mapped_type_adds_implicit_undefined(
        &db, &db, mapped
    ));
}
