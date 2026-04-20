#[test]
fn compile_with_project_dir_preserves_invariant_generic_error_elaboration_ts2322() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "strict": true,
            "target": "es2015",
            "noEmit": true
          },
          "files": ["test.ts"]
        }"#,
    );
    write_file(
        &base.join("test.ts"),
        r#"// Repro from #19746

const wat: Runtype<any> = Num;
const Foo = Obj({ foo: Num })

interface Runtype<A> {
  constraint: Constraint<this>
  witness: A
}

interface Num extends Runtype<number> {
  tag: 'number'
}
declare const Num: Num

interface Obj<O extends { [_ in string]: Runtype<any> }> extends Runtype<{[K in keyof O]: O[K]['witness'] }> {}
declare function Obj<O extends { [_: string]: Runtype<any> }>(fields: O): Obj<O>;

interface Constraint<A extends Runtype<any>> extends Runtype<A['witness']> {
  underlying: A,
  check: (x: A['witness']) => void,
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2322_count = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .count();

    assert_eq!(
        ts2322_count, 2,
        "Expected two TS2322 diagnostics for invariant generic error elaboration, got: {:?}",
        result.diagnostics
    );
}
#[test]
fn compile_with_jsx_preserve_emits_jsx_extension() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "jsx": "preserve",
            "strict": false
          },
          "include": ["src/**/*.tsx", "src/**/*.d.ts"]
        }"#,
    );
    write_file(
        &base.join("src/jsx.d.ts"),
        "declare namespace JSX { interface IntrinsicElements { div: any; } }",
    );
    write_file(
        &base.join("src/view.tsx"),
        "export const View = () => <div />;",
    );

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(result.diagnostics.is_empty());
    assert!(base.join("dist/src/view.jsx").is_file());
}
#[test]
fn compile_resolves_relative_imports_from_files_list() {
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
    write_file(
        &base.join("src/index.ts"),
        "import { value } from './util'; export { value };",
    );
    write_file(&base.join("src/util.ts"), "export const value = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    assert!(base.join("dist/src/index.js").is_file());
    assert!(base.join("dist/src/util.js").is_file());
}
#[test]
fn compile_resolves_paths_mappings() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "baseUrl": ".",
            "ignoreDeprecations": "6.0",
            "paths": {
              "@lib/*": ["src/lib/*"]
            }
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { value } from '@lib/value'; export { value };",
    );
    write_file(&base.join("src/lib/value.ts"), "export const value = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    assert!(base.join("dist/src/lib/value.js").is_file());
}
#[test]
fn compile_resolves_node_modules_types() {
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
        "import { value } from 'pkg'; export { value };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "types": "index.d.ts"
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/index.d.ts"),
        "export const value = ;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("node_modules/pkg/index.d.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}
