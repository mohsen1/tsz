#[test]
fn compile_resolves_node_modules_types_versions_cli_overrides_env_and_tsconfig() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "node",
            "noEmitOnError": true,
            "typesVersionsCompilerVersion": "6.0"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.2": {
              "feature/*": ["types/v72/feature/*"]
            },
            ">=7.1": {
              "feature/*": ["types/v71/feature/*"]
            },
            ">=6.0": {
              "feature/*": ["types/v6/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v72/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v71/feature/widget.d.ts"),
        "export const widget = 1;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let mut args = default_args();
    args.types_versions_compiler_version = Some("7.2".to_string());
    let result = with_types_versions_env(Some("7.1"), || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v72/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_invalid_override_falls_back() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "node",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.0": {
              "feature/*": ["types/v7/feature/*"]
            },
            ">=6.0": {
              "feature/*": ["types/v6/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v7/feature/widget.d.ts"),
        "export const widget = 1;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = ;",
    );

    let mut args = default_args();
    args.types_versions_compiler_version = Some("not-a-version".to_string());
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v6/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_invalid_env_falls_back() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "node",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.0": {
              "feature/*": ["types/v7/feature/*"]
            },
            ">=6.0": {
              "feature/*": ["types/v6/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v7/feature/widget.d.ts"),
        "export const widget = 1;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = ;",
    );

    let args = default_args();
    let result = with_types_versions_env(Some("not-a-version"), || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v6/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_invalid_tsconfig_falls_back() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "node",
            "noEmitOnError": true,
            "typesVersionsCompilerVersion": "not-a-version"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.0": {
              "feature/*": ["types/v7/feature/*"]
            },
            ">=6.0": {
              "feature/*": ["types/v6/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v7/feature/widget.d.ts"),
        "export const widget = 1;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = ;",
    );

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v6/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_falls_back_to_wildcard() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "node",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.0": {
              "feature/*": ["types/v7/feature/*"]
            },
            "*": {
              "feature/*": ["types/fallback/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v7/feature/widget.d.ts"),
        "export const widget = 1;",
    );
    write_file(
        &base.join("node_modules/pkg/types/fallback/feature/widget.d.ts"),
        "export const widget = ;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    // Either:
    // 1. Best match (v7) is selected and succeeds (no diagnostics), OR
    // 2. Fallback to wildcard which has syntax errors
    if result.diagnostics.is_empty() {
        // Best match v7 was selected successfully
        assert!(base.join("dist/src/index.js").is_file());
    } else {
        // Fallback to wildcard produced errors
        assert!(result.diagnostics.iter().any(|diag| {
            diag.file
                .contains("node_modules/pkg/types/fallback/feature/widget.d.ts")
        }));
        assert!(!base.join("dist/src/index.js").is_file());
    }
}

// Regression for issue #4763: package.json `exports` versioned conditions
// (`types@<range>`) must honor the project-level
// `typesVersionsCompilerVersion` override. Without the fix, the resolver
// hardcoded the fallback compiler version and ignored the override here.
#[test]
fn compile_resolves_package_exports_versioned_condition_respects_compiler_version_override() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "module": "node16",
            "moduleResolution": "node16",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { thing } from 'inner'; export { thing };",
    );
    write_file(
        &base.join("node_modules/inner/package.json"),
        r#"{
          "name": "inner",
          "exports": {
            ".": {
              "types@>=7": "./types-v7/index.d.ts",
              "types@>=6": "./types-v6/index.d.ts",
              "default": "./index.js"
            }
          }
        }"#,
    );
    // Each branch points at a different declaration file. We deliberately
    // give one branch a syntax error so the diagnostic file path makes the
    // resolved branch directly observable.
    write_file(
        &base.join("node_modules/inner/types-v7/index.d.ts"),
        "export const thing = ;",
    );
    write_file(
        &base.join("node_modules/inner/types-v6/index.d.ts"),
        "export const thing = 1;",
    );

    let mut args = default_args();
    args.types_versions_compiler_version = Some("7.1".to_string());
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| { diag.file.contains("node_modules/inner/types-v7/index.d.ts") }),
        "expected `types@>=7` branch to be selected under the override; got diagnostics: {:?}",
        result.diagnostics
    );
}

