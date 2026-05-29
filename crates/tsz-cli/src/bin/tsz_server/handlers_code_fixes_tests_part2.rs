//! Code-fix test part 2: JSDoc imports, type-only merges, pnpm, async, interface, and JSX fixes.
use super::{LineMap, Server, TsServerRequest, positions_overlap};
use crate::handlers_code_fixes_utils::parse_identifier_call_expression;
use crate::{CheckOptions, LogConfig, LogLevel, ServerMode};
use rustc_hash::FxHashMap;
use std::path::PathBuf;
use tsz::checker::diagnostics::DiagnosticCategory;
use tsz::parser::ParserState;

fn make_server() -> Server {
    Server {
        completion_import_module_specifier_ending: None,
        import_module_specifier_preference: None,
        organize_imports_type_order: None,
        organize_imports_ignore_case: false,
        auto_import_file_exclude_patterns: Vec::new(),
        lib_dir: PathBuf::from("/nonexistent"),
        tests_lib_dir: PathBuf::from("/nonexistent"),
        lib_cache: FxHashMap::default(),
        unified_lib_cache: None,
        checks_completed: 0,
        response_seq: 0,
        open_files: FxHashMap::default(),
        external_project_files: FxHashMap::default(),
        _server_mode: ServerMode::Semantic,
        _log_config: LogConfig {
            level: LogLevel::Off,
            file: None,
            trace_to_console: false,
        },
        enable_telemetry: false,
        allow_importing_ts_extensions: false,
        inferred_check_options: CheckOptions::default(),
        inferred_projectinfo_options: None,
        auto_imports_allowed_for_inferred_projects: true,
        inferred_module_is_none_for_projects: false,
        auto_import_specifier_exclude_regexes: Vec::new(),
        include_completions_with_class_member_snippets: false,
        include_inlay_parameter_name_hints: None,
        generate_return_in_doc_template: None,
        new_line_character: None,
        plugin_configs: FxHashMap::default(),
        native_ts_worker: None,
        pending_events: Vec::new(),
    }
}

#[test]
fn handle_get_code_fixes_jsdoc_import_returns_single_missing_import_fix() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo.ts".to_string(),
        "export const A = 1;\nexport type B = { x: number };\nexport type C = 1;\nexport class D { y: string }\n".to_string(),
    );
    let test_js = "/**\n * @import { A, D, C } from \"./foo\"\n */\n\n/**\n * @param { typeof A } a\n * @param { B | C } b\n * @param { C } c\n * @param { D } d\n */\nexport function f(a, b, c, d) { }\n";
    server
        .open_files
        .insert("/test.js".to_string(), test_js.to_string());

    let diag_req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "semanticDiagnosticsSync".to_string(),
        arguments: serde_json::json!({
            "file": "/test.js",
            "includeLinePosition": true
        }),
    };
    let diag_resp = server.handle_semantic_diagnostics_sync(1, &diag_req);
    assert!(
        diag_resp.success,
        "expected semanticDiagnosticsSync to succeed"
    );
    let missing_name_diags: Vec<serde_json::Value> = diag_resp
        .body
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .expect("expected diagnostics array")
        .iter()
        .filter(|diag| {
            diag.get("code")
                .and_then(serde_json::Value::as_u64)
                .map(|code| code as u32)
                == Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME)
        })
        .cloned()
        .collect();
    assert_eq!(
        missing_name_diags.len(),
        1,
        "expected one cannot-find-name diagnostic in diagnostics flow, got {missing_name_diags:?}"
    );

    let mut import_fix_texts = Vec::new();
    for diag in &missing_name_diags {
        let code = diag
            .get("code")
            .and_then(serde_json::Value::as_u64)
            .expect("diagnostic code") as u32;
        let (start, end) =
            if let (Some(start_line), Some(start_offset), Some(end_line), Some(end_offset)) = (
                diag.get("start")
                    .and_then(|start| start.get("line"))
                    .and_then(serde_json::Value::as_u64),
                diag.get("start")
                    .and_then(|start| start.get("offset"))
                    .and_then(serde_json::Value::as_u64),
                diag.get("end")
                    .and_then(|end| end.get("line"))
                    .and_then(serde_json::Value::as_u64),
                diag.get("end")
                    .and_then(|end| end.get("offset"))
                    .and_then(serde_json::Value::as_u64),
            ) {
                (
                    tsz::lsp::position::Position::new(
                        (start_line as u32).saturating_sub(1),
                        (start_offset as u32).saturating_sub(1),
                    ),
                    tsz::lsp::position::Position::new(
                        (end_line as u32).saturating_sub(1),
                        (end_offset as u32).saturating_sub(1),
                    ),
                )
            } else {
                let start_off = diag
                    .get("start")
                    .and_then(serde_json::Value::as_u64)
                    .expect("diagnostic start offset") as u32;
                let length = diag
                    .get("length")
                    .and_then(serde_json::Value::as_u64)
                    .expect("diagnostic length") as u32;
                let line_map = super::LineMap::build(test_js);
                (
                    line_map.offset_to_position(start_off, test_js),
                    line_map.offset_to_position(start_off + length, test_js),
                )
            };
        let req = TsServerRequest {
            seq: 1,
            _msg_type: "request".to_string(),
            command: "getCodeFixes".to_string(),
            arguments: serde_json::json!({
                "file": "/test.js",
                "startLine": start.line + 1,
                "startOffset": start.character + 1,
                "endLine": end.line + 1,
                "endOffset": end.character + 1,
                "errorCodes": [code],
                "preferences": {
                    "preferTypeOnlyAutoImports": true
                }
            }),
        };
        let resp = server.handle_get_code_fixes(1, &req);
        assert!(resp.success, "expected getCodeFixes to succeed");
        let actions = resp
            .body
            .as_ref()
            .and_then(serde_json::Value::as_array)
            .expect("expected getCodeFixes actions");
        for action in actions {
            if action.get("fixName").and_then(serde_json::Value::as_str) != Some("import") {
                continue;
            }
            let Some(changes) = action.get("changes").and_then(serde_json::Value::as_array) else {
                continue;
            };
            let Some(file_change) = changes.first() else {
                continue;
            };
            let Some(text_changes) = file_change
                .get("textChanges")
                .and_then(serde_json::Value::as_array)
            else {
                continue;
            };
            let Some(new_text) = text_changes
                .first()
                .and_then(|change| change.get("newText"))
                .and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            import_fix_texts.push(new_text.to_string());
        }
    }

    assert_eq!(
        import_fix_texts.len(),
        1,
        "expected one import fix from diagnostics flow, got {import_fix_texts:?}"
    );
    assert!(
        import_fix_texts[0].contains("@import { A, D, C, B } from \"./foo\""),
        "expected JSDoc @import merge edit, got {:?}",
        import_fix_texts[0]
    );
}

