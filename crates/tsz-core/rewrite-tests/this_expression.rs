use std::sync::Arc;

use tsz::diagnostics::DiagnosticCategory;
use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

fn compile(source: &str) -> tsz::CompileOutput {
    compile_with_strict(source, false)
}

fn compile_with_strict(source: &str, strict: bool) -> tsz::CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            no_emit: false,
            strict,
            // This suite exercises lexical `this`, not class-field
            // downleveling. ES2022 is the first target that preserves the
            // authored field hosts used by these semantic witnesses.
            target: "es2022".to_string(),
            ..CompilerOptions::default()
        },
    )
}

fn compile_without_check(source: &str) -> tsz::CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            no_check: true,
            no_emit: false,
            strict: true,
            target: "es2022".to_string(),
            ..CompilerOptions::default()
        },
    )
}

#[test]
fn annotated_class_property_arrows_keep_context_through_renaming_wrapping_and_static_members() {
    let source = concat!(
        "class ContextOwner {",
        "handle:(renamed:string)=>void=((renamed)=>{const kept:string=renamed;});",
        "static count:(other:number)=>number=((other)=>other);",
        "}",
    );
    for _ in 0..2 {
        let output = compile_with_strict(source, true);
        assert_complete_without_diagnostics(&output);
    }
}

fn assert_complete_without_diagnostics(output: &tsz::CompileOutput) {
    assert_eq!(output.diagnostics, []);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        output.stats.semantic_completion,
        SemanticCompletion::Complete
    );
    assert_eq!(output.exit_status, CompileExitStatus::Success);
}

fn assert_deferred_without_diagnostics(output: &tsz::CompileOutput) {
    assert_eq!(output.diagnostics, []);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        output.stats.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
}

#[test]
fn generic_annotated_class_property_arrows_wait_for_type_parameter_substitution() {
    for source in [
        concat!(
            "class RenamedGeneric<Unused> {",
            "callback:(renamed:string)=>void=((renamed)=>{const kept:string=renamed;});",
            "}",
        ),
        concat!(
            "class TypedOwner<Item> {",
            "callback:(value:Item)=>void=value=>{const kept:Item=value;};",
            "}",
        ),
    ] {
        for _ in 0..2 {
            let output = compile_with_strict(source, true);
            assert_deferred_without_diagnostics(&output);
        }
    }
}

#[test]
fn generic_annotated_class_property_deferral_keeps_independent_diagnostics() {
    let source = concat!(
        "class GenericBoundary<Unused> {",
        "callback:(renamed:string)=>void=renamed=>{};",
        "}",
        "const independent:MissingGenericBoundary=1;",
    );
    let output = compile_with_strict(source, true);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
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
            2304,
            source.find("MissingGenericBoundary").unwrap() as u32,
            "MissingGenericBoundary".len() as u32,
            "Cannot find name 'MissingGenericBoundary'.",
        )]
    );
}

#[test]
fn unannotated_generic_class_property_arrows_keep_implicit_any_diagnostics() {
    let source = "class LooseGeneric<Unused>{callback=orphan=>{}}";
    let output = compile_with_strict(source, true);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
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
            7006,
            source.find("orphan").unwrap() as u32,
            "orphan".len() as u32,
            "Parameter 'orphan' implicitly has an 'any' type.",
        )]
    );
}

#[test]
fn unannotated_class_property_host_expressions_do_not_force_unsupported_class_shapes() {
    for source in [
        "class SelfOwner{static instance=new SelfOwner();}",
        concat!(
            "class Consumer{",
            "constructor(){new Dependency().property;}",
            "value=new Dependency().property;",
            "}",
            "class Dependency{property='';}",
        ),
    ] {
        for _ in 0..2 {
            let output = compile(source);
            assert_complete_without_diagnostics(&output);
        }
    }
}

#[test]
fn unannotated_class_property_arrows_keep_semantic_ownership_through_wrappers() {
    for (source, parameter) in [
        ("class Direct{callback=direct=>{}}", "direct"),
        ("class Wrapped{callback=((wrapped)=>{})}", "wrapped"),
        ("class Nested{container={callback:nested=>{}}}", "nested"),
    ] {
        let output = compile_with_strict(source, true);
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(
            output.exit_status,
            CompileExitStatus::DiagnosticsPresentOutputsGenerated
        );
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| (diagnostic.code, diagnostic.start, diagnostic.length))
                .collect::<Vec<_>>(),
            vec![(
                7006,
                source.find(parameter).unwrap() as u32,
                parameter.len() as u32,
            )]
        );
    }
}

