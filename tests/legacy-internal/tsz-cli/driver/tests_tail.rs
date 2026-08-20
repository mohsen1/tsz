use super::*;

/// TS18003 must not fire when `references[]` is non-empty: a references-only
/// root tsconfig (orchestrates child builds without owning .ts inputs) is a
/// standard TypeScript Project References pattern that tsc accepts silently.
#[test]
fn test_compile_no_ts18003_for_references_only_tsconfig() {
    let dir = tempfile::tempdir().expect("temp dir");
    let child_dir = dir.path().join("child");
    fs::create_dir_all(&child_dir).expect("create child dir");

    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "include": ["./global.d.ts"],
  "references": [{ "path": "./child" }]
}"#,
    )
    .expect("write root tsconfig");
    fs::write(dir.path().join("global.d.ts"), "").expect("write global.d.ts");
    fs::write(
        child_dir.join("tsconfig.json"),
        r#"{ "compilerOptions": { "composite": true, "noEmit": true } }"#,
    )
    .expect("write child tsconfig");
    fs::write(child_dir.join("a.ts"), "export const x = 1;\n").expect("write child source");

    let project = dir.path().to_string_lossy().to_string();
    let args = CliArgs::try_parse_from(["tsz", "--project", project.as_str(), "--noEmit"])
        .expect("project args");
    let result = compile(&args, dir.path()).expect("compile succeeds");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&18003),
        "TS18003 must not fire for references-only tsconfig, got: {codes:?}"
    );
}

#[test]
fn test_ts5108_allow_synthetic_default_imports_false() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{ "compilerOptions": { "allowSyntheticDefaultImports": false, "noEmit": true, "strict": true } }"#,
    )
    .expect("write tsconfig");
    fs::write(dir.path().join("a.ts"), "export const x = 1;\n").expect("write source");

    let project = dir.path().to_string_lossy().to_string();
    let args =
        CliArgs::try_parse_from(["tsz", "--project", project.as_str()]).expect("project args");
    let result = compile(&args, dir.path()).expect("compile succeeds");
    let ts5108: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 5108)
        .collect();
    assert!(
        !ts5108.is_empty(),
        "expected TS5108, got: {:?}",
        result.diagnostics
    );
    assert_eq!(
        ts5108[0].message_text,
        "Option 'allowSyntheticDefaultImports=false' has been removed. Please remove it from your configuration."
    );
}

#[test]
fn test_ts5108_module_resolution_node10_uses_canonical_alias() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{ "compilerOptions": { "moduleResolution": "node10", "noEmit": true } }"#,
    )
    .expect("write tsconfig");
    fs::write(dir.path().join("a.ts"), "export const x = 1;\n").expect("write source");

    let project = dir.path().to_string_lossy().to_string();
    let args =
        CliArgs::try_parse_from(["tsz", "--project", project.as_str()]).expect("project args");
    let result = compile(&args, dir.path()).expect("compile succeeds");
    let ts5108: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 5108)
        .collect();
    assert!(
        !ts5108.is_empty(),
        "expected TS5108, got: {:?}",
        result.diagnostics
    );
    assert!(
        ts5108
            .iter()
            .any(|d| d.message_text.contains("moduleResolution=node10")),
        "TS5108 should canonicalize the node alias, got: {ts5108:?}"
    );
}

/// `find_latest_dts_file` must select the newest declaration output by
/// mtime and ignore non-declaration outputs entirely, even when those are
/// newer. Declaration matching follows tsc's `isDeclarationFileName`
/// (`.d.ts`/`.d.mts`/`.d.cts`), not `Path::extension()` — the final-dot
/// extension of `a.d.ts` is just `ts`, which previously made this function
/// return `None` for every emitted file.
#[test]
fn find_latest_dts_file_picks_newest_declaration_and_ignores_non_dts() {
    use std::time::{Duration, SystemTime};

    let dir = tempfile::tempdir().expect("temp dir");
    let base = dir.path();
    fs::create_dir_all(base.join("dist/nested")).expect("create dist dirs");

    let old_dts = base.join("dist/alpha.d.ts");
    let new_dts = base.join("dist/nested/beta.d.ts");
    let newer_js = base.join("dist/alpha.js");
    let newer_map = base.join("dist/alpha.d.ts.map");
    for (path, contents) in [
        (&old_dts, "export declare const a: number;\n"),
        (&new_dts, "export declare const b: number;\n"),
        (&newer_js, "var a = 1;\n"),
        (&newer_map, "{}\n"),
    ] {
        fs::write(path, contents).expect("write output");
    }

    let now = SystemTime::now();
    let set_mtime = |path: &Path, age_secs: u64| {
        fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("open output")
            .set_modified(now - Duration::from_secs(age_secs))
            .expect("set mtime");
    };
    set_mtime(&old_dts, 300);
    set_mtime(&new_dts, 200);
    // Non-declaration outputs are newest overall and must still lose.
    set_mtime(&newer_js, 0);
    set_mtime(&newer_map, 0);

    let emitted = vec![newer_js, old_dts, newer_map, new_dts];
    assert_eq!(
        super::super::find_latest_dts_file(&emitted, base).as_deref(),
        Some("dist/nested/beta.d.ts"),
        "newest .d.ts must win; .js and .d.ts.map outputs must be ignored"
    );
}