#[test]
fn get_code_fixes_adds_missing_value_import_with_existing_type_only_import() {
    let mut server = make_server();
    server.open_files.insert(
        "/node_modules/react/index.d.ts".to_string(),
        "export interface ComponentType {}\nexport interface ComponentProps {}\nexport declare function useState<T>(initialState: T): [T, (newState: T) => void];\nexport declare function useEffect(callback: () => void, deps: any[]): void;\n".to_string(),
    );
    server.open_files.insert(
        "/main.ts".to_string(),
        "import type { ComponentType } from \"react\";\nimport { useState } from \"react\";\n\nexport function Component({ prop } : { prop: ComponentType }) {\n    const codeIsUnimportant = useState(1);\n    useEffect(() => {}, []);\n}\n".to_string(),
    );

    let content = server
        .open_files
        .get("/main.ts")
        .expect("missing main.ts")
        .clone();
    let line_map = LineMap::build(&content);
    let (_, binder, _, _) = server
        .parse_and_bind_file("/main.ts")
        .expect("expected parse_and_bind_file for /main.ts");
    let synthetic =
        server.synthetic_missing_name_expression_diagnostics("/main.ts", &content, &binder);
    assert!(
        synthetic.iter().any(|diag| {
            diag.code == tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME
                && diag.message_text.contains("useEffect")
                && {
                    let start = line_map.offset_to_position(diag.start, &content);
                    let end = line_map.offset_to_position(diag.start + diag.length, &content);
                    positions_overlap(
                        tsz::lsp::position::Position::new(5, 4),
                        tsz::lsp::position::Position::new(5, 13),
                        start,
                        end,
                    )
                }
        }),
        "expected synthetic cannot-find-name diagnostic for useEffect, got {synthetic:?}"
    );

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": "/main.ts",
            "startLine": 6,
            "startOffset": 5,
            "endLine": 6,
            "endOffset": 14,
            "errorCodes": [2304]
        }),
    };

    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected getCodeFixes to succeed");
    let body = resp.body.expect("expected getCodeFixes body");
    let fixes = body.as_array().expect("expected array response");
    let mut import_fix_texts = Vec::new();
    for fix in fixes {
        if fix.get("fixName").and_then(serde_json::Value::as_str) != Some("import") {
            continue;
        }
        let Some(changes) = fix.get("changes").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for change in changes {
            let Some(text_changes) = change
                .get("textChanges")
                .and_then(serde_json::Value::as_array)
            else {
                continue;
            };
            for text_change in text_changes {
                if let Some(new_text) = text_change
                    .get("newText")
                    .and_then(serde_json::Value::as_str)
                {
                    import_fix_texts.push(new_text.to_string());
                }
            }
        }
    }

    assert!(
        import_fix_texts
            .iter()
            .any(|text| text.contains("useEffect")),
        "expected import fix text to include useEffect, got {import_fix_texts:?}"
    );
}

#[test]
fn get_code_fixes_prefers_merging_type_only_import_into_type_clause() {
    let mut server = make_server();
    server.open_files.insert(
        "/node_modules/react/index.d.ts".to_string(),
        "export interface ComponentType {}\nexport interface ComponentProps {}\nexport declare function useState<T>(initialState: T): [T, (newState: T) => void];\n".to_string(),
    );
    server.open_files.insert(
        "/main2.ts".to_string(),
        "import { useState } from \"react\";\nimport type { ComponentType } from \"react\";\n\ntype _ = ComponentProps;\n".to_string(),
    );

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": "/main2.ts",
            "startLine": 4,
            "startOffset": 10,
            "endLine": 4,
            "endOffset": 24,
            "errorCodes": [2304]
        }),
    };

    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected getCodeFixes to succeed");
    let body = resp.body.expect("expected getCodeFixes body");
    let fixes = body.as_array().expect("expected array response");
    let mut first_import_changes: Option<Vec<serde_json::Value>> = None;
    for fix in fixes {
        if fix.get("fixName").and_then(serde_json::Value::as_str) != Some("import") {
            continue;
        }
        let Some(changes) = fix.get("changes").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for change in changes {
            let Some(text_changes) = change
                .get("textChanges")
                .and_then(serde_json::Value::as_array)
            else {
                continue;
            };
            first_import_changes = Some(text_changes.clone());
            break;
        }
        if first_import_changes.is_some() {
            break;
        }
    }

    let mut updated = server
        .open_files
        .get("/main2.ts")
        .expect("missing main2.ts")
        .clone();
    let mut edits = first_import_changes.expect("expected at least one import fix");
    edits.sort_by(|a, b| {
        let a_line = a["start"]["line"].as_u64().unwrap_or(0);
        let a_offset = a["start"]["offset"].as_u64().unwrap_or(0);
        let b_line = b["start"]["line"].as_u64().unwrap_or(0);
        let b_offset = b["start"]["offset"].as_u64().unwrap_or(0);
        (b_line, b_offset).cmp(&(a_line, a_offset))
    });
    for edit in edits {
        updated = Server::apply_change(
            &updated,
            edit["start"]["line"].as_u64().expect("start line") as u32,
            edit["start"]["offset"].as_u64().expect("start offset") as u32,
            edit["end"]["line"].as_u64().expect("end line") as u32,
            edit["end"]["offset"].as_u64().expect("end offset") as u32,
            edit["newText"].as_str().expect("new text"),
        );
    }
    assert!(
        updated.contains("import type { ComponentProps, ComponentType } from \"react\";"),
        "expected merged type-only import, got {updated:?}"
    );
}

