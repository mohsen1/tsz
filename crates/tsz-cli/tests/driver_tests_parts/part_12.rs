#[test]
fn compile_project_nested_thisless_module_state_avoids_ts18046() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "strict": true,
            "strictNullChecks": true,
            "strictFunctionTypes": true,
            "strictBindCallApply": true,
            "strictPropertyInitialization": true,
            "target": "esnext",
            "noEmit": true
          },
          "include": ["*.ts", "*.tsx", "**/*.ts", "**/*.tsx"],
          "exclude": ["node_modules"]
        }"#,
    );
    write_file(
        &base.join("test.ts"),
        r#"
export type StateFunction<State> = (s: State, ...args: any[]) => any;

type Options<State, Modules> = {
  state?: State | (() => State) | { (): State };
  mutations?: Record<string, StateFunction<State>>;
  modules?: {
    [k in keyof Modules]: Options<Modules[k], never>;
  };
};

export function create<
  State extends Record<string, unknown>,
  Modules extends Record<string, Record<string, unknown>>
>(options: Options<State, Modules>) {}

create({
  state() {
    return { bar2: 1 };
  },
  mutations: { inc: (state123) => state123.bar2++ },
  modules: {
    foo: {
      state() {
        return { bar2: 1 };
      },
      mutations: { inc: (state) => state.bar2++ },
    },
  },
});
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::IS_OF_TYPE_UNKNOWN),
        "Nested module state should be inferred from sibling state() before mutation callbacks are checked, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_vue_query_style_promise_chain_and_const_key_has_no_checker_errors() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("node_modules/@tanstack/vue-query/build/modern/useQuery-CPqkvEsh.d.ts"),
        r#"
type QueryKey = ReadonlyArray<unknown>;

interface Register {}

type DefaultError = Register extends {
  defaultError: infer TError;
}
  ? TError
  : Error;

type QueryFunctionContext<
  TQueryKey extends QueryKey = QueryKey,
  TPageParam = never,
> = [TPageParam] extends [never]
  ? {
      queryKey: TQueryKey;
    }
  : {
      queryKey: TQueryKey;
      pageParam: TPageParam;
    };

type QueryFunction<
  T = unknown,
  TQueryKey extends QueryKey = QueryKey,
  TPageParam = never,
> = (context: QueryFunctionContext<TQueryKey, TPageParam>) => T | Promise<T>;

interface QueryOptions<
  TQueryFnData = unknown,
  TError = DefaultError,
  TData = TQueryFnData,
  TQueryKey extends QueryKey = QueryKey,
  TPageParam = never,
> {
  queryKey?: TQueryKey;
  queryFn?: QueryFunction<TQueryFnData, TQueryKey, TPageParam>;
  initialData?: TData;
}

interface QueryObserverOptions<
  TQueryFnData = unknown,
  TError = DefaultError,
  TData = TQueryFnData,
  TQueryData = TQueryFnData,
  TQueryKey extends QueryKey = QueryKey,
  TPageParam = never,
> extends QueryOptions<
    TQueryFnData,
    TError,
    TQueryData,
    TQueryKey,
    TPageParam
  > {
  select?: (data: TQueryData) => TData;
}

type UseQueryOptions<
  TQueryFnData = unknown,
  TError = DefaultError,
  TData = TQueryFnData,
  TQueryData = TQueryFnData,
  TQueryKey extends QueryKey = QueryKey,
> = {
  [Property in keyof QueryObserverOptions<
    TQueryFnData,
    TError,
    TData,
    TQueryData,
    TQueryKey
  >]: QueryObserverOptions<
    TQueryFnData,
    TError,
    TData,
    TQueryData,
    TQueryKey
  >[Property];
};

type UndefinedInitialQueryOptions<
  TQueryFnData = unknown,
  TError = DefaultError,
  TData = TQueryFnData,
  TQueryKey extends QueryKey = QueryKey,
> = UseQueryOptions<TQueryFnData, TError, TData, TQueryFnData, TQueryKey> & {
  initialData?: undefined;
};

interface UseQueryReturnType<TData, TError> {
  data: TData | undefined;
  error: TError | null;
}

declare function useQuery<
  TQueryFnData = unknown,
  TError = DefaultError,
  TData = TQueryFnData,
  TQueryKey extends QueryKey = QueryKey,
>(
  options: UndefinedInitialQueryOptions<TQueryFnData, TError, TData, TQueryKey>,
): UseQueryReturnType<TData, TError>;

export { type UseQueryReturnType, useQuery };
"#,
    );

    write_file(
        &base.join("node_modules/@tanstack/vue-query/build/modern/index.d.ts"),
        r#"export { UseQueryReturnType, useQuery } from './useQuery-CPqkvEsh.js';
"#,
    );

    write_file(
        &base.join("node_modules/@tanstack/vue-query/package.json"),
        r#"{
  "name": "@tanstack/vue-query",
  "type": "module",
  "exports": {
    ".": {
      "import": {
        "types": "./build/modern/index.d.ts",
        "default": "./build/modern/index.js"
      },
      "require": {
        "types": "./build/modern/index.d.cts",
        "default": "./build/modern/index.cjs"
      }
    }
  }
}
"#,
    );

    write_file(
        &base.join("src/index.mts"),
        r#"
import { useQuery } from '@tanstack/vue-query';

const baseUrl = 'https://api.publicapis.org/';

interface IEntry {
    API: string;
    Description: string;
    Auth: string;
    HTTPS: boolean;
    Cors: string;
    Link: string;
    Category: string;
}

const testApi = {
    getEntries: (): Promise<IEntry[]> => {
        return fetch(baseUrl + 'entries')
            .then((res) => res.json())
            .then((data) => data.entries)
            .catch((err) => console.log(err));
    },
};

const entryKeys = {
    all: ['entries'] as const,
    list: () => [...entryKeys.all, 'list'] as const,
};

export const useEntries = () => {
    return useQuery({
        queryKey: entryKeys.list(),
        queryFn: testApi.getEntries,
        select: (data) => data.slice(0, 10),
    });
};
"#,
    );

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "alwaysStrict": true,
    "declaration": true,
    "module": "nodenext",
    "moduleResolution": "nodenext",
    "noEmit": true,
    "noImplicitAny": true,
    "noImplicitThis": true,
    "strict": true,
    "strictBindCallApply": true,
    "strictFunctionTypes": true,
    "strictNullChecks": true,
    "strictPropertyInitialization": true,
    "target": "esnext",
    "useUnknownInCatchVariables": true
  },
  "include": [
    "*.ts",
    "*.tsx",
    "*.js",
    "*.jsx",
    "**/*.ts",
    "**/*.tsx",
    "**/*.js",
    "**/*.jsx"
  ],
  "files": [
    "node_modules/@tanstack/vue-query/build/modern/useQuery-CPqkvEsh.d.ts",
    "node_modules/@tanstack/vue-query/build/modern/index.d.ts",
    "src/index.mts"
  ]
}
"#,
    );

    let args = default_args();

    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "Expected vue-query-style fixture to avoid checker diagnostics, got: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}

