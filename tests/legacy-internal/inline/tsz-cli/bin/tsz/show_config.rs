//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/bin/tsz/show_config.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 6882010929a7690f2e0e0b88414fda8f5424d2722fdb3ce915f5f47e3b9a35b0 1136 compiler_options_to_json_preserves_strict
    #[test]
    fn compiler_options_to_json_preserves_strict() {
        let cfg = TsConfig {
            compiler_options: Some(CoreCompilerOptions {
                strict: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        let map = build_compiler_options_map(Some(&cfg), &empty_args(), &base_dir());
        assert_eq!(map.get("strict"), Some(&serde_json::Value::Bool(true)));
    }
// TSZ_INLINE_TEST_END 6882010929a7690f2e0e0b88414fda8f5424d2722fdb3ce915f5f47e3b9a35b0

// TSZ_INLINE_TEST_BEGIN 7c86a31367e092d07360bf974a8143365c48240950d9a3f532093904db75dd9e 1149 compiler_options_to_json_normalises_es2015_target_to_es6
    #[test]
    fn compiler_options_to_json_normalises_es2015_target_to_es6() {
        let cfg = TsConfig {
            compiler_options: Some(CoreCompilerOptions {
                target: Some("es2015".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let map = build_compiler_options_map(Some(&cfg), &empty_args(), &base_dir());
        assert_eq!(
            map.get("target"),
            Some(&serde_json::Value::String("es6".to_string()))
        );
    }
// TSZ_INLINE_TEST_END 7c86a31367e092d07360bf974a8143365c48240950d9a3f532093904db75dd9e

// TSZ_INLINE_TEST_BEGIN 730f7a04ad9d6bcc4b8918d2dcf7ca09adca82d4d39cc0a471ae71616f8bb625 1165 cli_override_target_wins_over_tsconfig
    #[test]
    fn cli_override_target_wins_over_tsconfig() {
        let cfg = TsConfig {
            compiler_options: Some(CoreCompilerOptions {
                target: Some("es5".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut args = empty_args();
        args.target = Some(tsz_cli::args::Target::Es2020);
        let map = build_compiler_options_map(Some(&cfg), &args, &base_dir());
        assert_eq!(
            map.get("target"),
            Some(&serde_json::Value::String("es2020".to_string()))
        );
    }
// TSZ_INLINE_TEST_END 730f7a04ad9d6bcc4b8918d2dcf7ca09adca82d4d39cc0a471ae71616f8bb625

// TSZ_INLINE_TEST_BEGIN 4288f7e31d5d15953d44f9cff898c84c24dc059692daa51bf01da7a5e6b66767 1183 cli_override_strict_true_sets_bool
    #[test]
    fn cli_override_strict_true_sets_bool() {
        let mut args = empty_args();
        args.strict = true;
        let map = build_compiler_options_map(None, &args, &base_dir());
        assert_eq!(map.get("strict"), Some(&serde_json::Value::Bool(true)));
    }
// TSZ_INLINE_TEST_END 4288f7e31d5d15953d44f9cff898c84c24dc059692daa51bf01da7a5e6b66767

// TSZ_INLINE_TEST_BEGIN 3737ff6d8692886f6a393ce87a3e7f03b06fd2e7125bcc372f53d85c434aa3b3 1191 cli_override_strict_false_sets_bool_false
    #[test]
    fn cli_override_strict_false_sets_bool_false() {
        // Parse `--strict false` via preprocess_args which injects the hidden flag
        let args = CliArgs::try_parse_from(["tsz", "--__explicitly-disabled-bool-flag", "strict"])
            .expect("hidden flag should parse");
        let map = build_compiler_options_map(None, &args, &base_dir());
        assert_eq!(map.get("strict"), Some(&serde_json::Value::Bool(false)));
    }
// TSZ_INLINE_TEST_END 3737ff6d8692886f6a393ce87a3e7f03b06fd2e7125bcc372f53d85c434aa3b3

// TSZ_INLINE_TEST_BEGIN 38390cb478c9fae1d7719af97390283d01a82e01659a98bf7531d381d3945724 1200 implied_module_added_when_target_set
    #[test]
    fn implied_module_added_when_target_set() {
        // target=es5 → module=commonjs (not the default es2022)
        let mut args = empty_args();
        args.target = Some(tsz_cli::args::Target::Es5);
        let map = build_compiler_options_map(None, &args, &base_dir());
        // es5 → CommonJs → not the default es2022, so module should be implied
        assert!(
            map.contains_key("module"),
            "module should be implied for non-default target"
        );
    }
// TSZ_INLINE_TEST_END 38390cb478c9fae1d7719af97390283d01a82e01659a98bf7531d381d3945724

// TSZ_INLINE_TEST_BEGIN d494ac79f46e14c477d941d90acc5fd10115724ff186c497ebbbb8ed4975e29a 1213 composite_implies_declaration_and_incremental
    #[test]
    fn composite_implies_declaration_and_incremental() {
        let cfg = TsConfig {
            compiler_options: Some(CoreCompilerOptions {
                composite: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        let map = build_compiler_options_map(Some(&cfg), &empty_args(), &base_dir());
        assert_eq!(
            map.get("declaration"),
            Some(&serde_json::Value::Bool(true)),
            "composite implies declaration"
        );
        assert_eq!(
            map.get("incremental"),
            Some(&serde_json::Value::Bool(true)),
            "composite implies incremental"
        );
    }
// TSZ_INLINE_TEST_END d494ac79f46e14c477d941d90acc5fd10115724ff186c497ebbbb8ed4975e29a

// TSZ_INLINE_TEST_BEGIN 591fa38a2fe5d962c12c6d72014753ac2c9829d885077d4350feac7838858d97 1235 verbatim_module_syntax_implies_isolated_modules
    #[test]
    fn verbatim_module_syntax_implies_isolated_modules() {
        let cfg = TsConfig {
            compiler_options: Some(CoreCompilerOptions {
                verbatim_module_syntax: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        let map = build_compiler_options_map(Some(&cfg), &empty_args(), &base_dir());
        assert_eq!(
            map.get("isolatedModules"),
            Some(&serde_json::Value::Bool(true)),
            "verbatimModuleSyntax implies isolatedModules"
        );
    }
// TSZ_INLINE_TEST_END 591fa38a2fe5d962c12c6d72014753ac2c9829d885077d4350feac7838858d97

// TSZ_INLINE_TEST_BEGIN f57212a2de7e389779417849684231a34086d4c8a0791ab5280927da56c18eb6 1254 render_output_empty_config_produces_valid_json
    #[test]
    fn render_output_empty_config_produces_valid_json() {
        let map = serde_json::Map::new();
        let output = render_output(&map, &[], &[], None, &base_dir());
        assert!(output.starts_with('{'));
        assert!(output.trim_end().ends_with('}'));
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert!(parsed.get("compilerOptions").is_some());
    }
// TSZ_INLINE_TEST_END f57212a2de7e389779417849684231a34086d4c8a0791ab5280927da56c18eb6

// TSZ_INLINE_TEST_BEGIN 69f1365af5274ca8cecc7a512f6ab83f52fe51d7ebf9fe1baf497bc98047d569 1264 render_output_includes_files_array
    #[test]
    fn render_output_includes_files_array() {
        let map = serde_json::Map::new();
        let files = vec!["./a.ts".to_string(), "./b.ts".to_string()];
        let output = render_output(&map, &files, &[], None, &base_dir());
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        let files_arr = parsed["files"].as_array().expect("files array");
        assert_eq!(files_arr.len(), 2);
        assert_eq!(files_arr[0], "./a.ts");
        assert_eq!(files_arr[1], "./b.ts");
    }
// TSZ_INLINE_TEST_END 69f1365af5274ca8cecc7a512f6ab83f52fe51d7ebf9fe1baf497bc98047d569

// TSZ_INLINE_TEST_BEGIN fa702d8e11d56f7a32261fe0394cdf43904390f133deecc7c1e241a6774698f8 1276 render_output_includes_exclude_array
    #[test]
    fn render_output_includes_exclude_array() {
        let map = serde_json::Map::new();
        let excludes = vec!["./dist".to_string()];
        let cfg = TsConfig {
            exclude: Some(vec!["dist".to_string()]),
            ..Default::default()
        };
        let output = render_output(&map, &[], &excludes, Some(&cfg), &base_dir());
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        let exc_arr = parsed["exclude"].as_array().expect("exclude array");
        assert_eq!(exc_arr[0], "./dist");
    }
// TSZ_INLINE_TEST_END fa702d8e11d56f7a32261fe0394cdf43904390f133deecc7c1e241a6774698f8

// TSZ_INLINE_TEST_BEGIN 63e2dc0bc6583468fda10d45934035044dc95b4c19778c371b0bfa913f344dbf 1290 render_output_references_appear_before_files
    #[test]
    fn render_output_references_appear_before_files() {
        use tsz_cli::config::TsConfigReference;
        let map = serde_json::Map::new();
        let cfg = TsConfig {
            references: Some(vec![TsConfigReference {
                path: "../lib".to_string(),
                prepend: false,
            }]),
            ..Default::default()
        };
        let files = vec!["./src/index.ts".to_string()];
        let output = render_output(&map, &files, &[], Some(&cfg), &base_dir());
        let refs_pos = output.find("\"references\"").expect("references key");
        let files_pos = output.find("\"files\"").expect("files key");
        assert!(
            refs_pos < files_pos,
            "references must appear before files in output"
        );
    }
// TSZ_INLINE_TEST_END 63e2dc0bc6583468fda10d45934035044dc95b4c19778c371b0bfa913f344dbf

// TSZ_INLINE_TEST_BEGIN a2e10bd1b9fde854be328932e382a4b56b2d1087e83ea0487d45c5ea362537b5 1313 normalize_relative_prepends_dot_slash
    #[test]
    fn normalize_relative_prepends_dot_slash() {
        let base = PathBuf::from("/project");
        let path = PathBuf::from("src/index.ts");
        assert_eq!(normalize_relative(&base, &path), "./src/index.ts");
    }
// TSZ_INLINE_TEST_END a2e10bd1b9fde854be328932e382a4b56b2d1087e83ea0487d45c5ea362537b5

// TSZ_INLINE_TEST_BEGIN ba6b3de4edfa29ab7506c02215c5cedc6fbc2c466d9969d0fb4f380ea874ce80 1320 normalize_relative_strips_base_dir_from_absolute_path
    #[test]
    fn normalize_relative_strips_base_dir_from_absolute_path() {
        let base = PathBuf::from("/project");
        let path = PathBuf::from("/project/src/index.ts");
        assert_eq!(normalize_relative(&base, &path), "./src/index.ts");
    }
// TSZ_INLINE_TEST_END ba6b3de4edfa29ab7506c02215c5cedc6fbc2c466d9969d0fb4f380ea874ce80

// TSZ_INLINE_TEST_BEGIN ecde98b8250679eb5a18ed9e7620cee044efe76c22f9134668a2cd4572ffb291 1327 normalize_relative_preserves_dot_slash_prefix
    #[test]
    fn normalize_relative_preserves_dot_slash_prefix() {
        let base = PathBuf::from("/project");
        let path = PathBuf::from("./src/index.ts");
        assert_eq!(normalize_relative(&base, &path), "./src/index.ts");
    }
// TSZ_INLINE_TEST_END ecde98b8250679eb5a18ed9e7620cee044efe76c22f9134668a2cd4572ffb291

// TSZ_INLINE_TEST_BEGIN e769295af8b229776360efe7582215431bb7c274d83675b41375648fc08ddf8d 1336 format_value_string
    #[test]
    fn format_value_string() {
        assert_eq!(
            format_value(&serde_json::Value::String("es6".into()), 0),
            "\"es6\""
        );
    }
// TSZ_INLINE_TEST_END e769295af8b229776360efe7582215431bb7c274d83675b41375648fc08ddf8d

// TSZ_INLINE_TEST_BEGIN 6eb085f0b74405fd639db87b6d157686239bcadbab96a0b4d2617ef4eb3d5b1f 1344 format_value_bool_true
    #[test]
    fn format_value_bool_true() {
        assert_eq!(format_value(&serde_json::Value::Bool(true), 0), "true");
    }
// TSZ_INLINE_TEST_END 6eb085f0b74405fd639db87b6d157686239bcadbab96a0b4d2617ef4eb3d5b1f

// TSZ_INLINE_TEST_BEGIN 146368976a425d2946a8c94191b7babf8507d886da20bd0d2678a57471b38d78 1349 format_value_empty_array
    #[test]
    fn format_value_empty_array() {
        assert_eq!(format_value(&serde_json::Value::Array(vec![]), 0), "[]");
    }
// TSZ_INLINE_TEST_END 146368976a425d2946a8c94191b7babf8507d886da20bd0d2678a57471b38d78
