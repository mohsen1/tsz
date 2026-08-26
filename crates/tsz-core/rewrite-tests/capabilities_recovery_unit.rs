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
                && reason.reason == NonclaimReason::Syntax(SyntaxGap::GeneratorFunctionLike)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        generator,
        vec![CapabilityNonclaim {
            target: CapabilityTarget::SemanticCheck,
            scope,
            reason: NonclaimReason::Syntax(SyntaxGap::GeneratorFunctionLike),
            deletion: DeletionCondition::DeepestSemanticOwner(SyntaxGap::GeneratorFunctionLike,),
        }],
    );
    assert!(analysis.semantic_check_node_allows_claimed_descendants(file.source.id, owner));
    assert!(!analysis.semantic_check_node_allows_recovery_identifiers(file.source.id, owner));
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
            && reason.reason == NonclaimReason::Syntax(SyntaxGap::GeneratorFunctionLike)
            && reason.deletion
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
        .parser_recovery_facts()
        .iter()
        .find(|recovery| recovery.authored_span.start == 0)
        .expect("opening JSX recovery");
    assert_eq!(opening.kind, ParserRecoveryKind::Expression);
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
            Some(&RecoveryStatementRole::RepresentationalFragment),
            "{marker}: {:#?}",
            file.syntax.statements,
        );
    }
}
