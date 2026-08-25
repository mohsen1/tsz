use std::path::PathBuf;
use std::sync::Arc;

use tsz::service::LanguageService;
use tsz::source::{FileId, SourceText};
use tsz::syntax::{BinaryOperator, Expression, ExpressionKind, StatementKind, parse_source};
use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

fn parse_expression(source: &str) -> Expression {
    let parsed = parse_source(&SourceText::new(
        FileId(0),
        PathBuf::from("unsigned-shift.ts"),
        Arc::<str>::from(source),
    ));
    assert_eq!(
        parsed.diagnostics,
        [],
        "{source}: {:#?}",
        parsed.diagnostics
    );
    let [statement] = parsed.unit.statements.as_slice() else {
        panic!("one expression statement expected: {source}")
    };
    let StatementKind::Expression(expression) = &statement.kind else {
        panic!("expression statement expected: {source}")
    };
    expression.clone()
}

fn operator_text(operator: BinaryOperator) -> &'static str {
    match operator {
        BinaryOperator::UnsignedRightShift => ">>>",
        BinaryOperator::LessThan => "<",
        BinaryOperator::BitwiseOr => "|",
        BinaryOperator::Add => "+",
        BinaryOperator::Multiply => "*",
        _ => panic!("unexpected operator: {operator:?}"),
    }
}

fn expression_shape(expression: &Expression) -> String {
    match &expression.kind {
        ExpressionKind::Identifier { name, .. } => name.clone(),
        ExpressionKind::Binary {
            left,
            operator,
            right,
            ..
        } => format!(
            "({} {} {})",
            expression_shape(left),
            operator_text(*operator),
            expression_shape(right)
        ),
        ExpressionKind::Parenthesized(inner) => format!("({})", expression_shape(inner)),
        kind => panic!("unexpected expression: {kind:?}"),
    }
}

fn compile_files(files: &[(&str, &str)], options: CompilerOptions) -> tsz::CompileOutput {
    Compiler::new().compile(
        files
            .iter()
            .map(|(path, source)| SourceInput::new(*path, Arc::<str>::from(*source)))
            .collect(),
        &options,
    )
}

fn checked_options() -> CompilerOptions {
    CompilerOptions {
        no_emit: true,
        strict: true,
        target: "es2022".to_string(),
        ..CompilerOptions::default()
    }
}

#[test]
fn unsigned_shift_has_typescript_precedence_associativity_and_parentheses() {
    for (source, expected) in [
        (
            "cedar < birch >>> pine + oak * elm;",
            "(cedar < (birch >>> (pine + (oak * elm))))",
        ),
        (
            "renamed < willow >>> fir + ash * yew;",
            "(renamed < (willow >>> (fir + (ash * yew))))",
        ),
        ("cedar >>> birch >>> pine;", "((cedar >>> birch) >>> pine)"),
        (
            "cedar >>> (birch >>> pine);",
            "(cedar >>> ((birch >>> pine)))",
        ),
        ("cedar | birch >>> pine;", "(cedar | (birch >>> pine))"),
        ("cedar >>> birch | pine;", "((cedar >>> birch) | pine)"),
        ("cedar >>> birch < pine;", "((cedar >>> birch) < pine)"),
    ] {
        assert_eq!(
            expression_shape(&parse_expression(source)),
            expected,
            "{source}"
        );
    }
}

#[test]
fn unsigned_shift_assignment_remains_separate_parser_recovery() {
    for source in ["cedar >>>= birch;", ">>>= cedar;"] {
        let parsed = parse_source(&SourceText::new(
            FileId(0),
            PathBuf::from("assignment.ts"),
            Arc::<str>::from(source),
        ));
        assert!(!parsed.diagnostics.is_empty());
        assert!(parsed.unit.statements.iter().all(|statement| {
            !matches!(
                statement.kind,
                StatementKind::Expression(Expression {
                    kind: ExpressionKind::Binary {
                        operator: BinaryOperator::UnsignedRightShift,
                        ..
                    },
                    ..
                })
            )
        }));
        let output = compile_files(
            &[("assignment.ts", source)],
            CompilerOptions {
                no_check: true,
                target: "es2022".to_string(),
                ..CompilerOptions::default()
            },
        );
        assert!(output.emitted_files.is_empty(), "{source}");
    }
}

