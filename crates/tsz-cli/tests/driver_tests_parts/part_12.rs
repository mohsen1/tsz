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
// Issue #3077: AMD/System/Classic module/resolution modes still emit TS2792
// for unresolved value imports. The deprecation diagnostic (TS5107) those
// modes produce is additive — not a substitute for missing-module reporting.
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
fn ts2792_emitted_for_missing_import_under_module_amd() {
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
        codes.contains(&2792),
        "expected TS2792 under module: amd, got codes: {codes:?}\ndiagnostics: {diagnostics:#?}"
    );
    assert!(
        !codes.contains(&5107),
        "ignoreDeprecations: 6.0 should silence TS5107, got codes: {codes:?}"
    );
}

#[test]
fn ts2792_emitted_for_missing_import_under_module_system() {
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
        codes.contains(&2792),
        "expected TS2792 under module: system, got codes: {codes:?}\ndiagnostics: {diagnostics:#?}"
    );
}

#[test]
fn ts2792_emitted_for_missing_import_under_classic_resolution() {
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
        codes.contains(&2792),
        "expected TS2792 under moduleResolution: classic, got codes: {codes:?}\ndiagnostics: {diagnostics:#?}"
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

#[test]
fn ts5011_not_emitted_for_js_emit_only_out_dir_without_root_dir() {
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
        !codes.contains(&5011),
        "Should NOT emit TS5011 for outDir-only JS emit, got: {codes:?}"
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
