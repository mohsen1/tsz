use super::*;

use crate::program::{CompileExitStatus, Compiler, SemanticCompletion, SourceInput};
use crate::syntax::AccessorKind;

#[test]
fn accessor_pairs_defer_declaration_emit_when_either_published_type_needs_inference() {
    let source = concat!(
        "export declare class GetterFirst<T>{",
        "public get value():T;",
        "public set value(next);",
        "}",
        "export declare class SetterFirst{",
        "protected set amount(next:number);",
        "protected get amount();",
        "}",
        "export declare class Hidden<T>{",
        "private get secret():T;",
        "private set secret(next);",
        "}",
    );
    let file = program_file(0, "accessor-summary.ts", source);
    let classes = file
        .syntax
        .statements
        .iter()
        .filter_map(|statement| match &statement.kind {
            StatementKind::Class(class) => Some(class),
            _ => None,
        })
        .collect::<Vec<_>>();
    let affected = [classes[0].members[1].id, classes[1].members[1].id];
    let private_control = [classes[2].members[0].id, classes[2].members[1].id];
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
    for owner in affected {
        let node = CapabilityScope::node(file.source.id, owner);
        for target in [
            CapabilityTarget::SemanticDiagnostics,
            CapabilityTarget::Declaration,
        ] {
            let CapabilityClaim::Nonclaimed(reasons) = analysis.claim(target, node) else {
                panic!("accessor inference needs a checked declaration summary")
            };
            assert_eq!(
                reasons.copied().collect::<Vec<_>>(),
                [CapabilityNonclaim {
                    target,
                    scope: node,
                    reason: NonclaimReason::Semantic(SemanticGap::DeclarationAccessorSummary),
                    deletion: DeletionCondition::SemanticOwner(
                        SemanticGap::DeclarationAccessorSummary,
                    ),
                }],
            );
        }
    }
    for owner in private_control {
        assert!(
            analysis
                .claim(
                    CapabilityTarget::Declaration,
                    CapabilityScope::node(file.source.id, owner),
                )
                .is_claimed(),
            "private accessor types are erased",
        );
    }

    for no_check in [false, true] {
        let output = Compiler::new().compile(
            vec![
                SourceInput::new("accessor-summary.ts", Arc::<str>::from(source)),
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
            ["accessor-summary.js", "stable.d.ts", "stable.js"],
        );
    }
}

#[test]
fn explicit_this_accessors_report_the_oracle_diagnostic_without_parser_recovery_noise() {
    let source = concat!(
        "class Box { get read(this:Box):number{return 1} set write(this:Box,value:number){} }\n",
        "interface Shape { get read(this:Shape):number; set write(this:Shape,value:number); }\n",
        "type Alias = { get read(this:Alias):number; set write(this:Alias,value:number); };\n",
    );
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "accessor-this.ts",
            Arc::<str>::from(source),
        )],
        &CompilerOptions {
            no_emit: true,
            target: "es2022".to_string(),
            ..CompilerOptions::default()
        },
    );
    let diagnostics = output
        .diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.as_str(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(diagnostics.len(), 6, "{diagnostics:#?}");
    for (code, start, length, message) in diagnostics {
        assert_eq!(code, 2784);
        assert_eq!(length, 4);
        assert_eq!(
            &source[start as usize..start as usize + length as usize],
            "this"
        );
        assert_eq!(
            message,
            "'get' and 'set' accessors cannot declare 'this' parameters."
        );
    }
    let unchecked = Compiler::new().compile(
        vec![SourceInput::new(
            "accessor-this.ts",
            Arc::<str>::from(source),
        )],
        &CompilerOptions {
            no_check: true,
            no_emit: true,
            target: "es2022".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(
        unchecked.diagnostics.is_empty(),
        "{:#?}",
        unchecked.diagnostics
    );
    assert_eq!(unchecked.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(unchecked.exit_status, CompileExitStatus::Success);
}

#[test]
fn generic_accessors_report_ts1094_at_the_accessor_name_in_every_modeled_host() {
    let source = concat!(
        "class Box { get read<T>():number{return 1} set write<T>(value:number){} }\n",
        "interface Shape { get read<T>():number; set write<T>(value:number); }\n",
        "type Alias = { get read<T>():number; set write<T>(value:number); };\n",
    );
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "generic-accessors.ts",
            Arc::<str>::from(source),
        )],
        &CompilerOptions {
            no_emit: true,
            target: "es2022".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(output.diagnostics.len(), 6, "{:#?}", output.diagnostics);
    for diagnostic in &output.diagnostics {
        assert_eq!(diagnostic.code, 1094);
        assert_eq!(
            diagnostic.message_text,
            "An accessor cannot have type parameters."
        );
        assert!(matches!(diagnostic.length, 4 | 5));
        assert!(matches!(
            &source
                [diagnostic.start as usize..diagnostic.start as usize + diagnostic.length as usize],
            "read" | "write"
        ));
    }
    let unchecked = Compiler::new().compile(
        vec![SourceInput::new(
            "generic-accessors.ts",
            Arc::<str>::from(source),
        )],
        &CompilerOptions {
            no_check: true,
            no_emit: true,
            target: "es2022".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(
        unchecked.diagnostics.is_empty(),
        "{:#?}",
        unchecked.diagnostics
    );
    assert_eq!(unchecked.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(unchecked.exit_status, CompileExitStatus::Success);
}

#[test]
fn no_check_emits_generic_and_explicit_this_accessor_products_exactly() {
    let source = concat!(
        "export class GenericAccessors {\n",
        "    get read<T>(): number { return 1; }\n",
        "    set write<T>(value: number) {}\n",
        "}\n",
        "export class ThisAccessors {\n",
        "    get read(this: ThisAccessors): number { return 1; }\n",
        "    set write(this: ThisAccessors, value: number) {}\n",
        "}\n",
    );
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "unchecked-accessors.ts",
            Arc::<str>::from(source),
        )],
        &CompilerOptions {
            declaration: true,
            no_check: true,
            module: "esnext".to_string(),
            target: "es2022".to_string(),
            ..CompilerOptions::default()
        },
    );

    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(output.exit_status, CompileExitStatus::Success);
    assert_eq!(
        output
            .emitted_files
            .iter()
            .map(|file| file.path.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        ["unchecked-accessors.d.ts", "unchecked-accessors.js"],
    );
    let product = |path: &str| {
        output
            .emitted_files
            .iter()
            .find(|file| file.path.to_string_lossy() == path)
            .unwrap_or_else(|| panic!("missing {path}"))
            .text
            .as_str()
    };
    assert_eq!(
        product("unchecked-accessors.js"),
        concat!(
            "export class GenericAccessors {\n",
            "    get read() {\n",
            "        return 1;\n",
            "    }\n",
            "    set write(value) { }\n",
            "}\n",
            "export class ThisAccessors {\n",
            "    get read() {\n",
            "        return 1;\n",
            "    }\n",
            "    set write(value) { }\n",
            "}\n",
        ),
    );
    assert_eq!(
        product("unchecked-accessors.d.ts"),
        concat!(
            "export declare class GenericAccessors {\n",
            "    get read(): number;\n",
            "    set write(value: number);\n",
            "}\n",
            "export declare class ThisAccessors {\n",
            "    get read(this: ThisAccessors): number;\n",
            "    set write(this: ThisAccessors, value: number);\n",
            "}\n",
        ),
    );
}

#[test]
fn bounded_accessor_bodies_are_owned_without_promoting_unmodeled_neighbors() {
    let source = concat!(
        "class Cases{",
        "get empty():number{}",
        "get partial():number{if(Math.random())return 1}",
        "get wrong():number{return 'bad'}",
        "get valid():number{return 1}",
        "set returned(value:number){return value}",
        "set stable(value:number){}",
        "}",
    );
    let file = program_file(0, "accessor-bodies.ts", source);
    let StatementKind::Class(class) = &file.syntax.statements[0].kind else {
        panic!("class expected")
    };
    let analysis = default_analysis(&file);
    for member in [&class.members[2], &class.members[3], &class.members[5]] {
        let node = CapabilityScope::node(file.source.id, member.id);
        for target in [
            CapabilityTarget::SemanticCheck,
            CapabilityTarget::RequiredType,
            CapabilityTarget::SemanticDiagnostics,
        ] {
            assert!(
                analysis.claim(target, node).is_claimed(),
                "bounded accessor {target:?} should be owned",
            );
        }
    }
    for member in [&class.members[0], &class.members[1], &class.members[4]] {
        let node = CapabilityScope::node(file.source.id, member.id);
        for target in [
            CapabilityTarget::SemanticCheck,
            CapabilityTarget::RequiredType,
            CapabilityTarget::SemanticDiagnostics,
        ] {
            let CapabilityClaim::Nonclaimed(reasons) = analysis.claim(target, node) else {
                panic!("unmodeled accessor semantics must remain deferred")
            };
            assert_eq!(
                reasons.copied().collect::<Vec<_>>(),
                [CapabilityNonclaim {
                    target,
                    scope: node,
                    reason: NonclaimReason::Semantic(SemanticGap::DeclarationAccessorSummary),
                    deletion: DeletionCondition::SemanticOwner(
                        SemanticGap::DeclarationAccessorSummary,
                    ),
                }],
            );
        }
    }
    for member in &class.members {
        let node = CapabilityScope::node(file.source.id, member.id);
        for target in [CapabilityTarget::JavaScript, CapabilityTarget::Declaration] {
            assert!(analysis.claim(target, node).is_claimed(), "{target:?}");
        }
    }

    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "accessor-bodies.ts",
            Arc::<str>::from(source),
        )],
        &CompilerOptions {
            no_emit: true,
            ..CompilerOptions::default()
        },
    );
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [2322],
        "a claimed sibling must still be checked: {:#?}",
        output.diagnostics,
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
}

