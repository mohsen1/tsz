use std::sync::Arc;

use tsz::service::LanguageService;
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

fn compile_without_implicit_any(source: &str) -> tsz::CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            no_emit: true,
            strict: false,
            no_implicit_any: Some(false),
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
fn rest_parameters_expand_for_zero_one_and_many_call_arguments() {
    let output = compile(
        r#"
        function takeAll(...values: number[]): void {}
        takeAll();
        takeAll(1);
        takeAll(1, 2, 3);
        takeAll("wrong");
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
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "pinned TS7 seed did not reach a definitive semantic result for {source:?}"
        );
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
    assert_eq!(output.semantic_completion, SemanticCompletion::Cycle);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
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
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(output.emitted_files.len(), 1);
    assert_eq!(
        output.emitted_files[0].text,
        "\"use strict\";\nconst count = \"wrong\";\n"
    );
}

#[test]
fn language_service_keeps_semantic_completion_separate_from_diagnostics() {
    let mut service = LanguageService::new(CompilerOptions {
        no_emit: true,
        strict: true,
        ..CompilerOptions::default()
    });
    service.open(
        "case.ts",
        Arc::<str>::from("const text:string=''; const size:number=text.length;"),
    );

    assert!(service.syntactic_diagnostics("case.ts").is_empty());
    let semantic = service.semantic_diagnostics("case.ts");
    assert!(semantic.diagnostics.is_empty());
    assert_eq!(semantic.semantic_completion, SemanticCompletion::Deferred);
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
    let forward = vec![first.clone(), second.clone()];
    let reverse = vec![second, first];
    let expected = fingerprint(&Compiler::new().compile(forward.clone(), &options));
    assert_eq!(
        expected
            .iter()
            .map(|diagnostic| diagnostic.0.as_str())
            .collect::<Vec<_>>(),
        ["a.ts", "b.ts"]
    );
    for iteration in 0..10 {
        for inputs in [forward.clone(), reverse.clone()] {
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

#[test]
fn quick_info_infers_widened_object_literals_at_top_level_and_in_nested_scopes() {
    for (name, property) in [("item", "count"), ("renamed", "total")] {
        let source = format!(
            "const {name} = {{ {property}: 1 }};\nfunction scope(): void {{ const nested = {{ {name}: {{ label: \"ok\" }}, ready: true }}; }}"
        );
        let mut service = LanguageService::new(CompilerOptions::default());
        service.open("case.ts", Arc::<str>::from(source.clone()));

        let declaration = source.find(name).unwrap() as u32;
        assert_eq!(
            service
                .quick_info("case.ts", declaration + 1)
                .unwrap()
                .display,
            format!("const {name}: {{ {property}: number; }}")
        );

        let nested = source.find("nested").unwrap() as u32;
        assert_eq!(
            service.quick_info("case.ts", nested + 1).unwrap().display,
            format!("const nested: {{ {name}: {{ label: string; }}; ready: boolean; }}")
        );
    }
}

#[test]
fn fresh_const_aliases_widen_at_mutable_object_properties_but_annotations_do_not() {
    // Structural rule: when an inferred const literal flows through a mutable
    // object property, TypeScript 7 widens its fresh provenance. A literal
    // annotation is regular provenance and remains literal through the same
    // shorthand/nested shapes. `TypeStore` owns that distinction.
    for (text_name, number_name, boolean_name) in
        [("text", "count", "ready"), ("label", "total", "enabled")]
    {
        let source = format!(
            r#"
            const {text_name} = "seed";
            const {number_name} = 1;
            const {boolean_name} = true;
            const exactText: "seed" = "seed";
            const exactNumber: 1 = 1;
            const exactBoolean: true = true;
            const mutable = {{
                {text_name},
                {number_name},
                {boolean_name},
                nested: {{ {text_name} }},
                renamed: {text_name}
            }};
            const annotated = {{ exactText, exactNumber, exactBoolean }};
            const badText: "seed" = mutable.{text_name};
            const badNumber: 1 = mutable.{number_name};
            const badBoolean: true = mutable.{boolean_name};
            const badNested: "seed" = mutable.nested.{text_name};
            const goodText: "seed" = annotated.exactText;
            const goodNumber: 1 = annotated.exactNumber;
            const goodBoolean: true = annotated.exactBoolean;
            "#
        );
        let output = compile(&source);
        assert_eq!(codes(&output), vec![2322, 2322, 2322, 2322]);
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message_text.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Type 'string' is not assignable to type '\"seed\"'.",
                "Type 'number' is not assignable to type '1'.",
                "Type 'boolean' is not assignable to type 'true'.",
                "Type 'string' is not assignable to type '\"seed\"'.",
            ],
            "wrong provenance result for renamed binders {text_name}/{number_name}/{boolean_name}"
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }
}

#[test]
fn fresh_array_elements_widen_through_the_same_provenance_gateway() {
    for (fresh_name, regular_name) in [("seed", "fixed"), ("label", "pinned")] {
        let source = format!(
            r#"
            const {fresh_name} = "start";
            const {regular_name}: "start" = "start";
            const direct = ["a", "b"];
            const fromFresh = [{fresh_name}];
            const fromRegular = [{regular_name}];
            const narrowDirect: ("a" | "b")[] = direct;
            const narrowFresh: "start"[] = fromFresh;
            const exactRegular: "start"[] = fromRegular;
            "#
        );
        let output = compile(&source);
        assert_eq!(codes(&output), vec![2322, 2322], "{:?}", output.diagnostics);
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }
}

#[test]
fn quick_info_renders_parsed_type_shapes_without_unknown_placeholders() {
    let cases = [
        (
            "ObjectShape",
            "type ObjectShape = { readonly count?: number; label: string };",
            "type ObjectShape = { readonly count?: number; label: string; }",
        ),
        (
            "Callback",
            "type Callback = (value: string, ...indexes: number[]) => boolean;",
            "type Callback = (value: string, ...indexes: number[]) => boolean",
        ),
        (
            "Builder",
            "type Builder = abstract new (value: number) => Box;",
            "type Builder = abstract new (value: number) => Box",
        ),
        (
            "Keys",
            "type Keys = keyof ObjectShape;",
            "type Keys = keyof ObjectShape",
        ),
        (
            "Choice",
            "type Choice = string extends number ? true : false;",
            "type Choice = string extends number ? true : false",
        ),
        (
            "Projection",
            "type Projection = { -readonly [P in keyof ObjectShape]-?: ObjectShape[P] };",
            "type Projection = { -readonly [P in keyof ObjectShape]-?: ObjectShape[P]; }",
        ),
        (
            "Indexed",
            "type Indexed = ObjectShape[\"count\"];",
            "type Indexed = ObjectShape[\"count\"]",
        ),
        (
            "Grouped",
            "type Grouped = (string | number);",
            "type Grouped = (string | number)",
        ),
    ];

    for (name, source, expected) in cases {
        let mut service = LanguageService::new(CompilerOptions::default());
        service.open("case.ts", Arc::<str>::from(source));
        let offset = source.find(name).unwrap() as u32;
        let info = service.quick_info("case.ts", offset + 1).unwrap();
        assert_eq!(info.display, expected, "wrong display for {name}");
        assert!(!info.display.ends_with("unknown"));
    }
}

#[test]
fn quick_info_returns_none_when_initializer_or_type_display_needs_missing_semantics() {
    let source = concat!(
        "const fromCall = createValue();\n",
        "const list = [1, 2];\n",
        "const explicit: unknown = fromCall;\n",
        "type Broken = ;\n",
    );
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open("case.ts", Arc::<str>::from(source));

    for name in ["fromCall", "list", "Broken"] {
        let offset = source.find(name).unwrap() as u32;
        assert!(
            service.quick_info("case.ts", offset + 1).is_none(),
            "unsupported {name} received confident quickinfo"
        );
    }

    let explicit = source.find("explicit").unwrap() as u32;
    assert_eq!(
        service.quick_info("case.ts", explicit + 1).unwrap().display,
        "const explicit: unknown"
    );
}

#[test]
fn navigation_uses_bound_identity_across_files_and_shadowed_scopes() {
    for name in ["shared", "renamedValue"] {
        let first = format!(
            "const {name} = 1;\nfunction wrap({name}: number) {{ return {name}; }}\n{name};\n"
        );
        let second = format!("{name};\n");
        let declaration = first.find(name).unwrap() as u32;
        let parameter = first[declaration as usize + name.len()..]
            .find(name)
            .map(|offset| offset + declaration as usize + name.len())
            .unwrap() as u32;

        let mut service = LanguageService::new(CompilerOptions::default());
        service.open("a.ts", Arc::<str>::from(first));
        service.open("b.ts", Arc::<str>::from(second));

        let global = service.references("a.ts", declaration + 1);
        assert_eq!(global.len(), 1, "missing global symbol for {name}");
        assert_eq!(global[0].references.len(), 3, "shadow leaked for {name}");
        assert!(
            global[0]
                .references
                .iter()
                .any(|reference| reference.file_name == "b.ts")
        );

        let local = service.references("a.ts", parameter + 1);
        assert_eq!(local.len(), 1, "missing parameter symbol for {name}");
        assert_eq!(local[0].references.len(), 2);
        assert!(
            local[0]
                .references
                .iter()
                .all(|reference| reference.file_name == "a.ts")
        );

        let definition = service.definition_and_bound_span("b.ts", 1).unwrap();
        assert_eq!(definition.text_span.start, 0);
        assert_eq!(definition.definitions[0].file_name, "a.ts");
        assert_eq!(definition.definitions[0].text_span.start, declaration);

        let highlights =
            service.document_highlights("a.ts", declaration + 1, &["b.ts".to_string()]);
        assert_eq!(highlights.len(), 1);
        assert_eq!(highlights[0].file_name, "b.ts");
        assert_eq!(highlights[0].highlight_spans.len(), 1);

        let rename = service.rename("a.ts", declaration + 1);
        assert!(rename.info.can_rename);
        assert_eq!(rename.info.display_name.as_deref(), Some(name));
        assert_eq!(rename.info.trigger_span.unwrap().start, declaration);
        assert_eq!(rename.info.trigger_span.unwrap().length, name.len() as u32);
        assert_eq!(rename.locations.len(), 3);
        assert!(!service.rename("a.ts", 0).info.can_rename);
    }
}

#[test]
fn definition_metadata_comes_from_bound_scope_and_declaration_context() {
    let source = "function wrap() { var local; local = 1; }";
    let reference = source.rfind("local").unwrap() as u32;
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open("local.ts", Arc::<str>::from(source));

    let local = service
        .definition_and_bound_span("local.ts", reference + 1)
        .unwrap();
    let local = &local.definitions[0];
    assert_eq!(local.kind, "local var");
    assert_eq!(local.name, "local");
    assert!(local.is_local);
    assert!(!local.is_ambient);
    assert!(!local.unverified);
    assert!(!local.failed_alias_resolution);

    let ambient_source = "interface Ambient {}\nlet item: Ambient;";
    let ambient_reference = ambient_source.rfind("Ambient").unwrap() as u32;
    let mut ambient_service = LanguageService::new(CompilerOptions::default());
    ambient_service.open("ambient.d.ts", Arc::<str>::from(ambient_source));
    let ambient = ambient_service
        .definition_and_bound_span("ambient.d.ts", ambient_reference + 1)
        .unwrap();
    assert!(ambient.definitions[0].is_ambient);
    assert!(!ambient.definitions[0].is_local);
}

#[test]
fn navigation_keeps_type_and_value_meanings_separate() {
    let source = "type Envelope = string; const Envelope = 1; const item: Envelope = '';";
    let type_reference = source.rfind("Envelope").unwrap() as u32;
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open("case.ts", Arc::<str>::from(source));

    let definition = service
        .definition_and_bound_span("case.ts", type_reference + 1)
        .unwrap();
    assert_eq!(definition.definitions.len(), 1);
    assert_eq!(definition.definitions[0].kind, "type");
    assert_eq!(definition.definitions[0].text_span.start, 5);

    let references = service.references("case.ts", type_reference + 1);
    assert_eq!(references[0].references.len(), 2);
    assert!(
        references[0]
            .references
            .iter()
            .all(|reference| reference.text_span.start != 30)
    );
}

#[test]
fn modules_classes_and_deferred_type_syntax_share_the_fresh_pipeline() {
    let output = compile(
        r#"
        import { token as imported } from "./dep";
        type Dict<K extends keyof any, T> = { [P in K]?: T };
        interface Config {
            readonly id: number;
            name: string;
            enabled: boolean;
            options?: Dict<string, unknown>;
        }
        export class Service implements Config {
            readonly id: number = 1;
            name: string;
            enabled: boolean = true;
            private items: string[] = [];
            constructor(name: string) { this.name = name; }
            getItems(): readonly string[] { return this.items; }
            static create(name: string): Service { return new Service(name); }
        }
        const copy = imported;
        "#,
    );
    assert!(
        output.diagnostics.is_empty(),
        "valid module/class syntax cascaded into diagnostics: {:?}",
        output.diagnostics
    );
}

#[test]
fn class_overload_groups_report_exact_missing_implementation_diagnostics() {
    let source = "class Example {\n  constructor();\n  method(): void;\n}\n";
    let output = compile(source);
    assert_eq!(codes(&output), vec![2390, 2391]);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        output.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );
    assert_eq!(
        (
            output.diagnostics[0].start,
            output.diagnostics[0].length,
            output.diagnostics[0].message_text.as_str(),
        ),
        (
            source.find("constructor").unwrap() as u32,
            "constructor".len() as u32,
            "Constructor implementation is missing.",
        )
    );
    assert_eq!(
        (
            output.diagnostics[1].start,
            output.diagnostics[1].length,
            output.diagnostics[1].message_text.as_str(),
        ),
        (
            source.find("method").unwrap() as u32,
            "method".len() as u32,
            "Function implementation is missing or not immediately following the declaration.",
        )
    );
}

#[test]
fn class_overload_implementation_matching_is_structural() {
    for method in ["convert", "renamedMethod"] {
        let valid = format!(
            "class Example {{\n  constructor(value: string);\n  constructor() {{}}\n  {method}(value: string): void;\n  {method}() {{}}\n}}"
        );
        let output = compile(&valid);
        assert!(
            output.diagnostics.is_empty(),
            "valid overload group for {method} failed: {:?}",
            output.diagnostics
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(output.exit_status, CompileExitStatus::Success);

        let wrong_implementation =
            format!("class Example {{\n  {method}(): void;\n  different() {{}}\n}}");
        let output = compile(&wrong_implementation);
        let [diagnostic] = output.diagnostics.as_slice() else {
            panic!(
                "expected the renamed implementation diagnostic, got {:?}",
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
            (
                2389,
                wrong_implementation.find("different").unwrap() as u32,
                "different".len() as u32,
                format!("Function implementation name must be '{method}'.").as_str(),
            )
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(
            output.exit_status,
            CompileExitStatus::DiagnosticsPresentOutputsSkipped
        );
    }
}

#[test]
fn declaration_only_overload_hosts_complete_until_value_use_demands_resolution() {
    // TypeScript 7 accepts both declaration groups without selecting an
    // overload. These mirror functionOverloads6 and functionOverloads10 while
    // renaming the authored symbols.
    for source in [
        "class Renamed { static choose(); static choose(value:string); static choose(value?:any){} }",
        "function select(value:string, position:number); function select(value:string); function select(value:any){}",
    ] {
        let output = compile_without_implicit_any(source);
        assert!(
            output.diagnostics.is_empty(),
            "declaration-only overload failed for {source}: {:?}",
            output.diagnostics
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(output.exit_status, CompileExitStatus::Success);
    }

    for source in [
        "function select(value:string):string; function select(value:any):any{return value} select('ready');",
        "function choose(value:string):string; function choose(value:any):any{return value} const alias=choose; alias('ready');",
    ] {
        let output = compile(source);
        assert!(
            output.diagnostics.is_empty(),
            "overload demand invented a diagnostic for {source}: {:?}",
            output.diagnostics
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    }

    let separated = compile(
        "function pending(value:string):string; const boundary=1; function pending(value:any):any{return value}",
    );
    assert_eq!(codes(&separated), vec![2391]);
    assert_eq!(
        separated.diagnostics[0].message_text,
        "Function implementation is missing or not immediately following the declaration."
    );
    assert_eq!(
        (
            separated.diagnostics[0].start,
            separated.diagnostics[0].length
        ),
        ("function ".len() as u32, "pending".len() as u32)
    );
    assert_eq!(separated.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        separated.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );

    let recovered = compile(
        "function renamed(value:number):number; const numeric=renamed(1); const rejected:string=numeric;",
    );
    assert_eq!(codes(&recovered), vec![2391, 2322]);
    assert_eq!(recovered.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        recovered.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );

    let ambient = Compiler::new().compile(
        vec![SourceInput::new(
            "ambient.d.ts",
            Arc::<str>::from("declare function ambient(value:string):string;"),
        )],
        &CompilerOptions {
            no_emit: true,
            strict: true,
            ..CompilerOptions::default()
        },
    );
    assert!(ambient.diagnostics.is_empty(), "{:?}", ambient.diagnostics);
    assert_eq!(ambient.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(ambient.exit_status, CompileExitStatus::Success);
}

#[test]
fn function_overload_owner_matrix_is_monotonic_and_fail_closed() {
    let single_source = "function bodyless(); const text:string=bodyless();";
    let single = compile(single_source);
    assert_eq!(codes(&single), vec![2391, 7010]);
    for diagnostic in &single.diagnostics {
        assert_eq!(
            (diagnostic.start, diagnostic.length),
            (
                single_source.find("bodyless").unwrap() as u32,
                "bodyless".len() as u32,
            )
        );
    }
    assert_eq!(
        single.diagnostics[1].message_text,
        "'bodyless', which lacks return-type annotation, implicitly has an 'any' return type."
    );
    assert_eq!(single.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        single.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );

    let unannotated_pair = compile("function pair(); function pair();");
    assert_eq!(codes(&unannotated_pair), vec![7010, 2391, 7010]);
    assert_eq!(
        unannotated_pair.semantic_completion,
        SemanticCompletion::Complete
    );

    let annotated_pair = compile("function pair():void; function pair():void;");
    assert_eq!(codes(&annotated_pair), vec![2391]);
    assert_eq!(
        annotated_pair.diagnostics[0].start,
        "function pair():void; function ".len() as u32
    );
    assert_eq!(
        annotated_pair.semantic_completion,
        SemanticCompletion::Complete
    );

    let renamed_source = "function pending(value:number):number; function different(){return 1}";
    let renamed = compile(renamed_source);
    assert_eq!(codes(&renamed), vec![2389]);
    assert_eq!(
        (
            renamed.diagnostics[0].start,
            renamed.diagnostics[0].length,
            renamed.diagnostics[0].message_text.as_str(),
        ),
        (
            renamed_source.find("different").unwrap() as u32,
            "different".len() as u32,
            "Function implementation name must be 'pending'.",
        )
    );
    assert_eq!(renamed.semantic_completion, SemanticCompletion::Complete);

    let ambient_mismatch =
        compile("function ambientMismatch():void; declare function ambientMismatch():void;");
    assert_eq!(codes(&ambient_mismatch), vec![2384]);
    assert_eq!(
        ambient_mismatch.diagnostics[0].message_text,
        "Overload signatures must all be ambient or non-ambient."
    );
    assert_eq!(
        ambient_mismatch.semantic_completion,
        SemanticCompletion::Complete
    );

    let export_mismatch = compile("export function exposed():void; function exposed():void {}");
    assert_eq!(codes(&export_mismatch), vec![2383]);
    assert_eq!(
        export_mismatch.diagnostics[0].message_text,
        "Overload signatures must all be exported or non-exported."
    );
    assert_eq!(
        export_mismatch.semantic_completion,
        SemanticCompletion::Complete
    );

    let compatible = compile(
        "function compatible(value:number):number; function compatible(value:any):any{return value}",
    );
    assert!(
        compatible.diagnostics.is_empty(),
        "{:?}",
        compatible.diagnostics
    );
    assert_eq!(compatible.semantic_completion, SemanticCompletion::Complete);

    let incompatible = compile(
        "function incompatible(value:number):number; function incompatible(value:string):string{return value}",
    );
    assert!(
        incompatible.diagnostics.is_empty(),
        "{:?}",
        incompatible.diagnostics
    );
    assert_eq!(
        incompatible.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert_eq!(
        incompatible.exit_status,
        CompileExitStatus::SemanticIncomplete
    );

    let declared = Compiler::new().compile(
        vec![SourceInput::new(
            "ambient.d.ts",
            Arc::<str>::from("declare function ambientCallable();"),
        )],
        &CompilerOptions {
            no_emit: true,
            strict: true,
            ..CompilerOptions::default()
        },
    );
    assert_eq!(codes(&declared), vec![7010]);
    assert_eq!(declared.semantic_completion, SemanticCompletion::Complete);

    let unowned_dts = Compiler::new().compile(
        vec![SourceInput::new(
            "ambient.d.ts",
            Arc::<str>::from("function unowned():void;"),
        )],
        &CompilerOptions {
            no_emit: true,
            strict: true,
            ..CompilerOptions::default()
        },
    );
    assert!(unowned_dts.diagnostics.is_empty());
    assert_eq!(
        unowned_dts.semantic_completion,
        SemanticCompletion::Deferred
    );
}

#[test]
fn global_overload_demand_uses_the_program_owned_merged_group() {
    let inputs = [
        SourceInput::new(
            "a.ts",
            Arc::<str>::from("declare function choose(value:string):string;"),
        ),
        SourceInput::new(
            "b.ts",
            Arc::<str>::from("declare function choose(value:number):number;"),
        ),
    ];
    let declarations = Compiler::new().compile(
        inputs.to_vec(),
        &CompilerOptions {
            no_emit: true,
            strict: true,
            ..CompilerOptions::default()
        },
    );
    assert!(declarations.diagnostics.is_empty());
    assert_eq!(
        declarations.semantic_completion,
        SemanticCompletion::Complete
    );

    for reverse in [false, true] {
        let mut roots = inputs.to_vec();
        if reverse {
            roots.reverse();
        }
        roots.push(SourceInput::new(
            "use.ts",
            Arc::<str>::from("const result=choose(true);"),
        ));
        let demanded = Compiler::new().compile(
            roots,
            &CompilerOptions {
                no_emit: true,
                strict: true,
                ..CompilerOptions::default()
            },
        );
        assert!(
            demanded.diagnostics.is_empty(),
            "{:?}",
            demanded.diagnostics
        );
        assert_eq!(demanded.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(demanded.exit_status, CompileExitStatus::SemanticIncomplete);
    }
}

#[test]
fn external_module_roots_never_enter_the_script_global_group() {
    let overloaded_module = SourceInput::new(
        "module.ts",
        Arc::<str>::from(
            "export function choose(value:string):string; \
             export function choose(value:any):any{return value}",
        ),
    );
    let script = SourceInput::new(
        "script.ts",
        Arc::<str>::from(
            "declare function choose(value:number):number; \
             const numeric:number=choose(1);",
        ),
    );
    for roots in [
        vec![overloaded_module.clone(), script.clone()],
        vec![script.clone(), overloaded_module],
    ] {
        let output = Compiler::new().compile(
            roots,
            &CompilerOptions {
                no_emit: true,
                strict: true,
                ..CompilerOptions::default()
            },
        );
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(output.exit_status, CompileExitStatus::Success);
    }

    let module_local = Compiler::new().compile(
        vec![
            SourceInput::new(
                "local.ts",
                Arc::<str>::from(
                    "export function choose(value:string):string{return value} \
                     export const local:string=choose('ready');",
                ),
            ),
            script,
        ],
        &CompilerOptions {
            no_emit: true,
            strict: true,
            ..CompilerOptions::default()
        },
    );
    assert!(module_local.diagnostics.is_empty());
    assert_eq!(
        module_local.semantic_completion,
        SemanticCompletion::Complete
    );
    assert_eq!(module_local.exit_status, CompileExitStatus::Success);

    let path_owned_module = SourceInput::new(
        "path-owned.mts",
        Arc::<str>::from(
            "function choose(value:string):string; \
             function choose(value:any):any{return value}",
        ),
    );
    let path_script = SourceInput::new(
        "path-script.ts",
        Arc::<str>::from(
            "declare function choose(value:number):number; \
             const numeric:number=choose(1);",
        ),
    );
    for roots in [
        vec![path_owned_module.clone(), path_script.clone()],
        vec![path_script, path_owned_module],
    ] {
        let output = Compiler::new().compile(
            roots,
            &CompilerOptions {
                no_emit: true,
                strict: true,
                ..CompilerOptions::default()
            },
        );
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(output.exit_status, CompileExitStatus::Success);
    }
}

#[test]
fn class_overload_compatibility_is_complete_only_for_the_bounded_owner() {
    let unannotated = compile("class Vessel { method(); }");
    assert_eq!(codes(&unannotated), vec![2391]);
    assert_eq!(
        unannotated.semantic_completion,
        SemanticCompletion::Deferred
    );

    for source in [
        "class Vessel { method(value:number):number; method(value:string):string{return value} }",
        "class Vessel { constructor(value:number); constructor(value:string){} }",
        "class Vessel { protected constructor(value:number); private constructor(value:any){} }",
    ] {
        let output = compile(source);
        assert!(
            output.diagnostics.is_empty(),
            "{source}: {:?}",
            output.diagnostics
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{source}"
        );
    }
}

#[test]
fn class_overload_promotion_requires_an_ordinary_empty_implementation() {
    for source in [
        "abstract class Shape { abstract convert(value:string):string; convert(value:any):any{} }",
        "class DeclaredMember { declare convert(value:string):string; convert(value:any):any{} }",
        "class AccessorHost { get value():string; get value(){return 'ready'} }",
        "class AsyncHost { async convert(value:string):Promise<string>; async convert(value:any):Promise<any>{} }",
        "class CheckedBody { convert(value:string):string; convert(value:any):any{return value} }",
        "class CheckedConstructor { constructor(value:string); constructor(value:any){value=1} }",
    ] {
        let output = compile(source);
        assert!(
            output.diagnostics.is_empty(),
            "{source}: {:?}",
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

    for source in [
        "class OverrideHost { override method():void; }",
        "class AccessorHost { accessor method():void; }",
        "class DuplicateHost { public public method():void; }",
        "class OptionalHost { method?():void; }",
        "class DefiniteHost { method!():void; }",
        "class StringHost { 'method'():void; }",
        "class NumericHost { 1():void; }",
        "class PrivateHost { #method():void; }",
        "class ComputedHost { [method]():void; }",
        "class PropertyHost { field=missingValue; method():void; }",
        "class AutoAccessorHost { accessor value:number; method():void; }",
        "class ThisHost { method(this:any,value:string):void; method(this:any,value:any):void{} }",
        "class BindingHost { method({value}:any):void; method(value:any):void{} }",
        "class BrokenConstructor { constructor?(); }",
    ] {
        let output = compile(source);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{source}: {:?}",
            output.diagnostics
        );
        assert_ne!(output.exit_status, CompileExitStatus::Success, "{source}");
    }
}

#[test]
fn declaration_emit_blocks_unmodeled_overload_implementation_filtering() {
    let source = concat!(
        "export function select(value:string):string; ",
        "export function select(value:any):any{return value} ",
        "export class Vessel { ",
        "constructor(value:string); constructor(value:any){} ",
        "method(value:string):string; method(value:any):any{return value} ",
        "}",
    );
    for no_check in [false, true] {
        let output = Compiler::new().compile(
            vec![SourceInput::new("overloads.ts", Arc::<str>::from(source))],
            &CompilerOptions {
                declaration: true,
                no_check,
                strict: true,
                target: "esnext".to_string(),
                module: "esnext".to_string(),
                ..CompilerOptions::default()
            },
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        assert!(
            !output.emitted_files.iter().any(|file| file.declaration),
            "{no_check}: {:?}",
            output.emitted_files
        );
        let javascript = output
            .emitted_files
            .iter()
            .find(|file| !file.declaration)
            .expect("runtime overload implementation");
        assert_eq!(
            javascript.text,
            "export function select(value) {\n    return value;\n}\nexport class Vessel {\n    constructor(value) { }\n    method(value) {\n        return value;\n    }\n}\n"
        );
    }

    let cross_file = Compiler::new().compile(
        vec![
            SourceInput::new(
                "signature.ts",
                Arc::<str>::from("function merged(value:string):string;"),
            ),
            SourceInput::new(
                "implementation.ts",
                Arc::<str>::from("function merged(value:any):any{return value}"),
            ),
        ],
        &CompilerOptions {
            declaration: true,
            no_check: true,
            strict: true,
            ..CompilerOptions::default()
        },
    );
    assert_eq!(cross_file.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        cross_file.exit_status,
        CompileExitStatus::SemanticIncomplete
    );
    assert!(!cross_file.emitted_files.iter().any(|file| file.declaration));

    let modules = [
        SourceInput::new(
            "a.ts",
            Arc::<str>::from(
                "export function shared(value:string):string; export function shared(value:any):any{return value}",
            ),
        ),
        SourceInput::new(
            "b.ts",
            Arc::<str>::from("export function shared(value:number):number{return value}"),
        ),
    ];
    let checked_modules = Compiler::new().compile(
        modules.to_vec(),
        &CompilerOptions {
            no_emit: true,
            strict: true,
            ..CompilerOptions::default()
        },
    );
    assert!(
        checked_modules.diagnostics.is_empty(),
        "{:?}",
        checked_modules.diagnostics
    );
    assert_eq!(
        checked_modules.semantic_completion,
        SemanticCompletion::Complete
    );

    let module_products = Compiler::new().compile(
        modules.to_vec(),
        &CompilerOptions {
            declaration: true,
            no_check: true,
            strict: true,
            ..CompilerOptions::default()
        },
    );
    assert_eq!(
        module_products.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert_eq!(
        module_products
            .emitted_files
            .iter()
            .filter(|file| file.declaration)
            .map(|file| file.path.to_string_lossy().to_string())
            .collect::<Vec<_>>(),
        ["b.d.ts"]
    );
}

#[test]
fn function_modifier_owners_and_products_fail_closed() {
    for source in [
        "async function delayed(value:string):number; async function delayed(value:any):any{return value}",
        "abstract function impossible():void;",
        "declare declare function repeated():void;",
    ] {
        let output = compile(source);
        assert!(
            output.diagnostics.is_empty(),
            "{source}: {:?}",
            output.diagnostics
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    }

    for source in [
        "function receiver(this:any):void;",
        "function implemented(this:any):void {}",
        "function parameterProperty(public value:number):void;",
        "function implementedProperty(public value:number):void {}",
        "function recovered({value}:any):void;",
    ] {
        let output = compile(source);
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_ne!(output.exit_status, CompileExitStatus::Success);
    }

    let default_overload = concat!(
        "export default function selected(value:string):string; ",
        "export default function selected(value:any):any{return value}",
    );
    for product_source in [
        default_overload,
        "export function outer(){async async function inner():Promise<void>{}}",
    ] {
        for module in ["commonjs", "esnext"] {
            for no_check in [false, true] {
                let output = Compiler::new().compile(
                    vec![SourceInput::new(
                        "default.ts",
                        Arc::<str>::from(product_source),
                    )],
                    &CompilerOptions {
                        declaration: true,
                        no_check,
                        module: module.to_string(),
                        target: "esnext".to_string(),
                        ..CompilerOptions::default()
                    },
                );
                assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
                assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
                assert!(
                    output.emitted_files.is_empty(),
                    "{module}/{no_check}: {:?}",
                    output.emitted_files
                );
            }
        }
    }

    let abstract_product = Compiler::new().compile(
        vec![SourceInput::new(
            "abstract.ts",
            Arc::<str>::from("abstract function impossible():void;"),
        )],
        &CompilerOptions {
            declaration: true,
            no_check: true,
            ..CompilerOptions::default()
        },
    );
    assert_eq!(
        abstract_product.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert!(abstract_product.emitted_files.is_empty());

    let recovered_product = Compiler::new().compile(
        vec![SourceInput::new(
            "recovered.ts",
            Arc::<str>::from("function recovered({value}:any):void;"),
        )],
        &CompilerOptions {
            declaration: true,
            no_check: true,
            ..CompilerOptions::default()
        },
    );
    assert_eq!(
        recovered_product.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert!(recovered_product.emitted_files.is_empty());

    let async_product = Compiler::new().compile(
        vec![SourceInput::new(
            "async.ts",
            Arc::<str>::from(
                "async function delayed(value:string):Promise<string>; \
                 async function delayed(value:any):Promise<any>{}",
            ),
        )],
        &CompilerOptions {
            declaration: true,
            no_check: true,
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(
        async_product.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert_eq!(
        async_product
            .emitted_files
            .iter()
            .filter(|file| file.declaration)
            .count(),
        0
    );
    assert_eq!(
        async_product
            .emitted_files
            .iter()
            .filter(|file| !file.declaration)
            .count(),
        1
    );
}

#[test]
fn bodyless_class_method_declaration_emit_recovers_with_any() {
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "loose.ts",
            Arc::<str>::from("export class Loose { method(); }"),
        )],
        &CompilerOptions {
            declaration: true,
            no_check: true,
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(output.exit_status, CompileExitStatus::Success);
    let javascript = output
        .emitted_files
        .iter()
        .find(|file| !file.declaration)
        .expect("runtime output");
    assert_eq!(javascript.text, "export class Loose {\n}\n");
    let declaration = output
        .emitted_files
        .iter()
        .find(|file| file.declaration)
        .expect("declaration output");
    assert_eq!(
        declaration.text,
        "export declare class Loose {\n    method(): any;\n}\n"
    );
}

#[test]
fn class_product_preflight_blocks_unowned_member_syntax_in_every_mode() {
    for source in [
        "export class Vessel { accessor value:number; method():void; }",
        "export function wrapped(){ class Nested { accessor value:number; } }",
        "async class Vessel {}",
        "export export class Vessel {}",
        "export class Vessel { override method():void; }",
        "export class Vessel { declare method():void; }",
        "export abstract class Vessel { abstract method():void; }",
        "export class Vessel { public public method():void; }",
        "export class Vessel { method?():void; }",
        "export class Vessel { method!():void; }",
        "export class Vessel { method(:void; }",
    ] {
        for module in ["commonjs", "esnext"] {
            for no_check in [false, true] {
                let output = Compiler::new().compile(
                    vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
                    &CompilerOptions {
                        declaration: true,
                        no_check,
                        target: "esnext".to_string(),
                        module: module.to_string(),
                        ..CompilerOptions::default()
                    },
                );
                assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
                assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
                assert!(
                    output.emitted_files.is_empty(),
                    "{source}/{module}/{no_check}"
                );
            }
        }
    }
}

#[test]
fn class_overload_exemptions_and_boundaries_match_ts7() {
    let ambient = Compiler::new().compile(
        vec![SourceInput::new(
            "ambient.d.ts",
            Arc::<str>::from(
                "class Ambient { constructor(); method(): void; static create(): void; }",
            ),
        )],
        &CompilerOptions {
            no_emit: true,
            strict: true,
            ..CompilerOptions::default()
        },
    );
    assert!(ambient.diagnostics.is_empty(), "{:?}", ambient.diagnostics);

    let declared_and_abstract = compile(
        "declare class Ambient { constructor(); method(): void; }\n\
         abstract class Shape { abstract area(): number; }",
    );
    assert!(
        declared_and_abstract.diagnostics.is_empty(),
        "{:?}",
        declared_and_abstract.diagnostics
    );

    let static_missing = compile("class Factory { static create(): void; }");
    assert_eq!(codes(&static_missing), vec![2391]);

    let static_implementation = compile(
        "class Factory { create(): void; static create() {} }\n\
         class Instance { static create(): void; create() {} }",
    );
    assert_eq!(codes(&static_implementation), vec![2388, 2387]);

    let abstract_constructor = compile("abstract class Base { constructor(); }");
    assert_eq!(codes(&abstract_constructor), vec![2390]);

    let multiple_missing = compile(
        "class Missing {\n\
           constructor(value: string);\n\
           constructor(value: number);\n\
           method(value: string): void;\n\
           method(value: number): void;\n\
         }",
    );
    assert_eq!(codes(&multiple_missing), vec![2390, 2391]);
    assert_eq!(
        multiple_missing.diagnostics[0].start,
        multiple_missing
            .program
            .source(tsz::source::FileId(0))
            .unwrap()
            .text
            .rfind("constructor")
            .unwrap() as u32
    );

    let property_boundary =
        compile("class Interrupted { method(): void; value: number = 1; method() {} }");
    assert_eq!(codes(&property_boundary), vec![2391]);
}

#[test]
fn modern_type_operator_grammar_is_structured_without_recovery_diagnostics() {
    let source = r#"
        export type Primitive =
            | string
            | number
            | bigint
            | boolean
            | symbol
            | null
            | undefined;
        declare const add: (amount: number) => { value: number };
        export type Add = typeof add;
        export type Nested = Promise<Array<string>>;
        export type Extract<T> = T extends (arg: any) => infer R ? R : never;
        export type Factory<T extends object> = abstract new (...args: any[]) => T;
        export function isPrimitive(value: unknown): value is Primitive;
    "#;
    let output = Compiler::new().compile(
        vec![SourceInput::new("operators.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            no_check: true,
            no_emit: true,
            ..CompilerOptions::default()
        },
    );
    assert!(
        output.diagnostics.is_empty(),
        "valid modern type grammar recovered as errors: {:?}",
        output.diagnostics
    );
}

#[test]
fn unary_equality_and_logical_expressions_follow_precedence_without_recovery() {
    let source = r#"
        const isText = (value: unknown): value is string =>
            !value || (typeof value === "string" && value != null);
    "#;
    let output = Compiler::new().compile(
        vec![SourceInput::new("operators.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            no_check: true,
            no_emit: true,
            ..CompilerOptions::default()
        },
    );
    assert!(
        output.diagnostics.is_empty(),
        "valid operator grammar recovered as errors: {:?}",
        output.diagnostics
    );
}

#[test]
fn logical_and_unary_queries_complete_only_when_their_operands_decide_the_result() {
    let output = compile(
        r#"
        const one: number = true && 1;
        const text: string = false || "ok";
        const fallback: string = null ?? "ok";
        const wrongLogical: string = true && 1;
        const negative: number = -1;
        const wrongUnary: string = -1;
        "#,
    );
    assert_eq!(codes(&output), vec![2322, 2322]);
}

#[test]
fn unsupported_deferred_forms_stay_typed_without_fabricated_diagnostics() {
    let conditional = compile(
        r#"
        type Choice<T> = T extends string ? number : boolean;
        let choice: Choice<string>;
        choice = "wrong";
        "#,
    );
    assert!(conditional.diagnostics.is_empty());

    let mapped = compile(
        r#"
        type Flags<T> = { [K in keyof T]: T[K] };
        let flags: Flags<{ ready: boolean }>;
        flags = "wrong";
        "#,
    );
    assert!(mapped.diagnostics.is_empty());

    let type_query = compile(
        r#"
        let math: typeof Math;
        math = 1;
        "#,
    );
    assert!(type_query.diagnostics.is_empty());

    let identifier = compile(
        r#"
        let promise: Promise<number>;
        let value = promise;
        const count: number = value;
        "#,
    );
    assert!(identifier.diagnostics.is_empty());

    let call_and_member = compile(
        r#"
        const text: string = Math.max(1, 2);
        "#,
    );
    assert!(call_and_member.diagnostics.is_empty());
}

#[test]
fn control_flow_syntax_has_structured_nodes_and_exact_spans() {
    let source = concat!(
        "const ready = true;\n",
        "if (ready) { work(); } else fallback();\n",
        "switch (ready) {\n",
        "  case true: work(); break;\n",
        "  default: { fallback(); continue outer; }\n",
        "}\n",
    );
    let output = Compiler::new().compile(
        vec![SourceInput::new("flow.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            no_check: true,
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(
        output.diagnostics.is_empty(),
        "valid control flow recovered as errors: {:?}",
        output.diagnostics
    );

    let file = &output.program.files[0];
    let tsz::syntax::StatementKind::If(if_statement) = &file.syntax.statements[1].kind else {
        panic!("if statement was not represented structurally");
    };
    assert_eq!(file.source.slice(if_statement.condition.span), "ready");
    assert_eq!(
        file.source.slice(if_statement.then_statement.span),
        "{ work(); }"
    );
    assert_eq!(
        file.source
            .slice(if_statement.else_statement.as_ref().unwrap().span),
        "fallback();"
    );

    let tsz::syntax::StatementKind::Switch(switch_statement) = &file.syntax.statements[2].kind
    else {
        panic!("switch statement was not represented structurally");
    };
    assert_eq!(file.source.slice(switch_statement.expression.span), "ready");
    assert_eq!(switch_statement.clauses.len(), 2);
    assert_eq!(
        file.source.slice(switch_statement.clauses[0].span),
        "case true: work(); break;"
    );
    assert_eq!(
        file.source.slice(switch_statement.clauses[1].span),
        "default: { fallback(); continue outer; }"
    );
    assert!(matches!(
        switch_statement.clauses[0].kind,
        tsz::syntax::SwitchClauseKind::Case(_)
    ));
    assert!(matches!(
        switch_statement.clauses[1].kind,
        tsz::syntax::SwitchClauseKind::Default
    ));

    assert_eq!(
        output.emitted_files[0].text,
        concat!(
            "\"use strict\";\n",
            "const ready = true;\n",
            "if (ready) {\n",
            "    work();\n",
            "}\n",
            "else\n",
            "    fallback();\n",
            "switch (ready) {\n",
            "    case true:\n",
            "        work();\n",
            "        break;\n",
            "    default:\n",
            "        {\n",
            "            fallback();\n",
            "            continue outer;\n",
            "        }\n",
            "}\n",
        )
    );
}

#[test]
fn navigation_visits_control_flow_conditions_cases_and_branches() {
    let source = concat!(
        "const value = 1;\n",
        "if (value) { value; } else { value; }\n",
        "switch (value) { case value: value; break; default: value; }\n",
    );
    let declaration = source.find("value").unwrap() as u32;
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open("flow.ts", Arc::<str>::from(source));

    let references = service.references("flow.ts", declaration + 1);
    assert_eq!(references.len(), 1);
    assert_eq!(references[0].references.len(), 8);
}

#[test]
fn contextual_keywords_are_identifiers_in_binding_reference_and_module_names() {
    let source = r#"
        let get = 1;
        let set = get;
        let from = set;
        let of = from;
        let type = of;
        let as = type;
        function getValue(set: number): number { return set; }
        type fromType = string;
        type ofType = fromType;
        type type = ofType;
        type as = type;
        export { get, set, from, of, type, as };
        import * as set from "./dep";
        import { get, set as from, type, type as as } from "./dep";
        import { type fromType, type as } from "./dep";
    "#;
    let output = Compiler::new().compile(
        vec![SourceInput::new("contextual.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            no_check: true,
            no_emit: true,
            ..CompilerOptions::default()
        },
    );
    assert!(
        output.diagnostics.is_empty(),
        "contextual identifiers triggered parser recovery: {:?}",
        output.diagnostics
    );

    let statements = &output.program.files[0].syntax.statements;
    let tsz::syntax::StatementKind::Export(export) = &statements[11].kind else {
        panic!("contextual export was not represented structurally");
    };
    assert_eq!(
        export
            .specifiers
            .iter()
            .map(|specifier| specifier.local.as_str())
            .collect::<Vec<_>>(),
        ["get", "set", "from", "of", "type", "as"]
    );

    let tsz::syntax::StatementKind::Import(named) = &statements[13].kind else {
        panic!("contextual named import was not represented structurally");
    };
    assert_eq!(
        named
            .bindings
            .iter()
            .map(|binding| (binding.imported.as_deref(), binding.local.as_str()))
            .collect::<Vec<_>>(),
        [
            (Some("get"), "get"),
            (Some("set"), "from"),
            (Some("type"), "type"),
            (Some("type"), "as"),
        ]
    );
}

#[test]
fn readonly_object_members_require_complete_mapped_type_lookahead() {
    let source = r#"
        type Ordinary<T> = { readonly foo: T; readonly: T; readonly?: T };
        type Mapped<T> = { readonly [P in keyof T]?: T[P] };
        type Added<T> = { +readonly [P in keyof T]+?: T[P] };
        type Removed<T> = { -readonly [P in keyof T]-?: T[P] };
    "#;
    let output = Compiler::new().compile(
        vec![SourceInput::new("mapped.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            no_check: true,
            no_emit: true,
            ..CompilerOptions::default()
        },
    );
    assert!(
        output.diagnostics.is_empty(),
        "readonly object syntax was misclassified as mapped: {:?}",
        output.diagnostics
    );

    let statements = &output.program.files[0].syntax.statements;
    let tsz::syntax::StatementKind::TypeAlias(ordinary) = &statements[0].kind else {
        panic!("ordinary type alias was not represented structurally");
    };
    let tsz::syntax::TypeNodeKind::Object(members) = &ordinary.ty.kind else {
        panic!("ordinary readonly members were parsed as a mapped type");
    };
    assert_eq!(
        members
            .iter()
            .map(|member| {
                let tsz::syntax::TypeMemberKind::Property { name, optional, .. } = &member.kind
                else {
                    panic!("ordinary member was not a property signature");
                };
                (
                    name.semantic_name().expect("identifier member name"),
                    member.modifiers.readonly,
                    *optional,
                )
            })
            .collect::<Vec<_>>(),
        [
            ("foo", true, false),
            ("readonly", false, false),
            ("readonly", false, true),
        ]
    );

    for statement in &statements[1..] {
        let tsz::syntax::StatementKind::TypeAlias(alias) = &statement.kind else {
            panic!("mapped type alias was not represented structurally");
        };
        assert!(matches!(
            alias.ty.kind,
            tsz::syntax::TypeNodeKind::Mapped { .. }
        ));
    }
}

#[test]
fn dynamic_import_and_import_meta_stay_on_the_expression_path() {
    let source = concat!(
        "import(\"package\");\n",
        "import.meta;\n",
        "const load = import(\"package\");\n",
        "const url = import.meta;\n",
        "import value from \"package\";\n",
    );
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "dynamic-import.ts",
            Arc::<str>::from(source),
        )],
        &CompilerOptions {
            no_check: true,
            no_emit: true,
            ..CompilerOptions::default()
        },
    );
    assert!(
        output.diagnostics.is_empty(),
        "dynamic import syntax routed through static imports: {:?}",
        output.diagnostics
    );

    let statements = &output.program.files[0].syntax.statements;
    let tsz::syntax::StatementKind::Expression(call) = &statements[0].kind else {
        panic!("dynamic import call was not an expression statement");
    };
    assert!(matches!(
        call.kind,
        tsz::syntax::ExpressionKind::Call { .. }
    ));
    let tsz::syntax::StatementKind::Expression(meta) = &statements[1].kind else {
        panic!("import.meta was not an expression statement");
    };
    assert!(matches!(
        meta.kind,
        tsz::syntax::ExpressionKind::Member { .. }
    ));
    assert!(matches!(
        statements[4].kind,
        tsz::syntax::StatementKind::Import(_)
    ));
}

#[test]
fn emit_blocks_unchecked_module_reachability_and_formats_empty_bodies() {
    let source = "class B{x=0} export class D extends B{constructor(override x:number){super()}}";
    for no_check in [false, true] {
        let output = Compiler::new().compile(
            vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
            &CompilerOptions {
                declaration: true,
                no_check,
                target: "esnext".to_string(),
                module: "esnext".to_string(),
                ..CompilerOptions::default()
            },
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.emitted_files.len(), 1);
        assert!(!output.emitted_files[0].declaration);
        assert_eq!(
            output.emitted_files[0].text,
            "class B {\n    x = 0;\n}\nexport class D extends B {\n    x;\n    constructor(x) {\n        super();\n        this.x = x;\n    }\n}\n"
        );
    }

    let ambient = Compiler::new().compile(
        vec![SourceInput::new(
            "case.ts",
            Arc::<str>::from("declare function work():void;export class C{m(){work()}}"),
        )],
        &CompilerOptions {
            declaration: true,
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(ambient.semantic_completion, SemanticCompletion::Deferred);
    assert!(ambient.emitted_files.iter().all(|file| !file.declaration));

    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "case.ts",
            Arc::<str>::from("export function f(){} export class C{m(){}constructor(){}}"),
        )],
        &CompilerOptions {
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(
        output.emitted_files[0].text,
        "export function f() { }\nexport class C {\n    m() { }\n    constructor() { }\n}\n"
    );
}
