use super::*;
use serde_json::{Value, json};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;

use crate::checker::types::diagnostics::diagnostic_codes::CANNOT_FIND_NAME;
use crate::lsp::{DiagnosticSeverity, LspDiagnostic, Position, Range};

#[test]
fn test_compare_strings_case_sensitive() {
    // Equal strings
    assert_eq!(
        compare_strings_case_sensitive(Some("abc".into()), Some("abc".into())),
        Comparison::EqualTo
    );

    // Less than
    assert_eq!(
        compare_strings_case_sensitive(Some("abc".into()), Some("abd".into())),
        Comparison::LessThan
    );

    // Greater than
    assert_eq!(
        compare_strings_case_sensitive(Some("abd".into()), Some("abc".into())),
        Comparison::GreaterThan
    );

    // Case matters: 'B' (66) < 'a' (97) in ASCII
    assert_eq!(
        compare_strings_case_sensitive(Some("B".into()), Some("a".into())),
        Comparison::LessThan
    );

    // None handling
    assert_eq!(
        compare_strings_case_sensitive(None, Some("a".into())),
        Comparison::LessThan
    );
    assert_eq!(
        compare_strings_case_sensitive(Some("a".into()), None),
        Comparison::GreaterThan
    );
    assert_eq!(
        compare_strings_case_sensitive(None, None),
        Comparison::EqualTo
    );
}

#[test]
fn test_compare_strings_case_insensitive() {
    // Case-insensitive equal
    assert_eq!(
        compare_strings_case_insensitive(Some("ABC".into()), Some("abc".into())),
        Comparison::EqualTo
    );

    // Case-insensitive less than
    assert_eq!(
        compare_strings_case_insensitive(Some("abc".into()), Some("ABD".into())),
        Comparison::LessThan
    );

    // Case-insensitive greater than
    assert_eq!(
        compare_strings_case_insensitive(Some("ABD".into()), Some("abc".into())),
        Comparison::GreaterThan
    );
}

#[test]
fn test_equate_strings() {
    assert!(equate_strings_case_sensitive("abc", "abc"));
    assert!(!equate_strings_case_sensitive("abc", "ABC"));

    assert!(equate_strings_case_insensitive("abc", "ABC"));
    assert!(equate_strings_case_insensitive("ABC", "abc"));
    assert!(!equate_strings_case_insensitive("abc", "abd"));
}

// Path utility tests

#[test]
fn test_is_any_directory_separator() {
    assert!(is_any_directory_separator('/' as u32));
    assert!(is_any_directory_separator('\\' as u32));
    assert!(!is_any_directory_separator('a' as u32));
    assert!(!is_any_directory_separator(':' as u32));
}

#[test]
fn test_normalize_slashes() {
    assert_eq!(normalize_slashes("path/to/file"), "path/to/file");
    assert_eq!(normalize_slashes("path\\to\\file"), "path/to/file");
    assert_eq!(normalize_slashes("path\\to/file"), "path/to/file");
    assert_eq!(
        normalize_slashes("c:\\windows\\system32"),
        "c:/windows/system32"
    );
}

#[test]
fn test_has_trailing_directory_separator() {
    assert!(has_trailing_directory_separator("/path/to/dir/"));
    assert!(has_trailing_directory_separator("path\\"));
    assert!(!has_trailing_directory_separator("/path/to/file.ext"));
    assert!(!has_trailing_directory_separator(""));
}

#[test]
fn test_path_is_relative() {
    assert!(path_is_relative("./path"));
    assert!(path_is_relative(".\\path"));
    assert!(path_is_relative("../path"));
    assert!(path_is_relative("..\\path"));
    assert!(path_is_relative("."));
    assert!(path_is_relative(".."));
    assert!(!path_is_relative("/absolute/path"));
    assert!(!path_is_relative("path/to/file"));
    assert!(!path_is_relative("c:/windows"));
}