// ---------------------------------------------------------------------------
// Issue #3050: imported JS modules report TS7016 (not TS6504) when allowJs is
// disabled. TS6504 is reserved for explicit JS *root* files.
// ---------------------------------------------------------------------------

/// Build the issue-3050 reproducer in `base` and compile it. Returns the
/// resulting diagnostics so each test can assert the specific shape it cares
/// about.
fn run_imported_js_no_allow_js_fixture(base: &Path) -> Vec<tsz_common::diagnostics::Diagnostic> {
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "noEmit": true,
    "strict": true,
    "target": "es2022",
    "module": "esnext",
    "moduleResolution": "bundler"
  },
  "files": [
    "relative-extension.ts",
    "relative-extensionless.ts",
    "dynamic-import.ts",
    "re-export.ts",
    "package-import.ts"
  ]
}"#,
    );
    write_file(&base.join("dep.js"), "export const value = 1;\n");
    write_file(
        &base.join("relative-extension.ts"),
        "import { value } from \"./dep.js\";\nvoid value;\n",
    );
    write_file(
        &base.join("relative-extensionless.ts"),
        "import { value } from \"./dep\";\nvoid value;\n",
    );
    write_file(
        &base.join("dynamic-import.ts"),
        "export async function load() { return import(\"./dep.js\"); }\n",
    );
    write_file(
        &base.join("re-export.ts"),
        "export { value } from \"./dep.js\";\n",
    );
    write_file(
        &base.join("package-import.ts"),
        "import { packageValue } from \"untyped-pkg\";\nvoid packageValue;\n",
    );
    write_file(
        &base.join("node_modules/untyped-pkg/package.json"),
        r#"{"name":"untyped-pkg","main":"index.js"}"#,
    );
    write_file(
        &base.join("node_modules/untyped-pkg/index.js"),
        "module.exports.packageValue = 1;\n",
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));
    let result = compile(&args, base).expect("compile should succeed");
    result.diagnostics
}

#[test]
fn ts7016_emitted_for_imported_js_when_allow_js_disabled_relative_extension() {
    let temp = TempDir::new().expect("temp dir");
    let diagnostics = run_imported_js_no_allow_js_fixture(temp.path.as_path());

    let on_relative: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.file.ends_with("relative-extension.ts"))
        .collect();
    assert!(
        on_relative.iter().any(|d| d.code == 7016),
        "expected TS7016 at the import site for ./dep.js, got: {diagnostics:#?}"
    );
    assert!(
        !on_relative.iter().any(|d| d.code == 6504),
        "TS6504 must not appear for an *imported* JS module, got: {diagnostics:#?}"
    );
}

#[test]
fn ts7016_emitted_for_imported_js_extensionless_relative() {
    let temp = TempDir::new().expect("temp dir");
    let diagnostics = run_imported_js_no_allow_js_fixture(temp.path.as_path());

    assert!(
        diagnostics
            .iter()
            .filter(|d| d.file.ends_with("relative-extensionless.ts"))
            .any(|d| d.code == 7016),
        "expected TS7016 for extensionless relative import ./dep, got: {diagnostics:#?}"
    );
}

#[test]
fn ts7016_emitted_for_imported_js_dynamic_import_and_re_export() {
    let temp = TempDir::new().expect("temp dir");
    let diagnostics = run_imported_js_no_allow_js_fixture(temp.path.as_path());

    assert!(
        diagnostics
            .iter()
            .filter(|d| d.file.ends_with("dynamic-import.ts"))
            .any(|d| d.code == 7016),
        "expected TS7016 for dynamic import(\"./dep.js\"), got: {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .filter(|d| d.file.ends_with("re-export.ts"))
            .any(|d| d.code == 7016),
        "expected TS7016 for re-export of ./dep.js, got: {diagnostics:#?}"
    );
}

#[test]
fn ts7016_emitted_for_imported_js_untyped_package() {
    let temp = TempDir::new().expect("temp dir");
    let diagnostics = run_imported_js_no_allow_js_fixture(temp.path.as_path());

    assert!(
        diagnostics
            .iter()
            .filter(|d| d.file.ends_with("package-import.ts"))
            .any(|d| d.code == 7016),
        "expected TS7016 for untyped node_modules package, got: {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|d| d.code == 6504),
        "TS6504 must not appear anywhere for imported JS, got: {diagnostics:#?}"
    );
}

#[test]
fn ts7016_message_quotes_specifier_and_resolved_path() {
    // The user-facing TS7016 message is structurally derived from the
    // specifier and the resolved path — never from a printer-rendered form.
    // This test pins both placeholders so a future printer change can't
    // silently drop the resolved-path hint.
    let temp = TempDir::new().expect("temp dir");
    let diagnostics = run_imported_js_no_allow_js_fixture(temp.path.as_path());

    let msg = diagnostics
        .iter()
        .find(|d| d.code == 7016 && d.file.ends_with("relative-extension.ts"))
        .map(|d| d.message_text.clone())
        .expect("missing TS7016 for ./dep.js");
    assert!(
        msg.contains("Could not find a declaration file for module './dep.js'."),
        "TS7016 should quote the user's specifier verbatim, got: {msg}"
    );
    assert!(
        msg.contains("dep.js'") && msg.contains("implicitly has an 'any' type."),
        "TS7016 should mention the resolved path and 'any' fallback, got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Issue #3077 pinned TS2792 for unresolved value imports under the
// AMD/System/Classic module/resolution modes. TS7 removed those modes
// entirely: `ignoreDeprecations` no longer applies, tsc 7.0.2 reports the
// TS5108 removed-option error, and the fatal configuration stops source
// checking so no missing-module diagnostics surface.
// ---------------------------------------------------------------------------

/// Compile a one-file program importing a known-missing package under the
/// supplied compiler options. Used by the three AMD/System/Classic
/// regression tests.
fn run_missing_import_under_options(
    options_json: &str,
) -> Vec<tsz_common::diagnostics::Diagnostic> {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    write_file(
        &base.join("tsconfig.json"),
        &format!(
            r#"{{
  "compilerOptions": {options_json},
  "files": ["index.ts"]
}}"#
        ),
    );
    write_file(
        &base.join("index.ts"),
        "import { value } from \"definitely-missing-package\";\nvoid value;\n",
    );
    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));
    let result = compile(&args, base).expect("compile should succeed");
    result.diagnostics
}

