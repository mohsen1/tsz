use super::*;

#[test]
fn auto_import_prefix_candidates_include_barrel_and_direct_path_variants() {
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
        "export * from \"./thing1B\";\nexport * from \"./thing2B\";\n".to_string(),
    );
    project.set_file(
        "/src/dirB/thing1B.ts".to_string(),
        "export class Thing1B {}".to_string(),
    );
    project.set_file(
        "/src/dirB/thing2B.ts".to_string(),
        "export class Thing2B {}".to_string(),
    );

    let mut thing2_specs: Vec<String> = project
        .get_import_candidates_for_prefix("/src/dirA/thing1A.ts", "Thing")
        .into_iter()
        .filter(|candidate| candidate.local_name == "Thing2B")
        .map(|candidate| candidate.module_specifier)
        .collect();
    thing2_specs.sort();
    thing2_specs.dedup();

    assert_eq!(
        thing2_specs,
        vec!["~/dirB".to_string(), "~/dirB/thing2B".to_string()]
    );
}

#[test]
fn auto_import_candidates_include_ambient_module_exports() {
    let mut project = Project::new();
    project.set_file(
            "/node_modules/lib/index.d.ts".to_string(),
            "declare module \"ambient\" { export const x: number; }\ndeclare module \"ambient/utils\" { export const x: number; }\n".to_string(),
        );
    project.set_file("/index.ts".to_string(), "x".to_string());

    let mut specs: Vec<String> = project
        .get_import_candidates_for_prefix("/index.ts", "x")
        .into_iter()
        .map(|candidate| candidate.module_specifier)
        .collect();
    specs.sort();
    specs.dedup();

    assert_eq!(
        specs,
        vec!["ambient".to_string(), "ambient/utils".to_string()]
    );
}

#[test]
fn auto_import_candidates_include_commonjs_exports_from_js_files() {
    let mut project = Project::new();
    project.set_file(
        "/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "module": "node18",
    "allowJs": true,
    "checkJs": true
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/matrix.js".to_string(),
        "exports.variants = [];".to_string(),
    );
    project.set_file("/main.js".to_string(), "variants".to_string());

    let specs: Vec<String> = project
        .get_import_candidates_for_prefix("/main.js", "variants")
        .into_iter()
        .filter(|candidate| candidate.local_name == "variants")
        .map(|candidate| candidate.module_specifier)
        .collect();

    assert!(
        specs.iter().any(|spec| spec == "./matrix.js"),
        "expected './matrix.js' auto-import candidate, got {specs:?}"
    );
}

#[test]
fn auto_import_candidates_include_workspace_file_dependency_alias_specifier() {
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

    let specs: Vec<String> = project
        .get_import_candidates_for_prefix(
            "/home/src/workspaces/solution/packages/web/src/index.ts",
            "gainUtility",
        )
        .into_iter()
        .filter(|candidate| candidate.local_name == "gainUtility")
        .map(|candidate| candidate.module_specifier)
        .collect();

    assert!(
        specs.iter().any(|spec| spec == "@monorepo/utils"),
        "expected @monorepo/utils candidate from file-linked workspace dependency, got {specs:?}"
    );
}

#[test]
fn auto_import_candidates_prefer_workspace_dependency_name_for_reexported_symbol() {
    let mut project = Project::new();
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
        "export * from \"./mySubDir\";".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/packages/mylib/mySubDir/index.ts".to_string(),
        "export * from \"./myClass\";".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/packages/mylib/mySubDir/myClass.ts".to_string(),
        "export class MyClass {}".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/src/index.ts".to_string(),
        "const a = new MyClass();".to_string(),
    );

    let specs: Vec<String> = project
        .get_import_candidates_for_prefix("/home/src/workspaces/project/src/index.ts", "MyClass")
        .into_iter()
        .filter(|candidate| candidate.local_name == "MyClass")
        .map(|candidate| candidate.module_specifier)
        .collect();

    assert!(
        specs.iter().any(|spec| spec == "mylib"),
        "expected workspace dependency specifier 'mylib' for re-exported symbol, got {specs:?}"
    );
}

#[test]
fn project_relative_sort_prefers_workspace_dependency_for_reexported_symbol() {
    let mut project = Project::new();
    project.set_import_module_specifier_preference(Some("project-relative".to_string()));
    project.set_file(
        "/home/src/workspaces/project/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "lib": ["es5"],
    "module": "commonjs"
  }
}"#
        .to_string(),
    );
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
        "export * from \"./mySubDir\";".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/packages/mylib/mySubDir/index.ts".to_string(),
        "export * from \"./myClass\";".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/packages/mylib/mySubDir/myClass.ts".to_string(),
        "export class MyClass {}".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/src/index.ts".to_string(),
        "const a = new MyClass();".to_string(),
    );

    let mut specs: Vec<String> = project
        .get_import_candidates_for_prefix("/home/src/workspaces/project/src/index.ts", "MyClass")
        .into_iter()
        .filter(|candidate| candidate.local_name == "MyClass")
        .map(|candidate| candidate.module_specifier)
        .collect();

    specs.sort_by(|a, b| {
        let a_segments = a.matches('/').count();
        let b_segments = b.matches('/').count();
        let candidate_rank = |candidate: &str| -> u8 {
            if candidate.starts_with("./") {
                0
            } else if !candidate.starts_with('.') {
                1
            } else if candidate.starts_with("../") {
                2
            } else {
                3
            }
        };
        let index_penalty = |candidate: &str| -> u8 {
            if candidate == "." || candidate == ".." || candidate.ends_with("/index") {
                1
            } else {
                0
            }
        };
        a_segments
            .cmp(&b_segments)
            .then_with(|| candidate_rank(a).cmp(&candidate_rank(b)))
            .then_with(|| index_penalty(a).cmp(&index_penalty(b)))
            .then_with(|| a.len().cmp(&b.len()))
            .then_with(|| a.cmp(b))
    });

    assert_eq!(
        specs.first().map(String::as_str),
        Some("mylib"),
        "expected project-relative ordering to still prefer workspace dependency alias, got {specs:?}"
    );
}

