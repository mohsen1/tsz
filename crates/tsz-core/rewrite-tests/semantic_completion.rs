use std::sync::Arc;

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

fn assert_completion(output: &tsz::CompileOutput, expected: SemanticCompletion) {
    assert_eq!(
        output.semantic_completion, expected,
        "unexpected completion for diagnostics {:?}",
        output.diagnostics
    );
    assert_eq!(output.stats.semantic_completion, expected);
    if !expected.is_complete() {
        assert_eq!(
            output.exit_status,
            CompileExitStatus::SemanticIncomplete,
            "an incomplete checked compile must not claim an ordinary TypeScript exit"
        );
    }
}

#[test]
fn semantic_completion_dominance_is_order_independent() {
    let verdicts = [
        SemanticCompletion::Complete,
        SemanticCompletion::Deferred,
        SemanticCompletion::Cycle,
        SemanticCompletion::Limit,
    ];
    for left in verdicts {
        for right in verdicts {
            assert_eq!(left.combine(right), std::cmp::max(left, right));
            assert_eq!(left.combine(right), right.combine(left));
        }
    }
}

#[test]
fn recursive_interfaces_relate_by_active_symbolic_pair() {
    let cases = [
        "interface A { next:A } interface B { next:B } \
         function convert(value:A):B { return value; }",
        "interface Cedar { tail:Cedar } interface Birch { tail:Birch } \
         function rename(input:Cedar):Birch { return input; }",
        "interface WrappedA { edge:{ next:WrappedA } } \
         interface WrappedB { edge:{ next:WrappedB } } \
         function wrapped(value:WrappedA):WrappedB { return value; }",
        "interface Chain<T> { next:Chain<T> } \
         function concrete(value:Chain<string>):Chain<string> { return value; }",
    ];
    for source in cases {
        let output = compile(source);
        assert!(
            output.diagnostics.is_empty(),
            "recursive structural peers were rejected: {:?}",
            output.diagnostics
        );
        assert_completion(&output, SemanticCompletion::Complete);
    }
}

#[test]
fn recursive_interfaces_still_reject_a_definitive_nested_mismatch() {
    let cases = [
        "interface A { next:A; tag:string } interface B { next:B; tag:number } \
         function convert(value:A):B { return value; }",
        "interface Left { edge:{ next:Left; value:string } } \
         interface Right { edge:{ next:Right; value:boolean } } \
         function nested(value:Left):Right { return value; }",
    ];
    for source in cases {
        let output = compile(source);
        assert_eq!(codes(&output), vec![2322], "{:?}", output.diagnostics);
    }
}

#[test]
fn class_value_and_instance_models_do_not_fall_back_to_compatible_error() {
    let direct = compile("class C {} const n:number=C;");
    assert_eq!(codes(&direct), vec![2322]);
    assert_eq!(
        direct.diagnostics[0].message_text,
        "Type 'typeof C' is not assignable to type 'number'."
    );

    let instance = compile("class C {} let c:C; const n:number=c;");
    assert_eq!(codes(&instance), vec![2322]);
    assert_eq!(
        instance.diagnostics[0].message_text,
        "Type 'C' is not assignable to type 'number'."
    );

    let renamed = compile(
        "class Vessel {} const text:string=Vessel; \
         let vessel:Vessel; const flag:boolean=vessel;",
    );
    assert_eq!(codes(&renamed), vec![2322, 2322]);
}

#[test]
fn empty_class_instances_keep_their_structural_positive_cases() {
    let output = compile(
        "class First {} class Second {} \
         let first:First; const same:First=first; \
         const peer:Second=first; const fromNumber:First=1;",
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_completion(&output, SemanticCompletion::Complete);
}

#[test]
fn missing_indexed_properties_are_structured_errors_without_cached_success() {
    let source = "type Bad={ present:number }[\"missing\"]; \
                  let bad:Bad; const text:string=bad;";
    let output = compile(source);
    assert_eq!(codes(&output), vec![2339], "{:?}", output.diagnostics);
    assert_eq!(
        output.diagnostics[0].message_text,
        "Property 'missing' does not exist on type '{ present: number; }'."
    );
    assert_eq!(
        (output.diagnostics[0].start, output.diagnostics[0].length),
        (source.find("\"missing\"").unwrap() as u32, 9)
    );

    let renamed = compile(
        "type Broken={ ready:boolean }[\"omitted\"]; \
         let broken:Broken; \
         const count:number=broken;",
    );
    assert_eq!(codes(&renamed), vec![2339], "{:?}", renamed.diagnostics);
}

#[test]
fn required_annotations_visit_nested_components_in_unused_and_ambient_declarations() {
    let source = "let direct:{present:number}[\"missing\"]; \
                  interface Nested { member:{ready:boolean}[\"absent\"] } \
                  declare function ambient(value:{payload:{count:number}[\"omitted\"]}):void; \
                  const root={present:1}; let projected:typeof root.missing;";
    let output = compile(source);
    assert_eq!(codes(&output), vec![2339, 2339, 2339, 2339]);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message_text.as_str())
            .collect::<Vec<_>>(),
        vec![
            "Property 'missing' does not exist on type '{ present: number; }'.",
            "Property 'absent' does not exist on type '{ ready: boolean; }'.",
            "Property 'omitted' does not exist on type '{ count: number; }'.",
            "Property 'missing' does not exist on type '{ present: number; }'.",
        ]
    );
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.start, diagnostic.length))
            .collect::<Vec<_>>(),
        vec![
            (source.find("\"missing\"").unwrap() as u32, 9),
            (source.find("\"absent\"").unwrap() as u32, 8),
            (source.find("\"omitted\"").unwrap() as u32, 9),
            (source.rfind("missing").unwrap() as u32, 7),
        ]
    );
    assert_completion(&output, SemanticCompletion::Complete);
    assert_ne!(output.exit_status, CompileExitStatus::Success);

    let renamed = compile(
        "let leaf:{available:string}[\"gone\"]; \
         interface Wrapper { inner:{flag:boolean}[\"lost\"] } \
         const vessel={payload:{count:1}}; \
         let projection:typeof vessel.payload.omitted;",
    );
    assert_eq!(codes(&renamed), vec![2339, 2339, 2339]);
    assert_completion(&renamed, SemanticCompletion::Complete);

    let repeated_source = "const origin={present:1}; \
                           let first:typeof origin.missing; \
                           let second:typeof origin.missing;";
    let repeated = compile(repeated_source);
    assert_eq!(codes(&repeated), vec![2339, 2339]);
    assert_eq!(
        repeated
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.start)
            .collect::<Vec<_>>(),
        vec![
            repeated_source.find("missing").unwrap() as u32,
            repeated_source.rfind("missing").unwrap() as u32,
        ]
    );
    assert_completion(&repeated, SemanticCompletion::Complete);
}

#[test]
fn class_and_function_type_annotations_share_the_required_type_boundary() {
    let output = compile(
        "declare class Vessel { \
           field:{present:number}[\"missing\"]; \
           method(value:{ready:boolean}[\"absent\"]):{done:string}[\"gone\"]; \
         } \
         type Handler=(value:{open:number}[\"closed\"])=>{yes:boolean}[\"no\"];",
    );
    assert_eq!(codes(&output), vec![2339, 2339, 2339, 2339, 2339]);
    assert_completion(&output, SemanticCompletion::Complete);
}

