use super::*;

#[test]
fn package_specifier_prefers_package_root_for_commonjs_main_module_entrypoint() {
    let mut project = Project::new();
    project.set_file(
        "/node_modules/pkg/package.json".to_string(),
        r#"{
  "name": "pkg",
  "version": "1.0.0",
  "main": "lib",
  "module": "lib"
}"#
        .to_string(),
    );
    project.set_file(
        "/node_modules/pkg/lib/index.js".to_string(),
        "export function foo() {}".to_string(),
    );

    assert_eq!(
        project.package_specifier_from_node_modules("/node_modules/pkg/lib/index.js"),
        Some("pkg".to_string())
    );
}

#[test]
fn package_specifier_uses_subpath_for_type_module_main_entrypoint() {
    let mut project = Project::new();
    project.set_file(
        "/node_modules/pkg/package.json".to_string(),
        r#"{
  "name": "pkg",
  "version": "1.0.0",
  "main": "lib",
  "type": "module"
}"#
        .to_string(),
    );
    project.set_file(
        "/node_modules/pkg/lib/index.js".to_string(),
        "export function foo() {}".to_string(),
    );

    assert_eq!(
        project.package_specifier_from_node_modules("/node_modules/pkg/lib/index.js"),
        Some("pkg/lib".to_string())
    );
}

#[test]
fn package_specifier_maps_dmts_to_mjs_without_collapsing_to_package_root() {
    let mut project = Project::new();
    project.set_file(
        "/node_modules/pkg/package.json".to_string(),
        r#"{
  "name": "pkg",
  "version": "1.0.0",
  "main": "lib"
}"#
        .to_string(),
    );
    project.set_file(
        "/node_modules/pkg/lib/index.d.mts".to_string(),
        "export declare function foo(): any;".to_string(),
    );

    assert_eq!(
        project.package_specifier_from_node_modules("/node_modules/pkg/lib/index.d.mts"),
        Some("pkg/lib/index.mjs".to_string())
    );
}

#[test]
fn package_specifier_maps_dcts_to_cjs_when_no_package_json_exists() {
    let mut project = Project::new();
    project.set_file(
        "/node_modules/lit/index.d.cts".to_string(),
        "export declare function customElement(name: string): any;".to_string(),
    );

    assert_eq!(
        project.package_specifier_from_node_modules("/node_modules/lit/index.d.cts"),
        Some("lit/index.cjs".to_string())
    );
}

#[test]
fn package_specifier_collapses_extensionless_root_index_to_package_name() {
    let mut project = Project::new();
    project.set_file(
        "/node_modules/bar/index.d.ts".to_string(),
        "export declare const fromBar: number;".to_string(),
    );

    assert_eq!(
        project.package_specifier_from_node_modules("/node_modules/bar/index.d.ts"),
        Some("bar".to_string())
    );
}

