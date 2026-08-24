use std::path::PathBuf;
use std::sync::Arc;

use crate::bind::bind_source;
use crate::source::{FileId, SourceText};
use crate::syntax::{ClassMemberKind, TypeNodeKind, parse_source};

use super::*;

#[path = "../../../rewrite-tests/capabilities_emit_unit.rs"]
mod emit;

#[path = "../../../rewrite-tests/capabilities_recovery_unit.rs"]
mod recovery;

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

fn parser_recovery_statement_roles(
    file: &ProgramFile,
    recovery: &crate::syntax::ParserRecoveryFact,
    recovery_extent: Span,
) -> BTreeMap<NodeId, RecoveryStatementRole> {
    recovery_statement_owners(
        file,
        recovery.owner,
        recovery.authored_span,
        recovery_extent,
        RecoveryStatementSource::Parser {
            recovered_declarator_initializers: &recovered_declarator_initializer_owners(file),
        },
    )
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
fn typed_region_reason_does_not_poison_a_sibling_statement() {
    let file = program_file(
        0,
        "mixed.ts",
        "const gap = `plain`; const sibling: string = missingOwned;",
    );
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    let [gap, sibling] = file.syntax.statements.as_slice() else {
        panic!("two statements expected");
    };

    let CapabilityClaim::Nonclaimed(reasons) = analysis.claim(
        CapabilityTarget::SemanticCheck,
        CapabilityScope::node(file.source.id, gap.id),
    ) else {
        panic!("template statement must be a typed nonclaim");
    };
    let reasons = reasons.copied().collect::<Vec<_>>();
    assert_eq!(reasons.len(), 1);
    assert_eq!(
        reasons[0].reason,
        NonclaimReason::Syntax(SyntaxGap::Template)
    );
    assert_eq!(
        reasons[0].deletion,
        DeletionCondition::DeepestSemanticOwner(SyntaxGap::Template)
    );
    assert_eq!(
        reasons[0].scope,
        CapabilityScope::node(file.source.id, gap.id)
    );
    assert!(
        analysis
            .claim(
                CapabilityTarget::SemanticCheck,
                CapabilityScope::node(file.source.id, sibling.id),
            )
            .is_claimed()
    );
    assert!(
        !analysis.semantic_diagnostics_file_is_claimed(file.source.id),
        "a file-level diagnostics request aggregates its region nonclaims"
    );
}

#[test]
fn nested_parser_recovery_owns_the_smallest_statement() {
    let source = concat!(
        "function wrapper() {\n",
        "  const gap: string = `head${\"value\"}tail`;\n",
        "  const kept: MissingInside = 1;\n",
        "}\n",
    );
    let file = program_file(0, "nested.ts", source);
    let StatementKind::Function(function) = &file.syntax.statements[0].kind else {
        panic!("function expected: {:#?}", file.syntax.statements);
    };
    let gap = &function.body[0];
    let kept = function.body.last().expect("kept nested statement");
    assert!(
        file.syntax.parser_recovery_facts().iter().all(|fact| {
            fact.owner.root_statement == file.syntax.statements[0].id
                && function
                    .body
                    .iter()
                    .take(function.body.len() - 1)
                    .any(|statement| statement.id == fact.owner.statement)
                && fact.recovery_extent.end <= kept.span.start
        }),
        "gap={gap:#?}, kept={kept:#?}, facts={:#?}",
        file.syntax.parser_recovery_facts(),
    );
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    assert!(
        analysis
            .claim(
                CapabilityTarget::SemanticCheck,
                CapabilityScope::node(file.source.id, kept.id),
            )
            .is_claimed(),
        "kept statement must remain claimed: {:#?}",
        analysis.nonclaims,
    );
}

#[test]
fn recovered_binding_pattern_extent_includes_declarator_continuations() {
    for (source, dependent, expected_role, allows_identifiers) in [
        (
            "let {} = missingPattern; const outside = 1;",
            "missingPattern",
            RecoveryStatementRole::SemanticOwner,
            false,
        ),
        (
            "let {}, recovered = missingComma; const outside = 1;",
            "missingComma",
            RecoveryStatementRole::RecoveredDeclaratorInitializer,
            true,
        ),
        (
            concat!(
                "const callback = function () { let {} = missingNested; ",
                "MissingBody; }; const outside = 1;",
            ),
            "missingNested",
            RecoveryStatementRole::SemanticOwner,
            false,
        ),
    ] {
        let file = program_file(0, "binding-pattern.ts", source);
        let recovery = file
            .syntax
            .parser_recovery_facts()
            .iter()
            .find(|fact| fact.kind == ParserRecoveryKind::Declaration)
            .expect("binding-pattern recovery fact");
        let dependent_end = source.find(dependent).expect("dependent tail") + dependent.len();
        let outside_start = source.find("const outside").expect("closed sibling");
        assert!(recovery.recovery_extent.end >= dependent_end as u32);
        assert!(recovery.recovery_extent.end <= outside_start as u32);

        let dependent_start = source.find(dependent).expect("dependent tail") as u32;
        let dependent_end = dependent_start + dependent.len() as u32;
        let mut dependent_owner = None;
        for_each_statement_in(&file.syntax.statements, &mut |statement| {
            if statement.span.start <= dependent_start
                && dependent_end <= statement.span.end
                && dependent_owner.is_none_or(|(width, _)| statement.span.len() < width)
            {
                dependent_owner = Some((statement.span.len(), statement.id));
            }
        });
        let (_, dependent_owner) = dependent_owner.expect("represented dependent tail");
        let roles = parser_recovery_statement_roles(&file, recovery, recovery.recovery_extent);
        assert_eq!(roles.get(&dependent_owner), Some(&expected_role),);
        let analysis = CapabilityAnalysis::derive(
            std::slice::from_ref(&file),
            &CompilerOptions::default(),
            CapabilityContext::default(),
        );
        assert!(!analysis.semantic_check_node_is_claimed(file.source.id, dependent_owner));
        assert_eq!(
            analysis
                .semantic_check_node_allows_recovery_identifiers(file.source.id, dependent_owner,),
            allows_identifiers,
        );
    }
}

#[test]
fn recovery_statement_roles_separate_real_subtrees_from_flat_fragments() {
    let object_source = concat!(
        "const value = { kept: function () { MissingOwnedBody; }, ",
        "method() { MissingTail; } };",
    );
    let object = program_file(0, "object-member-fragment.ts", object_source);
    let object_recovery = object
        .syntax
        .parser_recovery_facts()
        .iter()
        .find(|fact| fact.authored_span.start == object_source.find("method").unwrap() as u32)
        .expect("object member recovery fact");
    let object_roles =
        parser_recovery_statement_roles(&object, object_recovery, object_recovery.recovery_extent);
    let body = statement_starting_at(&object, object_source, "MissingOwnedBody");
    let tail = statement_starting_at(&object, object_source, "MissingTail");
    assert_eq!(
        object_recovery.recovery_extent.start,
        object_recovery.authored_span.start,
    );
    assert_eq!(object_roles.get(&body), None);
    assert_eq!(
        object_roles.get(&tail),
        Some(&RecoveryStatementRole::RepresentationalFragment),
    );

    let spread_source = "const value = { ...(function () { MissingSpreadBody; }) };";
    let spread = program_file(0, "object-spread-fragment.ts", spread_source);
    let spread_recovery = spread
        .syntax
        .parser_recovery_facts()
        .iter()
        .find(|fact| fact.authored_span.start == spread_source.find("...").unwrap() as u32)
        .expect("object spread recovery fact");
    let spread_roles =
        parser_recovery_statement_roles(&spread, spread_recovery, spread_recovery.recovery_extent);
    let spread_body = statement_starting_at(&spread, spread_source, "MissingSpreadBody");
    assert_eq!(
        spread_roles.get(&spread_body),
        None,
        "recovery outside a FunctionLike cannot absorb its body owner",
    );

    let arrow_source = "const value = { ...(() => { MissingArrowBody; }) };";
    let arrow = program_file(0, "arrow-spread-fragment.ts", arrow_source);
    let arrow_recovery = arrow
        .syntax
        .parser_recovery_facts()
        .iter()
        .find(|fact| fact.authored_span.start == arrow_source.find("...").unwrap() as u32)
        .expect("arrow spread recovery fact");
    let arrow_roles =
        parser_recovery_statement_roles(&arrow, arrow_recovery, arrow_recovery.recovery_extent);
    let arrow_body = statement_starting_at(&arrow, arrow_source, "MissingArrowBody");
    assert_eq!(
        arrow_roles.get(&arrow_body),
        None,
        "recovery outside an arrow cannot absorb its body owner",
    );

    let header_source = "const value = function broken(@) { MissingRecoveredBody; };";
    let header = program_file(0, "recovered-function-header.ts", header_source);
    let header_recovery = header
        .syntax
        .parser_recovery_facts()
        .iter()
        .find(|fact| fact.authored_span.start == header_source.find("function").unwrap() as u32)
        .expect("function header recovery fact");
    let header_roles =
        parser_recovery_statement_roles(&header, header_recovery, header_recovery.recovery_extent);
    let recovered_body = statement_starting_at(&header, header_source, "MissingRecoveredBody");
    assert_eq!(
        header_roles.get(&recovered_body),
        Some(&RecoveryStatementRole::SemanticOwner),
        "a FunctionLike containing its own recovery remains closed",
    );

    let template = program_file(
        0,
        "template-fragment.ts",
        "const tag: any = 0; tag `x`.escaped; const kept = 1;",
    );
    let template_recovery = template
        .syntax
        .parser_recovery_facts()
        .iter()
        .find(|fact| fact.kind == ParserRecoveryKind::Template)
        .expect("template recovery fact");
    let template_roles = parser_recovery_statement_roles(
        &template,
        template_recovery,
        template_recovery.recovery_extent,
    );
    assert_eq!(
        template_roles.get(&template_recovery.owner.statement),
        Some(&RecoveryStatementRole::SemanticOwner),
    );
    assert!(
        template_roles
            .values()
            .any(|role| *role == RecoveryStatementRole::RepresentationalFragment),
        "the reparsed property tail must not become a semantic owner: {template_roles:#?}",
    );
    let template_fragment = template_roles
        .iter()
        .find_map(|(owner, role)| {
            (*role == RecoveryStatementRole::RepresentationalFragment).then_some(*owner)
        })
        .expect("represented template fragment");
    let template_analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&template),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    assert!(
        template_analysis
            .semantic_check_node_allows_claimed_descendants(template.source.id, template_fragment,),
        "mixed exact recovery may discover independently claimed nested owners",
    );
    assert!(
        !template_analysis.semantic_check_node_allows_recovery_identifiers(
            template.source.id,
            template_fragment,
        ),
        "a represented recovery fragment cannot publish direct names",
    );

    let declaration_tail = program_file(
        0,
        "declaration-fragment.ts",
        "const tag: any = 0; tag `x` const leaked = 1; const closed = 1;",
    );
    let declaration_tail_recovery = declaration_tail
        .syntax
        .parser_recovery_facts()
        .iter()
        .find(|fact| fact.kind == ParserRecoveryKind::Template)
        .expect("template recovery fact with declaration tail");
    let declaration_tail_roles = parser_recovery_statement_roles(
        &declaration_tail,
        declaration_tail_recovery,
        declaration_tail_recovery.recovery_extent,
    );
    let mut leaked = None;
    let mut closed = None;
    for statement in &declaration_tail.syntax.statements {
        statement.for_each_statement(&mut |statement| {
            if let StatementKind::Variable(declaration) = &statement.kind {
                match declaration.name.as_str() {
                    "leaked" => leaked = Some(statement.id),
                    "closed" => closed = Some(statement.id),
                    _ => {}
                }
            }
        });
    }
    let leaked = leaked.expect("represented declaration tail");
    let closed = closed.expect("closed declaration sibling");
    assert_eq!(
        declaration_tail_roles.get(&leaked),
        Some(&RecoveryStatementRole::RepresentationalFragment),
    );
    let declaration_tail_analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&declaration_tail),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    for target in [
        CapabilityTarget::DeclarationModel,
        CapabilityTarget::DeclarationValue,
    ] {
        assert!(
            !declaration_tail_analysis
                .claim(
                    target,
                    CapabilityScope::node(declaration_tail.source.id, leaked),
                )
                .is_claimed(),
            "a flat declaration fragment cannot publish a semantic model",
        );
        assert!(
            declaration_tail_analysis
                .claim(
                    target,
                    CapabilityScope::node(declaration_tail.source.id, closed),
                )
                .is_claimed(),
            "the closed declaration sibling remains definitive",
        );
    }

    let signature = program_file(
        0,
        "signature-subtree.ts",
        "function wrapper(value: ) { const hidden: string = value; }",
    );
    let root = &signature.syntax.statements[0];
    let StatementKind::Function(function) = &root.kind else {
        panic!("function expected");
    };
    let hidden = &function.body[0];
    let signature_recovery = signature
        .syntax
        .parser_recovery_facts()
        .iter()
        .find(|fact| fact.owner.statement == root.id)
        .expect("signature recovery fact");
    let signature_roles = parser_recovery_statement_roles(
        &signature,
        signature_recovery,
        Span {
            end: root.span.end,
            ..signature_recovery.recovery_extent
        },
    );
    assert_eq!(
        signature_roles.get(&hidden.id),
        Some(&RecoveryStatementRole::SemanticOwner),
        "a represented statement in the real owner subtree keeps semantic identity",
    );

    let variable_list = program_file(
        0,
        "initializer-fragment.ts",
        "let invoke: any, first = invoke(1, nestedOnly), last = 'owned';",
    );
    let variable_recovery = variable_list
        .syntax
        .parser_recovery_facts()
        .iter()
        .find(|fact| fact.kind == ParserRecoveryKind::Declaration)
        .expect("variable-list recovery fact");
    let variable_roles = parser_recovery_statement_roles(
        &variable_list,
        variable_recovery,
        variable_recovery.recovery_extent,
    );
    let attached_initializer = variable_list
        .syntax
        .statements
        .iter()
        .find(|statement| {
            let StatementKind::Expression(expression) = &statement.kind else {
                return false;
            };
            let ExpressionKind::Assignment { left, .. } = &expression.kind else {
                return false;
            };
            left.span.start
                == variable_list
                    .source
                    .text
                    .find("first")
                    .expect("recovered declarator") as u32
        })
        .expect("represented recovered initializer assignment");
    assert_eq!(
        variable_roles.get(&attached_initializer.id),
        Some(&RecoveryStatementRole::RecoveredDeclaratorInitializer),
        "a recovered declarator assignment retains its represented RHS semantics",
    );
    let literal_roles = recovery_statement_owners(
        &variable_list,
        variable_recovery.owner,
        variable_recovery.authored_span,
        variable_recovery.recovery_extent,
        RecoveryStatementSource::Literal,
    );
    assert_eq!(
        literal_roles.get(&attached_initializer.id),
        Some(&RecoveryStatementRole::RepresentationalFragment),
        "authored-literal recovery cannot inherit the parser-only declarator exception",
    );
    let variable_analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&variable_list),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    assert!(
        variable_analysis.semantic_check_node_allows_claimed_descendants(
            variable_list.source.id,
            attached_initializer.id,
        ),
        "overlapping recovery facts must agree on recovered initializer semantics; facts={:#?}, nonclaims={:#?}",
        variable_list.syntax.parser_recovery_facts(),
        variable_analysis.nonclaims,
    );
    assert!(
        variable_analysis.semantic_check_node_allows_recovery_identifiers(
            variable_list.source.id,
            attached_initializer.id,
        ),
        "only a parser-identified recovered initializer may publish its RHS names",
    );

    let generic_source = "let actual = invoke<A, B, C>(), kept = 1;";
    let generic_commas = program_file(0, "generic-commas.ts", generic_source);
    let generic_initializers = recovered_declarator_initializer_owners(&generic_commas);
    let comma_only_argument_spans = ["B", "C"].map(|name| {
        let start = generic_source.find(name).expect("generic type argument");
        Span::new(generic_commas.source.id, start, start + name.len())
    });
    let mut initializer_left_spans = Vec::new();
    for statement in &generic_commas.syntax.statements {
        statement.for_each_statement(&mut |statement| {
            if generic_initializers.contains(&statement.id)
                && let StatementKind::Expression(expression) = &statement.kind
                && let ExpressionKind::Assignment { left, .. } = &expression.kind
            {
                initializer_left_spans.push(left.span);
            }
        });
    }
    assert!(
        !initializer_left_spans.is_empty(),
        "the real trailing declarator must retain initializer semantics",
    );
    assert!(
        comma_only_argument_spans.iter().all(|argument_span| {
            !initializer_left_spans
                .iter()
                .any(|left_span| left_span == argument_span)
        }),
        "generic-call comma arguments without exact assignment LHS spans cannot gain initializer semantics",
    );
}