#[test]
fn auto_import_candidates_use_type_module_main_subpath_without_index() {
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
    project.set_file(
        "/package.json".to_string(),
        r#"{
  "dependencies": {
    "pkg": "*"
  }
}"#
        .to_string(),
    );
    project.set_file("/index.ts".to_string(), "foo".to_string());

    let mut specs: Vec<String> = project
        .get_import_candidates_for_prefix("/index.ts", "foo")
        .into_iter()
        .filter(|candidate| candidate.local_name == "foo")
        .map(|candidate| candidate.module_specifier)
        .collect();
    specs.sort();
    specs.dedup();

    assert_eq!(specs, vec!["pkg/lib".to_string()]);
}

#[test]
fn diagnostics_import_candidates_use_type_module_main_subpath_without_index() {
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
    project.set_file(
        "/package.json".to_string(),
        r#"{
  "dependencies": {
    "pkg": "*"
  }
}"#
        .to_string(),
    );
    project.set_file("/index.ts".to_string(), "foo".to_string());

    let diagnostics = vec![LspDiagnostic {
        range: Range::new(Position::new(0, 0), Position::new(0, 3)),
        message: "Cannot find name 'foo'.".to_string(),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
        severity: None,
        source: None,
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    }];

    let mut specs: Vec<String> = project
        .get_import_candidates_for_diagnostics("/index.ts", &diagnostics)
        .into_iter()
        .filter(|candidate| candidate.local_name == "foo")
        .map(|candidate| candidate.module_specifier)
        .collect();
    specs.sort();
    specs.dedup();

    assert_eq!(specs, vec!["pkg/lib".to_string()]);
}

