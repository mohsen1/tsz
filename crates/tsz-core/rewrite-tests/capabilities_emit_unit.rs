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
                && reason.deletion_condition()
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
            .parser_recovery_facts
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

#[path = "capabilities_emit_parts/declaration_products.rs"]
mod declaration_products;

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
                && reason.deletion_condition()
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
                    && reason.deletion_condition() == DeletionCondition::SyntaxOwner(gap)
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

#[test]
fn async_functions_keep_declaration_emit_and_defer_only_downlevel_javascript() {
    let source = concat!(
        "export async function cedar():Promise<void>{}\n",
        "export class Grove{",
        "async publicBranch():Promise<void>{}",
        "protected async guardedBranch():Promise<void>{}",
        "static async sharedBranch():Promise<void>{}",
        "private async hiddenBranch():Promise<void>{}",
        "}\n",
        "export function make():any{",
        "class Nested{async nestedBranch():Promise<void>{}}",
        "return Nested;",
        "}\n",
    );
    let file = program_file(0, "async-functions.ts", source);
    let mut async_owners = Vec::new();
    for_each_statement_in(
        &file.syntax.statements,
        &mut |statement| match &statement.kind {
            StatementKind::Function(function) if function.is_async && function.has_body => {
                async_owners.push(statement.id);
            }
            StatementKind::Class(class) => {
                async_owners.extend(class.members.iter().filter_map(|member| {
                    (member.modifiers.async_member
                        && matches!(member.kind, ClassMemberKind::Method { has_body: true, .. }))
                    .then_some(member.id)
                }))
            }
            _ => {}
        },
    );
    assert_eq!(async_owners.len(), 6);
    let options = CompilerOptions {
        declaration: true,
        module: "esnext".to_string(),
        target: "es2015".to_string(),
        ..CompilerOptions::default()
    };
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &options,
        CapabilityContext::default(),
    );
    for owner in &async_owners {
        let node = CapabilityScope::node(file.source.id, *owner);
        let CapabilityClaim::Nonclaimed(reasons) =
            analysis.claim(CapabilityTarget::JavaScript, node)
        else {
            panic!("ES2015 needs an async-function JavaScript transform")
        };
        assert_eq!(
            reasons.copied().collect::<Vec<_>>(),
            [CapabilityNonclaim {
                target: CapabilityTarget::JavaScript,
                scope: node,
                reason: NonclaimReason::Syntax(SyntaxGap::AsyncFunctionTransform),
            }],
        );
        assert!(
            analysis
                .claim(CapabilityTarget::Declaration, node)
                .is_claimed(),
            "authored signatures remain declaration-owned",
        );
    }

    for no_check in [false, true] {
        let output = Compiler::new().compile(
            vec![
                SourceInput::new("async-functions.ts", Arc::<str>::from(source)),
                SourceInput::new(
                    "stable.ts",
                    Arc::<str>::from("export const stable:number=1;"),
                ),
            ],
            &CompilerOptions {
                no_check,
                ..options.clone()
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
            ["async-functions.d.ts", "stable.d.ts", "stable.js"],
        );
        assert_eq!(
            output
                .emitted_files
                .iter()
                .find(|file| file.path.as_path() == std::path::Path::new("async-functions.d.ts"))
                .expect("affected declaration output")
                .text,
            concat!(
                "export declare function cedar(): Promise<void>;\n",
                "export declare class Grove {\n",
                "    publicBranch(): Promise<void>;\n",
                "    protected guardedBranch(): Promise<void>;\n",
                "    static sharedBranch(): Promise<void>;\n",
                "    private hiddenBranch;\n",
                "}\n",
                "export declare function make(): any;\n",
            ),
        );
    }

    let preserved = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &CompilerOptions {
            target: "es2017".to_string(),
            ..options
        },
        CapabilityContext::default(),
    );
    for owner in async_owners {
        assert!(
            preserved
                .claim(
                    CapabilityTarget::JavaScript,
                    CapabilityScope::node(file.source.id, owner),
                )
                .is_claimed(),
            "ES2017 preserves async function syntax",
        );
    }
}

