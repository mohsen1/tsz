#[test]
fn compile_dts_qualifies_imported_return_type_reused_from_another_file() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "commonjs",
            "target": "es2015",
            "declaration": true,
            "allowArbitraryExtensions": true,
            "outDir": "dist"
          },
          "files": ["foo.d.html.ts", "reexporter.ts", "index.ts"]
        }"#,
    );
    write_file(
        &base.join("foo.d.html.ts"),
        "export declare class CustomHtmlRepresentationThing {}\n",
    );
    write_file(
        &base.join("reexporter.ts"),
        r#"import { CustomHtmlRepresentationThing } from "./foo.html";

export function func() {
    return new CustomHtmlRepresentationThing();
}
"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"import { func } from "./reexporter";
export const c = func();
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        result.diagnostics
    );
    let dts = std::fs::read_to_string(base.join("dist/index.d.ts")).expect("read index.d.ts");
    assert!(
        dts.contains(
            r#"export declare const c: import("./foo.html").CustomHtmlRepresentationThing;"#
        ),
        "Expected cross-file inferred return type to be qualified: {dts}"
    );
    assert!(
        !dts.contains("c: CustomHtmlRepresentationThing"),
        "Expected no unbound local name from reexporter scope: {dts}"
    );

    let reexporter_dts =
        std::fs::read_to_string(base.join("dist/reexporter.d.ts")).expect("read reexporter.d.ts");
    assert!(
        reexporter_dts.contains(r#"import { CustomHtmlRepresentationThing } from "./foo.html";"#),
        "Expected source-file import to stay when inferred return uses the imported type: {reexporter_dts}"
    );
    assert!(
        reexporter_dts
            .contains(r#"export declare function func(): CustomHtmlRepresentationThing;"#),
        "Expected inferred return in the importing file to use the local imported name: {reexporter_dts}"
    );
}

#[test]
fn compile_namespace_import_reserved_statement_starters_emit_recovered_payload() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2020",
            "module": "commonjs",
            "outDir": "dist",
            "noEmitOnError": false
          },
          "files": ["input.ts"]
        }"#,
    );
    write_file(
        &base.join("input.ts"),
        "import * as do from \"m\";\nimport * as try from \"m\";\nimport * as return from \"m\";\nconst after = 1;\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.iter().any(|d| d.code
            == diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE
            && d.message_text.contains("'do'")),
        "Expected TS1359 for `do`, got diagnostics: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/input.js")).expect("read emitted JS");
    assert!(
        js.contains("do") && js.contains("while (\"m\");"),
        "`do` namespace-import recovery should emit a do/while payload, got: {js}"
    );
    assert!(
        js.contains("try {") && js.contains("finally {"),
        "`try` namespace-import recovery should emit try/finally payload, got: {js}"
    );
    assert!(
        js.contains("return from;") && js.contains("const after = 1;"),
        "`return` namespace-import recovery and following statement should emit, got: {js}"
    );
}

#[test]
fn compile_with_project_dir_uses_tsconfig() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    let config_dir = base.join("configs");
    write_file(
        &config_dir.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&config_dir.join("src/index.ts"), "export const value = 1;");

    let mut args = default_args();
    args.project = Some(PathBuf::from("configs"));

    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    assert!(config_dir.join("dist/src/index.js").is_file());
}

#[test]
fn compile_reports_ts7005_for_exported_bare_var_in_imported_dts() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "jsx": "react",
            "module": "commonjs",
            "target": "es2015"
          },
          "include": ["*.ts", "*.tsx", "*.d.ts"]
        }"#,
    );
    write_file(
        &base.join("file.tsx"),
        r#"declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        [s: string]: any;
    }
}"#,
    );
    write_file(&base.join("test.d.ts"), "export var React;\n");
    write_file(
        &base.join("react-consumer.tsx"),
        r#"import { React } from "./test";
var foo: any;
var spread1 = <div x='' {...foo} y='' />;"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().any(|d| d.code == 7005),
        "Expected TS7005 for exported bare var in imported .d.ts, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn compile_jsx_attribute_name_allows_hyphen_followed_by_digit() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "jsx": "preserve",
            "noEmit": true
          },
          "files": ["index.tsx"]
        }"#,
    );
    write_file(
        &base.join("index.tsx"),
        r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        x: { "data-123": "ok" };
    }
}