#[test]
fn every_supported_explicit_type_position_uses_one_required_boundary() {
    let source = r#"
declare class Crate<T> {}
const arrow = (
  first: { present: number }["missing"],
): { ready: boolean }["absent"] =>
  (second: { open: string }["closed"]): { done: number }["gone"] => second;
const asserted = 1 as { value: number }["omitted"];
const constructed = new Crate<{ item: string }["lost"]>();
type Alias<
  T extends { constraint: number }["badConstraint"] = { fallback: string }["badDefault"],
> = T;
interface Vessel<
  T extends { interfaceConstraint: number }["badInterfaceConstraint"] = { interfaceDefault: string }["badInterfaceDefault"],
> {}
class Container<
  T extends { classConstraint: number }["badClassConstraint"] = { classDefault: string }["badClassDefault"],
> {
  field = 1 as { fieldValue: number }["badField"];
  constructor() {
    const nested = (value: { ctorValue: number }["badCtor"]): void => {};
  }
  method<
    U extends { methodConstraint: number }["badMethodConstraint"] = { methodDefault: string }["badMethodDefault"],
  >(): void {
    const nested = new Crate<{ bodyValue: number }["badBody"]>();
  }
}
function named<
  T extends { functionConstraint: number }["badFunctionConstraint"] = { functionDefault: string }["badFunctionDefault"],
>(value: T): void {}
"#;
    let expected = [
        ("missing", "{ present: number; }"),
        ("absent", "{ ready: boolean; }"),
        ("closed", "{ open: string; }"),
        ("gone", "{ done: number; }"),
        ("omitted", "{ value: number; }"),
        ("lost", "{ item: string; }"),
        ("badConstraint", "{ constraint: number; }"),
        ("badDefault", "{ fallback: string; }"),
        ("badInterfaceConstraint", "{ interfaceConstraint: number; }"),
        ("badInterfaceDefault", "{ interfaceDefault: string; }"),
        ("badClassConstraint", "{ classConstraint: number; }"),
        ("badClassDefault", "{ classDefault: string; }"),
        ("badField", "{ fieldValue: number; }"),
        ("badCtor", "{ ctorValue: number; }"),
        ("badMethodConstraint", "{ methodConstraint: number; }"),
        ("badMethodDefault", "{ methodDefault: string; }"),
        ("badBody", "{ bodyValue: number; }"),
        ("badFunctionConstraint", "{ functionConstraint: number; }"),
        ("badFunctionDefault", "{ functionDefault: string; }"),
    ];
    let output = compile(source);
    assert_eq!(codes(&output), vec![2339; expected.len()]);
    for (diagnostic, (property, receiver)) in output.diagnostics.iter().zip(expected) {
        let quoted = format!("\"{property}\"");
        assert_eq!(diagnostic.start, source.find(&quoted).unwrap() as u32);
        assert_eq!(diagnostic.length, quoted.len() as u32);
        assert_eq!(
            diagnostic.message_text,
            format!("Property '{property}' does not exist on type '{receiver}'.")
        );
    }
    assert_completion(&output, SemanticCompletion::Complete);
}

#[test]
fn explicit_type_boundary_keeps_supported_positive_forms_complete() {
    let output = compile(
        "declare class Crate<T> { value:T } \
         const arrow=(value:number):number=>value; \
         const nestedArrow=()=>(nested:number):number=>nested; \
         const asserted=1 as number; const constructed=new Crate<string>(); \
         type Alias<T extends {present:number}={present:number}> =T; \
         interface Vessel<T extends {present:number}={present:number}>{value:T} \
         class Container<T extends {present:number}={present:number}>{ \
           field=1 as number; \
           constructor(){const nested=(value:number):number=>value;} \
           method<U extends number=number>():void{const asserted=1 as number;} \
         } \
         function named<T extends {present:number}={present:number}>(value:T):void{}",
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_completion(&output, SemanticCompletion::Complete);
}

#[test]
fn property_diagnostic_origin_and_display_are_not_semantic_identity() {
    let source = "declare const root:{zeta:number;alpha:string;nested:{bravo:boolean;able:number}}; \
                  let first:typeof root   .   missing; \
                  let second:typeof root . nested   .   missing; \
                  root.missing; root.nested.missing;";
    let output = compile(source);
    let starts = source
        .match_indices("missing")
        .map(|(start, _)| start as u32)
        .collect::<Vec<_>>();
    assert_eq!(codes(&output), vec![2339, 2339, 2339, 2339]);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.start, diagnostic.length))
            .collect::<Vec<_>>(),
        starts
            .into_iter()
            .map(|start| (start, 7))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message_text.as_str())
            .collect::<Vec<_>>(),
        vec![
            "Property 'missing' does not exist on type '{ zeta: number; alpha: string; nested: { bravo: boolean; able: number; }; }'.",
            "Property 'missing' does not exist on type '{ bravo: boolean; able: number; }'.",
            "Property 'missing' does not exist on type '{ zeta: number; alpha: string; nested: { bravo: boolean; able: number; }; }'.",
            "Property 'missing' does not exist on type '{ bravo: boolean; able: number; }'.",
        ]
    );
    assert_completion(&output, SemanticCompletion::Complete);
}

#[test]
fn identical_property_queries_are_cold_warm_and_root_order_independent() {
    let shared = "declare const root:{zeta:number;alpha:string};";
    let first = "let first:typeof root  .  missing;";
    let second = "let second:typeof root . missing;";
    let forward_files = [("shared.ts", shared), ("a.ts", first), ("b.ts", second)];
    let reverse_files = [("b.ts", second), ("a.ts", first), ("shared.ts", shared)];
    let forward = compile_files(&forward_files);
    let warm = compile_files(&forward_files);
    let reversed = compile_files(&reverse_files);
    let records = |output: &tsz::CompileOutput| {
        output
            .diagnostics
            .iter()
            .map(|diagnostic| {
                (
                    diagnostic.file.clone(),
                    diagnostic.start,
                    diagnostic.length,
                    diagnostic.code,
                    diagnostic.message_text.clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(records(&forward), records(&warm));
    assert_eq!(records(&forward), records(&reversed));
    assert_eq!(codes(&forward), vec![2339, 2339]);
    assert!(forward.diagnostics.iter().all(|diagnostic| {
        diagnostic.message_text
            == "Property 'missing' does not exist on type '{ zeta: number; alpha: string; }'."
    }));
    assert_completion(&forward, SemanticCompletion::Complete);
    assert_completion(&warm, SemanticCompletion::Complete);
    assert_completion(&reversed, SemanticCompletion::Complete);
}

#[test]
fn unsupported_required_annotation_components_remain_deferred() {
    let cases = [
        "let direct:{value:keyof number};",
        "interface Nested { member:{value:keyof number} }",
        "type Wrapped={branch:{value:keyof number}};",
        "declare function ambient(value:{payload:keyof number}):keyof number;",
        "declare class Vessel { field:{payload:keyof number}; \
         method(value:{nested:keyof number}):keyof number; }",
    ];
    for source in cases {
        let output = compile(source);
        assert!(
            output.diagnostics.is_empty(),
            "{source}: {:?}",
            output.diagnostics
        );
        assert_completion(&output, SemanticCompletion::Deferred);
    }
}

#[test]
fn declaration_only_symbolic_hosts_validate_children_without_forcing_the_owner() {
    let source = concat!(
        "type Conditional<T> = T extends string\n",
        " ? {conditional:number}[\"missingTrue\"]\n",
        " : {fallback:boolean}[\"missingFalse\"];\n",
        "type Mapped = {[K in \"left\"|\"right\"]:{mapped:number}[\"missingMapped\"]};\n",
        "declare function predicate(value:unknown):value is {predicate:string}[\"missingPredicate\"];",
    );
    let output = compile(source);
    assert_eq!(codes(&output), vec![2339, 2339, 2339, 2339]);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message_text.as_str())
            .collect::<Vec<_>>(),
        [
            "Property 'missingTrue' does not exist on type '{ conditional: number; }'.",
            "Property 'missingFalse' does not exist on type '{ fallback: boolean; }'.",
            "Property 'missingMapped' does not exist on type '{ mapped: number; }'.",
            "Property 'missingPredicate' does not exist on type '{ predicate: string; }'.",
        ]
    );
    let tokens = [
        "\"missingTrue\"",
        "\"missingFalse\"",
        "\"missingMapped\"",
        "\"missingPredicate\"",
    ];
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.start, diagnostic.length))
            .collect::<Vec<_>>(),
        tokens
            .iter()
            .map(|token| (source.find(token).unwrap() as u32, token.len() as u32))
            .collect::<Vec<_>>()
    );
    assert!(
        output
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.related_information.is_empty())
    );
    assert_completion(&output, SemanticCompletion::Complete);
}

