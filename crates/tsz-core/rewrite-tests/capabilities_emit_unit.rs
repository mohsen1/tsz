use super::*;

use crate::program::{CompileExitStatus, Compiler, SemanticCompletion, SourceInput};

#[test]
fn bodyless_object_method_records_typed_model_and_program_service_nonclaims() {
    let file = program_file(
        0,
        "bodyless.ts",
        "const holder = { renamed<Value>();, sibling: 1 };",
    );
    let analysis = default_analysis(&file);
    let owner = analysis.function_like_owners[0].1;
    let node = CapabilityScope::node(file.source.id, owner);
    for (target, scope) in [
        (CapabilityTarget::DeclarationModel, node),
        (CapabilityTarget::QuickInfo, CapabilityScope::Program),
        (CapabilityTarget::Definition, CapabilityScope::Program),
        (CapabilityTarget::References, CapabilityScope::Program),
        (CapabilityTarget::Highlights, CapabilityScope::Program),
        (CapabilityTarget::Rename, CapabilityScope::Program),
    ] {
        let CapabilityClaim::Nonclaimed(reasons) = analysis.claim(target, scope) else {
            panic!("{target:?} must wait for object-method identity");
        };
        assert!(reasons.into_iter().any(|reason| {
            reason.scope == scope
                && reason.reason == NonclaimReason::Semantic(SemanticGap::FunctionLikeService)
                && reason.deletion
                    == DeletionCondition::SemanticOwner(SemanticGap::FunctionLikeService)
        }));
    }
}

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
                deletion: DeletionCondition::SemanticOwner(
                    SemanticGap::DeclarationExpressionSummary,
                ),
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
fn unsigned_shift_withholds_only_inferred_declaration_and_quick_info_products() {
    let file = program_file(
        0,
        "unsigned-shift.ts",
        concat!(
            "declare const input:number;",
            "export const inferred=()=>{return input>>>0};",
            "export const typed:number=input>>>0;",
            "export function stable():number{return input>>>0}",
        ),
    );
    let [_, inferred, typed, stable] = file.syntax.statements.as_slice() else {
        panic!("four declarations expected")
    };
    let analysis = default_analysis(&file);
    for target in [CapabilityTarget::Declaration, CapabilityTarget::QuickInfo] {
        let scope = CapabilityScope::node(file.source.id, inferred.id);
        let CapabilityClaim::Nonclaimed(reasons) = analysis.claim(target, scope) else {
            panic!("{target:?} must wait for checked unsigned-shift inference")
        };
        assert_eq!(
            reasons.copied().collect::<Vec<_>>(),
            [CapabilityNonclaim {
                target,
                scope,
                reason: NonclaimReason::Semantic(SemanticGap::UnsignedRightShift),
                deletion: DeletionCondition::SemanticOwner(SemanticGap::UnsignedRightShift),
            }]
        );
        for owner in [typed.id, stable.id] {
            assert!(
                analysis
                    .claim(target, CapabilityScope::node(file.source.id, owner))
                    .is_claimed(),
                "authored annotations make {target:?} independent"
            );
        }
    }
    assert!(
        analysis
            .claim(
                CapabilityTarget::JavaScript,
                CapabilityScope::File(file.source.id),
            )
            .is_claimed()
    );

    let nested = program_file(
        0,
        "nested-unsigned-shift.ts",
        concat!(
            "declare const input:number;",
            "function stable():number{const local=input>>>0;",
            "function defaulted(value=input>>>0):void{}return 1}",
            "class Vessel{field=input>>>0;typed:number=input>>>0}",
        ),
    );
    let StatementKind::Function(function) = &nested.syntax.statements[1].kind else {
        panic!("function expected")
    };
    let class = &nested.syntax.statements[2];
    let nested_analysis = default_analysis(&nested);
    for owner in [function.body[0].id, function.body[1].id, class.id] {
        let scope = CapabilityScope::node(nested.source.id, owner);
        let CapabilityClaim::Nonclaimed(reasons) =
            nested_analysis.claim(CapabilityTarget::QuickInfo, scope)
        else {
            panic!("inferred QuickInfo owner must remain nonclaimed")
        };
        assert!(reasons.into_iter().any(|reason| {
            reason.scope == scope
                && reason.reason == NonclaimReason::Semantic(SemanticGap::UnsignedRightShift)
                && reason.deletion
                    == DeletionCondition::SemanticOwner(SemanticGap::UnsignedRightShift)
        }));
    }
    assert!(
        nested_analysis
            .claim(
                CapabilityTarget::Declaration,
                CapabilityScope::node(nested.source.id, nested.syntax.statements[1].id),
            )
            .is_claimed(),
        "an authored function return type is independent of its local shift"
    );

    for (path, source, gap) in [
        (
            "assignment-recovery.ts",
            "cedar>>>=birch;",
            SyntaxGap::UnsignedRightShiftAssignmentRecovery,
        ),
        (
            "prefix-assignment-recovery.ts",
            ">>>=cedar;",
            SyntaxGap::UnsignedRightShiftAssignmentRecovery,
        ),
        (
            "operand-recovery.ts",
            "declare function f<T>():T;export const invalid=f<number>>>>0;",
            SyntaxGap::UnsignedRightShiftOperandRecovery,
        ),
        (
            "operand-recovery-spaced.ts",
            "declare function f<T>():T;export const invalid=f<number> >>> 0;",
            SyntaxGap::UnsignedRightShiftOperandRecovery,
        ),
        (
            "operand-recovery-spaced.tsx",
            "declare function f<T>():T;export const invalid=f<number> >>> 0;",
            SyntaxGap::UnsignedRightShiftOperandRecovery,
        ),
    ] {
        let recovered = program_file(0, path, source);
        let recovered_analysis = default_analysis(&recovered);
        for target in [CapabilityTarget::JavaScript, CapabilityTarget::Declaration] {
            let scope = CapabilityScope::File(recovered.source.id);
            let CapabilityClaim::Nonclaimed(reasons) = recovered_analysis.claim(target, scope)
            else {
                panic!("{path}: recovered shift must withhold {target:?}")
            };
            assert!(reasons.into_iter().any(|reason| {
                reason.target == target
                    && reason.scope == scope
                    && reason.reason == NonclaimReason::Syntax(gap)
                    && reason.deletion == DeletionCondition::SyntaxOwner(gap)
            }));
        }
    }
}

