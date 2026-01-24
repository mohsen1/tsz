//! Tests for trailing comma tracking in the parser
//!
//! These tests verify that the parser correctly identifies trailing commas
//! in various contexts where TypeScript allows them.

#[cfg(test)]
mod tests {
    use crate::ScannerState;
    use crate::parser::ParserState;

    fn parse_code(code: &str) -> Vec<crate::parser::ParseDiagnostic> {
        let mut scanner = ScannerState::new("test.ts".to_string(), code.to_string());
        scanner.scan();
        let mut parser = ParserState::new("test.ts".to_string(), scanner);
        parser.parse_source_file();
        parser.get_diagnostics()
    }

    #[test]
    fn test_trailing_comma_in_parameter_list() {
        let code = r#"
function foo(a: string, b: number,) {
    return a + b;
}
"#;
        let diagnostics = parse_code(code);
        // Should not emit any errors - trailing comma is allowed
        let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
        assert_eq!(
            ts1005_count, 0,
            "Trailing comma in parameter list should not emit TS1005"
        );
    }

    #[test]
    fn test_trailing_comma_in_enum() {
        let code = r#"
enum Color {
    Red,
    Green,
    Blue,
}
"#;
        let diagnostics = parse_code(code);
        // Should not emit any errors - trailing comma is allowed
        let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
        assert_eq!(
            ts1005_count, 0,
            "Trailing comma in enum should not emit TS1005"
        );
    }

    #[test]
    fn test_trailing_comma_in_array_literal() {
        let code = r#"
const arr = [1, 2, 3,];
"#;
        let diagnostics = parse_code(code);
        // Should not emit any errors - trailing comma is allowed
        let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
        assert_eq!(
            ts1005_count, 0,
            "Trailing comma in array literal should not emit TS1005"
        );
    }

    #[test]
    fn test_trailing_comma_in_object_literal() {
        let code = r#"
const obj = { a: 1, b: 2, };
"#;
        let diagnostics = parse_code(code);
        // Should not emit any errors - trailing comma is allowed
        let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
        assert_eq!(
            ts1005_count, 0,
            "Trailing comma in object literal should not emit TS1005"
        );
    }

    #[test]
    fn test_trailing_comma_in_type_parameters() {
        let code = r#"
function foo<T, U,>() {
    // ...
}
"#;
        let diagnostics = parse_code(code);
        // Should not emit any errors - trailing comma is allowed
        let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
        assert_eq!(
            ts1005_count, 0,
            "Trailing comma in type parameters should not emit TS1005"
        );
    }

    #[test]
    fn test_trailing_comma_in_type_arguments() {
        let code = r#"
const arr: Array<string, number,> = [1, 2];
"#;
        let diagnostics = parse_code(code);
        // Should not emit any errors - trailing comma is allowed
        let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
        assert_eq!(
            ts1005_count, 0,
            "Trailing comma in type arguments should not emit TS1005"
        );
    }

    #[test]
    fn test_no_trailing_comma() {
        let code = r#"
function foo(a: string, b: number) {
    return a + b;
}
"#;
        let diagnostics = parse_code(code);
        // Should not emit any errors - no trailing comma is fine too
        let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
        assert_eq!(ts1005_count, 0, "No trailing comma should not emit TS1005");
    }

    #[test]
    fn test_asi_after_return() {
        let code = r#"
function foo() {
    return
    42;
}
"#;
        let diagnostics = parse_code(code);
        // TypeScript applies ASI here, so this parses as `return; 42;`
        // The expression `42` is never returned, but this is valid syntax
        let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
        assert_eq!(ts1005_count, 0, "ASI after return should not emit TS1005");
    }

    #[test]
    fn test_asi_after_break() {
        let code = r#"
while (true) {
    break
    // some comment
}
"#;
        let diagnostics = parse_code(code);
        // ASI applies after break
        let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
        assert_eq!(ts1005_count, 0, "ASI after break should not emit TS1005");
    }

    #[test]
    fn test_function_overloads_no_duplicate_error() {
        let code = r#"
function foo(x: string): void;
function foo(x: number): void;
function foo(x: string | number): void {
    console.log(x);
}
"#;
        let diagnostics = parse_code(code);
        // Function overloads should not emit TS2300
        let ts2300_count = diagnostics.iter().filter(|d| d.code == 2300).count();
        assert_eq!(ts2300_count, 0, "Function overloads should not emit TS2300");
    }

    #[test]
    fn test_interface_merging_no_duplicate_error() {
        let code = r#"
interface Box {
    width: number;
}
interface Box {
    height: number;
}
"#;
        let diagnostics = parse_code(code);
        // Interface merging should not emit TS2300
        let ts2300_count = diagnostics.iter().filter(|d| d.code == 2300).count();
        assert_eq!(ts2300_count, 0, "Interface merging should not emit TS2300");
    }

    #[test]
    fn test_namespace_function_merging_no_duplicate_error() {
        let code = r#"
namespace Utils {
    export function helper(): void {}
}
function Utils() {
    // Implementation
}
"#;
        let diagnostics = parse_code(code);
        // Namespace + function merging should not emit TS2300
        let ts2300_count = diagnostics.iter().filter(|d| d.code == 2300).count();
        assert_eq!(
            ts2300_count, 0,
            "Namespace + function merging should not emit TS2300"
        );
    }
}