#[test]
fn get_code_fixes_prefers_merging_type_only_import_into_type_clause_at_point() {
    let mut server = make_server();
    server.open_files.insert(
        "/node_modules/react/index.d.ts".to_string(),
        "export interface ComponentType {}\nexport interface ComponentProps {}\nexport declare function useState<T>(initialState: T): [T, (newState: T) => void];\n".to_string(),
    );
    server.open_files.insert(
        "/main2.ts".to_string(),
        "import { useState } from \"react\";\nimport type { ComponentType } from \"react\";\n\ntype _ = ComponentProps;\n".to_string(),
    );

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": "/main2.ts",
            "startLine": 4,
            "startOffset": 24,
            "endLine": 4,
            "endOffset": 24,
            "preferences": {
                "preferTypeOnlyAutoImports": true
            }
        }),
    };

    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected point getCodeFixes to succeed");
    let body = resp.body.expect("expected getCodeFixes body");
    let fixes = body.as_array().expect("expected array response");
    let import_fix = fixes
        .iter()
        .find(|fix| fix.get("fixName").and_then(serde_json::Value::as_str) == Some("import"))
        .expect("expected import fix");
    let edits = import_fix["changes"][0]["textChanges"]
        .as_array()
        .expect("expected text changes");

    let mut updated = server
        .open_files
        .get("/main2.ts")
        .expect("missing main2.ts")
        .clone();
    let mut edits = edits.clone();
    edits.sort_by(|a, b| {
        let a_line = a["start"]["line"].as_u64().unwrap_or(0);
        let a_offset = a["start"]["offset"].as_u64().unwrap_or(0);
        let b_line = b["start"]["line"].as_u64().unwrap_or(0);
        let b_offset = b["start"]["offset"].as_u64().unwrap_or(0);
        (b_line, b_offset).cmp(&(a_line, a_offset))
    });
    for edit in edits {
        updated = Server::apply_change(
            &updated,
            edit["start"]["line"].as_u64().expect("start line") as u32,
            edit["start"]["offset"].as_u64().expect("start offset") as u32,
            edit["end"]["line"].as_u64().expect("end line") as u32,
            edit["end"]["offset"].as_u64().expect("end offset") as u32,
            edit["newText"].as_str().expect("new text"),
        );
    }

    assert!(
        updated.contains("import type { ComponentProps, ComponentType } from \"react\";"),
        "expected merged type-only import for point request, got {updated:?}"
    );
}

#[test]
fn get_code_fixes_returns_type_only_import_for_type_annotation_point_request() {
    let mut server = make_server();
    server.open_files.insert(
        "/exports1.ts".to_string(),
        "export const a = 0;\nexport const A = 1;\nexport type x = 6;\nexport const X = 7;\nexport type y = 8;\nexport const Y = 9;\nexport const Z = 10;\n".to_string(),
    );
    server.open_files.insert(
        "/index0.ts".to_string(),
        "import { type X, type Y, type Z } from \"./exports1\";\nconst foo: x;\nconst bar: y;\n"
            .to_string(),
    );

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": "/index0.ts",
            "startLine": 2,
            "startOffset": 13,
            "endLine": 2,
            "endOffset": 13,
            "preferences": {
                "organizeImportsTypeOrder": "last"
            }
        }),
    };

    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected point getCodeFixes to succeed");
    let body = resp.body.expect("expected getCodeFixes body");
    let fixes = body.as_array().expect("expected array response");
    let import_fix = fixes
        .iter()
        .find(|fix| fix.get("fixName").and_then(serde_json::Value::as_str) == Some("import"))
        .expect("expected import fix");
    let new_text = import_fix["changes"][0]["textChanges"][0]["newText"]
        .as_str()
        .expect("expected import text change");
    assert_eq!(new_text, ", type x");
}

#[test]
fn get_code_fixes_returns_type_only_import_for_function_parameter_point_request() {
    let mut server = make_server();
    server.open_files.insert(
        "/exports.ts".to_string(),
        "class SomeClass {}\nexport type { SomeClass };\n".to_string(),
    );
    server.open_files.insert(
        "/a.ts".to_string(),
        "import {} from \"./exports.js\";\nfunction takeSomeClass(c: SomeClass)\n".to_string(),
    );

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": "/a.ts",
            "startLine": 2,
            "startOffset": 36,
            "endLine": 2,
            "endOffset": 36
        }),
    };

    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected point getCodeFixes to succeed");
    let body = resp.body.expect("expected getCodeFixes body");
    let fixes = body.as_array().expect("expected array response");
    let import_fix = fixes
        .iter()
        .find(|fix| fix.get("fixName").and_then(serde_json::Value::as_str) == Some("import"))
        .expect("expected import fix");
    let new_text = import_fix["changes"][0]["textChanges"][0]["newText"]
        .as_str()
        .expect("expected import text change");
    assert!(new_text.contains("type SomeClass"));
}

