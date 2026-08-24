use std::sync::Arc;

use tsz::service::LanguageService;
use tsz::source::{FileId, SourceText};
use tsz::syntax::{
    Expression, ExpressionKind, FunctionLikeSyntax, ParameterNameKind, StatementKind, parse_source,
};
use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

fn compile(source: &str, no_emit: bool, no_check: bool) -> tsz::CompileOutput {
    compile_with_comments(source, no_emit, no_check, false)
}

fn compile_with_comments(
    source: &str,
    no_emit: bool,
    no_check: bool,
    remove_comments: bool,
) -> tsz::CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new(
            "function-expression.ts",
            Arc::<str>::from(source),
        )],
        &CompilerOptions {
            target: "es2022".to_string(),
            strict: true,
            no_emit,
            no_check,
            remove_comments,
            ..CompilerOptions::default()
        },
    )
}

fn compile_nonstrict(source: &str) -> tsz::CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new(
            "function-expression.ts",
            Arc::<str>::from(source),
        )],
        &CompilerOptions {
            target: "es2015".to_string(),
            strict: false,
            no_emit: true,
            ..CompilerOptions::default()
        },
    )
}

fn parse(source: &str) -> tsz::syntax::ParseOutput {
    parse_path("function-expression.ts", source)
}

fn parse_path(path: &str, source: &str) -> tsz::syntax::ParseOutput {
    let source = SourceText::new(FileId(0), path.into(), Arc::<str>::from(source));
    parse_source(&source)
}

fn variable_initializer(parsed: &tsz::syntax::ParseOutput) -> &Expression {
    let Some(StatementKind::Variable(declaration)) = parsed
        .unit
        .statements
        .first()
        .map(|statement| &statement.kind)
    else {
        panic!("expected a variable declaration");
    };
    declaration
        .initializer
        .as_ref()
        .expect("expected a variable initializer")
}

#[test]
fn plain_function_expression_retains_name_signature_body_and_typed_this() {
    let parsed = parse(concat!(
        "const callable = function renamed(",
        "this: { tag: string }, value: number): number { return value; };",
    ));
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);

    let ExpressionKind::FunctionLike(function) = &variable_initializer(&parsed).kind else {
        panic!("expected a function-like initializer");
    };
    assert!(function.type_parameters.is_empty());
    assert_eq!(function.parameters.len(), 2);
    assert_eq!(function.parameters[0].name, "this");
    assert_eq!(function.parameters[0].name_kind, ParameterNameKind::This);
    assert_eq!(function.parameters[1].name, "value");
    assert_eq!(function.parameters[1].name_kind, ParameterNameKind::Binding);
    assert!(function.return_type.is_some());

    let FunctionLikeSyntax::Function { name, body, .. } = &function.syntax else {
        panic!("expected ordinary function syntax");
    };
    assert_eq!(
        name.as_ref().map(|name| name.name.as_str()),
        Some("renamed")
    );
    assert_eq!(body.len(), 1);
}

#[test]
fn arrow_parameters_do_not_admit_the_function_expression_this_path() {
    let source = "const arrow = (this: any) => 1;";
    let parsed = parse(source);
    assert_eq!(
        parsed.diagnostics.first().map(|diagnostic| diagnostic.code),
        Some(1003),
        "{:#?}",
        parsed.diagnostics,
    );
}

#[test]
fn malformed_function_expression_headers_and_bodies_stay_recovered_nodes() {
    for source in [
        "const value = function named { };",
        "const value = function named(value: number);",
        "const value = function named(@) { };",
    ] {
        let parsed = parse(source);
        assert!(!parsed.diagnostics.is_empty(), "{source}");
        let expression = variable_initializer(&parsed);
        assert!(
            matches!(
                &expression.kind,
                ExpressionKind::FunctionLike(function)
                    if matches!(&function.syntax, FunctionLikeSyntax::Function { .. })
            ),
            "{source}: {:#?}",
            expression.kind,
        );
        assert_eq!(
            expression.span.start as usize,
            source.find("function").unwrap()
        );
        assert!(expression.span.end > expression.span.start);
        assert!(expression.span.end as usize <= source.len());
    }
}

#[test]
fn contextual_function_expression_is_complete_without_parameter_fallout() {
    let source = concat!(
        "let map: (value: number) => number;\n",
        "map = function (renamed) { return renamed; };\n",
    );
    let output = compile(source, true, false);
    assert_eq!(output.diagnostics, [], "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(output.exit_status, CompileExitStatus::Success);
}

#[test]
fn arrow_context_completion_is_unchanged_by_the_function_like_owner() {
    let source = concat!(
        "declare const invoke: any;\n",
        "invoke(() => { }, () => { });\n",
    );
    let output = compile(source, true, false);
    assert_eq!(output.diagnostics, [], "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn named_function_expression_self_is_local_and_uses_its_authored_signature() {
    let source = concat!(
        "const value = function recur(input: number): number { return recur(input); };\n",
        "recur(1);\n",
    );
    let output = compile(source, true, false);
    let [diagnostic] = output.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", output.diagnostics);
    };
    assert_eq!(diagnostic.code, 2304);
    assert_eq!(diagnostic.start, source.rfind("recur").unwrap() as u32);
    assert_eq!(diagnostic.length, 5);
    assert_eq!(diagnostic.message_text, "Cannot find name 'recur'.");
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        output.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );
}

