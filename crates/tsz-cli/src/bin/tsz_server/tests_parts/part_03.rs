#[ignore = "regressed by blDAJ binder change; needs LSP follow-through"]
#[test]
fn test_references_full_quoted_alias_includes_export_alias_side_definition_span() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo.ts".to_string(),
        [
            "type foo = \"foo\";",
            "export { type foo as \"__<alias>\" };",
            "import { type \"__<alias>\" as bar } from \"./foo\";",
            "const testBar: bar = \"foo\";",
        ]
        .join("\n"),
    );
    server.open_files.insert(
        "/bar.ts".to_string(),
        [
            "import { type \"__<alias>\" as first } from \"./foo\";",
            "export { type \"__<alias>\" as \"<other>\" } from \"./foo\";",
            "import { type \"<other>\" as second } from \"./bar\";",
            "const testFirst: first = \"foo\";",
            "const testSecond: second = \"foo\";",
        ]
        .join("\n"),
    );

    let req = make_request(
        "references-full",
        serde_json::json!({
            "file": "/foo.ts",
            "line": 2,
            "offset": 24
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("references-full should return body");
    let entries = body
        .as_array()
        .expect("references-full response should be array");

    let has_export_alias_side_span = entries.iter().any(|entry| {
        let Some(def_file) = entry["definition"]
            .get("fileName")
            .and_then(serde_json::Value::as_str)
        else {
            return false;
        };
        if def_file != "/bar.ts" {
            return false;
        }
        let Some(start) = entry["definition"]["textSpan"]
            .get("start")
            .and_then(serde_json::Value::as_u64)
        else {
            return false;
        };
        let Some(length) = entry["definition"]["textSpan"]
            .get("length")
            .and_then(serde_json::Value::as_u64)
        else {
            return false;
        };
        let end = start.saturating_add(length);
        let bar_source = server
            .open_files
            .get("/bar.ts")
            .expect("bar.ts should be open");
        bar_source
            .get(start as usize..end as usize)
            .is_some_and(|text| text == "\"<other>\"")
    });
    assert!(
        has_export_alias_side_span,
        "expected one references-full definition span to anchor on export alias-side token \"<other>\": {entries:?}"
    );
}

#[test]
fn test_type_only_quoted_alias_references_work_from_type_keyword_offset() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo.ts".to_string(),
        [
            "type foo = \"foo\";",
            "export { type foo as \"__<alias>\" };",
            "import { type \"__<alias>\" as bar } from \"./foo\";",
            "const testBar: bar = \"foo\";",
        ]
        .join("\n"),
    );
    server.open_files.insert(
        "/bar.ts".to_string(),
        [
            "import { type \"__<alias>\" as first } from \"./foo\";",
            "export { type \"__<alias>\" as \"<other>\" } from \"./foo\";",
            "import { type \"<other>\" as second } from \"./bar\";",
            "const testFirst: first = \"foo\";",
            "const testSecond: second = \"foo\";",
        ]
        .join("\n"),
    );

    // Place the query on `type`, not on the quoted string literal token.
    let refs_req = make_request(
        "references",
        serde_json::json!({
            "file": "/bar.ts",
            "line": 1,
            "offset": 10
        }),
    );
    let refs_resp = server.handle_tsserver_request(refs_req);
    assert!(refs_resp.success);
    let refs = refs_resp
        .body
        .expect("references should return body")
        .get("refs")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .expect("references should include refs array");
    assert!(
        refs.iter()
            .filter_map(|entry| entry.get("file").and_then(serde_json::Value::as_str))
            .any(|file| file == "/foo.ts"),
        "expected type-only quoted alias refs to include /foo.ts: {refs:?}"
    );
    assert!(
        refs.iter()
            .filter_map(|entry| entry.get("file").and_then(serde_json::Value::as_str))
            .any(|file| file == "/bar.ts"),
        "expected type-only quoted alias refs to include /bar.ts: {refs:?}"
    );
}

