use std::path::PathBuf;
use std::sync::Arc;

use crate::bind::bind_source_with_kind;
use crate::source::{FileId, SourceKind, SourceText};
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

#[path = "../../../rewrite-tests/capabilities_class_property_unit.rs"]
mod class_property;

/// Typed exit criterion derived from a structural nonclaim reason. Keeping it
/// derived lets tests enumerate owner obligations without duplicating state in
/// each immutable capability record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum DeletionCondition {
    SyntaxOwner(SyntaxGap),
    DeepestSemanticOwner(SyntaxGap),
    SemanticOwner(SemanticGap),
    EssentialLibraryUniverse,
    CompilerOptionOwner,
}

impl CapabilityNonclaim {
    const fn deletion_condition(self) -> DeletionCondition {
        match self.reason {
            NonclaimReason::Syntax(gap) => DeletionCondition::SyntaxOwner(gap),
            NonclaimReason::SyntaxAtSemanticOwner(gap) => {
                DeletionCondition::DeepestSemanticOwner(gap)
            }
            NonclaimReason::Semantic(gap) => DeletionCondition::SemanticOwner(gap),
            NonclaimReason::MissingEssentialTypes => DeletionCondition::EssentialLibraryUniverse,
            NonclaimReason::FatalCompilerOption
            | NonclaimReason::UnsupportedCompilerOption(_)
            | NonclaimReason::DeferredCompilerOption(_) => DeletionCondition::CompilerOptionOwner,
        }
    }
}

fn program_file(id: u32, path: &str, text: &str) -> ProgramFile {
    let source = SourceText::new(FileId(id), PathBuf::from(path), Arc::<str>::from(text));
    let parsed = parse_source(&source);
    let bindings = bind_source_with_kind(source.id, SourceKind::TypeScript, &parsed.unit);
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

fn position_scope(file: &ProgramFile, offset: u32) -> CapabilityScope {
    let owner = match file.capability_scope_at(offset) {
        Some(CapabilityScope::Node { owner, .. }) => Some(owner),
        _ => None,
    };
    CapabilityScope::Position {
        file: file.source.id,
        owner,
        offset,
    }
}

fn navigation_identity_nonclaim<'a>(
    analysis: &'a CapabilityAnalysis,
    target: CapabilityTarget,
    file: &ProgramFile,
    offset: u32,
) -> Option<&'a CapabilityNonclaim> {
    let CapabilityClaim::Nonclaimed(reasons) = analysis.claim(target, position_scope(file, offset))
    else {
        return None;
    };
    reasons
        .into_iter()
        .find(|reason| reason.reason == NonclaimReason::Semantic(SemanticGap::NavigationIdentity))
}

fn semantic_descendant_is_claimed(
    analysis: &CapabilityAnalysis,
    file: FileId,
    owner: NodeId,
    identifiers: bool,
) -> bool {
    analysis
        .claim(
            CapabilityTarget::SemanticCheck,
            CapabilityScope::semantic_descendant(file, owner, identifiers),
        )
        .is_claimed()
}

fn function_like_descendant_is_claimed(
    analysis: &CapabilityAnalysis,
    file: FileId,
    owner: NodeId,
    identifiers: bool,
) -> bool {
    analysis
        .claim(
            CapabilityTarget::SemanticCheck,
            CapabilityScope::function_like_descendant(file, owner, identifiers),
        )
        .is_claimed()
}

fn required_function_like_is_claimed(
    analysis: &CapabilityAnalysis,
    file: FileId,
    owner: NodeId,
) -> bool {
    analysis
        .claim(
            CapabilityTarget::RequiredType,
            CapabilityScope::required_function_like(file, owner),
        )
        .is_claimed()
}

fn parser_recovery_statement_roles(
    file: &ProgramFile,
    recovery: &crate::syntax::ParserRecoveryFact,
    recovery_extent: Span,
) -> BTreeMap<NodeId, RecoveryRole> {
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
                    && reason.deletion_condition() == DeletionCondition::SyntaxOwner(gap)
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
                && reason.deletion_condition() == DeletionCondition::SyntaxOwner(gap)
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
                    && reason.deletion_condition() == DeletionCondition::SyntaxOwner(gap)
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
fn swallowed_template_identifiers_withhold_exhaustive_symbol_sets_program_wide() {
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
        for (target, expected) in [
            (CapabilityTarget::References, contains_identifier),
            (CapabilityTarget::Highlights, contains_identifier),
            (CapabilityTarget::Rename, contains_identifier),
        ] {
            let program_reason = analysis.nonclaims.iter().any(|nonclaim| {
                nonclaim.target == target
                    && nonclaim.scope == CapabilityScope::Program
                    && nonclaim.reason == NonclaimReason::Syntax(SyntaxGap::Template)
                    && nonclaim.deletion_condition()
                        == DeletionCondition::SyntaxOwner(SyntaxGap::Template)
            });
            assert_eq!(program_reason, expected, "{path}: {target:?}");
        }
        for target in [CapabilityTarget::QuickInfo, CapabilityTarget::Definition] {
            assert!(
                !analysis.nonclaims.iter().any(|nonclaim| {
                    nonclaim.target == target
                        && nonclaim.scope == CapabilityScope::Program
                        && nonclaim.reason == NonclaimReason::Syntax(SyntaxGap::Template)
                }),
                "{path}: the exhaustive binder fence must not own {target:?}",
            );
        }
    }
}