#[test]
fn accessor_pair_modifier_checks_follow_binder_groups_without_blocking_authored_emit() {
    let source = concat!(
        "export class InvalidPairs{",
        "protected get value():number{return 1}",
        "public set value(next:number){}",
        "protected static get '\\x65scaped'():number{return 1}",
        "public static set escaped(next:number){}",
        "}",
        "export class CompatiblePairs{",
        "public get guarded():number{return 1}",
        "protected set guarded(next:number){}",
        "public get hiddenWrite():number{return 1}",
        "private set hiddenWrite(next:number){}",
        "private get hidden():number{return 1}",
        "private set hidden(next:number){}",
        "}",
    );
    let file = program_file(0, "accessor-modifiers.ts", source);
    let classes = file
        .syntax
        .statements
        .iter()
        .filter_map(|statement| match &statement.kind {
            StatementKind::Class(class) => Some(class),
            _ => None,
        })
        .collect::<Vec<_>>();
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &CompilerOptions {
            declaration: true,
            module: "esnext".to_string(),
            target: "es2022".to_string(),
            ..CompilerOptions::default()
        },
        CapabilityContext::default(),
    );
    for member in &classes[0].members {
        let scope = CapabilityScope::node(file.source.id, member.id);
        for target in [
            CapabilityTarget::SemanticCheck,
            CapabilityTarget::RequiredType,
            CapabilityTarget::SemanticDiagnostics,
        ] {
            assert!(
                matches!(
                    analysis.claim(target, scope),
                    CapabilityClaim::Nonclaimed(_)
                ),
                "incompatible accessor-pair modifiers need one bound-symbol check",
            );
        }
        let declaration = analysis.claim(CapabilityTarget::Declaration, scope);
        assert!(
            declaration.is_claimed(),
            "fully authored accessor types remain printable: {declaration:?}",
        );
    }
    for member in &classes[1].members {
        let scope = CapabilityScope::node(file.source.id, member.id);
        for target in [
            CapabilityTarget::SemanticCheck,
            CapabilityTarget::RequiredType,
            CapabilityTarget::SemanticDiagnostics,
        ] {
            assert!(
                analysis.claim(target, scope).is_claimed(),
                "bounded compatible accessor bodies are checker-owned",
            );
        }
        assert!(
            analysis
                .claim(CapabilityTarget::Declaration, scope)
                .is_claimed(),
            "compatible authored accessor signatures remain printable",
        );
    }

    let abstract_file = program_file(
        1,
        "abstract-accessor-modifiers.ts",
        concat!(
            "export abstract class AbstractMismatch{",
            "abstract get state():number;",
            "set state(next:number){}",
            "}",
        ),
    );
    let abstract_analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&abstract_file),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    let StatementKind::Class(abstract_class) = &abstract_file.syntax.statements[0].kind else {
        panic!("abstract accessor class")
    };
    for member in &abstract_class.members {
        let scope = CapabilityScope::node(abstract_file.source.id, member.id);
        for target in [
            CapabilityTarget::SemanticCheck,
            CapabilityTarget::RequiredType,
            CapabilityTarget::SemanticDiagnostics,
        ] {
            assert!(matches!(
                abstract_analysis.claim(target, scope),
                CapabilityClaim::Nonclaimed(_)
            ));
        }
    }

    let output = Compiler::new().compile(
        vec![
            SourceInput::new("accessor-modifiers.ts", Arc::<str>::from(source)),
            SourceInput::new(
                "stable.ts",
                Arc::<str>::from("export const stable:number=1;"),
            ),
        ],
        &CompilerOptions {
            declaration: true,
            module: "esnext".to_string(),
            target: "es2022".to_string(),
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
        [
            "accessor-modifiers.d.ts",
            "accessor-modifiers.js",
            "stable.d.ts",
            "stable.js",
        ],
    );
}

#[test]
fn invalid_accessor_grammar_defers_semantics_but_keeps_authored_products() {
    let source = concat!(
        "export class InvalidClass{",
        "get value(seed:number):number{return seed}",
        "set value(left:number,right:number){}",
        "static get '\\x6bey'(seed:number):number{return seed}",
        "static set key(left:number,right:number){}",
        "}",
        "export interface InvalidInterface{",
        "get item(seed:number):number;",
        "set item(left:number,right:number);",
        "}",
        "export type InvalidNested={inner:{",
        "get amount(seed:number):number;",
        "set amount(left:number,right:number);",
        "}};",
        "export class ValidAccessors{",
        "get current():number{return 1}",
        "set current(next:number){}",
        "private get hidden():number{return 1}",
        "private set hidden(next:number){}",
        "}",
    );
    let file = program_file(0, "accessor-grammar.ts", source);
    let options = CompilerOptions {
        declaration: true,
        module: "esnext".to_string(),
        target: "es2022".to_string(),
        ..CompilerOptions::default()
    };
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &options,
        CapabilityContext::default(),
    );
    let invalid_class = match &file.syntax.statements[0].kind {
        StatementKind::Class(class) => class,
        _ => panic!("invalid accessor class"),
    };
    for owner in invalid_class.members.iter().map(|member| member.id).chain(
        file.syntax.statements[1..3]
            .iter()
            .map(|statement| statement.id),
    ) {
        let scope = CapabilityScope::node(file.source.id, owner);
        for target in [
            CapabilityTarget::SemanticCheck,
            CapabilityTarget::RequiredType,
            CapabilityTarget::SemanticDiagnostics,
        ] {
            assert!(matches!(
                analysis.claim(target, scope),
                CapabilityClaim::Nonclaimed(_)
            ));
        }
        for target in [CapabilityTarget::JavaScript, CapabilityTarget::Declaration] {
            assert!(
                analysis.claim(target, scope).is_claimed(),
                "authored invalid-accessor products remain syntax-owned",
            );
        }
    }
    let valid_class = match &file.syntax.statements[3].kind {
        StatementKind::Class(class) => class,
        _ => panic!("valid accessor class"),
    };
    for member in &valid_class.members {
        let scope = CapabilityScope::node(file.source.id, member.id);
        for target in [
            CapabilityTarget::SemanticCheck,
            CapabilityTarget::RequiredType,
            CapabilityTarget::SemanticDiagnostics,
        ] {
            assert!(analysis.claim(target, scope).is_claimed());
        }
        assert!(
            analysis
                .claim(CapabilityTarget::Declaration, scope)
                .is_claimed()
        );
    }

    let output = Compiler::new().compile(
        vec![
            SourceInput::new("accessor-grammar.ts", Arc::<str>::from(source)),
            SourceInput::new(
                "stable.ts",
                Arc::<str>::from("export const stable:number=1;"),
            ),
        ],
        &options,
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
        [
            "accessor-grammar.d.ts",
            "accessor-grammar.js",
            "stable.d.ts",
            "stable.js",
        ],
    );
}

