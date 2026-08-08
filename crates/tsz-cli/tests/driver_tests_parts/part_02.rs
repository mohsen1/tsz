#[test]
fn compile_import_equals_const_enum_only_elides_require() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "commonjs",
            "outDir": "dist",
            "noCheck": true,
            "noLib": true
          },
          "files": ["m.d.ts", "main.ts"]
        }"#,
    );
    write_file(
        &base.join("m.d.ts"),
        "export const enum E { A = 1, B = 2 }\n",
    );
    write_file(
        &base.join("main.ts"),
        "import X = require(\"./m\");\nconst v = X.E.A;\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().all(|diag| diag.code
            != diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS),
        "expected import-equals module to resolve, got: {:?}",
        result.diagnostics
    );
    let js = std::fs::read_to_string(base.join("dist/main.js")).expect("read emitted JS");
    assert!(
        js.contains("1 /* X.E.A */"),
        "const enum member through import-equals alias should inline.\nOutput:\n{js}"
    );
    assert!(
        !js.contains("require(\"./m\")"),
        "require should be elided when the import-equals alias is only used for const enum access.\nOutput:\n{js}"
    );
}

#[test]
fn compile_namespace_import_const_enum_only_elides_require() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "commonjs",
            "outDir": "dist",
            "noCheck": true,
            "noLib": true
          },
          "files": ["m.d.ts", "main.ts"]
        }"#,
    );
    write_file(&base.join("m.d.ts"), "export const enum E { A = 1 }\n");
    write_file(
        &base.join("main.ts"),
        "import * as X from \"./m\";\nconst v = X.E.A;\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().all(|diag| diag.code
            != diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS),
        "expected namespace import module to resolve, got: {:?}",
        result.diagnostics
    );
    let js = std::fs::read_to_string(base.join("dist/main.js")).expect("read emitted JS");
    assert!(
        js.contains("1 /* X.E.A */"),
        "const enum member through namespace import should inline.\nOutput:\n{js}"
    );
    assert!(
        !js.contains("require(\"./m\")"),
        "namespace import should be elided when only used for const enum access.\nOutput:\n{js}"
    );
}

#[test]
fn compile_import_equals_const_enum_keeps_require_for_runtime_member() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "commonjs",
            "outDir": "dist",
            "noCheck": true,
            "noLib": true
          },
          "files": ["m.d.ts", "main.ts"]
        }"#,
    );
    write_file(
        &base.join("m.d.ts"),
        "export const enum E { A = 1 }\nexport const value: number;\n",
    );
    write_file(
        &base.join("main.ts"),
        "import X = require(\"./m\");\nconst v = X.E.A;\nconst runtime = X.value;\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().all(|diag| diag.code
            != diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS),
        "expected import-equals module to resolve, got: {:?}",
        result.diagnostics
    );
    let js = std::fs::read_to_string(base.join("dist/main.js")).expect("read emitted JS");
    assert!(
        js.contains("1 /* X.E.A */"),
        "const enum member through import-equals alias should inline.\nOutput:\n{js}"
    );
    assert!(
        js.contains("require(\"./m\")"),
        "runtime use through the same alias should keep the require.\nOutput:\n{js}"
    );
    assert!(
        js.contains("X.value"),
        "runtime member access should be preserved.\nOutput:\n{js}"
    );
}

#[test]
fn compile_allow_js_passthrough_emits_root_node_modules_js() {
    // `node_modules/untyped/index.js` is an explicit program root here (tsc's
    // `rootNames` — a `files` entry), not a file merely discovered by
    // resolving `require("untyped")`. tsc never applies `maxNodeModuleJsDepth`
    // to a root — it is a real program input, not something reached "by
    // descending into node_modules" — so `bug40140.js`'s `require('untyped')`
    // resolves to the root's own (real, checked) export shape `{}` instead of
    // TS7016's implicit `any`. Oracle-verified against `typescript@7.0.2`
    // (pinned): with `noCheck: true` the per-file semantic pass that would
    // report `TS2339` on `u.assignment`/`u.noError()` never runs, and no
    // TS7016 is produced either. See #16928.
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "allowJs": true,
            "checkJs": true,
            "module": "commonjs",
            "target": "es2015",
            "outDir": "out",
            "noCheck": true,
            "noLib": true
          },
          "files": ["node_modules/untyped/index.js", "bug40140.js"]
        }"#,
    );
    write_file(
        &base.join("node_modules/untyped/index.js"),
        "module.exports = {}",
    );
    write_file(
        &base.join("bug40140.js"),
        "const u = require('untyped');\nu.assignment.nested = true\nu.noError()\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().all(|diag| diag.code
            != diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
            && diag.code != 7016),
        "root untyped package must resolve without TS2307/TS7016, got: {:?}",
        result.diagnostics
    );
    assert_eq!(
        std::fs::read_to_string(base.join("out/node_modules/untyped/index.js"))
            .expect("read emitted JS"),
        "module.exports = {}"
    );
}