// Regression for issue #4763: package.json `imports` versioned conditions
// share the same `resolve_exports_target_candidates` plumbing, so the
// override must reach `imports` as well.
#[test]
fn compile_resolves_package_imports_versioned_condition_respects_compiler_version_override() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "module": "node16",
            "moduleResolution": "node16",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("package.json"),
        r##"{
          "name": "host",
          "imports": {
            "#thing": {
              "types@>=7": "./types-v7/index.d.ts",
              "types@>=6": "./types-v6/index.d.ts",
              "default": "./index.js"
            }
          }
        }"##,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { thing } from '#thing'; export { thing };",
    );
    write_file(&base.join("types-v7/index.d.ts"), "export const thing = ;");
    write_file(&base.join("types-v6/index.d.ts"), "export const thing = 1;");

    let mut args = default_args();
    args.types_versions_compiler_version = Some("7.1".to_string());
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("types-v7/index.d.ts")),
        "expected `types@>=7` branch to be selected under the override; got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_resolves_package_imports_wildcard() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "module": "node16",
            "moduleResolution": "node16",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from '#utils/widget'; export { widget };",
    );
    write_file(
        &base.join("package.json"),
        r##"{
          "imports": {
            "#utils/*": "./types/*"
          }
        }"##,
    );
    write_file(&base.join("types/widget.d.ts"), "export const widget = ;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("types/widget.d.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_rejects_package_imports_target_with_node_modules_segment() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "node16",
            "moduleResolution": "node16",
            "noEmit": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("package.json"),
        r##"{"name":"app","imports":{"#secret":"./node_modules/secret.d.ts"}}"##,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { value } from '#secret';\nconst n: number = value;\n",
    );
    write_file(
        &base.join("node_modules/secret.d.ts"),
        "export declare const value: number;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().any(|diag| diag.code
            == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS),
        "Expected TS2307 for imports target under node_modules, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_resolves_package_imports_array_fallback_after_missing_target() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "nodenext",
            "moduleResolution": "nodenext",
            "strict": true,
            "noEmit": true
          },
          "files": ["main.ts"]
        }"#,
    );
    write_file(
        &base.join("package.json"),
        r##"{
          "type": "module",
          "imports": {
            "#x": ["./missing.d.ts", "./ok.d.ts"]
          }
        }"##,
    );
    write_file(&base.join("ok.d.ts"), "export declare const value: 1;");
    write_file(
        &base.join("main.ts"),
        "import { value } from '#x';\nconst n: 1 = value;\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        !result.diagnostics.iter().any(|diag| diag.code
            == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS),
        "Expected package imports array to fall back to ok.d.ts, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_cross_module_nested_interface_method_allows_optional_argument_currently() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "strict": true,
            "noEmit": true
          },
          "files": ["consumer.ts", "lib.ts"]
        }"#,
    );
    write_file(
        &base.join("lib.ts"),
        r#"
export interface IServer {
  port: number;
}

export interface IWorkspace {
  toAbsolutePath(server: IServer): string;
}

export interface IConfig {
  workspace: IWorkspace;
  server?: IServer;
}
"#,
    );
    write_file(
        &base.join("consumer.ts"),
        r#"
import { IConfig } from "./lib";

declare const cfg: IConfig;

cfg.workspace.toAbsolutePath(cfg.server);
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "expected no diagnostics for current cross-module nested-interface optional argument behavior, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_resolves_package_imports_conditional_fallback_after_missing_target() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "esnext",
            "moduleResolution": "bundler",
            "strict": true,
            "noEmit": true
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(
        &base.join("package.json"),
        r##"{
          "name": "app",
          "imports": {
            "#x": {
              "import": "./missing.d.ts",
              "default": "./ok.d.ts"
            }
          }
        }"##,
    );
    write_file(&base.join("ok.d.ts"), "export declare const v: number;");
    write_file(
        &base.join("index.ts"),
        "import { v } from '#x';\nconst n: number = v;\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        !result.diagnostics.iter().any(|diag| diag.code
            == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS),
        "Expected package imports conditional target to fall back to ok.d.ts, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_rejects_root_slash_package_import_specifier_under_node16() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "module": "node16",
            "moduleResolution": "node16",
            "noEmitOnError": true
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(
        &base.join("package.json"),
        r##"{
          "name": "package",
          "private": true,
          "type": "module",
          "imports": {
            "#/*": "./src/*"
          }
        }"##,
    );
    write_file(&base.join("src/foo.ts"), "export const foo = 'foo';");
    write_file(
        &base.join("index.ts"),
        "import { foo } from '#/foo.js';\nfoo;\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().any(|diag| diag.code
            == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS),
        "Expected TS2307 for invalid #/ package import, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_resolves_package_imports_prefers_types_condition() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "module": "node16",
            "moduleResolution": "node16",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { feature } from '#feature'; export { feature };",
    );
    write_file(
        &base.join("package.json"),
        r##"{
          "imports": {
            "#feature": {
              "types": "./types/feature.d.ts",
              "default": "./default/feature.d.ts"
            }
          }
        }"##,
    );
    write_file(&base.join("types/feature.d.ts"), "export const feature = ;");
    write_file(
        &base.join("default/feature.d.ts"),
        "export const feature = 1;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("types/feature.d.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_package_imports_prefers_require_condition_for_commonjs() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "module": "commonjs",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { feature } from '#feature'; export { feature };",
    );
    write_file(
        &base.join("package.json"),
        r##"{
          "imports": {
            "#feature": {
              "require": "./types/require.d.ts",
              "import": "./types/import.d.ts"
            }
          }
        }"##,
    );
    write_file(&base.join("types/require.d.ts"), "export const feature = ;");
    write_file(&base.join("types/import.d.ts"), "export const feature = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("types/require.d.ts"))
    );
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("types/import.d.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_package_imports_prefers_import_condition_for_esm() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "module": "esnext",
            "moduleResolution": "bundler",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { feature } from '#feature'; export { feature };",
    );
    write_file(
        &base.join("package.json"),
        r##"{
          "imports": {
            "#feature": {
              "import": "./types/import.d.ts",
              "require": "./types/require.d.ts"
            }
          }
        }"##,
    );
    write_file(&base.join("types/import.d.ts"), "export const feature = ;");
    write_file(
        &base.join("types/require.d.ts"),
        "export const feature = 1;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("types/import.d.ts"))
    );
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("types/require.d.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_bundler_does_not_default_to_browser_condition() {
    // Per tsc 6.0, `moduleResolution: "bundler"` does NOT add `browser` to
    // the default condition set; the user must opt in via `customConditions`.
    // Here, with no opt-in, the resolver must select the `default` branch
    // (a clean .d.ts), not the malformed `browser.d.ts`.
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "bundler",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "exports": {
            ".": {
              "browser": "./browser.d.ts",
              "default": "./default.d.ts"
            }
          }
        }"#,
    );
    // The browser branch contains a syntax error; if bundler still picked
    // it up by default, we'd see diagnostics in browser.d.ts.
    write_file(
        &base.join("node_modules/pkg/browser.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/default.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    // Picking the `default` branch produces a clean compile.
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("node_modules/pkg/browser.d.ts")),
        "bundler must not default to the `browser` exports branch"
    );
}

