use super::{LineMap, Server, TsServerRequest};
use crate::handlers_code_fixes_utils::reorder_import_candidates_for_package_roots;
use crate::{CheckOptions, LogConfig, LogLevel, ServerMode};
use rustc_hash::FxHashMap;
use std::path::PathBuf;
use tsz::lsp::code_actions::ImportCandidate;
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

/// Issue #3848: tsserver does NOT inject filename-gated `inferFromUsage`
/// placeholders for JSDoc `@type {function(...)}` annotations — the same
/// source under any other file name returns just the
/// `Annotate with type from JSDoc` fix. Verify tsz-server now matches that
/// invariant: the response must contain the annotate fix and must NOT carry
/// any empty-changes `inferFromUsage` actions.
#[test]
fn get_code_fixes_jsdoc_does_not_emit_filename_gated_placeholders() {
    let mut server = make_server();
    let file = "/annotateWithTypeFromJSDoc16.ts";
    let content = "/** @type {function(*, ...number, ...boolean): void} */\nvar x = (x, ys, ...zs) => { x; ys; zs; };\n";

    server
        .open_files
        .insert(file.to_string(), content.to_string());
    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": file,
            "startLine": 1,
            "startOffset": 1,
            "endLine": 1,
            "endOffset": 1,
            "errorCodes": [80004]
        }),
    };
    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected getCodeFixes to succeed for {file}");
    let actions = resp
        .body
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .expect("expected getCodeFixes actions array");
    let any_annotate = actions.iter().any(|action| {
        action
            .get("description")
            .and_then(serde_json::Value::as_str)
            == Some("Annotate with type from JSDoc")
    });
    assert!(
        any_annotate,
        "expected an 'Annotate with type from JSDoc' action for {file}, got {actions:?}"
    );
    let any_infer_placeholder = actions.iter().any(|action| {
        action.get("fixId").and_then(serde_json::Value::as_str) == Some("inferFromUsage")
    });
    assert!(
        !any_infer_placeholder,
        "tsserver does not emit `inferFromUsage` for JSDoc @type function annotations; got {actions:?}"
    );
}

#[test]
fn fix_missing_imports_combines_sequential_import_merges() {
    let src = "import { Test1, Test4 } from './file1';\ninterface Testing {\n    test1: Test1;\n    test2: Test2;\n    test3: Test3;\n    test4: Test4;\n}\n";
    let candidates = vec![
        ImportCandidate::named(
            "./file1".to_string(),
            "Test2".to_string(),
            "Test2".to_string(),
        ),
        ImportCandidate::named(
            "./file1".to_string(),
            "Test3".to_string(),
            "Test3".to_string(),
        ),
    ];

    let updated = Server::apply_missing_imports_fix_all("file2.ts", src, &candidates)
        .expect("expected missing import fix-all to produce an edit");

    assert_eq!(
        updated,
        "import { Test1, Test2, Test3, Test4 } from './file1';\ninterface Testing {\n    test1: Test1;\n    test2: Test2;\n    test3: Test3;\n    test4: Test4;\n}\n"
    );
}

#[test]
fn fix_missing_imports_uses_require_for_commonjs_js_files() {
    let src = "exports.dedupeLines = data => {\n  variants\n}\n";
    let candidates = vec![ImportCandidate::named(
        "./matrix.js".to_string(),
        "variants".to_string(),
        "variants".to_string(),
    )];

    let updated = Server::apply_missing_imports_fix_all("main.js", src, &candidates)
        .expect("expected commonjs missing import to produce an edit");

    assert_eq!(
        updated,
        "const { variants } = require(\"./matrix\")\n\nexports.dedupeLines = data => {\n  variants\n}\n"
    );
}

#[test]
fn synthetic_missing_name_detects_commonjs_export_candidates() {
    let mut server = make_server();
    server.open_files.insert(
        "/matrix.js".to_string(),
        "exports.variants = [];".to_string(),
    );
    let main = "exports.dedupeLines = data => {\n  variants\n}\n".to_string();
    server
        .open_files
        .insert("/main.js".to_string(), main.clone());

    let mut parser = ParserState::new("/main.js".to_string(), main.clone());
    let root = parser.parse_source_file();
    let arena = parser.into_arena();
    let mut binder = tsz::binder::BinderState::new();
    binder.bind_source_file(&arena, root);

    let diagnostics =
        server.synthetic_missing_name_expression_diagnostics("/main.js", &main, &binder);
    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME
                && diag.message_text.contains("variants")
        }),
        "expected synthetic missing-name diagnostic for 'variants', got {diagnostics:?}"
    );
}

#[test]
fn rewrite_single_import_to_commonjs_require_converts_named_import() {
    let rewritten = Server::rewrite_single_import_to_commonjs_require(
        "import { variants } from \"./matrix.js\";\n",
    )
    .expect("expected named import rewrite");
    assert_eq!(rewritten, "const { variants } = require(\"./matrix\")\n");
}