#[test]
fn auto_import_candidates_include_exports_types_root_and_subpath_entries() {
    let mut project = Project::new();
    project.set_file(
        "/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "lib": ["es5"],
    "module": "nodenext"
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/package.json".to_string(),
        r#"{
  "dependencies": {
    "dependency": "^1.0.0"
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/node_modules/dependency/package.json".to_string(),
        r#"{
  "type": "module",
  "name": "dependency",
  "version": "1.0.0",
  "exports": {
    ".": { "types": "./lib/index.d.ts" },
    "./lol": { "types": "./lib/lol.d.ts" }
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/node_modules/dependency/lib/index.d.ts".to_string(),
        "export function fooFromIndex(): void;".to_string(),
    );
    project.set_file(
        "/node_modules/dependency/lib/lol.d.ts".to_string(),
        "export function fooFromLol(): void;".to_string(),
    );
    project.set_file("/src/foo.ts".to_string(), "fooFrom".to_string());

    let candidates = project.get_import_candidates_for_prefix("/src/foo.ts", "fooFrom");
    let specs_for = |name: &str| -> Vec<String> {
        candidates
            .iter()
            .filter(|candidate| candidate.local_name == name)
            .map(|candidate| candidate.module_specifier.clone())
            .collect()
    };

    let index_specs = specs_for("fooFromIndex");
    assert!(
        index_specs
            .iter()
            .any(|specifier| specifier == "dependency"),
        "expected fooFromIndex auto-import from dependency root export-map types entry, got {index_specs:?}"
    );

    let lol_specs = specs_for("fooFromLol");
    assert!(
        lol_specs
            .iter()
            .any(|specifier| specifier == "dependency/lol"),
        "expected fooFromLol auto-import from dependency/lol export-map types entry, got {lol_specs:?}"
    );
}

#[test]
fn auto_import_file_exclude_patterns_hide_store_layout_package_candidates() {
    let mut project = Project::new();
    project
        .set_auto_import_file_exclude_patterns(vec!["/**/@remix-run/server-runtime".to_string()]);
    project.set_file(
        "/home/src/workspaces/project/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "module": "commonjs"
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/package.json".to_string(),
        r#"{
  "dependencies": {
    "@remix-run/server-runtime": "*"
  }
}"#
        .to_string(),
    );
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
    project.set_file(
        "/home/src/workspaces/project/index.ts".to_string(),
        "ServerRuntimeMetaFunction".to_string(),
    );

    let candidates = project.get_import_candidates_for_prefix(
        "/home/src/workspaces/project/index.ts",
        "ServerRuntimeMetaFunction",
    );
    assert!(
        !candidates
            .iter()
            .any(|candidate| candidate.local_name == "ServerRuntimeMetaFunction"),
        "expected store-layout package candidate to be excluded, got {candidates:?}"
    );
}

#[test]
fn ambient_module_auto_import_candidates_respect_specifier_exclude_regexes() {
    let mut project = Project::new();
    project.set_auto_import_specifier_exclude_regexes(vec!["utils".to_string()]);
    project.set_file(
            "/node_modules/lib/index.d.ts".to_string(),
            "declare module \"ambient\" { export const x: number; }\ndeclare module \"ambient/utils\" { export const x: number; }\n".to_string(),
        );
    project.set_file("/index.ts".to_string(), "x".to_string());

    let mut specs: Vec<String> = project
        .get_import_candidates_for_prefix("/index.ts", "x")
        .into_iter()
        .map(|candidate| candidate.module_specifier)
        .collect();
    specs.sort();
    specs.dedup();

    assert_eq!(specs, vec!["ambient".to_string()]);
}

#[test]
fn ambient_module_auto_import_file_exclude_patterns_are_all_or_nothing() {
    let mut project = Project::new();
    project.set_auto_import_file_exclude_patterns(vec!["/**/ambient1.d.ts".to_string()]);
    project.set_file(
        "/ambient1.d.ts".to_string(),
        "declare module \"foo\" { export const x = 1; }\n".to_string(),
    );
    project.set_file(
        "/ambient2.d.ts".to_string(),
        "declare module \"foo\" { export const y = 2; }\n".to_string(),
    );
    project.set_file("/index.ts".to_string(), "x".to_string());

    let names: FxHashSet<String> = project
        .get_import_candidates_for_prefix("/index.ts", "")
        .into_iter()
        .filter(|candidate| candidate.module_specifier == "foo")
        .map(|candidate| candidate.local_name)
        .collect();

    assert!(
        names.contains("x"),
        "Expected ambient module symbol `x` to remain when only part of a merged ambient module is excluded"
    );
    assert!(
        names.contains("y"),
        "Expected ambient module symbol `y` to remain when only part of a merged ambient module is excluded"
    );
}

#[test]
fn ambient_module_auto_import_file_exclude_patterns_hide_when_all_declarations_excluded() {
    let mut project = Project::new();
    project.set_auto_import_file_exclude_patterns(vec!["/**/ambient*".to_string()]);
    project.set_file(
        "/ambient1.d.ts".to_string(),
        "declare module \"foo\" { export const x = 1; }\n".to_string(),
    );
    project.set_file(
        "/ambient2.d.ts".to_string(),
        "declare module \"foo\" { export const y = 2; }\n".to_string(),
    );
    project.set_file("/index.ts".to_string(), "x".to_string());

    let candidates = project.get_import_candidates_for_prefix("/index.ts", "");
    assert!(
        !candidates
            .iter()
            .any(|candidate| candidate.module_specifier == "foo"),
        "Expected ambient module `foo` to be excluded when all declaration files are excluded"
    );
}

#[test]
fn auto_import_candidates_include_export_equals_identifier_default() {
    let mut project = Project::new();
    project.set_file(
        "/ts.d.ts".to_string(),
        r#"declare namespace ts {
  interface SourceFile {
    text: string;
  }
}
export = ts;
"#
        .to_string(),
    );
    project.set_file("/types.ts".to_string(), "ts".to_string());

    let has_ts_default = project
        .get_import_candidates_for_prefix("/types.ts", "ts")
        .into_iter()
        .any(|candidate| {
            candidate.local_name == "ts"
                && candidate.module_specifier == "./ts"
                && matches!(candidate.kind, ImportCandidateKind::Default)
        });

    assert!(
        has_ts_default,
        "expected default auto-import candidate `ts` from `./ts` for `export = ts` declarations"
    );
}

#[test]
fn diagnostics_import_candidates_include_default_from_export_star_as_default() {
    let mut project = Project::new();
    project.set_file("/a.ts".to_string(), "export class A {}\n".to_string());
    project.set_file(
        "/ns.ts".to_string(),
        "export * as default from \"./a\";\n".to_string(),
    );
    project.set_file("/e.ts".to_string(), "let x: ns.A;\n".to_string());

    let diagnostics = vec![LspDiagnostic {
        range: Range::new(Position::new(0, 7), Position::new(0, 9)),
        message: "Cannot find name 'ns'.".to_string(),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAMESPACE),
        severity: None,
        source: None,
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    }];

    let candidates = project.get_import_candidates_for_diagnostics("/e.ts", &diagnostics);
    let has_default_ns = candidates.iter().any(|candidate| {
        candidate.local_name == "ns"
            && candidate.module_specifier == "./ns"
            && matches!(candidate.kind, ImportCandidateKind::Default)
    });

    assert!(
        has_default_ns,
        "expected default import candidate for `ns` from `./ns` when re-exported via `export * as default`, got: {candidates:?}"
    );
}

