use std::sync::Arc;

use tsz::service::LanguageService;
use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

fn compile(source: &str, strict: bool, no_implicit_any: Option<bool>) -> tsz::CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            target: "es2022".to_string(),
            strict,
            no_implicit_any,
            no_emit: true,
            ..CompilerOptions::default()
        },
    )
}

fn assert_complete(source: &str) {
    let output = compile(source, true, None);
    assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
    assert_eq!(
        output.semantic_completion,
        SemanticCompletion::Complete,
        "{source}"
    );
    assert_eq!(output.exit_status, CompileExitStatus::Success, "{source}");
}

fn assert_deferred(source: &str) {
    let output = compile(source, true, None);
    assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
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

fn diagnostic_codes(output: &tsz::CompileOutput) -> Vec<u32> {
    output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn service_options() -> CompilerOptions {
    CompilerOptions {
        target: "es2022".to_string(),
        strict: true,
        no_emit: true,
        ..CompilerOptions::default()
    }
}

fn open_service(files: &[(&str, &str)]) -> LanguageService {
    let mut service = LanguageService::new(service_options());
    for (path, source) in files {
        service.open(*path, Arc::<str>::from(*source));
    }
    service
}

#[test]
fn loose_unused_accesses_still_run_the_owned_query() {
    // Graduation: constructor indexing becomes Complete only after class value-side static shapes
    // are part of the ElementAccess query input; unused expressions must never bypass that query.
    for source in [
        "declare const uncertain:unknown;(uncertain)['renamed'];",
        "class Vessel{static cargo:string='x';}(Vessel)['cargo'];",
    ] {
        let output = compile(source, true, Some(false));
        assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{source}"
        );
    }

    let proven_loose_fallback = compile(
        "declare const shaped:{present:number};(shaped)['renamedMissing'];",
        false,
        Some(false),
    );
    assert_eq!(proven_loose_fallback.diagnostics, []);
    assert_eq!(
        proven_loose_fallback.semantic_completion,
        SemanticCompletion::Complete
    );
}

#[test]
fn arrays_accept_only_canonical_numeric_string_indices_before_loose_fallback() {
    for source in [
        "declare const cargo:string[];const mismatch:number=(cargo)['0'];",
        "declare const renamed:string[];const mismatch:number=((renamed)['-1']);",
    ] {
        let output = compile(source, true, None);
        assert_eq!(diagnostic_codes(&output), vec![2322], "{source}");
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source}"
        );
    }

    // Graduation: strict nonnumeric keys become diagnostics only with an owned TS7015 lookup reason.
    assert_deferred("declare const cargo:string[];const value=(cargo)['01'];");

    let loose = compile(
        "declare const cargo:string[];const value=(cargo)['01'];",
        false,
        Some(false),
    );
    assert_eq!(loose.diagnostics, []);
    assert_eq!(loose.semantic_completion, SemanticCompletion::Complete);

    assert_complete(concat!(
        "const direct:string='abc'['0'];",
        "declare const renamed:string;",
        "const wrapped:string=((renamed)['0']);",
    ));
    assert_deferred("declare const renamed:string;const value=(renamed)['01'];");

    let loose_string = compile(
        "declare const renamed:string;const value=(renamed)['missing'];",
        false,
        Some(false),
    );
    assert_eq!(loose_string.diagnostics, []);
    assert_eq!(
        loose_string.semantic_completion,
        SemanticCompletion::Complete
    );
}

#[test]
fn applicable_index_signatures_precede_an_any_index_fallback() {
    for no_implicit_any in [Some(true), Some(false)] {
        let source = concat!(
            "declare const table:{[renamed:string]:boolean};",
            "declare const key:any;",
            "const mismatch:string=(table)[(key)];",
        );
        let output = compile(source, true, no_implicit_any);
        assert_eq!(diagnostic_codes(&output), vec![2322]);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source}"
        );
    }

    assert_complete(concat!(
        "declare const table:{[renamed:string]:boolean};",
        "declare const key:any;",
        "const value:boolean=(table)[key];",
    ));

    for source in [
        "declare const values:number[];declare const key:any;const mismatch:string=values[key];",
        "declare const text:string;declare const key:any;const mismatch:number=text[key];",
        "declare const pair:[number,string];declare const key:any;const mismatch:boolean=pair[key];",
    ] {
        let output = compile(source, true, None);
        assert_eq!(diagnostic_codes(&output), vec![2322], "{source}");
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source}"
        );
    }
}