#[test]
fn collect_import_candidates_normalizes_commonjs_js_specifiers() {
    let mut server = make_server();
    server.open_files.insert(
        "/matrix.js".to_string(),
        "exports.variants = [];".to_string(),
    );
    server.open_files.insert(
        "/totally-irrelevant-no-way-this-changes-things-right.js".to_string(),
        "export default 0;".to_string(),
    );
    let main = "exports.dedupeLines = data => {\n  variants\n}\n".to_string();
    server.open_files.insert("/main.js".to_string(), main);

    let diagnostics = vec![tsz::lsp::diagnostics::LspDiagnostic {
        range: tsz::lsp::position::Range::new(
            tsz::lsp::position::Position::new(1, 2),
            tsz::lsp::position::Position::new(1, 10),
        ),
        message: "Cannot find name 'variants'.".to_string(),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
        severity: Some(tsz::lsp::diagnostics::DiagnosticSeverity::Error),
        source: Some("tsz".to_string()),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    }];

    let candidates =
        server.collect_import_candidates("/main.js", &diagnostics, &[], &[], None, None);
    let module_specifiers: Vec<String> = candidates
        .into_iter()
        .map(|candidate| candidate.module_specifier)
        .collect();

    assert!(
        module_specifiers.iter().any(|spec| spec == "./matrix"),
        "expected normalized './matrix' specifier, got {module_specifiers:?}"
    );
    assert!(
        module_specifiers
            .iter()
            .all(|spec| spec != "./totally-irrelevant-no-way-this-changes-things-right"),
        "did not expect unrelated default export candidate, got {module_specifiers:?}"
    );
}

#[test]
fn get_code_fixes_unresolved_function_call_offers_missing_function_declaration() {
    // Plain unresolved call expression `foo(1);` must produce a
    // `fixMissingFunctionDeclaration` action, not an empty
    // `Add all missing imports` action. Regression for
    // https://github.com/mohsen1/tsz/issues/3806.
    let mut server = make_server();
    let content = "foo(1);\n";
    server
        .open_files
        .insert("/a.ts".to_string(), content.to_string());

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": "/a.ts",
            "startLine": 1,
            "startOffset": 1,
            "endLine": 1,
            "endOffset": 4,
            "errorCodes": [2304]
        }),
    };

    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success);
    let actions = resp
        .body
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .expect("expected getCodeFixes actions array");

    let fix_names: Vec<&str> = actions
        .iter()
        .filter_map(|a| a.get("fixName").and_then(serde_json::Value::as_str))
        .collect();
    assert!(
        fix_names.contains(&"fixMissingFunctionDeclaration"),
        "expected fixMissingFunctionDeclaration in {fix_names:?}"
    );
    assert!(
        !fix_names.contains(&"quickfix"),
        "empty fixMissingImport quickfix must not appear, got {actions:?}"
    );

    let fix = actions
        .iter()
        .find(|a| {
            a.get("fixName").and_then(serde_json::Value::as_str)
                == Some("fixMissingFunctionDeclaration")
        })
        .expect("missing the function-declaration fix");
    let description = fix
        .get("description")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    assert_eq!(
        description, "Add missing function declaration 'foo'",
        "unexpected description: {description}"
    );
    let new_text = fix
        .get("changes")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("textChanges"))
        .and_then(|t| t.get(0))
        .and_then(|t| t.get("newText"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    assert!(
        new_text.contains("function foo("),
        "expected function declaration in newText, got {new_text:?}"
    );
}

#[test]
fn get_code_fixes_does_not_offer_spelling_for_plain_missing_property_2339() {
    let mut server = make_server();
    server.open_files.insert(
        "/declarations.d.ts".to_string(),
        "interface Response {}\n".to_string(),
    );
    let content = "import './declarations.d.ts'\ndeclare const resp: Response\nresp.test()\n";
    server
        .open_files
        .insert("/foo.ts".to_string(), content.to_string());

    let line_map = LineMap::build(content);
    let start = content.find("test").expect("expected test property access") as u32;
    let end = start + "test".len() as u32;
    let start_pos = line_map.offset_to_position(start, content);
    let end_pos = line_map.offset_to_position(end, content);

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": "/foo.ts",
            "startLine": start_pos.line + 1,
            "startOffset": start_pos.character + 1,
            "endLine": end_pos.line + 1,
            "endOffset": end_pos.character + 1,
            "errorCodes": [2339]
        }),
    };

    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected getCodeFixes to succeed");
    let actions = resp
        .body
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .expect("expected getCodeFixes actions array");
    let descriptions: Vec<_> = actions
        .iter()
        .filter_map(|action| {
            action
                .get("description")
                .and_then(serde_json::Value::as_str)
        })
        .collect();

    assert_eq!(
        descriptions,
        vec![
            "Declare method 'test'",
            "Declare property 'test'",
            "Add index signature for property 'test'",
        ],
        "unexpected codefix list for plain TS2339 missing property: {actions:?}"
    );

    for action in actions {
        let changes = action
            .get("changes")
            .and_then(serde_json::Value::as_array)
            .expect("missing-member fix should include changes");
        assert_eq!(changes.len(), 1, "expected one file change: {action:?}");
        assert_eq!(
            changes[0]
                .get("fileName")
                .and_then(serde_json::Value::as_str),
            Some("/declarations.d.ts")
        );
        let text_changes = changes[0]
            .get("textChanges")
            .and_then(serde_json::Value::as_array)
            .expect("missing textChanges");
        assert_eq!(
            text_changes.len(),
            1,
            "expected one insertion edit: {action:?}"
        );
        assert_eq!(
            text_changes[0].get("start"),
            text_changes[0].get("end"),
            "missing-member fix should insert into the target interface"
        );
        assert!(
            text_changes[0]
                .get("newText")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|text| text.contains("unknown")
                    && (text.contains("test") || text.contains("[x: string]"))),
            "missing-member edit should declare the requested member: {action:?}"
        );
    }
}