#[test]
fn auto_import_candidates_include_reexported_type_namespace_name() {
    let mut project = Project::new();
    project.set_file(
        "/home/src/workspaces/project/package.json".to_string(),
        r#"{
  "dependencies": {
    "@jest/types": "*",
    "ts-jest": "*"
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/node_modules/@jest/types/package.json".to_string(),
        r#"{
  "name": "@jest/types"
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/node_modules/@jest/types/index.d.ts".to_string(),
        "import type * as Config from \"./Config\";\nexport type { Config };\n".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/node_modules/@jest/types/Config.d.ts".to_string(),
        "export interface ConfigGlobals { [k: string]: unknown; }\n".to_string(),
    );
    project.set_file(
            "/home/src/workspaces/project/node_modules/ts-jest/index.d.ts".to_string(),
            "export {};\ndeclare module \"@jest/types\" {\n  namespace Config { interface ConfigGlobals { \"ts-jest\": any; } }\n}\n".to_string(),
        );
    project.set_file(
        "/home/src/workspaces/project/index.ts".to_string(),
        "C".to_string(),
    );

    let has_config = project
        .get_import_candidates_for_prefix("/home/src/workspaces/project/index.ts", "C")
        .into_iter()
        .any(|candidate| {
            candidate.local_name == "Config"
                && candidate.module_specifier == "@jest/types"
                && matches!(
                    candidate.kind,
                    ImportCandidateKind::Named { ref export_name } if export_name == "Config"
                )
        });
    assert!(
        has_config,
        "expected re-exported type namespace `Config` auto-import candidate from @jest/types"
    );
}

#[test]
fn diagnostics_import_candidates_include_package_typings_root_specifier() {
    let mut project = Project::new();
    project.set_file(
        "/home/src/workspaces/project/node_modules/@angular/forms/package.json".to_string(),
        r#"{
  "name": "@angular/forms",
  "typings": "./forms.d.ts"
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/node_modules/@angular/forms/forms.d.ts".to_string(),
        "export class PatternValidator {}\n".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "lib": ["es5"]
  }
}"#
        .to_string(),
    );
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
        "/home/src/workspaces/project/index.ts".to_string(),
        "PatternValidator".to_string(),
    );

    let diagnostics = vec![LspDiagnostic {
        range: Range::new(Position::new(0, 0), Position::new(0, 16)),
        message: "Cannot find name 'PatternValidator'.".to_string(),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
        severity: None,
        source: None,
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    }];

    let specs: Vec<String> = project
        .get_import_candidates_for_diagnostics(
            "/home/src/workspaces/project/index.ts",
            &diagnostics,
        )
        .into_iter()
        .filter(|candidate| candidate.local_name == "PatternValidator")
        .map(|candidate| candidate.module_specifier)
        .collect();

    assert!(
        specs.iter().any(|specifier| specifier == "@angular/forms"),
        "expected diagnostics auto-import candidate from @angular/forms, got {specs:?}"
    );
}

#[test]
fn diagnostics_import_candidates_include_pnpm_store_dependency_alias() {
    let mut project = Project::new();
    project.set_file(
        "/home/src/workspaces/project/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "module": "commonjs",
    "lib": ["es5"]
  }
}"#
        .to_string(),
    );
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
    project.set_file(
        "/home/src/workspaces/project/index.ts".to_string(),
        "autorun".to_string(),
    );

    let diagnostics = vec![LspDiagnostic {
        range: Range::new(Position::new(0, 0), Position::new(0, 7)),
        message: "Cannot find name 'autorun'.".to_string(),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
        severity: None,
        source: None,
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    }];

    let specs: Vec<String> = project
        .get_import_candidates_for_diagnostics(
            "/home/src/workspaces/project/index.ts",
            &diagnostics,
        )
        .into_iter()
        .filter(|candidate| candidate.local_name == "autorun")
        .map(|candidate| candidate.module_specifier)
        .collect();

    assert!(
        specs.iter().any(|specifier| specifier == "mobx"),
        "expected diagnostics auto-import candidate from pnpm package alias `mobx`, got {specs:?}"
    );
}

#[test]
fn diagnostics_import_candidates_non_relative_pref_keeps_relative_cross_project_specifier() {
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
        "export function square(n: number) { return n * 2; }\n".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/web/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "lib": ["es5"],
    "module": "esnext",
    "moduleResolution": "node",
    "noEmit": true,
    "baseUrl": "."
  },
  "include": ["src"],
  "references": [{ "path": "../common" }]
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/web/src/MyApp.ts".to_string(),
        "import { square } from \"../../common/dist/src/MyModule\";\n".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/web/src/Helper.ts".to_string(),
        "square(2);\n".to_string(),
    );

    let diagnostics = vec![LspDiagnostic {
        range: Range::new(Position::new(0, 0), Position::new(0, 6)),
        message: "Cannot find name 'square'.".to_string(),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
        severity: None,
        source: None,
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    }];

    let specs: Vec<String> = project
        .get_import_candidates_for_diagnostics(
            "/home/src/workspaces/project/web/src/Helper.ts",
            &diagnostics,
        )
        .into_iter()
        .filter(|candidate| candidate.local_name == "square")
        .map(|candidate| candidate.module_specifier)
        .collect();

    assert!(
        specs
            .iter()
            .any(|specifier| specifier == "../../common/src/MyModule"),
        "expected diagnostics auto-import candidate ../../common/src/MyModule under non-relative preference, got {specs:?}"
    );
}

