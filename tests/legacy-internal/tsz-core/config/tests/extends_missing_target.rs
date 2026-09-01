//! Missing-`extends`-target diagnosis: TS5083 vs TS6053 (#17079).
//!
//! `tsc`'s `getExtendsConfigPath` (`commandLineParser.ts`) splits a failed
//! `extends` resolution two ways, and the code it reports is *not* uniform:
//!
//! - A **rooted/relative** specifier that already carries a `.json` extension is
//!   returned from `getExtendsConfigPath` unchecked (the `.json`-append fallback
//!   only fires for extensionless specifiers). The ensuing file read fails and
//!   surfaces a file-less **TS5083** `Cannot read file '{0}'.` whose `{0}` is the
//!   lexically normalized absolute path.
//! - Every other miss — an extensionless / non-`.json` relative specifier whose
//!   `.json`-appended candidate is absent, or a bare/package specifier that
//!   fails Node module resolution — is the specifier-anchored **TS6053**
//!   `File '{0}' not found.`
//!
//! Oracle-verified against pinned `typescript@7.0.2` with
//! `--noEmit --strict --pretty false --lib es2022 --target es2022
//! --singleThreaded --stableTypeOrdering`.

use super::super::*;
use tempfile::tempdir;

/// Build a project whose root `tsconfig.json` extends `spec`, load it with
/// diagnostics, and return the parsed result. A single local option (`strict`)
/// is set so callers can assert it survives the recoverable `extends` failure.
fn load_extending(spec: &str) -> (tempfile::TempDir, ParsedTsConfig) {
    let temp = tempdir().expect("create temp dir");
    let project = temp.path().join("project");
    std::fs::create_dir_all(&project).expect("create project dir");
    let child_path = project.join("tsconfig.json");
    let source = format!(r#"{{ "extends": "{spec}", "compilerOptions": {{ "strict": true }} }}"#);
    std::fs::write(&child_path, source).expect("write child");
    let parsed = load_tsconfig_with_diagnostics(&child_path).expect("load must succeed");
    (temp, parsed)
}

#[test]
fn missing_relative_json_reports_ts5083_not_ts6053() {
    let (temp, parsed) = load_extending("./nope.json");

    assert!(
        !parsed.diagnostics.iter().any(|d| d.code == 6053),
        "a missing relative .json extends must not report the generic TS6053, got: {:?}",
        parsed.diagnostics
    );
    let ts5083: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 5083)
        .collect();
    assert_eq!(
        ts5083.len(),
        1,
        "exactly one TS5083 for the unreadable relative .json: {:?}",
        parsed.diagnostics
    );

    // {0} is the lexically normalized absolute path of the resolved target.
    let expected_path = temp
        .path()
        .join("project")
        .join("nope.json")
        .to_string_lossy()
        .into_owned();
    assert!(
        ts5083[0].message_text.contains(&expected_path),
        "TS5083 names the resolved target path {expected_path}: {}",
        ts5083[0].message_text
    );

    // TS5083 comes from the file-read layer, so it is a file-less compiler
    // diagnostic anchored at the whole compilation, not at the specifier span.
    assert_eq!(
        ts5083[0].file, "",
        "TS5083 is a file-less diagnostic (no specifier anchor)"
    );
    assert_eq!(ts5083[0].start, 0, "TS5083 carries no source position");

    let opts = parsed
        .config
        .compiler_options
        .expect("local options retained");
    assert_eq!(
        opts.strict,
        Some(true),
        "local options survive an unreadable extends"
    );
}

#[test]
fn missing_relative_json_normalizes_parent_segments_in_message() {
    let temp = tempdir().expect("create temp dir");
    let nested = temp.path().join("project").join("nested");
    std::fs::create_dir_all(&nested).expect("create nested dir");
    let child_path = nested.join("tsconfig.json");
    std::fs::write(&child_path, r#"{ "extends": "../nope.json" }"#).expect("write child");

    let parsed = load_tsconfig_with_diagnostics(&child_path).expect("load must succeed");
    let ts5083: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 5083)
        .collect();
    assert_eq!(ts5083.len(), 1, "one TS5083: {:?}", parsed.diagnostics);

    // `../nope.json` collapses to the sibling directory; the message must not
    // carry a `/../` spelling.
    let expected_path = temp
        .path()
        .join("project")
        .join("nope.json")
        .to_string_lossy()
        .into_owned();
    assert!(
        ts5083[0].message_text.contains(&expected_path) && !ts5083[0].message_text.contains("/../"),
        "TS5083 path is lexically normalized ({expected_path}): {}",
        ts5083[0].message_text
    );
}