#[test]
fn function_expression_return_mismatch_uses_the_return_statement_span() {
    let source = "const bad = function (input: number): string { return input; };\n";
    let output = compile(source, true, false);
    let [diagnostic] = output.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", output.diagnostics);
    };
    assert_eq!(diagnostic.code, 2322);
    assert_eq!(diagnostic.start, source.find("return").unwrap() as u32);
    assert_eq!(diagnostic.length, 6);
    assert_eq!(
        diagnostic.message_text,
        "Type 'number' is not assignable to type 'string'."
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn explicit_this_is_erased_in_javascript_and_remains_a_semantic_nonclaim() {
    let emit_source = concat!(
        "const tagged = function (this: { tag: string }, value: number): number {\n",
        "    return value;\n",
        "};\n",
    );
    let emitted = compile(emit_source, false, true);
    assert_eq!(emitted.diagnostics, [], "{:#?}", emitted.diagnostics);
    assert_eq!(
        emitted
            .emitted_files
            .iter()
            .find(|file| !file.declaration)
            .expect("JavaScript output")
            .text,
        concat!(
            "\"use strict\";\n",
            "const tagged = function (value) {\n",
            "    return value;\n",
            "};\n",
        )
    );

    let semantic_source = concat!(
        "const tagged = function (this: { tag: string }, value: number): number { return value; };\n",
        "const dependent: string = tagged(1);\n",
        "const independent: MissingSibling = 1;\n",
    );
    let output = compile(semantic_source, true, false);
    let [diagnostic] = output.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", output.diagnostics);
    };
    assert_eq!(diagnostic.code, 2304);
    assert_eq!(
        diagnostic.start,
        semantic_source.find("MissingSibling").unwrap() as u32
    );
    assert_eq!(diagnostic.length, "MissingSibling".len() as u32);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn explicit_this_in_function_types_defers_runtime_arity() {
    let source = concat!(
        "type Callback = (this: void, value: number) => void;\n",
        "declare const callback: Callback;\n",
        "callback(1);\n",
        "const independent: MissingFunctionTypeSibling = 1;\n",
    );
    let output = compile(source, true, false);
    let [diagnostic] = output.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", output.diagnostics);
    };
    assert_eq!(diagnostic.code, 2304);
    assert_eq!(
        diagnostic.start,
        source.find("MissingFunctionTypeSibling").unwrap() as u32
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn generic_function_expression_is_typed_but_semantically_deferred() {
    let source = concat!(
        "const identity = function<Item>(value: Item): Item { return value; };\n",
        "const dependent: string = identity(1);\n",
        "const independent: MissingGenericSibling = 1;\n",
    );
    let output = compile(source, true, false);
    let [diagnostic] = output.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", output.diagnostics);
    };
    assert_eq!(diagnostic.code, 2304);
    assert_eq!(
        diagnostic.start,
        source.find("MissingGenericSibling").unwrap() as u32
    );
    assert_eq!(diagnostic.length, "MissingGenericSibling".len() as u32);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn function_expression_comments_remain_a_typed_javascript_nonclaim() {
    let source = concat!(
        "const named = function /*dropNamed*/ kept/*afterName*/(",
        "/*afterOpen*/ first: number/*afterFirst*/,",
        "/*afterComma*/ second: string/*beforeClose*/) { };\n",
        "const anonymous = function /*dropAnonymous*/(",
        "/*dropThisLeading*/this: unknown/*dropThisTrailing*/,",
        "/*keepBeforeRuntime*/ value: number/*keepBeforeClose*/) { return value; };\n",
        "const rest = function (.../*3*/y: string[]) { };\n",
        "const empty = function(/*inside*/) { /*dropBody*/ };\n",
    );
    for output in [
        compile(source, false, true),
        compile_with_comments(source, false, true, true),
    ] {
        assert!(output.emitted_files.is_empty());
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }
}

#[test]
fn assertion_erasure_keeps_function_expression_statement_grammar() {
    let source = concat!(
        "(function direct() { } as any);\n",
        "(function called() { } as any)();\n",
        "(function membered() { } as any).value;\n",
        "(function chained() { } as any)().value;\n",
        "(function memberCalled() { } as any)().value();\n",
        "(function elementCalled() { } as any)[0]();\n",
        "(function nestedCalled() { } as any)()();\n",
        "(function indexed() { } as any)[0];\n",
        "(function binary() { } as any) + 1;\n",
        "const assigned = (function anonymous() { } as any);\n",
        "new (function directNew() { } as any)();\n",
        "new ((function calledNew() { } as any)())();\n",
        "new ((function memberNew() { } as any).value)();\n",
        "(new (function statementNew() { } as any)());\n",
        "const assignedNew = new (function assignedNewCtor() { } as any)();\n",
    );
    let output = compile(source, false, true);
    assert_eq!(
        output.emitted_files[0].text,
        concat!(
            "\"use strict\";\n",
            "(function direct() { });\n",
            "(function called() { })();\n",
            "(function membered() { }.value);\n",
            "(function chained() { }().value);\n",
            "(function memberCalled() { }().value());\n",
            "(function elementCalled() { }[0]());\n",
            "(function nestedCalled() { }()());\n",
            "(function indexed() { }[0]);\n",
            "(function binary() { } + 1);\n",
            "const assigned = function anonymous() { };\n",
            "new function directNew() { }();\n",
            "new (function calledNew() { }())();\n",
            "new (function memberNew() { }.value)();\n",
            "(new function statementNew() { }());\n",
            "const assignedNew = new function assignedNewCtor() { }();\n",
        )
    );
}

#[test]
fn one_line_function_expression_bodies_keep_their_authored_layout() {
    let source = concat!(
        "const mapped = function (renamed: number): number { return renamed; };\n",
        "const common = function (x: number) { const y = x; y; return y; ; };\n",
        "const header = function\n(x: number): number { return x; };\n",
    );
    let output = compile(source, false, false);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        output.emitted_files[0].text,
        concat!(
            "\"use strict\";\n",
            "const mapped = function (renamed) { return renamed; };\n",
            "const common = function (x) { const y = x; y; return y; ; };\n",
            "const header = function (x) { return x; };\n",
        )
    );
}

#[test]
fn function_assignment_accepts_fewer_but_not_more_required_parameters() {
    let source = concat!(
        "let accepts: (value: string) => void = function () { };\n",
        "let rejects: () => void = function (value: string) { };\n",
    );
    let output = compile(source, true, false);
    let [diagnostic] = output.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", output.diagnostics);
    };
    assert_eq!(diagnostic.code, 2322);
    assert_eq!(diagnostic.start, source.find("rejects").unwrap() as u32);
    assert_eq!(diagnostic.length, "rejects".len() as u32);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn uncontextualized_implicit_any_waits_for_the_function_expression_owner() {
    let source = concat!(
        "const value = function (renamed) { return renamed; };\n",
        "const independent: MissingImplicitAnySibling = 1;\n",
    );
    let output = compile(source, true, false);
    let [diagnostic] = output.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", output.diagnostics);
    };
    assert_eq!(diagnostic.code, 2304);
    assert_eq!(
        diagnostic.start,
        source.find("MissingImplicitAnySibling").unwrap() as u32
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn contextual_void_accepts_a_return_value_and_void_parameters_may_be_omitted() {
    let source = concat!(
        "let callback: (value: number) => void = function (value) { return value; };\n",
        "declare function optionalVoid(value: void): void;\n",
        "let noParameters: () => void = optionalVoid;\n",
    );
    let output = compile(source, true, false);
    assert_eq!(output.diagnostics, [], "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn immediate_function_calls_enforce_required_and_maximum_arity() {
    let source = concat!(
        "(function (first: number, second: number) { })();\n",
        "(function (nothing: void) { })();\n",
        "(function (only: number) { })(1, 2);\n",
    );
    let output = compile(source, true, false);
    let [annotated_too_few, too_many] = output.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", output.diagnostics);
    };
    assert_eq!(annotated_too_few.code, 2554);
    assert_eq!(annotated_too_few.start, 0);
    assert_eq!(too_many.code, 2554);
    assert_eq!(too_many.start, source.rfind('2').unwrap() as u32);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn immediate_unannotated_parameters_follow_context_sensitive_minimum_arity() {
    let source = concat!(
        "(function (inferred) { })();\n",
        "(function (inferred, annotated: number) { })();\n",
    );
    let output = compile_nonstrict(source);
    let [diagnostic] = output.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", output.diagnostics);
    };
    assert_eq!(diagnostic.code, 2554);
    assert_eq!(
        diagnostic.start,
        source.find("(function (inferred, annotated").unwrap() as u32
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn direct_unannotated_calls_are_omittable_but_stored_functions_are_not() {
    let source = concat!(
        "(function (first, second) { })();\n",
        "((first, second) => 0)();\n",
        "const stored = function (value) { };\n",
        "stored();\n",
    );
    let output = compile_nonstrict(source);
    let [diagnostic] = output.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", output.diagnostics);
    };
    assert_eq!(diagnostic.code, 2554);
    assert_eq!(diagnostic.start, source.rfind("stored();").unwrap() as u32);
    assert_eq!(diagnostic.message_text, "Expected 1 arguments, but got 0.");
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn contextually_typed_iife_nonstrict_control_has_no_count_diagnostics() {
    let source = concat!(
        "(function (value, undefined) { value; })(42);\n",
        "((first, second, third) => 42)();\n",
    );
    let output = compile_nonstrict(source);
    assert_eq!(output.diagnostics, [], "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn void_aliases_and_unions_are_omittable_but_absorbing_unions_are_not() {
    let source = concat!(
        "type Empty = void;\n",
        "type DefaultEmpty<Item = void> = Item;\n",
        "(function (aliasValue: Empty) { })();\n",
        "(function (defaultAlias: DefaultEmpty) { })();\n",
        "(function (renamedDefaultAlias: DefaultEmpty) { })();\n",
        "const storedAlias = function (storedValue: Empty) { };\n",
        "storedAlias();\n",
        "(function (unioned: void | number) { })();\n",
        "type NestedEmpty = void;\n",
        "interface NestedRequired {}\n",
        "type NestedAbsorber = unknown;\n",
        "(function (nestedAlias: string | NestedEmpty) { })();\n",
        "(function (nestedInterface: void | NestedRequired) { })();\n",
        "(function (nestedAbsorber: void | NestedAbsorber) { })();\n",
        "(function (absorbedAny: void | any) { })();\n",
        "(function (absorbedUnknown: void | unknown) { })();\n",
    );
    let output = compile(source, true, false);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                diagnostic.start,
                diagnostic.message_text.as_str()
            ))
            .collect::<Vec<_>>(),
        [
            (
                2554,
                source.find("(function (nestedAbsorber").unwrap() as u32,
                "Expected 1 arguments, but got 0."
            ),
            (
                2554,
                source.find("(function (absorbedAny").unwrap() as u32,
                "Expected 1 arguments, but got 0."
            ),
            (
                2554,
                source.find("(function (absorbedUnknown").unwrap() as u32,
                "Expected 1 arguments, but got 0."
            ),
        ]
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn absorbing_aliases_decide_arity_on_both_sides_of_a_deferred_member() {
    let source = concat!(
        "type Hard<Item extends string> = Item;\n",
        "type FirstAbsorber = any;\n",
        "type SecondAbsorber = unknown;\n",
        "(function (left: void | Hard<string> | FirstAbsorber) { })();\n",
        "(function (right: SecondAbsorber | Hard<string> | void) { })();\n",
    );
    let output = compile(source, true, false);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                diagnostic.start,
                diagnostic.message_text.as_str()
            ))
            .collect::<Vec<_>>(),
        [
            (
                2554,
                source.find("(function (left").unwrap() as u32,
                "Expected 1 arguments, but got 0."
            ),
            (
                2554,
                source.find("(function (right").unwrap() as u32,
                "Expected 1 arguments, but got 0."
            ),
        ]
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn aliases_to_constrained_interface_and_class_types_stay_required_without_shape_forcing() {
    let source = concat!(
        "interface Wrapped<Item extends string> { value: Item }\n",
        "declare class Stored<Item extends string> { value: Item }\n",
        "type WrappedAlias = Wrapped<'ok'>;\n",
        "type StoredAlias = Stored<'ok'>;\n",
        "(function (wrapped: WrappedAlias) { })();\n",
        "(function (stored: StoredAlias) { })();\n",
    );
    let output = compile(source, true, false);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                diagnostic.start,
                diagnostic.message_text.as_str()
            ))
            .collect::<Vec<_>>(),
        [
            (
                2554,
                source.find("(function (wrapped").unwrap() as u32,
                "Expected 1 arguments, but got 0."
            ),
            (
                2554,
                source.find("(function (stored").unwrap() as u32,
                "Expected 1 arguments, but got 0."
            ),
        ]
    );
}