#[test]
fn collect_import_candidates_uses_external_project_files() {
    let mut server = make_server();
    let temp_dir = std::env::temp_dir().join(format!(
        "tsz_external_project_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");
    let main_path = temp_dir.join("main.ts");
    let dep_path = temp_dir.join("dep.ts");
    std::fs::write(&main_path, "externalValue;").expect("write main file");
    std::fs::write(&dep_path, "export const externalValue = 1;").expect("write dep file");
    let main_path = main_path.to_string_lossy().to_string();
    let dep_path = dep_path.to_string_lossy().to_string();

    server
        .open_files
        .insert(main_path.clone(), "externalValue;".to_string());
    server.external_project_files.insert(
        "/tsconfig.json".to_string(),
        vec![main_path.clone(), dep_path],
    );

    let diagnostics = vec![tsz::lsp::diagnostics::LspDiagnostic {
        range: tsz::lsp::position::Range::new(
            tsz::lsp::position::Position::new(0, 0),
            tsz::lsp::position::Position::new(0, 13),
        ),
        severity: Some(tsz::lsp::diagnostics::DiagnosticSeverity::Error),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
        source: Some("tsc-rust".to_string()),
        message: "Cannot find name 'externalValue'.".to_string(),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    }];

    let candidates =
        server.collect_import_candidates(&main_path, &diagnostics, &[], &[], None, None);
    assert!(
        candidates.iter().any(|candidate| {
            candidate.local_name == "externalValue" && candidate.module_specifier == "./dep"
        }),
        "expected import candidate from external project files, got: {candidates:?}"
    );
}

#[test]
fn has_potential_auto_import_symbol_scans_external_project_files() {
    let mut server = make_server();
    let temp_dir = std::env::temp_dir().join(format!(
        "tsz_external_symbol_scan_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos()
    ));
    let dep_dir = temp_dir.join("node_modules/.pnpm/mobx@6.0.4/node_modules/mobx/dist");
    std::fs::create_dir_all(&dep_dir).expect("create dep dir");
    let current_path = temp_dir.join("index.ts");
    let dep_path = dep_dir.join("mobx.d.ts");
    std::fs::write(&current_path, "autorun").expect("write index.ts");
    std::fs::write(&dep_path, "export declare function autorun(): void;").expect("write mobx.d.ts");

    let current_path = current_path.to_string_lossy().to_string();
    let dep_path = dep_path.to_string_lossy().to_string();
    server
        .open_files
        .insert(current_path.clone(), "autorun".to_string());
    server.external_project_files.insert(
        "/tsconfig.json".to_string(),
        vec![current_path.clone(), dep_path],
    );

    assert!(
        server.has_potential_auto_import_symbol(&current_path, "autorun"),
        "expected external project declaration file to be considered for auto-import probe"
    );
}

#[test]
fn collect_import_candidates_falls_back_to_side_effect_import_specifier() {
    let mut server = make_server();
    server
        .open_files
        .insert("/index.ts".to_string(), "autorun".to_string());
    server
        .open_files
        .insert("/utils.ts".to_string(), "import \"mobx\";".to_string());

    let diagnostics = vec![tsz::lsp::diagnostics::LspDiagnostic {
        range: tsz::lsp::position::Range::new(
            tsz::lsp::position::Position::new(0, 0),
            tsz::lsp::position::Position::new(0, 7),
        ),
        message: "Cannot find name 'autorun'.".to_string(),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
        severity: Some(tsz::lsp::diagnostics::DiagnosticSeverity::Error),
        source: Some("tsz".to_string()),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    }];

    let candidates =
        server.collect_import_candidates("/index.ts", &diagnostics, &[], &[], None, None);
    assert!(
        candidates.iter().any(|candidate| {
            candidate.local_name == "autorun" && candidate.module_specifier == "mobx"
        }),
        "expected fallback candidate from side-effect import, got {candidates:?}"
    );
}

#[test]
fn collect_import_candidates_falls_back_to_external_project_node_modules_paths() {
    let mut server = make_server();
    server
        .open_files
        .insert("/index.ts".to_string(), "autorun".to_string());
    server.external_project_files.insert(
        "/tsconfig.json".to_string(),
        vec![
            "/index.ts".to_string(),
            "/node_modules/.pnpm/mobx@6.0.4/node_modules/mobx/dist/mobx.d.ts".to_string(),
        ],
    );

    let diagnostics = vec![tsz::lsp::diagnostics::LspDiagnostic {
        range: tsz::lsp::position::Range::new(
            tsz::lsp::position::Position::new(0, 0),
            tsz::lsp::position::Position::new(0, 7),
        ),
        message: "Cannot find name 'autorun'.".to_string(),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
        severity: Some(tsz::lsp::diagnostics::DiagnosticSeverity::Error),
        source: Some("tsz".to_string()),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    }];

    let candidates =
        server.collect_import_candidates("/index.ts", &diagnostics, &[], &[], None, None);
    assert!(
        candidates.iter().any(|candidate| {
            candidate.local_name == "autorun" && candidate.module_specifier == "mobx"
        }),
        "expected fallback candidate from external project path, got {candidates:?}"
    );
}

#[test]
fn module_specifier_from_node_modules_path_normalizes_pnpm_types_entry() {
    let mut existing = rustc_hash::FxHashSet::default();
    existing.insert("mobx".to_string());
    let spec = Server::module_specifier_from_node_modules_path(
        "/node_modules/.pnpm/mobx@6.0.4/node_modules/mobx/dist/mobx.d.ts",
        &existing,
    );
    assert_eq!(spec.as_deref(), Some("mobx"));
}

#[test]
fn module_specifier_from_node_modules_path_preserves_case_sensitive_subpath() {
    let mut existing = rustc_hash::FxHashSet::default();
    existing.insert("MobX/Foo".to_string());
    let spec = Server::module_specifier_from_node_modules_path(
        "/node_modules/.pnpm/mobx@6.0.4/node_modules/MobX/Foo.d.ts",
        &existing,
    );
    assert_eq!(spec.as_deref(), Some("MobX/Foo"));
}

#[test]
fn collect_import_candidates_prefers_package_root_specifier_before_subpath() {
    let mut server = make_server();
    server.open_files.insert(
        "/node_modules/pkg/package.json".to_string(),
        r#"{
    "name": "pkg",
    "version": "1.0.0",
    "exports": {
        ".": "./index.js",
        "./utils": "./utils.js"
    }
}"#
        .to_string(),
    );
    server.open_files.insert(
        "/node_modules/pkg/utils.d.ts".to_string(),
        "export function add(a: number, b: number) {}".to_string(),
    );
    server.open_files.insert(
        "/node_modules/pkg/index.d.ts".to_string(),
        "export * from \"./utils\";".to_string(),
    );
    server
        .open_files
        .insert("/src/index.ts".to_string(), "add".to_string());

    let diagnostics = vec![tsz::lsp::diagnostics::LspDiagnostic {
        range: tsz::lsp::position::Range::new(
            tsz::lsp::position::Position::new(0, 0),
            tsz::lsp::position::Position::new(0, 3),
        ),
        message: "Cannot find name 'add'.".to_string(),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
        severity: Some(tsz::lsp::diagnostics::DiagnosticSeverity::Error),
        source: Some("tsz".to_string()),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    }];

    let candidates =
        server.collect_import_candidates("/src/index.ts", &diagnostics, &[], &[], None, None);
    let module_specifiers: Vec<String> = candidates
        .into_iter()
        .filter(|candidate| candidate.local_name == "add")
        .map(|candidate| candidate.module_specifier)
        .collect();

    assert_eq!(
        module_specifiers,
        vec!["pkg".to_string(), "pkg/utils".to_string()]
    );
}

