use std::fs;

#[test]
fn member_access_uses_centralized_mutual_assignability_gateway() {
    let source = fs::read_to_string("src/state_checking_members/member_access.rs")
        .expect("failed to read src/state_checking_members/member_access.rs");

    assert!(
        !source.contains("&& self.is_assignable_to(current_type, *first_type)")
            && !source.contains("&& self.is_assignable_to(current_type, first_type)"),
        "member_access should route bidirectional relation checks through are_mutually_assignable",
    );
    assert!(
        source.contains("self.are_mutually_assignable(*first_type, current_type)")
            && source.contains("!self.are_mutually_assignable(first_type, current_type)"),
        "member_access should use are_mutually_assignable for duplicate-property type consistency checks",
    );
}
