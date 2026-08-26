use std::sync::Arc;

use tsz::diagnostics::DiagnosticCategory;
use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

const LEFT_MESSAGE: &str = "The left-hand side of an 'instanceof' expression must be of type 'any', an object type or a type parameter.";
const RIGHT_MESSAGE: &str = "The right-hand side of an 'instanceof' expression must be either of type 'any', a class, function, or other type assignable to the 'Function' interface type, or an object type with a 'Symbol.hasInstance' method.";

fn options() -> CompilerOptions {
    CompilerOptions {
        no_emit: true,
        strict: true,
        target: "es2015".to_string(),
        ..CompilerOptions::default()
    }
}

fn compile(files: &[(&str, &str)]) -> tsz::CompileOutput {
    Compiler::new().compile(
        files
            .iter()
            .map(|(path, source)| SourceInput::new(*path, Arc::<str>::from(*source)))
            .collect(),
        &options(),
    )
}

fn assert_diagnostic(
    diagnostic: &tsz::diagnostics::Diagnostic,
    source: &str,
    needle: &str,
    code: u32,
    message: &str,
) {
    assert_eq!(diagnostic.code, code, "{diagnostic:#?}");
    assert_eq!(diagnostic.category, DiagnosticCategory::Error);
    assert_eq!(
        (diagnostic.start, diagnostic.length),
        (
            source.find(needle).expect("diagnostic target") as u32,
            needle.len() as u32,
        ),
    );
    assert_eq!(diagnostic.message_text, message);
    assert!(diagnostic.related_information.is_empty());
}

#[test]
fn definitive_invalid_operands_publish_independent_exact_diagnostics() {
    let source = "const primitive = 'x'; const result = ((primitive)) instanceof ((2));";
    let output = compile(&[("invalid.ts", source)]);
    let [left, right] = output.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", output.diagnostics);
    };
    assert_diagnostic(left, source, "((primitive))", 2358, LEFT_MESSAGE);
    assert_diagnostic(right, source, "((2))", 2359, RIGHT_MESSAGE);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        output.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );
}

#[test]
fn empty_object_rhs_is_definitively_not_callable() {
    let source = "declare const dynamic: any; dynamic instanceof {};";
    let output = compile(&[("empty-object.ts", source)]);
    let [diagnostic] = output.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", output.diagnostics);
    };
    assert_diagnostic(diagnostic, source, "{}", 2359, RIGHT_MESSAGE);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn callable_rhs_and_nonprimitive_lhs_are_complete_under_renaming() {
    for source in [
        "declare const dynamic: any; dynamic instanceof function renamed() {};",
        "class Renamed {} declare const value: {}; value instanceof Renamed;",
        "declare const dynamic: any; dynamic instanceof (() => 1);",
        "declare const dynamic: any; dynamic instanceof Function;",
    ] {
        let output = compile(&[("valid.ts", source)]);
        assert_eq!(
            output.diagnostics,
            [],
            "{source}: {:#?}",
            output.diagnostics
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source}"
        );
        assert_eq!(output.exit_status, CompileExitStatus::Success, "{source}");
    }
}

#[test]
fn primitive_unions_and_type_parameter_constraints_use_the_typed_domain() {
    let source = concat!(
        "function primitives(value: string | number) { value instanceof Object; }",
        "function constrained<T extends string>(value: T) { value instanceof Object; }",
        "function unconstrained<T>(value: T, constructor: T) {",
        "value instanceof Object; ({} as any) instanceof constructor; }",
    );
    let output = compile(&[("generic.ts", source)]);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [2358, 2358, 2359],
        "{:#?}",
        output.diagnostics,
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn computed_has_instance_and_symbolic_constraints_fail_closed() {
    let source = concat!(
        "declare const dynamic: any;",
        "interface Custom { [Symbol.hasInstance](value: unknown): boolean; }",
        "declare const custom: Custom; dynamic instanceof custom;",
        "dynamic instanceof ({} as Function);",
        "function symbolic<T extends string | number>(value: T) { value instanceof Object; }",
    );
    let output = compile(&[("deferred.ts", source)]);
    assert_eq!(output.diagnostics, [], "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
}

#[test]
fn decisive_union_members_are_independent_of_authored_order() {
    let source = concat!(
        "function leftFirst<T extends string | number>(value: T | {}) {",
        "value instanceof function () {}; }",
        "function leftLast<T extends string | number>(value: {} | T) {",
        "value instanceof function () {}; }",
        "function rightFirst<T extends Function>(value: T | {}) {",
        "({} as any) instanceof value; }",
        "function rightLast<T extends Function>(value: {} | T) {",
        "({} as any) instanceof value; }",
    );
    let output = compile(&[("union-order.ts", source)]);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [2359, 2359],
        "{:#?}",
        output.diagnostics,
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn logical_consumers_fail_closed_without_withdrawing_independent_boolean_use() {
    let dependent = concat!(
        "class Guard { member: string = ''; }",
        "declare const candidate: Guard | string;",
        "const result: string | false = candidate instanceof Guard && candidate.member;",
    );
    let output = compile(&[("logical-dependent.ts", dependent)]);
    assert_eq!(output.diagnostics, [], "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);

    let independent = concat!(
        "class Guard {} declare const candidate: {};",
        "const result = true && (candidate instanceof Guard);",
    );
    let output = compile(&[("logical-independent.ts", independent)]);
    assert_eq!(output.diagnostics, [], "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn flow_nonclaim_is_path_local_and_keeps_independent_diagnostics() {
    let source = concat!(
        "class Guard {}",
        "function inspect(value: { tag: string | number; other: string | number }) {",
        "if (value.tag instanceof Guard) {",
        "const dependent: string = value.tag;",
        "const independent: string = value.other;",
        "MissingInside;",
        "}",
        "const after: string = value.other;",
        "MissingAfter;",
        "}",
    );
    let output = compile(&[("flow.ts", source)]);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [2358, 2322, 2304, 2322, 2304],
        "{:#?}",
        output.diagnostics,
    );
    assert!(
        output
            .diagnostics
            .iter()
            .all(|diagnostic| { !source[diagnostic.start as usize..].starts_with("dependent") })
    );
}

#[test]
fn negated_instanceof_condition_keeps_the_same_path_local_boundary() {
    let source = concat!(
        "class Guard {}",
        "function inspect(value: { tag: string | number; other: string | number }) {",
        "if (!(value.tag instanceof Guard)) {",
        "const dependent: string = value.tag;",
        "const independent: string = value.other;",
        "}",
        "}",
    );
    let output = compile(&[("negated-flow.ts", source)]);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [2358, 2322],
        "{:#?}",
        output.diagnostics,
    );
    assert!(
        output
            .diagnostics
            .iter()
            .all(|diagnostic| !source[diagnostic.start as usize..].starts_with("dependent"))
    );
}

#[test]
fn aliases_root_order_and_repeated_compiles_are_stable() {
    let declarations = "class Constructor {} const renamed = 'x';";
    let use_site = "renamed instanceof Constructor;";
    let roots = [("defs.ts", declarations), ("use.ts", use_site)];
    let mut reversed = roots;
    reversed.reverse();
    for files in [&roots[..], &roots[..], &reversed[..]] {
        let output = compile(files);
        let [diagnostic] = output.diagnostics.as_slice() else {
            panic!("unexpected diagnostics: {:#?}", output.diagnostics);
        };
        assert_diagnostic(diagnostic, use_site, "renamed", 2358, LEFT_MESSAGE);
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }
}