#[test]
fn flow_contained_recovery_fragments_allow_name_only_descendant_discovery() {
    let source = concat!(
        "function shell(value:string|number){\n",
        "  if(MissingBefore&&`head${\"gap\"}tail`&&MissingAfter){\n",
        "    MissingThen;\n",
        "  }else{MissingElse;}\n",
        "}\n",
    );
    let file = program_file(0, "flow-recovery-fragments.ts", source);
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );

    for name in ["MissingAfter", "MissingThen", "MissingElse"] {
        let start = source.find(name).expect("name witness") as u32;
        let end = start + name.len() as u32;
        let mut mixed_owner_count = 0;
        for root in &file.syntax.statements {
            root.for_each_statement(&mut |statement| {
                if statement.span.start > start || end > statement.span.end {
                    return;
                }
                let CapabilityClaim::Nonclaimed(reasons) = analysis.claim(
                    CapabilityTarget::SemanticCheck,
                    CapabilityScope::node(file.source.id, statement.id),
                ) else {
                    return;
                };
                let reasons = reasons.collect::<Vec<_>>();
                let has_recovery = reasons.iter().any(|reason| {
                    matches!(
                        reason.deletion,
                        DeletionCondition::DeepestSemanticOwner(_)
                            | DeletionCondition::SyntaxOwner(_)
                    )
                });
                let has_flow_region = reasons.iter().any(|reason| {
                    reason.deletion
                        == DeletionCondition::SemanticOwner(SemanticGap::FlowTypeOfReference)
                });
                if has_recovery && has_flow_region {
                    mixed_owner_count += 1;
                    assert!(
                        analysis.semantic_check_node_allows_claimed_descendants(
                            file.source.id,
                            statement.id,
                        ),
                        "{name} must remain discoverable below {statement:#?}: {reasons:#?}",
                    );
                    assert!(
                        analysis.semantic_check_node_allows_recovery_identifiers(
                            file.source.id,
                            statement.id,
                        ),
                        "{name} must retain name discovery below {statement:#?}: {reasons:#?}",
                    );
                }
            });
        }
        assert!(
            mixed_owner_count > 0,
            "no mixed recovery/flow owner for {name}"
        );
    }
}

