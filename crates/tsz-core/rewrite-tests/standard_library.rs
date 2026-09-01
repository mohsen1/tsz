use std::sync::Arc;

use tsz::bind::Meaning;
use tsz::{Compiler, CompilerOptions, SemanticCompletion, SourceInput};

fn compile(source: &str, options: CompilerOptions) -> tsz::CompileOutput {
    compile_files(&[("case.ts", source)], options)
}

fn compile_files(sources: &[(&str, &str)], options: CompilerOptions) -> tsz::CompileOutput {
    Compiler::new().compile(
        sources
            .iter()
            .map(|(path, source)| SourceInput::new(*path, Arc::<str>::from(*source)))
            .collect(),
        &CompilerOptions {
            no_emit: true,
            ..options
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

fn assert_missing_essential_globals(output: &tsz::CompileOutput) {
    let expected_names = [
        "Array",
        "Boolean",
        "CallableFunction",
        "Function",
        "IArguments",
        "NewableFunction",
        "Number",
        "Object",
        "RegExp",
        "String",
    ];
    assert_eq!(codes(output), vec![2318; expected_names.len()]);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.file.clone(),
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.clone(),
            ))
            .collect::<Vec<_>>(),
        expected_names
            .iter()
            .map(|name| (
                String::new(),
                0,
                0,
                format!("Cannot find global type '{name}'.")
            ))
            .collect::<Vec<_>>()
    );
}

#[test]
fn default_library_contributes_generated_type_and_value_symbols() {
    // When a selected pinned TS7 library declares a global, the program owns
    // one ambient declaration identity and name lookup uses that identity.
    let output = compile(
        "let table: Record<string, number>; parseInt; console;",
        CompilerOptions::default(),
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);

    for (name, meaning) in [
        ("Array", Meaning::Type),
        ("Array", Meaning::Value),
        ("Map", Meaning::Type),
        ("Map", Meaning::Value),
        ("Record", Meaning::Type),
        ("parseInt", Meaning::Value),
        ("console", Meaning::Value),
    ] {
        let declaration = output.program.resolve_global(name, meaning).unwrap();
        let declaration = output
            .program
            .standard_library_declaration(declaration)
            .unwrap();
        assert_eq!(declaration.name, name);
        assert_eq!(declaration.meaning, meaning);
    }
}

#[test]
fn canonical_map_references_and_constructor_shells_complete() {
    // When the binder resolves the canonical pinned-library Map declarations,
    // the checker preserves their identity through generic aliases, wrappers,
    // values, and the zero-argument constructor overload.
    for source in [
        "type Registry<Key,Value>=Map<Key,Value>;declare const value:Registry<string,number>;const exact:Map<string,number>=value;",
        "type Registry<Key,Value>=Map<Key,Value>;type Wrapped<Item>={inner:Registry<string,Item>};declare const value:Wrapped<number>;const exact:Wrapped<number>=value;",
        "Map;const Factory=Map;Factory;",
        "new Map();",
        "new Map<string,number>();",
        "const Factory=Map;new Factory<string,number>();",
        "const exact:Map<string,number>=new Map<string,number>();",
        "const exact:Map<string,number>=new Map();",
        "const Factory=Map;const exact:Map<string,number>=new Factory();",
        "const Factory=Map;const Wrapped=Factory;const exact:Map<string,number>=(new Wrapped());",
    ] {
        let output = compile(source, CompilerOptions::default());
        assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source}: {:?}",
            output.stats
        );
    }
}

