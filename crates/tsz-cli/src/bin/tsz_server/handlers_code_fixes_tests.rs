use super::{
    LineMap, Server, TsServerRequest, parse_identifier_call_expression, positions_overlap,
};
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
        auto_imports_allowed_for_inferred_projects: true,
        inferred_module_is_none_for_projects: false,
        auto_import_specifier_exclude_regexes: Vec::new(),
        include_completions_with_class_member_snippets: false,
        plugin_configs: FxHashMap::default(),
    }
}

#[test]
fn get_code_fixes_jsdoc_infer_placeholders_match_fourslash_order_24_26() {
    let mut server = make_server();
    let cases = [
        (
            "/annotateWithTypeFromJSDoc24.ts",
            "class C {\n    /**\n     * @private\n     * @param {number} foo\n     * @param {Object} [bar]\n     * @param {String} bar.a\n     * @param {Number} [bar.b]\n     * @param bar.c\n     */\n    m(foo, bar) { }\n}\n",
            2usize,
        ),
        (
            "/annotateWithTypeFromJSDoc25.ts",
            "class C {\n    /**\n     * @private\n     * @param {number} foo\n     * @param {Object} [bar]\n     * @param {String} bar.a\n     * @param {Object} [baz]\n     * @param {number} baz.c\n     */\n    m(foo, bar, baz) { }\n}\n",
            3usize,
        ),
        (
            "/annotateWithTypeFromJSDoc26.ts",
            "class C {\n    /**\n     * @private\n     * @param {Object} [foo]\n     * @param {Object} foo.a\n     * @param {String} [foo.a.b]\n     */\n    m(foo) { }\n}\n",
            1usize,
        ),
    ];

    for (file, content, annotate_index_one_based) in cases {
        server
            .open_files
            .insert(file.to_string(), content.to_string());
        let callsite_offset = content.find("m(").expect("expected method declaration");
        let line_map = LineMap::build(content);
        let pos = line_map.offset_to_position(callsite_offset as u32, content);
        let req = TsServerRequest {
            seq: 1,
            _msg_type: "request".to_string(),
            command: "getCodeFixes".to_string(),
            arguments: serde_json::json!({
                "file": file,
                "startLine": pos.line + 1,
                "startOffset": pos.character + 1,
                "endLine": pos.line + 1,
                "endOffset": pos.character + 1,
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
        assert!(
            actions.len() >= annotate_index_one_based,
            "expected at least {annotate_index_one_based} actions for {file}, got {actions:?}"
        );
        let annotate = &actions[annotate_index_one_based - 1];
        assert_eq!(
            annotate
                .get("description")
                .and_then(serde_json::Value::as_str),
            Some("Annotate with type from JSDoc"),
            "unexpected action order for {file}: {actions:?}"
        );
    }
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
    assert_eq!(rewritten, "const { variants } = require(\"./matrix\");\n");
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

fn extract_module_specifier_from_import_change(new_text: &str) -> Option<String> {
    let (prefix_len, open_char) = if let Some(idx) = new_text.find("from \"") {
        (idx + "from ".len(), '"')
    } else if let Some(idx) = new_text.find("from '") {
        (idx + "from ".len(), '\'')
    } else if let Some(idx) = new_text.find("require(\"") {
        (idx + "require(".len(), '"')
    } else if let Some(idx) = new_text.find("require('") {
        (idx + "require(".len(), '\'')
    } else {
        return None;
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
    server.open_files.insert("/a.ts".to_string(), content.to_string());

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
    server.open_files.insert("/app.ts".to_string(), content.clone());

    let (_, binder, _, _) = server
        .parse_and_bind_file("/app.ts")
        .expect("expected parse_and_bind_file for /app.ts");
    let synthetic = server.synthetic_missing_name_expression_diagnostics("/app.ts", &content, &binder);

    assert!(
        synthetic.is_empty(),
        "expected no synthetic missing-name diagnostics for import type queries, got {synthetic:?}"
    );
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
