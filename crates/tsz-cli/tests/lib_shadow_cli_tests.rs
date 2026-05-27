use crate::args::CliArgs;
use clap::Parser;

#[test]
fn cli_module_local_symbol_const_shadows_lib_global_value() {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(
        dir.path().join("repro.ts"),
        r#"
export {};
const Symbol = () => 1;
const key = Symbol();
type T = { [key]: string };
declare const t: T;
t[key].toUpperCase();
"#,
    )
    .expect("write repro");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--ignoreConfig",
        "--noEmit",
        "--strict",
        "--target",
        "es2020",
        "repro.ts",
    ])
    .expect("parse args");

    let result = crate::driver::compile(&args, dir.path()).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|diag| diag.code).collect();

    assert!(
        !codes.contains(&2451),
        "module-local `const Symbol` must shadow the lib global value, got diagnostics: {:#?}",
        result.diagnostics
    );
}