#[test]
fn explicit_new_arguments_use_the_typed_construct_owner() {
    // TS7 accepts exact local and canonical/renamed Map applications in both
    // checked and --noCheck programs. The latter skips constraint and arity
    // diagnostics; TSZ keeps those unported checked demands explicitly deferred.
    for no_check in [false, true] {
        for source in [
            "class Local<Item>{}new Local<string>();",
            "new Map<string,number>();",
            "const Factory=Map;new Factory<string,number>();",
            "new Map<string,number>([]);new Map<string,number>().get('key');",
            "const Factory=Map;const Wrapped=Factory;new Wrapped<string,number>([]);",
            "new Map<string,number>([] as never[]);",
        ] {
            let output = compile(
                source,
                CompilerOptions {
                    no_check,
                    ..CompilerOptions::default()
                },
            );
            assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
            assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        }
    }

    for source in [
        "class Local<Item>{}new Local<string,number>();",
        "class Local<Item extends string>{}new Local<number>();",
        "new Map<string>();",
        "new Map<string,>();",
        "new Map<string,number>(42);",
        "new Map<string,number>([['key',1]]);",
        "new Map<string,number>(null);",
        "new Map([]);",
        "new Map<string,number>([]);new Map<string,number>(42);",
    ] {
        let checked = compile(source, CompilerOptions::default());
        assert_eq!(
            checked.diagnostics,
            [],
            "{source}: {:?}",
            checked.diagnostics
        );
        assert_eq!(checked.semantic_completion, SemanticCompletion::Deferred);

        let unchecked = compile(
            source,
            CompilerOptions {
                no_check: true,
                ..CompilerOptions::default()
            },
        );
        assert_eq!(
            unchecked.diagnostics,
            [],
            "{source}: {:?}",
            unchecked.diagnostics
        );
        assert_eq!(unchecked.semantic_completion, SemanticCompletion::Complete);
    }
}

#[test]
fn map_call_without_new_remains_bounded() {
    let source = "Map();";
    let output = compile(source, CompilerOptions::default());
    assert_eq!(output.diagnostics, [], "{:?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn canonical_map_get_and_set_project_type_arguments() {
    // When a receiver is the canonical pinned-library Map application, the
    // member query substitutes its key/value arguments into get/set.
    for source in [
        "export function read<Key,Value>(map:Map<Key,Value>,key:Key):Value|undefined{return map.get(key);}",
        "export function write<Key,Value>(map:Map<Key,Value>,key:Key,value:Value):Map<Key,Value>{return map.set(key,value);}",
        "declare const registry:Map<string,number>;const found:number|undefined=registry.get('ready');",
        "declare const registry:Map<string,number>;const updated:Map<string,number>=registry.set('ready',1);",
        "type Registry<Index,Item>=Map<Index,Item>;declare const renamed:Registry<string,number>;const found:number|undefined=renamed.get('ready');",
        "type Registry<Index,Item>=Map<Index,Item>;type Wrapped<Item>={inner:Registry<string,Item>};declare const wrapped:Wrapped<number>;const updated:Registry<string,number>=wrapped.inner.set('ready',1);",
        "declare const registry:Map<string,number>;const found:number|undefined=(registry).get('ready');",
    ] {
        let output = compile(source, CompilerOptions::default());
        assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source}: {:?}",
            output.stats
        );
    }
}

#[test]
fn canonical_map_member_calls_report_key_value_and_arity_diagnostics() {
    for (source, start, length, message) in [
        (
            "declare const registry:Map<string,number>;registry.get(1);",
            "declare const registry:Map<string,number>;registry.get(".len(),
            1,
            "Argument of type 'number' is not assignable to parameter of type 'string'.",
        ),
        (
            "declare const registry:Map<string,number>;registry.set(1,2);",
            "declare const registry:Map<string,number>;registry.set(".len(),
            1,
            "Argument of type 'number' is not assignable to parameter of type 'string'.",
        ),
        (
            "declare const registry:Map<string,number>;registry.set('ready','wrong');",
            "declare const registry:Map<string,number>;registry.set('ready',".len(),
            7,
            "Argument of type 'string' is not assignable to parameter of type 'number'.",
        ),
    ] {
        let output = compile(source, CompilerOptions::default());
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(codes(&output), [2345], "{source}: {:?}", output.diagnostics);
        assert_eq!(
            (output.diagnostics[0].start, output.diagnostics[0].length),
            (start as u32, length)
        );
        assert_eq!(output.diagnostics[0].message_text, message);
    }

    for (call, anchor, length, message) in [
        (
            "registry.get();",
            "get",
            3,
            "Expected 1 arguments, but got 0.",
        ),
        (
            "registry.get('ready',1);",
            "1",
            1,
            "Expected 1 arguments, but got 2.",
        ),
        (
            "registry.set('ready');",
            "set",
            3,
            "Expected 2 arguments, but got 1.",
        ),
        (
            "registry.set('ready',1,2);",
            "2",
            1,
            "Expected 2 arguments, but got 3.",
        ),
    ] {
        let source = format!("declare const registry:Map<string,number>;{call}");
        let output = compile(&source, CompilerOptions::default());
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(codes(&output), [2554], "{source}: {:?}", output.diagnostics);
        assert_eq!(
            (output.diagnostics[0].start, output.diagnostics[0].length),
            (source.rfind(anchor).unwrap() as u32, length)
        );
        assert_eq!(output.diagnostics[0].message_text, message);
    }
}

