use super::*;

#[test]
fn recovered_for_binding_values_are_not_claimed_without_the_iterable_producer() {
    let source = "for (const { value: renamed } of values) { renamed; MissingLoopBody; }";
    let file = program_file(0, "for-binding.ts", source);
    let renamed = file
        .bindings
        .declarations
        .iter()
        .find(|declaration| declaration.name == "renamed")
        .expect("recovered for binding");
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    assert!(
        !analysis.semantic_declaration_is_claimed(std::slice::from_ref(&file), renamed.id),
        "a represented binding cannot fabricate a complete value when its iterable is omitted",
    );
}

#[test]
fn recovered_generator_declarator_keeps_its_exact_typed_owner() {
    let source = concat!(
        "let seed = 0, items = function* renamedItems() {",
        "for (const { value: changed } of this) { yield changed; }",
        "}, kept = 1;",
    );
    let file = program_file(0, "generator-declarator.ts", source);
    let [statement] = file.syntax.statements.as_slice() else {
        panic!(
            "one variable statement expected: {:#?}",
            file.syntax.statements
        )
    };
    let StatementKind::Variable(variable) = &statement.kind else {
        panic!("variable statement expected: {statement:#?}")
    };
    let items = variable
        .declarators
        .iter()
        .find(|declaration| declaration.name == "items")
        .expect("items declarator");
    let owner = statement.id;
    let scope = CapabilityScope::node(file.source.id, owner);
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    let CapabilityClaim::Nonclaimed(reasons) =
        analysis.claim(CapabilityTarget::SemanticCheck, scope)
    else {
        panic!("the recovered generator declarator must be nonclaimed");
    };
    let generator = reasons
        .copied()
        .filter(|reason| {
            reason.scope == scope
                && reason.reason
                    == NonclaimReason::SyntaxAtSemanticOwner(SyntaxGap::GeneratorFunctionLike)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        generator,
        vec![CapabilityNonclaim {
            target: CapabilityTarget::SemanticCheck,
            scope,
            reason: NonclaimReason::SyntaxAtSemanticOwner(SyntaxGap::GeneratorFunctionLike),
        }],
    );
    assert!(semantic_descendant_is_claimed(
        &analysis,
        file.source.id,
        owner,
        false,
    ));
    assert!(!semantic_descendant_is_claimed(
        &analysis,
        file.source.id,
        owner,
        true,
    ));
    assert!(
        analysis
            .claim(CapabilityTarget::DeclarationModel, scope)
            .is_claimed()
    );
    let function_owner = items
        .initializer
        .as_ref()
        .filter(|initializer| matches!(initializer.kind, ExpressionKind::FunctionLike(_)))
        .map(|initializer| initializer.id);
    let function_scope = CapabilityScope::node(
        file.source.id,
        function_owner.expect("represented generator expression"),
    );
    assert!(
        analysis
            .claim(CapabilityTarget::DeclarationModel, function_scope)
            .is_claimed()
    );
    let CapabilityClaim::Nonclaimed(value_reasons) =
        analysis.claim(CapabilityTarget::DeclarationValue, function_scope)
    else {
        panic!("the recovered generator value must remain nonclaimed");
    };
    assert!(value_reasons.into_iter().any(|reason| {
        reason.scope == function_scope
            && reason.reason
                == NonclaimReason::SyntaxAtSemanticOwner(SyntaxGap::GeneratorFunctionLike)
            && reason.deletion_condition()
                == DeletionCondition::DeepestSemanticOwner(SyntaxGap::GeneratorFunctionLike)
    }));
}

#[test]
fn jsx_text_and_entity_fragments_share_the_opening_expression_recovery_owner() {
    let source = concat!(
        "<section>Be cautious of &quot;-tail!</section>;\n",
        "const kept: MissingSibling = 1;\n",
    );
    let file = program_file(0, "jsx-recovery.tsx", source);
    let opening = file
        .syntax
        .parser_recovery_facts
        .iter()
        .find(|recovery| recovery.authored_span.start == 0)
        .expect("opening JSX recovery");
    assert_eq!(opening.kind, ParserRecoveryKind::MissingExpression);
    assert_eq!(
        opening.recovery_extent,
        Span::new(file.source.id, 0, source.lines().next().unwrap().len()),
    );

    let nodes = recovery_nodes(
        &file,
        opening.owner,
        opening.authored_span,
        opening.recovery_extent,
    );
    for marker in ["cautious", "of", "quot", "tail"] {
        let start = source.find(marker).expect("JSX fragment") as u32;
        let statement = file
            .syntax
            .statements
            .iter()
            .find(|statement| statement.span.start <= start && start < statement.span.end)
            .expect("represented JSX fragment");
        assert_eq!(
            nodes.owners.get(&statement.id),
            Some(&RecoveryRole::RepresentationalFragment),
            "{marker}: {:#?}",
            file.syntax.statements,
        );
    }
}

#[test]
fn missing_conditional_expression_hosts_have_one_local_javascript_fence() {
    // TypeScript 7 does not synthesize `void 0` for an absent conditional
    // branch. Until emit owns that recovery transform, immutable capability
    // analysis withholds only the JavaScript product that consumes the fact.
    for (source, kind) in [
        (
            "const renamed = flag ? : 1; const independent = 2;",
            ParserRecoveryKind::MissingExpression,
        ),
        (
            "const renamed = flag ? 1 : ; const independent = 2;",
            ParserRecoveryKind::MissingExpression,
        ),
        (
            "const renamed = flag ? 1 2; const independent = 3;",
            ParserRecoveryKind::ConditionalExpression,
        ),
        (
            "consume(flag ? : 1); const independent = 2;",
            ParserRecoveryKind::MissingExpression,
        ),
        (
            "[flag ? 1 : ]; const independent = 2;",
            ParserRecoveryKind::MissingExpression,
        ),
        (
            "({ value: flag ? : 1 }); const independent = 2;",
            ParserRecoveryKind::MissingExpression,
        ),
    ] {
        let file = program_file(0, "conditional-recovery.ts", source);
        let recovery = file
            .syntax
            .parser_recovery_facts
            .iter()
            .find(|fact| fact.kind == kind)
            .unwrap_or_else(|| panic!("missing {kind:?} fact for {source}"));
        let scope = CapabilityScope::node(file.source.id, recovery.owner.statement);
        let analysis = default_analysis(&file);
        let CapabilityClaim::Nonclaimed(reasons) =
            analysis.claim(CapabilityTarget::JavaScript, scope)
        else {
            panic!("recovered JavaScript owner was claimed for {source}")
        };
        assert_eq!(
            reasons.copied().collect::<Vec<_>>(),
            [CapabilityNonclaim {
                target: CapabilityTarget::JavaScript,
                scope,
                reason: NonclaimReason::Syntax(SyntaxGap::Expression),
            }],
            "{source}",
        );
        let independent = file
            .syntax
            .statements
            .last()
            .expect("independent statement");
        assert_ne!(independent.id, recovery.owner.statement, "{source}");
        assert!(
            analysis
                .claim(
                    CapabilityTarget::JavaScript,
                    CapabilityScope::node(file.source.id, independent.id),
                )
                .is_claimed(),
            "independent same-file JavaScript was withheld for {source}",
        );
        assert!(!analysis.product_is_claimed(
            CapabilityTarget::JavaScript,
            CapabilityScope::File(file.source.id),
            &CompilerOptions::default(),
        ));
    }
}
