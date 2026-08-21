use std::sync::Arc;

use tsz::bind::{DeclarationKind, Meaning, TypeMemberSymbol, bind_source};
use tsz::service::LanguageService;
use tsz::source::{FileId, SourceText};
use tsz::syntax::{StatementKind, TypeMemberKind, TypeMemberNameKind, TypeNodeKind, parse_source};
use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

fn compile(source: &str, strict: bool) -> tsz::CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            no_emit: true,
            strict,
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

fn parse(source: &str) -> (SourceText, tsz::syntax::ParseOutput) {
    let source = SourceText::new(FileId(0), "case.ts".into(), Arc::<str>::from(source));
    let parsed = parse_source(&source);
    (source, parsed)
}

fn sole_interface(parsed: &tsz::syntax::ParseOutput) -> &tsz::syntax::InterfaceDeclaration {
    let [statement] = parsed.unit.statements.as_slice() else {
        panic!("expected one interface");
    };
    let StatementKind::Interface(interface) = &statement.kind else {
        panic!("expected an interface, got {:?}", statement.kind);
    };
    interface
}

#[test]
fn parser_method_signature_1_through_12_keep_typed_names_and_generics() {
    // Inline copies of parserMethodSignature1-12 keep this rewrite test
    // hermetic when the ignored TypeScript oracle checkout is absent in CI.
    let cases = [
        "interface I { A(); }",
        "interface I { B?(); }",
        "interface I { C<T>(); }",
        "interface I { D?<T>(); }",
        "interface I { \"E\"(); }",
        "interface I { \"F\"?(); }",
        "interface I { \"G\"<T>(); }",
        "interface I { \"H\"?<T>(); }",
        "interface I { 0(); }",
        "interface I { 1?(); }",
        "interface I { 2<T>(); }",
        "interface I { 3?<T>(); }",
    ];

    for (index, source) in cases.into_iter().enumerate() {
        let (_, parsed) = parse(source);
        assert!(
            parsed.diagnostics.is_empty(),
            "case {}: {:?}",
            index + 1,
            parsed.diagnostics
        );
        let [member] = sole_interface(&parsed).members.as_slice() else {
            panic!("case {} did not retain one member", index + 1);
        };
        let TypeMemberKind::Method {
            name,
            optional,
            type_parameters,
            ..
        } = &member.kind
        else {
            panic!("case {} was not a method", index + 1);
        };
        assert_eq!(*optional, matches!(index + 1, 2 | 4 | 6 | 8 | 10 | 12));
        assert_eq!(
            !type_parameters.is_empty(),
            matches!(index + 1, 3 | 4 | 7 | 8 | 11 | 12)
        );
        match index + 1 {
            1..=4 => assert!(matches!(name.kind, TypeMemberNameKind::Identifier(_))),
            5..=8 => assert!(matches!(name.kind, TypeMemberNameKind::StringLiteral(_))),
            9..=12 => assert!(matches!(name.kind, TypeMemberNameKind::NumericLiteral(_))),
            _ => unreachable!(),
        }
    }
}

#[test]
fn one_ordered_member_list_preserves_all_bounded_variants() {
    let source = r#"interface Gateway<T> {
        readonly value: T;
        run<U>(arg: U): T;
        (input: string): number;
        new (seed: number): object;
        readonly [key: string]: T;
        get label(): string;
        set label(value: string);
    }"#;
    let (_, parsed) = parse(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let members = &sole_interface(&parsed).members;
    assert_eq!(members.len(), 7);
    assert!(matches!(members[0].kind, TypeMemberKind::Property { .. }));
    assert!(matches!(members[1].kind, TypeMemberKind::Method { .. }));
    assert!(matches!(members[2].kind, TypeMemberKind::Call { .. }));
    assert!(matches!(members[3].kind, TypeMemberKind::Construct { .. }));
    assert!(matches!(members[4].kind, TypeMemberKind::Index { .. }));
    assert!(matches!(members[5].kind, TypeMemberKind::Accessor { .. }));
    assert!(matches!(members[6].kind, TypeMemberKind::Accessor { .. }));
    assert!(members[0].modifiers.readonly);
    assert!(members[4].modifiers.readonly);
    assert!(
        members
            .windows(2)
            .all(|pair| pair[0].span.end <= pair[1].span.start)
    );
}

#[test]
fn parser_index_signature_1_through_11_match_pinned_diagnostic_codes() {
    // Inline parserIndexSignature1-11 witnesses, pinned by
    // scripts/conformance/tsc-cache-full.json.
    let cases = [
        ("interface I { [...a] }", vec![1017]),
        ("interface I { [public a] }", vec![2369, 1018]),
        ("interface I { [a?] }", vec![1019]),
        ("interface I { [a = 0] }", vec![1169, 2304]),
        ("interface I { [a] }", vec![2304]),
        ("interface I { [a:boolean] }", vec![1268]),
        ("interface I { [a:string] }", vec![1021]),
        (
            "let first:{[index:any];}; let second:{[index:RegExp];};",
            vec![1268, 1268],
        ),
        ("interface I { []:number }", vec![1096]),
        ("interface I { [a,b]:number }", vec![1096]),
        (
            "interface I { [p]; [p1:string]; [p2:string,p3:number]; }",
            vec![2304, 1021, 1096],
        ),
    ];

    for (index, (source, expected)) in cases.into_iter().enumerate() {
        let output = compile(source, false);
        assert_eq!(
            codes(&output),
            expected,
            "case {}: {:?}",
            index + 1,
            output.diagnostics
        );
    }
}

#[test]
fn index_modifier_diagnostics_retain_exact_token_provenance() {
    let source = "interface I { [public a] }";
    let output = compile(source, false);
    let [parameter_property, accessibility] = output.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:?}", output.diagnostics);
    };
    assert_eq!(
        (parameter_property.code, parameter_property.length),
        (2369, 8)
    );
    assert_eq!(
        &source[parameter_property.start as usize
            ..(parameter_property.start + parameter_property.length) as usize],
        "public a"
    );
    assert_eq!((accessibility.code, accessibility.length), (1018, 1));
    assert_eq!(
        &source
            [accessibility.start as usize..(accessibility.start + accessibility.length) as usize],
        "a"
    );
}