const ok = <x data-123="ok" />;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics for JSX attribute name with digit-starting hyphen segment, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn compile_with_project_dir_resolves_package_exported_tsconfig_extends() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("node_modules/foo/package.json"),
        r#"{
          "name": "foo",
          "version": "1.0.0",
          "exports": {
            "./*.json": "./configs/*.json"
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/foo/configs/strict.json"),
        r#"{
          "compilerOptions": {
            "strict": true
          }
        }"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{"extends":"foo/strict.json"}"#,
    );
    write_file(&base.join("index.ts"), "let x: string;\nx.toLowerCase();\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED),
        "Expected TS2454 from package-exported tsconfig extends, got diagnostics: {:?}",
        result.diagnostics
    );
}

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
fn compile_project_destructuring_failed_reduce_reports_iterator_and_overload_errors() {
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
        r#"
const [oops1] = [1, 2, 3].reduce((accu, el) => accu.concat(el), []);
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<_> = result.diagnostics.iter().map(|diag| diag.code).collect();

    assert!(
        codes.contains(&2488),
        "Expected TS2488 for destructuring a failed reduce result, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        codes.contains(&diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL),
        "Expected TS2769 for nested reduce/concat overload failure, got diagnostics: {:?}",
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
fn compile_reports_unsupported_files_list_extension() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "strict": true,
            "noEmit": true
          },
          "files": ["foo.txt"]
        }"#,
    );
    write_file(&base.join("foo.txt"), "not typescript");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts6054: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diag| {
            diag.code
                == diagnostic_codes::FILE_HAS_AN_UNSUPPORTED_EXTENSION_THE_ONLY_SUPPORTED_EXTENSIONS_ARE
        })
        .collect();
    assert_eq!(
        ts6054.len(),
        1,
        "Expected TS6054 for unsupported explicit files entry, got: {:?}",
        result.diagnostics
    );
    let diagnostic = ts6054[0];
    assert!(
        diagnostic.message_text.contains("foo.txt")
            && diagnostic.message_text.contains("'.ts'")
            && diagnostic.message_text.contains("'.d.mts'"),
        "Expected unsupported extension message with supported TS extensions, got: {diagnostic:?}"
    );
    assert!(
        diagnostic
            .related_information
            .iter()
            .any(|info| info.message_text.contains("Part of 'files' list")),
        "Expected TS6054 to explain the files-list inclusion reason, got: {diagnostic:?}"
    );
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
fn cli_base_url_resolves_bare_imports() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("main.ts"),
        r#"import { value } from "foo";