#[test]
fn generic_call_closes_stay_distinct_from_unsigned_shift_recovery() {
    let valid = concat!(
        "declare function f<T>():T;",
        "export const tight=f<number>()>>>0;",
        "export const nested=f<Array<number>>()>>>0;",
    );
    for path in ["generic-valid.ts", "generic-valid.tsx"] {
        let parsed = parse_source(&SourceText::new(
            FileId(0),
            PathBuf::from(path),
            Arc::<str>::from(valid),
        ));
        assert_eq!(parsed.diagnostics, [], "{path}: {:#?}", parsed.diagnostics);
        let output = compile_files(
            &[(path, valid)],
            CompilerOptions {
                no_check: true,
                module: "esnext".to_string(),
                target: "es2022".to_string(),
                ..CompilerOptions::default()
            },
        );
        assert_eq!(output.diagnostics, [], "{path}");
        assert_eq!(
            output.emitted_files[0].text,
            concat!(
                "export const tight = f() >>> 0;\n",
                "export const nested = f() >>> 0;\n",
            ),
            "{path}"
        );
    }

    for (path, expression) in [
        ("generic-invalid-tight.ts", "f<number>>>>0"),
        ("generic-invalid-tight.tsx", "f<number>>>>0"),
        ("generic-invalid-spaced.ts", "f<number> >>> 0"),
        ("generic-invalid-spaced.tsx", "f<number> >>> 0"),
    ] {
        let invalid = format!("declare function f<T>():T;export const invalid={expression};");
        let parsed = parse_source(&SourceText::new(
            FileId(0),
            PathBuf::from(path),
            Arc::<str>::from(invalid.as_str()),
        ));
        assert!(!parsed.diagnostics.is_empty(), "{path}");
        let output = compile_files(
            &[(path, &invalid)],
            CompilerOptions {
                no_check: true,
                module: "esnext".to_string(),
                target: "es2022".to_string(),
                ..CompilerOptions::default()
            },
        );
        assert!(output.emitted_files.is_empty(), "{path}");
    }
}

#[test]
fn unsigned_shift_is_one_operation_local_deferred_type_for_all_operand_controls() {
    // Pinned TypeScript 7.0.2: unknown operands each produce TS18046; bigint/bigint
    // produces TS2365; number and any combinations produce number. TSZ publishes
    // none of those answers until the complete operation is owned.
    for (kind, renamed) in [
        ("number", "cedar"),
        ("any", "willow"),
        ("unknown", "fir"),
        ("bigint", "ash"),
    ] {
        let source = format!(
            "declare const {renamed}Left:{kind};declare const {renamed}Right:{kind};{renamed}Left>>>{renamed}Right;"
        );
        let output = compile_files(&[("case.ts", &source)], checked_options());
        assert_eq!(
            output.diagnostics,
            [],
            "{source}: {:#?}",
            output.diagnostics
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{source}"
        );
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    }

    // TS6807 is a suggestion at top level and an error in enum members, may
    // coexist with TS18046/TS2365, and owns the whole binary span. A shift by
    // 32 therefore remains Deferred rather than falsely Complete.
    let overshift = compile_files(
        &[("overshift.ts", "const value=4294967295>>>32;")],
        checked_options(),
    );
    assert_eq!(overshift.diagnostics, []);
    assert_eq!(overshift.semantic_completion, SemanticCompletion::Deferred);

    let missing_source = "declare const cedar:number;cedar>>>MissingOperand;";
    let missing = compile_files(&[("missing.ts", missing_source)], checked_options());
    let [diagnostic] = missing.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", missing.diagnostics)
    };
    assert_eq!(diagnostic.code, 2304);
    assert_eq!(
        (diagnostic.start, diagnostic.length),
        (
            missing_source.find("MissingOperand").unwrap() as u32,
            "MissingOperand".len() as u32,
        )
    );
    assert_eq!(missing.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn dependent_relations_defer_while_independent_diagnostics_survive() {
    let same = concat!(
        "declare const left:number;declare const right:number;",
        "const dependent:string=left>>>right;",
        "const stable:MissingIndependent=1;",
    );
    let output = compile_files(&[("same.ts", same)], checked_options());
    let [diagnostic] = output.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", output.diagnostics)
    };
    assert_eq!(
        (diagnostic.file.as_str(), diagnostic.code),
        ("same.ts", 2304)
    );
    assert_eq!(
        (diagnostic.start, diagnostic.length),
        (
            same.find("MissingIndependent").unwrap() as u32,
            "MissingIndependent".len() as u32,
        )
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);

    let affected = "declare const left:number;const dependent:string=left>>>0;";
    let stable = "const stable:MissingCrossFile=1;";
    let cross = compile_files(
        &[("affected.ts", affected), ("stable.ts", stable)],
        checked_options(),
    );
    assert_eq!(
        cross
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.file.as_str(), diagnostic.code))
            .collect::<Vec<_>>(),
        [("stable.ts", 2304)]
    );
    let mut service = LanguageService::new(checked_options());
    service.open("affected.ts", Arc::<str>::from(affected));
    service.open("stable.ts", Arc::<str>::from(stable));
    assert_eq!(
        service
            .semantic_diagnostics("affected.ts")
            .semantic_completion,
        SemanticCompletion::Deferred
    );
    assert_eq!(
        service
            .semantic_diagnostics("stable.ts")
            .semantic_completion,
        SemanticCompletion::Complete
    );
}