#[test]
fn annotated_class_property_roots_keep_missing_name_and_invalid_call_diagnostics() {
    let source = concat!(
        "class DiagnosticOwner{",
        "missing:number=MissingFieldValue;",
        "invalid:number=(1)();",
        "}",
    );
    let output = compile(source);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        output.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsGenerated
    );
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        vec![2304, 2349]
    );
}

#[test]
fn class_property_arrows_inherit_instance_this_through_wrappers_and_renaming() {
    for (class_name, callback_name) in [("RenamedHolder", "dispatch"), ("Other", "deliver")] {
        let source = format!(
            "class {class_name} {{\n\
             public payload = {{\n\
             invoke: ({callback_name}) => () => {{\n\
             var _this = 2;\n\
             return {callback_name}((this));\n\
             }}\n\
             }}\n\
             }}"
        );
        for _ in 0..2 {
            let output = compile(&source);
            assert_complete_without_diagnostics(&output);
            assert_eq!(output.emitted_files.len(), 1);
            assert_eq!(output.emitted_files[0].path.to_string_lossy(), "case.js");
            assert_eq!(output.emitted_files[0].text.matches("this").count(), 2);
        }
    }
}

#[test]
fn class_property_arrow_this_distinguishes_instance_and_static_owners() {
    let source = concat!(
        "declare function acceptInstance(value:InstanceOwner):void;",
        "class InstanceOwner {",
        "value:number=1;",
        "capture:()=>void=()=>{acceptInstance((this));};",
        "}",
        "declare function acceptConstructor(value:typeof StaticOwner):void;",
        "class StaticOwner {",
        "static capture=()=>{acceptConstructor(this);};",
        "}",
    );
    let output = compile(source);
    assert_complete_without_diagnostics(&output);
    assert_eq!(output.emitted_files.len(), 1);
    assert_eq!(output.emitted_files[0].text.matches("(this)").count(), 2);
}

#[test]
fn no_this_control_remains_complete() {
    let source = concat!(
        "class Control {",
        "public payload={invoke:(handler)=>()=>{",
        "var local=2;return handler(local);",
        "}}",
        "}",
    );
    let output = compile(source);
    assert_complete_without_diagnostics(&output);
}

#[test]
fn this_without_a_lexical_owner_defers_and_keeps_independent_diagnostics() {
    let source = "const capture=()=>this;const kept:MissingIndependent=1;";
    let output = compile(source);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.file.as_str(),
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.category,
                diagnostic.message_text.as_str(),
                diagnostic.related_information.as_slice(),
            ))
            .collect::<Vec<_>>(),
        vec![(
            "case.ts",
            2304,
            source.find("MissingIndependent").unwrap() as u32,
            "MissingIndependent".len() as u32,
            DiagnosticCategory::Error,
            "Cannot find name 'MissingIndependent'.",
            &[][..],
        )]
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        output.stats.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
}

#[test]
fn supported_lexical_this_methods_contextually_type_object_callbacks() {
    for source in [
        concat!(
            "class RenamedController {",
            "dispatch(value:{accept(text:string):void}):void{}",
            "property=this.dispatch({accept:renamed=>{const kept:string=renamed;}});",
            "}",
        ),
        concat!(
            "class OtherController {",
            "deliver(value:{receive(count:number):void}){}",
            "property=(this.deliver)({receive:other=>{const kept:number=other;}});",
            "}",
        ),
        concat!(
            "class SplitSides {",
            "static dispatch(value:number):void{}",
            "dispatch(value:{accept(text:string):void}):void{}",
            "property=this.dispatch({accept:renamed=>{const kept:string=renamed;}});",
            "}",
        ),
    ] {
        for _ in 0..2 {
            let output = compile_with_strict(source, true);
            assert_complete_without_diagnostics(&output);
        }
    }
}

#[test]
fn corpus_shaped_function_properties_are_clean_but_keep_their_required_type_nonclaim() {
    let source = concat!(
        "interface Contract {first:(text:string)=>void;second:()=>(count:number)=>unknown;}",
        "class Controller {",
        "method(value:Contract):void{}",
        "property=this.method({",
        "first:renamed=>{},",
        "second:()=>nested=>this,",
        "});",
        "}",
    );
    for _ in 0..2 {
        let output = compile_with_strict(source, true);
        assert_eq!(output.diagnostics, []);
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(
            output.stats.semantic_completion,
            SemanticCompletion::Deferred
        );
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    }
}

