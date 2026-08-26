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
fn explicit_arguments_do_not_materialize_structural_defaults() {
    let cases = [
        "type Wrapper<Value=Record<string,unknown>>=Value;\
         type Applied<Item>=Wrapper<Item>;\
         declare const value:Applied<number>;const numberValue:number=value;",
        "type Container<Payload={fallback:string}>={payload:Payload};\
         type Nested<Model>={inner:Container<Model>};\
         declare const nested:Nested<boolean>;\
         const flag:boolean=nested.inner.payload;",
        "interface Box<Contents={fallback:string}>{contents:Contents}\
         declare const box:Box<number>;const contents:number=box.contents;",
        "declare class Crate<Entry={fallback:string}>{entry:Entry}\
         declare const crate:Crate<boolean>;const entry:boolean=crate.entry;",
    ];

    for source in cases {
        assert_complete(&compile(source));
    }
}

#[test]
fn explicit_constrained_references_complete_only_after_proving_the_arguments() {
    let complete = [
        "type EventKey=string|symbol;interface Emitter<Events extends Record<EventKey,unknown>>{all:Map<keyof Events,unknown>}declare const emitter:Emitter<{ready:number}>;",
        "type EventKey=string|symbol;interface Channel<Model extends Record<EventKey,unknown>>{all:Map<keyof Model,unknown>}declare function identity<Events extends Record<EventKey,unknown>>(value:Channel<Events>):Channel<Events>;",
        "type EventKey=string|symbol;interface Box<Value extends Record<EventKey,unknown>>{value:Value}declare function wrap<Model extends {renamed:number}>(value:Box<Model>):Box<Model>;",
    ];
    for source in complete {
        assert_complete(&compile(source));
    }

    for source in [
        "type EventKey=string|symbol;interface Emitter<Events extends Record<EventKey,unknown>>{all:Map<keyof Events,unknown>}declare const emitter:Emitter<number>;",
        "type EventKey=string|symbol;interface Emitter<Events extends Record<EventKey,unknown>>{all:Map<keyof Events,unknown>}declare function identity<Events>(value:Emitter<Events>):Emitter<Events>;",
        "interface Box<Value extends Record<string,boolean>>{value:Value}declare const box:Box<{ready:number}>;",
        "interface Text<Value extends string>{value:Value}declare const text:Text<number>;",
    ] {
        let output = compile(source);
        assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{source}: {:?}",
            output.stats
        );
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    }
}

#[test]
fn constrained_reference_applications_are_cold_warm_and_root_order_stable() {
    let compiler = Compiler::new();
    let declaration = SourceInput::new(
        "channel.ts",
        Arc::<str>::from(
            "type Key=string|symbol;interface Channel<Model extends Record<Key,unknown>>{all:Map<keyof Model,unknown>}",
        ),
    );
    let consumer = SourceInput::new(
        "consumer.ts",
        Arc::<str>::from("declare const channel:Channel<{renamed:number}>;"),
    );
    let run = |files| compiler.compile(files, &options());
    let cold = run(vec![declaration.clone(), consumer.clone()]);
    let warm = run(vec![declaration.clone(), consumer.clone()]);
    let reversed = run(vec![consumer, declaration]);
    for output in [&cold, &warm, &reversed] {
        assert_complete(output);
    }
    assert_eq!(cold.stats.types, warm.stats.types);
    assert_eq!(cold.stats.types, reversed.stats.types);
}

#[test]
fn declaration_owned_symbolic_keyof_remains_safe_inside_object_shapes() {
    // When `keyof` keeps a declaration-owned type parameter symbolic, its
    // result is still wholly within PropertyKey. Object-shape construction can
    // therefore retain the symbolic child without materializing the operand.
    for source in [
        "interface Emitter<Events>{all:Map<keyof Events,unknown>}declare function identity<Events>(emitter:Emitter<Events>):Emitter<Events>;",
        "interface Registry<Model>{all:Map<(keyof Model),unknown>}declare function preserve<Model>(registry:Registry<Model>):Registry<Model>;",
        "type Registry<Subject>=Map<keyof Subject,unknown>;interface Holder<Subject>{all:Registry<Subject>}declare function preserve<Subject>(holder:Holder<Subject>):Holder<Subject>;",
        "interface Registry<State>{all:Map<keyof State,unknown>}interface Wrapped<State>{nested:{registry:Registry<State>}}declare function preserve<State>(value:Wrapped<State>):Wrapped<State>;",
    ] {
        let output = compile(source);
        assert!(
            output.diagnostics.is_empty(),
            "{source}: {:?}",
            output.diagnostics
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source}: {:?}",
            output.stats
        );
        assert_eq!(output.exit_status, CompileExitStatus::Success, "{source}");
    }
}

