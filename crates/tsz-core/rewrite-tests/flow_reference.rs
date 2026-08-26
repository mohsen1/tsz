use std::sync::Arc;

use tsz::diagnostics::{DiagnosticCategory, RelatedInformation};
use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

type Fingerprint = (
    String,
    u32,
    u32,
    u32,
    DiagnosticCategory,
    String,
    Vec<(String, u32, u32, u32, String, u32)>,
);

fn fingerprint_related(
    related: &[RelatedInformation],
) -> Vec<(String, u32, u32, u32, String, u32)> {
    related
        .iter()
        .map(|related| {
            (
                related.file.clone(),
                related.code,
                related.start,
                related.length,
                related.message_text.clone(),
                related.depth,
            )
        })
        .collect()
}

fn compile(path: &str, source: &str) -> tsz::CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new(path, Arc::<str>::from(source))],
        &CompilerOptions {
            no_emit: true,
            strict: true,
            target: "es2015".to_string(),
            ..CompilerOptions::default()
        },
    )
}

fn fingerprints(output: &tsz::CompileOutput) -> Vec<Fingerprint> {
    output
        .diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.file.clone(),
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.category,
                diagnostic.message_text.clone(),
                fingerprint_related(&diagnostic.related_information),
            )
        })
        .collect()
}

fn codes(output: &tsz::CompileOutput) -> Vec<u32> {
    output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn assignment(path: &str, source: &str, name: &str, actual: &str, expected: &str) -> Fingerprint {
    let related = if actual == "string | number" && expected == "string" {
        vec![(
            String::new(),
            2322,
            0,
            0,
            "Type 'number' is not assignable to type 'string'.".to_string(),
            1,
        )]
    } else {
        Vec::new()
    };
    (
        path.to_string(),
        2322,
        source.find(name).expect("assignment name") as u32,
        name.len() as u32,
        DiagnosticCategory::Error,
        format!("Type '{actual}' is not assignable to type '{expected}'."),
        related,
    )
}

fn missing(path: &str, source: &str, name: &str) -> Fingerprint {
    (
        path.to_string(),
        2304,
        source.find(name).expect("missing name") as u32,
        name.len() as u32,
        DiagnosticCategory::Error,
        format!("Cannot find name '{name}'."),
        Vec::new(),
    )
}

fn argument(
    path: &str,
    source: &str,
    call: &str,
    name: &str,
    actual: &str,
    expected: &str,
) -> Fingerprint {
    let call_start = source.find(call).expect("call argument");
    let related = match (actual, expected) {
        ("string | number", "string") => vec![(
            String::new(),
            2345,
            0,
            0,
            "Type 'number' is not assignable to type 'string'.".to_string(),
            1,
        )],
        ("string | number", "number") => vec![(
            String::new(),
            2345,
            0,
            0,
            "Type 'string' is not assignable to type 'number'.".to_string(),
            1,
        )],
        _ => Vec::new(),
    };
    (
        path.to_string(),
        2345,
        (call_start + call.find(name).expect("argument name")) as u32,
        name.len() as u32,
        DiagnosticCategory::Error,
        format!("Argument of type '{actual}' is not assignable to parameter of type '{expected}'."),
        related,
    )
}

#[test]
fn direct_and_parenthesized_typeof_switches_narrow_only_matching_references() {
    for (path, subject, wrapper) in [
        ("direct.ts", "subject", "subject"),
        ("renamed.ts", "candidate", "(((candidate)))"),
    ] {
        let source = format!(
            concat!(
                "declare function takeText(value:string):void;",
                "function inspect({subject}:string|number,other:string|number):void{{",
                "const beforeSubject:string={subject};",
                "switch(typeof {wrapper}){{",
                "case 'string':",
                "takeText({subject});",
                "const insideOther:string=other;",
                "MissingInside;break;",
                "default:break;}}",
                "const afterOther:string=other;",
                "MissingAfter;}}",
            ),
            subject = subject,
            wrapper = wrapper,
        );
        let expected = vec![
            assignment(path, &source, "beforeSubject", "string | number", "string"),
            assignment(path, &source, "insideOther", "string | number", "string"),
            missing(path, &source, "MissingInside"),
            assignment(path, &source, "afterOther", "string | number", "string"),
            missing(path, &source, "MissingAfter"),
        ];
        for _ in 0..2 {
            let output = compile(path, &source);
            assert_eq!(fingerprints(&output), expected, "{path}");
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Complete,
                "{path}"
            );
            assert_eq!(
                output.stats.semantic_completion,
                SemanticCompletion::Complete
            );
            assert_eq!(
                output.exit_status,
                CompileExitStatus::DiagnosticsPresentOutputsSkipped
            );
        }
    }
}