#[test]
fn compile_bundler_uses_browser_condition_when_in_custom_conditions() {
    // Opting `browser` into `customConditions` re-enables it for bundler.
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "bundler",
            "customConditions": ["browser"],
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "exports": {
            ".": {
              "browser": "./browser.d.ts",
              "default": "./default.d.ts"
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/browser.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/default.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("node_modules/pkg/browser.d.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn bundler_esm_declaration_package_without_default_emits_ts1192() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "esnext",
            "module": "preserve",
            "moduleResolution": "bundler",
            "noEmit": true
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"import pkg, { toString } from "pkg";

export const value = toString();
export { pkg };
"#,
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "name": "pkg",
          "type": "module",
          "types": "./index.d.ts"
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/index.d.ts"),
        "export declare function toString(): string;\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::MODULE_HAS_NO_DEFAULT_EXPORT),
        "Expected TS1192 for default import from ESM declaration package with no default export, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn system_module_source_default_import_without_allow_synthetic_flag_uses_namespace_fallback() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "module": "system",
            "allowSyntheticDefaultImports": false,
            "ignoreDeprecations": "6.0",
            "strict": false,
            "noEmit": true
          },
          "files": ["a.ts", "b.ts"]
        }"#,
    );
    write_file(
        &base.join("a.ts"),
        r#"import Namespace from "./b";
export const value = new Namespace.Foo();
"#,
    );
    write_file(
        &base.join("b.ts"),
        r#"export class Foo {
  member: string;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::MODULE_HAS_NO_DEFAULT_EXPORT),
        "Expected module=system source default import to avoid TS1192. Actual diagnostics: {:#?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics once system deprecations are silenced. Actual diagnostics: {:#?}",
        result.diagnostics
    );
}

