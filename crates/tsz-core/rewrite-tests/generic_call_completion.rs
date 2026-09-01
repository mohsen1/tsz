use std::sync::Arc;

use tsz::diagnostics::DiagnosticCategory;
use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

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

fn assert_completion(output: &tsz::CompileOutput, expected: SemanticCompletion) {
    assert_eq!(
        output.semantic_completion, expected,
        "{:?}",
        output.diagnostics
    );
    assert_eq!(output.stats.semantic_completion, expected);
    if !expected.is_complete() {
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    }
}

#[test]
fn deferred_call_context_defers_dependent_diagnostics_and_keeps_independent_ones() {
    let source = concat!(
        "const selected=[\"\"].find((entry,index,all)=>{",
        "const wrong:number=\"bad\";return true;});",
        "declare function consume<T>(callback:(item:T)=>void):void;",
        "consume<string>(((renamed)=>{const okay:string=renamed;}));",
        "declare function configure<T>(config:{handler:(item:T)=>void}):void;",
        "configure<string>({handler:((wrapped)=>{const okay:string=wrapped;})});",
        "declare function batch<T>(callbacks:((item:T)=>void)[]):void;",
        "batch<string>([((nested)=>{const okay:string=nested;})]);",
        "declare function produce<T>(factory:()=>((item:T)=>T)):void;",
        "produce<string>(()=>{return leaf=>leaf;});",
        "const model=(seed:string)=>seed;",
        "function factory():typeof model{return derived=>derived;}",
        "const independent=(orphan)=>orphan;",
    );
    let output = compile(source);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.file.as_str(),
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.category,
                diagnostic.message_text.as_str(),
                diagnostic.related_information.as_slice(),
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                "case.ts",
                2322,
                source.find("wrong").unwrap() as u32,
                5,
                DiagnosticCategory::Error,
                "Type 'string' is not assignable to type 'number'.",
                &[][..],
            ),
            (
                "case.ts",
                7006,
                source.find("orphan").unwrap() as u32,
                6,
                DiagnosticCategory::Error,
                "Parameter 'orphan' implicitly has an 'any' type.",
                &[][..],
            ),
        ]
    );
    assert_completion(&output, SemanticCompletion::Deferred);
}

#[test]
fn implicit_generic_calls_do_not_treat_signature_parameters_as_concrete_targets() {
    let source = concat!(
        "type Vessel<T>={value:T};",
        "declare function createVessel<T>(value:T):Vessel<T>;",
        "declare function wrapVessel<T>(value:{nested:T}):Vessel<T>;",
        "const first=createVessel('kept');",
        "const second=wrapVessel({nested:2});",
        "const independent:MissingIndependent=1;",
    );
    let output = compile(source);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.file.as_str(),
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.category,
                diagnostic.message_text.as_str(),
                diagnostic.related_information.as_slice(),
            ))
            .collect::<Vec<_>>(),
        vec![(
            "case.ts",
            2304,
            source.find("MissingIndependent").unwrap() as u32,
            18,
            DiagnosticCategory::Error,
            "Cannot find name 'MissingIndependent'.",
            &[][..],
        )]
    );
    assert_completion(&output, SemanticCompletion::Deferred);
}

#[test]
fn generic_call_nonclaims_follow_binders_through_aliases_and_keep_concrete_arguments() {
    let source = concat!(
        "declare function make<T>():T;",
        "const asText:string=make();",
        "const asCount:number=make();",
        "declare function identify<T>(value:T):T;",
        "const renamed=identify;",
        "const aliased=renamed('alias');",
        "declare function mix<T>(value:T,count:number):T;",
        "mix('generic','mixedBad');",
        "declare function rest<T>(...args:[number,T]):T;",
        "rest('restBad','generic');",
        "const independent:MissingIndependent=1;",
    );
    for _ in 0..2 {
        let output = compile(source);
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| (
                    diagnostic.file.as_str(),
                    diagnostic.code,
                    diagnostic.start,
                    diagnostic.length,
                    diagnostic.category,
                    diagnostic.message_text.as_str(),
                    diagnostic.related_information.as_slice(),
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    "case.ts",
                    2345,
                    source.find("'mixedBad'").unwrap() as u32,
                    "'mixedBad'".len() as u32,
                    DiagnosticCategory::Error,
                    "Argument of type 'string' is not assignable to parameter of type 'number'.",
                    &[][..],
                ),
                (
                    "case.ts",
                    2345,
                    source.find("'restBad'").unwrap() as u32,
                    "'restBad'".len() as u32,
                    DiagnosticCategory::Error,
                    "Argument of type 'string' is not assignable to parameter of type 'number'.",
                    &[][..],
                ),
                (
                    "case.ts",
                    2304,
                    source.find("MissingIndependent").unwrap() as u32,
                    "MissingIndependent".len() as u32,
                    DiagnosticCategory::Error,
                    "Cannot find name 'MissingIndependent'.",
                    &[][..],
                ),
            ]
        );
        assert_completion(&output, SemanticCompletion::Deferred);
    }
}

#[test]
fn unused_generic_binders_and_captured_outer_binders_remain_definitive() {
    let source = concat!(
        "declare function fixed<T>(count:number):number;",
        "fixed('fixedBad');",
        "function outer<T>(value:T):T{",
        "function take(input:T):T{return input;}",
        "return take(value);}",
    );
    let output = compile(source);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.file.as_str(),
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.category,
                diagnostic.message_text.as_str(),
                diagnostic.related_information.as_slice(),
            ))
            .collect::<Vec<_>>(),
        vec![(
            "case.ts",
            2345,
            source.find("'fixedBad'").unwrap() as u32,
            "'fixedBad'".len() as u32,
            DiagnosticCategory::Error,
            "Argument of type 'string' is not assignable to parameter of type 'number'.",
            &[][..],
        )]
    );
    assert_completion(&output, SemanticCompletion::Complete);
}
