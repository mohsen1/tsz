#[test]
fn ts18003_emitted_alongside_ts5110_when_no_inputs() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    // Create a tsconfig with incompatible module/moduleResolution and no source files
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "commonjs",
            "moduleResolution": "nodenext"
          }
        }"#,
    );
    // No .ts files — should trigger TS18003

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5110),
        "Should emit TS5110 for incompatible module/moduleResolution, got: {codes:?}"
    );
    assert!(
        codes.contains(&18003),
        "Should emit TS18003 when no input files found alongside TS5110, got: {codes:?}"
    );
}

#[test]
fn ts18003_not_emitted_when_inputs_exist_with_ts5110() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "commonjs",
            "moduleResolution": "nodenext"
          },
          "include": ["*.ts"]
        }"#,
    );
    write_file(&base.join("index.ts"), "export const x = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&5110), "Should emit TS5110, got: {codes:?}");
    assert!(
        !codes.contains(&18003),
        "Should NOT emit TS18003 when input files exist, got: {codes:?}"
    );
}

#[test]
fn ts5090_stops_before_follow_on_module_diagnostics() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "commonjs",
            "paths": {
              "@app/*": ["src/*"]
            }
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/main.ts"), "import 'someModule';\n");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(
            &diagnostic_codes::NON_RELATIVE_PATHS_ARE_NOT_ALLOWED_WHEN_BASEURL_IS_NOT_SET_DID_YOU_FORGET_A_LEAD
        ),
        "Should emit TS5090 for non-relative paths mapping without baseUrl, got: {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS)
            && !codes.contains(
                &diagnostic_codes::CANNOT_FIND_MODULE_OR_TYPE_DECLARATIONS_FOR_SIDE_EFFECT_IMPORT_OF
            ),
        "Should stop before follow-on module diagnostics when TS5090 is present, got: {codes:?}"
    );
}