#[test]
fn workspace_dependency_uses_declared_package_name_specifier() {
    let mut project = Project::new();
    project.set_file(
        "/home/src/workspaces/project/packages/common/package.json".to_string(),
        r#"{
  "name": "@company/common",
  "version": "1.0.0",
  "main": "./lib/index.tsx"
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/packages/common/lib/index.tsx".to_string(),
        "export function Tooltip() {}".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/packages/app/package.json".to_string(),
        r#"{
  "name": "@company/app",
  "version": "1.0.0",
  "dependencies": {
    "@company/common": "1.0.0"
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/packages/app/lib/index.ts".to_string(),
        "Tooltip".to_string(),
    );

    let specifiers = project.auto_import_module_specifiers_from_files(
        "/home/src/workspaces/project/packages/app/lib/index.ts",
        "/home/src/workspaces/project/packages/common/lib/index.tsx",
    );
    assert!(
        specifiers
            .iter()
            .any(|specifier| specifier == "@company/common"),
        "expected @company/common specifier from workspace dependency, got {specifiers:?}"
    );
}

#[test]
fn workspace_file_dependency_alias_uses_dependency_name_specifier() {
    let mut project = Project::new();
    project.set_file(
        "/home/src/workspaces/solution/packages/utils/package.json".to_string(),
        r#"{
  "name": "utils",
  "version": "1.0.0",
  "exports": "./dist/index.js"
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/solution/packages/utils/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "lib": ["es5"],
    "composite": true,
    "module": "nodenext",
    "rootDir": "src",
    "outDir": "dist"
  },
  "include": ["src"]
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/solution/packages/utils/src/index.ts".to_string(),
        "export function gainUtility() { return 0; }".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/solution/packages/web/package.json".to_string(),
        r#"{
  "name": "web",
  "version": "1.0.0",
  "dependencies": {
    "@monorepo/utils": "file:../utils"
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/solution/packages/web/src/index.ts".to_string(),
        "gainUtility".to_string(),
    );

    let specifiers = project.auto_import_module_specifiers_from_files(
        "/home/src/workspaces/solution/packages/web/src/index.ts",
        "/home/src/workspaces/solution/packages/utils/src/index.ts",
    );
    assert!(
        specifiers
            .iter()
            .any(|specifier| specifier == "@monorepo/utils"),
        "expected @monorepo/utils specifier from file-linked dependency alias, got {specifiers:?}"
    );
}

#[test]
fn project_relative_preference_still_prefers_workspace_dependency_bare_specifier() {
    let mut project = Project::new();
    project.set_import_module_specifier_preference(Some("project-relative".to_string()));
    project.set_file(
        "/home/src/workspaces/project/package.json".to_string(),
        r#"{
  "dependencies": {
    "mylib": "file:packages/mylib"
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/src/index.ts".to_string(),
        "const value = MyClass".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/packages/mylib/package.json".to_string(),
        r#"{
  "name": "mylib",
  "version": "1.0.0",
  "main": "index.js",
  "types": "index"
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/packages/mylib/index.ts".to_string(),
        "export class MyClass {}".to_string(),
    );

    let specifiers = project.auto_import_module_specifiers_from_files(
        "/home/src/workspaces/project/src/index.ts",
        "/home/src/workspaces/project/packages/mylib/index.ts",
    );

    assert_eq!(
        specifiers.first().map(String::as_str),
        Some("mylib"),
        "expected workspace package bare specifier to win under project-relative preference, got {specifiers:?}"
    );
    assert!(
        !specifiers.iter().any(|specifier| specifier.contains(".ts")),
        "expected runtime-safe specifiers without .ts extensions, got {specifiers:?}"
    );
}

#[test]
fn workspace_file_dependency_alias_works_without_target_package_manifest_loaded() {
    let mut project = Project::new();
    project.set_import_module_specifier_preference(Some("project-relative".to_string()));
    project.set_file(
        "/home/src/workspaces/project/package.json".to_string(),
        r#"{
  "dependencies": {
    "mylib": "file:packages/mylib"
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/src/index.ts".to_string(),
        "const value = MyClass".to_string(),
    );
    // Intentionally omit /packages/mylib/package.json to mirror server runs
    // where only a subset of project files is loaded into the in-memory snapshot.
    project.set_file(
        "/home/src/workspaces/project/packages/mylib/index.ts".to_string(),
        "export class MyClass {}".to_string(),
    );

    let specifiers = project.auto_import_module_specifiers_from_files(
        "/home/src/workspaces/project/src/index.ts",
        "/home/src/workspaces/project/packages/mylib/index.ts",
    );

    assert_eq!(
        specifiers.first().map(String::as_str),
        Some("mylib"),
        "expected file-linked dependency alias to survive without target package.json, got {specifiers:?}"
    );
}

#[test]
fn workspace_file_dependency_alias_works_without_requesting_package_manifest_loaded() {
    let mut project = Project::new();
    project.set_import_module_specifier_preference(Some("project-relative".to_string()));
    // Intentionally omit /project/package.json to mirror adapter snapshots
    // where only source files and some config files are opened.
    project.set_file(
        "/home/src/workspaces/project/src/index.ts".to_string(),
        "const value = MyClass".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/packages/mylib/package.json".to_string(),
        r#"{
  "name": "mylib",
  "version": "1.0.0",
  "main": "index.js",
  "types": "index"
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/packages/mylib/index.ts".to_string(),
        "export class MyClass {}".to_string(),
    );

    let specifiers = project.auto_import_module_specifiers_from_files(
        "/home/src/workspaces/project/src/index.ts",
        "/home/src/workspaces/project/packages/mylib/index.ts",
    );

    assert_eq!(
        specifiers.first().map(String::as_str),
        Some("mylib"),
        "expected target package name fallback to avoid deep relative import, got {specifiers:?}"
    );
}

#[test]
fn workspace_package_path_fallback_avoids_deep_relative_when_manifests_are_missing() {
    let mut project = Project::new();
    project.set_import_module_specifier_preference(Some("project-relative".to_string()));
    project.set_file(
        "/home/src/workspaces/project/src/index.ts".to_string(),
        "const value = MyClass".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/packages/mylib/index.ts".to_string(),
        "export class MyClass {}".to_string(),
    );

    let specifiers = project.auto_import_module_specifiers_from_files(
        "/home/src/workspaces/project/src/index.ts",
        "/home/src/workspaces/project/packages/mylib/index.ts",
    );

    assert_eq!(
        specifiers.first().map(String::as_str),
        Some("mylib"),
        "expected /packages path fallback to prefer inferred package specifier, got {specifiers:?}"
    );
}

#[test]
fn workspace_dependency_respects_package_exports_visibility() {
    let mut project = Project::new();
    project.set_file(
        "/repo/packages/pack/package.json".to_string(),
        r#"{
  "name": "pack",
  "version": "1.0.0",
  "exports": {
    ".": "./dist/main.mjs"
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/repo/packages/pack/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "lib": ["es5"],
    "composite": true,
    "module": "nodenext",
    "rootDir": "src",
    "outDir": "dist"
  },
  "include": ["src"]
}"#
        .to_string(),
    );
    project.set_file(
        "/repo/packages/pack/src/unreachable.ts".to_string(),
        "export const fromUnreachable = 0;".to_string(),
    );
    project.set_file(
        "/repo/packages/app/package.json".to_string(),
        r#"{
  "name": "app",
  "dependencies": {
    "pack": "file:../pack"
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/repo/packages/app/src/index.ts".to_string(),
        "x".to_string(),
    );

    let specifiers = project.auto_import_module_specifiers_from_files(
        "/repo/packages/app/src/index.ts",
        "/repo/packages/pack/src/unreachable.ts",
    );
    assert!(
        !specifiers.iter().any(|specifier| specifier == "pack"),
        "expected hidden exports target to avoid pack bare specifier, got {specifiers:?}"
    );
}

#[test]
fn package_specifier_uses_package_name_from_store_layout_package_json() {
    let mut project = Project::new();
    project.set_file(
            "/home/src/workspaces/project/node_modules/.store/@remix-run-server-runtime-virtual-c72daf0d/package/package.json".to_string(),
            r#"{
  "name": "@remix-run/server-runtime",
  "version": "0.0.0",
  "main": "index.js"
}"#
            .to_string(),
        );
    project.set_file(
            "/home/src/workspaces/project/node_modules/.store/@remix-run-server-runtime-virtual-c72daf0d/package/index.d.ts".to_string(),
            "export declare function ServerRuntimeMetaFunction(): void;".to_string(),
        );

    assert_eq!(
            project.package_specifier_from_node_modules(
                "/home/src/workspaces/project/node_modules/.store/@remix-run-server-runtime-virtual-c72daf0d/package/index.d.ts"
            ),
            Some("@remix-run/server-runtime".to_string())
        );
}

#[test]
fn package_specifier_uses_nested_pnpm_node_modules_package_name() {
    let mut project = Project::new();
    project.set_file(
        "/repo/node_modules/.pnpm/@scope+pkg@1.0.0/node_modules/@scope/pkg/package.json"
            .to_string(),
        r#"{
  "name": "@scope/pkg",
  "version": "1.0.0"
}"#
        .to_string(),
    );
    project.set_file(
        "/repo/node_modules/.pnpm/@scope+pkg@1.0.0/node_modules/@scope/pkg/sub/path/file.d.ts"
            .to_string(),
        "export declare const value: number;".to_string(),
    );

    assert_eq!(
        project.package_specifier_from_node_modules(
            "/repo/node_modules/.pnpm/@scope+pkg@1.0.0/node_modules/@scope/pkg/sub/path/file.d.ts"
        ),
        Some("@scope/pkg/sub/path/file".to_string())
    );
}

#[test]
fn nested_package_manifest_inside_package_keeps_parent_subpath_specifier() {
    let mut project = Project::new();
    project.set_file(
        "/project/node_modules/preact/hooks/package.json".to_string(),
        r#"{ "name": "hooks", "version": "0.1.0", "types": "src/index.d.ts" }"#.to_string(),
    );
    project.set_file(
        "/project/node_modules/preact/hooks/src/index.d.ts".to_string(),
        "export declare function useMemo<T>(factory: () => T): T;".to_string(),
    );

    assert_eq!(
        project.package_specifier_from_node_modules(
            "/project/node_modules/preact/hooks/src/index.d.ts"
        ),
        Some("preact/hooks".to_string())
    );
}

#[test]
fn node10_module_resolution_does_not_use_exports_subpath_aliases() {
    let mut project = Project::new();
    project.set_file(
        "/home/src/workspaces/project/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "module": "commonjs",
    "moduleResolution": "node10",
    "lib": ["es5"]
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/package.json".to_string(),
        r#"{
  "dependencies": {
    "dependency": "^1.0.0"
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/node_modules/dependency/package.json".to_string(),
        r#"{
  "name": "dependency",
  "types": "./lib/index.d.ts",
  "exports": {
    ".": { "types": "./lib/index.d.ts" },
    "./lol": { "types": "./lib/lol.d.ts" }
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/node_modules/dependency/lib/index.d.ts".to_string(),
        "export declare function fooFromIndex(): void;".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/node_modules/dependency/lib/lol.d.ts".to_string(),
        "export declare function fooFromLol(): void;".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/src/foo.ts".to_string(),
        "fooFrom".to_string(),
    );

    let root_specs = project.auto_import_module_specifiers_from_files(
        "/home/src/workspaces/project/src/foo.ts",
        "/home/src/workspaces/project/node_modules/dependency/lib/index.d.ts",
    );
    assert!(
        root_specs.iter().any(|specifier| specifier == "dependency"),
        "expected dependency root specifier under node10 moduleResolution, got {root_specs:?}"
    );

    let subpath_specs = project.auto_import_module_specifiers_from_files(
        "/home/src/workspaces/project/src/foo.ts",
        "/home/src/workspaces/project/node_modules/dependency/lib/lol.d.ts",
    );
    assert!(
        !subpath_specs
            .iter()
            .any(|specifier| specifier == "dependency/lol"),
        "expected node10 moduleResolution to avoid exports subpath alias dependency/lol, got {subpath_specs:?}"
    );
}

#[test]
fn exports_import_and_require_conditions_follow_importer_extension() {
    let mut project = Project::new();
    project.set_file(
        "/home/src/workspaces/project/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "module": "nodenext",
    "lib": ["es5"]
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/package.json".to_string(),
        r#"{
  "dependencies": {
    "dependency": "^1.0.0"
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/node_modules/dependency/package.json".to_string(),
        r#"{
  "name": "dependency",
  "exports": {
    "./lol": {
      "import": "./lib/index.js",
      "require": "./lib/lol.js"
    }
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/node_modules/dependency/lib/index.d.ts".to_string(),
        "export declare function fooFromIndex(): void;".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/node_modules/dependency/lib/lol.d.ts".to_string(),
        "export declare function fooFromLol(): void;".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/src/foo.cts".to_string(),
        "fooFrom".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/src/foo.mts".to_string(),
        "fooFrom".to_string(),
    );

    let cts_specs = project.auto_import_module_specifiers_from_files(
        "/home/src/workspaces/project/src/foo.cts",
        "/home/src/workspaces/project/node_modules/dependency/lib/lol.d.ts",
    );
    assert!(
        cts_specs
            .iter()
            .any(|specifier| specifier == "dependency/lol"),
        "expected .cts importer to follow require branch for dependency/lol, got {cts_specs:?}"
    );

    let mts_specs = project.auto_import_module_specifiers_from_files(
        "/home/src/workspaces/project/src/foo.mts",
        "/home/src/workspaces/project/node_modules/dependency/lib/index.d.ts",
    );
    assert!(
        mts_specs
            .iter()
            .any(|specifier| specifier == "dependency/lol"),
        "expected .mts importer to follow import branch for dependency/lol, got {mts_specs:?}"
    );
}

#[test]
fn root_dirs_prefers_shortest_relative_specifier_across_roots() {
    let mut project = Project::new();
    project.set_file(
        "/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "module": "commonjs",
    "rootDirs": [".", "./some/other/root"]
  }
}"#
        .to_string(),
    );

    assert_eq!(
        project.root_dirs_relative_specifier_from_files("/index.ts", "/some/other/root/types.ts"),
        Some("./types".to_string())
    );

    assert_eq!(
        project.auto_import_module_specifiers_from_files("/index.ts", "/some/other/root/types.ts"),
        vec!["./types".to_string(), "./some/other/root/types".to_string()]
    );
}

#[test]
fn path_mapping_collapses_index_suffix_for_barrel_target() {
    let mut project = Project::new();
    project.set_file(
        "/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "module": "commonjs",
    "paths": {
      "~/*": ["src/*"]
    }
  }
}"#
        .to_string(),
    );
    project.set_file("/src/dirA/thing1A.ts".to_string(), "Thing".to_string());
    project.set_file(
        "/src/dirB/index.ts".to_string(),
        "export * from \"./thing1B\";".to_string(),
    );

    assert_eq!(
        project.path_mapping_specifiers_from_files("/src/dirA/thing1A.ts", "/src/dirB/index.ts"),
        vec!["~/dirB".to_string()]
    );
}

#[test]
fn path_mapping_uses_referenced_project_outdir_when_composite_rootdir_is_implicit() {
    let mut project = Project::new();
    project.set_import_module_specifier_preference(Some("non-relative".to_string()));
    project.set_file(
        "/home/src/workspaces/project/common/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "lib": ["es5"],
    "module": "commonjs",
    "outDir": "dist",
    "composite": true
  },
  "include": ["src"]
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/common/src/MyModule.ts".to_string(),
        "export function square(n: number) { return n * n; }".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/web/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "lib": ["es5"],
    "module": "esnext",
    "moduleResolution": "node",
    "noEmit": true,
    "paths": {
      "@common/*": ["../common/dist/src/*"]
    }
  },
  "include": ["src"],
  "references": [{ "path": "../common" }]
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/web/src/Helper.ts".to_string(),
        "square(2);".to_string(),
    );

    let specifiers = project.auto_import_module_specifiers_from_files(
        "/home/src/workspaces/project/web/src/Helper.ts",
        "/home/src/workspaces/project/common/src/MyModule.ts",
    );
    assert!(
        specifiers.contains(&"@common/MyModule".to_string()),
        "expected @common/MyModule to be generated from dist/src path mapping, got {specifiers:?}"
    );
}

