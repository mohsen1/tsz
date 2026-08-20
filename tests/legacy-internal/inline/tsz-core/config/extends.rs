//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-core/src/config/extends.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 2f116b086132bce080f3f113c122f0e919a5393b008fbb410bf9a9441d900b8c 731 substitute_config_dir_expands_root_selectors_and_path_options
    #[test]
    fn substitute_config_dir_expands_root_selectors_and_path_options() {
        let config_dir = Path::new("/proj/app");
        let mut config = TsConfig {
            include: Some(vec![
                "${configDir}/src".to_string(),
                "src/**/*.ts".to_string(),
            ]),
            exclude: Some(vec!["${configDir}/dist".to_string()]),
            files: Some(vec!["${configDir}/entry.ts".to_string()]),
            compiler_options: Some(CompilerOptions {
                base_url: Some("${configDir}".to_string()),
                out_dir: Some("${configDir}/dist".to_string()),
                type_roots: Some(vec![
                    "${configDir}/types".to_string(),
                    "./node_modules/@types".to_string(),
                ]),
                paths: Some(
                    [("@app/*".to_string(), vec!["${configDir}/src/*".to_string()])]
                        .into_iter()
                        .collect(),
                ),
                ..Default::default()
            }),
            ..Default::default()
        };

        substitute_config_dir_templates(&mut config, config_dir);

        let include = config.include.as_ref().unwrap();
        assert_eq!(
            include[0], "/proj/app/src",
            "${{configDir}}/src resolves against the root config dir"
        );
        assert_eq!(
            include[1], "src/**/*.ts",
            "non-template selectors are left for the extends anchoring step"
        );
        assert_eq!(config.exclude.as_ref().unwrap()[0], "/proj/app/dist");
        assert_eq!(config.files.as_ref().unwrap()[0], "/proj/app/entry.ts");

        let opts = config.compiler_options.as_ref().unwrap();
        assert_eq!(
            opts.base_url.as_deref(),
            Some("/proj/app"),
            "bare ${{configDir}} resolves to the directory itself"
        );
        assert_eq!(opts.out_dir.as_deref(), Some("/proj/app/dist"));
        let type_roots = opts.type_roots.as_ref().unwrap();
        assert_eq!(type_roots[0], "/proj/app/types");
        assert_eq!(
            type_roots[1], "./node_modules/@types",
            "non-template entries untouched"
        );
        assert_eq!(opts.paths.as_ref().unwrap()["@app/*"][0], "/proj/app/src/*");
    }
// TSZ_INLINE_TEST_END 2f116b086132bce080f3f113c122f0e919a5393b008fbb410bf9a9441d900b8c

// TSZ_INLINE_TEST_BEGIN ec78fb5fdb04743cd95c44fa3c26422e5103bae6181d61053578de6884b15fd6 788 substitute_config_dir_only_matches_leading_token
    #[test]
    fn substitute_config_dir_only_matches_leading_token() {
        let config_dir = Path::new("/proj");
        let mut config = TsConfig {
            // The TS spec only honors `${configDir}` at the start of a value.
            include: Some(vec!["src/${configDir}/x".to_string()]),
            ..Default::default()
        };
        substitute_config_dir_templates(&mut config, config_dir);
        assert_eq!(
            config.include.as_ref().unwrap()[0],
            "src/${configDir}/x",
            "a non-leading template is left literal, matching tsc"
        );
    }
// TSZ_INLINE_TEST_END ec78fb5fdb04743cd95c44fa3c26422e5103bae6181d61053578de6884b15fd6

// TSZ_INLINE_TEST_BEGIN 6e13d1c2a31942918a7514349bb00d79f0ac13936af350048744e5317a3d02bc 804 merge_configs_child_overrides_base_compiler_options
    #[test]
    fn merge_configs_child_overrides_base_compiler_options() {
        let base = TsConfig {
            compiler_options: Some(CompilerOptions {
                strict: Some(false),
                target: Some("ES5".to_string()),
                ..Default::default()
            }),
            include: Some(vec!["base/**/*".to_string()]),
            ..Default::default()
        };
        let child = TsConfig {
            compiler_options: Some(CompilerOptions {
                strict: Some(true),
                ..Default::default()
            }),
            include: Some(vec!["child/**/*".to_string()]),
            ..Default::default()
        };

        let merged = merge_configs(base, child);

        let opts = merged.compiler_options.expect("merged compiler options");
        assert_eq!(opts.strict, Some(true), "child overrides base");
        assert_eq!(
            opts.target.as_deref(),
            Some("ES5"),
            "child does not erase base when unset"
        );
        assert_eq!(
            merged.include.as_deref(),
            Some(&["child/**/*".to_string()][..]),
            "child include wins"
        );
    }
