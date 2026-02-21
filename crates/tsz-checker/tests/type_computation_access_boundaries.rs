use super::*;
use tsz_solver::{TupleElement, TypeId, TypeInterner};

#[test]
fn exposes_type_computation_access_boundary_queries() {
    let types = TypeInterner::new();

    let tuple = types.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let string_literal = tsz_solver::type_queries_extended::create_string_literal_type(&types, "x");
    let number_literal = tsz_solver::type_queries_extended::create_number_literal_type(&types, 1.0);
    let object = types.object(vec![]);

    assert_eq!(
        tuple_elements(&types, tuple).map(|elements| elements.len()),
        Some(2)
    );
    assert_eq!(
        literal_property_name(&types, string_literal).map(|atom| types.resolve_atom(atom)),
        Some("x".to_string())
    );
    assert_eq!(
        literal_property_name(&types, number_literal).map(|atom| types.resolve_atom(atom)),
        Some("1".to_string())
    );
    assert!(is_valid_spread_type(&types, object));
    assert!(!is_valid_spread_type(&types, TypeId::NUMBER));
}
