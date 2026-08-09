//! A relative/absolute `extends` target ending in `.json`: TS5083 ("Cannot
//! read file") vs. the generic TS6053 ("File not found").
//!
//! Split out of `module_resolution.rs` to keep it under the 2000-line limit
//! (§19; ratchet tracked by #8280) rather than growing an already-large file.

use super::super::*;
use tempfile::tempdir;

#[test]
fn extends_array_reports_each_unresolved_entry() {
    // Array `extends` (TS 5.0): every entry that cannot be resolved gets its
    // own diagnostic, and resolvable entries still merge. Each missing entry
    // here already carries a `.json` extension, so per tsc's
    // `getExtendsConfigPath` (oracle-verified against pinned typescript@7.0.2)
    // each independently reports TS5083 ("Cannot read file") against its own
    // resolved absolute path, not TS6053 — see #17079.
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
        r#"{ "extends": ["./present.json", "./missing-a.json", "./missing-b.json"] }"#,
    )
    .expect("write child");

    let parsed = load_tsconfig_with_diagnostics(&child_path).expect("load must succeed");
    let ts5083: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 5083)
        .collect();
    assert_eq!(
        ts5083.len(),
        2,
        "one TS5083 per unresolved .json array entry: {:?}",
        parsed.diagnostics
    );
    let missing_a = project.join("missing-a.json");
    let missing_b = project.join("missing-b.json");
    assert!(
        ts5083[0]
            .message_text
            .contains(&missing_a.to_string_lossy().into_owned()),
        "TS5083 names the first entry's own resolved path: {}",
        ts5083[0].message_text
    );
    assert!(
        ts5083[1]
            .message_text
            .contains(&missing_b.to_string_lossy().into_owned()),
        "TS5083 names the second entry's own resolved path: {}",
        ts5083[1].message_text
    );
    for diagnostic in &ts5083 {
        assert!(
            diagnostic.file.is_empty() && diagnostic.start == 0 && diagnostic.length == 0,
            "tsc reports TS5083 as a bare filesystem read failure with no \
             file(line,col) prefix, not anchored at the config source: {diagnostic:?}"
        );
    }
    let opts = parsed.config.compiler_options.expect("present base merged");
    assert_eq!(
        opts.target.as_deref(),
        Some("ES2021"),
        "the resolvable array entry is still applied"
    );
}

#[test]
fn extends_missing_relative_json_emits_ts5083_at_resolved_path() {
    // A relative `extends` specifier that already carries a `.json`
    // extension skips tsc's existence probe entirely; the failure surfaces
    // as TS5083 ("Cannot read file") against the resolved absolute path,
    // not TS6053's raw-specifier "File not found." Oracle-verified against
    // pinned typescript@7.0.2. See #17079.
    let temp = tempdir().expect("create temp dir");
    let project = temp.path().join("project");
    std::fs::create_dir_all(&project).expect("create project dir");
    let child_path = project.join("tsconfig.json");
    let child_source = r#"{ "extends": "./nope.json", "compilerOptions": { "strict": true } }"#;
    std::fs::write(&child_path, child_source).expect("write child");

    let parsed = load_tsconfig_with_diagnostics(&child_path).expect("load must succeed");
    let ts5083: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 5083)
        .collect();
    assert_eq!(
        ts5083.len(),
        1,
        "exactly one TS5083 for the unreadable extends target: {:?}",
        parsed.diagnostics
    );
    let expected_path = project.join("nope.json");
    assert!(
        ts5083[0]
            .message_text
            .contains(&expected_path.to_string_lossy().into_owned()),
        "TS5083 names the resolved absolute path, not the raw specifier: {}",
        ts5083[0].message_text
    );
    assert!(
        ts5083[0].file.is_empty() && ts5083[0].start == 0 && ts5083[0].length == 0,
        "tsc reports TS5083 unanchored (no file(line,col) prefix), unlike TS6053: {:?}",
        ts5083[0]
    );
    assert!(
        !parsed.diagnostics.iter().any(|d| d.code == 6053),
        "a .json-suffixed relative extends must not also emit TS6053: {:?}",
        parsed.diagnostics
    );
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
fn extends_missing_relative_extensionless_keeps_ts6053() {
    // The negative control: an extensionless relative specifier (no `.json`)
    // still fails resolution up front (tsc probes the raw specifier, then
    // the specifier with `.json` appended) and keeps the raw-specifier
    // TS6053, not TS5083. Oracle-verified against pinned typescript@7.0.2.
    let temp = tempdir().expect("create temp dir");
    let project = temp.path().join("project");
    std::fs::create_dir_all(&project).expect("create project dir");
    let child_path = project.join("tsconfig.json");
    let child_source = r#"{ "extends": "./nope", "compilerOptions": { "strict": true } }"#;
    std::fs::write(&child_path, child_source).expect("write child");

    let parsed = load_tsconfig_with_diagnostics(&child_path).expect("load must succeed");
    let ts6053: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 6053)
        .collect();
    assert_eq!(
        ts6053.len(),
        1,
        "extensionless relative extends keeps TS6053: {:?}",
        parsed.diagnostics
    );
    assert!(
        ts6053[0].message_text.contains("./nope"),
        "TS6053 names the raw specifier, not a resolved path: {}",
        ts6053[0].message_text
    );
    assert!(
        !ts6053[0].file.is_empty(),
        "TS6053, unlike TS5083, anchors at the extends specifier literal: {:?}",
        ts6053[0]
    );
    assert!(
        !parsed.diagnostics.iter().any(|d| d.code == 5083),
        "an extensionless relative extends must not emit TS5083: {:?}",
        parsed.diagnostics
    );
}

