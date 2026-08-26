use std::path::Path;

use tempfile::TempDir;

use super::{ProjectRequest, ProjectSelection, parse_jsonc, partial_options, resolve_project};
use crate::host::SystemHost;
use crate::program::{CompileExitStatus, Compiler, SemanticCompletion};

#[test]
fn one_jsonc_scan_preserves_original_option_and_reference_byte_spans() {
    let source = concat!(
        "\u{feff}{\n",
        "  // comments remain part of the original byte coordinate space\n",
        "  \"references\" : [ /* open */\n",
        "    { \"path\": \"./dependency\" }, /* trailing */\n",
        "  ],\n",
        "  \"compilerOptions\" /* owner */ : {\n",
        "    \"tar\\u0067et\" /* key gap */ : \"wat\", /* trailing */\n",
        "    \"module\": \"co//mmonjs\",\n",
        "  },\n",
        "}\n",
    );
    let document = parse_jsonc(source).expect("valid JSONC");
    assert_eq!(document.value["compilerOptions"]["target"], "wat");
    assert_eq!(document.value["compilerOptions"]["module"], "co//mmonjs");

    let option = document.source_spans.compiler_options["target"];
    assert_eq!(
        option.key_start,
        source.find("\"tar\\u0067et\"").unwrap() as u32
    );
    assert_eq!(option.key_length, "\"tar\\u0067et\"".len() as u32);
    assert_eq!(
        option.value_start,
        Some(source.find("\"wat\"").unwrap() as u32)
    );
    assert_eq!(option.value_length, Some("\"wat\"".len() as u32));

    let object_start = source.find("{ \"path\"").unwrap();
    let object_end = source[object_start..].find('}').unwrap() + object_start + 1;
    assert_eq!(
        document.source_spans.references,
        [(object_start as u32, (object_end - object_start) as u32)]
    );
}

#[test]
fn project_path_normalization_keeps_legacy_leading_parent_behavior() {
    assert_eq!(
        super::normalize_path(Path::new("../entry.ts")),
        Path::new("../entry.ts")
    );
    assert_eq!(
        super::normalize_path(Path::new("../../entry.ts")),
        Path::new("entry.ts")
    );
    assert_eq!(
        super::normalize_path(Path::new("../../../entry.ts")),
        Path::new("../entry.ts")
    );
}

#[test]
fn check_js_schema_keeps_false_distinct_from_absence() {
    let absent = parse_jsonc(r#"{"compilerOptions":{"allowJs":true}}"#).unwrap();
    let explicit = parse_jsonc(r#"{"compilerOptions":{"checkJs":false}}"#).unwrap();
    let absent = partial_options(absent.value.as_object().unwrap(), Path::new("."));
    let explicit = partial_options(explicit.value.as_object().unwrap(), Path::new("."));
    assert_eq!((absent.allow_js, absent.check_js), (Some(true), None));
    assert_eq!((explicit.allow_js, explicit.check_js), (None, Some(false)));
}

#[test]
fn use_define_for_class_fields_schema_keeps_false_distinct_from_absence_and_true() {
    let absent = parse_jsonc(r#"{"compilerOptions":{"target":"es2022"}}"#).unwrap();
    let explicit_false =
        parse_jsonc(r#"{"compilerOptions":{"useDefineForClassFields":false}}"#).unwrap();
    let explicit_true =
        parse_jsonc(r#"{"compilerOptions":{"useDefineForClassFields":true}}"#).unwrap();
    let absent = partial_options(absent.value.as_object().unwrap(), Path::new("."));
    let explicit_false = partial_options(explicit_false.value.as_object().unwrap(), Path::new("."));
    let explicit_true = partial_options(explicit_true.value.as_object().unwrap(), Path::new("."));
    assert_eq!(absent.use_define_for_class_fields, None);
    assert_eq!(explicit_false.use_define_for_class_fields, Some(false));
    assert_eq!(explicit_true.use_define_for_class_fields, Some(true));
}

#[test]
fn resolved_false_class_field_semantics_defer_only_the_affected_javascript_product() {
    let fixture = TempDir::new().unwrap();
    std::fs::write(
        fixture.path().join("tsconfig.json"),
        r#"{
            "compilerOptions": {
                "declaration": true,
                "module": "esnext",
                "target": "es2022",
                "useDefineForClassFields": false,
                "noCheck": true,
                "rootDir": ".",
                "outDir": "dist"
            },
            "files": ["class-field.ts", "stable.ts"]
        }"#,
    )
    .unwrap();
    std::fs::write(
        fixture.path().join("class-field.ts"),
        "export class DeferredField { value: number = 1; }\n",
    )
    .unwrap();
    std::fs::write(
        fixture.path().join("stable.ts"),
        "export const stable: number = 1;\n",
    )
    .unwrap();

    let host = SystemHost::new(fixture.path());
    let resolved = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(fixture.path().into())),
    );
    assert_eq!(resolved.options.use_define_for_class_fields, Some(false));
    let options = resolved.options.clone();
    let output = Compiler::new().compile_resolved(resolved, &options);

    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    let mut emitted_paths = output
        .emitted_files
        .iter()
        .map(|file| {
            file.path
                .strip_prefix(fixture.path())
                .expect("project output belongs to fixture")
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect::<Vec<_>>();
    emitted_paths.sort();
    assert_eq!(
        emitted_paths,
        [
            "dist/class-field.d.ts",
            "dist/stable.d.ts",
            "dist/stable.js"
        ],
    );
}

#[test]
fn check_js_implies_discovery_but_explicit_allow_js_false_reports_ts5052() {
    let fixture = TempDir::new().unwrap();
    std::fs::write(
        fixture.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"checkJs":true,"noEmit":true}}"#,
    )
    .unwrap();
    std::fs::write(fixture.path().join("included.js"), "MissingName;\n").unwrap();
    let host = SystemHost::new(fixture.path());
    let resolved = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(fixture.path().into())),
    );
    assert_eq!(
        (resolved.options.allow_js, resolved.options.check_js),
        (true, Some(true))
    );
    assert_eq!(resolved.root_files.len(), 1);
    assert!(resolved.root_files[0].ends_with("included.js"));

    let invalid = fixture.path().join("invalid");
    std::fs::create_dir(&invalid).unwrap();
    let config = r#"{"compilerOptions":{"allowJs":false,"checkJs":true,"noEmit":true},"files":["entry.ts"]}"#;
    std::fs::write(invalid.join("tsconfig.json"), config).unwrap();
    std::fs::write(invalid.join("entry.ts"), "const stable = 1;\n").unwrap();
    let host = SystemHost::new(&invalid);
    let resolved = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(invalid)),
    );
    assert_eq!(
        (resolved.options.allow_js, resolved.options.check_js),
        (false, Some(true))
    );
    let options = resolved.options.clone();
    let output = Compiler::new().compile_resolved(resolved, &options);
    assert_eq!(output.diagnostics.len(), 1);
    let diagnostic = &output.diagnostics[0];
    assert_eq!(
        (diagnostic.code, diagnostic.file.as_str()),
        (5052, "tsconfig.json")
    );
    assert_eq!(diagnostic.start, config.find("\"checkJs\"").unwrap() as u32);
    assert_eq!(
        diagnostic.message_text,
        "Option 'checkJs' cannot be specified without specifying option 'allowJs'."
    );
}
