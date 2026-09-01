use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

#[test]
fn unmatched_include_is_a_normal_diagnostic_and_still_writes_project_stats() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{
            "compilerOptions": { "noEmit": true },
            "include": ["missing/**/*.ts"]
        }"#,
    )
    .unwrap();
    let stats_path = project.path().join("stats.json");

    let output = run_tsz(
        project.path(),
        [
            "--project",
            project.path().to_str().unwrap(),
            "--pretty",
            "false",
            "--extendedDiagnostics",
            "--perf-counters-json",
            stats_path.to_str().unwrap(),
        ],
    );

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("error TS18003: No inputs were found"),
        "{stdout}"
    );
    assert!(
        stdout.contains("Root files:                    0"),
        "{stdout}"
    );
    assert!(
        stdout.contains("Source files:                  0"),
        "{stdout}"
    );
    assert!(
        stdout.contains("Project configs:               1"),
        "{stdout}"
    );
    assert!(
        stdout.contains("Project references:            0"),
        "{stdout}"
    );

    let stats = read_json(&stats_path);
    assert_eq!(stats["schema_version"], 2);
    assert_eq!(stats["stats"]["files"], 0);
    assert_eq!(stats["stats"]["root_files"], 0);
    assert_eq!(stats["stats"]["source_files"], 0);
    assert_eq!(stats["stats"]["root_file_paths"], serde_json::json!([]));
    assert_eq!(stats["stats"]["source_file_paths"], serde_json::json!([]));
    assert_eq!(stats["stats"]["project_configs"], 1);
    assert_eq!(stats["stats"]["project_references"], 0);
}

#[test]
fn references_only_project_succeeds_without_inventing_source_counts() {
    let project = tempfile::tempdir().unwrap();
    let dependency = project.path().join("dependency");
    std::fs::create_dir(&dependency).unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{
            "files": [],
            "references": [{ "path": "./dependency" }],
            "compilerOptions": { "noEmit": true }
        }"#,
    )
    .unwrap();
    std::fs::write(
        dependency.join("tsconfig.json"),
        r#"{ "files": ["index.ts"], "compilerOptions": { "noEmit": true } }"#,
    )
    .unwrap();
    std::fs::write(dependency.join("index.ts"), "export const value = 1;").unwrap();
    let stats_path = project.path().join("stats.json");

    let output = run_tsz(
        project.path(),
        [
            "--project",
            project.path().to_str().unwrap(),
            "--pretty",
            "false",
            "--perf-counters-json",
            stats_path.to_str().unwrap(),
        ],
    );

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    let stats = read_json(&stats_path);
    assert_eq!(stats["stats"]["files"], 0);
    assert_eq!(stats["stats"]["root_files"], 0);
    assert_eq!(stats["stats"]["source_files"], 0);
    assert_eq!(stats["stats"]["project_configs"], 1);
    assert_eq!(stats["stats"]["project_references"], 1);
}

#[test]
fn explicit_command_line_false_overrides_project_true() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{
            "compilerOptions": { "noEmit": true },
            "files": ["case.ts"]
        }"#,
    )
    .unwrap();
    std::fs::write(project.path().join("case.ts"), "export const value = 1;").unwrap();

    let output = run_tsz(
        project.path(),
        [
            "--project",
            project.path().to_str().unwrap(),
            "--noEmit",
            "false",
            "--pretty",
            "false",
        ],
    );

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert!(project.path().join("case.js").is_file());
}