#[test]
fn module_amd_reports_ts5108_removed_option_instead_of_ts2792() {
    let diagnostics = run_missing_import_under_options(
        r#"{
            "ignoreDeprecations": "6.0",
            "module": "amd",
            "noEmit": true,
            "strict": true,
            "target": "es2022"
        }"#,
    );
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5108),
        "expected TS5108 for the removed module=AMD option, got codes: {codes:?}\ndiagnostics: {diagnostics:#?}"
    );
    assert!(
        !codes.contains(&2792),
        "removed-option config errors stop source checking, so no TS2792 must surface, got codes: {codes:?}"
    );
}

#[test]
fn module_system_reports_ts5108_removed_option_instead_of_ts2792() {
    let diagnostics = run_missing_import_under_options(
        r#"{
            "ignoreDeprecations": "6.0",
            "module": "system",
            "noEmit": true,
            "strict": true,
            "target": "es2022"
        }"#,
    );
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5108),
        "expected TS5108 for the removed module=System option, got codes: {codes:?}\ndiagnostics: {diagnostics:#?}"
    );
    assert!(
        !codes.contains(&2792),
        "removed-option config errors stop source checking, so no TS2792 must surface, got codes: {codes:?}"
    );
}

#[test]
fn classic_resolution_reports_ts5108_removed_option_instead_of_ts2792() {
    let diagnostics = run_missing_import_under_options(
        r#"{
            "ignoreDeprecations": "6.0",
            "module": "esnext",
            "moduleResolution": "classic",
            "noEmit": true,
            "strict": true,
            "target": "es2022"
        }"#,
    );
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5108),
        "expected TS5108 for the removed moduleResolution=Classic option, got codes: {codes:?}\ndiagnostics: {diagnostics:#?}"
    );
    assert!(
        !codes.contains(&2792),
        "removed-option config errors stop source checking, so no TS2792 must surface, got codes: {codes:?}"
    );
}

#[test]
fn vite_client_reference_suppresses_asset_import_diagnostics() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "ESNext",
            "moduleResolution": "bundler",
            "noEmit": true,
            "noUncheckedSideEffectImports": true,
            "strict": true,
            "target": "ES2020"
          },
          "include": ["src"]
        }"#,
    );
    write_file(
        &base.join("node_modules/vite/package.json"),
        r#"{
          "name": "vite",
          "version": "0.0.0",
          "exports": {
            "./client": {
              "types": "./client.d.ts",
              "default": "./dist/client.js"
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/vite/client.d.ts"),
        r#"declare module "*.css" {}
declare module "*.svg" {
  const src: string;
  export default src;
}
declare module "*.png" {
  const src: string;
  export default src;
}
"#,
    );
    write_file(
        &base.join("src/vite-env.d.ts"),
        r#"/// <reference types="vite/client" />
"#,
    );
    write_file(
        &base.join("src/main.ts"),
        r#"import "./style.css";
import tsLogo from "./assets/typescript.svg";
import hero from "./assets/hero.png";

const assets: string[] = [tsLogo, hero];
console.log(assets.join(","));
"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes
            .contains(&diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS)
            && !codes.contains(&2882),
        "Vite asset ambient modules should suppress missing-module diagnostics, got codes: {codes:?}\ndiagnostics: {:#?}",
        result.diagnostics
    );
}

// TS5011: outDir set, rootDir omitted, inferred common source dir differs
// from tsconfig dir. Mirrors the issue #3822 repro.
#[test]
fn ts5011_emitted_when_out_dir_without_root_dir_and_inferred_subdir() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(
        &base.join("src/collections.ts"),
        "export class Stack<T> { private items: T[] = []; push(i: T): void { this.items.push(i); } }",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5011),
        "Should emit TS5011 when outDir is set without rootDir and the inferred common source dir differs, got: {codes:?}"
    );
    let ts5011 = result
        .diagnostics
        .iter()
        .find(|d| d.code == 5011)
        .expect("TS5011 diagnostic");
    assert!(
        ts5011.message_text.contains("./src"),
        "TS5011 message should reference the inferred common source dir, got: {}",
        ts5011.message_text
    );
}

// tsc 6.0 emits TS5011 for *every* emit, not only declaration emit. A plain
// JavaScript build with `outDir` set, no `rootDir`, and an inferred common
// source subdirectory triggers the same migration warning, because the JS
// output layout changes in TypeScript 7.0 the same way the declaration layout
// does. Verified against the `tsc@6.0.x` oracle.
#[test]
fn ts5011_emitted_for_js_emit_only_out_dir_without_root_dir() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/main.ts"), "export const x = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5011),
        "Should emit TS5011 for outDir JS emit without rootDir, got: {codes:?}"
    );
    let ts5011 = result
        .diagnostics
        .iter()
        .find(|d| d.code == 5011)
        .expect("TS5011 diagnostic");
    assert!(
        ts5011.message_text.contains("./src"),
        "TS5011 message should reference the inferred common source dir, got: {}",
        ts5011.message_text
    );
    assert!(
        ts5011
            .message_text
            .contains("Visit https://aka.ms/ts6 for migration information."),
        "TS5011 message should carry the TS6 migration URL, got: {}",
        ts5011.message_text
    );
}

