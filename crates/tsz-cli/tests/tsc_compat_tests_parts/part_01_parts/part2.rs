#[test]
fn show_config_inherited_base_url_and_root_dirs_stay_declaring_relative() {
    let temp = TempDir::new("show_config_inherited_path_options").expect("temp dir");
    std::fs::create_dir_all(temp.path.join("base/src")).expect("create base src");
    std::fs::create_dir_all(temp.path.join("base/generated")).expect("create base generated");
    std::fs::create_dir_all(temp.path.join("app/src")).expect("create app src");
    write_file(&temp.path.join("app/src/a.ts"), "export {}\n");
    write_file(
        &temp.path.join("base/tsconfig.base.json"),
        r#"{
  "compilerOptions": {
    "baseUrl": ".",
    "rootDirs": ["src", "generated"]
  }
}
"#,
    );
    write_file(
        &temp.path.join("app/tsconfig.json"),
        r#"{
  "extends": "../base/tsconfig.base.json",
  "files": ["src/a.ts"]
}
"#,
    );

    let output = run_tsz(&temp.path.join("app"), &["--showConfig"]).expect("tsz should run");
    let json: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or_else(|_| panic!("invalid showConfig JSON:\n{output}"));
    let options = json
        .get("compilerOptions")
        .and_then(|v| v.as_object())
        .unwrap_or_else(|| panic!("missing compilerOptions in showConfig output:\n{output}"));

    assert_eq!(
        options.get("baseUrl"),
        Some(&serde_json::Value::String("../base".to_string())),
        "inherited baseUrl should render relative to the child config: {output}"
    );
    assert_eq!(
        options.get("rootDirs"),
        Some(&serde_json::json!(["../base/src", "../base/generated"])),
        "inherited rootDirs should render relative to the child config: {output}"
    );
}

#[test]
fn tsc_parity_init() {
    if !tsc_available() {
        return;
    }
    // Run --init in separate temp dirs and compare generated tsconfig.json
    let temp_tsc = TempDir::new("init_tsc").expect("temp dir");
    let temp_tsz = TempDir::new("init_tsz").expect("temp dir");

    let tsc_out = run_tsc(&temp_tsc.path, &["--init"]).expect("tsc --init failed");
    let tsz_out = run_tsz(&temp_tsz.path, &["--init"]).expect("tsz --init failed");

    // Console output should match
    if let Some(diff) = diff_outputs(&tsc_out, &tsz_out) {
        panic!("--init console output mismatch:\n{diff}\n\ntsc:\n{tsc_out}\n\ntsz:\n{tsz_out}");
    }

    // Generated tsconfig.json should match
    let tsc_config =
        std::fs::read_to_string(temp_tsc.path.join("tsconfig.json")).expect("tsc tsconfig.json");
    let tsz_config =
        std::fs::read_to_string(temp_tsz.path.join("tsconfig.json")).expect("tsz tsconfig.json");
    assert_eq!(
        tsc_config, tsz_config,
        "--init: generated tsconfig.json files differ"
    );
}

/// Regression test for #3905. When `--init` is invoked together with
/// recognized compiler options, the generated tsconfig.json should reflect
/// those options instead of the hardcoded template. This exercises three
/// distinct override paths: replacing a commented template line (`rootDir`,
/// `outDir`), overwriting an active template line (`module`, `target`,
/// `strict`), and appending an option that has no template slot (`pretty`).
#[test]
fn tsc_parity_init_with_options() {
    if !tsc_available() {
        return;
    }
    let temp_tsc = TempDir::new("init_opts_tsc").expect("temp dir");
    let temp_tsz = TempDir::new("init_opts_tsz").expect("temp dir");

    let opts: &[&str] = &[
        "--init",
        "--target",
        "es2015",
        "--module",
        "commonjs",
        "--rootDir",
        "src",
        "--outDir",
        "dist",
        "--strict",
        "false",
        "--pretty",
        "false",
    ];

    let tsc_out = run_tsc(&temp_tsc.path, opts).expect("tsc --init failed");
    let tsz_out = run_tsz(&temp_tsz.path, opts).expect("tsz --init failed");

    if let Some(diff) = diff_outputs(&tsc_out, &tsz_out) {
        panic!("--init console output mismatch:\n{diff}\n\ntsc:\n{tsc_out}\n\ntsz:\n{tsz_out}");
    }

    let tsc_config =
        std::fs::read_to_string(temp_tsc.path.join("tsconfig.json")).expect("tsc tsconfig.json");
    let tsz_config =
        std::fs::read_to_string(temp_tsz.path.join("tsconfig.json")).expect("tsz tsconfig.json");
    assert_eq!(
        tsc_config, tsz_config,
        "--init with options: generated tsconfig.json files differ"
    );
}

/// Multiple command-line-only options (`--diagnostics`, `--listFiles`,
/// `--noEmit`, `--pretty`) get appended after the template body in the order
/// they appeared on the command line.
#[test]
fn tsc_parity_init_appends_command_line_options_in_order() {
    if !tsc_available() {
        return;
    }
    let temp_tsc = TempDir::new("init_append_tsc").expect("temp dir");
    let temp_tsz = TempDir::new("init_append_tsz").expect("temp dir");

    let opts: &[&str] = &[
        "--init",
        "--listFiles",
        "--noEmit",
        "--diagnostics",
        "--pretty",
        "false",
    ];

    let tsc_out = run_tsc(&temp_tsc.path, opts).expect("tsc --init failed");
    let tsz_out = run_tsz(&temp_tsz.path, opts).expect("tsz --init failed");

    if let Some(diff) = diff_outputs(&tsc_out, &tsz_out) {
        panic!("--init console output mismatch:\n{diff}\n\ntsc:\n{tsc_out}\n\ntsz:\n{tsz_out}");
    }

    let tsc_config =
        std::fs::read_to_string(temp_tsc.path.join("tsconfig.json")).expect("tsc tsconfig.json");
    let tsz_config =
        std::fs::read_to_string(temp_tsz.path.join("tsconfig.json")).expect("tsz tsconfig.json");
    assert_eq!(
        tsc_config, tsz_config,
        "--init append-only options: generated tsconfig.json files differ"
    );
}

#[test]
fn tsc_parity_plain_single_ts2304() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("plain_ts2304").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const z = unknownVar;\n");
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "plain single TS2304",
    );
}

#[test]
fn tsc_parity_plain_multiple_ts2304() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("plain_multi_ts2304").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "const a = foo;\nconst b = bar;\nconst c = baz;\n",
    );
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "plain multiple TS2304",
    );
}

#[test]
fn tsc_parity_plain_multi_file() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("plain_multi_file").expect("temp dir");
    write_file(&temp.path.join("a.ts"), "const a = foo;\n");
    write_file(&temp.path.join("b.ts"), "const b = bar;\n");
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "false", "a.ts", "b.ts"],
        "plain multi-file",
    );
}

#[test]
fn tsc_parity_plain_no_errors() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("plain_clean").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "const x: number = 42;\nconst y: string = \"hello\";\n",
    );
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "plain no errors",
    );
}

#[test]
fn tsc_parity_pretty_single_ts2304() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("pretty_ts2304").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const z = unknownVar;\n");
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "true", "test.ts"],
        "pretty single TS2304",
    );
}