#[test]
fn optional_properties_and_never_array_writes_remain_local_deferred_queries() {
    // Graduation: optional properties require option-owned read/write projection, and evolving
    // arrays require an inference owner that can replace Array<never> after observed writes.
    for source in [
        concat!(
            "declare const parcel:{cargo?:string};",
            "const mismatch:number=(parcel)['cargo'];",
        ),
        concat!(
            "declare let wrapped:{value?:string};",
            "(wrapped)['value']=undefined;",
        ),
        "let evolving=[];(evolving)[0]=1;",
    ] {
        assert_deferred(source);
    }

    assert_complete(concat!(
        "declare let parcel:{cargo:string};",
        "const cargo:string=(parcel)['cargo'];",
        "((parcel)['cargo'])='renamed';",
    ));

    for source in [
        "declare let fixed:never[];(fixed)[0]=1;",
        "const renamed:never[]=[];((renamed)['-1'])='text';",
    ] {
        let output = compile(source, true, None);
        assert_eq!(diagnostic_codes(&output), vec![2322], "{source}");
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(
            output.diagnostics[0].message_text,
            if source.contains("'text'") {
                "Type 'string' is not assignable to type 'never'."
            } else {
                "Type 'number' is not assignable to type 'never'."
            }
        );
    }
}

#[test]
fn evolving_array_write_nonclaims_follow_the_declaration_value_across_files() {
    let producer = "let values=[];(values)[0]=1;";
    let consumer = "const mismatch:string=((values)[0]);";
    for (producer_path, consumer_path) in [
        ("a-producer.ts", "z-consumer.ts"),
        ("z-producer.ts", "a-consumer.ts"),
    ] {
        let service = open_service(&[(producer_path, producer), (consumer_path, consumer)]);
        for path in [producer_path, consumer_path] {
            let result = service.semantic_diagnostics(path);
            assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
            assert!(
                result.diagnostics.is_empty(),
                "{path}: {:#?}",
                result.diagnostics
            );
        }
    }
}

#[test]
fn tuple_indices_support_broad_reads_but_require_exact_signed_literals() {
    assert_complete(concat!(
        "declare const pair:[number,string];",
        "const direct:string=pair[1];",
        "const wrapped:string=((pair)['1']);",
    ));

    let broad = compile(
        concat!(
            "declare const pair:[number,string];",
            "declare const key:number;",
            "const mismatch:boolean=pair[(key)];",
        ),
        true,
        None,
    );
    assert_eq!(diagnostic_codes(&broad), vec![2322]);
    assert_eq!(broad.semantic_completion, SemanticCompletion::Complete);
    assert_complete("declare const pair:[number,string];const value:string=pair[+1];");

    // Graduation: signed and out-of-range indices need tuple-specific TS2493/TS2514 reasons
    // before they may publish a value or a diagnostic.
    let source = "declare const renamed:[number,string];const value=((renamed)[-1]);";
    assert_deferred(source);
}

