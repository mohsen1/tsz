//! `extends: ""` diagnosis (TS18051).
//!
//! Split out of `module_resolution.rs` to keep it under the 2000-line limit
//! (§19; ratchet tracked by #8280) rather than growing an already-large file.

use super::super::*;
use tempfile::tempdir;

#[test]
fn extends_empty_string_reports_ts18051_not_ts6053() {
    // `tsc`'s `getExtendsConfigPathOrArray` (`commandLineParser.ts`) checks
    // `extendedConfig === ""` only after every resolution strategy (relative,
    // absolute, package/node_modules) has already failed on the empty
    // specifier, and reports TS18051 there instead of the generic TS6053
    // "File '' not found." every other unresolved specifier gets.
    let temp = tempdir().expect("create temp dir");
    let project = temp.path().join("project");
    std::fs::create_dir_all(&project).expect("create project dir");
    let child_path = project.join("tsconfig.json");
    let child_source = r#"{ "extends": "", "compilerOptions": { "strict": true } }"#;
    std::fs::write(&child_path, child_source).expect("write child");

    let parsed = load_tsconfig_with_diagnostics(&child_path).expect("load must succeed");

    assert!(
        !parsed.diagnostics.iter().any(|d| d.code == 6053),
        "an empty extends must not report the generic TS6053, got: {:?}",
        parsed.diagnostics
    );
    let ts18051: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 18051)
        .collect();
    assert_eq!(
        ts18051.len(),
        1,
        "exactly one TS18051 for the empty extends: {:?}",
        parsed.diagnostics
    );
    assert!(
        ts18051[0].message_text.contains("'extends'"),
        "TS18051 names the 'extends' option: {}",
        ts18051[0].message_text
    );
    let expected_start = child_source.find("\"\"").expect("empty literal present") as u32;
    assert_eq!(
        ts18051[0].start, expected_start,
        "TS18051 anchors at the empty extends literal"
    );

    let opts = parsed
        .config
        .compiler_options
        .expect("local options retained");
    assert_eq!(
        opts.strict,
        Some(true),
        "local options survive an empty extends"
    );
}

#[test]
fn extends_array_with_one_empty_entry_reports_ts18051_for_that_entry_only() {
    // An empty entry inside an array `extends` gets its own TS18051; sibling
    // entries resolve/report independently (mirrors the unresolved-entry
    // array coverage in `module_resolution.rs`).
    let temp = tempdir().expect("create temp dir");
    let project = temp.path().join("project");
    std::fs::create_dir_all(&project).expect("create project dir");
    std::fs::write(
        project.join("present.json"),
        r#"{ "compilerOptions": { "target": "ES2021" } }"#,
    )
    .expect("write present base");
    let child_path = project.join("tsconfig.json");
    std::fs::write(&child_path, r#"{ "extends": ["./present.json", ""] }"#).expect("write child");

    let parsed = load_tsconfig_with_diagnostics(&child_path).expect("load must succeed");
    assert!(
        !parsed.diagnostics.iter().any(|d| d.code == 6053),
        "the empty array entry must not report TS6053, got: {:?}",
        parsed.diagnostics
    );
    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .filter(|d| d.code == 18051)
            .count(),
        1,
        "one TS18051 for the empty array entry: {:?}",
        parsed.diagnostics
    );
    let opts = parsed.config.compiler_options.expect("present base merged");
    assert_eq!(
        opts.target.as_deref(),
        Some("ES2021"),
        "the resolvable array entry is still applied alongside the empty one"
    );
}