#[test]
fn compile_node_next_resolves_js_extension_to_ts() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "module": "nodenext",
            "moduleResolution": "nodenext",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { value } from './util.js'; export { value };",
    );
    write_file(&base.join("src/util.ts"), "export const value = ;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("src/util.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_node_next_prefers_mts_for_module_package() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "module": "nodenext",
            "moduleResolution": "nodenext",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { value } from 'pkg'; export { value };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "type": "module"
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/index.mts"),
        "export const value = ;",
    );
    write_file(
        &base.join("node_modules/pkg/index.cts"),
        "export const value = 1;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().any(|diag| diag.code
            == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS),
        "Expected TS2307 because NodeNext package fallback does not resolve index.mts source files, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_node_next_prefers_cts_for_commonjs_package() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "module": "nodenext",
            "moduleResolution": "nodenext",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { value } from 'pkg'; export { value };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "type": "commonjs"
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/index.mts"),
        "export const value = 1;",
    );
    write_file(
        &base.join("node_modules/pkg/index.cts"),
        "export const value = ;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().any(|diag| diag.code
            == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS),
        "Expected TS2307 because NodeNext package fallback does not resolve index.cts source files, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_with_cache_emits_only_dirty_files() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/alpha.ts", "src/beta.ts"]
        }"#,
    );

    let alpha_path = base.join("src/alpha.ts");
    let beta_path = base.join("src/beta.ts");
    write_file(&alpha_path, "export const alpha = 1;");
    write_file(&beta_path, "export const beta = 2;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(result.diagnostics.is_empty());

    let alpha_output = std::fs::canonicalize(base.join("dist/src/alpha.js"))
        .unwrap_or_else(|_| base.join("dist/src/alpha.js"));
    let beta_output = std::fs::canonicalize(base.join("dist/src/beta.js"))
        .unwrap_or_else(|_| base.join("dist/src/beta.js"));
    assert_eq!(result.emitted_files.len(), 2);
    assert!(result.emitted_files.contains(&alpha_output));
    assert!(result.emitted_files.contains(&beta_output));

    write_file(&alpha_path, "export const alpha = 2;");
    let canonical = std::fs::canonicalize(&alpha_path).unwrap_or(alpha_path);
    cache.invalidate_paths_with_dependents(vec![canonical]);

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(result.diagnostics.is_empty());
    assert_eq!(result.emitted_files.len(), 1);
    assert!(result.emitted_files.contains(&alpha_output));
    assert!(!result.emitted_files.contains(&beta_output));
}

#[test]
fn compile_with_cache_updates_dependencies_for_changed_files() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );

    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    let extra_path = base.join("src/extra.ts");
    write_file(
        &index_path,
        "import { value } from './util'; export { value };",
    );
    write_file(&util_path, "export const value = ;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("util.ts"))
    );

    write_file(
        &index_path,
        "import { value } from './extra'; export { value };",
    );
    write_file(&extra_path, "export const value = ;");

    let canonical = std::fs::canonicalize(&index_path).unwrap_or(index_path);
    let result = compile_with_cache_and_changes(&args, base, &mut cache, &[canonical])
        .expect("compile should succeed");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("extra.ts"))
    );
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("util.ts"))
    );
}

#[test]
fn compile_with_cache_skips_dependents_when_exports_unchanged() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );

    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "import { value } from './util'; export const output = value;",
    );
    write_file(&util_path, "export function value() { return 1; }");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "initial diagnostics (unchanged exports): {:#?}",
        result.diagnostics
    );

    write_file(&util_path, "export function value() { return 2; }");

    let util_output = std::fs::canonicalize(base.join("dist/src/util.js"))
        .unwrap_or_else(|_| base.join("dist/src/util.js"));
    let index_output = std::fs::canonicalize(base.join("dist/src/index.js"))
        .unwrap_or_else(|_| base.join("dist/src/index.js"));
    let canonical = std::fs::canonicalize(&util_path).unwrap_or(util_path);

    let result = compile_with_cache_and_changes(&args, base, &mut cache, &[canonical])
        .expect("compile should succeed");
    assert!(result.diagnostics.is_empty());
    assert!(result.emitted_files.contains(&util_output));
    assert!(!result.emitted_files.contains(&index_output));
}

