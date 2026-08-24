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
    let owner = statement_starting_at(&file, source, "items =");
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
    let mut function_owner = None;
    for_each_statement_in(&file.syntax.statements, &mut |statement| {
        if statement.id == owner
            && let StatementKind::Expression(expression) = &statement.kind
            && let ExpressionKind::Assignment { right, .. } = &expression.kind
            && matches!(right.kind, ExpressionKind::FunctionLike(_))
        {
            function_owner = Some(right.id);
        }
    });
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