#[test]
fn declaration_nonclaims_follow_direct_and_return_value_owners() {
    for (path, body) in [
        ("condition.ts", "if (`head${\"gap\"}tail`) {}"),
        ("expression.ts", "`head${\"gap\"}tail`;"),
    ] {
        let source = format!(
            "function shell(value: string | number) {{\n  const before: string = value;\n  {body}\n}}\n"
        );
        let file = program_file(0, path, &source);
        let analysis = CapabilityAnalysis::derive(
            std::slice::from_ref(&file),
            &CompilerOptions::default(),
            CapabilityContext::default(),
        );
        for name in ["shell", "value"] {
            let declaration = file
                .bindings
                .declarations
                .iter()
                .find(|declaration| declaration.name == name)
                .unwrap_or_else(|| panic!("bound declaration {name}"));
            assert!(
                analysis
                    .semantic_declaration_is_claimed(std::slice::from_ref(&file), declaration.id,)
            );
        }
    }

    for (path, source, name) in [
        (
            "direct.ts",
            "const direct = `head${\"gap\"}tail`;",
            "direct",
        ),
        (
            "return.ts",
            "function returned() { return `head${\"gap\"}tail`; }",
            "returned",
        ),
        (
            "arrow.ts",
            "const arrow = () => { return `head${\"gap\"}tail`; };",
            "arrow",
        ),
        (
            "class.ts",
            "class Holder { method() { return `head${\"gap\"}tail`; } }",
            "Holder",
        ),
    ] {
        let file = program_file(0, path, source);
        let analysis = CapabilityAnalysis::derive(
            std::slice::from_ref(&file),
            &CompilerOptions::default(),
            CapabilityContext::default(),
        );
        let declaration = file
            .bindings
            .declarations
            .iter()
            .find(|declaration| declaration.name == name)
            .unwrap_or_else(|| panic!("bound declaration {name}: {source}"));
        assert!(
            !analysis.semantic_declaration_is_claimed(std::slice::from_ref(&file), declaration.id,)
        );
    }

    let signature = program_file(0, "signature.ts", "function broken(value: ) {}");
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&signature),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    for name in ["broken", "value"] {
        let declaration = signature
            .bindings
            .declarations
            .iter()
            .find(|declaration| declaration.name == name)
            .unwrap_or_else(|| panic!("bound declaration {name}"));
        assert!(
            !analysis
                .semantic_declaration_is_claimed(std::slice::from_ref(&signature), declaration.id,)
        );
    }
}