#[test]
fn diagnostics_import_candidates_include_symlinked_workspace_dependency_name() {
    let mut project = Project::new();
    project.set_file(
        "/home/src/workspaces/project/a/package.json".to_string(),
        r#"{
  "dependencies": {
    "b": "*"
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/a/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "lib": ["es5"],
    "module": "commonjs",
    "target": "esnext"
  },
  "references": [{ "path": "../b" }]
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/a/index.ts".to_string(),
        "new Shape".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/b/package.json".to_string(),
        r#"{
  "types": "out/index.d.ts"
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/b/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "lib": ["es5"],
    "outDir": "out",
    "composite": true
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/b/index.ts".to_string(),
        "export class Shape {}\n".to_string(),
    );

    let diagnostics = vec![LspDiagnostic {
        range: Range::new(Position::new(0, 4), Position::new(0, 9)),
        message: "Cannot find name 'Shape'.".to_string(),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
        severity: None,
        source: None,
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    }];

    let specs: Vec<String> = project
        .get_import_candidates_for_diagnostics(
            "/home/src/workspaces/project/a/index.ts",
            &diagnostics,
        )
        .into_iter()
        .filter(|candidate| candidate.local_name == "Shape")
        .map(|candidate| candidate.module_specifier)
        .collect();

    assert!(
        specs.iter().any(|specifier| specifier == "b"),
        "expected diagnostics auto-import candidate `b` for referenced workspace package, got {specs:?}"
    );
}

