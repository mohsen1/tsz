use std::fs;

#[test]
fn member_access_uses_centralized_mutual_assignability_gateway() {
    // After the member_access.rs split, duplicate-property type consistency
    // checks live in interface_checks.rs.
    let source = fs::read_to_string("src/state/state_checking_members/interface_checks.rs")
        .expect("failed to read src/state/state_checking_members/interface_checks.rs");

    assert!(
        !source.contains("&& self.is_assignable_to(current_type, *first_type)")
            && !source.contains("&& self.is_assignable_to(current_type, first_type)"),
        "interface_checks should route bidirectional relation checks through are_mutually_assignable",
    );
    assert!(
        source.contains("are_mutually_assignable(first_type, current_type)"),
        "interface_checks should use are_mutually_assignable for duplicate-property type consistency checks",
    );
}