#[test]
fn compile_checked_js_prototype_optional_chain_method_call_suppresses_ts2531() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "strict": true,
            "allowJs": true,
            "checkJs": true,
            "noEmit": true,
            "pretty": false,
            "types": []
          },
          "files": ["index.js"]
        }"#,
    );
    write_file(
        &base.join("index.js"),
        r#"
// @ts-check
Element.prototype.remove = function () {
  this.parentNode?.removeChild(document.body);
};
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .all(|diag| diag.code != diagnostic_codes::OBJECT_IS_POSSIBLY_NULL),
        "optional chain should suppress TS2531, got diagnostics: {:?}",
        result.diagnostics
    );

    write_file(
        &base.join("index.js"),
        r#"
// @ts-check
Element.prototype.remove = function () {
  this.parentNode.removeChild(document.body);
};
"#,
    );

    let control = compile(&args, base).expect("compile should succeed");
    assert!(
        control
            .diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::OBJECT_IS_POSSIBLY_NULL),
        "plain nullable receiver should still report TS2531, got diagnostics: {:?}",
        control.diagnostics
    );
}

#[test]
fn compile_emit_bom_prefixes_output_files() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "emitBOM": true
          },
          "files": ["main.ts"]
        }"#,
    );
    write_file(&base.join("main.ts"), "const x = 1;\n");

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        result.diagnostics
    );
    let bytes = std::fs::read(base.join("dist/main.js")).expect("read output");
    assert!(
        bytes.starts_with(&[0xef, 0xbb, 0xbf]),
        "Expected emitted JS to start with UTF-8 BOM, got first bytes: {:?}",
        &bytes[..bytes.len().min(8)]
    );
}

#[test]
fn compile_single_source_amd_outfile_reports_removed_options() {
    // TS 7.0.2 removed `module: amd` (TS5108) and `outFile` (TS5102), and the
    // default `moduleResolution: bundler` is incompatible with AMD (TS5095,
    // anchored at the "compilerOptions" key since the option is defaulted).
    // No bundle is emitted.
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2022",
            "module": "amd",
            "outFile": "dist/bundle.js",
            "strict": true,
            "ignoreDeprecations": "6.0"
          },
          "files": ["main.ts"]
        }"#,
    );
    write_file(&base.join("main.ts"), "export const value = 1;");

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    let mut codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    codes.sort_unstable();
    assert_eq!(
        codes,
        [5095, 5102, 5108],
        "expected bundler-conflict + removed-option rejections: {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.message_text
                == "Option 'module=AMD' has been removed. Please remove it from your configuration."),
        "expected the module=AMD removal message: {:?}",
        result.diagnostics
    );
    assert!(
        !base.join("dist").exists() && !base.join("main.js").exists(),
        "removed-option config must not emit, emitted: {:?}",
        result.emitted_files
    );
}

#[test]
fn compile_module_none_outfile_is_rejected_before_emit() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2020",
            "module": "none",
            "outFile": "dist/bundle.js",
            "allowJs": true,
            "ignoreDeprecations": "6.0"
          },
          "files": ["main.ts"]
        }"#,
    );
    write_file(&base.join("main.ts"), "const loaded = import(\"./dep\");\n");
    write_file(&base.join("dep.js"), "export default 1;\n");

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(result.diagnostics.iter().any(|diag| diag.code == 6046));
    assert!(!base.join("dist/bundle.js").exists());
}

#[test]
fn compile_module_none_outfile_with_dependencies_is_rejected_before_emit() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2020",
            "module": "none",
            "outFile": "dist/bundle.js",
            "allowJs": true,
            "ignoreDeprecations": "6.0"
          },
          "files": ["main.ts", "root-script.js"]
        }"#,
    );
    write_file(
        &base.join("main.ts"),
        "/// <reference path=\"./referenced.js\" />\nconst loaded = import(\"./dynamic\");\n",
    );
    write_file(&base.join("root-script.js"), "const rootScriptValue = 1;\n");
    write_file(&base.join("referenced.js"), "const referencedValue = 2;\n");
    write_file(&base.join("dynamic.js"), "export const dynamicValue = 3;\n");

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(result.diagnostics.iter().any(|diag| diag.code == 6046));
    assert!(!base.join("dist/bundle.js").exists());
}