#[test]
fn reorder_import_candidates_prefers_shallower_relative_specifier_for_same_symbol() {
    let mut candidates = vec![
        ImportCandidate::named(
            "./lib/components/button/Button".to_string(),
            "Button".to_string(),
            "Button".to_string(),
        ),
        ImportCandidate::named(
            "./lib/main".to_string(),
            "Button".to_string(),
            "Button".to_string(),
        ),
    ];

    reorder_import_candidates_for_package_roots(&mut candidates);
    let module_specifiers: Vec<String> = candidates
        .iter()
        .map(|candidate| candidate.module_specifier.clone())
        .collect();

    assert_eq!(
        module_specifiers,
        vec![
            "./lib/main".to_string(),
            "./lib/components/button/Button".to_string()
        ]
    );
}

#[test]
fn collect_import_candidates_excludes_index_shorthand_specifiers_for_codefixes() {
    let mut server = make_server();
    server.open_files.insert(
        "/lib/components/button/Button.ts".to_string(),
        "export function Button() {}\n".to_string(),
    );
    server.open_files.insert(
        "/lib/components/button/index.ts".to_string(),
        "export * from \"./Button\";\n".to_string(),
    );
    server.open_files.insert(
        "/lib/components/index.ts".to_string(),
        "export * from \"./button\";\n".to_string(),
    );
    server.open_files.insert(
        "/lib/main.ts".to_string(),
        "export { Button } from \"./components\";\n".to_string(),
    );
    server.open_files.insert(
        "/lib/index.ts".to_string(),
        "export * from \"./main\";\n".to_string(),
    );
    server
        .open_files
        .insert("/i-hate-index-files.ts".to_string(), "Button\n".to_string());

    let diagnostics = vec![tsz::lsp::diagnostics::LspDiagnostic {
        range: tsz::lsp::position::Range::new(
            tsz::lsp::position::Position::new(0, 0),
            tsz::lsp::position::Position::new(0, 6),
        ),
        message: "Cannot find name 'Button'.".to_string(),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
        severity: Some(tsz::lsp::diagnostics::DiagnosticSeverity::Error),
        source: Some("tsz".to_string()),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    }];

    let candidates = server.collect_import_candidates(
        "/i-hate-index-files.ts",
        &diagnostics,
        &["/**/index.*".to_string()],
        &[],
        None,
        None,
    );
    let module_specifiers: Vec<String> = candidates
        .into_iter()
        .filter(|candidate| candidate.local_name == "Button")
        .map(|candidate| candidate.module_specifier)
        .collect();

    assert_eq!(
        module_specifiers,
        vec![
            "./lib/main".to_string(),
            "./lib/components/button/Button".to_string()
        ]
    );
}