#[test]
fn swallowed_template_program_fence_closes_same_and_cross_file_navigation() {
    let sources = [
        (
            "tagged.ts",
            "declare const tag: any; const renamed = 1; tag`${renamed}`; renamed;",
        ),
        ("independent.ts", "const independent = 2; independent;"),
    ];
    let files = sources
        .iter()
        .enumerate()
        .map(|(id, (path, source))| program_file(id as u32, path, source))
        .collect::<Vec<_>>();
    assert!(
        files[0]
            .syntax
            .has_source_syntax_fact(SourceSyntaxFact::TemplateExpressionIdentifier)
    );
    let analysis = CapabilityAnalysis::derive(
        &files,
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );

    for (file, (_, source)) in files.iter().zip(sources) {
        let offset = source.rfind(';').expect("trailing reference") as u32 - 1;
        for target in [
            CapabilityTarget::References,
            CapabilityTarget::Highlights,
            CapabilityTarget::Rename,
        ] {
            assert!(
                !analysis.navigation_query_is_claimed(target, file, offset),
                "{}: {target:?} requires the complete program symbol set",
                file.source.path.display(),
            );
        }
        for target in [CapabilityTarget::QuickInfo, CapabilityTarget::Definition] {
            assert!(
                analysis.navigation_query_is_claimed(target, file, offset),
                "{}: {target:?} remains independently claimable",
                file.source.path.display(),
            );
        }
    }
}

#[test]
fn strict_dependent_deferred_boolean_values_use_the_effective_strict_default() {
    let file = program_file(0, "options.ts", "export const value = 1;");
    for option in [
        DeferredCompilerOption::NoImplicitThis,
        DeferredCompilerOption::StrictBindCallApply,
        DeferredCompilerOption::StrictFunctionTypes,
        DeferredCompilerOption::UseUnknownInCatchVariables,
    ] {
        for strict in [false, true] {
            for authored_value in [None, Some(false), Some(true)] {
                let mut options = CompilerOptions {
                    strict,
                    ..CompilerOptions::default()
                };
                if let Some(value) = authored_value {
                    options.deferred_options.insert(
                        option,
                        super::super::DeferredCompilerOptionValue::Boolean(value),
                    );
                }
                let analysis = CapabilityAnalysis::derive(
                    std::slice::from_ref(&file),
                    &options,
                    CapabilityContext::default(),
                );
                let expected_nonclaim =
                    matches!(authored_value, Some(true)) || strict && authored_value == Some(false);

                for &target in &SEMANTIC_TYPE_TARGETS {
                    assert_eq!(
                        analysis
                            .claim(target, CapabilityScope::Program)
                            .is_claimed(),
                        !expected_nonclaim,
                        "{option:?}, strict={strict}, authored={authored_value:?}, {target:?}",
                    );
                }
                if expected_nonclaim {
                    let CapabilityClaim::Nonclaimed(reasons) =
                        analysis.claim(CapabilityTarget::SemanticCheck, CapabilityScope::Program)
                    else {
                        panic!("{option:?} must carry its typed program nonclaim");
                    };
                    assert!(reasons.into_iter().any(|reason| {
                        reason.scope == CapabilityScope::Program
                            && reason.reason == NonclaimReason::DeferredCompilerOption(option)
                            && reason.deletion_condition() == DeletionCondition::CompilerOptionOwner
                    }));
                }
                for target in [
                    CapabilityTarget::JavaScript,
                    CapabilityTarget::Definition,
                    CapabilityTarget::References,
                    CapabilityTarget::Highlights,
                    CapabilityTarget::Rename,
                    CapabilityTarget::SyntacticDiagnostics,
                ] {
                    assert!(
                        analysis
                            .claim(target, CapabilityScope::Program)
                            .is_claimed(),
                        "{option:?}, strict={strict}, authored={authored_value:?}, {target:?}",
                    );
                }
            }
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
        analysis
            .claim(
                CapabilityTarget::RequiredType,
                CapabilityScope::required_function_like(file.source.id, statement.id),
            )
            .is_claimed()
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
                    && record.deletion_condition() == DeletionCondition::SyntaxOwner(expected)
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
                && record.deletion_condition()
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
        }

        let root = output.program.files[0]
            .bindings
            .declarations
            .iter()
            .find(|declaration| {
                declaration.kind == DeclarationKind::Variable && declaration.name == "renamedRoot"
            })
            .expect("root declaration");
        let root_offset = root.name_span.start;
        for &target in &ALL_TARGETS[7..] {
            assert!(
                output.capabilities.navigation_query_is_claimed(
                    target,
                    &output.program.files[0],
                    root_offset,
                ),
                "checker display completion must not mutate the structural {target:?} claim",
            );
        }
        assert!(
            navigation_identity_nonclaim(
                &output.capabilities,
                CapabilityTarget::QuickInfo,
                &output.program.files[0],
                root_offset,
            )
            .is_none()
        );
    }
}