#[test]
fn relation_demand_still_forces_symbolic_conditional_and_mapped_owners() {
    let cases = [
        "type Choice<T> = T extends string?number:boolean; \
         declare const choice:Choice<unknown>; const result:number=choice;",
        "type Copy = {[K in \"value\"]:number}; \
         declare const copy:Copy; const result:{value:number}=copy;",
    ];
    for source in cases {
        let output = compile(source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_completion(&output, SemanticCompletion::Deferred);
    }
}

#[test]
fn required_annotation_cycles_use_active_type_identity() {
    let complete_cases = [
        "interface Link { next:Link }",
        "interface Cedar { branch:{next:Cedar} }",
        "type Node={next:Node}; let node:Node;",
        "type Wrapped={edge:{next:Wrapped}};",
        "declare class Branch { next:Branch; }",
    ];
    for source in complete_cases {
        let output = compile(source);
        assert!(
            output.diagnostics.is_empty(),
            "{source}: {:?}",
            output.diagnostics
        );
        assert_completion(&output, SemanticCompletion::Complete);
    }

    for source in ["type Loop=Loop;", "type Recurrence=Recurrence;"] {
        let output = compile(source);
        assert_eq!(
            codes(&output),
            vec![2456],
            "{source}: {:?}",
            output.diagnostics
        );
        assert_completion(&output, SemanticCompletion::Cycle);
    }
}

#[test]
fn indexed_access_preserves_positive_and_object_negative_cases() {
    let positive = compile(
        "type Present={ present:number }[\"present\"]; \
         let present:Present; const value:number=present;",
    );
    assert!(
        positive.diagnostics.is_empty(),
        "{:?}",
        positive.diagnostics
    );

    let negative = compile(
        "type Missing={ ready:boolean }[\"absent\"]; \
         let missing:Missing; const value:boolean=missing;",
    );
    assert_eq!(codes(&negative), vec![2339], "{:?}", negative.diagnostics);
}

#[test]
fn member_access_only_closes_over_modeled_object_properties() {
    let primitive = compile(
        "const text:string=''; const textSize:number=text.length; \
         const values:number[]=[]; const count:number=values.length;",
    );
    assert!(
        primitive.diagnostics.is_empty(),
        "{:?}",
        primitive.diagnostics
    );
    assert_completion(&primitive, SemanticCompletion::Deferred);

    let source = "declare const record:{ present:number }; record.missing;";
    let closed = compile(source);
    assert_eq!(codes(&closed), vec![2339], "{:?}", closed.diagnostics);
    assert_eq!(
        closed.diagnostics[0].message_text,
        "Property 'missing' does not exist on type '{ present: number; }'."
    );
    assert_eq!(
        (closed.diagnostics[0].start, closed.diagnostics[0].length),
        (source.find("missing").unwrap() as u32, 7)
    );
    assert_completion(&closed, SemanticCompletion::Complete);
}

#[test]
fn new_uses_the_canonical_bounded_class_instance() {
    let source = "class Vessel { item:string=\"x\"; } \
                  const instance=new Vessel(); const count:number=instance;";
    let output = compile(source);
    assert_eq!(codes(&output), vec![2322], "{:?}", output.diagnostics);
    assert_eq!(
        output.diagnostics[0].message_text,
        "Type 'Vessel' is not assignable to type 'number'."
    );

    let renamed = compile(
        "class Parcel { payload:boolean=false; } \
         const parcel:Parcel=new Parcel(); \
         const payload:boolean=parcel.payload; const text:string=parcel;",
    );
    assert_eq!(codes(&renamed), vec![2322], "{:?}", renamed.diagnostics);
    assert_eq!(
        renamed.diagnostics[0].message_text,
        "Type 'Parcel' is not assignable to type 'string'."
    );
}

#[test]
fn unsupported_class_construction_stays_incomplete_instead_of_any() {
    let cases = [
        "class Built { constructor() {} } \
         const built:Built=new Built();",
        "class Active { run():string { return \"x\"; } } \
         const active:Active=new Active();",
        "class Base {} class Derived extends Base {} \
         const derived:Derived=new Derived();",
        "class Inferred { item=\"x\"; } \
         const inferred:Inferred=new Inferred();",
        "class Secret { private value:number=1; } \
         const secret:Secret=new Secret();",
    ];
    for source in cases {
        let output = compile(source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_completion(&output, SemanticCompletion::Deferred);
    }

    let argument = compile("class Vessel {} const value=new Vessel(missingArgument);");
    assert_eq!(
        codes(&argument),
        vec![2304, 2554],
        "{:?}",
        argument.diagnostics
    );
    assert_completion(&argument, SemanticCompletion::Complete);
}

#[test]
fn implicit_default_constructors_own_zero_argument_arity() {
    let source = "class C {} new C(123); new C();";
    let output = compile(source);
    assert_eq!(codes(&output), vec![2554], "{:?}", output.diagnostics);
    assert_eq!(
        output.diagnostics[0].message_text,
        "Expected 0 arguments, but got 1."
    );
    assert_eq!(
        (output.diagnostics[0].start, output.diagnostics[0].length),
        (source.find("123").unwrap() as u32, 3)
    );

    let renamed = compile(
        "class Vessel { item:string=\"x\"; } new Vessel(true, false); \
         class Explicit { constructor(value:number) {} } new Explicit(1);",
    );
    assert_eq!(codes(&renamed), vec![2554], "{:?}", renamed.diagnostics);
    assert_eq!(
        renamed.diagnostics[0].message_text,
        "Expected 0 arguments, but got 2."
    );

    let inherited = compile(
        "class Parent { constructor(value:number) {} } \
         class Child extends Parent {} new Child(1);",
    );
    assert!(
        inherited.diagnostics.is_empty(),
        "{:?}",
        inherited.diagnostics
    );
}

#[test]
fn named_function_returns_are_inferred_without_any_fallback() {
    let output = compile(
        "function f(){ return \"x\"; } const fromText:number=f(); \
         function g(){} const fromVoid:number=g();",
    );
    assert_eq!(codes(&output), vec![2322, 2322], "{:?}", output.diagnostics);
    assert_eq!(
        output.diagnostics[0].message_text,
        "Type 'string' is not assignable to type 'number'."
    );
    assert_eq!(
        output.diagnostics[1].message_text,
        "Type 'void' is not assignable to type 'number'."
    );

    let renamed = compile(
        "function label(){ return \"ok\"; } const text:string=label(); \
         function annotated():number { return 1; } const count:number=annotated();",
    );
    assert!(renamed.diagnostics.is_empty(), "{:?}", renamed.diagnostics);
}

#[test]
fn multiple_function_returns_form_a_bounded_union() {
    let output = compile(
        "function choose(flag:boolean) { \
           if (flag) { return \"yes\"; } else { return 1; } \
         } \
         const value:string|number=choose(true); \
         const wrong:boolean=choose(false);",
    );
    assert_eq!(codes(&output), vec![2322], "{:?}", output.diagnostics);
}

#[test]
fn unsupported_function_control_flow_stays_incomplete() {
    let switch_body = compile(
        "function choose(flag:boolean) { \
           switch (flag) { case true: return \"yes\"; default: return \"no\"; } \
         } \
         const text:string=choose(true);",
    );
    assert!(
        switch_body.diagnostics.is_empty(),
        "{:?}",
        switch_body.diagnostics
    );
    assert_completion(&switch_body, SemanticCompletion::Deferred);

    let partial = compile(
        "function partial(flag:boolean) { if (flag) return \"yes\"; } \
         const text:string|undefined=partial(true);",
    );
    assert!(partial.diagnostics.is_empty(), "{:?}", partial.diagnostics);
    assert_completion(&partial, SemanticCompletion::Deferred);
}

#[test]
fn keyof_closed_object_shapes_preserves_their_exact_keys() {
    let output = compile(
        "type Fields={ left:number; right:string }; type Names=keyof Fields; \
         let name:Names; const exact:\"left\"|\"right\"=name; \
         const wrong:number=name;",
    );
    assert_eq!(codes(&output), vec![2322], "{:?}", output.diagnostics);

    let renamed = compile(
        "class Vessel { item:string=\"x\"; } type VesselKeys=keyof Vessel; \
         let key:VesselKeys; const item:\"item\"=key;",
    );
    assert!(renamed.diagnostics.is_empty(), "{:?}", renamed.diagnostics);
}

#[test]
fn keyof_generic_aliases_decide_only_after_a_modeled_substitution() {
    let output = compile(
        "type Names<T> = keyof T; \
         let name:Names<{ alpha:number; beta:boolean }>; \
         const exact:\"alpha\"|\"beta\"=name; const wrong:number=name;",
    );
    assert_eq!(codes(&output), vec![2322], "{:?}", output.diagnostics);

    let primitive = compile(
        "type Names<T> = keyof T; let name:Names<number>; \
         const wrong:number=name;",
    );
    assert!(
        primitive.diagnostics.is_empty(),
        "{:?}",
        primitive.diagnostics
    );
    assert_completion(&primitive, SemanticCompletion::Deferred);
}

#[test]
fn keyof_primitive_is_incomplete_and_keyof_any_is_property_key() {
    let primitive = compile("type Keys=keyof number; let key:Keys; const value:number=key;");
    assert!(
        primitive.diagnostics.is_empty(),
        "{:?}",
        primitive.diagnostics
    );
    assert_completion(&primitive, SemanticCompletion::Deferred);

    let library_member = compile(
        "type Length=string[\"length\"]; let length:Length; \
         const value:number=length;",
    );
    assert!(
        library_member.diagnostics.is_empty(),
        "{:?}",
        library_member.diagnostics
    );
    assert_completion(&library_member, SemanticCompletion::Deferred);

    let any_keys = compile(
        "type Keys=keyof any; let key:Keys; \
         const valid:string|number|symbol=key; const wrong:number=key;",
    );
    assert_eq!(codes(&any_keys), vec![2322], "{:?}", any_keys.diagnostics);
    assert_completion(&any_keys, SemanticCompletion::Complete);

    let never_keys = compile(
        "type Keys=keyof never; let key:Keys; \
         const valid:string|number|symbol=key; const wrong:boolean=key;",
    );
    assert_eq!(
        codes(&never_keys),
        vec![2322],
        "{:?}",
        never_keys.diagnostics
    );
    assert_completion(&never_keys, SemanticCompletion::Complete);
}

#[test]
fn qualified_type_queries_preserve_every_property_projection() {
    let output = compile(
        "const root={ leaf:\"text\" }; \
         const exact:typeof root.leaf=\"text\"; \
         const wrong:typeof root.leaf={ leaf:\"text\" }; \
         const vessel={ payload:{ count:1 } }; \
         const nested:typeof vessel.payload.count=\"wrong\";",
    );
    assert_eq!(codes(&output), vec![2322, 2322], "{:?}", output.diagnostics);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message_text.as_str())
            .collect::<Vec<_>>(),
        vec![
            "Type '{ leaf: string; }' is not assignable to type 'string'.",
            "Type 'string' is not assignable to type 'number'.",
        ]
    );
    assert_completion(&output, SemanticCompletion::Complete);

    let renamed = compile(
        "const container={ branch:{ enabled:true } }; \
         const exact:typeof container.branch.enabled=true;",
    );
    assert!(renamed.diagnostics.is_empty(), "{:?}", renamed.diagnostics);
    assert_completion(&renamed, SemanticCompletion::Complete);
}

