use super::*;

#[test]
fn inferred_implementation_returns_have_one_typed_declaration_boundary() {
    let affected = program_file(
        0,
        "affected.ts",
        concat!(
            "export function cedar(){return 1}",
            "export async function ash(){return 1}",
            "export function birch(){const value=1}",
        ),
    );
    let [cedar, ash, birch] = affected.syntax.statements.as_slice() else {
        panic!("three function declarations expected")
    };
    let analysis = default_analysis(&affected);
    assert!(
        analysis
            .claim(
                CapabilityTarget::JavaScript,
                CapabilityScope::File(affected.source.id),
            )
            .is_claimed()
    );
    let CapabilityClaim::Nonclaimed(reasons) = analysis.claim(
        CapabilityTarget::Declaration,
        CapabilityScope::File(affected.source.id),
    ) else {
        panic!("the declaration product requires checked return summaries")
    };
    assert_eq!(
        reasons.copied().collect::<Vec<_>>(),
        [ash.id, birch.id]
            .map(|owner| CapabilityNonclaim {
                target: CapabilityTarget::Declaration,
                scope: CapabilityScope::node(affected.source.id, owner),
                reason: NonclaimReason::Semantic(SemanticGap::DeclarationFunctionSummary),
            })
            .to_vec()
    );
    assert!(
        analysis
            .claim(
                CapabilityTarget::Declaration,
                CapabilityScope::node(affected.source.id, cedar.id),
            )
            .is_claimed(),
        "the ordinary literal-return summary is checker-owned",
    );

    for (path, source) in [
        (
            "controls.ts",
            "function empty(){} declare function bodyless(); function typed():number{return 1}",
        ),
        (
            "nested.ts",
            "function wrapper():void{function nested(){return 1}}",
        ),
    ] {
        let file = program_file(0, path, source);
        let claims = default_analysis(&file);
        for target in [CapabilityTarget::JavaScript, CapabilityTarget::Declaration] {
            assert!(
                claims
                    .claim(target, CapabilityScope::File(file.source.id))
                    .is_claimed(),
                "{path}: {target:?}",
            );
        }
    }
}

#[test]
fn inferred_async_returns_remain_typed_declaration_nonclaims() {
    for (path, source, expected_claimed) in [
        ("ordinary.ts", "function ordinary(){return 1}", true),
        ("async.ts", "async function deferred(){return 1}", false),
        (
            "exported.ts",
            "export function ordinary(){return 'cedar'}",
            true,
        ),
        (
            "exported-async.ts",
            "export async function deferred(){return 'cedar'}",
            false,
        ),
        (
            "default.ts",
            "export default function named(){return true}",
            true,
        ),
        (
            "default-async.ts",
            "export default async function named(){return true}",
            false,
        ),
        (
            "anonymous-default.ts",
            "export default function (){return 1}",
            true,
        ),
        (
            "anonymous-default-async.ts",
            "export default async function (){return 1}",
            false,
        ),
    ] {
        let file = program_file(0, path, source);
        let [statement] = file.syntax.statements.as_slice() else {
            panic!("{path}: one function declaration expected")
        };
        let StatementKind::Function(function) = &statement.kind else {
            panic!("{path}: function declaration expected")
        };
        for target in ["es2015", "es2022"] {
            for no_check in [false, true] {
                let analysis = CapabilityAnalysis::derive(
                    std::slice::from_ref(&file),
                    &CompilerOptions {
                        target: target.to_string(),
                        no_check,
                        ..CompilerOptions::default()
                    },
                    CapabilityContext::default(),
                );
                let claim = analysis.claim(
                    CapabilityTarget::Declaration,
                    CapabilityScope::node(file.source.id, statement.id),
                );
                let has_inferred_summary_nonclaim = match &claim {
                    CapabilityClaim::Claimed => false,
                    CapabilityClaim::Nonclaimed(reasons) => reasons.clone().any(|reason| {
                        reason.reason
                            == NonclaimReason::Semantic(SemanticGap::DeclarationFunctionSummary)
                    }),
                };
                assert_eq!(
                    has_inferred_summary_nonclaim, !expected_claimed,
                    "{path}: target={target}, noCheck={no_check}, async={}",
                    function.is_async,
                );
                if !function.default_export {
                    assert_eq!(
                        claim.is_claimed(),
                        expected_claimed,
                        "{path}: target={target}, noCheck={no_check}",
                    );
                }
            }
        }
    }
}