#[test]
fn handle_get_code_fixes_returns_pnpm_import_fix_for_missing_name() {
    let mut server = make_server();
    server.open_files.insert(
        "/tsconfig.json".to_string(),
        r#"{ "compilerOptions": { "module": "commonjs" } }"#.to_string(),
    );
    server.open_files.insert(
        "/node_modules/.pnpm/mobx@6.0.4/node_modules/mobx/package.json".to_string(),
        r#"{ "types": "dist/mobx.d.ts" }"#.to_string(),
    );
    server.open_files.insert(
        "/node_modules/.pnpm/mobx@6.0.4/node_modules/mobx/dist/mobx.d.ts".to_string(),
        "export declare function autorun(): void;".to_string(),
    );
    server
        .open_files
        .insert("/index.ts".to_string(), "autorun".to_string());
    server
        .open_files
        .insert("/utils.ts".to_string(), "import \"mobx\";".to_string());

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": "/index.ts",
            "startLine": 1,
            "startOffset": 1,
            "endLine": 1,
            "endOffset": 8,
            "errorCodes": [tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME]
        }),
    };

    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected getCodeFixes to succeed");
    let fixes = resp
        .body
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .expect("expected getCodeFixes actions");
    let mut import_texts = Vec::new();
    for fix in fixes {
        if fix.get("fixName").and_then(serde_json::Value::as_str) != Some("import") {
            continue;
        }
        let Some(changes) = fix.get("changes").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for change in changes {
            let Some(text_changes) = change
                .get("textChanges")
                .and_then(serde_json::Value::as_array)
            else {
                continue;
            };
            for text_change in text_changes {
                if let Some(new_text) = text_change
                    .get("newText")
                    .and_then(serde_json::Value::as_str)
                {
                    import_texts.push(new_text.to_string());
                }
            }
        }
    }

    assert!(
        import_texts
            .iter()
            .any(|text| text.contains("import { autorun } from \"mobx\";")),
        "expected pnpm missing-name import fix, got {import_texts:?}"
    );
}

#[test]
fn handle_get_code_fixes_uses_side_effect_import_when_dependency_content_missing() {
    let mut server = make_server();
    server
        .open_files
        .insert("/index.ts".to_string(), "autorun".to_string());
    server
        .open_files
        .insert("/utils.ts".to_string(), "import \"mobx\";".to_string());

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": "/index.ts",
            "startLine": 1,
            "startOffset": 1,
            "endLine": 1,
            "endOffset": 8,
            "errorCodes": [tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME]
        }),
    };

    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected getCodeFixes to succeed");
    let fixes = resp
        .body
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .expect("expected getCodeFixes actions");
    let mut import_texts = Vec::new();
    for fix in fixes {
        if fix.get("fixName").and_then(serde_json::Value::as_str) != Some("import") {
            continue;
        }
        let Some(changes) = fix.get("changes").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for change in changes {
            let Some(text_changes) = change
                .get("textChanges")
                .and_then(serde_json::Value::as_array)
            else {
                continue;
            };
            for text_change in text_changes {
                if let Some(new_text) = text_change
                    .get("newText")
                    .and_then(serde_json::Value::as_str)
                {
                    import_texts.push(new_text.to_string());
                }
            }
        }
    }

    assert!(
        import_texts
            .iter()
            .any(|text| text.contains("import { autorun } from \"mobx\";")),
        "expected missing-name import fix from side-effect import fallback, got {import_texts:?}"
    );
}

#[test]
fn semantic_diagnostics_sync_adds_synthetic_missing_name_for_bare_identifier() {
    let mut server = make_server();
    server
        .open_files
        .insert("/index.ts".to_string(), "autorun".to_string());

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "semanticDiagnosticsSync".to_string(),
        arguments: serde_json::json!({
            "file": "/index.ts",
            "includeLinePosition": true
        }),
    };

    let resp = server.handle_semantic_diagnostics_sync(1, &req);
    assert!(resp.success, "expected semanticDiagnosticsSync to succeed");
    let diagnostics = resp
        .body
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .expect("expected diagnostics array");
    assert!(
        diagnostics.iter().any(|diag| {
            diag.get("code").and_then(serde_json::Value::as_u64)
                == Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME as u64)
        }),
        "expected synthetic cannot-find-name diagnostic, got {diagnostics:?}"
    );
}

#[test]
fn semantic_diagnostics_sync_does_not_add_missing_name_for_class_method_declaration() {
    let mut server = make_server();
    server.open_files.insert(
        "/index.ts".to_string(),
        "class Foo {\n    constructor() { }\n    constructor() { }\n    fn() { }\n}\n".to_string(),
    );

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "semanticDiagnosticsSync".to_string(),
        arguments: serde_json::json!({
            "file": "/index.ts",
            "includeLinePosition": true
        }),
    };

    let resp = server.handle_semantic_diagnostics_sync(1, &req);
    assert!(resp.success, "expected semanticDiagnosticsSync to succeed");
    let diagnostics = resp
        .body
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .expect("expected diagnostics array");

    let fn_missing_name_count = diagnostics
        .iter()
        .filter(|diag| {
            diag.get("code").and_then(serde_json::Value::as_u64)
                == Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME as u64)
                && diag
                    .get("message")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|msg| msg.contains("'fn'"))
        })
        .count();
    assert_eq!(
        fn_missing_name_count, 0,
        "did not expect synthetic missing-name diagnostics for class method declarations: {diagnostics:?}"
    );
}

#[test]
fn parse_identifier_call_expression_ignores_keywords() {
    assert_eq!(
        parse_identifier_call_expression("useEffect(() => {})"),
        Some((0, "useEffect"))
    );
    assert_eq!(parse_identifier_call_expression("if (cond)"), None);
    assert_eq!(parse_identifier_call_expression("fn() { }"), None);
    assert_eq!(
        parse_identifier_call_expression("fn(): number { return 1; }"),
        None
    );
}