#[test]
fn predicate_types_are_nonclaims_until_narrowing_owns_their_semantics() {
    let ambient_invalid = compile("declare function isValue(actual:unknown):missing is string;");
    assert!(
        ambient_invalid.diagnostics.is_empty(),
        "an unsupported ambient predicate must remain a semantic nonclaim: {:?}",
        ambient_invalid.diagnostics
    );
    assert_completion(&ambient_invalid, SemanticCompletion::Deferred);

    let asserts_invalid =
        compile("function requireValue(actual:unknown):asserts missing is string {}");
    assert!(
        asserts_invalid.diagnostics.is_empty(),
        "an unsupported assertion predicate must remain a semantic nonclaim: {:?}",
        asserts_invalid.diagnostics
    );
    assert_completion(&asserts_invalid, SemanticCompletion::Deferred);

    let invalid_parameter = compile(
        "function isValue(actual:unknown):missing is string { return true; } \
         const result:boolean=isValue(\"text\");",
    );
    assert!(
        invalid_parameter.diagnostics.is_empty(),
        "an unsupported predicate must not fabricate an unrelated diagnostic: {:?}",
        invalid_parameter.diagnostics
    );
    assert_completion(&invalid_parameter, SemanticCompletion::Deferred);

    let renamed_valid_shape = compile(
        "function isText(candidate:unknown):candidate is string { return true; } \
         const result:boolean=isText(\"text\");",
    );
    assert!(
        renamed_valid_shape.diagnostics.is_empty(),
        "{:?}",
        renamed_valid_shape.diagnostics
    );
    assert_completion(&renamed_valid_shape, SemanticCompletion::Deferred);
}

#[test]
fn builtin_fast_paths_follow_resolved_library_identity_not_user_spelling() {
    let shadowed_array = compile(
        "function checkShadow() { \
           type Array<T> = { boxed: T }; \
           const value: Array<string> = [\"text\"]; \
         }",
    );
    assert!(
        shadowed_array.diagnostics.is_empty(),
        "unmodeled array members must not fabricate an incompatibility: {:?}",
        shadowed_array.diagnostics
    );
    assert_completion(&shadowed_array, SemanticCompletion::Deferred);

    let empty_object = compile("const visible:{} = [\"text\"];");
    assert!(
        empty_object.diagnostics.is_empty(),
        "{:?}",
        empty_object.diagnostics
    );
    assert_completion(&empty_object, SemanticCompletion::Complete);

    let ambient_array = compile("const values:Array<number> = [1, 2];");
    assert!(
        ambient_array.diagnostics.is_empty(),
        "{:?}",
        ambient_array.diagnostics
    );
    assert_completion(&ambient_array, SemanticCompletion::Complete);

    let shadowed_undefined = compile(
        "function check(undefined:number) { \
           const value:undefined=undefined; \
         }",
    );
    assert_eq!(codes(&shadowed_undefined), vec![2322]);
    assert_completion(&shadowed_undefined, SemanticCompletion::Complete);

    let ambient_undefined = compile("const value:undefined=undefined;");
    assert!(
        ambient_undefined.diagnostics.is_empty(),
        "{:?}",
        ambient_undefined.diagnostics
    );
    assert_completion(&ambient_undefined, SemanticCompletion::Complete);
}

