use std::sync::Arc;

use tsz::diagnostics::DiagnosticCategory;
use tsz::{Compiler, CompilerOptions, SemanticCompletion, SourceInput};

fn compile(source: &str) -> tsz::CompileOutput {
    compile_files(&[("case.ts", source)])
}

fn compile_files(files: &[(&str, &str)]) -> tsz::CompileOutput {
    Compiler::new().compile(
        files
            .iter()
            .map(|(path, source)| SourceInput::new(*path, Arc::<str>::from(*source)))
            .collect(),
        &CompilerOptions {
            no_emit: true,
            strict: true,
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
fn generic_function_parameters_keep_their_lexical_type_context() {
    for source in [
        "function pair<T,U>(left:T,right:U):void {const values=[left,right];}",
        "interface Base{value:string} function renamed<Cedar extends Base,Birch>(left:Cedar,right:Birch):void {const values=[left,right];}",
        "function wrapped<T>(value:{item:T[]}):void {const item=value;}",
    ] {
        let output = compile(source);
        assert!(
            output.diagnostics.is_empty(),
            "generic parameter annotations lost lexical context for {source}: {:?}",
            output.diagnostics
        );
    }

    let source = "function pair<T,U>(left:T,right:U):void {const values=[left,right];} const independent:MissingType=1;";
    let output = compile(source);
    assert_eq!(codes(&output), vec![2304]);
    assert_eq!(
        (
            output.diagnostics[0].start,
            output.diagnostics[0].length,
            output.diagnostics[0].message_text.as_str(),
        ),
        (
            source.find("MissingType").unwrap() as u32,
            "MissingType".len() as u32,
            "Cannot find name 'MissingType'.",
        )
    );
}

#[test]
fn arrow_parameter_values_use_their_owned_context_inside_nested_bodies() {
    for source in [
        concat!(
            "declare function consume(fn:(item:string)=>string):void;",
            "consume((renamed):string=>\".\"+renamed);",
        ),
        "const wrapped=(handler:any)=>()=>handler(10)",
        "const nested=((renamed:any)=>()=>renamed(20))",
        "class Box{field={wrapped:(handler:any)=>()=>handler(10)}}",
        "class Vessel{field={wrapped:(renamed:any)=>()=>renamed(20)}}",
    ] {
        let output = compile(source);
        assert_eq!(
            (output.semantic_completion, codes(&output)),
            (SemanticCompletion::Complete, Vec::new()),
            "{source}: {:#?}",
            output.diagnostics,
        );
    }

    for source in [
        "class Box{field={wrapped:(handler)=>()=>handler(10)}}",
        "class Vessel{field={wrapped:(renamed)=>()=>renamed(20)}}",
    ] {
        let output = Compiler::new().compile(
            vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
            &CompilerOptions {
                no_emit: true,
                strict: false,
                ..CompilerOptions::default()
            },
        );
        assert_eq!(
            (output.semantic_completion, codes(&output)),
            (SemanticCompletion::Complete, Vec::new()),
            "{source}: {:#?}",
            output.diagnostics,
        );
    }
}

#[test]
fn generic_arrow_parameter_callability_waits_for_the_apparent_constraint_query() {
    for source in [
        concat!(
            "function outer<T extends (value:number)=>number>(){",
            "const wrapped=(handler:T)=>()=>handler(10);}",
        ),
        concat!(
            "function renamed<Callback extends (item:number)=>number>(){",
            "const nested=((candidate:Callback)=>()=>candidate(20));}",
        ),
        "function unconstrained<Value>(value:Value){value()}",
        concat!(
            "type Fn=(value:number)=>number;type Tagged={tag:string};",
            "const wrapped=(handler:Fn&Tagged)=>()=>handler(10);",
        ),
        concat!(
            "type Left=(value:number)=>number;type Right=(item:number)=>number;",
            "const nested=((candidate:Left|Right)=>()=>candidate(20));",
        ),
        "const wrapped=((handler:unknown)=>()=>handler(10))",
        "const wrapped=((missing:null)=>()=>missing())",
        "const nested=(absent:undefined)=>()=>absent()",
        "class Constructor{} const wrapped=(value:typeof Constructor)=>()=>value()",
        "const wrapped=(value:boolean)=>()=>value()",
        "const wrapped=(value:number)=>()=>value()",
        "const wrapped=(value:string)=>()=>value()",
        "const wrapped=(value:bigint)=>()=>value()",
        "const wrapped=(value:object)=>()=>value()",
        "const wrapped=(value:symbol)=>()=>value()",
    ] {
        let first = compile(source);
        let second = compile(source);
        assert_eq!(
            first.semantic_completion,
            SemanticCompletion::Deferred,
            "{source}"
        );
        assert!(
            codes(&first).is_empty(),
            "{source}: {:#?}",
            first.diagnostics
        );
        assert_eq!(
            (second.semantic_completion, codes(&second)),
            (first.semantic_completion, codes(&first)),
            "{source}",
        );
    }

    for (source, name, rendered) in [
        ("declare const empty:void;empty()", "empty", "void"),
        ("declare const bottom:never;bottom()", "bottom", "never"),
    ] {
        let output = compile(source);
        assert_eq!(
            (output.semantic_completion, codes(&output)),
            (SemanticCompletion::Complete, vec![2349]),
            "{source}: {:#?}",
            output.diagnostics,
        );
        let expected_message =
            format!("This expression is not callable. Type '{rendered}' has no call signatures.");
        let diagnostic = &output.diagnostics[0];
        assert_eq!(
            (
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.as_str(),
            ),
            (
                source.rfind(name).expect("callee") as u32,
                name.len() as u32,
                expected_message.as_str(),
            ),
            "{source}: {:#?}",
            output.diagnostics,
        );
    }
}

#[test]
fn optional_annotated_arrow_values_wait_for_undefined_aware_calls_in_strict_mode() {
    let source = "const wrapped=(handler?:(value:number)=>number)=>()=>handler(10)";
    let strict = compile(source);
    assert_eq!(
        (strict.semantic_completion, codes(&strict)),
        (SemanticCompletion::Deferred, Vec::<u32>::new()),
        "{:#?}",
        strict.diagnostics,
    );

    let loose = Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            no_emit: true,
            strict: false,
            ..CompilerOptions::default()
        },
    );
    assert_eq!(
        (loose.semantic_completion, codes(&loose)),
        (SemanticCompletion::Complete, Vec::<u32>::new()),
        "{:#?}",
        loose.diagnostics,
    );
}

#[test]
fn recovered_overload_hosts_do_not_publish_overload_or_duplicate_name_diagnostics() {
    let recovered = [
        "function* generate() { return 1; }",
        "class Holder { method(@decorate value:number) {} }",
        "class Renamed { @decorate(\"x\", true) method() {} }",
    ]
    .into_iter()
    .map(|source| {
        let output = compile(source);
        assert!(output.diagnostics.iter().all(|diagnostic| {
            diagnostic.file == "case.ts"
                && diagnostic.category == DiagnosticCategory::Error
                && diagnostic.related_information.is_empty()
        }));
        (
            output.semantic_completion,
            output
                .diagnostics
                .iter()
                .map(|diagnostic| {
                    (
                        diagnostic.code,
                        diagnostic.start,
                        diagnostic.length,
                        diagnostic.message_text.clone(),
                    )
                })
                .collect::<Vec<_>>(),
        )
    })
    .collect::<Vec<_>>();
    assert_eq!(
        recovered,
        vec![
            (
                SemanticCompletion::Deferred,
                vec![
                    (1003, 8, 1, "Identifier expected.".to_string()),
                    (1005, 10, 8, "'(' expected.".to_string()),
                    (1005, 18, 1, "')' expected.".to_string()),
                    (1109, 19, 1, "Expression expected.".to_string()),
                    (1005, 21, 1, "')' expected.".to_string()),
                ],
            ),
            (
                SemanticCompletion::Deferred,
                vec![
                    (1003, 22, 1, "Identifier expected.".to_string()),
                    (1005, 23, 8, "')' expected.".to_string()),
                    (1003, 44, 1, "Identifier expected.".to_string()),
                    (1003, 46, 1, "Identifier expected.".to_string()),
                    (1109, 49, 1, "Expression expected.".to_string()),
                ],
            ),
            (
                SemanticCompletion::Deferred,
                vec![
                    (1003, 16, 1, "Identifier expected.".to_string()),
                    (1003, 26, 3, "Identifier expected.".to_string()),
                    (1003, 31, 4, "Identifier expected.".to_string()),
                ],
            ),
        ]
    );

    let missing = compile("class Holder { method():void; }");
    assert_eq!(codes(&missing), vec![2391]);
    let renamed = compile("class Holder { method():void; other(){} }");
    assert_eq!(codes(&renamed), vec![2389]);
    let duplicate = compile("function ordinary(value:number,value:string):void {}");
    assert_eq!(codes(&duplicate), vec![2300, 2300]);
}

#[test]
fn predicate_flow_defers_only_returns_that_consume_the_narrowed_value() {
    let source = concat!(
        "interface A{a:string} interface B{b:string}",
        "declare function isB(value:any,mode:number):value is B;",
        "function merge(value:A,mode:number):A&B|null{",
        "if((isB(value,mode))){return value;}else{return null;}}",
        "function independent(value:A):A&B|null{",
        "if(isB(value,0)){const wrong:number=\"bad\";return 1;}return null;}",
        "function shadowed(value:A):A&B|null{",
        "if(isB(value,0)){{const value=1;return value;}}return null;}",
    );
    let output = compile(source);
    assert_eq!(
        codes(&output),
        vec![2322, 2322, 2322],
        "{:?}",
        output.diagnostics
    );
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.start, diagnostic.length))
            .collect::<Vec<_>>(),
        vec![
            (source.find("wrong").unwrap() as u32, "wrong".len() as u32),
            (
                source.find("return 1").unwrap() as u32,
                "return".len() as u32
            ),
            (
                source.rfind("return value").unwrap() as u32,
                "return".len() as u32,
            ),
        ]
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn predicate_true_flow_uses_binder_identity_across_arguments_files_and_root_orders() {
    let renamed = concat!(
        "interface Cedar{cedar:string}interface Birch{birch:string}",
        "declare function isBirch(mode:number,candidate:any):candidate is Birch;",
        "function merge(candidate:Cedar,mode:number):Cedar&Birch|null{",
        "if((((isBirch(mode,(candidate)))))){return candidate;}return null;}",
    );
    let output = compile(renamed);
    assert_eq!(
        (output.semantic_completion, codes(&output)),
        (SemanticCompletion::Complete, Vec::new()),
        "{:#?}",
        output.diagnostics,
    );

    let declarations = concat!(
        "interface Left{left:string}interface Right{right:string}",
        "declare function isRight(value:any):value is Right;",
    );
    let consumer = concat!(
        "function combine(value:Left):Left&Right|null{",
        "if(isRight(value)){return value;}return null;}",
    );
    for files in [
        [("declarations.ts", declarations), ("consumer.ts", consumer)],
        [("consumer.ts", consumer), ("declarations.ts", declarations)],
    ] {
        let output = compile_files(&files);
        assert_eq!(
            (output.semantic_completion, codes(&output)),
            (SemanticCompletion::Complete, Vec::new()),
            "{:#?}",
            output.diagnostics,
        );
    }
}

