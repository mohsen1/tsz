use super::*;
use tsz_solver::{TypeId, TypeInterner};

#[test]
fn exposes_property_checker_boundary_queries() {
    let types = TypeInterner::new();
    let string_literal = tsz_solver::type_queries_extended::create_string_literal_type(&types, "x");
    let number_literal = tsz_solver::type_queries_extended::create_number_literal_type(&types, 1.0);

    assert!(!is_type_usable_as_property_name(&types, TypeId::STRING));
    assert!(!is_type_usable_as_property_name(&types, TypeId::NUMBER));
    assert!(!is_type_usable_as_property_name(&types, TypeId::SYMBOL));
    assert!(is_type_usable_as_property_name(&types, TypeId::ANY));
    assert!(is_type_usable_as_property_name(&types, string_literal));
    assert!(is_type_usable_as_property_name(&types, number_literal));
    assert!(!is_type_usable_as_property_name(&types, TypeId::BOOLEAN));
}