#[test]
fn handle_get_code_fixes_implement_interface_excludes_reexport_file_from_auto_import_patterns() {
    let mut server = make_server();
    server.open_files.insert(
        "/src/vs/test.ts".to_string(),
        "import { Parts } from './parts';\nexport class Extended implements Parts {\n}\n"
            .to_string(),
    );
    server.open_files.insert(
        "/src/vs/parts.ts".to_string(),
        "import { Event } from '../thing';\nexport interface Parts {\n    readonly options: Event;\n}\n"
            .to_string(),
    );
    server.open_files.insert(
        "/src/event/event.ts".to_string(),
        "export interface Event {\n    (): string;\n}\n".to_string(),
    );
    server.open_files.insert(
        "/src/thing.ts".to_string(),
        "import { Event } from './event/event';\nexport { Event };\n".to_string(),
    );
    server.open_files.insert(
        "/src/a.ts".to_string(),
        "import './thing';\ndeclare module './thing' {\n    interface Event {\n        c: string;\n    }\n}\n"
            .to_string(),
    );

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": "/src/vs/test.ts",
            "startLine": 2,
            "startOffset": 14,
            "endLine": 2,
            "endOffset": 22,
            "errorCodes": [2420],
            "preferences": {
                "autoImportFileExcludePatterns": ["src/thing.ts"]
            }
        }),
    };

    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected getCodeFixes to succeed");
    let actions = resp
        .body
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .expect("expected getCodeFixes actions array");
    let action = actions
        .iter()
        .find(|action| {
            action.get("fixName").and_then(serde_json::Value::as_str)
                == Some("fixClassIncorrectlyImplementsInterface")
        })
        .expect("expected implement-interface codefix");
    let new_text = action["changes"][0]["textChanges"][0]["newText"]
        .as_str()
        .expect("expected replacement text");

    assert_eq!(
        new_text,
        "import { Event } from '../event/event';\nimport { Parts } from './parts';\nexport class Extended implements Parts {\n    options: Event;\n}\n"
    );
}

#[test]
fn handle_get_code_fixes_implement_interface_skips_unusable_reexport_chain() {
    let mut server = make_server();
    server.open_files.insert(
        "/src/vs/test.ts".to_string(),
        "import { Parts } from './parts';\nexport class Extended implements Parts {\n}\n"
            .to_string(),
    );
    server.open_files.insert(
        "/src/vs/parts.ts".to_string(),
        "import { Event } from '../thing';\nexport interface Parts {\n    readonly options: Event;\n}\n"
            .to_string(),
    );
    server.open_files.insert(
        "/src/event/event.ts".to_string(),
        "export interface Event {\n    (): string;\n}\n".to_string(),
    );
    server.open_files.insert(
        "/src/thing.ts".to_string(),
        "import { Event } from '../event/event';\nexport { Event };\n".to_string(),
    );
    server.open_files.insert(
        "/src/a.ts".to_string(),
        "import './thing';\ndeclare module './thing' {\n    interface Event {\n        c: string;\n    }\n}\n"
            .to_string(),
    );

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": "/src/vs/test.ts",
            "startLine": 2,
            "startOffset": 14,
            "endLine": 2,
            "endOffset": 22,
            "errorCodes": [2420],
            "preferences": {
                "autoImportFileExcludePatterns": ["src/thing.ts"]
            }
        }),
    };

    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected getCodeFixes to succeed");
    let actions = resp
        .body
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .expect("expected getCodeFixes actions array");
    let action = actions
        .iter()
        .find(|action| {
            action.get("fixName").and_then(serde_json::Value::as_str)
                == Some("fixClassIncorrectlyImplementsInterface")
        })
        .expect("expected implement-interface codefix");
    let new_text = action["changes"][0]["textChanges"][0]["newText"]
        .as_str()
        .expect("expected replacement text");

    assert_eq!(
        new_text,
        "import { Parts } from './parts';\nexport class Extended implements Parts {\n    options: Event;\n}\n"
    );
}

#[test]
fn handle_get_code_fixes_implements_same_file_interface_with_negative_literal_union() {
    let mut server = make_server();
    server.open_files.insert(
        "/index.ts".to_string(),
        "interface X { value: -1 | 0 | 1; }\nclass Y implements X { }".to_string(),
    );

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": "/index.ts",
            "startLine": 2,
            "startOffset": 7,
            "endLine": 2,
            "endOffset": 8,
            "errorCodes": [2420],
            "formatOptions": {
                "indentSize": 4,
                "tabSize": 4,
                "convertTabsToSpaces": true,
                "newLineCharacter": "\n"
            }
        }),
    };

    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected getCodeFixes to succeed");
    let body = resp.body.expect("expected getCodeFixes body");
    let actions = body
        .as_array()
        .expect("expected getCodeFixes actions array");
    let action = actions
        .iter()
        .find(|action| {
            action.get("fixName").and_then(serde_json::Value::as_str)
                == Some("fixClassIncorrectlyImplementsInterface")
        })
        .expect("expected implement-interface codefix for same-file interface");
    assert_eq!(
        action
            .get("description")
            .and_then(serde_json::Value::as_str),
        Some("Implement interface 'X'")
    );
    let new_text = action["changes"][0]["textChanges"][0]["newText"]
        .as_str()
        .expect("expected replacement text");
    assert!(
        new_text.contains("value: -1 | 0 | 1;"),
        "expected generated member text in codefix, got: {new_text}"
    );
}