#[test]
fn explicit_no_implicit_any_false_overrides_strict_default() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("implicit.ts"),
        "function identity(value) { return value; }\n",
    )
    .unwrap();

    let strict_default = run_tsz(
        project.path(),
        [
            "--ignoreConfig",
            "--noEmit",
            "--pretty",
            "false",
            "implicit.ts",
        ],
    );
    assert_eq!(strict_default.status.code(), Some(1));
    assert!(
        String::from_utf8(strict_default.stdout)
            .unwrap()
            .contains("error TS7006")
    );

    let opted_out = run_tsz(
        project.path(),
        [
            "--ignoreConfig",
            "--noEmit",
            "--noImplicitAny",
            "false",
            "--pretty",
            "false",
            "implicit.ts",
        ],
    );
    assert_eq!(opted_out.status.code(), Some(0));
    assert!(opted_out.stdout.is_empty());
    assert!(opted_out.stderr.is_empty());
}

#[test]
fn command_line_out_dir_excludes_stale_outputs_during_project_discovery() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmit":true}}"#,
    )
    .unwrap();
    std::fs::write(project.path().join("source.ts"), "export {};\n").unwrap();
    std::fs::create_dir(project.path().join("dist")).unwrap();
    std::fs::write(project.path().join("dist/old.ts"), "export {};\n").unwrap();
    let stats = project.path().join("stats.json");

    let output = run_tsz(
        project.path(),
        [
            "--project",
            ".",
            "--outDir",
            "dist",
            "--pretty",
            "false",
            "--perf-counters-json",
            stats.to_str().unwrap(),
        ],
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    let stats = read_json(&stats);
    assert_eq!(stats["stats"]["root_files"], 1);
    assert_eq!(stats["stats"]["source_files"], 1);
}

#[test]
fn project_exit_status_distinguishes_zero_root_config_errors() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("tsconfig.json"), r#"{"files":[]}"#).unwrap();
    let zero_roots = run_tsz(
        project.path(),
        ["--project", ".", "--noEmit", "--pretty", "false"],
    );
    assert_eq!(zero_roots.status.code(), Some(2));
    assert!(
        String::from_utf8(zero_roots.stdout)
            .unwrap()
            .contains("error TS18002")
    );

    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{"files":[],"include":["present.ts"]}"#,
    )
    .unwrap();
    std::fs::write(project.path().join("present.ts"), "export {};\n").unwrap();
    let with_root = run_tsz(
        project.path(),
        ["--project", ".", "--noEmit", "--pretty", "false"],
    );
    assert_eq!(with_root.status.code(), Some(1));
    assert!(
        String::from_utf8(with_root.stdout)
            .unwrap()
            .contains("error TS18002")
    );

    std::fs::remove_file(project.path().join("present.ts")).unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{"references":[{"path":"./missing"}]}"#,
    )
    .unwrap();
    let missing_reference = run_tsz(
        project.path(),
        ["--project", ".", "--noEmit", "--pretty", "false"],
    );
    assert_eq!(missing_reference.status.code(), Some(2));
    assert!(
        String::from_utf8(missing_reference.stdout)
            .unwrap()
            .contains("error TS6053")
    );

    let missing_project = run_tsz(
        project.path(),
        ["--project", "./absent", "--noEmit", "--pretty", "false"],
    );
    assert_eq!(missing_project.status.code(), Some(1));
}

