#[test]
fn checked_js_async_jsdoc_promise_prefixed_alias_reports_ts1064() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "allowJs": true,
    "checkJs": true,
    "strict": true,
    "noEmit": true,
    "module": "commonjs",
    "target": "es2020",
    "types": []
  },
  "files": ["main.js"]
}"#,
    );
    write_file(
        &base.join("main.js"),
        r#"// @ts-check

/**
 * @template T
 * @typedef {{ value: T }} PromiseButNot
 */

/** @type {function(): Promise<string>} */
const ok = async () => "ok";

/** @type {function(): PromiseButNot<string>} */
const f = async () => "ok";

ok;
f;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts1064: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 1064)
        .collect();
    assert_eq!(
        ts1064.len(),
        1,
        "expected TS1064 only for PromiseButNot, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        ts1064[0].message_text.contains("PromiseButNot<string>"),
        "expected TS1064 to suggest wrapping PromiseButNot<string>, got: {:?}",
        ts1064[0]
    );
}

#[test]
fn checked_js_async_jsdoc_shadowed_promise_typedef_reports_ts1064() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "allowJs": true,
    "checkJs": true,
    "strict": true,
    "noEmit": true,
    "module": "commonjs",
    "target": "es2020",
    "types": []
  },
  "files": ["main.js"]
}"#,
    );
    write_file(
        &base.join("main.js"),
        r#"// @ts-check
export {};

/**
 * @template T
 * @typedef {{ value: T }} Promise
 */

/** @type {function(): Promise<string>} */
const f = async () => "ok";

f;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == 1064 && d.message_text.contains("Promise<Promise<string>>")),
        "expected TS1064 for shadowed Promise typedef, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().any(|d| d.code == 2322),
        "expected assignment mismatch alongside TS1064, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn checked_js_external_module_typedef_does_not_suppress_generic_arg_ts2304() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "allowJs": true,
    "checkJs": true,
    "strict": true,
    "noEmit": true,
    "module": "esnext",
    "typeRoots": ["./empty-types"]
  },
  "files": ["a.js", "b.js"]
}"#,
    );
    write_file(&base.join("empty-types/.keep"), "");
    write_file(
        &base.join("a.js"),
        r#"// @ts-check
/** @typedef {Array<Missing>} A */
export {};
"#,
    );
    write_file(
        &base.join("b.js"),
        r#"// @ts-check
/** @typedef {number} Missing */
export {};
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let missing_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code == diagnostic_codes::CANNOT_FIND_NAME
                && diagnostic.message_text.contains("'Missing'")
        })
        .collect();

    assert_eq!(
        missing_diags.len(),
        1,
        "Expected TS2304 for unimported JSDoc typedef in another external module, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_typeof_import_type_query_non_literal_reports_ts1141() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "noEmit": true,
    "types": []
  },
  "files": ["index.ts"]
}"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"
type ImportByKey<K extends string> = typeof import(K);
type MappedImport<T extends string[]> = {
    [K in T[number]]: typeof import(K);
};
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let ts1141_count = result
        .diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::STRING_LITERAL_EXPECTED)
        .count();

    assert_eq!(
        ts1141_count, 2,
        "Expected TS1141 for both typeof import(K) type queries, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn checked_js_jsdoc_import_backtick_reports_ts1141_in_project_mode() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "allowJs": true,
    "checkJs": true,
    "strict": true,
    "noEmit": true
  },
  "files": ["index.js", "dep.d.ts"]
}"#,
    );
    write_file(
        &base.join("dep.d.ts"),
        r#"export interface Foo {
  x: string;
}
"#,
    );
    write_file(
        &base.join("index.js"),
        r#"// @ts-check

/** @type {import(`./dep`).Foo} */
const value = { x: "ok" };

value.x.toUpperCase();
value.y;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|diag| diag.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::STRING_LITERAL_EXPECTED),
        "Expected TS1141 for backtick JSDoc import type, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Invalid JSDoc import syntax should not resolve Foo and report downstream TS2339, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn checked_js_jsdoc_import_string_literal_export_names_resolve() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "allowJs": true,
    "checkJs": true,
    "noEmit": true,
    "types": []
  },
  "files": ["index.js", "dep.d.ts"]
}"#,
    );
    write_file(
        &base.join("dep.d.ts"),
        r#"export declare const value: number;
export { value as "a,b" };
export { value as "as" };
export { value as "from" };
"#,
    );
    write_file(
        &base.join("index.js"),
        r#"// @ts-check
/** @import { "a,b" as CommaName, "as" as AsName, "from" as FromName } from "./dep" */
/** @type {CommaName} */
const a = "x";
/** @type {AsName} */
const b = "x";
/** @type {FromName} */
const c = "x";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|diag| diag.code).collect();

    let assignability_count = codes.iter().filter(|&&code| code == 2322).count();
    assert_eq!(
        assignability_count, 3,
        "Expected three TS2322 diagnostics from resolved JSDoc imports, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_NAME),
        "String-literal JSDoc import aliases should resolve, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN)
            && !codes.contains(&diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER)
            && !codes.contains(&diagnostic_codes::HAS_NO_EXPORTED_MEMBER_NAMED_DID_YOU_MEAN),
        "String-literal export names should not produce unresolved-name or bogus member diagnostics: {:?}",
        result.diagnostics
    );
}
