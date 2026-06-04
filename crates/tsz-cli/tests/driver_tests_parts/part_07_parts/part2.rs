#[test]
fn compile_module_renamed_reexports() {
    // Test renamed re-exports: export { foo as bar } from "./module"
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": ".",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/internal.ts"),
        r#"
export function internalHelper(): string {
    return "helper";
}

export const internalValue = 42;
"#,
    );

    write_file(
        &base.join("src/index.ts"),
        r#"
export { internalHelper as helper, internalValue as value } from "./internal";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(index_dts.contains("helper"), "helper should be re-exported");
    assert!(index_dts.contains("value"), "value should be re-exported");
}

#[test]
fn compile_module_star_reexports() {
    // Test star re-exports: export * from "./module"
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": ".",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/math.ts"),
        r#"
export function sum(arr: number[]): number {
    let total = 0;
    for (const n of arr) {
        total += n;
    }
    return total;
}

export function average(arr: number[]): number {
    return sum(arr) / arr.length;
}
"#,
    );

    write_file(
        &base.join("src/index.ts"),
        r#"
export * from "./math";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(
        index_dts.contains("sum") || index_dts.contains("*"),
        "sum should be re-exported or star export present"
    );
}

#[test]
fn wildcard_reexport_collision_emits_ts2308() {
    // When two modules both export the same name and a third does `export * from` both,
    // TS2308 must be reported. Verify the rule is structural and not name-sensitive
    // by testing with two different exported names.
    for exported_name in ["value", "result"] {
        let temp = TempDir::new().expect("temp dir");
        let base = &temp.path;

        write_file(
            &base.join("tsconfig.json"),
            r#"{"compilerOptions":{"module":"commonjs","noEmit":true},"include":["*.ts"]}"#,
        );
        write_file(
            &base.join("a.ts"),
            &format!("export const {exported_name} = 1;\n"),
        );
        write_file(
            &base.join("b.ts"),
            &format!("export const {exported_name} = 2;\n"),
        );
        write_file(
            &base.join("index.ts"),
            "export * from './a';\nexport * from './b';\n",
        );

        let args = default_args();
        let result = compile(&args, base).expect("compile should succeed");

        assert!(
            result.diagnostics.iter().any(|d| d.code == 2308),
            "Expected TS2308 for collision on '{exported_name}', got: {:?}",
            result.diagnostics
        );
        assert!(
            result
                .diagnostics
                .iter()
                .all(|d| !d.message_text.contains("escape")),
            "Global lib symbol 'escape' must not appear in TS2308 diagnostics"
        );
    }
}

#[test]
fn wildcard_reexport_no_collision_no_ts2308() {
    // Three modules with disjoint exports, all star-re-exported from an index.
    // Global lib symbols like `escape` must not cause spurious TS2308 diagnostics
    // even though they are visible in every file's scope.
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{"compilerOptions":{"module":"commonjs","noEmit":true},"include":["*.ts"]}"#,
    );
    write_file(&base.join("a.ts"), "export const alpha = 1;\n");
    write_file(&base.join("b.ts"), "export const beta = 2;\n");
    write_file(&base.join("c.ts"), "export const gamma = 3;\n");
    write_file(
        &base.join("index.ts"),
        "export * from './a';\nexport * from './b';\nexport * from './c';\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics for non-colliding star re-exports, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_module_chained_reexports() {
    // Test chained re-exports: A re-exports from B which re-exports from C
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": ".",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // Level 3: core module
    write_file(
        &base.join("src/core.ts"),
        r#"
export function coreFunction(): string {
    return "core";
}

export const CORE_VERSION = "1.0.0";
"#,
    );

    // Level 2: intermediate module
    write_file(
        &base.join("src/intermediate.ts"),
        r#"
export { coreFunction, CORE_VERSION } from "./core";

export function intermediateFunction(): string {
    return "intermediate";
}
"#,
    );

    // Level 1: public module
    write_file(
        &base.join("src/index.ts"),
        r#"
export { coreFunction, CORE_VERSION, intermediateFunction } from "./intermediate";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    // All files should be compiled
    assert!(base.join("dist/src/core.js").is_file());
    assert!(base.join("dist/src/intermediate.js").is_file());
    assert!(base.join("dist/src/index.js").is_file());

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(
        index_dts.contains("coreFunction"),
        "coreFunction should be re-exported"
    );
    assert!(
        index_dts.contains("intermediateFunction"),
        "intermediateFunction should be re-exported"
    );
}