#[test]
fn array_relation_diagnostics_preserve_root_pairs_and_nested_causes() {
    let source = concat!(
        "const fresh=[\"other\"]; const assigned:\"seed\"[]=fresh;\n",
        "function consume(values:\"seed\"[]){} const argumentValues=[\"other\"]; consume(argumentValues);\n",
        "function produce():\"seed\"[]{ const returnValues=[\"other\"]; return returnValues; }\n",
        "declare const unionValues:(\"left\"|\"right\")[]; const fromUnion:\"seed\"[]=unionValues;\n",
        "const nested=[[\"other\"]]; const nestedTarget:\"seed\"[][]=nested;\n",
        "const regular:string[]=[\"seed\"]; const regularTarget:string[]=regular;",
    );
    let output = compile(source);
    assert_eq!(
        codes(&output),
        vec![2322, 2345, 2322, 2322, 2322],
        "{:?}",
        output.diagnostics
    );
    let fingerprints = output
        .diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.message_text.as_str(),
                diagnostic
                    .related_information
                    .iter()
                    .map(|related| (related.depth, related.message_text.as_str()))
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        fingerprints,
        vec![
            (
                "Type 'string[]' is not assignable to type '\"seed\"[]'.",
                vec![(1, "Type 'string' is not assignable to type '\"seed\"'.")],
            ),
            (
                "Argument of type 'string[]' is not assignable to parameter of type '\"seed\"[]'.",
                vec![(1, "Type 'string' is not assignable to type '\"seed\"'.")],
            ),
            (
                "Type 'string[]' is not assignable to type '\"seed\"[]'.",
                vec![(1, "Type 'string' is not assignable to type '\"seed\"'.")],
            ),
            (
                "Type '(\"left\" | \"right\")[]' is not assignable to type '\"seed\"[]'.",
                vec![
                    (
                        1,
                        "Type '\"left\" | \"right\"' is not assignable to type '\"seed\"'."
                    ),
                    (2, "Type '\"left\"' is not assignable to type '\"seed\"'."),
                ],
            ),
            (
                "Type 'string[][]' is not assignable to type '\"seed\"[][]'.",
                vec![
                    (1, "Type 'string[]' is not assignable to type '\"seed\"[]'."),
                    (2, "Type 'string' is not assignable to type '\"seed\"'."),
                ],
            ),
        ]
    );
}

#[test]
fn contextual_array_literals_report_the_failing_element_once() {
    let source = concat!(
        "const direct:\"seed\"[]=[\"other\"];\n",
        "function consume(values:\"seed\"[]){} consume([\"other\"]);\n",
        "function produce():\"seed\"[]{ return [\"other\"]; }\n",
        "declare const choice:\"left\"|\"right\"; const unionTarget:\"seed\"[]=[choice];\n",
        "const positive:\"seed\"[]=[\"seed\"];",
    );
    let output = compile(source);
    assert_eq!(codes(&output), vec![2322; 4], "{:?}", output.diagnostics);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message_text.as_str())
            .collect::<Vec<_>>(),
        [
            "Type '\"other\"' is not assignable to type '\"seed\"'.",
            "Type '\"other\"' is not assignable to type '\"seed\"'.",
            "Type '\"other\"' is not assignable to type '\"seed\"'.",
            "Type '\"left\" | \"right\"' is not assignable to type '\"seed\"'.",
        ]
    );
    assert!(
        output.diagnostics[..3]
            .iter()
            .all(|diagnostic| diagnostic.related_information.is_empty())
    );
    assert_eq!(
        output.diagnostics[3]
            .related_information
            .iter()
            .map(|related| (related.depth, related.message_text.as_str()))
            .collect::<Vec<_>>(),
        [(1, "Type '\"left\"' is not assignable to type '\"seed\"'.")]
    );
}

#[test]
fn contextual_array_literals_report_every_bad_leaf_in_source_order() {
    let source = concat!(
        "const direct:\"seed\"[]=[\"first\",\"second\"];\n",
        "const mixed:\"seed\"[]=[\"seed\",\"other\",\"seed\",\"wrong\"];\n",
        "const nested:\"seed\"[][]=[[\"one\",\"two\"],[\"seed\"],[\"three\"]];\n",
        "function accept(values:\"seed\"[]){} accept([\"callOne\",\"seed\",\"callTwo\"]);\n",
        "function produce():\"seed\"[]{ return [\"returnOne\",\"returnTwo\"]; }\n",
        "const wrapped:\"seed\"[]=[(\"wrapped\"),((\"deep\"))];\n",
        "const positive:\"seed\"[][]=[[\"seed\"],[\"seed\",\"seed\"]];",
    );
    let output = compile(source);
    let bad_values = [
        "first",
        "second",
        "other",
        "wrong",
        "one",
        "two",
        "three",
        "callOne",
        "callTwo",
        "returnOne",
        "returnTwo",
        "wrapped",
        "deep",
    ];
    assert_eq!(codes(&output), vec![2322; bad_values.len()]);
    for (diagnostic, value) in output.diagnostics.iter().zip(bad_values) {
        let quoted = format!("\"{value}\"");
        assert_eq!(diagnostic.start, source.find(&quoted).unwrap() as u32);
        assert_eq!(diagnostic.length, quoted.len() as u32);
        assert_eq!(
            diagnostic.message_text,
            format!("Type '{quoted}' is not assignable to type '\"seed\"'.")
        );
        assert!(diagnostic.related_information.is_empty());
    }
    assert_completion(&output, SemanticCompletion::Complete);
}

#[test]
fn array_union_targets_keep_canonical_outer_and_contextual_diagnostics() {
    let source = concat!(
        "const contextual:(\"b\"[]|\"a\"[])=[\"other\"];\n",
        "declare const values:string[]; const assigned:\"b\"[]|\"a\"[]=values;",
    );
    let output = compile(source);
    assert_eq!(codes(&output), vec![2322, 2322], "{:?}", output.diagnostics);
    assert_eq!(
        output.diagnostics[0].message_text,
        "Type '\"other\"' is not assignable to type '\"a\" | \"b\"'."
    );
    assert!(output.diagnostics[0].related_information.is_empty());
    assert_eq!(
        output.diagnostics[1].message_text,
        "Type 'string[]' is not assignable to type '\"a\"[] | \"b\"[]'."
    );
    assert_eq!(
        output.diagnostics[1]
            .related_information
            .iter()
            .map(|related| (related.depth, related.message_text.as_str()))
            .collect::<Vec<_>>(),
        [
            (1, "Type 'string[]' is not assignable to type '\"a\"[]'."),
            (2, "Type 'string' is not assignable to type '\"a\"'."),
        ]
    );
}