#[test]
fn zero_width_eof_recovery_keeps_its_nested_statement_owner() {
    let source = concat!("function wrapper() {\n", "  type Broken =",);
    let file = program_file(0, "nested-eof.ts", source);
    let root = &file.syntax.statements[0];
    let StatementKind::Function(function) = &root.kind else {
        panic!("function expected: {root:#?}");
    };
    let nested = function.body.first().expect("nested recovered type alias");
    assert!(
        file.syntax.parser_recovery_facts().iter().any(|fact| {
            fact.authored_span.end == source.len() as u32
                && fact.owner.root_statement == root.id
                && fact.owner.statement == nested.id
        }),
        "root={root:#?}, nested={nested:#?}, facts={:#?}",
        file.syntax.parser_recovery_facts(),
    );
    let declaration = file
        .bindings
        .declarations
        .iter()
        .find(|declaration| declaration.name == "Broken")
        .expect("recovered type-alias declaration")
        .id;
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    assert!(!analysis.semantic_declaration_is_claimed(std::slice::from_ref(&file), declaration,));
}

#[test]
fn opaque_namespace_extent_nonclaims_every_recovered_root_fragment() {
    let file = program_file(
        0,
        "namespace.ts",
        concat!(
            "declare namespace Container {\n",
            "  class Shape { value: string; }\n",
            "  let current: Shape;\n",
            "  current = current;\n",
            "}\n",
            "const kept: MissingAfter = 1;\n",
        ),
    );
    let host = file
        .syntax
        .unmodeled_declaration_hosts()
        .first()
        .expect("namespace host fact");
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    let host_declaration = file
        .bindings
        .declarations
        .iter()
        .find(|declaration| declaration.name == "Container")
        .expect("opaque namespace declaration");
    assert!(
        !analysis
            .semantic_declaration_is_claimed(std::slice::from_ref(&file), host_declaration.id,),
        "host={host:#?}, declaration={host_declaration:#?}, statements={:#?}",
        file.syntax.statements,
    );
    let mut fragments = Vec::new();
    for root in &file.syntax.statements {
        root.for_each_statement(&mut |statement| {
            if host.recovery_extent.start <= statement.span.start
                && statement.span.start < host.recovery_extent.end
            {
                fragments.push(statement.id);
            }
        });
    }
    assert!(
        fragments.len() >= 4,
        "fragments={fragments:#?}, host={host:#?}"
    );
    for statement in fragments {
        let CapabilityClaim::Nonclaimed(reasons) = analysis.claim(
            CapabilityTarget::SemanticCheck,
            CapabilityScope::node(file.source.id, statement),
        ) else {
            panic!("fragment must be quarantined: {statement:#?}");
        };
        assert!(reasons.into_iter().any(|reason| {
            reason.reason == NonclaimReason::Syntax(SyntaxGap::DeclarationHost)
                && reason.deletion == DeletionCondition::SyntaxOwner(SyntaxGap::DeclarationHost)
        }));
    }
    let kept = file.syntax.statements.last().expect("following sibling");
    assert!(analysis.semantic_check_node_is_claimed(file.source.id, kept.id));
}