#[test]
fn type_member_accessor_pairs_defer_declaration_emit_until_pair_summaries_exist() {
    let source = concat!(
        "export interface Direct<T>{",
        "get item():T;",
        "set item(next);",
        "}",
        "export type Nested<T>={outer:{",
        "set amount(next:T);",
        "get amount();",
        "}};",
        "export class Header<T extends {get key():number;set key(next);}>{}",
    );
    let file = program_file(0, "accessor-types.ts", source);
    let options = CompilerOptions {
        declaration: true,
        module: "esnext".to_string(),
        target: "es2022".to_string(),
        ..CompilerOptions::default()
    };
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &options,
        CapabilityContext::default(),
    );
    for statement in &file.syntax.statements {
        let scope = CapabilityScope::node(file.source.id, statement.id);
        for target in [
            CapabilityTarget::SemanticCheck,
            CapabilityTarget::RequiredType,
            CapabilityTarget::SemanticDiagnostics,
            CapabilityTarget::Declaration,
        ] {
            let CapabilityClaim::Nonclaimed(reasons) = analysis.claim(target, scope) else {
                panic!("type-member accessors need a checked pair summary")
            };
            assert_eq!(
                reasons.copied().collect::<Vec<_>>(),
                [CapabilityNonclaim {
                    target,
                    scope,
                    reason: NonclaimReason::Semantic(SemanticGap::DeclarationAccessorSummary),
                }],
            );
        }
    }

    for no_check in [false, true] {
        let output = Compiler::new().compile(
            vec![
                SourceInput::new("accessor-types.ts", Arc::<str>::from(source)),
                SourceInput::new(
                    "stable.ts",
                    Arc::<str>::from("export const stable:number=1;"),
                ),
            ],
            &CompilerOptions {
                no_check,
                ..options.clone()
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
            ["accessor-types.js", "stable.d.ts", "stable.js"],
        );
    }

    let exact = program_file(
        0,
        "exact-accessors.ts",
        concat!(
            "export interface Exact<T>{get value():T;set value(next:T);}",
            "export type Concrete={set amount(next:number);get amount():number;};",
        ),
    );
    assert!(
        CapabilityAnalysis::derive(
            std::slice::from_ref(&exact),
            &options,
            CapabilityContext::default(),
        )
        .claim(
            CapabilityTarget::Declaration,
            CapabilityScope::File(exact.source.id),
        )
        .is_claimed(),
        "fully authored accessor types need no summary",
    );
}