#[test]
fn path_mapping_uses_outdir_source_alternatives_for_cross_project_subpaths() {
    let mut project = Project::new();
    project.set_file(
        "/home/src/workspaces/project/packages/app/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "lib": ["es5"],
    "module": "commonjs",
    "outDir": "dist",
    "rootDir": "src",
    "baseUrl": ".",
    "paths": {
      "dep": ["../dep/src/main"],
      "dep/dist/*": ["../dep/src/*"]
    }
  },
  "references": [{ "path": "../dep" }]
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/packages/app/src/utils.ts".to_string(),
        "dep2;".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/packages/dep/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": { "lib": ["es5"], "outDir": "dist", "rootDir": "src", "module": "commonjs" }
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/packages/dep/src/sub/folder/index.ts".to_string(),
        "export const dep2 = 0;".to_string(),
    );

    let specifiers = project.auto_import_module_specifiers_from_files(
        "/home/src/workspaces/project/packages/app/src/utils.ts",
        "/home/src/workspaces/project/packages/dep/src/sub/folder/index.ts",
    );
    assert!(
        specifiers.contains(&"dep/dist/sub/folder".to_string()),
        "expected dep/dist/sub/folder path-mapped specifier, got {specifiers:?}"
    );
}

#[test]
fn package_imports_from_outdir_mapping_prefer_js_even_with_allow_ts_extensions() {
    let specs = package_import_specifiers_for_target(
        r##"{
  "type": "module",
  "imports": {
    "#*": {
      "types": "./types/*",
      "default": "./dist/*"
    }
  }
}"##,
        "/",
        "/src/add.ts",
        true,
        &["/dist/add".to_string(), "/types/add".to_string()],
    );

    assert_eq!(specs, vec!["#add.js".to_string()]);
}