#[test]
fn collect_import_candidates_respects_node_next_package_exports_root_only() {
    let mut server = make_server();
    server.open_files.insert(
        "/node_modules/pack/package.json".to_string(),
        r#"{
    "name": "pack",
    "version": "1.0.0",
    "exports": {
        ".": "./main.mjs"
    }
}"#
        .to_string(),
    );
    server.open_files.insert(
        "/node_modules/pack/main.d.mts".to_string(),
        "import {} from \"./unreachable.mjs\";\nexport const fromMain = 0;".to_string(),
    );
    server.open_files.insert(
        "/node_modules/pack/unreachable.d.mts".to_string(),
        "export const fromUnreachable = 0;".to_string(),
    );
    server.open_files.insert(
        "/index.mts".to_string(),
        "import { fromMain } from \"pack\";\nfromUnreachable".to_string(),
    );

    let diagnostics = vec![tsz::lsp::diagnostics::LspDiagnostic {
        range: tsz::lsp::position::Range::new(
            tsz::lsp::position::Position::new(1, 0),
            tsz::lsp::position::Position::new(1, 15),
        ),
        message: "Cannot find name 'fromUnreachable'.".to_string(),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
        severity: Some(tsz::lsp::diagnostics::DiagnosticSeverity::Error),
        source: Some("tsz".to_string()),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    }];

    let candidates =
        server.collect_import_candidates("/index.mts", &diagnostics, &[], &[], None, None);
    assert!(
        candidates.is_empty(),
        "expected no import candidates for unreachable node-next subpath export, got {candidates:?}"
    );
}

#[test]
fn collect_import_candidates_prefers_paths_mapping_over_node_modules_package_specifier() {
    let mut server = make_server();
    server.open_files.insert(
        "tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "module": "amd",
    "moduleResolution": "node",
    "rootDir": "ts",
    "baseUrl": ".",
    "paths": {
      "*": ["node_modules/@woltlab/wcf/ts/*"]
    }
  },
  "include": ["ts", "node_modules/@woltlab/wcf/ts"]
}"#
        .to_string(),
    );
    server.open_files.insert(
        "node_modules/@woltlab/wcf/ts/WoltLabSuite/Core/Component/Dialog.ts".to_string(),
        "export class Dialog {}".to_string(),
    );
    server
        .open_files
        .insert("ts/main.ts".to_string(), "Dialog".to_string());

    let diagnostics = vec![tsz::lsp::diagnostics::LspDiagnostic {
        range: tsz::lsp::position::Range::new(
            tsz::lsp::position::Position::new(0, 0),
            tsz::lsp::position::Position::new(0, 6),
        ),
        message: "Cannot find name 'Dialog'.".to_string(),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
        severity: Some(tsz::lsp::diagnostics::DiagnosticSeverity::Error),
        source: Some("tsz".to_string()),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    }];

    let candidates =
        server.collect_import_candidates("ts/main.ts", &diagnostics, &[], &[], None, None);
    let module_specifiers: Vec<String> = candidates
        .into_iter()
        .filter(|candidate| candidate.local_name == "Dialog")
        .map(|candidate| candidate.module_specifier)
        .collect();

    assert_eq!(
        module_specifiers,
        vec!["WoltLabSuite/Core/Component/Dialog".to_string()]
    );
}

