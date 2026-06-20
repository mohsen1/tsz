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