#[test]
fn array_union_diagnostics_are_root_order_and_cold_warm_stable() {
    let options = CompilerOptions {
        no_emit: true,
        strict: true,
        ..CompilerOptions::default()
    };
    let declarations = SourceInput::new(
        "allocations.ts",
        Arc::<str>::from("type Z=\"z\"[]; type A=\"a\"[]; declare const unused:Z|A;"),
    );
    let witness = SourceInput::new(
        "witness.ts",
        Arc::<str>::from("declare const values:string[]; const assigned:\"b\"[]|\"a\"[]=values;"),
    );
    let forward = vec![declarations.clone(), witness.clone()];
    let reverse = vec![witness, declarations];
    let fingerprint = |inputs| {
        let output = Compiler::new().compile(inputs, &options);
        assert_completion(&output, SemanticCompletion::Complete);
        serde_json::to_vec(&output.diagnostics).unwrap()
    };
    let expected = fingerprint(forward.clone());
    for iteration in 0..5 {
        assert_eq!(
            fingerprint(forward.clone()),
            expected,
            "warm run {iteration}"
        );
        assert_eq!(
            fingerprint(reverse.clone()),
            expected,
            "root order {iteration}"
        );
    }
}

#[test]
fn numeric_union_order_is_allocation_root_and_repeated_run_stable() {
    let options = CompilerOptions {
        no_emit: true,
        strict: true,
        ..CompilerOptions::default()
    };
    let allocations = SourceInput::new(
        "numeric-allocations.ts",
        Arc::<str>::from("type Ten=10[]; type Two=2[]; declare const allocations:Ten|Two;"),
    );
    let witness = SourceInput::new(
        "numeric-witness.ts",
        Arc::<str>::from("declare const broad:number[]; const target:10[]|2[]=broad;"),
    );
    let fingerprint = |inputs| {
        let output = Compiler::new().compile(inputs, &options);
        assert_eq!(codes(&output), vec![2322]);
        assert_eq!(
            output.diagnostics[0].message_text,
            "Type 'number[]' is not assignable to type '2[] | 10[]'."
        );
        serde_json::to_vec(&output.diagnostics).unwrap()
    };
    let forward = vec![allocations.clone(), witness.clone()];
    let reverse = vec![witness, allocations];
    let expected = fingerprint(forward.clone());
    for _ in 0..3 {
        assert_eq!(fingerprint(forward.clone()), expected);
        assert_eq!(fingerprint(reverse.clone()), expected);
    }
}

#[test]
fn renamed_fresh_array_value_aliases_keep_the_same_diagnostic_shape() {
    let output = compile(
        "const sourceValues=[\"other\"]; const renamedValues=sourceValues; \
         const destination:\"seed\"[]=renamedValues;",
    );
    assert_eq!(codes(&output), vec![2322], "{:?}", output.diagnostics);
    assert_eq!(
        output.diagnostics[0].message_text,
        "Type 'string[]' is not assignable to type '\"seed\"[]'."
    );
    assert_eq!(output.diagnostics[0].related_information.len(), 1);

    // Alias display provenance is a distinct follow-up: the relation must
    // remain a definitive failure rather than becoming Deferred or passing.
    let named_type_alias = compile(
        "type SeedList=\"seed\"[]; const values=[\"other\"]; \
         const destination:SeedList=values;",
    );
    assert_eq!(
        codes(&named_type_alias),
        vec![2322],
        "{:?}",
        named_type_alias.diagnostics
    );
    assert_completion(&named_type_alias, SemanticCompletion::Complete);
}

#[test]
fn call_site_union_policy_matches_literal_reduction_and_numeric_order() {
    let output = compile(
        "declare const numerics:(10|2)[]; const one:1[]=numerics; \
         declare const flags:(true|false)[]; const truth:true[]=flags; \
         declare const text:(\"literal\"|string)[]; const seed:\"seed\"[]=text; \
         declare const reachable:(never|\"literal\")[]; const other:\"seed\"[]=reachable; \
         declare const permissive:(any|\"literal\")[]; const accepted:\"seed\"[]=permissive; \
         declare const dominantAny:(unknown|any)[]; const alsoAccepted:\"seed\"[]=dominantAny; \
         declare const opaque:(unknown|\"literal\")[]; const rejected:\"seed\"[]=opaque;",
    );
    assert_eq!(
        codes(&output),
        vec![2322, 2322, 2322, 2322, 2322],
        "{:?}",
        output.diagnostics
    );
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message_text.as_str())
            .collect::<Vec<_>>(),
        [
            "Type '(2 | 10)[]' is not assignable to type '1[]'.",
            "Type 'boolean[]' is not assignable to type 'true[]'.",
            "Type 'string[]' is not assignable to type '\"seed\"[]'.",
            "Type '\"literal\"[]' is not assignable to type '\"seed\"[]'.",
            "Type 'unknown[]' is not assignable to type '\"seed\"[]'.",
        ]
    );
    assert_completion(&output, SemanticCompletion::Complete);
}

#[test]
fn contextual_union_objects_report_property_leaves_for_all_entry_points() {
    let source = concat!(
        "const assigned:{kind:\"a\";value:\"x\"}[]|{kind:\"b\";value:\"y\"}[]=[{kind:\"b\",value:\"wrong\"}];\n",
        "function consume(values:{kind:\"a\";value:\"x\"}[]|{kind:\"b\";value:\"y\"}[]){} consume([{kind:\"b\",value:\"callWrong\"}]);\n",
        "function produce():{kind:\"b\";value:\"y\"}[]|{kind:\"a\";value:\"x\"}[]{return [{kind:\"b\",value:\"returnWrong\"}];}\n",
        "const nested:{tag:\"l\";box:{item:\"x\"}}[]|{tag:\"r\";box:{item:\"y\"}}[]=[{tag:\"r\",box:{item:\"deepWrong\"}}];\n",
        "const positive:{kind:\"a\";value:\"x\"}[]|{kind:\"b\";value:\"y\"}[]=[{kind:\"b\",value:\"y\"}];",
    );
    let output = compile(source);
    assert_eq!(codes(&output), vec![2322, 2322, 2322, 2322]);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message_text.as_str())
            .collect::<Vec<_>>(),
        [
            "Type '\"wrong\"' is not assignable to type '\"x\" | \"y\"'.",
            "Type '\"callWrong\"' is not assignable to type '\"x\" | \"y\"'.",
            "Type '\"returnWrong\"' is not assignable to type '\"x\" | \"y\"'.",
            "Type '\"deepWrong\"' is not assignable to type '\"x\" | \"y\"'.",
        ]
    );
    for diagnostic in &output.diagnostics {
        let highlighted =
            &source[diagnostic.start as usize..(diagnostic.start + diagnostic.length) as usize];
        assert!(matches!(highlighted, "value" | "item"), "{highlighted}");
        assert!(diagnostic.related_information.is_empty());
    }
    assert_completion(&output, SemanticCompletion::Complete);
}

#[test]
fn indexed_and_construct_queries_keep_per_use_origins_outside_identity() {
    let source = concat!(
        "type First={zeta:number;alpha:string}[\"missing\"];\n",
        "type Second={alpha:string;zeta:number}[\"missing\"];\n",
        "class Vessel{} new Vessel(1); new Vessel(2);",
    );
    let output = compile(source);
    assert_eq!(codes(&output), vec![2339, 2339, 2554, 2554]);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message_text.as_str())
            .collect::<Vec<_>>(),
        [
            "Property 'missing' does not exist on type '{ zeta: number; alpha: string; }'.",
            "Property 'missing' does not exist on type '{ alpha: string; zeta: number; }'.",
            "Expected 0 arguments, but got 1.",
            "Expected 0 arguments, but got 1.",
        ]
    );
    let starts = output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.start)
        .collect::<Vec<_>>();
    assert_eq!(starts[0], source.find("\"missing\"").unwrap() as u32);
    assert_eq!(starts[1], source.rfind("\"missing\"").unwrap() as u32);
    assert_eq!(starts[2], source.find("(1)").unwrap() as u32 + 1);
    assert_eq!(starts[3], source.find("(2)").unwrap() as u32 + 1);
}