#[test]
fn canonical_record_key_failures_use_authored_argument_spans_and_relation_reasons() {
    let cases: [(&str, &str, &str, &[&str]); 8] = [
        ("type Invalid<Key>=Record<Key,unknown>;", "Key", "Key", &[]),
        (
            "type Invalid=Record<boolean,unknown>;",
            "boolean",
            "boolean",
            &[],
        ),
        ("type Invalid=Record<{},unknown>;", "{}", "{}", &[]),
        (
            "type Invalid=Record<unknown,any>;",
            "unknown",
            "unknown",
            &[],
        ),
        (
            "type Invalid=Record<string|boolean,unknown>;",
            "string|boolean",
            "string | boolean",
            &["Type 'boolean' is not assignable to type 'string | number | symbol'."],
        ),
        (
            "type RenamedKey=boolean;type Invalid=Record<RenamedKey,unknown>;",
            "RenamedKey",
            "boolean",
            &[],
        ),
        ("type Nested={entry:Record<{},unknown>};", "{}", "{}", &[]),
        (
            "type Invalid<Key extends boolean>=Record<Key,unknown>;",
            "Key",
            "Key",
            &["Type 'boolean' is not assignable to type 'string | number | symbol'."],
        ),
    ];
    for (source, needle, source_name, related) in cases {
        let output = compile(source);
        let [diagnostic] = output.diagnostics.as_slice() else {
            panic!("{source}: {:#?}", output.diagnostics);
        };
        assert_eq!(diagnostic.file, "case.ts", "{source}");
        assert_eq!(diagnostic.code, 2344, "{source}");
        assert_eq!(
            diagnostic.start,
            source.rfind(needle).unwrap() as u32,
            "{source}"
        );
        assert_eq!(diagnostic.length, needle.len() as u32, "{source}");
        assert_eq!(
            diagnostic.message_text,
            format!(
                "Type '{source_name}' does not satisfy the constraint 'string | number | symbol'."
            ),
            "{source}"
        );
        assert_eq!(
            diagnostic
                .related_information
                .iter()
                .map(|information| (
                    information.message_text.as_str(),
                    information.code,
                    information.depth,
                ))
                .collect::<Vec<_>>(),
            related
                .iter()
                .enumerate()
                .map(|(depth, message)| (*message, 2344, depth as u32 + 1))
                .collect::<Vec<_>>(),
            "{source}"
        );
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
fn record_key_constraint_controls_preserve_valid_nocheck_and_unowned_boundaries() {
    for source in [
        "type Valid<Key extends string|symbol>=Record<Key,unknown>;",
        "type Valid=Record<string|number|symbol,unknown>;",
        "type Valid=Record<PropertyKey,unknown>;",
    ] {
        assert_complete(&compile(source));
    }

    let no_check = compile("// @ts-nocheck\ntype Invalid<Key>=Record<Key,unknown>;");
    assert_complete(&no_check);

    let deferred = compile(concat!(
        "type DeferredExtract=Record<Extract<string,'ready'>,unknown>;",
        "type DeferredConditional<Key>=Record<Key extends string?Key:never,unknown>;",
    ));
    assert_eq!(deferred.diagnostics, []);
    assert_eq!(deferred.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(deferred.exit_status, CompileExitStatus::SemanticIncomplete);
}

#[test]
fn record_key_constraint_failures_are_cold_warm_and_root_order_stable() {
    let compiler = Compiler::new();
    let declarations = SourceInput::new(
        "models.ts",
        Arc::<str>::from("type RenamedInvalidKey=boolean;"),
    );
    let consumer = SourceInput::new(
        "consumer.ts",
        Arc::<str>::from("type Invalid=Record<RenamedInvalidKey,unknown>;"),
    );
    let run = |files| compiler.compile(files, &options());
    let cold = run(vec![declarations.clone(), consumer.clone()]);
    let warm = run(vec![declarations.clone(), consumer.clone()]);
    let reversed = run(vec![consumer, declarations]);
    for output in [&cold, &warm, &reversed] {
        let [diagnostic] = output.diagnostics.as_slice() else {
            panic!("{:#?}", output.diagnostics);
        };
        assert_eq!(diagnostic.file, "consumer.ts");
        assert_eq!(diagnostic.start, 20);
        assert_eq!(diagnostic.length, 17);
        assert_eq!(diagnostic.code, 2344);
        assert_eq!(
            diagnostic.message_text,
            "Type 'boolean' does not satisfy the constraint 'string | number | symbol'."
        );
        assert!(diagnostic.related_information.is_empty());
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(
            output.exit_status,
            CompileExitStatus::DiagnosticsPresentOutputsSkipped
        );
    }
    assert_eq!(cold.stats.types, warm.stats.types);
    assert_eq!(cold.stats.types, reversed.stats.types);
}

#[test]
fn symbolic_keyof_object_shapes_are_cold_warm_and_root_order_stable() {
    let compiler = Compiler::new();
    let declarations = SourceInput::new(
        "models.ts",
        Arc::<str>::from(
            "type Registry<Model>=Map<keyof Model,unknown>;interface Wrapped<Model>{nested:{registry:Registry<Model>}}",
        ),
    );
    let consumer = SourceInput::new(
        "consumer.ts",
        Arc::<str>::from("declare function preserve<Model>(value:Wrapped<Model>):Wrapped<Model>;"),
    );
    let run = |files| compiler.compile(files, &options());
    let cold = run(vec![declarations.clone(), consumer.clone()]);
    let warm = run(vec![declarations.clone(), consumer.clone()]);
    let reversed = run(vec![consumer, declarations]);
    for output in [&cold, &warm, &reversed] {
        assert_complete(output);
    }
    assert_eq!(cold.stats.types, warm.stats.types);
    assert_eq!(cold.stats.types, reversed.stats.types);
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
        "type Wrapper<Value=Record<string,unknown>>=Value;declare const value:Wrapper;value;",
        "type Pair<First,Second=Record<string,unknown>>={first:First;second:Second};\
         declare const pair:Pair<number>;pair.second;",
        "type Bound<Value extends string=string>=Value;\
         declare const value:Bound<number>;value;",
        "type Projection<Value=Record<string,unknown>>=Value[keyof Value];\
         declare const value:Projection<string>;value;",
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
