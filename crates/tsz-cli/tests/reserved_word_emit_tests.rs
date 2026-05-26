use super::args::CliArgs;
use super::driver::compile;
use clap::Parser;
use std::path::Path;

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("failed to create parent directory");
    }
    std::fs::write(path, contents).expect("failed to write file");
}

fn write_es2015_tsconfig(base: &Path, files: &[&str]) {
    let files_json = files
        .iter()
        .map(|f| format!("\"{f}\""))
        .collect::<Vec<_>>()
        .join(", ");
    write_file(
        &base.join("tsconfig.json"),
        &format!(
            r#"{{
          "compilerOptions": {{
            "target": "es2015",
            "module": "commonjs",
            "outDir": "dist",
            "noEmitOnError": false
          }},
          "files": [{files_json}]
        }}"#
        ),
    );
}

fn default_args() -> CliArgs {
    CliArgs::try_parse_from(["tsz"]).expect("default args should parse")
}

#[test]
fn reserved_word_parameters_emit_as_empty() {
    // Reserved-word parameter names create a synthesized empty identifier whose
    // source range spans the keyword. JS emit must not reuse that range.
    let temp = tempfile::TempDir::new().expect("temp dir");
    let base = temp.path();

    write_es2015_tsconfig(base, &["input.ts"]);
    write_file(
        &base.join("input.ts"),
        "function f1(enum) {}\nfunction f2(class) {}\nfunction f3(while) {}\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.iter().any(|d| d.code == 1390),
        "Expected TS1390 for reserved-word parameter: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/input.js")).expect("read emitted JS");
    assert!(
        js.contains("function f1()"),
        "reserved word `enum` must not appear as a parameter name; got: {js}"
    );
    assert!(
        js.contains("function f2()"),
        "reserved word `class` must not appear as a parameter name; got: {js}"
    );
    assert!(
        js.contains("function f3()"),
        "reserved word `while` must not appear as a parameter name; got: {js}"
    );
    assert!(
        !js.contains("function f1(enum)"),
        "`enum` must not be emitted as a parameter name; got: {js}"
    );
    assert!(
        !js.contains("function f2(class)"),
        "`class` must not be emitted as a parameter name; got: {js}"
    );
    assert!(
        !js.contains("function f3(while)"),
        "`while` must not be emitted as a parameter name; got: {js}"
    );
}

#[test]
fn void_keyword_type_alias_reports_ts2457_recovery() {
    // `tsc` keeps `void` on the type-alias recovery path and reports TS2457
    // alongside parser/expression diagnostics.
    let temp = tempfile::TempDir::new().expect("temp dir");
    let base = temp.path();

    write_es2015_tsconfig(base, &["input.ts"]);
    write_file(&base.join("input.ts"), "type void = I;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == 1005 || d.code == 1109),
        "Expected parse errors for `type void = I;`, got: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().any(|d| d.code == 2457),
        "TS2457 must appear for `type void = I;`: {:?}",
        result.diagnostics
    );
}