// TSZ_INLINE_TEST_END 6e13d1c2a31942918a7514349bb00d79f0ac13936af350048744e5317a3d02bc

// TSZ_INLINE_TEST_BEGIN dbdd9d9eeef243ece4d90591d763da18d50af0f347b0882ff7af1fdee27831b6 840 merge_configs_child_compiler_options_absent_keeps_base
    #[test]
    fn merge_configs_child_compiler_options_absent_keeps_base() {
        let base = TsConfig {
            compiler_options: Some(CompilerOptions {
                strict: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        let child = TsConfig::default();

        let merged = merge_configs(base, child);
        assert_eq!(
            merged.compiler_options.as_ref().and_then(|o| o.strict),
            Some(true)
        );
    }
// TSZ_INLINE_TEST_END dbdd9d9eeef243ece4d90591d763da18d50af0f347b0882ff7af1fdee27831b6

// TSZ_INLINE_TEST_BEGIN a5c24226adf81c705ef635ced226eea7970bb3f7eedecbae52431f46590eb67b 858 merge_compiler_options_invalidated_combines_child_first
    #[test]
    fn merge_compiler_options_invalidated_combines_child_first() {
        let base = CompilerOptions {
            invalidated_options: vec!["target".to_string()],
            ..Default::default()
        };
        let child = CompilerOptions {
            invalidated_options: vec!["module".to_string()],
            ..Default::default()
        };

        let merged = merge_compiler_options(base, child);
        assert_eq!(
            merged.invalidated_options,
            vec!["module".to_string(), "target".to_string()],
            "child invalidations come first, then base"
        );
    }
// TSZ_INLINE_TEST_END a5c24226adf81c705ef635ced226eea7970bb3f7eedecbae52431f46590eb67b

// TSZ_INLINE_TEST_BEGIN 84a8a235782e1159e9894a5eb1544eb088ca4c89785c03991824c0b8a5c59259 877 merge_configs_references_only_from_child
    #[test]
    fn merge_configs_references_only_from_child() {
        let base = TsConfig {
            references: Some(vec![TsConfigReference {
                path: "../base-ref".to_string(),
                prepend: false,
            }]),
            ..Default::default()
        };
        let child = TsConfig::default();

        let merged = merge_configs(base, child);
        assert!(
            merged.references.is_none(),
            "references must not inherit through extends"
        );
    }
// TSZ_INLINE_TEST_END 84a8a235782e1159e9894a5eb1544eb088ca4c89785c03991824c0b8a5c59259

// TSZ_INLINE_TEST_BEGIN dac050b7dcddb6dd6e6bd4ba335c3f722c3ca7b064c41bb75e87184e36702c54 895 anchor_inherited_root_selectors_makes_relative_paths_absolute
    #[test]
    fn anchor_inherited_root_selectors_makes_relative_paths_absolute() {
        let temp = tempdir().unwrap();
        let config_dir = temp.path().join("nested");
        std::fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("tsconfig.json");

        let mut config = TsConfig {
            include: Some(vec!["src/**/*".to_string(), "/already/abs".to_string()]),
            exclude: Some(vec!["node_modules".to_string()]),
            files: Some(vec!["entry.ts".to_string()]),
            ..Default::default()
        };

        anchor_inherited_root_selectors(&mut config, &config_path);
        let parent_abs = std::fs::canonicalize(&config_dir).unwrap_or_else(|_| config_dir.clone());

        let include = config.include.as_ref().unwrap();
        assert_eq!(include[0], parent_abs.join("src/**/*").to_string_lossy());
        assert_eq!(include[1], "/already/abs", "absolute selectors untouched");
        let exclude = config.exclude.as_ref().unwrap();
        assert_eq!(
            exclude[0],
            parent_abs.join("node_modules").to_string_lossy()
        );
        let files = config.files.as_ref().unwrap();
        assert_eq!(files[0], parent_abs.join("entry.ts").to_string_lossy());
    }
// TSZ_INLINE_TEST_END dac050b7dcddb6dd6e6bd4ba335c3f722c3ca7b064c41bb75e87184e36702c54

// TSZ_INLINE_TEST_BEGIN 9f4b039419bbf22e868202230f4b3c706a4e3c71cffe5128a456f9d8967d8a6e 924 anchor_inherited_root_selectors_normalizes_dot_segments
    #[test]
    fn anchor_inherited_root_selectors_normalizes_dot_segments() {
        // Regression for the false TS18003 on a references-only root that
        // inherits `"include": ["./global.d.ts"]` from a base config (the
        // `mswjs/msw` shape). `Path::join` keeps the leading `./`, producing
        // an unmatchable glob `<dir>/./global.d.ts`; anchoring must collapse
        // `.`/`..` while leaving glob metacharacters intact.
        let temp = tempdir().unwrap();
        let config_dir = temp.path().join("project");
        std::fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("tsconfig.base.json");

        let mut config = TsConfig {
            include: Some(vec![
                "./global.d.ts".to_string(),
                "./src/**/*.ts".to_string(),
            ]),
            exclude: Some(vec!["./node_modules".to_string()]),
            files: Some(vec!["../shared/entry.ts".to_string()]),
            ..Default::default()
        };

        anchor_inherited_root_selectors(&mut config, &config_path);
        let parent_abs = std::fs::canonicalize(&config_dir).unwrap_or_else(|_| config_dir.clone());

        let include = config.include.as_ref().unwrap();
        assert_eq!(
            include[0],
            parent_abs.join("global.d.ts").to_string_lossy(),
            "leading ./ must be collapsed so the glob matches the real file"
        );
        assert_eq!(
            include[1],
            parent_abs.join("src/**/*.ts").to_string_lossy(),
            "glob metacharacters must survive normalization"
        );
        let exclude = config.exclude.as_ref().unwrap();
        assert_eq!(
            exclude[0],
            parent_abs.join("node_modules").to_string_lossy()
        );
        let files = config.files.as_ref().unwrap();
        assert_eq!(
            files[0],
            parent_abs
                .parent()
                .unwrap()
                .join("shared/entry.ts")
                .to_string_lossy(),
            ".. must resolve against the declaring config's directory"
        );

        for selector in include.iter().chain(exclude).chain(files) {
            assert!(
                !selector.contains("/./") && !selector.contains("/../"),
                "anchored selector must not retain dot segments: {selector}"
            );
        }
    }
// TSZ_INLINE_TEST_END 9f4b039419bbf22e868202230f4b3c706a4e3c71cffe5128a456f9d8967d8a6e

// TSZ_INLINE_TEST_BEGIN a9faa53a0f66ba89767ec33167508f7df9e42bbcf91e83217d1fca177cd958cc 984 anchor_inherited_path_options_anchors_baseurl_to_base_dir
    #[test]
    fn anchor_inherited_path_options_anchors_baseurl_to_base_dir() {
        let temp = tempdir().unwrap();
        let config_dir = temp.path().join("base");
        let dist_dir = config_dir.join("dist");
        std::fs::create_dir_all(&dist_dir).unwrap();
        let config_path = config_dir.join("tsconfig.json");

        let mut config = TsConfig {
            compiler_options: Some(CompilerOptions {
                base_url: Some(".".to_string()),
                out_dir: Some("./dist".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        anchor_inherited_path_options(&mut config, &config_path);

        let opts = config.compiler_options.unwrap();
        let canonical_base =
            std::fs::canonicalize(&config_dir).unwrap_or_else(|_| config_dir.clone());
        let canonical_dist = std::fs::canonicalize(&dist_dir).unwrap_or_else(|_| dist_dir.clone());
        assert_eq!(
            opts.base_url.as_deref(),
            Some(canonical_base.to_string_lossy().as_ref())
        );
        assert_eq!(
            opts.out_dir.as_deref(),
            Some(canonical_dist.to_string_lossy().as_ref())
        );
    }
// TSZ_INLINE_TEST_END a9faa53a0f66ba89767ec33167508f7df9e42bbcf91e83217d1fca177cd958cc

// TSZ_INLINE_TEST_BEGIN 056b31007ac3405de071d7a876c04942833f97d81bea48bf34cea391d93df54f 1017 anchor_inherited_path_options_leaves_absolute_untouched
    #[test]
    fn anchor_inherited_path_options_leaves_absolute_untouched() {
        let temp = tempdir().unwrap();
        let config_dir = temp.path().join("base");
        std::fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("tsconfig.json");

        let abs_path = "/absolute/elsewhere".to_string();
        let mut config = TsConfig {
            compiler_options: Some(CompilerOptions {
                base_url: Some(abs_path.clone()),
                ..Default::default()
            }),
            ..Default::default()
        };

        anchor_inherited_path_options(&mut config, &config_path);

        let opts = config.compiler_options.unwrap();
        assert_eq!(opts.base_url.as_deref(), Some(abs_path.as_str()));
    }
// TSZ_INLINE_TEST_END 056b31007ac3405de071d7a876c04942833f97d81bea48bf34cea391d93df54f

// TSZ_INLINE_TEST_BEGIN c5de8a69c8f94d8f7e2e65d1617093c147ca233788d0e435187edd0e248e7637 1039 resolve_extends_path_relative
    #[test]
    fn resolve_extends_path_relative() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("p");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("base.json"), "{}").unwrap();
        let child = project.join("tsconfig.json");

        // Extensionless relative specifier resolves by appending `.json`.
        let resolved = resolve_extends_path(&child, "./base").unwrap();
        assert_eq!(
            resolved,
            ExtendsResolution::Found(project.join("base.json"))
        );
    }
// TSZ_INLINE_TEST_END c5de8a69c8f94d8f7e2e65d1617093c147ca233788d0e435187edd0e248e7637

// TSZ_INLINE_TEST_BEGIN c0213b97522d9a8fd1a85a72ca94c89cd2b6711f6bfe0d62a6c48eb1e12385d8 1055 resolve_extends_path_relative_missing_extensionless_is_not_found
    #[test]
    fn resolve_extends_path_relative_missing_extensionless_is_not_found() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("p");
        std::fs::create_dir_all(&project).unwrap();
        let child = project.join("tsconfig.json");

        // An extensionless relative specifier whose `.json`-appended candidate
        // is also absent is a plain miss: the caller emits the specifier-
        // anchored TS6053, never TS5083.
        let resolved = resolve_extends_path(&child, "./missing").unwrap();
        assert_eq!(resolved, ExtendsResolution::NotFound);
    }
// TSZ_INLINE_TEST_END c0213b97522d9a8fd1a85a72ca94c89cd2b6711f6bfe0d62a6c48eb1e12385d8

// TSZ_INLINE_TEST_BEGIN d7346ef89f38765c69b148b8c9b416b848a48a47000623ef2067bb0456dd0b07 1069 resolve_extends_path_relative_missing_json_is_unreadable
    #[test]
    fn resolve_extends_path_relative_missing_json_is_unreadable() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("p");
        std::fs::create_dir_all(&project).unwrap();
        let child = project.join("tsconfig.json");

        // A relative specifier that already ends in `.json` and does not exist
        // resolves to a concrete-but-unreadable path: `tsc` returns it unchecked
        // and the file read fails with TS5083 anchored at the normalized path.
        let resolved = resolve_extends_path(&child, "./nope.json").unwrap();
        assert_eq!(
            resolved,
            ExtendsResolution::Unreadable(project.join("nope.json"))
        );
    }
// TSZ_INLINE_TEST_END d7346ef89f38765c69b148b8c9b416b848a48a47000623ef2067bb0456dd0b07

// TSZ_INLINE_TEST_BEGIN 33832e7e0647d46c7e25352efd4c4fa67bb401d1a1df34ee9a39d581963efe85 1086 resolve_extends_path_relative_missing_json_normalizes_parent_segments
    #[test]
    fn resolve_extends_path_relative_missing_json_normalizes_parent_segments() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("p");
        let nested = project.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        let child = nested.join("tsconfig.json");

        // The TS5083 path is lexically normalized: `../nope.json` collapses to
        // the sibling directory, never a `<dir>/../nope.json` spelling.
        let resolved = resolve_extends_path(&child, "../nope.json").unwrap();
        assert_eq!(
            resolved,
            ExtendsResolution::Unreadable(project.join("nope.json"))
        );
    }
// TSZ_INLINE_TEST_END 33832e7e0647d46c7e25352efd4c4fa67bb401d1a1df34ee9a39d581963efe85

// TSZ_INLINE_TEST_BEGIN 6fa6b8ea511d5a488ee8302e3da531a6cbff915c97506df6d1ddb4bf45965204 1103 resolve_extends_path_relative_missing_non_json_extension_is_not_found
    #[test]
    fn resolve_extends_path_relative_missing_non_json_extension_is_not_found() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("p");
        std::fs::create_dir_all(&project).unwrap();
        let child = project.join("tsconfig.json");

        // A non-`.json` extension is treated like an extensionless specifier
        // (`tsc` appends `.json` and re-probes), so a miss is TS6053 — TS5083 is
        // reserved for `.json` specifiers only.
        let resolved = resolve_extends_path(&child, "./nope.txt").unwrap();
        assert_eq!(resolved, ExtendsResolution::NotFound);
    }
// TSZ_INLINE_TEST_END 6fa6b8ea511d5a488ee8302e3da531a6cbff915c97506df6d1ddb4bf45965204

// TSZ_INLINE_TEST_BEGIN 6550b5315a6163f8d6a9cb699e1450c502d992ec77d2b8f96359fbd9029a65d1 1117 resolve_extends_path_absolute
    #[test]
    fn resolve_extends_path_absolute() {
        let temp = tempdir().unwrap();
        let abs = temp.path().join("abs.json");
        std::fs::write(&abs, "{}").unwrap();
        let project = temp.path().join("p");
        std::fs::create_dir_all(&project).unwrap();
        let child = project.join("tsconfig.json");

        let resolved = resolve_extends_path(&child, abs.to_string_lossy().as_ref()).unwrap();
        assert_eq!(resolved, ExtendsResolution::Found(abs));
    }
// TSZ_INLINE_TEST_END 6550b5315a6163f8d6a9cb699e1450c502d992ec77d2b8f96359fbd9029a65d1

// TSZ_INLINE_TEST_BEGIN aa114cb9db110b7ce396bbe6284c5c24e3237e603c4d7adb2d6107fcf9a1cb3e 1130 resolve_extends_path_absolute_missing_json_is_unreadable
    #[test]
    fn resolve_extends_path_absolute_missing_json_is_unreadable() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("p");
        std::fs::create_dir_all(&project).unwrap();
        let child = project.join("tsconfig.json");
        let abs_missing = temp.path().join("does").join("not").join("exist.json");

        // A rooted (absolute) `.json` specifier shares the relative branch's
        // TS5083 rule per `isRootedDiskPath` in `commandLineParser.ts`.
        let resolved =
            resolve_extends_path(&child, abs_missing.to_string_lossy().as_ref()).unwrap();
        assert_eq!(resolved, ExtendsResolution::Unreadable(abs_missing));
    }