#[test]
fn diagnostics_import_candidates_include_bare_and_deep_paths_for_reexported_types_entry() {
    let mut project = Project::new();
    project.set_file(
        "/home/src/workspaces/project/package.json".to_string(),
        r#"{
  "dependencies": {
    "react-hook-form": "*"
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/node_modules/react-hook-form/package.json".to_string(),
        r#"{
  "types": "dist/index.d.ts"
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/node_modules/react-hook-form/dist/index.d.ts".to_string(),
        "export * from \"./useForm\";\n".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/node_modules/react-hook-form/dist/useForm.d.ts".to_string(),
        "export declare function useForm(): void;\n".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/index.ts".to_string(),
        "useForm".to_string(),
    );

    let diagnostics = vec![LspDiagnostic {
        range: Range::new(Position::new(0, 0), Position::new(0, 7)),
        message: "Cannot find name 'useForm'.".to_string(),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
        severity: None,
        source: None,
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    }];

    let specs: Vec<String> = project
        .get_import_candidates_for_diagnostics(
            "/home/src/workspaces/project/index.ts",
            &diagnostics,
        )
        .into_iter()
        .filter(|candidate| candidate.local_name == "useForm")
        .map(|candidate| candidate.module_specifier)
        .collect();

    assert!(
        specs.iter().any(|specifier| specifier == "react-hook-form"),
        "expected bare react-hook-form diagnostics auto-import candidate, got {specs:?}"
    );
    assert!(
        specs
            .iter()
            .any(|specifier| specifier == "react-hook-form/dist/useForm"),
        "expected deep react-hook-form/dist/useForm diagnostics auto-import candidate, got {specs:?}"
    );
}

#[test]
fn diagnostics_import_candidates_subpackage_allowed_when_parent_absent_from_project_deps() {
    // The project's package.json lists "typescript" only. There is no parent
    // preact/package.json. preact/hooks has its own package.json so it should
    // still be offered as an auto-import candidate.
    let mut project = Project::new();
    project.set_file(
        "/project/app.tsx".to_string(),
        "const state = useMemo(() => 'Hello', []);".to_string(),
    );
    project.set_file(
        "/project/package.json".to_string(),
        r#"{ "name": "my-app", "dependencies": { "typescript": "^5.0.0" } }"#.to_string(),
    );
    // No /project/node_modules/preact/package.json — only the subpackage.
    project.set_file(
        "/project/node_modules/preact/hooks/package.json".to_string(),
        r#"{ "name": "hooks", "version": "0.1.0", "types": "src/index.d.ts" }"#.to_string(),
    );
    project.set_file(
            "/project/node_modules/preact/hooks/src/index.d.ts".to_string(),
            "export declare function useMemo<T>(factory: () => T, inputs: ReadonlyArray<unknown> | undefined): T;\n".to_string(),
        );

    let diagnostics = vec![LspDiagnostic {
        range: Range::new(Position::new(0, 14), Position::new(0, 21)),
        message: "Cannot find name 'useMemo'.".to_string(),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
        severity: None,
        source: None,
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    }];

    let specs: Vec<String> = project
        .get_import_candidates_for_diagnostics("/project/app.tsx", &diagnostics)
        .into_iter()
        .filter(|candidate| candidate.local_name == "useMemo")
        .map(|candidate| candidate.module_specifier)
        .collect();

    assert!(
        specs.iter().any(|specifier| specifier == "preact/hooks"),
        "expected preact/hooks candidate when project deps omit 'preact' but subpackage has own manifest, got {specs:?}"
    );
}

#[test]
fn diagnostics_import_candidates_use_parent_package_subpath_for_nested_package_manifest() {
    let mut project = Project::new();
    project.set_file(
        "/project/app.tsx".to_string(),
        "const state = useMemo(() => 'Hello', []);".to_string(),
    );
    project.set_file(
        "/project/node_modules/preact/package.json".to_string(),
        r#"{ "name": "preact", "version": "10.3.4", "types": "src/index.d.ts" }"#.to_string(),
    );
    project.set_file(
        "/project/node_modules/preact/hooks/package.json".to_string(),
        r#"{ "name": "hooks", "version": "0.1.0", "types": "src/index.d.ts" }"#.to_string(),
    );
    project.set_file(
            "/project/node_modules/preact/hooks/src/index.d.ts".to_string(),
            "export declare function useMemo<T>(factory: () => T, inputs: ReadonlyArray<unknown> | undefined): T;\n".to_string(),
        );

    let diagnostics = vec![LspDiagnostic {
        range: Range::new(Position::new(0, 14), Position::new(0, 21)),
        message: "Cannot find name 'useMemo'.".to_string(),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
        severity: None,
        source: None,
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    }];

    let specs: Vec<String> = project
        .get_import_candidates_for_diagnostics("/project/app.tsx", &diagnostics)
        .into_iter()
        .filter(|candidate| candidate.local_name == "useMemo")
        .map(|candidate| candidate.module_specifier)
        .collect();

    assert!(
        specs.iter().any(|specifier| specifier == "preact/hooks"),
        "expected diagnostics auto-import candidate preact/hooks, got {specs:?}"
    );
}

#[test]
fn auto_import_candidates_include_direct_exported_class_declarations() {
    let mut project = Project::new();
    project.set_file(
        "/home/src/workspaces/project/thing2A.ts".to_string(),
        "export class Thing2A {}".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/entry.ts".to_string(),
        "Thing2".to_string(),
    );

    let specs: Vec<String> = project
        .get_import_candidates_for_prefix("/home/src/workspaces/project/entry.ts", "Thing2")
        .into_iter()
        .filter(|candidate| candidate.local_name == "Thing2A")
        .map(|candidate| candidate.module_specifier)
        .collect();

    assert!(
        specs.iter().any(|specifier| specifier == "./thing2A"),
        "expected direct exported class declaration candidate ./thing2A, got {specs:?}"
    );
}

#[test]
fn auto_import_candidates_include_exported_functions_in_ambient_modules() {
    let mut project = Project::new();
    project.set_file(
        "/home/src/workspaces/project/ambient.d.ts".to_string(),
        r#"declare module "fs" {
  export function accessSync(path: string): void;
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/index.ts".to_string(),
        "access".to_string(),
    );

    let specs: Vec<String> = project
        .get_import_candidates_for_prefix("/home/src/workspaces/project/index.ts", "access")
        .into_iter()
        .filter(|candidate| candidate.local_name == "accessSync")
        .map(|candidate| candidate.module_specifier)
        .collect();

    assert!(
        specs.iter().any(|specifier| specifier == "fs"),
        "expected ambient module export candidate fs, got {specs:?}"
    );
}

#[test]
fn auto_import_candidates_do_not_infer_workspace_package_name_without_requesting_package() {
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
        "/home/src/workspaces/project/packages/utils/package.json".to_string(),
        r#"{
  "name": "utils",
  "version": "1.0.0",
  "main": "dist/index.js"
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/packages/utils/dist/index.d.ts".to_string(),
        "export const x: number;".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/packages/app/dist/index.d.ts".to_string(),
        "import {} from \"utils\";\nexport const app: number;".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/script.ts".to_string(),
        "import {} from \"./packages/app/dist/index.js\";\nx".to_string(),
    );

    let specs: Vec<String> = project
        .get_import_candidates_for_prefix("/home/src/workspaces/project/script.ts", "x")
        .into_iter()
        .map(|candidate| candidate.module_specifier)
        .collect();

    assert!(
        !specs.iter().any(|specifier| specifier == "utils/dist"),
        "did not expect inferred workspace package specifier utils/dist without requesting package metadata, got {specs:?}"
    );
    assert!(
        specs
            .iter()
            .any(|specifier| specifier == "./packages/utils/dist/index.js"),
        "expected relative candidate ./packages/utils/dist/index.js, got {specs:?}"
    );
}