#[test]
fn package_imports_without_allow_ts_extensions_emit_js_specifiers() {
    let specs = package_import_specifiers_for_target(
        r##"{
  "type": "module",
  "imports": {
    "#internal/*": "./dist/internal/*"
  }
}"##,
        "/home/src/workspaces/project",
        "/home/src/workspaces/project/src/internal/foo.ts",
        false,
        &["/home/src/workspaces/project/dist/internal/foo".to_string()],
    );

    assert_eq!(specs, vec!["#internal/foo.js".to_string()]);
}

#[test]
fn package_imports_with_trailing_slash_mapping_emit_subpath_js_specifiers() {
    let specs = package_import_specifiers_for_target(
        r##"{
  "type": "module",
  "imports": {
    "#internal/": "./dist/internal/"
  }
}"##,
        "/home/src/workspaces/project",
        "/home/src/workspaces/project/src/internal/foo.ts",
        false,
        &["/home/src/workspaces/project/dist/internal/foo".to_string()],
    );

    assert_eq!(specs, vec!["#internal/foo.js".to_string()]);
}

#[test]
fn jsconfig_paths_mapping_outranks_relative_for_shortest_preference() {
    let mut project = Project::new();
    project.set_file(
        "/package1/jsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "checkJs": true,
    "paths": {
      "package1/*": ["./*"],
      "package2/*": ["../package2/*"]
    },
    "baseUrl": "."
  }
}"#
        .to_string(),
    );
    project.set_file("/package1/file1.js".to_string(), "bar".to_string());
    project.set_file(
        "/package2/file1.js".to_string(),
        "export const bar = 0;".to_string(),
    );

    assert_eq!(
        project
            .auto_import_module_specifiers_from_files("/package1/file1.js", "/package2/file1.js"),
        vec![
            "package2/file1".to_string(),
            "../package2/file1.js".to_string()
        ]
    );
}