#[test]
fn absent_object_callback_context_retains_the_exact_implicit_any_diagnostic() {
    let source = concat!(
        "class Controller {",
        "method(value:{}):void{}",
        "property=this.method({callback:orphan=>{}});",
        "}",
    );
    let output = compile_with_strict(source, true);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
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
            7006,
            source.find("orphan").unwrap() as u32,
            "orphan".len() as u32,
            "Parameter 'orphan' implicitly has an 'any' type.",
        )]
    );
}

#[test]
fn lexical_this_method_projection_fails_closed_for_ambiguous_or_unsupported_hosts() {
    for source in [
        concat!(
            "class Overloaded {",
            "method(value:{accept(text:string):void}):void;",
            "method(value:{accept(text:string):void}):void{}",
            "property=this.method({accept:value=>{}});",
            "}",
        ),
        concat!(
            "class GenericMethod {",
            "method<Item>(value:{accept(item:Item):void}):void{}",
            "property=this.method({accept:value=>{}});",
            "}",
        ),
        concat!(
            "class Base {} class Derived extends Base {",
            "method(value:{accept(text:string):void}):void{}",
            "property=this.method({accept:value=>{}});",
            "}",
        ),
        concat!(
            "class StaticOnly {",
            "static method(value:{accept(text:string):void}):void{}",
            "property=this.method({accept:value=>{}});",
            "}",
        ),
    ] {
        let output = compile_with_strict(source, true);
        assert_eq!(output.diagnostics, []);
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    }
}

#[test]
fn authored_this_types_bind_structurally_in_renamed_class_and_interface_members() {
    let source = concat!(
        "declare class RenamedBase<Payload> {",
        "payload:Payload;",
        "set<Key extends keyof this>(key:Key,value:this[Key]):this[Key];",
        "}",
        "interface RenamedPair<Left,Right> {",
        "set<Index extends keyof this>(key:Index,value:this[Index]):this[Index];",
        "wrapped:(value:readonly this[])=>this;",
        "}",
        "interface AlternateShape {",
        "set<Field extends keyof this>(key:Field,value:this[Field]):this[Field];",
        "}",
    );
    assert_eq!(source.match_indices("this").count(), 11);
    let output = compile_with_strict(source, true);
    assert_eq!(output.diagnostics, []);
}

#[test]
fn authored_this_type_context_resets_at_static_constructor_and_nested_boundaries() {
    let source = concat!(
        "type Outside=this;",
        "class BoundaryOwner {",
        "static slot:this;",
        "constructor(value:this){const kept:this=this;}",
        "nested!:{value:this};",
        "method():void{",
        "type LocalAlias=this;",
        "const arrow=(value:this):this=>value;",
        "function ordinary(value:this):void{}",
        "}",
        "}",
    );
    let expected_starts = [
        source.find("=this").unwrap() + 1,
        source.find("slot:this").unwrap() + "slot:".len(),
        source.find("constructor(value:this").unwrap() + "constructor(value:".len(),
        source.find("{value:this}").unwrap() + "{value:".len(),
        source.find("ordinary(value:this").unwrap() + "ordinary(value:".len(),
    ];
    let output = compile_with_strict(source, true);
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
        expected_starts
            .into_iter()
            .map(|start| (
                2526,
                start as u32,
                4,
                "A 'this' type is available only in a non-static member of a class or interface.",
            ))
            .collect::<Vec<_>>()
    );
}

#[test]
fn ordinary_unresolved_type_names_remain_name_resolution_diagnostics() {
    let source = "class IdentifierControl{field!:NotThis;method(value:NotThis):void{}}";
    let output = compile_with_strict(source, true);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.message_text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (2304, "Cannot find name 'NotThis'."),
            (2304, "Cannot find name 'NotThis'."),
        ]
    );
}

#[test]
fn no_check_suppresses_this_type_semantics_without_changing_syntax_or_emit() {
    let source = "type Outside=this;class Boundary{static slot:this;}";
    let output = compile_without_check(source);
    assert_complete_without_diagnostics(&output);
    assert_eq!(output.emitted_files.len(), 1);
    assert!(!output.emitted_files[0].text.contains("this"));
}