// TSZ_INLINE_TEST_END aa114cb9db110b7ce396bbe6284c5c24e3237e603c4d7adb2d6107fcf9a1cb3e

// TSZ_INLINE_TEST_BEGIN e62a4fc875b580a680b23aec95071d50096af6f1846eb89ca9c82114400bdb5c 1145 resolve_extends_path_uses_node_modules_walk
    #[test]
    fn resolve_extends_path_uses_node_modules_walk() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("project");
        let pkg = project.join("node_modules").join("@scope").join("pkg");
        std::fs::create_dir_all(&pkg).unwrap();
        let base = pkg.join("recommended.json");
        std::fs::write(&base, "{}").unwrap();
        let child = project.join("tsconfig.json");

        let resolved = resolve_extends_path(&child, "@scope/pkg/recommended").unwrap();
        assert_eq!(resolved, ExtendsResolution::Found(base));
    }
// TSZ_INLINE_TEST_END e62a4fc875b580a680b23aec95071d50096af6f1846eb89ca9c82114400bdb5c

// TSZ_INLINE_TEST_BEGIN 2b77d971310c8f2964f07e32e764a682c3f840f2f5b8d46816fecbcff4ff4ecd 1159 resolve_extends_path_uses_node_modules_walk_with_explicit_json
    #[test]
    fn resolve_extends_path_uses_node_modules_walk_with_explicit_json() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("project");
        let pkg = project.join("node_modules").join("@scope").join("pkg");
        std::fs::create_dir_all(&pkg).unwrap();
        let base = pkg.join("tsconfig.base.json");
        std::fs::write(&base, "{}").unwrap();
        let child = project.join("tsconfig.json");

        let resolved = resolve_extends_path(&child, "@scope/pkg/tsconfig.base.json").unwrap();
        assert_eq!(resolved, ExtendsResolution::Found(base));
    }