#[test]
fn jsconfig_jsonc_unquoted_keys_are_supported_for_paths_mapping() {
    let mut project = Project::new();
    project.set_file(
        "/package1/jsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    checkJs: true,
    "paths": {
      "package1/*": ["./*"],
      "package2/*": ["../package2/*"]
    },
    "baseUrl": "."
  }
}"#
        .to_string(),
    );
    project.set_file("/package1/file1.js".to_string(), "bar".to_string());
    project.set_file(
        "/package2/file1.js".to_string(),
        "export const bar = 0;".to_string(),
    );

    assert_eq!(
        project
            .auto_import_module_specifiers_from_files("/package1/file1.js", "/package2/file1.js"),
        vec![
            "package2/file1".to_string(),
            "../package2/file1.js".to_string()
        ]
    );
}

#[test]
fn shortest_prefers_relative_over_paths_when_depth_matches() {
    let mut project = Project::new();
    project.set_file(
        "/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "module": "preserve",
    "paths": {
      "@app/*": ["./src/*"]
    }
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/src/utils.ts".to_string(),
        "export function add(a: number, b: number) {}".to_string(),
    );
    project.set_file("/src/index.ts".to_string(), "ad".to_string());

    assert_eq!(
        project.auto_import_module_specifiers_from_files("/src/index.ts", "/src/utils.ts"),
        vec!["./utils".to_string(), "@app/utils".to_string()]
    );
}

