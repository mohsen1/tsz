use std::path::PathBuf;
use std::sync::Arc;

use tsz::diagnostics::DiagnosticCategory;
use tsz::service::LanguageService;
use tsz::source::{FileId, SourceText};
use tsz::syntax::{
    AssignmentOperator, BinaryOperator, Expression, ExpressionKind, StatementKind, VariableKind,
    parse_source,
};
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
        BinaryOperator::LeftShift => "<<",
        BinaryOperator::SignedRightShift => ">>",
        BinaryOperator::UnsignedRightShift => ">>>",
        BinaryOperator::LessThan => "<",
        BinaryOperator::BitwiseOr => "|",
        BinaryOperator::BitwiseXor => "^",
        BinaryOperator::BitwiseAnd => "&",
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
fn variable_lists_and_compound_add_are_structural_syntax() {
    let source = concat!(
        "var alpha, beta = input << amount, gamma: number = beta ^ input >>> count;",
        "alpha += gamma;",
    );
    let parsed = parse_source(&SourceText::new(
        FileId(0),
        PathBuf::from("variable-list.ts"),
        Arc::<str>::from(source),
    ));
    assert_eq!(parsed.diagnostics, [], "{:#?}", parsed.diagnostics);
    let [variable, assignment] = parsed.unit.statements.as_slice() else {
        panic!("one variable statement and one assignment expected")
    };
    let StatementKind::Variable(variable) = &variable.kind else {
        panic!("variable statement expected")
    };
    assert_eq!(variable.declaration_kind, VariableKind::Var);
    assert_eq!(
        variable
            .declarators
            .iter()
            .map(|declaration| declaration.name.as_str())
            .collect::<Vec<_>>(),
        ["alpha", "beta", "gamma"],
    );
    assert!(variable.declarators[0].initializer.is_none());
    assert_eq!(
        expression_shape(
            variable.declarators[1]
                .initializer
                .as_ref()
                .expect("beta initializer"),
        ),
        "(input << amount)",
    );
    assert!(variable.declarators[2].annotation.is_some());
    assert_eq!(
        expression_shape(
            variable.declarators[2]
                .initializer
                .as_ref()
                .expect("gamma initializer"),
        ),
        "(beta ^ (input >>> count))",
    );
    assert!(matches!(
        &assignment.kind,
        StatementKind::Expression(Expression {
            kind: ExpressionKind::Assignment {
                operator: AssignmentOperator::AddAssign,
                left,
                right,
                ..
            },
            ..
        }) if matches!(&left.kind, ExpressionKind::Identifier { name, .. } if name == "alpha")
            && matches!(&right.kind, ExpressionKind::Identifier { name, .. } if name == "gamma")
    ));
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
        (
            "cedar | birch ^ pine & oak;",
            "(cedar | (birch ^ (pine & oak)))",
        ),
        ("cedar + birch << pine;", "((cedar + birch) << pine)"),
        ("cedar << birch + pine;", "(cedar << (birch + pine))"),
        ("cedar << birch >> pine;", "((cedar << birch) >> pine)"),
    ] {
        assert_eq!(
            expression_shape(&parse_expression(source)),
            expected,
            "{source}"
        );
    }
}

#[test]
fn renamed_nested_operator_controls_have_no_oracle_absent_diagnostics() {
    let source = concat!(
        "declare const holder: { value: number };",
        "function renamedOuter(flag: boolean) {",
        "var alpha, beta = holder.value, gamma: number;",
        "alpha = 1; gamma = 2;",
        "if (flag) { alpha += beta; }",
        "const shifted = (alpha << 3) | (alpha >>> 29);",
        "const signed = shifted >> 1;",
        "const combined = alpha & beta ^ gamma | shifted;",
        "return [alpha, signed, combined];",
        "}",
    );
    let output = compile_files(&[("renamed.ts", source)], checked_options());
    assert_eq!(output.diagnostics, [], "{:#?}", output.diagnostics);
}

#[test]
fn compound_add_and_new_binary_operators_report_owned_operand_errors() {
    for (path, source, code, needle) in [
        (
            "compound.ts",
            "let renamedNumber: number = 1; let renamedString: string = 'x'; renamedNumber += renamedString;",
            2322,
            "renamedNumber +=",
        ),
        (
            "boolean-xor.ts",
            "declare const left: boolean; declare const right: boolean; left ^ right;",
            2447,
            "^",
        ),
        (
            "string-shift.ts",
            "declare const text: string; text << 1;",
            2362,
            "text <<",
        ),
    ] {
        let output = compile_files(&[(path, source)], checked_options());
        let [diagnostic] = output.diagnostics.as_slice() else {
            panic!("{path}: unexpected diagnostics: {:#?}", output.diagnostics)
        };
        assert_eq!(diagnostic.code, code, "{path}: {diagnostic:#?}");
        assert_eq!(
            diagnostic.start,
            source.find(needle).expect("diagnostic target") as u32,
            "{path}",
        );
        if code == 2447 {
            assert!(diagnostic.message_text.contains("'!=='"), "{diagnostic:#?}");
        }
    }
}

