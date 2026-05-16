use serde_json::Value;
use tsz_solver::TypeInterner;

use crate::wasm_api::diagnostics::{
    diagnostic_category_name, flatten_diagnostic_message_text, format_ts_diagnostic,
    format_ts_diagnostics_with_color_and_context,
};
use crate::wasm_api::emit::{transpile, transpile_module};
use crate::wasm_api::enums::DiagnosticCategory;
use crate::wasm_api::language_service::TsLanguageService;
use crate::wasm_api::program::{TsCompilerOptions, create_ts_program};
use crate::wasm_api::utilities::{
    create_source_file, is_keyword, is_punctuation, parse_config_file_text_to_json,
    parse_json_text, scan_tokens, syntax_kind_to_name, token_to_string,
};
use crate::{Parser, TsDiagnostic, TsProgram, TsSourceFile, TsSymbol, TsType};
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
fn ts_program_accepts_nested_anonymous_object_literal_assignment() {
    let mut program = TsProgram::new();
    program.set_compiler_options("{\"strict\":true}").unwrap();
    program.add_source_file(
        "input.ts".to_string(),
        r#"
interface User {
  id: string,
  profile: {
    name: string,
    admin: boolean
  }
}

const user: User = {
  id: "u1",
  profile: {
    name: "ada",
    admin: true
  }
}
"#
        .to_string(),
    );

    let semantic = program.get_semantic_diagnostics_json(None);
    let semantic_json: Vec<Value> = serde_json::from_str(&semantic).unwrap();
    assert!(
        semantic_json.is_empty(),
        "expected nested anonymous object literal assignment to be diagnostic-free, got {semantic}"
    );
}

#[test]
fn test_ts_program_diagnostics_are_stable_after_parser_parse() {
    fn diagnostic_codes(json: &str) -> Vec<u64> {
        let diagnostics: Vec<Value> = serde_json::from_str(json).expect("valid diagnostics json");
        diagnostics
            .into_iter()
            .filter_map(|diag| diag.get("code").and_then(Value::as_u64))
            .collect()
    }

    let source = r#"let x: string = 42;

function greet(name: string): string {
  return "Hello, " + name;
}

greet(123);

interface User {
  name: string;
  age: number;
}

const user: User = {
  name: "Alice",
  age: "thirty",
};
"#;

    let mut baseline = TsProgram::new();
    baseline
        .set_compiler_options("{\"strict\":true,\"module\":99}")
        .unwrap();
    baseline.add_source_file("input.ts".to_string(), source.to_string());
    let baseline_codes = diagnostic_codes(&baseline.get_pre_emit_diagnostics_json());

    assert!(
        !baseline_codes.is_empty(),
        "expected baseline diagnostics for sample source"
    );

    let mut parser = Parser::new("input.ts".to_string(), source.to_string());
    parser
        .set_compiler_options("{\"strict\":true,\"module\":99}")
        .unwrap();
    parser.parse_source_file();

    let mut after_parser = TsProgram::new();
    after_parser
        .set_compiler_options("{\"strict\":true,\"module\":99}")
        .unwrap();
    after_parser.add_source_file("input.ts".to_string(), source.to_string());
    let after_parser_codes = diagnostic_codes(&after_parser.get_pre_emit_diagnostics_json());

    assert_eq!(
        baseline_codes, after_parser_codes,
        "TsProgram diagnostics changed after constructing/parsing a Parser"
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
    assert!(!source_file.is_declaration_file());

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
fn test_transpile_preserves_empty_module_from_ast() {
    let output = transpile("export{};", Some(1), Some(5));
    assert_eq!(output, "export {};\n");

    let json = transpile_module("import type { T } from './types';", r#"{"module":5}"#);
    let parsed: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["outputText"].as_str().unwrap(), "export {};\n");
    assert_eq!(parsed["diagnostics"].as_array().unwrap().len(), 0);
}

#[test]
fn test_transpile_ignores_module_words_in_trivia_and_strings() {
    let source = "// import type { T } from './types';\nconst text = 'export value';";
    let output = transpile(source, Some(1), Some(5));

    assert!(output.contains("text = 'export value'"));
    assert!(!output.contains("export {};"));
}

#[test]
fn test_transpile_module_reports_invalid_options_json() {
    let json = transpile_module("const n = 1;", "{ invalid json");
    let parsed: Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["outputText"].as_str().unwrap(), "");
    let diagnostics = parsed["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0]["category"].as_u64().unwrap(), 1);
    assert!(
        diagnostics[0]["message"]
            .as_str()
            .unwrap()
            .contains("Invalid transpile options JSON")
    );
}

#[test]
fn test_transpile_module_accepts_file_name_option() {
    let json = transpile_module(
        "export{};",
        r#"{"module":5,"fileName":"virtual/input.mts"}"#,
    );
    let parsed: Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["outputText"].as_str().unwrap(), "export {};\n");
    assert_eq!(parsed["diagnostics"].as_array().unwrap().len(), 0);
}

#[test]
fn test_transpile_module_emits_external_source_map() {
    let json = transpile_module(
        "const n: number = 1;\n",
        r#"{"target":1,"sourceMap":true,"fileName":"virtual/input.ts"}"#,
    );
    let parsed: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["diagnostics"].as_array().unwrap().len(), 0);

    let output = parsed["outputText"].as_str().unwrap();
    // Source map URL comment must be appended and reference the JS basename.
    assert!(
        output.contains("//# sourceMappingURL=input.js.map"),
        "missing sourceMappingURL comment in output: {output:?}"
    );
    // Inline base64 map MUST NOT be present in the external-map case.
    assert!(
        !output.contains("data:application/json;base64,"),
        "external sourceMap should not inline the map: {output:?}"
    );

    let map_text = parsed["sourceMapText"]
        .as_str()
        .expect("sourceMapText should be set when sourceMap is requested");
    let map: Value = serde_json::from_str(map_text).expect("sourceMapText is valid JSON");
    assert_eq!(map["version"].as_u64().unwrap(), 3);
    assert_eq!(map["file"].as_str().unwrap(), "input.js");
    let sources: Vec<&str> = map["sources"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(sources, vec!["input.ts"]);
    assert!(!map["mappings"].as_str().unwrap().is_empty());
}

#[test]
fn test_transpile_module_emits_inline_source_map() {
    let json = transpile_module(
        "const n: number = 1;\n",
        r#"{"target":1,"inlineSourceMap":true,"fileName":"input.ts"}"#,
    );
    let parsed: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["diagnostics"].as_array().unwrap().len(), 0);

    let output = parsed["outputText"].as_str().unwrap();
    assert!(
        output.contains("//# sourceMappingURL=data:application/json;base64,"),
        "missing inline sourceMappingURL data URL in output: {output:?}"
    );
    // External `.map` URL must not be present in the inline case.
    assert!(
        !output.contains("//# sourceMappingURL=input.js.map"),
        "inline sourceMap should not reference an external .map: {output:?}"
    );
    // Inline form keeps the map embedded; the separate field should be absent.
    assert!(parsed["sourceMapText"].is_null());
}

