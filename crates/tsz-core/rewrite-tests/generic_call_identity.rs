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

fn assert_complete(source: &str) {
    let output = compile(source);
    assert_eq!(output.diagnostics, [], "{:?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        output.stats.semantic_completion,
        SemanticCompletion::Complete
    );
    assert_eq!(output.exit_status, CompileExitStatus::Success);
}

#[test]
fn exact_recursive_union_and_expanding_interface_arguments_use_the_identity_mapper() {
    assert_complete(concat!(
        "interface Vessel<T>{value:T}",
        "function recur<T>(input:Vessel<T>|string):T{return recur(input);}",
    ));
    assert_complete(concat!(
        "interface Branch<T>{next:Branch<Branch<T>>;value:T}",
        "function visit<T>(branch:Branch<T>):void{visit(branch);}",
    ));
}

#[test]
fn renamed_callees_parentheses_and_all_multi_binders_preserve_identity() {
    assert_complete(concat!(
        "interface Cell<R>{value:R}",
        "function descend<R>(input:Cell<R>|string):R{",
        "const renamed=descend;return renamed((input));}",
    ));
    assert_complete(concat!(
        "interface Pair<Left,Right>{left:Left;right:Right}",
        "function pair<A,B>(left:A,right:B):Pair<A,B>{",
        "return pair((left),right);}",
    ));
}

#[test]
fn exact_generic_positions_do_not_hide_concrete_argument_mismatches() {
    let source = concat!(
        "function check<Item>(value:Item,count:number):Item{",
        "return check(value,\"wrong\");}",
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
            source.find("\"wrong\"").unwrap() as u32,
            "\"wrong\"".len() as u32,
            DiagnosticCategory::Error,
            "Argument of type 'string' is not assignable to parameter of type 'number'.",
            &[][..],
        )]
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        output.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );
}

#[test]
fn fixed_tuple_rest_arity_is_definitive_and_exact_supplied_elements_keep_identity() {
    let source = concat!(
        "function fill<Item>(value:Item,...tail:[number]):Item{",
        "return fill(value);}",
    );
    let output = compile(source);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.as_str(),
            ))
            .collect::<Vec<_>>(),
        vec![(
            2554,
            source.rfind("fill(value)").unwrap() as u32,
            "fill".len() as u32,
            "Expected 2 arguments, but got 1.",
        )]
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        output.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );

    assert_complete(concat!(
        "function repeat<Value>(value:Value,...tail:[Value]):Value{",
        "return repeat(value,value);}",
    ));
}

#[test]
fn missing_identity_candidates_and_generative_arguments_remain_deferred() {
    let sources = [
        concat!(
            "function unbound<Input,Output>(value:Input):Output{",
            "return unbound(value);}",
            "const independent:MissingIndependent=1;",
        ),
        concat!(
            "interface Wrap<T>{value:T}",
            "function grow<T>(value:Wrap<T>,expanded:Wrap<Wrap<T>>):T{",
            "return grow(expanded,expanded);}",
            "const independent:MissingIndependent=1;",
        ),
    ];
    for source in sources {
        let output = compile(source);
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| (diagnostic.code, diagnostic.start, diagnostic.length))
                .collect::<Vec<_>>(),
            vec![(
                2304,
                source.find("MissingIndependent").unwrap() as u32,
                "MissingIndependent".len() as u32,
            )]
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    }
}