#[test]
fn shortest_keeps_path_mapping_ahead_of_parent_relative_specifier() {
    let mut project = Project::new();
    project.set_file(
        "/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "paths": {
      "@root/*": ["${configDir}/src/*"]
    }
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/src/one.ts".to_string(),
        "export const one = 1;".to_string(),
    );
    project.set_file("/src/foo/two.ts".to_string(), "one".to_string());

    assert_eq!(
        project.auto_import_module_specifiers_from_files("/src/foo/two.ts", "/src/one.ts"),
        vec!["@root/one".to_string(), "../one".to_string()]
    );
}

#[test]
fn node_modules_paths_mapping_beats_package_specifier_for_shortest() {
    let mut project = Project::new();
    project.set_file(
        "/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "module": "amd",
    "moduleResolution": "node",
    "rootDir": "ts",
    "baseUrl": ".",
    "paths": {
      "*": ["node_modules/@woltlab/wcf/ts/*"]
    }
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/node_modules/@woltlab/wcf/ts/WoltLabSuite/Core/Component/Dialog.ts".to_string(),
        "export class Dialog {}".to_string(),
    );
    project.set_file("/ts/main.ts".to_string(), "Dialog".to_string());

    assert_eq!(
        project.auto_import_module_specifiers_from_files(
            "/ts/main.ts",
            "/node_modules/@woltlab/wcf/ts/WoltLabSuite/Core/Component/Dialog.ts"
        ),
        vec![
            "WoltLabSuite/Core/Component/Dialog".to_string(),
            "@woltlab/wcf/ts/WoltLabSuite/Core/Component/Dialog".to_string()
        ]
    );
}