#[test]
fn get_code_fixes_prefers_paths_mapping_module_specifier_for_node_modules_target() {
    let mut server = make_server();
    server.open_files.insert(
        "tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "module": "amd",
    "moduleResolution": "node",
    "rootDir": "ts",
    "baseUrl": ".",
    "paths": {
      "*": ["node_modules/@woltlab/wcf/ts/*"]
    }
  },
  "include": ["ts", "node_modules/@woltlab/wcf/ts"]
}"#
        .to_string(),
    );
    server.open_files.insert(
        "node_modules/@woltlab/wcf/ts/WoltLabSuite/Core/Component/Dialog.ts".to_string(),
        "export class Dialog {}".to_string(),
    );
    server
        .open_files
        .insert("ts/main.ts".to_string(), "Dialog".to_string());

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": "ts/main.ts",
            "startLine": 1,
            "startOffset": 1,
            "endLine": 1,
            "endOffset": 7,
            "errorCodes": [2304],
            "preferences": {
                "includeCompletionsForModuleExports": true,
                "includeCompletionsWithInsertText": true
            }
        }),
    };

    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected getCodeFixes to succeed");
    let body = resp.body.expect("expected getCodeFixes body");
    let fixes = body.as_array().expect("expected array response");
    let module_specifiers: Vec<String> = fixes
        .iter()
        .filter(|fix| fix.get("fixName").and_then(serde_json::Value::as_str) == Some("import"))
        .flat_map(|fix| {
            fix.get("changes")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
        })
        .flat_map(|change| {
            change
                .get("textChanges")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter_map(|text_change| {
            text_change
                .get("newText")
                .and_then(serde_json::Value::as_str)
        })
        .filter_map(extract_module_specifier_from_import_change)
        .collect();

    assert_eq!(
        module_specifiers,
        vec!["WoltLabSuite/Core/Component/Dialog".to_string()]
    );
}

#[test]
fn get_code_fixes_auto_import_package_root_path_type_module_prefers_main_subpath() {
    let mut server = make_server();
    server.open_files.insert(
        "/node_modules/pkg/package.json".to_string(),
        r#"{
    "name": "pkg",
    "version": "1.0.0",
    "main": "lib",
    "type": "module"
}"#
        .to_string(),
    );
    server.open_files.insert(
        "/node_modules/pkg/lib/index.js".to_string(),
        "export function foo() {}".to_string(),
    );
    server.open_files.insert(
        "/package.json".to_string(),
        r#"{
    "dependencies": {
       "pkg": "*"
    }
}"#
        .to_string(),
    );
    server
        .open_files
        .insert("/index.ts".to_string(), "foo".to_string());

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": "/index.ts",
            "startLine": 1,
            "startOffset": 1,
            "endLine": 1,
            "endOffset": 4,
            "errorCodes": [2304],
            "preferences": {
                "includeCompletionsForModuleExports": true,
                "includeCompletionsWithInsertText": true
            }
        }),
    };

    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected getCodeFixes to succeed");
    let body = resp.body.expect("expected getCodeFixes body");
    let fixes = body.as_array().expect("expected array response");
    let module_specifiers: Vec<String> = fixes
        .iter()
        .filter(|fix| fix.get("fixName").and_then(serde_json::Value::as_str) == Some("import"))
        .flat_map(|fix| {
            fix.get("changes")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
        })
        .flat_map(|change| {
            change
                .get("textChanges")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter_map(|text_change| {
            text_change
                .get("newText")
                .and_then(serde_json::Value::as_str)
        })
        .filter_map(extract_module_specifier_from_import_change)
        .collect();

    assert_eq!(module_specifiers, vec!["pkg/lib".to_string()]);
}

#[test]
fn get_code_fixes_prefers_package_import_map_specifier_for_non_relative_preference() {
    let mut server = make_server();
    server.allow_importing_ts_extensions = true;
    server.open_files.insert(
        "/package.json".to_string(),
        r##"{
  "type": "module",
  "imports": {
    "#src/*": "./SRC/*"
  }
}"##
        .to_string(),
    );
    server.open_files.insert(
        "/src/add.ts".to_string(),
        "export function add(a: number, b: number) {}".to_string(),
    );
    server
        .open_files
        .insert("/src/index.ts".to_string(), "add;\n".to_string());

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": "/src/index.ts",
            "startLine": 1,
            "startOffset": 1,
            "endLine": 1,
            "endOffset": 4,
            "errorCodes": [2304],
            "preferences": {
                "includeCompletionsForModuleExports": true,
                "includeCompletionsWithInsertText": true,
                "importModuleSpecifierPreference": "non-relative"
            }
        }),
    };

    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected getCodeFixes to succeed");
    let body = resp.body.expect("expected getCodeFixes body");
    let fixes = body.as_array().expect("expected array response");
    let module_specifiers: Vec<String> = fixes
        .iter()
        .filter(|fix| fix.get("fixName").and_then(serde_json::Value::as_str) == Some("import"))
        .flat_map(|fix| {
            fix.get("changes")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
        })
        .flat_map(|change| {
            change
                .get("textChanges")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter_map(|text_change| {
            text_change
                .get("newText")
                .and_then(serde_json::Value::as_str)
        })
        .filter_map(extract_module_specifier_from_import_change)
        .collect();

    assert_eq!(module_specifiers, vec!["#src/add.ts".to_string()]);
}

fn extract_module_specifier_from_import_change(new_text: &str) -> Option<String> {
    let (prefix_len, open_char) = if let Some(idx) = new_text.find("from \"") {
        (idx + "from ".len(), '"')
    } else if let Some(idx) = new_text.find("from '") {
        (idx + "from ".len(), '\'')
    } else if let Some(idx) = new_text.find("require(\"") {
        (idx + "require(".len(), '"')
    } else {
        let idx = new_text.find("require('")?;
        (idx + "require(".len(), '\'')
    };

    let rest = &new_text[prefix_len..];
    if !rest.starts_with(open_char) {
        return None;
    }

    let value = &rest[1..];
    let end = value.find(open_char)?;
    Some(value[..end].to_string())
}

#[test]
fn handle_get_combined_code_fix_fix_missing_import_merges_all_missing_names() {
    let mut server = make_server();
    let file1 = "/tests/cases/fourslash/file1.ts".to_string();
    let file2 = "/tests/cases/fourslash/file2.ts".to_string();
    server.open_files.insert(
        file1,
        "export interface Test1 {}\nexport interface Test2 {}\nexport interface Test3 {}\nexport interface Test4 {}\n".to_string(),
    );
    let original_file2 = "import { Test1, Test4 } from './file1';\ninterface Testing {\n    test1: Test1;\n    test2: Test2;\n    test3: Test3;\n    test4: Test4;\n}\n";
    server
        .open_files
        .insert(file2.clone(), original_file2.to_string());

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCombinedCodeFix".to_string(),
        arguments: serde_json::json!({
            "scope": { "type": "file", "args": { "file": file2 } },
            "fixId": "fixMissingImport",
            "preferences": {}
        }),
    };
    let resp = server.handle_get_combined_code_fix(1, &req);
    assert!(resp.success, "expected getCombinedCodeFix to succeed");

    let changes = resp
        .body
        .as_ref()
        .and_then(|body| body.get("changes"))
        .and_then(serde_json::Value::as_array)
        .expect("missing changes array");
    assert_eq!(changes.len(), 1, "expected one file change");
    let text_changes = changes[0]
        .get("textChanges")
        .and_then(serde_json::Value::as_array)
        .expect("missing textChanges");
    assert_eq!(
        text_changes.len(),
        1,
        "expected one consolidated text change"
    );

    let change = &text_changes[0];
    let start_line = change["start"]["line"].as_u64().expect("start line") as u32;
    let start_offset = change["start"]["offset"].as_u64().expect("start offset") as u32;
    let end_line = change["end"]["line"].as_u64().expect("end line") as u32;
    let end_offset = change["end"]["offset"].as_u64().expect("end offset") as u32;
    let new_text = change["newText"].as_str().expect("newText");

    let updated = Server::apply_change(
        original_file2,
        start_line,
        start_offset,
        end_line,
        end_offset,
        new_text,
    );

    assert_eq!(
        updated,
        "import { Test1, Test2, Test3, Test4 } from './file1';\ninterface Testing {\n    test1: Test1;\n    test2: Test2;\n    test3: Test3;\n    test4: Test4;\n}\n"
    );
}