#[test]
fn bounded_accessor_matrix_emits_exact_javascript() {
    let source = concat!(
        "export class Result {}\n",
        "export class Renamed {\n",
        "    get annotated(): Result { var seed = 1; return null; }\n",
        "    get inferred() { var sprout = 1; return null; }\n",
        "    get paired(): number { return 1; }\n",
        "    set paired(next: string) {}\n",
        "    set alpha(next) {}\n",
        "    set \"beta\"(next) {}\n",
        "    set 0(next) {}\n",
        "    static get current() { var receiver = this; return 1; }\n",
        "}\n",
    );
    let options = CompilerOptions {
        strict: false,
        module: "esnext".to_string(),
        target: "es2015".to_string(),
        ..CompilerOptions::default()
    };
    let file = program_file(0, "bounded-accessors.ts", source);
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &options,
        CapabilityContext::default(),
    );
    for statement in &file.syntax.statements {
        let StatementKind::Class(class) = &statement.kind else {
            continue;
        };
        for member in &class.members {
            let scope = CapabilityScope::node(file.source.id, member.id);
            assert!(
                analysis
                    .claim(CapabilityTarget::SemanticDiagnostics, scope)
                    .is_claimed(),
                "{} is outside the bounded accessor owner: {:?}",
                member.name,
                analysis.claim(CapabilityTarget::SemanticDiagnostics, scope),
            );
        }
    }
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "bounded-accessors.ts",
            Arc::<str>::from(source),
        )],
        &options,
    );
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    assert_eq!(
        output.semantic_completion,
        SemanticCompletion::Complete,
        "file checking: {:?}",
        output.check_file_completions,
    );
    assert_eq!(output.exit_status, CompileExitStatus::Success);
    assert_eq!(output.emitted_files.len(), 1);
    assert_eq!(
        output.emitted_files[0].path.to_string_lossy(),
        "bounded-accessors.js"
    );
    assert_eq!(
        output.emitted_files[0].text,
        concat!(
            "export class Result {\n",
            "}\n",
            "export class Renamed {\n",
            "    get annotated() {\n",
            "        var seed = 1;\n",
            "        return null;\n",
            "    }\n",
            "    get inferred() {\n",
            "        var sprout = 1;\n",
            "        return null;\n",
            "    }\n",
            "    get paired() {\n",
            "        return 1;\n",
            "    }\n",
            "    set paired(next) { }\n",
            "    set alpha(next) { }\n",
            "    set \"beta\"(next) { }\n",
            "    set 0(next) { }\n",
            "    static get current() {\n",
            "        var receiver = this;\n",
            "        return 1;\n",
            "    }\n",
            "}\n",
        ),
    );
}