#[test]
fn configured_invalid_roots_match_ts7_messages_chains_and_exit_codes() {
    let project = tempfile::tempdir().unwrap();
    let physical_root = std::fs::canonicalize(project.path()).unwrap();
    let javascript = physical_root.join("main.js");
    let text = physical_root.join("main.txt");
    let missing = physical_root.join("missing.ts");
    std::fs::write(&javascript, "const value = 1;\n").unwrap();
    std::fs::write(&text, "not valid TypeScript @@@\n").unwrap();
    let javascript = javascript.to_string_lossy().replace('\\', "/");
    let text = text.to_string_lossy().replace('\\', "/");
    let missing = missing.to_string_lossy().replace('\\', "/");

    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{"files":["main.js"]}"#,
    )
    .unwrap();
    let expected_javascript = format!(
        "error TS6504: File '{javascript}' is a JavaScript file. Did you mean to enable the 'allowJs' option?\n  The file is in the program because:\n    Part of 'files' list in tsconfig.json\n"
    );
    for arguments in [
        vec!["--project", ".", "--pretty", "false"],
        vec!["--project", ".", "--noEmit", "--pretty", "false"],
    ] {
        let output = run_tsz(project.path(), arguments);
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            expected_javascript
        );
        assert!(output.stderr.is_empty());
    }

    let allowed = run_tsz(
        project.path(),
        [
            "--project",
            ".",
            "--allowJs",
            "--noEmit",
            "--pretty",
            "false",
        ],
    );
    assert_eq!(allowed.status.code(), Some(0));
    assert!(allowed.stdout.is_empty());
    assert!(allowed.stderr.is_empty());

    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{"files":["main.txt"]}"#,
    )
    .unwrap();
    let unsupported = run_tsz(
        project.path(),
        ["--project", ".", "--noEmit", "--pretty", "false"],
    );
    assert_eq!(unsupported.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(unsupported.stdout).unwrap(),
        format!(
            "error TS6054: File '{text}' has an unsupported extension. The only supported extensions are '.ts', '.tsx', '.d.ts', '.cts', '.d.cts', '.mts', '.d.mts'.\n  The file is in the program because:\n    Part of 'files' list in tsconfig.json\n"
        )
    );

    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{"files":["missing.ts"]}"#,
    )
    .unwrap();
    let absent = run_tsz(
        project.path(),
        ["--project", ".", "--noEmit", "--pretty", "false"],
    );
    assert_eq!(absent.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(absent.stdout).unwrap(),
        format!(
            "error TS6053: File '{missing}' not found.\n  The file is in the program because:\n    Part of 'files' list in tsconfig.json\n"
        )
    );
}

#[test]
fn direct_invalid_roots_match_ts7_command_line_reason_and_exit_codes() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("main.js"), "const value = 1;\n").unwrap();
    std::fs::write(project.path().join("main.txt"), "not TypeScript @@@\n").unwrap();
    let physical_root = std::fs::canonicalize(project.path()).unwrap();
    let absolute_js = physical_root
        .join("main.js")
        .to_string_lossy()
        .replace('\\', "/");
    let absolute_txt = physical_root
        .join("main.txt")
        .to_string_lossy()
        .replace('\\', "/");

    let javascript = run_tsz(
        project.path(),
        ["--ignoreConfig", "--noEmit", "--pretty", "false", "main.js"],
    );
    assert_eq!(javascript.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(javascript.stdout).unwrap(),
        format!(
            "error TS6504: File '{absolute_js}' is a JavaScript file. Did you mean to enable the 'allowJs' option?\n  The file is in the program because:\n    Root file specified for compilation\nerror TS6504: File 'main.js' is a JavaScript file. Did you mean to enable the 'allowJs' option?\n  The file is in the program because:\n    Root file specified for compilation\n"
        )
    );

    let unsupported = run_tsz(
        project.path(),
        [
            "--ignoreConfig",
            "--noEmit",
            "--pretty",
            "false",
            "main.txt",
        ],
    );
    assert_eq!(unsupported.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(unsupported.stdout).unwrap(),
        format!(
            "error TS6054: File '{absolute_txt}' has an unsupported extension. The only supported extensions are '.ts', '.tsx', '.d.ts', '.cts', '.d.cts', '.mts', '.d.mts'.\n  The file is in the program because:\n    Root file specified for compilation\nerror TS6054: File 'main.txt' has an unsupported extension. The only supported extensions are '.ts', '.tsx', '.d.ts', '.cts', '.d.cts', '.mts', '.d.mts'.\n  The file is in the program because:\n    Root file specified for compilation\n"
        )
    );

    let missing = run_tsz(
        project.path(),
        [
            "--ignoreConfig",
            "--noEmit",
            "--pretty",
            "false",
            "missing.ts",
        ],
    );
    assert_eq!(missing.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(missing.stdout).unwrap(),
        "error TS6053: File 'missing.ts' not found.\n  The file is in the program because:\n    Root file specified for compilation\n"
    );

    std::fs::write(
        project.path().join("semantic.ts"),
        "const value: string = 1;\n",
    )
    .unwrap();
    let semantic = run_tsz(
        project.path(),
        [
            "--ignoreConfig",
            "--noEmit",
            "--pretty",
            "false",
            "semantic.ts",
        ],
    );
    assert_eq!(semantic.status.code(), Some(1));
    assert!(
        String::from_utf8(semantic.stdout)
            .unwrap()
            .contains("error TS2322")
    );
}