#[test]
fn test_transpile_module_omits_source_map_when_not_requested() {
    let json = transpile_module(
        "const n: number = 1;\n",
        r#"{"target":1,"fileName":"input.ts"}"#,
    );
    let parsed: Value = serde_json::from_str(&json).unwrap();
    let output = parsed["outputText"].as_str().unwrap();

    assert!(parsed["sourceMapText"].is_null());
    assert!(
        !output.contains("//# sourceMappingURL"),
        "no sourceMappingURL should be emitted without sourceMap option: {output:?}"
    );
}

#[test]
fn test_ts_program_emit_json_uses_module_file_extensions() {
    let mut program = TsProgram::new();
    program
        .set_compiler_options(r#"{"target":2,"module":5}"#)
        .unwrap();
    program.add_source_file(
        "entry.mts".to_string(),
        "export const value: number = 1;\n".to_string(),
    );
    program.add_source_file(
        "worker.cts".to_string(),
        "export const value: number = 1;\n".to_string(),
    );

    let json = program.emit_json();
    let parsed: Value = serde_json::from_str(&json).unwrap();
    let emitted_names: Vec<&str> = parsed["emittedFiles"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|file| file["name"].as_str())
        .collect();

    assert!(emitted_names.contains(&"entry.mjs"), "{emitted_names:?}");
    assert!(emitted_names.contains(&"worker.cjs"), "{emitted_names:?}");
    assert!(
        !emitted_names.contains(&"entry.mts.js"),
        "{emitted_names:?}"
    );
    assert!(
        !emitted_names.contains(&"worker.cts.js"),
        "{emitted_names:?}"
    );
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
fn test_wasm_scan_tokens_returns_token_stream() {
    let json = scan_tokens("const x = 1;");
    let tokens: Vec<Value> = serde_json::from_str(&json).unwrap();

    assert!(
        tokens.len() >= 5,
        "expected token stream for `const x = 1;`, got {tokens:?}"
    );

    // Trivia should be skipped: there should be no whitespace token
    // between `const` and `x`.
    let kinds: Vec<u64> = tokens.iter().map(|t| t["kind"].as_u64().unwrap()).collect();
    assert_eq!(kinds[0], SyntaxKind::ConstKeyword as u64);
    assert_eq!(kinds[1], SyntaxKind::Identifier as u64);
    assert_eq!(kinds[2], SyntaxKind::EqualsToken as u64);
    assert_eq!(kinds[3], SyntaxKind::NumericLiteral as u64);
    assert_eq!(kinds[4], SyntaxKind::SemicolonToken as u64);

    // Spans round-trip back to the source text and are non-decreasing.
    let mut last_end: u64 = 0;
    for token in &tokens {
        let start = token["start"].as_u64().unwrap();
        let end = token["end"].as_u64().unwrap();
        let text = token["text"].as_str().unwrap();
        assert!(start >= last_end, "tokens overlap: {tokens:?}");
        assert!(end > start, "empty-span token: {token:?}");
        assert_eq!(
            &"const x = 1;"[start as usize..end as usize],
            text,
            "token text does not match span: {token:?}"
        );
        last_end = end;
    }

    // EOF is not emitted as an explicit token in the stream.
    assert!(
        kinds
            .iter()
            .all(|k| *k != SyntaxKind::EndOfFileToken as u64),
        "EOF should not appear in the token stream: {kinds:?}"
    );
}

#[test]
fn test_wasm_scan_tokens_empty_input() {
    let json = scan_tokens("");
    let tokens: Vec<Value> = serde_json::from_str(&json).unwrap();
    assert!(tokens.is_empty());
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
fn test_source_file_declaration_file_extension_contract() {
    for file_name in [
        "index.d.ts",
        "index.d.mts",
        "index.d.cts",
        "style.d.css.ts",
        "types/INDEX.D.MTS",
    ] {
        let source_file = TsSourceFile::new(
            file_name.to_string(),
            "declare const value: string;".to_string(),
        );
        assert!(source_file.is_declaration_file(), "{file_name}");
    }

    for file_name in ["index.ts", "index.tsx", "style.css.ts", "index.d.tsx"] {
        let source_file = TsSourceFile::new(file_name.to_string(), "const value = 1;".to_string());
        assert!(!source_file.is_declaration_file(), "{file_name}");
    }
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

#[test]
fn test_byte_offset_to_utf16_conversion() {
    use crate::wasm_api::program::TsProgram;

    // ASCII-only: byte offset = UTF-16 offset
    assert_eq!(TsProgram::byte_offset_to_utf16("hello world", 5), 5);
    assert_eq!(TsProgram::byte_offset_to_utf16("hello world", 0), 0);

    // Em dash (U+2014) is 3 bytes in UTF-8 but 1 UTF-16 code unit
    // "ab—cd" = [61 62 E2 80 94 63 64] (7 bytes, 5 chars)
    let s = "ab\u{2014}cd";
    assert_eq!(s.len(), 7); // 7 UTF-8 bytes
    assert_eq!(s.chars().count(), 5); // 5 characters
    assert_eq!(TsProgram::byte_offset_to_utf16(s, 0), 0); // before 'a'
    assert_eq!(TsProgram::byte_offset_to_utf16(s, 2), 2); // before em dash
    assert_eq!(TsProgram::byte_offset_to_utf16(s, 5), 3); // after em dash (byte 5 = char 3)
    assert_eq!(TsProgram::byte_offset_to_utf16(s, 6), 4); // after 'c'
    assert_eq!(TsProgram::byte_offset_to_utf16(s, 7), 5); // end

    // Supplementary character (U+1F600, emoji) is 4 bytes UTF-8, 2 UTF-16 code units
    let s2 = "a\u{1F600}b";
    assert_eq!(s2.len(), 6); // 1 + 4 + 1 = 6 UTF-8 bytes
    assert_eq!(TsProgram::byte_offset_to_utf16(s2, 0), 0);
    assert_eq!(TsProgram::byte_offset_to_utf16(s2, 1), 1); // after 'a'
    assert_eq!(TsProgram::byte_offset_to_utf16(s2, 5), 3); // after emoji (2 UTF-16 units)
    assert_eq!(TsProgram::byte_offset_to_utf16(s2, 6), 4); // end

    // byte_length_to_utf16
    assert_eq!(TsProgram::byte_length_to_utf16(s, 0, 2), 2); // "ab"
    assert_eq!(TsProgram::byte_length_to_utf16(s, 2, 3), 1); // em dash span (3 bytes = 1 char)
    assert_eq!(TsProgram::byte_length_to_utf16(s, 5, 2), 2); // "cd"
}

#[test]
fn test_ts_program_target_drives_semantic_diagnostics() {
    // Issue #3489: `target` from setCompilerOptions must reach the checker
    // so target-aware semantic diagnostics like TS2737 (BigInt literals
    // require ES2020+) actually fire. Previously the checker target was
    // hardcoded to the default and the option was silently dropped.

    // target=1 → ES5: BigInt literal not allowed → TS2737 expected.
    let mut es5 = TsProgram::new();
    es5.set_compiler_options(r#"{"target":1}"#).unwrap();
    es5.add_source_file("a.ts".to_string(), "const x = 1n;".to_string());
    let codes_es5 = es5.get_all_diagnostic_codes();
    assert!(
        codes_es5.contains(&2737),
        "ES5 target must surface TS2737 for BigInt literal, got {codes_es5:?}"
    );

    // target=7 → ES2020: BigInt literal allowed → no TS2737.
    let mut es2020 = TsProgram::new();
    es2020.set_compiler_options(r#"{"target":7}"#).unwrap();
    es2020.add_source_file("a.ts".to_string(), "const x = 1n;".to_string());
    let codes_es2020 = es2020.get_all_diagnostic_codes();
    assert!(
        !codes_es2020.contains(&2737),
        "ES2020 target must not surface TS2737 for BigInt literal, got {codes_es2020:?}"
    );
}

#[test]
fn test_ts_compiler_options_threads_allow_js_and_declaration() {
    // Issue #4748 / #4734: TsCompilerOptions.to_checker_options previously
    // hardcoded allow_js:false and emit_declarations:false, silently
    // dropping the user-supplied allowJs / declaration fields.

    let opts: TsCompilerOptions =
        serde_json::from_str(r#"{"allowJs":true,"declaration":true}"#).unwrap();
    let checker_opts = opts.to_checker_options();
    assert!(
        checker_opts.allow_js,
        "allowJs:true must propagate to CheckerOptions.allow_js",
    );
    assert!(
        checker_opts.emit_declarations,
        "declaration:true must propagate to CheckerOptions.emit_declarations",
    );

    let opts_off: TsCompilerOptions =
        serde_json::from_str(r#"{"allowJs":false,"declaration":false}"#).unwrap();
    let checker_opts_off = opts_off.to_checker_options();
    assert!(!checker_opts_off.allow_js);
    assert!(!checker_opts_off.emit_declarations);

    // Defaults remain false when fields are omitted.
    let opts_default: TsCompilerOptions = serde_json::from_str("{}").unwrap();
    let checker_opts_default = opts_default.to_checker_options();
    assert!(!checker_opts_default.allow_js);
    assert!(!checker_opts_default.emit_declarations);
}

#[test]
fn test_ts_program_emit_json_threads_declaration_and_source_map_flags() {
    // Issue #4748 / #4738: emit_json hardcoded per-file metadata to
    // declaration:false / sourceMap:false; verify the configured values
    // now flow through to emittedFiles entries.

    let mut program = TsProgram::new();
    program
        .set_compiler_options(r#"{"declaration":true,"sourceMap":true}"#)
        .unwrap();
    program.add_source_file(
        "entry.ts".to_string(),
        "export const value: number = 1;\n".to_string(),
    );

    let json = program.emit_json();
    let parsed: Value = serde_json::from_str(&json).unwrap();
    let files = parsed["emittedFiles"].as_array().unwrap();
    assert!(!files.is_empty(), "expected at least one emitted file");
    for file in files {
        assert_eq!(
            file["declaration"],
            Value::Bool(true),
            "declaration flag must reflect compiler options",
        );
        assert_eq!(
            file["sourceMap"],
            Value::Bool(true),
            "sourceMap flag must reflect compiler options",
        );
    }

    // When neither option is set, both flags stay false.
    let mut program_off = TsProgram::new();
    program_off.add_source_file(
        "entry.ts".to_string(),
        "export const value: number = 1;\n".to_string(),
    );
    let json_off = program_off.emit_json();
    let parsed_off: Value = serde_json::from_str(&json_off).unwrap();
    for file in parsed_off["emittedFiles"].as_array().unwrap() {
        assert_eq!(file["declaration"], Value::Bool(false));
        assert_eq!(file["sourceMap"], Value::Bool(false));
    }
}

fn completion_labels_at(source: &str, line: u32, character: u32) -> Vec<String> {
    let service = TsLanguageService::new("mod.ts".to_string(), source.to_string());
    let json = service.get_completions_at_position(line, character);
    let items: Vec<Value> = serde_json::from_str(&json).unwrap();
    items
        .into_iter()
        .filter_map(|item| item["label"].as_str().map(str::to_string))
        .collect()
}

#[test]
fn test_wasm_language_service_function_member_completions() {
    let cases: &[(&str, u32, u32, &str)] = &[
        (
            "function add(a, b) { return a + b; }\nadd.",
            1,
            4,
            "named-function",
        ),
        (
            "const fn = (x: number) => x * 2;\nfn.",
            1,
            3,
            "arrow-function",
        ),
        (
            "const mul = function(a: number, b: number) { return a * b; };\nmul.",
            1,
            4,
            "function-expression",
        ),
    ];
    for (source, line, character, desc) in cases {
        let labels = completion_labels_at(source, *line, *character);
        for expected in ["apply", "bind", "call", "length", "name"] {
            assert!(
                labels.iter().any(|l| l == expected),
                "expected `{expected}` in {desc} completions but got {labels:?}"
            );
        }
    }
}