// TS7 removed `outFile` (TS5102) and `module: amd` (TS5108): the outFile
// bundle-emit TS5011 this test used to pin is unreachable. (tsc 7.0.2 still
// prints TS5011 alongside the removed-option errors for this config; tsz
// stops at the fatal config diagnostics — a documented divergence.)
#[test]
fn out_file_reports_ts5102_removed_option() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outFile": "bundle.js",
            "module": "amd"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/main.ts"), "export const x = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5102),
        "Should emit TS5102 for the removed outFile option, got: {codes:?}"
    );
}

#[test]
fn ts5011_not_emitted_when_root_dir_set() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": "src"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/main.ts"), "export const x = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5011),
        "Should NOT emit TS5011 when rootDir is set explicitly, got: {codes:?}"
    );
}

#[test]
fn ts5011_not_emitted_when_common_source_dir_equals_config_dir() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["*.ts"]
        }"#,
    );
    write_file(&base.join("a.ts"), "export const a = 1;");
    write_file(&base.join("b.ts"), "export const b = 2;");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5011),
        "Should NOT emit TS5011 when common source dir equals config dir, got: {codes:?}"
    );
}

#[test]
fn ts5011_not_emitted_when_no_out_dir() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {},
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/main.ts"), "export const x = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5011),
        "Should NOT emit TS5011 when outDir is not set, got: {codes:?}"
    );
}

#[test]
fn ts5011_not_emitted_with_no_emit() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmit": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/main.ts"), "export const x = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5011),
        "Should NOT emit TS5011 when noEmit is true, got: {codes:?}"
    );
}

// Issue #3693: TS1192 must NOT leak into JS files when checkJs is not
// enabled. tsc routes TS1192 through getSemanticDiagnostics, which is
// suppressed for unchecked JS, so tsz must mirror that.
#[test]
fn ts1192_suppressed_for_js_default_import_without_check_js() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "allowJs": true,
            "noEmit": true,
            "module": "esnext",
            "moduleResolution": "bundler"
          },
          "files": ["a.js", "mod.js"]
        }"#,
    );
    write_file(&base.join("a.js"), "import d from \"./mod\";\nd;\n");
    write_file(&base.join("mod.js"), "export const named = 1;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&1192),
        "TS1192 must not appear for unchecked JS, got: {codes:?}"
    );
}

// tsc 6.0 defaults `esModuleInterop` to `true` when it is not set on the
// command line or in tsconfig. The config path already applies this default in
// `resolve_compiler_options`; the CLI-only path must mirror it so that
// `tsz file.ts` (no tsconfig) emits the same default-import interop helper as
// `tsc file.ts`. Regression for the divergence behind issue #11330.
#[test]
fn cli_only_default_import_uses_es_module_interop_default_true() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("dep.ts"),
        "export default { version: \"1.0.0\" };\n",
    );
    write_file(
        &base.join("a.ts"),
        "import dep from './dep';\nexport const v = dep.version;\n",
    );

    let args = parse_args(&[
        "tsz",
        "--ignoreConfig",
        "--module",
        "commonjs",
        "--target",
        "es2020",
        "--outDir",
        "out",
        "a.ts",
    ]);

    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "expected clean compile, got: {:#?}",
        result.diagnostics
    );

    let a_js = std::fs::read_to_string(base.join("out/a.js")).expect("read out/a.js");
    assert!(
        a_js.contains("__importDefault"),
        "CLI-only commonjs emit must apply the tsc 6.0 esModuleInterop=true \
         default and wrap the default import with __importDefault, got:\n{a_js}"
    );
}

// TS 7.0.2 removed `esModuleInterop=false`: the explicit CLI opt-out is now a
// TS5108 removed-option error instead of switching back to the classic
// (no-helper) default-import lowering.
#[test]
fn cli_only_explicit_es_module_interop_false_reports_ts5108() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("dep.ts"),
        "export default { version: \"1.0.0\" };\n",
    );
    write_file(
        &base.join("a.ts"),
        "import dep from './dep';\nexport const v = dep.version;\n",
    );

    let mut args = parse_args(&[
        "tsz",
        "--ignoreConfig",
        "--module",
        "commonjs",
        "--target",
        "es2020",
        "--outDir",
        "out",
        "a.ts",
    ]);
    args.explicitly_disabled_bool_flags
        .push("esModuleInterop".to_string());

    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.iter().any(|d| d.code == 5108),
        "explicit --esModuleInterop false must report the TS5108 removed-option error, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn invalid_config_es_module_interop_keeps_cli_file_emit_classic_unless_cli_enables() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "esModuleInterop": "invalid",
            "module": "commonjs",
            "target": "es2020"
          }
        }"#,
    );
    write_file(
        &base.join("dep.ts"),
        "export default { version: \"1.0.0\" };\n",
    );
    write_file(
        &base.join("a.ts"),
        "import dep from './dep';\nexport const v = dep.version;\n",
    );

    let args = parse_args(&[
        "tsz",
        "--module",
        "commonjs",
        "--target",
        "es2020",
        "--outDir",
        "out-invalid",
        "a.ts",
    ]);

    let result = compile(&args, base).expect("compile should recover through TS5024");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&5024), "expected TS5024, got: {codes:?}");

    let a_js = std::fs::read_to_string(base.join("out-invalid/a.js"))
        .expect("read out-invalid/a.js");
    assert!(
        !a_js.contains("__importDefault"),
        "invalid tsconfig esModuleInterop must suppress the tsc 6.0 default \
         during CLI file emit recovery, got:\n{a_js}"
    );

    let args = parse_args(&[
        "tsz",
        "--esModuleInterop",
        "--module",
        "commonjs",
        "--target",
        "es2020",
        "--outDir",
        "out-cli",
        "a.ts",
    ]);

    let result = compile(&args, base).expect("compile should recover through TS5024");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&5024), "expected TS5024, got: {codes:?}");

    let a_js = std::fs::read_to_string(base.join("out-cli/a.js")).expect("read out-cli/a.js");
    assert!(
        a_js.contains("__importDefault"),
        "explicit --esModuleInterop must still enable the interop helper, got:\n{a_js}"
    );
}