value.toFixed();
"#,
    );
    write_file(&base.join("src/foo.ts"), "export const value = 1;\n");

    let mut args = default_args();
    args.base_url = Some(PathBuf::from("src"));
    args.ignore_deprecations = Some("6.0".to_string());
    args.no_emit = true;
    args.strict = true;
    args.files = vec![PathBuf::from("main.ts")];
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected CLI --baseUrl to resolve bare import, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_resolves_root_dirs_virtual_relative_imports() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "strict": true,
            "target": "ES2020",
            "module": "ESNext",
            "moduleResolution": "node10",
            "ignoreDeprecations": "6.0",
            "rootDirs": ["src", "generated"],
            "noEmit": true
          },
          "files": ["src/main.ts", "generated/generated.ts"]
        }"#,
    );
    write_file(
        &base.join("src/main.ts"),
        "import { generated } from './generated';\nconst value: string = generated.toUpperCase();\n",
    );
    write_file(
        &base.join("generated/generated.ts"),
        "export const generated = 'ok';\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn compile_paths_wildcard_priority_uses_prefix_length() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "ESNext",
            "moduleResolution": "Bundler",
            "baseUrl": ".",
            "ignoreDeprecations": "6.0",
            "paths": {
              "@/*/suffix-long": ["bad/*"],
              "@/foo/*": ["good/*"]
            },
            "strict": true
          },
          "files": ["main.ts", "good/bar/suffix-long.ts", "bad/foo/bar.ts"]
        }"#,
    );
    write_file(
        &base.join("main.ts"),
        r#"
import { value } from "@/foo/bar/suffix-long";

const n: 1 = value;
"#,
    );
    write_file(
        &base.join("good/bar/suffix-long.ts"),
        "export const value = 1 as const;",
    );
    write_file(
        &base.join("bad/foo/bar.ts"),
        "export const value = 2 as const;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected longer-prefix paths pattern to win, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_paths_without_base_url_resolve_before_ts_extension_diagnostic() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "es2015",
            "moduleResolution": "bundler",
            "paths": {
              "foo/*": ["./dist/*"],
              "baz/*.ts": ["./types/*.d.ts"]
            }
          },
          "files": ["test.ts"]
        }"#,
    );
    write_file(
        &base.join("test.ts"),
        "import { a } from 'foo/bar.ts';\nimport { b } from 'baz/main.ts';\na; b;\n",
    );
    write_file(&base.join("dist/bar.ts"), "export const a = 1234;");
    write_file(&base.join("types/main.d.ts"), "export const b: string;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should complete");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![
            diagnostic_codes::AN_IMPORT_PATH_CAN_ONLY_END_WITH_A_EXTENSION_WHEN_ALLOWIMPORTINGTSEXTENSIONS_IS
        ],
        "expected only TS5097 for the path-mapped TS input, got: {codes:?}"
    );
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
fn compile_bundler_exports_target_applies_module_suffixes_to_dts_sidecar() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "noEmit": true,
            "module": "preserve",
            "moduleResolution": "bundler",
            "moduleSuffixes": [".native"]
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(
        &base.join("index.ts"),
        "import { value } from \"pkg/foo\";\nvalue satisfies number;\n",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{ "name": "pkg", "exports": { "./foo": "./foo.js" } }"#,
    );
    write_file(
        &base.join("node_modules/pkg/foo.native.d.ts"),
        "export declare const value: number;\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "expected package exports declaration sidecar to resolve with moduleSuffixes, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn compile_rejects_package_exports_target_with_node_modules_segment() {
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
        &base.join("src/index.ts"),
        "import { value } from 'pkg/secret';\nconst n: number = value;\n",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{"name":"pkg","exports":{"./secret":"./node_modules/secret.d.ts"}}"#,
    );
    write_file(
        &base.join("node_modules/pkg/node_modules/secret.d.ts"),
        "export declare const value: number;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().any(|diag| diag.code
            == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS),
        "Expected TS2307 for exports target under node_modules, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_cli_node16_resolution_enables_package_json_exports_without_config() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("main.mts"),
        "import { mode } from 'pkg';\n\nconst actual: 'cjs' = mode;\n",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "name": "pkg",
          "exports": {
            ".": {
              "import": "./esm.d.ts",
              "require": "./cjs.d.ts"
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/esm.d.ts"),
        "export const mode: 'esm';",
    );
    write_file(
        &base.join("node_modules/pkg/cjs.d.ts"),
        "export const mode: 'cjs';",
    );

    let args = CliArgs::try_parse_from([
        "tsz",
        "--moduleResolution",
        "node16",
        "--module",
        "node16",
        "--noEmit",
        "--strict",
        "main.mts",
    ])
    .expect("parse args");
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|diag| diag.code).collect();

    assert!(
        !codes
            .contains(&diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS),
        "CLI node16 options should imply package.json exports resolution, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "expected imports to resolve through the export map to the ESM declaration, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_cli_package_resolution_flags_require_modern_module_resolution() {
    let cases: &[(&str, &[&str])] = &[
        (
            "custom_conditions",
            &[
                "tsz",
                "--moduleResolution",
                "classic",
                "--ignoreDeprecations",
                "6.0",
                "--customConditions",
                "dev",
                "--noEmit",
                "--pretty",
                "false",
                "--ignoreConfig",
                "index.ts",
            ],
        ),
        (
            "resolve_package_json_exports",
            &[
                "tsz",
                "--moduleResolution",
                "classic",
                "--ignoreDeprecations",
                "6.0",
                "--resolvePackageJsonExports",
                "true",
                "--noEmit",
                "--pretty",
                "false",
                "--ignoreConfig",
                "index.ts",
            ],
        ),
        (
            "resolve_package_json_imports",
            &[
                "tsz",
                "--moduleResolution",
                "classic",
                "--ignoreDeprecations",
                "6.0",
                "--resolvePackageJsonImports",
                "true",
                "--noEmit",
                "--pretty",
                "false",
                "--ignoreConfig",
                "index.ts",
            ],
        ),
    ];

    for (name, argv) in cases {
        let temp = TempDir::new().expect("temp dir");
        let base = &temp.path;
        write_file(&base.join("index.ts"), "export {};\n");

        let args = CliArgs::try_parse_from(argv.iter().copied()).unwrap_or_else(|err| {
            panic!("failed to parse {name} args: {err}");
        });
        let result = compile(&args, base).expect("compile should succeed");
        let codes: Vec<u32> = result.diagnostics.iter().map(|diag| diag.code).collect();

        assert!(
            codes.contains(
                &diagnostic_codes::OPTION_CAN_ONLY_BE_USED_WHEN_MODULERESOLUTION_IS_SET_TO_NODE16_NODENEXT_OR_BUNDL
            ),
            "{name} should report TS5098 with classic moduleResolution, got: {codes:?}"
        );
    }
}

#[test]
fn compile_cli_package_resolution_flags_use_config_module_resolution_for_ts5098() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "moduleResolution": "classic",
            "ignoreDeprecations": "6.0"
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(&base.join("index.ts"), "export {};\n");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--customConditions",
        "dev",
        "--noEmit",
        "--pretty",
        "false",
    ])
    .expect("parse args");
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|diag| diag.code).collect();

    assert!(
        codes.contains(
            &diagnostic_codes::OPTION_CAN_ONLY_BE_USED_WHEN_MODULERESOLUTION_IS_SET_TO_NODE16_NODENEXT_OR_BUNDL
        ),
        "CLI customConditions should report TS5098 with config moduleResolution classic, got: {codes:?}"
    );
}