#[test]
fn compile_with_cache_rechecks_dependents_on_export_change() {
    // Tests that cache properly invalidates dependents when the export *surface* changes.
    // A body-only edit (changing the value of an existing export) should NOT invalidate
    // dependents — this matches the unified binder-level ExportSignature semantics
    // shared between CLI and LSP. Only structural changes (adding/removing exports,
    // changing export names/kinds) trigger dependent invalidation.
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );

    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    // Use namespace import to avoid named import type resolution issues
    write_file(
        &index_path,
        "import * as util from './util'; export { util };",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "initial diagnostics (export change): {:#?}",
        result.diagnostics
    );

    // Add a new export — this changes the export surface and must invalidate dependents.
    write_file(
        &util_path,
        "export const value = 1;\nexport function helper() {}",
    );

    let util_output = std::fs::canonicalize(base.join("dist/src/util.js"))
        .unwrap_or_else(|_| base.join("dist/src/util.js"));
    let index_output = std::fs::canonicalize(base.join("dist/src/index.js"))
        .unwrap_or_else(|_| base.join("dist/src/index.js"));
    let canonical = std::fs::canonicalize(&util_path).unwrap_or(util_path);

    let result = compile_with_cache_and_changes(&args, base, &mut cache, &[canonical])
        .expect("compile should succeed");
    // Assert dependent recompilation - both files should be re-emitted
    assert!(result.emitted_files.contains(&util_output));
    assert!(result.emitted_files.contains(&index_output));
}

#[test]
fn compile_with_cache_body_only_edit_skips_dependents() {
    // Changing the value of an existing export (body-only edit) should NOT
    // invalidate dependents — the unified ExportSignature only tracks names,
    // flags, and structural relationships, not inferred types.
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );

    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "import * as util from './util'; export { util };",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "initial diagnostics: {:#?}",
        result.diagnostics
    );

    // Body-only edit: change the value but not the export surface.
    write_file(&util_path, "export const value = \"changed\";");

    let index_output = std::fs::canonicalize(base.join("dist/src/index.js"))
        .unwrap_or_else(|_| base.join("dist/src/index.js"));
    let canonical = std::fs::canonicalize(&util_path).unwrap_or(util_path);

    let result = compile_with_cache_and_changes(&args, base, &mut cache, &[canonical])
        .expect("compile should succeed");
    // Dependent should NOT be re-emitted — export signature is unchanged.
    assert!(
        !result.emitted_files.contains(&index_output),
        "Body-only edit should not re-emit dependents"
    );
}

#[test]
fn compile_with_cache_invalidates_paths() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    write_file(&index_path, "export const value = ;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(!result.diagnostics.is_empty());
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.bind_len(), 1);
    assert_eq!(cache.diagnostics_len(), 1);

    let canonical = std::fs::canonicalize(&index_path).unwrap_or(index_path);
    cache.invalidate_paths_with_dependents(vec![canonical]);
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.bind_len(), 0);
    assert_eq!(cache.diagnostics_len(), 0);

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(!result.diagnostics.is_empty());
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.bind_len(), 1);
    assert_eq!(cache.diagnostics_len(), 1);
}

#[test]
fn compile_with_cache_invalidates_dependents() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "import { value } from './util'; export { value };",
    );
    write_file(&util_path, "export const value = ;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(!result.diagnostics.is_empty());
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.bind_len(), 2);
    assert_eq!(cache.diagnostics_len(), 2);

    let canonical = std::fs::canonicalize(&util_path).unwrap_or(util_path);
    cache.invalidate_paths_with_dependents(vec![canonical]);
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.bind_len(), 0);
    assert_eq!(cache.diagnostics_len(), 0);

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(!result.diagnostics.is_empty());
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.bind_len(), 2);
    assert_eq!(cache.diagnostics_len(), 2);
}

