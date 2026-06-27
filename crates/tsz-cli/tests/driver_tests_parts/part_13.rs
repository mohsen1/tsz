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

/// A `declare global { interface Window { [K]?: T } }` augmentation whose member
/// is keyed by a computed property name `[K]` (K a string `const`, including an
/// `import type`-aliased one) must resolve when the augmented member is accessed
/// (`self.$_TSR`) from a DIFFERENT file than the one declaring the augmentation.
///
/// #14137 fixed the same-file form; this is the cross-file residual. The driver
/// aggregates per-file `global_augmentations`, so this whole-program harness (not
/// the single-binder checker harness) is what exercises the cross-arena
/// computed-key member-name evaluation. Witness: tanstack-router
/// `ssr/tsrScript.ts` accesses `self.$_TSR`/`self.$R` while `ssr/ssr-client.ts`
/// declares the augmentation with keys imported from `ssr/constants.ts`.
#[test]
fn compile_cross_file_declare_global_computed_const_key_resolves() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2017",
            "module": "esnext",
            "moduleResolution": "bundler",
            "lib": ["es2017", "dom"],
            "strict": true,
            "noEmit": true
          },
          "include": ["*.ts"]
        }"#,
    );
    // Both an inferred `const` and a declared-literal-type `declare const`, imported
    // via `import type` into the augmenting file.
    write_file(
        &base.join("constants.ts"),
        r#"export const GLOBAL_TSR = '$_TSR'
export declare const GLOBAL_SEROVAL: '$R'
"#,
    );
    write_file(
        &base.join("aug.ts"),
        r#"import type { GLOBAL_TSR, GLOBAL_SEROVAL } from './constants'
interface TsrSsrGlobal { hydrated: boolean }
declare global {
  interface Window {
    [GLOBAL_TSR]?: TsrSsrGlobal
    [GLOBAL_SEROVAL]?: number
  }
}
export {}
"#,
    );
    // Access the augmented members from a different file. Before the fix tsz
    // dropped the computed-`const`-keyed members here and reported false TS2339.
    write_file(
        &base.join("usage.ts"),
        r#"self.$_TSR = { hydrated: false }
const ok: boolean = self.$_TSR!.hydrated
self.$R = 1
export {}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let no_member_drop = !result
        .diagnostics
        .iter()
        .any(|d| d.code == 2339 && (d.message_text.contains("$_TSR") || d.message_text.contains("$R")));
    assert!(
        no_member_drop,
        "cross-file computed-const Window augmentation members ($_TSR/$R) must resolve \
         (no false TS2339). Diagnostics: {:?}",
        result.diagnostics
    );
}

/// Negative bound for the cross-file computed-`const`-key fix: it must key a
/// SPECIFIC member, not synthesize an index signature. An ABSENT member on
/// `Window` still errors TS2339, matching tsc.
#[test]
fn compile_cross_file_declare_global_computed_const_key_does_not_overbroaden() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2017",
            "module": "esnext",
            "moduleResolution": "bundler",
            "lib": ["es2017", "dom"],
            "strict": true,
            "noEmit": true
          },
          "include": ["*.ts"]
        }"#,
    );
    write_file(
        &base.join("aug.ts"),
        r#"const GLOBAL_TSR = '$_TSR'
interface TsrSsrGlobal { hydrated: boolean }
declare global {
  interface Window {
    [GLOBAL_TSR]?: TsrSsrGlobal
  }
}
export {}
"#,
    );
    write_file(
        &base.join("usage.ts"),
        r#"const bad = self.totallyAbsentMember
export {}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == 2339 && d.message_text.contains("totallyAbsentMember")),
        "an absent Window member must still error TS2339 (the fix keys a specific \
         member, not an index signature). Diagnostics: {:?}",
        result.diagnostics
    );
}

// ---------------------------------------------------------------------------
// End-to-end driver witness for the genuine-`unknown`-bodied generic alias
// reduction landed in #14595 (#13212 slice). A generic type alias whose body is
// exactly `unknown` (`type Foo<T> = unknown`) must reduce its applications to
// canonical `unknown`, matching tsc's eager alias substitution; otherwise a
// later reflexive relation `unknown <: Foo<Args>` on an interface/object member
// typed by exactly this alias produces a false TS2322. #14595 carries the solver
// reduction plus checker/solver unit guards; this pins the same behavior through
// the full project driver (tsconfig + multi-position usage), the path on which
// the regression originally surfaced. Binder names vary across cases
// (alias/interface/param identifiers) so no name string drives the result.
// ---------------------------------------------------------------------------
#[test]
fn compile_generic_alias_with_unknown_body_reduces_in_member_and_return_positions() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "noEmit": true,
    "strict": true,
    "target": "es2020"
  },
  "files": ["main.ts"]
}"#,
    );
    write_file(
        &base.join("main.ts"),
        r#"
declare const top: unknown;

// Generic alias whose body ignores its parameter and is exactly `unknown`.
type Boxed<TElement> = unknown;

// Member position (object literal -> interface).
interface Holder { slot: Boxed<number>; }
const viaLiteral: Holder = { slot: top };

// Member position (non-fresh variable source).
declare const holderSrc: { slot: unknown };
const viaVariable: Holder = holderSrc;

// Function-return position, concrete argument.
const viaReturn: () => Boxed<string> = () => top;

// Function-argument position.
declare function consume(holder: Holder): void;
function callConsume(): void {
    consume({ slot: top });
}

// Standalone generic function-return position (alias arg is a type parameter).
function generic<TParam>(): void {
    const localReturn: () => Boxed<TParam> = () => top;
    void localReturn;
}

// A different alias name, used as a plain (non-function) member.
type AnyShape<TKey> = unknown;
interface Wrapper<TKey> { payload: AnyShape<TKey>; }
function makeWrapper<TKey>(): Wrapper<TKey> {
    return { payload: top };
}

void viaLiteral;
void viaVariable;
void viaReturn;
void callConsume;
void generic;
void makeWrapper;
export {};
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code != 2322 && d.code != 2345),
        "a generic alias with an `unknown` body must reduce to `unknown` in member \
         and return positions (no false TS2322/TS2345). Diagnostics: {:?}",
        result.diagnostics
    );
}