#[test]
fn call_signatures_without_annotations_are_any_in_non_strict_mode() {
    let source = "function foo(x){} let r=foo(1); interface I { (); f(); } let i:I; let r2=i(); let r3=i.f(); let a:{();f();}; let r4=a(); let r5=a.f();";
    let output = compile(source, false);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn overload_optional_mismatches_report_each_deviating_member() {
    let source = "let c:{func4?(x:number):number;func4(s:string):string;}; let c2:{func4<T>(x:T):number;func4?<T>(s:T):string;};";
    let output = compile(source, false);
    assert_eq!(codes(&output), vec![2386, 2386]);
    for diagnostic in &output.diagnostics {
        assert_eq!(diagnostic.length, 5);
        assert_eq!(
            &source[diagnostic.start as usize..(diagnostic.start + diagnostic.length) as usize],
            "func4"
        );
        assert_eq!(
            diagnostic.message_text,
            "Overload signatures must all be optional or required."
        );
    }
}

#[test]
fn duplicate_signature_parameters_are_reported_source_forward_once() {
    let source = "interface D { m(a:number,a:string,b:boolean,b:number):void; (c:number,c:string):void; new(d:number,d:string):object; } let value:D;";
    let output = compile(source, false);
    assert_eq!(codes(&output), vec![2300; 8], "{:?}", output.diagnostics);
    assert!(
        output
            .diagnostics
            .windows(2)
            .all(|pair| pair[0].start < pair[1].start)
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn rest_array_likeness_uses_aliases_and_wrappers_without_false_success() {
    let source = r#"
        type Words = string[];
        type TextValue = string;
        interface RestCases {
            aliasArray(...a: Words): void;
            aliasString(...b: TextValue): void;
            readonlyArray(...c: readonly string[]): void;
            intersection(...d: string[] & { tag?: 1 }): void;
            unionArrays(...e: string[] | number[]): void;
            mixedUnion(...f: string[] | string): void;
            globalArray(...g: Array<string>): void;
            globalReadonly(...h: ReadonlyArray<string>): void;
            bottom(...i: never): void;
            generic<T>(...j: T): void;
        }
        let restCases: RestCases;
    "#;
    let output = compile(source, false);
    assert_eq!(codes(&output), vec![2370, 2370], "{:?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);

    let optional = compile(
        "interface O { a(...x?:any):void; b(...y?:string[]):void; c(...z?:string):void } let o:O;",
        true,
    );
    assert_eq!(codes(&optional), vec![1047, 2370, 1047, 2370, 1047]);
    assert_eq!(optional.semantic_completion, SemanticCompletion::Deferred);

    // TS1014 (rest must be last) is not yet owned by this slice, but the
    // affected signature remains a typed nonclaim rather than false success.
    let rest_not_last = compile(
        "interface Last { run(first:number,...items:string[],last:number):void } let value:Last;",
        true,
    );
    assert!(!codes(&rest_not_last).contains(&1014));
    assert_eq!(
        rest_not_last.semantic_completion,
        SemanticCompletion::Deferred
    );
}

#[test]
fn index_shapes_complete_only_at_the_bounded_string_key_boundary() {
    let source =
        "interface Both { [name:string]:unknown; [position:number]:number } let both:Both;";
    let output = compile(source, true);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);

    let string_only = compile(
        "declare let bag:{[name:string]:number}; let value:number=bag.answer;",
        true,
    );
    assert!(
        string_only.diagnostics.is_empty(),
        "{:?}",
        string_only.diagnostics
    );
    assert_eq!(
        string_only.semantic_completion,
        SemanticCompletion::Complete
    );

    let number_only = compile(
        "declare let numbers:{[position:number]:number}; let value=numbers[0];",
        true,
    );
    assert!(
        number_only.diagnostics.is_empty(),
        "{:?}",
        number_only.diagnostics
    );
    assert_eq!(
        number_only.semantic_completion,
        SemanticCompletion::Deferred
    );

    let duplicate = compile(
        "interface Duplicate { [first:string]:number; [second:string]:string } let duplicate:Duplicate;",
        true,
    );
    assert!(
        duplicate.diagnostics.is_empty(),
        "{:?}",
        duplicate.diagnostics
    );
    assert_eq!(duplicate.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn member_scopes_own_typeof_parameters_and_do_not_capture_outer_names() {
    for (outer, parameter) in [("value", "value"), ("outside", "candidate")] {
        let source = format!(
            "const {outer}=1; interface I {{ m({parameter}:string): typeof {parameter}; }}"
        );
        let (source_text, parsed) = parse(&source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let interface = parsed
            .unit
            .statements
            .iter()
            .find_map(|statement| match &statement.kind {
                StatementKind::Interface(interface) => Some(interface),
                _ => None,
            })
            .expect("interface");
        let [member] = interface.members.as_slice() else {
            panic!("method member");
        };
        let TypeMemberKind::Method {
            parameters,
            return_type: Some(return_type),
            ..
        } = &member.kind
        else {
            panic!("method signature");
        };
        let TypeNodeKind::TypeQuery { name, .. } = &return_type.kind else {
            panic!("typeof return");
        };
        assert_eq!(name, parameter);

        let bound = bind_source(source_text.id, &parsed.unit);
        let member_scope = bound.scope_for_node[&member.id];
        let parameter_declaration = bound
            .declarations
            .iter()
            .find(|declaration| {
                declaration.owner == member.id
                    && declaration.kind == DeclarationKind::Parameter
                    && declaration.name == parameters[0].name
            })
            .expect("parameter declaration");
        assert_eq!(
            bound.resolve(member_scope, name, Meaning::Value),
            Some(parameter_declaration.id)
        );
        let outer_declaration = bound
            .declarations
            .iter()
            .find(|declaration| declaration.kind == DeclarationKind::Variable)
            .expect("outer declaration");
        assert_ne!(parameter_declaration.id, outer_declaration.id);
    }

    for source in [
        "type F=(value:string)=>typeof value;",
        "type Renamed=(candidate:string)=>typeof candidate;",
        "type Generic=<T>(item:T)=>typeof item;",
        "type Built=new (seed:string)=>typeof seed;",
    ] {
        let output = compile(source, true);
        assert!(
            output.diagnostics.is_empty(),
            "{source}: {:?}",
            output.diagnostics
        );
        assert_eq!(
            output.semantic_completion,
            if source.contains("<T>") {
                SemanticCompletion::Deferred
            } else {
                SemanticCompletion::Complete
            },
            "wrong anonymous signature completion for {source}"
        );
    }
}

#[test]
fn signature_constraints_use_the_enclosing_value_scope() {
    let source = concat!(
        "function named<T extends typeof namedArg>(namedArg:string):void {}\n",
        "class Host { method<U extends typeof methodArg>(methodArg:string):void {} }\n",
        "declare function ambient<V extends typeof ambientArg>(ambientArg:string):void;\n",
        "type FunctionAlias=<W extends typeof functionArg>(functionArg:string)=>void;\n",
        "type ConstructorAlias=new <X extends typeof constructArg>(constructArg:string)=>unknown;\n",
        "interface Contract { method<Y extends typeof interfaceArg>(interfaceArg:string):void; }\n",
        "type Literal={ <Z extends typeof callArg>(callArg:string):void };\n",
    );
    let output = compile(source, true);
    assert_eq!(codes(&output), vec![2304; 7], "{:?}", output.diagnostics);
    let expected_names = [
        "namedArg",
        "methodArg",
        "ambientArg",
        "functionArg",
        "constructArg",
        "interfaceArg",
        "callArg",
    ];
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| {
                &source[diagnostic.start as usize..(diagnostic.start + diagnostic.length) as usize]
            })
            .collect::<Vec<_>>(),
        expected_names
    );

    for source in [
        "type Sibling=(first:typeof second,second:string)=>typeof first;",
        "type Renamed=(candidate:typeof following,following:number)=>typeof candidate;",
    ] {
        let output = compile(source, true);
        assert!(
            output.diagnostics.is_empty(),
            "{source}: {:?}",
            output.diagnostics
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }

    for source in [
        "type F<T>=(value:T)=>typeof value; type S=F<string>; type N=F<number>;",
        "type F<T>=(candidate:T)=>typeof candidate; type N=F<number>; type S=F<string>;",
    ] {
        for _ in 0..2 {
            let output = compile(source, true);
            assert!(
                output.diagnostics.is_empty(),
                "{source}: {:?}",
                output.diagnostics
            );
            assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        }
    }
}

#[test]
fn type_parameter_defaults_only_reference_prior_parameters() {
    let source = concat!(
        "type Alias<T=U,U=string>=T;\n",
        "interface Contract<T=U,U=string>{}\n",
        "class Host<T=U,U=string>{}\n",
        "function named<T=U,U=string>():void {}\n",
        "declare function ambient<T=U,U=string>():void;\n",
        "type FunctionAlias=<T=U,U=string>()=>void;\n",
        "type ConstructorAlias=new <T=U,U=string>()=>unknown;\n",
        "interface Members{method<T=U,U=string>():void}\n",
        "type Calls={<T=U,U=string>():void};\n",
        "type Constructs={new <T=U,U=string>():unknown};\n",
    );
    let output = compile(source, true);
    assert_eq!(codes(&output), vec![2744; 10], "{:?}", output.diagnostics);
    assert!(output.diagnostics.iter().all(|diagnostic| {
        diagnostic.length == 1
            && &source[diagnostic.start as usize..(diagnostic.start + diagnostic.length) as usize]
                == "U"
            && diagnostic.message_text
                == "Type parameter defaults can only reference previously declared type parameters."
    }));

    let applied = "type Applied<T=U<string>,U=number>=T;";
    let output = compile(applied, true);
    assert_eq!(codes(&output), vec![2315], "{:?}", output.diagnostics);
    let diagnostic = &output.diagnostics[0];
    assert_eq!(
        &applied[diagnostic.start as usize..(diagnostic.start + diagnostic.length) as usize],
        "U<string>"
    );
    assert_eq!(diagnostic.message_text, "Type 'U' is not generic.");

    let nested = "type Box<Value>=Value; type Nested<T=Box<U>,U=string>=T;";
    let output = compile(nested, true);
    assert_eq!(codes(&output), vec![2744, 2744], "{:?}", output.diagnostics);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| {
                &nested[diagnostic.start as usize..(diagnostic.start + diagnostic.length) as usize]
            })
            .collect::<Vec<_>>(),
        vec!["Box<U>", "U"]
    );

    for source in [
        "type ForwardConstraint<T extends U,U=string>=T;",
        "type PriorDefault<T=string,U=T>=U;",
        "type RenamedConstraint<First extends Second,Second=number>=First;",
    ] {
        let output = compile(source, true);
        assert!(
            output.diagnostics.is_empty(),
            "{source}: {:?}",
            output.diagnostics
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }
}

#[test]
fn quick_info_fails_closed_for_unmerged_or_noncanonical_type_members() {
    for (source, name) in [
        (
            "var container:{func4?(x:number):number;func4(s:string):string;};",
            "container",
        ),
        (
            r#"declare let names:{"\x61":number;"a":number;0:string;0x0:string};"#,
            "names",
        ),
        ("declare let callable:{(value:string):number};", "callable"),
        (
            "declare let mapped:{[K in keyof string]:string;extra:number};",
            "mapped",
        ),
    ] {
        let mut service = LanguageService::new(CompilerOptions::default());
        service.open("case.ts", Arc::<str>::from(source));
        let offset = source.find(name).expect("declaration name") as u32;
        assert!(
            service.quick_info("case.ts", offset + 1).is_none(),
            "unsupported member shape received confident quickinfo: {source}"
        );
    }
}

#[test]
fn navigation_keeps_type_member_signature_locals_separate_from_outer_names() {
    let cases = [
        (
            "type MethodType=string; const methodValue=0; interface I{method<MethodType>(methodValue:MethodType):[MethodType,typeof methodValue]}",
            "MethodType",
            "methodValue",
        ),
        (
            "type CallType=string; const callValue=0; type Calls={<CallType>(callValue:CallType):[CallType,typeof callValue]};",
            "CallType",
            "callValue",
        ),
        (
            "type BuildType=string; const buildValue=0; type Builds={new <BuildType>(buildValue:BuildType):[BuildType,typeof buildValue]};",
            "BuildType",
            "buildValue",
        ),
        (
            "type FunctionType=string; const functionValue=0; type FunctionAlias=<FunctionType>(functionValue:FunctionType)=>[FunctionType,typeof functionValue];",
            "FunctionType",
            "functionValue",
        ),
        (
            "type ConstructorType=string; const constructorValue=0; type ConstructorAlias=new <ConstructorType>(constructorValue:ConstructorType)=>[ConstructorType,typeof constructorValue];",
            "ConstructorType",
            "constructorValue",
        ),
    ];
    for (source, type_name, value_name) in cases {
        let mut service = LanguageService::new(CompilerOptions::default());
        service.open("case.ts", Arc::<str>::from(source));

        let type_positions = source
            .match_indices(type_name)
            .map(|(position, _)| position as u32)
            .collect::<Vec<_>>();
        assert_eq!(type_positions.len(), 4, "{source}");
        for reference in &type_positions[2..] {
            let definition = service
                .definition_and_bound_span("case.ts", *reference + 1)
                .expect("inner type-parameter definition");
            assert_eq!(definition.definitions[0].text_span.start, type_positions[1]);
        }
        assert_eq!(
            service.references("case.ts", type_positions[1] + 1)[0]
                .references
                .len(),
            3
        );
        assert_eq!(
            service
                .rename("case.ts", type_positions[1] + 1)
                .locations
                .len(),
            3
        );
        assert_eq!(
            service.references("case.ts", type_positions[0] + 1)[0]
                .references
                .len(),
            1
        );

        let value_positions = source
            .match_indices(value_name)
            .map(|(position, _)| position as u32)
            .collect::<Vec<_>>();
        assert_eq!(value_positions.len(), 3, "{source}");
        let definition = service
            .definition_and_bound_span("case.ts", value_positions[2] + 1)
            .expect("signature parameter definition");
        assert_eq!(
            definition.definitions[0].text_span.start,
            value_positions[1]
        );
        assert_eq!(
            service.references("case.ts", value_positions[1] + 1)[0]
                .references
                .len(),
            2
        );
        assert_eq!(
            service
                .rename("case.ts", value_positions[1] + 1)
                .locations
                .len(),
            2
        );
        assert_eq!(
            service.references("case.ts", value_positions[0] + 1)[0]
                .references
                .len(),
            1
        );
    }
}

#[test]
fn navigation_keeps_container_generics_and_retained_initializers_scoped() {
    for (source, name) in [
        ("type T=string;interface I<T>{m(x:T):T}", "T"),
        ("type U=string;type Box<U>={m(x:U):U}", "U"),
    ] {
        let mut service = LanguageService::new(CompilerOptions::default());
        service.open("case.ts", Arc::<str>::from(source));
        let positions = source
            .match_indices(name)
            .map(|(position, _)| position as u32)
            .collect::<Vec<_>>();
        assert_eq!(positions.len(), 4, "{source}");
        for reference in &positions[2..] {
            let definition = service
                .definition_and_bound_span("case.ts", *reference + 1)
                .expect("container type-parameter definition");
            assert_eq!(definition.definitions[0].text_span.start, positions[1]);
        }
        assert_eq!(
            service.references("case.ts", positions[1] + 1)[0]
                .references
                .len(),
            3
        );
        assert_eq!(
            service.references("case.ts", positions[0] + 1)[0]
                .references
                .len(),
            1
        );
    }

    let source = "const seed=1;interface I{x?:number=seed;m(value=seed):void}type F=(arg=seed)=>void;declare function f(input=seed):void;";
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open("case.ts", Arc::<str>::from(source));
    let positions = source
        .match_indices("seed")
        .map(|(position, _)| position as u32)
        .collect::<Vec<_>>();
    assert_eq!(positions.len(), 5);
    for reference in &positions[1..] {
        let definition = service
            .definition_and_bound_span("case.ts", *reference + 1)
            .expect("initializer definition");
        assert_eq!(definition.definitions[0].text_span.start, positions[0]);
    }
    assert_eq!(
        service.references("case.ts", positions[0] + 1)[0]
            .references
            .len(),
        5
    );
    assert_eq!(
        service.rename("case.ts", positions[0] + 1).locations.len(),
        5
    );
}

#[test]
fn callable_aliases_optional_and_noncanonical_names_are_explicit_nonclaims() {
    let cases = [
        "type Fn=(argument:number)=>string; interface I { invoke:Fn } let value:I;",
        "type Fn=(candidate:number)=>string; interface I { invoke:Fn } let value:I;",
        "interface I { optional?:number } let value:I;",
        "interface I { optional?():void } let value:I;",
        "interface I { get value():number; set value(next:number); } let item:I;",
        "interface I { 'quoted'():void; 1():void } let value:I;",
        "declare const key:symbol; interface I { [key]:number } let value:I;",
        "interface I { run(x:number):void; run(x:string):void } let value:I;",
    ];
    for source in cases {
        let output = compile(source, false);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "false Complete for {source:?}: {:?}",
            output.diagnostics
        );
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    }
}

#[test]
fn binder_groups_members_without_magic_string_collisions() {
    let source = "interface G { run(x:number):void; run(x:string):void; ():void; new():object; [x:string]:unknown; __call:number }";
    let (source_text, parsed) = parse(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let members = &sole_interface(&parsed).members;
    let bound = bind_source(source_text.id, &parsed.unit);
    assert_eq!(
        bound.type_member_group(members[0].id).map(<[_]>::len),
        Some(2)
    );
    assert_eq!(
        bound.canonical_type_member_declaration(members[0].id),
        bound.canonical_type_member_declaration(members[1].id)
    );
    assert!(matches!(
        bound.type_members[&members[2].id].symbol,
        Some(TypeMemberSymbol::Call)
    ));
    assert!(matches!(
        bound.type_members[&members[5].id].symbol,
        Some(TypeMemberSymbol::Named(ref name)) if name == "__call"
    ));
}

#[test]
fn declaration_emit_keeps_member_source_order_and_authored_parameter_names() {
    let source = "export interface Mixed { z:boolean; method(value:number):string; (text:string):number; new(seed:number):object; readonly [key:string]:unknown; a:number }";
    let output = Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            declaration: true,
            target: "es2015".to_string(),
            module: "commonjs".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let declaration = output
        .emitted_files
        .iter()
        .find(|file| file.declaration)
        .expect("declaration output");
    for member in [
        "z: boolean;",
        "method(value: number): string;",
        "(text: string): number;",
        "new (seed: number): object;",
        "readonly [key: string]: unknown;",
        "a: number;",
    ] {
        assert!(
            declaration.text.contains(member),
            "missing {member:?}: {}",
            declaration.text
        );
    }
    let positions = ["z:", "method(", "(text:", "new (", "readonly [", "a:"].map(|needle| {
        declaration
            .text
            .find(needle)
            .expect("member in declaration")
    });
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn index_rest_keeps_ts1017_and_only_the_applicable_ts2370() {
    let source = r#"
        type AliasAny = any;
        interface I {
            [...a]: void;
            [...b: string]: void;
            [...c?: string[]]: void;
            [...d?: never]: void;
            [...e?: AliasAny]: void;
        }
    "#;
    let output = Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            no_emit: true,
            strict: true,
            no_implicit_any: Some(false),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(
        codes(&output),
        vec![1017, 1017, 2370, 1017, 2370, 1017, 2370, 1017],
        "{:?}",
        output.diagnostics
    );
    assert!(!codes(&output).contains(&1047));

    let strict = compile(source, true);
    assert_eq!(codes(&strict)[..2], [1017, 7019]);
    let implicit_any = strict
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == 7019)
        .expect("rest implicit any");
    assert_eq!(
        &source[implicit_any.start as usize..(implicit_any.start + implicit_any.length) as usize],
        "...a"
    );

    let ordinary = compile(
        "type AliasAny=any; interface O { run(...value?:AliasAny):void }",
        true,
    );
    assert_eq!(codes(&ordinary), vec![1047]);
}

#[test]
fn diagnosed_error_children_remain_complete_error_cascades() {
    for (source, expected) in [
        ("type O={x:Missing}; let value:O;", vec![2304]),
        (
            "type O={x:{present:number}[\"missing\"]}; let value:O;",
            vec![2339],
        ),
    ] {
        let output = compile(source, true);
        assert_eq!(codes(&output), expected, "{:?}", output.diagnostics);
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(
            output.exit_status,
            CompileExitStatus::DiagnosticsPresentOutputsSkipped
        );
        assert!(!codes(&output).contains(&2370));
    }

    for (source, expected) in [
        ("declare function missing(...x:Missing):void;", vec![2304]),
        (
            "declare function projected(...x:{present:number}[\"missing\"]):void;",
            vec![2339],
        ),
    ] {
        let rest_errors = compile(source, true);
        assert_eq!(codes(&rest_errors), expected);
        assert_eq!(
            rest_errors.semantic_completion,
            SemanticCompletion::Complete,
            "false incomplete rest error cascade for {source}"
        );
        assert!(!codes(&rest_errors).contains(&2370));
    }
}

#[test]
fn recursive_object_skeletons_complete_and_projection_recursion_is_explicit() {
    for source in [
        "type SelfNode={next:SelfNode}; let node:SelfNode;",
        "type LeftNode={right:RightNode}; type RightNode={left:LeftNode}; let left:LeftNode;",
        "type RightNode={left:LeftNode}; type LeftNode={right:RightNode}; let right:RightNode;",
    ] {
        let output = compile(source, true);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }

    for source in [
        "interface Left{next:Left;tag:string}interface Right{next:Right;tag:number}declare let actual:Right;let rejected:Left=actual;",
        "type Left={next:Left;tag:string};type Right={next:Right;tag:number};declare let actual:Right;let rejected:Left=actual;",
    ] {
        let output = compile(source, true);
        assert_eq!(codes(&output), vec![2322]);
        assert!(
            output.diagnostics[0]
                .message_text
                .starts_with("Type 'Right' is not assignable to type 'Left'.")
        );
    }

    // Key/index projection recursion still lacks TS7's productive-vs-TS2502
    // owner. Both forms fail closed instead of becoming a cached shape.
    for source in [
        "type KeyLoop={p:keyof KeyLoop}; let value:KeyLoop;",
        "type PickForward={p:PickForward[\"q\"];q:string}; let value:PickForward;",
        "type PickCycle={p:PickCycle[\"p\"]}; let value:PickCycle;",
    ] {
        let output = compile(source, true);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "false Complete for {source}: {:?}",
            output.diagnostics
        );
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    }
}

#[test]
fn method_shape_relations_defer_until_display_provenance_is_modeled() {
    for (source, expected) in [
        (
            "function convert(value:{m(value:string):string}):{m(candidate:string):string}{return value}",
            SemanticCompletion::Complete,
        ),
        (
            "function convert(value:{m(value:string):string}):{m(candidate:string):number}{return value}",
            SemanticCompletion::Deferred,
        ),
    ] {
        let output = compile(source, true);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion, expected,
            "wrong completion for {source}: {:?}",
            output.diagnostics
        );
        assert_eq!(
            output.exit_status,
            if expected == SemanticCompletion::Complete {
                CompileExitStatus::Success
            } else {
                CompileExitStatus::SemanticIncomplete
            }
        );
    }

    let extraction = compile(
        "declare function accept(wanted:(wanted:number)=>string):void; declare const container:{method(authored:string):number}; accept(container.method);",
        true,
    );
    assert!(
        extraction.diagnostics.is_empty(),
        "synthetic callable diagnostic leaked: {:?}",
        extraction.diagnostics
    );
    assert_eq!(extraction.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        extraction.exit_status,
        CompileExitStatus::SemanticIncomplete
    );
}

#[test]
fn generic_function_properties_and_mapped_trailing_members_recover_structurally() {
    let (_, mapped) = parse("type M<T> = { [K in keyof T]?: T[K] }");
    assert!(mapped.diagnostics.is_empty(), "{:?}", mapped.diagnostics);
    let StatementKind::TypeAlias(alias) = &mapped.unit.statements[0].kind else {
        panic!("mapped alias");
    };
    assert!(matches!(alias.ty.kind, TypeNodeKind::Mapped { .. }));

    let generic_source = "type F<X> = {invoke:<Y>(arg:Y)=>[X,Y]}";
    let (_, parsed) = parse(generic_source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let StatementKind::TypeAlias(alias) = &parsed.unit.statements[0].kind else {
        panic!("generic function property alias");
    };
    let TypeNodeKind::Object(members) = &alias.ty.kind else {
        panic!("generic function property object");
    };
    let TypeMemberKind::Property { ty: Some(ty), .. } = &members[0].kind else {
        panic!("generic function property");
    };
    let TypeNodeKind::Function {
        type_parameters,
        parameters,
        ..
    } = &ty.kind
    else {
        panic!("generic FunctionType");
    };
    assert_eq!(type_parameters.len(), 1);
    assert_eq!(parameters.len(), 1);
    let output = compile(generic_source, true);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);

    for source in [
        "type First<X>={invoke:<Y extends unknown[]>(...args:Y)=>[X,Y]}; let first:First<string>;",
        "type Second<A>={invoke:<B extends any[]>(...items:B)=>[A,B]}; let second:Second<number>;",
    ] {
        let output = compile(source, true);
        assert!(
            output.diagnostics.is_empty(),
            "{source}: {:?}",
            output.diagnostics
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }

    let mapped_source = "type B<T> = { [K in keyof T]: T[K]; extra:string }";
    let (_, parsed) = parse(mapped_source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let StatementKind::TypeAlias(alias) = &parsed.unit.statements[0].kind else {
        panic!("mapped alias");
    };
    let TypeNodeKind::Mapped { members, .. } = &alias.ty.kind else {
        panic!("mapped type");
    };
    assert_eq!(members.len(), 1);
    assert!(matches!(members[0].kind, TypeMemberKind::Property { .. }));
    let output = compile(mapped_source, false);
    assert_eq!(codes(&output), vec![7061]);
    let diagnostic = &output.diagnostics[0];
    assert_eq!(
        &mapped_source[diagnostic.start as usize..(diagnostic.start + diagnostic.length) as usize],
        "extra"
    );
    assert_eq!(
        diagnostic.message_text,
        "A mapped type may not declare properties or methods."
    );

    for (source, offending) in [
        ("type A<T>={ [K in keyof T]:T[K]; extra }", "extra"),
        (
            "type A<T>={ [K in keyof T]:T[K]; run(x):any }",
            "run(x):any",
        ),
        (
            "type A<T>={ [K in keyof T]:T[K]; [x?:string]:any }",
            "[x?:string]:any",
        ),
        (
            "type A<T>={ [K in keyof T]:T[K]; extra:string; other:number }",
            "extra",
        ),
    ] {
        let output = compile(source, true);
        assert_eq!(
            codes(&output),
            vec![7061],
            "{source}: {:?}",
            output.diagnostics
        );
        let diagnostic = &output.diagnostics[0];
        assert_eq!(
            &source[diagnostic.start as usize..(diagnostic.start + diagnostic.length) as usize],
            offending
        );
    }
}

#[test]
fn computed_assignment_uses_the_authored_container_diagnostic() {
    for (source, expected_code) in [
        ("interface I { [missing = 0]: string }", 1169),
        ("type T = { [missing = 0]: string }", 1170),
    ] {
        let output = compile(source, false);
        assert_eq!(codes(&output), vec![expected_code, 2304]);
        let computed = &output.diagnostics[0];
        assert_eq!(
            &source[computed.start as usize..(computed.start + computed.length) as usize],
            "[missing = 0]"
        );
        assert!(computed.message_text.contains(if expected_code == 1169 {
            "in an interface"
        } else {
            "in a type literal"
        }));
    }
}

#[test]
fn strict_unannotated_type_members_report_exact_implicit_any_diagnostics() {
    let source = concat!(
        "interface I {\n",
        "  (value);\n",
        "  method(input);\n",
        "  new (item);\n",
        "  property;\n",
        "}",
    );
    let output = compile(source, true);
    assert_eq!(
        codes(&output),
        vec![7020, 7006, 7010, 7006, 7013, 7006, 7008]
    );
    let slices = output
        .diagnostics
        .iter()
        .map(|diagnostic| {
            &source[diagnostic.start as usize..(diagnostic.start + diagnostic.length) as usize]
        })
        .collect::<Vec<_>>();
    assert_eq!(
        slices,
        vec![
            "(value);",
            "value",
            "method(input);",
            "input",
            "new (item);",
            "item",
            "property",
        ]
    );
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message_text.as_str())
            .collect::<Vec<_>>(),
        vec![
            "Call signature, which lacks return-type annotation, implicitly has an 'any' return type.",
            "Parameter 'value' implicitly has an 'any' type.",
            "'method', which lacks return-type annotation, implicitly has an 'any' return type.",
            "Parameter 'input' implicitly has an 'any' type.",
            "Construct signature, which lacks return-type annotation, implicitly has an 'any' return type.",
            "Parameter 'item' implicitly has an 'any' type.",
            "Member 'property' implicitly has an 'any' type.",
        ]
    );
}

#[test]
fn declaration_emit_uses_any_for_unannotated_type_members() {
    let source = "export function foo(x){} export interface I{(value);f();g(input)} export let inline:{(candidate);method(argument)};";
    let output = Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            declaration: true,
            strict: false,
            target: "es2015".to_string(),
            module: "commonjs".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let declaration = output
        .emitted_files
        .iter()
        .find(|file| file.declaration)
        .expect("declaration output");
    assert_eq!(
        declaration.text,
        concat!(
            "export declare function foo(x: any): void;\n",
            "export interface I {\n",
            "    (value: any): any;\n",
            "    f(): any;\n",
            "    g(input: any): any;\n",
            "}\n",
            "export declare let inline: {\n",
            "    (candidate: any): any;\n",
            "    method(argument: any): any;\n",
            "};\n",
        )
    );
}

#[test]
fn retained_property_and_parameter_initializers_have_one_grammar_owner() {
    for (source, code, initializer) in [
        ("interface I { property:string=\"x\" }", 1246, "\"x\""),
        ("type T={property:string=\"x\"}", 1247, "\"x\""),
    ] {
        let output = compile(source, false);
        assert_eq!(codes(&output), vec![code], "{:?}", output.diagnostics);
        let diagnostic = &output.diagnostics[0];
        assert_eq!(
            &source[diagnostic.start as usize..(diagnostic.start + diagnostic.length) as usize],
            initializer
        );
    }

    for (source, code) in [
        ("interface I { property:string=missing }", 1246),
        ("type T={property:string=missing}", 1247),
    ] {
        let output = compile(source, false);
        assert_eq!(codes(&output), vec![code, 2304]);
        assert!(output.diagnostics.iter().all(|diagnostic| {
            &source[diagnostic.start as usize..(diagnostic.start + diagnostic.length) as usize]
                == "missing"
        }));
    }

    let signatures = compile(
        "type F=(x:number=1)=>void; type C=new(x:number=1)=>object; interface I{m(x:number=1):void;(x:number=1):void;new(x:number=1):object}",
        false,
    );
    assert_eq!(codes(&signatures), vec![2371; 5]);

    let callable = compile(
        "type F=(x=1)=>void; declare let f:F; f(); f(2); f(\"bad\");",
        true,
    );
    assert_eq!(
        codes(&callable),
        vec![2371, 2345],
        "{:?}",
        callable.diagnostics
    );

    let missing = compile("type F=(x=missing)=>void; declare let f:F;", true);
    assert_eq!(codes(&missing), vec![2371]);
    assert_eq!(missing.semantic_completion, SemanticCompletion::Deferred);

    let index = compile("interface I{[x:string=\"\"]:number}", false);
    assert_eq!(codes(&index), vec![1020, 2371]);

    let combinations = compile(
        "type Q=(x?:number=1)=>void; type R=(...x:number[]=[])=>void; function implemented(x?:number=1,...rest:number[]){}",
        false,
    );
    assert_eq!(codes(&combinations), vec![1015, 2371, 2371, 1048, 1015]);
}

#[test]
fn unique_symbol_is_preserved_syntactically_and_fails_closed_semantically() {
    let source = "type Alias=unique symbol; let value:unique symbol;";
    let (_, parsed) = parse(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let output = compile(source, false);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);

    for source in [
        "interface I{[key:unique symbol]:string}",
        "type T={[key:unique symbol]:string}",
    ] {
        let output = compile(source, false);
        assert_eq!(codes(&output), vec![1335]);
        let diagnostic = &output.diagnostics[0];
        assert_eq!(
            &source[diagnostic.start as usize..(diagnostic.start + diagnostic.length) as usize],
            "unique symbol"
        );
    }

    let computed = compile(
        "declare const key:unique symbol; interface I{readonly [key]:string}",
        false,
    );
    assert!(
        computed.diagnostics.is_empty(),
        "{:?}",
        computed.diagnostics
    );
    assert_eq!(computed.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn computed_names_use_the_structural_dynamic_name_classifier() {
    for (source, expected) in [
        ("interface I{[\"\"+\"\"]:number}", vec![1169]),
        ("type T={[\"\"+\"\"]:number}", vec![1170]),
        ("interface I{[Symbol()]:number}", vec![1169]),
        ("interface I{[(missing)]:number}", vec![1169, 2304]),
        ("type T={[(missing)]:number}", vec![1170, 2304]),
    ] {
        let output = compile(source, false);
        assert_eq!(
            codes(&output),
            expected,
            "{source}: {:?}",
            output.diagnostics
        );
    }
    for source in [
        "interface I{[\"literal\"]:number;[1]:string;[-1]:boolean}",
        "declare const ordinary:symbol; interface I{[ordinary]:number}",
    ] {
        let output = compile(source, false);
        assert!(
            output.diagnostics.is_empty(),
            "{source}: {:?}",
            output.diagnostics
        );
    }
    let missing = compile("interface I{[missing]:number}", false);
    assert_eq!(codes(&missing), vec![2304]);
}

#[test]
fn callable_shape_provenance_guards_every_diagnostic_printer() {
    for source in [
        "declare const container:{method(authored:string):number}; const n:number=container;",
        "declare let union:{method(authored:string):number}|string; union();",
        "declare const nested:{inside:{method(authored:string):number}}; nested.missing;",
        "type Missing={method(authored:string):number}[\"missing\"]; let value:Missing;",
        "declare let C:{new(seed:number):object}; new C(\"wrong\");",
        "declare let indexed:{[key:string]:(authored:string)=>number}; declare let expected:(wanted:number)=>string; expected=indexed.answer;",
    ] {
        let output = compile(source, true);
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

    let mixed = compile(
        "let target:{method(authored:string):number;count:number}={method:(candidate:string)=>\"bad\",count:\"bad\"};",
        true,
    );
    assert_eq!(codes(&mixed), vec![2322], "{:?}", mixed.diagnostics);
    assert!(!mixed.diagnostics[0].message_text.contains("arg0"));
    assert_eq!(mixed.semantic_completion, SemanticCompletion::Deferred);

    let zero_construct = compile("declare let C:{new():object}; const value=new C();", true);
    assert!(zero_construct.diagnostics.is_empty());
    assert_eq!(
        zero_construct.semantic_completion,
        SemanticCompletion::Complete
    );
}

#[test]
fn call_diagnostics_stop_at_arity_or_the_first_incompatible_argument() {
    let too_many = "declare function f(value:number):void; f(\"bad\",2);";
    let output = compile(too_many, true);
    assert_eq!(codes(&output), vec![2554]);
    let diagnostic = &output.diagnostics[0];
    assert_eq!(
        &too_many[diagnostic.start as usize..(diagnostic.start + diagnostic.length) as usize],
        "2"
    );

    let too_few = "declare function g(first:number,second:string):void; g(\"bad\");";
    let output = compile(too_few, true);
    assert_eq!(codes(&output), vec![2554]);
    let diagnostic = &output.diagnostics[0];
    assert_eq!(
        &too_few[diagnostic.start as usize..(diagnostic.start + diagnostic.length) as usize],
        "g"
    );

    let two_bad = compile(
        "declare function h(first:number,second:string):void; h(\"bad\",1);",
        true,
    );
    assert_eq!(codes(&two_bad), vec![2345]);

    let deferred_first = compile(
        "declare function take(first:(wanted:number)=>string,second:number):void; declare const c:{method(authored:string):number}; take(c.method,\"bad\");",
        true,
    );
    assert!(
        deferred_first.diagnostics.is_empty(),
        "{:?}",
        deferred_first.diagnostics
    );
    assert_eq!(
        deferred_first.semantic_completion,
        SemanticCompletion::Deferred
    );
}

#[test]
fn constructor_parameter_properties_emit_fields_assignments_and_declarations() {
    let source = "export class C{constructor(public readonly x:number){}} export class D{constructor(public x:number=1){}}";
    let output = Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            declaration: true,
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    let javascript = output
        .emitted_files
        .iter()
        .find(|file| !file.declaration)
        .expect("javascript");
    assert_eq!(
        javascript.text,
        "export class C {\n    x;\n    constructor(x) {\n        this.x = x;\n    }\n}\nexport class D {\n    x;\n    constructor(x = 1) {\n        this.x = x;\n    }\n}\n"
    );
    let declaration = output
        .emitted_files
        .iter()
        .find(|file| file.declaration)
        .expect("declaration");
    assert_eq!(
        declaration.text,
        "export declare class C {\n    readonly x: number;\n    constructor(x: number);\n}\nexport declare class D {\n    x: number;\n    constructor(x?: number);\n}\n"
    );

    let es2015 = Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            target: "es2015".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    let javascript = es2015
        .emitted_files
        .iter()
        .find(|file| !file.declaration)
        .expect("javascript");
    assert!(!javascript.text.contains("    x;"));
    assert_eq!(javascript.text.matches("this.x = x;").count(), 2);
}

#[test]
fn optional_and_override_parameter_properties_keep_exact_emit_structure() {
    let source = concat!(
        "export declare class B{x:number} ",
        "export class Optional{constructor(public value?:number){}} ",
        "export class Derived extends B{constructor(override x:number){super()}}",
    );
    let output = Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            declaration: true,
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    let javascript = &output
        .emitted_files
        .iter()
        .find(|file| !file.declaration)
        .expect("javascript")
        .text;
    assert!(javascript.contains(
        "class Optional {\n    value;\n    constructor(value) {\n        this.value = value;\n    }\n}"
    ));
    assert!(javascript.contains(
        "class Derived extends B {\n    x;\n    constructor(x) {\n        super();\n        this.x = x;\n    }\n}"
    ));
    let declaration = &output
        .emitted_files
        .iter()
        .find(|file| file.declaration)
        .expect("declaration")
        .text;
    assert!(declaration.contains(
        "export declare class Optional {\n    value?: number | undefined;\n    constructor(value?: number | undefined);\n}"
    ));
    assert!(declaration.contains(
        "export declare class Derived extends B {\n    x: number;\n    constructor(x: number);\n}"
    ));
    assert!(!declaration.contains("override x"));
}

#[test]
fn declaration_emit_distinguishes_empty_implementations_from_bodyless_signatures() {
    let source = "export function implemented(){} export declare function ambient(); export function defaulted(x=1){}";
    let output = Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            declaration: true,
            target: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    let declaration = output
        .emitted_files
        .iter()
        .find(|file| file.declaration)
        .expect("declaration");
    assert_eq!(
        declaration.text,
        "export declare function implemented(): void;\nexport declare function ambient(): any;\nexport declare function defaulted(x?: number): void;\n"
    );
}

#[test]
fn navigation_and_duplicate_recovery_keep_first_signature_bindings() {
    let source = "const seed=1; function f(x=seed){} const g=(x=seed)=>x; class C{m(x=seed){} constructor(x=seed){}}";
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open("case.ts", Arc::<str>::from(source));
    let seed = source.find("seed").expect("seed declaration") as u32;
    assert_eq!(
        service.references("case.ts", seed + 1)[0].references.len(),
        5
    );
    assert_eq!(service.rename("case.ts", seed + 1).locations.len(), 5);

    let duplicate = "type R=(x:number,x:string)=>typeof x; declare let r:R; const value=r(1,\"s\"); const numberUse:number=value; const stringUse:string=value;";
    let output = compile(duplicate, true);
    assert_eq!(
        codes(&output),
        vec![2300, 2300, 2322],
        "{:?}",
        output.diagnostics
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open("case.ts", Arc::<str>::from(duplicate));
    let positions = duplicate
        .match_indices("x")
        .map(|(position, _)| position as u32)
        .collect::<Vec<_>>();
    assert_eq!(positions.len(), 3);
    let definition = service
        .definition_and_bound_span("case.ts", positions[2] + 1)
        .expect("typeof duplicate parameter definition");
    assert_eq!(definition.definitions[0].text_span.start, positions[0]);
    assert_eq!(
        service.references("case.ts", positions[0] + 1)[0]
            .references
            .len(),
        2
    );
    assert_eq!(
        service.references("case.ts", positions[1] + 1)[0]
            .references
            .len(),
        1
    );

    let implemented = compile(
        "function direct(x:number,x:string){return x} const value=direct(1,\"s\"); const numberUse:number=value; const stringUse:string=value;",
        true,
    );
    assert_eq!(codes(&implemented), vec![2300, 2300, 2322]);
    assert_eq!(
        implemented.semantic_completion,
        SemanticCompletion::Complete
    );
}

#[test]
fn unsupported_signature_display_and_index_writes_fail_closed() {
    for (source, name) in [
        ("let modified:{readonly m():void};", "modified"),
        ("let generic:<const T>(x:T)=>T;", "generic"),
        ("let parameter:(public x:number)=>void;", "parameter"),
    ] {
        let mut service = LanguageService::new(CompilerOptions::default());
        service.open("case.ts", Arc::<str>::from(source));
        let offset = source.find(name).expect("declaration") as u32;
        assert!(
            service.quick_info("case.ts", offset + 1).is_none(),
            "{source}"
        );
    }

    let readonly = compile(
        "let dictionary:{readonly [key:string]:number}; dictionary.answer=1;",
        true,
    );
    assert!(
        readonly.diagnostics.is_empty(),
        "{:?}",
        readonly.diagnostics
    );
    assert_eq!(readonly.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn contextual_modifier_names_and_keyword_computed_names_recover_structurally() {
    let source = "export interface I{readonly=1}";
    let (_, parsed) = parse(source);
    let [member] = sole_interface(&parsed).members.as_slice() else {
        panic!("one property");
    };
    let TypeMemberKind::Property {
        name, initializer, ..
    } = &member.kind
    else {
        panic!("property");
    };
    assert!(matches!(&name.kind, TypeMemberNameKind::Identifier(name) if name == "readonly"));
    assert!(initializer.is_some());
    assert!(member.recovered);
    assert_eq!(codes(&compile(source, false)), vec![1131, 1128]);

    let output = Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            declaration: true,
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert!(output.emitted_files.is_empty());

    for (source, host_code) in [
        ("interface I{[this]:number}", 1169),
        ("type T={[this]:number}", 1170),
        ("interface I{[super.x]:number}", 1169),
        ("type T={[import.meta]:number}", 1170),
    ] {
        let output = compile(source, false);
        assert_eq!(
            codes(&output),
            vec![host_code],
            "{source}: {:?}",
            output.diagnostics
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }
}

#[test]
fn type_member_start_boundary_drops_bare_initializers_but_retains_started_members() {
    let bare = Compiler::new().compile(
        vec![SourceInput::new(
            "case.ts",
            Arc::<str>::from("export type T={x=1}"),
        )],
        &CompilerOptions {
            declaration: true,
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(codes(&bare), vec![1131, 1128]);
    assert_eq!(bare.semantic_completion, SemanticCompletion::Deferred);
    assert!(bare.emitted_files.is_empty());

    let optional = Compiler::new().compile(
        vec![SourceInput::new(
            "case.ts",
            Arc::<str>::from("export interface I{x?=1}"),
        )],
        &CompilerOptions {
            declaration: true,
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(codes(&optional), vec![1246]);
    assert_eq!(
        optional
            .emitted_files
            .iter()
            .find(|file| file.declaration)
            .expect("declaration")
            .text,
        "export interface I {\n    x?: number | undefined;\n}\n"
    );

    let computed = Compiler::new().compile(
        vec![SourceInput::new(
            "case.ts",
            Arc::<str>::from("declare const k:unique symbol; export type T={[k]=1}"),
        )],
        &CompilerOptions {
            declaration: true,
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(codes(&computed), vec![1247]);
    assert_eq!(computed.semantic_completion, SemanticCompletion::Deferred);
    assert!(computed.emitted_files.iter().all(|file| !file.declaration));
}

#[test]
fn nested_and_multi_tail_member_recovery_blocks_unrepresented_products() {
    for (source, expected_codes, expected_tokens) in [
        ("export let v:{x=1};", vec![1131, 1128], vec!["x", "}"]),
        (
            "export function f(a:{x=1}){}",
            vec![1131, 1005],
            vec!["x", "}"],
        ),
    ] {
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
            assert_eq!(codes(&output), expected_codes, "{source}");
            assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
            assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
            assert!(output.emitted_files.is_empty(), "{source}");
            for (diagnostic, expected) in output.diagnostics.iter().zip(&expected_tokens) {
                assert_eq!(
                    &source[diagnostic.start as usize
                        ..(diagnostic.start + diagnostic.length) as usize],
                    *expected
                );
            }
        }
    }

    for source in [
        "export interface I{x=1;y:number}",
        "export type T={x=1;y:number}",
    ] {
        let output = Compiler::new().compile(
            vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
            &CompilerOptions {
                declaration: true,
                target: "esnext".to_string(),
                module: "esnext".to_string(),
                ..CompilerOptions::default()
            },
        );
        assert_eq!(codes(&output), vec![1131, 1128], "{source}");
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert!(output.emitted_files.is_empty(), "{source}");
    }

    for source in [
        "export class C{value:{x=1}}",
        "export function f<T extends {x=1}>(){}",
    ] {
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
            assert!(output.emitted_files.is_empty(), "{source}");
        }
    }
}

#[test]
fn bigint_literals_preserve_syntax_without_collapsing_literal_identity() {
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "case.ts",
            Arc::<str>::from("export function f(x=1n){} export interface I{1n:string}"),
        )],
        &CompilerOptions {
            declaration: true,
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(codes(&output), vec![1539]);
    let javascript = &output
        .emitted_files
        .iter()
        .find(|file| !file.declaration)
        .expect("javascript")
        .text;
    assert_eq!(javascript, "export function f(x = 1n) { }\n");
    let declaration = &output
        .emitted_files
        .iter()
        .find(|file| file.declaration)
        .expect("declaration")
        .text;
    assert!(declaration.contains("f(x?: bigint): void;"));
    assert!(declaration.contains("1n: string;"));

    for (source, target) in [
        ("let value:1n=2n;", "esnext"),
        ("function low(value=1n){}", "es2015"),
    ] {
        let output = Compiler::new().compile(
            vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
            &CompilerOptions {
                no_emit: true,
                target: target.to_string(),
                ..CompilerOptions::default()
            },
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{source}"
        );
    }
}

#[test]
fn literal_defaults_feed_parameter_values_and_unknown_defaults_block_dts() {
    let literal = compile(
        "function f(x=1){return x} f(); f(2); f(\"bad\"); const wrong:string=f();",
        true,
    );
    assert_eq!(
        codes(&literal),
        vec![2345, 2322],
        "{:?}",
        literal.diagnostics
    );

    let null_output = Compiler::new().compile(
        vec![SourceInput::new(
            "case.ts",
            Arc::<str>::from("export function nullable(value=null){}"),
        )],
        &CompilerOptions {
            declaration: true,
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    let declaration = null_output
        .emitted_files
        .iter()
        .find(|file| file.declaration)
        .expect("null declaration");
    assert_eq!(
        declaration.text,
        "export declare function nullable(value?: null): void;\n"
    );

    for no_check in [false, true] {
        let output = Compiler::new().compile(
            vec![SourceInput::new(
                "case.ts",
                Arc::<str>::from("export function sum(value=1+0){}"),
            )],
            &CompilerOptions {
                declaration: true,
                no_check,
                target: "esnext".to_string(),
                module: "esnext".to_string(),
                ..CompilerOptions::default()
            },
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        assert!(output.emitted_files.iter().any(|file| !file.declaration));
        assert!(!output.emitted_files.iter().any(|file| file.declaration));
    }

    let inferred_return = Compiler::new().compile(
        vec![SourceInput::new(
            "case.ts",
            Arc::<str>::from("export function identity(value=1){return value}"),
        )],
        &CompilerOptions {
            declaration: true,
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(
        inferred_return.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert!(
        inferred_return
            .emitted_files
            .iter()
            .any(|file| !file.declaration)
    );
    assert!(
        !inferred_return
            .emitted_files
            .iter()
            .any(|file| file.declaration)
    );

    for source in [
        "function forward(value=later,later=1){}",
        "function bodyLocal(value=local){let local=1}",
        "function query(value:typeof local){let local=1}",
    ] {
        let output = compile(source, true);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{source}"
        );
    }

    let bodyless = compile("function bodyless();", true);
    assert_eq!(codes(&bodyless), vec![2391, 7010]);
    assert_eq!(bodyless.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        bodyless.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );
}

#[test]
fn derived_parameter_property_emit_preserves_directives_and_super_order() {
    let source = concat!(
        "class B{} ",
        "export class Base{constructor(public x:number){\"use custom\";work();}} ",
        "export class Derived extends B{constructor(public x:number){\"use custom\";super();work();}}",
    );
    let esnext = Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    let text = &esnext
        .emitted_files
        .iter()
        .find(|file| !file.declaration)
        .expect("esnext javascript")
        .text;
    assert!(text.contains(
        "constructor(x) {\n        \"use custom\";\n        this.x = x;\n        work();\n    }"
    ));
    assert!(text.contains(
        "constructor(x) {\n        \"use custom\";\n        super();\n        this.x = x;\n        work();\n    }"
    ));
    assert_eq!(text.matches("    x;\n").count(), 2);

    let es2015 = Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            target: "es2015".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    let text = &es2015
        .emitted_files
        .iter()
        .find(|file| !file.declaration)
        .expect("es2015 javascript")
        .text;
    assert!(!text.contains("    x;\n"));
    assert!(text.find("super();").unwrap() < text.rfind("this.x = x;").unwrap());

    let instance = compile(
        "class C{constructor(public x:number){}} const value=new C(1).x;",
        true,
    );
    assert_eq!(instance.semantic_completion, SemanticCompletion::Deferred);
}