#[test]
fn indexed_origins_are_root_order_and_repeated_run_stable() {
    let compiler = Compiler::new();
    let options = CompilerOptions {
        no_emit: true,
        strict: true,
        ..CompilerOptions::default()
    };
    let first = SourceInput::new(
        "a.ts",
        Arc::<str>::from("type A={zeta:number;alpha:string}[\"missing\"];"),
    );
    let second = SourceInput::new(
        "b.ts",
        Arc::<str>::from("type B={alpha:string;zeta:number}[\"missing\"];"),
    );
    let fingerprint = |inputs| {
        let output = compiler.compile(inputs, &options);
        assert_eq!(codes(&output), vec![2339, 2339]);
        serde_json::to_vec(&output.diagnostics).unwrap()
    };
    let forward = vec![first.clone(), second.clone()];
    let reverse = vec![second, first];
    let expected = fingerprint(forward.clone());
    for _ in 0..3 {
        assert_eq!(fingerprint(forward.clone()), expected);
        assert_eq!(fingerprint(reverse.clone()), expected);
    }
}

#[test]
fn conditional_infer_bindings_are_true_branch_local_in_required_types() {
    let accepted = compile(
        "type Element<T> = T extends (infer Item)[] ? Item : never; \
         type Nested<T> = T extends {payload:infer Value extends string} ? {kept:Value} : never; \
         type Renamed<T> = T extends [infer Result] ? Result : never;",
    );
    assert!(
        accepted.diagnostics.is_empty(),
        "{:?}",
        accepted.diagnostics
    );
    assert_completion(&accepted, SemanticCompletion::Complete);

    let rejected = compile(
        "type FalseLeak<T> = T extends infer Hidden ? string : Hidden; \
         type Constraint<T> = T extends infer Bound extends MissingConstraint ? Bound : never;",
    );
    assert_eq!(codes(&rejected), vec![2304, 2304]);
    assert!(rejected.diagnostics[0].message_text.contains("Hidden"));
    assert!(
        rejected.diagnostics[1]
            .message_text
            .contains("MissingConstraint")
    );
}

#[test]
fn authored_object_union_order_and_reason_tree_match_each_source_order() {
    let output = compile(
        "declare const left:{kind:\"b\";value:string}[]; \
         const first:{kind:\"a\";value:\"x\"}[]|{kind:\"b\";value:\"y\"}[]=left; \
         declare const right:{kind:\"a\";value:string}[]; \
         const second:{kind:\"b\";value:\"y\"}[]|{kind:\"a\";value:\"x\"}[]=right;",
    );
    assert_eq!(codes(&output), vec![2322, 2322]);
    assert!(
        output.diagnostics[0]
            .message_text
            .ends_with("'{ kind: \"a\"; value: \"x\"; }[] | { kind: \"b\"; value: \"y\"; }[]'.")
    );
    assert!(
        output.diagnostics[1]
            .message_text
            .ends_with("'{ kind: \"b\"; value: \"y\"; }[] | { kind: \"a\"; value: \"x\"; }[]'.")
    );
    for diagnostic in &output.diagnostics {
        assert_eq!(diagnostic.related_information.len(), 4);
        assert!(
            diagnostic.related_information[2]
                .message_text
                .starts_with("Types of property 'kind'")
        );
        assert_eq!(diagnostic.related_information[3].depth, 4);
    }
}

#[test]
fn evaluator_limit_dominates_cycle_and_deferred_without_becoming_success() {
    let mut source =
        String::from("type PrimitiveKeys=keyof number; type Loop=Loop; let loopValue:Loop; ");
    for index in 0..=102 {
        source.push_str(&format!("type Chain{index}=Chain{}; ", index + 1));
    }
    source.push_str("type Chain103=number; let value:Chain0;");

    let output = compile(&source);
    assert_completion(&output, SemanticCompletion::Limit);
    assert!(
        codes(&output).contains(&2456),
        "cycle witness should keep its real TypeScript diagnostic: {:?}",
        output.diagnostics
    );
}

#[test]
fn numeric_value_identity_mixed_union_order_and_alias_display_match_ts7() {
    let output = compile(
        "declare const unsafeValues:(9007199254740993|9007199254740992)[]; \
         const unsafeTarget:1[]=unsafeValues; \
         declare const exponentValues:(1e3|2e1|3)[]; \
         const exponentTarget:1[]=exponentValues; \
         declare const nullish:(undefined|\"z\"|null|\"a\")[]; \
         const nullishTarget:\"seed\"[]=nullish; \
         type Cedar={kind:\"c\";value:\"x\"}[]; \
         type Birch={kind:\"b\";value:\"y\"}[]; \
         declare const values:{kind:\"b\";value:string}[]; \
         const aliases:Birch|Cedar=values;",
    );
    assert_eq!(codes(&output), vec![2322, 2322, 2322, 2322]);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message_text.as_str())
            .collect::<Vec<_>>(),
        [
            "Type '9007199254740992[]' is not assignable to type '1[]'.",
            "Type '(3 | 20 | 1000)[]' is not assignable to type '1[]'.",
            "Type '(\"a\" | \"z\" | null | undefined)[]' is not assignable to type '\"seed\"[]'.",
            "Type '{ kind: \"b\"; value: string; }[]' is not assignable to type 'Birch | Cedar'.",
        ]
    );
    assert_eq!(
        output.diagnostics[2]
            .related_information
            .last()
            .map(|related| related.message_text.as_str()),
        Some("Type 'undefined' is not assignable to type '\"seed\"'.")
    );
    assert_eq!(
        output.diagnostics[3].related_information[0].message_text,
        "Type '{ kind: \"b\"; value: string; }[]' is not assignable to type 'Birch'."
    );
    assert_completion(&output, SemanticCompletion::Complete);
}

#[test]
fn collapsed_and_keyword_boolean_keep_their_canonical_mixed_union_position() {
    let output = compile(
        "declare const collapsed:undefined|null|\"z\"|\"a\"|3|2|true|false; \
         const first:never=collapsed; \
         declare const nested:(true|false)|\"z\"|3|null; const second:never=nested; \
         declare const keyword:boolean|\"z\"|3|null; const third:never=keyword;",
    );
    assert_eq!(codes(&output), vec![2322, 2322, 2322]);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message_text.as_str())
            .collect::<Vec<_>>(),
        [
            "Type '\"a\" | \"z\" | 2 | 3 | boolean | null | undefined' is not assignable to type 'never'.",
            "Type '\"z\" | 3 | boolean | null' is not assignable to type 'never'.",
            "Type '\"z\" | 3 | boolean | null' is not assignable to type 'never'.",
        ]
    );
    assert_completion(&output, SemanticCompletion::Complete);
}

#[test]
fn discriminated_contexts_unlock_only_branch_exclusive_properties() {
    let source = concat!(
        "type Variant={tag:\"left\";left:\"x\"}|{tag:\"right\";right:\"y\"};\n",
        "const assigned:Variant[]=[{tag:\"right\",right:\"wrong\"},{tag:\"right\",right:\"y\"}];\n",
        "function consume(values:Variant[]){} consume([{tag:\"right\",right:\"callWrong\"}]);\n",
        "function produce():Variant[]{return [{tag:\"right\",right:\"returnWrong\"}];}\n",
        "type Nested={mode:\"a\";payload:{renamed:\"one\"}}|{mode:\"b\";payload:{renamed:\"two\"}};\n",
        "const nested:Nested[]=[{mode:\"b\",payload:{renamed:\"nestedWrong\"}},{mode:\"b\",payload:{renamed:\"two\"}}];\n",
        "type Reversed={code:\"b\";renamed:\"two\"}|{code:\"a\";renamed:\"one\"};\n",
        "const reversed:Reversed[]=[{code:\"a\",renamed:\"reverseWrong\"},{code:\"a\",renamed:\"one\"}];",
    );
    let output = compile(source);
    assert_eq!(codes(&output), vec![2322; 5], "{:?}", output.diagnostics);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message_text.as_str())
            .collect::<Vec<_>>(),
        [
            "Type '\"wrong\"' is not assignable to type '\"y\"'.",
            "Type '\"callWrong\"' is not assignable to type '\"y\"'.",
            "Type '\"returnWrong\"' is not assignable to type '\"y\"'.",
            "Type '\"nestedWrong\"' is not assignable to type '\"one\" | \"two\"'.",
            "Type '\"reverseWrong\"' is not assignable to type '\"one\" | \"two\"'.",
        ]
    );
    for diagnostic in &output.diagnostics {
        assert!(diagnostic.related_information.is_empty());
    }
    assert_completion(&output, SemanticCompletion::Complete);
}

