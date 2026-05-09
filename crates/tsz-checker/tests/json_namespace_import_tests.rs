use tsz_checker::context::CheckerOptions;
use tsz_common::common::{ModuleKind, ScriptTarget};

fn diagnostics_for_json_namespace_import(
    json_source: &str,
    user_file_name: &str,
    user_source: &str,
) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_multi_file(
        &[("config.json", json_source), (user_file_name, user_source)],
        user_file_name,
        CheckerOptions {
            no_lib: true,
            target: ScriptTarget::ES2022,
            module: ModuleKind::Node18,
            resolve_json_module: true,
            ..Default::default()
        },
    )
    .into_iter()
    .filter(|d| d.code != 2318)
    .map(|d| (d.code, d.message_text))
    .collect()
}

#[test]
fn cts_json_namespace_import_exposes_json_object_directly() {
    let diagnostics = diagnostics_for_json_namespace_import(
        r#"{ "version": 1 }"#,
        "main.cts",
        r#"
import * as config from "./config.json";
config.version;
config.default;
"#,
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        1,
        "Expected one TS2339 for config.default, got: {diagnostics:#?}"
    );
    assert!(
        ts2339[0]
            .1
            .contains("Property 'default' does not exist on type '{ version: number; }'."),
        "Expected the CJS JSON namespace to be the JSON object shape, got: {diagnostics:#?}"
    );
}

#[test]
fn mts_json_namespace_import_exposes_default_only() {
    let diagnostics = diagnostics_for_json_namespace_import(
        r#"{ "version": 1 }"#,
        "main.mts",
        r#"
import * as config from "./config.json" with { type: "json" };
config.version;
config.default;
"#,
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        1,
        "Expected one TS2339 for config.version, got: {diagnostics:#?}"
    );
    assert!(
        ts2339[0].1.contains(
            "Property 'version' does not exist on type '{ default: { version: number; }; }'."
        ),
        "Expected the ESM JSON namespace to expose only default, got: {diagnostics:#?}"
    );
}

#[test]
fn cts_json_namespace_default_property_points_at_json_object() {
    let diagnostics = diagnostics_for_json_namespace_import(
        r#"{ "name": "pkg", "default": "misedirection" }"#,
        "main.cts",
        r#"
import * as config from "./config.json";
config.name;
config.default.name;
config.default.default;
"#,
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected the CJS JSON namespace `default` property to expose the JSON object, got: {diagnostics:#?}"
    );
}