#[test]
fn member_call_arity_span_change_preserves_non_map_and_array_boundaries() {
    let source = "declare const service:{run(value:string):void};service.run();";
    let output = compile(source, CompilerOptions::default());
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(codes(&output), [2554]);
    assert_eq!(
        output.diagnostics[0].start,
        source.rfind("run").unwrap() as u32
    );
    assert_eq!(output.diagnostics[0].length, 3);

    let array = compile(
        "const values=[1];values.indexOf();",
        CompilerOptions::default(),
    );
    assert_eq!(array.diagnostics, []);
    assert_eq!(array.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn canonical_map_reference_relations_follow_typescript_argument_order() {
    for source in [
        "declare const broad:Map<any,any>;const exact:Map<string,number>=broad;",
        "declare const exact:Map<string,number>;const broad:Map<unknown,unknown>=exact;",
        "declare const empty:Map<string,never>;const exact:Map<string,number>=empty;",
    ] {
        let output = compile(source, CompilerOptions::default());
        assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }

    for (source, name, source_name, target_name, leaf) in [
        (
            "declare const text:Map<string,string>;const numeric:Map<string,number>=text;",
            "numeric",
            "Map<string, string>",
            "Map<string, number>",
            "Type 'string' is not assignable to type 'number'.",
        ),
        (
            "const wrong:Map<string,number>=new Map<number,number>();",
            "wrong",
            "Map<number, number>",
            "Map<string, number>",
            "Type 'number' is not assignable to type 'string'.",
        ),
    ] {
        let output = compile(source, CompilerOptions::default());
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(
            codes(&output),
            vec![2322],
            "{source}: {:?}",
            output.diagnostics
        );
        let diagnostic = &output.diagnostics[0];
        assert_eq!(diagnostic.start, source.find(name).unwrap() as u32);
        assert_eq!(diagnostic.length, name.len() as u32);
        assert_eq!(
            diagnostic.message_text,
            format!("Type '{source_name}' is not assignable to type '{target_name}'.")
        );
        assert_eq!(diagnostic.related_information.len(), 1);
        assert_eq!(diagnostic.related_information[0].code, 2322);
        assert_eq!(diagnostic.related_information[0].depth, 1);
        assert_eq!(diagnostic.related_information[0].message_text, leaf);
    }
}

#[test]
fn canonical_map_constructor_relation_respects_authored_identity_boundaries() {
    let merged = compile(
        "interface Map<K,V>{local:K}const exact:Map<string,number>=new Map();",
        CompilerOptions::default(),
    );
    assert_eq!(merged.diagnostics, []);
    assert_eq!(merged.semantic_completion, SemanticCompletion::Deferred);

    let local = compile(
        "export {};class Map<Key,Value>{}const exact:Map<string,number>=new Map();",
        CompilerOptions::default(),
    );
    assert_eq!(local.diagnostics, [], "{:?}", local.diagnostics);
    assert_eq!(local.semantic_completion, SemanticCompletion::Complete);

    let no_library = compile(
        "const exact=1;",
        CompilerOptions {
            no_lib: true,
            ..CompilerOptions::default()
        },
    );
    assert!(
        no_library
            .program
            .resolve_global("Map", Meaning::Type)
            .is_none()
            && no_library
                .program
                .resolve_global("Map", Meaning::Value)
                .is_none()
    );
}

#[test]
fn map_identity_does_not_claim_missing_libraries_or_authored_declarations() {
    let missing = compile(
        "const value=1;",
        CompilerOptions {
            lib: Some(vec!["ES5".to_string()]),
            ..CompilerOptions::default()
        },
    );
    assert!(
        missing
            .program
            .resolve_global("Map", Meaning::Type)
            .is_none()
            && missing
                .program
                .resolve_global("Map", Meaning::Value)
                .is_none()
    );

    let merged = compile(
        "interface Map<K,V>{local:K}declare const value:Map<string,number>;value.local;",
        CompilerOptions::default(),
    );
    assert_eq!(merged.diagnostics, []);
    assert_eq!(merged.semantic_completion, SemanticCompletion::Deferred);

    let local = compile(
        "export interface Map<Key,Value>{local:Key}declare const value:Map<string,number>;const exact:string=value.local;",
        CompilerOptions::default(),
    );
    assert_eq!(local.diagnostics, [], "{:?}", local.diagnostics);
    assert_eq!(local.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn map_member_projection_respects_merge_shadow_and_no_lib_boundaries() {
    let merged = compile(
        "interface Map<Key,Value>{local(value:Value):Key}declare const registry:Map<string,number>;registry.get('ready');",
        CompilerOptions::default(),
    );
    assert_eq!(merged.diagnostics, []);
    assert_eq!(merged.semantic_completion, SemanticCompletion::Deferred);

    let local = compile(
        "export {};interface Map<Index,Item>{get(key:Index):Item}declare const registry:Map<string,number>;registry.get('ready');",
        CompilerOptions::default(),
    );
    assert_eq!(local.diagnostics, []);
    assert_eq!(local.semantic_completion, SemanticCompletion::Complete);

    let no_library = compile(
        "declare const registry:Map<string,number>;registry.get('ready');",
        CompilerOptions {
            no_lib: true,
            ..CompilerOptions::default()
        },
    );
    assert_missing_essential_globals(&no_library);
    assert!(
        no_library
            .program
            .resolve_global("Map", Meaning::Type)
            .is_none()
            && no_library
                .program
                .resolve_global("Map", Meaning::Value)
                .is_none()
    );
}

#[test]
fn canonical_map_member_projection_is_cold_warm_and_root_order_stable() {
    let compiler = Compiler::new();
    let declarations = SourceInput::new(
        "models.ts",
        Arc::<str>::from(
            "type Registry<Index,Item>=Map<Index,Item>;declare const registry:Registry<string,number>;",
        ),
    );
    let use_site = SourceInput::new(
        "use.ts",
        Arc::<str>::from(
            "const found:number|undefined=registry.get('ready');const updated:Map<string,number>=registry.set('ready',1);",
        ),
    );
    let run = |files| {
        compiler.compile(
            files,
            &CompilerOptions {
                no_emit: true,
                ..CompilerOptions::default()
            },
        )
    };
    let cold = run(vec![declarations.clone(), use_site.clone()]);
    let warm = run(vec![declarations.clone(), use_site.clone()]);
    let reversed = run(vec![use_site, declarations]);
    for output in [&cold, &warm, &reversed] {
        assert_eq!(output.diagnostics, [], "{:?}", output.diagnostics);
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }
    assert_eq!(cold.stats.types, warm.stats.types);
    assert_eq!(cold.stats.types, reversed.stats.types);
}

#[test]
fn canonical_map_references_are_cold_warm_and_root_order_stable() {
    let compiler = Compiler::new();
    let declarations = SourceInput::new(
        "models.ts",
        Arc::<str>::from(
            "type Registry<Key,Value>=Map<Key,Value>;declare const value:Registry<string,number>;const Factory=Map;const Wrapped=Factory;",
        ),
    );
    let use_site = SourceInput::new(
        "use.ts",
        Arc::<str>::from(
            "const exact:Map<string,number>=value;const direct:Map<string,number>=new Map<string,number>([]);const renamed:Map<string,number>=new Wrapped<string,number>([]);new Map<string,number>();",
        ),
    );
    let run = |files| {
        compiler.compile(
            files,
            &CompilerOptions {
                no_emit: true,
                ..CompilerOptions::default()
            },
        )
    };
    let cold = run(vec![declarations.clone(), use_site.clone()]);
    let warm = run(vec![declarations.clone(), use_site.clone()]);
    let reversed = run(vec![use_site, declarations]);

    for output in [&cold, &warm, &reversed] {
        assert_eq!(output.diagnostics, [], "{:?}", output.diagnostics);
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }
    assert_eq!(cold.stats.types, warm.stats.types);
    assert_eq!(cold.stats.types, reversed.stats.types);
}

#[test]
fn array_value_identity_projects_generated_function_method_in_strict_mode() {
    for source in [
        "Array['toString'];",
        "(Array)['toString'];",
        "const renamed=Array;renamed['toString'];",
        "const method:()=>string=Array['toString'];",
        "const rendered:string=Array['toString']();",
        "Array['toString']=()=>'';",
        "const renamed=Array;(renamed)['toString']=()=>'';",
    ] {
        let output = compile(source, CompilerOptions::default());
        assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source}"
        );
    }

    for source in [
        "Array['renamedMissing'];",
        "declare const key:string;Array[key];",
        "Array['toLocaleString'];",
        "const Array={};Array['toString'];",
    ] {
        let output = compile(source, CompilerOptions::default());
        assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{source}"
        );
    }
}

#[test]
fn array_function_member_defers_only_matching_global_augmentations() {
    for strict in [true, false] {
        for source in [
            concat!(
                "interface ArrayConstructor{toString():number}",
                "const expected:()=>number=Array['toString'];",
            ),
            concat!(
                "interface CallableFunction{toString():number}",
                "const renamed=Array;const expected:()=>number=renamed['toString'];",
            ),
            concat!(
                "interface Function{toString():number}",
                "const expected:()=>number=(Array)['toString'];",
            ),
            concat!(
                "interface Function{toString:()=>number}",
                "const expected:()=>number=Array['toString'];",
            ),
            concat!(
                "interface Function{'toString'():number}",
                "const expected:()=>number=Array['toString'];",
            ),
        ] {
            let output = compile(
                source,
                CompilerOptions {
                    strict,
                    ..CompilerOptions::default()
                },
            );
            assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Deferred,
                "strict={strict}: {source}"
            );
        }

        for source in [
            "interface ArrayConstructor{renamed():void}Array['toString'];",
            "interface CallableFunction{renamed():void}(Array)['toString'];",
            "interface Function{renamed():void}const alias=Array;alias['toString'];",
            "interface Object{toString():number}Array['toString'];",
            "interface RenamedFunction{toString():number}Array['toString'];",
        ] {
            let output = compile(
                source,
                CompilerOptions {
                    strict,
                    ..CompilerOptions::default()
                },
            );
            assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Complete,
                "strict={strict}: {source}"
            );
        }
    }
}