#[test]
fn project_reference_targets_require_json_configs_and_render_object_spans() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("without-config")).unwrap();
    std::fs::write(project.path().join("plain.txt"), "not a config\n").unwrap();
    std::fs::write(project.path().join("named.json"), r#"{"files":[]}"#).unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{
  "files": [],
  "references": [
    { "path": "./without-config" },
    { "path": "./plain.txt" },
    { "path": "./named.json" }
  ]
}"#,
    )
    .unwrap();
    let output = run_tsz(
        project.path(),
        ["--project", ".", "--noEmit", "--pretty", "false"],
    );
    assert_eq!(output.status.code(), Some(2));
    let physical_root = std::fs::canonicalize(project.path()).unwrap();
    let without_config = physical_root
        .join("without-config")
        .to_string_lossy()
        .replace('\\', "/");
    let plain = physical_root
        .join("plain.txt")
        .to_string_lossy()
        .replace('\\', "/");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!(
            "tsconfig.json(4,5): error TS6053: File '{without_config}' not found.\ntsconfig.json(5,5): error TS6053: File '{plain}' not found.\n"
        )
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn project_directory_and_upward_search_are_delegated_to_core() {
    let project = tempfile::tempdir().unwrap();
    let nested = project.path().join("nested");
    std::fs::create_dir(&nested).unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{
            "compilerOptions": { "noEmit": true },
            "files": ["case.ts"]
        }"#,
    )
    .unwrap();
    std::fs::write(project.path().join("case.ts"), "const value = 1;").unwrap();
    let stats_path = project.path().join("explicit-stats.json");

    let explicit = run_tsz(
        project.path(),
        [
            "--project",
            project.path().to_str().unwrap(),
            "--pretty",
            "false",
            "--perf-counters-json",
            stats_path.to_str().unwrap(),
        ],
    );
    assert_eq!(explicit.status.code(), Some(0));
    assert!(explicit.stdout.is_empty());
    assert!(explicit.stderr.is_empty());
    let stats = read_json(&stats_path);
    assert_eq!(stats["schema_version"], 2);
    assert_eq!(stats["stats"]["files"], 1);
    assert_eq!(stats["stats"]["root_files"], 1);
    assert_eq!(stats["stats"]["source_files"], 1);
    assert_eq!(
        stats["stats"]["root_file_paths"],
        stats["stats"]["source_file_paths"]
    );
    let source_paths = stats["stats"]["source_file_paths"].as_array().unwrap();
    assert_eq!(source_paths.len(), 1);
    assert!(source_paths[0].as_str().unwrap().ends_with("/case.ts"));
    assert_eq!(stats["stats"]["project_configs"], 1);
    assert_eq!(stats["stats"]["project_references"], 0);

    let searched = run_tsz(&nested, std::iter::empty::<&str>());
    assert_eq!(searched.status.code(), Some(0));
    assert!(searched.stdout.is_empty());
    assert!(searched.stderr.is_empty());
}

