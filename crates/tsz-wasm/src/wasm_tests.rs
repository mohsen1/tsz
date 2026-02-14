use serde_json::Value;
use tsz_solver::TypeInterner;

use crate::wasm_api::diagnostics::{
    diagnostic_category_name, flatten_diagnostic_message_text, format_ts_diagnostic,
    format_ts_diagnostics_with_color_and_context,
};
use crate::wasm_api::emit::{transpile, transpile_module};
use crate::wasm_api::enums::DiagnosticCategory;
use crate::wasm_api::language_service::TsLanguageService;
use crate::wasm_api::program::create_ts_program;
use crate::wasm_api::utilities::{
    create_source_file, is_keyword, is_punctuation, parse_config_file_text_to_json,
    parse_json_text, syntax_kind_to_name, token_to_string,
};
use crate::{TsDiagnostic, TsProgram, TsSourceFile, TsSymbol, TsType};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

#[test]
fn test_type_interner_basic() {
    use tsz_solver::TypeId;

    // Test the underlying TypeInterner directly (works on all targets)
    let interner = TypeInterner::new();

    // Should start empty (no user-defined types, only intrinsics)
    assert!(interner.is_empty());
    let initial_count = interner.len();
    assert_eq!(
        initial_count,
        TypeId::FIRST_USER as usize,
        "TypeInterner should have intrinsics"
    );

    // Intern a string
    let atom1 = interner.intern_string("hello");
    let atom2 = interner.intern_string("hello");
    assert_eq!(atom1, atom2); // Deduplication

    // Resolve the string
    let resolved = interner.resolve_atom(atom1);
    assert_eq!(resolved, "hello");

    // Intern a literal type - this should make it non-empty
    let _str_type = interner.literal_string("test");
    assert!(!interner.is_empty());
    assert!(interner.len() > initial_count);
}

#[test]
fn test_parallel_parsing() {
    // Test the parallel parsing directly (works on all targets)
    let files = vec![
        ("a.ts".to_string(), "let x = 1;".to_string()),
        ("b.ts".to_string(), "let y = 2;".to_string()),
    ];

    let results = tsz::parallel::parse_files_parallel(files);
    assert_eq!(results.len(), 2);
}

#[test]
fn test_parallel_compile_and_check() {
    // Test the full pipeline directly (works on all targets)
    let files = vec![
        (
            "a.ts".to_string(),
            "function add(x: number, y: number): number { return x + y; }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "function mul(x: number, y: number): number { return x * y; }".to_string(),
        ),
    ];

    let program = tsz::parallel::compile_files(files);
    assert_eq!(program.files.len(), 2);

    let (_result, stats) = tsz::parallel::check_functions_with_stats(&program);
    assert_eq!(stats.file_count, 2);
    assert!(stats.function_count >= 2);
}

#[test]
fn test_ts_program_json_diagnostics_and_diagnostic_codes() {
    let mut program = TsProgram::new();
    program.set_compiler_options("{\"strict\":false}").unwrap();
    program.add_source_file("test.ts".to_string(), "const x: number = ;".to_string());

    let syntax = program.get_syntactic_diagnostics_json(None);
    let syntax_json: Vec<Value> = serde_json::from_str(&syntax).unwrap();
    assert!(!syntax_json.is_empty());

    let all_codes = program.get_all_diagnostic_codes();
    assert!(!all_codes.is_empty());
}

#[test]
fn test_create_ts_program_factory_smoke() {
    let program = create_ts_program(
        r#"["index.ts"]"#,
        "{\"strict\":true}",
        r#"{"index.ts":"const add = (x: number, y: number) => x + y;"}"#,
    )
    .unwrap();

    assert_eq!(program.get_source_file_count(), 1);
}

#[test]
fn test_create_ts_program_factory_roundtrip_options() {
    let program = create_ts_program(
        r#"["index.ts"]"#,
        "{\"strict\":true,\"allowJs\":true}",
        r#"{"index.ts":"const answer = 42;"}"#,
    )
    .unwrap();

    let options = serde_json::from_str::<Value>(&program.get_compiler_options_json()).unwrap();
    assert_eq!(options["strict"], true);
}

