use crate::thin_parser::ThinParserState;

fn main() {
    let source = r#"function f( { }"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    println!("Diagnostics: {:?}", diagnostics);
    for diag in diagnostics {
        println!("Code: {}, Message: {}", diag.code, diag.message);
    }
}