/// Declaration matching must cover the full tsc `isDeclarationFileName`
/// family, not just `.d.ts`: `.d.mts` and `.d.cts` outputs participate too.
#[test]
fn find_latest_dts_file_matches_dmts_and_dcts_like_tsc() {
    use std::time::{Duration, SystemTime};

    let dir = tempfile::tempdir().expect("temp dir");
    let base = dir.path();
    let old_dts = base.join("old.d.ts");
    let mid_dcts = base.join("mid.d.cts");
    let new_dmts = base.join("new.d.mts");
    for path in [&old_dts, &mid_dcts, &new_dmts] {
        fs::write(path, "export {};\n").expect("write output");
    }

    let now = SystemTime::now();
    for (path, age_secs) in [(&old_dts, 300u64), (&mid_dcts, 200), (&new_dmts, 100)] {
        fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("open output")
            .set_modified(now - Duration::from_secs(age_secs))
            .expect("set mtime");
    }

    let emitted = vec![old_dts, mid_dcts, new_dmts];
    assert_eq!(
        super::super::find_latest_dts_file(&emitted, base).as_deref(),
        Some("new.d.mts"),
        "newest declaration output must win across .d.ts/.d.cts/.d.mts"
    );
}

/// No declaration outputs (or no outputs at all) yields `None`; the caller's
/// carry-forward then preserves the previously saved value.
#[test]
fn find_latest_dts_file_returns_none_without_declaration_outputs() {
    let dir = tempfile::tempdir().expect("temp dir");
    let base = dir.path();
    let js = base.join("dist/only.js");
    fs::create_dir_all(js.parent().expect("parent")).expect("create dist");
    fs::write(&js, "var x = 1;\n").expect("write js");

    assert_eq!(super::super::find_latest_dts_file(&[js], base), None);
    assert_eq!(super::super::find_latest_dts_file(&[], base), None);
}

/// Regression: an incremental save that emits no declaration output must
/// preserve the previously recorded `latestChangedDtsFile` instead of
/// clearing it. tsc seeds builder state from the old program
/// (`createBuilderProgramState` in src/compiler/builder.ts) and only
/// reassigns the field when a declaration file is written, so a `--noEmit`
/// incremental re-save keeps the prior value.
#[test]
fn test_incremental_no_emit_save_carries_forward_latest_changed_dts_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "incremental": true,
    "declaration": true,
    "outDir": "dist",
    "tsBuildInfoFile": "cache.tsbuildinfo"
  },
  "files": ["a.ts"]
}"#,
    )
    .expect("write tsconfig");
    fs::write(dir.path().join("a.ts"), "export const x: number = 1;\n").expect("write source");

    let args = CliArgs::try_parse_from(["tsz", "-p", "tsconfig.json", "--pretty", "false"])
        .expect("parse args");
    let result = compile(&args, dir.path()).expect("first compile");
    assert!(
        result.diagnostics.is_empty(),
        "expected clean first build, got: {:?}",
        result.diagnostics
    );

    let build_info_path = dir.path().join("cache.tsbuildinfo");
    let first = crate::incremental::BuildInfo::load(&build_info_path)
        .expect("load first build info")
        .expect("first build info is compatible");
    assert_eq!(
        first.latest_changed_dts_file.as_deref(),
        Some("dist/a.d.ts"),
        "declaration emit must record the latest changed .d.ts output"
    );
    let first_version = first
        .get_file_info("a.ts")
        .expect("a.ts recorded in first build info")
        .version
        .clone();

    // Change the source so the second run actually recompiles and re-saves
    // BuildInfo, then run with --noEmit so no declaration output is written.
    fs::write(dir.path().join("a.ts"), "export const x: number = 2;\n").expect("rewrite source");
    let no_emit_args = CliArgs::try_parse_from([
        "tsz",
        "-p",
        "tsconfig.json",
        "--noEmit",
        "--pretty",
        "false",
    ])
    .expect("parse args");
    let second_result = compile(&no_emit_args, dir.path()).expect("second compile");
    assert!(
        second_result.diagnostics.is_empty(),
        "expected clean no-emit build, got: {:?}",
        second_result.diagnostics
    );

    let second = crate::incremental::BuildInfo::load(&build_info_path)
        .expect("load second build info")
        .expect("second build info is compatible");
    assert_ne!(
        second
            .get_file_info("a.ts")
            .expect("a.ts recorded in second build info")
            .version,
        first_version,
        "second compile must have re-saved BuildInfo \
         (guards against a stale file masking the carry-forward)"
    );
    assert_eq!(
        second.latest_changed_dts_file.as_deref(),
        Some("dist/a.d.ts"),
        "no-emit incremental save must preserve the prior latestChangedDtsFile (tsc parity)"
    );
}