#[test]
fn explicit_accessor_pair_emits_exact_declarations_with_distinct_types() {
    let source = concat!(
        "export class Pair {\n",
        "    get renamed(): number { return 1; }\n",
        "    set renamed(payload: string) {}\n",
        "}\n",
    );
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "accessor-pair.ts",
            Arc::<str>::from(source),
        )],
        &CompilerOptions {
            declaration: true,
            module: "esnext".to_string(),
            target: "es2015".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(output.exit_status, CompileExitStatus::Success);
    let product = |path: &str| {
        output
            .emitted_files
            .iter()
            .find(|file| file.path.to_string_lossy() == path)
            .unwrap_or_else(|| panic!("missing {path}"))
            .text
            .as_str()
    };
    assert_eq!(
        product("accessor-pair.d.ts"),
        concat!(
            "export declare class Pair {\n",
            "    get renamed(): number;\n",
            "    set renamed(payload: string);\n",
            "}\n",
        ),
    );
    assert_eq!(
        product("accessor-pair.js"),
        concat!(
            "export class Pair {\n",
            "    get renamed() {\n",
            "        return 1;\n",
            "    }\n",
            "    set renamed(payload) { }\n",
            "}\n",
        ),
    );
}

#[test]
fn nested_static_and_private_accessors_keep_capability_locality() {
    let source = concat!(
        "function wrapper() {",
        "class NestedBirch {",
        "private get leaf():number{return 1}",
        "static get crown():number{return 'bad'}",
        "get branch():number{if(true)return 1}",
        "set returned(value:number){return value}",
        "set implicit(value){}",
        "}",
        "}",
    );
    let file = program_file(0, "nested-accessors.ts", source);
    let analysis = default_analysis(&file);
    let mut members = BTreeMap::new();
    crate::syntax::for_each_statement_in(&file.syntax.statements, &mut |statement| {
        if let StatementKind::Class(class) = &statement.kind {
            members.extend(
                class
                    .members
                    .iter()
                    .map(|member| (member.name.as_str(), member)),
            );
        }
    });
    for name in ["leaf", "crown"] {
        let member = members[name];
        let scope = CapabilityScope::node(file.source.id, member.id);
        for target in [
            CapabilityTarget::SemanticCheck,
            CapabilityTarget::RequiredType,
            CapabilityTarget::SemanticDiagnostics,
        ] {
            assert!(
                analysis.claim(target, scope).is_claimed(),
                "{name} {target:?} should be owned",
            );
        }
    }
    for name in ["branch", "returned", "implicit"] {
        let member = members[name];
        let scope = CapabilityScope::node(file.source.id, member.id);
        for target in [
            CapabilityTarget::SemanticCheck,
            CapabilityTarget::RequiredType,
            CapabilityTarget::SemanticDiagnostics,
        ] {
            let CapabilityClaim::Nonclaimed(reasons) = analysis.claim(target, scope) else {
                panic!("{name} must remain outside the bounded accessor owner")
            };
            assert_eq!(
                reasons.copied().collect::<Vec<_>>(),
                [CapabilityNonclaim {
                    target,
                    scope,
                    reason: NonclaimReason::Semantic(SemanticGap::DeclarationAccessorSummary),
                    deletion: DeletionCondition::SemanticOwner(
                        SemanticGap::DeclarationAccessorSummary,
                    ),
                }],
            );
        }
    }

    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "nested-accessors.ts",
            Arc::<str>::from(source),
        )],
        &CompilerOptions {
            no_emit: true,
            ..CompilerOptions::default()
        },
    );
    let bad_return = source.find("return 'bad'").unwrap() as u32;
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
        [(
            2322,
            bad_return,
            6,
            "Type 'string' is not assignable to type 'number'.",
        )],
        "a claimed nested sibling must remain checkable: {:#?}",
        output.diagnostics,
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);

    let unchecked = Compiler::new().compile(
        vec![SourceInput::new(
            "nested-accessors.ts",
            Arc::<str>::from(source),
        )],
        &CompilerOptions {
            no_check: true,
            no_emit: true,
            ..CompilerOptions::default()
        },
    );
    assert!(
        unchecked.diagnostics.is_empty(),
        "{:#?}",
        unchecked.diagnostics
    );
    assert_eq!(unchecked.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(unchecked.exit_status, CompileExitStatus::Success);
}