#[test]
fn tuple_rest_aliases_publish_the_oracle_minimum_and_maximum() {
    let source = concat!(
        "(function (required: number, ...values: string[]) { })();\n",
        "(function (required: number, ...values: string[]) { })(1, 'a', 'b');\n",
        "(function (...values: any) { })();\n",
        "(function (required: number, ...values: any) { })();\n",
        "(function (required: number, ...values: any) { })(1, 'a', true);\n",
        "type Pair = [number, string];\n",
        "const tuple = function (...values: Pair) { };\n",
        "tuple();\n",
        "tuple(1);\n",
        "tuple(1, 'ok');\n",
        "tuple(1, 'ok', true);\n",
        "type RestAbsorber = unknown;\n",
        "type AbsorbedRest = [void | RestAbsorber];\n",
        "(function (...values: AbsorbedRest) { })();\n",
        "type RestEmpty = void;\n",
        "type MaybeSecond = [number, string | RestEmpty];\n",
        "(function (...values: MaybeSecond) { })();\n",
        "(function (...values: MaybeSecond) { })(1, undefined, 3);\n",
    );
    let output = compile(source, true, false);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                diagnostic.start,
                diagnostic.message_text.as_str()
            ))
            .collect::<Vec<_>>(),
        [
            (
                2555,
                source
                    .find("(function (required: number, ...values: string[]) { })();")
                    .unwrap() as u32,
                "Expected at least 1 arguments, but got 0."
            ),
            (
                2555,
                source
                    .find("(function (required: number, ...values: any) { })();")
                    .unwrap() as u32,
                "Expected at least 1 arguments, but got 0."
            ),
            (
                2554,
                source.find("tuple();").unwrap() as u32,
                "Expected 2 arguments, but got 0."
            ),
            (
                2554,
                source.find("tuple(1);").unwrap() as u32,
                "Expected 2 arguments, but got 1."
            ),
            (
                2554,
                (source.find("tuple(1, 'ok', true);").unwrap() + "tuple(1, 'ok', ".len()) as u32,
                "Expected 2 arguments, but got 3."
            ),
            (
                2554,
                source
                    .find("(function (...values: AbsorbedRest) { })();")
                    .unwrap() as u32,
                "Expected 1 arguments, but got 0."
            ),
            (
                2554,
                source
                    .find("(function (...values: MaybeSecond) { })();")
                    .unwrap() as u32,
                "Expected 1-2 arguments, but got 0."
            ),
            (
                2554,
                (source
                    .find("(function (...values: MaybeSecond) { })(1, undefined, 3);")
                    .unwrap()
                    + "(function (...values: MaybeSecond) { })(1, undefined, ".len())
                    as u32,
                "Expected 1-2 arguments, but got 3."
            ),
        ]
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn void_arity_scans_from_the_syntactic_minimum() {
    let source = concat!(
        "function needsPrefix(prefix = 1, tail: void) { }\n",
        "needsPrefix();\n",
        "needsPrefix(1);\n",
    );
    let output = compile(source, true, false);
    let [diagnostic] = output.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", output.diagnostics);
    };
    assert_eq!(diagnostic.code, 2554);
    assert_eq!(
        diagnostic.start,
        source.find("needsPrefix();").unwrap() as u32
    );
    assert_eq!(diagnostic.length, "needsPrefix".len() as u32);
    assert_eq!(
        diagnostic.message_text,
        "Expected 1-2 arguments, but got 0."
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn tuple_rest_void_scan_crosses_the_fixed_parameter_boundary() {
    let source = concat!(
        "type EmptyTail = [];\n",
        "type VoidTail = [void];\n",
        "const allVoid = function (prefix?: void, ...renamed: VoidTail) { };\n",
        "const numberPrefix = function (prefix?: number, ...renamed: VoidTail) { };\n",
        "const emptyTail = function (prefix?: number, ...renamed: EmptyTail) { };\n",
        "allVoid();\n",
        "numberPrefix();\n",
        "emptyTail();\n",
    );
    let output = compile(source, true, false);
    let [diagnostic] = output.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", output.diagnostics);
    };
    assert_eq!(diagnostic.code, 2554);
    assert_eq!(
        diagnostic.start,
        source.find("numberPrefix();").unwrap() as u32
    );
    assert_eq!(diagnostic.length, "numberPrefix".len() as u32);
    assert_eq!(
        diagnostic.message_text,
        "Expected 1-2 arguments, but got 0."
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn recognized_non_tuple_rests_publish_counts_and_argument_relations() {
    let source = concat!(
        "declare function readonlyRest(required: number, ...renamed: ReadonlyArray<string>): void;\n",
        "readonlyRest();\n",
        "readonlyRest(1, 'ok');\n",
        "readonlyRest(1, 2);\n",
        "declare function bottomRest(required: number, ...renamed: never): void;\n",
        "bottomRest();\n",
        "bottomRest(1, 2);\n",
        "declare function missingRest(required: number, ...renamed: MissingRest): void;\n",
        "missingRest();\n",
    );
    let output = compile(source, true, false);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.as_str()
            ))
            .collect::<Vec<_>>(),
        [
            (
                2555,
                source.find("readonlyRest();").unwrap() as u32,
                "readonlyRest".len() as u32,
                "Expected at least 1 arguments, but got 0."
            ),
            (
                2345,
                (source.find("readonlyRest(1, 2);").unwrap() + "readonlyRest(1, ".len()) as u32,
                1,
                "Argument of type 'number' is not assignable to parameter of type 'string'."
            ),
            (
                2555,
                source.find("bottomRest();").unwrap() as u32,
                "bottomRest".len() as u32,
                "Expected at least 1 arguments, but got 0."
            ),
            (
                2345,
                (source.find("bottomRest(1, 2);").unwrap() + "bottomRest(1, ".len()) as u32,
                1,
                "Argument of type 'number' is not assignable to parameter of type 'never'."
            ),
            (
                2304,
                source.find("MissingRest").unwrap() as u32,
                "MissingRest".len() as u32,
                "Cannot find name 'MissingRest'."
            ),
            (
                2555,
                source.find("missingRest();").unwrap() as u32,
                "missingRest".len() as u32,
                "Expected at least 1 arguments, but got 0."
            ),
        ]
    );
}