#[test]
fn recovery_separator_facts_are_orthogonal_and_records_are_deterministic() {
    let recovery = program_file(0, "a.ts", "const value = 1__0;");
    let template = program_file(1, "b.ts", "const text = `plain`;");
    let options = CompilerOptions::default();
    let forward = CapabilityAnalysis::derive(
        &[recovery.clone(), template.clone()],
        &options,
        CapabilityContext::default(),
    );
    let reverse = CapabilityAnalysis::derive(
        &[template, recovery.clone()],
        &options,
        CapabilityContext::default(),
    );
    assert_eq!(forward.nonclaims, reverse.nonclaims);
    assert!(forward.nonclaims.windows(2).all(|pair| pair[0] < pair[1]));

    let statement = &recovery.syntax.statements[0];
    let reasons = match forward.claim(
        CapabilityTarget::SemanticCheck,
        CapabilityScope::node(recovery.source.id, statement.id),
    ) {
        CapabilityClaim::Claimed => panic!("invalid separator recovery must defer"),
        CapabilityClaim::Nonclaimed(reasons) => reasons
            .map(|reason| (reason.reason, reason.deletion, reason.scope))
            .collect::<std::collections::BTreeSet<_>>(),
    };
    assert_eq!(
        reasons,
        std::collections::BTreeSet::from([
            (
                NonclaimReason::Syntax(SyntaxGap::NumericRecovery),
                DeletionCondition::DeepestSemanticOwner(SyntaxGap::NumericRecovery),
                CapabilityScope::node(recovery.source.id, statement.id),
            ),
            (
                NonclaimReason::Syntax(SyntaxGap::NumericSeparator),
                DeletionCondition::DeepestSemanticOwner(SyntaxGap::NumericSeparator),
                CapabilityScope::node(recovery.source.id, statement.id),
            ),
        ])
    );
}

#[test]
fn module_local_declaration_groups_inherit_one_typed_peer_nonclaim() {
    let file = program_file(
        0,
        "module.ts",
        concat!(
            "export {}; ",
            "function shared(value: string): string; ",
            "function shared(value: string) { return `plain`; }",
        ),
    );
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    let signature = &file.syntax.statements[1];
    for target in [
        CapabilityTarget::DeclarationModel,
        CapabilityTarget::Definition,
    ] {
        let CapabilityClaim::Nonclaimed(reasons) =
            analysis.claim(target, CapabilityScope::node(file.source.id, signature.id))
        else {
            panic!("the signature model must inherit its implementation peer nonclaim");
        };
        assert!(reasons.into_iter().any(|record| {
            record.reason == NonclaimReason::Syntax(SyntaxGap::Template)
                && record.deletion == DeletionCondition::DeepestSemanticOwner(SyntaxGap::Template)
        }));
    }
    assert!(
        analysis
            .claim(
                CapabilityTarget::SemanticCheck,
                CapabilityScope::node(file.source.id, signature.id),
            )
            .is_claimed(),
        "declaration-model closure must not suppress independent statement checking",
    );
}