#[test]
fn bounded_getter_return_reports_the_exact_oracle_relation_diagnostic() {
    let source = "class Matrix { get changed(): number { return \"wrong\"; } }";
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "wrong-accessor-return.ts",
            Arc::<str>::from(source),
        )],
        &CompilerOptions {
            no_emit: true,
            target: "es2015".to_string(),
            ..CompilerOptions::default()
        },
    );
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
        [(
            2322,
            source.find("return").unwrap() as u32,
            6,
            "Type 'string' is not assignable to type 'number'.",
        )],
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        output.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );
}

#[test]
fn quoted_constructor_names_use_constructor_identity_and_emit_spelling() {
    let source = concat!(
        "export class Quoted{'constructor'(value:number){}}\n",
        "export class Escaped{'\\x63onstructor'(value:number){}}\n",
    );
    let file = program_file(0, "quoted-constructor.ts", source);
    for statement in &file.syntax.statements {
        let StatementKind::Class(class) = &statement.kind else {
            panic!("class expected")
        };
        assert!(matches!(
            class.members[0].kind,
            ClassMemberKind::Constructor { .. }
        ));
    }
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "quoted-constructor.ts",
            Arc::<str>::from(source),
        )],
        &CompilerOptions {
            declaration: true,
            no_check: true,
            module: "esnext".to_string(),
            target: "es2022".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(output.exit_status, CompileExitStatus::Success);
    let product = |path: &str| {
        output
            .emitted_files
            .iter()
            .find(|file| file.path.to_string_lossy() == path)
            .unwrap_or_else(|| panic!("missing {path}"))
            .text
            .as_str()
    };
    assert_eq!(
        product("quoted-constructor.js"),
        "export class Quoted {\n    constructor(value) { }\n}\n\
         export class Escaped {\n    constructor(value) { }\n}\n",
    );
    assert_eq!(
        product("quoted-constructor.d.ts"),
        "export declare class Quoted {\n    constructor(value: number);\n}\n\
         export declare class Escaped {\n    constructor(value: number);\n}\n",
    );
}