// TSZ_INLINE_TEST_END 2b77d971310c8f2964f07e32e764a682c3f840f2f5b8d46816fecbcff4ff4ecd

// TSZ_INLINE_TEST_BEGIN 06018523e188c0c95389b9dabbc2aee4300b66be5a1b4bacd36c08193ca090fe 1173 resolve_extends_path_node_modules_walk_from_nested_dir
    #[test]
    fn resolve_extends_path_node_modules_walk_from_nested_dir() {
        // The package config lives in the workspace-root `node_modules`, while
        // the consuming config is several directories down (the directus /
        // rocketchat / cal-com monorepo shape). The walk must climb ancestors.
        let temp = tempdir().unwrap();
        let root = temp.path().join("repo");
        let pkg = root.join("node_modules").join("@scope").join("tsconfig");
        std::fs::create_dir_all(&pkg).unwrap();
        let base = pkg.join("node22.json");
        std::fs::write(&base, "{}").unwrap();
        let nested = root.join("apps").join("web");
        std::fs::create_dir_all(&nested).unwrap();
        let child = nested.join("tsconfig.json");

        let resolved = resolve_extends_path(&child, "@scope/tsconfig/node22.json").unwrap();
        assert_eq!(resolved, ExtendsResolution::Found(base));
    }
// TSZ_INLINE_TEST_END 06018523e188c0c95389b9dabbc2aee4300b66be5a1b4bacd36c08193ca090fe