#[test]
fn handle_get_code_fixes_prefers_add_missing_async_for_promise_assignability() {
    let mut server = make_server();
    let content = "interface Stuff {\n    b: () => Promise<string>;\n}\n\nfunction foo(): Stuff | Date {\n    return {\n        b: () => \"hello\",\n    }\n}\n";
    server
        .open_files
        .insert("/index.ts".to_string(), content.to_string());

    let line_map = LineMap::build(content);
    let start = content
        .find("hello")
        .expect("expected marker text for async fix") as u32;
    let end = start + "hello".len() as u32;
    let start_pos = line_map.offset_to_position(start, content);
    let end_pos = line_map.offset_to_position(end, content);

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": "/index.ts",
            "startLine": start_pos.line + 1,
            "startOffset": start_pos.character + 1,
            "endLine": end_pos.line + 1,
            "endOffset": end_pos.character + 1,
            "errorCodes": [2322]
        }),
    };

    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected getCodeFixes to succeed");
    let actions = resp
        .body
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .expect("expected getCodeFixes actions array");
    let first = actions
        .first()
        .expect("expected at least one codefix action for assignability mismatch");
    assert_eq!(
        first.get("fixId").and_then(serde_json::Value::as_str),
        Some("addMissingAsync"),
        "expected addMissingAsync to be prioritized, got: {actions:?}"
    );
    assert_eq!(
        first.get("description").and_then(serde_json::Value::as_str),
        Some("Add async modifier to containing function")
    );
    let new_text = first["changes"][0]["textChanges"][0]["newText"]
        .as_str()
        .expect("expected replacement text");
    assert!(
        new_text.contains("b: async () => \"hello\""),
        "expected async arrow update, got: {new_text}"
    );
}

#[test]
fn handle_get_code_fixes_prefers_add_missing_async_for_underscore_arrow_parameter() {
    let mut server = make_server();
    let content = "interface Stuff {\n    b: () => Promise<string>;\n}\n\nfunction foo(): Stuff | Date {\n    return {\n        b: _ => \"hello\",\n    }\n}\n";
    server
        .open_files
        .insert("/index.ts".to_string(), content.to_string());

    let line_map = LineMap::build(content);
    let start = content
        .find("hello")
        .expect("expected marker text for async fix") as u32;
    let end = start + "hello".len() as u32;
    let start_pos = line_map.offset_to_position(start, content);
    let end_pos = line_map.offset_to_position(end, content);

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": "/index.ts",
            "startLine": start_pos.line + 1,
            "startOffset": start_pos.character + 1,
            "endLine": end_pos.line + 1,
            "endOffset": end_pos.character + 1,
            "errorCodes": [2322]
        }),
    };

    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected getCodeFixes to succeed");
    let actions = resp
        .body
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .expect("expected getCodeFixes actions array");
    let first = actions
        .first()
        .expect("expected at least one codefix action for assignability mismatch");
    assert_eq!(
        first.get("fixId").and_then(serde_json::Value::as_str),
        Some("addMissingAsync"),
        "expected addMissingAsync to be prioritized, got: {actions:?}"
    );
    let new_text = first["changes"][0]["textChanges"][0]["newText"]
        .as_str()
        .expect("expected replacement text");
    assert!(
        new_text.contains("b: async (_) => \"hello\""),
        "expected async underscored-parameter arrow update, got: {new_text}"
    );
}

// Issue #3832: getCodeFixes should return an empty body when the requested
// span does not overlap any matching diagnostic (no fallback to all matching
// diagnostics in the file).
#[test]
fn get_code_fixes_returns_empty_when_span_misses_diagnostic() {
    let mut server = make_server();
    let file = "/missing_name_outside_span.ts";
    let content = "missingName = 1;\nconst ok = 1;\n";

    server
        .open_files
        .insert(file.to_string(), content.to_string());

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": file,
            "startLine": 2,
            "startOffset": 1,
            "endLine": 2,
            "endOffset": 1,
            "errorCodes": [2304]
        }),
    };

    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected getCodeFixes to succeed");
    let actions = resp
        .body
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .expect("expected getCodeFixes actions array");
    assert!(
        actions.is_empty(),
        "expected no code fixes when request span misses the diagnostic, got: {actions:?}"
    );
}

// Issue #3938: implement-interface codefix should generate stubs for method
// signatures, not just property signatures. tsc emits a method body that
// throws "Method not implemented." for each missing method.
#[test]
fn handle_get_code_fixes_implement_interface_method_signature() {
    let mut server = make_server();
    let content = "interface I { m(): void; }\nclass C implements I {}\n";
    server
        .open_files
        .insert("/method_iface.ts".to_string(), content.to_string());

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": "/method_iface.ts",
            "startLine": 2,
            "startOffset": 7,
            "endLine": 2,
            "endOffset": 8,
            "errorCodes": [2420],
        }),
    };

    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected getCodeFixes to succeed");
    let body = resp.body.expect("expected getCodeFixes body");
    let actions = body
        .as_array()
        .expect("expected getCodeFixes actions array");
    let action = actions
        .iter()
        .find(|action| {
            action.get("fixName").and_then(serde_json::Value::as_str)
                == Some("fixClassIncorrectlyImplementsInterface")
        })
        .unwrap_or_else(|| {
            panic!("expected implement-interface codefix for method signature, got: {actions:?}")
        });
    let new_text = action["changes"][0]["textChanges"][0]["newText"]
        .as_str()
        .expect("expected replacement text");
    assert!(
        new_text.contains("m(): void {"),
        "expected method stub for missing method, got: {new_text}"
    );
    assert!(
        new_text.contains("throw new Error(\"Method not implemented.\");"),
        "expected throw stub in method body, got: {new_text}"
    );
}