#[test]
fn compile_module_none_outfile_cache_keeps_rejection_stable() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2020",
            "module": "none",
            "outFile": "dist/bundle.js",
            "allowJs": true,
            "ignoreDeprecations": "6.0"
          },
          "files": ["main.ts"]
        }"#,
    );
    write_file(
        &base.join("main.ts"),
        "/// <reference path=\"./referenced.js\" />\nconst loaded = import(\"./dynamic\");\n",
    );
    let referenced_path = base.join("referenced.js");
    write_file(&referenced_path, "const referencedValue = 2;\n");
    write_file(&base.join("dynamic.js"), "export const dynamicValue = 3;\n");

    let args = default_args();
    let mut cache = CompilationCache::default();
    let result = with_types_versions_env(None, || {
        compile_with_cache(&args, base, &mut cache).expect("compile should succeed")
    });
    assert!(result.diagnostics.iter().any(|diag| diag.code == 6046));

    write_file(&referenced_path, "const referencedValue = 4;\n");
    let result = with_types_versions_env(None, || {
        compile_with_cache_and_changes(
            &args,
            base,
            &mut cache,
            std::slice::from_ref(&referenced_path),
        )
        .expect("compile should succeed")
    });
    assert!(result.diagnostics.iter().any(|diag| diag.code == 6046));
    assert!(!base.join("dist/bundle.js").exists());
}

#[test]
fn compile_single_source_amd_declaration_outfile_reports_removed_options() {
    // Declaration-only variant of the AMD outFile pin: TS 7.0.2 rejects the
    // removed `module: amd`/`outFile` pair (plus the TS5095 bundler default
    // conflict) before any declaration bundling happens.
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "declaration": true,
            "emitDeclarationOnly": true,
            "ignoreDeprecations": "6.0",
            "module": "amd",
            "outFile": "dist/bundle.js",
            "strict": true
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(&base.join("index.ts"), "export const value = 1;");

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    let mut codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    codes.sort_unstable();
    assert_eq!(
        codes,
        [5095, 5102, 5108],
        "expected bundler-conflict + removed-option rejections: {:?}",
        result.diagnostics
    );
    assert!(
        !base.join("dist").exists(),
        "removed-option config must not emit a declaration bundle, emitted: {:?}",
        result.emitted_files
    );
}

#[test]
fn compile_amd_declaration_outfile_with_dependency_directive_reports_removed_options() {
    // An `amd-dependency` triple-slash directive in the source does not
    // change the outcome: the removed `module: amd`/`outFile` pair is
    // rejected up front and no declaration bundle is produced.
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "declaration": true,
            "emitDeclarationOnly": true,
            "ignoreDeprecations": "6.0",
            "module": "amd",
            "outFile": "dist/bundle.js",
            "strict": true
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"/// <amd-dependency name="legacyAlias" path="legacy/module" />
export const value = 1;"#,
    );

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    let mut codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    codes.sort_unstable();
    assert_eq!(
        codes,
        [5095, 5102, 5108],
        "expected bundler-conflict + removed-option rejections: {:?}",
        result.diagnostics
    );
    assert!(
        !base.join("dist").exists(),
        "removed-option config must not emit a declaration bundle, emitted: {:?}",
        result.emitted_files
    );
}

#[test]
fn compile_with_source_map_emits_map_outputs() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {"rootDir": ".",
            "outDir": "dist",
            "sourceMap": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 1;");

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(result.diagnostics.is_empty());
    let js_path = base.join("dist/src/index.js");
    let map_path = base.join("dist/src/index.js.map");
    assert!(js_path.is_file());
    assert!(map_path.is_file());
    let js_contents = std::fs::read_to_string(&js_path).expect("read js output");
    assert!(js_contents.contains("sourceMappingURL=index.js.map"));
    let map_contents = std::fs::read_to_string(&map_path).expect("read map output");
    let map_json: Value = serde_json::from_str(&map_contents).expect("parse map json");
    let file_field = map_json
        .get("file")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert_eq!(file_field, "index.js");
    let source_root = map_json
        .get("sourceRoot")
        .and_then(|value| value.as_str())
        .unwrap_or("__missing__");
    assert_eq!(source_root, "");
    let sources_content = map_json
        .get("sourcesContent")
        .and_then(|value| value.as_array())
        .expect("expected sourcesContent");
    assert_eq!(sources_content.len(), 1);
    assert_eq!(
        sources_content[0].as_str().unwrap_or(""),
        "export const value = 1;"
    );
    let mappings = map_json
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert!(
        mappings.contains(',') || mappings.contains(';'),
        "expected non-trivial mappings, got: {mappings}"
    );
}

#[test]
fn compile_with_inline_source_map_embeds_map_data_url() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {"rootDir": ".",
            "outDir": "dist",
            "inlineSourceMap": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 1;");

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(result.diagnostics.is_empty());
    let js_path = base.join("dist/src/index.js");
    let map_path = base.join("dist/src/index.js.map");
    assert!(js_path.is_file());
    assert!(
        !map_path.exists(),
        "inlineSourceMap should not write a sibling .map file"
    );
    let js_contents = std::fs::read_to_string(&js_path).expect("read js output");
    assert!(
        js_contents.contains("//# sourceMappingURL=data:application/json;base64,"),
        "Expected inline source map data URL in JS output: {js_contents}"
    );
    assert!(
        !js_contents.contains("sourceMappingURL=index.js.map"),
        "Expected no external source map comment in JS output: {js_contents}"
    );
}