#[test]
fn missing_relative_extensionless_reports_ts6053_not_ts5083() {
    let (_temp, parsed) = load_extending("./nope");

    assert!(
        !parsed.diagnostics.iter().any(|d| d.code == 5083),
        "an extensionless relative miss must not report TS5083, got: {:?}",
        parsed.diagnostics
    );
    let ts6053: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 6053)
        .collect();
    assert_eq!(
        ts6053.len(),
        1,
        "exactly one TS6053 for the extensionless miss: {:?}",
        parsed.diagnostics
    );
    assert!(
        ts6053[0].message_text.contains("./nope"),
        "TS6053 names the specifier as written: {}",
        ts6053[0].message_text
    );
    assert!(
        !ts6053[0].file.is_empty(),
        "TS6053 is anchored in the config file: {:?}",
        ts6053[0]
    );
}

#[test]
fn missing_relative_non_json_extension_reports_ts6053() {
    // A non-`.json` extension is treated like an extensionless specifier
    // (`tsc` appends `.json` and re-probes), so a miss stays TS6053.
    let (_temp, parsed) = load_extending("./nope.txt");

    assert!(
        !parsed.diagnostics.iter().any(|d| d.code == 5083),
        "a non-.json relative miss must not report TS5083, got: {:?}",
        parsed.diagnostics
    );
    let ts6053: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 6053)
        .collect();
    assert_eq!(
        ts6053.len(),
        1,
        "exactly one TS6053 for the non-.json miss: {:?}",
        parsed.diagnostics
    );
    assert!(
        ts6053[0].message_text.contains("./nope.txt"),
        "TS6053 names the specifier as written: {}",
        ts6053[0].message_text
    );
}

#[test]
fn missing_absolute_json_reports_ts5083() {
    let temp = tempdir().expect("create temp dir");
    let project = temp.path().join("project");
    std::fs::create_dir_all(&project).expect("create project dir");
    let child_path = project.join("tsconfig.json");
    let abs_missing = temp.path().join("does").join("not").join("exist.json");
    let source = format!(
        r#"{{ "extends": "{}" }}"#,
        abs_missing.to_string_lossy().replace('\\', "\\\\")
    );
    std::fs::write(&child_path, source).expect("write child");

    let parsed = load_tsconfig_with_diagnostics(&child_path).expect("load must succeed");
    let ts5083: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 5083)
        .collect();
    assert_eq!(
        ts5083.len(),
        1,
        "a rooted (absolute) .json miss shares the TS5083 rule: {:?}",
        parsed.diagnostics
    );
    assert!(
        ts5083[0]
            .message_text
            .contains(&abs_missing.to_string_lossy().into_owned()),
        "TS5083 names the absolute target path: {}",
        ts5083[0].message_text
    );
}

#[test]
fn unresolvable_package_keeps_ts6053() {
    // A bare/package specifier that fails Node module resolution must keep
    // TS6053 — TS5083 must not be widened to cover it.
    let (_temp, parsed) = load_extending("no-such-pkg/tsconfig.json");

    assert!(
        !parsed.diagnostics.iter().any(|d| d.code == 5083),
        "an unresolvable package must not report TS5083, got: {:?}",
        parsed.diagnostics
    );
    let ts6053: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 6053)
        .collect();
    assert_eq!(
        ts6053.len(),
        1,
        "exactly one TS6053 for the unresolvable package: {:?}",
        parsed.diagnostics
    );
    assert!(
        ts6053[0].message_text.contains("no-such-pkg/tsconfig.json"),
        "TS6053 names the package specifier: {}",
        ts6053[0].message_text
    );
}

#[test]
fn array_extends_diagnoses_each_entry_independently() {
    // An array `extends` mixing a resolvable base, an unreadable relative
    // `.json` (TS5083), and an unresolvable package (TS6053) must diagnose each
    // entry independently and still merge the resolvable base's options.
    let temp = tempdir().expect("create temp dir");
    let project = temp.path().join("project");
    std::fs::create_dir_all(&project).expect("create project dir");
    std::fs::write(
        project.join("present.json"),
        r#"{ "compilerOptions": { "target": "ES2021" } }"#,
    )
    .expect("write present base");
    let child_path = project.join("tsconfig.json");
    std::fs::write(
        &child_path,
        r#"{ "extends": ["./present.json", "./gone.json", "no-such-pkg"] }"#,
    )
    .expect("write child");

    let parsed = load_tsconfig_with_diagnostics(&child_path).expect("load must succeed");

    assert_eq!(
        parsed.diagnostics.iter().filter(|d| d.code == 5083).count(),
        1,
        "the unreadable .json entry gets exactly one TS5083: {:?}",
        parsed.diagnostics
    );
    assert_eq!(
        parsed.diagnostics.iter().filter(|d| d.code == 6053).count(),
        1,
        "the unresolvable package entry gets exactly one TS6053: {:?}",
        parsed.diagnostics
    );
    let opts = parsed.config.compiler_options.expect("present base merged");
    assert_eq!(
        opts.target.as_deref(),
        Some("ES2021"),
        "the resolvable entry is still applied alongside the failing ones"
    );
}