#[test]
fn handle_get_combined_code_fix_fix_missing_import_in_declaration_file_keeps_value_and_type_split()
{
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "export class A {}\nexport class B {}\n".to_string(),
    );
    let original = "new A();\nlet x: B;\n";
    server
        .open_files
        .insert("/d.ts".to_string(), original.to_string());

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCombinedCodeFix".to_string(),
        arguments: serde_json::json!({
            "scope": { "type": "file", "args": { "file": "/d.ts" } },
            "fixId": "fixMissingImport",
            "preferences": {
                "preferTypeOnlyAutoImports": true
            }
        }),
    };

    let resp = server.handle_get_combined_code_fix(1, &req);
    assert!(resp.success, "expected getCombinedCodeFix to succeed");

    let changes = resp
        .body
        .as_ref()
        .and_then(|body| body.get("changes"))
        .and_then(serde_json::Value::as_array)
        .expect("missing changes array");
    assert_eq!(changes.len(), 1, "expected one file change");
    let text_changes = changes[0]
        .get("textChanges")
        .and_then(serde_json::Value::as_array)
        .expect("missing textChanges");
    assert_eq!(
        text_changes.len(),
        1,
        "expected one consolidated text change"
    );

    let change = &text_changes[0];
    let start_line = change["start"]["line"].as_u64().expect("start line") as u32;
    let start_offset = change["start"]["offset"].as_u64().expect("start offset") as u32;
    let end_line = change["end"]["line"].as_u64().expect("end line") as u32;
    let end_offset = change["end"]["offset"].as_u64().expect("end offset") as u32;
    let new_text = change["newText"].as_str().expect("newText");

    let updated = Server::apply_change(
        original,
        start_line,
        start_offset,
        end_line,
        end_offset,
        new_text,
    );

    assert_eq!(
        updated,
        "import { A, type B } from \"./a\";\n\nnew A();\nlet x: B;\n"
    );
}

#[test]
fn handle_get_code_fixes_missing_namespace_type_only_default_import() {
    let mut server = make_server();
    server.open_files.insert(
        "/tsconfig.json".to_string(),
        "{\n  \"compilerOptions\": {\n    \"module\": \"esnext\",\n    \"moduleResolution\": \"bundler\"\n  }\n}\n".to_string(),
    );
    server
        .open_files
        .insert("/a.ts".to_string(), "export class A {}\n".to_string());
    server.open_files.insert(
        "/ns.ts".to_string(),
        "export * as default from \"./a\";\n".to_string(),
    );
    let original = "let x: ns.A;\n";
    server
        .open_files
        .insert("/e.ts".to_string(), original.to_string());

    let diag_req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "semanticDiagnosticsSync".to_string(),
        arguments: serde_json::json!({
            "file": "/e.ts",
            "includeLinePosition": true
        }),
    };
    let diag_resp = server.handle_semantic_diagnostics_sync(1, &diag_req);
    assert!(
        diag_resp.success,
        "expected semanticDiagnosticsSync to succeed"
    );

    let namespace_diag = diag_resp
        .body
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .and_then(|diags| {
            diags.iter().find(|diag| {
                diag.get("code")
                    .and_then(serde_json::Value::as_u64)
                    .map(|code| code as u32)
                    == Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAMESPACE)
            })
        })
        .cloned()
        .expect("expected cannot-find-namespace diagnostic");

    let (start_line, start_offset, end_line, end_offset) =
        if let (Some(start_line), Some(start_offset), Some(end_line), Some(end_offset)) = (
            namespace_diag
                .get("start")
                .and_then(|start| start.get("line"))
                .and_then(serde_json::Value::as_u64),
            namespace_diag
                .get("start")
                .and_then(|start| start.get("offset"))
                .and_then(serde_json::Value::as_u64),
            namespace_diag
                .get("end")
                .and_then(|end| end.get("line"))
                .and_then(serde_json::Value::as_u64),
            namespace_diag
                .get("end")
                .and_then(|end| end.get("offset"))
                .and_then(serde_json::Value::as_u64),
        ) {
            (
                start_line as u32,
                start_offset as u32,
                end_line as u32,
                end_offset as u32,
            )
        } else {
            let line_map = super::LineMap::build(original);
            let start_off = namespace_diag
                .get("start")
                .and_then(serde_json::Value::as_u64)
                .expect("diagnostic start offset") as u32;
            let length = namespace_diag
                .get("length")
                .and_then(serde_json::Value::as_u64)
                .expect("diagnostic length") as u32;
            let start = line_map.offset_to_position(start_off, original);
            let end = line_map.offset_to_position(start_off + length, original);
            (
                start.line + 1,
                start.character + 1,
                end.line + 1,
                end.character + 1,
            )
        };

    let req = TsServerRequest {
        seq: 2,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": "/e.ts",
            "startLine": start_line,
            "startOffset": start_offset,
            "endLine": end_line,
            "endOffset": end_offset,
            "errorCodes": [tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAMESPACE],
            "preferences": {
                "preferTypeOnlyAutoImports": true
            }
        }),
    };
    let resp = server.handle_get_code_fixes(2, &req);
    assert!(resp.success, "expected getCodeFixes to succeed");

    let actions = resp
        .body
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .expect("expected actions array");
    let import_text = actions
        .iter()
        .find(|action| action.get("fixName").and_then(serde_json::Value::as_str) == Some("import"))
        .and_then(|action| action.get("changes"))
        .and_then(serde_json::Value::as_array)
        .and_then(|changes| changes.first())
        .and_then(|change| change.get("textChanges"))
        .and_then(serde_json::Value::as_array)
        .and_then(|text_changes| text_changes.first())
        .and_then(|text_change| text_change.get("newText"))
        .and_then(serde_json::Value::as_str)
        .expect("expected import code fix text change");

    assert!(
        import_text.contains("import type ns from \"./ns\";"),
        "expected type-only default namespace import edit, got: {import_text}"
    );

    // Fourslash `importFixAtPosition` probes a point location; ensure we
    // still surface the namespace import fix when no explicit error code is supplied.
    let point_req = TsServerRequest {
        seq: 3,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": "/e.ts",
            "startLine": start_line,
            "startOffset": start_offset,
            "endLine": start_line,
            "endOffset": start_offset,
            "preferences": {
                "preferTypeOnlyAutoImports": true
            }
        }),
    };
    let point_resp = server.handle_get_code_fixes(3, &point_req);
    assert!(point_resp.success, "expected point getCodeFixes to succeed");
    let point_actions = point_resp
        .body
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .expect("expected point actions array");
    let point_import_actions: Vec<&serde_json::Value> = point_actions
        .iter()
        .filter(|action| {
            action.get("fixName").and_then(serde_json::Value::as_str) == Some("import")
        })
        .collect();
    assert_eq!(
        point_import_actions.len(),
        1,
        "expected one point-position import fix, got: {point_actions:?}"
    );
    let point_import_text = point_import_actions
        .first()
        .and_then(|action| action.get("changes"))
        .and_then(serde_json::Value::as_array)
        .and_then(|changes| changes.first())
        .and_then(|change| change.get("textChanges"))
        .and_then(serde_json::Value::as_array)
        .and_then(|text_changes| text_changes.first())
        .and_then(|text_change| text_change.get("newText"))
        .and_then(serde_json::Value::as_str)
        .expect("expected point import code fix text change");
    assert!(
        point_import_text.contains("import type ns from \"./ns\";"),
        "expected point-position request to return default type-only namespace import, got: {point_actions:?}"
    );
}