#[test]
fn accessor_summaries_follow_only_published_declaration_surfaces() {
    let hidden = program_file(
        0,
        "hidden-accessors.ts",
        concat!(
            "export function stable():number{",
            "class Local{get value():number{return 1}set value(next){}}",
            "const cast=1 as unknown as {get item():number;set item(next);};",
            "return 1;",
            "}",
            "export class Hidden{",
            "private field:{get item():number;set item(next);};",
            "#opaque:{get item():number;set item(next);};",
            "private method(value:{get item():number;set item(next);}):void{}",
            "private constructor(value:{get item():number;set item(next);}){}",
            "}",
        ),
    );
    let options = CompilerOptions {
        declaration: true,
        module: "esnext".to_string(),
        target: "es2022".to_string(),
        strict_property_initialization: Some(false),
        ..CompilerOptions::default()
    };
    let hidden_analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&hidden),
        &options,
        CapabilityContext::default(),
    );
    if let CapabilityClaim::Nonclaimed(reasons) = hidden_analysis.claim(
        CapabilityTarget::Declaration,
        CapabilityScope::File(hidden.source.id),
    ) {
        let reasons = reasons.copied().collect::<Vec<_>>();
        assert!(
            reasons.iter().all(|reason| {
                reason.reason != NonclaimReason::Semantic(SemanticGap::DeclarationAccessorSummary)
            }),
            "local and erased private types are not published declaration summaries: {reasons:?}",
        );
    }
    assert!(matches!(
        hidden_analysis.claim(
            CapabilityTarget::SemanticDiagnostics,
            CapabilityScope::File(hidden.source.id),
        ),
        CapabilityClaim::Nonclaimed(_)
    ));
    for no_check in [false, true] {
        let output = Compiler::new().compile(
            vec![SourceInput::new(
                "hidden-accessors.ts",
                Arc::<str>::from(hidden.source.text()),
            )],
            &CompilerOptions {
                no_check,
                ..options.clone()
            },
        );
        assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            if no_check {
                SemanticCompletion::Complete
            } else {
                SemanticCompletion::Deferred
            },
        );
        assert_eq!(
            output.exit_status,
            if no_check {
                CompileExitStatus::Success
            } else {
                CompileExitStatus::SemanticIncomplete
            },
        );
        assert_eq!(
            output
                .emitted_files
                .iter()
                .map(|file| file.path.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["hidden-accessors.d.ts", "hidden-accessors.js"],
        );
    }

    let published = program_file(
        0,
        "published-accessors.ts",
        concat!(
            "export class Published{private constructor(",
            "public carried:{get value():number;set value(next);}",
            "){} }",
        ),
    );
    let published_analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&published),
        &options,
        CapabilityContext::default(),
    );
    let published_constructor = match &published.syntax.statements[0].kind {
        StatementKind::Class(class) => class.members[0].id,
        _ => panic!("published class"),
    };
    for target in [
        CapabilityTarget::SemanticDiagnostics,
        CapabilityTarget::Declaration,
    ] {
        assert!(
            matches!(
                published_analysis.claim(
                    target,
                    CapabilityScope::node(published.source.id, published_constructor),
                ),
                CapabilityClaim::Nonclaimed(_)
            ),
            "published parameter-property types need the accessor summary",
        );
    }
}