#[test]
fn commonjs_namespace_import_reexport_has_one_typed_javascript_boundary() {
    for (path, module) in [
        ("src/constants.ts", "commonjs"),
        ("src/constants.cts", "nodenext"),
    ] {
        let file = program_file(
            0,
            path,
            concat!(
                "import * as cedar from '../dep';\n",
                "function wrapper(): void { const cedar = 1; }\n",
                "export { cedar as birch };\n",
            ),
        );
        let export = file.syntax.statements.last().expect("export statement");
        let analysis = CapabilityAnalysis::derive(
            std::slice::from_ref(&file),
            &CompilerOptions {
                module: module.to_string(),
                ..CompilerOptions::default()
            },
            CapabilityContext::default(),
        );
        let CapabilityClaim::Nonclaimed(reasons) = analysis.claim(
            CapabilityTarget::JavaScript,
            CapabilityScope::File(file.source.id),
        ) else {
            panic!("{path}: CommonJS namespace re-export must not publish JavaScript")
        };
        assert_eq!(
            reasons.copied().collect::<Vec<_>>(),
            [CapabilityNonclaim {
                target: CapabilityTarget::JavaScript,
                scope: CapabilityScope::node(file.source.id, export.id),
                reason: NonclaimReason::Syntax(SyntaxGap::CommonJsNamespaceImportReexport),
                deletion: DeletionCondition::SyntaxOwner(
                    SyntaxGap::CommonJsNamespaceImportReexport,
                ),
            }]
        );
        for target in [
            CapabilityTarget::SemanticCheck,
            CapabilityTarget::SemanticDiagnostics,
            CapabilityTarget::Declaration,
        ] {
            assert!(
                analysis
                    .claim(target, CapabilityScope::File(file.source.id))
                    .is_claimed(),
                "{path}: {target:?}",
            );
        }
    }
}

#[test]
fn commonjs_namespace_import_reexport_controls_stay_claimed() {
    for (path, module, source) in [
        (
            "esm.ts",
            "esnext",
            "import * as cedar from './dep'; export { cedar as birch };",
        ),
        (
            "named.ts",
            "commonjs",
            "import { value as cedar } from './dep'; export { cedar as birch };",
        ),
        (
            "default.ts",
            "commonjs",
            "import cedar from './dep'; export { cedar as birch };",
        ),
        (
            "type-import.ts",
            "commonjs",
            "import type * as cedar from './dep'; export { type cedar as birch };",
        ),
        (
            "type-export.ts",
            "commonjs",
            "import * as cedar from './dep'; export type { cedar as birch };",
        ),
        (
            "remote.ts",
            "commonjs",
            "import * as cedar from './dep'; export { cedar as birch } from './other';",
        ),
        (
            "export-all.ts",
            "commonjs",
            "export * as cedar from './dep';",
        ),
        (
            "different-local.ts",
            "commonjs",
            "import * as cedar from './dep'; const birch = cedar; export { birch };",
        ),
        (
            "redeclared.ts",
            "commonjs",
            "import * as cedar from './dep'; const cedar = 1; export { cedar };",
        ),
        (
            "module.mts",
            "nodenext",
            "import * as cedar from './dep'; export { cedar as birch };",
        ),
    ] {
        let file = program_file(0, path, source);
        let analysis = CapabilityAnalysis::derive(
            std::slice::from_ref(&file),
            &CompilerOptions {
                module: module.to_string(),
                ..CompilerOptions::default()
            },
            CapabilityContext::default(),
        );
        let claim = analysis.claim(
            CapabilityTarget::JavaScript,
            CapabilityScope::File(file.source.id),
        );
        assert!(claim.is_claimed(), "{path}: {claim:?}");
    }
}

#[test]
fn commonjs_namespace_import_reexport_withholds_only_its_file_javascript() {
    let output = Compiler::new().compile(
        vec![
            SourceInput::new(
                "dep.d.ts",
                Arc::<str>::from("export declare const value: number;"),
            ),
            SourceInput::new(
                "src/affected.ts",
                Arc::<str>::from("import * as cedar from '../dep'; export { cedar as birch };"),
            ),
            SourceInput::new(
                "stable.ts",
                Arc::<str>::from("export const stable: number = 1;"),
            ),
        ],
        &CompilerOptions {
            module: "commonjs".to_string(),
            target: "es2015".to_string(),
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
        ["stable.js"]
    );
}