#[test]
fn array_function_augmentation_is_global_group_and_root_order_structural() {
    let augmentation = ("augmentation.ts", "interface Function{toString():number}");
    let consumer = (
        "consumer.ts",
        "const renamed=Array;const expected:()=>number=renamed['toString'];",
    );
    for sources in [vec![augmentation, consumer], vec![consumer, augmentation]] {
        let output = compile_files(&sources, CompilerOptions::default());
        assert_eq!(output.diagnostics, [], "{:?}", output.diagnostics);
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }

    for augmentation in [
        "interface Function{renamed():void}",
        "interface Object{toString():number}",
        "interface RenamedFunction{toString():number}",
        "export const marker=1;interface Function{toString():number}",
    ] {
        let sources = [
            ("augmentation.ts", augmentation),
            (
                "consumer.ts",
                "const renamed=Array;const expected:()=>string=renamed['toString'];",
            ),
        ];
        let output = compile_files(&sources, CompilerOptions::default());
        assert_eq!(
            output.diagnostics,
            [],
            "{augmentation}: {:?}",
            output.diagnostics
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{augmentation}"
        );
    }
}

#[test]
fn loose_array_value_lookup_and_unmodeled_constructor_calls_stay_bounded() {
    for source in [
        "Array['toString'];",
        "(Array)['renamedMissing'];",
        "const renamed=Array;renamed['toString'];",
    ] {
        let output = compile(
            source,
            CompilerOptions {
                strict: false,
                ..CompilerOptions::default()
            },
        );
        assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source}"
        );
    }

    for source in ["Array();", "new Array();"] {
        let output = compile(source, CompilerOptions::default());
        assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{source}"
        );
    }
}

