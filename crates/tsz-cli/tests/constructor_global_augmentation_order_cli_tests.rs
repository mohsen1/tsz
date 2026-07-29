//! Cross-file `declare global` constructor groups share one declaration owner.
//!
//! TypeScript scans construct overloads from all merged declarations in program
//! order, keeps the earliest direct-literal candidate globally, and reverses
//! regular declaration groups so the latest group wins. Separate binder/arena
//! symbols for each `declare global` block must therefore be canonicalized to
//! the merged global interface owner without changing inherited owners.

use crate::args::CliArgs;
use clap::Parser;
use tsz_checker::diagnostics::Diagnostic;

fn compile_files(files: &[(&str, &str)], root: &str) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("temp dir");
    for (name, source) in files {
        std::fs::write(dir.path().join(name), source).expect("write repro file");
    }

    let args = CliArgs::try_parse_from([
        "tsz",
        "--ignoreConfig",
        "--noEmit",
        "--strict",
        "--target",
        "es2022",
        "--lib",
        "es2022",
        root,
    ])
    .expect("parse args");
    crate::driver::compile(&args, dir.path())
        .expect("compile should succeed")
        .diagnostics
}

#[test]
fn cross_file_global_construct_groups_share_candidate_order() {
    let diagnostics = compile_files(
        &[
            (
                "first.ts",
                r#"
export {};
declare global {
    interface FirstRegular { owner: "first" }
    interface FirstLiteral { owner: "first-literal" }
    interface SharedGlobalCtor {
        firstProperty: "first";
        new (value: string): FirstRegular;
        new (value: "pick"): FirstLiteral;
    }
}
"#,
            ),
            (
                "second.ts",
                r#"
export {};
declare global {
    interface SecondRegular { owner: "second" }
    interface SecondLiteral { owner: "second-literal" }
    interface SharedGlobalCtor {
        secondProperty: "second";
        new (value: string): SecondRegular;
        new (value: "pick"): SecondLiteral;
    }
}
"#,
            ),
            (
                "consumer.ts",
                r#"
import "./first";
import "./second";
declare const Ctor: SharedGlobalCtor;
const firstProperty: "first" = Ctor.firstProperty;
const secondProperty: "second" = Ctor.secondProperty;
const literal: FirstLiteral = new Ctor("pick");
declare const dynamic: string;
const regular: SecondRegular = new Ctor(dynamic);
new Ctor(true);
"#,
            ),
        ],
        "consumer.ts",
    );
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert_eq!(
        codes,
        vec![2769],
        "cross-file global groups must use one merged owner for literal-first/later-regular ordering while retaining the inapplicable fallback; got: {diagnostics:?}"
    );
}