#[test]
fn branch_exclusive_property_presence_supplies_context_without_a_tag() {
    let source = concat!(
        "type Exclusive={left:\"x\"}|{right:\"y\"};\n",
        "const assigned:Exclusive[]=[{right:\"y\"},{right:\"wrong\"}];\n",
        "function consume(values:Exclusive[]){} consume([{right:\"y\"},{right:\"callWrong\"}]);\n",
        "function produce():Exclusive[]{return [{right:\"y\"},{right:\"returnWrong\"}];}\n",
        "type Renamed={cedar:\"c\"}|{birch:\"b\"};\n",
        "const accepted:Renamed[]=[{birch:\"b\"},{cedar:\"c\"}];",
    );
    let output = compile(source);
    assert_eq!(codes(&output), vec![2322, 2322, 2322]);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message_text.as_str())
            .collect::<Vec<_>>(),
        [
            "Type '\"wrong\"' is not assignable to type '\"y\"'.",
            "Type '\"callWrong\"' is not assignable to type '\"y\"'.",
            "Type '\"returnWrong\"' is not assignable to type '\"y\"'.",
        ]
    );
    assert!(
        output
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.related_information.is_empty())
    );
    assert_completion(&output, SemanticCompletion::Complete);
}

#[test]
fn authored_property_order_owns_paths_and_missing_aggregates() {
    let output = compile(
        "declare const direct:{zeta:string;alpha:string}[]; \
         const directTarget:{zeta:\"z\";alpha:\"a\"}[]=direct; \
         declare const nested:{payload:{zeta:string;alpha:string}}[]; \
         const nestedTarget:{payload:{zeta:\"z\";alpha:\"a\"}}[]=nested; \
         declare const missing:{present:number}[]; \
         const missingTarget:{zeta:string;alpha:number}[]=missing;",
    );
    assert_eq!(codes(&output), vec![2322, 2322, 2322]);
    let related = |index: usize| {
        output.diagnostics[index]
            .related_information
            .iter()
            .map(|related| related.message_text.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        related(0),
        [
            "Type '{ zeta: string; alpha: string; }' is not assignable to type '{ zeta: \"z\"; alpha: \"a\"; }'.",
            "Types of property 'zeta' are incompatible.",
            "Type 'string' is not assignable to type '\"z\"'.",
        ]
    );
    assert_eq!(
        related(1)[1],
        "The types of 'payload.zeta' are incompatible between these types."
    );
    assert_eq!(
        related(2),
        [
            "Type '{ present: number; }' is missing the following properties from type '{ zeta: string; alpha: number; }': zeta, alpha"
        ]
    );
    assert_completion(&output, SemanticCompletion::Complete);
}

#[test]
fn indexed_union_keys_keep_every_reason_and_alias_receiver() {
    let source = concat!(
        "type Ordered={zeta:number;alpha:string};\n",
        "type Missing=Ordered[\"second\"|\"first\"];\n",
        "type Repeated={present:number}[\"fourth\"|\"third\"];",
    );
    let output = compile(source);
    assert_eq!(codes(&output), vec![2339; 4]);
    assert_eq!(output.diagnostics[0].start, output.diagnostics[1].start);
    assert_eq!(output.diagnostics[2].start, output.diagnostics[3].start);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message_text.as_str())
            .collect::<Vec<_>>(),
        [
            "Property 'first' does not exist on type 'Ordered'.",
            "Property 'second' does not exist on type 'Ordered'.",
            "Property 'fourth' does not exist on type '{ present: number; }'.",
            "Property 'third' does not exist on type '{ present: number; }'.",
        ]
    );
    assert_completion(&output, SemanticCompletion::Complete);
}

#[test]
fn unresolved_error_graphs_cascade_without_becoming_incomplete_or_cached_success() {
    let source = concat!(
        "type Direct=Missing; type Nested=Missing|string;\n",
        "type Conditional<T> = Missing extends T ? string : number; type Keys = keyof Missing;\n",
        "declare const direct:Direct; declare const nested:Nested;\n",
        "declare const conditional:Conditional<string>; declare const keys:Keys;\n",
        "const a:number=direct; const b:number=nested; const c:number=conditional; const d:number=keys;\n",
        "const call=MissingValue(); const logical=MissingValue||\"x\"; const unary=+MissingValue;",
    );
    let compiler = Compiler::new();
    let options = CompilerOptions {
        no_emit: true,
        strict: true,
        ..CompilerOptions::default()
    };
    let run = || {
        compiler.compile(
            vec![SourceInput::new("error.ts", Arc::<str>::from(source))],
            &options,
        )
    };
    let first = run();
    let warm = run();
    assert_eq!(
        codes(&first),
        vec![2304, 2304, 2304, 2304, 2322, 2322, 2304, 2304, 2304]
    );
    assert_eq!(
        serde_json::to_vec(&first.diagnostics).unwrap(),
        serde_json::to_vec(&warm.diagnostics).unwrap()
    );
    assert_completion(&first, SemanticCompletion::Complete);
    assert_completion(&warm, SemanticCompletion::Complete);
}

#[test]
fn unresolved_names_keep_per_use_display_while_nested_errors_stay_uncached() {
    let source = concat!(
        "type Branch = MissingCheck extends [infer Kept] ? {kept:Kept} : {leaked:Kept}[\"missing\"];\n",
        "type Broken={value:MissingLeaf}; type First=Broken[\"missing\"]; type Second=Broken[\"missing\"];",
    );
    let compiler = Compiler::new();
    let options = CompilerOptions {
        no_emit: true,
        strict: true,
        ..CompilerOptions::default()
    };
    let run = || {
        compiler.compile(
            vec![SourceInput::new(
                "nested-error.ts",
                Arc::<str>::from(source),
            )],
            &options,
        )
    };
    let cold = run();
    let warm = run();
    assert_eq!(codes(&cold), vec![2304, 2304, 2339, 2304, 2339, 2339]);
    assert_eq!(
        cold.diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message_text.as_str())
            .collect::<Vec<_>>(),
        [
            "Cannot find name 'MissingCheck'.",
            "Cannot find name 'Kept'.",
            "Property 'missing' does not exist on type '{ leaked: Kept; }'.",
            "Cannot find name 'MissingLeaf'.",
            "Property 'missing' does not exist on type 'Broken'.",
            "Property 'missing' does not exist on type 'Broken'.",
        ]
    );
    assert_eq!(
        serde_json::to_vec(&cold.diagnostics).unwrap(),
        serde_json::to_vec(&warm.diagnostics).unwrap()
    );
    assert_completion(&cold, SemanticCompletion::Complete);
    assert_completion(&warm, SemanticCompletion::Complete);
}
