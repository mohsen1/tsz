use super::*;

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
        "function recovered({value}:any):void;",
    ] {
        let output = compile(source);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{source}"
        );
        assert_ne!(output.exit_status, CompileExitStatus::Success);
    }
    for (source, expected) in [
        (
            "function parameterProperty(public value:number):void;",
            vec![
                (
                    2391,
                    9,
                    17,
                    "Function implementation is missing or not immediately following the declaration.",
                ),
                (
                    2369,
                    27,
                    19,
                    "A parameter property is only allowed in a constructor implementation.",
                ),
            ],
        ),
        (
            "function implementedProperty(public value:number):void {}",
            vec![(
                2369,
                29,
                19,
                "A parameter property is only allowed in a constructor implementation.",
            )],
        ),
    ] {
        let output = compile(source);
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(
            output.exit_status,
            CompileExitStatus::DiagnosticsPresentOutputsSkipped
        );
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
            expected,
            "{source}",
        );
        assert!(
            output
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.category == DiagnosticCategory::Error)
        );
    }
    let default_overload = concat!(
        "export default function selected(value:string):string; ",
        "export default function selected(value:any):any{return value}",
    );
    for (product_source, esnext_javascript) in [
        (default_overload, true),
        (
            "export function outer(){async async function inner():Promise<void>{}}",
            false,
        ),
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
                let expected_javascript = (module == "esnext" && esnext_javascript)
                    .then_some("export default function selected(value) { return value; }\n");
                assert_eq!(
                    output
                        .emitted_files
                        .iter()
                        .filter(|file| !file.declaration)
                        .map(|file| file.text.as_str())
                        .collect::<Vec<_>>(),
                    expected_javascript.into_iter().collect::<Vec<_>>(),
                    "{module}/{no_check}: {:?}",
                    output.emitted_files,
                );
                assert!(
                    output.emitted_files.iter().all(|file| !file.declaration),
                    "the unmodeled overload declaration summary must not enter declaration emit: {module}/{no_check}: {:?}",
                    output.emitted_files,
                );
            }
        }
    }
    for (path, source) in [
        ("abstract.ts", "abstract function impossible():void;"),
        ("recovered.ts", "function recovered({value}:any):void;"),
    ] {
        let output = Compiler::new().compile(
            vec![SourceInput::new(path, Arc::<str>::from(source))],
            &CompilerOptions {
                declaration: true,
                no_check: true,
                ..CompilerOptions::default()
            },
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert!(output.emitted_files.is_empty());
    }
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