#[test]
fn auto_import_candidates_include_component_from_react_types_package() {
    let mut project = Project::new();
    project.set_file(
        "/home/src/workspaces/project/tsconfig.json".to_string(),
        r#"{ "compilerOptions": { "module": "commonjs", "lib": ["es2019"], "types": ["*"] } }"#
            .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/package.json".to_string(),
        r#"{ "dependencies": { "antd": "*", "react": "*" } }"#.to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/node_modules/@types/react/index.d.ts".to_string(),
        "export declare function Component(): void;".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/node_modules/antd/index.d.ts".to_string(),
        "import \"react\";".to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/index.ts".to_string(),
        "Compon".to_string(),
    );

    let specs: Vec<String> = project
        .get_import_candidates_for_prefix("/home/src/workspaces/project/index.ts", "Compon")
        .into_iter()
        .filter(|candidate| candidate.local_name == "Component")
        .map(|candidate| candidate.module_specifier)
        .collect();

    assert!(
        specs.iter().any(|specifier| specifier == "react"),
        "expected react auto-import candidate for Component, got {specs:?}"
    );
}

#[test]
fn auto_import_candidates_include_ambient_reexport_source_module() {
    let mut project = Project::new();
    project.set_file(
        "/home/src/workspaces/project/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "module": "commonjs",
    "types": ["*"],
    "lib": ["es5"]
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/node_modules/@types/node/index.d.ts".to_string(),
        r#"declare module "fs" {
  export function accessSync(path: string): void;
}"#
        .to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/node_modules/@types/fs-extra/index.d.ts".to_string(),
        r#"export * from "fs";"#.to_string(),
    );
    project.set_file(
        "/home/src/workspaces/project/index.ts".to_string(),
        "access".to_string(),
    );

    let specs: Vec<String> = project
        .get_import_candidates_for_prefix("/home/src/workspaces/project/index.ts", "access")
        .into_iter()
        .filter(|candidate| candidate.local_name == "accessSync")
        .map(|candidate| candidate.module_specifier)
        .collect();

    assert!(
        specs.iter().any(|specifier| specifier == "fs"),
        "expected fs ambient module candidate, got {specs:?}"
    );
    assert!(
        specs.iter().any(|specifier| specifier == "fs-extra"),
        "expected fs-extra re-export candidate, got {specs:?}"
    );
}

#[test]
fn has_wildcard_reexport_cached_on_project_file() {
    use super::super::ProjectFile;
    use super::super::core::compute_has_wildcard_reexport;

    // Files without wildcard re-exports.
    let plain = ProjectFile::new("/a.ts".to_string(), "export const x = 1;".to_string());
    assert!(
        !plain.has_wildcard_reexport,
        "plain export should not be flagged as wildcard reexport"
    );

    let named_only = ProjectFile::new("/b.ts".to_string(), "export { x } from './a';".to_string());
    assert!(
        !named_only.has_wildcard_reexport,
        "named-only re-export should not be flagged"
    );

    // Files with wildcard re-exports.
    let star = ProjectFile::new("/c.ts".to_string(), "export * from './a';".to_string());
    assert!(star.has_wildcard_reexport, "export * should be flagged");

    let default_reexport = ProjectFile::new(
        "/d.ts".to_string(),
        "export { default } from './a';".to_string(),
    );
    assert!(
        default_reexport.has_wildcard_reexport,
        "export {{ default }} re-export should be flagged"
    );

    // Compute_has_wildcard_reexport agrees with the cached field (both code paths exercise).
    for (file, expected) in [
        (&plain, false),
        (&named_only, false),
        (&star, true),
        (&default_reexport, true),
    ] {
        let computed = compute_has_wildcard_reexport(file.arena(), file.root());
        assert_eq!(
            computed, expected,
            "compute_has_wildcard_reexport disagrees with expected for {:?}",
            file.file_name
        );
    }
}

#[test]
fn has_wildcard_reexport_cache_updates_with_project_file_source_changes() {
    use super::super::ProjectFile;

    let mut file = ProjectFile::new(
        "/barrel.ts".to_string(),
        "export { x } from './a';".to_string(),
    );
    assert!(
        !file.has_wildcard_reexport,
        "named-only re-export should start with cached flag off"
    );

    file.update_source("export * from './a';".to_string());
    assert!(
        file.has_wildcard_reexport,
        "full source update adding export * should refresh cached flag"
    );

    file.update_source("export { default } from './a';".to_string());
    assert!(
        file.has_wildcard_reexport,
        "full source update adding default re-export should keep cached flag on"
    );

    file.update_source("export { x } from './a';".to_string());
    assert!(
        !file.has_wildcard_reexport,
        "full source update removing wildcard/default re-export should refresh cached flag off"
    );
}

#[test]
fn file_exclude_patterns_applied_once_per_request_not_per_symbol() {
    // This test verifies that when multiple symbols from the same excluded file
    // are searched, none of them appear as candidates — proving the precomputed
    // excluded_file_set gates correctly across symbol iterations.
    let mut project = Project::new();
    project.set_auto_import_file_exclude_patterns(vec!["**/excluded/**".to_string()]);
    project.set_file(
        "/tsconfig.json".to_string(),
        r#"{"compilerOptions":{"module":"commonjs"}}"#.to_string(),
    );
    project.set_file(
            "/node_modules/excluded/index.d.ts".to_string(),
            "export declare function alpha(): void;\nexport declare function beta(): void;\nexport declare function gamma(): void;".to_string(),
        );
    project.set_file(
        "/node_modules/included/index.d.ts".to_string(),
        "export declare function alpha(): void;".to_string(),
    );
    project.set_file("/src/index.ts".to_string(), "alpha".to_string());

    let candidates = project.get_import_candidates_for_prefix("/src/index.ts", "al");

    // The `included` package's `alpha` is reachable.
    assert!(
        candidates
            .iter()
            .any(|c| c.local_name == "alpha" && c.module_specifier.contains("included")),
        "expected alpha from included package, got {candidates:?}"
    );

    // None of the excluded package's symbols appear.
    for excluded_fn in ["alpha", "beta", "gamma"] {
        assert!(
            !candidates
                .iter()
                .any(|c| c.local_name == excluded_fn && c.module_specifier.contains("excluded")),
            "expected {excluded_fn} from excluded package to be hidden, got {candidates:?}"
        );
    }
}