#[test]
fn reserved_class_member_names_and_constructor_extras_defer_only_semantics() {
    let source = concat!(
        "class Reserved{",
        "#constructor:number;",
        "static prototype:number;",
        "static 'constructor'(){}",
        "'constructor'():void{}",
        "'constructor'<T>(){}",
        "#renamed:number;",
        "prototype:number;",
        "}",
    );
    let file = program_file(0, "reserved-members.ts", source);
    let StatementKind::Class(class) = &file.syntax.statements[0].kind else {
        panic!("class expected")
    };
    let analysis = default_analysis(&file);
    for member in &class.members[..4] {
        let scope = CapabilityScope::node(file.source.id, member.id);
        for target in [
            CapabilityTarget::SemanticCheck,
            CapabilityTarget::RequiredType,
            CapabilityTarget::SemanticDiagnostics,
        ] {
            let CapabilityClaim::Nonclaimed(reasons) = analysis.claim(target, scope) else {
                panic!("reserved class-member semantics must defer")
            };
            assert!(reasons.into_iter().any(|reason| {
                reason.target == target
                    && reason.scope == scope
                    && reason.reason == NonclaimReason::Semantic(SemanticGap::ClassMemberSemantics)
                    && reason.deletion
                        == DeletionCondition::SemanticOwner(SemanticGap::ClassMemberSemantics)
            }));
        }
    }
    for member in &class.members[4..] {
        let scope = CapabilityScope::node(file.source.id, member.id);
        for target in [
            CapabilityTarget::SemanticCheck,
            CapabilityTarget::RequiredType,
            CapabilityTarget::SemanticDiagnostics,
        ] {
            assert!(analysis.claim(target, scope).is_claimed(), "{target:?}");
        }
    }
}