#[test]
fn structurally_indexed_library_records_check_object_property_values() {
    let output = compile(
        "const table: Record<string, number> = { one: 'wrong' };",
        CompilerOptions::default(),
    );
    assert_eq!(codes(&output), vec![2322]);
    assert_eq!(
        output.diagnostics[0].message_text,
        "Type 'string' is not assignable to type 'number'."
    );

    let renamed = compile(
        "const flags:Record<string,boolean> = { ready:true, done:false }; \
         const broken:Record<string,boolean> = { ready:1 };",
        CompilerOptions::default(),
    );
    assert_eq!(codes(&renamed), vec![2322]);
    assert_eq!(
        renamed.diagnostics[0].message_text,
        "Type 'number' is not assignable to type 'boolean'."
    );

    let unsupported_key = compile(
        "const exact:Record<'item',number> = { item:'wrong' };",
        CompilerOptions::default(),
    );
    assert!(
        unsupported_key.diagnostics.is_empty(),
        "{:?}",
        unsupported_key.diagnostics
    );
}

#[test]
fn canonical_record_accepts_the_complete_property_key_domain() {
    // When the canonical pinned-library Record receives a key whose forced
    // type or declaration-owned constraint is wholly within PropertyKey, TS7
    // accepts the mapped key and TSZ completes that key-domain query.
    for source in [
        "type Key=string|symbol;type R=Record<Key,unknown>;",
        "type First=string|symbol;type Second=(First);type R=Record<Second,unknown>;",
        "type Wrapped=((string|number));type R=Record<Wrapped,unknown>;",
        "type R=Record<number,unknown>;",
        "type R=Record<symbol,unknown>;",
        "type R=Record<number|symbol,unknown>;",
        "type R=Record<string|number|symbol,unknown>;",
        "type R=Record<'ready'|1,unknown>;",
        "type R=Record<PropertyKey,unknown>;",
        "type R<Key extends string|symbol>=Record<Key,unknown>;",
        "type Keys=string|symbol;type R<Key extends Keys>=Record<Key,unknown>;",
        "type R<Key extends PropertyKey>=Record<Key,unknown>;",
        "type R<Key extends keyof any>=Record<Key,unknown>;",
    ] {
        let output = compile(source, CompilerOptions::default());
        assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source}: {:?}",
            output.stats
        );
    }
}