#[test]
fn javascript_emit_preserves_variable_lists_compound_add_and_precedence() {
    let source = concat!(
        "export var alpha, beta=input<<2, gamma=beta^input>>>1;",
        "alpha += gamma;",
        "export const grouped=(alpha^beta)<<gamma;",
    );
    let output = compile_files(
        &[("emit.ts", source)],
        CompilerOptions {
            no_check: true,
            module: "esnext".to_string(),
            target: "es2022".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(output.diagnostics, [], "{:#?}", output.diagnostics);
    assert_eq!(
        output.emitted_files[0].text,
        concat!(
            "export var alpha, beta = input << 2, gamma = beta ^ input >>> 1;\n",
            "alpha += gamma;\n",
            "export const grouped = (alpha ^ beta) << gamma;\n",
        ),
    );
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
fn bounded_literal_unsigned_shift_completes_only_for_number_left_operands() {
    for (path, source) in [
        ("direct.ts", "declare const cedar:number;cedar>>>0;"),
        ("renamed.ts", "declare const willow:31;((willow))>>>((31));"),
        (
            "call.ts",
            "declare function locate(value:number):number;locate(1)>>>0;",
        ),
        (
            "literal-alias.ts",
            "declare const birch:number;declare const amount:0;birch>>>amount;",
        ),
    ] {
        let output = compile_files(&[(path, source)], checked_options());
        assert_eq!(output.diagnostics, [], "{path}: {:#?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{path}"
        );
        assert_eq!(output.exit_status, CompileExitStatus::Success, "{path}");
    }

    // These values are valid or diagnosable in TS7, but stay Deferred until
    // constant folding, negative/fractional counts, and operand diagnostics
    // are owned together with TS6807.
    for (path, source) in [
        (
            "number-right.ts",
            "declare const left:number;declare const amount:number;left>>>amount;",
        ),
        (
            "folded-right.ts",
            "declare const left:number;left>>>(16+16);",
        ),
        ("overshift.ts", "declare const left:number;left>>>32;"),
        ("negative.ts", "declare const left:number;left>>>-1;"),
        ("fractional.ts", "declare const left:number;left>>>1.5;"),
        ("any-left.ts", "declare const left:any;left>>>0;"),
        ("unknown-left.ts", "declare const left:unknown;left>>>0;"),
        ("bigint-left.ts", "declare const left:bigint;left>>>0;"),
        ("string-left.ts", "declare const left:string;left>>>0;"),
    ] {
        let output = compile_files(&[(path, source)], checked_options());
        assert_eq!(output.diagnostics, [], "{path}: {:#?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{path}"
        );
        assert_eq!(
            output.exit_status,
            CompileExitStatus::SemanticIncomplete,
            "{path}"
        );
    }
}

#[test]
fn bounded_literal_shift_is_exact_across_warm_queries_runs_and_root_order() {
    let affected = concat!(
        "declare const cedar:number;",
        "const warm=cedar>>>0;",
        "const mismatch:string=cedar>>>0;",
    );
    let independent = "const stable:MissingIndependent=1;";
    let expected = vec![
        (
            "affected.ts".to_string(),
            affected.find("mismatch").unwrap() as u32,
            "mismatch".len() as u32,
            DiagnosticCategory::Error,
            2322,
            "Type 'number' is not assignable to type 'string'.".to_string(),
            0,
        ),
        (
            "independent.ts".to_string(),
            independent.find("MissingIndependent").unwrap() as u32,
            "MissingIndependent".len() as u32,
            DiagnosticCategory::Error,
            2304,
            "Cannot find name 'MissingIndependent'.".to_string(),
            0,
        ),
    ];
    let compiler = Compiler::new();
    for iteration in 0..2 {
        for files in [
            [("affected.ts", affected), ("independent.ts", independent)],
            [("independent.ts", independent), ("affected.ts", affected)],
        ] {
            let output = compiler.compile(
                files
                    .into_iter()
                    .map(|(path, source)| SourceInput::new(path, Arc::<str>::from(source)))
                    .collect(),
                &checked_options(),
            );
            let actual = output
                .diagnostics
                .iter()
                .map(|diagnostic| {
                    (
                        diagnostic.file.clone(),
                        diagnostic.start,
                        diagnostic.length,
                        diagnostic.category,
                        diagnostic.code,
                        diagnostic.message_text.clone(),
                        diagnostic.related_information.len(),
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "iteration {iteration}");
            assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
            assert_eq!(
                output.exit_status,
                CompileExitStatus::DiagnosticsPresentOutputsSkipped
            );
        }
    }
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
        [("affected.ts", 2322), ("stable.ts", 2304)]
    );
    let mut service = LanguageService::new(checked_options());
    service.open("affected.ts", Arc::<str>::from(affected));
    service.open("stable.ts", Arc::<str>::from(stable));
    assert_eq!(
        service
            .semantic_diagnostics("affected.ts")
            .semantic_completion,
        SemanticCompletion::Complete
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
    let checked = compile_files(
        &[
            ("globals.d.ts", "declare const x:number;"),
            ("affected.ts", affected),
        ],
        checked_options(),
    );
    assert_eq!(checked.diagnostics, []);
    assert_eq!(checked.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(checked.exit_status, CompileExitStatus::Success);

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