#[test]
fn project_option_cannot_be_mixed_with_explicit_files() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{ "compilerOptions": { "noEmit": true }, "files": ["case.ts"] }"#,
    )
    .unwrap();
    std::fs::write(project.path().join("case.ts"), "const value = 1;").unwrap();

    let output = run_tsz(
        project.path(),
        ["--project", ".", "--pretty", "false", "case.ts"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "error TS5042: Option 'project' cannot be mixed with source files on a command line.\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn explicit_files_require_ignore_config_when_a_project_is_discoverable() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{ "compilerOptions": { "noEmit": true }, "files": ["project.ts"] }"#,
    )
    .unwrap();
    std::fs::write(project.path().join("project.ts"), "const project = 1;").unwrap();
    std::fs::write(project.path().join("explicit.ts"), "const explicit = 1;").unwrap();

    let rejected = run_tsz(
        project.path(),
        ["--pretty", "false", "--noEmit", "explicit.ts"],
    );
    assert_eq!(rejected.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(rejected.stdout).unwrap(),
        concat!(
            "error TS5112: tsconfig.json is present but will not be loaded if files are ",
            "specified on commandline. Use '--ignoreConfig' to skip this error.\n"
        )
    );
    assert!(rejected.stderr.is_empty());

    let ignored = run_tsz(
        project.path(),
        [
            "--pretty",
            "false",
            "--noEmit",
            "--ignoreConfig",
            "explicit.ts",
        ],
    );
    assert_eq!(ignored.status.code(), Some(0));
    assert!(ignored.stdout.is_empty());
    assert!(ignored.stderr.is_empty());
}

#[test]
fn empty_invocation_without_a_discoverable_project_prints_help_and_fails() {
    let directory = tempfile::tempdir().unwrap();

    for arguments in [Vec::new(), vec!["--ignoreConfig", "--pretty", "false"]] {
        let output = run_tsz(directory.path(), arguments);

        assert_eq!(output.status.code(), Some(1));
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.starts_with("tsz "), "{stdout}");
        assert!(
            stdout.contains("Usage: tsz [options] [file ...]"),
            "{stdout}"
        );
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn project_emit_does_not_flatten_same_named_nested_sources() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join("src/one")).unwrap();
    std::fs::create_dir_all(project.path().join("src/two")).unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{
            "compilerOptions": {
                "rootDir": "src",
                "outDir": "dist",
                "declaration": true,
                "declarationDir": "types"
            },
            "files": ["src/one/index.ts", "src/two/index.ts"]
        }"#,
    )
    .unwrap();
    std::fs::write(
        project.path().join("src/one/index.ts"),
        "export const one: number = 1;\n",
    )
    .unwrap();
    std::fs::write(
        project.path().join("src/two/index.ts"),
        "export const two: number = 2;\n",
    )
    .unwrap();

    let output = run_tsz(project.path(), ["--project", ".", "--pretty", "false"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert_eq!(
        std::fs::read_to_string(project.path().join("dist/one/index.js")).unwrap(),
        "export const one = 1;\n"
    );
    assert_eq!(
        std::fs::read_to_string(project.path().join("dist/two/index.js")).unwrap(),
        "export const two = 2;\n"
    );
    assert!(project.path().join("types/one/index.d.ts").is_file());
    assert!(project.path().join("types/two/index.d.ts").is_file());
    assert!(!project.path().join("dist/index.js").exists());
    assert!(!project.path().join("types/index.d.ts").exists());
}

#[test]
fn command_line_emit_maps_nested_sources_from_their_common_directory() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join("src/one")).unwrap();
    std::fs::create_dir_all(project.path().join("src/two")).unwrap();
    std::fs::write(
        project.path().join("src/one/index.ts"),
        "export const one: number = 1;\n",
    )
    .unwrap();
    std::fs::write(
        project.path().join("src/two/index.ts"),
        "export const two: number = 2;\n",
    )
    .unwrap();

    let output = run_tsz(
        project.path(),
        [
            "--ignoreConfig",
            "--outDir",
            "dist",
            "--declaration",
            "--declarationDir",
            "types",
            "--pretty",
            "false",
            "src/one/index.ts",
            "src/two/index.ts",
        ],
    );

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert!(project.path().join("dist/one/index.js").is_file());
    assert!(project.path().join("dist/two/index.js").is_file());
    assert!(project.path().join("types/one/index.d.ts").is_file());
    assert!(project.path().join("types/two/index.d.ts").is_file());
    assert!(!project.path().join("dist/index.js").exists());
}