#[test]
fn delete_access_retains_boolean_value_and_records_deferred_completion() {
    // Graduation: delete becomes Complete when optionality and TS2790 validation are owned by a
    // typed Delete demand; its orthogonal expression value remains boolean meanwhile.
    for source in [
        "declare const parcel:{cargo:number};const removed:boolean=delete parcel.cargo;",
        "declare const renamed:{value:number};const removed:boolean=delete (renamed)['value'];",
    ] {
        assert_deferred(source);
    }

    let with_independent_error = compile(
        concat!(
            "declare const parcel:{cargo:number};",
            "const removed:boolean=delete (parcel)['cargo'];",
            "const independent:number='text';",
        ),
        true,
        None,
    );
    assert_eq!(diagnostic_codes(&with_independent_error), vec![2322]);
    assert_eq!(
        with_independent_error.semantic_completion,
        SemanticCompletion::Deferred
    );

    for source in ["delete 1;", "let local=1;delete local;"] {
        assert_deferred(source);
    }

    for (source, code) in [
        ("declare const values:number[];delete values[];", 1011),
        ("delete Missing.foo;", 2304),
    ] {
        let output = compile(source, true, None);
        assert_eq!(diagnostic_codes(&output), vec![code], "{source}");
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }
}

#[test]
fn arithmetic_and_bitwise_results_require_proven_operand_kinds() {
    assert_complete(concat!(
        "declare const left:number;declare const renamed:number;",
        "const a:number=(left)+(renamed);const b:number=(left)-(renamed);",
        "const c:number=(left)*(renamed);const d:number=(left)/(renamed);",
        "const e:number=(left)%(renamed);const f:number=(left)&(renamed);",
        "const g:number=(left)|(renamed);const text:string=('value')+(left);",
    ));
    assert_complete(concat!(
        "declare const left:bigint;declare const renamed:bigint;",
        "const a:bigint=(left)+(renamed);const b:bigint=(left)-(renamed);",
        "const c:bigint=(left)*(renamed);const d:bigint=(left)/(renamed);",
        "const e:bigint=(left)%(renamed);const f:bigint=(left)&(renamed);",
        "const g:bigint=(left)|(renamed);",
    ));
    assert_complete(concat!(
        "declare const dynamic:any;declare const count:number;",
        "const numericAdd:string=(dynamic)+count;",
        "const stringAdd:string=(dynamic)+'renamed';",
    ));

    for operator in ["-", "*", "/", "%", "&", "|"] {
        for expression in [
            format!("(dynamic){operator}(renamed)"),
            format!("(renamed){operator}(dynamic)"),
        ] {
            let source = format!(
                "declare const dynamic:any;declare const renamed:number;const mismatch:string={expression};"
            );
            let output = compile(&source, true, None);
            assert_eq!(diagnostic_codes(&output), vec![2322], "{source}");
            assert_eq!(
                output.diagnostics[0].message_text,
                "Type 'number' is not assignable to type 'string'.",
                "{source}"
            );
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Complete,
                "{source}"
            );
        }
    }

    assert_complete(concat!(
        "declare const dynamic:any;declare const wide:bigint;",
        "const left:bigint=(dynamic)-wide;const right:bigint=wide|(dynamic);",
    ));

    for source in [
        "declare const dynamic:any;const mismatch:number=dynamic-1n;",
        "declare const dynamic:any;const mismatch:number=1n-dynamic;",
        "const mismatch:number=2n-1n;",
        "const mismatch:number='x'+1n;",
        "declare const bottom:never;const mismatch:number='x'+bottom;",
        "declare const bottom:never;const mismatch:string=bottom-1;",
        "declare const bottom:never;declare const dynamic:any;const mismatch:number=dynamic+bottom;",
        "declare const bottom:never;const mismatch:string=bottom|bottom;",
        "const one=1n;declare const dynamic:any;const mismatch:number=dynamic-one;",
        "let renamed=1n;declare const dynamic:any;const mismatch:number=renamed-dynamic;",
        "const one=1n;const mismatch:number='x'+one;",
    ] {
        let output = compile(source, true, None);
        assert_eq!(diagnostic_codes(&output), vec![2322], "{source}");
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source}"
        );
    }
    assert_complete("declare const dynamic:any;const value:string=dynamic+1n;");

    let source =
        "declare const text:string;const first:number=(text)&1;const second:number=(text)&1;";
    let output = compile(source, true, None);
    let text_starts = source
        .match_indices("(text)")
        .map(|(start, _)| start as u32)
        .collect::<Vec<_>>();
    assert_eq!(diagnostic_codes(&output), vec![2362, 2362]);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| {
                (
                    diagnostic.start,
                    diagnostic.length,
                    diagnostic.message_text.as_str(),
                )
            })
            .collect::<Vec<_>>(),
        text_starts
            .iter()
            .map(|start| (*start, "(text)".len() as u32, "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type."))
            .collect::<Vec<_>>()
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);

    for (source, code, needle, message) in [
        (
            "declare const count:number;declare const wide:bigint;const value:number=(count)+(wide);",
            2365,
            "(count)+(wide)",
            "Operator '+' cannot be applied to types 'number' and 'bigint'.",
        ),
        (
            "declare const flag:boolean;const value:number=((flag)-1);",
            2362,
            "(flag)",
            "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.",
        ),
    ] {
        let output = compile(source, true, None);
        assert_eq!(diagnostic_codes(&output), vec![code], "{source}");
        assert_eq!(
            output.diagnostics[0].start,
            source.find(needle).expect("operator diagnostic span") as u32
        );
        assert_eq!(output.diagnostics[0].length, needle.len() as u32);
        assert_eq!(output.diagnostics[0].message_text, message);
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }

    // Graduation: these string-concatenation pairs stay local until the checker owns TS2469
    // and object-to-primitive coercion.
    for source in [
        "declare const token:symbol;const value:string=('renamed')+(token);",
        "declare const payload:object;const value:string=(payload)+('renamed');",
    ] {
        assert_deferred(source);
    }
}