/// A generic type alias whose body is a genuinely-registered `unknown`
/// (`type U<T> = unknown`, or a conditional whose selected branch is `unknown`)
/// reduces its applications to canonical `unknown`. When such an application
/// appears as the RETURN TYPE of a function-typed interface member
/// (`run: () => U<T>`) and the interface is instantiated, the relation layer's
/// return-type comparison previously misclassified the genuine `unknown` body
/// as a missing-body placeholder and fell back to comparing the raw deferred
/// `Application` against the source — a false TS2322
/// `unknown` =< `U<...>`. tsc 6.0.2 is clean. This is the function-return-position
/// slice of the #13212 identity family (the #14595 fix covered property/direct
/// positions via the evaluator; the relation-layer return-type gate needs the
/// same genuine-vs-placeholder distinction).
///
/// Binder names are varied across the cases (anti-hardcoding): the rule keys on
/// the structural `unknown`-body classification, never on identifier text.
#[test]
fn compile_generic_unknown_bodied_alias_in_function_return_position_no_ts2322() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "esnext",
            "strict": true,
            "noEmit": true
          },
          "include": ["*.ts"]
        }"#,
    );
    write_file(
        &base.join("main.ts"),
        r#"
// Genuine `unknown` body, function-return position, generic interface.
type Boxed<T> = unknown;
interface Carrier<T> {
  run: () => Boxed<T>;
}
function makeCarrier<T>(): Carrier<T> {
  return { run: () => 1 as unknown };
}

// Conditional alias whose both branches are `unknown` (selected branch is the
// canonical intrinsic), distinct binder names.
type Reduced<Elem> = Elem extends 1 ? unknown : unknown;
interface Holder<Elem> {
  emit: () => Reduced<Elem>;
}
function makeHolder<Elem>(): Holder<Elem> {
  return { emit: () => "anything" };
}

// Nested function-return position: `() => () => U<T>`.
interface DeepCarrier<Shape> {
  thunk: () => () => Boxed<Shape>;
}
function makeDeep<Shape>(): DeepCarrier<Shape> {
  return { thunk: () => () => true as unknown };
}

export { makeCarrier, makeHolder, makeDeep };
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "a genuine `unknown`-bodied generic alias in function-return position must \
         relate as canonical `unknown` (everything is assignable), matching tsc — \
         no false TS2322. Diagnostics: {:?}",
        result.diagnostics
    );
}

/// Soundness bound for the function-return-position fix: it must NOT make every
/// alias-returning member relate vacuously. A generic alias with a NON-`unknown`
/// body still compares structurally, so a mismatched return value is rejected
/// exactly like tsc.
#[test]
fn compile_generic_alias_function_return_still_rejects_mismatched_non_unknown_body() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "esnext",
            "strict": true,
            "noEmit": true
          },
          "include": ["*.ts"]
        }"#,
    );
    write_file(
        &base.join("main.ts"),
        r#"
// Both branches are `string`, never `unknown`: the member return must still be
// compared structurally, so returning a `number` is a genuine error.
type AlwaysStr<Param> = Param extends 1 ? string : string;
interface Sink<Param> {
  run: () => AlwaysStr<Param>;
}
function makeSink<Param>(): Sink<Param> {
  return { run: () => 123 };
}
export { makeSink };
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().any(|d| d.code == 2322),
        "a non-`unknown` alias body in function-return position must still reject a \
         mismatched return value (TS2322), matching tsc. Diagnostics: {:?}",
        result.diagnostics
    );
}

// Regression for #14852: a triple-slash `/// <reference path="./x" />` whose path
// begins with a `./` or `../` relative prefix must still probe `.ts`/`.tsx`/`.d.ts`
// extensions. The old guard tested the whole path for `.`, so the `.` in the
// relative prefix skipped probing and the referenced file was never pulled,
// producing a downstream TS2304. The referenced declaration is reachable ONLY via
// the reference (it is not listed in tsconfig `files`), so a regression resurfaces
// the TS2304. tsc resolves both relative and bare forms.
#[test]
fn triple_slash_reference_dot_slash_prefix_probes_extensions() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

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
    write_file(&base.join("dep.d.ts"), "declare const DEP_VAL: number;\n");
    write_file(
        &base.join("main.ts"),
        "/// <reference path=\"./dep\" />\nconst y: number = DEP_VAL;\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME),
        "`./dep` reference must pull dep.d.ts (no TS2304). Diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.is_empty(),
        "the program must be clean, matching tsc. Diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn triple_slash_reference_dot_dot_slash_prefix_probes_extensions() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2022",
            "strict": true,
            "noEmit": true
          },
          "files": ["sub/main.ts"]
        }"#,
    );
    write_file(&base.join("dep.d.ts"), "declare const DEP_VAL: number;\n");
    write_file(
        &base.join("sub/main.ts"),
        "/// <reference path=\"../dep\" />\nconst y: number = DEP_VAL;\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "`../dep` reference must pull dep.d.ts from the parent dir (no TS2304). \
         Diagnostics: {:?}",
        result.diagnostics
    );
}