/// The (code, message) pairs of any "Type 'X' is not assignable to type 'X'"
/// self-assignment diagnostics (TS2322/TS2719) in a result, for the #12464
/// regression guards below.
fn self_assign_diagnostics(diagnostics: &[tsz_common::diagnostics::Diagnostic]) -> Vec<(u32, String)> {
    diagnostics
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2719)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// Issue #12464: a cross-file generic interface used as the call-signature
/// parameter of a hybrid callable, assigned back through a homomorphic mapped
/// type, must not produce a spurious "Type 'X' is not assignable to type 'X'".
///
/// This needs the full driver (real lib load + project-wide symbol resolution):
/// the consuming file routes the imported generic interface through the lib
/// resolution path during generic-reference prewarming, so it lands in
/// `lib_type_resolution_cache`. Before the fix, `lib_heritage_cache_override`
/// substituted that cached body for the *user* interface, yielding a divergent
/// `TypeId` from the same alias resolved normally, and the two forms failed to
/// relate. The check is name-agnostic — `tsz` (and `tsc`) accept this program.
#[test]
fn cross_file_generic_callable_param_through_mapped_type_no_false_self_assign() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("atom.ts"),
        "export interface Atom<Value> {\n  read: (get: <V>(a: Atom<V>) => V) => Value\n}\n",
    );
    write_file(
        &base.join("main.ts"),
        r#"import type { Atom } from "./atom";
type AnyAtom = Atom<unknown>;
type Hook = {
  (atom: AnyAtom): void;
  add(atom: AnyAtom, cb: () => void): () => void;
};
type Hooks = { readonly m?: Hook };
declare const v: Hook;
function init(hooks: Hooks): void {
  type Mut = { -readonly [P in keyof Hooks]: Hooks[P] };
  (hooks as Mut).m = v;
}
"#,
    );

    let args = parse_args(&["tsz", "--noEmit", "--strict", "atom.ts", "main.ts"]);
    let result = compile(&args, base).expect("compile should succeed");
    let self_assign = self_assign_diagnostics(&result.diagnostics);
    assert!(
        self_assign.is_empty(),
        "cross-file generic callable param through mapped type must not report \
         a self-assignment TS2322/TS2719, got: {self_assign:?}"
    );
}

/// Same rule, every binder name changed (interface / alias / property), to keep
/// the guard structural rather than tied to the jotai spellings.
#[test]
fn cross_file_generic_callable_param_through_mapped_type_renamed_no_false_self_assign() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("cell.ts"),
        "export interface Cell<T> {\n  peek: (get: <U>(c: Cell<U>) => U) => T\n}\n",
    );
    write_file(
        &base.join("main.ts"),
        r#"import type { Cell } from "./cell";
type AnyCell = Cell<unknown>;
type Listener = {
  (cell: AnyCell): void;
  attach(cell: AnyCell, cb: () => void): () => void;
};
type Listeners = { readonly slot?: Listener };
declare const listener: Listener;
function setup(ls: Listeners): void {
  type Mutable = { -readonly [P in keyof Listeners]: Listeners[P] };
  (ls as Mutable).slot = listener;
}
"#,
    );

    let args = parse_args(&["tsz", "--noEmit", "--strict", "cell.ts", "main.ts"]);
    let result = compile(&args, base).expect("compile should succeed");
    let self_assign = self_assign_diagnostics(&result.diagnostics);
    assert!(
        self_assign.is_empty(),
        "renamed cross-file generic callable param must not report a \
         self-assignment TS2322/TS2719, got: {self_assign:?}"
    );
}

/// Negative guard: a genuine member mismatch through the same mapped-type path
/// must still be reported as TS2322, so the fix does not silence real errors.
#[test]
fn cross_file_generic_callable_param_genuine_mismatch_still_reports_ts2322() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("atom.ts"),
        "export interface Atom<Value> {\n  read: (get: <V>(a: Atom<V>) => V) => Value\n}\n",
    );
    write_file(
        &base.join("main.ts"),
        r#"import type { Atom } from "./atom";
type AnyAtom = Atom<unknown>;
type Hook = { (atom: AnyAtom): void; a: number };
type Hooks = { readonly m?: Hook };
declare const wrong: { (atom: AnyAtom): void; a: string };
function init(hooks: Hooks): void {
  type Mut = { -readonly [P in keyof Hooks]: Hooks[P] };
  (hooks as Mut).m = wrong;
}
"#,
    );

    let args = parse_args(&["tsz", "--noEmit", "--strict", "atom.ts", "main.ts"]);
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2322),
        "genuine member mismatch (a: string vs a: number) must still report \
         TS2322; got: {codes:?}"
    );
}

/// A class declared in a `node_modules` `.d.ts` module must keep its
/// instance/constructor split when several root files share it: the value
/// side (`new C()`, static access) resolves to the constructor and the type
/// side (annotations) resolves to the instance, regardless of which root is
/// checked first (#13185). Before the fix, the co-included root's value-side
/// resolution populated the shared SYMBOL bucket with the constructor and the
/// second root's field annotation `Relay<M>` flipped to `typeof Relay`
/// (false TS2739 + TS2339 on instance members).
#[test]
fn cross_file_dts_class_keeps_instance_constructor_split_across_roots() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("node_modules/wire-kit/package.json"),
        r#"{ "name": "wire-kit", "main": "index.js", "types": "index.d.ts" }"#,
    );
    write_file(
        &base.join("node_modules/wire-kit/index.d.ts"),
        r#"declare type TopicMap = {
    [topic: string]: Array<unknown>;
};
declare class Relay<Topics extends TopicMap> {
    private slots;
    static highWaterMark: number;
    static slotCount<Topics extends TopicMap>(relay: Relay<TopicMap>, topic: keyof Topics): number;
    constructor();
    attach<Name extends keyof Topics>(topic: Name, sink: (...data: Topics[Name]) => void): this;
    push<Name extends keyof Topics>(topic: Name, ...data: Topics[Name]): boolean;
}
export { Relay, TopicMap };
"#,
    );
    write_file(
        &base.join("first-root.ts"),
        r#"import { Relay } from 'wire-kit'

export const direct = new Relay<{ ping: [number] }>()
export function waterMark(): number {
  return Relay.highWaterMark
}
"#,
    );
    write_file(
        &base.join("second-root.ts"),
        r#"import { Relay } from 'wire-kit'

type FeedMap = {
  update: [payload: string]
}

class Feed {
  #relay: Relay<FeedMap>

  constructor() {
    this.#relay = new Relay<FeedMap>()
  }

  listen(): void {
    this.#relay.attach('update', (payload) => {
      payload.slice(0)
    })
  }
}

