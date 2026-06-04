#[test]
fn ts5107_es5_target_suppresses_accessor_call_follow_on_error() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es5",
            "noEmit": true
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"class Test24554 {
    get property(): number { return 1; }
}
function test24554(x: Test24554) {
    return x.property();
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&5107),
        "Expected TS5107 for deprecated ES5 target, got: {codes:?}"
    );
    assert!(
        !codes.contains(&6234),
        "Did not expect TS6234 alongside deprecated ES5 target, got: {codes:?}"
    );
}

#[test]
fn ts5107_suppresses_arrow_line_terminator_follow_on_errors() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "strict": false,
            "alwaysStrict": false,
            "noEmit": true
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"var f = ()
    => { }
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![5107],
        "Expected only TS5107 for deprecated strict expansion, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn json_default_bindings_with_import_assertions_do_not_emit_ts2305() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "esnext",
            "module": "esnext",
            "ignoreDeprecations": "6.0",
            "noEmit": true
          },
          "files": ["a.ts", "c.ts", "consumer.ts"]
        }"#,
    );
    write_file(
        &base.join("a.ts"),
        r#"import { default as pkg } from "./package.json" assert { type: "json" };
export const pkgValue = pkg;
"#,
    );
    write_file(
        &base.join("c.ts"),
        r#"export { default as config } from "./config.json" assert { type: "json" };
"#,
    );
    write_file(
        &base.join("consumer.ts"),
        r#"import { config } from "./c";

const exact: { answer: number } = config;
void exact;
"#,
    );
    write_file(&base.join("package.json"), r#"{ "name": "tsz" }"#);
    write_file(&base.join("config.json"), r#"{ "answer": 1 }"#);

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().all(|d| d.code != 2305),
        "Did not expect TS2305 for JSON default import/re-export bindings, got diagnostics: {:#?}",
        result.diagnostics
    );
}

#[test]
fn ignore_config_explicit_file_mode_implies_resolve_json_module_for_bundler() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("index.ts"),
        r#"import data from "./data.json";
const answer: number = data.answer;
void answer;
"#,
    );
    write_file(&base.join("data.json"), r#"{ "answer": 42 }"#);

    let args = parse_args(&[
        "tsz",
        "--ignoreConfig",
        "--noEmit",
        "--pretty",
        "false",
        "index.ts",
    ]);
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected no TS2732 for JSON import in no-config explicit-file mode, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn cts_json_namespace_import_default_property_is_json_object() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2022",
            "module": "node16",
            "moduleResolution": "node16",
            "resolveJsonModule": true,
            "noEmit": true
          },
          "files": ["index.cts"]
        }"#,
    );
    write_file(
        &base.join("index.cts"),
        r#"import * as pkg from "./package.json";

export const name = pkg.default.name;
"#,
    );
    write_file(
        &base.join("package.json"),
        r#"{ "name": "pkg", "default": "misedirection" }"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().all(|d| d.code != 2339),
        "Did not expect TS2339 for JSON namespace default property, got diagnostics: {:#?}",
        result.diagnostics
    );
}

#[test]
fn resolve_json_module_does_not_make_included_json_files_roots() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2022",
            "module": "node16",
            "moduleResolution": "node16",
            "resolveJsonModule": true,
            "types": [],
            "noEmit": true
          },
          "include": ["**/*"]
        }"#,
    );
    write_file(&base.join("app.ts"), "export const x = 1;\n");
    write_file(&base.join("data.json"), "{ not valid json }\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "unimported JSON matched by include should not be parsed as a root: {:#?}",
        result.diagnostics
    );
}

#[test]
fn property_diagnostic_does_not_use_conformance_fingerprint_rewrite() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "noEmit": true
          },
          "files": ["repro.ts"]
        }"#,
    );
    write_file(
        &base.join("repro.ts"),
        r#"
type A = { c: number };
type constr<Source, Tgt> = Source & Tgt;
declare const q: { [key: string]: A };
q["asd"].b;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let ts2339: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE)
        .collect();

    assert_eq!(
        ts2339.len(),
        1,
        "expected exactly one TS2339, got diagnostics: {:#?}",
        result.diagnostics
    );
    let message = &ts2339[0].message_text;
    assert!(
        message.contains("type 'A'"),
        "TS2339 should preserve the user alias receiver, got: {message}"
    );
    assert!(
        !message.contains("{ a: string; }"),
        "TS2339 must not use the conformance fingerprint display, got: {message}"
    );
}