#[test]
fn test_ts_program_semantic_diagnostics_contract() {
    let mut program = TsProgram::new();
    program.set_compiler_options("{\"strict\":true}").unwrap();
    program.add_source_file(
        "bad.ts".to_string(),
        "const x: number = \"bad\";".to_string(),
    );

    let syntax = program.get_syntactic_diagnostics_json(None);
    let syntax_json: Vec<Value> = serde_json::from_str(&syntax).unwrap();
    assert!(syntax_json.is_empty());

    let semantic = program.get_semantic_diagnostics_json(None);
    let semantic_json: Vec<Value> = serde_json::from_str(&semantic).unwrap();
    assert!(
        semantic_json
            .iter()
            .any(|diag| diag.get("code").and_then(|v| v.as_u64()) == Some(2322)),
        "Expected TS2322 diagnostics in {semantic}"
    );
}

#[test]
fn test_wasm_language_service_is_callable() {
    let service = TsLanguageService::new("mod.ts".to_string(), "const value = 1;".to_string());
    let completions =
        serde_json::from_str::<Value>(&service.get_completions_at_position(0, 0)).unwrap();
    assert!(completions.is_array());
}

#[test]
fn test_ts_source_file_node_api_contracts() {
    let mut source_file = TsSourceFile::new("mod.tsx".to_string(), "const x = 1;".to_string());

    assert_eq!(source_file.file_name(), "mod.tsx");
    assert_eq!(
        source_file.language_version(),
        crate::wasm_api::enums::ScriptTarget::ESNext
    );
    assert_eq!(source_file.end() as usize, "const x = 1;".len());
    assert!(source_file.is_declaration_file() == false);

    let root = source_file.get_root_handle();
    assert_ne!(root, u32::MAX);
    let statements = source_file.get_statement_handles();
    assert!(!statements.is_empty());

    let first = statements[0];
    assert_eq!(source_file.get_node_pos(first), 0);
    assert_eq!(source_file.get_node_text(first), "const x = 1;");
    assert!(source_file.get_node_end(first) as usize >= 11);
    assert_eq!(source_file.get_node_text(root), source_file.text());
}

#[test]
fn test_type_checker_contracts() {
    let mut program = TsProgram::new();
    program.set_compiler_options("{\"strict\":true}").unwrap();
    program.add_source_file(
        "test.ts".to_string(),
        "const value: number = 42;".to_string(),
    );

    let checker = program.get_type_checker();
    assert_eq!(checker.type_to_string(checker.get_number_type()), "number");
    assert_eq!(checker.type_to_string(checker.get_string_type()), "string");
    assert!(checker.is_type_assignable_to(checker.get_number_type(), checker.get_any_type()));
    assert!(!checker.is_type_assignable_to(checker.get_string_type(), checker.get_number_type()));
    assert_eq!(checker.get_type_flags(checker.get_boolean_type()), 16);
}

#[test]
fn test_transpile_helpers_emit_contracts() {
    let output = transpile("const n: number = 1;", Some(1), Some(1));
    assert!(output.contains("n = 1"));

    let json = transpile_module("const n: number = 1;", "{}");
    let parsed: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["diagnostics"].as_array().unwrap().len(), 0);
    assert!(parsed["outputText"].as_str().unwrap().contains("var n"));
}

#[test]
fn test_json_and_syntax_kind_utilities_contracts() {
    let parsed = parse_json_text("{ // comment\n  \"ok\": true\n}");
    let parsed_value: Value = serde_json::from_str(&parsed).unwrap();
    assert_eq!(parsed_value["ok"], true);

    let bad_config = parse_config_file_text_to_json("tsconfig.json", "{ strict: true }");
    let bad_config_value: Value = serde_json::from_str(&bad_config).unwrap();
    assert!(bad_config_value["error"].is_object());
    assert_eq!(bad_config_value["error"]["code"], 5083);

    let good_config = parse_config_file_text_to_json(
        "tsconfig.json",
        "{ \"compilerOptions\": { \"strict\": true } }",
    );
    let good_config_value: Value = serde_json::from_str(&good_config).unwrap();
    assert!(good_config_value["error"].is_null());
    assert!(!good_config_value["config"].is_null());

    assert_eq!(syntax_kind_to_name(14), "RegularExpressionLiteral");
}

#[test]
fn test_wasm_utility_kind_predicates_and_token_text() {
    assert!(is_keyword(SyntaxKind::ClassKeyword as u16));
    assert!(is_punctuation(SyntaxKind::SlashToken as u16));
    assert_eq!(token_to_string(42), Some("/".to_string()));
    assert!(is_keyword(SyntaxKind::ClassKeyword as u16));
}

