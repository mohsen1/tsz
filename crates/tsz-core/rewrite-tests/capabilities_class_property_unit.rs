use super::*;

use crate::program::{CompileExitStatus, Compiler, SemanticCompletion, SourceInput};

fn declaration_options() -> CompilerOptions {
    CompilerOptions {
        declaration: true,
        target: "esnext".to_string(),
        module: "esnext".to_string(),
        ..CompilerOptions::default()
    }
}

fn emitted(output: &crate::program::CompileOutput) -> Vec<(String, bool, String)> {
    output
        .emitted_files
        .iter()
        .map(|file| {
            (
                file.path.to_string_lossy().into_owned(),
                file.declaration,
                file.text.clone(),
            )
        })
        .collect()
}

#[test]
fn class_property_jsdoc_signature_nonclaims_are_attached_to_direct_function_owners() {
    // Deletion condition: the checker owns TypeScript 7 JSDoc signature parsing
    // and contextual parameter types for JavaScript function-like syntax.
    let file = program_file(
        0,
        "properties.js",
        concat!(
            "class Vessel{",
            "/** @param {number} value */ direct=value=>value;",
            "/** @param {number} value */ wrapped=(((value)=>value));",
            "/** @param {number} value */ expression=function(value){return value};",
            "/** @param {number} value */ static fixed=value=>value;",
            "/* ordinary */ ordinary=value=>value;",
            "plain=value=>value;",
            "/** detached */\n\n detached=value=>value;",
            "/** member documentation */ nested=wrap(value=>value);",
            "}",
        ),
    );
    let StatementKind::Class(class) = &file.syntax.statements[0].kind else {
        panic!("class declaration expected")
    };
    assert_eq!(
        class
            .members
            .iter()
            .map(|member| member.has_leading_jsdoc)
            .collect::<Vec<_>>(),
        [true, true, true, true, false, false, false, true],
    );

    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &CompilerOptions {
            allow_js: true,
            check_js: Some(true),
            no_emit: true,
            ..CompilerOptions::default()
        },
        CapabilityContext::default(),
    );
    let initializer = |index: usize| match &class.members[index].kind {
        ClassMemberKind::Property {
            initializer: Some(initializer),
            ..
        } => initializer,
        _ => panic!("property initializer expected at {index}"),
    };

    for index in 0..4 {
        let owner = initializer(index).peel_parentheses().id;
        let scope = CapabilityScope::node(file.source.id, owner);
        let CapabilityClaim::Nonclaimed(reasons) =
            analysis.claim(CapabilityTarget::SemanticCheck, scope)
        else {
            panic!("documented function owner {index} must be nonclaimed")
        };
        assert_eq!(
            reasons.copied().collect::<Vec<_>>(),
            [CapabilityNonclaim {
                target: CapabilityTarget::SemanticCheck,
                scope,
                reason: NonclaimReason::Semantic(SemanticGap::JavaScriptJSDocSignature),
            }],
        );
        assert!(
            analysis
                .claim(
                    CapabilityTarget::SemanticCheck,
                    CapabilityScope::node(file.source.id, class.members[index].id),
                )
                .is_claimed(),
            "the class property remains an independently claimed owner",
        );
    }

    for index in 4..7 {
        assert!(
            analysis
                .claim(
                    CapabilityTarget::SemanticCheck,
                    CapabilityScope::node(
                        file.source.id,
                        initializer(index).peel_parentheses().id,
                    ),
                )
                .is_claimed(),
            "ordinary, absent, and detached comments do not create JSDoc signature gaps",
        );
    }
    let crate::syntax::ExpressionKind::Call { arguments, .. } =
        &initializer(7).peel_parentheses().kind
    else {
        panic!("nested call initializer expected")
    };
    let nested_owner = arguments[0].peel_parentheses().id;
    assert!(
        analysis
            .claim(
                CapabilityTarget::SemanticCheck,
                CapabilityScope::node(file.source.id, nested_owner),
            )
            .is_claimed(),
        "member JSDoc must not leak through an intervening call into a nested function",
    );
}