#[test]
fn test_remove_trailing_directory_separator() {
    assert_eq!(
        remove_trailing_directory_separator("/path/to/dir/"),
        "/path/to/dir"
    );
    assert_eq!(
        remove_trailing_directory_separator("/path/to/file"),
        "/path/to/file"
    );
    assert_eq!(remove_trailing_directory_separator("/"), "/");
}

#[test]
fn test_ensure_trailing_directory_separator() {
    assert_eq!(
        ensure_trailing_directory_separator("/path/to/dir"),
        "/path/to/dir/"
    );
    assert_eq!(
        ensure_trailing_directory_separator("/path/to/dir/"),
        "/path/to/dir/"
    );
}

#[test]
fn test_get_base_file_name() {
    assert_eq!(get_base_file_name("/path/to/file.ext"), "file.ext");
    assert_eq!(get_base_file_name("/path/to/"), "to");
    assert_eq!(get_base_file_name("file.ext"), "file.ext");
    assert_eq!(get_base_file_name("/"), "");
}

#[test]
fn test_has_extension() {
    assert!(has_extension("file.ext"));
    assert!(has_extension("/path/to/file.ts"));
    assert!(!has_extension("/path/to/"));
    assert!(!has_extension("noextension"));
}

#[test]
fn test_file_extension_is() {
    assert!(file_extension_is("file.ts", ".ts"));
    assert!(file_extension_is("/path/to/file.d.ts", ".d.ts"));
    assert!(!file_extension_is("file.ts", ".js"));
    assert!(!file_extension_is(".ts", ".ts")); // path must be longer than extension
}

#[test]
fn test_to_file_name_lower_case() {
    // Already lowercase - should return unchanged (optimization)
    assert_eq!(to_file_name_lower_case("file.ts"), "file.ts");
    assert_eq!(to_file_name_lower_case("path/to/file"), "path/to/file");

    // Uppercase letters should be lowercased
    assert_eq!(to_file_name_lower_case("FILE.TS"), "file.ts");
    assert_eq!(to_file_name_lower_case("Path/To/File"), "path/to/file");

    // Mixed case
    assert_eq!(to_file_name_lower_case("MyFile.ts"), "myfile.ts");
    assert_eq!(to_file_name_lower_case("PaTh/To/FiLe"), "path/to/file");

    // Special Unicode characters - should remain unchanged (Turkish locale handling)
    // \u{0130} (İ - Latin capital I with dot above)
    assert_eq!(to_file_name_lower_case("\u{0130}file.ts"), "\u{0130}file.ts");
    // \u{0131} (ı - Latin small letter dotless i)
    assert_eq!(to_file_name_lower_case("file\u{0131}.ts"), "file\u{0131}.ts");
    // \u{00DF} (ß - Latin small letter sharp s)
    assert_eq!(to_file_name_lower_case("file\u{00DF}.ts"), "file\u{00DF}.ts");

    // Safe characters - should remain unchanged
    assert_eq!(to_file_name_lower_case("file-0123456789.ts"), "file-0123456789.ts");
    assert_eq!(to_file_name_lower_case("path_to_file.ts"), "path_to_file.ts");
    assert_eq!(to_file_name_lower_case("path:to:file"), "path:to:file");
    assert_eq!(to_file_name_lower_case("path.to.file"), "path.to.file");
    assert_eq!(to_file_name_lower_case("path to file"), "path to file");

    // Mixed: safe chars + uppercase letters
    assert_eq!(to_file_name_lower_case("MY-File_01.TS"), "my-file_01.ts");
    assert_eq!(to_file_name_lower_case("/PATH/TO/FILE.TS"), "/path/to/file.ts");

    // Edge cases
    assert_eq!(to_file_name_lower_case(""), "");
    assert_eq!(to_file_name_lower_case("A"), "a");
    assert_eq!(to_file_name_lower_case("a"), "a");
}

// Character classification tests