#[test]
fn nested_switches_and_shadowed_binders_keep_distinct_flow_identities() {
    let source = concat!(
        "declare function takeText(value:string):void;",
        "declare function takeNumber(value:number):void;",
        "function inspect(value:string|number,other:string|number):void{",
        "switch(typeof value){case 'string':",
        "{let value:number=1;takeNumber(value);}",
        "const nested=():void=>{switch(typeof (((other)))){",
        "case 'number':takeNumber(other);break;default:break;}};nested();",
        "takeText(value);MissingNested;break;default:break;}}",
    );
    for path in ["nested.ts", "nested-renamed.ts"] {
        let output = compile(path, source);
        assert_eq!(
            fingerprints(&output),
            vec![missing(path, source, "MissingNested")]
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }
}

#[test]
fn script_global_flow_subjects_finalize_after_program_binding() {
    for (producer_path, consumer_path, subject) in [
        ("a-global.ts", "b-use.ts", "value"),
        ("a-global-renamed.ts", "b-use-renamed.ts", "candidate"),
    ] {
        let producer =
            format!("let {subject}:string|number=0;declare function takeText(value:string):void;");
        let consumer = format!(
            concat!(
                "switch(typeof ((({subject})))){{case 'string':takeText({subject});",
                "const independentWrong:string=1;MissingGlobal;break;default:break;}}",
            ),
            subject = subject,
        );
        let roots = vec![
            SourceInput::new(producer_path, Arc::<str>::from(producer.clone())),
            SourceInput::new(consumer_path, Arc::<str>::from(consumer.clone())),
        ];
        let mut reversed = roots.clone();
        reversed.reverse();
        for inputs in [roots.clone(), roots, reversed] {
            let output = Compiler::new().compile(inputs, &CompilerOptions::default());
            assert_eq!(
                fingerprints(&output),
                vec![
                    assignment(
                        consumer_path,
                        &consumer,
                        "independentWrong",
                        "number",
                        "string",
                    ),
                    missing(consumer_path, &consumer, "MissingGlobal"),
                ],
                "{consumer_path}",
            );
            assert!(!matches!(
                output.semantic_completion,
                SemanticCompletion::Cycle | SemanticCompletion::Limit
            ));
        }
    }
}

#[test]
fn switch_discriminants_resolve_before_case_block_lexicals() {
    for (path, subject) in [("shadow.ts", "value"), ("shadow-renamed.ts", "candidate")] {
        let source = format!(
            concat!(
                "function inspect({subject}:string|number):void{{",
                "switch(typeof ((({subject})))){{case 'string':",
                "let {subject}:number=1;const wrong:string={subject};break;default:break;}}}}",
            ),
            subject = subject,
        );
        let output = compile(path, &source);
        assert_eq!(
            fingerprints(&output),
            vec![assignment(path, &source, "wrong", "number", "string")],
            "{path}",
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }
}

#[test]
fn captured_outer_subjects_defer_at_fresh_function_boundaries() {
    for (path, subject) in [("capture.ts", "value"), ("capture-renamed.ts", "candidate")] {
        let source = format!(
            concat!(
                "declare function takeText(value:string):void;",
                "function inspect({subject}:string|number,other:string|number):void{{",
                "switch(typeof ((({subject})))){{case 'string':",
                "const later=()=>{{takeText({subject});",
                "const independentWrong:string=other;MissingInside;}};",
                "later();break;default:break;}}}}",
            ),
            subject = subject,
        );
        let output = compile(path, &source);
        assert_eq!(
            fingerprints(&output),
            vec![
                assignment(
                    path,
                    &source,
                    "independentWrong",
                    "string | number",
                    "string",
                ),
                missing(path, &source, "MissingInside"),
            ],
            "{path}",
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }
}

#[test]
fn ordinary_functions_do_not_capture_creation_point_narrowing() {
    for (path, subject) in [
        ("ordinary-function.ts", "value"),
        ("ordinary-function-renamed.ts", "candidate"),
    ] {
        let source = format!(
            concat!(
                "declare function takeText(value:string):void;",
                "function inspect({subject}:string|number):void{{",
                "switch(typeof ((({subject})))){{case 'string':",
                "function nested():void{{takeText({subject});MissingOrdinary;}}",
                "nested();break;default:break;}}}}",
            ),
            subject = subject,
        );
        let call = format!("takeText({subject})");
        let output = compile(path, &source);
        assert_eq!(
            fingerprints(&output),
            vec![
                argument(path, &source, &call, subject, "string | number", "string",),
                missing(path, &source, "MissingOrdinary"),
            ],
            "{path}",
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }
}

#[test]
fn callable_and_construct_only_object_shapes_use_the_function_witness() {
    let source = concat!(
        "interface Constructable{new():{value:number};}",
        "declare function takeConstructor(value:Constructable):void;",
        "declare function takeText(value:string):void;",
        "function inspect(value:Constructable|string):void{",
        "switch(typeof value){",
        "case 'function':takeConstructor(value);break;",
        "case 'string':takeText(value);break;default:break;}}",
    );
    let output = compile("construct.ts", source);
    assert_eq!(fingerprints(&output), []);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(output.exit_status, CompileExitStatus::Success);
}

#[test]
fn adjacent_empty_labels_and_default_exclusion_are_complete() {
    let source = concat!(
        "declare function takeScalar(value:string|number|boolean|bigint|symbol|undefined):void;",
        "declare function takeRecord(value:{tag:string}):void;",
        "function inspect(choice:string|number|boolean|bigint|symbol|undefined|{tag:string}):void{",
        "switch(typeof (((choice)))){case 'string':case 'number':case 'boolean':",
        "case 'bigint':case 'symbol':case 'undefined':takeScalar(choice);break;",
        "default:takeRecord(choice);break;}}",
    );
    for path in ["labels.ts", "labels-renamed.ts"] {
        let output = compile(path, source);
        assert_eq!(fingerprints(&output), [], "{path}");
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{path}"
        );
        assert_eq!(output.exit_status, CompileExitStatus::Success);
    }
}

#[test]
fn empty_default_fallthrough_defers_the_matching_reference() {
    for (path, subject) in [("default.ts", "value"), ("default-renamed.ts", "candidate")] {
        let source = format!(
            concat!(
                "function inspect({subject}:string|number,other:string|number):void{{",
                "switch(typeof ((({subject})))){{default:case 'string':",
                "const dependentWrong:string={subject};",
                "const independentWrong:string=other;MissingIndependent;}}}}",
            ),
            subject = subject,
        );
        let output = compile(path, &source);
        assert_eq!(
            fingerprints(&output),
            vec![
                assignment(
                    path,
                    &source,
                    "independentWrong",
                    "string | number",
                    "string",
                ),
                missing(path, &source, "MissingIndependent"),
            ],
            "{path}",
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }
}

#[test]
fn unsupported_graph_nodes_defer_only_the_matching_reference() {
    for (path, case_label) in [("unsupported.ts", "'other'"), ("wrapped.ts", "(('other'))")] {
        let source = format!(
            concat!(
                "declare function takeText(value:string):void;",
                "function inspect(value:string|number,independent:string|number){{",
                "switch(typeof (((value)))){{case {case_label}:",
                "takeText(value);",
                "const keptWrong:string=independent;",
                "MissingKept;break;default:break;}}",
                "const afterWrong:string=independent;MissingAfter;}}",
            ),
            case_label = case_label,
        );
        let output = compile(path, &source);
        assert_eq!(
            fingerprints(&output),
            vec![
                assignment(path, &source, "keptWrong", "string | number", "string"),
                missing(path, &source, "MissingKept"),
                assignment(path, &source, "afterWrong", "string | number", "string"),
                missing(path, &source, "MissingAfter"),
            ],
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }
}

#[test]
fn an_unforced_unsupported_flow_reference_does_not_change_completion() {
    let path = "unused.ts";
    let source = concat!(
        "function inspect(value:string|number):void{",
        "switch(typeof value){case 'unsupported':value;MissingIndependent;break;",
        "default:break;}}",
    );
    let output = compile(path, source);
    assert_eq!(
        fingerprints(&output),
        vec![missing(path, source, "MissingIndependent")]
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn assignment_in_a_clause_defers_the_subject_without_poisoning_siblings() {
    let path = "assignment.ts";
    let source = concat!(
        "declare function takeText(value:string):void;",
        "function inspect(candidate:string|number,other:string|number){",
        "switch(typeof candidate){case 'string':",
        "candidate=1;takeText(candidate);",
        "const independentWrong:string=other;MissingIndependent;break;",
        "default:break;}}",
    );
    let output = compile(path, source);
    assert_eq!(
        fingerprints(&output),
        vec![
            argument(
                path,
                source,
                "takeText(candidate)",
                "candidate",
                "number",
                "string",
            ),
            assignment(
                path,
                source,
                "independentWrong",
                "string | number",
                "string",
            ),
            missing(path, source, "MissingIndependent"),
        ],
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn simple_assignment_mutations_defer_only_the_affected_declaration_after_the_write() {
    for (path, subject, target) in [
        ("mutation.ts", "subject", "other"),
        ("mutation-renamed.ts", "choice", "peer"),
    ] {
        let source = format!(
            concat!(
                "declare function takeNumber(value:number):void;",
                "function inspect({subject}:string|number,{target}:string|number):void{{",
                "switch(typeof ((({subject})))){{case 'string':",
                "const beforeWrong:string={target};({target})=1;takeNumber({target});",
                "const independentWrong:string=1;MissingInside;break;default:break;}}",
                "takeNumber({target});const afterWrong:string=1;MissingAfter;}}",
            ),
            subject = subject,
            target = target,
        );
        let output = compile(path, &source);
        assert_eq!(
            fingerprints(&output),
            vec![
                assignment(path, &source, "beforeWrong", "string | number", "string"),
                assignment(path, &source, "independentWrong", "number", "string"),
                missing(path, &source, "MissingInside"),
                assignment(path, &source, "afterWrong", "number", "string"),
                missing(path, &source, "MissingAfter"),
            ],
            "{path}",
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }
}

#[test]
fn unsupported_clause_and_exit_state_close_the_post_switch_subject_query() {
    for (path, subject, clause) in [
        (
            "join-assignment.ts",
            "value",
            "case 'string':value=1;break;default:break;",
        ),
        (
            "join-return-renamed.ts",
            "candidate",
            "case 'string':{return;}default:break;",
        ),
        (
            "join-call.ts",
            "item",
            "case 'string':takeNumber(1);break;default:break;",
        ),
    ] {
        let source = format!(
            concat!(
                "declare function takeNumber(value:number):void;",
                "function inspect({subject}:string|number):void{{",
                "switch(typeof ((({subject})))){{{clause}}}",
                "takeNumber({subject});const independentWrong:string=1;MissingIndependent;}}",
            ),
            subject = subject,
            clause = clause,
        );
        let output = compile(path, &source);
        assert_eq!(
            fingerprints(&output),
            vec![
                assignment(path, &source, "independentWrong", "number", "string"),
                missing(path, &source, "MissingIndependent"),
            ],
            "{path}",
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }
}

#[test]
fn nonempty_fallthrough_defers_only_the_consumed_subject() {
    let path = "fallthrough.ts";
    let source = concat!(
        "declare function takeText(value:string):void;",
        "declare function takeNumber(value:number):void;",
        "function inspect(value:string|number,other:string|number):void{",
        "switch(typeof value){case 'string':takeText(value);",
        "case 'number':takeNumber(value);",
        "const independentWrong:string=other;MissingIndependent;break;",
        "default:break;}}",
    );
    let output = compile(path, source);
    assert_eq!(
        fingerprints(&output),
        vec![
            assignment(
                path,
                source,
                "independentWrong",
                "string | number",
                "string",
            ),
            missing(path, source, "MissingIndependent"),
        ],
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn unsupported_broad_object_void_and_generic_types_fail_closed_at_the_query() {
    for (path, annotation, case_label, expected) in [
        ("object.ts", "object", "'string'", "never"),
        ("void.ts", "void", "'undefined'", "undefined"),
        ("generic.ts", "T", "'string'", "string"),
    ] {
        let type_parameters = if annotation == "T" {
            "<T extends string|number>"
        } else {
            ""
        };
        let source = format!(
            concat!(
                "declare function consume(value:{expected}):void;",
                "function inspect{type_parameters}(value:{annotation},other:string|number){{",
                "switch(typeof value){{case {case_label}:",
                "consume(value);const independentWrong:string=other;MissingIndependent;break;",
                "default:break;}}}}",
            ),
            expected = expected,
            type_parameters = type_parameters,
            annotation = annotation,
            case_label = case_label,
        );
        let output = compile(path, &source);
        assert_eq!(
            fingerprints(&output),
            vec![
                assignment(
                    path,
                    &source,
                    "independentWrong",
                    "string | number",
                    "string",
                ),
                missing(path, &source, "MissingIndependent"),
            ],
            "{path}",
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{path}"
        );
    }
}

#[test]
fn string_equality_narrows_empty_object_like_values_in_both_orders() {
    let direct = concat!(
        "const check=(value:unknown):string=>{",
        "if(!value){return 'fallback';}",
        "if((((value)))===`xyz`){return value;}",
        "return '';};",
    );
    let reversed = concat!(
        "const inspect=(candidate:unknown):string=>{",
        "if('xyz'!==(((candidate)))){return '';}",
        "else{return candidate;}};",
    );
    for strict_null_checks in [false, true] {
        for (path, source) in [
            ("literal-direct.ts", direct),
            ("literal-reversed.ts", reversed),
        ] {
            let output = Compiler::new().compile(
                vec![SourceInput::new(path, Arc::<str>::from(source))],
                &CompilerOptions {
                    no_emit: true,
                    strict: true,
                    strict_null_checks: Some(strict_null_checks),
                    ..CompilerOptions::default()
                },
            );
            assert_eq!(
                fingerprints(&output),
                [],
                "{path}, strictNullChecks={strict_null_checks}"
            );
            assert!(!matches!(
                output.semantic_completion,
                SemanticCompletion::Cycle | SemanticCompletion::Limit
            ));
        }
    }

    let negative = concat!(
        "const reject=(value:unknown):number=>{",
        "if(value==='xyz'){return value;}",
        "return 0;};",
    );
    let output = compile("literal-negative.ts", negative);
    assert_eq!(codes(&output), [2322]);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn direct_string_union_equality_filters_concrete_non_string_members() {
    for (path, subject, operator, reversed, equality) in [
        ("strict-equal.ts", "value", "===", false, true),
        ("strict-equal-reversed.ts", "candidate", "===", true, true),
        ("loose-equal.ts", "item", "==", false, true),
        ("loose-equal-reversed.ts", "choice", "==", true, true),
        ("strict-not-equal.ts", "entry", "!==", false, false),
        ("strict-not-equal-reversed.ts", "option", "!==", true, false),
        ("loose-not-equal.ts", "selection", "!=", false, false),
        ("loose-not-equal-reversed.ts", "result", "!=", true, false),
    ] {
        let condition = if reversed {
            format!("'x'{operator}((({subject})))")
        } else {
            format!("((({subject}))){operator}'x'")
        };
        let (then_statement, else_statement) = if equality {
            (
                format!("takeOtherText({subject});"),
                format!("takeOtherNumber({subject});"),
            )
        } else {
            (
                format!("takeOtherNumber({subject});"),
                format!("takeOtherText({subject});"),
            )
        };
        let source = format!(
            concat!(
                "declare function takeOtherText(value:'y'):void;",
                "declare function takeOtherNumber(value:2):void;",
                "function inspect({subject}:'x'|1):void{{",
                "if({condition}){{{then_statement}}}else{{{else_statement}}}}}",
            ),
            subject = subject,
            condition = condition,
            then_statement = then_statement,
            else_statement = else_statement,
        );
        let output = compile(path, &source);
        let number_call = format!("takeOtherNumber({subject})");
        let string_call = format!("takeOtherText({subject})");
        let number_wrong = argument(path, &source, &number_call, subject, "1", "2");
        let string_wrong = argument(path, &source, &string_call, subject, "\"x\"", "\"y\"");
        assert_eq!(
            fingerprints(&output),
            if equality {
                vec![string_wrong, number_wrong]
            } else {
                vec![number_wrong, string_wrong]
            },
            "{path}",
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }
}

#[test]
fn a_single_returning_if_branch_propagates_the_opposite_direct_narrowing() {
    for (
        path,
        subject,
        annotation,
        condition,
        branches,
        consumer,
        rejected_consumer,
        actual,
        expected,
    ) in [
        (
            "return-then.ts",
            "value",
            "'stop'|number",
            "(((value)))==='stop'",
            "{return;}",
            "takeNumber",
            "takeText",
            "number",
            "string",
        ),
        (
            "return-then-reversed.ts",
            "candidate",
            "string|number",
            "'stop'!==(((candidate)))",
            "{return;}else{takeText(candidate);}",
            "takeText",
            "takeGo",
            "\"stop\"",
            "\"go\"",
        ),
        (
            "return-else-renamed.ts",
            "item",
            "'stop'|number",
            "(((item)))!=='stop'",
            "{takeNumber(item);}else{return;}",
            "takeNumber",
            "takeText",
            "number",
            "string",
        ),
    ] {
        let source = format!(
            concat!(
                "declare function takeText(value:string):void;",
                "declare function takeNumber(value:number):void;",
                "declare function takeGo(value:'go'):void;",
                "function inspect({subject}:{annotation},other:string|number):void{{",
                "if({condition}){branches}{consumer}((({subject})));",
                "{rejected_consumer}({subject});",
                "const independentWrong:string=other;MissingIndependent;}}",
            ),
            subject = subject,
            annotation = annotation,
            condition = condition,
            branches = branches,
            consumer = consumer,
            rejected_consumer = rejected_consumer,
        );
        let output = compile(path, &source);
        let rejected_call = format!("{rejected_consumer}({subject})");
        assert_eq!(
            fingerprints(&output),
            vec![
                argument(path, &source, &rejected_call, subject, actual, expected,),
                assignment(
                    path,
                    &source,
                    "independentWrong",
                    "string | number",
                    "string",
                ),
                missing(path, &source, "MissingIndependent"),
            ],
            "{path}",
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }

    let path = "non-return-control.ts";
    let source = concat!(
        "declare function takeText(value:string):void;",
        "function inspect(value:string|number):void{",
        "if((((value)))==='stop'){takeText(value);}",
        "const stillUnion:string=value;MissingControl;}",
    );
    let output = compile(path, source);
    assert_eq!(
        fingerprints(&output),
        vec![
            assignment(path, source, "stillUnion", "string | number", "string"),
            missing(path, source, "MissingControl"),
        ],
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn property_and_element_discriminants_narrow_renamed_wrapped_unions() {
    let source = concat!(
        "type Left={kind:'left';left:number};",
        "type Right={kind:'right';right:string};",
        "declare function takeLeft(value:Left):void;",
        "declare function takeRight(value:Right):void;",
        "declare function takeNever(value:never):void;",
        "function inspect(candidate:Left|Right):void{",
        "if('right'===((((candidate)))[`kind`])){takeRight(candidate);}",
        "else{takeLeft(candidate);}",
        "switch((((candidate)))['kind']){",
        "case 'left':takeLeft(candidate);break;",
        "case `right`:takeRight(candidate);break;",
        "default:takeNever(candidate);}}",
    );
    let output = compile("literal-discriminants.ts", source);
    assert_eq!(fingerprints(&output), []);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn nonexhaustive_literal_default_retains_the_remaining_union_member() {
    let source = concat!(
        "type Alpha={kind:'alpha';a:number};",
        "type Beta={kind:'beta';b:string};",
        "declare function takeAlpha(value:Alpha):void;",
        "function inspect(value:Alpha|Beta):void{",
        "switch(value.kind){case 'alpha':break;default:takeAlpha(value);}}",
    );
    let output = compile("literal-nonexhaustive.ts", source);
    assert_eq!(codes(&output), [2345]);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn default_only_switches_preserve_subject_diagnostics_inside_and_after() {
    for (path, subject) in [
        ("default-only.ts", "candidate"),
        ("default-only-renamed.ts", "item"),
    ] {
        let source = format!(
            concat!(
                "declare function takeText(value:string):void;",
                "function inspect({subject}:{{tag:string}},other:string|number):void{{",
                "const beforeWrong:string={subject};",
                "switch(((({subject}))).tag){{default:",
                "takeText({subject});const independentWrong:string=other;MissingInside;}}",
                "const afterWrong:string={subject};takeText(((({subject}))));MissingAfter;}}",
            ),
            subject = subject,
        );
        let inside_call = format!("takeText({subject})");
        let wrapped = format!("((({subject})))");
        let after_call = format!("takeText({wrapped})");
        let output = compile(path, &source);
        assert_eq!(
            fingerprints(&output),
            vec![
                assignment(path, &source, "beforeWrong", "{ tag: string; }", "string"),
                argument(
                    path,
                    &source,
                    &inside_call,
                    subject,
                    "{ tag: string; }",
                    "string"
                ),
                assignment(
                    path,
                    &source,
                    "independentWrong",
                    "string | number",
                    "string"
                ),
                missing(path, &source, "MissingInside"),
                assignment(path, &source, "afterWrong", "{ tag: string; }", "string"),
                argument(
                    path,
                    &source,
                    &after_call,
                    &wrapped,
                    "{ tag: string; }",
                    "string"
                ),
                missing(path, &source, "MissingAfter"),
            ],
            "{path}",
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{path}"
        );
    }
}

#[test]
fn default_only_mutations_defer_the_assignment_value_owner_without_poisoning_siblings() {
    // Deletion owner: replace this containment when a typed assignment-value flow node
    // lets the checker evaluate the right-hand side instead of deferring the mutation.
    for (path, subject, other) in [
        ("default-mutation.ts", "candidate", "other"),
        ("default-mutation-renamed.ts", "item", "peer"),
    ] {
        let source = format!(
            concat!(
                "declare function takeNumber(value:number):void;",
                "function inspect({subject}:string|number,{other}:string|number):void{{",
                "switch(typeof ((({subject})))){{default:",
                "takeNumber({subject});((({subject})))=1;takeNumber({subject});",
                "const independentWrong:string={other};MissingInside;}}",
                "takeNumber({subject});const afterWrong:string={other};MissingAfter;}}",
            ),
            subject = subject,
            other = other,
        );
        let before_call = format!("takeNumber({subject})");
        let output = compile(path, &source);
        assert_eq!(
            fingerprints(&output),
            vec![
                argument(
                    path,
                    &source,
                    &before_call,
                    subject,
                    "string | number",
                    "number",
                ),
                assignment(
                    path,
                    &source,
                    "independentWrong",
                    "string | number",
                    "string",
                ),
                missing(path, &source, "MissingInside"),
                assignment(path, &source, "afterWrong", "string | number", "string",),
                missing(path, &source, "MissingAfter"),
            ],
            "{path}",
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }
}

#[test]
fn literal_flow_mutations_and_unsupported_paths_defer_only_the_subject() {
    let mutation = concat!(
        "type A={kind:'a';a:number};type B={kind:'b';b:string};",
        "declare function takeA(value:A):void;",
        "function inspect(value:A|B,replacement:A|B,other:string|number):void{",
        "if(value.kind==='a'){value=replacement;takeA(value);",
        "const independentWrong:string=other;MissingMutation;}}",
    );
    let output = compile("literal-mutation.ts", mutation);
    let argument =
        mutation.find("takeA(value)").expect("mutated argument") as u32 + "takeA(".len() as u32;
    assert_eq!(
        fingerprints(&output),
        vec![
            (
                "literal-mutation.ts".to_string(),
                2345,
                argument,
                "value".len() as u32,
                DiagnosticCategory::Error,
                "Argument of type 'A | B' is not assignable to parameter of type 'A'."
                    .to_string(),
                vec![
                    (
                        String::new(),
                        2345,
                        0,
                        0,
                        "Type '{ kind: \"b\"; b: string; }' is not assignable to type '{ kind: \"a\"; a: number; }'.".to_string(),
                        1,
                    ),
                    (
                        String::new(),
                        2345,
                        0,
                        0,
                        "Property 'a' is missing in type '{ kind: \"b\"; b: string; }' but required in type '{ kind: \"a\"; a: number; }'.".to_string(),
                        2,
                    ),
                ],
            ),
            assignment(
                "literal-mutation.ts",
                mutation,
                "independentWrong",
                "string | number",
                "string",
            ),
            missing("literal-mutation.ts", mutation, "MissingMutation"),
        ]
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);

    let unsupported = concat!(
        "type A={meta:{kind:'a'};a:number};type B={meta:{kind:'b'};b:string};",
        "declare function takeA(value:A):void;",
        "function inspect(candidate:A|B,other:string|number):void{",
        "if(candidate.meta.kind==='a'){takeA(candidate);",
        "const independentWrong:string=other;MissingUnsupported;}}",
    );
    let output = compile("literal-unsupported.ts", unsupported);
    assert_eq!(
        fingerprints(&output),
        vec![
            assignment(
                "literal-unsupported.ts",
                unsupported,
                "independentWrong",
                "string | number",
                "string",
            ),
            missing("literal-unsupported.ts", unsupported, "MissingUnsupported"),
        ]
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn nested_different_subjects_preserve_outer_flow_antecedents() {
    let outer_x = concat!(
        "declare function takeX(value:'x'):void;",
        "function inspect(x:string,y:string):void{",
        "if(x==='x'){if(y==='y'){takeX(x);}}}",
    );
    let outer_y = concat!(
        "declare function takeY(value:'y'):void;",
        "function inspect(x:string,y:string):void{",
        "if(y==='y'){if(x==='x'){takeY(y);}}}",
    );
    for (path, source) in [
        ("outer-x-inner-y.ts", outer_x),
        ("outer-y-inner-x.ts", outer_y),
    ] {
        let output = compile(path, source);
        assert_eq!(fingerprints(&output), [], "{path}");
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{path}"
        );
    }
}

#[test]
fn straight_line_map_fallback_flow_is_structural_and_ordered() {
    let prefix = "interface Holder{value:Map<string,number>;}";
    for (path, setup, body) in [
        ("map-direct.ts", "", "target=target||new Map();"),
        (
            "map-parenthesized.ts",
            "",
            "target=(((target))||((new Map())));",
        ),
        (
            "map-renamed.ts",
            "const Empty=Map;",
            "target=target||new Empty();",
        ),
        ("map-nested.ts", "", "{target=target||new Map();}"),
        (
            "map-if-before.ts",
            "",
            "if(target){void target;}target=target||new Map();",
        ),
        (
            "map-if-after.ts",
            "",
            "target=target||new Map();if(target){void target;}",
        ),
    ] {
        let source = format!(
            "{setup}{prefix}export function fill(target?:Map<string,number>):Holder{{{body}return{{value:target}};}}"
        );
        for _ in 0..2 {
            let output = compile(path, &source);
            assert_eq!(fingerprints(&output), [], "{path}");
            assert_eq!(output.exit_status, CompileExitStatus::Success, "{path}");
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Complete,
                "{path}"
            );
        }
    }

    for (path, parameters, body) in [
        (
            "map-reverse.ts",
            "target?:Map<string,number>",
            "target=new Map()||target;",
        ),
        (
            "map-and.ts",
            "target?:Map<string,number>",
            "target=target&&new Map();",
        ),
        (
            "map-nullish.ts",
            "target?:Map<string,number>",
            "target=target??new Map();",
        ),
        (
            "map-nonself.ts",
            "target?:Map<string,number>,other?:Map<string,number>",
            "target=other||new Map();",
        ),
        (
            "map-later-write.ts",
            "target?:Map<string,number>",
            "target=target||new Map();target=undefined;",
        ),
        (
            "map-prior-write.ts",
            "target?:Map<string,number>",
            "target=undefined;target=target||new Map();",
        ),
    ] {
        let source = format!(
            "{prefix}export function deferred({parameters}):Holder{{{body}return{{value:target}};}}"
        );
        let output = compile(path, &source);
        assert_eq!(fingerprints(&output), [], "{path}");
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{path}"
        );
    }

    let never_target = compile(
        "map-never.ts",
        concat!(
            "interface Holder{value:Map<never,unknown>;} ",
            "export function fill(target?:Map<never,unknown>):Holder{",
            "target=target||new Map();return{value:target};}",
        ),
    );
    assert_eq!(fingerprints(&never_target), []);
    assert_eq!(
        never_target.semantic_completion,
        SemanticCompletion::Deferred
    );

    let mut fallback = "new Map()".to_string();
    for _ in 0..105 {
        fallback = format!("target||({fallback})");
    }
    let deep = compile(
        "map-deep.ts",
        &format!("export function fill(target?:Map<string,number>){{target={fallback};}}"),
    );
    assert_eq!(fingerprints(&deep), []);
    assert_eq!(deep.semantic_completion, SemanticCompletion::Complete);

    for (path, source, expected) in [
        (
            "map-generic.ts",
            concat!(
                "interface Holder<Key>{value:Map<Key,number>;} ",
                "export function fill<Key>(target?:Map<Key,number>):Holder<Key>{",
                "target=target||new Map();return{value:target};}",
            ),
            SemanticCompletion::Complete,
        ),
        (
            "map-alias.ts",
            concat!(
                "type Bag<Key,Value>=Map<Key,Value>;interface Holder{value:Bag<string,number>;} ",
                "export function fill(target?:Bag<string,number>):Holder{",
                "target=target||new Map();return{value:target};}",
            ),
            SemanticCompletion::Complete,
        ),
        (
            "map-shadow.ts",
            concat!(
                "class Map<Key,Value>{}interface Holder{value:Map<string,number>;} ",
                "export function fill(target?:Map<string,number>):Holder{",
                "target=target||new Map();return{value:target};}",
            ),
            SemanticCompletion::Deferred,
        ),
    ] {
        let output = compile(path, source);
        assert_eq!(fingerprints(&output), [], "{path}");
        assert_eq!(output.semantic_completion, expected, "{path}");
    }

    let loose = Compiler::new().compile(
        vec![SourceInput::new(
            "map-loose.ts",
            Arc::<str>::from(concat!(
                "interface Holder{value:Map<string,number>;} ",
                "export function fill(target?:Map<string,number>):Holder{",
                "target=target||new Map();return{value:target};}",
            )),
        )],
        &CompilerOptions {
            no_emit: true,
            strict_null_checks: Some(false),
            target: "es2015".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(fingerprints(&loose), []);
    assert_eq!(loose.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn canonical_map_flow_resolution_is_root_order_stable() {
    let declarations = SourceInput::new(
        "types.ts",
        Arc::<str>::from("interface Holder{value:Map<string,number>;}"),
    );
    let implementation = SourceInput::new(
        "implementation.ts",
        Arc::<str>::from(concat!(
            "export function fill(target?:Map<string,number>):Holder{",
            "target=target||new Map();return{value:target};}",
        )),
    );
    for roots in [
        vec![declarations.clone(), implementation.clone()],
        vec![implementation, declarations],
    ] {
        let output = Compiler::new().compile(
            roots,
            &CompilerOptions {
                no_emit: true,
                strict: true,
                target: "es2015".to_string(),
                ..CompilerOptions::default()
            },
        );
        assert_eq!(fingerprints(&output), []);
        assert_eq!(output.exit_status, CompileExitStatus::Success);
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }
}

#[test]
fn straight_line_modeled_sources_rebase_later_references() {
    for (path, source) in [
        (
            "void-reference-chain.ts",
            "var x:void;var y:any;var z:void;y=x;x=y;x=z;",
        ),
        (
            "reference-chain.ts",
            concat!(
                "let source:string='';let first:string|number=0;let second:string|number=0;",
                "first=source;second=first;const text:string=second;",
            ),
        ),
        (
            "literal-self-chain.ts",
            concat!(
                "let value:string|number=0;value='ready';value=value;",
                "const text:string=value;",
            ),
        ),
        (
            "direct-call-chain.ts",
            concat!(
                "declare function fixed(value:string):string;let input:string='';",
                "let value:string|number=0;value=fixed(input);const text:string=value;",
            ),
        ),
    ] {
        let output = compile(path, source);
        assert_eq!(fingerprints(&output), [], "{path}");
        assert_eq!(output.exit_status, CompileExitStatus::Success, "{path}");
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{path}"
        );
    }

    let unsupported = concat!(
        "let value:string|number=0;value='ready';value=1+1;",
        "const text:string=value;",
    );
    let output = compile("unsupported-straight-line.ts", unsupported);
    assert_eq!(fingerprints(&output), []);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}