#[test]
fn unmodeled_standard_library_alias_arity_stays_deferred() {
    let source = concat!(
        "declare function optionalAwaited(value: Awaited<void>): void;\n",
        "declare function absorbedAwaited(value: void | Awaited<any>): void;\n",
        "optionalAwaited();\n",
        "absorbedAwaited();\n",
    );
    let output = compile(source, true, false);
    assert_eq!(output.diagnostics, [], "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn unsupported_rest_union_defers_counts_and_relations_but_infers_arguments() {
    let source = concat!(
        "type Choice = [number] | [string, string];\n",
        "const deferred = function (...values: Choice) { };\n",
        "deferred(MissingArgument);\n",
    );
    let output = compile(source, true, false);
    let [diagnostic] = output.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", output.diagnostics);
    };
    assert_eq!(diagnostic.code, 2304);
    assert_eq!(
        diagnostic.start,
        source.find("MissingArgument").unwrap() as u32
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn missing_function_body_does_not_consume_the_following_statement() {
    let source = "const callable = function(); const sibling: MissingSibling = 1;";
    let parsed = parse(source);
    assert!(!parsed.diagnostics.is_empty());
    assert_eq!(
        parsed.unit.statements.len(),
        2,
        "{:#?}",
        parsed.unit.statements
    );
    assert!(matches!(
        &parsed.unit.statements[1].kind,
        StatementKind::Variable(declaration) if declaration.name == "sibling"
    ));
}

#[test]
fn recovered_function_header_blocks_emit_but_body_recovery_keeps_siblings_visible() {
    let malformed = compile("const value = function named(@) { };", false, true);
    assert!(malformed.emitted_files.is_empty());
    assert_eq!(malformed.semantic_completion, SemanticCompletion::Deferred);

    let source = concat!(
        "const value = function named(input: number): number { const broken = ; return input; };\n",
        "const independent: MissingBodySibling = 1;\n",
    );
    let emit = compile(source, false, true);
    assert!(emit.emitted_files.is_empty());
    assert_eq!(emit.semantic_completion, SemanticCompletion::Deferred);
    let output = compile(source, true, false);
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == 2304
            && diagnostic.start == source.find("MissingBodySibling").unwrap() as u32
    }));
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn generic_nonclaim_retains_its_owned_type_parameters_in_nested_body_types() {
    let source = concat!(
        "const identity = function<Item>(value: Item) { const nested: Item = value; };\n",
        "const independent: MissingNestedGeneric = 1;\n",
    );
    let output = compile(source, true, false);
    let [diagnostic] = output.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", output.diagnostics);
    };
    assert_eq!(diagnostic.code, 2304);
    assert_eq!(
        diagnostic.start,
        source.find("MissingNestedGeneric").unwrap() as u32
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn unsupported_unannotated_return_analysis_propagates_deferred_completion() {
    let source = "let flag: boolean; const value = function () { if (flag) return 1; };";
    let output = compile(source, true, false);
    assert_eq!(output.diagnostics, [], "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn authored_return_mismatch_survives_a_deferred_flow_host() {
    let source = concat!(
        "let subject: string | number = 0;\n",
        "switch (subject.) { default: break; }\n",
        "const nested = function self(input: number): number { return 'bad'; };\n",
    );
    let output = compile(source, true, false);
    assert!(
        output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == 2322)
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn wrapped_immediate_function_calls_and_constructors_invalidate_flow_claims() {
    for constructor in [false, true] {
        let invoke = if constructor { "new " } else { "" };
        let source = format!(
            concat!(
                "let state: 'open' | 'closed' = 'open';\n",
                "switch (state) {{ case 'open': {invoke}(function () {{ state = 'closed'; }} as any)();\n",
                "const dependent: 'open' = state; break; }}\n",
                "const independent: MissingImmediateSibling = 1;\n",
            ),
            invoke = invoke,
        );
        let output = compile(&source, true, false);
        assert!(output.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == 2304
                && diagnostic.start == source.find("MissingImmediateSibling").unwrap() as u32
        }));
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }
}

#[test]
fn contextual_keyword_self_names_defer_until_the_strict_grammar_owner_exists() {
    let source = concat!(
        "const value = function yield() { };\n",
        "const independent: MissingStrictName = 1;\n",
    );
    let output = compile(source, true, false);
    let [diagnostic] = output.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", output.diagnostics);
    };
    assert_eq!(diagnostic.code, 2304);
    assert_eq!(
        diagnostic.start,
        source.find("MissingStrictName").unwrap() as u32
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn function_expression_service_identity_is_local_and_capability_scoped() {
    let source = concat!(
        "const value = function inner<Item extends typeof inner>(input: Item): Item { return input; };\n",
        "inner;\n",
    );
    let positions = source
        .match_indices("inner")
        .map(|(offset, _)| offset as u32)
        .collect::<Vec<_>>();
    assert_eq!(positions.len(), 3);
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open("function-expression.ts", Arc::<str>::from(source));
    let definition = service
        .definition_and_bound_span("function-expression.ts", positions[1] + 1)
        .expect("inner constraint reference");
    assert_eq!(definition.definitions[0].text_span.start, positions[0]);
    assert_eq!(
        service
            .rename("function-expression.ts", positions[0] + 1)
            .locations
            .len(),
        2
    );
    assert!(
        service
            .definition_and_bound_span("function-expression.ts", positions[2] + 1)
            .is_none()
    );
    assert!(
        service
            .quick_info("function-expression.ts", positions[0] + 1)
            .is_none(),
        "FunctionExpression QuickInfo remains an explicit typed nonclaim"
    );

    let malformed = "const value = function broken(@) { };";
    service.change("function-expression.ts", Arc::<str>::from(malformed));
    let broken = malformed.find("broken").unwrap() as u32;
    assert!(
        service
            .definition_and_bound_span("function-expression.ts", broken + 1)
            .is_none()
    );
    assert!(
        !service
            .rename("function-expression.ts", broken + 1)
            .info
            .can_rename
    );

    let recovered = concat!(
        "const external = 1;\n",
        "const value = function broken(@) { return external; };\n",
        "external;\n",
    );
    service.change("function-expression.ts", Arc::<str>::from(recovered));
    let external = recovered
        .match_indices("external")
        .map(|(offset, _)| offset as u32)
        .collect::<Vec<_>>();
    assert_eq!(external.len(), 3);
    let body_definition = service
        .definition_and_bound_span("function-expression.ts", external[1] + 1)
        .expect("the represented body query remains independently claimed");
    assert_eq!(body_definition.definitions[0].text_span.start, external[0]);
    let definition = service
        .definition_and_bound_span("function-expression.ts", external[2] + 1)
        .expect("the sibling query remains independently claimed");
    assert_eq!(definition.definitions[0].text_span.start, external[0]);
}

#[test]
fn function_expression_signature_navigation_uses_its_independent_owner() {
    let recovered = concat!(
        "type Alias = number;\n",
        "const value = { ...(function inner(input: Alias): Alias { return input; }) };\n",
    );
    let aliases = recovered
        .match_indices("Alias")
        .map(|(offset, _)| offset as u32)
        .collect::<Vec<_>>();
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open("function-expression.ts", Arc::<str>::from(recovered));
    for reference in &aliases[1..] {
        let definition = service
            .definition_and_bound_span("function-expression.ts", reference + 1)
            .expect("the represented FunctionExpression signature owns this query");
        assert_eq!(definition.definitions[0].text_span.start, aliases[0]);
    }

    let queries = concat!(
        "type SomeType = number;\n",
        "const external = 1;\n",
        "const value = function inner(input: SomeType | typeof external): typeof inner { return inner; };\n",
    );
    service.change("function-expression.ts", Arc::<str>::from(queries));
    for name in ["SomeType", "external", "inner"] {
        let positions = queries
            .match_indices(name)
            .map(|(offset, _)| offset as u32)
            .collect::<Vec<_>>();
        for reference in &positions[1..] {
            let definition = service
                .definition_and_bound_span("function-expression.ts", reference + 1)
                .unwrap_or_else(|| panic!("missing signature definition for {name}"));
            assert_eq!(definition.definitions[0].text_span.start, positions[0]);
        }
    }
}

#[test]
fn function_expression_services_scope_outer_bindings_and_preserve_nested_metadata() {
    let source = concat!(
        "const outer = (function self(input: number): number {\n",
        "  const nested: string = 'ok';\n",
        "  return nested;\n",
        "});\n",
        "const authored: (input: number) => number = function (input: number): number { return input; };\n",
    );
    let outer = source.find("outer").unwrap() as u32;
    let nested = source
        .match_indices("nested")
        .map(|(offset, _)| offset as u32)
        .collect::<Vec<_>>();
    let authored = source.find("authored").unwrap() as u32;
    let nested_statement = "const nested: string = 'ok';";
    let nested_context = source.find(nested_statement).unwrap() as u32;
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open("function-expression.ts", Arc::<str>::from(source));

    assert!(
        service
            .quick_info("function-expression.ts", outer + 1)
            .is_none(),
        "an inferred outer binding must reuse the FunctionExpression QuickInfo nonclaim",
    );
    let authored = service
        .quick_info("function-expression.ts", authored + 1)
        .expect("an authored outer type remains independently answerable");
    assert_eq!(authored.kind, "const");
    assert_eq!(
        authored.display,
        "const authored: (input: number) => number"
    );

    let nested_info = service
        .quick_info("function-expression.ts", nested[0] + 1)
        .expect("the nested declaration has its own claimed QuickInfo scope");
    assert_eq!(nested_info.kind, "const");
    assert_eq!(nested_info.text_span.start, nested[0]);
    assert_eq!(nested_info.display, "const nested: string");

    let definition = service
        .definition_and_bound_span("function-expression.ts", nested[1] + 1)
        .expect("nested reference definition");
    let declaration = &definition.definitions[0];
    assert_eq!(declaration.kind, "const");
    assert_eq!(declaration.text_span.start, nested[0]);
    let context = declaration
        .context_span
        .expect("nested declaration context");
    assert_eq!(context.start, nested_context);
    assert_eq!(context.length, nested_statement.len() as u32);

    let references = service.references("function-expression.ts", nested[0] + 1);
    assert_eq!(references.len(), 1);
    assert_eq!(references[0].definition.kind, "const");
    assert_eq!(references[0].definition.name, "const nested: string");
    assert_eq!(references[0].definition.context_span, Some(context));
}

#[test]
fn unowned_async_and_generator_forms_retain_structure_for_typed_nonclaims() {
    for source in [
        "const value = async function () { };",
        "const value = function* () { };",
        "const value = async function* () { };",
    ] {
        let parsed = parse(source);
        assert!(
            matches!(
                &variable_initializer(&parsed).kind,
                ExpressionKind::FunctionLike(function)
                    if matches!(&function.syntax, FunctionLikeSyntax::Function { .. })
            ),
            "{source}: {:#?}",
            variable_initializer(&parsed).kind,
        );
    }
}

#[test]
fn missing_generic_arrow_return_type_uses_the_source_kind_grammar() {
    for source in [
        "const value = <Cedar,>(): => 1;",
        "const value = <Cedar extends Box<Birch>>(): => 1;",
    ] {
        let typescript = parse_path("case.ts", source);
        assert!(!matches!(
            variable_initializer(&typescript).kind,
            ExpressionKind::FunctionLike(_)
        ));
        assert!(
            typescript
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != 1110)
        );
        assert!(!typescript.unit.statements.iter().any(|statement| matches!(
            &statement.kind,
            StatementKind::Expression(expression)
                if matches!(&expression.kind, ExpressionKind::FunctionLike(_))
        )));

        let tsx = parse_path("case.tsx", source);
        assert!(matches!(
            &variable_initializer(&tsx).kind,
            ExpressionKind::FunctionLike(function)
                if matches!(&function.syntax, FunctionLikeSyntax::Arrow(_))
        ));
        assert_eq!(
            tsx.diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            vec![1110],
        );
    }

    let source = "const value = <Cedar,>(): => 1;";
    let rejected = parse_path("case.ts", source);
    let expected = [
        (1005, ",", 1, "'>' expected."),
        (1134, ">", 1, "Variable declaration expected."),
        (1109, ")", 1, "Expression expected."),
        (1005, ":", 1, "';' expected."),
        (1128, "=>", 2, "Declaration or statement expected."),
    ];
    assert_eq!(rejected.diagnostics.len(), expected.len());
    for (diagnostic, &(code, marker, length, message)) in rejected.diagnostics.iter().zip(&expected)
    {
        assert_eq!(diagnostic.code, code);
        assert_eq!(diagnostic.start, source.find(marker).unwrap() as u32);
        assert_eq!(diagnostic.length, length);
        assert_eq!(diagnostic.message_text, message);
    }

    for source in [
        "const value = (named): => 1;",
        "const value = (named? changed): => 1;",
        "const value = (public as): => 1;",
        "const value = (named): ) => 1;",
        "const value = (named): <Cedar> => Birch => 1;",
        "const value = (named): new => Birch => 1;",
        "const value = (named? changed): Cedar => 1;",
        "const value = (public as): Cedar => 1;",
        "const value = (named changed): Cedar => 1;",
        "const value = (1): Cedar => 1;",
        "const value = <,>(): Cedar => 1;",
        "const value = <123>(): Cedar => 1;",
        "const value = <const,>(): => 1;",
    ] {
        let uncertain = parse_path("case.ts", source);
        assert!(!matches!(
            variable_initializer(&uncertain).kind,
            ExpressionKind::FunctionLike(_)
        ));
    }

    for source in [
        "const value = (named): Cedar => 1;",
        "const value = (left, right): Cedar => 1;",
        "const value = (named = 1): Cedar => 1;",
        "const value = ({ named }): Cedar => 1;",
    ] {
        let parsed = parse_path("case.ts", source);
        assert!(matches!(
            &variable_initializer(&parsed).kind,
            ExpressionKind::FunctionLike(function)
                if matches!(&function.syntax, FunctionLikeSyntax::Arrow(_))
        ));
    }

    let modifier_name = parse_path("case.ts", "const value = (public: Cedar): => 1;");
    assert!(matches!(
        &variable_initializer(&modifier_name).kind,
        ExpressionKind::FunctionLike(function)
            if matches!(&function.syntax, FunctionLikeSyntax::Arrow(_))
    ));
    assert_eq!(
        modifier_name
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        vec![1110],
    );

    for reserved in ["const", "default"] {
        let source = format!("const value = ({reserved} changed) => changed;");
        let parsed = parse_path("case.ts", &source);
        assert!(matches!(
            &variable_initializer(&parsed).kind,
            ExpressionKind::FunctionLike(function)
                if matches!(&function.syntax, FunctionLikeSyntax::Arrow(_))
        ));
        assert_eq!(
            parsed
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            vec![1359, 1005],
            "{source}: {:#?}",
            parsed.diagnostics,
        );
    }

    let const_source = "const value = <const>(): Cedar => 1;";
    let const_parameter = parse_path("case.ts", const_source);
    assert!(matches!(
        &variable_initializer(&const_parameter).kind,
        ExpressionKind::FunctionLike(function)
            if matches!(&function.syntax, FunctionLikeSyntax::Arrow(_))
    ));
    assert!(!const_parameter.diagnostics.is_empty());
    let const_output = compile(const_source, false, true);
    assert_eq!(
        const_output.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert!(const_output.emitted_files.is_empty());
    let const_parameter_tsx = parse_path("case.tsx", "const value = <const>(): Cedar => 1;");
    assert!(!matches!(
        variable_initializer(&const_parameter_tsx).kind,
        ExpressionKind::FunctionLike(_)
    ));

    let const_variance = parse_path(
        "case.ts",
        "const value = <const in Cedar>(input: Cedar): Cedar => input;",
    );
    assert!(const_variance.diagnostics.is_empty());
    assert!(matches!(
        &variable_initializer(&const_variance).kind,
        ExpressionKind::FunctionLike(function)
            if matches!(&function.syntax, FunctionLikeSyntax::Arrow(_))
    ));
}

#[test]
fn parenthesized_arrow_certainty_owns_missing_arrow_tokens_without_false_heads() {
    for (path, source) in [
        ("case.ts", "const value = () { return 1; }"),
        ("case.ts", "const value = (named: Cedar) { return named; }"),
        ("case.ts", "const value = (...renamed) { return renamed; }"),
        (
            "case.ts",
            "const value = (public renamed) { return renamed; }",
        ),
        ("case.ts", "const value = (named?: Cedar) { return named; }"),
        ("case.ts", "const value = (named) { return named; }"),
        ("case.ts", "const value = (left, right) { return left; }"),
        ("case.ts", "const value = ({ named }) { return named; }"),
        ("case.ts", "const value = <Cedar>() { return 1; }"),
        ("case.tsx", "const value = <Cedar,>() { return 1; }"),
        ("case.ts", "const value = (named: Cedar);"),
    ] {
        let parsed = parse_path(path, source);
        assert!(
            matches!(
                &variable_initializer(&parsed).kind,
                ExpressionKind::FunctionLike(function)
                    if matches!(&function.syntax, FunctionLikeSyntax::Arrow(_))
            ),
            "{path}: {source}: {:#?}",
            parsed.diagnostics,
        );
        assert_eq!(
            parsed
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            vec![1005],
            "{path}: {source}: {:#?}",
            parsed.diagnostics,
        );
    }

    for source in [
        "const value = (named);",
        "const value = (named, changed);",
        "const value = (named, changed + other) => named;",
        "const value = (named, 1) => named;",
        "const value = (named, const) => named;",
        "const value = (named, const changed) => named;",
        "const value = (named, default changed) => named;",
        "const value = (named, in) => named;",
        "const value = (named, in\nchanged) => named;",
        "const value = <Cedar>(named + changed) => named;",
        "const value = <Cedar>(1) { return 1; }",
    ] {
        let parsed = parse_path("case.ts", source);
        assert!(
            !matches!(
                &variable_initializer(&parsed).kind,
                ExpressionKind::FunctionLike(_)
            ),
            "{source}: {:#?}",
            parsed.diagnostics,
        );
    }

    let contextual_name = parse_path("case.ts", "const value = (named, public) => named;");
    assert!(matches!(
        &variable_initializer(&contextual_name).kind,
        ExpressionKind::FunctionLike(function)
            if matches!(&function.syntax, FunctionLikeSyntax::Arrow(_))
    ));
    for source in [
        "const value = (named, in changed) => named;",
        "const value = (named, export changed) => named;",
        "const value = (named, export\nchanged) => named;",
        "const value = (named, static changed) => named;",
        "const value = (named, static\nchanged) => named;",
        "const value = (named, public\nchanged) => named;",
    ] {
        let parsed = parse_path("case.ts", source);
        assert!(
            matches!(
                &variable_initializer(&parsed).kind,
                ExpressionKind::FunctionLike(function)
                    if matches!(&function.syntax, FunctionLikeSyntax::Arrow(_))
            ),
            "{source}: {:#?}",
            parsed.diagnostics,
        );
    }

    let recovered_separator = parse_path(
        "case.ts",
        "const value = (named, public\nchanged) => named;",
    );
    assert_eq!(
        recovered_separator
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        vec![1005],
    );
    for source in [
        "const value = (named, export\nchanged) => named;",
        "const value = (named, static\nchanged) => named;",
    ] {
        assert!(
            parse_path("case.ts", source).diagnostics.is_empty(),
            "{source}"
        );
    }

    for source in [
        "const value = (named, changed third) => named;",
        "const value = <Cedar>(named changed) => named;",
    ] {
        let parsed = parse_path("case.ts", source);
        assert!(matches!(
            &variable_initializer(&parsed).kind,
            ExpressionKind::FunctionLike(function)
                if matches!(&function.syntax, FunctionLikeSyntax::Arrow(_))
        ));
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == 1005)
        );
    }

    for (source, function_count) in [
        (
            concat!(
                "declare const consume: (...values: any[]) => void;",
                "consume((): => 1, (renamed) { return renamed; }, MissingSameCall);",
            ),
            2,
        ),
        (
            concat!(
                "declare const consume: (...values: any[]) => void;",
                "consume(<Cedar,>(): => 1, (renamed) { return renamed; }, MissingSameCall);",
            ),
            1,
        ),
    ] {
        let parsed = parse_path("case.ts", source);
        let StatementKind::Expression(Expression {
            kind: ExpressionKind::Call { arguments, .. },
            ..
        }) = &parsed.unit.statements[1].kind
        else {
            panic!("expected a call expression: {:#?}", parsed.unit.statements);
        };
        assert_eq!(
            arguments
                .iter()
                .filter(|argument| matches!(argument.kind, ExpressionKind::FunctionLike(_)))
                .count(),
            function_count,
            "{source}: {arguments:#?}",
        );

        let output = compile(source, true, false);
        let missing_names = output
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == 2304)
            .map(|diagnostic| diagnostic.start)
            .collect::<Vec<_>>();
        assert_eq!(
            missing_names,
            vec![source.find("MissingSameCall").unwrap() as u32],
            "{source}: {:#?}",
            output.diagnostics,
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }
}