#[test]
fn test_is_line_break() {
    assert!(is_line_break(0x0A)); // LF
    assert!(is_line_break(0x0D)); // CR
    assert!(is_line_break(0x2028)); // Line separator
    assert!(is_line_break(0x2029)); // Paragraph separator
    assert!(!is_line_break(0x20)); // Space
    assert!(!is_line_break(0x09)); // Tab
}

#[test]
fn test_is_white_space_single_line() {
    assert!(is_white_space_single_line(0x20)); // Space
    assert!(is_white_space_single_line(0x09)); // Tab
    assert!(is_white_space_single_line(0x0B)); // Vertical tab
    assert!(is_white_space_single_line(0x0C)); // Form feed
    assert!(is_white_space_single_line(0xA0)); // Non-breaking space
    assert!(!is_white_space_single_line(0x0A)); // LF is not single-line whitespace
    assert!(!is_white_space_single_line(0x61)); // 'a'
}

#[test]
fn test_is_white_space_like() {
    // Includes both single-line and line breaks
    assert!(is_white_space_like(0x20)); // Space
    assert!(is_white_space_like(0x0A)); // LF
    assert!(is_white_space_like(0x0D)); // CR
    assert!(!is_white_space_like(0x61)); // 'a'
}

#[test]
fn test_is_digit() {
    assert!(is_digit('0' as u32));
    assert!(is_digit('5' as u32));
    assert!(is_digit('9' as u32));
    assert!(!is_digit('a' as u32));
    assert!(!is_digit('A' as u32));
}

#[test]
fn test_is_octal_digit() {
    assert!(is_octal_digit('0' as u32));
    assert!(is_octal_digit('7' as u32));
    assert!(!is_octal_digit('8' as u32));
    assert!(!is_octal_digit('9' as u32));
}

#[test]
fn test_is_hex_digit() {
    assert!(is_hex_digit('0' as u32));
    assert!(is_hex_digit('9' as u32));
    assert!(is_hex_digit('a' as u32));
    assert!(is_hex_digit('f' as u32));
    assert!(is_hex_digit('A' as u32));
    assert!(is_hex_digit('F' as u32));
    assert!(!is_hex_digit('g' as u32));
    assert!(!is_hex_digit('G' as u32));
}

#[test]
fn test_is_ascii_letter() {
    assert!(is_ascii_letter('a' as u32));
    assert!(is_ascii_letter('z' as u32));
    assert!(is_ascii_letter('A' as u32));
    assert!(is_ascii_letter('Z' as u32));
    assert!(!is_ascii_letter('0' as u32));
    assert!(!is_ascii_letter('_' as u32));
}

#[test]
fn test_is_word_character() {
    assert!(is_word_character('a' as u32));
    assert!(is_word_character('Z' as u32));
    assert!(is_word_character('0' as u32));
    assert!(is_word_character('_' as u32));
    assert!(!is_word_character('-' as u32));
    assert!(!is_word_character(' ' as u32));
}