#[test]
fn auto_imports_disabled_for_module_none_es5() {
    let mut project = Project::new();
    project.set_file(
        "/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "module": "none",
    "target": "es5"
  }
}"#
        .to_string(),
    );

    assert!(!project.auto_imports_allowed_for_file("/index.ts"));
}

#[test]
fn auto_imports_enabled_for_module_none_es2015() {
    let mut project = Project::new();
    project.set_file(
        "/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "module": "none",
    "target": "es2015"
  }
}"#
        .to_string(),
    );

    assert!(project.auto_imports_allowed_for_file("/index.ts"));
}

#[test]
fn mts_auto_import_sources_stay_extensionless_even_with_js_imports() {
    let mut project = Project::new();
    project.set_file(
        "/mod.ts".to_string(),
        "export interface I {}\nexport class C {}\n".to_string(),
    );
    project.set_file(
        "/a.mts".to_string(),
        "import type { I } from \"./mod.js\";\nconst x: I = new C();\n".to_string(),
    );

    let specifiers = project.auto_import_module_specifiers_from_files("/a.mts", "/mod.ts");
    assert_eq!(specifiers, vec!["./mod".to_string()]);
}

#[test]
fn node_modules_types_entry_uses_package_root_for_declaration_target() {
    let mut project = Project::new();
    project.set_file(
        "/home/src/workspaces/project/package.json".to_string(),
        r#"{
  "dependencies": {
    "@angular/forms": "*"
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/node_modules/@angular/forms/package.json".to_string(),
        r#"{
  "name": "@angular/forms",
  "typings": "./forms.d.ts"
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/index.ts".to_string(),
        "PatternValidator".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/node_modules/@angular/forms/forms.d.ts".to_string(),
        "export class PatternValidator {}\n".to_string(),
    );

    let specifiers = project.auto_import_module_specifiers_from_files(
        "/home/src/workspaces/project/index.ts",
        "/home/src/workspaces/project/node_modules/@angular/forms/forms.d.ts",
    );

    assert!(
        specifiers
            .first()
            .is_some_and(|specifier| specifier == "@angular/forms"),
        "expected @angular/forms to be preferred for typings entrypoint declarations, got {specifiers:?}"
    );
}

#[test]
fn pnpm_store_package_without_name_uses_linked_dependency_alias() {
    let mut project = Project::new();
    project.set_file(
        "/home/src/workspaces/project/package.json".to_string(),
        r#"{
  "dependencies": {
    "mobx": "*"
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/index.ts".to_string(),
        "autorun".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/node_modules/.pnpm/mobx@6.0.4/node_modules/mobx/package.json"
            .to_string(),
        r#"{
  "types": "dist/mobx.d.ts"
}"#
        .to_string(),
    );
    project.set_file(
            "/home/src/workspaces/project/node_modules/.pnpm/mobx@6.0.4/node_modules/mobx/dist/mobx.d.ts"
                .to_string(),
            "export declare function autorun(): void;\n".to_string(),
        );

    let specifiers = project.auto_import_module_specifiers_from_files(
            "/home/src/workspaces/project/index.ts",
            "/home/src/workspaces/project/node_modules/.pnpm/mobx@6.0.4/node_modules/mobx/dist/mobx.d.ts",
        );

    assert!(
        specifiers.iter().any(|specifier| specifier == "mobx"),
        "expected pnpm store package target to resolve to dependency alias `mobx`, got {specifiers:?}"
    );
}
