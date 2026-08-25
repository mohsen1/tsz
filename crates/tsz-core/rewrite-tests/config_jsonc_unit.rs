use std::path::Path;

use tempfile::TempDir;

use super::{
    ProjectRequest, ProjectSelection, compiler_option_spans, parse_jsonc, partial_options,
    reference_object_spans, resolve_project,
};
use crate::host::SystemHost;
use crate::program::Compiler;

#[test]
fn one_jsonc_scan_preserves_original_option_and_reference_byte_spans() {
    let source = concat!(
        "\u{feff}{\n",
        "  // comments remain part of the original byte coordinate space\n",
        "  \"compilerOptions\" /* owner */ : {\n",
        "    \"target\" /* key gap */ : \"wat\", /* trailing */\n",
        "    \"module\": \"co//mmonjs\",\n",
        "  },\n",
        "  \"references\" : [ /* open */\n",
        "    { \"path\": \"./dependency\" }, /* trailing */\n",
        "  ],\n",
        "}\n",
    );
    let document = parse_jsonc(source).expect("valid JSONC");
    assert_eq!(document.value["compilerOptions"]["target"], "wat");
    assert_eq!(document.value["compilerOptions"]["module"], "co//mmonjs");

    let option = compiler_option_spans(&document.tokens)["target"];
    assert_eq!(option.key_start, source.find("\"target\"").unwrap() as u32);
    assert_eq!(option.key_length, "\"target\"".len() as u32);
    assert_eq!(
        option.value_start,
        Some(source.find("\"wat\"").unwrap() as u32)
    );
    assert_eq!(option.value_length, Some("\"wat\"".len() as u32));

    let object_start = source.find("{ \"path\"").unwrap();
    let object_end = source[object_start..].find('}').unwrap() + object_start + 1;
    assert_eq!(
        reference_object_spans(&document.tokens),
        [Some((
            object_start as u32,
            (object_end - object_start) as u32
        ))]
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