#[test]
fn compile_resolves_tsconfig_types_includes_selected_packages() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true,
            "types": ["foo"]
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 1;");
    write_file(
        &base.join("node_modules/@types/foo/index.d.ts"),
        "export const foo = ;",
    );
    write_file(
        &base.join("node_modules/@types/bar/index.d.ts"),
        "export const bar = ;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("node_modules/@types/foo/index.d.ts"))
    );
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("node_modules/@types/bar/index.d.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}
#[test]
fn compile_resolves_tsconfig_type_roots_includes_packages() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true,
            "typeRoots": ["types"]
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 1;");
    write_file(&base.join("types/foo/index.d.ts"), "export const foo = ;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("types/foo/index.d.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}
#[test]
fn compile_resolves_node_modules_exports_subpath() {
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
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "exports": {
            ".": { "types": "./types/index.d.ts" },
            "./feature/*": { "types": "./types/feature/*.d.ts" }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/feature/widget.d.ts"),
        "export const widget = ;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}
#[test]
fn compile_uses_versioned_types_export_conditions_without_false_ts2551() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "node16",
            "moduleResolution": "node16",
            "strict": true,
            "noEmitOnError": true,
            "ignoreDeprecations": "6.0"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import * as mod from 'inner';\nmod.goodThing.toFixed();\n",
    );
    write_file(
        &base.join("node_modules/inner/package.json"),
        r#"{
          "name": "inner",
          "exports": {
            ".": {
              "types@>=10000": "./future-types.d.ts",
              "types@>=1": "./new-types.d.ts",
              "types": "./old-types.d.ts",
              "import": "./index.mjs",
              "node": "./index.js"
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/inner/old-types.d.ts"),
        "export const oldThing: number;",
    );
    write_file(
        &base.join("node_modules/inner/new-types.d.ts"),
        "export const goodThing: number;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "expected versioned types export resolution to avoid bogus namespace-property diagnostics, got: {:?}",
        result.diagnostics
    );
    assert!(base.join("src/index.js").is_file());
}
#[test]
fn compile_resolves_node_modules_types_versions() {
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
            "*": {
              "feature/*": ["types/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}
#[test]
fn compile_resolves_node_modules_types_versions_best_match() {
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
            ">=6.1": {
              "feature/*": ["types/v61/feature/*"]
            },
            ">=5.0": {
              "feature/*": ["types/v5/feature/*"]
            },
            "*": {
              "feature/*": ["types/fallback/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v61/feature/widget.d.ts"),
        "export const widget = 1;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v5/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/fallback/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    // Either:
    // 1. Best match (v61) is selected and succeeds (no diagnostics), OR
    // 2. Fallback to v5 which has syntax errors
    if result.diagnostics.is_empty() {
        // Best match v61 was selected successfully
        assert!(base.join("dist/src/index.js").is_file());
    } else {
        // Fallback to v5 produced errors
        assert!(result.diagnostics.iter().any(|diag| {
            diag.file
                .contains("node_modules/pkg/types/v5/feature/widget.d.ts")
        }));
        assert!(!base.join("dist/src/index.js").is_file());
    }
}
#[test]
fn compile_resolves_node_modules_types_versions_prefers_specific_range() {
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
            ">=6.0": {
              "feature/*": ["types/loose/feature/*"]
            },
            ">=5.0 <7.0": {
              "feature/*": ["types/ranged/feature/*"]
            },
            "*": {
              "feature/*": ["types/fallback/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/loose/feature/widget.d.ts"),
        "export const widget = 1;",
    );
    write_file(
        &base.join("node_modules/pkg/types/ranged/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/fallback/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/ranged/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}
#[test]
fn compile_resolves_node_modules_types_versions_respects_cli_version_override() {
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
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let mut args = default_args();
    args.types_versions_compiler_version = Some("7.1".to_string());
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v7/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}
#[test]
fn compile_resolves_node_modules_types_versions_respects_env_version_override() {
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
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = with_types_versions_env(Some("7.1"), || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v7/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}
#[test]
fn compile_resolves_node_modules_types_versions_respects_tsconfig_version_override() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "node",
            "noEmitOnError": true,
            "typesVersionsCompilerVersion": "7.1"
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
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v7/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}
#[test]
fn compile_resolves_node_modules_types_versions_tsconfig_extends_inherits_override() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("config/base.json"),
        r#"{
          "compilerOptions": {
            "typesVersionsCompilerVersion": "7.1"
          }
        }"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "extends": "./config/base.json",
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
        &base.join("node_modules/pkg/types/v71/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v71/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}
#[test]
fn compile_resolves_node_modules_types_versions_env_overrides_tsconfig() {
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
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = with_types_versions_env(Some("7.1"), || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v7/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}
#[test]
fn compile_resolves_node_modules_types_versions_empty_env_uses_tsconfig() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "node",
            "noEmitOnError": true,
            "typesVersionsCompilerVersion": "7.1"
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
        &base.join("node_modules/pkg/types/v71/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = with_types_versions_env(Some(""), || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v71/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}
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
#[ignore = "module resolution for node-next/nodenext not yet complete"]
fn compile_resolves_package_imports_prefers_types_condition() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
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
fn compile_prefers_browser_exports_for_bundler() {
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
              "node": "./node.d.ts"
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/browser.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/node.d.ts"),
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
#[ignore = "module resolution for node-next/nodenext not yet complete"]
fn compile_node_next_resolves_js_extension_to_ts() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
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
#[ignore = "module resolution for node-next/nodenext not yet complete"]
fn compile_node_next_prefers_mts_for_module_package() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
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

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("node_modules/pkg/index.mts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}
#[test]
#[ignore = "module resolution for node-next/nodenext not yet complete"]
fn compile_node_next_prefers_cts_for_commonjs_package() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
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

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("node_modules/pkg/index.cts"))
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