#[test]
fn malformed_generic_arrow_type_parameter_lists_remain_nonclaiming() {
    for source in [
        "const value = <Cedar Birch>(): Elm => 1;",
        "const value = <Renamed Changed>(): Result => 1;",
        "const value = <const ...Cedar>(): Elm => 1;",
        "const value = <const ...Renamed>(): Result => 1;",
        "const value = <const ?Cedar>(): Elm => 1;",
        "const value = <const ?Renamed>(): Result => 1;",
    ] {
        let parsed = parse_path("case.ts", source);
        assert!(!parsed.diagnostics.is_empty(), "{source}");
        let output = compile(source, false, true);
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert!(output.emitted_files.is_empty(), "{source}: {output:#?}");
    }

    for source in [
        "const value = <const in Cedar>(): Elm => 1;",
        "const value = <const in Renamed>(): Result => 1;",
    ] {
        let parsed = parse_path("case.ts", source);
        assert!(parsed.diagnostics.is_empty(), "{source}");
        assert!(matches!(
            &variable_initializer(&parsed).kind,
            ExpressionKind::FunctionLike(function)
                if matches!(&function.syntax, FunctionLikeSyntax::Arrow(_))
        ));
    }

    for source in [
        "const value = <Cedar Birch",
        "const value = <Renamed Changed",
    ] {
        let parsed = parse_path("case.ts", source);
        assert!(!matches!(
            &variable_initializer(&parsed).kind,
            ExpressionKind::FunctionLike(_)
        ));
    }
}

