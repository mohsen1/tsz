use std::sync::Arc;

use tsz::diagnostics::DiagnosticCategory;
use tsz::service::LanguageService;
use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

fn options() -> CompilerOptions {
    CompilerOptions {
        target: "es2015".to_string(),
        strict: true,
        no_emit: true,
        ..CompilerOptions::default()
    }
}

fn compile(source: &str) -> tsz::CompileOutput {
    compile_with(source, options())
}

fn compile_with(source: &str, options: CompilerOptions) -> tsz::CompileOutput {
    compile_files(&[("case.ts", source)], options)
}

fn compile_files(files: &[(&str, &str)], options: CompilerOptions) -> tsz::CompileOutput {
    Compiler::new().compile(
        files
            .iter()
            .map(|(path, source)| SourceInput::new(*path, Arc::<str>::from(*source)))
            .collect(),
        &options,
    )
}

fn assert_complete(source: &str) {
    let output = compile(source);
    assert_eq!(
        output.diagnostics,
        [],
        "{source}: {:#?}",
        output.diagnostics
    );
    assert_eq!(
        output.semantic_completion,
        SemanticCompletion::Complete,
        "{source}"
    );
    assert_eq!(output.exit_status, CompileExitStatus::Success, "{source}");
}

fn assert_deferred(source: &str) {
    let output = compile(source);
    assert_eq!(
        output.diagnostics,
        [],
        "{source}: {:#?}",
        output.diagnostics
    );
    assert_eq!(
        output.semantic_completion,
        SemanticCompletion::Deferred,
        "{source}"
    );
    assert_eq!(
        output.exit_status,
        CompileExitStatus::SemanticIncomplete,
        "{source}"
    );
}

#[test]
fn canonical_array_search_calls_project_receiver_elements_for_dot_and_literal_keys() {
    for source in [
        "const values:number[]=[];const found:number=values.indexOf(1);",
        "const renamed:string[]=[];const found:number=renamed.lastIndexOf('x',0);",
        "const values:number[]=[];const found:number=values['indexOf'](1,undefined);",
        "const values:number[]=[];const found:number=(values['lastIndexOf'])(1);",
        "const values=[] as number[];const found:number=(values).indexOf(1);",
        "function locate<Element>(items:Array<Element>,hit:Element):number{return items.indexOf(hit);}",
        "type Handler=(value:unknown)=>void;declare const handlers:Array<Handler>;declare const handler:Handler;const found=handlers.indexOf(handler)>>>0;",
    ] {
        assert_complete(source);
    }
}

#[test]
fn concrete_argument_mismatches_use_the_existing_exact_ts2345_relation() {
    for (source, needle) in [
        ("const values:number[]=[];values.indexOf('bad');", "'bad'"),
        (
            "const values:number[]=[];values.lastIndexOf(1,'bad');",
            "'bad'",
        ),
        (
            "const values:number[]=[];values['indexOf']('bad');",
            "'bad'",
        ),
    ] {
        let output = compile(source);
        let [diagnostic] = output.diagnostics.as_slice() else {
            panic!("{source}: {:#?}", output.diagnostics);
        };
        assert_eq!(diagnostic.code, 2345, "{source}");
        assert_eq!(diagnostic.file, "case.ts", "{source}");
        assert_eq!(diagnostic.category, DiagnosticCategory::Error, "{source}");
        assert_eq!(
            diagnostic.start,
            source.find(needle).unwrap() as u32,
            "{source}"
        );
        assert_eq!(diagnostic.length, needle.len() as u32, "{source}");
        assert_eq!(
            diagnostic.message_text,
            "Argument of type 'string' is not assignable to parameter of type 'number'.",
            "{source}"
        );
        assert!(diagnostic.related_information.is_empty(), "{source}");
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source}"
        );
        assert_eq!(
            output.exit_status,
            CompileExitStatus::DiagnosticsPresentOutputsSkipped,
            "{source}"
        );
    }
}

#[test]
fn unsupported_array_search_call_boundaries_remain_deferred_without_fabricated_diagnostics() {
    for source in [
        "const values=[];values.indexOf(1);",
        "const values=[];const alias=values;alias.indexOf(1);",
        "const values=[];values['lastIndexOf'](1);",
        "const box={values:[]};box.values.indexOf(1);",
        "const values:never[]=[];values.indexOf(1);",
        "type Empty=never[];declare const values:Empty;declare const hit:never;(values).indexOf(hit);",
        "const values:number[]=[];values.indexOf();",
        "const values:number[]=[];values.indexOf(1,0,2);",
        "function locate<Element>(items:Array<Element>):number{return items.indexOf(0);}",
        "function locate<Element>(items:Array<Element>,value:any):number{return items.indexOf(value);}",
        "function locate<Element>(items:Array<Element>,value:never):number{return items.indexOf(value);}",
        "const values:number[]=[];values.indexOf(1,null);",
        "declare const offset:number|undefined;const values:number[]=[];values.indexOf(1,offset);",
        "const values:number[]=[];const method=values.indexOf;",
        "const values:number[]=[];const method=values['lastIndexOf'];",
        "declare const values:ReadonlyArray<number>;values.indexOf(1);",
        "declare const values:[number,number];values.indexOf(1);",
        "declare const values:number[]|string[];values.indexOf(1);",
        "declare const key:string;const values:number[]=[];values[key](1);",
        "const key='indexOf';const values:number[]=[];values[key](1);",
        "interface Array<T>{renamed?:T}const values:number[]=[];values.indexOf(1);",
    ] {
        assert_deferred(source);
    }

    let no_lib = compile_with(
        "const values:number[]=[];values.indexOf(1);",
        CompilerOptions {
            no_lib: true,
            ..options()
        },
    );
    assert_eq!(
        no_lib
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        vec![2318; 10]
    );
    assert_eq!(no_lib.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        no_lib.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );

    let custom = compile_with(
        "const values:number[]=[];values.indexOf(1);",
        CompilerOptions {
            lib: Some(vec!["es2015.core".to_string()]),
            ..options()
        },
    );
    assert_eq!(
        custom
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        vec![2318; 6]
    );
    assert_eq!(custom.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        custom.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );
}

