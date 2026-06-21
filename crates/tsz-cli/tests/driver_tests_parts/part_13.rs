#[test]
fn compile_import_alias_indexer_does_not_leak_instance_side_into_namespace_static_side() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "module": "commonjs",
            "strict": true,
            "noEmit": true
          },
          "include": [
            "*.ts", "*.tsx", "*.js", "*.jsx",
            "**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx"
          ],
          "exclude": ["node_modules"]
        }"#,
    );
    write_file(
        &base.join("extendingClassFromAliasAndUsageInIndexer_backbone.ts"),
        r#"export class Model {
    public someData: string;
}
"#,
    );
    write_file(
        &base.join("extendingClassFromAliasAndUsageInIndexer_moduleA.ts"),
        r#"import Backbone = require("./extendingClassFromAliasAndUsageInIndexer_backbone");
export class VisualizationModel extends Backbone.Model {
}
"#,
    );
    write_file(
        &base.join("extendingClassFromAliasAndUsageInIndexer_moduleB.ts"),
        r#"import Backbone = require("./extendingClassFromAliasAndUsageInIndexer_backbone");
export class VisualizationModel extends Backbone.Model {
}
"#,
    );
    write_file(
        &base.join("extendingClassFromAliasAndUsageInIndexer_main.ts"),
        r#"import Backbone = require("./extendingClassFromAliasAndUsageInIndexer_backbone");
import moduleA = require("./extendingClassFromAliasAndUsageInIndexer_moduleA");
import moduleB = require("./extendingClassFromAliasAndUsageInIndexer_moduleB");
interface IHasVisualizationModel {
    VisualizationModel: typeof Backbone.Model;
}
var moduleATyped: IHasVisualizationModel = moduleA;
var moduleMap: { [key: string]: IHasVisualizationModel } = {
    "moduleA": moduleA,
    "moduleB": moduleB
};
var moduleName: string;
var visModel = new moduleMap[moduleName].VisualizationModel();
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let mut codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    codes.sort_unstable();

    assert_eq!(
        codes,
        vec![2454, 2564],
        "Expected only TS2454 and TS2564 for alias indexer usage. Diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .all(|diag| diag.code != diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 from instance-side leakage into module namespace static side. Diagnostics: {:?}",
        result.diagnostics
    );
}

/// `import X = E.Member` whose root `E` is an unresolved namespace import
/// (`import * as E from 'missing'`, TS2307 emitted) must bind `X` to the
/// error/`any` type: tsc types members reached through the `any` namespace as
/// `any`, so `X<args>.member` access does not cascade into TS2339. tsz
/// previously lowered `X<args>` to a real generic application (e.g. `Eq<number>`)
/// with no members, producing a false TS2339.
#[test]
fn compile_import_equals_member_of_unresolved_namespace_suppresses_member_access_cascade() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "module": "commonjs",
            "strict": true,
            "moduleResolution": "node10",
            "ignoreDeprecations": "6.0",
            "noEmit": true
          },
          "files": ["importEqualsUnresolvedRoot.ts"]
        }"#,
    );
    write_file(
        &base.join("importEqualsUnresolvedRoot.ts"),
        r#"import * as E from 'totally-missing-pkg/lib/Eq';
import Eq = E.Eq;
declare const e: Eq<number>;
e.equals(1, 2);
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes.iter().filter(|&&c| c == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS).count(),
        1,
        "Expected exactly one TS2307 for the missing module. Diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Expected no TS2339 cascade on the any-typed alias member. Diagnostics: {:?}",
        result.diagnostics
    );
}

/// Renamed binders plus an assignment-position use of the unresolved-rooted
/// alias: the alias is `any`, so assigning it to a concrete annotation does not
/// cascade into TS2322. Also exercises a deep `Pkg.Inner.Leaf` entity name.
#[test]
fn compile_import_equals_unresolved_namespace_suppresses_assignment_and_deep_cascade() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "module": "commonjs",
            "strict": true,
            "moduleResolution": "node10",
            "ignoreDeprecations": "6.0",
            "noEmit": true
          },
          "files": ["importEqualsRenamedRoot.ts", "importEqualsDeepRoot.ts"]
        }"#,
    );
    write_file(
        &base.join("importEqualsRenamedRoot.ts"),
        r#"import * as NS from 'another-missing/Codec';
import Codec = NS.Type;
declare const c: Codec<string>;
const n: number = c;
"#,
    );
    write_file(
        &base.join("importEqualsDeepRoot.ts"),
        r#"import * as Pkg from 'deep-missing/mod';
import Deep = Pkg.Inner.Leaf;
declare const d: Deep<string>;
d.foo.bar;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes.iter().filter(|&&c| c == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS).count(),
        2,
        "Expected one TS2307 per missing module. Diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 cascade assigning an any-typed alias. Diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Expected no TS2339 cascade on the deep any-typed alias member. Diagnostics: {:?}",
        result.diagnostics
    );
}