#[test]
fn compile_resolves_self_name_exports_with_virtual_absolute_output_paths_from_package_root() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    let package_root = base.join("pkg");

    write_file(
        &package_root.join("tsconfig.json"),
        &format!(
            r#"{{
          "compilerOptions": {{
            "module": "nodenext",
            "moduleResolution": "nodenext",
            "rootDir": "{root_dir}",
            "outDir": "{out_dir}",
            "declaration": true,
            "declarationDir": "{declaration_dir}",
            "noEmit": true
          }},
          "include": ["src/**/*.ts"]
        }}"#,
            root_dir = package_root.join("src").display(),
            out_dir = package_root.join("dist").display(),
            declaration_dir = package_root.join("types").display(),
        ),
    );
    write_file(
        &package_root.join("package.json"),
        r#"{
          "name": "@this/package",
          "type": "module",
          "exports": {
            ".": {
              "import": "./dist/index.js"
            }
          }
        }"#,
    );
    write_file(
        &package_root.join("src/index.ts"),
        "import {} from '@this/package';\nexport const value = 1;\n",
    );

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, &package_root).expect("compile should succeed")
    });

    let ts2307: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2307)
        .collect();
    assert!(
        ts2307.is_empty(),
        "expected self-name import to resolve, got diagnostics: {:#?}",
        result.diagnostics
    );
}

#[test]
fn private_static_accessor_on_derived_constructor_reports_ts2339_in_project_mode() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "noEmit": true
          },
          "include": ["test.ts"]
        }"#,
    );
    write_file(
        &base.join("test.ts"),
        r#"
class Base {
    static get #prop(): number { return 123; }
    static method(x: typeof Derived) {
        console.log(x.#prop);
    }
}
class Derived extends Base {
    static method(x: typeof Derived) {
        console.log(x.#prop);
    }
}
"#,
    );

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    let ts2339_count = result
        .diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE)
        .count();
    assert_eq!(
        ts2339_count, 2,
        "expected two TS2339 diagnostics in project mode, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn compile_with_declaration_map_emits_map_outputs() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": ".",
            "declaration": true,
            "declarationMap": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 1;");

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(result.diagnostics.is_empty());
    let dts_path = base.join("dist/src/index.d.ts");
    let map_path = base.join("dist/src/index.d.ts.map");
    assert!(dts_path.is_file());
    assert!(map_path.is_file());
    let dts_contents = std::fs::read_to_string(&dts_path).expect("read d.ts output");
    assert!(dts_contents.contains("sourceMappingURL=index.d.ts.map"));
    let map_contents = std::fs::read_to_string(&map_path).expect("read map output");
    let map_json: Value = serde_json::from_str(&map_contents).expect("parse map json");
    let file_field = map_json
        .get("file")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert_eq!(file_field, "index.d.ts");
    let source_root = map_json
        .get("sourceRoot")
        .and_then(|value| value.as_str())
        .unwrap_or("__missing__");
    assert_eq!(source_root, "");
    assert_eq!(
        map_json
            .get("sources")
            .and_then(|value| value.as_array())
            .and_then(|sources| sources.first())
            .and_then(|source| source.as_str()),
        Some("../../src/index.ts")
    );
    assert!(
        map_json.get("sourcesContent").is_none(),
        "declaration maps should not embed source text: {map_json:?}"
    );
    let mappings = map_json
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert!(
        mappings.contains(',') || mappings.contains(';'),
        "expected non-trivial mappings, got: {mappings}"
    );
}

#[test]
fn compile_with_explicit_files_without_tsconfig() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("main.ts"), "export const value = 1;");

    let mut args = default_args();
    args.files = vec![PathBuf::from("main.ts")];

    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    assert!(base.join("main.js").is_file());
}

#[test]
fn compile_with_explicit_files_no_lib_no_emit_without_tsconfig_returns() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("main.ts"), "const value = 1;\n");

    let args = parse_args(&["tsz", "main.ts", "--noLib", "--noEmit", "--pretty", "false"]);

    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.iter().all(|code| *code == 2318),
        "expected only TS2318 missing global type diagnostics, got: {:?}",
        result.diagnostics
    );
    assert!(!codes.is_empty(), "expected TS2318 diagnostics");
    assert!(result.emitted_files.is_empty());
    assert_eq!(result.files_read.len(), 1);
    assert!(result.files_read[0].ends_with("main.ts"));
}

#[test]
fn compile_explicit_files_no_emit_without_tsconfig_still_checks_semantics() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("main.ts"),
        "const a: string = 1;\nmissingName;\n",
    );

    let args = parse_args(&[
        "tsz",
        "--ignoreConfig",
        "--pretty",
        "false",
        "--strict",
        "--noEmit",
        "main.ts",
    ]);

    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "expected direct --noEmit compile to report TS2322, got: {:?}",
        result.diagnostics
    );
    assert!(
        codes.contains(&diagnostic_codes::CANNOT_FIND_NAME),
        "expected direct --noEmit compile to report TS2304, got: {:?}",
        result.diagnostics
    );
    assert!(result.emitted_files.is_empty());
}