#[test]
fn canonical_record_reports_invalid_keys_and_defers_unowned_key_evaluation() {
    for (source, name) in [
        ("type R<Key>=Record<Key,unknown>;", "Key"),
        ("type R=Record<boolean,unknown>;", "boolean"),
        ("type R=Record<{},unknown>;", "{}"),
        ("type R=Record<unknown,unknown>;", "unknown"),
        ("type R=Record<string|boolean,unknown>;", "string | boolean"),
        ("type R=Record<bigint,unknown>;", "bigint"),
    ] {
        let output = compile(source, CompilerOptions::default());
        let [diagnostic] = output.diagnostics.as_slice() else {
            panic!("{source}: {:#?}", output.diagnostics);
        };
        assert_eq!(diagnostic.code, 2344, "{source}");
        assert_eq!(
            diagnostic.message_text,
            format!("Type '{name}' does not satisfy the constraint 'string | number | symbol'."),
            "{source}"
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source}"
        );
    }

    for source in [
        "type R=Record<Extract<string,'ready'>,unknown>;",
        "type R<Key>=Record<Key extends string?Key:never,unknown>;",
    ] {
        let output = compile(source, CompilerOptions::default());
        assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{source}"
        );
    }
}

#[test]
fn record_key_completion_is_identity_and_root_order_stable() {
    let singleton = compile(
        "interface Record<Key,Value>{authored:Key}declare const value:Record<string|symbol,unknown>;value.authored;",
        CompilerOptions::default(),
    );
    assert_eq!(codes(&singleton), vec![2300, 2300]);
    assert_eq!(
        singleton
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.file.as_str(),
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.as_str(),
            ))
            .collect::<Vec<_>>(),
        vec![
            ("case.ts", 10, 6, "Duplicate identifier 'Record'."),
            ("lib.es5.d.ts", 74202, 6, "Duplicate identifier 'Record'."),
        ]
    );
    assert_eq!(
        singleton.diagnostics[1].render(None),
        "lib.es5.d.ts(1611,6): error TS2300: Duplicate identifier 'Record'."
    );
    assert_eq!(singleton.semantic_completion, SemanticCompletion::Complete);

    let alias = compile(
        "type Record<Key,Value>={authored:Key};declare const value:Record<string,unknown>;value.authored;",
        CompilerOptions::default(),
    );
    assert_eq!(codes(&alias), vec![2300, 2300]);
    assert_eq!(alias.diagnostics[0].start, 5);
    assert_eq!(alias.diagnostics[1].start, 74202);
    assert_eq!(alias.semantic_completion, SemanticCompletion::Complete);

    let source_directive = compile(
        "// @ts-nocheck\ninterface Record<Key,Value>{authored:Key}",
        CompilerOptions::default(),
    );
    assert_eq!(codes(&source_directive), vec![2300]);
    assert_eq!(source_directive.diagnostics[0].file, "lib.es5.d.ts");

    let no_check = compile(
        "interface Record<Key,Value>{authored:Key}",
        CompilerOptions {
            no_check: true,
            ..CompilerOptions::default()
        },
    );
    assert_eq!(no_check.diagnostics, []);

    let local = compile(
        "export type Record<Key,Value>={authored:Key};type R=Record<string|symbol,unknown>;",
        CompilerOptions::default(),
    );
    assert_eq!(local.diagnostics, [], "{:?}", local.diagnostics);
    assert_eq!(local.semantic_completion, SemanticCompletion::Complete);

    for source in [
        "interface Ledger<Key,Value>{authored:Key}declare const value:Ledger<string,unknown>;value.authored;",
        "interface Array<T>{authored:T}",
    ] {
        let output = compile(source, CompilerOptions::default());
        assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }
    for source in [
        "interface Record<K,V>{first:K}interface Record<K,V>{second:V}declare const value:Record<string,unknown>;value.first;",
        "class Record<K,V>{authored!:K}declare const value:Record<string,unknown>;value.authored;",
    ] {
        let output = compile(source, CompilerOptions::default());
        assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{source}"
        );
    }
    let no_lib = compile(
        "interface Record<Key,Value>{authored:Key}",
        CompilerOptions {
            no_lib: true,
            ..CompilerOptions::default()
        },
    );
    assert!(!codes(&no_lib).contains(&2300), "{:?}", no_lib.diagnostics);

    let compiler = Compiler::new();
    let declarations = SourceInput::new(
        "models.ts",
        Arc::<str>::from("type EventKey=string|symbol;"),
    );
    let wrapper = SourceInput::new(
        "wrapper.ts",
        Arc::<str>::from("type Wrapped<E extends Record<EventKey,unknown>>=E;"),
    );
    let run = |files| {
        compiler.compile(
            files,
            &CompilerOptions {
                no_emit: true,
                ..CompilerOptions::default()
            },
        )
    };
    let cold = run(vec![declarations.clone(), wrapper.clone()]);
    let warm = run(vec![declarations.clone(), wrapper.clone()]);
    let reversed = run(vec![wrapper, declarations]);
    for output in [&cold, &warm, &reversed] {
        assert_eq!(output.diagnostics, [], "{:?}", output.diagnostics);
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }
    assert_eq!(cold.stats.types, warm.stats.types);
    assert_eq!(cold.stats.types, reversed.stats.types);

    let authored = SourceInput::new(
        "record.ts",
        Arc::<str>::from("interface Record<Key,Value>{authored:Key}"),
    );
    let use_site = SourceInput::new(
        "use.ts",
        Arc::<str>::from("declare const value:Record<string,unknown>;value.authored;"),
    );
    let cold = run(vec![authored.clone(), use_site.clone()]);
    let warm = run(vec![authored.clone(), use_site.clone()]);
    let reversed = run(vec![use_site, authored]);
    for output in [&cold, &warm, &reversed] {
        assert_eq!(codes(output), vec![2300, 2300], "{:?}", output.diagnostics);
        assert_eq!(output.diagnostics[0].file, "record.ts");
        assert_eq!(output.diagnostics[1].file, "lib.es5.d.ts");
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }
    assert_eq!(cold.stats.types, warm.stats.types);
    assert_eq!(cold.stats.types, reversed.stats.types);
}