#[test]
fn test_type_sizes() {
    use crate::parser::ast::*;
    use std::mem::size_of;

    // =========================================================================
    // SIZE ANALYSIS: Node enum variant sizes
    // To see sizes: run with RUST_TEST_PRINT=1 or check the panic message
    // =========================================================================

    // Building blocks (exact values verified)
    assert_eq!(size_of::<NodeIndex>(), 4, "NodeIndex should be 4 bytes");
    assert_eq!(
        size_of::<crate::interner::Atom>(),
        4,
        "Atom should be 4 bytes"
    );

    // Capture sizes for analysis
    let sizes = [
        ("NodeBase", size_of::<NodeBase>()),
        ("NodeList", size_of::<NodeList>()),
        ("Option<NodeList>", size_of::<Option<NodeList>>()),
        ("String", size_of::<String>()),
        ("Option<String>", size_of::<Option<String>>()),
        ("Vec<String>", size_of::<Vec<String>>()),
        // Largest variants
        ("SourceFile", size_of::<SourceFile>()),
        ("Identifier", size_of::<Identifier>()),
        ("FunctionDeclaration", size_of::<FunctionDeclaration>()),
        ("FunctionExpression", size_of::<FunctionExpression>()),
        ("ClassDeclaration", size_of::<ClassDeclaration>()),
        ("MethodDeclaration", size_of::<MethodDeclaration>()),
        ("ArrowFunction", size_of::<ArrowFunction>()),
        ("StringLiteral", size_of::<StringLiteral>()),
        // Medium
        ("BinaryExpression", size_of::<BinaryExpression>()),
        ("CallExpression", size_of::<CallExpression>()),
        ("IfStatement", size_of::<IfStatement>()),
        ("Block", size_of::<Block>()),
        // Small
        ("ReturnStatement", size_of::<ReturnStatement>()),
        ("EmptyStatement", size_of::<EmptyStatement>()),
        // Node enum
        ("Node", size_of::<crate::parser::Node>()),
        ("Type", size_of::<crate::checker::Type>()),
    ];

    // Build a report
    let mut report = String::from("\n\n=== SIZE ANALYSIS ===\n");
    for (name, size) in &sizes {
        report.push_str(&format!("{:25} {:4} bytes\n", name, size));
    }

    let node_size = size_of::<crate::parser::Node>();
    let type_size = size_of::<crate::checker::Type>();

    report.push_str(&format!(
        "\n📏 Node: {} bytes = {:.2} nodes/cache-line (target: 4)",
        node_size,
        64.0 / node_size as f64
    ));
    report.push_str(&format!("\n📏 Type: {} bytes\n", type_size));

    // Uncomment to see the report as a test "failure":
    // panic!("{}", report);

    // Verify constraints
    assert!(
        node_size <= 256,
        "Node enum too large: {} bytes{}",
        node_size,
        report
    );
    assert!(type_size <= 256, "Type enum too large: {} bytes", type_size);
}