#[test]
fn test_definition_type_only_quoted_import_alias_resolves_to_exported_symbol() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo.ts".to_string(),
        [
            "type foo = \"foo\";",
            "export { type foo as \"__<alias>\" };",
            "import { type \"__<alias>\" as bar } from \"./foo\";",
            "const testBar: bar = \"foo\";",
        ]
        .join("\n"),
    );

    // Symbol metadata (`name`) lives on the `-full` shape, not on plain
    // `definition` (see #4002). Use `definition-full` to inspect it.
    let req = make_request(
        "definition-full",
        serde_json::json!({
            "file": "/foo.ts",
            "line": 3,
            "offset": 18
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let defs = resp
        .body
        .expect("definition-full should return body")
        .as_array()
        .cloned()
        .expect("definition-full response should be an array");
    assert!(
        defs.iter()
            .any(|entry| entry.get("name").and_then(serde_json::Value::as_str) == Some("foo")),
        "expected type-only quoted alias definition to resolve to exported symbol `foo`, got: {defs:?}"
    );
}

#[test]
fn test_definition_type_only_quoted_alias_marks_non_declare_target_as_local_non_ambient() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo.ts".to_string(),
        [
            "type foo = \"foo\";",
            "export { type foo as \"__<alias>\" };",
            "import { type \"__<alias>\" as bar } from \"./foo\";",
            "const testBar: bar = \"foo\";",
        ]
        .join("\n"),
    );

    // `isAmbient` / `isLocal` live on the `-full` shape, not on plain
    // `definition` (see #4002). Use `definition-full` to inspect them.
    let req = make_request(
        "definition-full",
        serde_json::json!({
            "file": "/foo.ts",
            "line": 3,
            "offset": 18
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let defs = resp
        .body
        .expect("definition-full should return body")
        .as_array()
        .cloned()
        .expect("definition-full response should be an array");
    let foo_def = defs
        .iter()
        .find(|entry| entry.get("name").and_then(serde_json::Value::as_str) == Some("foo"))
        .expect("expected foo definition entry");
    assert_eq!(
        foo_def
            .get("isAmbient")
            .and_then(serde_json::Value::as_bool),
        Some(false),
        "non-declare quoted alias definition should not be ambient: {foo_def:?}"
    );
    assert_eq!(
        foo_def.get("isLocal").and_then(serde_json::Value::as_bool),
        Some(true),
        "non-declare quoted alias definition should be local: {foo_def:?}"
    );
}

#[test]
fn test_check_options_experimental_decorators_deserialize() {
    // Verify that experimentalDecorators in JSON is correctly deserialized
    let json = r#"{"experimentalDecorators": true}"#;
    let options: CheckOptions = serde_json::from_str(json).unwrap();
    assert!(
        options.experimental_decorators,
        "experimentalDecorators should be true after deserialize"
    );
}

#[test]
fn test_check_options_experimental_decorators_default_false() {
    // Verify that default value is false
    let json = r#"{}"#;
    let options: CheckOptions = serde_json::from_str(json).unwrap();
    assert!(
        !options.experimental_decorators,
        "experimentalDecorators should default to false"
    );
}

#[test]
fn test_check_options_legacy_options_round_trip_to_checker_options() {
    // Issue #3579: the legacy `check` request previously hardcoded several
    // checker options to `false` even when the client supplied them. Now
    // they're plumbed through. Locks the deserialize+plumbing for the six
    // options the issue called out.
    let json = serde_json::json!({
        "verbatimModuleSyntax": true,
        "erasableSyntaxOnly": true,
        "allowImportingTsExtensions": true,
        "rewriteRelativeImportExtensions": true,
        "allowUmdGlobalAccess": true,
        "preserveConstEnums": true,
    });
    let options: CheckOptions = serde_json::from_value(json).unwrap();
    assert!(options.verbatim_module_syntax);
    assert!(options.erasable_syntax_only);
    assert!(options.allow_importing_ts_extensions);
    assert!(options.rewrite_relative_import_extensions);
    assert!(options.allow_umd_global_access);
    assert!(options.preserve_const_enums);

    // Shape-check the conversion to `CheckerOptions` — we don't run a full
    // check here (libs aren't loaded in the unit test), but the field
    // round-trip is the meaningful behavior change.
    let server = make_server();
    let checker = server.build_checker_options(&options);
    assert!(checker.verbatim_module_syntax);
    assert!(checker.erasable_syntax_only);
    assert!(checker.allow_importing_ts_extensions);
    assert!(checker.rewrite_relative_import_extensions);
    assert!(checker.allow_umd_global_access);
    assert!(checker.preserve_const_enums);
}

#[test]
fn test_check_options_legacy_options_default_false() {
    let options: CheckOptions = serde_json::from_str(r#"{}"#).unwrap();
    assert!(!options.verbatim_module_syntax);
    assert!(!options.erasable_syntax_only);
    assert!(!options.allow_importing_ts_extensions);
    assert!(!options.rewrite_relative_import_extensions);
    assert!(!options.allow_umd_global_access);
    assert!(!options.preserve_const_enums);
}

#[test]
fn test_check_options_deserializes_numeric_tsserver_enums() {
    let options: CheckOptions = serde_json::from_value(serde_json::json!({
        "noImplicitAny": true,
        "target": 2,
        "module": 1
    }))
    .unwrap();

    assert_eq!(options.target.as_deref(), Some("es2015"));
    assert_eq!(options.module.as_deref(), Some("commonjs"));
    assert_eq!(options.no_implicit_any, Some(true));
    assert_eq!(
        Server::parse_target(&Some("2".to_string())),
        tsz::emitter::ScriptTarget::ES2015
    );
    assert_eq!(
        Server::parse_module(&Some("1".to_string())),
        tsz::ModuleKind::CommonJS
    );
}

// =============================================================================
// handle_project_info / compute_project_info tests
// =============================================================================

#[test]
fn test_project_info_inferred_project_returns_libs_and_active_file() {
    // goTo.file("a.ts") on an inferred project (no tsconfig) with @lib: es5
    // should return [lib files..., a.ts] — only the active file, no siblings.
    let mut server = make_server_with_real_libs();
    server.open_files.insert(
        "/tests/cases/fourslash/server/a.ts".to_string(),
        "export var test = \"test String\"\n".to_string(),
    );
    server.open_files.insert(
        "/tests/cases/fourslash/server/b.ts".to_string(),
        "import test from \"./a\"\n".to_string(),
    );
    server.inferred_check_options.lib = Some(vec!["es5".to_string()]);

    let (config, files) = server.compute_project_info("/tests/cases/fourslash/server/a.ts");
    assert_eq!(config, "", "no tsconfig for inferred project");
    assert_eq!(
        files.last().map(String::as_str),
        Some("/tests/cases/fourslash/server/a.ts"),
        "active file must be last in inferred project list"
    );
    let lib_count = files
        .iter()
        .filter(|p| p.starts_with("/home/src/tslibs/TS/Lib/lib."))
        .count();
    assert!(
        lib_count >= 1,
        "expected at least one virtual lib file, got {files:?}"
    );
    // b.ts must NOT be in the list because goTo.file("a.ts") is the active file
    // and a.ts does not import b.ts.
    assert!(
        !files.iter().any(|p| p.ends_with("/b.ts")),
        "b.ts must not appear when active file is a.ts, got {files:?}"
    );
}

#[test]
fn test_project_info_inferred_project_uses_numeric_target_for_libs() {
    let mut server = make_server_with_real_libs();
    server
        .open_files
        .insert("/main.ts".to_string(), "const value = 1;\n".to_string());

    let options_resp = server.handle_tsserver_request(make_request(
        "compilerOptionsForInferredProjects",
        serde_json::json!({
            "options": {
                "target": 99
            }
        }),
    ));
    assert!(options_resp.success);

    let project_info_resp = server.handle_tsserver_request(make_request(
        "projectInfo",
        serde_json::json!({
            "file": "/main.ts",
            "needFileNameList": true
        }),
    ));

    assert!(project_info_resp.success);
    let file_names = project_info_resp
        .body
        .and_then(|body| body.get("fileNames").cloned())
        .and_then(|file_names| file_names.as_array().cloned())
        .expect("projectInfo should include fileNames");
    assert!(
        file_names.iter().any(|file| file
            .as_str()
            .is_some_and(|path| path.ends_with("lib.esnext.full.d.ts"))),
        "numeric target 99 should select ESNext libs, got {file_names:?}"
    );
}

#[test]
fn test_project_info_inferred_project_includes_transitive_imports() {
    // b.ts imports ./a — goTo.file("b.ts") should include a.ts first, then b.ts.
    let mut server = make_server_with_real_libs();
    server.open_files.insert(
        "/tests/cases/fourslash/server/a.ts".to_string(),
        "export var test = \"test String\"\n".to_string(),
    );
    server.open_files.insert(
        "/tests/cases/fourslash/server/b.ts".to_string(),
        "import test from \"./a\"\n".to_string(),
    );
    server.inferred_check_options.lib = Some(vec!["es5".to_string()]);

    let (_, files) = server.compute_project_info("/tests/cases/fourslash/server/b.ts");
    let project_files: Vec<&str> = files
        .iter()
        .filter(|p| !p.starts_with("/home/src/tslibs/TS/Lib/"))
        .map(String::as_str)
        .collect();
    assert_eq!(
        project_files,
        vec![
            "/tests/cases/fourslash/server/a.ts",
            "/tests/cases/fourslash/server/b.ts",
        ],
        "expected transitive imports before active file, got {project_files:?}"
    );
}

#[test]
fn test_project_info_configured_project_lists_files_and_config_last() {
    // tsconfig declares files: [a.ts, b.ts]; output should be
    // [libs..., a.ts, b.ts, tsconfig.json] and exclude non-existent files.
    let mut server = make_server_with_real_libs();
    let tsconfig_path = "/tests/cases/fourslash/server/tsconfig.json".to_string();
    server.open_files.insert(
        tsconfig_path.clone(),
        r#"{ "files": ["a.ts", "b.ts"], "compilerOptions": { "lib": ["es5"] } }"#.to_string(),
    );
    server.open_files.insert(
        "/tests/cases/fourslash/server/a.ts".to_string(),
        "export var test = \"test String\"\n".to_string(),
    );
    server.open_files.insert(
        "/tests/cases/fourslash/server/b.ts".to_string(),
        "export var test2 = \"test String\"\n".to_string(),
    );

    let (config, files) = server.compute_project_info("/tests/cases/fourslash/server/a.ts");
    assert_eq!(config, tsconfig_path);
    let non_lib: Vec<&str> = files
        .iter()
        .filter(|p| !p.starts_with("/home/src/tslibs/TS/Lib/"))
        .map(String::as_str)
        .collect();
    assert_eq!(
        non_lib,
        vec![
            "/tests/cases/fourslash/server/a.ts",
            "/tests/cases/fourslash/server/b.ts",
            "/tests/cases/fourslash/server/tsconfig.json",
        ],
        "expected tsconfig files in declared order with config file last"
    );
}

#[test]
fn test_project_info_configured_project_expands_include_files() {
    let mut server = make_server_with_real_libs();
    let tsconfig_path = "/tests/cases/fourslash/server/tsconfig.json".to_string();
    server.open_files.insert(
        tsconfig_path.clone(),
        r#"{ "include": ["src/**/*.ts"], "compilerOptions": { "lib": ["es5"] } }"#.to_string(),
    );
    server.open_files.insert(
        "/tests/cases/fourslash/server/src/a.ts".to_string(),
        "export const a = 1;\n".to_string(),
    );
    server.open_files.insert(
        "/tests/cases/fourslash/server/src/b.ts".to_string(),
        "export const b = 2;\n".to_string(),
    );
    server.open_files.insert(
        "/tests/cases/fourslash/server/other.ts".to_string(),
        "export const other = 3;\n".to_string(),
    );

    let (config, files) = server.compute_project_info("/tests/cases/fourslash/server/src/a.ts");
    assert_eq!(config, tsconfig_path);
    let non_lib: Vec<&str> = files
        .iter()
        .filter(|p| !p.starts_with("/home/src/tslibs/TS/Lib/"))
        .map(String::as_str)
        .collect();
    assert_eq!(
        non_lib,
        vec![
            "/tests/cases/fourslash/server/src/a.ts",
            "/tests/cases/fourslash/server/src/b.ts",
            "/tests/cases/fourslash/server/tsconfig.json",
        ],
        "include glob should add matching project files before the config"
    );
}

#[test]
fn test_project_info_configured_project_excludes_non_existent_files() {
    // tsconfig declares files: [a.ts, c.ts, b.ts]; c.ts is not in open_files
    // so it must be filtered out, preserving the relative order of the rest.
    let mut server = make_server_with_real_libs();
    server.open_files.insert(
        "/tests/cases/fourslash/server/tsconfig.json".to_string(),
        r#"{ "files": ["a.ts", "c.ts", "b.ts"], "compilerOptions": { "lib": ["es5"] } }"#
            .to_string(),
    );
    server.open_files.insert(
        "/tests/cases/fourslash/server/a.ts".to_string(),
        "export var test = \"test String\"\n".to_string(),
    );
    server.open_files.insert(
        "/tests/cases/fourslash/server/b.ts".to_string(),
        "export var test2 = \"test String\"\n".to_string(),
    );

    let (_, files) = server.compute_project_info("/tests/cases/fourslash/server/a.ts");
    let non_lib: Vec<&str> = files
        .iter()
        .filter(|p| !p.starts_with("/home/src/tslibs/TS/Lib/"))
        .map(String::as_str)
        .collect();
    assert_eq!(
        non_lib,
        vec![
            "/tests/cases/fourslash/server/a.ts",
            "/tests/cases/fourslash/server/b.ts",
            "/tests/cases/fourslash/server/tsconfig.json",
        ],
        "non-existent c.ts must be excluded"
    );
}

#[test]
fn test_project_info_lib_files_use_fourslash_virtual_folder() {
    let mut server = make_server_with_real_libs();
    server.open_files.insert(
        "/tests/cases/fourslash/server/a.ts".to_string(),
        "export var t = 1;\n".to_string(),
    );
    server.inferred_check_options.lib = Some(vec!["es5".to_string()]);

    let (_, files) = server.compute_project_info("/tests/cases/fourslash/server/a.ts");
    let libs: Vec<&str> = files
        .iter()
        .filter(|p| p.starts_with("/home/src/tslibs/TS/Lib/"))
        .map(String::as_str)
        .collect();
    assert!(
        libs.contains(&"/home/src/tslibs/TS/Lib/lib.es5.d.ts"),
        "expected lib.es5.d.ts under fourslash virtual lib folder, got {libs:?}"
    );
    // @lib: es5 pulls in decorators + decorators.legacy via transitive refs.
    assert!(
        libs.iter().any(|p| p.ends_with("/lib.decorators.d.ts")),
        "expected lib.decorators.d.ts to be transitively included, got {libs:?}"
    );
}

#[test]
fn test_project_info_lib_files_use_real_paths_outside_fourslash() {
    // Production tsserver clients send real on-disk paths, not the fourslash
    // VFS mount. projectInfo must return the actual lib paths the server is
    // using, not the harness's `/home/src/tslibs/TS/Lib/...` rewrites.
    // Regression for https://github.com/mohsen1/tsz/issues/3779.
    let mut server = make_server_with_real_libs();
    server.open_files.insert(
        "/private/tmp/tsz-projectinfo-repro/a.ts".to_string(),
        "const value = 1;\n".to_string(),
    );
    server.inferred_check_options.lib = Some(vec!["es5".to_string()]);

    let (_, files) = server.compute_project_info("/private/tmp/tsz-projectinfo-repro/a.ts");

    let leaked: Vec<&str> = files
        .iter()
        .filter(|p| p.starts_with("/home/src/tslibs/TS/Lib/"))
        .map(String::as_str)
        .collect();
    assert!(
        leaked.is_empty(),
        "production paths must not contain fourslash VFS lib paths, got {leaked:?}"
    );

    let lib_files: Vec<&str> = files
        .iter()
        .filter(|p| p.contains("lib.") && p.ends_with(".d.ts"))
        .map(String::as_str)
        .collect();
    assert!(
        lib_files.iter().any(|p| p.ends_with("/lib.es5.d.ts")),
        "expected real lib.es5.d.ts path among lib files, got {lib_files:?}"
    );
    for path in &lib_files {
        assert!(
            std::path::Path::new(path).is_absolute(),
            "lib path must be absolute (real on-disk path), got {path:?}"
        );
        assert!(
            !path.starts_with("/home/src/tslibs/"),
            "lib path must not be a fourslash VFS path, got {path:?}"
        );
    }
}

#[test]
fn test_project_info_no_lib_suppresses_lib_files() {
    let mut server = make_server_with_real_libs();
    server.open_files.insert(
        "/tests/cases/fourslash/server/a.ts".to_string(),
        "export var t = 1;\n".to_string(),
    );
    server.inferred_check_options.no_lib = true;
    server.inferred_check_options.lib = Some(vec!["es5".to_string()]);

    let (_, files) = server.compute_project_info("/tests/cases/fourslash/server/a.ts");
    let lib_count = files
        .iter()
        .filter(|p| p.starts_with("/home/src/tslibs/TS/Lib/"))
        .count();
    assert_eq!(
        lib_count, 0,
        "noLib must suppress all lib files, got {files:?}"
    );
}

mod brace_code_map {
    use super::super::{build_code_map, scan_forward};

    fn match_brace(src: &str, open_pos: usize) -> Option<usize> {
        let bytes = src.as_bytes();
        let map = build_code_map(bytes);
        assert!(
            map[open_pos],
            "test setup error: open position {open_pos} is not in code"
        );
        scan_forward(bytes, &map, open_pos, b'{', b'}')
    }

    #[test]
    fn brace_match_skips_close_brace_inside_regex_literal() {
        // Repro from issue #4013: the `}` inside `/}/` must be ignored when
        // matching the outer block braces.
        let src = "if (true) { const r = /}/; }";
        let open = src.find('{').unwrap();
        let outer_close = src.rfind('}').unwrap();
        assert_eq!(match_brace(src, open), Some(outer_close));
    }

    #[test]
    fn brace_match_skips_close_brace_inside_regex_character_class() {
        // `[}]` is a character class containing only `}`; outer braces still
        // pair with the trailing `}`.
        let src = "{ const r = /[}]/; }";
        let open = src.find('{').unwrap();
        let outer_close = src.rfind('}').unwrap();
        assert_eq!(match_brace(src, open), Some(outer_close));
    }

    #[test]
    fn brace_match_handles_regex_with_flags() {
        let src = "{ const r = /}/gi; }";
        let open = src.find('{').unwrap();
        let outer_close = src.rfind('}').unwrap();
        assert_eq!(match_brace(src, open), Some(outer_close));
    }

    #[test]
    fn brace_match_treats_division_as_code_not_regex() {
        // After a value-producing token (`)`), `/` is division, not a regex.
        // The `}` inside the comment is non-code; `b` and `c` are identifiers.
        // This guards against the regex heuristic mis-firing on division.
        let src = "{ let x = (a) / b / c; }";
        let open = src.find('{').unwrap();
        let outer_close = src.rfind('}').unwrap();
        assert_eq!(match_brace(src, open), Some(outer_close));
    }

    #[test]
    fn brace_match_handles_regex_after_return_keyword() {
        let src = "function f() { return /}/.test(\"\"); }";
        let open = src.find('{').unwrap();
        let outer_close = src.rfind('}').unwrap();
        assert_eq!(match_brace(src, open), Some(outer_close));
    }

    #[test]
    fn brace_match_handles_regex_with_escaped_slash() {
        let src = "{ const r = /\\/}/; }";
        let open = src.find('{').unwrap();
        let outer_close = src.rfind('}').unwrap();
        assert_eq!(match_brace(src, open), Some(outer_close));
    }
}

// Issue #3718: refactor handlers must accept position-only requests
// (`line`/`offset`) in addition to range requests
// (`startLine`/`startOffset`/`endLine`/`endOffset`). tsserver dispatches
// `FileLocationOrRangeRequestArgs` to the language service in either form.
#[test]
fn parse_refactor_request_range_accepts_range_form() {
    let request = make_request(
        "getApplicableRefactors",
        serde_json::json!({
            "file": "/a.ts",
            "startLine": 2,
            "startOffset": 13,
            "endLine": 2,
            "endOffset": 18,
        }),
    );
    let span = Server::parse_refactor_request_range(&request).expect("range form must parse");
    assert_eq!(span, (2, 13, 2, 18));
}

#[test]
fn parse_refactor_request_range_accepts_position_only_form_as_zero_width() {
    // Issue #3718: when only `line`/`offset` are sent, treat the request as a
    // zero-width range at that position so the handler proceeds instead of
    // bailing through the optional-chained `?` on the missing range fields.
    let request = make_request(
        "getApplicableRefactors",
        serde_json::json!({
            "file": "/a.ts",
            "line": 2,
            "offset": 9,
        }),
    );
    let span = Server::parse_refactor_request_range(&request).expect("position-only must parse");
    assert_eq!(span, (2, 9, 2, 9));
}

#[test]
fn parse_refactor_request_range_prefers_explicit_range_over_position() {
    // If both forms are present, the explicit range wins (matches tsserver).
    let request = make_request(
        "getApplicableRefactors",
        serde_json::json!({
            "file": "/a.ts",
            "startLine": 1, "startOffset": 1, "endLine": 1, "endOffset": 5,
            "line": 9, "offset": 9,
        }),
    );
    let span = Server::parse_refactor_request_range(&request).expect("range form must win");
    assert_eq!(span, (1, 1, 1, 5));
}

#[test]
fn parse_refactor_request_range_returns_none_without_position_or_range() {
    let request = make_request(
        "getApplicableRefactors",
        serde_json::json!({"file": "/a.ts"}),
    );
    assert!(Server::parse_refactor_request_range(&request).is_none());
}

#[test]
fn applicable_refactors_position_only_request_returns_success_not_error() {
    // End-to-end smoke check that position-only requests no longer fail the
    // request shape. The body may be empty depending on whether an
    // extractable expression sits at the cursor; the contract is that the
    // request completes successfully (it used to return `None` from the
    // closure when `startLine` was missing).
    let mut server = make_server();
    assert!(
        server
            .handle_tsserver_request(make_request(
                "open",
                serde_json::json!({
                    "file": "/src/pos.ts",
                    "fileContent": "function f() {\n  const value = 1 + 2;\n  return value;\n}\n",
                }),
            ))
            .success
    );
    let response = server.handle_tsserver_request(make_request(
        "getApplicableRefactors",
        serde_json::json!({"file": "/src/pos.ts", "line": 2, "offset": 9}),
    ));
    assert!(response.success);
    assert!(response.body.is_some_and(|b| b.is_array()));
}

// Issue #3803: getApplicableRefactors must NOT emit the `_scope_1` actions
// when the request range has no enclosing function — only the global scope
// is meaningful — and every action must carry a `range`.
#[test]
fn applicable_refactors_top_level_expression_emits_one_scope_with_range() {
    let mut server = make_server();
    assert!(
        server
            .handle_tsserver_request(make_request(
                "open",
                serde_json::json!({
                    "file": "/src/top.ts",
                    "fileContent": "const y = 1 + 2;\n",
                }),
            ))
            .success
    );

    let response = server.handle_tsserver_request(make_request(
        "getApplicableRefactors",
        serde_json::json!({
            "file": "/src/top.ts",
            "startLine": 1,
            "startOffset": 11,
            "endLine": 1,
            "endOffset": 16,
        }),
    ));

    assert!(response.success);
    let body = response.body.expect("refactors should return a body");
    let refactors = body.as_array().expect("refactors must be an array");
    let actions: Vec<_> = refactors
        .iter()
        .flat_map(|r| {
            r.get("actions")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
        })
        .collect();

    let names: Vec<&str> = actions
        .iter()
        .filter_map(|a| a.get("name").and_then(serde_json::Value::as_str))
        .collect();
    assert!(
        names.contains(&"function_scope_0"),
        "expected function_scope_0, got {names:?}"
    );
    assert!(
        names.contains(&"constant_scope_0"),
        "expected constant_scope_0, got {names:?}"
    );
    assert!(
        !names.contains(&"function_scope_1"),
        "must not emit function_scope_1 at module level, got {names:?}"
    );
    assert!(
        !names.contains(&"constant_scope_1"),
        "must not emit constant_scope_1 at module level, got {names:?}"
    );

    for action in &actions {
        assert!(
            action.get("range").is_some(),
            "action missing range: {action:#}"
        );
    }

    let fn_action = actions
        .iter()
        .find(|a| a.get("name").and_then(serde_json::Value::as_str) == Some("function_scope_0"))
        .expect("function_scope_0 must exist");
    let desc = fn_action
        .get("description")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    assert!(
        desc.contains("global scope"),
        "function_scope_0 should target global scope at module level, got: {desc}"
    );
}

// Issue #3784: emit-output should honor the owning project's
// `compilerOptions.module` and `compilerOptions.outDir` so the result
// matches what tsc reports for the same file.
#[test]
fn emit_output_honors_tsconfig_module_and_out_dir() {
    let temp = tempfile::tempdir().expect("temp dir");
    let root = temp.path();
    let config = root.join("tsconfig.json");
    let a = root.join("a.ts");
    std::fs::write(
        &config,
        r#"{"compilerOptions":{"target":"es2015","module":"commonjs","outDir":"dist"},"files":["a.ts"]}"#,
    )
    .expect("write config");
    std::fs::write(&a, "export const value = 1;\n").expect("write a");

    let mut server = make_server();
    let a_str = a.to_string_lossy().to_string();

    let open =
        server.handle_tsserver_request(make_request("open", serde_json::json!({ "file": &a_str })));
    assert!(open.success);

    let response = server.handle_tsserver_request(make_request(
        "emit-output",
        serde_json::json!({ "file": &a_str }),
    ));
    assert!(response.success);
    let body = response.body.expect("emit-output body");

    // Output path should land under outDir/dist/a.js, not next to the source.
    let expected_out = root.join("dist").join("a.js");
    let expected_out_str = expected_out.to_string_lossy().to_string();
    assert_eq!(
        body["outputFiles"][0]["name"], expected_out_str,
        "expected outDir-relative path, got: {body:#}"
    );

    // CommonJS module output should have an exports.value assignment.
    let text = body["outputFiles"][0]["text"]
        .as_str()
        .expect("output text");
    assert!(
        text.contains("exports.value"),
        "expected CommonJS exports lowering, got: {text}"
    );
    assert!(
        !text.contains("export const"),
        "must not preserve ES module syntax when module=commonjs, got: {text}"
    );
}

// Issue #3731: tsserver's jsxClosingTag returns a closing tag for ALL JSX
// elements including intrinsic HTML void elements like <input>; tsz used
// to suppress them.
#[test]
fn jsx_closing_tag_returns_close_for_intrinsic_void_elements() {
    let void_cases = [
        ("/input.tsx", "<input>", 8, "</input>"),
        ("/img.tsx", "<img>", 6, "</img>"),
        ("/br.tsx", "<br>", 5, "</br>"),
        ("/hr.tsx", "<hr>", 5, "</hr>"),
    ];

    for (file, content, offset, expected_close) in void_cases {
        let mut server = make_server();
        server
            .open_files
            .insert(file.to_string(), content.to_string());

        let response = server.handle_tsserver_request(make_request(
            "jsxClosingTag",
            serde_json::json!({"file": file, "line": 1, "offset": offset}),
        ));
        assert!(response.success, "{file}: jsxClosingTag should succeed");
        let body = response
            .body
            .unwrap_or_else(|| panic!("{file}: expected a body for {content:?}"));
        assert_eq!(
            body.get("newText").and_then(serde_json::Value::as_str),
            Some(expected_close),
            "{file}: expected {expected_close}, got body={body:?}"
        );
        assert_eq!(
            body.get("caretOffset").and_then(serde_json::Value::as_u64),
            Some(0),
            "{file}: must include caretOffset: 0, got body={body:?}"
        );
    }
}

// Issue #3718: getApplicableRefactors and getEditsForRefactor accept
// FileLocationOrRangeRequestArgs — a client may send `{ line, offset }`
// instead of `{ startLine, startOffset, endLine, endOffset }`. Pre-fix,
// the handlers bailed via `?` because the range fields were absent.
#[test]
fn applicable_refactors_accepts_position_only_request() {
    let mut server = make_server();
    server.open_files.insert(
        "/src/pos.ts".to_string(),
        "function f() {\n  const value = 1 + 2;\n  return value;\n}\n".to_string(),
    );

    let response = server.handle_tsserver_request(make_request(
        "getApplicableRefactors",
        serde_json::json!({
            "file": "/src/pos.ts",
            "line": 2,
            "offset": 17,
        }),
    ));

    assert!(response.success);
    let body = response
        .body
        .expect("position-only refactor request must return a body");
    assert!(
        body.is_array(),
        "position-only refactor body must be an array, got: {body:?}"
    );
}

#[test]
fn edits_for_refactor_accepts_position_only_request() {
    let mut server = make_server();
    server.open_files.insert(
        "/src/pos2.ts".to_string(),
        "function f() {\n  const value = 1 + 2;\n  return value;\n}\n".to_string(),
    );

    let response = server.handle_tsserver_request(make_request(
        "getEditsForRefactor",
        serde_json::json!({
            "file": "/src/pos2.ts",
            "line": 2,
            "offset": 17,
            "refactor": "Extract Symbol",
            "action": "constant_scope_0",
        }),
    ));

    assert!(response.success);
    let body = response
        .body
        .expect("position-only edits request must return a body");
    assert!(
        body.get("edits").is_some(),
        "edits-for-refactor body must contain `edits`: {body:?}"
    );
}

// Issue #3752: docCommentTemplate must NOT scan into a nested same-line
// function-like signature when the documented declaration is a non-callable
// kind (type alias, interface, class, …). tsc returns the unchanged
// one-line `/** */` template in those cases.
#[test]
fn doc_comment_template_non_callable_type_alias_returns_one_line() {
    let mut server = make_server();
    server.open_files.insert(
        "/src/type_alias.ts".to_string(),
        "/** */\ntype F = (x: string) => number;\n".to_string(),
    );

    let response = server.handle_tsserver_request(make_request(
        "docCommentTemplate",
        serde_json::json!({
            "file": "/src/type_alias.ts",
            "line": 1,
            "offset": 4,
            "generateReturnInDocTemplate": true,
        }),
    ));

    assert!(response.success);
    let body = response.body.expect("docCommentTemplate body");
    assert_eq!(
        body.get("newText").and_then(serde_json::Value::as_str),
        Some("/** */"),
        "type alias must not extract @param from nested signature, got: {body:?}"
    );
    assert_eq!(
        body.get("caretOffset").and_then(serde_json::Value::as_u64),
        Some(3)
    );
}

#[test]
fn doc_comment_template_non_callable_interface_returns_one_line() {
    let mut server = make_server();
    server.open_files.insert(
        "/src/iface.ts".to_string(),
        "/** */\ninterface I { m(x: string): void; }\n".to_string(),
    );

    let response = server.handle_tsserver_request(make_request(
        "docCommentTemplate",
        serde_json::json!({
            "file": "/src/iface.ts",
            "line": 1,
            "offset": 4,
            "generateReturnInDocTemplate": true,
        }),
    ));

    assert!(response.success);
    let body = response.body.expect("docCommentTemplate body");
    assert_eq!(
        body.get("newText").and_then(serde_json::Value::as_str),
        Some("/** */"),
        "interface must not extract @param from nested method signature, got: {body:?}"
    );
}

#[test]
fn doc_comment_template_non_callable_class_returns_one_line() {
    let mut server = make_server();
    server.open_files.insert(
        "/src/class.ts".to_string(),
        "/** */\nclass C { constructor(public x: number) {} }\n".to_string(),
    );

    let response = server.handle_tsserver_request(make_request(
        "docCommentTemplate",
        serde_json::json!({
            "file": "/src/class.ts",
            "line": 1,
            "offset": 4,
            "generateReturnInDocTemplate": true,
        }),
    ));

    assert!(response.success);
    let body = response.body.expect("docCommentTemplate body");
    assert_eq!(
        body.get("newText").and_then(serde_json::Value::as_str),
        Some("/** */"),
        "class must not extract @param from nested constructor, got: {body:?}"
    );
}

// Sanity: a regular function declaration must STILL extract @param tags.
#[test]
fn doc_comment_template_function_decl_still_extracts_params() {
    let mut server = make_server();
    server.open_files.insert(
        "/src/fn.ts".to_string(),
        "/** */\nfunction f(x: string, y: number) {}\n".to_string(),
    );

    let response = server.handle_tsserver_request(make_request(
        "docCommentTemplate",
        serde_json::json!({
            "file": "/src/fn.ts",
            "line": 1,
            "offset": 4,
            "generateReturnInDocTemplate": false,
        }),
    ));

    assert!(response.success);
    let body = response.body.expect("docCommentTemplate body");
    let new_text = body
        .get("newText")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    assert!(
        new_text.contains("@param x"),
        "function decl should extract @param x, got: {new_text}"
    );
    assert!(
        new_text.contains("@param y"),
        "function decl should extract @param y, got: {new_text}"
    );
}

// Issue #3798: getMoveToRefactoringFileSuggestions should derive newFileName
// from the selected declaration and include configured project files in the
// candidate list, not just open files.
#[test]
fn move_to_file_suggestions_derive_new_file_name_from_declaration() {
    let temp = tempfile::tempdir().expect("temp dir");
    let root = temp.path();
    let config = root.join("tsconfig.json");
    let a = root.join("a.ts");
    let b = root.join("b.ts");
    std::fs::write(
        &config,
        r#"{"compilerOptions":{"target":"es2020","module":"esnext"},"files":["a.ts","b.ts"]}"#,
    )
    .expect("config");
    std::fs::write(&a, "export function moveMe() {}\n").expect("a");
    std::fs::write(&b, "export const other = 1;\n").expect("b");

    let mut server = make_server();
    let a_str = a.to_string_lossy().to_string();
    let b_str = b.to_string_lossy().to_string();

    // Open ONLY a.ts. tsserver should still surface b.ts via the project's file list.
    let open =
        server.handle_tsserver_request(make_request("open", serde_json::json!({ "file": &a_str })));
    assert!(open.success);

    let response = server.handle_tsserver_request(make_request(
        "getMoveToRefactoringFileSuggestions",
        serde_json::json!({
            "file": &a_str,
            "startLine": 1,
            "startOffset": 1,
            "endLine": 1,
            "endOffset": 28,
        }),
    ));
    assert!(response.success);
    let body = response.body.expect("move-to suggestions body");

    let new_file = body["newFileName"]
        .as_str()
        .expect("newFileName must be a string");
    assert!(
        new_file.ends_with("/moveMe.ts") || new_file.ends_with("\\moveMe.ts"),
        "newFileName should be derived from the selected declaration `moveMe`, got: {new_file}"
    );

    let files = body["files"].as_array().expect("files must be an array");
    let file_set: std::collections::HashSet<&str> =
        files.iter().filter_map(serde_json::Value::as_str).collect();
    assert!(
        file_set.contains(b_str.as_str()),
        "files must include the configured project's b.ts even though it's not open, got: {files:?}"
    );
    // Don't include the source file itself
    assert!(
        !file_set.contains(a_str.as_str()),
        "files must not include the source file itself, got: {files:?}"
    );
}