// Issue #3938: a property whose type is a function type (e.g.
// `m: () => void;`) must continue to be rendered as a property assignment,
// not as a method stub. The method-signature parse path must only fire when
// the member name is followed directly by `(` or `<`, not by `:`.
#[test]
fn handle_get_code_fixes_implement_interface_function_typed_property() {
    let mut server = make_server();
    let content = "interface I { m: () => void; }\nclass C implements I {}\n";
    server
        .open_files
        .insert("/fn_prop_iface.ts".to_string(), content.to_string());

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": "/fn_prop_iface.ts",
            "startLine": 2,
            "startOffset": 7,
            "endLine": 2,
            "endOffset": 8,
            "errorCodes": [2420],
        }),
    };
    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected getCodeFixes to succeed");
    let body = resp.body.expect("expected getCodeFixes body");
    let actions = body
        .as_array()
        .expect("expected getCodeFixes actions array");
    let action = actions
        .iter()
        .find(|action| {
            action.get("fixName").and_then(serde_json::Value::as_str)
                == Some("fixClassIncorrectlyImplementsInterface")
        })
        .expect("expected implement-interface codefix for function-typed property");
    let new_text = action["changes"][0]["textChanges"][0]["newText"]
        .as_str()
        .expect("expected replacement text");
    assert!(
        new_text.contains("m: () => void;"),
        "expected property assignment for function-typed property, got: {new_text}"
    );
    assert!(
        !new_text.contains("Method not implemented."),
        "function-typed properties should not be rendered as throwing methods, got: {new_text}"
    );
}

// ── fixMissingTypeAnnotationOnExports (TS9010) ───────────────────────────────
//
// These tests exercise `apply_isolated_decl_type_annotation_fix` and
// `infer_type_for_isolated_decl_initializer` directly, using a synthetic
// TS9010 diagnostic at the variable-name position. The structural rule:
//
//   When an exported variable's initializer is any JSX expression
//   (`JsxElement`, `JsxSelfClosingElement`, or `JsxFragment`), the correct
//   annotation type is `JSX.Element`. The two tests use distinct variable names
//   and distinct JSX node shapes to confirm the fix is keyed on the node kind,
//   not on any specific spelling.

fn make_ts9010_diagnostic(
    file: &str,
    start: u32,
    length: u32,
) -> tsz::checker::diagnostics::Diagnostic {
    tsz::checker::diagnostics::Diagnostic {
        category: DiagnosticCategory::Error,
        code: 9010,
        file: file.to_string(),
        start,
        length,
        message_text: "Variable must have an explicit type annotation with --isolatedDeclarations."
            .to_string(),
        related_information: Vec::new(),
    }
}

fn parse_to_arena(file: &str, content: &str) -> tsz::parser::node::NodeArena {
    let mut parser = ParserState::new(file.to_string(), content.to_string());
    parser.parse_source_file();
    parser.into_arena()
}

/// `JsxSelfClosingElement` initializer, variable named `element`.
/// Rule: any self-closing JSX expression → annotation `JSX.Element`.
#[test]
fn fix_missing_type_annotation_jsx_self_closing() {
    // offset 13: "element" (after "export const ")
    let content = "export const element = <div/>;";
    let file = "/fix_missing_jsx_self_closing.tsx";
    let arena = parse_to_arena(file, content);
    let line_map = LineMap::build(content);

    let diag = make_ts9010_diagnostic(file, 13, 7 /* "element" */);
    let fixes = Server::apply_isolated_decl_type_annotation_fix(
        file,
        content,
        &arena,
        &line_map,
        &[diag],
        &[9010],
        None,
    );

    assert_eq!(fixes.len(), 2, "expected exactly two fix variants");

    let direct_text = fixes[0]["changes"][0]["textChanges"][0]["newText"]
        .as_str()
        .expect("direct annotation newText");
    assert_eq!(
        direct_text, ": JSX.Element",
        "direct annotation should insert `: JSX.Element`"
    );

    let satisfies_text = fixes[1]["changes"][0]["textChanges"][1]["newText"]
        .as_str()
        .expect("satisfies+cast newText");
    assert!(
        satisfies_text.contains("JSX.Element"),
        "satisfies+cast should reference JSX.Element, got: {satisfies_text}"
    );

    // Both fixes must carry the canonical fixId for the client to batch them.
    for fix in &fixes {
        assert_eq!(
            fix["fixId"].as_str(),
            Some(super::FIX_MISSING_TYPE_ANNOTATION_FIX_ID),
            "fixId mismatch"
        );
    }

    // Confirm the fix inserts after the name, not at the start of the line.
    let insert_offset = fixes[0]["changes"][0]["textChanges"][0]["start"]["offset"]
        .as_u64()
        .expect("start offset");
    assert!(
        insert_offset > 13,
        "annotation must be inserted after the variable name"
    );
}

/// `JsxElement` (open/close tags) initializer, variable named `wrapper`.
/// Rule: any JSX element expression → annotation `JSX.Element`, regardless
/// of variable name or tag choice.
#[test]
fn fix_missing_type_annotation_jsx_element() {
    // offset 13: "wrapper" (after "export const ")
    let content = "export const wrapper = <span><b/></span>;";
    let file = "/fix_missing_jsx_element.tsx";
    let arena = parse_to_arena(file, content);
    let line_map = LineMap::build(content);

    let diag = make_ts9010_diagnostic(file, 13, 7 /* "wrapper" */);
    let fixes = Server::apply_isolated_decl_type_annotation_fix(
        file,
        content,
        &arena,
        &line_map,
        &[diag],
        &[9010],
        None,
    );

    assert_eq!(
        fixes.len(),
        2,
        "expected exactly two fix variants for JsxElement"
    );

    let direct_text = fixes[0]["changes"][0]["textChanges"][0]["newText"]
        .as_str()
        .expect("direct annotation newText");
    assert_eq!(direct_text, ": JSX.Element");

    let satisfies_text = fixes[1]["changes"][0]["textChanges"][1]["newText"]
        .as_str()
        .expect("satisfies+cast close newText");
    assert!(
        satisfies_text.contains("JSX.Element"),
        "satisfies+cast should reference JSX.Element, got: {satisfies_text}"
    );
}