// Target-gated diagnostics that depend on the checker seeing the *precise*
// `--target`, not a value collapsed to ESNext. These exercise the
// `using`/`await using` disposable-global checks (TS2318), which `tsc` emits
// based purely on whether the global `Disposable`/`AsyncDisposable` types are
// present in the loaded lib — independent of the target's native support.

#[test]
fn using_declaration_requires_disposable_global_at_es2022() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    // The default lib for `es2022` does not declare `Disposable` (it lives in
    // `esnext.disposable`), so `tsc` reports TS2318 here.
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2022",
            "strict": true,
            "noEmit": true
          },
          "files": ["main.ts"]
        }"#,
    );
    // `null as any` is disposable by convention, so the only target-driven
    // diagnostic is the missing global type — no TS2850.
    write_file(&base.join("main.ts"), "export {};\nusing resource = null as any;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2318),
        "es2022 `using` must report TS2318 (missing global `Disposable`), got: {:?}",
        result.diagnostics
    );
}

#[test]
fn await_using_requires_both_disposable_globals_at_es2022() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2022",
            "module": "esnext",
            "strict": true,
            "noEmit": true
          },
          "files": ["main.ts"]
        }"#,
    );
    // `await using` is also a using declaration: `tsc` resolves both the
    // `AsyncDisposable` and the `Disposable` global, so both must be reported.
    write_file(
        &base.join("main.ts"),
        "export {};\nasync function run() {\n  await using handle = null as any;\n}\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let missing: Vec<&str> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 2318)
        .map(|d| d.message_text.as_str())
        .collect();
    assert!(
        missing.iter().any(|m| m.contains("Disposable"))
            && missing.iter().any(|m| m.contains("AsyncDisposable")),
        "es2022 `await using` must report TS2318 for both `Disposable` and `AsyncDisposable`, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn using_declaration_no_disposable_diagnostic_at_esnext() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    // The default lib for `esnext` declares `Disposable`/`AsyncDisposable`, so
    // no missing-global diagnostic should fire — matching `tsc`.
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "esnext",
            "strict": true,
            "noEmit": true
          },
          "files": ["main.ts"]
        }"#,
    );
    write_file(&base.join("main.ts"), "export {};\nusing resource = null as any;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2318),
        "esnext `using` must not report TS2318 (the disposable globals are in the lib), got: {:?}",
        result.diagnostics
    );
}

// Compile a `moduleSuffixes: [".ios", ""]` project from the given on-disk files
// (the first file written is always the shared tsconfig) and return the emitted
// diagnostic codes. Globs `**/*.ts`, so every written source enters the program.
fn module_suffixes_project_codes(sources: &[(&str, &str)]) -> Vec<u32> {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "module": "esnext",
            "moduleResolution": "bundler",
            "strict": true,
            "noEmit": true,
            "skipLibCheck": true,
            "moduleSuffixes": [".ios", ""]
          },
          "include": ["**/*.ts"],
          "exclude": ["node_modules"]
        }"#,
    );
    for (path, contents) in sources {
        write_file(&base.join(path), contents);
    }
    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));
    let result = compile(&args, base).expect("compile should succeed");
    result.diagnostics.iter().map(|d| d.code).collect()
}

// A relative import in a project that configures `moduleSuffixes` must bind to
// the highest-priority suffix variant the module resolver selected, even when
// the lower-priority base file is *also* in the program (the React Native shape
// where `widget.ios.ts` and `widget.ts` are both globbed and export the same
// API). The checker's `global_file_name_index` fan-out spells `<stem>.<ext>`
// directly and does not apply `moduleSuffixes`, so it used to bind `./widget`
// to the base `widget.ts`; the fix prefers the driver's authoritative
// `resolved_module_paths` (which honored the `.ios` suffix). Binder names
// deliberately differ from the original witness so the behavior follows the
// resolved file, not a spelling.
#[test]
fn compile_project_module_suffixes_relative_import_binds_suffix_variant_over_base() {
    let codes = module_suffixes_project_codes(&[
        // Highest-priority suffix variant: the API is a string-literal type.
        ("src/mobileWidget.ios.ts", "export const palette = \"ios-theme\";\n"),
        // Base file (also globbed into the program): the SAME export name with an
        // incompatible type. If the import mis-binds here it surfaces as TS2322.
        ("src/mobileWidget.ts", "export const palette: number = 42;\n"),
        (
            "src/appEntry.ts",
            "import { palette } from \"./mobileWidget\";\nconst check: \"ios-theme\" = palette;\nexport { check };\n",
        ),
    ]);

    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "`./mobileWidget` must bind to the `.ios` suffix variant (palette: \"ios-theme\"), \
         not the base `mobileWidget.ts` (palette: number). Diagnostic codes: {codes:?}",
    );
}

