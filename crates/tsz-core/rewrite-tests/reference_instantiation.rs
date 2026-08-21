use std::sync::Arc;

use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

fn options() -> CompilerOptions {
    CompilerOptions {
        no_emit: true,
        strict: true,
        ..CompilerOptions::default()
    }
}

fn compile(source: &str) -> tsz::CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &options(),
    )
}

fn codes(output: &tsz::CompileOutput) -> Vec<u32> {
    output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn assert_complete(output: &tsz::CompileOutput) {
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        output.stats.semantic_completion,
        SemanticCompletion::Complete
    );
    assert_eq!(output.exit_status, CompileExitStatus::Success);
}

#[test]
fn closed_trailing_defaults_normalize_omitted_reference_arguments() {
    let cases = [
        "interface Box<Value=string>{value:Value}declare const box:Box;const value:string=box.value;",
        "interface Pair<Left,Right=boolean>{left:Left;right:Right}declare const pair:Pair<number>;const left:number=pair.left;const right:boolean=pair.right;",
        "interface Duo<First=number,Second=string>{first:First;second:Second}declare const duo:Duo;const first:number=duo.first;const second:string=duo.second;",
        "interface Flag<State=\"on\">{state:State}declare const flag:Flag;const state:\"on\"=flag.state;",
        "interface Vessel<Payload=(string)>{payload:Payload}declare const vessel:Vessel;const payload:string=vessel.payload;",
        "type Parcel<Payload=string>={payload:Payload};declare const parcel:Parcel;const payload:string=parcel.payload;",
    ];

    for source in cases {
        assert_complete(&compile(source));
    }
}

#[test]
fn explicit_arguments_override_closed_defaults_exactly() {
    let source = "interface Box<Value=string>{value:Value}\
                  declare const numeric:Box<number>;\
                  const value:number=numeric.value;";
    assert_complete(&compile(source));

    let mismatch = compile(
        "interface Box<Value=string>{value:Value}\
         declare const numeric:Box<number>;\
         const text:string=numeric.value;",
    );
    assert_eq!(codes(&mismatch), vec![2322], "{:?}", mismatch.diagnostics);
    assert_eq!(mismatch.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn aliases_and_class_references_preserve_closed_defaults() {
    let output = compile(
        "declare class TableClass<Subject=any>{_field:Subject}\
         type Table=TableClass;\
         declare const table:Table;table._field;\
         declare const direct:TableClass;direct._field;\
         interface Box<Value=string>{value:Value}\
         type Wrapped=Box;declare const wrapped:Wrapped;\
         const text:string=wrapped.value;",
    );
    assert_complete(&output);
}

#[test]
fn missing_arity_constraints_and_nonclosed_defaults_stay_incomplete() {
    let cases = [
        "interface Box<Value>{value:Value}declare const box:Box;box.value;",
        "interface Box<Value=string>{value:Value}declare const box:Box<string,number>;box.value;",
        "interface Box<Value extends string=string>{value:Value}declare const box:Box;box.value;",
        "interface Broken<First=string,Second>{first:First;second:Second}declare const broken:Broken;broken.first;",
        "interface Box<Value=Box>{value:Value}declare const box:Box;box.value;",
        "interface Box<Value=1n>{value:Value}declare const box:Box;box.value;",
    ];

    for source in cases {
        let output = compile(source);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{source}: {:?}",
            output.diagnostics
        );
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        assert!(output.stats.types < 512, "{source}: {:?}", output.stats);
    }
}

#[test]
fn incomplete_and_cyclic_defaults_keep_authored_diagnostic_provenance() {
    let missing = compile(
        "interface Box<Value=Missing>{value:Value}\
         declare const box:Box;box.value;",
    );
    assert_eq!(codes(&missing), vec![2304], "{:?}", missing.diagnostics);
    assert_eq!(missing.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(missing.exit_status, CompileExitStatus::SemanticIncomplete);

    let cycle = compile(
        "interface Loop<Value=Value>{value:Value}\
         declare const loop:Loop;loop.value;",
    );
    assert_eq!(codes(&cycle), vec![2744], "{:?}", cycle.diagnostics);
    assert_eq!(cycle.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(cycle.exit_status, CompileExitStatus::SemanticIncomplete);
}

#[test]
fn defaulted_recursive_references_do_not_enter_generative_admission() {
    let output = compile(
        "interface Stream<Value=string>{next:Stream<Value[]>}\
         declare let text:Stream;declare let numeric:Stream<number>;\
         text=numeric;",
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    assert!(output.stats.types < 512, "{:?}", output.stats);
}

#[test]
fn closed_default_results_are_cold_warm_and_root_order_stable() {
    let compiler = Compiler::new();
    let declarations = SourceInput::new(
        "declarations.ts",
        Arc::<str>::from(
            "interface Crate<Payload=string>{payload:Payload}\
             type Wrapped=Crate;declare const wrapped:Wrapped;",
        ),
    );
    let use_site = SourceInput::new(
        "use.ts",
        Arc::<str>::from("const payload:string=wrapped.payload;"),
    );
    let run = |files| compiler.compile(files, &options());

    let cold = run(vec![declarations.clone(), use_site.clone()]);
    let warm = run(vec![declarations.clone(), use_site.clone()]);
    let reversed = run(vec![use_site, declarations]);
    for output in [&cold, &warm, &reversed] {
        assert_complete(output);
    }
    assert_eq!(cold.stats.types, warm.stats.types);
    assert_eq!(cold.stats.types, reversed.stats.types);
}