#[test]
fn javascript_emit_preserves_unsigned_shift_on_supported_targets() {
    let source = concat!(
        "export const mixed=cedar<birch>>>pine+oak*elm;",
        "export const nested=cedar>>>(birch>>>pine);",
        "export const grouped=(cedar>>>birch)+pine;",
        "export const commented=cedar /* before */ >>> /* after */ birch;",
    );
    for target in ["es2015", "es2022", "esnext"] {
        let output = compile_files(
            &[("emit.ts", source)],
            CompilerOptions {
                no_check: true,
                module: "esnext".to_string(),
                target: target.to_string(),
                ..CompilerOptions::default()
            },
        );
        assert_eq!(
            output.diagnostics,
            [],
            "{target}: {:#?}",
            output.diagnostics
        );
        let javascript = output
            .emitted_files
            .iter()
            .find(|file| !file.declaration)
            .expect("JavaScript output");
        assert_eq!(
            javascript.text,
            concat!(
                "export const mixed = cedar < birch >>> pine + oak * elm;\n",
                "export const nested = cedar >>> (birch >>> pine);\n",
                "export const grouped = (cedar >>> birch) + pine;\n",
                "export const commented = cedar /* before */ >>> /* after */ birch;\n",
            ),
            "{target}"
        );
    }
}

#[test]
fn declaration_emit_withholds_only_inference_dependent_unsigned_shifts() {
    let affected = "export const shifted=x>>>0;";
    let stable = concat!(
        "export const y:number=x>>>0;",
        "export function f():number{return x>>>0}",
    );
    let output = compile_files(
        &[
            ("globals.d.ts", "declare const x:number;"),
            ("affected.ts", affected),
            ("stable.ts", stable),
        ],
        CompilerOptions {
            declaration: true,
            strict: true,
            module: "esnext".to_string(),
            target: "es2022".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(output.diagnostics, []);
    let paths = output
        .emitted_files
        .iter()
        .map(|file| file.path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(paths, ["affected.js", "stable.d.ts", "stable.js"]);
    assert!(output.emitted_files[0].text.contains("x >>> 0"));
    assert_eq!(
        output.emitted_files[1].text,
        concat!(
            "export declare const y: number;\n",
            "export declare function f(): number;\n",
        )
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);

    let nested = "export const fnValue=()=>{return 1>>>0};";
    let nested = compile_files(
        &[("nested.ts", nested)],
        CompilerOptions {
            declaration: true,
            module: "esnext".to_string(),
            target: "es2022".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(nested.diagnostics, []);
    assert_eq!(nested.emitted_files.len(), 1);
    assert_eq!(nested.emitted_files[0].path.to_string_lossy(), "nested.js");
    assert!(nested.emitted_files[0].text.contains("1 >>> 0"));
}

#[test]
fn inferred_variable_quick_info_defers_without_blocking_operand_navigation() {
    let source = concat!(
        "const input:number=1;",
        "const shifted=input>>>0;",
        "const typed:number=input>>>0;",
        "function defaulted(value=input>>>0):void{}",
        "function stable():number{const nestedShift=input>>>0;return 1}",
        "class Vessel{shiftedField=input>>>0}",
    );
    let mut service = LanguageService::new(checked_options());
    service.open("service.ts", Arc::<str>::from(source));
    let shifted = source.find("shifted").unwrap() as u32;
    let typed = source.find("typed").unwrap() as u32;
    assert!(service.quick_info("service.ts", shifted + 1).is_none());
    assert_eq!(
        service
            .quick_info("service.ts", typed + 1)
            .expect("annotated variable quick info")
            .display,
        "const typed: number"
    );
    let nested = source.find("nestedShift").unwrap() as u32;
    let defaulted = source.find("defaulted").unwrap() as u32;
    let field = source.find("shiftedField").unwrap() as u32;
    assert!(service.quick_info("service.ts", nested + 1).is_none());
    assert!(service.quick_info("service.ts", defaulted + 1).is_none());
    assert!(service.quick_info("service.ts", field + 1).is_none());

    let operand = source.find("input>>>").unwrap() as u32;
    let definition = service
        .definition_and_bound_span("service.ts", operand + 1)
        .expect("operand definition remains claimed");
    assert_eq!(definition.definitions.len(), 1);
    assert_eq!(definition.definitions[0].name, "input");
    let rename = service.rename("service.ts", operand + 1);
    assert!(rename.info.can_rename);
    assert_eq!(rename.locations.len(), 6);
}
