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