#[test]
fn handle_get_code_fixes_omits_registry_only_placeholders() {
    let mut server = make_server();
    server.open_files.insert(
        "/tsconfig.json".to_string(),
        "{\n  \"compilerOptions\": {\n    \"target\": \"esnext\",\n    \"strict\": true,\n    \"lib\": [\"es2015\"]\n  }\n}\n".to_string(),
    );
    let content = "const p4: Promise<number> = new Promise(resolve => resolve());\n";
    server
        .open_files
        .insert("/a.ts".to_string(), content.to_string());

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": "/a.ts",
            "startLine": 1,
            "startOffset": 52,
            "endLine": 1,
            "endOffset": 59,
            "errorCodes": [2554]
        }),
    };
    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected getCodeFixes to succeed");
    let actions = resp
        .body
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .expect("expected actions array");
    assert!(
        actions.is_empty(),
        "expected no codefixes for unsupported TS2554 case, got: {actions:?}"
    );
}

#[test]
fn synthetic_missing_name_skips_qualified_type_names() {
    let mut server = make_server();
    server
        .open_files
        .insert("/a.ts".to_string(), "export class A {}\n".to_string());
    server.open_files.insert(
        "/ns.ts".to_string(),
        "export * as default from \"./a\";\n".to_string(),
    );
    let content = "let x: ns.A;\n".to_string();
    server
        .open_files
        .insert("/e.ts".to_string(), content.clone());

    let (_, binder, _, _) = server
        .parse_and_bind_file("/e.ts")
        .expect("expected parse_and_bind_file for /e.ts");
    let synthetic =
        server.synthetic_missing_name_expression_diagnostics("/e.ts", &content, &binder);

    assert!(
        synthetic.is_empty(),
        "expected no synthetic missing-name diagnostics for qualified type names, got {synthetic:?}"
    );
}

#[test]
fn synthetic_missing_name_skips_import_type_queries() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo/types/types.ts".to_string(),
        "export type Full = { prop: string; };\n".to_string(),
    );
    server.open_files.insert(
        "/foo/types/index.ts".to_string(),
        "import * as foo from './types';\nexport { foo };\n".to_string(),
    );
    let content = "import { foo } from './foo/types';\nexport type fullType = foo.Full;\ntype namespaceImport = typeof import('./foo/types');\ntype fullType2 = import('./foo/types').foo.Full;\n".to_string();
    server
        .open_files
        .insert("/app.ts".to_string(), content.clone());

    let (_, binder, _, _) = server
        .parse_and_bind_file("/app.ts")
        .expect("expected parse_and_bind_file for /app.ts");
    let synthetic =
        server.synthetic_missing_name_expression_diagnostics("/app.ts", &content, &binder);

    assert!(
        synthetic.is_empty(),
        "expected no synthetic missing-name diagnostics for import type queries, got {synthetic:?}"
    );
}
