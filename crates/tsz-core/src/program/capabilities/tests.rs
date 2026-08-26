use std::path::PathBuf;
use std::sync::Arc;

use crate::bind::bind_source;
use crate::source::{FileId, SourceText};
use crate::syntax::{
    ClassMemberKind, PropertyNameKind, TypeNodeKind, for_each_statement_in, parse_source,
};

use super::*;

#[path = "../../../rewrite-tests/capabilities_emit_unit.rs"]
mod emit;

#[path = "../../../rewrite-tests/capabilities_accessor_unit.rs"]
mod accessor;

#[path = "../../../rewrite-tests/capabilities_recovery_unit.rs"]
mod recovery;

#[path = "../../../rewrite-tests/capabilities_recovery_closure_unit.rs"]
mod recovery_closure;

fn program_file(id: u32, path: &str, text: &str) -> ProgramFile {
    let source = SourceText::new(FileId(id), PathBuf::from(path), Arc::<str>::from(text));
    let parsed = parse_source(&source);
    let bindings = bind_source(source.id, &parsed.unit);
    ProgramFile {
        source,
        syntax: parsed.unit,
        bindings,
    }
}

fn default_analysis(file: &ProgramFile) -> CapabilityAnalysis {
    CapabilityAnalysis::derive(
        std::slice::from_ref(file),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    )
}

fn parser_recovery_statement_roles(
    file: &ProgramFile,
    recovery: &crate::syntax::ParserRecoveryFact,
    recovery_extent: Span,
) -> BTreeMap<NodeId, RecoveryStatementRole> {
    recovery_nodes(
        file,
        recovery.owner,
        recovery.authored_span,
        recovery_extent,
    )
    .owners
}

fn statement_starting_at(file: &ProgramFile, source: &str, text: &str) -> NodeId {
    let start = source.find(text).expect("statement text") as u32;
    let mut owner = None;
    for_each_statement_in(&file.syntax.statements, &mut |statement| {
        if statement.span.start == start {
            owner = Some(statement.id);
        }
    });
    owner.expect("represented statement")
}

#[test]
fn asserted_switch_expression_does_not_start_a_flow_region() {
    let file = program_file(
        0,
        "asserted-switch.ts",
        concat!(
            "let subject: string | number = 0;\n",
            "switch (subject as string | number) { default: break; }\n",
            "const kept: string = 1;\n",
        ),
    );
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    for statement in &file.syntax.statements {
        assert!(
            analysis
                .claim(
                    CapabilityTarget::SemanticCheck,
                    CapabilityScope::node(file.source.id, statement.id),
                )
                .is_claimed(),
            "assertions are not transparent narrowing references: {statement:#?}",
        );
    }
}

#[test]
fn authored_function_expression_modifiers_fail_both_emit_products_closed() {
    let gap = SyntaxGap::FunctionExpressionModifier;
    for (path, source) in [
        (
            "async-expression.ts",
            "const renamed = async function changed() {};\nconst independent = 1;",
        ),
        (
            "generator-expression.ts",
            "const renamed = function* changed() {};",
        ),
        (
            "async-generator-expression.ts",
            "const renamed = async function* changed() {};",
        ),
    ] {
        let file = program_file(0, path, source);
        assert!(
            file.syntax
                .has_source_syntax_fact(SourceSyntaxFact::AuthoredFunctionExpressionModifier),
            "{path}",
        );
        let analysis = CapabilityAnalysis::derive(
            std::slice::from_ref(&file),
            &CompilerOptions::default(),
            CapabilityContext::default(),
        );
        for target in [CapabilityTarget::JavaScript, CapabilityTarget::Declaration] {
            let CapabilityClaim::Nonclaimed(reasons) =
                analysis.claim(target, CapabilityScope::File(file.source.id))
            else {
                panic!("{target:?} must be withheld for {path}");
            };
            assert!(reasons.into_iter().any(|reason| {
                reason.scope == CapabilityScope::File(file.source.id)
                    && reason.reason == NonclaimReason::Syntax(gap)
                    && reason.deletion == DeletionCondition::SyntaxOwner(gap)
            }));
        }
        if path == "async-expression.ts" {
            let async_start = source.find("async").expect("async token") as u32;
            let recovery = file
                .syntax
                .parser_recovery_facts
                .iter()
                .find(|recovery| recovery.authored_span.start == async_start)
                .expect("async FunctionExpression recovery");
            assert_eq!(recovery.kind, ParserRecoveryKind::Expression);
            assert_eq!(
                recovery.authored_span.end,
                async_start + "async".len() as u32
            );
            assert_eq!(
                recovery.recovery_extent.end,
                source.find(";\n").expect("closed modifier expression") as u32 + 1,
            );
        }
    }
}