#[test]
fn extends_missing_parent_relative_json_normalizes_dotdot() {
    // `../` must lexically collapse against the declaring config's directory
    // before it reaches the TS5083 message — tsc's `getNormalizedAbsolutePath`
    // never renders an embedded `/../` segment. Oracle-verified against
    // pinned typescript@7.0.2.
    let temp = tempdir().expect("create temp dir");
    let project = temp.path().join("project");
    let nested = project.join("nested");
    std::fs::create_dir_all(&nested).expect("create nested dir");
    let child_path = nested.join("tsconfig.json");
    let child_source = r#"{ "extends": "../nope.json", "compilerOptions": { "strict": true } }"#;
    std::fs::write(&child_path, child_source).expect("write child");

    let parsed = load_tsconfig_with_diagnostics(&child_path).expect("load must succeed");
    let ts5083: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 5083)
        .collect();
    assert_eq!(
        ts5083.len(),
        1,
        "exactly one TS5083 for the unreadable parent-relative extends target: {:?}",
        parsed.diagnostics
    );
    let expected_path = project.join("nope.json");
    assert!(
        ts5083[0]
            .message_text
            .contains(&expected_path.to_string_lossy().into_owned()),
        "TS5083 names the lexically-collapsed path, not one with an embedded '/../': {}",
        ts5083[0].message_text
    );
    assert!(
        !ts5083[0].message_text.contains("/../"),
        "the resolved path must not carry an embedded '/../' segment: {}",
        ts5083[0].message_text
    );
}

#[test]
fn extends_missing_absolute_json_emits_ts5083_at_specifier_path() {
    // Absolute specifiers follow the same `.json`-suffix split as relative
    // ones: an absolute path is used as-is (no anchoring join needed), so
    // the TS5083 message names the specifier verbatim. Oracle-verified
    // against pinned typescript@7.0.2.
    let temp = tempdir().expect("create temp dir");
    let project = temp.path().join("project");
    std::fs::create_dir_all(&project).expect("create project dir");
    let child_path = project.join("tsconfig.json");
    let child_source = r#"{ "extends": "/definitely/not/a/real/path/nope.json", "compilerOptions": { "strict": true } }"#;
    std::fs::write(&child_path, child_source).expect("write child");

    let parsed = load_tsconfig_with_diagnostics(&child_path).expect("load must succeed");
    let ts5083: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 5083)
        .collect();
    assert_eq!(
        ts5083.len(),
        1,
        "exactly one TS5083 for the unreadable absolute extends target: {:?}",
        parsed.diagnostics
    );
    assert!(
        ts5083[0]
            .message_text
            .contains("/definitely/not/a/real/path/nope.json"),
        "TS5083 names the absolute specifier path verbatim: {}",
        ts5083[0].message_text
    );
    assert!(
        ts5083[0].file.is_empty() && ts5083[0].start == 0 && ts5083[0].length == 0,
        "tsc reports TS5083 unanchored (no file(line,col) prefix): {:?}",
        ts5083[0]
    );
}