#[test]
fn diagnostics_import_candidates_include_unexported_jsdoc_typedef_as_inline_import() {
    let mut project = Project::new();
    project.set_file(
        "/a.js".to_string(),
        "export {};\n/** @typedef {number} T */\n".to_string(),
    );
    project.set_file(
        "/b.js".to_string(),
        "/** @type {T} */\nconst x = 0;\n".to_string(),
    );

    let diagnostics = vec![LspDiagnostic {
        range: Range::new(Position::new(0, 11), Position::new(0, 12)),
        message: "Cannot find name 'T'.".to_string(),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
        severity: None,
        source: None,
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    }];

    let candidates = project.get_import_candidates_for_diagnostics("/b.js", &diagnostics);
    let t_candidate = candidates
        .iter()
        .find(|c| c.local_name == "T")
        .unwrap_or_else(|| panic!("expected a 'T' candidate, got {candidates:?}"));

    // Bare, no `.js`: the inline `import("./mod").Name` type query tsc
    // emits for this fix never runs the ending-preference sniffing a real
    // added import statement does (oracle-verified via
    // `importNameCodeFix_importType.ts`, which expects
    // `import("./a").T` from this exact file pair).
    assert_eq!(t_candidate.module_specifier, "./a");
    assert!(t_candidate.is_type_only);
    assert!(t_candidate.jsdoc_typedef);
}

#[test]
fn diagnostics_import_candidates_prefer_exported_value_over_jsdoc_typedef_path_in_js_file() {
    let mut project = Project::new();
    project.set_file(
        "/a.js".to_string(),
        "export function foo() {}\n".to_string(),
    );
    project.set_file("/b.js".to_string(), "foo();\n".to_string());

    let diagnostics = vec![LspDiagnostic {
        range: Range::new(Position::new(0, 0), Position::new(0, 3)),
        message: "Cannot find name 'foo'.".to_string(),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
        severity: None,
        source: None,
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    }];

    let candidates = project.get_import_candidates_for_diagnostics("/b.js", &diagnostics);
    let foo_candidate = candidates
        .iter()
        .find(|c| c.local_name == "foo")
        .unwrap_or_else(|| panic!("expected a 'foo' candidate, got {candidates:?}"));

    assert!(!foo_candidate.is_type_only);
    assert!(!foo_candidate.jsdoc_typedef);
}

/// End-to-end regression for `importNameCodeFix_importType.ts`: a real
/// checker-produced `TS2304` on an unimported local JSDoc `@typedef`
/// resolves to a code action rewriting `@type {T}` to
/// `@type {import("./a").T}` — bare, no `.js` extension, even though a
/// plain JS-to-JS *value* import of the same pair of files keeps `.js`
/// (see `jsconfig_paths_mapping_outranks_relative_for_shortest_preference`
/// in `module_specifiers/tests.rs`). The inline `import("./mod").Name` type
/// query tsc emits for this fix never runs the ending-preference sniffing a
/// real added import statement does — it always emits a bare specifier.
#[test]
fn jsdoc_typedef_inline_import_code_action_strips_js_extension() {
    let mut project = Project::new();
    project.set_file(
        "/tsconfig.json".to_string(),
        r#"{"compilerOptions":{"allowJs":true,"checkJs":true}}"#.to_string(),
    );
    project.set_file(
        "/a.js".to_string(),
        "export {};\n/** @typedef {number} T */\n".to_string(),
    );
    project.set_file(
        "/b.js".to_string(),
        "/** @type {T} */\nconst x = 0;\n".to_string(),
    );

    let diags = project.get_diagnostics("/b.js").unwrap_or_default();
    let t_diag = diags
        .iter()
        .find(|d| d.code == Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME))
        .cloned()
        .expect("expected a real TS2304 diagnostic for the unimported JSDoc typedef");

    let actions = project
        .get_code_actions("/b.js", t_diag.range, vec![t_diag], None)
        .unwrap_or_default();
    let import_action = actions
        .iter()
        .find(|a| a.title.starts_with("Import 'T'"))
        .unwrap_or_else(|| panic!("expected an import quickfix for 'T', got {actions:?}"));

    assert_eq!(import_action.title, "Import 'T' via 'import(\"./a\").T'");
    let edits = &import_action
        .edit
        .as_ref()
        .expect("import action should carry a workspace edit")
        .changes["/b.js"];
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, "import(\"./a\").");
}