export { Feed }
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "strict": true,
    "target": "esnext",
    "module": "esnext",
    "moduleResolution": "bundler",
    "skipLibCheck": true,
    "noEmit": true,
    "types": []
  },
  "files": ["first-root.ts", "second-root.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));
    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "tsc reports zero diagnostics for this project; instance/constructor \
         identity must not flip with co-included roots, got: {:#?}",
        result.diagnostics
    );
}

/// Same project with the root order reversed: the type-position root checked
/// first must not poison the value-position root's static access with the
/// instance type (the reverse direction of the #13185 flip).
#[test]
fn cross_file_dts_class_keeps_split_with_reversed_root_order() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("node_modules/wire-kit/package.json"),
        r#"{ "name": "wire-kit", "main": "index.js", "types": "index.d.ts" }"#,
    );
    write_file(
        &base.join("node_modules/wire-kit/index.d.ts"),
        r#"declare type TopicMap = {
    [topic: string]: Array<unknown>;
};
declare class Relay<Topics extends TopicMap> {
    private slots;
    static highWaterMark: number;
    constructor();
    attach<Name extends keyof Topics>(topic: Name, sink: (...data: Topics[Name]) => void): this;
}
export { Relay, TopicMap };
"#,
    );
    write_file(
        &base.join("typed-root.ts"),
        r#"import { Relay } from 'wire-kit'

export declare const feed: Relay<{ update: [string] }>
export function poke(r: Relay<{ update: [string] }>): void {
  r.attach('update', (payload) => payload.slice(0))
}
"#,
    );
    write_file(
        &base.join("value-root.ts"),
        r#"import { Relay } from 'wire-kit'

export const direct = new Relay<{ ping: [number] }>()
export function waterMark(): number {
  return Relay.highWaterMark
}
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "strict": true,
    "target": "esnext",
    "module": "esnext",
    "moduleResolution": "bundler",
    "skipLibCheck": true,
    "noEmit": true,
    "types": []
  },
  "files": ["typed-root.ts", "value-root.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));
    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "static access on the class value must keep resolving to the \
         constructor when another root used the class in type position, \
         got: {:#?}",
        result.diagnostics
    );
}

/// A second package exporting a SAME-NAMED class, imported type-only by a
/// co-included root, must not overwrite the first import's value-side type
/// through name-keyed lib resolution (the `file_locals[name]` overwrite in
/// `resolve_lib_type_by_name`, #13185).
#[test]
fn same_named_dts_classes_in_sibling_packages_do_not_cross_poison() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("node_modules/pkg-one/package.json"),
        r#"{ "name": "pkg-one", "main": "index.js", "types": "index.d.ts" }"#,
    );
    write_file(
        &base.join("node_modules/pkg-one/index.d.ts"),
        r#"declare class Mailbox<Letters> {
    static capacity: number;
    deliver(letter: Letters): void;
}
export { Mailbox };
"#,
    );
    write_file(
        &base.join("node_modules/pkg-two/package.json"),
        r#"{ "name": "pkg-two", "main": "index.js", "types": "index.d.ts" }"#,
    );
    write_file(
        &base.join("node_modules/pkg-two/index.d.ts"),
        r#"declare class Mailbox<Payload> {
    static brand: string;
    push(value: Payload): void;
}
declare namespace Mailbox {
    type Of<T extends Mailbox<unknown>> = T extends Mailbox<infer P> ? P : never;
}
export { Mailbox };
"#,
    );
    write_file(
        &base.join("uses-one.ts"),
        r#"import { Mailbox } from 'pkg-one'
import type { Mailbox as TwoBox } from 'pkg-two'

export declare const other: TwoBox<number>
export const box = new Mailbox<string>()
export function capacity(): number {
  return Mailbox.capacity
}
"#,
    );
    write_file(
        &base.join("uses-one-typed.ts"),
        r#"import { Mailbox } from 'pkg-one'

class Office {
  #box: Mailbox<string>

  constructor() {
    this.#box = new Mailbox<string>()
  }

  send(): void {
    this.#box.deliver('hi')
  }
}

export { Office }
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "strict": true,
    "target": "esnext",
    "module": "esnext",
    "moduleResolution": "bundler",
    "skipLibCheck": true,
    "noEmit": true,
    "types": []
  },
  "files": ["uses-one.ts", "uses-one-typed.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));
    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "same-named classes from sibling packages must keep independent \
         value/type identities, got: {:#?}",
        result.diagnostics
    );
}

/// Negative control: genuine misuse of the instance/static split must still
/// be reported when roots are co-included — the fix must not blanket-silence
/// class member errors.
#[test]
fn cross_file_dts_class_genuine_static_instance_misuse_still_reported() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("node_modules/wire-kit/package.json"),
        r#"{ "name": "wire-kit", "main": "index.js", "types": "index.d.ts" }"#,
    );
    write_file(
        &base.join("node_modules/wire-kit/index.d.ts"),
        r#"declare class Relay<Topics> {
    static highWaterMark: number;
    attach(topic: keyof Topics): void;
}
export { Relay };
"#,
    );
    write_file(
        &base.join("value-root.ts"),
        r#"import { Relay } from 'wire-kit'
export const direct = new Relay<{ ping: [number] }>()
"#,
    );
    write_file(
        &base.join("misuse-root.ts"),
        r#"import { Relay } from 'wire-kit'

declare const instance: Relay<{ update: [string] }>
// Static member accessed through the instance: must stay an error.
export const wrong = instance.highWaterMark
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "strict": true,
    "target": "esnext",
    "module": "esnext",
    "moduleResolution": "bundler",
    "skipLibCheck": true,
    "noEmit": true,
    "types": []
  },
  "files": ["value-root.ts", "misuse-root.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2339),
        "static member accessed through an instance must still report \
         TS2339; got: {codes:?}"
    );
}

