use super::*;

#[test]
fn test_cli_sound_report_only_sets_sound_mode_and_report_only() {
    let args = CliArgs::try_parse_from(["tsz", "--soundReportOnly"]).expect("parse args");
    let mut options = ResolvedCompilerOptions::default();
    super::super::apply_cli_overrides(&mut options, &args).expect("apply overrides");
    assert!(
        options.checker.sound_mode,
        "--soundReportOnly must enable sound_mode (implied)"
    );
    assert!(
        options.checker.sound_report_only,
        "--soundReportOnly must enable sound_report_only"
    );
}

#[test]
fn test_cli_sound_report_only_kebab_alias_works() {
    let args = CliArgs::try_parse_from(["tsz", "--sound-report-only"]).expect("parse kebab alias");
    let mut options = ResolvedCompilerOptions::default();
    super::super::apply_cli_overrides(&mut options, &args).expect("apply overrides");
    assert!(options.checker.sound_mode);
    assert!(options.checker.sound_report_only);
}

#[test]
fn test_cli_sound_report_only_false_override_clears_only_report_only() {
    let args = CliArgs::try_parse_from([
        "tsz",
        "--sound",
        "--__explicitly-disabled-bool-flag=soundReportOnly",
    ])
    .expect("parse args");
    let mut options = ResolvedCompilerOptions::default();
    super::super::apply_cli_overrides(&mut options, &args).expect("apply overrides");
    assert!(options.checker.sound_mode, "sound_mode must stay true");
    assert!(
        !options.checker.sound_report_only,
        "sound_report_only must be cleared"
    );
}

#[test]
fn test_sound_report_only_defaults_false() {
    let options = ResolvedCompilerOptions::default();
    assert!(!options.checker.sound_report_only);
    assert!(!options.checker.sound_mode);
}

#[test]
fn test_cli_sound_flag_does_not_set_report_only() {
    let args = CliArgs::try_parse_from(["tsz", "--sound"]).expect("parse args");
    let mut options = ResolvedCompilerOptions::default();
    super::super::apply_cli_overrides(&mut options, &args).expect("apply overrides");
    assert!(options.checker.sound_mode, "sound_mode must be true");
    assert!(
        !options.checker.sound_report_only,
        "--sound alone must not set sound_report_only"
    );
}

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

/// TS5107 for `allowSyntheticDefaultImports=false` must NOT append the migration URL
/// ("Visit <https://aka.ms/ts6>"). tsc 6.0.3 only chains the URL for options with an
/// active migration target (moduleResolution, module, target). This was a regression
/// introduced in #12292.
#[test]
fn test_ts5107_allow_synthetic_default_imports_false_no_migration_url() {
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
    let ts5107: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 5107)
        .collect();
    assert!(
        !ts5107.is_empty(),
        "expected TS5107, got: {:?}",
        result.diagnostics
    );
    for d in &ts5107 {
        assert!(
            !d.message_text.contains("aka.ms/ts6"),
            "TS5107 for allowSyntheticDefaultImports=false must not contain migration URL, got: {}",
            d.message_text
        );
    }
}

/// TS5107 for `moduleResolution=node10` MUST append the migration URL because
/// it has an active migration target (node16/bundler). Confirms the allowlist
/// retains the URL for options that need it.
#[test]
fn test_ts5107_module_resolution_node10_has_migration_url() {
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
    let ts5107: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 5107)
        .collect();
    assert!(
        !ts5107.is_empty(),
        "expected TS5107, got: {:?}",
        result.diagnostics
    );
    assert!(
        ts5107.iter().any(|d| d.message_text.contains("aka.ms/ts6")),
        "TS5107 for moduleResolution=node10 must contain migration URL, got: {ts5107:?}"
    );
}

#[test]
fn test_compile_sound_report_only_collects_diagnostics_from_sound_mode() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(dir.path().join("a.ts"), "const x: string = 42;\n").expect("write source");

    let args_sound_report_only =
        CliArgs::try_parse_from(["tsz", "--noEmit", "--soundReportOnly", "a.ts"])
            .expect("parse args");
    let result = compile(&args_sound_report_only, dir.path()).expect("compile");
    assert!(
        result.diagnostics.iter().any(|d| d.code == 2322),
        "TS2322 should still be reported in sound_report_only mode, got: {:?}",
        result.diagnostics
    );
}