#[test]
fn inferred_nonempty_array_search_receivers_are_not_evolving_empty_arrays() {
    for source in [
        "const values=[1,2];const found:number=values.indexOf(1);",
        "const values=['a','b'];const found:number=(values).lastIndexOf('a');",
    ] {
        assert_complete(source);
    }
}

#[test]
fn inferred_search_result_withholds_only_its_unowned_declaration_summary() {
    let output = compile_files(
        &[
            (
                "affected.ts",
                "export const values:number[]=[];export const found=values.indexOf(1);",
            ),
            (
                "stable.ts",
                "export const stable:number=1;export function kept():number{return stable;}",
            ),
        ],
        CompilerOptions {
            declaration: true,
            strict: true,
            target: "es2015".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(output.diagnostics, []);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    assert_eq!(
        output
            .emitted_files
            .iter()
            .map(|file| file.path.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        ["affected.js", "stable.d.ts", "stable.js"]
    );
}

#[test]
fn inferred_search_declaration_products_fail_closed_across_supported_hosts() {
    let emit_options = CompilerOptions {
        declaration: true,
        module: "esnext".to_string(),
        no_emit: false,
        ..options()
    };
    for source in [
        "const values:number[]=[];const found=values.indexOf(1);",
        "export class Holder{found=([] as number[]).indexOf(1);}",
        "export function locate(value=([] as number[]).indexOf(1)):void{}",
        "export class Holder{constructor(value=([] as number[]).indexOf(1)){}}",
        "export default ([] as number[]).indexOf(1);",
    ] {
        let output = compile_with(source, emit_options.clone());
        assert_eq!(
            output.diagnostics,
            [],
            "{source}: {:#?}",
            output.diagnostics
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        assert_eq!(
            output
                .emitted_files
                .iter()
                .map(|file| (file.path.to_string_lossy().into_owned(), file.declaration))
                .collect::<Vec<_>>(),
            [("case.js".to_string(), false)],
            "{source}"
        );
    }

    for source in [
        "export const values:number[]=[];export const found:number=values.indexOf(1);",
        "export class Holder{found:number=([] as number[]).indexOf(1);}",
    ] {
        let output = compile_with(source, emit_options.clone());
        assert_eq!(
            output.diagnostics,
            [],
            "{source}: {:#?}",
            output.diagnostics
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source}"
        );
        assert_eq!(output.exit_status, CompileExitStatus::Success, "{source}");
        assert_eq!(
            output
                .emitted_files
                .iter()
                .map(|file| (file.path.to_string_lossy().into_owned(), file.declaration))
                .collect::<Vec<_>>(),
            [
                ("case.d.ts".to_string(), true),
                ("case.js".to_string(), false),
            ],
            "{source}"
        );
    }
}

#[test]
fn unsupported_search_calls_preserve_independent_diagnostics_across_files() {
    let output = compile_files(
        &[
            (
                "affected.ts",
                "const values:number[]=[];values.indexOf();MissingSame;",
            ),
            ("independent.ts", "MissingCross;"),
        ],
        options(),
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.file.as_str(),
                diagnostic.code,
                diagnostic.message_text.as_str(),
            ))
            .collect::<Vec<_>>(),
        [
            ("affected.ts", 2304, "Cannot find name 'MissingSame'.",),
            ("independent.ts", 2304, "Cannot find name 'MissingCross'.",),
        ]
    );
}

#[test]
fn array_search_property_service_responses_remain_unavailable() {
    let source = "const values:number[]=[];values.indexOf(1);";
    let mut service = LanguageService::new(options());
    service.open("case.ts", Arc::<str>::from(source));
    let member = source.find("indexOf").unwrap() as u32 + 1;
    assert!(service.quick_info("case.ts", member).is_none());
    assert!(
        service
            .definition_and_bound_span("case.ts", member)
            .is_none()
    );
}

#[test]
fn array_search_selection_and_merge_fences_are_order_independent() {
    assert_complete(
        "export {};interface Array<T>{renamed?:T}const values:number[]=[];values.indexOf(1);",
    );
    let files = [
        ("merge.ts", "interface Array<T>{renamed?:T}"),
        ("use.ts", "const values:number[]=[];values.indexOf(1);"),
    ];
    for files in [files, [files[1], files[0]]] {
        let output = compile_files(&files, options());
        assert_eq!(output.diagnostics, []);
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }
    for lib in [
        vec!["es2015".to_string(), "dom".to_string()],
        vec!["dom".to_string(), "es2015".to_string()],
    ] {
        let output = compile_with(
            "const values:number[]=[];values.lastIndexOf(1);",
            CompilerOptions {
                lib: Some(lib),
                ..options()
            },
        );
        assert_eq!(output.diagnostics, []);
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }
}

#[test]
fn loose_null_and_repeated_uncached_queries_agree() {
    let source = "const values:number[]=[];values.indexOf(1,null);";
    let options = CompilerOptions {
        strict: false,
        ..options()
    };
    for _ in 0..2 {
        let output = compile_with(source, options.clone());
        assert_eq!(output.diagnostics, []);
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }
}
