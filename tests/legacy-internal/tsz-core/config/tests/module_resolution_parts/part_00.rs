#[test]
fn extends_unresolved_package_emits_ts6053_and_keeps_local_options() {
    // The package providing the base config is not installed (the canary
    // clone-without-deps shape). tsc emits TS6053 anchored at the `extends`
    // specifier and keeps compiling with the local options; tsz must not abort
    // the whole config load.
    let temp = tempdir().expect("create temp dir");
    let project = temp.path().join("project");
    std::fs::create_dir_all(&project).expect("create project dir");
    let child_path = project.join("tsconfig.json");
    let child_source =
        r#"{ "extends": "@scope/pkg/file.json", "compilerOptions": { "strict": true } }"#;
    std::fs::write(&child_path, child_source).expect("write child");

    let parsed = load_tsconfig_with_diagnostics(&child_path).expect("load must succeed, not abort");

    let ts6053: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 6053)
        .collect();
    assert_eq!(
        ts6053.len(),
        1,
        "exactly one TS6053 for the unresolved extends: {:?}",
        parsed.diagnostics
    );
    assert!(
        ts6053[0].message_text.contains("@scope/pkg/file.json"),
        "TS6053 names the unresolved specifier: {}",
        ts6053[0].message_text
    );
    let expected_start = child_source
        .find("\"@scope/pkg/file.json\"")
        .expect("specifier present in source") as u32;
    assert_eq!(
        ts6053[0].start, expected_start,
        "TS6053 anchors at the extends specifier literal"
    );

    let opts = parsed
        .config
        .compiler_options
        .expect("local options retained");
    assert_eq!(
        opts.strict,
        Some(true),
        "local options survive an unresolved extends"
    );
}

#[test]
fn extends_array_reports_each_unresolved_entry() {
    // Array `extends` (TS 5.0): every unresolvable entry gets its own TS6053
    // (entries are extensionless; missing `.json` is TS5083, covered elsewhere).
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
        r#"{ "extends": ["./present.json", "./missing-a", "./missing-b"] }"#,
    )
    .expect("write child");

    let parsed = load_tsconfig_with_diagnostics(&child_path).expect("load must succeed");
    let ts6053: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 6053)
        .collect();
    assert_eq!(
        ts6053.len(),
        2,
        "one TS6053 per unresolved array entry: {:?}",
        parsed.diagnostics
    );
    let opts = parsed.config.compiler_options.expect("present base merged");
    assert_eq!(
        opts.target.as_deref(),
        Some("ES2021"),
        "the resolvable array entry is still applied"
    );
}

#[test]
fn config_dir_template_expands_against_leaf_config_dir() {
    // TS 5.5 `${configDir}`: a root config's template resolves to its own
    // directory and produces absolute selectors/paths.
    let temp = tempdir().expect("create temp dir");
    let project_dir = temp.path().join("project");
    std::fs::create_dir_all(project_dir.join("src")).expect("create src dir");

    let config_path = project_dir.join("tsconfig.json");
    std::fs::write(
        &config_path,
        r#"{
"compilerOptions": { "noEmit": true, "outDir": "${configDir}/dist" },
"include": ["${configDir}/src"]
}"#,
    )
    .expect("write config");

    let merged = load_tsconfig(&config_path).expect("load config");
    let canonical_project = std::fs::canonicalize(&project_dir).unwrap_or(project_dir);
    let expected_src = canonical_project.join("src").to_string_lossy().into_owned();
    let expected_dist = canonical_project
        .join("dist")
        .to_string_lossy()
        .into_owned();

    assert_eq!(
        merged.include.as_deref(),
        Some(&[expected_src][..]),
        "${{configDir}}/src must expand to the config's own directory"
    );
    assert_eq!(
        merged
            .compiler_options
            .as_ref()
            .and_then(|o| o.out_dir.as_deref()),
        Some(expected_dist.as_str()),
    );
}

#[test]
fn config_dir_template_in_base_resolves_to_inheriting_config_dir() {
    // The defining behavior of `${configDir}`: a shared base config can write
    // `${configDir}/...` and every consumer resolves it against the consumer's
    // (leaf) directory, NOT the base config's own directory.
    let temp = tempdir().expect("create temp dir");
    let base_dir = temp.path().join("shared");
    let app_dir = temp.path().join("app");
    std::fs::create_dir_all(app_dir.join("src")).expect("create app src");
    std::fs::create_dir_all(&base_dir).expect("create base dir");

    let base_path = base_dir.join("tsconfig.base.json");
    std::fs::write(
        &base_path,
        r#"{
"compilerOptions": { "outDir": "${configDir}/dist", "baseUrl": "${configDir}" },
"include": ["${configDir}/src"]
}"#,
    )
    .expect("write base");

    let child_path = app_dir.join("tsconfig.json");
    std::fs::write(
        &child_path,
        r#"{ "extends": "../shared/tsconfig.base.json", "compilerOptions": { "noEmit": true } }"#,
    )
    .expect("write child");

    let merged = load_tsconfig(&child_path).expect("load child");
    let canonical_app = std::fs::canonicalize(&app_dir).unwrap_or(app_dir);
    let canonical_base = std::fs::canonicalize(&base_dir).unwrap_or(base_dir);

    let include = merged.include.expect("inherited include present");
    assert_eq!(
        include[0],
        canonical_app.join("src").to_string_lossy(),
        "${{configDir}} in the base must resolve to the inheriting config's dir"
    );
    assert!(
        !include[0].starts_with(canonical_base.to_string_lossy().as_ref()),
        "${{configDir}} must not anchor at the base config's own directory: {:?}",
        include[0]
    );

    let opts = merged.compiler_options.expect("compiler options merged");
    assert_eq!(
        opts.out_dir.as_deref(),
        Some(canonical_app.join("dist").to_string_lossy().as_ref()),
    );
    assert_eq!(
        opts.base_url.as_deref(),
        Some(canonical_app.to_string_lossy().as_ref()),
    );
}