#[test]
fn no_lib_contributes_no_ambient_symbols() {
    let output = compile(
        "const table: Record<string, number> = { one: 1 }; parseInt;",
        CompilerOptions {
            no_lib: true,
            ..CompilerOptions::default()
        },
    );
    assert_missing_essential_globals(&output);
    assert!(output.program.standard_library.declarations().is_empty());
    assert!(
        output
            .program
            .resolve_global("Record", Meaning::Type)
            .is_none()
    );
    assert!(
        output
            .program
            .resolve_global("parseInt", Meaning::Value)
            .is_none()
    );
}

#[test]
fn authored_global_types_satisfy_the_essential_environment_gate() {
    let output = compile(
        concat!(
            "interface Array<T> {}\n",
            "interface Boolean {}\n",
            "interface CallableFunction {}\n",
            "interface Function {}\n",
            "interface IArguments {}\n",
            "interface NewableFunction {}\n",
            "interface Number {}\n",
            "interface Object {}\n",
            "interface RegExp {}\n",
            "interface String {}\n",
            "const value: number = 'wrong';\n",
        ),
        CompilerOptions {
            no_lib: true,
            ..CompilerOptions::default()
        },
    );
    assert_eq!(codes(&output), vec![2322]);
}

#[test]
fn explicit_lib_replaces_the_default_full_library_set() {
    let output = compile(
        "const accepted = 1;",
        CompilerOptions {
            lib: Some(vec!["ES5".to_string()]),
            ..CompilerOptions::default()
        },
    );
    assert!(
        output
            .program
            .resolve_global("Record", Meaning::Type)
            .is_some()
    );
    assert!(
        output
            .program
            .resolve_global("parseInt", Meaning::Value)
            .is_some()
    );
    assert!(
        output
            .program
            .resolve_global("console", Meaning::Value)
            .is_none()
    );
    assert_eq!(
        output.program.standard_library.selected_libraries(),
        &["decorators", "decorators.legacy", "es5"]
    );
}