// TSZ_INLINE_TEST_BEGIN 85333b172972e68d136a9c8e4d7f8592caa56258ea843211f2fae3c6ccc29fe3 1192 resolve_extends_path_bare_package_uses_root_tsconfig
    #[test]
    fn resolve_extends_path_bare_package_uses_root_tsconfig() {
        // A bare package specifier resolves to the package root's tsconfig.json.
        let temp = tempdir().unwrap();
        let project = temp.path().join("project");
        let pkg = project.join("node_modules").join("shared-config");
        std::fs::create_dir_all(&pkg).unwrap();
        let base = pkg.join("tsconfig.json");
        std::fs::write(&base, "{}").unwrap();
        let child = project.join("tsconfig.json");

        let resolved = resolve_extends_path(&child, "shared-config").unwrap();
        assert_eq!(resolved, ExtendsResolution::Found(base));
    }
// TSZ_INLINE_TEST_END 85333b172972e68d136a9c8e4d7f8592caa56258ea843211f2fae3c6ccc29fe3

// TSZ_INLINE_TEST_BEGIN c31119d14de0ab661fbdb6cdf8cf01b68eacc87c6c9435c6dda7798b46e7cee2 1207 resolve_extends_path_missing_package_is_none
    #[test]
    fn resolve_extends_path_missing_package_is_none() {
        // A non-relative specifier whose package is not installed resolves to a
        // miss (caller emits TS6053), never to a literal config-dir path-join.
        let temp = tempdir().unwrap();
        let project = temp.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        let child = project.join("tsconfig.json");

        let resolved = resolve_extends_path(&child, "@scope/pkg/file.json").unwrap();
        assert_eq!(resolved, ExtendsResolution::NotFound);
    }
// TSZ_INLINE_TEST_END c31119d14de0ab661fbdb6cdf8cf01b68eacc87c6c9435c6dda7798b46e7cee2
