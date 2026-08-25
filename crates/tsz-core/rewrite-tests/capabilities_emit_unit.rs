use super::*;

use crate::program::{CompileExitStatus, Compiler, SemanticCompletion, SourceInput};

#[test]
fn typescript_type_declarations_preserve_emit_claims() {
    let file = program_file(
        0,
        "owned-declarations.ts",
        concat!(
            "interface RenamedPoint { value: number; }\n",
            "type RenamedScalar = number;\n",
            "const renamed: RenamedPoint = { value: 1 };\n",
            "function project(value: number): number { return value; }\n",
        ),
    );
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    for target in [CapabilityTarget::JavaScript, CapabilityTarget::Declaration] {
        let claim = analysis.claim(target, CapabilityScope::File(file.source.id));
        assert!(
            claim.is_claimed(),
            "{target:?} must stay claimed: {claim:?}"
        );
    }

    let ordinary = program_file(0, "ordinary-recovery.ts", "const kept trailing;");
    assert!(
        ordinary
            .syntax
            .parser_recovery_facts()
            .iter()
            .any(|fact| { fact.kind == ParserRecoveryKind::Declaration })
    );
    let ordinary = CapabilityAnalysis::derive(
        std::slice::from_ref(&ordinary),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    assert!(
        [CapabilityTarget::JavaScript, CapabilityTarget::Declaration]
            .into_iter()
            .all(|target| ordinary
                .claim(target, CapabilityScope::File(FileId(0)))
                .is_claimed())
    );
}

#[test]
fn inferred_implementation_returns_have_one_typed_declaration_boundary() {
    let affected = program_file(
        0,
        "affected.ts",
        "export function cedar(){return 1} export function birch(){const value=1}",
    );
    let [cedar, birch] = affected.syntax.statements.as_slice() else {
        panic!("two function declarations expected")
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
        [cedar.id, birch.id]
            .map(|owner| CapabilityNonclaim {
                target: CapabilityTarget::Declaration,
                scope: CapabilityScope::node(affected.source.id, owner),
                reason: NonclaimReason::Semantic(SemanticGap::DeclarationFunctionSummary),
                deletion: DeletionCondition::SemanticOwner(
                    SemanticGap::DeclarationFunctionSummary,
                ),
            })
            .to_vec()
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
fn inferred_return_nonclaim_withholds_only_its_file_declaration_product() {
    let output = Compiler::new().compile(
        vec![
            SourceInput::new(
                "affected.ts",
                Arc::<str>::from(
                    "export function cedar(){return 1} export const same:string='ok';",
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
        "export function cedar() {\n    return 1;\n}\nexport const same = 'ok';\n"
    );
    assert_eq!(
        output.emitted_files[1].text,
        "export declare function birch(): number;\nexport declare const other: number;\n"
    );
}