/// Run this test to print the size analysis:
/// cargo test test_print_type_sizes -- --nocapture --ignored
#[test]
#[ignore]
fn test_print_type_sizes() {
    use crate::checker::types::type_def as types;
    use crate::parser::ast::*;
    use std::mem::size_of;

    eprintln!("\n=== NODE BUILDING BLOCKS ===");
    eprintln!("NodeBase:         {:4} bytes", size_of::<NodeBase>());
    eprintln!("NodeList:         {:4} bytes", size_of::<NodeList>());
    eprintln!("NodeIndex:        {:4} bytes", size_of::<NodeIndex>());
    eprintln!("String:           {:4} bytes", size_of::<String>());
    eprintln!(
        "Option<NodeList>: {:4} bytes",
        size_of::<Option<NodeList>>()
    );
    eprintln!("Vec<String>:      {:4} bytes", size_of::<Vec<String>>());

    eprintln!("\n=== NODE LARGEST VARIANTS ===");
    eprintln!("SourceFile:          {:4} bytes", size_of::<SourceFile>());
    eprintln!("Identifier:          {:4} bytes", size_of::<Identifier>());
    eprintln!(
        "FunctionDeclaration: {:4} bytes",
        size_of::<FunctionDeclaration>()
    );
    eprintln!(
        "FunctionExpression:  {:4} bytes",
        size_of::<FunctionExpression>()
    );
    eprintln!(
        "ClassDeclaration:    {:4} bytes",
        size_of::<ClassDeclaration>()
    );
    eprintln!(
        "MethodDeclaration:   {:4} bytes",
        size_of::<MethodDeclaration>()
    );
    eprintln!(
        "ArrowFunction:       {:4} bytes",
        size_of::<ArrowFunction>()
    );
    eprintln!(
        "StringLiteral:       {:4} bytes",
        size_of::<StringLiteral>()
    );

    eprintln!("\n=== NODE MEDIUM VARIANTS ===");
    eprintln!(
        "BinaryExpression: {:4} bytes",
        size_of::<BinaryExpression>()
    );
    eprintln!("CallExpression:   {:4} bytes", size_of::<CallExpression>());
    eprintln!("IfStatement:      {:4} bytes", size_of::<IfStatement>());
    eprintln!("Block:            {:4} bytes", size_of::<Block>());

    eprintln!("\n=== NODE SMALL VARIANTS ===");
    eprintln!("ReturnStatement: {:4} bytes", size_of::<ReturnStatement>());
    eprintln!("EmptyStatement:  {:4} bytes", size_of::<EmptyStatement>());

    eprintln!("\n=== TYPE VARIANTS (determines enum size) ===");
    eprintln!(
        "IntrinsicType:       {:4} bytes  (NOT boxed)",
        size_of::<types::IntrinsicType>()
    );
    eprintln!(
        "LiteralType:         {:4} bytes  (NOT boxed)",
        size_of::<types::LiteralType>()
    );
    eprintln!(
        "LiteralValue:        {:4} bytes",
        size_of::<types::LiteralValue>()
    );
    eprintln!(
        "ObjectType:          {:4} bytes  (boxed → 8)",
        size_of::<types::ObjectType>()
    );
    eprintln!(
        "TypeReference:       {:4} bytes  (boxed → 8)",
        size_of::<types::TypeReference>()
    );
    eprintln!(
        "UnionType:           {:4} bytes  (boxed → 8)",
        size_of::<types::UnionType>()
    );
    eprintln!(
        "IntersectionType:    {:4} bytes  (boxed → 8)",
        size_of::<types::IntersectionType>()
    );
    eprintln!(
        "TypeParameter:       {:4} bytes  (boxed → 8)",
        size_of::<types::TypeParameter>()
    );
    eprintln!(
        "ConditionalType:     {:4} bytes  (boxed → 8)",
        size_of::<types::ConditionalType>()
    );
    eprintln!(
        "MappedType:          {:4} bytes  (boxed → 8)",
        size_of::<types::MappedType>()
    );
    eprintln!(
        "IndexedAccessType:   {:4} bytes  (boxed → 8)",
        size_of::<types::IndexedAccessType>()
    );
    eprintln!(
        "IndexType:           {:4} bytes  (boxed → 8)",
        size_of::<types::IndexType>()
    );
    eprintln!(
        "TemplateLiteralType: {:4} bytes  (boxed → 8)",
        size_of::<types::TemplateLiteralType>()
    );
    eprintln!(
        "FunctionType:        {:4} bytes  (boxed → 8)",
        size_of::<types::FunctionType>()
    );
    eprintln!(
        "ArrayTypeInfo:       {:4} bytes  (boxed → 8)",
        size_of::<types::ArrayTypeInfo>()
    );
    eprintln!(
        "TupleTypeInfo:       {:4} bytes  (boxed → 8)",
        size_of::<types::TupleTypeInfo>()
    );
    eprintln!(
        "EnumTypeInfo:        {:4} bytes  (boxed → 8)",
        size_of::<types::EnumTypeInfo>()
    );
    eprintln!(
        "ThisTypeMarker:      {:4} bytes  (boxed → 8)",
        size_of::<types::ThisTypeMarker>()
    );
    eprintln!(
        "UniqueSymbolType:    {:4} bytes  (boxed → 8)",
        size_of::<types::UniqueSymbolType>()
    );

    eprintln!("\n=== TOTAL ===");
    let node_size = size_of::<crate::parser::Node>();
    let type_size = size_of::<crate::checker::Type>();
    eprintln!(
        "📏 Node enum: {:4} bytes ({:.2} nodes/cache-line)",
        node_size,
        64.0 / node_size as f64
    );
    eprintln!(
        "📏 Type enum: {:4} bytes ({:.2} types/cache-line)",
        type_size,
        64.0 / type_size as f64
    );
    eprintln!("   Node target: 16 bytes (4 nodes/cache-line) - use ThinNode");
    eprintln!("   Type is OK: already boxed, 48 bytes = 1.33 types/cache-line\n");
}