/// Returns args for a `--noCheck --noEmit` run with no config file loaded.
fn no_check_args(files: Vec<PathBuf>) -> CliArgs {
    let mut args = default_args();
    args.ignore_config = true;
    args.no_check = true;
    args.no_emit = true;
    args.files = files;
    args
}

#[test]
fn compile_no_check_expect_error_does_not_suppress_parse_diagnostics() {
    // TS2578 must not be emitted because --noCheck skips type-checking entirely.
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("main.ts"),
        "// @ts-expect-error\nconst broken = ;\n// @ts-expect-error\nconst fine = 1;\n",
    );

    let args = no_check_args(vec![PathBuf::from("main.ts")]);
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&1109),
        "TS1109 must be reported under --noCheck even with @ts-expect-error, got: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&2578),
        "TS2578 must not be emitted under --noCheck, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_no_check_ts_ignore_does_not_suppress_parse_diagnostics() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("main.ts"), "// @ts-ignore\nconst broken = ;\n");

    let args = no_check_args(vec![PathBuf::from("main.ts")]);
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&1109),
        "TS1109 must survive @ts-ignore under --noCheck, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_no_check_expect_error_does_not_suppress_js_grammar_diagnostics() {
    // TS2578 must not be emitted because --noCheck skips type-checking entirely.
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("main.js"),
        "// @ts-expect-error\nlet x: number;\n",
    );

    let args = no_check_args(vec![PathBuf::from("main.js")]);
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&8010),
        "TS8010 must be reported under --noCheck even with @ts-expect-error, got: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&2578),
        "TS2578 must not be emitted under --noCheck, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_expect_error_keeps_parse_diagnostic_without_unused_directive() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("main.ts"),
        "// @ts-expect-error\nconst broken = ;\n",
    );

    let args = parse_args(&[
        "tsz",
        "--ignoreConfig",
        "--noEmit",
        "--pretty",
        "false",
        "main.ts",
    ]);
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&1109),
        "TS1109 must be reported even with @ts-expect-error, got: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&2578),
        "TS2578 must not be emitted when @ts-expect-error targets a parse diagnostic, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_expect_error_keeps_js_syntactic_diagnostic_without_unused_directive() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("main.js"),
        "// @ts-expect-error\nlet x: number;\n",
    );

    let args = parse_args(&[
        "tsz",
        "--ignoreConfig",
        "--checkJs",
        "--noEmit",
        "--pretty",
        "false",
        "main.js",
    ]);
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&8010),
        "TS8010 must be reported even with @ts-expect-error, got: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&2578),
        "TS2578 must not be emitted when @ts-expect-error targets a JS syntactic diagnostic, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_no_check_no_emit_is_parse_only() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("main.ts"),
        "import { value } from './missing';\nconst typed: string = 1;\nconst broken = ;\n",
    );

    let mut args = default_args();
    args.ignore_config = true;
    args.no_check = true;
    args.no_emit = true;
    args.files = vec![PathBuf::from("main.ts")];

    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&1109),
        "expected --noCheck --noEmit to report TS1109 parse error, got: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&2307),
        "expected --noCheck --noEmit to skip module resolution diagnostics, got: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&2322),
        "expected --noCheck --noEmit to skip type diagnostics, got: {:?}",
        result.diagnostics
    );
    assert!(result.emitted_files.is_empty());
    assert_eq!(result.files_read.len(), 1);
}

#[test]
fn compile_no_check_no_emit_suppresses_unused_expect_error() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("main.ts"),
        "// @ts-expect-error\nconst value = 1;\n",
    );

    let mut args = default_args();
    args.ignore_config = true;
    args.no_check = true;
    args.no_emit = true;
    args.files = vec![PathBuf::from("main.ts")];

    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        !codes.contains(&diagnostic_codes::UNUSED_TS_EXPECT_ERROR_DIRECTIVE),
        "expected --noCheck to skip unused @ts-expect-error diagnostics, got: {:?}",
        result.diagnostics
    );
    assert!(result.emitted_files.is_empty());
}

#[test]
fn compile_no_check_no_emit_expect_error_keeps_parse_error() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("main.ts"),
        "// @ts-expect-error\nconst broken = ;\n",
    );

    let mut args = default_args();
    args.ignore_config = true;
    args.no_check = true;
    args.no_emit = true;
    args.files = vec![PathBuf::from("main.ts")];

    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "expected --noCheck to keep TS1109 parse diagnostics despite @ts-expect-error, got: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&diagnostic_codes::UNUSED_TS_EXPECT_ERROR_DIRECTIVE),
        "expected --noCheck to skip unused @ts-expect-error diagnostics, got: {:?}",
        result.diagnostics
    );
    assert!(result.emitted_files.is_empty());
}