#[test]
fn rejected_generic_arrow_prefixes_fail_only_their_file_products_closed() {
    let source = "export const value = <Cedar,>(): => 1;";
    let affected = program_file(0, "affected.ts", source);
    let stable = program_file(1, "stable.ts", "export const sibling = 1;");
    let recovery = affected
        .syntax
        .parser_recovery_facts
        .iter()
        .find(|recovery| recovery.kind == ParserRecoveryKind::RejectedGenericArrowPrefix)
        .copied()
        .expect("typed rejected generic-arrow prefix");
    assert_eq!(
        recovery.authored_span.start,
        source.find('<').unwrap() as u32
    );
    assert_eq!(recovery.owner.statement, affected.syntax.statements[0].id);
    let files = [affected, stable];
    let analysis = CapabilityAnalysis::derive(
        &files,
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    let gap = SyntaxGap::RejectedGenericArrowPrefix;
    let scope = CapabilityScope::node(files[0].source.id, recovery.owner.statement);
    assert!(
        analysis
            .claim(CapabilityTarget::DeclarationModel, scope)
            .is_claimed()
    );
    for target in [CapabilityTarget::JavaScript, CapabilityTarget::Declaration] {
        let CapabilityClaim::Nonclaimed(reasons) =
            analysis.claim(target, CapabilityScope::File(files[0].source.id))
        else {
            panic!("{target:?} must be withheld for the affected file");
        };
        assert!(reasons.into_iter().any(|reason| {
            reason.scope == scope
                && reason.reason == NonclaimReason::Syntax(gap)
                && reason.deletion == DeletionCondition::SyntaxOwner(gap)
        }));
        assert!(
            analysis
                .claim(target, CapabilityScope::File(files[1].source.id))
                .is_claimed(),
            "the stable file must keep {target:?}",
        );
    }

    for source in [
        "const value = <Cedar>(renamed);",
        "const value = <Cedar>({ renamed });",
    ] {
        let ordinary = program_file(0, "ordinary.ts", source);
        assert!(
            ordinary
                .syntax
                .parser_recovery_facts
                .iter()
                .all(|recovery| recovery.kind != ParserRecoveryKind::RejectedGenericArrowPrefix),
            "{source}",
        );
        assert!(
            ordinary
                .syntax
                .parser_recovery_facts
                .iter()
                .any(|recovery| { recovery.kind == ParserRecoveryKind::AngleAssertion })
        );
        let ordinary_analysis = CapabilityAnalysis::derive(
            std::slice::from_ref(&ordinary),
            &CompilerOptions::default(),
            CapabilityContext::default(),
        );
        for target in [CapabilityTarget::JavaScript, CapabilityTarget::Declaration] {
            assert!(
                ordinary_analysis
                    .claim(target, CapabilityScope::File(ordinary.source.id))
                    .is_claimed(),
                "angle-assertion products are structurally representable: {source}",
            );
        }
        for target in [
            CapabilityTarget::SemanticCheck,
            CapabilityTarget::SemanticDiagnostics,
            CapabilityTarget::RequiredType,
        ] {
            assert!(
                !ordinary_analysis
                    .claim(target, CapabilityScope::File(ordinary.source.id))
                    .is_claimed(),
                "angle-assertion semantics remain explicitly nonclaimed: {source}",
            );
        }
    }
}

#[test]
fn recovered_function_like_binding_patterns_have_one_typed_product_boundary() {
    let gap = SyntaxGap::FunctionLikeBindingPattern;
    for (path, source) in [
        (
            "arrow-binding-pattern.ts",
            "const callback = ({ renamed }: any) => renamed;",
        ),
        (
            "function-binding-pattern.ts",
            "const callback = function ({ renamed }: any) { return renamed; };",
        ),
        (
            "empty-arrow-binding-pattern.ts",
            "const callback = ({}) => 1;",
        ),
        (
            "empty-function-binding-pattern.ts",
            "const callback = function ([]) { return 1; };",
        ),
    ] {
        let file = program_file(0, path, source);
        let StatementKind::Variable(variable) = &file.syntax.statements[0].kind else {
            panic!("variable expected: {:#?}", file.syntax.statements);
        };
        let function = variable.declarators[0]
            .initializer
            .as_ref()
            .expect("initializer");
        assert!(matches!(function.kind, ExpressionKind::FunctionLike(_)));
        let scope = CapabilityScope::node(file.source.id, function.id);
        let analysis = CapabilityAnalysis::derive(
            std::slice::from_ref(&file),
            &CompilerOptions::default(),
            CapabilityContext::default(),
        );

        for target in [
            CapabilityTarget::SemanticCheck,
            CapabilityTarget::DeclarationValue,
            CapabilityTarget::RequiredType,
            CapabilityTarget::SemanticDiagnostics,
            CapabilityTarget::JavaScript,
            CapabilityTarget::Declaration,
            CapabilityTarget::QuickInfo,
            CapabilityTarget::Definition,
            CapabilityTarget::References,
            CapabilityTarget::Highlights,
            CapabilityTarget::Rename,
        ] {
            let CapabilityClaim::Nonclaimed(reasons) = analysis.claim(target, scope) else {
                panic!("{path}: {target:?} must be nonclaimed");
            };
            assert!(reasons.into_iter().any(|reason| {
                reason.scope == scope
                    && reason.reason == NonclaimReason::Syntax(gap)
                    && reason.deletion == DeletionCondition::SyntaxOwner(gap)
            }));
        }
        if let Some(parameter) = file
            .bindings
            .declarations
            .iter()
            .find(|declaration| declaration.name == "renamed")
        {
            assert_eq!(parameter.kind, DeclarationKind::Parameter);
            assert_eq!(parameter.owner, function.id);
            assert!(
                !analysis
                    .semantic_declaration_is_claimed(std::slice::from_ref(&file), parameter.id,),
                "{path}: recovered parameter values use the FunctionLike capability owner",
            );
        }
    }
}

#[test]
fn swallowed_template_identifiers_withhold_reference_enumeration_program_wide() {
    for (path, source, contains_identifier) in [
        (
            "template-reference.ts",
            "const safe = 1; const gap = `${safe}`; const useSafe = safe;",
            false,
        ),
        (
            "template-literal.ts",
            "const safe = 1; const gap = `${\"safe\"}`; const useSafe = safe;",
            false,
        ),
        (
            "tagged-template-reference.ts",
            "declare const tag: any; const safe = 1; const gap = tag`${safe}`;",
            true,
        ),
    ] {
        let file = program_file(0, path, source);
        assert_eq!(
            file.syntax
                .has_source_syntax_fact(SourceSyntaxFact::TemplateExpressionIdentifier),
            contains_identifier,
            "{path}",
        );
        let analysis = CapabilityAnalysis::derive(
            std::slice::from_ref(&file),
            &CompilerOptions::default(),
            CapabilityContext::default(),
        );
        for target in &ALL_TARGETS[9..] {
            let program_reason = analysis.nonclaims.iter().any(|nonclaim| {
                nonclaim.target == *target
                    && nonclaim.scope == CapabilityScope::Program
                    && nonclaim.reason == NonclaimReason::Syntax(SyntaxGap::Template)
                    && nonclaim.deletion == DeletionCondition::SyntaxOwner(SyntaxGap::Template)
            });
            assert_eq!(program_reason, contains_identifier, "{path}: {target:?}");
        }
    }
}

#[test]
fn exact_template_recovery_may_reenter_a_claimed_arrow_required_type_owner() {
    let source =
        "const values = [(null as any)`head${\"gap\"}tail`, (value: MissingArrowType) => value];";
    let file = program_file(0, "required-arrow.ts", source);
    assert_eq!(
        file.syntax
            .parser_recovery_facts
            .iter()
            .filter(|fact| fact.kind == ParserRecoveryKind::Template)
            .count(),
        1,
    );
    let statement = &file.syntax.statements[0];
    let StatementKind::Variable(variable) = &statement.kind else {
        panic!("variable expected: {statement:#?}");
    };
    let initializer = variable.declarators[0]
        .initializer
        .as_ref()
        .expect("array initializer");
    let ExpressionKind::Array(elements) = &initializer.kind else {
        panic!("array initializer expected: {variable:#?}");
    };
    let arrow = &elements[1];
    let ExpressionKind::FunctionLike(function) = &arrow.kind else {
        panic!("arrow expected: {arrow:#?}");
    };
    assert!(matches!(
        function.parameters[0]
            .annotation
            .as_ref()
            .map(|annotation| &annotation.kind),
        Some(TypeNodeKind::Reference { name, .. }) if name == "MissingArrowType"
    ));
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    assert!(
        !analysis
            .claim(
                CapabilityTarget::RequiredType,
                CapabilityScope::node(file.source.id, statement.id),
            )
            .is_claimed()
    );
    assert!(
        analysis.required_type_node_allows_function_like_reentry(file.source.id, statement.id,)
    );
    assert!(
        analysis
            .claim(
                CapabilityTarget::RequiredType,
                CapabilityScope::node(file.source.id, arrow.id),
            )
            .is_claimed()
    );
}

#[test]
fn named_tuple_arrow_recovery_is_owned_by_its_function_like_signature() {
    let source = concat!(
        "const renamed = (...values: [label: \"label\", item: \"item\"]): void => {",
        "values; const dependent: MissingTupleBody = 1; };\n",
        "const independent: MissingTupleSibling = 1;",
    );
    let file = program_file(0, "recovered-arrow-header.ts", source);
    let labels = file
        .syntax
        .parser_recovery_facts
        .iter()
        .filter(|recovery| recovery.kind == ParserRecoveryKind::Type)
        .collect::<Vec<_>>();
    assert_eq!(labels.len(), 2, "{:#?}", file.syntax.parser_recovery_facts);
    assert!(labels.iter().all(|recovery| {
        recovery.authored_span.start >= source.find("label").unwrap() as u32
            && recovery.recovery_extent.end < source.find("values;").unwrap() as u32
    }));

    let StatementKind::Variable(variable) = &file.syntax.statements[0].kind else {
        panic!("variable expected")
    };
    let arrow = variable.declarators[0]
        .initializer
        .as_ref()
        .expect("arrow initializer");
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    assert!(
        !analysis
            .claim(
                CapabilityTarget::RequiredType,
                CapabilityScope::node(file.source.id, arrow.id),
            )
            .is_claimed()
    );
    assert!(
        analysis
            .claim(
                CapabilityTarget::SemanticCheck,
                CapabilityScope::node(file.source.id, file.syntax.statements[0].id),
            )
            .is_claimed(),
        "the containing variable remains independently checkable",
    );
}

#[test]
fn recovered_type_members_use_the_type_recovery_product_owner() {
    for source in [
        "export interface Renamed{x=1}",
        "export type Outer={nested:{x=1}}; export const sibling=1;",
    ] {
        let file = program_file(0, "recovered-type-member.ts", source);
        let facts = file
            .syntax
            .parser_recovery_facts
            .iter()
            .filter(|recovery| recovery.kind == ParserRecoveryKind::Type)
            .collect::<Vec<_>>();
        assert_eq!(facts.len(), 1, "{source}: {facts:#?}");
        assert_eq!(file.source.slice(facts[0].authored_span), "x", "{source}");

        let analysis = default_analysis(&file);
        let scope = CapabilityScope::File(file.source.id);
        assert!(analysis.product_is_claimed(
            CapabilityTarget::JavaScript,
            scope,
            &CompilerOptions::default(),
        ));
        assert!(!analysis.product_is_claimed(
            CapabilityTarget::Declaration,
            scope,
            &CompilerOptions::default(),
        ));
    }
}

#[test]
fn non_expression_parameters_retain_their_containing_statement_capability_scope() {
    let source = concat!(
        "class Holder { method(value = (null as any)`head${renamed}tail`) { return value; } } ",
        "const renamed = 1;",
    );
    let file = program_file(0, "member-parameter.ts", source);
    let parameter = file
        .bindings
        .declarations
        .iter()
        .find(|declaration| {
            declaration.kind == DeclarationKind::Parameter && declaration.name == "value"
        })
        .expect("method parameter");
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    assert!(
        !analysis.semantic_declaration_is_claimed(std::slice::from_ref(&file), parameter.id),
        "member parameters inherit the recovered class statement owner",
    );
}

#[test]
fn ordinary_and_line_break_function_expression_controls_remain_javascript_claimed() {
    for (path, source) in [
        (
            "plain-expression.ts",
            "const renamed = function changed() {};",
        ),
        (
            "line-break-control.ts",
            "const async = 1; const renamed = async\nfunction changed() {}",
        ),
    ] {
        let file = program_file(0, path, source);
        assert!(
            !file
                .syntax
                .has_source_syntax_fact(SourceSyntaxFact::AuthoredFunctionExpressionModifier),
            "{path}",
        );
        let analysis = CapabilityAnalysis::derive(
            std::slice::from_ref(&file),
            &CompilerOptions::default(),
            CapabilityContext::default(),
        );
        let claim = analysis.claim(
            CapabilityTarget::JavaScript,
            CapabilityScope::File(file.source.id),
        );
        assert!(claim.is_claimed(), "{path}: {claim:#?}");
    }
}

#[test]
fn javascript_claims_stop_at_unowned_function_product_interactions() {
    let assert_nonclaim =
        |path: &str, source: &str, options: CompilerOptions, expected: SyntaxGap| {
            let file = program_file(0, path, source);
            let analysis = CapabilityAnalysis::derive(
                std::slice::from_ref(&file),
                &options,
                CapabilityContext::default(),
            );
            let CapabilityClaim::Nonclaimed(reasons) = analysis.claim(
                CapabilityTarget::JavaScript,
                CapabilityScope::File(file.source.id),
            ) else {
                panic!("JavaScript must be withheld for {expected:?}");
            };
            assert!(reasons.into_iter().any(|record| {
                record.reason == NonclaimReason::Syntax(expected)
                    && record.deletion == DeletionCondition::SyntaxOwner(expected)
            }));
        };

    assert_nonclaim(
        "downlevel-class.ts",
        concat!(
            "class RenamedHost {\n",
            "  retained = 1;\n",
            "  method() { return function changed(value: number) { return value; }; }\n",
            "}\n",
        ),
        CompilerOptions {
            target: "es2015".to_string(),
            ..CompilerOptions::default()
        },
        SyntaxGap::FunctionExpressionClassPropertyTransform,
    );
    assert_nonclaim(
        "commonjs-module.ts",
        concat!(
            "import { renamed } from './dependency';\n",
            "const callback = function changed() { return renamed; };\n",
        ),
        CompilerOptions {
            module: "commonjs".to_string(),
            ..CompilerOptions::default()
        },
        SyntaxGap::FunctionExpressionCommonJsTransform,
    );
    assert_nonclaim(
        "outer-comment.ts",
        "const callbacks = [/*outside*/ function changed() { }];\n",
        CompilerOptions::default(),
        SyntaxGap::FunctionExpressionOuterComments,
    );
    for (path, source, options) in [
        (
            "preserved-class.ts",
            concat!(
                "class RenamedHost {\n",
                "  retained = 1;\n",
                "  method() { return function changed(value: number) { return value; }; }\n",
                "}\n",
            ),
            CompilerOptions {
                target: "es2022".to_string(),
                ..CompilerOptions::default()
            },
        ),
        (
            "comment-free.ts",
            "const callback = function changed(value: number) { return value; };\n",
            CompilerOptions::default(),
        ),
        (
            "multiline-declaration.ts",
            "function changed(value: number) {\n  return value;\n}\n",
            CompilerOptions::default(),
        ),
    ] {
        let file = program_file(0, path, source);
        let analysis = CapabilityAnalysis::derive(
            std::slice::from_ref(&file),
            &options,
            CapabilityContext::default(),
        );
        assert!(
            analysis
                .claim(
                    CapabilityTarget::JavaScript,
                    CapabilityScope::File(file.source.id),
                )
                .is_claimed(),
            "control must remain claimed: {path}; nonclaims={:#?}",
            analysis.nonclaims,
        );
    }
}

#[test]
fn malformed_assertion_tail_keeps_later_line_declaration_represented() {
    let bounded = program_file(
        0,
        "bounded-tail.ts",
        "const x = value as T changed\nconst y = 1;\ny;",
    );
    let [_, y_statement, y_reference] = bounded.syntax.statements.as_slice() else {
        panic!("the later-line declaration and reference must remain represented")
    };
    assert!(matches!(
        &y_statement.kind,
        StatementKind::Variable(statement)
            if statement.declarators.iter().any(|declaration| declaration.name == "y")
    ));
    assert!(matches!(
        &y_reference.kind,
        StatementKind::Expression(expression)
            if matches!(&expression.kind, ExpressionKind::Identifier { name, .. } if name == "y")
    ));
    assert!(bounded.bindings.declarations.iter().any(|declaration| {
        declaration.kind == DeclarationKind::Variable
            && declaration.name == "y"
            && declaration.owner == y_statement.id
    }));
}

#[test]
fn javascript_property_navigation_records_one_exact_tuple_per_scope_and_target() {
    use crate::program::{Compiler, SourceInput};

    let assignments = "renamedRoot.renamedProperty = 1;".repeat(16);
    let uses = "renamedRoot.renamedProperty;".repeat(64);
    for source in [
        format!("const renamedRoot = {{}};{assignments}{uses}"),
        format!("{uses}const renamedRoot = {{}};{assignments}"),
        format!("const renamedRoot = {{known: 1}};{assignments}{uses}"),
    ] {
        let output = Compiler::new().compile(
            vec![SourceInput::new("navigation.js", Arc::<str>::from(source))],
            &CompilerOptions {
                allow_js: true,
                check_js: Some(true),
                no_emit: true,
                ..CompilerOptions::default()
            },
        );
        let mut scopes = BTreeSet::new();
        for file in &output.program.files {
            scopes.extend(
                file.bindings
                    .javascript_property_uses
                    .iter()
                    .map(|&owner| CapabilityScope::node(file.source.id, owner)),
            );
            scopes.extend(
                file.bindings
                    .javascript_property_assignments
                    .iter()
                    .filter_map(|assignment| assignment.declaration)
                    .filter_map(|id| {
                        file.bindings
                            .declaration(id)
                            .map(|declaration| CapabilityScope::node(id.file, declaration.owner))
                    }),
            );
        }
        let records = output.capabilities.nonclaims.iter().filter(|record| {
            record.reason == NonclaimReason::Semantic(SemanticGap::JavaScriptPropertyNavigation)
        });
        assert_eq!(
            records.clone().count(),
            scopes.len() * ALL_TARGETS[7..].len()
        );
        assert!(records.into_iter().all(|record| {
            scopes.contains(&record.scope)
                && ALL_TARGETS[7..].contains(&record.target)
                && record.deletion
                    == DeletionCondition::SemanticOwner(SemanticGap::JavaScriptPropertyNavigation)
        }));
        let file = output.program.files[0].source.id;
        for requested in [
            CapabilityScope::Program,
            CapabilityScope::File(file),
            *scopes.first().expect("member scope"),
        ] {
            let expected = output
                .capabilities
                .nonclaims
                .iter()
                .filter(|record| {
                    record.target == CapabilityTarget::QuickInfo
                        && scope_applies(record.scope, requested)
                })
                .collect::<Vec<_>>();
            let CapabilityClaim::Nonclaimed(reasons) = output
                .capabilities
                .claim(CapabilityTarget::QuickInfo, requested)
            else {
                panic!("indexed stress claim must be nonclaimed")
            };
            assert_eq!(reasons.clone().collect::<Vec<_>>(), expected);
            assert_eq!(
                reasons
                    .ranges
                    .iter()
                    .map(|range| range.len())
                    .sum::<usize>(),
                expected.len()
            );
        }
    }
}
