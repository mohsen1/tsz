use std::sync::Arc;

use tsz::{Compiler, CompilerOptions, SourceInput};

#[test]
fn public_identity_keeps_distinct_payloads_and_deduplicates_exact_host_aliases() {
    let first = "export const x:number='bad'; export const z:boolean=1;";
    let second = "export const x:string=1;";
    let output = Compiler::new().compile(
        vec![
            SourceInput::with_host_path("case.ts", "a.ts", Arc::<str>::from(first)),
            SourceInput::with_host_path("case.ts", "b.ts", Arc::<str>::from(second)),
            SourceInput::with_host_path(
                "case.ts",
                "c.ts",
                Arc::<str>::from("export const x:number='bad';"),
            ),
        ],
        &CompilerOptions {
            no_emit: true,
            strict: true,
            ..CompilerOptions::default()
        },
    );

    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.file.as_str(),
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.as_str(),
                diagnostic.related_information.len(),
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                "case.ts",
                2322,
                first.find("x:number").unwrap() as u32,
                1,
                "Type 'number' is not assignable to type 'string'.",
                0,
            ),
            (
                "case.ts",
                2322,
                first.find("x:number").unwrap() as u32,
                1,
                "Type 'string' is not assignable to type 'number'.",
                0,
            ),
            (
                "case.ts",
                2322,
                first.find("z:boolean").unwrap() as u32,
                1,
                "Type 'number' is not assignable to type 'boolean'.",
                0,
            ),
        ],
    );
}