#[test]
fn boolean_bitwise_pairs_use_the_typescript_operator_diagnostic() {
    for (source, operator, suggestion, operator_start) in [
        (
            "const result:number=true /* decoy & */ & false;",
            "&",
            "&&",
            "const result:number=true /* decoy & */ ".len(),
        ),
        (
            "const result:number=true | /* decoy | */ false;",
            "|",
            "||",
            "const result:number=true ".len(),
        ),
    ] {
        let output = compile(source, true, None);
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
                2447,
                operator_start as u32,
                1,
                format!(
                    "The '{operator}' operator is not allowed for boolean types. Consider using '{suggestion}' instead."
                )
                .as_str(),
            )],
            "{source}"
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }

    for (source, code, start, message) in [
        (
            "const result=true & 1;",
            2362,
            "const result=".len(),
            "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.",
        ),
        (
            "const result=1 | false;",
            2363,
            "const result=1 | ".len(),
            "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.",
        ),
    ] {
        let output = compile(source, true, None);
        assert_eq!(diagnostic_codes(&output), vec![code], "{source}");
        assert_eq!(output.diagnostics[0].start, start as u32, "{source}");
        assert_eq!(output.diagnostics[0].message_text, message, "{source}");
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }

    let output = compile("const mismatch:string=true&false;", true, None);
    assert_eq!(diagnostic_codes(&output), vec![2322, 2447]);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn bigint_operand_categories_survive_cross_file_declaration_wrappers() {
    let producer = "const one=1n;";
    let consumer = "declare const dynamic:any;const mismatch:number=dynamic-one;";
    for (producer_path, consumer_path) in [
        ("a-producer.ts", "z-consumer.ts"),
        ("z-producer.ts", "a-consumer.ts"),
    ] {
        let service = open_service(&[(producer_path, producer), (consumer_path, consumer)]);
        let result = service.semantic_diagnostics(consumer_path);
        assert_eq!(result.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(
            result
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            vec![2322]
        );
    }
}

#[test]
fn binary_recovery_propagates_an_existing_complete_error_sentinel() {
    let output = compile(
        "declare const values:number[];const recovered=1+values[];",
        true,
        None,
    );
    assert_eq!(diagnostic_codes(&output), vec![1011]);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        output.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );

    for (source, owned_code) in [
        ("const mismatch:number='x'+Missing;", 2304),
        (
            "declare const values:number[];const mismatch:number='x'+values[];",
            1011,
        ),
    ] {
        let output = compile(source, true, None);
        assert_eq!(
            diagnostic_codes(&output),
            vec![2322, owned_code],
            "{source}"
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }
}

#[test]
fn invalid_assignment_targets_defer_before_relation_diagnostics() {
    // Graduation: replace this local nonclaim when assignment-target validation owns TS2364.
    for source in ["1=2;", "(('renamed'))=(2);"] {
        assert_deferred(source);
    }

    assert_complete(concat!(
        "let count=0;(count)=1;",
        "declare let parcel:{cargo:number};(parcel).cargo=2;",
        "let values:number[]=[0];((values)[0])=3;",
    ));
}

#[test]
fn repeated_declaration_value_queries_preserve_incomplete_provenance_in_every_root_order() {
    let producer = concat!(
        "declare const box:{known:string};",
        "const seed='x'+box['missing'];",
    );
    let consumer = "const mismatch:number=seed;";
    let safe = "const independent:string=1;";

    for (producer_path, consumer_path) in [
        ("a-producer.ts", "z-consumer.ts"),
        ("z-producer.ts", "a-consumer.ts"),
    ] {
        let service = open_service(&[
            (producer_path, producer),
            (consumer_path, consumer),
            ("m-safe.ts", safe),
        ]);
        for _ in 0..2 {
            let consumer_result = service.semantic_diagnostics(consumer_path);
            assert_eq!(
                consumer_result.semantic_completion,
                SemanticCompletion::Deferred,
                "{producer_path} -> {consumer_path}"
            );
            assert_eq!(
                consumer_result
                    .diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic.code)
                    .collect::<Vec<_>>(),
                vec![2322]
            );
            assert_eq!(
                consumer_result.diagnostics[0].message_text,
                "Type 'string' is not assignable to type 'number'."
            );
        }
        let producer_result = service.semantic_diagnostics(producer_path);
        assert_eq!(
            producer_result.semantic_completion,
            SemanticCompletion::Deferred
        );
        assert!(producer_result.diagnostics.is_empty());

        let safe_result = service.semantic_diagnostics("m-safe.ts");
        assert_eq!(
            safe_result.semantic_completion,
            SemanticCompletion::Complete
        );
        assert_eq!(
            safe_result
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            vec![2322]
        );
    }
}