#[test]
fn test_wasm_source_file_factory_contract() {
    let mut source_file = create_source_file("mod.tsx", "const value = 1;", None);

    assert_eq!(source_file.file_name(), "mod.tsx");
    let root = source_file.get_root_handle();
    assert_ne!(root, u32::MAX);
    assert!(!source_file.get_statement_handles().is_empty());
}

#[test]
fn test_diagnostic_formatting_contracts() {
    let diagnostic = TsDiagnostic::new(
        Some("mod.ts".to_string()),
        0,
        1,
        "message".to_string(),
        DiagnosticCategory::Error,
        12345,
    );

    assert_eq!(
        diagnostic_category_name(DiagnosticCategory::Warning),
        "Warning"
    );
    assert_eq!(
        flatten_diagnostic_message_text("message text", "\n"),
        "message text"
    );

    let rendered = format_ts_diagnostic(&diagnostic, "\n");
    assert!(rendered.contains("12345"));
    assert!(rendered.contains("mod.ts"));

    let diagnostics_json = serde_json::json!([
        {
            "file_name": "mod.ts",
            "start": 0,
            "length": 1,
            "message_text": "message",
            "category": 1,
            "code": 12345
        }
    ])
    .to_string();

    let sources_json = serde_json::json!({ "mod.ts": "const value = 1;" }).to_string();
    let full =
        format_ts_diagnostics_with_color_and_context(&diagnostics_json, &sources_json, false);
    assert!(full.contains("12345"));
}

#[test]
fn test_source_file_child_navigation_contract() {
    let mut source_file = TsSourceFile::new(
        "mod.ts".to_string(),
        "const n = 1;\nfunction f(x: number) { return x; }".to_string(),
    );

    let root = source_file.get_root_handle();
    assert_ne!(root, u32::MAX);
    let statements = source_file.get_statement_handles();
    assert_eq!(statements.len(), 2);

    let first = statements[0];
    let children = source_file.get_child_handles(first);
    assert!(!children.is_empty());
    let node_kind = source_file.get_node_kind(first);
    assert!(node_kind > 0);
    assert_eq!(
        source_file.get_node_text(root),
        "const n = 1;\nfunction f(x: number) { return x; }"
    );
}

#[test]
fn test_diagnostic_type_contracts() {
    let diagnostic = TsDiagnostic::new(
        Some("mod.ts".to_string()),
        2,
        3,
        "test diagnostic".to_string(),
        DiagnosticCategory::Error,
        9999,
    );

    assert_eq!(diagnostic.file_name(), Some("mod.ts".to_string()));
    assert_eq!(diagnostic.start(), 2);
    assert_eq!(diagnostic.length(), 3);
    assert_eq!(diagnostic.code(), 9999);
    assert!(diagnostic.is_error());
    assert!(!diagnostic.is_warning());

    let json = serde_json::from_str::<Value>(&diagnostic.to_json()).unwrap();
    assert_eq!(json["code"], 9999);
    assert_eq!(json["category"], 1);
}

#[test]
fn test_type_and_symbol_predicate_contracts() {
    let any_type = TsType::new(TypeId::ANY.0, 1);
    assert_eq!(any_type.handle(), TypeId::ANY.0);
    assert!(any_type.is_any());

    let string_type = TsType::new(TypeId::STRING.0, 1 << 2);
    assert!(string_type.is_string());
    assert!(string_type.flags() > 0);

    let symbol = TsSymbol::new(7, 1 << 4, "value".to_string());
    assert_eq!(symbol.handle(), 7);
    assert_eq!(symbol.name(), "value");
    assert!(symbol.is_function());
}

#[test]
fn test_language_service_hover_and_definition_contracts() {
    let service = TsLanguageService::new(
        "mod.ts".to_string(),
        "const x: number = 1; function f() { return x; }".to_string(),
    );
    let quick = service.get_quick_info_at_position(0, 7);
    let quick_value: Value = serde_json::from_str(&quick).unwrap();
    assert!(quick_value.is_object() || quick_value.is_null());

    let definitions = service.get_definition_at_position(0, 6);
    let defs: Vec<Value> = serde_json::from_str(&definitions).unwrap();
    assert!(
        defs.iter()
            .all(|d| d.get("fileName").is_some() || d.is_object())
    );
}