#[test]
fn predicate_flow_fails_closed_only_for_the_dependent_unsupported_reference() {
    let prefix = concat!(
        "interface A{a:string}interface B{b:string}",
        "declare function isB(value:any):value is B;",
    );
    // Delete these path nonclaims only when projection owns ordered union/intersection
    // reconstruction and optional reads produce `T | undefined` before narrowing.
    for (suffix, completion) in [
        (
            concat!(
                "function falsePath(value:A):A&B|null{",
                "if(isB(value)){return null;}else{return value;}}",
            ),
            SemanticCompletion::Deferred,
        ),
        (
            concat!(
                "declare function isGeneric<T>(value:any):value is T;",
                "function generic(value:A):A&B|null{",
                "if(isGeneric<B>(value)){return value;}return null;}",
            ),
            SemanticCompletion::Deferred,
        ),
        (
            concat!(
                "declare const holder:{value:A};",
                "function member():A&B|null{",
                "if(isB(holder.value)){return holder.value;}return null;}",
            ),
            SemanticCompletion::Complete,
        ),
        (
            concat!(
                "const alias=isB;function aliased(value:A):A&B|null{",
                "if(alias(value)){return value;}return null;}",
            ),
            SemanticCompletion::Deferred,
        ),
        (
            concat!(
                "declare const unionHolder:{value:A}|{value:B};",
                "function unionMember():A&B|null{",
                "if(isB(unionHolder.value)){return unionHolder.value;}return null;}",
            ),
            SemanticCompletion::Deferred,
        ),
        (
            concat!(
                "declare const optionalHolder:{value?:A};",
                "function optionalMember():A&B|null{",
                "if(isB(optionalHolder.value)){return optionalHolder.value;}return null;}",
            ),
            SemanticCompletion::Deferred,
        ),
        (
            concat!(
                "declare const intersectionHolder:{value:A}&{tag:string};",
                "function intersectionMember():A&B|null{",
                "if(isB(intersectionHolder.value)){return intersectionHolder.value;}return null;}",
            ),
            SemanticCompletion::Deferred,
        ),
    ] {
        let source = format!("{prefix}{suffix}");
        let output = compile(&source);
        assert_eq!(
            (output.semantic_completion, codes(&output)),
            (completion, Vec::new()),
            "{source}: {:#?}",
            output.diagnostics,
        );
    }

    let ordinary = format!(
        "{prefix}declare function ordinary(value:any):boolean;{}",
        concat!(
            "function mismatch(value:A):A&B|null{",
            "if(ordinary(value)){return value;}return null;}",
        ),
    );
    let output = compile(&ordinary);
    assert_eq!(
        (output.semantic_completion, codes(&output)),
        (SemanticCompletion::Complete, vec![2322]),
        "{:#?}",
        output.diagnostics,
    );
    assert_eq!(
        (output.diagnostics[0].start, output.diagnostics[0].length),
        (
            ordinary.rfind("return value").unwrap() as u32,
            "return".len() as u32,
        ),
    );
}