#[test]
fn rejected_generic_arrow_prefix_withholds_only_the_affected_file_products() {
    let output = Compiler::new().compile(
        vec![
            SourceInput::new(
                "affected.ts",
                Arc::<str>::from("export const value = <Cedar,>(): => 1;"),
            ),
            SourceInput::new("stable.ts", Arc::<str>::from("export const sibling = 1;")),
        ],
        &CompilerOptions {
            target: "es2022".to_string(),
            declaration: true,
            no_check: true,
            no_emit_on_error: false,
            ..CompilerOptions::default()
        },
    );
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert!(
        output
            .emitted_files
            .iter()
            .all(|file| { matches!(file.path.to_str(), Some("stable.js" | "stable.d.ts")) })
    );
    assert_eq!(output.emitted_files.len(), 2, "{:#?}", output.emitted_files);
    assert!(output.emitted_files.iter().any(|file| !file.declaration));
    assert!(output.emitted_files.iter().any(|file| file.declaration));

    let assertion = compile("const value = <Cedar>(renamed);", false, true);
    assert_eq!(assertion.semantic_completion, SemanticCompletion::Deferred);
    assert!(assertion.emitted_files.is_empty(), "{assertion:#?}");
}

#[test]
fn angle_assertion_type_names_remain_typed_nonclaims() {
    for source in [
        "type Cedar = number; const renamed = 0; const value = <Cedar>renamed;",
        "const Cedar = 1; const renamed = 0; const value = <Cedar>renamed;",
        "const renamed = 0; const value = <MissingType>renamed;",
    ] {
        let output = compile(source, true, false);
        assert!(output.diagnostics.is_empty(), "{source}: {output:#?}");
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert!(output.emitted_files.is_empty(), "{source}: {output:#?}");
    }
}