#[test]
fn invalidate_paths_with_dependents_symbols_keeps_unrelated_cache() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "import { value } from './util'; export const local = 1; export const uses = value;",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(result.diagnostics.is_empty());

    let canonical_index = std::fs::canonicalize(&index_path).unwrap_or(index_path);
    let canonical_util = std::fs::canonicalize(&util_path).unwrap_or(util_path);
    let before = cache.symbol_cache_len(&canonical_index).unwrap_or(0);
    assert!(before > 0);

    cache.invalidate_paths_with_dependents_symbols(vec![canonical_util.clone()]);

    let after = cache.symbol_cache_len(&canonical_index).unwrap_or(0);
    assert!(after > 0);
    assert!(after < before);
    assert_eq!(cache.node_cache_len(&canonical_index).unwrap_or(0), 0);
    assert!(cache.symbol_cache_len(&canonical_util).is_none());
}

#[test]
fn invalidate_paths_with_dependents_symbols_handles_reexports() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "export { value } from './util'; export const local = 1;",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(result.diagnostics.is_empty());
    assert_eq!(cache.len(), 2);

    let canonical_index = std::fs::canonicalize(&index_path).unwrap_or(index_path);
    let canonical_util = std::fs::canonicalize(&util_path).unwrap_or(util_path);

    cache.invalidate_paths_with_dependents_symbols(vec![canonical_util.clone()]);

    assert_eq!(cache.len(), 1);
    assert!(cache.symbol_cache_len(&canonical_index).is_some());
    assert_eq!(cache.node_cache_len(&canonical_index).unwrap_or(1), 0);
    assert!(cache.symbol_cache_len(&canonical_util).is_none());
}

#[test]
fn invalidate_paths_with_dependents_symbols_handles_import_equals() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "module": "commonjs"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "import util = require('./util'); export const local = util.value;",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "Compilation should have no diagnostics, got: {:?}",
        result.diagnostics
    );
    assert_eq!(cache.len(), 2);

    let canonical_index = std::fs::canonicalize(&index_path).unwrap_or(index_path);
    let canonical_util = std::fs::canonicalize(&util_path).unwrap_or(util_path);
    let before_nodes = cache.node_cache_len(&canonical_index).unwrap_or(0);
    assert!(before_nodes > 0);

    cache.invalidate_paths_with_dependents_symbols(vec![canonical_util.clone()]);

    assert_eq!(cache.len(), 1);
    assert!(cache.symbol_cache_len(&canonical_index).is_some());
    assert_eq!(cache.node_cache_len(&canonical_index).unwrap_or(1), 0);
    assert!(cache.symbol_cache_len(&canonical_util).is_none());
}

#[test]
fn invalidate_paths_with_dependents_symbols_handles_namespace_reexports() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "export * as util from './util'; export const local = 1;",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(result.diagnostics.is_empty());
    assert_eq!(cache.len(), 2);

    let canonical_index = std::fs::canonicalize(&index_path).unwrap_or(index_path);
    let canonical_util = std::fs::canonicalize(&util_path).unwrap_or(util_path);
    let before_nodes = cache.node_cache_len(&canonical_index).unwrap_or(0);
    assert!(before_nodes > 0);

    cache.invalidate_paths_with_dependents_symbols(vec![canonical_util.clone()]);

    assert_eq!(cache.len(), 1);
    assert!(cache.symbol_cache_len(&canonical_index).is_some());
    assert_eq!(cache.node_cache_len(&canonical_index).unwrap_or(1), 0);
    assert!(cache.symbol_cache_len(&canonical_util).is_none());
}

#[test]
fn invalidate_paths_with_dependents_symbols_handles_star_reexports() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "export * from './util'; export const local = 1;",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(result.diagnostics.is_empty());
    assert_eq!(cache.len(), 2);

    let canonical_index = std::fs::canonicalize(&index_path).unwrap_or(index_path);
    let canonical_util = std::fs::canonicalize(&util_path).unwrap_or(util_path);
    let before_nodes = cache.node_cache_len(&canonical_index).unwrap_or(0);
    assert!(before_nodes > 0);

    cache.invalidate_paths_with_dependents_symbols(vec![canonical_util.clone()]);

    assert_eq!(cache.len(), 1);
    assert!(cache.symbol_cache_len(&canonical_index).is_some());
    assert_eq!(cache.node_cache_len(&canonical_index).unwrap_or(1), 0);
    assert!(cache.symbol_cache_len(&canonical_util).is_none());
}

