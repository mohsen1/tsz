use crate::thin_parser::ThinParserState;

#[test]
fn debug_missing_paren() {
    let source = "function f( { }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    println!("Diagnostics: {:#?}", diagnostics);

    for diagnostic in diagnostics {
        println!("Code: {}, Message: {}", diagnostic.code, diagnostic.message);
    }

    assert!(!diagnostics.is_empty(), "Should have diagnostics but got none");
}