#[test]
fn command_line_root_dir_controls_project_javascript_and_declaration_layout() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join("src/one")).unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{ "files": ["src/one/index.ts"] }"#,
    )
    .unwrap();
    std::fs::write(
        project.path().join("src/one/index.ts"),
        "export const value: number = 1;\n",
    )
    .unwrap();

    let output = run_tsz(
        project.path(),
        [
            "--project",
            ".",
            "--rootDir",
            "src",
            "--outDir",
            "dist",
            "--declaration",
            "--declarationDir",
            "types",
            "--pretty",
            "false",
        ],
    );

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert!(project.path().join("dist/one/index.js").is_file());
    assert!(project.path().join("types/one/index.d.ts").is_file());
    assert!(!project.path().join("dist/src/one/index.js").exists());
}

#[test]
fn inferred_project_emit_root_matches_ts7_diagnostic_and_exit_behavior() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join("src")).unwrap();
    std::fs::write(
        project.path().join("src/value.ts"),
        "export const value = 1;\n",
    )
    .unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{
            "compilerOptions": { "outDir": "dist" },
            "files": ["src/value.ts"]
        }"#,
    )
    .unwrap();

    let emitted = run_tsz(project.path(), ["--project", ".", "--pretty", "false"]);
    assert_eq!(emitted.status.code(), Some(2));
    let stdout = String::from_utf8(emitted.stdout).unwrap();
    assert!(stdout.contains("error TS5011:"), "{stdout}");
    assert!(
        stdout.contains(
            "The common source directory of 'tsconfig.json' is './src'. The 'rootDir' setting must be explicitly set to this or another path to adjust your output's file layout.\n  Visit https://aka.ms/ts6 for migration information."
        ),
        "{stdout}"
    );
    assert!(project.path().join("dist/src/value.js").is_file());

    std::fs::remove_dir_all(project.path().join("dist")).unwrap();
    let no_emit = run_tsz(
        project.path(),
        ["--project", ".", "--noEmit", "--pretty", "false"],
    );
    assert_eq!(no_emit.status.code(), Some(0));
    assert!(no_emit.stdout.is_empty());
    assert!(!project.path().join("dist").exists());

    let no_emit_on_error = run_tsz(
        project.path(),
        ["--project", ".", "--noEmitOnError", "--pretty", "false"],
    );
    assert_eq!(no_emit_on_error.status.code(), Some(1));
    assert!(
        String::from_utf8(no_emit_on_error.stdout)
            .unwrap()
            .contains("error TS5011:")
    );
    assert!(!project.path().join("dist").exists());
}