#[test]
fn templates_in_skipped_class_hosts_defer_only_the_class_member_semantics_owner() {
    let source = concat!(
        "class ConstructorString { constructor() { const value = `x${\"y\"}z`; } }\n",
        "class ConstructorConditional { constructor(flag:boolean) { const value = `x${flag ? 1 : 2}z`; } }\n",
        "class MethodBody { method(value:string) { return `x${value}z`; } }\n",
        "class PropertyInitializer { value = `x${\"y\"}z`; }\n",
        "class ConstructorDefault { constructor(value = ((`x${1}z`))) {} }\n",
        "class MethodDefault { method(value = `x${1}z`) {} }\n",
        "class NestedMethodBody { method() { const callback = () => `x${1}z`; } }\n",
        "const independent: string = 1;\n",
    );
    let file = program_file(0, "constructor-template.ts", source);
    let classes = file
        .syntax
        .statements
        .iter()
        .filter_map(|statement| match &statement.kind {
            StatementKind::Class(class) => Some(class),
            _ => None,
        })
        .collect::<Vec<_>>();
    let affected = classes
        .iter()
        .map(|class| &class.members[0])
        .collect::<Vec<_>>();
    let analysis = default_analysis(&file);
    for member in [
        affected[0],
        affected[2],
        affected[3],
        affected[4],
        affected[5],
        affected[6],
    ] {
        let scope = CapabilityScope::node(file.source.id, member.id);
        for target in [
            CapabilityTarget::SemanticCheck,
            CapabilityTarget::RequiredType,
            CapabilityTarget::SemanticDiagnostics,
        ] {
            let CapabilityClaim::Nonclaimed(reasons) = analysis.claim(target, scope) else {
                panic!("a skipped class semantic host must defer: {target:?}")
            };
            assert!(reasons.into_iter().any(|reason| {
                reason.target == target
                    && reason.scope == scope
                    && reason.reason == NonclaimReason::Semantic(SemanticGap::ClassMemberSemantics)
                    && reason.deletion
                        == DeletionCondition::SemanticOwner(SemanticGap::ClassMemberSemantics)
            }));
        }
        assert!(
            analysis
                .claim(CapabilityTarget::QuickInfo, scope)
                .is_claimed(),
            "the class-member semantic gap does not erase binder-owned QuickInfo",
        );
    }

    let conditional_source =
        "class Conditional { constructor(flag: boolean) { const value = `x${flag ? 1 : 2}z`; } }";
    let conditional_output = Compiler::new().compile(
        vec![SourceInput::new(
            "constructor-conditional-template.ts",
            Arc::<str>::from(conditional_source),
        )],
        &CompilerOptions {
            no_emit: true,
            strict: true,
            target: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(
        conditional_output.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert_eq!(
        conditional_output.exit_status,
        CompileExitStatus::SemanticIncomplete
    );
    assert!(conditional_output.diagnostics.is_empty());

    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "constructor-template.ts",
            Arc::<str>::from(source),
        )],
        &CompilerOptions {
            no_emit: true,
            strict: true,
            target: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [2322],
        "an independent same-file diagnostic remains visible",
    );

    let positive = concat!(
        "class Annotated { value: \"x1y\" = `x${1}y`; }\n",
        "class Accessor { get value(): \"x\" { return `x`; } }\n",
        "class ConstructorDefault { constructor(callback = () => `x${1}y`) {} }\n",
        "class MethodDefault { method(callback = () => `x${1}y`) {} }\n",
        "class PropertyFunction { value = () => `x${1}y`; }\n",
        "class LexicalCall { dispatch(value: string): void {} value = (this.dispatch)(`x${1}y`); }\n",
        "const exact: \"x1y\" = `x${((1))}y`;\n",
    );
    let positive_file = program_file(0, "checked-class-template.ts", positive);
    let positive_analysis = default_analysis(&positive_file);
    let positive_classes = positive_file
        .syntax
        .statements
        .iter()
        .filter_map(|statement| {
            let StatementKind::Class(class) = &statement.kind else {
                return None;
            };
            Some(class)
        })
        .collect::<Vec<_>>();
    for member in [
        &positive_classes[0].members[0],
        &positive_classes[1].members[0],
        &positive_classes[2].members[0],
        &positive_classes[3].members[0],
        &positive_classes[4].members[0],
        &positive_classes[5].members[1],
    ] {
        let scope = CapabilityScope::node(positive_file.source.id, member.id);
        for target in [
            CapabilityTarget::SemanticCheck,
            CapabilityTarget::RequiredType,
            CapabilityTarget::SemanticDiagnostics,
        ] {
            assert!(
                positive_analysis.claim(target, scope).is_claimed(),
                "visited annotated/accessor semantics remain claimed: {target:?}",
            );
        }
    }
    let compile_checked = |source: &str| {
        Compiler::new().compile(
            vec![SourceInput::new(
                "checked-class-template.ts",
                Arc::<str>::from(source),
            )],
            &CompilerOptions {
                no_emit: true,
                strict: true,
                target: "esnext".to_string(),
                ..CompilerOptions::default()
            },
        )
    };
    for source in [
        "class ConstructorDefault { constructor(callback = () => `x${1}y`) {} }",
        "class MethodDefault { method(callback = () => `x${1}y`) {} }",
    ] {
        let output = compile_checked(source);
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        assert!(output.diagnostics.is_empty(), "{source}");
    }
    for source in [
        "class Annotated { value: \"x1y\" = `x${1}y`; }",
        "class Accessor { get value(): \"x\" { return `x`; } }",
        "class PropertyFunction { value = () => `x${1}y`; }",
        "class LexicalCall { dispatch(value: string): void {} value = (this.dispatch)(`x${1}y`); }",
        "const exact: \"x1y\" = `x${((1))}y`;",
    ] {
        let output = compile_checked(source);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source}"
        );
        assert_eq!(output.exit_status, CompileExitStatus::Success, "{source}");
        assert!(output.diagnostics.is_empty(), "{source}");
    }
}