#[test]
fn compile_no_check_no_emit_expect_error_keeps_js_grammar_error() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("main.js"),
        "// @ts-expect-error\nlet x: number;\n",
    );

    let mut args = default_args();
    args.allow_js = true;
    args.check_js = true;
    args.ignore_config = true;
    args.no_check = true;
    args.no_emit = true;
    args.files = vec![PathBuf::from("main.js")];

    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::TYPE_ANNOTATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES),
        "expected --noCheck to keep TS8010 JS grammar diagnostics despite @ts-expect-error, got: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&diagnostic_codes::UNUSED_TS_EXPECT_ERROR_DIRECTIVE),
        "expected --noCheck to skip unused @ts-expect-error diagnostics, got: {:?}",
        result.diagnostics
    );
    assert!(result.emitted_files.is_empty());
}

#[test]
fn compile_skip_lib_check_no_emit_declaration_project_is_parse_only() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "noEmit": true,
            "skipLibCheck": true,
            "types": [],
            "ignoreDeprecations": "6.0"
          },
          "files": ["index.d.ts"]
        }"#,
    );
    write_file(
        &base.join("index.d.ts"),
        r#"
import type {MissingImport} from "missing-package";
export type UsesMissing = MissingImport | MissingName;
export interface Broken {
    value: ;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.iter().any(|code| *code < 2000),
        "expected parse diagnostics to survive skipLibCheck, got: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&2307),
        "expected skipLibCheck declaration project to suppress missing imports, got: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&2304),
        "expected skipLibCheck declaration project to suppress semantic missing-name errors, got: {:?}",
        result.diagnostics
    );
    assert_eq!(
        result.files_read.len(),
        1,
        "non-listFiles pure declaration no-emit path should avoid default-lib reads"
    );
}

#[test]
fn compile_higher_order_compose_reports_callback_property_error() {
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
          "files": ["main.ts"]
        }"#,
    );
    write_file(
        &base.join("main.ts"),
        r#"
declare class SetOf<A> {
  transform<B>(transformer: (a: SetOf<A>) => SetOf<B>): SetOf<B>;
}

declare function compose<A, B, C, D, E>(
  fnA: (a: SetOf<A>) => SetOf<B>,
  fnB: (b: SetOf<B>) => SetOf<C>,
  fnC: (c: SetOf<C>) => SetOf<D>,
  fnD: (c: SetOf<D>) => SetOf<E>,
): (x: SetOf<A>) => SetOf<E>;

declare function map<A, B>(fn: (a: A) => B): (s: SetOf<A>) => SetOf<B>;
declare function filter<A>(predicate: (a: A) => boolean): (s: SetOf<A>) => SetOf<A>;

declare const testSet: SetOf<number>;

testSet.transform(
  compose(
    filter(x => x % 1 === 0),
    map(x => x + x),
    map(x => 123),
    map(x => x.toUpperCase())
  )
);
"#,
    );

    let args = default_args();

    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "expected TS2339 for toUpperCase on number from the previous map, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(result.emitted_files.is_empty());
}

#[test]
fn compile_promise_is_assignable_to_promise_like_with_default_libs() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("main.ts"),
        r#"
declare const p: Promise<number>;
const q: PromiseLike<number> = p;
"#,
    );

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.target = Some(crate::args::Target::Es2015);
    args.files = vec![PathBuf::from("main.ts")];

    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected Promise<T> to be assignable to PromiseLike<T>, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}

#[test]
fn compile_recursive_generic_signature_assignment_reports_only_tsc_direction() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("main.ts"),
        r#"
interface I2<T> { p: T }
declare var x: <T extends I2<T>>(z: T) => void;
declare var y: <T extends I2<I2<T>>>(z: T) => void;
x = y;
y = x;
"#,
    );

    let mut args = default_args();
    args.ignore_config = true;
    args.target = Some(crate::args::Target::Es2015);
    args.files = vec![PathBuf::from("main.ts")];

    let result = compile(&args, base).expect("compile should succeed");
    let ts2322_messages: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|diagnostic| diagnostic.message_text.as_str())
        .collect();

    assert_eq!(
        ts2322_messages.len(),
        1,
        "Expected only y = x to report TS2322, got: {:?}",
        result.diagnostics
    );
    assert!(
        ts2322_messages[0].contains(
            "Type '<T extends I2<T>>(z: T) => void' is not assignable to type '<T extends I2<I2<T>>>(z: T) => void'"
        ),
        "Expected the y = x diagnostic to match TypeScript, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_constructor_parameters_rest_contextually_types_object_literal_methods() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("main.ts"),
        r#"
declare function createInstance<Ctor extends new (...args: any[]) => any, R extends InstanceType<Ctor>>(ctor: Ctor, ...args: ConstructorParameters<Ctor>): R;

export interface IMenuWorkbenchToolBarOptions {
    toolbarOptions: {
        foo(bar: string): string
    };
}

class MenuWorkbenchToolBar {
    constructor(
        options: IMenuWorkbenchToolBarOptions | undefined,
    ) { }
}

createInstance(MenuWorkbenchToolBar, {
    toolbarOptions: {
        foo(bar) { return bar; }
    }
});
"#,
    );

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.no_implicit_any = Some(true);
    args.strict_null_checks = Some(true);
    args.target = Some(crate::args::Target::Es2015);
    args.files = vec![PathBuf::from("main.ts")];

    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected ConstructorParameters rest contextual typing to avoid TS2345/TS7006, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}