#[test]
fn wrapped_declaration_values_cannot_publish_completion_erasing_force_entries() {
    let producer = concat!(
        "declare const box:{known:string};",
        "const seed='x'+box['missing'];",
    );
    let wrapper = "const wrapped=((seed))[0];";
    let consumer = "const mismatch:number=wrapped;";

    for files in [
        [
            ("a-producer.ts", producer),
            ("m-wrapper.ts", wrapper),
            ("z-consumer.ts", consumer),
        ],
        [
            ("z-producer.ts", producer),
            ("m-wrapper.ts", wrapper),
            ("a-consumer.ts", consumer),
        ],
    ] {
        let consumer_path = files[2].0;
        let service = open_service(&files);
        for _ in 0..2 {
            let result = service.semantic_diagnostics(consumer_path);
            assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
            assert_eq!(
                result
                    .diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic.code)
                    .collect::<Vec<_>>(),
                vec![2322]
            );
        }
    }
}

#[test]
fn authored_annotations_are_complete_value_boundaries_for_consumers() {
    let producer = concat!(
        "declare const box:{known:string};",
        "const seed:string='x'+box['missing'];",
    );
    let consumer = "const mismatch:number=seed;";
    let service = open_service(&[("z-producer.ts", producer), ("a-consumer.ts", consumer)]);

    let producer_result = service.semantic_diagnostics("z-producer.ts");
    assert_eq!(
        producer_result.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert!(producer_result.diagnostics.is_empty());

    let consumer_result = service.semantic_diagnostics("a-consumer.ts");
    assert_eq!(
        consumer_result.semantic_completion,
        SemanticCompletion::Complete
    );
    assert_eq!(
        consumer_result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        vec![2322]
    );
}