#[test]
fn accessor_summaries_follow_inferred_function_results_but_not_local_temporaries() {
    let file = program_file(
        0,
        "accessor-results.ts",
        concat!(
            "export const expression=()=>1 as unknown as {get x():number;set x(next);};",
            "export const block=()=>{return 1 as unknown as {get x():number;set x(next);};};",
            "export const functionValue=function(){",
            "return 1 as unknown as {get x():number;set x(next);};};",
            "export class Carrier{field=()=>{",
            "return 1 as unknown as {get x():number;set x(next);};};}",
            "export const stable=()=>{",
            "const local=1 as unknown as {get x():number;set x(next);};return 1;};",
            "export const typed=():number=>{",
            "const local=1 as unknown as {get x():number;set x(next);};return 1;};",
            "export const defaulted=(value:number=",
            "1 as unknown as {get x():number;set x(next);})=>value;",
        ),
    );
    let analysis = default_analysis(&file);
    let mut affected = file.syntax.statements[..3]
        .iter()
        .map(|statement| statement.id)
        .collect::<Vec<_>>();
    let class_member = match &file.syntax.statements[3].kind {
        StatementKind::Class(class) => class.members[0].id,
        _ => panic!("class result host"),
    };
    affected.push(class_member);
    for (index, owner) in affected.into_iter().enumerate() {
        let scope = CapabilityScope::node(file.source.id, owner);
        let CapabilityClaim::Nonclaimed(reasons) =
            analysis.claim(CapabilityTarget::Declaration, scope)
        else {
            panic!("published function result {index} needs an accessor summary")
        };
        assert!(reasons.into_iter().any(|reason| {
            reason.scope == scope
                && reason.reason
                    == NonclaimReason::Semantic(SemanticGap::DeclarationAccessorSummary)
        }));
    }

    for (index, message) in [
        (
            4,
            "a local-only assertion does not affect the inferred result",
        ),
        (5, "an authored return type cuts the body-result dependency"),
        (6, "an authored parameter type cuts the default dependency"),
    ] {
        let scope = CapabilityScope::node(file.source.id, file.syntax.statements[index].id);
        assert!(
            analysis
                .claim(CapabilityTarget::Declaration, scope)
                .is_claimed(),
            "{message}",
        );
        assert!(matches!(
            analysis.claim(CapabilityTarget::SemanticDiagnostics, scope),
            CapabilityClaim::Nonclaimed(_)
        ));
    }

    let output = Compiler::new().compile(
        vec![
            SourceInput::new("accessor-results.ts", Arc::<str>::from(file.source.text())),
            SourceInput::new(
                "stable.ts",
                Arc::<str>::from("export const stable:number=1;"),
            ),
        ],
        &CompilerOptions {
            declaration: true,
            module: "esnext".to_string(),
            target: "es2022".to_string(),
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
        ["accessor-results.js", "stable.d.ts", "stable.js"],
    );
}

#[test]
fn private_method_groups_use_binder_identity_for_declaration_summaries() {
    let source = concat!(
        "export declare class Overloads<T>{",
        "private alpha(value:T):void;",
        "private alpha(value:string):void;",
        "private alpha(value:unknown):void;",
        "private static beta(value:T):void;",
        "private static beta(value:number):void;",
        "private renamed(value:T):void;",
        "private renamed(value:boolean):void;",
        "}",
    );
    let file = program_file(0, "private-overloads.ts", source);
    let options = CompilerOptions {
        declaration: true,
        module: "esnext".to_string(),
        target: "es2015".to_string(),
        ..CompilerOptions::default()
    };
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &options,
        CapabilityContext::default(),
    );
    let scope = CapabilityScope::File(file.source.id);
    let CapabilityClaim::Nonclaimed(reasons) = analysis.claim(CapabilityTarget::Declaration, scope)
    else {
        panic!("ambient private overloads need binder-owned symbol summaries")
    };
    assert_eq!(
        reasons.copied().collect::<Vec<_>>(),
        [CapabilityNonclaim {
            target: CapabilityTarget::Declaration,
            scope,
            reason: NonclaimReason::Syntax(SyntaxGap::DeclarationOverloadSummary),
        }],
    );

    for no_check in [false, true] {
        let output = Compiler::new().compile(
            vec![
                SourceInput::new("private-overloads.ts", Arc::<str>::from(source)),
                SourceInput::new(
                    "stable.ts",
                    Arc::<str>::from("export const stable:number=1;"),
                ),
            ],
            &CompilerOptions {
                no_check,
                ..options.clone()
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
            ["private-overloads.js", "stable.d.ts", "stable.js"],
        );
    }

    let single = program_file(
        0,
        "single-private.ts",
        "export declare class Single<T>{private renamed(value:T):void;}",
    );
    assert!(
        CapabilityAnalysis::derive(
            std::slice::from_ref(&single),
            &options,
            CapabilityContext::default(),
        )
        .claim(
            CapabilityTarget::Declaration,
            CapabilityScope::File(single.source.id),
        )
        .is_claimed(),
        "one private signature needs no overload grouping",
    );

    let duplicate_bodies = program_file(
        0,
        "duplicate-private.ts",
        concat!(
            "export class Duplicate{",
            "private renamed():void{} private renamed(value:number):void{}",
            "private static text():void{} private static 'text'(value:number):void{}",
            "private 1():void{} private '1'(value:number):void{}",
            "private escaped():void{} private '\\x65scaped'(value:number):void{}",
            "private 2():void{} private '\\u0032'(value:number):void{}",
            "}",
        ),
    );
    assert!(
        matches!(
            CapabilityAnalysis::derive(
                std::slice::from_ref(&duplicate_bodies),
                &options,
                CapabilityContext::default(),
            )
            .claim(
                CapabilityTarget::Declaration,
                CapabilityScope::File(duplicate_bodies.source.id),
            ),
            CapabilityClaim::Nonclaimed(_)
        ),
        "duplicate implementations still need one binder-symbol declaration marker",
    );
    let escaped_members = duplicate_bodies
        .syntax
        .statements
        .iter()
        .find_map(|statement| match &statement.kind {
            StatementKind::Class(class) => Some(&class.members),
            _ => None,
        })
        .expect("escaped private class members");
    for index in [6, 7, 8, 9] {
        assert_eq!(
            duplicate_bodies
                .bindings
                .class_member_group(escaped_members[index].id)
                .map(<[_]>::len),
            Some(2),
            "scanner-cooked string property keys share binder identity",
        );
    }

    let implementation_groups = program_file(
        0,
        "implementation-groups.ts",
        concat!(
            "export class Implementations{",
            "renamed():void{} 'renamed'(value:number):void{}",
            "protected static held():void{} protected static 'held'(value:number):void{}",
            "escaped():void{} '\\u{65}scaped'(value:number):void{}",
            "constructor(){} constructor(value:number){}",
            "}",
            "export class PrivateConstructors{",
            "private constructor(){} private constructor(value:number){}",
            "}",
        ),
    );
    assert!(matches!(
        CapabilityAnalysis::derive(
            std::slice::from_ref(&implementation_groups),
            &options,
            CapabilityContext::default(),
        )
        .claim(
            CapabilityTarget::Declaration,
            CapabilityScope::File(implementation_groups.source.id),
        ),
        CapabilityClaim::Nonclaimed(_)
    ));

    let signature_controls = program_file(
        0,
        "signature-controls.ts",
        concat!(
            "export declare class Signatures{",
            "renamed():void; 'renamed'(value:number):void;",
            "escaped():void; '\\x65scaped'(value:number):void;",
            "constructor(); constructor(value:number);",
            "get current():number; get 'current'():number;",
            "set changed(value:number); set 'changed'(value:number);",
            "property:number; 'property':number;",
            "}",
        ),
    );
    assert!(
        CapabilityAnalysis::derive(
            std::slice::from_ref(&signature_controls),
            &options,
            CapabilityContext::default(),
        )
        .claim(
            CapabilityTarget::Declaration,
            CapabilityScope::File(signature_controls.source.id),
        )
        .is_claimed(),
        "public signatures and accessor/property duplicates remain authored products",
    );

    let distinct_source = concat!(
        "export declare class Distinct<T>{",
        "private alpha(value:T):void; private beta(value:T):void;",
        "private same(value:T):void; private static same(value:T):void;",
        "}",
    );
    let distinct = program_file(0, "distinct-private.ts", distinct_source);
    assert!(
        CapabilityAnalysis::derive(
            std::slice::from_ref(&distinct),
            &options,
            CapabilityContext::default(),
        )
        .claim(
            CapabilityTarget::Declaration,
            CapabilityScope::File(distinct.source.id),
        )
        .is_claimed(),
        "distinct symbols and static/instance peers do not form overload groups",
    );
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "distinct-private.ts",
            Arc::<str>::from(distinct_source),
        )],
        &CompilerOptions {
            no_check: true,
            ..options
        },
    );
    assert_eq!(output.exit_status, CompileExitStatus::Success);
    assert_eq!(
        output
            .emitted_files
            .iter()
            .find(|file| file.path.as_path() == std::path::Path::new("distinct-private.d.ts"))
            .expect("distinct private declaration output")
            .text,
        concat!(
            "export declare class Distinct<T> {\n",
            "    private alpha;\n",
            "    private beta;\n",
            "    private same;\n",
            "    private static same;\n",
            "}\n",
        ),
    );
}