#[test]
fn generic_quoted_constructor_names_remain_ordinary_methods() {
    let source = concat!(
        "export class Generic{'constructor'<T>(){}}\n",
        "export class Escaped{static '\\x63onstructor'<T>(){}}\n",
        "export class Hidden{private 'constructor'<T>():number{return 1}}\n",
    );
    let file = program_file(0, "generic-quoted-constructor.ts", source);
    for statement in &file.syntax.statements {
        let StatementKind::Class(class) = &statement.kind else {
            panic!("class expected")
        };
        assert!(matches!(
            class.members[0].kind,
            ClassMemberKind::Method { .. }
        ));
    }
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "generic-quoted-constructor.ts",
            Arc::<str>::from(source),
        )],
        &CompilerOptions {
            declaration: true,
            no_check: true,
            module: "esnext".to_string(),
            target: "es2022".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(output.exit_status, CompileExitStatus::Success);
    let product = |path: &str| {
        output
            .emitted_files
            .iter()
            .find(|file| file.path.to_string_lossy() == path)
            .unwrap_or_else(|| panic!("missing {path}"))
            .text
            .as_str()
    };
    assert_eq!(
        product("generic-quoted-constructor.js"),
        "export class Generic {\n    'constructor'() { }\n}\n\
         export class Escaped {\n    static '\\x63onstructor'() { }\n}\n\
         export class Hidden {\n    'constructor'() {\n        return 1;\n    }\n}\n",
    );
    assert_eq!(
        product("generic-quoted-constructor.d.ts"),
        "export declare class Generic {\n    'constructor'<T>(): void;\n}\n\
         export declare class Escaped {\n    static '\\x63onstructor'<T>(): void;\n}\n\
         export declare class Hidden {\n    private 'constructor';\n}\n",
    );
}