/// A package declaration file that is also listed as a root must not poison
/// later app roots that import the package. This mirrors conformance
/// `propTypeValidatorInference.ts`: tsc accepts the validator map even when the
/// harness lists `node_modules/prop-types/index.d.ts` before the importing file.
#[test]
fn explicit_node_modules_dts_root_does_not_poison_prop_types_importer() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("node_modules/prop-types/package.json"),
        r#"{ "name": "prop-types", "main": "index.js", "types": "index.d.ts" }"#,
    );
    write_file(
        &base.join("node_modules/prop-types/index.d.ts"),
        r#"export const nominalTypeHack: unique symbol;

export type IsOptional<T> = undefined | null extends T ? true : undefined extends T ? true : null extends T ? true : false;
export type RequiredKeys<V> = { [K in keyof V]-?: Exclude<V[K], undefined> extends Validator<infer T> ? IsOptional<T> extends true ? never : K : never }[keyof V];
export type OptionalKeys<V> = Exclude<keyof V, RequiredKeys<V>>;
export type InferPropsInner<V> = { [K in keyof V]-?: InferType<V[K]>; };

export interface Validator<T> {
    (props: object, propName: string): Error | null;
    [nominalTypeHack]?: T;
}

export interface Requireable<T> extends Validator<T> {
    isRequired: Validator<NonNullable<T>>;
}

export type ValidationMap<T> = { [K in keyof T]?: Validator<T[K]> };
export type InferType<V> = V extends Validator<infer T> ? T : any;
export type InferProps<V> =
    & InferPropsInner<Pick<V, RequiredKeys<V>>>
    & Partial<InferPropsInner<Pick<V, OptionalKeys<V>>>>;

export const any: Requireable<any>;
export const array: Requireable<any[]>;
export const bool: Requireable<boolean>;
export const string: Requireable<string>;
export const number: Requireable<number>;
export function shape<P extends ValidationMap<any>>(type: P): Requireable<InferProps<P>>;
export function oneOfType<T extends Validator<any>>(types: T[]): Requireable<NonNullable<InferType<T>>>;
"#,
    );
    write_file(
        &base.join("file.ts"),
        r#"import * as PropTypes from "prop-types";

interface Props {
    any?: any;
    array: string[];
    bool: boolean;
    shape: { foo: string; bar?: boolean; baz?: any };
    oneOfType: string | boolean | { foo?: string; bar: number };
}

type PropTypesMap = PropTypes.ValidationMap<Props>;

const innerProps = {
    foo: PropTypes.string.isRequired,
    bar: PropTypes.bool,
    baz: PropTypes.any
};

const arrayOfTypes = [PropTypes.string, PropTypes.bool, PropTypes.shape({
    foo: PropTypes.string,
    bar: PropTypes.number.isRequired
})];

const propTypes: PropTypesMap = {
    any: PropTypes.any,
    array: PropTypes.array.isRequired,
    bool: PropTypes.bool.isRequired,
    shape: PropTypes.shape(innerProps).isRequired,
    oneOfType: PropTypes.oneOfType(arrayOfTypes).isRequired,
};

const propTypesWithoutAnnotation = {
    any: PropTypes.any,
    array: PropTypes.array.isRequired,
    bool: PropTypes.bool.isRequired,
    shape: PropTypes.shape(innerProps).isRequired,
    oneOfType: PropTypes.oneOfType(arrayOfTypes).isRequired,
};

type ExtractedProps = PropTypes.InferProps<typeof propTypes>;
type ExtractedPropsWithoutAnnotation = PropTypes.InferProps<typeof propTypesWithoutAnnotation>;
type ExtractPropsMatch = ExtractedProps extends ExtractedPropsWithoutAnnotation ? true : false;
const x: true = (null as any as ExtractPropsMatch);
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "strict": true,
    "target": "es2015",
    "module": "commonjs",
    "noEmit": true
  },
  "files": ["node_modules/prop-types/index.d.ts", "file.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));
    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "explicit package declaration roots must not change imported validator \
         inference, got: {:#?}",
        result.diagnostics
    );
}

// =========================================================================
// TS5097/TS2846 for re-export and import-equals module specifiers
//
// tsc's `resolveExternalModule` anchors its TypeScript-extension specifier
// diagnostics on `findAncestor(location, isImportDeclaration)?.importClause
// || findAncestor(location, or(isImportEqualsDeclaration,
// isExportDeclaration))`, so `export ... from "./x.ts"` and
// `import x = require("./x.ts")` report TS5097 exactly like
// `import ... from "./x.ts"` (and `.d.ts` specifiers report TS2846).
// Statement-level type-only forms are exempt; specifier-level `{ type x }`
// modifiers are not. Verified against tsc 5.5.4.
// =========================================================================

fn ts5097_matrix_compile(base: &Path, entry_source: &str, extra_options: &str) -> Vec<u32> {
    write_file(
        &base.join("tsconfig.json"),
        &format!(
            r#"{{
              "compilerOptions": {{
                "module": "esnext",
                "moduleResolution": "bundler",
                "noEmit": true{extra_options}
              }},
              "include": ["**/*"]
            }}"#
        ),
    );
    write_file(&base.join("pieces.ts"), "export const rook = 1;\n");
    write_file(&base.join("board.ts"), entry_source);

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    result.diagnostics.iter().map(|d| d.code).collect()
}

#[test]
fn export_from_ts_extension_reports_ts5097_per_reexport_form() {
    let temp = TempDir::new().expect("temp dir");
    let codes = ts5097_matrix_compile(
        temp.path.as_path(),
        "export { rook } from './pieces.ts';\n\
         export * from './pieces.ts';\n\
         export * as squares from './pieces.ts';\n",
        "",
    );
    assert_eq!(
        codes,
        vec![5097, 5097, 5097],
        "named, star, and namespace re-exports of a .ts specifier must each \
         report TS5097, got: {codes:?}"
    );
}

#[test]
fn export_type_from_ts_extension_is_exempt_but_specifier_type_modifier_is_not() {
    let temp = TempDir::new().expect("temp dir");
    let codes = ts5097_matrix_compile(
        temp.path.as_path(),
        "export type { rook } from './pieces.ts';\n\
         export { type rook as rookT } from './pieces.ts';\n",
        "",
    );
    // tsc exempts statement-level `export type ... from`, but the
    // specifier-level `{ type x }` modifier does NOT suppress TS5097.
    assert_eq!(
        codes,
        vec![5097],
        "`export type {{ }} from` must be exempt while `export {{ type x }} from` \
         still reports TS5097, got: {codes:?}"
    );
}