#[test]
fn private_identifiers_keep_declaration_emit_until_javascript_can_preserve_them() {
    let source = concat!(
        "export class Vault{",
        "#field:number=1;",
        "static #shared:number=2;",
        "#method():number{return 1}",
        "get #value():number{return 1}",
        "set #value(next:number){}",
        "async #later():Promise<void>{}",
        "}",
    );
    let file = program_file(0, "private-identifiers.ts", source);
    let members = file
        .syntax
        .statements
        .iter()
        .find_map(|statement| match &statement.kind {
            StatementKind::Class(class) => Some(&class.members),
            _ => None,
        })
        .expect("private class members");
    assert_eq!(members.len(), 6);
    assert!(
        members
            .iter()
            .all(|member| member.name_kind == PropertyNameKind::PrivateIdentifier)
    );
    let options = CompilerOptions {
        declaration: true,
        module: "esnext".to_string(),
        target: "es2021".to_string(),
        no_check: true,
        ..CompilerOptions::default()
    };
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &options,
        CapabilityContext::default(),
    );
    for member in members {
        let node = CapabilityScope::node(file.source.id, member.id);
        let CapabilityClaim::Nonclaimed(reasons) =
            analysis.claim(CapabilityTarget::JavaScript, node)
        else {
            panic!("ES2021 needs a private-identifier transform")
        };
        let mut expected = vec![CapabilityNonclaim {
            target: CapabilityTarget::JavaScript,
            scope: node,
            reason: NonclaimReason::Syntax(SyntaxGap::PrivateIdentifierTransform),
        }];
        if matches!(member.kind, ClassMemberKind::Property { .. }) {
            expected.insert(
                0,
                CapabilityNonclaim {
                    target: CapabilityTarget::JavaScript,
                    scope: node,
                    reason: NonclaimReason::Syntax(SyntaxGap::ClassFieldTransform),
                },
            );
        }
        assert_eq!(reasons.copied().collect::<Vec<_>>(), expected,);
        assert!(
            analysis
                .claim(CapabilityTarget::Declaration, node)
                .is_claimed()
        );
    }

    let output = Compiler::new().compile(
        vec![
            SourceInput::new("private-identifiers.ts", Arc::<str>::from(source)),
            SourceInput::new(
                "stable.ts",
                Arc::<str>::from("export const stable:number=1;"),
            ),
        ],
        &options,
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
        ["private-identifiers.d.ts", "stable.d.ts", "stable.js"],
    );
    assert_eq!(
        output
            .emitted_files
            .iter()
            .find(|file| {
                file.path.as_path() == std::path::Path::new("private-identifiers.d.ts")
            })
            .expect("private declaration output")
            .text,
        "export declare class Vault {\n    #private;\n}\n",
    );

    let preserved = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &CompilerOptions {
            target: "es2022".to_string(),
            ..options
        },
        CapabilityContext::default(),
    );
    for member in members {
        assert!(
            preserved
                .claim(
                    CapabilityTarget::JavaScript,
                    CapabilityScope::node(file.source.id, member.id),
                )
                .is_claimed(),
            "ES2022 preserves private identifier syntax",
        );
    }
}