#[test]
fn compile_contextually_typed_jsx_attribute2_react16_fixture_has_no_ts7006() {
    let mut source = load_typescript_fixture(
        "TypeScript/tests/cases/compiler/contextuallyTypedJsxAttribute2.tsx",
    );
    let react16 = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts");

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    source = source.replace("\"/.lib/react16.d.ts\"", "\"./.lib/react16.d.ts\"");

    write_file(&base.join("test.tsx"), &source);
    write_file(&base.join(".lib/react16.d.ts"), &react16);

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.no_implicit_any = Some(true);
    args.target = Some(crate::args::Target::Es2015);
    args.jsx = Some(crate::args::JsxEmit::React);
    args.es_module_interop = true;
    args.no_emit = true;
    args.files = vec![PathBuf::from("test.tsx")];

    let result = compile(&args, base).expect("compile should succeed");
    let ts7006: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE)
        .collect();

    assert!(
        ts7006.is_empty(),
        "Expected real react16 JSX fixture to avoid TS7006, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}

#[test]
fn compile_contextually_typed_jsx_children2_include_project_has_no_ts2739() {
    let mut source = load_typescript_fixture(
        "TypeScript/tests/cases/compiler/contextuallyTypedJsxChildren2.tsx",
    );
    let react16 = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts");

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    source = source.replace("\"/.lib/react16.d.ts\"", "\"./.lib/react16.d.ts\"");

    write_file(&base.join("test.tsx"), &source);
    write_file(&base.join(".lib/react16.d.ts"), &react16);
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "strict": true,
            "jsx": "react",
            "esModuleInterop": true,
            "noEmit": true,
            "skipLibCheck": true
          },
          "include": ["*.ts", "*.tsx", "*.js", "*.jsx", "**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx"],
          "exclude": ["node_modules"]
        }"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let ts2739: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE)
        .collect();

    assert!(
        ts2739.is_empty(),
        "Expected include-glob react16 JSX children fixture to avoid TS2739, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}

#[test]
fn compile_react16_automatic_jsx_intrinsics_keep_children_and_img_src() {
    let react16 = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts");

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join(".lib/react16.d.ts"), &react16);
    write_file(
        &base.join("one.tsx"),
        r#"/// <reference path="./.lib/react16.d.ts" />
/* @jsxRuntime classic */
import * as React from "react";
export const first = <img src="./image.png" />;
"#,
    );
    write_file(
        &base.join("two.tsx"),
        r#"/// <reference path="./.lib/react16.d.ts" />
/* @jsxRuntime automatic */
const props = { answer: 42 };
const a = <div key="foo" {...props}>text</div>;
const b = <img src="./image.png" />;

export { a, b };
"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"export * as one from "./one.js";
export * as two from "./two.js";
"#,
    );

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.target = Some(crate::args::Target::Es2015);
    args.jsx = Some(crate::args::JsxEmit::ReactJsx);
    args.module = Some(crate::args::Module::CommonJs);
    args.no_emit = true;
    args.files = vec![
        PathBuf::from("one.tsx"),
        PathBuf::from("two.tsx"),
        PathBuf::from("index.ts"),
    ];

    let result = compile(&args, base).expect("compile should succeed");
    let relevant: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                || d.code
                    == diagnostic_codes::COMPONENTS_DONT_ACCEPT_TEXT_AS_CHILD_ELEMENTS_TEXT_IN_JSX_HAS_THE_TYPE_STRING_BU
        })
        .collect();

    assert!(
        relevant.is_empty(),
        "Expected real react16 automatic JSX intrinsics to accept text children and img src, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}

#[test]
fn compile_jsx_call_elaboration_check_no_crash1_react16_fixture_reports_ts2322() {
    let mut source = load_typescript_fixture(
        "TypeScript/tests/cases/compiler/jsxCallElaborationCheckNoCrash1.tsx",
    );
    let react16 = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts");

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    source = source.replace("\"/.lib/react16.d.ts\"", "\"./.lib/react16.d.ts\"");

    write_file(&base.join("test.tsx"), &source);
    write_file(&base.join(".lib/react16.d.ts"), &react16);

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.target = Some(crate::args::Target::Es2015);
    args.jsx = Some(crate::args::JsxEmit::React);
    args.es_module_interop = true;
    args.no_emit = true;
    args.files = vec![PathBuf::from("test.tsx")];

    let result = compile(&args, base).expect("compile should succeed");
    let jsx_ts2322: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && d.message_text
                    .contains("LibraryManagedAttributes<Tag, DetailedHTMLProps")
        })
        .collect();

    assert!(
        !jsx_ts2322.is_empty(),
        "Expected real react16 generic intrinsic JSX fixture to report TS2322, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}

#[test]
fn compile_related_index_react16_fixture_reports_only_distinct_key_ts2322() {
    let mut source = load_typescript_fixture(
        "TypeScript/tests/cases/compiler/errorInfoForRelatedIndexTypesNoConstraintElaboration.ts",
    );
    let react16 = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts");

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    source = source.replace("\"/.lib/react16.d.ts\"", "\"./.lib/react16.d.ts\"");

    write_file(&base.join("test.ts"), &source);
    write_file(&base.join(".lib/react16.d.ts"), &react16);

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.target = Some(crate::args::Target::Es2015);
    args.no_emit = true;
    args.files = vec![PathBuf::from("test.ts")];

    let result = compile(&args, base).expect("compile should succeed");
    let ts2322_messages: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|d| d.message_text.as_str())
        .collect();

    assert!(
        ts2322_messages.iter().any(|message| message.contains(
            "Type 'IntrinsicElements[T1]' is not assignable to type 'IntrinsicElements[T2]'."
        )),
        "expected the distinct-key indexed-access TS2322; got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
    assert!(
        ts2322_messages.iter().all(|message| !message
            .contains("Type '{}' is not assignable to type 'IntrinsicElements[T1]'.")),
        "did not expect the false empty-object TS2322; got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}

#[test]
fn compile_generic_call_at_yield_expression_in_generic_call_fixture_reports_outer_ts2345() {
    let source = load_typescript_fixture(
        "TypeScript/tests/cases/compiler/genericCallAtYieldExpressionInGenericCall1.ts",
    );

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("test.ts"), &source);

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.target = Some(crate::args::Target::EsNext);
    args.no_emit = true;
    args.files = vec![PathBuf::from("test.ts")];

    let result = compile(&args, base).expect("compile should succeed");
    let ts2345: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .collect();
    let ts2488: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR)
        .collect();

    assert_eq!(
        ts2345.len(),
        2,
        "Expected fixture to report the two outer TS2345 callback mismatches, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
    assert_eq!(
        ts2488.len(),
        1,
        "Expected fixture to keep the single inner TS2488, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
    assert!(
        ts2345
            .iter()
            .all(|diag| diag.message_text.contains("Generator<number, void, any>")),
        "Expected outer TS2345 diagnostics to preserve the unannotated generator surface `Generator<number, void, any>`, got diagnostics: {ts2345:?}",
    );
    assert!(
        ts2488[0].message_text.contains("Type '() => T'"),
        "Expected inner TS2488 diagnostic to preserve the non-generic function surface `() => T`, got: {:?}",
        ts2488[0]
    );
}

#[test]
fn compile_generic_call_at_yield_expression_in_generic_call2_fixture_has_no_ts2345() {
    let source = load_typescript_fixture(
        "TypeScript/tests/cases/compiler/genericCallAtYieldExpressionInGenericCall2.ts",
    );

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("test.ts"), &source);

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.target = Some(crate::args::Target::EsNext);
    args.no_emit = true;
    args.files = vec![PathBuf::from("test.ts")];

    let result = compile(&args, base).expect("compile should succeed");
    let ts2345: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .collect();

    assert!(
        ts2345.is_empty(),
        "Expected fixture to avoid stale TS2345 diagnostics, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}

#[test]
fn compile_return_type_inference_contextual_parameter_types_in_generator_fixture_has_no_errors() {
    let source = load_typescript_fixture(
        "TypeScript/tests/cases/compiler/returnTypeInferenceContextualParameterTypesInGenerator1.ts",
    );

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("test.ts"), &source);

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.target = Some(crate::args::Target::EsNext);
    args.no_emit = true;
    args.files = vec![PathBuf::from("test.ts")];

    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected generator contextual return fixture to have no diagnostics, got: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}

#[test]
fn compile_excessive_stack_depth_flat_array_fixture_reports_normalized_jsx_key_target() {
    let source =
        load_typescript_fixture("TypeScript/tests/cases/compiler/excessiveStackDepthFlatArray.ts");

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("test.tsx"), &source);

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.target = Some(crate::args::Target::Es2015);
    args.jsx = Some(crate::args::JsxEmit::React);
    args.no_emit = true;
    args.files = vec![PathBuf::from("test.tsx")];

    let result = compile(&args, base).expect("compile should succeed");
    let jsx_key_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && d.message_text
                    .contains("Type '{ key: string; }' is not assignable to type")
        })
        .collect();

    assert!(
        jsx_key_diags.iter().any(|diag| {
            diag.message_text.contains("HTMLAttributes<HTMLLIElement>")
                && !diag.message_text.contains("DetailedHTMLProps")
        }),
        "Expected JSX key TS2322 to target normalized HTMLAttributes<HTMLLIElement>, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}
