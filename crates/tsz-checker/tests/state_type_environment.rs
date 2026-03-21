use super::*;
use tsz_solver::{DefId, TypeInterner};

#[test]
fn classifies_and_extracts_environment_resolution_shapes() {
    let types = TypeInterner::new();

    let lazy = types.lazy(DefId(42));
    let app = types.application(TypeId::STRING, vec![TypeId::NUMBER]);
    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let keyof_string = types.keyof(TypeId::STRING);

    assert!(matches!(
        lazy_def_id(&types, lazy),
        Some(def_id) if def_id == DefId(42)
    ));
    assert!(matches!(
        classify_for_type_resolution(&types, lazy),
        TypeResolutionKind::Lazy(def_id) if def_id == DefId(42)
    ));
    assert!(matches!(
        classify_for_property_access_resolution(&types, app),
        PropertyAccessResolutionKind::Application(_)
    ));
    assert_eq!(
        application_info(&types, app),
        Some((TypeId::STRING, vec![TypeId::NUMBER]))
    );
    assert_eq!(
        tsz_solver::type_queries::get_union_members(&types, union),
        Some(vec![TypeId::STRING, TypeId::NUMBER])
    );
    assert_eq!(
        tsz_solver::type_queries::get_intersection_members(&types, TypeId::STRING),
        None
    );
    assert!(matches!(
        classify_mapped_constraint(&types, keyof_string),
        MappedConstraintKind::KeyOf(inner) if inner == TypeId::STRING
    ));
}

#[test]
fn mapped_source_classification_via_boundary() {
    let types = TypeInterner::new();

    // Array source
    let arr = types.array(TypeId::NUMBER);
    assert!(matches!(
        classify_mapped_source(&types, arr),
        MappedSourceKind::Array(elem) if elem == TypeId::NUMBER
    ));

    // Object source
    let obj = types.object(vec![]);
    assert!(matches!(
        classify_mapped_source(&types, obj),
        MappedSourceKind::Object
    ));

    // Tuple source
    let tup = types.tuple(vec![tsz_solver::TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    assert!(matches!(
        classify_mapped_source(&types, tup),
        MappedSourceKind::Tuple(_)
    ));
}

#[test]
fn mapped_modifier_computation_via_boundary() {
    let types = TypeInterner::new();

    let mapped = tsz_solver::MappedType {
        type_param: tsz_solver::TypeParamInfo {
            name: types.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        name_type: None,
        template: TypeId::NUMBER,
        optional_modifier: Some(tsz_solver::MappedModifier::Remove),
        readonly_modifier: Some(tsz_solver::MappedModifier::Add),
    };

    let (opt, ro) = compute_mapped_modifiers(&mapped, true, true, false);
    assert!(!opt, "-? should remove optional");
    assert!(ro, "+readonly should add readonly");
}

#[test]
fn identity_name_mapping_detection_via_boundary() {
    let types = TypeInterner::new();
    let k_name = types.intern_string("K");

    // No name_type — identity
    let mapped = tsz_solver::MappedType {
        type_param: tsz_solver::TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        name_type: None,
        template: TypeId::NUMBER,
        optional_modifier: None,
        readonly_modifier: None,
    };
    assert!(is_identity_name_mapping(&types, &mapped));

    // name_type = K — identity
    let k_param = types.type_param(tsz_solver::TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    });
    let mapped_with_as_k = tsz_solver::MappedType {
        type_param: tsz_solver::TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        name_type: Some(k_param),
        template: TypeId::NUMBER,
        optional_modifier: None,
        readonly_modifier: None,
    };
    assert!(is_identity_name_mapping(&types, &mapped_with_as_k));
}