#[test]
fn class_fields_keep_declaration_emit_until_javascript_can_preserve_them() {
    let source = concat!(
        "export class Fields{",
        "value:number=1;",
        "pending:number;",
        "static shared:number=2;",
        "protected guarded:number;",
        "private hidden:number=3;",
        "readonly fixed:number=4;",
        "}",
    );
    let file = program_file(0, "class-fields.ts", source);
    let members = file
        .syntax
        .statements
        .iter()
        .find_map(|statement| match &statement.kind {
            StatementKind::Class(class) => Some(&class.members),
            _ => None,
        })
        .expect("class fields");
    assert_eq!(members.len(), 6);
    let options = CompilerOptions {
        declaration: true,
        module: "esnext".to_string(),
        target: "es2021".to_string(),
        no_check: true,
        ..CompilerOptions::default()
    };
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &options,
        CapabilityContext::default(),
    );
    for member in members {
        let node = CapabilityScope::node(file.source.id, member.id);
        let CapabilityClaim::Nonclaimed(reasons) =
            analysis.claim(CapabilityTarget::JavaScript, node)
        else {
            panic!("ES2021 needs a class-field transform")
        };
        assert_eq!(
            reasons.copied().collect::<Vec<_>>(),
            [CapabilityNonclaim {
                target: CapabilityTarget::JavaScript,
                scope: node,
                reason: NonclaimReason::Syntax(SyntaxGap::ClassFieldTransform),
            }],
        );
        assert!(
            analysis
                .claim(CapabilityTarget::Declaration, node)
                .is_claimed()
        );
    }

    let output = Compiler::new().compile(
        vec![
            SourceInput::new("class-fields.ts", Arc::<str>::from(source)),
            SourceInput::new(
                "stable.ts",
                Arc::<str>::from("export const stable:number=1;"),
            ),
        ],
        &options,
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
        ["class-fields.d.ts", "stable.d.ts", "stable.js"],
    );
    assert_eq!(
        output
            .emitted_files
            .iter()
            .find(|file| file.path.as_path() == std::path::Path::new("class-fields.d.ts"))
            .expect("class-field declaration output")
            .text,
        concat!(
            "export declare class Fields {\n",
            "    value: number;\n",
            "    pending: number;\n",
            "    static shared: number;\n",
            "    protected guarded: number;\n",
            "    private hidden;\n",
            "    readonly fixed: number;\n",
            "}\n",
        ),
    );

    let preserved = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &CompilerOptions {
            target: "es2022".to_string(),
            ..options
        },
        CapabilityContext::default(),
    );
    for member in members {
        assert!(
            preserved
                .claim(
                    CapabilityTarget::JavaScript,
                    CapabilityScope::node(file.source.id, member.id),
                )
                .is_claimed(),
            "ES2022 preserves class fields",
        );
    }
}