#[test]
fn output_collisions_skip_only_unsafe_products_and_return_exit_one() {
    let overwrite = tempfile::tempdir().unwrap();
    std::fs::write(
        overwrite.path().join("tsconfig.json"),
        r#"{
            "compilerOptions": { "allowJs": true, "declaration": true },
            "files": ["input.js"]
        }"#,
    )
    .unwrap();
    std::fs::write(overwrite.path().join("input.js"), "const value = 1;\n").unwrap();

    let overwrite_output = run_tsz(overwrite.path(), ["--project", ".", "--pretty", "false"]);
    assert_eq!(overwrite_output.status.code(), Some(1));
    let overwrite_stdout = String::from_utf8(overwrite_output.stdout).unwrap();
    assert!(
        overwrite_stdout.contains("error TS5055: Cannot write file '"),
        "{overwrite_stdout}"
    );
    assert!(
        overwrite_stdout.contains("input.js' because it would overwrite input file."),
        "{overwrite_stdout}"
    );
    assert!(overwrite.path().join("input.d.ts").is_file());

    let duplicate = tempfile::tempdir().unwrap();
    std::fs::write(
        duplicate.path().join("tsconfig.json"),
        r#"{
            "compilerOptions": {
                "declaration": true,
                "jsx": "react"
            },
            "files": ["same.ts", "same.tsx", "other.ts"]
        }"#,
    )
    .unwrap();
    std::fs::write(
        duplicate.path().join("same.ts"),
        "export const fromTs = 1;\n",
    )
    .unwrap();
    std::fs::write(
        duplicate.path().join("same.tsx"),
        "export const fromTsx = 2;\n",
    )
    .unwrap();
    std::fs::write(
        duplicate.path().join("other.ts"),
        "export const other = 3;\n",
    )
    .unwrap();

    let duplicate_output = run_tsz(duplicate.path(), ["--project", ".", "--pretty", "false"]);
    assert_eq!(duplicate_output.status.code(), Some(1));
    let duplicate_stdout = String::from_utf8(duplicate_output.stdout).unwrap();
    assert_eq!(duplicate_stdout.matches("error TS5056:").count(), 2);
    for suffix in ["same.js", "same.d.ts"] {
        assert!(!duplicate.path().join(suffix).exists(), "wrote {suffix}");
    }
    assert!(duplicate.path().join("other.js").is_file());
    assert!(duplicate.path().join("other.d.ts").is_file());

    for files in [
        r#"["other.ts", "same.tsx", "same.ts"]"#,
        r#"["same.ts", "same.tsx", "other.ts"]"#,
    ] {
        std::fs::write(
            duplicate.path().join("tsconfig.json"),
            format!(
                r#"{{
                    "compilerOptions": {{ "declaration": true, "jsx": "react" }},
                    "files": {files}
                }}"#
            ),
        )
        .unwrap();
        let repeated = run_tsz(duplicate.path(), ["--project", ".", "--pretty", "false"]);
        assert_eq!(repeated.status.code(), Some(1));
        assert_eq!(
            String::from_utf8(repeated.stdout)
                .unwrap()
                .matches("error TS5056:")
                .count(),
            2
        );
        assert!(duplicate.path().join("other.js").is_file());
        assert!(duplicate.path().join("other.d.ts").is_file());
    }
}

#[test]
fn explicit_root_dir_matches_ts7_chain_partial_emit_and_exit_status() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join("src")).unwrap();
    std::fs::create_dir_all(project.path().join("outside")).unwrap();
    std::fs::write(project.path().join("src/a.ts"), "export const a = 1;\n").unwrap();
    std::fs::write(project.path().join("outside/b.ts"), "export const b = 2;\n").unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": { "rootDir": "src", "outDir": "dist" },
  "files": ["src/a.ts", "outside/b.ts"]
}
"#,
    )
    .unwrap();

    let output = run_tsz(project.path(), ["--project", ".", "--pretty", "false"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());
    let physical_project = std::fs::canonicalize(project.path()).unwrap();
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!(
            "error TS6059: File '{}' is not under 'rootDir' '{}'. 'rootDir' is expected to contain all source files.\n  The file is in the program because:\n    Part of 'files' list in tsconfig.json\n",
            physical_project.join("outside/b.ts").to_string_lossy(),
            physical_project.join("src").to_string_lossy(),
        )
    );
    assert!(project.path().join("dist/a.js").is_file());
    assert!(project.path().join("outside/b.js").is_file());

    std::fs::remove_file(project.path().join("dist/a.js")).unwrap();
    std::fs::remove_file(project.path().join("outside/b.js")).unwrap();
    let no_emit = run_tsz(
        project.path(),
        ["--project", ".", "--noEmit", "--pretty", "false"],
    );
    assert_eq!(no_emit.status.code(), Some(1));
    assert!(
        String::from_utf8(no_emit.stdout)
            .unwrap()
            .contains("error TS6059:")
    );
    assert!(!project.path().join("dist/a.js").exists());
    assert!(!project.path().join("outside/b.js").exists());
}