#[test]
fn navigation_query_ranges_are_owned_once_by_capability_analysis() {
    let source = concat!(
        "const boundName = 1; boundName; missingName; ",
        "interface Shape { defaultMember: string; 'quotedMember': number; 77: boolean; } ",
        "class Box { #privateMember = 1; defaultClass = 1; 'quotedClass' = 1; 88 = 1; } ",
        "const holder = { propertyName: 1, 'quotedObject': 1, 99: 1 }; ",
        "'stringName'; // commentName\n",
    );
    let file = program_file(0, "navigation.ts", source);
    let analysis = default_analysis(&file);
    let declaration = source.find("boundName").unwrap() as u32;
    let reference = source.rfind("boundName").unwrap() as u32;
    let unresolved = source.find("missingName").unwrap() as u32;
    let unmodeled_names = [
        "defaultMember",
        "'quotedMember'",
        "77",
        "#privateMember",
        "defaultClass",
        "'quotedClass'",
        "88",
        "propertyName",
        "'quotedObject'",
        "99",
    ]
    .map(|name| {
        let start = source.find(name).unwrap() as u32;
        (start, start + name.len() as u32)
    });

    for &target in &ALL_TARGETS[7..] {
        for offset in [
            declaration,
            declaration + "boundName".len() as u32,
            reference + 3,
            unresolved + "missingName".len() as u32,
        ] {
            assert!(
                analysis.navigation_query_is_claimed(target, &file, offset),
                "bound and unresolved binder identities are definitive for {target:?} at {offset}",
            );
            assert!(
                navigation_identity_nonclaim(&analysis, target, &file, offset).is_none(),
                "modeled identifier must have no range nonclaim",
            );
        }

        for (start, end) in unmodeled_names {
            for offset in [start, start + (end - start) / 2, end] {
                let record = navigation_identity_nonclaim(&analysis, target, &file, offset)
                    .unwrap_or_else(|| {
                        panic!(
                            "unmodeled touching-name producer {:?} at {start}..{end} for {target:?}",
                            &source[start as usize..end as usize],
                        )
                    });
                assert_eq!(record.target, target);
                assert_eq!(
                    record.scope,
                    CapabilityScope::Span {
                        file: file.source.id,
                        start,
                        end,
                    }
                );
                assert_eq!(
                    record.reason,
                    NonclaimReason::Semantic(SemanticGap::NavigationIdentity)
                );
                assert_eq!(
                    record.deletion_condition(),
                    DeletionCondition::SemanticOwner(SemanticGap::NavigationIdentity)
                );
            }
            assert!(
                navigation_identity_nonclaim(&analysis, target, &file, end + 1).is_none(),
                "the temporary nonclaim ends with the authored name",
            );
        }
    }

    for offset in [
        source.find("stringName").unwrap() as u32 + 2,
        source.find("commentName").unwrap() as u32 + 2,
        source.find('=').unwrap() as u32,
        source.find(';').unwrap() as u32,
    ] {
        assert!(analysis.navigation_query_is_claimed(CapabilityTarget::Definition, &file, offset,));
    }
}

#[test]
fn checker_quick_info_completion_is_not_mirrored_into_capability_analysis() {
    let source =
        "type Box<TypeValue> = TypeValue; function useValue(value: number) { return value; }";
    let file = program_file(0, "quick-info-summary.ts", source);
    let incomplete = file
        .bindings
        .declarations
        .iter()
        .filter(|declaration| {
            matches!(
                declaration.kind,
                DeclarationKind::Parameter | DeclarationKind::TypeParameter
            )
        })
        .map(|declaration| declaration.id)
        .collect::<BTreeSet<_>>();
    assert_eq!(incomplete.len(), 2);
    let analysis = default_analysis(&file);

    for declaration in file
        .bindings
        .declarations
        .iter()
        .filter(|declaration| incomplete.contains(&declaration.id))
    {
        assert!(CapabilityAnalysis::navigation_declaration_has_identity(
            declaration
        ));
        for offset in [declaration.name_span.start, declaration.name_span.end] {
            assert!(
                navigation_identity_nonclaim(
                    &analysis,
                    CapabilityTarget::QuickInfo,
                    &file,
                    offset,
                )
                .is_none()
            );
            for &target in &ALL_TARGETS[7..] {
                assert!(
                    analysis.navigation_query_is_claimed(target, &file, offset),
                    "checker completion must stay outside the immutable {target:?} analysis",
                );
            }
        }
    }
}