#[test]
fn explicit_class_field_semantics_defer_only_unsupported_javascript_products() {
    let source = concat!(
        "export class Layout{",
        "field:number=1;",
        "pending:number;",
        "static shared:number=2;",
        "#secret:number=3;",
        "static #staticSecret:number=4;",
        "#method():void{}",
        "constructor(public value:number,private hidden:number){}",
        "}",
    );
    let file = program_file(0, "class-field-semantics.ts", source);
    let members = file
        .syntax
        .statements
        .iter()
        .find_map(|statement| match &statement.kind {
            StatementKind::Class(class) => Some(&class.members),
            _ => None,
        })
        .expect("class members");
    assert_eq!(members.len(), 7);

    for (target, use_define_for_class_fields) in
        [("es2022", false), ("esnext", false), ("es2021", true)]
    {
        let options = CompilerOptions {
            declaration: true,
            module: "esnext".to_string(),
            target: target.to_string(),
            use_define_for_class_fields: Some(use_define_for_class_fields),
            no_check: true,
            ..CompilerOptions::default()
        };
        let analysis = CapabilityAnalysis::derive(
            std::slice::from_ref(&file),
            &options,
            CapabilityContext::default(),
        );
        for member in [members[0].id, members[1].id, members[2].id, members[6].id] {
            assert!(
                matches!(
                    analysis.claim(
                        CapabilityTarget::JavaScript,
                        CapabilityScope::node(file.source.id, member),
                    ),
                    CapabilityClaim::Nonclaimed(_)
                ),
                "{target} with useDefineForClassFields={use_define_for_class_fields} needs a transform",
            );
        }
        for member in members {
            assert!(
                analysis
                    .claim(
                        CapabilityTarget::Declaration,
                        CapabilityScope::node(file.source.id, member.id),
                    )
                    .is_claimed(),
                "class-field runtime semantics do not affect declaration emit",
            );
        }
        if target != "es2021" {
            assert!(matches!(
                analysis.claim(
                    CapabilityTarget::JavaScript,
                    CapabilityScope::node(file.source.id, members[3].id),
                ),
                CapabilityClaim::Nonclaimed(_)
            ));
            for member in [members[4].id, members[5].id] {
                assert!(
                    analysis
                        .claim(
                            CapabilityTarget::JavaScript,
                            CapabilityScope::node(file.source.id, member),
                        )
                        .is_claimed(),
                    "native private names are unaffected by useDefineForClassFields=false",
                );
            }
        }
    }

    for use_define_for_class_fields in [None, Some(true)] {
        let analysis = CapabilityAnalysis::derive(
            std::slice::from_ref(&file),
            &CompilerOptions {
                target: "es2022".to_string(),
                use_define_for_class_fields,
                ..CompilerOptions::default()
            },
            CapabilityContext::default(),
        );
        for member in members {
            assert!(
                analysis
                    .claim(
                        CapabilityTarget::JavaScript,
                        CapabilityScope::node(file.source.id, member.id),
                    )
                    .is_claimed(),
                "ES2022 defaults to native define semantics",
            );
        }
    }

    let parameter_only = program_file(
        0,
        "parameter-property.ts",
        "export class ParameterOnly{constructor(public value:number){}}",
    );
    let constructor = match &parameter_only.syntax.statements[0].kind {
        StatementKind::Class(class) => class.members[0].id,
        _ => panic!("parameter-property class"),
    };
    for use_define_for_class_fields in [None, Some(false)] {
        assert!(
            CapabilityAnalysis::derive(
                std::slice::from_ref(&parameter_only),
                &CompilerOptions {
                    target: "es2021".to_string(),
                    use_define_for_class_fields,
                    ..CompilerOptions::default()
                },
                CapabilityContext::default(),
            )
            .claim(
                CapabilityTarget::JavaScript,
                CapabilityScope::node(parameter_only.source.id, constructor),
            )
            .is_claimed(),
            "pre-ES2022 defaults use the supported assignment parameter-property path",
        );
    }

    let output = Compiler::new().compile(
        vec![
            SourceInput::new("class-field-semantics.ts", Arc::<str>::from(source)),
            SourceInput::new(
                "stable.ts",
                Arc::<str>::from("export const stable:number=1;"),
            ),
        ],
        &CompilerOptions {
            declaration: true,
            module: "esnext".to_string(),
            target: "es2022".to_string(),
            use_define_for_class_fields: Some(false),
            no_check: true,
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
        ["class-field-semantics.d.ts", "stable.d.ts", "stable.js",],
    );
}

#[test]
fn private_class_member_types_are_erased_in_declaration_emit() {
    let output = Compiler::new().compile(
        vec![
            SourceInput::new(
                "private-shape.ts",
                Arc::<str>::from(concat!(
                    "export class PrivateShape<Row>{",
                    "#opaque:keyof Row;",
                    "private hidden?:keyof Row;",
                    "private readonly fixed:keyof Row;",
                    "private take(value:keyof Row):keyof Row{return value}",
                    "}",
                )),
            ),
            SourceInput::new(
                "private-construction.ts",
                Arc::<str>::from(concat!(
                    "export class PrivateConstruction<Row>{",
                    "private constructor(",
                    "private secret:keyof Row,",
                    "protected kept:keyof Row,",
                    "readonly shown:keyof Row",
                    "){}",
                    "}",
                )),
            ),
        ],
        &CompilerOptions {
            declaration: true,
            module: "esnext".to_string(),
            target: "esnext".to_string(),
            no_check: true,
            ..CompilerOptions::default()
        },
    );
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    let declaration_text = |path: &str| {
        output
            .emitted_files
            .iter()
            .find(|file| file.path.to_string_lossy() == path)
            .unwrap_or_else(|| panic!("missing declaration product {path}"))
            .text
            .as_str()
    };
    assert_eq!(
        declaration_text("private-shape.d.ts"),
        "export declare class PrivateShape<Row> {\n\
         \x20   #private;\n\
         \x20   private hidden?;\n\
         \x20   private readonly fixed;\n\
         \x20   private take;\n\
         }\n",
    );
    assert_eq!(
        declaration_text("private-construction.d.ts"),
        "export declare class PrivateConstruction<Row> {\n\
         \x20   private secret;\n\
         \x20   protected kept: keyof Row;\n\
         \x20   readonly shown: keyof Row;\n\
         \x20   private constructor();\n\
         }\n",
    );
}