#[test]
fn global_declaration_groups_inherit_nested_declaration_model_nonclaims() {
    let declared = program_file(0, "declared.ts", "function shared(value: string): string;");
    let implementation = program_file(
        1,
        "gap.ts",
        "function shared(value: string) { return `plain`; }",
    );
    let files = [declared, implementation];
    let analysis = CapabilityAnalysis::derive(
        &files,
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    for file in &files {
        let declaration = file.bindings.declarations[0].id;
        assert!(
            !analysis.semantic_declaration_is_claimed(&files, declaration),
            "{}: {:#?}",
            file.source.path.display(),
            analysis.nonclaims,
        );
    }
}

#[test]
fn every_temporary_owner_has_a_typed_deletion_condition() {
    let plain = program_file(0, "plain.ts", "const value = 1;");
    let missing = CapabilityAnalysis::derive(
        std::slice::from_ref(&plain),
        &CompilerOptions::default(),
        CapabilityContext {
            has_missing_essential_types: true,
            ..CapabilityContext::default()
        },
    );
    for target in [
        CapabilityTarget::SemanticCheck,
        CapabilityTarget::DeclarationModel,
        CapabilityTarget::DeclarationValue,
    ] {
        let CapabilityClaim::Nonclaimed(mut missing_reasons) =
            missing.claim(target, CapabilityScope::Program)
        else {
            panic!("missing essential library reason for {target:?}");
        };
        let missing_reason = *missing_reasons
            .next()
            .expect("missing essential library reason");
        assert_eq!(missing_reason.reason, NonclaimReason::MissingEssentialTypes);
        assert_eq!(
            missing_reason.deletion,
            DeletionCondition::EssentialLibraryUniverse
        );
    }
    assert!(
        missing
            .claim(
                CapabilityTarget::SemanticDiagnostics,
                CapabilityScope::Program,
            )
            .is_claimed(),
        "the closed TS2318 set is a definitive aggregate diagnostic product"
    );
    assert!(missing.semantic_diagnostics_are_claimed());
    assert!(missing.semantic_diagnostics_file_is_claimed(plain.source.id));

    let fatal = CapabilityAnalysis::derive(
        std::slice::from_ref(&plain),
        &CompilerOptions::default(),
        CapabilityContext {
            has_fatal_option_error: true,
            ..CapabilityContext::default()
        },
    );
    let CapabilityClaim::Nonclaimed(mut fatal_reasons) = fatal.claim(
        CapabilityTarget::SemanticDiagnostics,
        CapabilityScope::Program,
    ) else {
        panic!("fatal compiler options must still withhold semantic diagnostics");
    };
    let fatal_reason = fatal_reasons.next().expect("fatal option reason");
    assert_eq!(fatal_reason.reason, NonclaimReason::FatalCompilerOption);
    assert_eq!(
        fatal_reason.deletion,
        DeletionCondition::CompilerOptionOwner
    );

    let template = program_file(0, "template.ts", "const value = `plain`;");
    let options = CompilerOptions::default();
    let literal = CapabilityAnalysis::derive(
        std::slice::from_ref(&template),
        &options,
        CapabilityContext::default(),
    );
    assert!(literal.nonclaims.iter().any(|record| {
        record.reason
            == NonclaimReason::ProgramLiteralBoundary(ProgramLiteralFamily::NoSubstitutionTemplate)
            && record.deletion
                == DeletionCondition::LiteralProgramOwner(
                    ProgramLiteralFamily::NoSubstitutionTemplate,
                )
    }));

    let option = CapabilityAnalysis::derive(
        std::slice::from_ref(&template),
        &options,
        CapabilityContext {
            has_compiler_option_error: true,
            ..CapabilityContext::default()
        },
    );
    assert!(option.nonclaims.iter().any(|record| {
        record.reason == NonclaimReason::CompilerOptionWithAuthoredLiteral
            && record.deletion == DeletionCondition::CompilerOptionOwner
    }));
}

#[test]
fn flow_region_closes_over_the_container_suffix_but_not_function_like_bodies() {
    let file = program_file(
        0,
        "flow-suffix.ts",
        concat!(
            "let subject: string | number = 0;\n",
            "switch (subject.) { default: break; }\n",
            "const hidden: string = subject;\n",
            "function hole() { const functionBody: string = 1; }\n",
            "class Holder { method() { const methodBody: string = 1; } }\n",
            "const resumed: string = subject;\n",
            "type Kept = MissingType;\n",
        ),
    );
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    let [subject, switch, hidden, function, class, resumed, kept] =
        file.syntax.statements.as_slice()
    else {
        panic!(
            "expected the authored root inventory: {:#?}",
            file.syntax.statements
        );
    };

    assert!(
        analysis
            .claim(
                CapabilityTarget::SemanticCheck,
                CapabilityScope::node(file.source.id, subject.id),
            )
            .is_claimed()
    );
    for statement in [switch, hidden, function, class, resumed] {
        let CapabilityClaim::Nonclaimed(reasons) = analysis.claim(
            CapabilityTarget::SemanticCheck,
            CapabilityScope::node(file.source.id, statement.id),
        ) else {
            panic!("suffix statement must be flow-nonclaimed: {statement:#?}");
        };
        assert!(reasons.into_iter().any(|record| {
            record.reason == NonclaimReason::Semantic(SemanticGap::FlowTypeOfReference)
                && record.deletion
                    == DeletionCondition::SemanticOwner(SemanticGap::FlowTypeOfReference)
        }));
        for target in [
            CapabilityTarget::QuickInfo,
            CapabilityTarget::Definition,
            CapabilityTarget::References,
            CapabilityTarget::Highlights,
            CapabilityTarget::Rename,
        ] {
            assert!(
                analysis
                    .claim(target, CapabilityScope::node(file.source.id, statement.id))
                    .is_claimed(),
                "flow value incompleteness must not erase binder-owned service identity",
            );
        }
        assert!(
            analysis
                .semantic_check_node_function_like_descendant_permissions(
                    file.source.id,
                    statement.id,
                )
                .0,
            "pure flow-region hosts may inventory independent arrows",
        );
    }
    assert!(
        analysis
            .claim(
                CapabilityTarget::SemanticCheck,
                CapabilityScope::node(file.source.id, kept.id),
            )
            .is_claimed(),
        "type-only suffix members do not consume flow state",
    );

    let StatementKind::Function(function_declaration) = &function.kind else {
        panic!("function declaration expected");
    };
    let StatementKind::Class(class_declaration) = &class.kind else {
        panic!("class declaration expected");
    };
    let ClassMemberKind::Method { body, .. } = &class_declaration.members[0].kind else {
        panic!("method expected");
    };
    for independent_body in [&function_declaration.body[0], &body[0]] {
        assert!(
            analysis
                .claim(
                    CapabilityTarget::SemanticCheck,
                    CapabilityScope::node(file.source.id, independent_body.id),
                )
                .is_claimed(),
            "function-like body is a fresh flow container: {independent_body:#?}",
        );
    }

    for name in ["hidden", "hole", "Holder", "resumed"] {
        let declaration = file
            .bindings
            .declarations
            .iter()
            .find(|declaration| declaration.name == name)
            .unwrap_or_else(|| panic!("bound declaration {name}"));
        let scope = declaration_scope(std::slice::from_ref(&file), declaration.id, &[])
            .expect("bound declaration has a stable statement owner");
        assert!(
            analysis
                .claim(CapabilityTarget::DeclarationModel, scope)
                .is_claimed(),
            "flow containment must retain binder/model identity for {name}",
        );
        assert!(
            !analysis
                .claim(CapabilityTarget::DeclarationValue, scope)
                .is_claimed(),
            "semantic value materialization must defer for {name}",
        );
    }
}

#[test]
fn recovery_in_a_signature_seeds_only_its_executable_body_container() {
    let file = program_file(
        0,
        "recovered-signature.ts",
        concat!(
            "function wrapper(value: ) {\n",
            "  const hidden: string = value;\n",
            "  type Kept = MissingType;\n",
            "  function nested() { const nestedBody: string = 1; }\n",
            "}\n",
        ),
    );
    let root = &file.syntax.statements[0];
    let StatementKind::Function(function) = &root.kind else {
        panic!("function expected");
    };
    let [hidden, kept, nested] = function.body.as_slice() else {
        panic!("three body statements expected");
    };
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    assert!(analysis.semantic_check_node_allows_claimed_descendants(file.source.id, root.id));
    assert!(
        !analysis.semantic_check_node_allows_recovery_identifiers(file.source.id, root.id),
        "a generic recovered signature may discover body statements, not header fragments",
    );
    assert!(
        !analysis
            .semantic_check_node_function_like_descendant_permissions(file.source.id, root.id)
            .0,
        "syntax-recovery hosts cannot publish arrow signatures",
    );
    assert!(
        !analysis
            .claim(
                CapabilityTarget::SemanticCheck,
                CapabilityScope::node(file.source.id, hidden.id),
            )
            .is_claimed()
    );
    assert!(
        analysis
            .claim(
                CapabilityTarget::SemanticCheck,
                CapabilityScope::node(file.source.id, kept.id),
            )
            .is_claimed(),
        "type-only declarations remain independently checkable",
    );
    assert!(
        !analysis.semantic_check_node_allows_claimed_descendants(file.source.id, nested.id),
        "the nested function declaration belongs to the outer suffix",
    );
    let StatementKind::Function(nested_function) = &nested.kind else {
        panic!("nested function expected");
    };
    assert!(
        analysis
            .claim(
                CapabilityTarget::SemanticCheck,
                CapabilityScope::node(file.source.id, nested_function.body[0].id),
            )
            .is_claimed(),
        "the nested function body starts a fresh container",
    );
}

#[test]
fn claimed_descendant_descent_requires_exact_node_scoped_reasons() {
    let file = FileId(0);
    let owner = NodeId(7);
    let requested_scope = CapabilityScope::node(file, owner);
    let recovery = |scope| CapabilityNonclaim {
        target: CapabilityTarget::SemanticCheck,
        scope,
        reason: NonclaimReason::Syntax(SyntaxGap::TypeRecovery),
        deletion: DeletionCondition::DeepestSemanticOwner(SyntaxGap::TypeRecovery),
    };
    let fragment = |scope| CapabilityNonclaim {
        target: CapabilityTarget::SemanticCheck,
        scope,
        reason: NonclaimReason::Syntax(SyntaxGap::TypeRecovery),
        deletion: DeletionCondition::SyntaxOwner(SyntaxGap::TypeRecovery),
    };
    let flow = |scope| CapabilityNonclaim {
        target: CapabilityTarget::SemanticCheck,
        scope,
        reason: NonclaimReason::Semantic(SemanticGap::FlowTypeOfReference),
        deletion: DeletionCondition::SemanticOwner(SemanticGap::FlowTypeOfReference),
    };
    let function_like = |scope, gap| CapabilityNonclaim {
        target: CapabilityTarget::SemanticCheck,
        scope,
        reason: NonclaimReason::Semantic(gap),
        deletion: DeletionCondition::SemanticOwner(gap),
    };
    let required_recovery = |scope, deletion| CapabilityNonclaim {
        target: CapabilityTarget::RequiredType,
        scope,
        reason: NonclaimReason::Syntax(SyntaxGap::TypeRecovery),
        deletion,
    };
    let analysis = |nonclaims: Vec<CapabilityNonclaim>| CapabilityAnalysis {
        nonclaims: nonclaims.into_boxed_slice(),
        ..CapabilityAnalysis::default()
    };

    assert!(
        analysis(vec![recovery(requested_scope)])
            .semantic_check_node_allows_claimed_descendants(file, owner),
        "an exact-node semantic recovery owner may enter independently claimed descendants",
    );
    let mixed = analysis(vec![recovery(requested_scope), fragment(requested_scope)]);
    assert!(
        mixed.semantic_check_node_allows_claimed_descendants(file, owner),
        "an exact semantic owner may discover descendants through its represented fragment",
    );
    assert!(
        !mixed.semantic_check_node_allows_recovery_identifiers(file, owner),
        "a represented fragment does not publish direct names",
    );
    assert!(
        !analysis(vec![fragment(requested_scope)])
            .semantic_check_node_allows_claimed_descendants(file, owner),
        "a representational fragment alone cannot discover semantic descendants",
    );
    assert!(
        analysis(vec![recovery(requested_scope), flow(requested_scope)])
            .semantic_check_node_allows_claimed_descendants(file, owner),
        "an exact-node flow region may accompany its exact-node recovery owner",
    );
    assert!(
        !analysis(vec![flow(requested_scope)])
            .semantic_check_node_allows_claimed_descendants(file, owner),
        "a pure flow region does not itself authorize descendant traversal",
    );
    assert!(
        analysis(vec![flow(requested_scope)])
            .semantic_check_node_function_like_descendant_permissions(file, owner)
            .0,
        "an exact-node flow region may inventory independent function-like expressions",
    );
    for gap in [
        SemanticGap::FunctionLikeTypeParameters,
        SemanticGap::ExplicitThisParameter,
        SemanticGap::FunctionExpressionBindingName,
    ] {
        assert_eq!(
            analysis(vec![function_like(requested_scope, gap)])
                .semantic_check_node_function_like_descendant_permissions(file, owner)
                .0,
            gap != SemanticGap::FunctionLikeTypeParameters,
            "generic environments remain dependency-closed while local binding gaps may enter a nested FunctionLike gate",
        );
        assert!(
            !analysis(vec![
                function_like(requested_scope, gap),
                fragment(requested_scope),
            ])
            .semantic_check_node_function_like_descendant_permissions(file, owner)
            .0,
            "syntax recovery must keep nested FunctionLike signatures dependency-closed",
        );
    }
    assert!(
        analysis(vec![required_recovery(
            requested_scope,
            DeletionCondition::DeepestSemanticOwner(SyntaxGap::TypeRecovery),
        )])
        .required_type_node_allows_function_like_reentry(file, owner),
        "an exact semantic recovery owner may re-enter a nested required-type owner",
    );
    assert!(
        !analysis(vec![required_recovery(
            requested_scope,
            DeletionCondition::SyntaxOwner(SyntaxGap::TypeRecovery),
        )])
        .required_type_node_allows_function_like_reentry(file, owner),
        "a representational fragment cannot publish a nested function signature",
    );

    for broader_scope in [CapabilityScope::Program, CapabilityScope::File(file)] {
        assert!(
            !analysis(vec![recovery(broader_scope)])
                .semantic_check_node_allows_claimed_descendants(file, owner),
            "a {broader_scope:?} recovery reason cannot unlock node descent",
        );
        assert!(
            !analysis(vec![recovery(broader_scope), flow(requested_scope)])
                .semantic_check_node_allows_claimed_descendants(file, owner),
            "an exact-node flow reason cannot narrow a {broader_scope:?} recovery reason",
        );
        assert!(
            !analysis(vec![recovery(requested_scope), flow(broader_scope)])
                .semantic_check_node_allows_claimed_descendants(file, owner),
            "an exact-node recovery reason cannot narrow a {broader_scope:?} flow reason",
        );
        assert!(
            !analysis(vec![flow(broader_scope)])
                .semantic_check_node_function_like_descendant_permissions(file, owner)
                .0,
            "a {broader_scope:?} flow reason cannot unlock node-owned function-like semantics",
        );
        assert!(
            !analysis(vec![required_recovery(
                broader_scope,
                DeletionCondition::DeepestSemanticOwner(SyntaxGap::TypeRecovery),
            )])
            .required_type_node_allows_function_like_reentry(file, owner),
            "a {broader_scope:?} recovery reason cannot unlock nested required-type owners",
        );
    }
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
                .parser_recovery_facts()
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
        .parser_recovery_facts()
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
                .parser_recovery_facts()
                .iter()
                .all(|recovery| recovery.kind != ParserRecoveryKind::RejectedGenericArrowPrefix),
            "{source}",
        );
        assert!(
            ordinary
                .syntax
                .parser_recovery_facts()
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
                !ordinary_analysis
                    .claim(target, CapabilityScope::File(ordinary.source.id))
                    .is_claimed(),
                "angle-assertion semantics and products remain explicitly nonclaimed: {source}",
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
        let function = variable.initializer.as_ref().expect("initializer");
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
            true,
        ),
        (
            "template-literal.ts",
            "const safe = 1; const gap = `${\"safe\"}`; const useSafe = safe;",
            false,
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
    let source = "const values = [`head${\"gap\"}tail`, (value: MissingArrowType) => value];";
    let file = program_file(0, "required-arrow.ts", source);
    let statement = &file.syntax.statements[0];
    let StatementKind::Variable(variable) = &statement.kind else {
        panic!("variable expected: {statement:#?}");
    };
    let initializer = variable.initializer.as_ref().expect("array initializer");
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
        .parser_recovery_facts()
        .iter()
        .filter(|recovery| recovery.kind == ParserRecoveryKind::Type)
        .collect::<Vec<_>>();
    assert_eq!(
        labels.len(),
        2,
        "{:#?}",
        file.syntax.parser_recovery_facts()
    );
    assert!(labels.iter().all(|recovery| {
        recovery.authored_span.start >= source.find("label").unwrap() as u32
            && recovery.recovery_extent.end < source.find("values;").unwrap() as u32
    }));

    let StatementKind::Variable(variable) = &file.syntax.statements[0].kind else {
        panic!("variable expected")
    };
    let arrow = variable.initializer.as_ref().expect("arrow initializer");
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
fn non_expression_parameters_retain_their_containing_statement_capability_scope() {
    let source = concat!(
        "class Holder { method(value = `head${renamed}tail`) { return value; } } ",
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
fn assertion_declarator_tail_has_typed_file_emit_nonclaims() {
    let source = "export const x = value as T, y = 1;";
    let file = program_file(0, "assertion-tail.ts", source);
    let tail = file
        .syntax
        .parser_recovery_facts()
        .iter()
        .find(|fact| fact.kind == ParserRecoveryKind::VariableDeclaratorTail)
        .expect("typed variable-declarator tail");
    assert_eq!(tail.authored_span.start, source.find(',').unwrap() as u32);
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    let scope = CapabilityScope::File(file.source.id);
    for target in [CapabilityTarget::JavaScript, CapabilityTarget::Declaration] {
        let records = analysis
            .nonclaims
            .iter()
            .filter(|record| record.target == target && record.scope == scope)
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(
            records,
            vec![CapabilityNonclaim {
                target,
                scope,
                reason: NonclaimReason::Syntax(SyntaxGap::VariableDeclaratorTail),
                deletion: DeletionCondition::SyntaxOwner(SyntaxGap::VariableDeclaratorTail),
            }],
        );
    }

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
        StatementKind::Variable(declaration) if declaration.name == "y"
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
