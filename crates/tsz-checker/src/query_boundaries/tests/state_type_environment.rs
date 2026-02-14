use super::*;
use tsz_solver::{DefId, TypeInterner, TypeKey};

#[test]
fn classifies_and_extracts_environment_resolution_shapes() {
    let types = TypeInterner::new();

    let lazy = types.lazy(DefId(42));
    let app = types.application(TypeId::STRING, vec![TypeId::NUMBER]);
    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let keyof_string = types.intern(TypeKey::KeyOf(TypeId::STRING));

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
        union_members(&types, union),
        Some(vec![TypeId::NUMBER, TypeId::STRING])
    );
    assert_eq!(intersection_members(&types, TypeId::STRING), None);
    assert!(matches!(
        classify_mapped_constraint(&types, keyof_string),
        MappedConstraintKind::KeyOf(inner) if inner == TypeId::STRING
    ));
}