/// `JsxFragment` initializer (`<>...</>`), variable named `items`.
/// Rule: JSX fragment is also a JSX expression → annotation `JSX.Element`.
#[test]
fn fix_missing_type_annotation_jsx_fragment() {
    // offset 13: "items" (after "export const ")
    let content = "export const items = <><li/></>;";
    let file = "/fix_missing_jsx_fragment.tsx";
    let arena = parse_to_arena(file, content);
    let line_map = LineMap::build(content);

    let diag = make_ts9010_diagnostic(file, 13, 5 /* "items" */);
    let fixes = Server::apply_isolated_decl_type_annotation_fix(
        file,
        content,
        &arena,
        &line_map,
        &[diag],
        &[9010],
        None,
    );

    assert_eq!(fixes.len(), 2, "expected two fix variants for JsxFragment");
    let direct_text = fixes[0]["changes"][0]["textChanges"][0]["newText"]
        .as_str()
        .expect("newText");
    assert_eq!(direct_text, ": JSX.Element");
}

/// Non-JSX initializer (numeric literal) must produce no fixes — the
/// function only handles the JSX class of shapes; other cases fall back
/// to the full checker-backed path.
#[test]
fn fix_missing_type_annotation_non_jsx_produces_no_fixes() {
    let content = "export const count = 42;";
    let file = "/fix_missing_non_jsx.ts";
    let arena = parse_to_arena(file, content);
    let line_map = LineMap::build(content);

    let diag = make_ts9010_diagnostic(file, 13, 5 /* "count" */);
    let fixes = Server::apply_isolated_decl_type_annotation_fix(
        file,
        content,
        &arena,
        &line_map,
        &[diag],
        &[9010],
        None,
    );

    assert!(
        fixes.is_empty(),
        "non-JSX initializer should produce no fixes from the AST-structural path"
    );
}

/// Calling with `error_codes` that do NOT include 9010 must return nothing
/// even when a TS9010 diagnostic is present — the guard is on the request,
/// not the diagnostic bag.
#[test]
fn fix_missing_type_annotation_wrong_error_code_returns_empty() {
    let content = "export const el = <div/>;";
    let file = "/fix_missing_wrong_code.tsx";
    let arena = parse_to_arena(file, content);
    let line_map = LineMap::build(content);

    let diag = make_ts9010_diagnostic(file, 13, 2 /* "el" */);
    // Client only asked about TS2304 — should not get 9010 fixes.
    let fixes = Server::apply_isolated_decl_type_annotation_fix(
        file,
        content,
        &arena,
        &line_map,
        &[diag],
        &[2304],
        None,
    );

    assert!(
        fixes.is_empty(),
        "should not produce fixes when error_codes does not include 9010"
    );
}

/// When the server did not generate a TS9010 diagnostic (e.g. isolatedDeclarations
/// is not in the server's inferred options) but the client supplies `request_span`,
/// the fix should use the span's start position to locate the variable declaration.
/// Structural rule: `error_codes`=[9010] + `request_span` covers variable name -> fix is
/// generated from the span even with an empty diagnostics slice.
#[test]
fn fix_missing_type_annotation_jsx_self_closing_span_fallback_no_server_diag() {
    let content = "export const myNode = <div/>;";
    let file = "/span_fallback.tsx";
    let arena = parse_to_arena(file, content);
    let line_map = LineMap::build(content);
    // "myNode" starts at byte 13 (0-indexed). LineMap uses 0-based line/col.
    // line=0, col=13.
    let span_start = tsz::lsp::position::Position::new(0, 13);
    let span_end = tsz::lsp::position::Position::new(0, 19);

    let fixes = Server::apply_isolated_decl_type_annotation_fix(
        file,
        content,
        &arena,
        &line_map,
        &[], // No server-generated TS9010 diagnostics
        &[9010],
        Some((span_start, span_end)),
    );

    assert_eq!(
        fixes.len(),
        2,
        "span-based fallback must produce two fix variants when diagnostics is empty: {fixes:?}"
    );
    let direct_text = fixes[0]["changes"][0]["textChanges"][0]["newText"]
        .as_str()
        .expect("direct annotation newText");
    assert_eq!(
        direct_text, ": JSX.Element",
        "span fallback should still infer JSX.Element"
    );
}

/// Span-based fallback for `JsxElement` shape (variable named `node`).
/// Confirms the fallback is keyed on the JSX node kind, not on the
/// diagnostic source.
#[test]
fn fix_missing_type_annotation_jsx_element_span_fallback_different_name() {
    let content = "export const node = <section><p/></section>;";
    let file = "/span_fallback_element.tsx";
    let arena = parse_to_arena(file, content);
    let line_map = LineMap::build(content);
    let span_start = tsz::lsp::position::Position::new(0, 13);
    let span_end = tsz::lsp::position::Position::new(0, 17);

    let fixes = Server::apply_isolated_decl_type_annotation_fix(
        file,
        content,
        &arena,
        &line_map,
        &[], // No diagnostics
        &[9010],
        Some((span_start, span_end)),
    );

    assert_eq!(
        fixes.len(),
        2,
        "span-based fallback must produce two fix variants for JsxElement: {fixes:?}"
    );
    let direct_text = fixes[0]["changes"][0]["textChanges"][0]["newText"]
        .as_str()
        .expect("direct annotation newText");
    assert_eq!(direct_text, ": JSX.Element");
}

/// Span with no `request_span` and empty diagnostics must return nothing.
#[test]
fn fix_missing_type_annotation_no_span_no_diag_returns_empty() {
    let content = "export const el = <div/>;";
    let file = "/no_span_no_diag.tsx";
    let arena = parse_to_arena(file, content);
    let line_map = LineMap::build(content);

    let fixes = Server::apply_isolated_decl_type_annotation_fix(
        file,
        content,
        &arena,
        &line_map,
        &[], // No diagnostics
        &[9010],
        None, // No span either
    );

    assert!(
        fixes.is_empty(),
        "without both diagnostics and span, no fix should be produced"
    );
}
