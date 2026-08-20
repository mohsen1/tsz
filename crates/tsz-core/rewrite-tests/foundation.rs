use std::sync::Arc;

use tsz::service::LanguageService;
use tsz::{Compiler, CompilerOptions, SourceInput};

fn compile(source: &str) -> tsz::CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            no_emit: true,
            strict: true,
            ..CompilerOptions::default()
        },
    )
}

fn codes(output: &tsz::CompileOutput) -> Vec<u32> {
    output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

#[test]
fn assignment_uses_structured_relation_failure() {
    let output = compile(r#"const count: number = "wrong";"#);
    assert_eq!(codes(&output), vec![2322]);
    assert_eq!(
        output.diagnostics[0].message_text,
        "Type 'string' is not assignable to type 'number'."
    );
    assert_eq!(
        (output.diagnostics[0].start, output.diagnostics[0].length),
        (6, 5)
    );
}

#[test]
fn call_arguments_share_the_relation_engine() {
    let output = compile(
        r#"
        function take(value: number): void {}
        take("wrong");
        "#,
    );
    assert_eq!(codes(&output), vec![2345]);
}

#[test]
fn seed_diagnostics_match_the_pinned_ts7_oracle() {
    let cases = [
        (
            r#"const count: number = "wrong";"#,
            Some((
                2322,
                6,
                5,
                "Type 'string' is not assignable to type 'number'.",
            )),
        ),
        (
            "function take(value: number): void {}\ntake(\"wrong\");",
            Some((
                2345,
                43,
                7,
                "Argument of type 'string' is not assignable to parameter of type 'number'.",
            )),
        ),
        (
            r#"function make(): number { return "wrong"; }"#,
            Some((
                2322,
                26,
                6,
                "Type 'string' is not assignable to type 'number'.",
            )),
        ),
        (
            r#"const point: { x: number } = { x: "wrong" };"#,
            Some((
                2322,
                31,
                1,
                "Type 'string' is not assignable to type 'number'.",
            )),
        ),
        (r#"const value: string | number = "ok";"#, None),
        ("const answer = 42;", None),
        ("let count = 1; count = 2;", None),
    ];

    for (source, expected) in cases {
        let output = compile(source);
        match expected {
            Some((code, start, length, message)) => {
                let [diagnostic] = output.diagnostics.as_slice() else {
                    panic!(
                        "expected one diagnostic for {source:?}, got {:?}",
                        output.diagnostics
                    );
                };
                assert_eq!(
                    (
                        diagnostic.code,
                        diagnostic.start,
                        diagnostic.length,
                        diagnostic.message_text.as_str(),
                    ),
                    (code, start, length, message),
                    "pinned TS7 seed mismatch for {source:?}"
                );
            }
            None => assert!(
                output.diagnostics.is_empty(),
                "pinned TS7 accepts {source:?}, got {:?}",
                output.diagnostics
            ),
        }
    }
}

#[test]
fn missing_names_are_reported() {
    let output = compile("missing;");
    assert_eq!(codes(&output), vec![2304]);
    assert_eq!(
        output.diagnostics[0].message_text,
        "Cannot find name 'missing'."
    );
}

#[test]
fn deferred_generic_aliases_are_forced_by_one_gateway() {
    let output = compile(
        r#"
        type Box<T> = { value: T };
        const box: Box<number> = { value: "wrong" };
        "#,
    );
    assert_eq!(codes(&output), vec![2322]);
}

#[test]
fn direct_alias_cycles_have_a_typed_cycle_result() {
    let output = compile("type Loop = Loop;");
    assert_eq!(codes(&output), vec![2456]);
    assert_eq!(
        output.diagnostics[0].message_text,
        "Type alias 'Loop' circularly references itself."
    );
}

#[test]
fn no_check_is_an_explicit_emit_mode() {
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "case.ts",
            Arc::<str>::from(r#"const count: number = "wrong";"#),
        )],
        &CompilerOptions {
            no_check: true,
            target: "es2022".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(output.diagnostics.is_empty());
    assert_eq!(output.emitted_files.len(), 1);
    assert_eq!(
        output.emitted_files[0].text,
        "\"use strict\";\nconst count = \"wrong\";\n"
    );
}

#[test]
fn ten_repeated_runs_and_both_source_orders_have_one_fingerprint() {
    let options = CompilerOptions {
        no_emit: true,
        strict: true,
        ..CompilerOptions::default()
    };
    let first = SourceInput::new("b.ts", Arc::<str>::from("const b: number = 'x';"));
    let second = SourceInput::new("a.ts", Arc::<str>::from("const a: string = 1;"));
    let fingerprint = |output: &tsz::CompileOutput| {
        output
            .diagnostics
            .iter()
            .map(|diagnostic| {
                (
                    diagnostic.file.clone(),
                    diagnostic.start,
                    diagnostic.code,
                    diagnostic.message_text.clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    let expected =
        fingerprint(&Compiler::new().compile(vec![first.clone(), second.clone()], &options));
    for iteration in 0..10 {
        for inputs in [
            vec![first.clone(), second.clone()],
            vec![second.clone(), first.clone()],
        ] {
            let actual = fingerprint(&Compiler::new().compile(inputs, &options));
            assert_eq!(
                actual, expected,
                "diagnostic fingerprint changed in iteration {iteration}"
            );
        }
    }
}

#[test]
fn diagnostics_use_one_based_line_and_column_rendering() {
    let output = compile("const ok = 1;\nmissing;");
    assert_eq!(codes(&output), vec![2304]);
    assert!(
        output.diagnostics[0]
            .render(output.program.source(tsz::source::FileId(0)))
            .starts_with("case.ts(2,1): error TS2304:")
    );
}

#[test]
fn quick_info_preserves_const_literals_and_widens_let_literals() {
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open(
        "case.ts",
        Arc::<str>::from("const fixed = 0; let changing = 0;"),
    );
    assert_eq!(
        service.quick_info("case.ts", 6).unwrap().display,
        "const fixed: 0"
    );
    assert_eq!(
        service.quick_info("case.ts", 21).unwrap().display,
        "let changing: number"
    );
}