#[test]
fn inferred_async_nonclaims_never_publish_partial_declarations() {
    for target in ["es2015", "es2022"] {
        for no_check in [false, true] {
            let output = Compiler::new().compile(
                vec![
                    SourceInput::new(
                        "ordinary.ts",
                        Arc::<str>::from("export function ordinary(){return 1}"),
                    ),
                    SourceInput::new(
                        "async-named.ts",
                        Arc::<str>::from("export async function deferred(){return 1}"),
                    ),
                    SourceInput::new(
                        "async-default.ts",
                        Arc::<str>::from("export default async function deferred(){return 1}"),
                    ),
                ],
                &CompilerOptions {
                    declaration: true,
                    module: "esnext".to_string(),
                    target: target.to_string(),
                    no_check,
                    ..CompilerOptions::default()
                },
            );
            assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
            assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
            assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
            let declarations = output
                .emitted_files
                .iter()
                .filter(|file| file.declaration)
                .map(|file| (file.path.to_string_lossy().into_owned(), file.text.as_str()))
                .collect::<Vec<_>>();
            assert_eq!(
                declarations,
                [(
                    "ordinary.d.ts".to_string(),
                    "export declare function ordinary(): number;\n"
                )],
                "target={target}, noCheck={no_check}",
            );
            for path in ["async-named.d.ts", "async-default.d.ts"] {
                assert!(
                    output
                        .emitted_files
                        .iter()
                        .all(|file| file.path != std::path::Path::new(path)),
                    "{path} must not publish a partial declaration product",
                );
            }
        }
    }
}

#[test]
fn inferred_return_nonclaim_withholds_only_its_file_declaration_product() {
    let output = Compiler::new().compile(
        vec![
            SourceInput::new(
                "affected.ts",
                Arc::<str>::from(
                    "export async function cedar<Leaf>(value:Leaf){return value} export const same:string='ok';",
                ),
            ),
            SourceInput::new(
                "stable.ts",
                Arc::<str>::from(
                    "export function birch():number{return 2} export const other:number=3;",
                ),
            ),
        ],
        &CompilerOptions {
            declaration: true,
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    assert_eq!(
        output
            .emitted_files
            .iter()
            .map(|file| file.path.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        ["affected.js", "stable.d.ts", "stable.js"]
    );
    assert_eq!(
        output.emitted_files[0].text,
        "export async function cedar(value) { return value; }\nexport const same = 'ok';\n"
    );
    assert_eq!(
        output.emitted_files[1].text,
        "export declare function birch(): number;\nexport declare const other: number;\n"
    );
}

#[test]
fn inferred_call_products_have_one_typed_declaration_boundary() {
    let file = program_file(
        0,
        "inferred-calls.ts",
        concat!(
            "declare const values:number[];",
            "const inferred=values.indexOf(1);",
            "const typed:number=values.indexOf(1);",
            "function defaulted(value=values.indexOf(1)):void{}",
            "function typedDefault(value:number=values.indexOf(1)):void{}",
            "class Holder{inferred=values.indexOf(1);",
            "typed:number=values.indexOf(1);",
            "method(value=values.indexOf(1)):void{}",
            "typedMethod(value:number=values.indexOf(1)):void{}}",
            "export = values.indexOf(1);",
        ),
    );
    let [_, inferred, typed, defaulted, typed_default, class, export] =
        file.syntax.statements.as_slice()
    else {
        panic!("seven declarations expected")
    };
    let StatementKind::Class(class_declaration) = &class.kind else {
        panic!("class declaration expected")
    };
    let analysis = default_analysis(&file);
    let affected = [
        inferred.id,
        defaulted.id,
        class_declaration.members[0].id,
        class_declaration.members[2].id,
        export.id,
    ];
    for owner in affected {
        let scope = CapabilityScope::node(file.source.id, owner);
        let CapabilityClaim::Nonclaimed(reasons) =
            analysis.claim(CapabilityTarget::Declaration, scope)
        else {
            panic!("inferred call declaration product must stay nonclaimed")
        };
        assert_eq!(
            reasons.copied().collect::<Vec<_>>(),
            [CapabilityNonclaim {
                target: CapabilityTarget::Declaration,
                scope,
                reason: NonclaimReason::Semantic(SemanticGap::DeclarationExpressionSummary),
            }]
        );
    }
    for owner in [
        typed.id,
        typed_default.id,
        class_declaration.members[1].id,
        class_declaration.members[3].id,
    ] {
        assert!(
            analysis
                .claim(
                    CapabilityTarget::Declaration,
                    CapabilityScope::node(file.source.id, owner),
                )
                .is_claimed(),
            "authored type makes the declaration product independent"
        );
    }
    assert!(
        analysis
            .claim(
                CapabilityTarget::JavaScript,
                CapabilityScope::File(file.source.id),
            )
            .is_claimed()
    );
}

#[test]
fn inferred_binary_product_has_one_typed_declaration_boundary() {
    let file = program_file(0, "binary.ts", "export const value = 1 + {};");
    let owner = file.syntax.statements[0].id;
    let analysis = default_analysis(&file);
    let scope = CapabilityScope::node(file.source.id, owner);
    let CapabilityClaim::Nonclaimed(reasons) = analysis.claim(CapabilityTarget::Declaration, scope)
    else {
        panic!("inferred binary declaration product must wait for a checked summary")
    };
    assert_eq!(
        reasons.copied().collect::<Vec<_>>(),
        vec![CapabilityNonclaim {
            target: CapabilityTarget::Declaration,
            scope,
            reason: NonclaimReason::Semantic(SemanticGap::DeclarationExpressionSummary),
        }]
    );
    assert!(
        analysis
            .claim(
                CapabilityTarget::JavaScript,
                CapabilityScope::File(file.source.id),
            )
            .is_claimed()
    );
}