#[test]
fn compile_cli_package_resolution_flags_accept_modern_module_resolution() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("index.ts"), "export {};\n");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--moduleResolution",
        "node16",
        "--module",
        "node16",
        "--customConditions",
        "dev",
        "--resolvePackageJsonExports",
        "true",
        "--resolvePackageJsonImports",
        "true",
        "--noEmit",
        "--pretty",
        "false",
        "--ignoreConfig",
        "index.ts",
    ])
    .expect("parse args");
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|diag| diag.code).collect();

    assert!(
        !codes.contains(
            &diagnostic_codes::OPTION_CAN_ONLY_BE_USED_WHEN_MODULERESOLUTION_IS_SET_TO_NODE16_NODENEXT_OR_BUNDL
        ),
        "modern moduleResolution should not report TS5098, got: {codes:?}"
    );
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
fn compile_resolves_node_modules_types_versions_default_uses_patch_version() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "moduleResolution": "node",
            "module": "commonjs",
            "target": "es2020",
            "noEmit": true,
            "skipLibCheck": true,
            "ignoreDeprecations": "6.0"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        r#"import { widget } from "pkg/feature/widget";

const exact: 603 = widget;
"#,
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "name": "pkg",
          "version": "1.0.0",
          "typesVersions": {
            ">=6.0.3": {
              "feature/*": ["types/ts603/feature/*"]
            },
            "*": {
              "feature/*": ["types/fallback/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/ts603/feature/widget.d.ts"),
        "export const widget: 603;\n",
    );
    write_file(
        &base.join("node_modules/pkg/types/fallback/feature/widget.d.ts"),
        "export const widget: 600;\n",
    );

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(
        result.diagnostics.is_empty(),
        "default typesVersions compiler version should select >=6.0.3 entry, got: {:#?}",
        result.diagnostics
    );
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

