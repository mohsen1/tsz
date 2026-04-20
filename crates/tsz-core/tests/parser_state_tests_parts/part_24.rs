#[test]
fn test_abstract_interface_emits_ts1242_not_ts1184() {
    // 'abstract interface I {}' should emit TS1242, not TS1184.
    // TSC gives the specific "'abstract' modifier can only appear on a class, method, or property declaration."
    use tsz_common::diagnostics::diagnostic_codes;

    let mut parser = ParserState::new("test.ts".to_string(), "abstract interface I {}".to_string());
    parser.parse_source_file();
    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(
            &diagnostic_codes::ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION
        ),
        "Expected TS1242 for abstract interface, got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE),
        "Should NOT emit TS1184 for abstract interface, got: {codes:?}"
    );
}