#[test]
fn semantic_nonclaims_reenter_nested_function_expression_owners() {
    let generic = concat!(
        "const outer = function<Item>(callback = ",
        "function renamed(): string { return 1; }) { };\n",
        "const sibling: MissingSibling = 1;\n",
    );
    let output = compile(generic, true, false);
    let [sibling] = output.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", output.diagnostics);
    };
    assert_eq!(sibling.code, 2304);
    assert_eq!(
        sibling.start,
        generic.find("MissingSibling").unwrap() as u32
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);

    let local = concat!(
        "const outer = function(this: unknown, callback = ",
        "function renamed(): string { return 1; }) { };\n",
        "const sibling: MissingSibling = 1;\n",
    );
    let output = compile(local, true, false);
    let [mismatch, sibling] = output.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", output.diagnostics);
    };
    assert_eq!(
        (mismatch.code, mismatch.start, mismatch.length),
        (2322, local.find("return").unwrap() as u32, 6,)
    );
    assert_eq!(
        (sibling.code, sibling.start, sibling.length),
        (
            2304,
            local.find("MissingSibling").unwrap() as u32,
            "MissingSibling".len() as u32,
        )
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn unowned_function_expression_modifiers_withhold_products_and_name_fallout() {
    for source in [
        "export const value = async function changed() { };",
        "export const value = function* changed() { };",
        "export const value = async function* changed() { };",
    ] {
        let output = Compiler::new().compile(
            vec![SourceInput::new(
                "function-expression.ts",
                Arc::<str>::from(source),
            )],
            &CompilerOptions {
                target: "es2022".to_string(),
                declaration: true,
                no_check: true,
                no_emit_on_error: false,
                ..CompilerOptions::default()
            },
        );
        assert!(output.emitted_files.is_empty(), "{source}: {output:#?}");
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }

    let checked = compile(
        concat!(
            "const value = async function changed() { };\n",
            "const independent: MissingIndependent = 1;\n",
        ),
        true,
        false,
    );
    let [diagnostic] = checked.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", checked.diagnostics);
    };
    assert_eq!(diagnostic.code, 2304);
    assert_eq!(
        diagnostic.start,
        "const value = async function changed() { };\nconst independent: ".len() as u32
    );
    assert_eq!(diagnostic.length, "MissingIndependent".len() as u32);
    assert_eq!(
        diagnostic.message_text,
        "Cannot find name 'MissingIndependent'."
    );
    assert_eq!(checked.semantic_completion, SemanticCompletion::Deferred);

    for source in [
        "const value = function changed() { };",
        "const async = 1;\nconst value = async\nfunction changed() { }\n",
    ] {
        let output = compile(source, false, true);
        assert!(
            output.emitted_files.iter().any(|file| !file.declaration),
            "{source}: {output:#?}",
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }
}

#[test]
fn generic_arrow_keeps_javascript_but_withholds_unowned_declaration_output() {
    for (path, source) in [
        (
            "generic-arrow.ts",
            "export const identity = <Cedar,>(value: Cedar): Cedar => value;",
        ),
        (
            "named-tuple.ts",
            "export type Renamed = [label: string]; export const stable = 1;",
        ),
    ] {
        let output = Compiler::new().compile(
            vec![SourceInput::new(path, Arc::<str>::from(source))],
            &CompilerOptions {
                target: "es2022".to_string(),
                declaration: true,
                no_check: true,
                no_emit_on_error: false,
                ..CompilerOptions::default()
            },
        );
        assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
        assert!(output.emitted_files.iter().any(|file| !file.declaration));
        assert!(output.emitted_files.iter().all(|file| !file.declaration));
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }
}

#[test]
fn recovered_function_like_binding_patterns_withhold_emit_products() {
    for source in [
        "export const callback = ({}) => 1;",
        "export const callback = function ([]) { return 1; };",
        "export const callback = ({ value: renamed }: any) => renamed;",
        "export const callback = function ({ value: changed }: any) { return changed; };",
    ] {
        let output = Compiler::new().compile(
            vec![SourceInput::new(
                "function-binding-pattern.ts",
                Arc::<str>::from(source),
            )],
            &CompilerOptions {
                declaration: true,
                no_check: true,
                no_emit_on_error: false,
                ..CompilerOptions::default()
            },
        );
        assert!(output.emitted_files.is_empty(), "{source}: {output:#?}");
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }
}

#[test]
fn recovered_arrow_binding_values_use_the_function_like_capability_owner() {
    let source = concat!(
        "const callback = ({ value: renamed }: any) => renamed;\n",
        "const independent: MissingIndependent = 1;\n",
    );
    let output = compile(source, true, false);
    let [diagnostic] = output.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", output.diagnostics);
    };
    assert_eq!(diagnostic.code, 2304);
    assert_eq!(
        diagnostic.start,
        source.find("MissingIndependent").unwrap() as u32
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn function_expression_emit_nonclaims_are_scoped_to_unowned_products() {
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "function-expression.ts",
            Arc::<str>::from("export const value = function broken(@) { };"),
        )],
        &CompilerOptions {
            declaration: true,
            no_emit_on_error: false,
            ..CompilerOptions::default()
        },
    );
    assert!(output.emitted_files.is_empty());
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);

    let unsupported_inline = compile(
        "const value = function () { if (true) return 1; };",
        false,
        true,
    );
    assert!(unsupported_inline.emitted_files.is_empty());
    assert_eq!(
        unsupported_inline.semantic_completion,
        SemanticCompletion::Deferred
    );

    let authored = Compiler::new().compile(
        vec![SourceInput::new(
            "function-expression.ts",
            Arc::<str>::from(
                "export const value = function inner(input: number): string { return ''; };",
            ),
        )],
        &CompilerOptions {
            declaration: true,
            ..CompilerOptions::default()
        },
    );
    assert!(authored.emitted_files.iter().any(|file| !file.declaration));
    assert!(authored.emitted_files.iter().all(|file| !file.declaration));
    assert_eq!(authored.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn javascript_emit_waits_for_function_product_interaction_owners() {
    let emit = |source: &str, target: &str, module: &str| {
        Compiler::new().compile(
            vec![SourceInput::new(
                "renamed-function-products.ts",
                Arc::<str>::from(source),
            )],
            &CompilerOptions {
                target: target.to_string(),
                module: module.to_string(),
                no_check: true,
                ..CompilerOptions::default()
            },
        )
    };

    for output in [
        emit(
            concat!(
                "class ChangedHost {\n",
                "  retained = 1;\n",
                "  method() { return function renamed(value: number) { return value; }; }\n",
                "}\n",
            ),
            "es2015",
            "preserve",
        ),
        emit(
            concat!(
                "import { renamed } from './dependency';\n",
                "const callback = function changed() { return renamed; };\n",
            ),
            "es2022",
            "commonjs",
        ),
        emit(
            "const callbacks = [/*outside*/ function changed() { }];\n",
            "es2022",
            "preserve",
        ),
    ] {
        assert!(
            output.emitted_files.is_empty(),
            "{:#?}",
            output.emitted_files
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }

    for output in [
        emit(
            concat!(
                "class ChangedHost {\n",
                "  retained = 1;\n",
                "  method() { return function renamed(value: number) { return value; }; }\n",
                "}\n",
            ),
            "es2022",
            "preserve",
        ),
        emit(
            "const callback = function changed(value: number) { return value; };\n",
            "es2022",
            "preserve",
        ),
        emit(
            "function changed(value: number) {\n  return value;\n}\n",
            "es2022",
            "preserve",
        ),
        emit(
            "function changed(value: number) { return value; }\n",
            "es2022",
            "preserve",
        ),
    ] {
        assert!(
            output.emitted_files.iter().any(|file| !file.declaration),
            "{:#?}",
            output.emitted_files,
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }
}