#[test]
fn unannotated_published_class_properties_have_typed_member_nonclaims() {
    let file = program_file(
        0,
        "properties.ts",
        concat!(
            "declare const source:number;declare function make():number;",
            "export class Vessel{",
            "primitive=1;readonly label='kept';static flag=true;",
            "linked=source;created=make();shaped={value:1};",
            "annotated:number=1;private hidden=2;#secret=3;pending;",
            "}",
        ),
    );
    let StatementKind::Class(class) = &file.syntax.statements[2].kind else {
        panic!("class declaration expected")
    };
    let analysis = default_analysis(&file);

    for index in [0, 1, 2, 3, 4, 5, 9] {
        let member = &class.members[index];
        let scope = CapabilityScope::node(file.source.id, member.id);
        let CapabilityClaim::Nonclaimed(reasons) =
            analysis.claim(CapabilityTarget::Declaration, scope)
        else {
            panic!("member {index} must wait for its checked declaration type")
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

    for index in [6, 7, 8] {
        let member = &class.members[index];
        assert!(
            analysis
                .claim(
                    CapabilityTarget::Declaration,
                    CapabilityScope::node(file.source.id, member.id),
                )
                .is_claimed(),
            "annotated and erased member {index} is independent of inferred display",
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
fn compiler_omits_incomplete_class_property_declarations_without_unknown_fallbacks() {
    for no_check in [false, true] {
        let mut options = declaration_options();
        options.no_check = no_check;
        let output = Compiler::new().compile(
            vec![SourceInput::new(
                "properties.ts",
                Arc::<str>::from(concat!(
                    "declare const source:number;declare function make():number;",
                    "export class Vessel{",
                    "primitive=1;readonly label='kept';static flag=true;",
                    "linked=source;created=make();shaped={value:1};pending;",
                    "}",
                )),
            )],
            &options,
        );

        assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        assert_eq!(
            output
                .emitted_files
                .iter()
                .map(|file| (file.path.to_string_lossy().into_owned(), file.declaration))
                .collect::<Vec<_>>(),
            [("properties.js".to_string(), false)]
        );
        assert!(
            output
                .emitted_files
                .iter()
                .all(|file| !file.text.contains("unknown"))
        );
    }
}

#[test]
fn annotated_and_erased_class_properties_keep_complete_declaration_emit() {
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "stable.ts",
            Arc::<str>::from("export class Stable{value:number=1;private hidden=2;#secret=3;}"),
        )],
        &declaration_options(),
    );

    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(output.exit_status, CompileExitStatus::Success);
    let declaration = output
        .emitted_files
        .iter()
        .find(|file| file.declaration)
        .expect("complete declaration product");
    assert!(
        declaration.text.contains("value: number;"),
        "{}",
        declaration.text
    );
    assert!(
        declaration.text.contains("private hidden;"),
        "{}",
        declaration.text
    );
    assert!(
        !declaration.text.contains("unknown"),
        "{}",
        declaration.text
    );
}

#[test]
fn class_property_declaration_nonclaims_are_root_order_independent() {
    let roots = [
        ("affected.ts", "export class Pending{value=1;}"),
        ("stable.ts", "export class Stable{value:number=1;}"),
    ];
    let compile = |roots: &[(&str, &str)]| {
        Compiler::new().compile(
            roots
                .iter()
                .map(|(path, text)| SourceInput::new(*path, Arc::<str>::from(*text)))
                .collect(),
            &declaration_options(),
        )
    };
    let forward = compile(&roots);
    let reverse = compile(&[roots[1], roots[0]]);

    assert_eq!(forward.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(reverse.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(emitted(&forward), emitted(&reverse));
    assert_eq!(
        forward
            .emitted_files
            .iter()
            .map(|file| file.path.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        ["affected.js", "stable.d.ts", "stable.js"]
    );
}