#[cfg(target_arch = "wasm32")]
#[test]
fn test_get_code_actions_with_context_missing_import() {
    let mut parser = ThinParser::new("b.ts".to_string(), "foo();\n".to_string());
    parser.parse_source_file();

    let diag = LspDiagnostic {
        range: Range::new(Position::new(0, 0), Position::new(0, 3)),
        severity: Some(DiagnosticSeverity::Error),
        code: Some(CANNOT_FIND_NAME),
        source: None,
        message: "Cannot find name 'foo'.".to_string(),
        related_information: None,
    };

    let diagnostics = serde_wasm_bindgen::to_value(&vec![diag]).unwrap();
    let only = JsValue::UNDEFINED;

    let import_candidates = vec![json!({
        "kind": "named",
        "moduleSpecifier": "./a",
        "localName": "foo",
        "exportName": "foo"
    })];
    let import_candidates = serde_wasm_bindgen::to_value(&import_candidates).unwrap();

    let actions_value = parser
        .get_code_actions_with_context(0, 0, 0, 0, diagnostics, only, import_candidates)
        .unwrap();
    let actions: Vec<Value> = serde_wasm_bindgen::from_value(actions_value).unwrap();

    let action = actions
        .iter()
        .find(|action| {
            action.get("title").and_then(Value::as_str) == Some("Import 'foo' from './a'")
        })
        .expect("Expected missing import code action");

    let edit = action.get("edit").expect("Expected workspace edit");
    let changes = edit.get("changes").expect("Expected changes");
    let edits = changes
        .get("b.ts")
        .and_then(Value::as_array)
        .expect("Expected edits for b.ts");

    let new_text = edits[0]
        .get("newText")
        .and_then(Value::as_str)
        .expect("Expected newText");

    assert_eq!(new_text, "import { foo } from \"./a\";\n");
}

#[test]
fn test_set_compiler_options() {
    let mut parser = ThinParser::new("test.ts".to_string(), "const x = 1;".to_string());

    // Test setting compiler options with valid JSON
    let json = r#"{
        "strict": true,
        "noImplicitAny": true,
        "strictNullChecks": false,
        "strictFunctionTypes": true,
        "target": "ES2015",
        "module": "ESNext"
    }"#;

    let result = parser.set_compiler_options(json.to_string());
    assert!(result.is_ok(), "Should successfully set compiler options");

    // Verify the options were stored
    assert!(parser.compiler_options.is_some());
}

#[test]
fn test_set_compiler_options_invalid_json() {
    let mut parser = ThinParser::new("test.ts".to_string(), "const x = 1;".to_string());

    // Test setting compiler options with invalid JSON
    let json = r#"{ invalid json }"#;

    let result = parser.set_compiler_options(json.to_string());
    assert!(result.is_err(), "Should fail with invalid JSON");
}

#[test]
fn test_mark_as_lib_file() {
    let mut parser = ThinParser::new("test.ts".to_string(), "const x = 1;".to_string());

    // Mark a few files as lib files
    parser.mark_as_lib_file(1);
    parser.mark_as_lib_file(2);
    parser.mark_as_lib_file(3);

    // Verify they were added to the set
    assert_eq!(parser.lib_file_ids.len(), 3);
    assert!(parser.lib_file_ids.contains(&1));
    assert!(parser.lib_file_ids.contains(&2));
    assert!(parser.lib_file_ids.contains(&3));
    assert!(!parser.lib_file_ids.contains(&4));

    // Test adding the same file ID multiple times (should not duplicate)
    parser.mark_as_lib_file(1);
    assert_eq!(parser.lib_file_ids.len(), 3);
}