// Control: when `moduleSuffixes` is configured but no suffix variant exists on
// disk, the relative import still resolves to the base file (the fix falls
// through to the file-index fan-out when the authoritative map has no
// higher-priority entry). Guards against the fix dropping plain relative
// resolution.
#[test]
fn compile_project_module_suffixes_relative_import_falls_back_to_base_when_no_variant() {
    let codes = module_suffixes_project_codes(&[
        ("src/desktopWidget.ts", "export const palette: number = 7;\n"),
        (
            "src/appEntry.ts",
            "import { palette } from \"./desktopWidget\";\nconst check: number = palette;\nexport { check };\n",
        ),
    ]);

    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS)
            && !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "`./desktopWidget` must still resolve to the base file when no suffix \
         variant exists. Diagnostic codes: {codes:?}",
    );
}

// An exact-name `declare module "<spec>"` takes precedence over a catch-all
// `paths` mapping (or on-disk stub) that also matches the bare specifier. tsc's
// `resolveExternalModule` calls `tryFindAmbientModule` before `getResolvedModule`,
// so the ambient module's named exports must be consulted instead of the
// path-mapped file's surface. Witnessed by the type-graphql project, whose
// `"*": ["stub.d.ts"]` mapping shadowed `declare module "graphql-scalars"`
// and produced false TS2614.
#[test]
fn compile_exact_name_ambient_module_wins_over_catch_all_paths_mapping() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2022",
            "module": "esnext",
            "strict": true,
            "types": [],
            "skipLibCheck": true,
            "noEmit": true,
            "moduleResolution": "bundler",
            "baseUrl": ".",
            "ignoreDeprecations": "6.0",
            "paths": { "*": ["stub.d.ts"] }
          },
          "include": ["src/**/*.ts", "ambients.d.ts"]
        }"#,
    );
    // The path-mapped stub exposes no named exports (`export =` of `any`).
    write_file(
        &base.join("stub.d.ts"),
        "declare const tszStub: any;\nexport = tszStub;\n",
    );
    // The exact-name ambient module DOES export the member.
    write_file(
        &base.join("ambients.d.ts"),
        "declare module 'somepkg' {\n  export const NamedFromAmbient: any;\n}\n",
    );
    write_file(
        &base.join("src/index.ts"),
        "export { NamedFromAmbient } from 'somepkg';\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2614: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 2614)
        .collect();
    let ts2305: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER)
        .collect();
    assert!(
        ts2614.is_empty() && ts2305.is_empty(),
        "Exact-name `declare module 'somepkg'` must win over the catch-all \
         `paths` mapping so `NamedFromAmbient` resolves. Got diagnostics: {:#?}",
        result.diagnostics
    );
}

// Negative control: when the exact-name ambient module exists but genuinely
// does NOT export the imported member, the no-exported-member diagnostic must
// still fire. And a bare specifier with NO ambient declaration must continue to
// resolve via `paths` (a present member is fine; an absent one still errors).
#[test]
fn compile_exact_name_ambient_precedence_keeps_missing_member_diagnostics() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2022",
            "module": "esnext",
            "strict": true,
            "types": [],
            "skipLibCheck": true,
            "noEmit": true,
            "moduleResolution": "bundler",
            "baseUrl": ".",
            "ignoreDeprecations": "6.0",
            "paths": { "realpkg": ["realpkg.ts"], "*": ["stub.d.ts"] }
          },
          "include": ["src/**/*.ts", "ambients.d.ts"]
        }"#,
    );
    write_file(
        &base.join("stub.d.ts"),
        "declare const tszStub: any;\nexport = tszStub;\n",
    );
    write_file(&base.join("realpkg.ts"), "export const RealNamed: number = 1;\n");
    write_file(
        &base.join("ambients.d.ts"),
        "declare module 'ambientpkg' {\n  export const Present: any;\n}\n",
    );
    write_file(
        &base.join("src/index.ts"),
        // (1) ambient exists but lacks `Missing` -> error.
        // (2) no ambient for `realpkg`; resolves via paths -> ok.
        // (3) `realpkg` resolved member genuinely absent -> error.
        "export { Missing } from 'ambientpkg';\n\
         export { RealNamed } from 'realpkg';\n\
         export { NotInReal } from 'realpkg';\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let missing_member_lines: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER || d.code == 2614
        })
        .map(|d| d.message_text.clone())
        .collect();
    // Exactly the two genuinely-absent members must error; `RealNamed` resolves.
    assert!(
        missing_member_lines.iter().any(|m| m.contains("Missing")),
        "ambient module without `Missing` must still error. Got: {:#?}",
        result.diagnostics
    );
    assert!(
        missing_member_lines.iter().any(|m| m.contains("NotInReal")),
        "path-resolved `realpkg` without `NotInReal` must still error. Got: {:#?}",
        result.diagnostics
    );
    assert!(
        !missing_member_lines.iter().any(|m| m.contains("RealNamed")),
        "`RealNamed` must resolve via the `paths` mapping (no ambient declared). \
         Got: {:#?}",
        result.diagnostics
    );
}