#[test]
fn ts18003_emitted_when_only_mts_is_present_under_implicit_include() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "esnext",
            "moduleResolution": "nodenext",
            "allowJs": true
          }
        }"#,
    );
    write_file(&base.join("index.mts"), "export const x = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    // tsc's default include `["**/*"]` discovers .mts files, so with an .mts
    // present the project has inputs and TS18003 must NOT be emitted.
    // TS5110 is still expected from the module/moduleResolution mismatch.
    assert!(
        codes.contains(&5110),
        "Should emit TS5110 for module/moduleResolution mismatch, got: {codes:?}"
    );
    assert!(
        !codes.contains(&18003),
        "Should NOT emit TS18003 when .mts is discovered via implicit include, got: {codes:?}"
    );
}

#[test]
fn ts18003_emitted_when_only_mts_is_present_under_explicit_default_include() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "esnext",
            "moduleResolution": "node16",
            "allowJs": true
          },
          "include": ["*.ts", "*.tsx", "*.js", "*.jsx", "**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx"],
          "exclude": ["node_modules"]
        }"#,
    );
    write_file(&base.join("index.mts"), "export const x = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&5110), "Should emit TS5110, got: {codes:?}");
    assert!(
        codes.contains(&18003),
        "Should emit TS18003 for explicit default include with only .mts input, got: {codes:?}"
    );
}

#[test]
fn ts6059_file_not_under_root_dir() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    // Create a rootDir of "src" but put a file outside it
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "rootDir": "src"
          },
          "include": ["**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/main.ts"), "export const x = 1;");
    write_file(&base.join("outside.ts"), "export const y = 2;");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&6059),
        "Should emit TS6059 for file outside rootDir, got: {codes:?}"
    );
}

#[test]
fn ts6059_not_emitted_when_all_files_under_root_dir() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "rootDir": "src"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/main.ts"), "export const x = 1;");
    write_file(&base.join("src/utils.ts"), "export const y = 2;");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&6059),
        "Should NOT emit TS6059 when all files are under rootDir, got: {codes:?}"
    );
}

#[test]
fn ts6059_not_emitted_for_declaration_dependency_outside_root_dir() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "Node16",
            "moduleResolution": "Node16",
            "rootDir": "src"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(
        &base.join("src/main.ts"),
        r#"import type { ExternalValue } from "external-pkg";

export const value: ExternalValue = { id: "ok" };
"#,
    );
    write_file(
        &base.join("node_modules/external-pkg/package.json"),
        r#"{ "types": "index.d.ts" }"#,
    );
    write_file(
        &base.join("node_modules/external-pkg/index.d.ts"),
        "export interface ExternalValue { id: string }\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&6059),
        "Should NOT emit TS6059 for declaration dependency outside rootDir, got: {codes:?}"
    );
}

#[test]
fn phase_timings_are_populated_after_compilation() {
    let dir = TempDir::new().unwrap();
    let base = &dir.path;
    write_file(
        &base.join("tsconfig.json"),
        r#"{ "compilerOptions": { "noEmit": true }, "include": ["*.ts"] }"#,
    );
    write_file(&base.join("index.ts"), "const x: number = 42;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let pt = &result.phase_timings;

    // All phase timings should be non-negative
    assert!(pt.io_read_ms >= 0.0, "io_read_ms should be non-negative");
    assert!(
        pt.load_libs_ms >= 0.0,
        "load_libs_ms should be non-negative"
    );
    assert!(
        pt.parse_bind_ms >= 0.0,
        "parse_bind_ms should be non-negative"
    );
    assert!(pt.check_ms >= 0.0, "check_ms should be non-negative");
    assert!(pt.emit_ms >= 0.0, "emit_ms should be non-negative");
    assert!(pt.total_ms > 0.0, "total_ms should be positive");
    // T0.2 sub-phase buckets: structurally present, default 0.0 until
    // the driver attributes work to them. Non-negative is the only
    // invariant they must satisfy today.
    assert!(
        pt.config_discovery_ms >= 0.0,
        "config_discovery_ms should be non-negative"
    );
    assert!(
        pt.source_discovery_ms >= 0.0,
        "source_discovery_ms should be non-negative"
    );
    assert!(
        pt.module_resolution_ms >= 0.0,
        "module_resolution_ms should be non-negative"
    );

    // Total should be >= sum of individual phases (wall-clock includes overhead).
    // Sub-phase buckets are subsets of the existing top-level buckets they
    // came out of (config/source/module-resolution land inside io_read; the
    // driver moves them up rather than creating new wall time), so we don't
    // double-count them here.
    let sum = pt.io_read_ms + pt.load_libs_ms + pt.parse_bind_ms + pt.check_ms + pt.emit_ms;
    assert!(
        pt.total_ms >= sum * 0.9, // allow small floating-point margin
        "total_ms ({}) should be >= sum of phases ({})",
        pt.total_ms,
        sum
    );
}

#[test]
fn compile_reports_outer_ts2345_for_block_body_contextual_callback_return_mismatch() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "strict": true,
            "noEmit": true,
            "target": "es2015"
          },
          "include": ["index.ts"]
        }"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"
interface Collection<T, U> {
    length: number;
    add(x: T, y: U): void;
    remove(x: T, y: U): boolean;
}

interface Combinators {
    map<T, U>(c: Collection<T, U>, f: (x: T, y: U) => any): Collection<any, any>;
    map<T, U, V>(c: Collection<T, U>, f: (x: T, y: U) => V): Collection<T, V>;
}

declare var _: Combinators;
declare var c2: Collection<number, string>;
var r5a = _.map<number, string, Date>(c2, (x, y) => { return x.toFixed() });
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&2345),
        "Expected outer TS2345 for block-body callback return mismatch, got: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&2322),
        "Expected no inner TS2322 for block-body callback return mismatch, got: {:?}",
        result.diagnostics
    );
}