#[test]
fn target_selects_the_corresponding_default_full_library() {
    let es2025 = compile("const accepted = 1;", CompilerOptions::default());
    assert!(
        es2025
            .program
            .resolve_global("WeakRef", Meaning::Value)
            .is_some()
    );
    assert_eq!(
        es2025.program.standard_library.selected_libraries().last(),
        Some(&"es2025.full")
    );

    let es2021 = compile(
        "const accepted = 1;",
        CompilerOptions {
            target: "ES2021".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(
        es2021
            .program
            .resolve_global("WeakRef", Meaning::Value)
            .is_some()
    );
    assert_eq!(
        es2021.program.standard_library.selected_libraries().last(),
        Some(&"es2021.full")
    );
}

#[test]
fn removed_es5_target_is_fatal_before_semantic_checking() {
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "case.ts",
            Arc::<str>::from("const count: number = 'wrong';"),
        )],
        &CompilerOptions {
            target: "ES5".to_string(),
            no_emit_on_error: false,
            ..CompilerOptions::default()
        },
    );
    assert_eq!(codes(&output), vec![5108]);
    assert_eq!(
        output.diagnostics[0].message_text,
        "Option 'target=ES5' has been removed. Please remove it from your configuration."
    );
    assert_eq!(
        output.exit_status,
        tsz::CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );
    assert_eq!(output.stats.types, 0);
    assert!(output.emitted_files.is_empty());
}

#[test]
fn es3_remains_an_invalid_fatal_target_in_the_pinned_oracle() {
    let output = compile(
        "const count: number = 'wrong';",
        CompilerOptions {
            target: "ES3".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(codes(&output), vec![6046]);
    assert_eq!(
        output.diagnostics[0].message_text,
        concat!(
            "Argument for '--target' option must be: 'es6', 'es2015', 'es2016', ",
            "'es2017', 'es2018', 'es2019', 'es2020', 'es2021', 'es2022', ",
            "'es2023', 'es2024', 'es2025', 'esnext'."
        )
    );
    assert!(output.emitted_files.is_empty());
}

#[test]
fn explicit_feature_lib_uses_its_reference_closure_only() {
    let output = compile(
        "Promise; const table: Record<string, number> = { one: 1 };",
        CompilerOptions {
            lib: Some(vec!["es2015.promise".to_string()]),
            ..CompilerOptions::default()
        },
    );
    assert_missing_essential_globals(&output);
    assert!(
        output
            .program
            .resolve_global("Promise", Meaning::Value)
            .is_some()
    );
    assert!(
        output
            .program
            .resolve_global("Record", Meaning::Type)
            .is_none()
    );
    assert_eq!(
        output.program.standard_library.selected_libraries(),
        &["es2015.promise"]
    );
}

#[test]
fn declaration_identity_order_is_independent_of_explicit_root_order() {
    let declarations = |libraries: &[&str]| {
        let output = compile(
            "const accepted = 1;",
            CompilerOptions {
                lib: Some(libraries.iter().map(|name| (*name).to_string()).collect()),
                ..CompilerOptions::default()
            },
        );
        output
            .program
            .standard_library
            .declarations()
            .iter()
            .map(|declaration| {
                (
                    declaration.id.local,
                    declaration.name.clone(),
                    declaration.meaning,
                )
            })
            .collect::<Vec<_>>()
    };

    assert_eq!(
        declarations(&["dom", "es2015"]),
        declarations(&["es2015", "dom"])
    );
}
