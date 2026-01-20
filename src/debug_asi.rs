//! Debug ASI parsing
#[cfg(test)]
mod tests {
    use crate::thin_parser::ThinParserState;

    #[test]
    fn debug_function_missing_paren() {
        let source = r#"function f( { }"#;
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        parser.parse_source_file();

        let diagnostics = parser.get_diagnostics();
        eprintln!("Diagnostics count: {}", diagnostics.len());
        for diag in diagnostics {
            eprintln!(
                "Code: {}, Message: '{}', Start: {}, Length: {}",
                diag.code, diag.message, diag.start, diag.length
            );
        }

        // For now, just check that we get some diagnostic
        assert!(diagnostics.len() > 0, "Expected at least one diagnostic");
    }
}