#[test]
fn definite_quoted_constructor_properties_recover_before_the_following_block() {
    let cases = [
        (
            "ordinary",
            "export class Ordinary { 'constructor'!(){} }\n",
            "export class Ordinary {\n    'constructor';\n}\n{ }\n",
        ),
        (
            "escaped",
            "export class Escaped { '\\x63onstructor'!(){} }\n",
            "export class Escaped {\n    '\\x63onstructor';\n}\n{ }\n",
        ),
        (
            "static",
            "export class Static { static 'constructor'!(){} }\n",
            "export class Static {\n    static 'constructor';\n}\n{ }\n",
        ),
        (
            "private",
            "export class Hidden { private 'constructor'!(){} }\n",
            "export class Hidden {\n    'constructor';\n}\n{ }\n",
        ),
    ];
    for (case, source, expected_javascript) in cases {
        let opening = source.find("!()").unwrap() as u32 + 1;
        let expected_diagnostics = [
            (
                1441,
                opening,
                "Cannot start a function call in a type annotation.",
            ),
            (
                1068,
                opening + 1,
                "Unexpected token. A constructor, method, accessor, or property was expected.",
            ),
            (
                1068,
                opening + 2,
                "Unexpected token. A constructor, method, accessor, or property was expected.",
            ),
            (
                1128,
                source.rfind('}').unwrap() as u32,
                "Declaration or statement expected.",
            ),
        ];
        for no_check in [false, true] {
            let path = format!("definite-{case}.ts");
            let output = Compiler::new().compile(
                vec![SourceInput::new(path.clone(), Arc::<str>::from(source))],
                &CompilerOptions {
                    no_check,
                    module: "esnext".to_string(),
                    target: "es2022".to_string(),
                    ..CompilerOptions::default()
                },
            );
            assert_eq!(
                output
                    .diagnostics
                    .iter()
                    .map(|diagnostic| (
                        diagnostic.code,
                        diagnostic.start,
                        diagnostic.message_text.as_str(),
                    ))
                    .collect::<Vec<_>>(),
                expected_diagnostics,
                "{case}, no_check={no_check}: {:#?}",
                output.diagnostics,
            );
            assert!(
                output
                    .diagnostics
                    .iter()
                    .all(|diagnostic| diagnostic.length == 1)
            );
            assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
            assert_eq!(
                output.exit_status,
                CompileExitStatus::DiagnosticsPresentOutputsGenerated
            );
            let javascript_path = format!("definite-{case}.js");
            let javascript = output
                .emitted_files
                .iter()
                .find(|file| file.path.to_string_lossy() == javascript_path)
                .unwrap_or_else(|| panic!("missing {javascript_path}"));
            assert_eq!(javascript.text, expected_javascript, "{case}");

            let class = output.program.files[0]
                .syntax
                .statements
                .iter()
                .find_map(|statement| match &statement.kind {
                    StatementKind::Class(class) => Some(class),
                    _ => None,
                })
                .expect("recovered class");
            assert!(matches!(
                class.members[0].kind,
                ClassMemberKind::Property { definite: true, .. }
            ));
        }
    }
}

#[test]
fn quoted_constructor_adjacent_forms_keep_their_distinct_member_grammar() {
    let source = concat!(
        "class Adjacent {",
        "'constructor'?(){}",
        "'constructor'<Renamed>(){}",
        "async 'constructor'(){}",
        "*'constructor'(){}",
        "get 'constructor'(){return 1}",
        "set 'constructor'(renamed:number){}",
        "}",
    );
    let parsed = crate::syntax::parse_source(&crate::source::SourceText::new(
        crate::source::FileId(0),
        "quoted-constructor-adjacent.ts".into(),
        Arc::<str>::from(source),
    ));
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let StatementKind::Class(class) = &parsed.unit.statements[0].kind else {
        panic!("class expected")
    };
    assert_eq!(class.members.len(), 6);
    assert!(matches!(
        class.members[0].kind,
        ClassMemberKind::Method { accessor: None, .. }
    ));
    assert!(matches!(
        &class.members[1].kind,
        ClassMemberKind::Method {
            type_parameters,
            accessor: None,
            ..
        } if type_parameters.len() == 1
    ));
    assert!(matches!(
        class.members[2].kind,
        ClassMemberKind::Constructor { .. }
    ));
    assert!(class.members[2].modifiers.async_member);
    assert!(matches!(
        class.members[3].kind,
        ClassMemberKind::Method { accessor: None, .. }
    ));
    assert!(matches!(
        class.members[4].kind,
        ClassMemberKind::Method {
            accessor: Some(AccessorKind::Get),
            ..
        }
    ));
    assert!(matches!(
        class.members[5].kind,
        ClassMemberKind::Method {
            accessor: Some(AccessorKind::Set),
            ..
        }
    ));
}