#[test]
fn predicate_leaf_reduces_top_union_boolean_and_proven_intersection_types() {
    let top_matrix = concat!(
        "declare function isAny(value:any):value is any;",
        "declare function isUnknown(value:any):value is unknown;",
        "function anyUnknown(value:any):void{if(isUnknown(value)){",
        "const wrong:string=value;}else{const exact:string=value;}}",
        "function unknownAny(value:unknown):void{if(isAny(value)){",
        "value.missing;}else{const exact:never=value;}}",
        "function concreteAny(value:number):void{if(isAny(value)){",
        "const exact:number=value;}else{const bottom:never=value;}}",
        "function concreteUnknown(value:number):void{if(isUnknown(value)){",
        "const exact:number=value;}else{const bottom:never=value;}}",
        "function sameAny(value:any):void{if(isAny(value)){",
        "value.missing;}else{const bottom:never=value;}}",
        "function sameUnknown(value:unknown):void{if(isUnknown(value)){",
        "const exact:unknown=value;}else{const bottom:never=value;}}",
    );
    let output = compile(top_matrix);
    assert_eq!(
        (output.semantic_completion, codes(&output)),
        (SemanticCompletion::Complete, vec![2322]),
        "{:#?}",
        output.diagnostics,
    );

    let recovery_matrix = concat!(
        "declare function isString(value:any):value is string;",
        "declare function isMissing(value:any):value is Missing;",
        "declare function isInvalid(value:any):",
        "value is {present:string}[\"absentTarget\"];",
        "declare const missing:Missing;",
        "declare const invalid:{present:string}[\"absentSource\"];",
        "function recoveryTargets(value:number):void{",
        "if(isMissing(value)){const first:number=value;}else{const second:number=value;}",
        "if(isInvalid(value)){const third:number=value;}else{const fourth:number=value;}}",
        "function recoverySources():void{",
        "if(isString(missing)){const wrongNumber:number=missing;}else{missing.anything;}",
        "if(isString(invalid)){const otherWrong:number=invalid;}else{invalid.anything;}}",
    );
    let output = compile(recovery_matrix);
    assert_eq!(
        (output.semantic_completion, codes(&output)),
        (
            SemanticCompletion::Complete,
            vec![2304, 2339, 2304, 2339, 2322, 2322]
        ),
        "{:#?}",
        output.diagnostics,
    );

    let source = concat!(
        "interface A{a:string}interface B{b:string}interface C{c:string}interface D{d:string}",
        "declare function isB(value:any):value is B;",
        "declare function isBorC(value:any):value is B|C;",
        "declare function isTrue(value:any):value is true;",
        "declare function isOne(value:any):value is 1;",
        "declare function isTwo(value:any):value is 2;",
        "declare function isNumber(value:any):value is number;",
        "declare function isString(value:any):value is string;",
        "declare function isNever(value:any):value is never;",
        "function anyTrue(value:any):number|null{",
        "if(isB(value)){return value.missing;}return null;}",
        "function unknownTrue(value:unknown):string|null{",
        "if(isB(value)){return value.b;}return null;}",
        "function overlap(value:A|B):B|null{",
        "if(isBorC(value)){return value;}return null;}",
        "function exclude(value:A|B):A|null{",
        "if(isB(value)){return null;}return value;}",
        "function anyFalse(value:any):number{",
        "if(isB(value)){return 0;}return value.missing;}",
        "function booleanFalse(value:boolean):false|null{",
        "if(isTrue(value)){return null;}return value;}",
        "function nestedBoolean(value:boolean|string):false|string|null{",
        "if(isTrue(value)){return null;}return value;}",
        "function proven(value:A&B):A&B|null{",
        "if(isB(value)){return value;}return null;}",
        "function numberToOne(value:number):1|null{if(isOne(value)){return value;}return null;}",
        "function oneToNumber(value:1):1|null{if(isNumber(value)){return value;}return null;}",
        "function disjoint(value:number):never|null{if(isString(value)){return value;}return null;}",
        "function disjointElse(value:number):number|null{",
        "if(isString(value)){return null;}return value;}",
        "function distinctLiteral(value:1):never|null{if(isTwo(value)){return value;}return null;}",
        "function neverSource(value:never):never{if(isString(value)){return value;}return value;}",
        "function neverTarget(value:number):void{if(isNever(value)){",
        "const bottom:never=value;}else{const exact:number=value;}}",
        "declare function isAorB(value:any):value is A|B;",
        "declare function takeD(value:D):void;",
        "function ordered(value:B|A|C):void{if(isAorB(value)){takeD(value);}}",
    );
    let output = compile(source);
    assert_eq!(
        (output.semantic_completion, codes(&output)),
        (SemanticCompletion::Complete, vec![2339, 2345]),
        "{:#?}",
        output.diagnostics,
    );
    assert!(
        output.diagnostics[1].message_text.contains("B | A"),
        "{:#?}",
        output.diagnostics[1],
    );

    let scalar = concat!(
        "type A={kind:'a';a:string};type B={kind:'b';b:string};",
        "type D={kind:'d'};",
        "declare function isB(value:unknown):value is B;",
        "declare function takeD(value:D):void;",
        "function scalar(value:A|B):void{if(isB(value)){takeD(value);}}",
    );
    let output = compile(scalar);
    assert_eq!(
        (
            output.semantic_completion,
            codes(&output),
            output.diagnostics[0].message_text.as_str(),
        ),
        (
            SemanticCompletion::Complete,
            vec![2345],
            "Argument of type 'B' is not assignable to parameter of type 'D'.",
        ),
        "{:#?}",
        output.diagnostics,
    );

    let unsupported = concat!(
        "interface A{a:string}interface B{b:string}interface C{c:string}",
        "declare function isC(value:any):value is C;",
        "function unproven(value:A&B):C|null{",
        "if(isC(value)){return value;}return null;}",
    );
    let output = compile(unsupported);
    assert_eq!(
        (output.semantic_completion, codes(&output)),
        (SemanticCompletion::Deferred, Vec::new()),
        "{:#?}",
        output.diagnostics,
    );
}