#[test]
fn export_from_ts_extension_allowed_with_allow_importing_ts_extensions() {
    let temp = TempDir::new().expect("temp dir");
    let codes = ts5097_matrix_compile(
        temp.path.as_path(),
        "export { rook } from './pieces.ts';\nexport * from './pieces.ts';\n",
        ",\n\"allowImportingTsExtensions\": true",
    );
    assert!(
        codes.is_empty(),
        "allowImportingTsExtensions must silence TS5097 for re-exports, got: {codes:?}"
    );
}

#[test]
fn export_from_extensionless_specifier_reports_nothing() {
    let temp = TempDir::new().expect("temp dir");
    let codes = ts5097_matrix_compile(
        temp.path.as_path(),
        "export { rook } from './pieces';\nexport * from './pieces';\n",
        "",
    );
    assert!(
        codes.is_empty(),
        "extensionless re-export specifiers are the no-diagnostic control, got: {codes:?}"
    );
}

#[test]
fn export_from_tsx_extension_reports_ts5097_with_tsx_text() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "esnext",
            "moduleResolution": "bundler",
            "jsx": "preserve",
            "noEmit": true
          },
          "include": ["**/*"]
        }"#,
    );
    write_file(&base.join("widget.tsx"), "export const gizmo = 2;\n");
    write_file(
        &base.join("panel.ts"),
        "export { gizmo } from './widget.tsx';\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec![5097], "got: {codes:?}");
    assert!(
        result.diagnostics[0].message_text.contains("'.tsx'"),
        "TS5097 must name the .tsx extension, got: {}",
        result.diagnostics[0].message_text
    );
}

#[test]
fn export_from_dts_extension_reports_ts2846_not_ts5097() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "esnext",
            "moduleResolution": "bundler",
            "noEmit": true
          },
          "include": ["**/*"]
        }"#,
    );
    write_file(
        &base.join("shapes.d.ts"),
        "export declare const circle: number;\n",
    );
    write_file(
        &base.join("canvas.ts"),
        "export { circle } from './shapes.d.ts';\nexport type { circle as circleT } from './shapes.d.ts';\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    // The non-type-only re-export reports TS2846; `export type ... from` is
    // exempt; TS5097 never applies to declaration-file specifiers.
    assert_eq!(
        codes,
        vec![2846],
        ".d.ts re-export must report TS2846 (and only for the non-type-only \
         form), got: {codes:?}"
    );
}

#[test]
fn import_equals_require_ts_extension_reports_ts5097() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "commonjs",
            "noEmit": true
          },
          "include": ["**/*"]
        }"#,
    );
    write_file(&base.join("gear.ts"), "export const cog = 3;\n");
    write_file(
        &base.join("machine.ts"),
        "import gears = require('./gear.ts');\nimport type gearsT = require('./gear.ts');\nexport const spin = gears.cog;\nexport type SpinT = typeof gearsT.cog;\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        vec![5097],
        "`import x = require('./y.ts')` must report TS5097 once (type-only \
         form exempt), got: {codes:?}"
    );
}

#[test]
fn import_control_ts_extension_still_reports_ts5097() {
    let temp = TempDir::new().expect("temp dir");
    let codes = ts5097_matrix_compile(
        temp.path.as_path(),
        "import { rook } from './pieces.ts';\nexport const r = rook;\n",
        "",
    );
    assert_eq!(
        codes,
        vec![5097],
        "plain import of a .ts specifier remains the working control, got: {codes:?}"
    );
}

#[test]
fn compile_export_from_ts_extension_reports_ts5097() {
    // tsc emits TS5097 for re-export module specifiers ending in `.ts` when
    // `allowImportingTsExtensions` is off, exactly like the import forms
    // (#13212 F2: tsz previously checked ImportDeclaration only).
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "esnext",
            "moduleResolution": "bundler",
            "noEmit": true
          },
          "files": ["star.ts", "named.ts", "ns.ts"]
        }"#,
    );
    write_file(&base.join("a.ts"), "export const x = 1;\nexport type T = number;");
    write_file(&base.join("star.ts"), "export * from './a.ts';\n");
    write_file(&base.join("named.ts"), "export { x } from './a.ts';\n");
    write_file(&base.join("ns.ts"), "export * as ns from './a.ts';\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should complete");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![
            diagnostic_codes::AN_IMPORT_PATH_CAN_ONLY_END_WITH_A_EXTENSION_WHEN_ALLOWIMPORTINGTSEXTENSIONS_IS;
            3
        ],
        "expected TS5097 for each export-from form (star, named, namespace), got: {codes:?}"
    );
}

#[test]
fn compile_type_only_export_from_ts_extension_matrix() {
    // Statement-level type-only re-exports suppress TS5097; a specifier-level
    // `type` does NOT (matches tsc).
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "esnext",
            "moduleResolution": "bundler",
            "noEmit": true
          },
          "files": ["stmt.ts", "spec.ts"]
        }"#,
    );
    write_file(&base.join("a.ts"), "export const x = 1;\nexport type T = number;");
    write_file(&base.join("stmt.ts"), "export type { T } from './a.ts';\n");
    write_file(&base.join("spec.ts"), "export { type T } from './a.ts';\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should complete");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![
            diagnostic_codes::AN_IMPORT_PATH_CAN_ONLY_END_WITH_A_EXTENSION_WHEN_ALLOWIMPORTINGTSEXTENSIONS_IS
        ],
        "statement-level type-only export must suppress TS5097 while \
         specifier-level `type` must not, got: {codes:?}"
    );
}

#[test]
fn compile_export_from_ts_extension_allowed_when_option_enabled() {
    // Control: allowImportingTsExtensions=true silences the diagnostic for
    // export-from exactly as for imports.
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "esnext",
            "moduleResolution": "bundler",
            "allowImportingTsExtensions": true,
            "noEmit": true
          },
          "files": ["star.ts"]
        }"#,
    );
    write_file(&base.join("a.ts"), "export const x = 1;");
    write_file(&base.join("star.ts"), "export * from './a.ts';\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should complete");

    assert!(
        result.diagnostics.is_empty(),
        "allowImportingTsExtensions must silence export-from TS5097, got: {:?}",
        result.diagnostics
    );
}

include!("part_12_tail.rs");