#[test]
fn config_option_diagnostics_preserve_owner_spans_across_extends_and_cli_overrides() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join("src")).unwrap();
    std::fs::write(
        project.path().join("src/value.ts"),
        "export const value = 1;\n",
    )
    .unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        concat!(
            "{\n",
            "  \"compilerOptions\": {\n",
            "    \"outDir\": \"dist\",\n",
            "    \"target\": \"wat\"\n",
            "  },\n",
            "  \"files\": [\"src/value.ts\"]\n",
            "}\n",
        ),
    )
    .unwrap();

    let owned = run_tsz(project.path(), ["--project", ".", "--pretty", "false"]);
    assert_eq!(owned.status.code(), Some(2));
    let owned_stdout = String::from_utf8(owned.stdout).unwrap();
    assert!(
        owned_stdout.starts_with("tsconfig.json(3,5): error TS5011:"),
        "{owned_stdout}"
    );
    assert!(
        owned_stdout.contains("tsconfig.json(4,15): error TS6046:"),
        "{owned_stdout}"
    );

    std::fs::write(
        project.path().join("base.json"),
        concat!(
            "{\n",
            "  \"compilerOptions\": {\n",
            "    \"outDir\": \"dist\",\n",
            "    \"target\": \"wat\"\n",
            "  }\n",
            "}\n",
        ),
    )
    .unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{ "extends": "./base.json", "files": ["src/value.ts"] }"#,
    )
    .unwrap();

    let inherited = run_tsz(project.path(), ["--project", ".", "--pretty", "false"]);
    assert_eq!(inherited.status.code(), Some(2));
    let inherited_stdout = String::from_utf8(inherited.stdout).unwrap();
    assert!(
        inherited_stdout.starts_with("error TS5011:"),
        "{inherited_stdout}"
    );
    assert!(
        inherited_stdout.contains("base.json(4,15): error TS6046:"),
        "{inherited_stdout}"
    );

    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{ "compilerOptions": { "target": "es2025" }, "files": ["src/value.ts"] }"#,
    )
    .unwrap();
    let overridden = run_tsz(
        project.path(),
        ["--project", ".", "--target", "wat", "--pretty", "false"],
    );
    assert_eq!(overridden.status.code(), Some(1));
    assert!(
        String::from_utf8(overridden.stdout)
            .unwrap()
            .starts_with("error TS6046:")
    );
}

#[test]
fn config_owned_invalid_target_emits_and_cli_invalid_target_skips() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        "{\"compilerOptions\":{\"target\":\"wat\"},\"files\":[\"a.ts\"]}\n",
    )
    .unwrap();
    std::fs::write(project.path().join("a.ts"), "export const a = 1;\n").unwrap();

    let configured = run_tsz(project.path(), ["--project", ".", "--pretty", "false"]);
    assert_eq!(configured.status.code(), Some(2));
    assert!(
        String::from_utf8(configured.stdout)
            .unwrap()
            .starts_with("tsconfig.json(1,30): error TS6046:")
    );
    assert!(project.path().join("a.js").is_file());

    std::fs::remove_file(project.path().join("a.js")).unwrap();
    let command_line = run_tsz(
        project.path(),
        ["--project", ".", "--target", "wat", "--pretty", "false"],
    );
    assert_eq!(command_line.status.code(), Some(1));
    assert!(
        String::from_utf8(command_line.stdout)
            .unwrap()
            .starts_with("error TS6046:")
    );
    assert!(!project.path().join("a.js").exists());
}

fn run_tsz<'a>(cwd: &Path, arguments: impl IntoIterator<Item = &'a str>) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tsz"))
        .current_dir(cwd)
        .args(arguments)
        .output()
        .unwrap()
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
